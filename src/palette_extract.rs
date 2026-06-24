//! Palette extraction (SPEC-006 Phase 2 auto-palette) — a faithful pure-Rust port of the offline
//! `tools/extract_palette.py`, so `live_import_reference` can reduce a reference to N colours
//! **live** without supplying a palette, and without a new crate dependency (the deferral reason).
//!
//! Three methods, picked per task (same as the Python):
//! - `frequency`  — the N most common colours; exact for art already limited-palette.
//! - `median_cut` — recursively split the colour box on its widest channel; good general reduction.
//! - `kmeans`     — Lloyd's algorithm, deterministically seeded from `median_cut`.
//!
//! Fully-transparent pixels are ignored; the result is luma-sorted (shadow → highlight) and
//! deduped. Pure (slice-in / Vec-out) so it is unit-tested without Aseprite.
#![allow(dead_code)]

use crate::color_ops::Rgba;
use image::RgbaImage;

/// Cap the working set so a large reference stays fast (deterministic stride sampling, no RNG —
/// matches `extract_palette.py::MAX_SAMPLES`). Pixel art is tiny; this only bites on photos.
pub const MAX_SAMPLES: usize = 50_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    Frequency,
    MedianCut,
    Kmeans,
}

impl Method {
    pub fn parse(s: &str) -> Result<Method, String> {
        match s {
            "frequency" => Ok(Method::Frequency),
            "median_cut" => Ok(Method::MedianCut),
            "kmeans" => Ok(Method::Kmeans),
            other => Err(format!(
                "palette method must be \"frequency\", \"median_cut\", or \"kmeans\" (got \"{other}\")"
            )),
        }
    }
}

fn luma(c: Rgba) -> f64 {
    0.299 * c.r as f64 + 0.587 * c.g as f64 + 0.114 * c.b as f64
}

fn avg(bucket: &[Rgba]) -> Rgba {
    let m = bucket.len() as u32;
    let (mut r, mut g, mut b) = (0u32, 0u32, 0u32);
    for c in bucket {
        r += c.r as u32;
        g += c.g as u32;
        b += c.b as u32;
    }
    Rgba::rgb((r / m) as u8, (g / m) as u8, (b / m) as u8)
}

/// The N most common colours (count desc; ties break to the lower packed RGB for determinism —
/// the one intentional difference from Python's insertion-order `Counter.most_common`).
fn frequency(samples: &[Rgba], n: usize) -> Vec<Rgba> {
    let mut counts: std::collections::HashMap<(u8, u8, u8), usize> = std::collections::HashMap::new();
    for c in samples {
        *counts.entry((c.r, c.g, c.b)).or_insert(0) += 1;
    }
    let mut v: Vec<_> = counts.into_iter().collect();
    v.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    v.into_iter().take(n).map(|((r, g, b), _)| Rgba::rgb(r, g, b)).collect()
}

/// Recursively split the bucket with the widest single-channel range, then average each bucket.
fn median_cut(samples: &[Rgba], n: usize) -> Vec<Rgba> {
    if samples.is_empty() || n < 1 {
        return Vec::new();
    }
    let mut buckets: Vec<Vec<Rgba>> = vec![samples.to_vec()];
    while buckets.len() < n {
        // Find the bucket+channel with the widest value range.
        let (mut best_i, mut best_range, mut best_ch) = (usize::MAX, -1i32, 0usize);
        for (i, b) in buckets.iter().enumerate() {
            if b.len() < 2 {
                continue;
            }
            for ch in 0..3 {
                let vals = b.iter().map(|c| chan(*c, ch) as i32);
                let (mn, mx) = vals.fold((255i32, 0i32), |(mn, mx), v| (mn.min(v), mx.max(v)));
                let rng = mx - mn;
                if rng > best_range {
                    best_range = rng;
                    best_i = i;
                    best_ch = ch;
                }
            }
        }
        if best_i == usize::MAX {
            break; // nothing left to split
        }
        // `Vec::remove` (order-preserving), NOT `swap_remove`: the split-target tie-break is by
        // bucket INDEX order (strict `>` above, first wins), so reordering the remaining buckets
        // would pick a different bucket on a later tie and diverge from the Python `list.pop(i)`.
        let mut b = buckets.remove(best_i);
        b.sort_by_key(|c| chan(*c, best_ch));
        let mid = b.len() / 2;
        let hi = b.split_off(mid);
        buckets.push(b);
        buckets.push(hi);
    }
    buckets.iter().filter(|b| !b.is_empty()).map(|b| avg(b)).collect()
}

