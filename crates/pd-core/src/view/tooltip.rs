//! The hover tooltip (S §5.9, §6.6).
//!
//! The app decides when to show it (after the pointer has been still for
//! 600 ms, S §17.1), because `pd-core` never reads a clock. This module says
//! what it shows and draws it.

use super::DiagramView;
use crate::annotation::{notes_text, qualifier_text, resource_text, sbo_id, sbo_name};
use crate::geometry::Pt;
use crate::model::Annotation;
use crate::render::constants::{TOOLTIP_BORDER, TOOLTIP_FILL, TOOLTIP_TEXT};
use crate::render::{Renderer, SRect, Stroke, spt};
use crate::text::{font_metrics, text_width};
use crate::view::Target;

/// Tooltip text size, in screen points (S §5.9).
const TEXT_SIZE: f64 = 11.0;
/// Offset of the box from the cursor, in screen points.
const OFFSET: f64 = 14.0;
const PADDING_X: f64 = 6.0;
const PADDING_Y: f64 = 4.0;
const CORNER_RADIUS: f32 = 3.0;
/// The space between two lines, as a multiple of the text's height.
const LINE_SPACING: f64 = 1.25;
/// How many of a qualifier's references are listed before "+ n more".
const MAX_REFERENCES: usize = 4;
/// How many letters of the notes' first line are shown.
const NOTES_LETTERS: usize = 60;

impl DiagramView {
    /// What the tooltip says for `target`: `id: initialValue` for a species
    /// (with up to 4 decimals; an alias shows its primary), `id: kineticLaw`
    /// for a reaction, or just the id if it has no law; `id: volume v` for a
    /// compartment. Below it, the item's display name and annotation, if any
    /// (docs/annotation-marks-design.md M1, M2), a line each. Nothing for
    /// empty canvas or a null node.
    pub fn tooltip_text(&self, target: &Target) -> Option<String> {
        match target {
            Target::Species(id) | Target::SpeciesLabel(id) => {
                let node = self.model.find_species(id)?;
                if node.is_null_node {
                    return None;
                }
                let primary = self.model.find_species(node.primary_id()).unwrap_or(node);
                let first = format!("{}: {}", primary.id, format_value(primary.initial_value));
                Some(with_annotation(
                    first,
                    primary.display_name.as_deref(),
                    &primary.annotation,
                ))
            }
            Target::Handle(h) => self.tooltip_text(&Target::Leg(h.reaction.clone())),
            Target::Junction(id)
            | Target::Leg(id)
            | Target::ReactionLabel(id)
            | Target::StemEnd(id, _)
            | Target::AddLine(id)
            | Target::ModifierLine(id, ..)
            | Target::CornerPoint(id, ..) => {
                let r = self.model.find_reaction(id)?;
                let first = if r.kinetic_law.trim().is_empty() {
                    r.id.clone()
                } else {
                    format!("{}: {}", r.id, r.kinetic_law)
                };
                Some(with_annotation(
                    first,
                    r.display_name.as_deref(),
                    &r.annotation,
                ))
            }
            Target::SpeciesCorner(..) => None,
            Target::Compartment(id) | Target::CompartmentCorner(id, _) => {
                let c = self.model.find_compartment(id)?;
                let first = format!("{}: volume {}", c.id, format_value(c.size));
                Some(with_annotation(
                    first,
                    c.display_name.as_deref(),
                    &c.annotation,
                ))
            }
            Target::Text(_) | Target::TextWrap(_) | Target::Empty => None,
        }
    }

