//! How a reaction is drawn, beyond what SBML Layout can say (SPEC-Rust §8.3).
//!
//! Layout records only geometry. A Linear reaction (one straight line, no
//! junction) and a Bézier reaction's smooth junction can't be told from the
//! curves, so the export adds a small note of PathwayDesigner's own to the
//! reaction glyph's annotation, and the import reads it back. Other tools
//! skip annotations they don't know, so the file stays valid everywhere.
//!
//! ```xml
//! <pd:reaction xmlns:pd="https://github.com/sys-bio/PathwayDesigner-rs/ns/sbml-layout/1"
//!              mode="linear"/>
//! ```
//!
//! Free text (S §7.8) is a text glyph with its words. SBML Layout has no
//! fonts, so its look goes in a note on the text glyph:
//!
//! ```xml
//! <pd:text xmlns:pd="…" size="14" typeface="serif" bold="true" italic="true"
//!          align="center" color="#CC0000FF" wrapWidth="120"/>
//! ```

use crate::json::colour::{from_hex, to_hex};
use crate::model::{Reaction, ReactionMode, Rgba, TextAlign, TextFont, TextItem, Typeface};

/// The namespace of PathwayDesigner's note.
pub const NS: &str = "https://github.com/sys-bio/PathwayDesigner-rs/ns/sbml-layout/1";

/// The note for a reaction, or `None` if its curves say everything: only
/// Linear reactions and smooth Bézier junctions need one.
pub fn note(r: &Reaction) -> Option<String> {
    let linear = r.mode == ReactionMode::Linear;
    let smooth = r.mode == ReactionMode::Bezier && r.junction_smooth;
    if !linear && !smooth {
        return None;
    }
    let mut attrs = String::new();
    if linear {
        attrs.push_str(" mode=\"linear\"");
    }
    if smooth {
        attrs.push_str(" smoothJunction=\"true\"");
    }
    Some(format!("\n<pd:reaction xmlns:pd=\"{NS}\"{attrs}/>\n"))
}

/// What a glyph's annotation says about its reaction: whether it is Linear,
/// and whether its junction is smooth. Anything else in the annotation, and
/// annotations that aren't well-formed, are ignored.
pub fn read(annotation: &str) -> (bool, bool) {
    // An annotation's content may hold several elements: wrap it to parse it.
    let wrapped = format!("<a>{annotation}</a>");
    let Ok(doc) = roxmltree::Document::parse(&wrapped) else {
        return (false, false);
    };
    let Some(note) = doc
        .descendants()
        .find(|n| n.tag_name().namespace() == Some(NS) && n.tag_name().name() == "reaction")
    else {
        return (false, false);
    };
    (
        note.attribute("mode") == Some("linear"),
        note.attribute("smoothJunction") == Some("true"),
    )
}

/// How a free text looks, as its note says.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TextLook {
    pub font: TextFont,
    pub align: TextAlign,
    pub size: f64,
    pub color: Rgba,
    pub wrap_width: f64,
}

/// The note for a free text's glyph: its size, and whatever isn't the default.
pub fn text_note(t: &TextItem) -> String {
    let mut attrs = format!(" size=\"{}\"", sbml_io::fmt_f64(t.size));
    if t.font.typeface == Typeface::Serif {
        attrs.push_str(" typeface=\"serif\"");
    }
    if t.font.bold {
        attrs.push_str(" bold=\"true\"");
    }
    if t.font.italic {
        attrs.push_str(" italic=\"true\"");
    }
    match t.align {
        TextAlign::Left => {}
        TextAlign::Center => attrs.push_str(" align=\"center\""),
        TextAlign::Right => attrs.push_str(" align=\"right\""),
    }
    if !t.color.is_default() {
        attrs.push_str(&format!(" color=\"{}\"", to_hex(t.color)));
    }
    if t.wrap_width > 0.0 {
        attrs.push_str(&format!(
            " wrapWidth=\"{}\"",
            sbml_io::fmt_f64(t.wrap_width)
        ));
    }
    format!("\n<pd:text xmlns:pd=\"{NS}\"{attrs}/>\n")
}

/// What a text glyph's note says, or `None` if it has none. Values that
/// don't read keep their defaults.
pub fn read_text(annotation: &str) -> Option<TextLook> {
    let wrapped = format!("<a>{annotation}</a>");
    let doc = roxmltree::Document::parse(&wrapped).ok()?;
    let note = doc
        .descendants()
        .find(|n| n.tag_name().namespace() == Some(NS) && n.tag_name().name() == "text")?;
    let number = |key: &str| note.attribute(key).and_then(|v| v.parse::<f64>().ok());
    let yes = |key: &str| note.attribute(key) == Some("true");
    Some(TextLook {
        font: TextFont {
            typeface: if note.attribute("typeface") == Some("serif") {
                Typeface::Serif
            } else {
                Typeface::Sans
            },
            bold: yes("bold"),
            italic: yes("italic"),
        },
        align: match note.attribute("align") {
            Some("center") => TextAlign::Center,
            Some("right") => TextAlign::Right,
            _ => TextAlign::Left,
        },
        size: number("size")
            .filter(|s| s.is_finite() && *s >= 1.0)
            .unwrap_or(crate::model::DEFAULT_TEXT_SIZE),
        color: note
            .attribute("color")
            .and_then(from_hex)
            .unwrap_or_default(),
        wrap_width: number("wrapWidth")
            .filter(|w| w.is_finite() && *w > 0.0)
            .unwrap_or(0.0),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Pt;

    #[test]
    fn a_text_note_round_trips() {
        let mut t = TextItem::new("T1", Pt::ZERO, "x");
        let plain = read_text(&text_note(&t)).unwrap();
        assert_eq!(plain.font, TextFont::default());
        assert_eq!((plain.size, plain.wrap_width), (t.size, 0.0));
        assert!(plain.color.is_default());
        t.font = TextFont {
            typeface: Typeface::Serif,
            bold: true,
            italic: true,
        };
        t.size = 18.5;
        t.align = TextAlign::Right;
        t.color = Rgba::rgb(0xCC, 0, 0);
        t.wrap_width = 120.0;
        let back = read_text(&text_note(&t)).unwrap();
        assert_eq!(
            back,
            TextLook {
                font: t.font,
                align: TextAlign::Right,
                size: 18.5,
                color: t.color,
                wrap_width: 120.0
            }
        );
        assert_eq!(read_text("<other/>"), None);
    }

    #[test]
    fn written_only_when_needed_and_read_back() {
        let mut r = Reaction::new("J", Pt::ZERO);
        assert_eq!(note(&r), None, "Straight: the curves say it");
        r.mode = ReactionMode::Bezier;
        assert_eq!(note(&r), None, "Bézier: the curves say it");
        r.junction_smooth = true;
        assert_eq!(read(&note(&r).unwrap()), (false, true));
        r.mode = ReactionMode::Linear;
        assert_eq!(read(&note(&r).unwrap()), (true, false));
    }

    #[test]
    fn other_annotations_are_ignored() {
        assert_eq!(read(""), (false, false));
        assert_eq!(
            read("<rdf:RDF xmlns:rdf=\"http://www.w3.org/1999/02/22-rdf-syntax-ns#\"/>"),
            (false, false)
        );
        assert_eq!(read("<not closed"), (false, false));
        // Next to another tool's note.
        let both =
            format!("<x:y xmlns:x=\"urn:x\"/><pd:reaction xmlns:pd=\"{NS}\" mode=\"linear\"/>");
        assert_eq!(read(&both), (true, false));
    }
}
