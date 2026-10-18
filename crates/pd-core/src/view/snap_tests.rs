//! Snapping to the grid (S §7.6): dragged and new items land on grid points,
//! unless Alt is held. Driven with made-up events; screen = world.

use super::*;
use crate::model::Participant;

/// S1 at (3, 7), S2 at (103, 7), a reaction S1 → S2 with its junction at
/// (55, 41), a text T at (11, 213) and a compartment C with its box's
/// top-left corner at (-38, -59). Snapping on.
fn view() -> DiagramView {
    let mut m = Model::new();
    m.push_species(crate::model::SpeciesNode::new("S1", pt(3.0, 7.0)))
        .unwrap();
    m.push_species(crate::model::SpeciesNode::new("S2", pt(103.0, 7.0)))
        .unwrap();
    let mut r = crate::model::Reaction::new("R", pt(55.0, 41.0));
    r.reactants.push(Participant::new("S1", 1.0));
    r.products.push(Participant::new("S2", 1.0));
    m.push_reaction(r).unwrap();
    m.push_text(crate::model::TextItem::new("T", pt(11.0, 213.0), "note"))
        .unwrap();
    let mut c = crate::model::Compartment::new("C");
    c.frame = Some(crate::model::Frame {
        // 410 wide, so snapping its centre would not put its corner on the
        // grid.
        center: pt(167.0, 41.0),
        width: 410.0,
        height: 200.0,
    });
    m.compartments.push(c);
    let mut v = DiagramView::new(m);
    v.snap_to_grid = true;
    v
}

fn press(v: &mut DiagramView, at: Pt) {
    v.handle(&ViewEvent::PointerDown {
        pos: at,
        button: Button::Primary,
        modifiers: Modifiers::NONE,
    });
}

fn release(v: &mut DiagramView, at: Pt) {
    v.handle(&ViewEvent::PointerUp {
        pos: at,
        button: Button::Primary,
    });
}

fn drag(v: &mut DiagramView, from: Pt, to: Pt) {
    press(v, from);
    v.handle(&ViewEvent::PointerMove { pos: to });
    release(v, to);
}

fn centre(v: &DiagramView, id: &str) -> Pt {
    v.model().find_species(id).unwrap().center
}

#[test]
fn grid_points_are_every_20_units() {
    assert_eq!(snap_point(pt(9.9, 10.1)), pt(0.0, 20.0));
    assert_eq!(snap_point(pt(-9.9, -10.1)), pt(0.0, -20.0));
    assert_eq!(snap_point(pt(-0.1, 0.1)), pt(0.0, 0.0));
    assert!(snap_point(pt(-0.1, 0.0)).x.is_sign_positive(), "no -0");
}

#[test]
fn the_species_grabbed_lands_on_the_grid_and_the_rest_keep_their_places() {
    let mut v = view();
    v.select_item(&Target::Species("S1".into()), false);
    v.select_item(&Target::Species("S2".into()), true);
    // S1 grabbed 7 left and 3 up of its centre, moved by (40, 37): its
    // centre would be at (43, 44), so it goes to (40, 40).
    drag(&mut v, pt(10.0, 10.0), pt(50.0, 47.0));
    assert_eq!(centre(&v, "S1"), pt(40.0, 40.0));
    // S2 moves by the same amount, keeping its distance from S1.
    assert_eq!(centre(&v, "S2"), pt(140.0, 40.0));
    v.undo();
    assert_eq!(centre(&v, "S1"), pt(3.0, 7.0));
    assert_eq!(centre(&v, "S2"), pt(103.0, 7.0));
}

#[test]
fn a_click_moves_nothing() {
    let mut v = view();
    press(&mut v, pt(10.0, 10.0));
    release(&mut v, pt(10.0, 10.0));
    assert_eq!(centre(&v, "S1"), pt(3.0, 7.0));
    assert!(!v.can_undo());
}

