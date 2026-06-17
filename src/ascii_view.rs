//! Text-grid readback of a sprite (Perception fast-follow, research doc Path 1 / §A).
//!
//! A vision model reads a small sprite image poorly — it cannot reliably attend to
//! an individual pixel or count grid cells (VLMs-are-blind; Anthropic vision patch
//! math). But LLMs read a *text* grid where each cell is one token far better than
//! they read the image (Text2Space). So this renders the active frame as one glyph
//! per pixel with row/column rulers and a colour legend, giving the agent an exact,
//! token-space view to VERIFY its work (and one that works for non-vision clients
//! like Codex). The image decoding/upscaling stays in the existing preview pipeline;
//! this module is the pure pixels→text transform so it is unit-testable.

use std::collections::{HashMap, HashSet};

use image::RgbaImage;

/// One stable glyph per distinct opaque colour, assigned in luma order so the
/// darkest colour is `0`. 62 glyphs cover any sane pixel-art palette.
const GLYPHS: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ";

/// Refuse a sprite whose width OR height exceeds this (so the grid is at most
/// 64×64): beyond it a single text row is too wide for a model to read reliably,
/// and the caller should crop a region first. A per-edge cap (not an area cap)
/// keeps the documented "64×64" contract honest for non-square sprites — a
/// 256×16 sprite has only 4096 cells but a 256-glyph row is unreadable.
pub const ASCII_MAX_EDGE: u32 = 64;

fn luma(c: &[u8; 4]) -> u32 {
    299 * c[0] as u32 + 587 * c[1] as u32 + 114 * c[2] as u32
}

/// Render an RGBA image as a one-glyph-per-pixel text grid + legend.
/// `.` = transparent; each distinct opaque colour gets a glyph (legend maps it to
/// `#rrggbb`). Columns carry a two-line (tens/units) ruler; rows are labelled.
pub fn image_to_ascii(img: &RgbaImage) -> Result<String, String> {
    let (w, h) = img.dimensions();
    if w == 0 || h == 0 {
        return Err("image has a zero dimension".to_string());
    }
    if w > ASCII_MAX_EDGE || h > ASCII_MAX_EDGE {
        return Err(format!(
            "{w}x{h} exceeds the {ASCII_MAX_EDGE}x{ASCII_MAX_EDGE} cap (each edge ≤ {ASCII_MAX_EDGE}); crop a region first"
        ));
    }

    // Distinct opaque colours, sorted by luma (then RGBA) for a deterministic glyph order.
    let mut uniq: HashSet<[u8; 4]> = HashSet::new();
    for p in img.pixels() {
        if p.0[3] != 0 {
            uniq.insert(p.0);
        }
    }
    let mut colours: Vec<[u8; 4]> = uniq.into_iter().collect();
    colours.sort_by_key(|c| (luma(c), c[0], c[1], c[2], c[3]));
    if colours.len() > GLYPHS.len() {
        return Err(format!(
            "{} distinct colours exceeds the {}-glyph budget; reduce the palette or crop",
            colours.len(),
            GLYPHS.len()
        ));
    }
    let glyph: HashMap<[u8; 4], char> = colours
        .iter()
        .enumerate()
        .map(|(i, c)| (*c, GLYPHS[i] as char))
        .collect();

    let row_w = (h.saturating_sub(1)).to_string().len().max(2);
    let pad: String = " ".repeat(row_w + 1);
    let digit = |d: u32| char::from_digit(d % 10, 10).unwrap();

    let mut out = String::new();
    out.push_str(&format!("{w}x{h}  '.'=transparent\n"));
    // Column tens ruler, then units ruler.
    out.push_str(&pad);
    for x in 0..w {
        out.push(digit(x / 10));
    }
    out.push('\n');
    out.push_str(&pad);
    for x in 0..w {
        out.push(digit(x));
    }
    out.push('\n');
    // Rows.
    for y in 0..h {
        out.push_str(&format!("{:>width$} ", y, width = row_w));
        for x in 0..w {
            let p = img.get_pixel(x, y).0;
            out.push(if p[3] == 0 {
                '.'
            } else {
                *glyph.get(&p).unwrap()
            });
        }
        out.push('\n');
    }
    // Legend.
    out.push_str("legend:");
    for (c, g) in colours.iter().zip(GLYPHS.iter()) {
        out.push_str(&format!(" {}=#{:02x}{:02x}{:02x}", *g as char, c[0], c[1], c[2]));
    }
    out.push('\n');
    Ok(out)
}

