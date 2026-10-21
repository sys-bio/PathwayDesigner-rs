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

// ----- Editing (step 2) ------------------------------------------------------

fn view() -> DiagramView {
    DiagramView::new(model(ReactionMode::Bezier))
}

/// A's line's corner points.
fn points(v: &DiagramView) -> Vec<Pt> {
    v.model().find_reaction("R1").unwrap().reactants[0]
        .points
        .clone()
}

fn a_leg(v: &DiagramView) -> Participant {
    v.model().find_reaction("R1").unwrap().reactants[0].clone()
}

/// A's line with stored handles: outer (0,30), inner (0,70), by its first
/// corner point (0,100).
fn with_handles(v: &mut DiagramView) {
    let p = &mut v.model.find_reaction_mut("R1").unwrap().reactants[0];
    (p.ctrl1, p.ctrl2, p.ctrl_pts_set) = (pt(0.0, 30.0), pt(0.0, 70.0), true);
}

fn press(v: &mut DiagramView, at: Pt, button: Button) -> ViewResponse {
    v.handle(&ViewEvent::PointerDown {
        pos: at,
        button,
        modifiers: Modifiers::NONE,
    })
}

fn drag(v: &mut DiagramView, from: Pt, to: Pt) {
    press(v, from, Button::Primary);
    v.handle(&ViewEvent::PointerMove { pos: to });
    v.handle(&ViewEvent::PointerUp {
        pos: to,
        button: Button::Primary,
    });
}

#[test]
fn a_selected_reaction_s_corner_points_can_be_grabbed() {
    let mut v = view();
    // Not while the reaction isn't selected: the line is there instead.
    assert_eq!(v.target_at(pt(0.0, 100.0)), Target::Leg("R1".into()));
    v.select_item(&Target::Leg("R1".into()), false);
    assert_eq!(
        v.target_at(pt(1.0, 101.0)),
        Target::CornerPoint("R1".into(), Role::Reactant, 0, 0)
    );
    assert_eq!(
        v.target_at(pt(200.0, 100.0)),
        Target::CornerPoint("R1".into(), Role::Reactant, 0, 1)
    );
    // A right click on one asks for the point's menu.
    let r = press(&mut v, pt(200.0, 100.0), Button::Secondary);
    assert_eq!(
        r.request,
        Some(Request::ContextMenu(Target::CornerPoint(
            "R1".into(),
            Role::Reactant,
            0,
            1
        )))
    );
}

#[test]
fn a_corner_point_is_dragged_and_its_handle_follows_the_first() {
    let mut v = view();
    with_handles(&mut v);
    v.select_item(&Target::Leg("R1".into()), false);
    // The second: only it moves.
    drag(&mut v, pt(200.0, 100.0), pt(200.0, 140.0));
    assert_eq!(points(&v), [pt(0.0, 100.0), pt(200.0, 140.0)]);
    assert_eq!(a_leg(&v).ctrl2, pt(0.0, 70.0));
    assert_eq!(v.undo_description(), Some("Move Corner Point"));
    // The first: the end piece's inner handle moves as much.
    drag(&mut v, pt(0.0, 100.0), pt(20.0, 120.0));
    assert_eq!(points(&v), [pt(20.0, 120.0), pt(200.0, 140.0)]);
    assert_eq!(a_leg(&v).ctrl2, pt(20.0, 90.0));
    assert_eq!(a_leg(&v).ctrl1, pt(0.0, 30.0), "the outer one stays");
    v.undo();
    assert_eq!(points(&v), [pt(0.0, 100.0), pt(200.0, 140.0)]);
    assert_eq!(a_leg(&v).ctrl2, pt(0.0, 70.0));
    // The arrow keys move the current one, its handle with it.
    v.select_item(&Target::Leg("R1".into()), false);
    press(&mut v, pt(0.0, 100.0), Button::Primary);
    v.handle(&ViewEvent::PointerUp {
        pos: pt(0.0, 100.0),
        button: Button::Primary,
    });
    assert_eq!(v.current_corner_point(), Some(pt(0.0, 100.0)));
    v.nudge_key(pt(1.0, 0.0), true, false);
    assert_eq!(points(&v)[0], pt(10.0, 100.0));
    assert_eq!(a_leg(&v).ctrl2, pt(10.0, 70.0));
}

