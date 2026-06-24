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

/// Smallest native edge that still counts as a *real* upscale. The block-uniformity grid
/// detector degenerates on a fully-uniform source (a solid swatch / flat background): every
/// block at every divisor is mode-uniform, so it reports `scale = gcd(w, h)` and a native of
/// ~1×1. A genuine sprite is at least a few pixels per side, so a smaller native is that
/// degenerate collapse, not an upscale — see `is_real_upscale`.
pub const MIN_NATIVE_EDGE: u32 = 4;

/// Whether a `style_profile::detect_grid` result is a real upscale worth honouring for the
/// import `regrid` path: an actual scale (`> 1`) AND a plausible (non-degenerate) native
/// grid. Guards the all-uniform collapse (`scale == gcd`, native ~1×1) that would otherwise
/// import the entire reference as a single cell while reporting a confident "recovery".
pub fn is_real_upscale(scale: u32, native_w: u32, native_h: u32) -> bool {
    scale > 1 && native_w >= MIN_NATIVE_EDGE && native_h >= MIN_NATIVE_EDGE
}

/// The regrid two-pass: recover a scaled reference to its exact native grid, then fit to the
/// requested `target`. When `target == native` it is a single dominant-vote pass (which both
/// recovers and snaps in one go); otherwise the native is recovered *without* snapping first,
/// so the final fit downscales clean 1× pixels rather than the scaled blur. Pure.
pub fn regrid_then_fit(
    src: &RgbaImage,
    native: (u32, u32),
    target: (u32, u32),
    palette: Option<&[CRgba]>,
    method: Method,
) -> RgbaImage {
    if target == native {
        return downscale_to_grid(src, target.0, target.1, palette, method);
    }
    // Recovering native is bounded by the source area / scale² (≤ ¼ of an already-permitted
    // ≤MAX_SOURCE_EDGE buffer), and is freed once the fit pass produces the ≤MAX_TARGET_EDGE result.
    let recovered = downscale_to_grid(src, native.0, native.1, None, method);
    downscale_to_grid(&recovered, target.0, target.1, palette, method)
}

/// Largest number of frames `live_import_animation` will lay down in one call.
pub const MAX_ANIM_FRAMES: u32 = 64;

