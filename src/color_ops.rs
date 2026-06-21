//! Constrained / semantic colour operations (SPEC-004 Phase 1).
//!
//! Pure, deterministic colour math — no Aseprite, no I/O — so every rule is
//! unit-testable (mirrors `autotile.rs` / `tileset_export.rs` / `ascii_view.rs`).
//! The live tools (`live_palette_snap`, `live_adjust_pixels`, `live_snap_colors`,
//! SPEC-004 Phases 2–4) fetch a region's *unique* colours from the plugin, build a
//! colour→colour map here, and apply it in one plugin replace pass.
//!
//! Why this exists: the #1 hand-drawing failure is the agent picking colours by
//! eye — off-palette strokes, value-only ramps, slightly-wrong shades. Here a
//! **real CIELAB** palette snap (CIEDE2000, not RGBA euclidean — the honest
//! version of the competitor pixel-mcp's claimed-but-fake "LAB snap") plus intent
//! ops (darken/lighten/hue-shift/colorize) that bake in the project hue-shift rule
//! (shadows cooler, highlights warmer) make every colour operation legal by
//! construction. See `docs/research/agent-pixel-art-techniques.md` §B/§D.
//!
//! Phase 1 ships the pure core; the live tools that consume it land in Phases 2–4,
//! so the public surface is exercised by the unit tests below until then.
#![allow(dead_code)]
// `Rgba::rgb` / `Rgba::rgba` is the idiomatic constructor pair (cf. the `image`
// crate); the rgba/Rgba name overlap is intentional and reads clearly.
#![allow(clippy::self_named_constructors)]

// ---------------------------------------------------------------------------
// Colour types
// ---------------------------------------------------------------------------

/// An 8-bit straight-alpha RGBA colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Rgba {
    pub fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    pub fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    /// Parse `#rrggbb` or `#rrggbbaa` (reuses the project's hex validation).
    pub fn from_hex(s: &str) -> Result<Self, String> {
        crate::utils::validate_hex_color(s)?;
        let (r, g, b, a) = crate::utils::parse_hex_color_with_alpha(s);
        Ok(Self { r, g, b, a })
    }

    /// `#rrggbb` when opaque, else `#rrggbbaa` — the wire format the live tools use.
    pub fn to_hex(self) -> String {
        if self.a == 255 {
            format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
        } else {
            format!("#{:02x}{:02x}{:02x}{:02x}", self.r, self.g, self.b, self.a)
        }
    }

    pub fn is_transparent(self) -> bool {
        self.a == 0
    }
}

/// A CIELAB colour (D65).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Lab {
    pub l: f64,
    pub a: f64,
    pub b: f64,
}

// ---------------------------------------------------------------------------
// sRGB ↔ CIELAB (D65)
// ---------------------------------------------------------------------------

fn srgb_channel_to_linear(c: f64) -> f64 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// Convert an sRGB colour to CIELAB (alpha ignored). D65 white point.
pub fn srgb_to_lab(c: Rgba) -> Lab {
    let r = srgb_channel_to_linear(c.r as f64 / 255.0);
    let g = srgb_channel_to_linear(c.g as f64 / 255.0);
    let b = srgb_channel_to_linear(c.b as f64 / 255.0);

    // Linear sRGB -> XYZ (D65).
    let x = 0.412_456_4 * r + 0.357_576_1 * g + 0.180_437_5 * b;
    let y = 0.212_672_9 * r + 0.715_152_2 * g + 0.072_175_0 * b;
    let z = 0.019_333_9 * r + 0.119_192_0 * g + 0.950_304_1 * b;

    // Normalise by the D65 reference white.
    let xn = x / 0.950_47;
    let yn = y / 1.0;
    let zn = z / 1.088_83;

    let f = |t: f64| {
        const DELTA: f64 = 6.0 / 29.0;
        if t > DELTA * DELTA * DELTA {
            t.cbrt()
        } else {
            t / (3.0 * DELTA * DELTA) + 4.0 / 29.0
        }
    };
    let (fx, fy, fz) = (f(xn), f(yn), f(zn));
    Lab {
        l: 116.0 * fy - 16.0,
        a: 500.0 * (fx - fy),
        b: 200.0 * (fy - fz),
    }
}

