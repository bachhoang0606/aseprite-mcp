//! Set-of-Mark numbered regions for the perception preview (SPEC-005 Phase 4, §A).
//!
//! VLMs ground *free-form* coordinates poorly but read *numbered* marks well: overlay a
//! small numbered badge on each region — a slice, a layer's cel, or a connected
//! component — and let the critic say "region 3 has a stray pixel". The server maps
//! `3 → that slice/layer/component` deterministically (the returned `[{n, region, bbox}]`
//! map), sidestepping the VLM's coordinate weakness entirely. Pixel art segments for
//! free by slice / layer / connected-component, so no SAM/ML is needed (research §A SoM).
//!
//! All of this is pure (struct/buffer in, struct/buffer out) so the connected-component
//! pass, the centroid/bbox math, the mark numbering, and the badge compositor are
//! unit-tested without the live bridge (mirrors `gutter.rs` / `preview.rs`). The badge
//! font is the one shared bitmap font in `gutter.rs`.
#![allow(dead_code)]

use crate::gutter;
use image::{Rgba, RgbaImage};

/// A source-space axis-aligned region rectangle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MarkRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl MarkRect {
    /// The bbox centre (rounded down) — the badge anchor and a stable centroid for the
    /// rectangular region sources (slices / layer cels).
    pub fn center(&self) -> (u32, u32) {
        (self.x + self.width / 2, self.y + self.height / 2)
    }
}

/// A candidate region to mark, in source (sprite) coordinates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Region {
    pub name: String,
    pub bbox: MarkRect,
}

/// A placed mark: its number, the region it points at, and that region's bbox — the
/// `[{n, region, bbox}]` map the orchestrator inverts (`n → slice/layer/component`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mark {
    pub n: u32,
    pub region: String,
    pub bbox: MarkRect,
}

/// Badge font scale, a touch larger than the gutter tick labels so a number reads at a
/// glance on the art. `scale/3`, floored to 2 so it stays legible even at low upscale.
pub fn badge_font_scale(scale: u32) -> u32 {
    (scale / 3).max(2)
}

/// Number `regions` `1..=N` in their given order → marks. Pure; the inverse `n → region`
/// is just `marks[n-1]` (asserted by the round-trip test).
pub fn assign_marks(regions: &[Region]) -> Vec<Mark> {
    regions
        .iter()
        .enumerate()
        .map(|(i, r)| Mark {
            n: i as u32 + 1,
            region: r.name.clone(),
            bbox: r.bbox,
        })
        .collect()
}

/// 4-connected components of the OPAQUE pixels of `img` (alpha ≠ 0), as bounding rects in
/// the image's own pixel space, ordered top-to-bottom then left-to-right by bbox origin
/// for stable mark numbers. Mirrors `tools/lint_sprite.py`'s opacity + 4-neighbour notion
/// (the linter's orphan check is the size-1 case). Iterative flood fill (an explicit
/// stack, no recursion) so a large blob can't blow the call stack.
pub fn connected_components(img: &RgbaImage) -> Vec<MarkRect> {
    let (w, h) = (img.width(), img.height());
    if w == 0 || h == 0 {
        return Vec::new();
    }
    let mut seen = vec![false; (w as usize) * (h as usize)];
    let mut rects = Vec::new();
    let mut stack: Vec<(u32, u32)> = Vec::new();
    for y0 in 0..h {
        for x0 in 0..w {
            let start = (y0 as usize) * (w as usize) + (x0 as usize);
            if seen[start] || img.get_pixel(x0, y0).0[3] == 0 {
                continue;
            }
            let (mut minx, mut miny, mut maxx, mut maxy) = (x0, y0, x0, y0);
            seen[start] = true;
            stack.push((x0, y0));
            while let Some((x, y)) = stack.pop() {
                minx = minx.min(x);
                miny = miny.min(y);
                maxx = maxx.max(x);
                maxy = maxy.max(y);
                // Visit the 4 orthogonal neighbours that exist.
                let mut nbrs = [(0u32, 0u32); 4];
                let mut k = 0;
                if x > 0 {
                    nbrs[k] = (x - 1, y);
                    k += 1;
                }
                if x + 1 < w {
                    nbrs[k] = (x + 1, y);
                    k += 1;
                }
                if y > 0 {
                    nbrs[k] = (x, y - 1);
                    k += 1;
                }
                if y + 1 < h {
                    nbrs[k] = (x, y + 1);
                    k += 1;
                }
                for &(nx, ny) in &nbrs[..k] {
                    let ni = (ny as usize) * (w as usize) + (nx as usize);
                    if !seen[ni] && img.get_pixel(nx, ny).0[3] != 0 {
                        seen[ni] = true;
                        stack.push((nx, ny));
                    }
                }
            }
            rects.push(MarkRect {
                x: minx,
                y: miny,
                width: maxx - minx + 1,
                height: maxy - miny + 1,
            });
        }
    }
    // The scan order already discovers top-to-bottom, left-to-right; sort defensively on
    // (y, x) so the numbering is deterministic regardless of fill order.
    rects.sort_by_key(|r| (r.y, r.x));
    rects
}

