//! The interaction state machine, driven with made-up events (SPEC-Rust §12),
//! against the rules of S §6.2–§6.4 and §9.

use super::*;
use crate::model::{Participant, SpeciesNode};

/// A at (0,0), B at (400,0), C at (400,200); R4: A → B with its junction at
/// (200,0); R5: A → C with its junction at (200,100). View at zoom 1, no
/// scroll, so screen = world.
fn view() -> DiagramView {
    let mut m = Model::new();
    let a = m.add_species(pt(0.0, 0.0), 80.0, 36.0);
    let b = m.add_species(pt(400.0, 0.0), 80.0, 36.0);
    let c = m.add_species(pt(400.0, 200.0), 80.0, 36.0);
    for (product, j) in [(&b, pt(200.0, 0.0)), (&c, pt(200.0, 100.0))] {
        let r = m.add_reaction(j);
        let r = m.find_reaction_mut(&r).unwrap();
        r.reactants.push(Participant::new(&a, 1.0));
        r.products.push(Participant::new(product.as_str(), 1.0));
    }
    DiagramView::new(m)
}

fn down(v: &mut DiagramView, x: f64, y: f64, modifiers: Modifiers) -> ViewResponse {
    v.handle(&ViewEvent::PointerDown {
        pos: pt(x, y),
        button: Button::Primary,
        modifiers,
    })
}

fn move_to(v: &mut DiagramView, x: f64, y: f64) -> ViewResponse {
    v.handle(&ViewEvent::PointerMove { pos: pt(x, y) })
}

fn up(v: &mut DiagramView, x: f64, y: f64) -> ViewResponse {
    v.handle(&ViewEvent::PointerUp {
        pos: pt(x, y),
        button: Button::Primary,
    })
}

/// Press at one point, drag to another, release.
fn drag(v: &mut DiagramView, from: (f64, f64), to: (f64, f64), modifiers: Modifiers) {
    down(v, from.0, from.1, modifiers);
    move_to(v, (from.0 + to.0) / 2.0, (from.1 + to.1) / 2.0);
    move_to(v, to.0, to.1);
    up(v, to.0, to.1);
}

fn click(v: &mut DiagramView, x: f64, y: f64, modifiers: Modifiers) {
    down(v, x, y, modifiers);
    up(v, x, y);
}

fn at(v: &DiagramView, id: &str) -> Pt {
    v.model().find_species(id).unwrap().center
}

fn junction(v: &DiagramView, id: &str) -> Pt {
    v.model().find_reaction(id).unwrap().junction
}

fn ids(list: &[&str]) -> Vec<Id> {
    list.iter().map(|s| (*s).to_owned()).collect()
}

// ----- Selection (S §6.3) ---------------------------------------------------

#[test]
fn click_on_an_unselected_species_selects_only_it() {
    let mut v = view();
    click(&mut v, 0.0, 0.0, Modifiers::NONE);
    click(&mut v, 400.0, 0.0, Modifiers::SHIFT);
    click(&mut v, 400.0, 200.0, Modifiers::NONE);
    assert_eq!(v.selection(), (ids(&["S3"]), vec![]));
}

#[test]
fn click_on_a_selected_species_keeps_the_selection_and_drags_it_all() {
    let mut v = view();
    click(&mut v, 0.0, 0.0, Modifiers::NONE);
    click(&mut v, 400.0, 0.0, Modifiers::SHIFT);
    drag(&mut v, (400.0, 0.0), (410.0, 20.0), Modifiers::NONE);
    assert_eq!(v.selection().0, ids(&["S1", "S2"]));
    assert_eq!(at(&v, "S1"), pt(10.0, 20.0));
    assert_eq!(at(&v, "S2"), pt(410.0, 20.0));
    assert_eq!(at(&v, "S3"), pt(400.0, 200.0));
}

#[test]
fn shift_click_toggles_a_species_without_dragging() {
    let mut v = view();
    click(&mut v, 0.0, 0.0, Modifiers::NONE);
    drag(&mut v, (400.0, 0.0), (450.0, 50.0), Modifiers::SHIFT);
    assert_eq!(v.selection().0, ids(&["S1", "S2"]));
    assert_eq!(at(&v, "S2"), pt(400.0, 0.0), "shift-click does not drag");
    click(&mut v, 0.0, 0.0, Modifiers::SHIFT);
    assert_eq!(v.selection().0, ids(&["S2"]));
    assert!(!v.can_undo());
}