// ---------------------------------------------------------------------------
// ΔE (CIEDE2000) — the perceptual colour difference. This is the metric that
// makes the palette snap "real LAB"; RGBA euclidean (what pixel-mcp ships) is
// kept only in tests to prove the two can disagree.
// ---------------------------------------------------------------------------

fn hue_deg(b: f64, a_prime: f64) -> f64 {
    if b == 0.0 && a_prime == 0.0 {
        return 0.0;
    }
    let mut h = b.atan2(a_prime).to_degrees();
    if h < 0.0 {
        h += 360.0;
    }
    h
}

/// CIEDE2000 colour difference between two CIELAB colours (kL=kC=kH=1).
pub fn ciede2000(p1: Lab, p2: Lab) -> f64 {
    let pow7 = |x: f64| x.powi(7);
    let twenty_five_pow7 = pow7(25.0);

    let c1 = (p1.a * p1.a + p1.b * p1.b).sqrt();
    let c2 = (p2.a * p2.a + p2.b * p2.b).sqrt();
    let c_bar = (c1 + c2) / 2.0;
    let c_bar7 = pow7(c_bar);
    let g = 0.5 * (1.0 - (c_bar7 / (c_bar7 + twenty_five_pow7)).sqrt());

    let a1p = (1.0 + g) * p1.a;
    let a2p = (1.0 + g) * p2.a;
    let c1p = (a1p * a1p + p1.b * p1.b).sqrt();
    let c2p = (a2p * a2p + p2.b * p2.b).sqrt();
    let h1p = hue_deg(p1.b, a1p);
    let h2p = hue_deg(p2.b, a2p);

    let dlp = p2.l - p1.l;
    let dcp = c2p - c1p;
    let dhp = if c1p * c2p == 0.0 {
        0.0
    } else {
        let diff = h2p - h1p;
        if diff > 180.0 {
            diff - 360.0
        } else if diff < -180.0 {
            diff + 360.0
        } else {
            diff
        }
    };
    let big_dhp = 2.0 * (c1p * c2p).sqrt() * (dhp.to_radians() / 2.0).sin();

    let lp_bar = (p1.l + p2.l) / 2.0;
    let cp_bar = (c1p + c2p) / 2.0;
    let hp_bar = if c1p * c2p == 0.0 {
        h1p + h2p
    } else if (h1p - h2p).abs() <= 180.0 {
        (h1p + h2p) / 2.0
    } else if h1p + h2p < 360.0 {
        (h1p + h2p + 360.0) / 2.0
    } else {
        (h1p + h2p - 360.0) / 2.0
    };

    let t = 1.0 - 0.17 * (hp_bar - 30.0).to_radians().cos()
        + 0.24 * (2.0 * hp_bar).to_radians().cos()
        + 0.32 * (3.0 * hp_bar + 6.0).to_radians().cos()
        - 0.20 * (4.0 * hp_bar - 63.0).to_radians().cos();
    let d_theta = 30.0 * (-(((hp_bar - 275.0) / 25.0).powi(2))).exp();
    let cp_bar7 = pow7(cp_bar);
    let rc = 2.0 * (cp_bar7 / (cp_bar7 + twenty_five_pow7)).sqrt();
    let sl = 1.0 + (0.015 * (lp_bar - 50.0).powi(2)) / (20.0 + (lp_bar - 50.0).powi(2)).sqrt();
    let sc = 1.0 + 0.045 * cp_bar;
    let sh = 1.0 + 0.015 * cp_bar * t;
    let rt = -(2.0 * d_theta).to_radians().sin() * rc;

    let term_l = dlp / sl;
    let term_c = dcp / sc;
    let term_h = big_dhp / sh;
    (term_l * term_l + term_c * term_c + term_h * term_h + rt * term_c * term_h).sqrt()
}

