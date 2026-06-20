//! Coordinate gutter for the perception preview (SPEC-005 Phase 1, research §A).
//!
//! VLMs are blind to grid geometry — counting grid rows/cols sits near chance —
//! but *in-grid numeric labels roughly double* that accuracy ([VLMs are Blind],
//! research §A). So after we nearest-neighbor upscale a sprite ([`crate::preview`]),
//! we composite a margin band with **chunky** numeric ticks every `step` source-px
//! along the top and left. Because the upscale factor is an *integer* and the gutter
//! width is known, any (x,y) the agent reads off the labelled preview inverts back to
//! an exact source coordinate it can feed to `live_draw_pixels` — closing the
//! see→locate→fix loop the raw preview left open.
//!
//! All of the rendering is pure (buffer-in / buffer-out, no Aseprite) so the tick
//! math, the coordinate inversion, and the off-palette label-colour pick are
//! unit-tested without the live bridge (mirrors `preview.rs` / `ascii_view.rs`).
//!
//! [VLMs are Blind]: https://arxiv.org/abs/2407.06581
//!
//! Phase 1 ships this pure compositor and wires it onto `live_save_preview`
//! (`live::finish_preview`): the preview is upscaled, then — on by default whenever
//! the tick spacing is legible — composited with this gutter before the PNG is
//! written, and the band extents go into the result sidecar so the agent can invert
//! any (x,y) it names. `preview_to_source` documents that inversion for callers and
//! is exercised by the tests below.
#![allow(dead_code)]

use crate::color_ops;
use image::{Rgba, RgbaImage};

/// Default source-px between coordinate ticks. 8 px matches the project's "chunky
/// 8-px guides, never 1-px hairlines" rule (research §A) and the typical pixel-art
/// sub-tile grid.
pub const DEFAULT_GUTTER_STEP: u32 = 8;

/// Minimum preview-space spacing (`step * scale`) between ticks for the labels to be
/// legible. Below this the gutter is refused with guidance to raise `scale`/`step` —
/// the "cap like ascii_view's 64-edge" the spec calls for, expressed as a density
/// bound so it holds for any (scale, step) combination rather than a raw size.
pub const MIN_TICK_PX: u32 = 24;

/// Gutter band background — a dark neutral the bright label colour contrasts against
/// and (being in a separate margin) never overlaps the art.
const BAND_BG: Rgba<u8> = Rgba([26, 26, 26, 255]);

/// 3×5 bitmap font for digits 0–9. Each entry is 5 rows top→bottom; the low 3 bits
/// of each row are the columns (bit 2 = left, bit 0 = right). No font dependency.
const DIGIT_GLYPHS: [[u8; 5]; 10] = [
    [0b111, 0b101, 0b101, 0b101, 0b111], // 0
    [0b010, 0b110, 0b010, 0b010, 0b111], // 1
    [0b111, 0b001, 0b111, 0b100, 0b111], // 2
    [0b111, 0b001, 0b111, 0b001, 0b111], // 3
    [0b101, 0b101, 0b111, 0b001, 0b001], // 4
    [0b111, 0b100, 0b111, 0b001, 0b111], // 5
    [0b111, 0b100, 0b111, 0b101, 0b111], // 6
    [0b111, 0b001, 0b010, 0b010, 0b010], // 7
    [0b111, 0b101, 0b111, 0b101, 0b111], // 8
    [0b111, 0b101, 0b111, 0b001, 0b111], // 9
];

const GLYPH_W: u32 = 3;
const GLYPH_H: u32 = 5;

/// Source-space coordinates of the ticks along an edge: `0, step, 2·step, …` while
/// `< dim`. `0` is always present (the origin tick); `step` is floored to ≥ 1.
pub fn tick_positions(dim: u32, step: u32) -> Vec<u32> {
    let step = step.max(1);
    (0..dim).step_by(step as usize).collect()
}

/// Pixel font scale for the labels, derived from the upscale factor so labels stay
/// proportional to the art and legible. `scale/4`, floored to 1.
pub fn label_font_scale(scale: u32) -> u32 {
    (scale / 4).max(1)
}

/// Preview-space start column/row of source coordinate `c` (the first preview pixel
/// of that source cell), offset by the gutter band extent. Integer math.
pub fn source_to_preview(c: u32, scale: u32, gutter_extent: u32) -> u32 {
    gutter_extent + c * scale.max(1)
}