fn chan(c: Rgba, ch: usize) -> u8 {
    match ch {
        0 => c.r,
        1 => c.g,
        _ => c.b,
    }
}

fn dist2(a: Rgba, b: Rgba) -> i64 {
    let dr = a.r as i64 - b.r as i64;
    let dg = a.g as i64 - b.g as i64;
    let db = a.b as i64 - b.b as i64;
    dr * dr + dg * dg + db * db
}

/// Lloyd's k-means, deterministically seeded from `median_cut` (no RNG), 12 iterations max.
fn kmeans(samples: &[Rgba], n: usize, iters: usize) -> Vec<Rgba> {
    let mut centroids = median_cut(samples, n);
    if centroids.is_empty() {
        return Vec::new();
    }
    for _ in 0..iters {
        let mut clusters: Vec<Vec<Rgba>> = vec![Vec::new(); centroids.len()];
        for &c in samples {
            let mut bi = 0usize;
            let mut bd = i64::MAX;
            for (i, &ct) in centroids.iter().enumerate() {
                let d = dist2(c, ct);
                if d < bd {
                    bd = d;
                    bi = i;
                }
            }
            clusters[bi].push(c);
        }
        let new: Vec<Rgba> = clusters
            .iter()
            .enumerate()
            .map(|(i, cl)| if cl.is_empty() { centroids[i] } else { avg(cl) })
            .collect();
        if new == centroids {
            break;
        }
        centroids = new;
    }
    centroids
}

/// Extract an `n`-colour palette from opaque `samples` with `method`, luma-sorted (shadow →
/// highlight) and deduped. Pure.
pub fn extract(samples: &[Rgba], method: Method, n: usize) -> Vec<Rgba> {
    let mut pal = match method {
        Method::Frequency => frequency(samples, n),
        Method::MedianCut => median_cut(samples, n),
        Method::Kmeans => kmeans(samples, n, 12),
    };
    pal.sort_by(|a, b| luma(*a).partial_cmp(&luma(*b)).unwrap_or(std::cmp::Ordering::Equal));
    let mut seen = std::collections::HashSet::new();
    pal.retain(|c| seen.insert((c.r, c.g, c.b)));
    pal
}

/// Collect opaque samples from an image, stride-capped at `MAX_SAMPLES` (deterministic), and
/// extract an `n`-colour palette. Returns empty when the image is fully transparent.
pub fn extract_from_image(img: &RgbaImage, method: Method, n: usize) -> Vec<Rgba> {
    let opaque: Vec<Rgba> = img
        .pixels()
        .filter(|p| p.0[3] != 0)
        .map(|p| Rgba::rgb(p.0[0], p.0[1], p.0[2]))
        .collect();
    let samples: Vec<Rgba> = if opaque.len() > MAX_SAMPLES {
        let stride = opaque.len().div_ceil(MAX_SAMPLES);
        opaque.iter().step_by(stride).copied().collect()
    } else {
        opaque
    };
    extract(&samples, method, n)
}