/// Perceptual ΔE (CIEDE2000) between two sRGB colours (alpha ignored).
pub fn delta_e(x: Rgba, y: Rgba) -> f64 {
    ciede2000(srgb_to_lab(x), srgb_to_lab(y))
}

// ---------------------------------------------------------------------------
// Palette snapping
// ---------------------------------------------------------------------------

/// Index of the perceptually-nearest palette colour (CIEDE2000). `None` for an
/// empty palette.
pub fn nearest_palette_index(c: Rgba, palette: &[Rgba]) -> Option<usize> {
    if palette.is_empty() {
        return None;
    }
    let mut best = 0usize;
    let mut best_d = f64::INFINITY;
    for (i, p) in palette.iter().enumerate() {
        let d = delta_e(c, *p);
        if d < best_d {
            best_d = d;
            best = i;
        }
    }
    Some(best)
}

/// Snap a colour to the nearest palette colour, preserving the input's alpha
/// (so a semi-transparent pixel keeps its alpha but takes a legal RGB).
pub fn clamp_to_palette(c: Rgba, palette: &[Rgba]) -> Rgba {
    match nearest_palette_index(c, palette) {
        Some(i) => {
            let p = palette[i];
            Rgba::rgba(p.r, p.g, p.b, c.a)
        }
        None => c,
    }
}

/// Map a colour to a ramp step by its luma (0..1 → ramp index 0..len-1, dark→light).
/// Transparent stays transparent and the original alpha is preserved. The ramp must be
/// ordered dark→light. Used by `live_gradient_map` (SPEC-009) to re-shade a region onto a
/// target ramp — a StyleProfile ramp feeds straight in, and the result is palette-legal by
/// construction (only ramp colours are emitted).
pub fn gradient_map(c: Rgba, ramp: &[Rgba]) -> Rgba {
    if ramp.is_empty() || c.is_transparent() {
        return c;
    }
    let luma = (0.299 * c.r as f64 + 0.587 * c.g as f64 + 0.114 * c.b as f64) / 255.0;
    let idx = ((luma * ramp.len() as f64) as usize).min(ramp.len() - 1);
    let p = ramp[idx];
    Rgba::rgba(p.r, p.g, p.b, c.a)
}

// ---------------------------------------------------------------------------
// HSV (for the semantic ops)
// ---------------------------------------------------------------------------

/// (hue [0,360), saturation [0,1], value [0,1]).
fn to_hsv(c: Rgba) -> (f64, f64, f64) {
    let r = c.r as f64 / 255.0;
    let g = c.g as f64 / 255.0;
    let b = c.b as f64 / 255.0;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let d = max - min;
    let v = max;
    let s = if max == 0.0 { 0.0 } else { d / max };
    let mut h = if d == 0.0 {
        0.0
    } else if max == r {
        60.0 * (((g - b) / d) % 6.0)
    } else if max == g {
        60.0 * ((b - r) / d + 2.0)
    } else {
        60.0 * ((r - g) / d + 4.0)
    };
    if h < 0.0 {
        h += 360.0;
    }
    (h, s, v)
}

fn from_hsv(h: f64, s: f64, v: f64, a: u8) -> Rgba {
    let h = ((h % 360.0) + 360.0) % 360.0;
    let s = s.clamp(0.0, 1.0);
    let v = v.clamp(0.0, 1.0);
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = v - c;
    let (r1, g1, b1) = match (h / 60.0) as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let to_u8 = |f: f64| (((f + m) * 255.0).round() as i64).clamp(0, 255) as u8;
    Rgba::rgba(to_u8(r1), to_u8(g1), to_u8(b1), a)
}

// ---------------------------------------------------------------------------
// Semantic intent ops — encode the project hue-shift rule: shadows shift toward
// blue (cooler), highlights toward orange/yellow (warmer).
// ---------------------------------------------------------------------------

