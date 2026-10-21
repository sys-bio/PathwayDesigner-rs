//! Hit testing (S §6.2).
//!
//! Candidates are tried in order and the first hit wins: a handle of a
//! selected Bézier reaction, the wrap handle of a selected text, a corner of a
//! selected compartment, a corner of a selected species, the topmost text, a junction, the
//! topmost name drawn outside its node, the topmost species, a
//! reaction leg, then a compartment's border or name (innermost first). Text
//! is drawn above everything, so it is hit first; compartments are drawn
//! below everything, and only their edge and name select them, so a click
//! inside one still reaches what is in it, or starts a rubber band. Tolerances are in screen points, so they are divided by the zoom to
//! compare in world space.

use crate::bezier::{Ctrl, leg_curve, leg_inner_end, leg_run};
use crate::geometry::{Pt, point_in_rect, point_to_segment_dist};
use crate::model::{Model, Participant, Reaction, ReactionMode, Role, SpeciesNode};
use crate::render::constants::{
    BEZIER_HIT_SAMPLES, HANDLE_HIT_TOLERANCE_PX, JUNCTION_HIT_TOLERANCE_PX, LEG_HIT_TOLERANCE_PX,
};
use crate::render::draw::{
    compartment_corners, compartment_label_box, effective_junction, effective_mode, leg_end,
    leg_start, outside_label_box, reaction_label_box, species_corners, wrap_handle,
};
use crate::textbox::text_box;
use crate::view::input::{HandleId, Target};

/// What is at world point `p`, at `zoom`. Hidden null nodes, and legs that
/// touch them, can't be hit.
pub fn hit_test(model: &Model, p: Pt, zoom: f64, show_null_species: bool) -> Target {
    hit_test_with(model, p, zoom, show_null_species, true)
}

