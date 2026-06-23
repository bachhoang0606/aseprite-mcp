//! Blob-47 autotile bitmask (SPEC-003 Phase 3 core). Pure, deterministic, no deps.
//!
//! An autotile picks a tile from which of its 8 neighbours share the same terrain.
//! A corner only matters when BOTH its adjacent cardinal edges are filled (else it
//! is visually cut off), so the corner is masked out otherwise. That rule collapses
//! the 256 raw neighbour configurations to the **47** canonical "blob" states.
//!
//! Convention (normalized here so generators don't silently disagree — SPEC-003):
//! edges in the low nibble, corners in the high nibble.
//! ```text
//!   NW   N   NE
//!    W  [.]  E
//!   SW   S   SE
//! ```
// SPEC-003 Phase 3 foundation: the tile generator/exporter that consumes these
// lands in later phases (which need the live Lua bridge), so the public surface is
// exercised by the unit tests below until then.
#![allow(dead_code)]

// Edge bits.
pub const N: u8 = 1;
pub const E: u8 = 2;
pub const S: u8 = 4;
pub const W: u8 = 8;
// Corner bits.
pub const NE: u8 = 16;
pub const SE: u8 = 32;
pub const SW: u8 = 64;
pub const NW: u8 = 128;

/// Build a raw 8-neighbour mask from booleans (true = same terrain present).
#[allow(clippy::too_many_arguments)]
pub fn raw_mask(n: bool, e: bool, s: bool, w: bool, ne: bool, se: bool, sw: bool, nw: bool) -> u8 {
    (n as u8 * N)
        | (e as u8 * E)
        | (s as u8 * S)
        | (w as u8 * W)
        | (ne as u8 * NE)
        | (se as u8 * SE)
        | (sw as u8 * SW)
        | (nw as u8 * NW)
}

/// Clear any corner whose two adjacent edges are not both present — the rule that
/// collapses 256 raw configs to the 47 canonical blob states. Idempotent.
pub fn canonical_mask(raw: u8) -> u8 {
    let n = raw & N != 0;
    let e = raw & E != 0;
    let s = raw & S != 0;
    let w = raw & W != 0;
    let mut m = raw;
    if !(n && e) {
        m &= !NE;
    }
    if !(s && e) {
        m &= !SE;
    }
    if !(s && w) {
        m &= !SW;
    }
    if !(n && w) {
        m &= !NW;
    }
    m
}

/// The 47 canonical masks in ascending order. This ordering IS the tile template
/// order (tile index 0..=46), so a generator and an exporter agree by construction.
pub fn blob47_masks() -> Vec<u8> {
    let mut set: Vec<u8> = (0u16..=255).map(|r| canonical_mask(r as u8)).collect();
    set.sort_unstable();
    set.dedup();
    set
}

/// Map any raw 8-neighbour config to its blob-47 tile index (0..=46).
pub fn blob47_tile_index(raw: u8) -> usize {
    let m = canonical_mask(raw);
    blob47_masks()
        .binary_search(&m)
        .expect("canonical mask is always one of the 47 states")
}

// ---- template compositor (SPEC-003 Phase 3): assemble 47 tiles from 4 corner quarters ----
use image::{Rgba, RgbaImage};

/// The four source corner-quarters the agent draws (each `q×q`, where `q = tile_size/2`), in a
/// canonical reference orientation:
/// - `fill`  — solid interior;
/// - `outer` — a CONVEX corner with the rounded/cut corner at the quarter's TOP-LEFT;
/// - `edge`  — a straight boundary along the quarter's TOP;
/// - `inner` — a CONCAVE corner (notch) at the quarter's TOP-LEFT.
///
/// The compositor rotates these to build all four quadrants of every tile, so the agent draws ~4
/// quarters instead of 47 tiles ("draw 5 → get 47", the 4-corners-per-tile model).
pub struct CornerPieces {
    pub fill: RgbaImage,
    pub outer: RgbaImage,
    pub edge: RgbaImage,
    pub inner: RgbaImage,
    /// Quarter edge length (= tile_size / 2).
    pub q: u32,
}