/// Hue (deg) shadows rotate toward.
const COOL_HUE: f64 = 240.0;
/// Hue (deg) highlights rotate toward.
const WARM_HUE: f64 = 50.0;
/// Max hue rotation (deg) at amount = 1.0.
const MAX_HUE_SHIFT: f64 = 40.0;
/// Below this saturation a colour is treated as neutral (no hue rotation).
const NEUTRAL_S: f64 = 0.04;

/// Rotate `h` toward `target` along the shortest arc, by at most `max_deg`
/// (never overshooting the target).
fn shift_hue_toward(h: f64, target: f64, max_deg: f64) -> f64 {
    let mut diff = target - h;
    while diff > 180.0 {
        diff -= 360.0;
    }
    while diff < -180.0 {
        diff += 360.0;
    }
    let step = diff.signum() * diff.abs().min(max_deg);
    ((h + step) % 360.0 + 360.0) % 360.0
}

/// Darken: lower value by `amount` (fraction 0..1) and cool the hue (toward blue).
pub fn darken(c: Rgba, amount: f64) -> Rgba {
    let amount = amount.clamp(0.0, 1.0);
    let (h, s, v) = to_hsv(c);
    let nv = v * (1.0 - amount);
    let nh = if s > NEUTRAL_S {
        shift_hue_toward(h, COOL_HUE, MAX_HUE_SHIFT * amount)
    } else {
        h
    };
    from_hsv(nh, s, nv, c.a)
}

/// Lighten: raise value toward white by `amount` and warm the hue (toward orange).
pub fn lighten(c: Rgba, amount: f64) -> Rgba {
    let amount = amount.clamp(0.0, 1.0);
    let (h, s, v) = to_hsv(c);
    let nv = v + (1.0 - v) * amount;
    let nh = if s > NEUTRAL_S {
        shift_hue_toward(h, WARM_HUE, MAX_HUE_SHIFT * amount)
    } else {
        h
    };
    from_hsv(nh, s, nv, c.a)
}

/// Rotate the hue by `degrees` (value/saturation kept).
pub fn hue_shift(c: Rgba, degrees: f64) -> Rgba {
    let (h, s, v) = to_hsv(c);
    from_hsv(h + degrees, s, v, c.a)
}

/// Set the hue to `target_hue`, keeping value and saturation (Magic Pencil).
pub fn colorize(c: Rgba, target_hue: f64) -> Rgba {
    let (_, s, v) = to_hsv(c);
    from_hsv(target_hue, s, v, c.a)
}

// ---------------------------------------------------------------------------
// Op dispatch + colour-map builder (the live-tool orchestration primitive)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ColorOp {
    /// Snap to the nearest palette colour (always palette-clamped).
    Snap,
    /// Darken by a 0..1 fraction.
    Darken(f64),
    /// Lighten by a 0..1 fraction.
    Lighten(f64),
    /// Rotate hue by N degrees.
    HueShift(f64),
    /// Set hue to N degrees.
    Colorize(f64),
}

impl ColorOp {
    /// Parse the live-tool params. `amount` is the fraction for darken/lighten and
    /// the degrees for hue_shift; `hue` is required for colorize.
    pub fn parse(op: &str, amount: f64, hue: Option<f64>) -> Result<ColorOp, String> {
        match op.trim().to_ascii_lowercase().as_str() {
            "snap" => Ok(ColorOp::Snap),
            "darken" => Ok(ColorOp::Darken(amount)),
            "lighten" => Ok(ColorOp::Lighten(amount)),
            "hue_shift" | "hueshift" => Ok(ColorOp::HueShift(amount)),
            "colorize" => hue
                .map(ColorOp::Colorize)
                .ok_or_else(|| "colorize requires `hue`".to_string()),
            other => Err(format!(
                "unknown op '{other}' (snap | darken | lighten | hue_shift | colorize)"
            )),
        }
    }