/// Diff two same-size frames as a text grid: `.` = unchanged, `-` = became
/// transparent, otherwise the glyph of the **new** colour at that cell (legend maps
/// glyph → `#rrggbb` over the changed cells' colours). The header reports the
/// changed-cell count. Lets the agent see EXACTLY what an edit changed, or where two
/// animation frames differ at the pixel level (research Path 1 / §A).
pub fn diff_to_ascii(a: &RgbaImage, b: &RgbaImage) -> Result<String, String> {
    if a.dimensions() != b.dimensions() {
        return Err(format!(
            "frame sizes differ: {:?} vs {:?}",
            a.dimensions(),
            b.dimensions()
        ));
    }
    let (w, h) = a.dimensions();
    if w == 0 || h == 0 {
        return Err("image has a zero dimension".to_string());
    }
    if w > ASCII_MAX_EDGE || h > ASCII_MAX_EDGE {
        return Err(format!(
            "{w}x{h} exceeds the {ASCII_MAX_EDGE}x{ASCII_MAX_EDGE} cap (each edge ≤ {ASCII_MAX_EDGE}); crop a region first"
        ));
    }

    // Distinct NEW (frame-b) colours at changed cells, for the legend/glyphs.
    let mut changed_colours: HashSet<[u8; 4]> = HashSet::new();
    let mut changed = 0u32;
    for y in 0..h {
        for x in 0..w {
            let (pa, pb) = (a.get_pixel(x, y).0, b.get_pixel(x, y).0);
            if pa != pb {
                changed += 1;
                if pb[3] != 0 {
                    changed_colours.insert(pb);
                }
            }
        }
    }
    let mut colours: Vec<[u8; 4]> = changed_colours.into_iter().collect();
    colours.sort_by_key(|c| (luma(c), c[0], c[1], c[2], c[3]));
    if colours.len() > GLYPHS.len() {
        return Err(format!(
            "{} changed colours exceeds the {}-glyph budget",
            colours.len(),
            GLYPHS.len()
        ));
    }
    let glyph: HashMap<[u8; 4], char> = colours
        .iter()
        .enumerate()
        .map(|(i, c)| (*c, GLYPHS[i] as char))
        .collect();

    let row_w = (h.saturating_sub(1)).to_string().len().max(2);
    let pad: String = " ".repeat(row_w + 1);
    let digit = |d: u32| char::from_digit(d % 10, 10).unwrap();

    let mut out = String::new();
    out.push_str(&format!(
        "{w}x{h}  {changed} cells changed  '.'=unchanged '-'=erased\n"
    ));
    out.push_str(&pad);
    for x in 0..w {
        out.push(digit(x / 10));
    }
    out.push('\n');
    out.push_str(&pad);
    for x in 0..w {
        out.push(digit(x));
    }
    out.push('\n');
    for y in 0..h {
        out.push_str(&format!("{:>width$} ", y, width = row_w));
        for x in 0..w {
            let (pa, pb) = (a.get_pixel(x, y).0, b.get_pixel(x, y).0);
            out.push(if pa == pb {
                '.'
            } else if pb[3] == 0 {
                '-'
            } else {
                *glyph.get(&pb).unwrap()
            });
        }
        out.push('\n');
    }
    out.push_str("legend(new):");
    for (c, g) in colours.iter().zip(GLYPHS.iter()) {
        out.push_str(&format!(" {}=#{:02x}{:02x}{:02x}", *g as char, c[0], c[1], c[2]));
    }
    out.push('\n');
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    fn img2x2() -> RgbaImage {
        let mut img = RgbaImage::new(2, 2);
        img.put_pixel(0, 0, Rgba([0, 0, 0, 255])); // black -> lowest luma -> '0'
        img.put_pixel(1, 0, Rgba([255, 255, 255, 255])); // white -> '1'
        img.put_pixel(0, 1, Rgba([0, 0, 0, 0])); // transparent -> '.'
        img.put_pixel(1, 1, Rgba([255, 255, 255, 255])); // white -> '1'
        img
    }

    #[test]
    fn maps_colours_to_luma_ordered_glyphs() {
        let s = image_to_ascii(&img2x2()).unwrap();
        assert!(s.contains(" 0 01\n"), "row 0 should be '01':\n{s}");
        assert!(s.contains(" 1 .1\n"), "row 1 should be '.1':\n{s}");
        assert!(s.contains("0=#000000"), "legend black:\n{s}");
        assert!(s.contains("1=#ffffff"), "legend white:\n{s}");
    }

    #[test]
    fn has_a_two_line_column_ruler() {
        // 12-wide: tens ruler shows the '1' starting at column 10.
        let img = RgbaImage::new(12, 1);
        let s = image_to_ascii(&img).unwrap();
        let lines: Vec<&str> = s.lines().collect();
        // lines[0] = header, [1] = tens ruler, [2] = units ruler
        assert!(lines[1].trim_start().starts_with("0000000000"), "tens: {:?}", lines[1]);
        assert!(lines[1].trim_start().ends_with("11"), "tens: {:?}", lines[1]);
        assert!(lines[2].trim_start().starts_with("0123456789"), "units: {:?}", lines[2]);
    }

    #[test]
    fn rejects_oversized_grids() {
        // Per-edge cap: a wide, low-cell-count sprite is refused too (a 256-glyph
        // row is unreadable) even though 256*16 = 4096 cells is small.
        assert!(image_to_ascii(&RgbaImage::new(65, 65)).unwrap_err().contains("exceeds"));
        assert!(image_to_ascii(&RgbaImage::new(256, 16)).unwrap_err().contains("exceeds"));
        assert!(image_to_ascii(&RgbaImage::new(65, 10)).unwrap_err().contains("exceeds"));
        // A 64x64 sprite (the boundary) is still accepted.
        assert!(image_to_ascii(&RgbaImage::new(64, 64)).is_ok());
    }

    #[test]
    fn transparent_only_image_has_no_legend_entries() {
        let img = RgbaImage::new(3, 3); // all alpha 0
        let s = image_to_ascii(&img).unwrap();
        assert!(s.contains("legend:\n"), "empty legend:\n{s}");
        assert!(s.contains(" 0 ...\n"));
    }

    #[test]
    fn diff_marks_changed_cells_with_the_new_glyph() {
        let mut a = RgbaImage::from_pixel(2, 1, Rgba([0, 0, 0, 255]));
        let mut b = a.clone();
        b.put_pixel(1, 0, Rgba([255, 0, 0, 255])); // cell (1,0) black -> red
        let _ = &mut a;
        let s = diff_to_ascii(&a, &b).unwrap();
        assert!(s.contains("1 cells changed"), "{s}");
        assert!(s.contains(" 0 .0\n"), "unchanged '.' then new-colour glyph:\n{s}");
        assert!(s.contains("legend(new): 0=#ff0000"), "{s}");
    }

    #[test]
    fn diff_of_identical_frames_is_all_dots() {
        let a = RgbaImage::from_pixel(3, 1, Rgba([1, 2, 3, 255]));
        let s = diff_to_ascii(&a, &a).unwrap();
        assert!(s.contains("0 cells changed"), "{s}");
        assert!(s.contains(" 0 ...\n"), "{s}");
    }

    #[test]
    fn diff_marks_erased_pixels_with_dash() {
        let a = RgbaImage::from_pixel(1, 1, Rgba([0, 0, 0, 255]));
        let b = RgbaImage::new(1, 1); // became transparent
        let s = diff_to_ascii(&a, &b).unwrap();
        assert!(s.contains(" 0 -\n"), "erased cell is '-':\n{s}");
    }

    #[test]
    fn diff_rejects_mismatched_sizes() {
        let a = RgbaImage::new(2, 2);
        let b = RgbaImage::new(3, 2);
        assert!(diff_to_ascii(&a, &b).unwrap_err().contains("sizes differ"));
    }
}
