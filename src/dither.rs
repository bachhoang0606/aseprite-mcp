//! Ordered (Bayer) dithering between two palette colours (SPEC-009 Phase 1, roadmap #8).
//!
//! Pixel-art shading often wants a *dithered* blend of two adjacent ramp steps — exactly
//! the tedious, deterministic work an LLM does worst freehand (§D). An ordered dither is
//! textbook: tile a normalized threshold matrix over the region and pick `color_b` where the
//! threshold is below the blend `level`, else `color_a`. The result is **palette-legal by
//! construction** — only the two inputs ever appear. Pure Rust, no Aseprite, no new dependency.

use crate::color_ops::Rgba;

/// 4×4 Bayer matrix (values 0..15; normalized by /16).
const BAYER4: [[u8; 4]; 4] = [
    [0, 8, 2, 10],
    [12, 4, 14, 6],
    [3, 11, 1, 9],
    [15, 7, 13, 5],
];
/// 2×2 Bayer matrix (values 0..3; normalized by /4).
const BAYER2: [[u8; 2]; 2] = [[0, 2], [3, 1]];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Matrix {
    Bayer4,
    Bayer2,
    Checker,
}

impl Matrix {
    pub fn parse(s: &str) -> Result<Matrix, String> {
        match s {
            "bayer4" => Ok(Matrix::Bayer4),
            "bayer2" => Ok(Matrix::Bayer2),
            "checker" => Ok(Matrix::Checker),
            other => Err(format!("'{other}' — use bayer4, bayer2, or checker")),
        }
    }
}

/// The ordered-dither threshold in `[0, 1)` for cell (x, y).
pub fn threshold(matrix: Matrix, x: u32, y: u32) -> f64 {
    match matrix {
        Matrix::Bayer4 => BAYER4[(y % 4) as usize][(x % 4) as usize] as f64 / 16.0,
        Matrix::Bayer2 => BAYER2[(y % 2) as usize][(x % 2) as usize] as f64 / 4.0,
        Matrix::Checker => {
            if (x + y) % 2 == 0 {
                0.0
            } else {
                0.5
            }
        }
    }
}

/// Dither a `w`×`h` region anchored at `(ox, oy)`. A cell takes `b` when its ordered
/// threshold is below `level`, else `a`. `level` is clamped to `[0, 1]`: 0 → all `a`,
/// 1 → all `b`, 0.5 → an even Bayer/checker blend. Returns one `(x, y, colour)` per cell.
#[allow(clippy::too_many_arguments)] // region + 2 colours + level + matrix read clearest flat
pub fn dither_region(
    ox: i32,
    oy: i32,
    w: u32,
    h: u32,
    a: Rgba,
    b: Rgba,
    level: f64,
    matrix: Matrix,
) -> Vec<(i32, i32, Rgba)> {
    let level = level.clamp(0.0, 1.0);
    let mut out = Vec::with_capacity((w * h) as usize);
    for y in 0..h {
        for x in 0..w {
            let c = if threshold(matrix, x, y) < level { b } else { a };
            out.push((ox + x as i32, oy + y as i32, c));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a() -> Rgba {
        Rgba::rgb(10, 20, 30)
    }
    fn b() -> Rgba {
        Rgba::rgb(200, 210, 220)
    }

    #[test]
    fn endpoints_are_pure() {
        let lo = dither_region(0, 0, 4, 4, a(), b(), 0.0, Matrix::Bayer4);
        assert!(lo.iter().all(|&(_, _, c)| c == a()), "level 0 = all a");
        let hi = dither_region(0, 0, 4, 4, a(), b(), 1.0, Matrix::Bayer4);
        assert!(hi.iter().all(|&(_, _, c)| c == b()), "level 1 = all b");
    }

    #[test]
    fn bayer4_half_is_even() {
        // Bayer-4 has thresholds 0/16..15/16; level 0.5 -> values < 8 take b -> exactly half.
        let px = dither_region(0, 0, 4, 4, a(), b(), 0.5, Matrix::Bayer4);
        let bs = px.iter().filter(|&&(_, _, c)| c == b()).count();
        assert_eq!(bs, 8, "half of the 16 cells are b");
        assert_eq!(px.len(), 16, "every cell once");
    }

    #[test]
    fn checker_half_is_alternating() {
        let px = dither_region(0, 0, 4, 4, a(), b(), 0.5, Matrix::Checker);
        // (x+y) even -> threshold 0.0 < 0.5 -> b; odd -> 0.5 < 0.5 false -> a.
        for &(x, y, c) in &px {
            let want = if (x + y) % 2 == 0 { b() } else { a() };
            assert_eq!(c, want, "checker at ({x},{y})");
        }
    }

    #[test]
    fn region_is_offset_and_complete() {
        let px = dither_region(5, 7, 3, 2, a(), b(), 0.5, Matrix::Bayer2);
        assert_eq!(px.len(), 6);
        let coords: std::collections::HashSet<_> = px.iter().map(|&(x, y, _)| (x, y)).collect();
        assert_eq!(coords.len(), 6, "no duplicate cells");
        assert!(coords.contains(&(5, 7)) && coords.contains(&(7, 8)), "offset by (5,7)");
    }
}