#[test]
fn alt_drags_freely_at_once() {
    let mut v = view();
    press(&mut v, pt(10.0, 10.0));
    v.handle(&ViewEvent::PointerMove {
        pos: pt(50.0, 47.0),
    });
    assert_eq!(centre(&v, "S1"), pt(40.0, 40.0));
    // Alt pressed during the drag: it follows the pointer exactly.
    v.set_alt_held(true);
    assert_eq!(centre(&v, "S1"), pt(43.0, 44.0));
    v.handle(&ViewEvent::PointerMove {
        pos: pt(51.0, 47.0),
    });
    assert_eq!(centre(&v, "S1"), pt(44.0, 44.0));
    // Let go of Alt: back on the grid.
    v.set_alt_held(false);
    assert_eq!(centre(&v, "S1"), pt(40.0, 40.0));
    release(&mut v, pt(51.0, 47.0));
    assert_eq!(centre(&v, "S1"), pt(40.0, 40.0));
}

#[test]
fn without_snapping_a_drag_follows_the_pointer() {
    let mut v = view();
    v.snap_to_grid = false;
    drag(&mut v, pt(10.0, 10.0), pt(50.0, 47.0));
    assert_eq!(centre(&v, "S1"), pt(43.0, 44.0));
}

#[test]
fn junctions_texts_and_compartments_snap_too() {
    // A junction.
    let mut v = view();
    drag(&mut v, pt(55.0, 41.0), pt(68.0, 50.0));
    assert_eq!(
        v.model().find_reaction("R").unwrap().junction,
        pt(60.0, 60.0)
    );
    // A text, by its top-left corner.
    let mut v = view();
    drag(&mut v, pt(15.0, 220.0), pt(40.0, 230.0));
    assert_eq!(v.model().find_text("T").unwrap().position, pt(40.0, 220.0));
    // A compartment, grabbed by its left border; its box's top-left corner snaps.
    let mut v = view();
    let name = pt(-38.0, 100.0);
    assert_eq!(v.target_at(name), Target::Compartment("C".into()));
    drag(&mut v, name, name + pt(6.0, 3.0));
    let f = v.model().find_compartment("C").unwrap().frame.unwrap();
    assert_eq!(f.min(), pt(-40.0, -60.0));
}

#[test]
fn new_items_are_placed_on_the_grid() {
    let mut v = view();
    v.set_tool(Tool::AddSpecies);
    press(&mut v, pt(307.0, 291.0));
    let new = v.model().species().last().unwrap();
    assert_eq!(new.center, pt(300.0, 300.0));
    v.set_tool(Tool::AddText);
    press(&mut v, pt(-13.0, 409.0));
    let t = v.model().texts().last().unwrap();
    assert_eq!(t.position, pt(-20.0, 400.0));
    // A compartment drawn from corner to corner.
    v.set_tool(Tool::AddCompartment);
    drag(&mut v, pt(503.0, 2.0), pt(689.0, 197.0));
    let f = v.model().compartments.last().unwrap().frame.unwrap();
    assert_eq!((f.min(), f.max()), (pt(500.0, 0.0), pt(680.0, 200.0)));
    // With Alt held, where the click was.
    v.set_alt_held(true);
    v.set_tool(Tool::AddSpecies);
    press(&mut v, pt(307.0, 291.0));
    assert_eq!(v.model().species().last().unwrap().center, pt(307.0, 291.0));
}

#[test]
fn a_locked_species_grabbed_doesnt_snap_the_rest() {
    // S1 is locked, so it stays; S2, selected with it, moves as the pointer
    // does, since nothing grabbed moves to snap.
    let mut v = view();
    v.model.find_species_mut("S1").unwrap().locked = true;
    v.select_item(&Target::Species("S1".into()), false);
    v.select_item(&Target::Species("S2".into()), true);
    drag(&mut v, pt(10.0, 10.0), pt(50.0, 47.0));
    assert_eq!(centre(&v, "S1"), pt(3.0, 7.0));
    assert_eq!(centre(&v, "S2"), pt(143.0, 44.0));
}

