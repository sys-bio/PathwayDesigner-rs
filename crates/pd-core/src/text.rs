//! Text measurement from the bundled fonts (SPEC-Rust §5.4).
//!
//! Label widths change saved geometry (a renamed node widens to fit its label,
//! S §5.3), so they must be the same on every platform. They are measured here
//! from the bundled fonts' own data, never from egui or the system. Text is
//! shaped with harfrust, the shaper egui uses, so a width measured here matches
//! the text egui draws, kerning included.
//!
//! Labels always use Noto Sans Regular. Free text (S §7.8) can also use bold,
//! italic and Noto Serif: eight faces in all, each a font file of its own.

use std::fmt::Write as _;
use std::sync::OnceLock;

use harfrust::{GlyphBuffer, ShaperData, UnicodeBuffer};
use skrifa::instance::{LocationRef, Size};
use skrifa::metrics::Metrics;
use skrifa::outline::{DrawSettings, OutlinePen};
use skrifa::{FontRef, GlyphId, MetadataProvider};

use crate::model::{TextFont, Typeface};

/// The label font, used for labels everywhere: on screen, in SVG, PNG and
/// PDF. Noto Sans Regular. All the bundled fonts are Noto, version 2.015,
/// under the SIL Open Font Licence (`assets/fonts/NotoSans-OFL.txt`).
pub static FONT_DATA: &[u8] = include_bytes!("../../../assets/fonts/NotoSans-Regular.ttf");

/// The family name of [`FONT_DATA`].
pub const FONT_NAME: &str = "Noto Sans";

/// The font file for each face, in [`face_index`] order.
static FACE_DATA: [&[u8]; 8] = [
    FONT_DATA,
    include_bytes!("../../../assets/fonts/NotoSans-Bold.ttf"),
    include_bytes!("../../../assets/fonts/NotoSans-Italic.ttf"),
    include_bytes!("../../../assets/fonts/NotoSans-BoldItalic.ttf"),
    include_bytes!("../../../assets/fonts/NotoSerif-Regular.ttf"),
    include_bytes!("../../../assets/fonts/NotoSerif-Bold.ttf"),
    include_bytes!("../../../assets/fonts/NotoSerif-Italic.ttf"),
    include_bytes!("../../../assets/fonts/NotoSerif-BoldItalic.ttf"),
];

/// The font file of `font`, for the app to give egui.
pub fn font_data(font: TextFont) -> &'static [u8] {
    FACE_DATA[face_index(font)]
}

/// Every face, in a fixed order.
pub fn all_fonts() -> [TextFont; 8] {
    let mut out = [TextFont::default(); 8];
    for (i, f) in out.iter_mut().enumerate() {
        *f = TextFont {
            typeface: if i < 4 {
                Typeface::Sans
            } else {
                Typeface::Serif
            },
            bold: i & 1 != 0,
            italic: i & 2 != 0,
        };
    }
    out
}

/// A short name for `font`, unique among the faces ("serif-bold-italic").
pub fn font_key(font: TextFont) -> &'static str {
    [
        "sans",
        "sans-bold",
        "sans-italic",
        "sans-bold-italic",
        "serif",
        "serif-bold",
        "serif-italic",
        "serif-bold-italic",
    ][face_index(font)]
}

fn face_index(font: TextFont) -> usize {
    let serif = usize::from(font.typeface == Typeface::Serif);
    serif * 4 + usize::from(font.bold) + 2 * usize::from(font.italic)
}

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

