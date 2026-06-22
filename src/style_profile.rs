//! StyleProfile derivation (SPEC-008 Phase 2, roadmap #11) — the Rust core behind the
//! live tool `live_extract_style_profile`. Takes a source-resolution `RgbaImage` and
//! returns a machine-checkable style contract `{grid, palette, ramps, outline_policy,
//! light_dir, heads_tall, ...}` (research §G). Kept algorithmically identical to the
//! offline `tools/{regrid,ramp_lint,extract_palette,style_profile}.py` (which the eval
//! gates test) — the tests below mirror their selftests so the two can't diverge.

use std::collections::HashMap;

use image::RgbaImage;
use serde::Serialize;

use crate::color_ops::Rgba;

#[derive(Debug, Serialize, PartialEq)]
pub struct Grid {
    pub cell_w: u32,
    pub cell_h: u32,
    pub native: [u32; 2],
    pub scale: u32,
}

#[derive(Debug, Serialize)]
pub struct Ramp {
    pub role: String,
    pub colors: Vec<String>,
    pub length: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lint: Option<f64>,
}

#[derive(Debug, Serialize)]
pub struct StyleProfile {
    pub size: [u32; 2],
    pub grid: Grid,
    pub frame_counts: Option<u32>,
    pub palette: Vec<String>,
    pub ramps: Vec<Ramp>,
    pub light_dir: String,
    pub heads_tall: Option<f64>,
    pub outline_policy: String,
}

fn px(p: &image::Rgba<u8>) -> Rgba {
    Rgba::rgba(p.0[0], p.0[1], p.0[2], p.0[3])
}

fn luma(c: Rgba) -> f64 {
    0.299 * c.r as f64 + 0.587 * c.g as f64 + 0.114 * c.b as f64
}

fn hsv(c: Rgba) -> (f64, f64, f64) {
    let (r, g, b) = (c.r as f64 / 255.0, c.g as f64 / 255.0, c.b as f64 / 255.0);
    let mx = r.max(g).max(b);
    let mn = r.min(g).min(b);
    let d = mx - mn;
    let s = if mx == 0.0 { 0.0 } else { d / mx };
    let h = if d == 0.0 {
        0.0
    } else if (mx - r).abs() < 1e-12 {
        60.0 * ((g - b) / d).rem_euclid(6.0)
    } else if (mx - g).abs() < 1e-12 {
        60.0 * ((b - r) / d + 2.0)
    } else {
        60.0 * ((r - g) / d + 4.0)
    };
    (h.rem_euclid(360.0), s, mx)
}

fn round3(x: f64) -> f64 {
    (x * 1000.0).round() / 1000.0
}

// ---- grid de-fake (port of regrid.py) ----
fn gcd(a: u32, b: u32) -> u32 {
    if b == 0 { a } else { gcd(b, a % b) }
}

fn blocks_uniform(img: &RgbaImage, n: u32, tol: f64) -> bool {
    let (w, h) = (img.width(), img.height());
    if w % n != 0 || h % n != 0 {
        return false;
    }
    let need = tol * (n * n) as f64;
    let mut by = 0;
    while by < h {
        let mut bx = 0;
        while bx < w {
            let mut counts: HashMap<[u8; 4], usize> = HashMap::new();
            for dy in 0..n {
                for dx in 0..n {
                    *counts.entry(img.get_pixel(bx + dx, by + dy).0).or_insert(0) += 1;
                }
            }
            if (*counts.values().max().unwrap_or(&0) as f64) < need {
                return false;
            }
            bx += n;
        }
        by += n;
    }
    true
}