    /// Apply to one colour. Fully-transparent pixels pass through unchanged. For
    /// non-snap ops, `clamp` (when true and the palette is non-empty) snaps the
    /// result back to the palette — legal by construction.
    pub fn apply(self, c: Rgba, palette: &[Rgba], clamp: bool) -> Rgba {
        if c.is_transparent() {
            return c;
        }
        let out = match self {
            ColorOp::Snap => return clamp_to_palette(c, palette),
            ColorOp::Darken(a) => darken(c, a),
            ColorOp::Lighten(a) => lighten(c, a),
            ColorOp::HueShift(d) => hue_shift(c, d),
            ColorOp::Colorize(h) => colorize(c, h),
        };
        if clamp && !palette.is_empty() {
            clamp_to_palette(out, palette)
        } else {
            out
        }
    }
}

/// Build the colour→colour map for a region's *unique* colours. Only colours that
/// actually change are returned, so the live apply is minimal (and a no-op call
/// returns an empty map). This is the whole point of the per-colour architecture:
/// only unique colours cross the wire, never per-pixel data.
pub fn build_color_map(
    uniques: &[Rgba],
    op: ColorOp,
    palette: &[Rgba],
    clamp: bool,
) -> Vec<(Rgba, Rgba)> {
    let mut map = Vec::new();
    for &c in uniques {
        let out = op.apply(c, palette, clamp);
        if out != c {
            map.push((c, out));
        }
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(s: &str) -> Rgba {
        Rgba::from_hex(s).unwrap()
    }

    fn approx(a: f64, b: f64, eps: f64) -> bool {
        (a - b).abs() <= eps
    }

    #[test]
    fn hex_round_trips() {
        assert_eq!(hex("#ff0000"), Rgba::rgb(255, 0, 0));
        assert_eq!(hex("#11223344"), Rgba::rgba(0x11, 0x22, 0x33, 0x44));
        assert_eq!(Rgba::rgb(255, 0, 0).to_hex(), "#ff0000");
        assert_eq!(Rgba::rgba(1, 2, 3, 4).to_hex(), "#01020304");
        assert!(Rgba::from_hex("nope").is_err());
    }

    #[test]
    fn srgb_to_lab_known_values() {
        let w = srgb_to_lab(Rgba::rgb(255, 255, 255));
        assert!(approx(w.l, 100.0, 0.05) && approx(w.a, 0.0, 0.05) && approx(w.b, 0.0, 0.05));
        let k = srgb_to_lab(Rgba::rgb(0, 0, 0));
        assert!(approx(k.l, 0.0, 0.05));
        // Reference red #ff0000 ≈ L 53.24, a 80.09, b 67.20.
        let red = srgb_to_lab(Rgba::rgb(255, 0, 0));
        assert!(approx(red.l, 53.24, 0.1), "L={}", red.l);
        assert!(approx(red.a, 80.09, 0.1), "a={}", red.a);
        assert!(approx(red.b, 67.20, 0.1), "b={}", red.b);
    }

    #[test]
    fn ciede2000_matches_sharma_reference_pairs() {
        // From Sharma et al.'s CIEDE2000 test data (Lab inputs, expected ΔE00).
        let cases = [
            (
                Lab { l: 50.0, a: 2.6772, b: -79.7751 },
                Lab { l: 50.0, a: 0.0, b: -82.7485 },
                2.0425,
            ),
            (
                Lab { l: 50.0, a: 2.4900, b: -0.0010 },
                Lab { l: 50.0, a: -2.4900, b: 0.0009 },
                7.1792,
            ),
            (
                Lab { l: 60.2574, a: -34.0099, b: 36.2677 },
                Lab { l: 60.4626, a: -34.1751, b: 39.4387 },
                1.2644,
            ),
            (Lab { l: 50.0, a: 0.0, b: 0.0 }, Lab { l: 50.0, a: -1.0, b: 2.0 }, 2.3669),
        ];
        for (p1, p2, expected) in cases {
            let got = ciede2000(p1, p2);
            assert!(approx(got, expected, 1e-3), "got {got}, want {expected}");
        }
    }

    #[test]
    fn delta_e_is_symmetric_and_zero_on_identity() {
        let a = hex("#3a7bd5");
        let b = hex("#d53a7b");
        assert!(approx(delta_e(a, a), 0.0, 1e-9));
        assert!(approx(delta_e(a, b), delta_e(b, a), 1e-9));
    }

    #[test]
    fn nearest_palette_index_basics() {
        let pal = [hex("#000000"), hex("#ff0000"), hex("#00ff00"), hex("#0000ff")];
        assert_eq!(nearest_palette_index(hex("#fe0101"), &pal), Some(1));
        assert_eq!(nearest_palette_index(hex("#010102"), &pal), Some(0));
        assert_eq!(nearest_palette_index(hex("#ff0000"), &[]), None);
    }

    #[test]
    fn clamp_to_palette_keeps_alpha() {
        let pal = [hex("#000000"), hex("#ffffff")];
        let snapped = clamp_to_palette(Rgba::rgba(250, 250, 250, 128), &pal);
        assert_eq!(snapped, Rgba::rgba(255, 255, 255, 128));
    }

    /// RGBA-euclidean nearest, kept only to prove the perceptual metric differs.
    fn nearest_rgb_index(c: Rgba, palette: &[Rgba]) -> usize {
        let mut best = 0usize;
        let mut best_d = i64::MAX;
        for (i, p) in palette.iter().enumerate() {
            let dr = c.r as i64 - p.r as i64;
            let dg = c.g as i64 - p.g as i64;
            let db = c.b as i64 - p.b as i64;
            let d = dr * dr + dg * dg + db * db;
            if d < best_d {
                best_d = d;
                best = i;
            }
        }
        best
    }

    #[test]
    fn lab_snap_can_disagree_with_rgb_snap() {
        // The honest differentiator: snapping by CIELAB ΔE picks a *different*
        // palette colour than RGBA-euclidean for some targets. Brute-force a
        // gamut-spanning palette to prove the two metrics genuinely diverge.
        let palette: Vec<Rgba> = [
            "#000000", "#ffffff", "#7f7f7f", "#ff0000", "#00ff00", "#0000ff", "#ffff00",
            "#00ffff", "#ff00ff", "#804000", "#008040", "#400080",
        ]
        .iter()
        .map(|h| hex(h))
        .collect();

        let mut disagreements = 0;
        let mut checked = 0;
        for r in (0..256).step_by(17) {
            for g in (0..256).step_by(17) {
                for b in (0..256).step_by(17) {
                    let t = Rgba::rgb(r as u8, g as u8, b as u8);
                    let lab_i = nearest_palette_index(t, &palette).unwrap();
                    let rgb_i = nearest_rgb_index(t, &palette);
                    checked += 1;
                    if lab_i != rgb_i {
                        disagreements += 1;
                    }
                }
            }
        }
        assert!(
            disagreements > 0,
            "LAB and RGB nearest never disagreed over {checked} targets — metric is suspect"
        );
    }

    #[test]
    fn darken_is_darker_and_cooler_lighten_is_lighter_and_warmer() {
        let red = hex("#e23838");
        let (h0, _, v0) = to_hsv(red);
        let dk = darken(red, 0.5);
        let lt = lighten(red, 0.5);
        let (hd, _, vd) = to_hsv(dk);
        let (hl, _, vl) = to_hsv(lt);
        assert!(vd < v0, "darken lowers value: {vd} !< {v0}");
        assert!(vl > v0, "lighten raises value: {vl} !> {v0}");
        let dist = |h: f64, t: f64| {
            let d = (h - t).abs();
            d.min(360.0 - d)
        };
        assert!(dist(hd, COOL_HUE) < dist(h0, COOL_HUE), "darken cools toward blue");
        assert!(dist(hl, WARM_HUE) < dist(h0, WARM_HUE), "lighten warms toward orange");
    }

    #[test]
    fn hue_shift_wraps_and_colorize_keeps_value() {
        let c = hex("#cc4040"); // a red
        let shifted = hue_shift(c, 120.0);
        let (h, _, _) = to_hsv(shifted);
        let (h0, _, v0) = to_hsv(c);
        let ang = |a: f64, b: f64| {
            let d = (a - b).abs();
            d.min(360.0 - d)
        };
        assert!(ang(h, (h0 + 120.0) % 360.0) < 2.0, "hue_shift +120: got {h}");
        let blued = colorize(c, 220.0);
        let (hb, _, vb) = to_hsv(blued);
        assert!(approx(hb, 220.0, 2.0), "colorize sets hue: {hb}");
        assert!(approx(vb, v0, 0.01), "colorize keeps value: {vb} vs {v0}");
    }

    #[test]
    fn build_color_map_skips_transparent_and_unchanged() {
        let pal = [hex("#101010"), hex("#e0e0e0")];
        let uniques = [
            hex("#111111"),          // snaps to #101010 (changes)
            hex("#101010"),          // already palette → no change
            Rgba::rgba(5, 5, 5, 0),  // transparent → pass through
        ];
        let map = build_color_map(&uniques, ColorOp::Snap, &pal, true);
        assert_eq!(map.len(), 1);
        assert_eq!(map[0].0, hex("#111111"));
        assert_eq!(map[0].1, hex("#101010"));

        // Snap is idempotent: snapping the palette itself yields an empty map.
        let pal_uniques: Vec<Rgba> = pal.to_vec();
        assert!(build_color_map(&pal_uniques, ColorOp::Snap, &pal, true).is_empty());
    }

    #[test]
    fn op_parse_and_clamp_toggle() {
        assert_eq!(ColorOp::parse("darken", 0.3, None).unwrap(), ColorOp::Darken(0.3));
        assert_eq!(ColorOp::parse("COLORIZE", 0.0, Some(200.0)).unwrap(), ColorOp::Colorize(200.0));
        assert!(ColorOp::parse("colorize", 0.0, None).is_err());
        assert!(ColorOp::parse("frobnicate", 0.0, None).is_err());

        // clamp=false on darken yields the raw shade (may be off-palette); clamp=true
        // snaps it into the palette.
        let pal = [hex("#000000"), hex("#ffffff")];
        let raw = ColorOp::Darken(0.5).apply(hex("#e23838"), &pal, false);
        let clamped = ColorOp::Darken(0.5).apply(hex("#e23838"), &pal, true);
        assert!(pal.contains(&clamped), "clamped result is palette-legal");
        assert!(!pal.contains(&raw), "raw result is the free shade");
    }

    #[test]
    fn gradient_map_picks_ramp_step_by_luma() {
        let ramp = [hex("#1b4d3e"), hex("#2e7d32"), hex("#a6d94a")]; // dark, mid, light
        assert_eq!(gradient_map(hex("#000000"), &ramp), ramp[0], "black -> darkest");
        assert_eq!(gradient_map(hex("#ffffff"), &ramp), ramp[2], "white -> lightest");
        let mid = gradient_map(hex("#808080"), &ramp);
        assert!(ramp.contains(&mid) && mid != ramp[0], "mid grey maps off the darkest");
        // transparent + empty ramp are no-ops; alpha is preserved.
        let t = Rgba::rgba(50, 60, 70, 0);
        assert_eq!(gradient_map(t, &ramp), t, "transparent unchanged");
        assert_eq!(gradient_map(hex("#000000"), &[]), hex("#000000"), "empty ramp unchanged");
        assert_eq!(gradient_map(Rgba::rgba(255, 255, 255, 128), &ramp).a, 128, "alpha preserved");
    }
}
