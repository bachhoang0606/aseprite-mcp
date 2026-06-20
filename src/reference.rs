//! Reference-image import core (SPEC-006 Phase 1, research §C2 / Path 3–4).
//!
//! Turn a high-res reference (photo, illustration, AI image, CC0 asset) into clean,
//! palette-locked pixel art the agent can trace over. Two deterministic steps fused into
//! one pass: a CONTENT-AWARE downscale to the target grid (a per-cell **majority vote**,
//! which keeps hard edges and invents no mixed colours — unlike a bilinear shrink) and a
//! CIELAB snap to a curated palette (`color_ops`). Pure (buffer-in / buffer-out, no
//! Aseprite) so the downscale + snap math is unit-tested without the live bridge.
#![allow(dead_code)]

use crate::color_ops::{self, Rgba as CRgba};
use image::{Rgba, RgbaImage};

/// Largest target long edge the import allows, so the live draw batch (one `draw_pixels`
/// call) can't explode (256×256 = 65 536 cells worst case).
pub const MAX_TARGET_EDGE: u32 = 256;

/// Largest reference edge the import will decode, so a pathological huge PNG can't OOM the
/// server (8192×8192 RGBA ≈ 256 MB — the ceiling, not a recommendation).
pub const MAX_SOURCE_EDGE: u32 = 8192;

/// How a source block collapses to one output cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    /// Majority vote over the cell's pixels (edge-preserving). Default.
    Dominant,
    /// Mean of the cell's opaque pixels (smoother).
    Average,
}

impl Method {
    pub fn parse(s: &str) -> Result<Method, String> {
        match s {
            "dominant" => Ok(Method::Dominant),
            "average" => Ok(Method::Average),
            other => Err(format!(
                "method must be \"dominant\" or \"average\" (got \"{other}\")"
            )),
        }
    }
}

/// The source-px span `[a, b)` that output cell `o` covers along one axis of length `src`
/// downscaled to `dst`. Integer area mapping in `u64`; the block is forced to ≥1 px so
/// every output cell samples something even when `dst ≈ src`.
fn cell_span(o: u32, dst: u32, src: u32) -> (u32, u32) {
    let dst = dst.max(1) as u64;
    let (o, srcu) = (o as u64, src as u64);
    let a = (o * srcu / dst) as u32;
    let b = (((o + 1) * srcu / dst) as u32).max(a + 1).min(src);
    (a, b)
}

/// Content-aware downscale of `src` to `tw×th`, optionally snapped to `palette`. Output is
/// exactly `tw×th` RGBA; a cell whose transparent pixels outnumber its most-common colour
/// is output transparent. Pure.
pub fn downscale_to_grid(
    src: &RgbaImage,
    tw: u32,
    th: u32,
    palette: Option<&[CRgba]>,
    method: Method,
) -> RgbaImage {
    let (tw, th) = (tw.max(1), th.max(1));
    let (sw, sh) = (src.width(), src.height());
    let mut out = RgbaImage::new(tw, th);
    if sw == 0 || sh == 0 {
        return out;
    }
    let palette = palette.filter(|p| !p.is_empty());
    for oy in 0..th {
        let (y0, y1) = cell_span(oy, th, sh);
        for ox in 0..tw {
            let (x0, x1) = cell_span(ox, tw, sw);
            out.put_pixel(ox, oy, collapse_cell(src, x0, x1, y0, y1, palette, method));
        }
    }
    out
}

fn collapse_cell(
    src: &RgbaImage,
    x0: u32,
    x1: u32,
    y0: u32,
    y1: u32,
    palette: Option<&[CRgba]>,
    method: Method,
) -> Rgba<u8> {
    match method {
        Method::Dominant => dominant_cell(src, x0, x1, y0, y1, palette),
        Method::Average => average_cell(src, x0, x1, y0, y1, palette),
    }
}

