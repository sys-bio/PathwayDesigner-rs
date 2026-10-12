//! Laying out free text (S §7.8): lines, their widths, and the text's box.
//!
//! Drawing, hit testing, alignment and output all use this one layout, so a
//! text is the same size everywhere. Measured from the bundled fonts, like
//! labels (SPEC-Rust §5.4).

use crate::geometry::{Pt, pt};
use crate::model::{TextAlign, TextItem};
use crate::text::{font_metrics_in, text_width_in};

/// Space around the text inside its box, for selecting and hit testing.
pub const TEXT_PADDING: f64 = 4.0;
/// The narrowest a text can be made to wrap.
pub const MIN_WRAP_WIDTH: f64 = 20.0;

/// One line of laid-out text.
#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    pub text: String,
    pub width: f64,
    /// Where the line starts, from the text's left edge: 0 for left-aligned
    /// text, further in for centred or right-aligned text.
    pub x: f64,
}

/// A text broken into lines, in world units, from its top-left corner.
#[derive(Clone, Debug, PartialEq)]
pub struct Layout {
    pub lines: Vec<Line>,
    /// From one baseline to the next: the font's ascent minus its descent.
    pub line_height: f64,
    /// From the top of a line to its baseline.
    pub ascent: f64,
    /// The text's width: its wrap width, or its widest line.
    pub width: f64,
    pub height: f64,
}

impl Layout {
    /// The baseline of line `i`, below the text's top.
    pub fn baseline(&self, i: usize) -> f64 {
        self.ascent + i as f64 * self.line_height
    }
}

/// Breaks `t` into lines: at each line break, and, when it has a wrap width,
/// between words so no line is wider than that. A word wider than the wrap
/// width gets a line of its own. Spaces are kept as typed.
pub fn lay_out(t: &TextItem) -> Layout {
    let m = font_metrics_in(t.font, t.size);
    let width_of = |s: &str| text_width_in(t.font, s, t.size);
    let mut lines = Vec::new();
    for paragraph in t.text.split('\n') {
        let paragraph = paragraph.strip_suffix('\r').unwrap_or(paragraph);
        if t.wrap_width <= 0.0 {
            lines.push(Line {
                text: paragraph.to_owned(),
                width: width_of(paragraph),
                x: 0.0,
            });
            continue;
        }
        let mut current: Option<String> = None;
        for word in paragraph.split(' ') {
            let candidate = match &current {
                Some(c) => format!("{c} {word}"),
                None => word.to_owned(),
            };
            match current {
                Some(c) if width_of(candidate.trim_end()) > t.wrap_width => {
                    lines.push(Line {
                        width: width_of(&c),
                        text: c,
                        x: 0.0,
                    });
                    current = Some(word.to_owned());
                }
                _ => current = Some(candidate),
            }
        }
        let last = current.unwrap_or_default();
        lines.push(Line {
            width: width_of(&last),
            text: last,
            x: 0.0,
        });
    }
    let line_height = m.ascent - m.descent;
    let widest = lines.iter().map(|l| l.width).fold(0.0, f64::max);
    let width = if t.wrap_width > 0.0 {
        t.wrap_width
    } else {
        widest
    };
    // Each line lines up in the text's width. A line wider than that (one
    // long word) starts at the left edge.
    for line in &mut lines {
        let room = (width - line.width).max(0.0);
        line.x = match t.align {
            TextAlign::Left => 0.0,
            TextAlign::Center => room / 2.0,
            TextAlign::Right => room,
        };
    }
    Layout {
        width,
        height: line_height * lines.len() as f64,
        lines,
        line_height,
        ascent: m.ascent,
    }
}