/// Native-grid auto-detect for a (possibly upscaled) reference — the block-uniformity /
/// GCD method (port of `tools/regrid.py`). Returns the largest cell size whose grid-aligned
/// blocks are mode-uniform (so N×-upscaled art reports cell N, native art reports cell 1).
/// Reused by `live_import_reference`'s `regrid` de-fake path (SPEC-006 Phase 2).
pub fn detect_grid(img: &RgbaImage) -> Grid {
    let (w, h) = (img.width(), img.height());
    let limit = gcd(w, h);
    let mut cell = 1u32;
    let mut n = 2;
    while n <= limit {
        if limit % n == 0 && blocks_uniform(img, n, 0.9) {
            cell = n;
        }
        n += 1;
    }
    Grid { cell_w: cell, cell_h: cell, native: [w / cell, h / cell], scale: cell }
}

// ---- palette (port of extract_palette.frequency; deterministic tie-break) ----
fn palette_frequency(img: &RgbaImage, n: usize) -> Vec<Rgba> {
    let mut counts: HashMap<(u8, u8, u8), usize> = HashMap::new();
    for p in img.pixels() {
        if p.0[3] != 0 {
            *counts.entry((p.0[0], p.0[1], p.0[2])).or_insert(0) += 1;
        }
    }
    let mut v: Vec<_> = counts.into_iter().collect();
    v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    v.into_iter().take(n).map(|((r, g, b), _)| Rgba::rgb(r, g, b)).collect()
}

// ---- ramp-lint score (port of ramp_lint.lint_ramp) ----
fn warmth(h_deg: f64) -> f64 {
    (h_deg - 45.0).to_radians().cos()
}

fn circular_span(mut hues: Vec<f64>) -> f64 {
    if hues.len() < 2 {
        return 0.0;
    }
    hues.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let mut gaps: Vec<f64> = (0..hues.len() - 1).map(|i| hues[i + 1] - hues[i]).collect();
    gaps.push(360.0 - hues[hues.len() - 1] + hues[0]);
    360.0 - gaps.into_iter().fold(0.0_f64, f64::max)
}

pub fn lint_ramp_score(colors: &[Rgba]) -> f64 {
    let n = colors.len();
    if n < 2 {
        return 0.0;
    }
    let lm: Vec<f64> = colors.iter().map(|&c| luma(c)).collect();
    let hv: Vec<(f64, f64, f64)> = colors.iter().map(|&c| hsv(c)).collect();
    let vm_ok = (0..n - 1).all(|i| lm[i + 1] > lm[i]);
    let hues: Vec<f64> = hv.iter().map(|t| t.0).collect();
    let span = circular_span(hues.clone());
    let warm: Vec<f64> = hues.iter().map(|&h| warmth(h)).collect();
    let warm_frac = (0..n - 1).filter(|&i| warm[i + 1] >= warm[i] - 1e-9).count() as f64 / (n - 1) as f64;
    let hue_score = (span / 10.0).min(1.0) * (0.5 + 0.5 * warm_frac);
    let mp_ok = if n >= 3 {
        let sat: Vec<f64> = hv.iter().map(|t| t.1).collect();
        let peak = (0..n).max_by(|&a, &b| sat[a].partial_cmp(&sat[b]).unwrap()).unwrap();
        peak > 0 && peak < n - 1
    } else {
        true
    };
    let nc_ok = !hv.iter().any(|&(_, s, v)| s > 0.9 && v > 0.95);
    let len_ok = (3..=5).contains(&n);
    let score = 0.30 * (vm_ok as i32 as f64)
        + 0.30 * hue_score
        + 0.15 * (mp_ok as i32 as f64)
        + 0.15 * (nc_ok as i32 as f64)
        + 0.10 * (len_ok as i32 as f64);
    round3(score.min(1.0))
}

// ---- ramp-sort (port of style_profile.ramp_sort) ----
fn hue_role(c: Rgba) -> &'static str {
    let (h, s, v) = hsv(c);
    if v < 0.12 {
        return "outline";
    }
    if s < 0.12 {
        return "neutral";
    }
    if !(20.0..330.0).contains(&h) {
        "red"
    } else if h < 45.0 {
        "leather"
    } else if h < 70.0 {
        "gold"
    } else if h < 170.0 {
        "skin"
    } else if h < 200.0 {
        "cyan"
    } else if h < 260.0 {
        "blue"
    } else {
        "magenta"
    }
}