/// Majority vote: a transparent pixel votes "transparent", an opaque one votes its nearest
/// palette index (or its packed RGB when no palette). The cell takes the biggest bucket;
/// ties between colours break to the lower palette index / lower packed RGB (determinism).
fn dominant_cell(
    src: &RgbaImage,
    x0: u32,
    x1: u32,
    y0: u32,
    y1: u32,
    palette: Option<&[CRgba]>,
) -> Rgba<u8> {
    let mut transparent = 0u32;
    // Best opaque bucket so far, with a deterministic key for tie-breaking.
    let mut best_count = 0u32;
    let mut best_key = u32::MAX; // packed rgb or palette index
    let mut best_color = Rgba([0, 0, 0, 0]);
    // Tally. For a palette, count votes per index in a small vec; else a hashmap of RGB.
    let mut pal_votes: Vec<u32> = palette.map(|p| vec![0u32; p.len()]).unwrap_or_default();
    let mut rgb_votes: std::collections::HashMap<u32, u32> = std::collections::HashMap::new();

    for y in y0..y1 {
        for x in x0..x1 {
            let p = src.get_pixel(x, y).0;
            if p[3] == 0 {
                transparent += 1;
                continue;
            }
            match palette {
                Some(pal) => {
                    let idx = color_ops::nearest_palette_index(CRgba::rgb(p[0], p[1], p[2]), pal)
                        .unwrap_or(0);
                    pal_votes[idx] += 1;
                    let c = pal[idx];
                    consider(&mut best_count, &mut best_key, &mut best_color, pal_votes[idx], idx as u32, Rgba([c.r, c.g, c.b, 255]));
                }
                None => {
                    let key = (p[0] as u32) << 16 | (p[1] as u32) << 8 | p[2] as u32;
                    let v = rgb_votes.entry(key).or_insert(0);
                    *v += 1;
                    consider(&mut best_count, &mut best_key, &mut best_color, *v, key, Rgba([p[0], p[1], p[2], 255]));
                }
            }
        }
    }
    if best_count == 0 || transparent > best_count {
        return Rgba([0, 0, 0, 0]);
    }
    best_color
}

/// Update the running winner: a higher vote wins; an equal vote breaks to the lower key.
fn consider(best_count: &mut u32, best_key: &mut u32, best_color: &mut Rgba<u8>, count: u32, key: u32, color: Rgba<u8>) {
    if count > *best_count || (count == *best_count && key < *best_key) {
        *best_count = count;
        *best_key = key;
        *best_color = color;
    }
}

/// Mean of the cell's opaque pixels, snapped to the palette if one is given. A
/// majority-transparent cell (transparent ≥ opaque) is output transparent.
fn average_cell(
    src: &RgbaImage,
    x0: u32,
    x1: u32,
    y0: u32,
    y1: u32,
    palette: Option<&[CRgba]>,
) -> Rgba<u8> {
    let (mut r, mut g, mut b, mut n) = (0u64, 0u64, 0u64, 0u64);
    let mut transparent = 0u64;
    for y in y0..y1 {
        for x in x0..x1 {
            let p = src.get_pixel(x, y).0;
            if p[3] == 0 {
                transparent += 1;
            } else {
                r += p[0] as u64;
                g += p[1] as u64;
                b += p[2] as u64;
                n += 1;
            }
        }
    }
    if n == 0 || transparent >= n {
        return Rgba([0, 0, 0, 0]);
    }
    let mean = CRgba::rgb((r / n) as u8, (g / n) as u8, (b / n) as u8);
    let c = match palette {
        Some(pal) => color_ops::clamp_to_palette(mean, pal),
        None => mean,
    };
    Rgba([c.r, c.g, c.b, 255])
}

