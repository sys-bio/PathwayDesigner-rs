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