/// Which source quarter a tile-quadrant uses.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Piece {
    Fill,
    Outer,
    Edge,
    Inner,
}

/// A tile's four quadrants (each `q×q`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Quadrant {
    Nw,
    Ne,
    Se,
    Sw,
}

/// Rotate a square `q×q` image by `r` quarter-turns clockwise (lossless — copies pixels, invents
/// no colour, so the assembled palette ⊆ the source pieces' palette).
pub fn rotate90(img: &RgbaImage, r: u8) -> RgbaImage {
    let r = r % 4;
    if r == 0 {
        return img.clone();
    }
    let q = img.width();
    debug_assert_eq!(q, img.height(), "rotate90 expects a square quarter");
    let mut out = RgbaImage::new(q, q);
    for y in 0..q {
        for x in 0..q {
            // One 90° CW step maps output (x,y) <- input (y, q-1-x); compose r times.
            let (mut sx, mut sy) = (x, y);
            for _ in 0..r {
                let (nx, ny) = (sy, q - 1 - sx);
                sx = nx;
                sy = ny;
            }
            out.put_pixel(x, y, *img.get_pixel(sx, sy));
        }
    }
    out
}

/// For a tile `mask` and a `quadrant`, the source `Piece` and the clockwise quarter-turns to apply.
/// Pure lookup — the heart of the deterministic 4-corners model. Each quadrant depends only on its
/// two adjacent cardinal edges (`a` = vertical neighbour, `b` = horizontal neighbour) and the
/// diagonal corner `d` (which the canonical rule already cleared unless both edges are present).
pub fn quadrant_piece(quadrant: Quadrant, mask: u8) -> (Piece, u8) {
    let bit = |b: u8| mask & b != 0;
    // (a, b, d, outer_rot, edge_a_rot, edge_b_rot, inner_rot) per quadrant.
    let (a, b, d, outer_rot, edge_a_rot, edge_b_rot, inner_rot) = match quadrant {
        Quadrant::Nw => (bit(N), bit(W), bit(NW), 0u8, 3u8, 0u8, 0u8),
        Quadrant::Ne => (bit(N), bit(E), bit(NE), 1, 1, 0, 1),
        Quadrant::Se => (bit(S), bit(E), bit(SE), 2, 1, 2, 2),
        Quadrant::Sw => (bit(S), bit(W), bit(SW), 3, 3, 2, 3),
    };
    match (a, b, d) {
        (false, false, _) => (Piece::Outer, outer_rot),
        (true, false, _) => (Piece::Edge, edge_a_rot),
        (false, true, _) => (Piece::Edge, edge_b_rot),
        (true, true, false) => (Piece::Inner, inner_rot),
        (true, true, true) => (Piece::Fill, 0),
    }
}

fn blit(dst: &mut RgbaImage, src: &RgbaImage, ox: u32, oy: u32) {
    for y in 0..src.height() {
        for x in 0..src.width() {
            dst.put_pixel(ox + x, oy + y, *src.get_pixel(x, y));
        }
    }
}

/// Assemble one `2q×2q` tile for `mask` from the four corner quarters.
pub fn assemble_tile(mask: u8, pieces: &CornerPieces) -> RgbaImage {
    let q = pieces.q;
    let mut tile = RgbaImage::from_pixel(2 * q, 2 * q, Rgba([0, 0, 0, 0]));
    for (quadrant, (ox, oy)) in [
        (Quadrant::Nw, (0, 0)),
        (Quadrant::Ne, (q, 0)),
        (Quadrant::Sw, (0, q)),
        (Quadrant::Se, (q, q)),
    ] {
        let (piece, rot) = quadrant_piece(quadrant, mask);
        let src = match piece {
            Piece::Fill => &pieces.fill,
            Piece::Outer => &pieces.outer,
            Piece::Edge => &pieces.edge,
            Piece::Inner => &pieces.inner,
        };
        blit(&mut tile, &rotate90(src, rot), ox, oy);
    }
    tile
}

