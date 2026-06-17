//! Animation film-strip compositor (Perception fast-follow, research doc Path 1 / §A).
//!
//! The Claude API only reads the FIRST frame of an animated GIF, so an agent
//! cannot review a walk/attack cycle by looking at a GIF. IG-VLM showed that a
//! single composite image-grid of uniformly-sampled frames conveys motion to an
//! image-only model better than video. So we lay every frame into a near-square
//! row-major grid (with a gap separator so boundaries read) which the Rust server
//! then upscales — one image the agent can actually review for timing and the #1
//! animation failure, cross-frame proportion drift. This module is the pure
//! frames→grid transform so it is unit-testable.

use image::{imageops, Rgba, RgbaImage};

/// Pixels of separator between/around cells (so frame boundaries are visible —
/// VLMs read grid structure poorly without an explicit gutter).
pub const FILMSTRIP_GAP: u32 = 2;
/// Neutral gray separator colour.
pub const FILMSTRIP_GAP_COLOR: [u8; 4] = [80, 80, 80, 255];

/// Lay equal-sized `frames` into a near-square row-major grid separated by
/// [`FILMSTRIP_GAP`]. Returns `(strip, cols, rows)`.
pub fn compose_grid(frames: &[RgbaImage]) -> Result<(RgbaImage, u32, u32), String> {
    if frames.is_empty() {
        return Err("no frames to compose".to_string());
    }
    let (fw, fh) = frames[0].dimensions();
    if fw == 0 || fh == 0 {
        return Err("frames have a zero dimension".to_string());
    }
    let n = frames.len() as u32;
    let cols = (n as f64).sqrt().ceil() as u32; // near-square
    let rows = (n + cols - 1) / cols;
    let g = FILMSTRIP_GAP;
    let sw = cols * fw + (cols + 1) * g;
    let sh = rows * fh + (rows + 1) * g;
    let mut strip = RgbaImage::from_pixel(sw, sh, Rgba(FILMSTRIP_GAP_COLOR));
    for (i, f) in frames.iter().enumerate() {
        if f.dimensions() != (fw, fh) {
            return Err(format!(
                "frame {} is {:?}, expected {:?} (all frames must match)",
                i,
                f.dimensions(),
                (fw, fh)
            ));
        }
        let cx = (i as u32) % cols;
        let cy = (i as u32) / cols;
        let ox = (g + cx * (fw + g)) as i64;
        let oy = (g + cy * (fh + g)) as i64;
        imageops::overlay(&mut strip, f, ox, oy);
    }
    Ok((strip, cols, rows))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(w: u32, h: u32, c: [u8; 4]) -> RgbaImage {
        RgbaImage::from_pixel(w, h, Rgba(c))
    }

    #[test]
    fn grid_is_near_square_with_gaps() {
        let frames: Vec<RgbaImage> = (0..4).map(|_| frame(2, 3, [10, 20, 30, 255])).collect();
        let (strip, cols, rows) = compose_grid(&frames).unwrap();
        assert_eq!((cols, rows), (2, 2));
        // sw = cols*fw + (cols+1)*gap = 2*2 + 3*2 = 10 ; sh = 2*3 + 3*2 = 12
        assert_eq!(strip.dimensions(), (10, 12));
        // gap pixel at (0,0) is the separator colour; first frame's top-left sits at (gap,gap).
        assert_eq!(*strip.get_pixel(0, 0), Rgba(FILMSTRIP_GAP_COLOR));
        assert_eq!(*strip.get_pixel(FILMSTRIP_GAP, FILMSTRIP_GAP), Rgba([10, 20, 30, 255]));
    }

    #[test]
    fn single_frame_is_a_one_cell_grid() {
        let (strip, cols, rows) = compose_grid(&[frame(4, 4, [1, 2, 3, 255])]).unwrap();
        assert_eq!((cols, rows), (1, 1));
        assert_eq!(strip.dimensions(), (4 + 2 * FILMSTRIP_GAP, 4 + 2 * FILMSTRIP_GAP));
    }

    #[test]
    fn six_frames_make_a_3x2_grid() {
        let frames: Vec<RgbaImage> = (0..6).map(|_| frame(24, 24, [0, 0, 0, 255])).collect();
        let (_s, cols, rows) = compose_grid(&frames).unwrap();
        assert_eq!((cols, rows), (3, 2)); // ceil(sqrt(6))=3, rows=2
    }

    #[test]
    fn mismatched_frame_sizes_are_rejected() {
        let frames = vec![frame(4, 4, [0, 0, 0, 255]), frame(5, 4, [0, 0, 0, 255])];
        assert!(compose_grid(&frames).unwrap_err().contains("must match"));
    }

    #[test]
    fn empty_is_an_error() {
        assert!(compose_grid(&[]).is_err());
    }
}