/// Distinct opaque colours in `img` (for the import summary).
pub fn distinct_colors(img: &RgbaImage) -> usize {
    let mut seen = std::collections::HashSet::new();
    for p in img.pixels() {
        if p.0[3] != 0 {
            seen.insert([p.0[0], p.0[1], p.0[2]]);
        }
    }
    seen.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn px(img: &mut RgbaImage, x: u32, y: u32, c: [u8; 4]) {
        img.put_pixel(x, y, Rgba(c));
    }

    #[test]
    fn cell_span_tiles_the_axis_with_nonempty_blocks() {
        // 4 -> 2: clean halves.
        assert_eq!(cell_span(0, 2, 4), (0, 2));
        assert_eq!(cell_span(1, 2, 4), (2, 4));
        // 10 -> 3: non-integer; every cell ≥1 px, last clamps to src.
        let spans: Vec<_> = (0..3).map(|o| cell_span(o, 3, 10)).collect();
        assert_eq!(spans, vec![(0, 3), (3, 6), (6, 10)]);
        // dst >= src: each cell still ≥1 px (never empty).
        for o in 0..4 {
            let (a, b) = cell_span(o, 4, 2);
            assert!(b > a, "empty cell at {o}: {a}..{b}");
            assert!(b <= 2);
        }
    }

    #[test]
    fn dominant_preserves_a_hard_edge_no_invented_colour() {
        // 4×2: left half red, right half blue. Downscale 4→2 wide: cell0=red, cell1=blue,
        // never a purple average.
        let mut img = RgbaImage::from_pixel(4, 2, Rgba([0, 0, 0, 255]));
        for y in 0..2 {
            px(&mut img, 0, y, [200, 0, 0, 255]);
            px(&mut img, 1, y, [200, 0, 0, 255]);
            px(&mut img, 2, y, [0, 0, 200, 255]);
            px(&mut img, 3, y, [0, 0, 200, 255]);
        }
        let out = downscale_to_grid(&img, 2, 1, None, Method::Dominant);
        assert_eq!((out.width(), out.height()), (2, 1));
        assert_eq!(*out.get_pixel(0, 0), Rgba([200, 0, 0, 255]));
        assert_eq!(*out.get_pixel(1, 0), Rgba([0, 0, 200, 255]));
    }

    #[test]
    fn dominant_with_palette_outputs_only_palette_colours() {
        // Near-red and near-blue source pixels snap to exact palette red/blue.
        let mut img = RgbaImage::from_pixel(2, 1, Rgba([0, 0, 0, 255]));
        px(&mut img, 0, 0, [210, 10, 12, 255]); // ~red
        px(&mut img, 1, 0, [12, 8, 205, 255]); // ~blue
        let palette = vec![CRgba::rgb(255, 0, 0), CRgba::rgb(0, 0, 255), CRgba::rgb(0, 0, 0)];
        let out = downscale_to_grid(&img, 2, 1, Some(&palette), Method::Dominant);
        assert_eq!(*out.get_pixel(0, 0), Rgba([255, 0, 0, 255]));
        assert_eq!(*out.get_pixel(1, 0), Rgba([0, 0, 255, 255]));
        // Every output colour is in the palette.
        for p in out.pixels() {
            if p.0[3] != 0 {
                assert!(palette.contains(&CRgba::rgb(p.0[0], p.0[1], p.0[2])), "off-palette {p:?}");
            }
        }
    }

    #[test]
    fn majority_transparent_cell_is_transparent() {
        // 2×2 cell: 3 transparent + 1 opaque -> transparent wins.
        let mut img = RgbaImage::from_pixel(2, 2, Rgba([0, 0, 0, 0]));
        px(&mut img, 0, 0, [200, 0, 0, 255]);
        let out = downscale_to_grid(&img, 1, 1, None, Method::Dominant);
        assert_eq!(out.get_pixel(0, 0).0[3], 0);
        // But a majority-opaque cell keeps the colour.
        px(&mut img, 1, 0, [200, 0, 0, 255]);
        px(&mut img, 0, 1, [200, 0, 0, 255]);
        let out = downscale_to_grid(&img, 1, 1, None, Method::Dominant);
        assert_eq!(*out.get_pixel(0, 0), Rgba([200, 0, 0, 255]));
    }

    #[test]
    fn average_returns_the_cell_mean() {
        // 2×1 of (100,0,0) and (200,0,0) -> mean (150,0,0).
        let mut img = RgbaImage::from_pixel(2, 1, Rgba([0, 0, 0, 255]));
        px(&mut img, 0, 0, [100, 0, 0, 255]);
        px(&mut img, 1, 0, [200, 0, 0, 255]);
        let out = downscale_to_grid(&img, 1, 1, None, Method::Average);
        assert_eq!(*out.get_pixel(0, 0), Rgba([150, 0, 0, 255]));
    }

    #[test]
    fn output_is_exactly_target_size_for_noninteger_ratio() {
        let img = RgbaImage::from_pixel(10, 7, Rgba([40, 90, 160, 255]));
        let out = downscale_to_grid(&img, 3, 3, None, Method::Dominant);
        assert_eq!((out.width(), out.height()), (3, 3));
        // Solid source -> solid output, every cell filled.
        for p in out.pixels() {
            assert_eq!(*p, Rgba([40, 90, 160, 255]));
        }
        assert_eq!(distinct_colors(&out), 1);
    }

    #[test]
    fn method_parse_rejects_unknown() {
        assert_eq!(Method::parse("dominant").unwrap(), Method::Dominant);
        assert_eq!(Method::parse("average").unwrap(), Method::Average);
        assert!(Method::parse("bilinear").is_err());
    }
}