/// Inverse of [`source_to_preview`]: the source coordinate a preview pixel falls in.
/// Returns `None` for preview pixels inside the gutter band (before the art). The
/// identity `preview_to_source(source_to_preview(c)) == Some(c)` holds for all `c`.
pub fn preview_to_source(preview_c: u32, scale: u32, gutter_extent: u32) -> Option<u32> {
    if preview_c < gutter_extent {
        return None;
    }
    Some((preview_c - gutter_extent) / scale.max(1))
}

/// Decimal digits of `n`, most-significant first (`0 -> [0]`).
fn digits(n: u32) -> Vec<u8> {
    if n == 0 {
        return vec![0];
    }
    let mut ds = Vec::new();
    let mut v = n;
    while v > 0 {
        ds.push((v % 10) as u8);
        v /= 10;
    }
    ds.reverse();
    ds
}

/// Rendered width in px of the label for `n` at `fs` (digits are `GLYPH_W` wide with
/// a 1-px column gap, scaled by `fs`).
fn label_width(n: u32, fs: u32) -> u32 {
    let k = digits(n).len() as u32;
    // k glyphs of GLYPH_W plus (k-1) single-column gaps, all ×fs.
    (k * GLYPH_W + k.saturating_sub(1)) * fs
}

/// Pick a label colour that is maximally distant (CIELAB ΔE) from every sprite
/// palette colour *and* the band background, so labels never read as art (research
/// §A: a red marker on red pixels confused the model). Reuses `color_ops` ΔE.
pub fn pick_label_color(palette: &[color_ops::Rgba]) -> color_ops::Rgba {
    // A spread of saturated + neutral candidates; we keep the one whose nearest
    // clash (palette ∪ background) is farthest away.
    const CANDIDATES: [color_ops::Rgba; 8] = [
        color_ops::Rgba { r: 255, g: 255, b: 255, a: 255 },
        color_ops::Rgba { r: 255, g: 0, b: 255, a: 255 },
        color_ops::Rgba { r: 0, g: 255, b: 255, a: 255 },
        color_ops::Rgba { r: 255, g: 255, b: 0, a: 255 },
        color_ops::Rgba { r: 0, g: 255, b: 0, a: 255 },
        color_ops::Rgba { r: 255, g: 128, b: 0, a: 255 },
        color_ops::Rgba { r: 0, g: 128, b: 255, a: 255 },
        color_ops::Rgba { r: 255, g: 0, b: 128, a: 255 },
    ];
    let bg = color_ops::Rgba { r: BAND_BG.0[0], g: BAND_BG.0[1], b: BAND_BG.0[2], a: 255 };
    let min_clash = |c: color_ops::Rgba| -> f64 {
        let mut m = color_ops::delta_e(c, bg);
        for p in palette {
            m = m.min(color_ops::delta_e(c, *p));
        }
        m
    };
    let mut best = CANDIDATES[0];
    let mut best_d = min_clash(best);
    for &c in &CANDIDATES[1..] {
        let d = min_clash(c);
        if d > best_d {
            best_d = d;
            best = c;
        }
    }
    best
}

/// Upper bound on distinct colours sampled for the label-colour pick. Pixel art is
/// small-paletted, so this is only a defensive ceiling on the ΔE work for a stray
/// photographic source.
const PALETTE_SAMPLE_CAP: usize = 256;

/// Sample the distinct opaque colours of the (already upscaled) preview `img`, one
/// sample per source cell by stepping `scale` px, so [`pick_label_color`] can steer
/// the gutter label off the sprite's own colours (research §A: a red marker on red
/// pixels confused the model). Fully-transparent pixels are ignored; sampling stops
/// at [`PALETTE_SAMPLE_CAP`] distinct colours.
pub fn sprite_palette(img: &RgbaImage, scale: u32) -> Vec<color_ops::Rgba> {
    let scale = scale.max(1);
    let mut seen = std::collections::HashSet::new();
    let mut palette = Vec::new();
    let mut y = 0;
    while y < img.height() {
        let mut x = 0;
        while x < img.width() {
            let p = img.get_pixel(x, y).0;
            if p[3] != 0 && seen.insert([p[0], p[1], p[2]]) {
                palette.push(color_ops::Rgba::rgb(p[0], p[1], p[2]));
                if palette.len() >= PALETTE_SAMPLE_CAP {
                    return palette;
                }
            }
            x += scale;
        }
        y += scale;
    }
    palette
}

fn put(img: &mut RgbaImage, x: u32, y: u32, color: Rgba<u8>) {
    if x < img.width() && y < img.height() {
        img.put_pixel(x, y, color);
    }
}