/// The text's box, with [`TEXT_PADDING`] all round: what selects it and what
/// alignment lines up. Never narrower than one em, so empty text can still be
/// clicked. Returns (min, max).
pub fn text_box(t: &TextItem) -> (Pt, Pt) {
    let l = lay_out(t);
    let w = l.width.max(t.size);
    let pad = pt(TEXT_PADDING, TEXT_PADDING);
    (t.position - pad, t.position + pt(w, l.height) + pad)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{TextFont, Typeface};
    use crate::text::text_width_in;

    fn item(text: &str, wrap: f64) -> TextItem {
        let mut t = TextItem::new("T1", pt(10.0, 20.0), text);
        t.wrap_width = wrap;
        t
    }

    fn texts(l: &Layout) -> Vec<&str> {
        l.lines.iter().map(|l| l.text.as_str()).collect()
    }

    #[test]
    fn line_breaks_make_lines() {
        let l = lay_out(&item("one\ntwo\r\n\nfour", 0.0));
        assert_eq!(texts(&l), ["one", "two", "", "four"]);
        assert!((l.height - 4.0 * l.line_height).abs() < 1e-9);
        let widest = l.lines.iter().map(|l| l.width).fold(0.0, f64::max);
        assert_eq!(l.width, widest);
    }

    #[test]
    fn wrapping_breaks_between_words() {
        let t = item("the quick brown fox jumps", 0.0);
        let w = text_width_in(t.font, "the quick", t.size).max(text_width_in(
            t.font,
            "brown fox",
            t.size,
        )) + 1.0;
        let l = lay_out(&item("the quick brown fox jumps", w));
        assert_eq!(texts(&l), ["the quick", "brown fox", "jumps"]);
        assert!(l.lines.iter().all(|line| line.width <= w));
        assert_eq!(l.width, w);
        // A word wider than the wrap width gets a line of its own.
        let l = lay_out(&item("a enormously b", 15.0));
        assert_eq!(texts(&l), ["a", "enormously", "b"]);
    }

    #[test]
    fn alignment_lines_up_each_line_in_the_width() {
        let mut t = item("a wide first line\nshort", 0.0);
        let left = lay_out(&t);
        assert!(left.lines.iter().all(|l| l.x == 0.0));
        let (w, short) = (left.width, left.lines[1].width);
        t.align = TextAlign::Center;
        let c = lay_out(&t);
        assert_eq!(c.lines[0].x, 0.0, "the widest line fills the width");
        assert!((c.lines[1].x - (w - short) / 2.0).abs() < 1e-9);
        t.align = TextAlign::Right;
        assert!((lay_out(&t).lines[1].x - (w - short)).abs() < 1e-9);
        // With a wrap width, lines line up in the wrap width.
        t.wrap_width = 500.0;
        let r = lay_out(&t);
        assert!((r.lines[1].x + r.lines[1].width - 500.0).abs() < 1e-9);
        assert_eq!(
            text_box(&t),
            {
                t.align = TextAlign::Left;
                text_box(&t)
            },
            "the box doesn't move"
        );
    }

    #[test]
    fn the_box_has_padding_and_room_for_empty_text() {
        let t = item("Hello world", 0.0);
        let (min, max) = text_box(&t);
        let l = lay_out(&t);
        assert_eq!(min, pt(10.0 - TEXT_PADDING, 20.0 - TEXT_PADDING));
        assert!((max.x - (10.0 + l.width + TEXT_PADDING)).abs() < 1e-9);
        let (min, max) = text_box(&item("", 0.0));
        assert!(max.x - min.x >= t.size);
        assert!(max.y - min.y > l.line_height);
    }

    #[test]
    fn the_face_changes_the_measure() {
        let mut t = item("Pathway", 0.0);
        let plain = lay_out(&t).width;
        t.font = TextFont {
            typeface: Typeface::Serif,
            bold: true,
            italic: false,
        };
        assert_ne!(lay_out(&t).width, plain);
        assert!(
            (lay_out(&t).baseline(1) - lay_out(&t).baseline(0) - lay_out(&t).line_height).abs()
                < 1e-9
        );
    }
}
