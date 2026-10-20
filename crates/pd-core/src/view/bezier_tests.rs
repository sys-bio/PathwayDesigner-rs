//! Bézier handles and the reaction shape commands, driven through the view
//! (S §6.4, §7.4, M5).

use super::*;
use crate::bezier::{Ctrl, leg_curve};
use crate::model::Participant;

/// A at (0,0) → B at (400,0), junction (200,0), Bézier, selected. C at
/// (400,200) and D at (0,200) are spare species. Screen = world.
fn view() -> DiagramView {
    let mut m = Model::new();
    let a = m.add_species(pt(0.0, 0.0), 80.0, 36.0);
    let b = m.add_species(pt(400.0, 0.0), 80.0, 36.0);
    m.add_species(pt(400.0, 200.0), 80.0, 36.0);
    m.add_species(pt(0.0, 200.0), 80.0, 36.0);
    let r = m.add_reaction(pt(200.0, 0.0));
    let r = m.find_reaction_mut(&r).unwrap();
    r.reactants.push(Participant::new(a, 1.0));
    r.products.push(Participant::new(b, 1.0));
    r.mode = ReactionMode::Bezier;
    r.selected = true;
    DiagramView::new(m)
}

fn drag(v: &mut DiagramView, from: Pt, to: Pt) {
    v.handle(&ViewEvent::PointerDown {
        pos: from,
        button: Button::Primary,
        modifiers: Modifiers::NONE,
    });
    v.handle(&ViewEvent::PointerMove {
        pos: (from + to) / 2.0,
    });
    v.handle(&ViewEvent::PointerMove { pos: to });
    v.handle(&ViewEvent::PointerUp {
        pos: to,
        button: Button::Primary,
    });
}

fn reaction(v: &DiagramView) -> &crate::model::Reaction {
    &v.model().reactions()[0]
}

// ----- Dragging handles (S §6.4) --------------------------------------------

#[test]
fn grabbing_an_automatic_handle_stores_both_and_undo_makes_them_automatic_again() {
    let mut v = view();
    // Automatic handles of A → J: (70,0) and (130,0). Drag the outer one.
    drag(&mut v, pt(70.0, 0.0), pt(70.0, -60.0));
    let p = &reaction(&v).reactants[0];
    assert!(p.ctrl_pts_set);
    assert_eq!(p.ctrl1, pt(70.0, -60.0));
    assert_eq!(p.ctrl2, pt(130.0, 0.0), "the other handle doesn't jump");
    assert_eq!(v.undo_description(), Some("Move Handle"));
    v.undo();
    assert!(!reaction(&v).reactants[0].ctrl_pts_set);
    v.redo();
    assert_eq!(reaction(&v).reactants[0].ctrl1, pt(70.0, -60.0));
}

#[test]
fn a_press_without_a_move_changes_nothing() {
    let mut v = view();
    let before = v.model().clone();
    drag(&mut v, pt(70.0, 0.0), pt(70.0, 0.0));
    assert_eq!(v.model(), &before);
    assert!(!v.can_undo());
}

#[test]
fn handles_are_hidden_and_not_hit_unless_the_reaction_is_selected() {
    let mut v = view();
    v.handle(&ViewEvent::Cancel);
    assert_eq!(v.target_at(pt(70.0, 0.0)), Target::Leg("R5".into()));
}

#[test]
fn a_handle_drag_says_so() {
    let mut v = view();
    assert!(!v.is_dragging_handle());
    v.handle(&ViewEvent::PointerDown {
        pos: pt(70.0, 0.0),
        button: Button::Primary,
        modifiers: Modifiers::NONE,
    });
    assert!(v.is_dragging_handle());
    v.handle(&ViewEvent::PointerUp {
        pos: pt(70.0, 0.0),
        button: Button::Primary,
    });
    assert!(!v.is_dragging_handle());
    // Dragging a node isn't dragging a handle.
    v.handle(&ViewEvent::PointerDown {
        pos: pt(400.0, 200.0),
        button: Button::Primary,
        modifiers: Modifiers::NONE,
    });
    assert!(v.is_busy() && !v.is_dragging_handle());
}