#[test]
fn a_double_click_adds_a_corner_point_on_the_piece_clicked() {
    // On the run, between the two corner points.
    let mut v = view();
    v.handle(&ViewEvent::DoubleClick {
        pos: pt(100.0, 100.0),
    });
    assert_eq!(
        points(&v),
        [pt(0.0, 100.0), pt(100.0, 100.0), pt(200.0, 100.0)]
    );
    assert_eq!(v.undo_description(), Some("Add Corner Point"));
    // On the last piece, into the junction.
    v.handle(&ViewEvent::DoubleClick {
        pos: pt(200.0, 50.0),
    });
    assert_eq!(points(&v).last(), Some(&pt(200.0, 50.0)));
    // On the end piece, by A: it becomes the first.
    let mut v = view();
    v.handle(&ViewEvent::DoubleClick { pos: pt(0.0, 60.0) });
    assert_eq!(points(&v)[..2], [pt(0.0, 60.0), pt(0.0, 100.0)]);
    // A line without corner points gets its first: B's, from the junction.
    let mut v = view();
    v.handle(&ViewEvent::DoubleClick {
        pos: pt(300.0, 0.0),
    });
    let r = v.model().find_reaction("R1").unwrap();
    assert_eq!(r.products[0].points, [pt(300.0, 0.0)]);
    // Which piece is where.
    let v = view();
    assert_eq!(
        v.line_piece_at("R1", pt(0.0, 60.0)),
        Some((Role::Reactant, 0, 0))
    );
    assert_eq!(
        v.line_piece_at("R1", pt(100.0, 100.0)),
        Some((Role::Reactant, 0, 1))
    );
    assert_eq!(
        v.line_piece_at("R1", pt(200.0, 50.0)),
        Some((Role::Reactant, 0, 2))
    );
    assert_eq!(v.line_piece_at("R1", pt(100.0, 300.0)), None);
}

#[test]
fn corner_points_are_removed_and_straightened_with_the_handle_following() {
    let mut v = view();
    with_handles(&mut v);
    let line = (Role::Reactant, 0);
    assert!(v.line_bent("R1", line));
    // The first goes: the end piece now runs to (200,100), its inner handle
    // moved as much.
    v.remove_corner_point("R1", line, 0);
    assert_eq!(points(&v), [pt(200.0, 100.0)]);
    assert_eq!(a_leg(&v).ctrl2, pt(200.0, 70.0));
    assert_eq!(v.undo_description(), Some("Remove Corner Point"));
    v.undo();
    // Straighten Line: all go, and the end piece runs to the junction.
    v.straighten_line("R1", line);
    assert!(points(&v).is_empty());
    assert!(!v.line_bent("R1", line));
    assert_eq!(a_leg(&v).ctrl2, pt(200.0, -30.0));
    assert_eq!(v.undo_description(), Some("Straighten Line"));
    // Nothing to straighten (C's line has no corner points): no undo step,
    // and the redo is still there.
    v.undo();
    v.straighten_line("R1", (Role::Reactant, 1));
    assert_eq!(v.undo_description(), None);
    assert_eq!(v.redo_description(), Some("Straighten Line"));
    assert_eq!(points(&v), [pt(0.0, 100.0), pt(200.0, 100.0)]);
}

#[test]
fn corner_points_move_only_when_the_species_and_the_junction_do() {
    let mut v = view();
    with_handles(&mut v);
    // A alone: the points and the inner handle stay; the outer one follows.
    v.select_item(&Target::Species("A".into()), false);
    drag(&mut v, pt(0.0, 0.0), pt(-20.0, 0.0));
    assert_eq!(points(&v), [pt(0.0, 100.0), pt(200.0, 100.0)]);
    assert_eq!(a_leg(&v).ctrl2, pt(0.0, 70.0));
    assert_eq!(a_leg(&v).ctrl1, pt(-20.0, 30.0));
    v.undo();
    // Everything: they all follow.
    for id in ["A", "B", "C"] {
        v.select_item(&Target::Species(id.into()), true);
    }
    drag(&mut v, pt(0.0, 0.0), pt(10.0, 20.0));
    assert_eq!(points(&v), [pt(10.0, 120.0), pt(210.0, 120.0)]);
    assert_eq!(a_leg(&v).ctrl2, pt(10.0, 90.0));
    // The junction alone: they stay, and so does the inner handle.
    let before = (points(&v), a_leg(&v).ctrl2);
    v.select_item(&Target::Junction("R1".into()), false);
    drag(&mut v, pt(210.0, 20.0), pt(210.0, 60.0));
    assert_eq!((points(&v), a_leg(&v).ctrl2), before);
    // Adding a stem leaves the routed line's inner handle too.
    v.add_stem("R1");
    assert_eq!(a_leg(&v).ctrl2, before.1);
    // Arrange moves species but leaves routes.
    v.select_item(&Target::Species("A".into()), false);
    v.select_item(&Target::Species("C".into()), true);
    v.arrange(crate::view::Arrange::Right);
    assert_eq!(points(&v), before.0);
}

