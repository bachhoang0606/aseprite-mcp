//! SPEC-009 — dep-free RotSprite rotation (roadmap #8, Path 2/5).
//!
//! Artifact-free pixel-art rotation that introduces **no new colours**. The classic
//! RotSprite pipeline (Xenowhirl, 2007), hand-rolled with zero new dependencies:
//!
//!   1. **Scale ×8** — Scale2× (EPX) applied three times. Each pass doubles the image
//!      and smooths single-pixel staircase edges into cleaner diagonals, choosing only
//!      among existing neighbour colours.
//!   2. **Rotate** the ×8 image by `angle` with **nearest-neighbour** sampling into the
//!      rotated bounding box. NN copies a source pixel — it never blends.
//!   3. **Downscale ×8** by per-block **mode** (the most common colour in each 8×8
//!      block). Mode picks an existing colour — it never averages.
//!
//! Because every stage *selects* an input colour and none ever *blends*, the output
//! palette ⊆ input palette ∪ {transparent}: **palette-legal by construction**, the whole
//! point of Path 2. Right-angle rotations (0/90/180/270) are exact rearrangements and
//! bypass the scale dance entirely. Pure Rust, unit-tested by `cargo test` (the CI gate).
//!
//! Angle convention: degrees, **positive = clockwise** (image y points down).

use crate::color_ops::Rgba;
use std::collections::HashMap;

/// The fully-transparent pixel used for "outside the source" after rotation.
pub const TRANSPARENT: Rgba = Rgba { r: 0, g: 0, b: 0, a: 0 };

/// Scale factor of the RotSprite up/down dance (Scale2× applied three times).
const SCALE: u32 = 8;

/// A simple owned RGBA raster (row-major, `width * height` pixels). Kept independent of
/// the `image` crate so the core is pure and trivially unit-testable.
#[derive(Clone, PartialEq, Debug)]
pub struct Raster {
    pub width: u32,
    pub height: u32,
    pub px: Vec<Rgba>,
}

impl Raster {
    pub fn new(width: u32, height: u32) -> Self {
        Self { width, height, px: vec![TRANSPARENT; (width as usize) * (height as usize)] }
    }

    #[inline]
    pub fn get(&self, x: u32, y: u32) -> Rgba {
        self.px[(y * self.width + x) as usize]
    }

    #[inline]
    pub fn set(&mut self, x: u32, y: u32, c: Rgba) {
        self.px[(y * self.width + x) as usize] = c;
    }

    /// Count of distinct non-transparent colours (for the tool's summary).
    pub fn distinct_colors(&self) -> usize {
        let mut seen = std::collections::HashSet::new();
        for &c in &self.px {
            if !c.is_transparent() {
                seen.insert(c);
            }
        }
        seen.len()
    }
}

/// Scale2× (EPX): double the image, expanding each pixel `P` into a 2×2 block whose
/// corners may take an orthogonal neighbour when that neighbour forms a clean diagonal.
/// Out-of-bounds neighbours are treated as `P` (the standard edge convention), so the
/// border never invents a colour. Only existing colours are emitted.
fn scale2x(src: &Raster) -> Raster {
    let (w, h) = (src.width, src.height);
    if w == 0 || h == 0 {
        return src.clone();
    }
    let mut out = Raster::new(w * 2, h * 2);
    for y in 0..h {
        for x in 0..w {
            let p = src.get(x, y);
            //   a            up
            // c p b   left  P  right
            //   d            down
            let a = if y > 0 { src.get(x, y - 1) } else { p };
            let d = if y < h - 1 { src.get(x, y + 1) } else { p };
            let c = if x > 0 { src.get(x - 1, y) } else { p };
            let b = if x < w - 1 { src.get(x + 1, y) } else { p };

            let (mut e0, mut e1, mut e2, mut e3) = (p, p, p, p);
            if c == a && c != d && a != b {
                e0 = a;
            }
            if a == b && a != c && b != d {
                e1 = b;
            }
            if d == c && d != b && c != a {
                e2 = c;
            }
            if b == d && b != a && d != c {
                e3 = d;
            }
            out.set(x * 2, y * 2, e0);
            out.set(x * 2 + 1, y * 2, e1);
            out.set(x * 2, y * 2 + 1, e2);
            out.set(x * 2 + 1, y * 2 + 1, e3);
        }
    }
    out
}

