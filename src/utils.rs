//! Small shared helpers: hex-colour validation/parsing and a generic clamp.

/// Validate that `s` is a `#rrggbb` or `#rrggbbaa` colour (the leading `#` is
/// optional). Returns a descriptive error otherwise.
pub fn validate_hex_color(s: &str) -> Result<(), String> {
    let body = s.strip_prefix('#').unwrap_or(s);
    match body.len() {
        6 | 8 if body.bytes().all(|b| b.is_ascii_hexdigit()) => Ok(()),
        6 | 8 => Err("hex colour contains non-hex characters".to_string()),
        n => Err(format!(
            "hex colour needs 6 or 8 digits (#rrggbb or #rrggbbaa), got {n}"
        )),
    }
}

/// Parse `#rrggbb` / `#rrggbbaa` into `(r, g, b, a)`; alpha defaults to 255 when
/// absent. Assumes [`validate_hex_color`] has passed — any unparseable channel
/// reads as 0 (alpha as 255).
pub fn parse_hex_color_with_alpha(s: &str) -> (u8, u8, u8, u8) {
    let h = s.strip_prefix('#').unwrap_or(s);
    let channel = |at: usize| {
        u8::from_str_radix(h.get(at..at + 2).unwrap_or("00"), 16).unwrap_or(0)
    };
    let a = if h.len() >= 8 {
        u8::from_str_radix(&h[6..8], 16).unwrap_or(255)
    } else {
        255
    };
    (channel(0), channel(2), channel(4), a)
}

/// Clamp `v` into the inclusive range `[lo, hi]`.
#[allow(dead_code)]
pub fn clamp<T: PartialOrd>(v: T, lo: T, hi: T) -> T {
    if v < lo {
        lo
    } else if v > hi {
        hi
    } else {
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_hex_shapes() {
        assert!(validate_hex_color("#aabbcc").is_ok());
        assert!(validate_hex_color("aabbcc").is_ok());
        assert!(validate_hex_color("#aabbccdd").is_ok());
        assert!(validate_hex_color("#abc").is_err()); // wrong length
        assert!(validate_hex_color("#gghhii").is_err()); // non-hex
    }

    #[test]
    fn parses_rgb_and_rgba() {
        assert_eq!(parse_hex_color_with_alpha("#ff8000"), (255, 128, 0, 255));
        assert_eq!(parse_hex_color_with_alpha("00ff0080"), (0, 255, 0, 128));
    }

    #[test]
    fn clamp_bounds() {
        assert_eq!(clamp(5, 0, 10), 5);
        assert_eq!(clamp(-3, 0, 10), 0);
        assert_eq!(clamp(42, 0, 10), 10);
    }
}
