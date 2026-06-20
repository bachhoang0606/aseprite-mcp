//! Preview rendering for the perception loop (research doc Path 1).
//!
//! A raw 1x PNG of a 16–64px sprite is below the resolution a vision model can
//! read: Anthropic's vision pipeline tokenizes images in 28x28-px patches and
//! flags images under ~200px as hallucination-prone, and grounding studies put
//! the sweet spot near ~1024px. So before an agent looks at its own work we
//! nearest-neighbor upscale the sprite (nearest, not bilinear — pixel art must
//! keep hard edges and introduce no new colours) so its long edge lands near
//! that budget. All of the image math lives here so it is unit-testable without
//! the live Aseprite bridge.

use std::path::Path;

/// Long-edge resolution we aim the upscaled preview at (~1024px is where vision
/// models ground pixel art most reliably).
pub const PREVIEW_TARGET_EDGE: u32 = 1024;
/// Never upscale beyond this factor: a 16px sprite at 16x is already 256px
/// (clear of the ~200px hallucination floor) and a bigger factor mostly wastes
/// tokens for tiny sprites.
pub const PREVIEW_MAX_SCALE: u32 = 16;
/// Ceiling the *upscale* respects: the chosen scale is clamped so neither output
/// axis exceeds this, bounding memory from a large explicit `scale`. (It does not
/// downscale a source that is already larger — the tool never shrinks pixel art.)
pub const PREVIEW_MAX_EDGE: u32 = 8192;

/// What `render_preview` did, surfaced to the caller (and the agent) so it knows
/// the mapping between preview pixels and real sprite coordinates. `source_*` are the
/// dimensions of the **previewed region** (the crop, or the whole sprite when
/// uncropped); `crop_x`/`crop_y` are that region's origin in full-sprite coordinates
/// (0,0 uncropped) so a preview pixel inverts to a real sprite (x,y) exactly:
/// `source_x = crop_x + (preview_x − gutter_left_w) / scale`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PreviewInfo {
    pub source_width: u32,
    pub source_height: u32,
    pub scale: u32,
    pub preview_width: u32,
    pub preview_height: u32,
    pub crop_x: u32,
    pub crop_y: u32,
}

/// A source-space crop rectangle (Phase 2 region crop). `width`/`height` are clamped
/// to the image by [`clamp_crop`]; a fully out-of-bounds rect is an error there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Crop {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// Clamp `crop` to the `[0,img_w) × [0,img_h)` canvas, returning the in-bounds
/// `(x, y, w, h)` — or an error if the origin lies outside the image or nothing
/// remains after clamping. Pure so the crop math is unit-tested without a PNG.
pub fn clamp_crop(img_w: u32, img_h: u32, crop: Crop) -> Result<(u32, u32, u32, u32), String> {
    if crop.x >= img_w || crop.y >= img_h {
        return Err(format!(
            "crop origin ({},{}) is outside the {img_w}x{img_h} preview",
            crop.x, crop.y
        ));
    }
    let w = crop.width.min(img_w - crop.x);
    let h = crop.height.min(img_h - crop.y);
    if w == 0 || h == 0 {
        return Err(format!(
            "crop {}x{} at ({},{}) has a zero dimension after clamping to {img_w}x{img_h}",
            crop.width, crop.height, crop.x, crop.y
        ));
    }
    Ok((crop.x, crop.y, w, h))
}

/// Pick an integer upscale factor so the sprite's long edge lands near
/// [`PREVIEW_TARGET_EDGE`]. Clamped to `[1, PREVIEW_MAX_SCALE]` — never
/// downscales, never magnifies a tiny sprite into a huge image.
pub fn auto_preview_scale(width: u32, height: u32) -> u32 {
    let long_edge = width.max(height).max(1);
    let factor = (PREVIEW_TARGET_EDGE as f64 / long_edge as f64).round() as i64;
    factor.clamp(1, PREVIEW_MAX_SCALE as i64) as u32
}

/// Clamp a (requested or auto) scale so neither output axis exceeds
/// [`PREVIEW_MAX_EDGE`]. Guards against OOM from a huge explicit `scale`.
pub fn clamp_scale_to_max_edge(width: u32, height: u32, scale: u32) -> u32 {
    let long_edge = width.max(height).max(1);
    let max_by_edge = (PREVIEW_MAX_EDGE / long_edge).max(1);
    scale.clamp(1, max_by_edge)
}