fn ramp_sort(palette: &[Rgba]) -> Vec<Ramp> {
    let mut groups: HashMap<&'static str, Vec<Rgba>> = HashMap::new();
    for &c in palette {
        groups.entry(hue_role(c)).or_default().push(c);
    }
    let mut ramps: Vec<Ramp> = groups
        .into_iter()
        .map(|(role, mut colors)| {
            colors.sort_by(|&a, &b| luma(a).partial_cmp(&luma(b)).unwrap());
            let lint = if colors.len() >= 2 { Some(lint_ramp_score(&colors)) } else { None };
            Ramp {
                role: role.to_string(),
                colors: colors.iter().map(|c| c.to_hex()).collect(),
                length: colors.len(),
                lint,
            }
        })
        .collect();
    // length desc, role asc — deterministic despite HashMap iteration order.
    ramps.sort_by(|a, b| b.length.cmp(&a.length).then(a.role.cmp(&b.role)));
    ramps
}

// ---- geometry (port of style_profile.{light_dir,heads_tall,outline_policy}) ----
fn opaque(img: &RgbaImage) -> Vec<(u32, u32, Rgba)> {
    img.enumerate_pixels()
        .filter(|(_, _, p)| p.0[3] > 0)
        .map(|(x, y, p)| (x, y, px(p)))
        .collect()
}

fn light_dir(img: &RgbaImage) -> String {
    let (w, h) = (img.width() as f64, img.height() as f64);
    let (mut tl, mut br) = (Vec::new(), Vec::new());
    for (x, y, c) in opaque(img) {
        if (x as f64) < w / 2.0 && (y as f64) < h / 2.0 {
            tl.push(luma(c));
        }
        if (x as f64) >= w / 2.0 && (y as f64) >= h / 2.0 {
            br.push(luma(c));
        }
    }
    if tl.is_empty() || br.is_empty() {
        return "unknown".into();
    }
    let mtl = tl.iter().sum::<f64>() / tl.len() as f64;
    let mbr = br.iter().sum::<f64>() / br.len() as f64;
    if mtl >= mbr { "top-left".into() } else { "bottom-right".into() }
}

fn heads_tall(img: &RgbaImage) -> Option<f64> {
    let mut rows: HashMap<u32, (u32, u32)> = HashMap::new();
    let (mut top, mut bottom) = (u32::MAX, 0u32);
    for (x, y, _) in opaque(img) {
        let e = rows.entry(y).or_insert((x, x));
        e.0 = e.0.min(x);
        e.1 = e.1.max(x);
        top = top.min(y);
        bottom = bottom.max(y);
    }
    if rows.is_empty() {
        return None;
    }
    let max_w = rows.values().map(|(a, b)| b - a + 1).max().unwrap();
    let mut head_h = 0u32;
    for y in top..=bottom {
        let wy = rows.get(&y).map(|(a, b)| b - a + 1).unwrap_or(0);
        if (wy as f64) <= 0.7 * (max_w as f64) {
            head_h += 1;
        } else {
            break;
        }
    }
    let total = bottom - top + 1;
    if head_h > 0 {
        Some(round3(total as f64 / head_h as f64))
    } else {
        None
    }
}