    /// Draws a tooltip with `text` near `cursor` (screen points), moved back
    /// inside `area` if it would stick out (S §5.9). Each line of `text` a
    /// line of the tooltip, from its left.
    pub fn draw_tooltip(&self, r: &mut dyn Renderer, text: &str, cursor: Pt, area: SRect) {
        let lines: Vec<&str> = text.lines().collect();
        let widths: Vec<f64> = lines.iter().map(|l| text_width(l, TEXT_SIZE)).collect();
        let w = widths.iter().copied().fold(0.0, f64::max) + 2.0 * PADDING_X;
        let m = font_metrics(TEXT_SIZE);
        let line = m.ascent - m.descent;
        let step = line * LINE_SPACING;
        let n = lines.len().max(1) as f64;
        let h = line + (n - 1.0) * step + 2.0 * PADDING_Y;
        let (min_x, min_y) = (f64::from(area.min.x), f64::from(area.min.y));
        let (max_x, max_y) = (f64::from(area.max.x), f64::from(area.max.y));
        let mut x = cursor.x + OFFSET;
        let mut y = cursor.y + OFFSET;
        // Keep it inside: flip to the other side of the cursor when there's no room.
        if x + w > max_x {
            x = (cursor.x - OFFSET - w).max(min_x);
        }
        if y + h > max_y {
            y = (cursor.y - OFFSET - h).max(min_y);
        }
        let rect = SRect {
            min: spt(x as f32, y as f32),
            max: spt((x + w) as f32, (y + h) as f32),
        };
        r.round_rect(
            rect,
            CORNER_RADIUS,
            Some(TOOLTIP_FILL),
            Some(Stroke::solid(1.0, TOOLTIP_BORDER)),
        );
        // Text is drawn about its centre: each line's centre put so that it
        // starts at the left padding.
        for (i, (l, lw)) in lines.iter().zip(&widths).enumerate() {
            let centre = spt(
                (x + PADDING_X + lw / 2.0) as f32,
                (y + PADDING_Y + line / 2.0 + i as f64 * step) as f32,
            );
            r.text(
                centre,
                l,
                TEXT_SIZE as f32,
                crate::model::TextFont::default(),
                TOOLTIP_TEXT,
            );
        }
    }
}

/// The tooltip's first line, then the item's display name and annotation:
/// its SBO term, its references a line per qualifier (`is CHEBI:4170 ·
/// kegg.compound:C00092`, the fifth on as "+ n more"), and its notes' first
/// line, cut short.
fn with_annotation(first: String, display_name: Option<&str>, a: &Annotation) -> String {
    let mut lines = vec![first];
    if let Some(name) = display_name.map(str::trim).filter(|n| !n.is_empty()) {
        lines.push(name.to_owned());
    }
    if let Some(n) = a.sbo {
        lines.push(match sbo_name(n) {
            Some(name) => format!("{} {name}", sbo_id(n)),
            None => sbo_id(n),
        });
    }
    let mut qualifiers: Vec<&str> = Vec::new();
    for r in &a.references {
        if !qualifiers.contains(&r.qualifier.as_str()) {
            qualifiers.push(&r.qualifier);
        }
    }
    for q in qualifiers {
        let all: Vec<String> = a
            .references
            .iter()
            .filter(|r| r.qualifier == q)
            .map(|r| resource_text(&r.resource))
            .collect();
        let mut shown = all[..all.len().min(MAX_REFERENCES)].join(" · ");
        if all.len() > MAX_REFERENCES {
            shown.push_str(&format!(" + {} more", all.len() - MAX_REFERENCES));
        }
        lines.push(format!("{} {shown}", qualifier_text(q)));
    }
    if let Some(notes) = &a.notes {
        let text = notes_text(notes);
        if let Some(l) = text.lines().map(str::trim).find(|l| !l.is_empty()) {
            let mut cut: String = l.chars().take(NOTES_LETTERS).collect();
            if l.chars().count() > NOTES_LETTERS {
                cut.push('…');
            }
            lines.push(format!("Notes: {cut}"));
        }
    }
    lines.join("\n")
}