/// Decode the PNG at `src` and nearest-neighbor upscale it by `requested_scale`
/// (or [`auto_preview_scale`] when `None`) into an **in-memory** RGBA buffer (no
/// file write), returning the buffer + the chosen [`PreviewInfo`]. Callers that
/// want to composite annotations (a coordinate gutter, Set-of-Mark badges) onto
/// the upscaled art *before* writing use this; [`render_preview`] is this plus a
/// PNG write. Pure (file-in / buffer-out) so it stays unit-testable without Aseprite.
pub fn render_preview_buffer(
    src: &Path,
    requested_scale: Option<u32>,
    crop: Option<Crop>,
) -> Result<(image::RgbaImage, PreviewInfo), String> {
    let img = image::open(src)
        .map_err(|e| format!("failed to decode preview source {}: {e}", src.display()))?;
    let (full_w, full_h) = (img.width(), img.height());
    if full_w == 0 || full_h == 0 {
        return Err(format!("preview source has a zero dimension ({full_w}x{full_h})"));
    }
    let rgba = img.to_rgba8();

    // Crop to the subject first (Phase 2) so the upscale budget lands on the crop's
    // long edge — a 16×16 cel on a 256×256 canvas fills ~1024px, not ~64px. A full-
    // canvas crop short-circuits to the uncropped buffer so `crop="sprite"` is byte-
    // for-byte today's output (no regression).
    let (crop_x, crop_y, w, h) = match crop {
        Some(c) => clamp_crop(full_w, full_h, c)?,
        None => (0, 0, full_w, full_h),
    };
    let base = if crop_x == 0 && crop_y == 0 && w == full_w && h == full_h {
        rgba
    } else {
        image::imageops::crop_imm(&rgba, crop_x, crop_y, w, h).to_image()
    };

    let scale = requested_scale
        .map(|s| s.max(1))
        .unwrap_or_else(|| auto_preview_scale(w, h));
    let scale = clamp_scale_to_max_edge(w, h, scale);

    let out = if scale == 1 {
        base
    } else {
        image::imageops::resize(&base, w * scale, h * scale, image::imageops::FilterType::Nearest)
    };

    Ok((
        out,
        PreviewInfo {
            source_width: w,
            source_height: h,
            scale,
            preview_width: w * scale,
            preview_height: h * scale,
            crop_x,
            crop_y,
        },
    ))
}

