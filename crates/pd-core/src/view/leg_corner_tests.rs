//! Corner points on reactant and product lines (S §5.6.5): the end piece and
//! the run, hitting, smooth junctions and files.

use super::*;
use crate::bezier::{apply_smooth_handles, leg_curve, leg_inner_end, leg_run};
use crate::model::{Participant, Reaction, SpeciesNode};

/// A (0,0) and C (0,200) → R1 junction (200,0) → B (400,0); nodes 80 × 36.
/// A's line is routed down through (0,100) and (200,100) and up to the
/// junction.
fn model(mode: ReactionMode) -> Model {
    let mut m = Model::new();
    for (id, x, y) in [("A", 0.0, 0.0), ("C", 0.0, 200.0), ("B", 400.0, 0.0)] {
        m.push_species(SpeciesNode::new(id, pt(x, y))).unwrap();
    }
    let mut r = Reaction::new("R1", pt(200.0, 0.0));
    r.mode = mode;
    r.reactants = vec![Participant::new("A", 1.0), Participant::new("C", 1.0)];
    r.reactants[0].points = vec![pt(0.0, 100.0), pt(200.0, 100.0)];
    r.products = vec![Participant::new("B", 1.0)];
    m.push_reaction(r).unwrap();
    m
}

#[test]
fn the_end_piece_ends_at_the_first_corner_and_the_run_at_the_anchor() {
    let mut m = model(ReactionMode::Bezier);
    let r = m.find_reaction("R1").unwrap().clone();
    let a = &r.reactants[0];
    assert_eq!(leg_inner_end(&r, Role::Reactant, a), pt(0.0, 100.0));
    let c = leg_curve(&m, &r, Role::Reactant, 0).unwrap();
    assert_eq!((c.p0, c.p3), (pt(0.0, 0.0), pt(0.0, 100.0)));
    // Not fanned, though the reaction has two reactants: a straight piece.
    assert!(c.p1.x.abs() < 1e-9 && c.p2.x.abs() < 1e-9, "{c:?}");
    assert_eq!(
        leg_run(&r, Role::Reactant, a),
        [pt(0.0, 100.0), pt(200.0, 100.0), pt(200.0, 0.0)]
    );
    // A leg without corner points is as before: to the junction, fanned.
    let c = leg_curve(&m, &r, Role::Reactant, 1).unwrap();
    assert_eq!(c.p3, pt(200.0, 0.0));
    assert!(leg_run(&r, Role::Reactant, &r.reactants[1]).is_empty());
    // With a stem, the run ends at its side's end of the stem.
    m.find_reaction_mut("R1").unwrap().stem = Some((pt(-30.0, 0.0), pt(30.0, 0.0)));
    let r = m.find_reaction("R1").unwrap();
    assert_eq!(
        leg_run(r, Role::Reactant, &r.reactants[0]).last(),
        Some(&pt(170.0, 0.0))
    );
}

#[test]
fn the_run_is_hit_in_straight_and_bezier_reactions() {
    for mode in [ReactionMode::Straight, ReactionMode::Bezier] {
        let m = model(mode);
        let hit = |x, y| hit_test(&m, pt(x, y), 1.0, true);
        // On the run, along the bottom and up into the junction.
        assert_eq!(hit(100.0, 102.0), Target::Leg("R1".into()), "{mode:?}");
        assert_eq!(hit(198.0, 50.0), Target::Leg("R1".into()), "{mode:?}");
        // On the end piece, below A.
        assert_eq!(hit(2.0, 60.0), Target::Leg("R1".into()), "{mode:?}");
        // Where the line would have run without its corners: nothing.
        assert_eq!(hit(100.0, 0.0), Target::Empty, "{mode:?}");
    }
    // A Linear reaction ignores them (it is drawn as one line).
    let mut m = model(ReactionMode::Linear);
    m.find_reaction_mut("R1").unwrap().reactants.truncate(1);
    m.find_reaction_mut("R1").unwrap().reactants[0].points = vec![pt(0.0, 100.0), pt(200.0, 100.0)];
    assert_eq!(hit_test(&m, pt(100.0, 100.0), 1.0, true), Target::Empty);
    assert_eq!(
        hit_test(&m, pt(100.0, 0.0), 1.0, true),
        Target::Leg("R1".into())
    );
}

#[test]
fn a_smooth_junction_leaves_lines_with_corner_points_alone() {
    let mut m = model(ReactionMode::Bezier);
    apply_smooth_handles(&mut m, "R1");
    let r = m.find_reaction("R1").unwrap();
    assert!(
        !r.reactants[0].ctrl_pts_set,
        "the routed line keeps its handles"
    );
    assert!(r.reactants[1].ctrl_pts_set, "the others are smoothed");
    assert!(r.products[0].ctrl_pts_set);
}

#[test]
fn files_keep_corner_points_as_version_15() {
    let m = model(ReactionMode::Bezier);
    assert_eq!(crate::json::needed_version(&m), 15);
    let text = crate::json::to_json(&m);
    let v: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(v["version"], 15);
    let back = crate::json::from_json(&text).unwrap().model;
    let r = back.find_reaction("R1").unwrap();
    assert_eq!(r.reactants[0].points, [pt(0.0, 100.0), pt(200.0, 100.0)]);
    assert!(r.reactants[1].points.is_empty());
    // Without corner points the file stays at the version it needed before.
    let mut plain = m.clone();
    plain.find_reaction_mut("R1").unwrap().reactants[0]
        .points
        .clear();
    assert!(crate::json::needed_version(&plain) < 15);
    assert!(!crate::json::to_json(&plain).contains("points"));
}