/// Scale ×8 = Scale2× three times.
fn scale8x(src: &Raster) -> Raster {
    scale2x(&scale2x(&scale2x(src)))
}

/// Nearest-neighbour rotation by `theta` radians into the rotated bounding box. The
/// destination is mapped back to the source by the inverse rotation `R(-theta)` and the
/// nearest source pixel is copied (or left transparent if it lands outside the source).
fn rotate_nn(src: &Raster, theta: f64) -> Raster {
    let (sw, sh) = (src.width as f64, src.height as f64);
    let (cos, sin) = (theta.cos(), theta.sin());
    let nw = (sw * cos.abs() + sh * sin.abs()).ceil().max(1.0);
    let nh = (sw * sin.abs() + sh * cos.abs()).ceil().max(1.0);
    let (nwu, nhu) = (nw as u32, nh as u32);
    let mut out = Raster::new(nwu, nhu);
    let (hcx, hcy) = (nw / 2.0, nh / 2.0);
    let (scx, scy) = (sw / 2.0, sh / 2.0);
    for oy in 0..nhu {
        for ox in 0..nwu {
            // Destination pixel centre, relative to the output centre.
            let dx = ox as f64 + 0.5 - hcx;
            let dy = oy as f64 + 0.5 - hcy;
            // Inverse rotation R(-theta), then shift into source-pixel space.
            let sx = dx * cos + dy * sin + scx;
            let sy = -dx * sin + dy * cos + scy;
            let (fx, fy) = (sx.floor(), sy.floor());
            if fx >= 0.0 && fy >= 0.0 && (fx as u32) < src.width && (fy as u32) < src.height {
                out.set(ox, oy, src.get(fx as u32, fy as u32));
            }
        }
    }
    out
}

/// The mode (most common colour) of the `bw × bh` block whose top-left is `(x0, y0)`.
/// Ties break toward the colour seen first in raster order — deterministic. Transparent
/// counts as a colour, so a mostly-empty edge block resolves to transparent.
fn block_mode(src: &Raster, x0: u32, y0: u32, bw: u32, bh: u32) -> Rgba {
    let mut counts: HashMap<Rgba, (u32, u32)> = HashMap::new();
    let mut order = 0u32;
    for y in y0..y0 + bh {
        for x in x0..x0 + bw {
            let e = counts.entry(src.get(x, y)).or_insert((0, order));
            e.0 += 1;
            order += 1;
        }
    }
    counts
        .into_iter()
        // Maximise count; on a tie, prefer the smaller first-seen index.
        .max_by(|(_, (ca, fa)), (_, (cb, fb))| ca.cmp(cb).then_with(|| fb.cmp(fa)))
        .map(|(c, _)| c)
        .unwrap_or(TRANSPARENT)
}

/// Downscale by `factor` taking the per-block mode. Output dims are `ceil(w/factor)` ×
/// `ceil(h/factor)`; edge blocks are clamped to the available pixels.
fn downscale_mode(src: &Raster, factor: u32) -> Raster {
    let w = src.width.div_ceil(factor).max(1);
    let h = src.height.div_ceil(factor).max(1);
    let mut out = Raster::new(w, h);
    for oy in 0..h {
        for ox in 0..w {
            let (x0, y0) = (ox * factor, oy * factor);
            let bw = factor.min(src.width - x0);
            let bh = factor.min(src.height - y0);
            out.set(ox, oy, block_mode(src, x0, y0, bw, bh));
        }
    }
    out
}