fn face(font: TextFont) -> &'static Face {
    static FACES: [OnceLock<Face>; 8] = [const { OnceLock::new() }; 8];
    let i = face_index(font);
    FACES[i].get_or_init(|| {
        let font = FontRef::new(FACE_DATA[i]).expect("the bundled fonts are valid font files");
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
fn units_to(face: &Face, size: f64) -> f64 {
    size / f64::from(face.metrics.units_per_em)
}

/// The advance width of one line of `text` set in the label font at `size`
/// (font size, the height of one em). An empty string has width 0.
pub fn text_width(text: &str, size: f64) -> f64 {
    text_width_in(TextFont::default(), text, size)
}

/// [`text_width`] in any of the bundled faces.
pub fn text_width_in(font: TextFont, text: &str, size: f64) -> f64 {
    if text.is_empty() {
        return 0.0;
    }
    let face = face(font);
    let units: i64 = shape(face, text)
        .glyph_positions()
        .iter()
        .map(|p| i64::from(p.x_advance))
        .sum();
    units as f64 * units_to(face, size)
}

fn shape(face: &Face, text: &str) -> GlyphBuffer {
    let shaper = face.shaper_data.shaper(&face.font).build();
    let mut buffer = UnicodeBuffer::new();
    buffer.push_str(text);
    buffer.guess_segment_properties();
    shaper.shape(buffer, harfrust::ShapeOptions::new())
}

/// Receives glyph outlines in drawing coordinates (y pointing down), from
/// [`text_outline`]. SVG and PDF output each turn them into their own path
/// syntax.
pub trait PathSink {
    fn move_to(&mut self, x: f64, y: f64);
    fn line_to(&mut self, x: f64, y: f64);
    fn quad_to(&mut self, cx: f64, cy: f64, x: f64, y: f64);
    fn cubic_to(&mut self, c1x: f64, c1y: f64, c2x: f64, c2y: f64, x: f64, y: f64);
    fn close(&mut self);
}

/// The glyph outlines of one line of `text` at `size`, with the left end of
/// its baseline at (`x`, `baseline`) and y pointing down. Used where the text
/// must look the same without the font installed: SVG and PDF output draw
/// labels as outlines (SPEC-Rust §9). Shaped exactly as [`text_width`]
/// measures it.
pub fn text_outline(text: &str, size: f64, x: f64, baseline: f64, sink: &mut dyn PathSink) {
    text_outline_in(TextFont::default(), text, size, x, baseline, sink);
}

/// [`text_outline`] in any of the bundled faces.
pub fn text_outline_in(
    font: TextFont,
    text: &str,
    size: f64,
    x: f64,
    baseline: f64,
    sink: &mut dyn PathSink,
) {
    let face = face(font);
    let k = units_to(face, size);
    let outlines = face.font.outline_glyphs();
    let mut pen = Placed {
        sink,
        k,
        x0: 0.0,
        y0: baseline,
    };
    let mut pen_x = x;
    let glyphs = shape(face, text);
    for (info, pos) in glyphs.glyph_infos().iter().zip(glyphs.glyph_positions()) {
        pen.x0 = pen_x + f64::from(pos.x_offset) * k;
        pen.y0 = baseline - f64::from(pos.y_offset) * k;
        if let Some(glyph) = outlines.get(GlyphId::new(info.glyph_id)) {
            let settings = DrawSettings::unhinted(Size::unscaled(), LocationRef::default());
            // A glyph that fails to draw is left out; the rest of the text still shows.
            let _ = glyph.draw(settings, &mut pen);
        }
        pen_x += f64::from(pos.x_advance) * k;
    }
}

/// One line of `text` as SVG path data: [`text_outline`] in SVG syntax.
pub fn text_path(text: &str, size: f64, x: f64, baseline: f64) -> String {
    text_path_in(TextFont::default(), text, size, x, baseline)
}

/// [`text_path`] in any of the bundled faces.
pub fn text_path_in(font: TextFont, text: &str, size: f64, x: f64, baseline: f64) -> String {
    let mut svg = SvgPath(String::new());
    text_outline_in(font, text, size, x, baseline, &mut svg);
    svg.0
}

/// Passes outline commands in font units on to a [`PathSink`], scaled by `k`
/// and placed at (`x0`, `y0`) with y flipped.
struct Placed<'a> {
    sink: &'a mut dyn PathSink,
    k: f64,
    x0: f64,
    y0: f64,
}

impl Placed<'_> {
    fn at(&self, x: f32, y: f32) -> (f64, f64) {
        (
            self.x0 + f64::from(x) * self.k,
            self.y0 - f64::from(y) * self.k,
        )
    }
}

impl OutlinePen for Placed<'_> {
    fn move_to(&mut self, x: f32, y: f32) {
        let (x, y) = self.at(x, y);
        self.sink.move_to(x, y);
    }

    fn line_to(&mut self, x: f32, y: f32) {
        let (x, y) = self.at(x, y);
        self.sink.line_to(x, y);
    }

    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        let (cx, cy) = self.at(cx0, cy0);
        let (x, y) = self.at(x, y);
        self.sink.quad_to(cx, cy, x, y);
    }

    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        let (c1x, c1y) = self.at(cx0, cy0);
        let (c2x, c2y) = self.at(cx1, cy1);
        let (x, y) = self.at(x, y);
        self.sink.cubic_to(c1x, c1y, c2x, c2y, x, y);
    }

    fn close(&mut self) {
        self.sink.close();
    }
}

/// SVG path data.
struct SvgPath(String);

impl SvgPath {
    fn point(&mut self, x: f64, y: f64) {
        let _ = write!(self.0, "{} {}", fmt_num(x), fmt_num(y));
    }
}

impl PathSink for SvgPath {
    fn move_to(&mut self, x: f64, y: f64) {
        self.0.push('M');
        self.point(x, y);
    }