/// Draw one decimal label with its left edge at `(x, y)` in `color` at scale `fs`.
fn draw_label(img: &mut RgbaImage, mut x: u32, y: u32, n: u32, fs: u32, color: Rgba<u8>) {
    for d in digits(n) {
        let glyph = &DIGIT_GLYPHS[d as usize];
        for (row, bits) in glyph.iter().enumerate() {
            for col in 0..GLYPH_W {
                // bit 2 = leftmost column.
                if bits & (1 << (GLYPH_W - 1 - col)) != 0 {
                    for dy in 0..fs {
                        for dx in 0..fs {
                            put(img, x + col * fs + dx, y + row as u32 * fs + dy, color);
                        }
                    }
                }
            }
        }
        x += (GLYPH_W + 1) * fs; // advance one glyph + 1-col gap
    }
}

/// What [`render_with_gutter`] produced — the band extents let the caller invert any
/// preview (x,y) the agent names back to a source coordinate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GutterInfo {
    pub left_w: u32,
    pub top_h: u32,
    pub scale: u32,
    pub step: u32,
    pub out_width: u32,
    pub out_height: u32,
}

/// Composite a labelled coordinate gutter (top + left) around an already-upscaled
/// preview `img`. `scale` is the integer upscale factor used to produce `img`;
/// `step` is the source-px tick spacing; `palette` (sprite colours) steers the
/// off-art label colour. Returns the bigger image + the band extents, or an error
/// when the tick spacing (`step * scale`) cannot fit the rendered labels legibly —
/// the floor is `MIN_TICK_PX` *and* the widest/tallest label box, so multi-digit
/// labels at a small `step`/large `scale` can't silently collide.
pub fn render_with_gutter(
    img: &RgbaImage,
    scale: u32,
    step: u32,
    palette: &[color_ops::Rgba],
) -> Result<(RgbaImage, GutterInfo), String> {
    let scale = scale.max(1);
    let step = step.max(1);
    let (pw, ph) = (img.width(), img.height());
    if pw == 0 || ph == 0 {
        return Err(format!("gutter source has a zero dimension ({pw}x{ph})"));
    }
    let src_w = pw / scale;
    let src_h = ph / scale;

    let fs = label_font_scale(scale);
    let xticks = tick_positions(src_w, step);
    let yticks = tick_positions(src_h, step);
    let max_x_label = *xticks.last().unwrap_or(&0);
    let max_y_label = *yticks.last().unwrap_or(&0);

    // Legibility floor. Ticks sit `step * scale` px apart in preview space and labels
    // are centred on them, so the gutter is only readable when that spacing also clears
    // the label box: the widest x-label must not overlap its neighbour, and a stacked
    // y-label must not overlap vertically. Gating on raw density (`step * scale`) alone
    // lets multi-digit labels collide at a small step / large scale — an unreadable
    // gutter is worse than none (research §A) — so fold the label extent into the floor.
    let spacing = step * scale;
    let widest_label = label_width(max_x_label, fs).max(label_width(max_y_label, fs));
    let tallest_label = GLYPH_H * fs;
    let floor = MIN_TICK_PX.max(widest_label + fs).max(tallest_label + fs);
    if spacing < floor {
        return Err(format!(
            "gutter tick spacing {spacing}px (step {step} × scale {scale}) is below the {floor}px \
             legibility floor (labels up to {widest_label}px wide / {tallest_label}px tall) — \
             raise the preview scale or the gutter step, or crop first"
        ));
    }

    // Band extents: top fits a label row + a tick stub; left fits the widest y-label.
    let left_w = label_width(max_y_label, fs) + 3 * fs; // label + tick stub + pad
    let top_h = GLYPH_H * fs + 3 * fs; // label + tick stub + pad

    let color = pick_label_color(palette);
    let label_col = Rgba([color.r, color.g, color.b, 255]);

    let out_w = left_w + pw;
    let out_h = top_h + ph;
    let mut out = RgbaImage::from_pixel(out_w, out_h, Rgba([0, 0, 0, 0]));

    // Fill the two gutter bands (the art quadrant stays transparent until the blit).
    for y in 0..out_h {
        for x in 0..out_w {
            if x < left_w || y < top_h {
                out.put_pixel(x, y, BAND_BG);
            }
        }
    }
    // Blit the upscaled art into the bottom-right quadrant.
    for y in 0..ph {
        for x in 0..pw {
            out.put_pixel(left_w + x, top_h + y, *img.get_pixel(x, y));
        }
    }

    // Top gutter: x-labels centred on their tick column + a tick stub.
    let label_y = top_h.saturating_sub(GLYPH_H * fs + fs);
    for &tx in &xticks {
        let col_px = source_to_preview(tx, scale, left_w);
        let lw = label_width(tx, fs);
        let lx = col_px.saturating_sub(lw / 2).max(left_w);
        draw_label(&mut out, lx, label_y, tx, fs, label_col);
        for dy in 0..(2 * fs) {
            put(&mut out, col_px, top_h.saturating_sub(1).saturating_sub(dy), label_col);
        }
    }
    // Left gutter: y-labels vertically centred on their tick row + a tick stub.
    for &ty in &yticks {
        let row_px = source_to_preview(ty, scale, top_h);
        let ly = row_px.saturating_sub(GLYPH_H * fs / 2);
        let lw = label_width(ty, fs);
        let lx = left_w.saturating_sub(lw + 2 * fs);
        draw_label(&mut out, lx, ly, ty, fs, label_col);
        for dx in 0..(2 * fs) {
            put(&mut out, left_w.saturating_sub(1).saturating_sub(dx), row_px, label_col);
        }
    }

    Ok((
        out,
        GutterInfo {
            left_w,
            top_h,
            scale,
            step,
            out_width: out_w,
            out_height: out_h,
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ticks_start_at_origin_and_step() {
        assert_eq!(tick_positions(32, 8), vec![0, 8, 16, 24]);
        assert_eq!(tick_positions(16, 8), vec![0, 8]);
        assert_eq!(tick_positions(10, 4), vec![0, 4, 8]);
        assert_eq!(tick_positions(8, 0), vec![0, 1, 2, 3, 4, 5, 6, 7]); // step floored to 1
    }

    #[test]
    fn coordinate_inversion_is_exact() {
        // The whole point of the gutter: a preview coord the agent reads inverts to
        // the exact source coordinate. Identity must hold for every (scale, gutter, c).
        for &scale in &[1u32, 4, 8, 16] {
            for &gutter in &[0u32, 7, 40, 123] {
                for c in 0..40u32 {
                    let p = source_to_preview(c, scale, gutter);
                    assert_eq!(preview_to_source(p, scale, gutter), Some(c));
                    // Any preview pixel within the source cell still inverts to c.
                    let mid = p + scale / 2;
                    assert_eq!(preview_to_source(mid, scale, gutter), Some(c));
                }
                // Inside the band there is no source coordinate.
                if gutter > 0 {
                    assert_eq!(preview_to_source(gutter - 1, scale, gutter), None);
                }
            }
        }
    }

    #[test]
    fn label_colour_is_off_palette_and_off_band() {
        // A grayscale palette (incl. near-black like the band) must not get a gray
        // label; the pick must be far in ΔE from every clash.
        let palette = vec![
            color_ops::Rgba::rgb(0, 0, 0),
            color_ops::Rgba::rgb(64, 64, 64),
            color_ops::Rgba::rgb(128, 128, 128),
            color_ops::Rgba::rgb(200, 200, 200),
            color_ops::Rgba::rgb(255, 255, 255),
        ];
        let c = pick_label_color(&palette);
        let bg = color_ops::Rgba::rgb(BAND_BG.0[0], BAND_BG.0[1], BAND_BG.0[2]);
        assert!(color_ops::delta_e(c, bg) > 20.0, "label clashes with band: {c:?}");
        for p in &palette {
            assert!(
                color_ops::delta_e(c, *p) > 20.0,
                "label {c:?} too close to palette {p:?}"
            );
        }
    }

    #[test]
    fn label_width_counts_digits() {
        // fs=1: 1 digit = 3px; 2 digits = 3+1+3 = 7px; 3 digits = 11px.
        assert_eq!(label_width(0, 1), 3);
        assert_eq!(label_width(7, 1), 3);
        assert_eq!(label_width(16, 1), 7);
        assert_eq!(label_width(128, 1), 11);
        assert_eq!(label_width(16, 2), 14);
    }

    #[test]
    fn refuses_unreadable_tick_density() {
        let img = RgbaImage::from_pixel(64, 64, Rgba([10, 20, 30, 255]));
        // scale 1, step 8 -> 8px spacing < 24 floor -> refused.
        assert!(render_with_gutter(&img, 1, 8, &[]).is_err());
        // scale 16, step 8 -> 128px spacing -> fine.
        assert!(render_with_gutter(&img, 16, 8, &[]).is_ok());
    }

    #[test]
    fn refuses_when_multidigit_labels_would_overlap() {
        // src 110x4 upscaled 12x. step 2 -> 24px spacing CLEARS the raw MIN_TICK_PX
        // floor, but the largest x-label is "108" which at fs=3 is 33px wide and would
        // overlap its neighbour — the label-extent floor must still refuse it.
        assert!(24 >= MIN_TICK_PX, "precondition: raw density alone would pass");
        let img = RgbaImage::from_pixel(1320, 48, Rgba([10, 20, 30, 255]));
        assert!(
            render_with_gutter(&img, 12, 2, &[]).is_err(),
            "24px spacing must be refused when a 33px label would overlap"
        );
        // Widen the step: 48px spacing clears the ~36px label floor -> OK.
        assert!(render_with_gutter(&img, 12, 4, &[]).is_ok());
    }

    #[test]
    fn render_grows_the_image_and_blits_the_art_unchanged() {
        // 4x4 source upscaled 16x = 64x64 preview, distinct corner colours.
        let scale = 16u32;
        let mut src = RgbaImage::from_pixel(64, 64, Rgba([0, 0, 0, 255]));
        let marker = Rgba([200, 50, 25, 255]);
        src.put_pixel(0, 0, marker); // top-left preview pixel
        let palette = vec![color_ops::Rgba::rgb(0, 0, 0), color_ops::Rgba::rgb(200, 50, 25)];

        let (out, info) = render_with_gutter(&src, scale, 8, &palette).unwrap();
        assert_eq!((out.width(), out.height()), (info.out_width, info.out_height));
        assert_eq!(info.out_width, info.left_w + 64);
        assert_eq!(info.out_height, info.top_h + 64);

        // The art is blitted byte-for-byte into the bottom-right quadrant.
        assert_eq!(*out.get_pixel(info.left_w, info.top_h), marker);
        for y in 0..64 {
            for x in 0..64 {
                assert_eq!(out.get_pixel(info.left_w + x, info.top_h + y), src.get_pixel(x, y));
            }
        }
        // The gutter bands are filled (not transparent).
        assert_eq!(*out.get_pixel(0, 0), BAND_BG);
    }

    #[test]
    fn sprite_palette_collects_distinct_opaque_colours() {
        // A 2x2 source upscaled 4x: each 4x4 block is one solid colour; transparent
        // pixels are skipped; duplicates collapse.
        let scale = 4u32;
        let mut img = RgbaImage::from_pixel(8, 8, Rgba([0, 0, 0, 0])); // transparent
        let red = Rgba([200, 30, 20, 255]);
        let blue = Rgba([20, 30, 200, 255]);
        for dy in 0..scale {
            for dx in 0..scale {
                img.put_pixel(dx, dy, red); // top-left cell
                img.put_pixel(scale + dx, dy, red); // top-right cell, same colour
                img.put_pixel(dx, scale + dy, blue); // bottom-left cell
                // bottom-right cell stays transparent
            }
        }
        let pal = sprite_palette(&img, scale);
        assert_eq!(pal.len(), 2, "two distinct opaque colours expected: {pal:?}");
        assert!(pal.contains(&color_ops::Rgba::rgb(200, 30, 20)));
        assert!(pal.contains(&color_ops::Rgba::rgb(20, 30, 200)));
        // A fully-transparent buffer yields no palette (pick falls back to a default).
        let blank = RgbaImage::from_pixel(8, 8, Rgba([0, 0, 0, 0]));
        assert!(sprite_palette(&blank, scale).is_empty());
    }

    #[test]
    fn render_draws_label_pixels_in_the_band() {
        let scale = 16u32;
        let src = RgbaImage::from_pixel(64, 64, Rgba([0, 0, 0, 255]));
        let palette = vec![color_ops::Rgba::rgb(0, 0, 0)];
        let (out, info) = render_with_gutter(&src, scale, 8, &palette).unwrap();
        let label = pick_label_color(&palette);
        let label_px = Rgba([label.r, label.g, label.b, 255]);

        // At least one label-coloured pixel exists inside the top band (a drawn digit).
        let mut found = false;
        'outer: for y in 0..info.top_h {
            for x in 0..info.out_width {
                if *out.get_pixel(x, y) == label_px {
                    found = true;
                    break 'outer;
                }
            }
        }
        assert!(found, "no label pixels rendered in the top gutter band");
    }
}