/// Clockwise 90°: output is `h × w`; source `(x, y)` → dest `(h-1-y, x)`.
fn rotate90(src: &Raster) -> Raster {
    let (w, h) = (src.width, src.height);
    let mut out = Raster::new(h, w);
    for y in 0..h {
        for x in 0..w {
            out.set(h - 1 - y, x, src.get(x, y));
        }
    }
    out
}

/// 180°: same dims; source `(x, y)` → dest `(w-1-x, h-1-y)`.
fn rotate180(src: &Raster) -> Raster {
    let (w, h) = (src.width, src.height);
    let mut out = Raster::new(w, h);
    for y in 0..h {
        for x in 0..w {
            out.set(w - 1 - x, h - 1 - y, src.get(x, y));
        }
    }
    out
}

/// Counter-clockwise 90° (= clockwise 270°): output is `h × w`; `(x, y)` → `(y, w-1-x)`.
fn rotate270(src: &Raster) -> Raster {
    let (w, h) = (src.width, src.height);
    let mut out = Raster::new(h, w);
    for y in 0..h {
        for x in 0..w {
            out.set(y, w - 1 - x, src.get(x, y));
        }
    }
    out
}

/// Rotate `src` by `angle_deg` (positive = clockwise) with the RotSprite pipeline.
/// Right angles are exact; every other angle scales ×8, rotates nearest-neighbour, and
/// downscales by mode — introducing no colour absent from the source.
pub fn rotsprite(src: &Raster, angle_deg: f64) -> Raster {
    if src.width == 0 || src.height == 0 {
        return src.clone();
    }
    let a = angle_deg.rem_euclid(360.0);
    const EPS: f64 = 1e-9;
    if a < EPS || (360.0 - a) < EPS {
        return src.clone();
    }
    if (a - 90.0).abs() < EPS {
        return rotate90(src);
    }
    if (a - 180.0).abs() < EPS {
        return rotate180(src);
    }
    if (a - 270.0).abs() < EPS {
        return rotate270(src);
    }
    let scaled = scale8x(src);
    let rotated = rotate_nn(&scaled, angle_deg.to_radians());
    downscale_mode(&rotated, SCALE)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(r: u8, g: u8, b: u8) -> Rgba {
        Rgba::rgb(r, g, b)
    }

    /// Build a raster from a slice of rows (each row a slice of colours).
    fn raster(rows: &[&[Rgba]]) -> Raster {
        let h = rows.len() as u32;
        let w = rows[0].len() as u32;
        let mut out = Raster::new(w, h);
        for (y, row) in rows.iter().enumerate() {
            for (x, &c) in row.iter().enumerate() {
                out.set(x as u32, y as u32, c);
            }
        }
        out
    }

    fn palette_of(rast: &Raster) -> std::collections::HashSet<Rgba> {
        rast.px.iter().copied().collect()
    }

    #[test]
    fn scale2x_doubles_and_uses_only_existing_colors() {
        let a = r(10, 20, 30);
        let b = r(200, 100, 50);
        let src = raster(&[&[a, b], &[b, a]]);
        let out = scale2x(&src);
        assert_eq!((out.width, out.height), (4, 4));
        // Every output colour must already exist in the source.
        let src_pal = palette_of(&src);
        assert!(out.px.iter().all(|c| src_pal.contains(c)));
    }

    #[test]
    fn scale2x_keeps_a_flat_field_flat() {
        let c = r(7, 7, 7);
        let src = raster(&[&[c, c], &[c, c]]);
        let out = scale2x(&src);
        assert!(out.px.iter().all(|&p| p == c));
    }

    #[test]
    fn right_angles_are_exact() {
        let a = r(1, 0, 0);
        let b = r(0, 1, 0);
        let c = r(0, 0, 1);
        let d = r(9, 9, 9);
        // Asymmetric so each rotation is distinguishable.
        let src = raster(&[&[a, b], &[c, d]]);

        // 0° / 360° identity.
        assert_eq!(rotsprite(&src, 0.0), src);
        assert_eq!(rotsprite(&src, 360.0), src);

        // 90° clockwise: top row becomes the right column.
        let cw = rotsprite(&src, 90.0);
        assert_eq!((cw.width, cw.height), (2, 2));
        assert_eq!(cw.get(1, 0), a); // a: (0,0) -> (1,0)
        assert_eq!(cw.get(0, 0), c); // c: (0,1) -> (0,0)
        assert_eq!(cw.get(1, 1), b); // b: (1,0) -> (1,1)
        assert_eq!(cw.get(0, 1), d); // d: (1,1) -> (0,1)

        // 180°.
        let half = rotsprite(&src, 180.0);
        assert_eq!(half.get(1, 1), a);
        assert_eq!(half.get(0, 0), d);

        // 270° (CCW 90) is the inverse of 90° CW.
        let ccw = rotsprite(&src, 270.0);
        assert_eq!(ccw.get(0, 1), a); // a: (0,0) -> (0,1)
    }

    #[test]
    fn rotsprite_45_introduces_no_new_colors() {
        // A small two-colour glyph on transparent background.
        let f = r(220, 40, 40);
        let t = TRANSPARENT;
        let src = raster(&[
            &[t, f, t],
            &[f, f, f],
            &[t, f, t],
        ]);
        let out = rotsprite(&src, 45.0);
        let allowed = palette_of(&src); // {transparent, f}
        assert!(
            out.px.iter().all(|c| allowed.contains(c)),
            "45° rotation invented a colour outside the source palette"
        );
        // The output must still contain the fill colour (it didn't erase everything).
        assert!(out.px.iter().any(|&c| c == f));
    }

    #[test]
    fn rotsprite_45_expands_the_bounding_box() {
        let f = r(0, 0, 0);
        // 8×8 solid block; a 45° rotation makes the bbox ~ side*sqrt(2).
        let row: Vec<Rgba> = vec![f; 8];
        let rows: Vec<&[Rgba]> = (0..8).map(|_| row.as_slice()).collect();
        let src = raster(&rows);
        let out = rotsprite(&src, 45.0);
        assert!(out.width > 8 && out.height > 8, "bbox should grow past 8 ({}x{})", out.width, out.height);
        // sqrt(2)*8 ≈ 11.3; allow a small downscale-rounding margin.
        assert!(out.width <= 13 && out.height <= 13, "bbox grew too far ({}x{})", out.width, out.height);
    }

    #[test]
    fn rotsprite_solid_square_stays_one_color() {
        let f = r(30, 120, 200);
        let row: Vec<Rgba> = vec![f; 10];
        let rows: Vec<&[Rgba]> = (0..10).map(|_| row.as_slice()).collect();
        let src = raster(&rows);
        let out = rotsprite(&src, 30.0);
        // Every non-transparent pixel is exactly the fill colour (no AA fringe).
        assert!(out.px.iter().filter(|c| !c.is_transparent()).all(|&c| c == f));
    }

    #[test]
    fn block_mode_picks_majority_and_breaks_ties_by_first_seen() {
        let a = r(1, 1, 1);
        let b = r(2, 2, 2);
        // Majority a (3 vs 1).
        let maj = raster(&[&[a, a], &[a, b]]);
        assert_eq!(block_mode(&maj, 0, 0, 2, 2), a);
        // 2–2 tie: `a` is seen first in raster order → wins.
        let tie = raster(&[&[a, b], &[a, b]]);
        assert_eq!(block_mode(&tie, 0, 0, 2, 2), a);
        let tie_b_first = raster(&[&[b, a], &[b, a]]);
        assert_eq!(block_mode(&tie_b_first, 0, 0, 2, 2), b);
    }

    #[test]
    fn downscale_mode_recovers_a_flat_image() {
        let c = r(5, 6, 7);
        let big = Raster { width: 16, height: 16, px: vec![c; 256] };
        let small = downscale_mode(&big, 8);
        assert_eq!((small.width, small.height), (2, 2));
        assert!(small.px.iter().all(|&p| p == c));
    }
}