#[test]
fn commands_that_reshape_a_reaction_take_its_lines_corner_points_off() {
    // Modifier E's arc keeps its own (S §5.7.1).
    let with_modifier = || {
        let mut v = view();
        with_handles(&mut v);
        v.model
            .push_species(SpeciesNode::new("E", pt(200.0, -200.0)))
            .unwrap();
        let r = v.model.find_reaction_mut("R1").unwrap();
        let mut e = Participant::new("E", 1.0);
        e.points = vec![pt(300.0, -200.0)];
        r.modifiers.push(e);
        v.select_item(&Target::Leg("R1".into()), false);
        v
    };
    let check = |v: &DiagramView, what: &str| {
        let r = v.model().find_reaction("R1").unwrap();
        assert!(r.reactants[0].points.is_empty(), "{what}");
        assert!(!r.reactants[0].ctrl_pts_set, "{what}: automatic handles");
        assert_eq!(r.modifiers[0].points, [pt(300.0, -200.0)], "{what}");
    };
    let mut v = with_modifier();
    v.set_straight();
    check(&v, "Set Straight");
    v.undo();
    assert_eq!(points(&v).len(), 2, "undo puts them back");
    let mut v = with_modifier();
    v.make_nice();
    check(&v, "Make Nice");
    let mut v = with_modifier();
    v.start_layout(crate::layout::LayoutParams::default(), 1);
    while v.is_laying_out() {
        v.layout_step(200);
    }
    check(&v, "Auto-layout");
    // Set Bézier keeps them.
    let mut v = with_modifier();
    v.set_bezier();
    assert_eq!(points(&v).len(), 2);
}

/// C's line with stored handles: outer (40,160), inner (150,50).
fn c_handles(v: &mut DiagramView) {
    let p = &mut v.model.find_reaction_mut("R1").unwrap().reactants[1];
    (p.ctrl1, p.ctrl2, p.ctrl_pts_set) = (pt(40.0, 160.0), pt(150.0, 50.0), true);
}

#[test]
fn what_moves_the_junction_alone_leaves_a_routed_line_s_inner_handle() {
    // A box round the junction only: dragging it carries the junction, not
    // the species, so the corner points stay, and so does the inner handle
    // by them.
    let mut v = view();
    with_handles(&mut v);
    let mut c = crate::model::Compartment::new("box");
    c.frame = Some(crate::model::Frame {
        center: pt(200.0, 0.0),
        width: 60.0,
        height: 40.0,
    });
    v.model.compartments.push(c);
    v.select_item(&Target::Compartment("box".into()), false);
    drag(&mut v, pt(215.0, -20.0), pt(215.0, 20.0));
    assert_eq!(
        v.model().find_reaction("R1").unwrap().junction,
        pt(200.0, 40.0)
    );
    assert_eq!(points(&v), [pt(0.0, 100.0), pt(200.0, 100.0)]);
    assert_eq!(a_leg(&v).ctrl2, pt(0.0, 70.0));

    // Arrange shifting the junction: the same.
    let mut v = view();
    with_handles(&mut v);
    for id in ["A", "B", "C"] {
        v.select_item(&Target::Species(id.into()), true);
    }
    v.arrange(crate::view::Arrange::Middle);
    assert_ne!(
        v.model().find_reaction("R1").unwrap().junction,
        pt(200.0, 0.0)
    );
    assert_eq!(points(&v), [pt(0.0, 100.0), pt(200.0, 100.0)]);
    assert_eq!(a_leg(&v).ctrl2, pt(0.0, 70.0));
}

#[test]
fn a_stem_end_dragged_leaves_a_routed_line_s_inner_handle() {
    let mut v = view();
    with_handles(&mut v);
    c_handles(&mut v);
    v.add_stem("R1");
    let r = v.model().find_reaction("R1").unwrap();
    let end = r.junction + r.stem.unwrap().0;
    let c_inner = r.reactants[1].ctrl2;
    v.select_item(&Target::Leg("R1".into()), false);
    assert_eq!(
        v.target_at(end),
        Target::StemEnd("R1".into(), Role::Reactant)
    );
    drag(&mut v, end, end + pt(0.0, 20.0));
    let r = v.model().find_reaction("R1").unwrap();
    assert_eq!(r.reactants[1].ctrl2, c_inner + pt(0.0, 20.0), "C's follows");
    assert_eq!(r.reactants[0].ctrl2, pt(0.0, 70.0), "A's stays");
}