#[test]
fn click_on_a_junction_selects_the_reaction_and_drags_the_junction() {
    let mut v = view();
    click(&mut v, 0.0, 0.0, Modifiers::NONE);
    drag(&mut v, (200.0, 0.0), (230.0, -40.0), Modifiers::NONE);
    assert_eq!(v.selection(), (vec![], ids(&["R4"])));
    assert_eq!(junction(&v, "R4"), pt(230.0, -40.0));
    assert_eq!(v.undo_description(), Some("Move Junction"));
    // With Shift the selection is kept.
    click(&mut v, 0.0, 0.0, Modifiers::SHIFT);
    click(&mut v, 200.0, 100.0, Modifiers::SHIFT);
    assert_eq!(v.selection(), (ids(&["S1"]), ids(&["R4", "R5"])));
}

#[test]
fn junction_drag_keeps_the_grab_offset() {
    let mut v = view();
    // Grab 3 units right of the junction: it moves by the delta, not to the cursor.
    drag(&mut v, (203.0, 0.0), (253.0, 0.0), Modifiers::NONE);
    assert_eq!(junction(&v, "R4"), pt(250.0, 0.0));
}

#[test]
fn click_on_a_leg_selects_only_that_reaction() {
    let mut v = view();
    v.select_all();
    drag(&mut v, (100.0, 0.0), (150.0, 50.0), Modifiers::NONE);
    assert_eq!(v.selection(), (vec![], ids(&["R4"])));
    assert_eq!(
        junction(&v, "R4"),
        pt(200.0, 0.0),
        "a leg click does not drag"
    );
    click(&mut v, 300.0, 0.0, Modifiers::SHIFT);
    assert_eq!(v.selection(), (vec![], vec![]));
}

#[test]
fn click_on_empty_canvas_clears_the_selection() {
    let mut v = view();
    v.select_all();
    let r = down(&mut v, 200.0, -300.0, Modifiers::NONE);
    assert!(r.selection_changed);
    up(&mut v, 200.0, -300.0);
    assert!(!v.has_selection());
}

#[test]
fn rubber_band_selects_touching_species_and_enclosed_junctions() {
    let mut v = view();
    // A band from (-10,-10) to (210,50): meets A, contains R4's junction
    // (200,0) but not R5's (200,100).
    drag(&mut v, (-10.0, -100.0), (210.0, 50.0), Modifiers::NONE);
    assert_eq!(v.selection(), (ids(&["S1"]), ids(&["R4"])));
    // Dragged the other way, with Shift: adds C and R5.
    drag(&mut v, (500.0, 250.0), (150.0, 90.0), Modifiers::SHIFT);
    assert_eq!(v.selection(), (ids(&["S1", "S3"]), ids(&["R4", "R5"])));
}

#[test]
fn rubber_band_is_drawn_while_dragging() {
    use crate::model::Rgba;
    #[derive(Default)]
    struct Count(Vec<Option<Rgba>>);
    impl Renderer for Count {
        fn round_rect(&mut self, _: SRect, _: f32, fill: Option<Rgba>, _: Option<Stroke>) {
            self.0.push(fill);
        }
        fn circle(&mut self, _: crate::render::SPt, _: f32, _: Option<Rgba>, _: Option<Stroke>) {}
        fn polyline(&mut self, _: &[crate::render::SPt], _: Stroke) {}
        fn cubic(
            &mut self,
            _: crate::render::SPt,
            _: crate::render::SPt,
            _: crate::render::SPt,
            _: crate::render::SPt,
            _: Stroke,
        ) {
        }
        fn polygon(&mut self, _: &[crate::render::SPt], _: Rgba) {}
        fn text(
            &mut self,
            _: crate::render::SPt,
            _: &str,
            _: f32,
            _: crate::model::TextFont,
            _: Rgba,
        ) {
        }
    }
    let mut v = view();
    down(&mut v, 600.0, 600.0, Modifiers::NONE);
    move_to(&mut v, 650.0, 650.0);
    let mut c = Count::default();
    v.draw(&mut c, SRect::default());
    let band = c.0.last().copied().flatten().unwrap();
    assert_eq!(band.0[3], RUBBER_BAND_FILL_ALPHA);
    up(&mut v, 650.0, 650.0);
    let mut c = Count::default();
    v.draw(&mut c, SRect::default());
    assert_ne!(c.0.last().copied().flatten(), Some(band));
}

#[test]
fn select_all() {
    let mut v = view();
    let r = v.select_all();
    assert!(r.selection_changed);
    assert_eq!(
        v.selection(),
        (ids(&["S1", "S2", "S3"]), ids(&["R4", "R5"]))
    );
    assert!(!v.select_all().selection_changed);
}