    fn line_to(&mut self, x: f64, y: f64) {
        self.0.push('L');
        self.point(x, y);
    }

    fn quad_to(&mut self, cx: f64, cy: f64, x: f64, y: f64) {
        self.0.push('Q');
        self.point(cx, cy);
        self.0.push(' ');
        self.point(x, y);
    }

    fn cubic_to(&mut self, c1x: f64, c1y: f64, c2x: f64, c2y: f64, x: f64, y: f64) {
        self.0.push('C');
        self.point(c1x, c1y);
        self.0.push(' ');
        self.point(c2x, c2y);
        self.0.push(' ');
        self.point(x, y);
    }

    fn close(&mut self) {
        self.0.push('Z');
    }
}

/// A number for SVG: at most 2 decimals, without trailing zeros.
pub fn fmt_num(v: f64) -> String {
    let s = format!("{v:.2}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" {
        "0".to_owned()
    } else {
        s.to_owned()
    }
}

/// Ascent, descent and cap height of the label font at `size`.
pub fn font_metrics(size: f64) -> FontMetrics {
    font_metrics_in(TextFont::default(), size)
}

/// [`font_metrics`] of any of the bundled faces.
pub fn font_metrics_in(font: TextFont, size: f64) -> FontMetrics {
    let face = face(font);
    let m = &face.metrics;
    let k = units_to(face, size);
    // Every version of Noto has a cap height; 0.7 em is the usual fallback.
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
    fn text_path_draws_every_glyph() {
        assert_eq!(text_path("", 12.0, 0.0, 0.0), "");
        // "I" is one closed outline; "S1" is two.
        let i = text_path("I", 12.0, 10.0, 20.0);
        assert!(i.starts_with('M') && i.ends_with('Z'));
        assert_eq!(i.matches('M').count(), 1);
        assert_eq!(text_path("S1", 12.0, 0.0, 0.0).matches('Z').count(), 2);
        // A space has no outline.
        assert_eq!(text_path(" ", 12.0, 0.0, 0.0), "");
    }

    #[test]
    fn text_path_sits_on_the_baseline_within_its_width() {
        // Collect every coordinate pair of an "H" and check it fills its box:
        // from the left side bearing to the advance, from the baseline up to
        // the cap height.
        let size = 100.0;
        let path = text_path("H", size, 50.0, 200.0);
        let nums: Vec<f64> = path
            .split(|c: char| c.is_ascii_alphabetic() || c == ' ')
            .filter(|t| !t.is_empty())
            .map(|t| t.parse().unwrap())
            .collect();
        let xs: Vec<f64> = nums.iter().step_by(2).copied().collect();
        let ys: Vec<f64> = nums.iter().skip(1).step_by(2).copied().collect();
        let (min_x, max_x) = (
            xs.iter().copied().fold(f64::MAX, f64::min),
            xs.iter().copied().fold(f64::MIN, f64::max),
        );
        let (min_y, max_y) = (
            ys.iter().copied().fold(f64::MAX, f64::min),
            ys.iter().copied().fold(f64::MIN, f64::max),
        );
        assert!(min_x > 50.0 && max_x < 50.0 + text_width("H", size));
        assert!(
            (max_y - 200.0).abs() < 0.5,
            "bottom of H on the baseline: {max_y}"
        );
        let cap = font_metrics(size).cap_height;
        assert!(
            (200.0 - min_y - cap).abs() < 0.5,
            "top of H at the cap height: {min_y}"
        );
    }

    #[test]
    fn numbers_for_svg() {
        assert_eq!(fmt_num(1.0), "1");
        assert_eq!(fmt_num(1.5), "1.5");
        assert_eq!(fmt_num(1.256), "1.26");
        assert_eq!(fmt_num(-0.001), "0");
        assert_eq!(fmt_num(-2.5), "-2.5");
    }

    #[test]
    fn every_face_loads_and_differs() {
        let plain = text_width_in(TextFont::default(), "Glucose", 100.0);
        let mut widths = Vec::new();
        for font in all_fonts() {
            let w = text_width_in(font, "Glucose", 100.0);
            assert!(w > 0.0, "{}", font_key(font));
            assert!(font_metrics_in(font, 12.0).cap_height > 0.0);
            assert!(!text_path_in(font, "G", 12.0, 0.0, 0.0).is_empty());
            widths.push(w);
        }
        // The first face is the label font.
        assert_eq!(widths[0], plain);
        // Bold is wider than regular, and serif differs from sans.
        assert!(widths[1] > widths[0]);
        assert_ne!(widths[4], widths[0]);
        // Each key is different.
        let mut keys: Vec<&str> = all_fonts().into_iter().map(font_key).collect();
        keys.dedup();
        assert_eq!(keys.len(), 8);
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
