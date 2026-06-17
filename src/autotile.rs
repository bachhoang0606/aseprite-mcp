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
}