#[test]
fn esc_cancels_and_clears_the_selection() {
    let mut v = view();
    down(&mut v, 0.0, 0.0, Modifiers::NONE);
    move_to(&mut v, 50.0, 50.0);
    assert_eq!(at(&v, "S1"), pt(50.0, 50.0));
    let r = v.handle(&ViewEvent::Cancel);
    assert!(r.selection_changed);
    assert_eq!(at(&v, "S1"), pt(0.0, 0.0), "the drag is undone");
    assert!(!v.has_selection());
    assert!(!v.is_busy());
    // The release that follows does nothing.
    up(&mut v, 50.0, 50.0);
    assert_eq!(at(&v, "S1"), pt(0.0, 0.0));
    assert!(!v.can_undo());
}

#[test]
fn losing_focus_cancels_the_drag_but_keeps_the_selection() {
    let mut v = view();
    down(&mut v, 200.0, 0.0, Modifiers::NONE);
    move_to(&mut v, 250.0, 50.0);
    v.handle(&ViewEvent::FocusLost);
    assert_eq!(junction(&v, "R4"), pt(200.0, 0.0));
    assert_eq!(v.selection().1, ids(&["R4"]));
    assert!(!v.can_undo());
}

// ----- Dragging (S §6.4) ----------------------------------------------------

#[test]
fn junction_moves_only_when_all_its_reactants_and_products_move() {
    let mut v = view();
    // A and B selected: R4 (A → B) moves its junction, R5 (A → C) doesn't.
    click(&mut v, 0.0, 0.0, Modifiers::NONE);
    click(&mut v, 400.0, 0.0, Modifiers::SHIFT);
    drag(&mut v, (0.0, 0.0), (0.0, 30.0), Modifiers::NONE);
    assert_eq!(junction(&v, "R4"), pt(200.0, 30.0));
    assert_eq!(junction(&v, "R5"), pt(200.0, 100.0));
}

#[test]
fn modifiers_dont_count_for_moving_a_junction() {
    let mut v = view();
    let mut m = v.model().clone();
    let e = m.add_species(pt(200.0, -200.0), 80.0, 36.0);
    m.find_reaction_mut("R4")
        .unwrap()
        .modifiers
        .push(Participant::new(e, 1.0));
    v.set_model(m);
    click(&mut v, 0.0, 0.0, Modifiers::NONE);
    click(&mut v, 400.0, 0.0, Modifiers::SHIFT);
    drag(&mut v, (0.0, 0.0), (0.0, 30.0), Modifiers::NONE);
    assert_eq!(junction(&v, "R4"), pt(200.0, 30.0));
}

#[test]
fn a_linear_junction_keeps_its_place_along_the_line() {
    // Reported: dragging one end of a Linear reaction slid its junction, and
    // the modifier arc ending there, into a node.
    use crate::render::draw::effective_junction;
    let mut v = view();
    let mut m = v.model().clone();
    let e = m.add_species(pt(200.0, -200.0), 80.0, 36.0);
    let r = m.find_reaction_mut("R4").unwrap();
    r.mode = ReactionMode::Linear;
    r.modifiers.push(Participant::new(e, 1.0));
    v.set_model(m);
    // B (400,0) dragged to (150,300): the junction stays halfway along.
    drag(&mut v, (400.0, 0.0), (150.0, 300.0), Modifiers::NONE);
    let m = v.model();
    let r = m.find_reaction("R4").unwrap();
    let j = effective_junction(m, r);
    assert!(j.distance(pt(75.0, 150.0)) < 1e-9, "junction at {j:?}");
    // Undo puts the stored junction back exactly.
    assert_eq!(v.undo_description(), Some("Move"));
    v.undo();
    assert_eq!(junction(&v, "R4"), pt(200.0, 0.0));
    // Esc during a drag does too.
    down(&mut v, 400.0, 0.0, Modifiers::NONE);
    move_to(&mut v, 100.0, 100.0);
    v.handle(&ViewEvent::Cancel);
    assert_eq!(junction(&v, "R4"), pt(200.0, 0.0));
}

