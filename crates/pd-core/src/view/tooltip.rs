//! The hover tooltip (S §5.9, §6.6).
//!
//! The app decides when to show it (after the pointer has been still for
//! 600 ms, S §17.1), because `pd-core` never reads a clock. This module says
//! what it shows and draws it.

use super::DiagramView;
use crate::geometry::Pt;
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

impl DiagramView {
    /// What the tooltip says for `target`: `id: initialValue` for a species
    /// (with up to 4 decimals; an alias shows its primary), `id: kineticLaw`
    /// for a reaction, or just the id if it has no law. Nothing for empty
    /// canvas or a null node.
    pub fn tooltip_text(&self, target: &Target) -> Option<String> {
        match target {
            Target::Species(id) => {
                let node = self.model.find_species(id)?;
                if node.is_null_node {
                    return None;
                }
                let primary = self.model.find_species(node.primary_id()).unwrap_or(node);
                Some(format!(
                    "{}: {}",
                    primary.id,
                    format_value(primary.initial_value)
                ))
            }
            Target::Handle(h) => self.tooltip_text(&Target::Leg(h.reaction.clone())),
            Target::Junction(id) | Target::Leg(id) => {
                let r = self.model.find_reaction(id)?;
                Some(if r.kinetic_law.trim().is_empty() {
                    r.id.clone()
                } else {
                    format!("{}: {}", r.id, r.kinetic_law)
                })
            }
            Target::Compartment(id) | Target::CompartmentCorner(id, _) => {
                let c = self.model.find_compartment(id)?;
                Some(format!("{}: volume {}", c.id, format_value(c.size)))
            }
            Target::Text(_) | Target::TextWrap(_) | Target::Empty => None,
        }
    }

    /// Draws a tooltip with `text` near `cursor` (screen points), moved back
    /// inside `area` if it would stick out (S §5.9).
    pub fn draw_tooltip(&self, r: &mut dyn Renderer, text: &str, cursor: Pt, area: SRect) {
        let w = text_width(text, TEXT_SIZE) + 2.0 * PADDING_X;
        let m = font_metrics(TEXT_SIZE);
        let h = (m.ascent - m.descent) + 2.0 * PADDING_Y;
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
        let centre = spt((x + w / 2.0) as f32, (y + h / 2.0) as f32);
        r.text(
            centre,
            text,
            TEXT_SIZE as f32,
            crate::model::TextFont::default(),
            TOOLTIP_TEXT,
        );
    }
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
        text: Option<(String, f32, Rgba)>,
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
        fn text(&mut self, _: SPt, s: &str, size: f32, _: crate::model::TextFont, color: Rgba) {
            self.text = Some((s.to_owned(), size, color));
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
        assert_eq!(rec.text, Some(("R3: k3*S1".to_owned(), 11.0, TOOLTIP_TEXT)));

        // Near the bottom-right corner it flips to stay inside.
        let mut rec = Rec::default();
        v.draw_tooltip(&mut rec, "R3: k3*S1", pt(390.0, 295.0), area);
        let rect = rec.rect.unwrap();
        assert!(rect.max.x <= 400.0 && rect.max.y <= 300.0);
        assert!(rect.min.x >= 0.0 && rect.min.y >= 0.0);
    }
}