/// Decode the PNG at `src`, nearest-neighbor upscale it by `requested_scale`
/// (or [`auto_preview_scale`] when `None`), and write the result as a PNG to
/// `dst`. Pure file-in / file-out so it can be exercised without Aseprite.
pub fn render_preview(
    src: &Path,
    dst: &Path,
    requested_scale: Option<u32>,
) -> Result<PreviewInfo, String> {
    let (out, info) = render_preview_buffer(src, requested_scale, None)?;
    out.save_with_format(dst, image::ImageFormat::Png)
        .map_err(|e| format!("failed to write preview {}: {e}", dst.display()))?;
    Ok(info)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgba, RgbaImage};

    #[test]
    fn auto_scale_targets_about_1024_and_caps_at_16() {
        assert_eq!(auto_preview_scale(16, 16), 16); // round(64) -> cap 16
        assert_eq!(auto_preview_scale(32, 32), 16); // round(32) -> cap 16
        assert_eq!(auto_preview_scale(64, 64), 16); // exactly 16
        assert_eq!(auto_preview_scale(128, 128), 8);
        assert_eq!(auto_preview_scale(256, 256), 4);
        assert_eq!(auto_preview_scale(512, 512), 2);
        assert_eq!(auto_preview_scale(1024, 1024), 1);
        assert_eq!(auto_preview_scale(4096, 2048), 1); // never below 1
    }

    #[test]
    fn auto_scale_uses_the_long_edge_for_non_square_sprites() {
        assert_eq!(auto_preview_scale(64, 8), 16);
        assert_eq!(auto_preview_scale(8, 64), 16);
    }

    #[test]
    fn clamp_keeps_the_result_under_max_edge() {
        // 1000px sprite, explicit 100x would be 100_000px -> floor(8192/1000)=8.
        assert_eq!(clamp_scale_to_max_edge(1000, 1000, 100), 8);
        // Small sprites are unaffected.
        assert_eq!(clamp_scale_to_max_edge(16, 16, 4), 4);
        // Never below 1.
        assert_eq!(clamp_scale_to_max_edge(9000, 9000, 5), 1);
    }

    #[test]
    fn nearest_upscale_replicates_pixels_into_solid_blocks() {
        let mut img = RgbaImage::new(2, 1);
        img.put_pixel(0, 0, Rgba([10, 20, 30, 255]));
        img.put_pixel(1, 0, Rgba([200, 100, 50, 255]));
        let out = image::imageops::resize(&img, 6, 3, image::imageops::FilterType::Nearest);
        assert_eq!(out.dimensions(), (6, 3));
        // Left 3x3 block keeps the first colour; right 3x3 keeps the second.
        assert_eq!(*out.get_pixel(0, 0), Rgba([10, 20, 30, 255]));
        assert_eq!(*out.get_pixel(2, 2), Rgba([10, 20, 30, 255]));
        assert_eq!(*out.get_pixel(3, 0), Rgba([200, 100, 50, 255]));
        assert_eq!(*out.get_pixel(5, 2), Rgba([200, 100, 50, 255]));
    }

    #[test]
    fn render_preview_round_trips_through_files() {
        let dir = std::env::temp_dir();
        let src = dir.join("aseprite_mcp_preview_src_test.png");
        let dst = dir.join("aseprite_mcp_preview_dst_test.png");
        let mut img = RgbaImage::new(4, 3);
        img.put_pixel(0, 0, Rgba([1, 2, 3, 255]));
        img.save_with_format(&src, image::ImageFormat::Png).unwrap();

        let info = render_preview(&src, &dst, Some(4)).unwrap();
        assert_eq!((info.source_width, info.source_height), (4, 3));
        assert_eq!(info.scale, 4);
        assert_eq!((info.preview_width, info.preview_height), (16, 12));

        let back = image::open(&dst).unwrap();
        assert_eq!((back.width(), back.height()), (16, 12));

        let _ = std::fs::remove_file(&src);
        let _ = std::fs::remove_file(&dst);
    }

    #[test]
    fn clamp_crop_clamps_to_canvas_and_rejects_out_of_bounds() {
        // Fully inside -> unchanged.
        assert_eq!(clamp_crop(64, 64, Crop { x: 8, y: 8, width: 16, height: 16 }).unwrap(), (8, 8, 16, 16));
        // Over-wide/tall -> clamped to the remaining canvas.
        assert_eq!(clamp_crop(64, 64, Crop { x: 60, y: 50, width: 100, height: 100 }).unwrap(), (60, 50, 4, 14));
        // Origin outside the image -> error.
        assert!(clamp_crop(64, 64, Crop { x: 64, y: 0, width: 4, height: 4 }).is_err());
        assert!(clamp_crop(64, 64, Crop { x: 0, y: 99, width: 4, height: 4 }).is_err());
        // Zero requested size -> nothing remains -> error.
        assert!(clamp_crop(64, 64, Crop { x: 0, y: 0, width: 0, height: 8 }).is_err());
    }

    #[test]
    fn render_preview_buffer_crops_then_scales_the_crop() {
        // A 64×64 canvas, distinct marker inside a 16×16 sub-region. Cropping to that
        // region then auto-scaling must fill ~1024px off the CROP's long edge (16→16x
        // = 256px), and report the crop origin for exact inversion.
        let dir = std::env::temp_dir();
        let src = dir.join("aseprite_mcp_preview_crop_src.png");
        let mut img = RgbaImage::from_pixel(64, 64, Rgba([0, 0, 0, 255]));
        let marker = Rgba([222, 111, 33, 255]);
        img.put_pixel(20, 24, marker); // inside the crop at (x=16,y=16,16×16)
        img.save_with_format(&src, image::ImageFormat::Png).unwrap();

        let crop = Crop { x: 16, y: 16, width: 16, height: 16 };
        let (buf, info) = render_preview_buffer(&src, None, Some(crop)).unwrap();
        assert_eq!((info.source_width, info.source_height), (16, 16));
        assert_eq!((info.crop_x, info.crop_y), (16, 16));
        assert_eq!(info.scale, 16); // 16px long edge -> 16x -> 256px
        assert_eq!((info.preview_width, info.preview_height), (256, 256));
        // The marker at source (20,24) is crop-local (4,8); at 16x its block starts at
        // preview (64,128) and carries the marker colour.
        assert_eq!((buf.width(), buf.height()), (256, 256));
        assert_eq!(*buf.get_pixel(64, 128), marker);

        let _ = std::fs::remove_file(&src);
    }

    #[test]
    fn render_preview_buffer_full_crop_matches_uncropped() {
        // crop covering the whole canvas must reproduce the uncropped buffer exactly.
        let dir = std::env::temp_dir();
        let src = dir.join("aseprite_mcp_preview_fullcrop_src.png");
        let mut img = RgbaImage::from_pixel(8, 6, Rgba([5, 6, 7, 255]));
        img.put_pixel(3, 2, Rgba([9, 9, 9, 255]));
        img.save_with_format(&src, image::ImageFormat::Png).unwrap();

        let (a, ia) = render_preview_buffer(&src, Some(4), None).unwrap();
        let (b, ib) = render_preview_buffer(&src, Some(4), Some(Crop { x: 0, y: 0, width: 8, height: 6 })).unwrap();
        assert_eq!(ia, ib);
        assert_eq!((ib.crop_x, ib.crop_y), (0, 0));
        assert_eq!(a.into_raw(), b.into_raw());

        let _ = std::fs::remove_file(&src);
    }

    #[test]
    fn render_preview_auto_scales_when_no_scale_given() {
        let dir = std::env::temp_dir();
        let src = dir.join("aseprite_mcp_preview_auto_src.png");
        let dst = dir.join("aseprite_mcp_preview_auto_dst.png");
        let img = RgbaImage::new(32, 32);
        img.save_with_format(&src, image::ImageFormat::Png).unwrap();

        let info = render_preview(&src, &dst, None).unwrap();
        assert_eq!(info.scale, 16); // 32px -> 16x -> 512px
        assert_eq!((info.preview_width, info.preview_height), (512, 512));

        let _ = std::fs::remove_file(&src);
        let _ = std::fs::remove_file(&dst);
    }
}