/// Assemble all 47 blob tiles, in canonical `blob47_masks()` order (tile index 0..=46) — so the
/// generated sheet and the bitmask→index mapping (`blob47_tile_index`) agree by construction.
pub fn assemble_blob47(pieces: &CornerPieces) -> Vec<RgbaImage> {
    blob47_masks().iter().map(|&m| assemble_tile(m, pieces)).collect()
}

/// All four corner bits set — used to compose the **edge-only** wang-16 set: with every diagonal
/// "present", a quadrant whose two cardinal edges are both filled is always `fill` (never the
/// concave `inner` corner), which is exactly the wang-16 rule (corners don't matter).
const ALL_CORNERS: u8 = NE | SE | SW | NW;

/// Assemble the **wang-16** edge-only set: 16 tiles indexed directly by the 4-bit cardinal edge
/// mask `N|E|S|W` (0..=15). Reuses the blob-47 quadrant compositor with the corner bits forced on,
/// so it needs only the `fill` / `outer` / `edge` quarters (the `inner` quarter is unused).
pub fn assemble_wang16(pieces: &CornerPieces) -> Vec<RgbaImage> {
    (0u8..=15).map(|m| assemble_tile(m | ALL_CORNERS, pieces)).collect()
}

/// Cut the four `q×q` source quarters from a left-to-right strip `[fill | outer | edge | inner]`
/// at `(sx, sy)` in a rendered image. Returns `None` if the strip runs off the image.
pub fn slice_corner_pieces(img: &RgbaImage, sx: u32, sy: u32, q: u32) -> Option<CornerPieces> {
    if q == 0 || sx + 4 * q > img.width() || sy + q > img.height() {
        return None;
    }
    let cut = |i: u32| image::imageops::crop_imm(img, sx + i * q, sy, q, q).to_image();
    Some(CornerPieces { fill: cut(0), outer: cut(1), edge: cut(2), inner: cut(3), q })
}