fn outline_policy(img: &RgbaImage) -> String {
    let (w, h) = (img.width(), img.height());
    let is_op = |x: i64, y: i64| {
        x >= 0 && y >= 0 && (x as u32) < w && (y as u32) < h && img.get_pixel(x as u32, y as u32).0[3] > 0
    };
    let mut boundary: Vec<Rgba> = Vec::new();
    for (x, y, p) in img.enumerate_pixels() {
        if p.0[3] == 0 {
            continue;
        }
        let (xi, yi) = (x as i64, y as i64);
        if !is_op(xi + 1, yi) || !is_op(xi - 1, yi) || !is_op(xi, yi + 1) || !is_op(xi, yi - 1) {
            boundary.push(px(p));
        }
    }
    if boundary.is_empty() {
        return "none".into();
    }
    let dark: Vec<Rgba> = boundary.iter().cloned().filter(|&c| luma(c) < 80.0).collect();
    if dark.is_empty() {
        return "none".into();
    }
    let mut counts: HashMap<(u8, u8, u8), usize> = HashMap::new();
    for c in &dark {
        *counts.entry((c.r, c.g, c.b)).or_insert(0) += 1;
    }
    let (top, cnt) = counts.iter().max_by_key(|(_, &v)| v).map(|(k, &v)| (*k, v)).unwrap();
    if (cnt as f64) >= 0.5 * (boundary.len() as f64) {
        format!("uniform #{:02x}{:02x}{:02x}", top.0, top.1, top.2)
    } else {
        "selective".into()
    }
}

pub fn derive(img: &RgbaImage, colors: usize) -> StyleProfile {
    let palette = palette_frequency(img, colors);
    StyleProfile {
        size: [img.width(), img.height()],
        grid: detect_grid(img),
        frame_counts: None,
        palette: palette.iter().map(|c| c.to_hex()).collect(),
        ramps: ramp_sort(&palette),
        light_dir: light_dir(img),
        heads_tall: heads_tall(img),
        outline_policy: outline_policy(img),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hexes(cs: &[&str]) -> Vec<Rgba> {
        cs.iter().map(|h| Rgba::from_hex(h).unwrap()).collect()
    }

    #[test]
    fn lint_matches_python_calibration() {
        // goblin skin ramp passes (>= 0.7); a grey value-only ramp fails.
        let good = hexes(&["#1b4d3e", "#2e7d32", "#4ca02c", "#6abe30", "#a6d94a"]);
        assert!(lint_ramp_score(&good) >= 0.7, "{}", lint_ramp_score(&good));
        let gray = hexes(&["#222222", "#555555", "#888888", "#bbbbbb", "#eeeeee"]);
        assert!(lint_ramp_score(&gray) < 0.7, "{}", lint_ramp_score(&gray));
    }

    fn solid(w: u32, h: u32, fill: impl Fn(u32, u32) -> [u8; 4]) -> RgbaImage {
        let mut img = RgbaImage::new(w, h);
        for y in 0..h {
            for x in 0..w {
                img.put_pixel(x, y, image::Rgba(fill(x, y)));
            }
        }
        img
    }

    #[test]
    fn grid_detects_scale() {
        // native: a varied small image → cell 1.
        let pal = [[200, 40, 40, 255], [40, 200, 60, 255], [50, 60, 220, 255], [230, 210, 40, 255], [0, 0, 0, 0]];
        let base = solid(8, 8, |x, y| pal[(((y * 8 + x) as usize * 1103515245 + 12345) >> 4) % 5]);
        assert_eq!(detect_grid(&base).cell_w, 1);
        // 4×-upscale → cell 4, native 8×8.
        let up = solid(32, 32, |x, y| {
            pal[((((y / 4) * 8 + (x / 4)) as usize * 1103515245 + 12345) >> 4) % 5]
        });
        let g = detect_grid(&up);
        assert_eq!((g.cell_w, g.native), (4, [8, 8]), "{g:?}");
    }

    #[test]
    fn derive_reads_geometry() {
        // 16×24 figure: narrow head (rows 0-7) over a wider body, brighter on the left.
        let img = solid(16, 24, |x, y| {
            let (x0, x1) = if y < 8 { (5u32, 11u32) } else { (2u32, 14u32) };
            if x >= x0 && x < x1 {
                if x < 8 { [60, 180, 70, 255] } else { [40, 120, 50, 255] }
            } else {
                [0, 0, 0, 0]
            }
        });
        let p = derive(&img, 8);
        assert_eq!(p.light_dir, "top-left");
        let ht = p.heads_tall.unwrap();
        assert!((2.5..=3.5).contains(&ht), "heads_tall {ht}");
        assert!(p.ramps.iter().any(|r| r.role == "skin"));
    }
}