/// [`hit_test`], with modifier arcs and their corner points hidden (and so
/// not hit) unless `show_modifiers` (S §7.6).
pub fn hit_test_with(
    model: &Model,
    p: Pt,
    zoom: f64,
    show_null_species: bool,
    show_modifiers: bool,
) -> Target {
    let visible = |s: &&SpeciesNode| show_null_species || !s.is_null_node;

    let handle_tol = HANDLE_HIT_TOLERANCE_PX / zoom;
    for r in model
        .selected_reactions()
        .filter(|r| effective_mode(r) == ReactionMode::Bezier)
    {
        for (role, list) in [(Role::Reactant, &r.reactants), (Role::Product, &r.products)] {
            for (index, part) in list.iter().enumerate() {
                let shown = model
                    .find_species(&part.species)
                    .is_some_and(|s| show_null_species || !s.is_null_node);
                let Some(c) = leg_curve(model, r, role, index).filter(|_| shown) else {
                    continue;
                };
                for (ctrl, at) in [(Ctrl::One, c.p1), (Ctrl::Two, c.p2)] {
                    if at.distance(p) <= handle_tol {
                        return Target::Handle(HandleId {
                            reaction: r.id.clone(),
                            role,
                            index,
                            ctrl,
                        });
                    }
                }
            }
        }
    }

    // The + handle by a selected reaction's junction (S §7.11).
    let plus_tol = (crate::render::draw::ADD_LINE_RADIUS_PX + 2.0) / zoom;
    for r in model.selected_reactions() {
        if crate::render::draw::add_line_handle(model, r, zoom)
            .is_some_and(|at| at.distance(p) <= plus_tol)
        {
            return Target::AddLine(r.id.clone());
        }
    }

    // The corner points of a selected reaction's modifier arcs (S §5.7.1).
    if show_modifiers {
        for r in model.selected_reactions() {
            for (mi, m) in r.modifiers.iter().enumerate() {
                if let Some(pi) = m.points.iter().position(|q| q.distance(p) <= handle_tol) {
                    return Target::CornerPoint(r.id.clone(), mi, pi);
                }
            }
        }
    }

    // The ends of a selected reaction's stem (S §5.13).
    for r in model.selected_reactions() {
        if let Some([a, _, b]) = crate::render::draw::stem_line(r) {
            for (role, at) in [(Role::Reactant, a), (Role::Product, b)] {
                if at.distance(p) <= handle_tol {
                    return Target::StemEnd(r.id.clone(), role);
                }
            }
        }
    }

    for t in model.selected_texts() {
        if wrap_handle(t).distance(p) <= handle_tol {
            return Target::TextWrap(t.id.clone());
        }
    }
    for c in model.selected_compartments() {
        let Some(f) = c.drawn_frame() else { continue };
        if let Some(i) = compartment_corners(&f)
            .iter()
            .position(|q| q.distance(p) <= handle_tol)
        {
            return Target::CompartmentCorner(c.id.clone(), i);
        }
    }
    for s in model
        .species()
        .iter()
        .filter(|s| s.selected && !s.is_null_node)
    {
        if let Some(i) = species_corners(s)
            .iter()
            .position(|q| q.distance(p) <= handle_tol)
        {
            return Target::SpeciesCorner(s.id.clone(), i);
        }
    }
    let in_box = |t: &&crate::model::TextItem| {
        let (a, b) = text_box(t);
        (a.x..=b.x).contains(&p.x) && (a.y..=b.y).contains(&p.y)
    };
    if let Some(t) = model.texts().iter().rev().find(in_box) {
        return Target::Text(t.id.clone());
    }

    let junction_tol = JUNCTION_HIT_TOLERANCE_PX / zoom;
    for r in model.reactions().iter().rev() {
        if effective_junction(model, r).distance(p) <= junction_tol {
            return Target::Junction(r.id.clone());
        }
    }

    let on_label = |s: &&SpeciesNode| {
        outside_label_box(s)
            .is_some_and(|(a, b)| (a.x..=b.x).contains(&p.x) && (a.y..=b.y).contains(&p.y))
    };
    if let Some(s) = model.species().iter().rev().find(on_label) {
        return Target::SpeciesLabel(s.id.clone());
    }
    let on_name = |r: &&crate::model::Reaction| {
        reaction_label_box(model, r)
            .is_some_and(|(a, b)| (a.x..=b.x).contains(&p.x) && (a.y..=b.y).contains(&p.y))
    };
    if let Some(r) = model.reactions().iter().rev().find(on_name) {
        return Target::ReactionLabel(r.id.clone());
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
    // Modifier arcs, piece by piece (S §5.7.1).
    if show_modifiers {
        for r in model.reactions().iter().rev() {
            for (mi, m) in r.modifiers.iter().enumerate() {
                let Some(route) = crate::render::draw::modifier_route(model, r, m) else {
                    continue;
                };
                if let Some(piece) = route
                    .windows(2)
                    .position(|w| point_to_segment_dist(p, w[0], w[1]) <= leg_tol)
                {
                    return Target::ModifierLine(r.id.clone(), mi, piece);
                }
            }
        }
    }

    let mut framed: Vec<_> = model
        .compartments
        .iter()
        .filter_map(|c| c.drawn_frame().map(|f| (c, f)))
        .collect();
    framed.sort_by(|a, b| a.1.area().total_cmp(&b.1.area()));
    for (c, f) in framed {
        let (a, b) = (f.min(), f.max());
        let (la, lb) = compartment_label_box(c, &f);
        let on_label = (la.x..=lb.x).contains(&p.x) && (la.y..=lb.y).contains(&p.y);
        let dx = (a.x - p.x).max(p.x - b.x);
        let dy = (a.y - p.y).max(p.y - b.y);
        // Distance to the outline: inside, to the nearest side; outside, to the box.
        let to_edge = if dx <= 0.0 && dy <= 0.0 {
            -dx.max(dy)
        } else {
            dx.max(0.0).hypot(dy.max(0.0))
        };
        if on_label || to_edge <= leg_tol {
            return Target::Compartment(c.id.clone());
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
    // A leg's straight run through its corner points (S §5.6.5).
    let run = |segments: &mut Vec<(Pt, Pt)>, role: Role, p: &Participant| {
        segments.extend(leg_run(r, role, p).windows(2).map(|w| (w[0], w[1])));
    };
    if let Some([a, j, b]) = crate::render::draw::stem_line(r) {
        segments.push((a, j));
        segments.push((j, b));
    }
    if effective_mode(r) == ReactionMode::Bezier {
        // The curve sampled at 20 points; samples inside the node are ignored.
        for (role, list) in [(Role::Reactant, &r.reactants), (Role::Product, &r.products)] {
            for (index, part) in list.iter().enumerate() {
                let (Some(n), Some(c)) = (node(&part.species), leg_curve(model, r, role, index))
                else {
                    continue;
                };
                let outside = |q: &Pt| !point_in_rect(*q, n.center, n.width / 2.0, n.height / 2.0);
                let samples: Vec<Pt> = (0..BEZIER_HIT_SAMPLES)
                    .map(|i| c.eval(i as f64 / (BEZIER_HIT_SAMPLES - 1) as f64))
                    .collect();
                for w in samples.windows(2) {
                    if outside(&w[0]) && outside(&w[1]) {
                        segments.push((w[0], w[1]));
                    }
                }
                run(&mut segments, role, part);
            }
        }
    } else if effective_mode(r) == ReactionMode::Linear {
        if let (Some(a), Some(b)) = (node(&r.reactants[0].species), node(&r.products[0].species)) {
            segments.push((leg_start(a, b.center), leg_end(b, a.center).0));
        }
    } else {
        for p in &r.reactants {
            if let Some(n) = node(&p.species) {
                let j = leg_inner_end(r, Role::Reactant, p);
                segments.push((leg_start(n, j), j));
                run(&mut segments, Role::Reactant, p);
            }
        }
        for p in &r.products {
            if let Some(n) = node(&p.species) {
                let j = leg_inner_end(r, Role::Product, p);
                segments.push((j, leg_end(n, j).0));
                run(&mut segments, Role::Product, p);
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
    fn bezier_legs_are_hit_along_the_curve() {
        let mut m = model();
        let r = m.find_reaction_mut("R3").unwrap();
        r.mode = ReactionMode::Bezier;
        r.reactants[0].ctrl_pts_set = true;
        r.reactants[0].ctrl1 = pt(0.0, 200.0);
        r.reactants[0].ctrl2 = pt(200.0, 200.0);
        // The curve bows down: its middle (t = 0.5) is at (125, 150).
        assert_eq!(
            hit_test(&m, pt(125.0, 152.0), 1.0, true),
            Target::Leg("R3".into())
        );
        // The straight line it would have been is now empty canvas.
        assert_eq!(hit_test(&m, pt(100.0, 0.0), 1.0, true), Target::Empty);
    }

    #[test]
    fn handles_of_selected_bezier_reactions_come_first() {
        let mut m = model();
        let r = m.find_reaction_mut("R3").unwrap();
        r.mode = ReactionMode::Bezier;
        // Automatic handles of the reactant leg A (0,0) → J (200,0): 70 and 130.
        assert_eq!(
            hit_test(&m, pt(70.0, 3.0), 1.0, true),
            Target::Leg("R3".into()),
            "not selected"
        );
        m.find_reaction_mut("R3").unwrap().selected = true;
        let handle = |role, index, ctrl| {
            Target::Handle(HandleId {
                reaction: "R3".into(),
                role,
                index,
                ctrl,
            })
        };
        assert_eq!(
            hit_test(&m, pt(70.0, 3.0), 1.0, true),
            handle(Role::Reactant, 0, Ctrl::One)
        );
        assert_eq!(
            hit_test(&m, pt(130.0, 0.0), 1.0, true),
            handle(Role::Reactant, 0, Ctrl::Two)
        );
        assert_eq!(
            hit_test(&m, pt(330.0, 0.0), 1.0, true),
            handle(Role::Product, 0, Ctrl::Two)
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