#[test]
fn esc_during_a_handle_drag_puts_it_back() {
    let mut v = view();
    v.handle(&ViewEvent::PointerDown {
        pos: pt(70.0, 0.0),
        button: Button::Primary,
        modifiers: Modifiers::NONE,
    });
    v.handle(&ViewEvent::PointerMove {
        pos: pt(90.0, 90.0),
    });
    v.handle(&ViewEvent::Cancel);
    assert!(!reaction(&v).reactants[0].ctrl_pts_set);
    assert!(!v.can_undo());
}

#[test]
fn inner_handles_stay_in_line_on_a_smooth_junction() {
    let mut v = view();
    v.toggle_smooth();
    let r = reaction(&v);
    let j = r.junction;
    let other_distance = r.products[0].ctrl1.distance(j);
    // Swing the reactant's inner handle up and left.
    let inner = r.reactants[0].ctrl2;
    let to = pt(150.0, -50.0);
    drag(&mut v, inner, to);
    let r = reaction(&v);
    let d = crate::geometry::normalize(to - j);
    assert_eq!(r.reactants[0].ctrl2, to);
    // The product's inner handle is opposite, at its old distance.
    assert!(r.products[0].ctrl1.distance(j - d * other_distance) < 1e-9);
    assert_eq!(v.undo_description(), Some("Move Handle"));
    v.undo();
    assert!(reaction(&v).reactants[0].ctrl2.distance(inner) < 1e-9);
}

#[test]
fn moving_a_node_never_makes_a_stored_handle_jump() {
    let mut v = view();
    drag(&mut v, pt(70.0, 0.0), pt(70.0, -60.0));
    let stored = reaction(&v).reactants[0].clone();
    // Drag B (the product): A's handles stay exactly where they were.
    v.handle(&ViewEvent::Cancel);
    drag(&mut v, pt(400.0, 0.0), pt(420.0, 30.0));
    let p = &reaction(&v).reactants[0];
    assert_eq!((p.ctrl1, p.ctrl2), (stored.ctrl1, stored.ctrl2));
    // Drag A: its own (outer) handle moves with it, the inner one stays.
    drag(&mut v, pt(-30.0, 10.0), pt(-20.0, 10.0));
    let p = &reaction(&v).reactants[0];
    assert_eq!(p.ctrl1, stored.ctrl1 + pt(10.0, 0.0));
    assert_eq!(p.ctrl2, stored.ctrl2);
}

#[test]
fn a_smooth_junction_recomputes_its_handles_when_its_nodes_move() {
    let mut v = view();
    v.toggle_smooth();
    v.handle(&ViewEvent::Cancel);
    v.select_all();
    // Press on A away from any handle.
    drag(&mut v, pt(-30.0, 10.0), pt(-30.0, 110.0));
    let r = reaction(&v);
    assert_eq!(r.junction, pt(200.0, 100.0));
    let mut expected = v.model().clone();
    crate::bezier::apply_smooth_handles(&mut expected, "R5");
    assert_eq!(r.reactants, expected.reactions()[0].reactants);
    assert_eq!(r.products, expected.reactions()[0].products);
}

// ----- Shape commands (S §7.4) ----------------------------------------------

#[test]
fn toggle_linear_on_and_off() {
    let mut v = view();
    let mut m = v.model().clone();
    m.find_reaction_mut("R5").unwrap().junction = pt(150.0, 80.0);
    v.set_model(m);
    v.select_all();
    v.toggle_linear();
    assert_eq!(reaction(&v).mode, ReactionMode::Linear);
    assert_eq!(
        reaction(&v).junction,
        pt(150.0, 80.0),
        "turning on leaves the junction"
    );
    v.toggle_linear();
    assert_eq!(reaction(&v).mode, ReactionMode::Straight);
    assert_eq!(
        reaction(&v).junction,
        pt(200.0, 0.0),
        "turning off centres it"
    );
    assert_eq!(v.undo_description(), Some("Toggle Linear"));
}

#[test]
fn toggle_linear_leaves_other_reactions_alone() {
    let mut v = view();
    let mut m = v.model().clone();
    m.find_reaction_mut("R5")
        .unwrap()
        .products
        .push(Participant::new("S3", 1.0));
    v.set_model(m);
    v.select_all();
    let r = v.toggle_linear();
    assert_eq!(r, ViewResponse::default(), "nothing to do, no undo step");
    assert_eq!(reaction(&v).mode, ReactionMode::Bezier);
}