#[test]
fn locked_species_stay_put() {
    let mut v = view();
    let mut m = v.model().clone();
    m.find_species_mut("S2").unwrap().locked = true;
    v.set_model(m);
    v.select_all();
    drag(&mut v, (0.0, 0.0), (20.0, 20.0), Modifiers::NONE);
    assert_eq!(at(&v, "S1"), pt(20.0, 20.0));
    assert_eq!(at(&v, "S2"), pt(400.0, 0.0), "locked");
    assert_eq!(at(&v, "S3"), pt(420.0, 220.0));
    // R4 has the locked B, so its junction stays; R5 (A → C) moves.
    assert_eq!(junction(&v, "R4"), pt(200.0, 0.0));
    assert_eq!(junction(&v, "R5"), pt(220.0, 120.0));
    // Dragging only a locked node records nothing.
    v.handle(&ViewEvent::Cancel);
    click(&mut v, 400.0, 0.0, Modifiers::NONE);
    let steps = v.undo.len();
    drag(&mut v, (400.0, 0.0), (500.0, 0.0), Modifiers::NONE);
    assert_eq!(at(&v, "S2"), pt(400.0, 0.0));
    assert_eq!(v.undo.len(), steps);
}

#[test]
fn positions_are_original_plus_delta() {
    let mut v = view();
    down(&mut v, 0.0, 0.0, Modifiers::NONE);
    for i in 0..1000 {
        let f = f64::from(i) * 0.1;
        move_to(&mut v, f.sin() * 37.3, f.cos() * 11.9);
    }
    move_to(&mut v, 0.0, 0.0);
    // Back exactly where it started: no accumulated rounding.
    assert_eq!(at(&v, "S1"), pt(0.0, 0.0));
    up(&mut v, 0.0, 0.0);
    assert!(
        !v.can_undo(),
        "a drag that ends where it started records nothing"
    );
}

#[test]
fn drag_deltas_are_in_world_units() {
    let mut v = view();
    v.set_view(2.0, pt(100.0, 100.0));
    // A is at screen (100,100); dragging 40 screen points moves it 20 units.
    drag(&mut v, (100.0, 100.0), (140.0, 100.0), Modifiers::NONE);
    assert_eq!(at(&v, "S1"), pt(20.0, 0.0));
}

#[test]
fn stored_handles_follow_their_end() {
    let mut v = view();
    let mut m = v.model().clone();
    {
        let r = m.find_reaction_mut("R4").unwrap();
        r.mode = ReactionMode::Bezier;
        for p in r.reactants.iter_mut().chain(&mut r.products) {
            p.ctrl_pts_set = true;
            p.ctrl1 = pt(1.0, 1.0);
            p.ctrl2 = pt(2.0, 2.0);
        }
    }
    v.set_model(m);
    // Move A only: R4's junction stays. A's handle on its reactant leg is
    // ctrl1 (species side); ctrl2 (junction side) stays. B's leg is untouched.
    drag(&mut v, (0.0, 0.0), (10.0, 0.0), Modifiers::NONE);
    let r = v.model().find_reaction("R4").unwrap();
    assert_eq!(
        (r.reactants[0].ctrl1, r.reactants[0].ctrl2),
        (pt(11.0, 1.0), pt(2.0, 2.0))
    );
    assert_eq!(
        (r.products[0].ctrl1, r.products[0].ctrl2),
        (pt(1.0, 1.0), pt(2.0, 2.0))
    );
    // A drag that moves handles is undone as a snapshot: everything comes back.
    assert_eq!(v.undo_description(), Some("Move"));
    v.undo();
    let r = v.model().find_reaction("R4").unwrap();
    assert_eq!(r.reactants[0].ctrl1, pt(1.0, 1.0));
    assert_eq!(at(&v, "S1"), pt(0.0, 0.0));

    // Move A and B: the junction moves, so every handle follows.
    v.select_all();
    // Press away from the handles: R4 is selected and Bézier, so they can be hit.
    drag(&mut v, (30.0, 10.0), (30.0, 15.0), Modifiers::NONE);
    let r = v.model().find_reaction("R4").unwrap();
    for p in r.reactants.iter().chain(&r.products) {
        assert_eq!((p.ctrl1, p.ctrl2), (pt(1.0, 6.0), pt(2.0, 7.0)));
    }
}

#[test]
fn smooth_junction_drag_moves_the_inner_handles() {
    let mut v = view();
    let mut m = v.model().clone();
    {
        let r = m.find_reaction_mut("R4").unwrap();
        r.mode = ReactionMode::Bezier;
        r.junction_smooth = true;
        r.reactants[0].ctrl_pts_set = true;
        r.reactants[0].ctrl1 = pt(50.0, 0.0);
        r.reactants[0].ctrl2 = pt(175.0, 0.0);
        r.products[0].ctrl_pts_set = true;
        r.products[0].ctrl1 = pt(225.0, 0.0);
        r.products[0].ctrl2 = pt(350.0, 0.0);
    }
    v.set_model(m);
    drag(&mut v, (200.0, 0.0), (200.0, 40.0), Modifiers::NONE);
    let r = v.model().find_reaction("R4").unwrap();
    assert_eq!(
        r.reactants[0].ctrl2,
        pt(175.0, 40.0),
        "inner reactant handle"
    );
    assert_eq!(r.products[0].ctrl1, pt(225.0, 40.0), "inner product handle");
    assert_eq!(r.reactants[0].ctrl1, pt(50.0, 0.0), "outer handles stay");
    v.undo();
    assert_eq!(
        v.model().find_reaction("R4").unwrap().reactants[0].ctrl2,
        pt(175.0, 0.0)
    );
}