/// Near-square row-major grid `(cols, rows)` that holds `n` tiles (for laying out the 47-tile sheet).
pub fn sheet_dims(n: usize) -> (u32, u32) {
    let cols = ((n as f64).sqrt().ceil() as u32).max(1);
    let rows = ((n as u32).div_ceil(cols)).max(1);
    (cols, rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn there_are_exactly_47_canonical_states() {
        assert_eq!(blob47_masks().len(), 47);
    }

    #[test]
    fn a_corner_needs_both_adjacent_edges() {
        // A lone NE corner with neither N nor E is cleared.
        assert_eq!(canonical_mask(NE), 0);
        // NE with only one adjacent edge is cleared.
        assert_eq!(canonical_mask(N | NE), N);
        assert_eq!(canonical_mask(E | NE), E);
        // NE with both adjacent edges survives.
        assert_eq!(canonical_mask(N | E | NE), N | E | NE);
    }

    #[test]
    fn canonical_is_idempotent() {
        for r in 0u16..=255 {
            let m = canonical_mask(r as u8);
            assert_eq!(canonical_mask(m), m, "raw={r}");
        }
    }

    #[test]
    fn tile_index_is_a_bijection_over_the_47_masks() {
        let masks = blob47_masks();
        for (i, &m) in masks.iter().enumerate() {
            assert_eq!(blob47_tile_index(m), i);
        }
        // Every one of the 256 raw configs lands inside 0..47.
        for r in 0u16..=255 {
            assert!(blob47_tile_index(r as u8) < 47);
        }
    }

    #[test]
    fn empty_and_full_are_the_endpoints() {
        assert_eq!(canonical_mask(0), 0); // isolated tile
        assert_eq!(canonical_mask(0xFF), 0xFF); // fully surrounded: all corners valid
        assert_eq!(*blob47_masks().first().unwrap(), 0);
        assert_eq!(*blob47_masks().last().unwrap(), 0xFF);
    }

    #[test]
    fn raw_mask_round_trips_through_bits() {
        let m = raw_mask(true, false, true, false, false, false, false, false);
        assert_eq!(m, N | S);
        assert_eq!(canonical_mask(m), N | S); // opposite edges, no valid corners
    }

    // ---- template compositor tests ----
    fn solid(q: u32, c: [u8; 4]) -> RgbaImage {
        RgbaImage::from_pixel(q, q, Rgba(c))
    }

    fn test_pieces() -> CornerPieces {
        let q = 2;
        let mut outer = solid(q, [10, 10, 10, 255]);
        outer.put_pixel(0, 0, Rgba([200, 0, 0, 255])); // a TOP-LEFT marker to track rotation
        CornerPieces {
            fill: solid(q, [0, 120, 0, 255]),
            outer,
            edge: solid(q, [0, 0, 200, 255]),
            inner: solid(q, [160, 160, 0, 255]),
            q,
        }
    }

    #[test]
    fn rotate90_moves_a_top_left_marker_clockwise() {
        let mut img = solid(2, [0, 0, 0, 255]);
        img.put_pixel(0, 0, Rgba([255, 0, 0, 255]));
        let mark = |im: &RgbaImage| {
            (0..2).flat_map(|y| (0..2).map(move |x| (x, y)))
                .find(|&(x, y)| im.get_pixel(x, y).0 == [255, 0, 0, 255]).unwrap()
        };
        assert_eq!(mark(&rotate90(&img, 0)), (0, 0));
        assert_eq!(mark(&rotate90(&img, 1)), (1, 0)); // top-left -> top-right (CW)
        assert_eq!(mark(&rotate90(&img, 2)), (1, 1)); // -> bottom-right
        assert_eq!(mark(&rotate90(&img, 3)), (0, 1)); // -> bottom-left
        assert_eq!(mark(&rotate90(&img, 4)), (0, 0)); // full turn
    }

    #[test]
    fn quadrant_piece_lookup_matches_the_model() {
        // Fully surrounded -> every quadrant is fill.
        for qd in [Quadrant::Nw, Quadrant::Ne, Quadrant::Se, Quadrant::Sw] {
            assert_eq!(quadrant_piece(qd, 0xFF).0, Piece::Fill);
            assert_eq!(quadrant_piece(qd, 0).0, Piece::Outer); // isolated -> outer corners
        }
        // Vertical strip (N|S): all four quadrants are single edges.
        for qd in [Quadrant::Nw, Quadrant::Ne, Quadrant::Se, Quadrant::Sw] {
            assert_eq!(quadrant_piece(qd, N | S).0, Piece::Edge);
        }
        // Connect N+E but the NE diagonal is EMPTY -> the NE quadrant is a concave inner corner.
        assert_eq!(quadrant_piece(Quadrant::Ne, N | E).0, Piece::Inner);
        // With the NE diagonal present -> that quadrant fills in.
        assert_eq!(quadrant_piece(Quadrant::Ne, N | E | NE).0, Piece::Fill);
    }

    #[test]
    fn assemble_isolated_tile_puts_outer_marks_at_the_four_tile_corners() {
        let p = test_pieces();
        let tile = assemble_tile(0, &p); // mask 0 -> four rotated outer corners
        assert_eq!(tile.dimensions(), (4, 4));
        let red = [200, 0, 0, 255];
        // The top-left marker of `outer` rotates into each of the tile's four corners.
        for &(x, y) in &[(0, 0), (3, 0), (3, 3), (0, 3)] {
            assert_eq!(tile.get_pixel(x, y).0, red, "missing outer mark at {x},{y}");
        }
    }

    #[test]
    fn assemble_full_tile_is_solid_fill() {
        let p = test_pieces();
        let tile = assemble_tile(0xFF, &p);
        for px in tile.pixels() {
            assert_eq!(px.0, [0, 120, 0, 255]); // fill colour everywhere
        }
    }

    #[test]
    fn wang16_is_16_edge_only_tiles_that_never_use_inner() {
        let p = test_pieces(); // inner = solid [160,160,0,255]
        let tiles = assemble_wang16(&p);
        assert_eq!(tiles.len(), 16);
        for (i, tile) in tiles.iter().enumerate() {
            assert_eq!(tile.dimensions(), (4, 4), "tile {i}");
            // wang-16 ignores corners → the concave `inner` quarter is never used.
            for px in tile.pixels() {
                assert_ne!(px.0, [160, 160, 0, 255], "wang16 tile {i} used the inner piece");
            }
        }
        // The wang-16 rule: both adjacent edges present -> fill (NOT the concave inner corner).
        let corners = NE | SE | SW | NW;
        assert_eq!(quadrant_piece(Quadrant::Ne, (N | E) | corners).0, Piece::Fill);
        // All four edges -> solid fill; index 0 (no edges) -> four outer corners.
        for px in assemble_tile((N | E | S | W) | corners, &p).pixels() {
            assert_eq!(px.0, [0, 120, 0, 255]);
        }
        assert_eq!(quadrant_piece(Quadrant::Nw, corners).0, Piece::Outer); // no edges -> outer
    }

    #[test]
    fn slice_corner_pieces_cuts_the_strip() {
        // A 8x2 strip = four 2x2 quarters, each a distinct solid colour.
        let cols = [[1, 0, 0, 255], [2, 0, 0, 255], [3, 0, 0, 255], [4, 0, 0, 255]];
        let mut strip = RgbaImage::new(8, 2);
        for (i, c) in cols.iter().enumerate() {
            for dx in 0..2 {
                for dy in 0..2 {
                    strip.put_pixel(i as u32 * 2 + dx, dy, Rgba(*c));
                }
            }
        }
        let p = slice_corner_pieces(&strip, 0, 0, 2).unwrap();
        assert_eq!(p.fill.get_pixel(0, 0).0, cols[0]);
        assert_eq!(p.outer.get_pixel(0, 0).0, cols[1]);
        assert_eq!(p.edge.get_pixel(0, 0).0, cols[2]);
        assert_eq!(p.inner.get_pixel(0, 0).0, cols[3]);
        // Off-image strip -> None.
        assert!(slice_corner_pieces(&strip, 2, 0, 2).is_none());
    }

    #[test]
    fn sheet_dims_holds_all_tiles_near_square() {
        let (c, r) = sheet_dims(47);
        assert_eq!((c, r), (7, 7));
        assert!(c * r >= 47);
        assert_eq!(sheet_dims(1), (1, 1));
        let (c16, r16) = sheet_dims(16);
        assert!(c16 * r16 >= 16);
    }

    #[test]
    fn assemble_blob47_count_and_invents_no_colour() {
        let p = test_pieces();
        let tiles = assemble_blob47(&p);
        assert_eq!(tiles.len(), 47);
        // Palette ⊆ the source pieces' colours (rotation + blit copy pixels, never blend).
        let mut allowed = std::collections::HashSet::new();
        for piece in [&p.fill, &p.outer, &p.edge, &p.inner] {
            for px in piece.pixels() {
                allowed.insert(px.0);
            }
        }
        for (i, tile) in tiles.iter().enumerate() {
            assert_eq!(tile.dimensions(), (4, 4), "tile {i} wrong size");
            for px in tile.pixels() {
                assert!(allowed.contains(&px.0), "tile {i} invented colour {:?}", px.0);
            }
        }
    }
}
