//! Colours in the native JSON file (S §11.1).
//!
//! Written as `#RRGGBBAA` in upper case, as the Delphi app writes them. Read
//! from `#RRGGBBAA` or `#RRGGBB` (opaque), with or without the `#`, or from the
//! legacy signed 32-bit integer in `0xAARRGGBB` form, which still turns up in
//! version 3 files.

use serde::de::{self, Deserializer, Visitor};
use serde::ser::Serializer;

use crate::model::Rgba;

/// `#RRGGBBAA`, upper case.
pub fn to_hex(c: Rgba) -> String {
    let [r, g, b, a] = c.0;
    format!("#{r:02X}{g:02X}{b:02X}{a:02X}")
}

/// Parses `#RRGGBBAA` or `#RRGGBB`, with or without the `#`, in either case.
pub fn from_hex(s: &str) -> Option<Rgba> {
    let hex = s.trim().strip_prefix('#').unwrap_or(s.trim());
    if !(hex.len() == 6 || hex.len() == 8) || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
    let alpha = if hex.len() == 8 { byte(6)? } else { 255 };
    Some(Rgba([byte(0)?, byte(2)?, byte(4)?, alpha]))
}

/// The legacy integer form: the bits of `0xAARRGGBB`, usually written as a
/// signed 32-bit number (so most opaque colours are negative).
pub fn from_legacy_int(v: i64) -> Option<Rgba> {
    if !(i64::from(i32::MIN)..=i64::from(u32::MAX)).contains(&v) {
        return None;
    }
    let [a, r, g, b] = (v as u32).to_be_bytes();
    Some(Rgba([r, g, b, a]))
}

pub fn serialize<S: Serializer>(c: &Rgba, s: S) -> Result<S::Ok, S::Error> {
    s.serialize_str(&to_hex(*c))
}

pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Rgba, D::Error> {
    struct ColourVisitor;

    impl Visitor<'_> for ColourVisitor {
        type Value = Rgba;

        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("a colour: \"#RRGGBBAA\", \"#RRGGBB\" or a 32-bit integer")
        }

        fn visit_str<E: de::Error>(self, v: &str) -> Result<Rgba, E> {
            from_hex(v).ok_or_else(|| E::invalid_value(de::Unexpected::Str(v), &self))
        }

        fn visit_i64<E: de::Error>(self, v: i64) -> Result<Rgba, E> {
            from_legacy_int(v).ok_or_else(|| E::invalid_value(de::Unexpected::Signed(v), &self))
        }

        fn visit_u64<E: de::Error>(self, v: u64) -> Result<Rgba, E> {
            i64::try_from(v)
                .ok()
                .and_then(from_legacy_int)
                .ok_or_else(|| E::invalid_value(de::Unexpected::Unsigned(v), &self))
        }

        fn visit_f64<E: de::Error>(self, v: f64) -> Result<Rgba, E> {
            if v.fract() == 0.0 {
                self.visit_i64(v as i64)
            } else {
                Err(E::invalid_value(de::Unexpected::Float(v), &self))
            }
        }
    }

    d.deserialize_any(ColourVisitor)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_round_trip() {
        let c = Rgba([0xFF, 0xE0, 0xB2, 0xFF]);
        assert_eq!(to_hex(c), "#FFE0B2FF");
        assert_eq!(from_hex("#FFE0B2FF"), Some(c));
        assert_eq!(to_hex(Rgba::DEFAULT), "#00000000");
    }

    #[test]
    fn hex_variants_are_accepted() {
        let c = Rgba([0x4A, 0x7F, 0xCB, 0xFF]);
        assert_eq!(from_hex("4A7FCBFF"), Some(c));
        assert_eq!(from_hex("#4a7fcbff"), Some(c));
        // Six digits mean opaque.
        assert_eq!(from_hex("#4A7FCB"), Some(c));
        assert_eq!(from_hex("4a7fcb"), Some(c));
        assert_eq!(from_hex("#12345678"), Some(Rgba([0x12, 0x34, 0x56, 0x78])));
    }

    #[test]
    fn bad_hex_is_rejected() {
        for bad in ["", "#", "#12345", "#1234567", "#GG0000FF", "#123456789"] {
            assert_eq!(from_hex(bad), None, "{bad}");
        }
    }

    #[test]
    fn legacy_integers_are_aarrggbb() {
        // -2572328 is 0xFFD8BFD8 (opaque thistle), from Models/model9.json.
        assert_eq!(
            from_legacy_int(-2572328),
            Some(Rgba([0xD8, 0xBF, 0xD8, 0xFF]))
        );
        assert_eq!(from_legacy_int(0), Some(Rgba::DEFAULT));
        // Unsigned form of the same bits.
        assert_eq!(
            from_legacy_int(0xFF4A7FCB),
            Some(Rgba([0x4A, 0x7F, 0xCB, 0xFF]))
        );
        assert_eq!(
            from_legacy_int(0x80112233),
            Some(Rgba([0x11, 0x22, 0x33, 0x80]))
        );
        assert_eq!(from_legacy_int(1 << 33), None);
        assert_eq!(from_legacy_int(i64::from(i32::MIN) - 1), None);
    }
}