#[test]
fn a_smooth_junction_s_handle_drag_leaves_routed_lines() {
    let mut v = view();
    with_handles(&mut v);
    c_handles(&mut v);
    v.model.find_reaction_mut("R1").unwrap().junction_smooth = true;
    v.select_item(&Target::Leg("R1".into()), false);
    // C's inner handle: the others swing into line, B's among them.
    drag(&mut v, pt(150.0, 50.0), pt(140.0, 60.0));
    let r = v.model().find_reaction("R1").unwrap();
    assert_eq!(r.reactants[1].ctrl2, pt(140.0, 60.0));
    assert!(r.products[0].ctrl_pts_set, "B's swung");
    assert_eq!(r.reactants[0].ctrl2, pt(0.0, 70.0), "A's stays");
}

// ----- SBML (step 3) ---------------------------------------------------------

const SBML_OPTS: crate::antimony::ImportOptions = crate::antimony::ImportOptions {
    convert_catalytic: false,
    seed: 1,
};

/// [`model`] with B's line routed too, up and over from the junction, and A's
/// with stored handles.
fn routed_both(mode: ReactionMode) -> Model {
    let mut m = model(mode);
    let r = m.find_reaction_mut("R1").unwrap();
    r.products[0].points = vec![pt(400.0, -100.0), pt(300.0, -100.0)];
    let a = &mut r.reactants[0];
    (a.ctrl1, a.ctrl2, a.ctrl_pts_set) = (pt(-10.0, 30.0), pt(10.0, 70.0), true);
    m
}

fn round_trip(m: &Model) -> Model {
    crate::sbml::import(&crate::sbml::export(m).xml, &SBML_OPTS)
        .unwrap()
        .model
}

/// The layout of `m` as written.
fn written(m: &Model) -> sbml_io::Document {
    sbml_io::read(&crate::sbml::export(m).xml).unwrap().document
}

/// The curve of the species reference glyph of `species` in reaction R1.
fn curve_of<'a>(doc: &'a sbml_io::Document, species: &str) -> &'a sbml_io::Curve {
    let l = &doc.model.as_ref().unwrap().layouts[0];
    let glyph_of: std::collections::HashMap<&str, &str> = l
        .species_glyphs
        .iter()
        .map(|g| (g.id.as_str(), g.species.as_deref().unwrap_or("")))
        .collect();
    let rg = &l.reaction_glyphs[0];
    let srg = rg
        .species_reference_glyphs
        .iter()
        .find(|s| glyph_of[s.species_glyph.as_str()] == species)
        .unwrap();
    srg.curve.as_ref().unwrap()
}

fn ends(s: &sbml_io::Segment) -> (Pt, Pt) {
    let (a, b) = (s.start(), s.end());
    (pt(a.x, a.y), pt(b.x, b.y))
}

#[test]
fn a_routed_line_is_written_as_drawn_in_the_direction_of_flow() {
    let doc = written(&routed_both(ReactionMode::Bezier));
    // A, a reactant: its end piece from A's border (curved, as drawn), then
    // straight on through its corner points to the junction.
    let a = curve_of(&doc, "A");
    assert_eq!(a.segments.len(), 3);
    assert!(matches!(a.segments[0], sbml_io::Segment::Bezier { .. }));
    let (start, end) = ends(&a.segments[0]);
    assert!((start.y - 18.0).abs() < 1e-4, "A's border: {start:?}");
    assert_eq!(end, pt(0.0, 100.0));
    assert_eq!(ends(&a.segments[1]), (pt(0.0, 100.0), pt(200.0, 100.0)));
    assert_eq!(ends(&a.segments[2]), (pt(200.0, 100.0), pt(200.0, 0.0)));
    // B, a product: from the junction back through its corner points, then
    // its end piece into B's border.
    let b = curve_of(&doc, "B");
    assert_eq!(b.segments.len(), 3);
    assert_eq!(ends(&b.segments[0]), (pt(200.0, 0.0), pt(300.0, -100.0)));
    assert_eq!(ends(&b.segments[1]), (pt(300.0, -100.0), pt(400.0, -100.0)));
    let (start, end) = ends(&b.segments[2]);
    assert_eq!(start, pt(400.0, -100.0));
    assert!((end.y + 18.0).abs() < 1e-4, "B's border: {end:?}");
    // C, without corner points: one piece, as before.
    assert_eq!(curve_of(&doc, "C").segments.len(), 1);
    // Straight: every piece a line.
    let doc = written(&routed_both(ReactionMode::Straight));
    let a = curve_of(&doc, "A");
    assert_eq!(a.segments.len(), 3);
    // It leaves A's border towards the first corner point, below A.
    assert_eq!(ends(&a.segments[0]), (pt(0.0, 18.0), pt(0.0, 100.0)));
    assert!(
        a.segments
            .iter()
            .all(|s| matches!(s, sbml_io::Segment::Line { .. }))
    );
}