#[test]
fn set_straight_and_set_bezier() {
    let mut v = view();
    let mut m = v.model().clone();
    m.find_reaction_mut("R5").unwrap().junction = pt(150.0, 80.0);
    m.find_reaction_mut("R5").unwrap().selected = true;
    v.set_model(m);
    v.set_straight();
    assert_eq!(reaction(&v).mode, ReactionMode::Straight);
    assert_eq!(reaction(&v).junction, pt(200.0, 0.0));
    v.set_bezier();
    assert_eq!(reaction(&v).mode, ReactionMode::Bezier);
    v.undo();
    assert_eq!(reaction(&v).mode, ReactionMode::Straight);
}

#[test]
fn toggle_smooth_stores_the_handles() {
    let mut v = view();
    v.toggle_smooth();
    let r = reaction(&v);
    assert!(r.junction_smooth);
    assert!(r.reactants[0].ctrl_pts_set && r.products[0].ctrl_pts_set);
    v.toggle_smooth();
    assert!(!reaction(&v).junction_smooth);
    // Only for Bézier reactions.
    v.set_straight();
    v.handle(&ViewEvent::Cancel);
    v.select_all();
    assert_eq!(v.toggle_smooth(), ViewResponse::default());
}

#[test]
fn smooth_junction_turns_a_mixed_selection_all_on_first() {
    let mut v = view();
    // A second Bézier reaction, C → D, smooth already.
    let mut m = v.model().clone();
    let r = m.add_reaction(pt(200.0, 200.0));
    let r = m.find_reaction_mut(&r).unwrap();
    r.reactants.push(Participant::new("S3", 1.0));
    r.products.push(Participant::new("S4", 1.0));
    r.mode = ReactionMode::Bezier;
    r.junction_smooth = true;
    r.selected = true;
    let mut v2 = DiagramView::new(m);
    assert!(!v2.selection_is_smooth(), "only one of two is smooth");
    v2.toggle_smooth();
    assert!(v2.selection_is_smooth());
    assert!(v2.model().reactions().iter().all(|r| r.junction_smooth));
    v2.toggle_smooth();
    assert!(v2.model().reactions().iter().all(|r| !r.junction_smooth));
    // Nothing selected: not pressed.
    v.handle(&ViewEvent::Cancel);
    assert!(!v.selection_is_smooth());
}

#[test]
fn make_nice_centres_the_junction_and_resets_handles() {
    let mut v = view();
    drag(&mut v, pt(70.0, 0.0), pt(70.0, -60.0));
    let mut m = v.model().clone();
    let r = m.find_reaction_mut("R5").unwrap();
    r.junction = pt(100.0, 150.0);
    r.mode = ReactionMode::Straight;
    v.set_model(m);
    v.make_nice_reactions(&["R5".to_owned()]);
    let r = reaction(&v);
    assert_eq!(r.mode, ReactionMode::Bezier);
    assert_eq!(r.junction, pt(200.0, 0.0));
    assert!(!r.reactants[0].ctrl_pts_set, "automatic handles again");
    assert_eq!(v.undo_description(), Some("Make Nice"));
}

#[test]
fn make_nice_on_a_smooth_junction_recomputes_smooth_handles() {
    let mut v = view();
    v.toggle_smooth();
    v.make_nice();
    let r = reaction(&v);
    assert!(r.junction_smooth && r.reactants[0].ctrl_pts_set);
}

#[test]
fn new_smooth_reactions_get_their_handles_at_once() {
    let mut v = view();
    v.set_tool(Tool::UNI_UNI);
    for p in [pt(0.0, 200.0), pt(400.0, 200.0)] {
        v.handle(&ViewEvent::PointerDown {
            pos: p,
            button: Button::Primary,
            modifiers: Modifiers::NONE,
        });
        v.handle(&ViewEvent::PointerUp {
            pos: p,
            button: Button::Primary,
        });
    }
    let r = &v.model().reactions()[1];
    assert!(r.junction_smooth);
    assert!(r.reactants[0].ctrl_pts_set && r.products[0].ctrl_pts_set);
    // The curve is the one the smooth rule gives.
    let c = leg_curve(v.model(), r, Role::Reactant, 0).unwrap();
    assert_eq!(c.p2, Ctrl::inner(Role::Reactant).get(&r.reactants[0]));
}

