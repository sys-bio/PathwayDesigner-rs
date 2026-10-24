//! Colors in a diagram (docs/diagram-format.md §5.1): 17 basic names, the
//! file's own constants, and hex values.

use crate::model::Rgba;

/// The 16 colors HTML and SVG have always had, and `orange`, with the values
/// libAntimony's color table gives them.
pub const BASIC: [(&str, Rgba); 17] = [
    ("black", Rgba::rgb(0x00, 0x00, 0x00)),
    ("white", Rgba::rgb(0xFF, 0xFF, 0xFF)),
    ("gray", Rgba::rgb(0x80, 0x80, 0x80)),
    ("silver", Rgba::rgb(0xC0, 0xC0, 0xC0)),
    ("red", Rgba::rgb(0xFF, 0x00, 0x00)),
    ("maroon", Rgba::rgb(0x80, 0x00, 0x00)),
    ("orange", Rgba::rgb(0xFF, 0xA5, 0x00)),
    ("yellow", Rgba::rgb(0xFF, 0xFF, 0x00)),
    ("olive", Rgba::rgb(0x80, 0x80, 0x00)),
    ("lime", Rgba::rgb(0x00, 0xFF, 0x00)),
    ("green", Rgba::rgb(0x00, 0x80, 0x00)),
    ("teal", Rgba::rgb(0x00, 0x80, 0x80)),
    ("aqua", Rgba::rgb(0x00, 0xFF, 0xFF)),
    ("blue", Rgba::rgb(0x00, 0x00, 0xFF)),
    ("navy", Rgba::rgb(0x00, 0x00, 0x80)),
    ("purple", Rgba::rgb(0x80, 0x00, 0x80)),
    ("fuchsia", Rgba::rgb(0xFF, 0x00, 0xFF)),
];

/// A basic color's value.
pub fn basic(name: &str) -> Option<Rgba> {
    BASIC.iter().find(|(n, _)| *n == name).map(|(_, c)| *c)
}

/// The basic name of exactly this color, if it has one.
pub fn basic_name(c: Rgba) -> Option<&'static str> {
    BASIC.iter().find(|(_, v)| *v == c).map(|(n, _)| *n)
}

/// `#RGB`, `#RRGGBB` or `#RRGGBBAA` (the `#` included), in either case.
pub fn parse_hex(s: &str) -> Option<Rgba> {
    let hex = s.strip_prefix('#')?;
    if hex.len() == 3 && hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        let d = |i: usize| u8::from_str_radix(&hex[i..i + 1], 16).ok().map(|v| v * 17);
        return Some(Rgba::rgb(d(0)?, d(1)?, d(2)?));
    }
    if hex.len() != 6 && hex.len() != 8 {
        return None;
    }
    crate::json::colour::from_hex(s)
}

/// `#RRGGBB`, or `#RRGGBBAA` when not opaque; upper case.
pub fn to_hex(c: Rgba) -> String {
    let [r, g, b, a] = c.0;
    if a == 255 {
        format!("#{r:02X}{g:02X}{b:02X}")
    } else {
        format!("#{r:02X}{g:02X}{b:02X}{a:02X}")
    }
}

/// The colors a file defines (`color metab = #FFD54F`), in order. A name
/// defined twice keeps its first value (format §13).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Constants(Vec<(String, Rgba)>);

impl Constants {
    /// Defines `name`; false (and no change) if it was defined already.
    pub fn define(&mut self, name: &str, value: Rgba) -> bool {
        if self.get(name).is_some() {
            return false;
        }
        self.0.push((name.to_owned(), value));
        true
    }

    pub fn get(&self, name: &str) -> Option<Rgba> {
        self.0.iter().find(|(n, _)| n == name).map(|(_, c)| *c)
    }

    /// A name's color: the file's constant, else the basic color.
    pub fn resolve(&self, name: &str) -> Option<Rgba> {
        self.get(name).or_else(|| basic(name))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_basic_names_have_their_html_values() {
        assert_eq!(BASIC.len(), 17);
        assert_eq!(basic("orange"), Some(Rgba::rgb(255, 165, 0)));
        assert_eq!(basic("gray"), Some(Rgba::rgb(128, 128, 128)));
        assert_eq!(basic("grey"), None);
        assert_eq!(basic("Orange"), None, "names are lower case");
        assert_eq!(basic_name(Rgba::rgb(0, 0, 128)), Some("navy"));
        assert_eq!(basic_name(Rgba::rgb(0, 0, 127)), None);
        // Each name once, each value once.
        for (i, (n, c)) in BASIC.iter().enumerate() {
            assert!(BASIC[i + 1..].iter().all(|(m, d)| m != n && d != c), "{n}");
        }
    }

    #[test]
    fn hex_in_three_lengths() {
        assert_eq!(parse_hex("#FFD54F"), Some(Rgba::rgb(0xFF, 0xD5, 0x4F)));
        assert_eq!(parse_hex("#ffd54f"), Some(Rgba::rgb(0xFF, 0xD5, 0x4F)));
        assert_eq!(parse_hex("#F80"), Some(Rgba::rgb(0xFF, 0x88, 0x00)));
        assert_eq!(parse_hex("#FFD54F80"), Some(Rgba([0xFF, 0xD5, 0x4F, 0x80])));
        for bad in ["FFD54F", "#FFD5", "#GGGGGG", "#", "#FFD54F8"] {
            assert_eq!(parse_hex(bad), None, "{bad}");
        }
        assert_eq!(to_hex(Rgba::rgb(0xFF, 0xD5, 0x4F)), "#FFD54F");
        assert_eq!(to_hex(Rgba([1, 2, 3, 4])), "#01020304");
    }

    #[test]
    fn constants_come_before_basic_names_and_keep_their_first_value() {
        let mut k = Constants::default();
        assert!(k.define("metab", Rgba::rgb(1, 2, 3)));
        assert!(k.define("blue", Rgba::rgb(0x15, 0x65, 0xC0)));
        assert!(!k.define("metab", Rgba::rgb(9, 9, 9)));
        assert_eq!(k.resolve("metab"), Some(Rgba::rgb(1, 2, 3)));
        assert_eq!(k.resolve("blue"), Some(Rgba::rgb(0x15, 0x65, 0xC0)));
        assert_eq!(k.resolve("red"), Some(Rgba::rgb(255, 0, 0)));
        assert_eq!(k.resolve("bisque"), None);
    }
}