/// A number with at most 4 decimals and no trailing zeros: 1 → "1",
/// 0.25 → "0.25", 1/3 → "0.3333".
fn format_value(v: f64) -> String {
    let s = format!("{v:.4}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" {
        "0".to_owned()
    } else {
        s.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::pt;
    use crate::model::{Model, Participant, Rgba};
    use crate::render::SPt;

    fn view() -> DiagramView {
        let mut m = Model::new();
        let a = m.add_species(pt(0.0, 0.0), 80.0, 36.0);
        m.find_species_mut(&a).unwrap().initial_value = 1.0 / 3.0;
        let b = m.add_species(pt(400.0, 0.0), 80.0, 36.0);
        let r = m.add_reaction(pt(200.0, 0.0));
        let rr = m.find_reaction_mut(&r).unwrap();
        rr.reactants.push(Participant::new(&a, 1.0));
        rr.products.push(Participant::new(&b, 1.0));
        rr.kinetic_law = "k3*S1".into();
        m.add_alias(&a, pt(0.0, 100.0));
        DiagramView::new(m)
    }

    #[test]
    fn texts() {
        let v = view();
        let t = |target: Target| v.tooltip_text(&target);
        assert_eq!(
            t(Target::Species("S1".into())).as_deref(),
            Some("S1: 0.3333")
        );
        assert_eq!(t(Target::Species("S2".into())).as_deref(), Some("S2: 0"));
        // An alias shows its primary.
        assert_eq!(
            t(Target::Species("S4".into())).as_deref(),
            Some("S1: 0.3333")
        );
        assert_eq!(
            t(Target::Junction("R3".into())).as_deref(),
            Some("R3: k3*S1")
        );
        assert_eq!(t(Target::Leg("R3".into())).as_deref(), Some("R3: k3*S1"));
        assert_eq!(t(Target::Empty), None);
        assert_eq!(t(Target::Species("nope".into())), None);
    }

    /// M1, M2: the display name and annotation below, for species (an alias
    /// its primary's), reactions and compartments.
    #[test]
    fn annotations_are_shown_below() {
        use crate::model::Reference;
        let mut v = view();
        let mut m = v.model().clone();
        let r = |q: &str, x: &str| Reference {
            qualifier: q.into(),
            resource: x.into(),
        };
        let s1 = m.find_species_mut("S1").unwrap();
        s1.display_name = Some("D-Glucose 6-phosphate".into());
        s1.annotation.sbo = Some(247);
        s1.annotation.references = vec![
            r("bqbiol:is", "https://identifiers.org/CHEBI:4170"),
            r("bqbiol:isVersionOf", "https://identifiers.org/CHEBI:17"),
            r("bqbiol:is", "urn:miriam:kegg.compound:C00092"),
        ];
        s1.annotation.notes = Some(format!(
            "<body xmlns=\"http://www.w3.org/1999/xhtml\"><p>{}</p><p>Second.</p></body>",
            "a".repeat(70)
        ));
        let rr = m.find_reaction_mut("R3").unwrap();
        rr.annotation.references = (1..=6)
            .map(|i| {
                r(
                    "bqbiol:isVersionOf",
                    &format!("https://identifiers.org/ec-code:1.1.1.{i}"),
                )
            })
            .collect();
        let c = m.add_compartment(crate::model::Frame::from_corners(
            pt(-500.0, -500.0),
            pt(-300.0, -300.0),
        ));
        m.find_compartment_mut(&c).unwrap().annotation.sbo = Some(9_999_999);
        v.set_model(m);
        let t = |target: Target| v.tooltip_text(&target).unwrap();
        let s1 = format!(
            "S1: 0.3333\nD-Glucose 6-phosphate\nSBO:0000247 simple chemical\n\
             is CHEBI:4170 · kegg.compound:C00092\nisVersionOf CHEBI:17\nNotes: {}…",
            "a".repeat(60)
        );
        assert_eq!(t(Target::Species("S1".into())), s1);
        assert_eq!(
            t(Target::Species("S4".into())),
            s1,
            "an alias shows its primary"
        );
        assert_eq!(
            t(Target::Leg("R3".into())),
            "R3: k3*S1\nisVersionOf ec-code:1.1.1.1 · ec-code:1.1.1.2 · ec-code:1.1.1.3 · \
             ec-code:1.1.1.4 + 2 more"
        );
        assert!(t(Target::Compartment(c)).ends_with("\nSBO:9999999"));
        // Nothing to add: one line, as before.
        assert_eq!(t(Target::Species("S2".into())), "S2: 0");
    }

    #[test]
    fn reaction_without_a_law_shows_its_id() {
        let mut v = view();
        let mut m = v.model().clone();
        m.find_reaction_mut("R3").unwrap().kinetic_law.clear();
        v.set_model(m);
        assert_eq!(
            v.tooltip_text(&Target::Leg("R3".into())).as_deref(),
            Some("R3")
        );
    }

    #[test]
    fn values() {
        assert_eq!(format_value(1.0), "1");
        assert_eq!(format_value(0.25), "0.25");
        assert_eq!(format_value(2.0 / 3.0), "0.6667");
        assert_eq!(format_value(-0.00001), "0");
        assert_eq!(format_value(1234.5), "1234.5");
    }

    #[derive(Default)]
    struct Rec {
        rect: Option<SRect>,
        texts: Vec<(SPt, String, f32, Rgba)>,
    }

    impl Renderer for Rec {
        fn round_rect(&mut self, r: SRect, _: f32, fill: Option<Rgba>, _: Option<Stroke>) {
            assert_eq!(fill, Some(TOOLTIP_FILL));
            self.rect = Some(r);
        }
        fn circle(&mut self, _: SPt, _: f32, _: Option<Rgba>, _: Option<Stroke>) {}
        fn polyline(&mut self, _: &[SPt], _: Stroke) {}
        fn cubic(&mut self, _: SPt, _: SPt, _: SPt, _: SPt, _: Stroke) {}
        fn polygon(&mut self, _: &[SPt], _: Rgba) {}
        fn text(&mut self, at: SPt, s: &str, size: f32, _: crate::model::TextFont, color: Rgba) {
            self.texts.push((at, s.to_owned(), size, color));
        }
    }

    #[test]
    fn drawn_below_right_of_the_cursor_and_kept_inside() {
        let v = view();
        let area = SRect {
            min: spt(0.0, 0.0),
            max: spt(400.0, 300.0),
        };
        let mut rec = Rec::default();
        v.draw_tooltip(&mut rec, "R3: k3*S1", pt(100.0, 100.0), area);
        let rect = rec.rect.unwrap();
        assert_eq!(rect.min, spt(114.0, 114.0));
        assert_eq!(rec.texts.len(), 1);
        assert_eq!(
            (rec.texts[0].1.as_str(), rec.texts[0].2, rec.texts[0].3),
            ("R3: k3*S1", 11.0, TOOLTIP_TEXT)
        );

        // Several lines: one below the other, each from the left, the box
        // as wide as the longest and tall enough for all.
        let mut rec = Rec::default();
        v.draw_tooltip(
            &mut rec,
            "S1: 1\nD-Glucose 6-phosphate",
            pt(100.0, 100.0),
            area,
        );
        let rect = rec.rect.unwrap();
        let (a, b) = (&rec.texts[0], &rec.texts[1]);
        assert_eq!(
            (a.1.as_str(), b.1.as_str()),
            ("S1: 1", "D-Glucose 6-phosphate")
        );
        assert!(b.0.y > a.0.y + 10.0, "below: {a:?} {b:?}");
        let left =
            |t: &(SPt, String, f32, Rgba)| f64::from(t.0.x) - text_width(&t.1, TEXT_SIZE) / 2.0;
        assert!((left(a) - left(b)).abs() < 0.01, "both from the left");
        assert!((left(a) - (f64::from(rect.min.x) + PADDING_X)).abs() < 0.01);
        let wide = text_width("D-Glucose 6-phosphate", TEXT_SIZE) + 2.0 * PADDING_X;
        assert!((f64::from(rect.width()) - wide).abs() < 0.01);
        assert!(f64::from(b.0.y) < f64::from(rect.max.y) - PADDING_Y);

        // Near the bottom-right corner it flips to stay inside.
        let mut rec = Rec::default();
        v.draw_tooltip(&mut rec, "R3: k3*S1", pt(390.0, 295.0), area);
        let rect = rec.rect.unwrap();
        assert!(rect.max.x <= 400.0 && rect.max.y <= 300.0);
        assert!(rect.min.x >= 0.0 && rect.min.y >= 0.0);
    }
}