#[test]
fn routed_lines_come_back_from_sbml() {
    for mode in [ReactionMode::Bezier, ReactionMode::Straight] {
        let m = routed_both(mode);
        let back = round_trip(&m);
        let (r, b) = (
            m.find_reaction("R1").unwrap(),
            back.find_reaction("R1").unwrap(),
        );
        assert_eq!(b.mode, mode);
        for (p, q) in r
            .reactants
            .iter()
            .chain(&r.products)
            .zip(b.reactants.iter().chain(&b.products))
        {
            assert_eq!(q.points, p.points, "{mode:?} {}", p.species);
        }
        if mode == ReactionMode::Bezier {
            // The stored handles, carried on to A's centre again.
            let a = &b.reactants[0];
            assert!(a.ctrl_pts_set);
            assert!(a.ctrl1.distance(pt(-10.0, 30.0)) < 1e-6, "{:?}", a.ctrl1);
            assert!(a.ctrl2.distance(pt(10.0, 70.0)) < 1e-6, "{:?}", a.ctrl2);
            // B's end piece, written from its first corner point into B, is
            // turned round: the same curve as drawn.
            let drawn = leg_curve(&m, r, Role::Product, 0).unwrap();
            let read = leg_curve(&back, b, Role::Product, 0).unwrap();
            assert!(read.p1.distance(drawn.p1) < 1e-6, "{read:?} {drawn:?}");
            assert!(read.p2.distance(drawn.p2) < 1e-6, "{read:?} {drawn:?}");
        }
    }
    // With a stem, the run ends at its end of the stem.
    let mut m = routed_both(ReactionMode::Bezier);
    m.find_reaction_mut("R1").unwrap().stem = Some((pt(0.0, 30.0), pt(30.0, 0.0)));
    let doc = written(&m);
    let a = curve_of(&doc, "A");
    assert_eq!(ends(a.segments.last().unwrap()).1, pt(200.0, 30.0));
    let back = round_trip(&m);
    let b = back.find_reaction("R1").unwrap();
    assert_eq!(b.stem, Some((pt(0.0, 30.0), pt(30.0, 0.0))));
    assert_eq!(b.reactants[0].points, [pt(0.0, 100.0), pt(200.0, 100.0)]);
}

#[test]
fn a_linear_reaction_writes_its_line_without_corner_points() {
    let mut m = model(ReactionMode::Linear);
    let r = m.find_reaction_mut("R1").unwrap();
    r.reactants.truncate(1);
    let doc = written(&m);
    assert_eq!(curve_of(&doc, "A").segments.len(), 1);
}

#[test]
fn another_tool_s_routed_curves_are_read_whichever_way_round() {
    // A's curve as another tool might write it: from the junction to A,
    // straight pieces, with a curved piece in the middle (read as straight).
    let m = routed_both(ReactionMode::Straight);
    let mut doc = written(&m);
    let l = &mut doc.model.as_mut().unwrap().layouts[0];
    let a_glyph = l
        .species_glyphs
        .iter()
        .find(|g| g.species.as_deref() == Some("A"))
        .unwrap()
        .id
        .clone();
    let srg = l.reaction_glyphs[0]
        .species_reference_glyphs
        .iter_mut()
        .find(|s| s.species_glyph == a_glyph)
        .unwrap();
    let p = |x, y| sbml_io::Point { x, y, z: None };
    srg.curve = Some(sbml_io::Curve {
        segments: vec![
            sbml_io::Segment::Line {
                start: p(200.0, 0.0),
                end: p(200.0, 100.0),
            },
            sbml_io::Segment::Bezier {
                start: p(200.0, 100.0),
                end: p(0.0, 100.0),
                base1: p(150.0, 120.0),
                base2: p(50.0, 120.0),
            },
            sbml_io::Segment::Line {
                start: p(0.0, 100.0),
                end: p(0.0, 18.0),
            },
        ],
    });
    let xml = sbml_io::write(&doc);
    let back = crate::sbml::import(&xml, &SBML_OPTS).unwrap().model;
    let r = back.find_reaction("R1").unwrap();
    assert_eq!(r.reactants[0].points, [pt(0.0, 100.0), pt(200.0, 100.0)]);
    assert!(!r.reactants[0].ctrl_pts_set, "its end piece is a line");
    assert_eq!(r.mode, ReactionMode::Straight);
}