#[test]
fn a_species_corner_lands_on_the_grid() {
    use crate::render::constants::HALO_OUTSET;
    let mut v = view();
    v.select_item(&Target::Species("S1".into()), false);
    // S1 is 80 × 36 at (3, 7): its bottom-right corner is at (43, 25), its
    // handle a little outside. Dragged by (10, 10), the corner would be at
    // (53, 35): it goes to (60, 40), the centre staying.
    let handle = pt(43.0 + HALO_OUTSET, 25.0 + HALO_OUTSET);
    drag(&mut v, handle, handle + pt(10.0, 10.0));
    let s = v.model().find_species("S1").unwrap();
    assert_eq!(s.center, pt(3.0, 7.0));
    assert_eq!((s.width, s.height), (114.0, 66.0));
    assert_eq!(s.center + pt(s.width / 2.0, s.height / 2.0), pt(60.0, 40.0));
    // With Alt, as the pointer goes.
    v.undo();
    v.select_item(&Target::Species("S1".into()), false);
    v.set_alt_held(true);
    drag(&mut v, handle, handle + pt(10.0, 10.0));
    let s = v.model().find_species("S1").unwrap();
    assert_eq!((s.width, s.height), (100.0, 56.0));
}

#[test]
fn a_compartment_corner_lands_on_the_grid() {
    let mut v = view();
    v.select_item(&Target::Compartment("C".into()), false);
    // Its box runs from (-38, -59) to (372, 141); the bottom-right corner,
    // dragged by (9, 4), goes to (380, 140). The other corner stays.
    drag(&mut v, pt(372.0, 141.0), pt(381.0, 145.0));
    let f = v.model().find_compartment("C").unwrap().frame.unwrap();
    assert_eq!((f.min(), f.max()), (pt(-38.0, -59.0), pt(380.0, 140.0)));
    // Alt pressed during the drag: as the pointer goes, at once.
    v.undo();
    v.select_item(&Target::Compartment("C".into()), false);
    press(&mut v, pt(372.0, 141.0));
    v.handle(&ViewEvent::PointerMove {
        pos: pt(381.0, 145.0),
    });
    v.set_alt_held(true);
    let f = v.model().find_compartment("C").unwrap().frame.unwrap();
    assert_eq!(f.max(), pt(381.0, 145.0));
    release(&mut v, pt(381.0, 145.0));
}

#[test]
fn arrow_keys_go_from_grid_line_to_grid_line() {
    let mut v = view();
    v.select_item(&Target::Species("S1".into()), false);
    v.select_item(&Target::Species("S2".into()), true);
    let right = pt(1.0, 0.0);
    // S1's centre is at x 3: first to the next line, 20, then one line at a
    // time; S2 keeps its distance.
    v.nudge_key(right, false, false);
    assert_eq!(centre(&v, "S1"), pt(20.0, 7.0), "across, it stays");
    assert_eq!(centre(&v, "S2"), pt(120.0, 7.0));
    v.nudge_key(right, false, false);
    assert_eq!(centre(&v, "S1"), pt(40.0, 7.0));
    // Shift: five lines.
    v.nudge_key(right, true, false);
    assert_eq!(centre(&v, "S1"), pt(140.0, 7.0));
    v.nudge_key(pt(-1.0, 0.0), false, false);
    assert_eq!(centre(&v, "S1"), pt(120.0, 7.0));
    // Up from y 7: the line at 0.
    v.nudge_key(pt(0.0, -1.0), false, false);
    assert_eq!(centre(&v, "S1"), pt(120.0, 0.0));
    // Alt: the fine steps, 1 and 10.
    v.nudge_key(right, false, true);
    v.nudge_key(right, true, true);
    assert_eq!(centre(&v, "S1"), pt(131.0, 0.0));
    // One undo step for the run, as before.
    v.undo();
    assert_eq!(centre(&v, "S1"), pt(3.0, 7.0));
    // Without snapping, the fine steps.
    v.snap_to_grid = false;
    v.select_item(&Target::Species("S1".into()), false);
    v.nudge_key(right, false, false);
    v.nudge_key(right, true, false);
    assert_eq!(centre(&v, "S1"), pt(14.0, 7.0));
}

#[test]
fn arrow_keys_take_a_junction_to_the_grid_when_it_leads() {
    let mut v = view();
    v.select_item(&Target::Junction("R".into()), false);
    v.nudge_key(pt(0.0, 1.0), false, false);
    assert_eq!(
        v.model().find_reaction("R").unwrap().junction,
        pt(55.0, 60.0)
    );
}