/// Draw a numbered badge centred at `(cx, cy)` (preview-space): a filled neutral box
/// behind the number in `fg` at font scale `fs`, clamped to the image so a badge near an
/// edge stays on-canvas. The backing box makes the number read on any art (research §A: a
/// marker that blends into the sprite confuses the model). Returns the badge's bbox.
pub fn draw_badge(img: &mut RgbaImage, cx: u32, cy: u32, n: u32, fs: u32, fg: Rgba<u8>) -> MarkRect {
    let fs = fs.max(1);
    let pad = fs;
    let lw = gutter::label_width(n, fs);
    let lh = gutter::GLYPH_H * fs;
    let box_w = lw + 2 * pad;
    let box_h = lh + 2 * pad;
    let (iw, ih) = (img.width(), img.height());
    // Anchor so the badge centres on (cx,cy) but never leaves the image.
    let bx = cx.saturating_sub(box_w / 2).min(iw.saturating_sub(box_w));
    let by = cy.saturating_sub(box_h / 2).min(ih.saturating_sub(box_h));
    for dy in 0..box_h {
        for dx in 0..box_w {
            gutter::put(img, bx + dx, by + dy, gutter::BAND_BG);
        }
    }
    gutter::draw_label(img, bx + pad, by + pad, n, fs, fg);
    MarkRect {
        x: bx,
        y: by,
        width: box_w,
        height: box_h,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opaque_at(img: &mut RgbaImage, x: u32, y: u32) {
        img.put_pixel(x, y, Rgba([200, 50, 25, 255]));
    }

    #[test]
    fn center_is_the_bbox_midpoint() {
        assert_eq!(MarkRect { x: 0, y: 0, width: 16, height: 16 }.center(), (8, 8));
        assert_eq!(MarkRect { x: 10, y: 4, width: 1, height: 1 }.center(), (10, 4));
        assert_eq!(MarkRect { x: 4, y: 6, width: 9, height: 3 }.center(), (8, 7));
    }

    #[test]
    fn connected_components_splits_disjoint_blobs_4_connected() {
        // Two separate 2×2 blobs + a single diagonal-touch pixel that must NOT join
        // (4-connectivity), on a transparent 16×16 canvas.
        let mut img = RgbaImage::from_pixel(16, 16, Rgba([0, 0, 0, 0]));
        // Blob A: (2,2)-(3,3)
        for y in 2..4 {
            for x in 2..4 {
                opaque_at(&mut img, x, y);
            }
        }
        // Blob B: (10,11)-(11,12)
        for y in 11..13 {
            for x in 10..12 {
                opaque_at(&mut img, x, y);
            }
        }
        // A lone pixel diagonally adjacent to blob A's corner (4,4) — separate component.
        opaque_at(&mut img, 5, 5);

        let comps = connected_components(&img);
        assert_eq!(comps.len(), 3, "got {comps:?}");
        // Ordered top-to-bottom: A, then the lone (5,5), then B.
        assert_eq!(comps[0], MarkRect { x: 2, y: 2, width: 2, height: 2 });
        assert_eq!(comps[1], MarkRect { x: 5, y: 5, width: 1, height: 1 }); // the orphan
        assert_eq!(comps[2], MarkRect { x: 10, y: 11, width: 2, height: 2 });
    }

    #[test]
    fn connected_components_merges_an_l_shape() {
        // An L (orthogonally connected) is ONE component spanning its bbox.
        let mut img = RgbaImage::from_pixel(8, 8, Rgba([0, 0, 0, 0]));
        for y in 1..5 {
            opaque_at(&mut img, 1, y); // vertical stroke
        }
        for x in 1..4 {
            opaque_at(&mut img, x, 4); // horizontal foot
        }
        let comps = connected_components(&img);
        assert_eq!(comps.len(), 1);
        assert_eq!(comps[0], MarkRect { x: 1, y: 1, width: 3, height: 4 });
    }

    #[test]
    fn connected_components_empty_for_transparent() {
        let img = RgbaImage::from_pixel(8, 8, Rgba([0, 0, 0, 0]));
        assert!(connected_components(&img).is_empty());
    }

    #[test]
    fn assign_marks_numbers_in_order_and_inverts() {
        let regions = vec![
            Region { name: "head".into(), bbox: MarkRect { x: 0, y: 0, width: 8, height: 8 } },
            Region { name: "weapon".into(), bbox: MarkRect { x: 20, y: 4, width: 6, height: 14 } },
            Region { name: "shadow".into(), bbox: MarkRect { x: 2, y: 30, width: 12, height: 3 } },
        ];
        let marks = assign_marks(&regions);
        assert_eq!(marks.len(), 3);
        for (i, m) in marks.iter().enumerate() {
            assert_eq!(m.n, i as u32 + 1);
            // The mark→region inversion is exact: n maps back to that region's name+bbox.
            assert_eq!(m.region, regions[i].name);
            assert_eq!(m.bbox, regions[i].bbox);
            // Look up "region 3" the way the orchestrator would: marks[n-1].
            assert_eq!(marks[(m.n - 1) as usize].region, m.region);
        }
        assert!(assign_marks(&[]).is_empty());
    }

    #[test]
    fn draw_badge_writes_label_pixels_and_stays_in_bounds() {
        let mut img = RgbaImage::from_pixel(80, 40, Rgba([0, 0, 0, 255]));
        let fg = Rgba([255, 0, 255, 255]);
        let bbox = draw_badge(&mut img, 40, 20, 12, 3, fg);
        // The badge bbox is inside the image.
        assert!(bbox.x + bbox.width <= img.width());
        assert!(bbox.y + bbox.height <= img.height());
        // At least one fg (label) pixel was drawn inside the badge.
        let mut found = false;
        for y in bbox.y..bbox.y + bbox.height {
            for x in bbox.x..bbox.x + bbox.width {
                if *img.get_pixel(x, y) == fg {
                    found = true;
                }
            }
        }
        assert!(found, "no label pixels drawn in the badge");
    }

    #[test]
    fn draw_badge_near_corner_is_clamped_on_canvas() {
        // A badge anchored past the bottom-right corner is pulled fully on-canvas.
        let mut img = RgbaImage::from_pixel(60, 30, Rgba([0, 0, 0, 255]));
        let bbox = draw_badge(&mut img, 59, 29, 128, 4, Rgba([0, 255, 255, 255]));
        assert!(bbox.x + bbox.width <= img.width(), "badge ran off the right: {bbox:?}");
        assert!(bbox.y + bbox.height <= img.height(), "badge ran off the bottom: {bbox:?}");
    }

    #[test]
    fn draw_badge_bigger_than_image_anchors_at_origin_without_panic() {
        // The badge box exceeds the tiny image; it anchors at (0,0) and every write is
        // bounds-checked, so it overflows harmlessly (no panic / OOB). The backing box
        // still fills the visible canvas (the label digits may clamp off-canvas).
        let mut img = RgbaImage::from_pixel(8, 8, Rgba([0, 0, 0, 255]));
        let bbox = draw_badge(&mut img, 4, 4, 128, 4, Rgba([255, 0, 255, 255]));
        assert_eq!((bbox.x, bbox.y), (0, 0));
        assert!(bbox.width > img.width(), "precondition: badge wider than image");
        assert_eq!(*img.get_pixel(0, 0), gutter::BAND_BG, "badge box did not back the canvas");
    }
}