/// Slice a sprite-sheet into `cols * rows` equal frames in row-major order (left→right,
/// then top→bottom — the canonical sheet layout). `width % cols` and `height % rows` must be
/// zero (a sheet's frames are equal cells); a non-divisible sheet or a zero `cols`/`rows` is a
/// loud `Err`. Each frame is `width/cols × height/rows`. Pure (image-in / images-out).
pub fn slice_sheet(img: &RgbaImage, cols: u32, rows: u32) -> Result<Vec<RgbaImage>, String> {
    if cols == 0 || rows == 0 {
        return Err(format!("sheet cols/rows must be ≥1 (got {cols}×{rows})"));
    }
    let (w, h) = (img.width(), img.height());
    if w % cols != 0 || h % rows != 0 {
        return Err(format!(
            "sheet {w}×{h} is not evenly divisible by {cols}×{rows} cells \
             (need width % cols == 0 and height % rows == 0)"
        ));
    }
    let (fw, fh) = (w / cols, h / rows);
    let mut frames = Vec::with_capacity((cols * rows) as usize);
    for r in 0..rows {
        for c in 0..cols {
            // crop the (c,r) cell; copy is bounded by the already-decoded ≤MAX_SOURCE_EDGE buffer.
            let mut frame = RgbaImage::new(fw, fh);
            for y in 0..fh {
                for x in 0..fw {
                    frame.put_pixel(x, y, *img.get_pixel(c * fw + x, r * fh + y));
                }
            }
            frames.push(frame);
        }
    }
    Ok(frames)
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
    fn slice_sheet_splits_row_major_into_equal_frames() {
        // 4×4 sheet as a 2×2 grid of four 2×2 cells, each a distinct solid colour.
        let mut img = RgbaImage::new(4, 4);
        let cell = [
            [10, 0, 0, 255],   // (col0,row0) top-left
            [0, 20, 0, 255],   // (col1,row0) top-right
            [0, 0, 30, 255],   // (col0,row1) bottom-left
            [40, 40, 0, 255],  // (col1,row1) bottom-right
        ];
        for r in 0..2u32 {
            for c in 0..2u32 {
                let color = cell[(r * 2 + c) as usize];
                for y in 0..2 {
                    for x in 0..2 {
                        px(&mut img, c * 2 + x, r * 2 + y, color);
                    }
                }
            }
        }
        let frames = slice_sheet(&img, 2, 2).expect("divisible sheet");
        assert_eq!(frames.len(), 4);
        // Row-major order matches the cell table; every frame is a uniform 2×2.
        for (i, f) in frames.iter().enumerate() {
            assert_eq!((f.width(), f.height()), (2, 2));
            for p in f.pixels() {
                assert_eq!(p.0, cell[i], "frame {i} should be one solid colour");
            }
        }
        // Non-divisible dims and zero cols/rows are loud errors.
        assert!(slice_sheet(&img, 3, 2).is_err()); // 4 % 3 != 0
        assert!(slice_sheet(&img, 0, 2).is_err());
        assert!(slice_sheet(&img, 2, 0).is_err());
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

    /// Nearest-neighbour ×n upscale (the "fake pixel art" operator) for the regrid test.
    fn upscale(src: &RgbaImage, n: u32) -> RgbaImage {
        let mut out = RgbaImage::new(src.width() * n, src.height() * n);
        for y in 0..out.height() {
            for x in 0..out.width() {
                out.put_pixel(x, y, *src.get_pixel(x / n, y / n));
            }
        }
        out
    }

    #[test]
    fn regrid_recovers_native_exactly() {
        // The SPEC-006 Phase 2 guarantee: a scaled reference, de-faked, is recovered to its
        // true 1× grid bit-for-bit. Build a varied 8×8 (adjacent pixels differ, so it reads
        // as native), upscale 4×, then detect + downscale-to-native (the import regrid path).
        let pal = [
            Rgba([200, 40, 40, 255]),
            Rgba([40, 200, 60, 255]),
            Rgba([50, 60, 220, 255]),
            Rgba([230, 210, 40, 255]),
        ];
        let mut native = RgbaImage::new(8, 8);
        for y in 0..8 {
            for x in 0..8 {
                // Diagonal 4-colour stripes. After a 4× upscale the image is mode-uniform at
                // every divisor cell up to 4 (n=2 and n=4 both pass), but a 4-colour diagonal
                // is never uniform at n=8; since detect_grid keeps the LARGEST passing divisor,
                // the detected scale is 4 (not 2, and not over-shooting to 8).
                native.put_pixel(x, y, pal[((x + y) % 4) as usize]);
            }
        }
        let up = upscale(&native, 4);

        // detect_grid (the shared block-uniformity detector) sees the 4× scale.
        let grid = crate::style_profile::detect_grid(&up);
        assert_eq!((grid.scale, grid.native), (4, [8, 8]), "{grid:?}");

        // Downscaling the upscaled image back to its native dims recovers it exactly.
        let recovered = downscale_to_grid(&up, grid.native[0], grid.native[1], None, Method::Dominant);
        assert_eq!(recovered.dimensions(), (8, 8));
        for y in 0..8 {
            for x in 0..8 {
                assert_eq!(recovered.get_pixel(x, y), native.get_pixel(x, y), "px {x},{y}");
            }
        }
        // The regrid helper's single-pass branch (target == native) recovers identically.
        assert_eq!(regrid_then_fit(&up, (8, 8), (8, 8), None, Method::Dominant), recovered);
    }

    #[test]
    fn solid_source_is_not_a_real_upscale() {
        // The degenerate the block-uniformity detector hits: a flat swatch reports scale ==
        // gcd(w,h) and a native of ~1×1. is_real_upscale rejects it so regrid no-ops instead
        // of importing the whole reference as a single cell.
        let solid = RgbaImage::from_pixel(64, 64, Rgba([120, 80, 200, 255]));
        let g = crate::style_profile::detect_grid(&solid);
        assert!(g.scale > 1 && g.native == [1, 1], "expected degenerate detection: {g:?}");
        assert!(!is_real_upscale(g.scale, g.native[0], g.native[1]), "must not honour a 1×1 native");
        // A genuine small upscale IS honoured; native art (scale 1) is not an upscale.
        assert!(is_real_upscale(4, 8, 8));
        assert!(!is_real_upscale(1, 64, 64));
    }

    #[test]
    fn regrid_two_pass_matches_true_native_fit() {
        // The two-pass path (target != native): recover the native from the blur first, then
        // fit — must equal fitting the TRUE native straight to the target, and differ from a
        // naive single downscale of the scaled blur (the quality bug the two-pass avoids).
        let pal = [
            Rgba([200, 40, 40, 255]),
            Rgba([40, 200, 60, 255]),
            Rgba([50, 60, 220, 255]),
            Rgba([230, 210, 40, 255]),
        ];
        let mut native = RgbaImage::new(8, 8);
        for y in 0..8 {
            for x in 0..8 {
                native.put_pixel(x, y, pal[((x + y) % 4) as usize]);
            }
        }
        let up = upscale(&native, 4); // 32×32, a clean 4× upscale
        let target = (6, 6);
        let truth = downscale_to_grid(&native, target.0, target.1, None, Method::Dominant);
        let two_pass = regrid_then_fit(&up, (8, 8), target, None, Method::Dominant);
        assert_eq!(two_pass, truth, "two-pass must equal fitting the true native");
        let single = downscale_to_grid(&up, target.0, target.1, None, Method::Dominant);
        assert_ne!(single, truth, "single-pass-from-blur should differ — the bug two-pass fixes");
    }
}