/// Extract ONE palette from many frames' opaque pixels combined (SPEC-012) — so an animation
/// snaps every frame to the same colours instead of flickering. Collects opaque samples across
/// all frames, stride-caps the union at `MAX_SAMPLES` (deterministic), and extracts. Empty when
/// every frame is fully transparent.
pub fn extract_from_images(imgs: &[&RgbaImage], method: Method, n: usize) -> Vec<Rgba> {
    let opaque: Vec<Rgba> = imgs
        .iter()
        .flat_map(|img| img.pixels())
        .filter(|p| p.0[3] != 0)
        .map(|p| Rgba::rgb(p.0[0], p.0[1], p.0[2]))
        .collect();
    let samples: Vec<Rgba> = if opaque.len() > MAX_SAMPLES {
        let stride = opaque.len().div_ceil(MAX_SAMPLES);
        opaque.iter().step_by(stride).copied().collect()
    } else {
        opaque
    };
    extract(&samples, method, n)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba as ImgRgba;

    fn s(colors: &[[u8; 3]]) -> Vec<Rgba> {
        colors.iter().map(|c| Rgba::rgb(c[0], c[1], c[2])).collect()
    }

    #[test]
    fn method_parse_rejects_unknown() {
        assert_eq!(Method::parse("kmeans").unwrap(), Method::Kmeans);
        assert!(Method::parse("octree").is_err());
    }

    #[test]
    fn extract_from_images_covers_colours_from_every_frame() {
        // Two solid frames with DISJOINT colours; the shared palette must contain both
        // (the cross-frame consistency guarantee for SPEC-012).
        let frame_a = RgbaImage::from_pixel(2, 2, ImgRgba([255, 0, 0, 255])); // red
        let frame_b = RgbaImage::from_pixel(2, 2, ImgRgba([0, 0, 255, 255])); // blue
        let pal = extract_from_images(&[&frame_a, &frame_b], Method::Frequency, 4);
        assert!(pal.contains(&Rgba::rgb(255, 0, 0)), "red missing: {pal:?}");
        assert!(pal.contains(&Rgba::rgb(0, 0, 255)), "blue missing: {pal:?}");
        // Fully-transparent frames yield an empty palette.
        let clear = RgbaImage::from_pixel(2, 2, ImgRgba([0, 0, 0, 0]));
        assert!(extract_from_images(&[&clear], Method::Frequency, 4).is_empty());
    }

    #[test]
    fn frequency_returns_the_most_common_luma_sorted() {
        // red ×3, blue ×1 — both survive top-2, output luma-sorted (blue < red).
        let mut samples = s(&[[255, 0, 0], [255, 0, 0], [255, 0, 0]]);
        samples.extend(s(&[[0, 0, 255]]));
        let pal = extract(&samples, Method::Frequency, 2);
        assert_eq!(pal, vec![Rgba::rgb(0, 0, 255), Rgba::rgb(255, 0, 0)]);
    }

    #[test]
    fn median_cut_splits_the_widest_channel() {
        // Four well-separated colours reduce to ~4 bucket averages; asking for 2 splits the
        // widest spread first. Deterministic.
        let samples = s(&[[0, 0, 0], [10, 0, 0], [240, 0, 0], [255, 0, 0]]);
        let pal = extract(&samples, Method::MedianCut, 2);
        assert_eq!(pal.len(), 2);
        // The split is on R (the only varying channel): a dark bucket and a light bucket.
        assert!(pal[0].r < 128 && pal[1].r > 128, "{pal:?}");
    }

    #[test]
    fn kmeans_is_deterministic_and_separates_clusters() {
        let mut samples = s(&[[10, 10, 10], [20, 20, 20], [12, 12, 12]]); // dark cluster
        samples.extend(s(&[[240, 240, 240], [250, 250, 250], [245, 245, 245]])); // light cluster
        let a = extract(&samples, Method::Kmeans, 2);
        let b = extract(&samples, Method::Kmeans, 2);
        assert_eq!(a, b, "kmeans must be deterministic");
        assert_eq!(a.len(), 2);
        assert!(a[0].r < 64 && a[1].r > 192, "clusters separated: {a:?}");
    }

    #[test]
    fn median_cut_matches_python_on_a_tie_heavy_input() {
        // Ground truth from `tools/extract_palette.py extract(..., "median_cut", 5)` — the
        // order-preserving `pop` this port mirrors via `Vec::remove`. This input has cross-bucket
        // range ties and DIVERGES under `swap_remove` (review-caught parity bug); locks the parity.
        let samples = s(&[
            [50, 100, 50], [150, 150, 0], [250, 0, 200], [200, 100, 100],
            [250, 100, 200], [150, 200, 150], [0, 0, 100],
        ]);
        let pal: Vec<String> =
            extract(&samples, Method::MedianCut, 5).iter().map(|c| c.to_hex()).collect();
        assert_eq!(pal, ["#000064", "#fa00c8", "#647d19", "#c86464", "#c896af"]);
    }

    #[test]
    fn requesting_more_than_distinct_returns_all_distinct() {
        let pal = extract(&s(&[[0, 0, 0], [255, 255, 255]]), Method::MedianCut, 16);
        assert_eq!(pal, vec![Rgba::rgb(0, 0, 0), Rgba::rgb(255, 255, 255)]);
    }

    #[test]
    fn extract_from_image_ignores_transparent() {
        let mut img = RgbaImage::from_pixel(2, 1, ImgRgba([255, 0, 0, 255]));
        img.put_pixel(1, 0, ImgRgba([0, 255, 0, 0])); // alpha 0 -> ignored
        assert_eq!(extract_from_image(&img, Method::Frequency, 8), vec![Rgba::rgb(255, 0, 0)]);
    }

    #[test]
    fn empty_or_zero_is_empty() {
        assert!(extract(&[], Method::MedianCut, 8).is_empty());
        assert!(extract(&s(&[[1, 2, 3]]), Method::Kmeans, 0).is_empty());
    }
}
