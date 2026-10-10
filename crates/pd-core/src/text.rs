//! Text measurement from the bundled font (SPEC-Rust §5.4).
//!
//! Label widths change saved geometry (a renamed node widens to fit its label,
//! S §5.3), so they must be the same on every platform. They are measured here
//! from the bundled font's own data, never from egui or the system. Text is
//! shaped with harfrust, the shaper egui uses, so a width measured here matches
//! the text egui draws, kerning included.

use std::sync::OnceLock;

use harfrust::{ShaperData, UnicodeBuffer};
use skrifa::FontRef;
use skrifa::instance::{LocationRef, Size};
use skrifa::metrics::Metrics;

/// The one bundled font, used for labels everywhere: on screen, in SVG, PNG and
/// PDF. Noto Sans Regular, under the SIL Open Font Licence
/// (`assets/fonts/NotoSans-OFL.txt`).
pub static FONT_DATA: &[u8] = include_bytes!("../../../assets/fonts/NotoSans-Regular.ttf");

/// The family name of [`FONT_DATA`].
pub const FONT_NAME: &str = "Noto Sans";

/// Vertical metrics of the bundled font at one size, in the same units as the
/// size. `descent` is negative (below the baseline).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FontMetrics {
    pub ascent: f64,
    pub descent: f64,
    pub cap_height: f64,
}

struct Face {
    font: FontRef<'static>,
    shaper_data: ShaperData,
    metrics: Metrics,
}

fn face() -> &'static Face {
    static FACE: OnceLock<Face> = OnceLock::new();
    FACE.get_or_init(|| {
        let font = FontRef::new(FONT_DATA).expect("the bundled font is a valid font file");
        let shaper_data = ShaperData::new(&font);
        let metrics = Metrics::new(&font, Size::unscaled(), LocationRef::default());
        Face {
            font,
            shaper_data,
            metrics,
        }
    })
}

/// Scale from font design units to `size` (the size of one em).
fn units_to(size: f64) -> f64 {
    size / f64::from(face().metrics.units_per_em)
}

/// The advance width of one line of `text` set in the bundled font at `size`
/// (font size, the height of one em). An empty string has width 0.
pub fn text_width(text: &str, size: f64) -> f64 {
    if text.is_empty() {
        return 0.0;
    }
    let face = face();
    let shaper = face.shaper_data.shaper(&face.font).build();
    let mut buffer = UnicodeBuffer::new();
    buffer.push_str(text);
    buffer.guess_segment_properties();
    let glyphs = shaper.shape(buffer, harfrust::ShapeOptions::new());
    let units: i64 = glyphs
        .glyph_positions()
        .iter()
        .map(|p| i64::from(p.x_advance))
        .sum();
    units as f64 * units_to(size)
}

/// Ascent, descent and cap height of the bundled font at `size`.
pub fn font_metrics(size: f64) -> FontMetrics {
    let m = &face().metrics;
    let k = units_to(size);
    // Every version of Noto Sans has a cap height; 0.7 em is the usual fallback.
    let cap = m
        .cap_height
        .map_or(0.7 * f64::from(m.units_per_em), f64::from);
    FontMetrics {
        ascent: f64::from(m.ascent) * k,
        descent: f64::from(m.descent) * k,
        cap_height: cap * k,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_text_has_no_width() {
        assert_eq!(text_width("", 12.0), 0.0);
    }

    #[test]
    fn width_is_positive_and_grows_with_the_text() {
        let s1 = text_width("S1", 12.0);
        let s123 = text_width("S123", 12.0);
        assert!(s1 > 0.0);
        assert!(s123 > s1);
    }

    #[test]
    fn width_scales_linearly_with_size() {
        let w12 = text_width("Glucose_6P", 12.0);
        let w24 = text_width("Glucose_6P", 24.0);
        assert!((w24 - 2.0 * w12).abs() < 1e-9);
    }

    #[test]
    fn kerning_is_applied() {
        // Noto Sans kerns "AV" through GPOS, so the pair is narrower than its
        // letters set apart. A measurer that ignored GPOS would get them equal.
        let pair = text_width("AV", 100.0);
        let apart = text_width("A", 100.0) + text_width("V", 100.0);
        assert!(pair < apart - 1.0, "pair {pair}, apart {apart}");
    }

    #[test]
    fn metrics_are_plausible() {
        let m = font_metrics(12.0);
        assert!(m.ascent > m.cap_height && m.cap_height > 0.0);
        assert!(m.descent < 0.0);
        // A cap height a little over 0.7 em, as in Noto Sans.
        assert!(
            (8.0..9.5).contains(&m.cap_height),
            "cap height {}",
            m.cap_height
        );
    }

    #[test]
    fn measurement_is_stable() {
        // Pins the measurement so an accidental change of font or shaping shows
        // up here, not as nodes that are quietly a little wider in saved files.
        let w = text_width("S1", 12.0);
        assert!((w - 13.452).abs() < 1e-9, "width of S1 at 12: {w}");
        let w = text_width("Glucose_6_phosphate", 12.0);
        assert!(
            (w - 122.736).abs() < 1e-9,
            "width of Glucose_6_phosphate at 12: {w}"
        );
    }
}
