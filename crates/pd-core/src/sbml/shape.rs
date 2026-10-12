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

use crate::model::{Reaction, ReactionMode};

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Pt;

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
