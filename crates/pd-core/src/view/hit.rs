//! Hit testing (S §6.2).
//!
//! Candidates are tried in order and the first hit wins: junction, then the
//! topmost species, then a reaction leg. Tolerances are in screen points, so
//! they are divided by the zoom to compare in world space. Bézier handles
//! (first in the order) and Bézier legs arrive with R5.

use crate::geometry::{Pt, point_in_rect, point_to_segment_dist};
use crate::model::{Model, Reaction, ReactionMode, SpeciesNode};
use crate::render::constants::{JUNCTION_HIT_TOLERANCE_PX, LEG_HIT_TOLERANCE_PX};
use crate::render::draw::{effective_junction, effective_mode, leg_end, leg_start};
use crate::view::input::Target;

/// What is at world point `p`, at `zoom`. Hidden null nodes, and legs that
/// touch them, can't be hit.
pub fn hit_test(model: &Model, p: Pt, zoom: f64, show_null_species: bool) -> Target {
    let visible = |s: &&SpeciesNode| show_null_species || !s.is_null_node;

    let junction_tol = JUNCTION_HIT_TOLERANCE_PX / zoom;
    for r in model.reactions().iter().rev() {
        if effective_junction(model, r).distance(p) <= junction_tol {
            return Target::Junction(r.id.clone());
        }
    }

    if let Some(s) = model
        .species()
        .iter()
        .rev()
        .filter(visible)
        .find(|s| point_in_rect(p, s.center, s.width / 2.0, s.height / 2.0))
    {
        return Target::Species(s.id.clone());
    }

    let leg_tol = LEG_HIT_TOLERANCE_PX / zoom;
    for r in model.reactions().iter().rev() {
        let near = leg_segments(model, r, show_null_species)
            .any(|(a, b)| point_to_segment_dist(p, a, b) <= leg_tol);
        if near {
            return Target::Leg(r.id.clone());
        }
    }
    Target::Empty
}

/// The straight segments a reaction is drawn with, as in the drawing pass.
fn leg_segments<'a>(
    model: &'a Model,
    r: &'a Reaction,
    show_null_species: bool,
) -> impl Iterator<Item = (Pt, Pt)> + 'a {
    let node = move |id: &str| {
        model
            .find_species(id)
            .filter(|s| show_null_species || !s.is_null_node)
    };
    let mut segments = Vec::new();
    if effective_mode(r) == ReactionMode::Linear {
        if let (Some(a), Some(b)) = (node(&r.reactants[0].species), node(&r.products[0].species)) {
            segments.push((leg_start(a, b.center), leg_end(b, a.center).0));
        }
    } else {
        let j = r.junction;
        for p in &r.reactants {
            if let Some(n) = node(&p.species) {
                segments.push((leg_start(n, j), j));
            }
        }
        for p in &r.products {
            if let Some(n) = node(&p.species) {
                segments.push((j, leg_end(n, j).0));
            }
        }
    }
    segments.into_iter()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::pt;
    use crate::model::Participant;

    /// A at (0,0) → R3 junction at (200,0) → B at (400,0).
    fn model() -> Model {
        let mut m = Model::new();
        let a = m.add_species(pt(0.0, 0.0), 80.0, 36.0);
        let b = m.add_species(pt(400.0, 0.0), 80.0, 36.0);
        let r = m.add_reaction(pt(200.0, 0.0));
        let r = m.find_reaction_mut(&r).unwrap();
        r.reactants.push(Participant::new(a, 1.0));
        r.products.push(Participant::new(b, 1.0));
        m
    }

    #[test]
    fn hit_order() {
        let m = model();
        let hit = |x: f64, y: f64| hit_test(&m, pt(x, y), 1.0, true);
        assert_eq!(hit(203.0, 4.0), Target::Junction("R3".into()));
        assert_eq!(hit(10.0, 10.0), Target::Species("S1".into()));
        assert_eq!(hit(100.0, 5.0), Target::Leg("R3".into()));
        assert_eq!(hit(300.0, -5.5), Target::Leg("R3".into()));
        assert_eq!(hit(100.0, 7.0), Target::Empty);
        assert_eq!(hit(200.0, 100.0), Target::Empty);
    }

    #[test]
    fn junction_beats_a_species_under_it() {
        let mut m = model();
        m.find_reaction_mut("R3").unwrap().junction = pt(10.0, 0.0);
        assert_eq!(
            hit_test(&m, pt(12.0, 0.0), 1.0, true),
            Target::Junction("R3".into())
        );
    }

    #[test]
    fn topmost_species_wins() {
        let mut m = model();
        let top = m.add_species(pt(20.0, 0.0), 80.0, 36.0);
        assert_eq!(hit_test(&m, pt(10.0, 0.0), 1.0, true), Target::Species(top));
    }

    #[test]
    fn tolerances_are_in_screen_points() {
        let m = model();
        // 7 world units off the leg is 14 screen points at zoom 2: a miss;
        // at zoom 0.5 it is 3.5 points: a hit.
        assert_eq!(hit_test(&m, pt(100.0, 7.0), 2.0, true), Target::Empty);
        assert_eq!(
            hit_test(&m, pt(100.0, 7.0), 0.5, true),
            Target::Leg("R3".into())
        );
        // Junction: 12 world units away is 6 points at zoom 0.5.
        assert_eq!(
            hit_test(&m, pt(212.0, 0.0), 0.5, true),
            Target::Junction("R3".into())
        );
    }

    #[test]
    fn hidden_null_nodes_and_their_legs_cant_be_hit() {
        let mut m = model();
        m.find_species_mut("S2").unwrap().is_null_node = true;
        assert_eq!(
            hit_test(&m, pt(400.0, 0.0), 1.0, true),
            Target::Species("S2".into())
        );
        assert_eq!(hit_test(&m, pt(400.0, 0.0), 1.0, false), Target::Empty);
        assert_eq!(hit_test(&m, pt(300.0, 0.0), 1.0, false), Target::Empty);
        assert_eq!(
            hit_test(&m, pt(100.0, 0.0), 1.0, false),
            Target::Leg("R3".into())
        );
    }

    #[test]
    fn linear_reaction_is_hit_on_its_drawn_line_and_effective_junction() {
        let mut m = model();
        let r = m.find_reaction_mut("R3").unwrap();
        r.mode = ReactionMode::Linear;
        r.junction = pt(200.0, 80.0);
        // The effective junction is on the line, at (200, 0).
        assert_eq!(
            hit_test(&m, pt(200.0, 3.0), 1.0, true),
            Target::Junction("R3".into())
        );
        assert_eq!(hit_test(&m, pt(200.0, 80.0), 1.0, true), Target::Empty);
        assert_eq!(
            hit_test(&m, pt(300.0, 2.0), 1.0, true),
            Target::Leg("R3".into())
        );
    }
}