// ----- Delete and undo (S §7.2, §9) -----------------------------------------

#[test]
fn delete_cascades_and_is_one_undo_step() {
    let mut v = view();
    click(&mut v, 0.0, 0.0, Modifiers::NONE);
    let before = v.model().clone();
    let r = v.delete_selection();
    assert!(r.model_changed && r.selection_changed);
    // Deleting A removes both reactions that use it.
    assert!(v.model().find_species("S1").is_none());
    assert!(v.model().reactions().is_empty());
    assert_eq!(v.undo_description(), Some("Delete"));
    v.undo();
    assert_eq!(
        v.model().species(),
        before
            .species()
            .iter()
            .map(|s| {
                let mut s = s.clone();
                s.selected = false;
                s
            })
            .collect::<Vec<SpeciesNode>>()
            .as_slice()
    );
    assert_eq!(v.model().reactions().len(), 2);
    v.redo();
    assert!(v.model().find_species("S1").is_none());
}

#[test]
fn delete_with_nothing_selected_does_nothing() {
    let mut v = view();
    assert_eq!(v.delete_selection(), ViewResponse::default());
    assert!(!v.can_undo());
}

#[test]
fn delete_a_species_and_a_reaction_together() {
    let mut v = view();
    click(&mut v, 400.0, 200.0, Modifiers::NONE);
    click(&mut v, 100.0, 0.0, Modifiers::SHIFT);
    v.delete_selection();
    // C took R5 with it; R4 was selected too.
    assert!(v.model().reactions().is_empty());
    assert_eq!(v.model().species().len(), 2);
}

#[test]
fn undo_and_redo_a_move() {
    let mut v = view();
    drag(&mut v, (0.0, 0.0), (30.0, 40.0), Modifiers::NONE);
    assert_eq!(v.undo_description(), Some("Move"));
    let r = v.undo();
    assert!(r.model_changed && r.selection_changed);
    assert_eq!(at(&v, "S1"), pt(0.0, 0.0));
    assert!(!v.has_selection(), "undo clears the selection");
    assert_eq!(v.redo_description(), Some("Move"));
    v.redo();
    assert_eq!(at(&v, "S1"), pt(30.0, 40.0));
    // Nothing left to redo: nothing happens.
    assert_eq!(v.redo(), ViewResponse::default());
}

#[test]
fn undo_during_a_drag_cancels_it_first() {
    let mut v = view();
    drag(&mut v, (0.0, 0.0), (30.0, 0.0), Modifiers::NONE);
    down(&mut v, 400.0, 0.0, Modifiers::NONE);
    move_to(&mut v, 450.0, 0.0);
    v.undo();
    assert_eq!(at(&v, "S2"), pt(400.0, 0.0));
    assert_eq!(at(&v, "S1"), pt(0.0, 0.0));
    assert!(!v.is_busy());
}

#[test]
fn opening_a_model_clears_the_history() {
    let mut v = view();
    drag(&mut v, (0.0, 0.0), (30.0, 0.0), Modifiers::NONE);
    v.set_model(Model::new());
    assert!(!v.can_undo());
}

#[test]
fn other_buttons_are_left_to_the_app() {
    let mut v = view();
    let r = v.handle(&ViewEvent::PointerDown {
        pos: pt(0.0, 0.0),
        button: Button::Middle,
        modifiers: Modifiers::NONE,
    });
    assert_eq!(r, ViewResponse::default());
    assert!(!v.is_busy());
}

#[test]
fn select_all_species_or_all_reactions() {
    let mut v = view();
    v.select_all_species();
    assert_eq!(v.selection().0.len(), 3);
    assert!(v.selection().1.is_empty());
    v.select_all_reactions();
    assert!(
        v.selection().0.is_empty(),
        "the species are no longer selected"
    );
    assert_eq!(v.selection().1.len(), 2);
    let r = v.select_all_reactions();
    assert!(!r.selection_changed);
    v.select_all();
    assert_eq!((v.selection().0.len(), v.selection().1.len()), (3, 2));
}