// ----- A smooth junction keeps hand-shaped outer handles (S §6.4, §7.5) -----

/// The test view with a smooth junction, and A's outer handle pulled 60 up
/// by hand. Returns the view and where that handle is.
fn smooth_with_a_shaped_outer_handle() -> (DiagramView, Pt) {
    let v = view();
    let mut m = v.model().clone();
    let id = m.reactions()[0].id.clone();
    m.find_reaction_mut(&id).unwrap().junction_smooth = true;
    crate::bezier::apply_smooth_handles(&mut m, &id);
    let r = m.find_reaction_mut(&id).unwrap();
    r.reactants[0].ctrl1 += pt(0.0, -60.0);
    let shaped = r.reactants[0].ctrl1;
    let mut v = DiagramView::new(m);
    v.handle(&ViewEvent::Cancel);
    (v, shaped)
}

fn select(v: &mut DiagramView, ids: &[&str]) {
    v.handle(&ViewEvent::Cancel);
    for id in ids {
        v.select_item(&Target::Species((*id).into()), true);
    }
}

/// Every inner handle is where a smooth junction puts it for the species'
/// and junction's places now (S §7.4.4).
fn inner_handles_are_smooth(v: &DiagramView) -> bool {
    let r = reaction(v);
    crate::bezier::smooth_handles(v.model(), r)
        .into_iter()
        .all(|(role, index, h)| {
            let (c1, c2) = h.expect("long legs");
            let p = &r.participants(role)[index];
            match role {
                crate::model::Role::Reactant => p.ctrl2.distance(c2) < 1e-9,
                _ => p.ctrl1.distance(c1) < 1e-9,
            }
        })
}

#[test]
fn dragging_a_whole_smooth_reaction_keeps_a_shaped_handle() {
    let (mut v, shaped) = smooth_with_a_shaped_outer_handle();
    let b_outer = reaction(&v).products[0].ctrl2;
    let (a, b) = (
        reaction(&v).reactants[0].species.clone(),
        reaction(&v).products[0].species.clone(),
    );
    select(&mut v, &[&a, &b]);
    drag(&mut v, pt(0.0, 0.0), pt(30.0, 20.0));
    let d = pt(30.0, 20.0);
    assert_eq!(reaction(&v).junction, pt(230.0, 20.0));
    assert_eq!(
        reaction(&v).reactants[0].ctrl1,
        shaped + d,
        "kept, moved with A"
    );
    assert_eq!(reaction(&v).products[0].ctrl2, b_outer + d);
    assert!(inner_handles_are_smooth(&v));
}

#[test]
fn nudging_species_of_a_smooth_reaction_keeps_a_shaped_handle() {
    // One species: its outer handle goes with it, nothing else changes.
    let (mut v, shaped) = smooth_with_a_shaped_outer_handle();
    let before = reaction(&v).clone();
    let a = before.reactants[0].species.clone();
    select(&mut v, &[&a]);
    v.nudge(pt(10.0, 0.0));
    let r = reaction(&v);
    assert_eq!(r.junction, before.junction);
    assert_eq!(r.reactants[0].ctrl1, shaped + pt(10.0, 0.0));
    assert_eq!(r.reactants[0].ctrl2, before.reactants[0].ctrl2);
    assert_eq!(r.products[0], before.products[0]);

    // Every species: the junction follows, the shaped handle is kept, and
    // the inner handles are lined up again.
    let (mut v, shaped) = smooth_with_a_shaped_outer_handle();
    let b = reaction(&v).products[0].species.clone();
    let a = reaction(&v).reactants[0].species.clone();
    select(&mut v, &[&a, &b]);
    v.nudge(pt(10.0, 5.0));
    assert_eq!(reaction(&v).junction, pt(210.0, 5.0));
    assert_eq!(reaction(&v).reactants[0].ctrl1, shaped + pt(10.0, 5.0));
    assert!(inner_handles_are_smooth(&v));
}
