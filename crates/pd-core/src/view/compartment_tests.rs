//! Compartments in the view (S §7.9): drawing, membership by position,
//! dragging with contents, resizing, deleting, selecting, auto-layout and
//! older files, driven with made-up events.

use super::*;
use crate::layout::LayoutParams;
use crate::model::{Compartment, Frame, Participant};

/// A at (0,0) → R3 → B at (300,0), both in the default compartment. Screen =
/// world.
fn view() -> DiagramView {
    let mut m = Model::new();
    let a = m.add_species(pt(0.0, 0.0), 80.0, 36.0);
    let b = m.add_species(pt(300.0, 0.0), 80.0, 36.0);
    let r = m.add_reaction(pt(150.0, 0.0));
    let r = m.find_reaction_mut(&r).unwrap();
    r.reactants.push(Participant::new(a, 1.0));
    r.products.push(Participant::new(b, 1.0));
    DiagramView::new(m)
}

fn press(v: &mut DiagramView, at: Pt, modifiers: Modifiers) {
    v.handle(&ViewEvent::PointerDown {
        pos: at,
        button: Button::Primary,
        modifiers,
    });
}

fn drag(v: &mut DiagramView, from: Pt, to: Pt) {
    press(v, from, Modifiers::NONE);
    v.handle(&ViewEvent::PointerMove {
        pos: (from + to) / 2.0,
    });
    v.handle(&ViewEvent::PointerUp {
        pos: to,
        button: Button::Primary,
    });
}

fn comp_of(v: &DiagramView, species: &str) -> String {
    v.model().find_species(species).unwrap().compartment.clone()
}

fn frame(v: &DiagramView, id: &str) -> Frame {
    v.model().find_compartment(id).unwrap().frame.unwrap()
}

/// Draws a compartment around A only, from (−100,−100) to (100,100).
fn draw_around_a(v: &mut DiagramView) -> Id {
    v.set_tool(Tool::AddCompartment);
    drag(v, pt(-100.0, -100.0), pt(100.0, 100.0));
    v.model().compartments.last().unwrap().id.clone()
}

#[test]
fn drawing_a_box_makes_a_compartment_and_takes_in_the_species_inside() {
    let mut v = view();
    let c = draw_around_a(&mut v);
    assert_eq!(c, "C4");
    let f = frame(&v, &c);
    assert_eq!((f.center, f.width, f.height), (pt(0.0, 0.0), 200.0, 200.0));
    assert_eq!(comp_of(&v, "S1"), c);
    assert_eq!(comp_of(&v, "S2"), "");
    assert_eq!(v.tool(), Tool::Select);
    assert_eq!(v.selected_compartment_ids(), std::slice::from_ref(&c));
    assert_eq!(v.undo_description(), Some("Add Compartment"));
    v.undo();
    assert!(v.model().compartments.is_empty());
    assert_eq!(comp_of(&v, "S1"), "");
    // A click makes the default size, centred on it.
    v.set_tool(Tool::AddCompartment);
    drag(&mut v, pt(500.0, 0.0), pt(501.0, 1.0));
    let id = v.model().compartments[0].id.clone();
    let f = frame(&v, &id);
    assert_eq!(
        (f.center, f.width, f.height),
        (pt(500.0, 0.0), 300.0, 200.0)
    );
}

#[test]
fn a_species_dragged_in_or_out_changes_compartment() {
    let mut v = view();
    let c = draw_around_a(&mut v);
    v.handle(&ViewEvent::Cancel);
    // B into the box.
    drag(&mut v, pt(300.0, 0.0), pt(50.0, 50.0));
    assert_eq!(comp_of(&v, "S2"), c);
    v.undo();
    assert_eq!(comp_of(&v, "S2"), "", "undo puts it back, compartment too");
    // A out of it.
    drag(&mut v, pt(0.0, 0.0), pt(0.0, 300.0));
    assert_eq!(comp_of(&v, "S1"), "");
    // A nudge counts too.
    v.select_item(&Target::Species("S1".into()), false);
    for _ in 0..300 {
        v.nudge(pt(0.0, -1.0));
    }
    assert_eq!(comp_of(&v, "S1"), c);
}

#[test]
fn dragging_a_compartment_carries_what_is_inside() {
    let mut v = view();
    let c = draw_around_a(&mut v);
    // A nested one, around nothing, inside the first.
    v.set_tool(Tool::AddCompartment);
    drag(&mut v, pt(30.0, 30.0), pt(90.0, 90.0));
    let inner = v.model().compartments.last().unwrap().id.clone();
    v.handle(&ViewEvent::Cancel);
    // Grab the outer box by its left edge and move it right by 100.
    assert_eq!(v.target_at(pt(-100.0, 0.0)), Target::Compartment(c.clone()));
    drag(&mut v, pt(-100.0, 0.0), pt(0.0, 0.0));
    assert_eq!(frame(&v, &c).center, pt(100.0, 0.0));
    assert_eq!(frame(&v, &inner).center, pt(160.0, 60.0));
    assert_eq!(v.model().find_species("S1").unwrap().center, pt(100.0, 0.0));
    assert_eq!(v.model().find_species("S2").unwrap().center, pt(300.0, 0.0));
    assert_eq!(
        comp_of(&v, "S1"),
        c,
        "carried species keep their compartment"
    );
    v.undo();
    assert_eq!(frame(&v, &c).center, pt(0.0, 0.0));
    assert_eq!(v.model().find_species("S1").unwrap().center, pt(0.0, 0.0));
}

#[test]
fn resizing_lets_species_in_and_out() {
    let mut v = view();
    let c = draw_around_a(&mut v);
    // Selected after drawing: drag the bottom-right corner out past B.
    assert_eq!(
        v.target_at(pt(100.0, 100.0)),
        Target::CompartmentCorner(c.clone(), 2)
    );
    drag(&mut v, pt(100.0, 100.0), pt(400.0, 100.0));
    assert_eq!(frame(&v, &c).max(), pt(400.0, 100.0));
    assert_eq!(comp_of(&v, "S2"), c);
    assert_eq!(v.undo_description(), Some("Resize Compartment"));
    v.undo();
    assert_eq!(comp_of(&v, "S2"), "");
    // Never smaller than the smallest box.
    v.select_item(&Target::Compartment(c.clone()), false);
    drag(&mut v, pt(100.0, 100.0), pt(-500.0, -500.0));
    let f = frame(&v, &c);
    assert_eq!((f.width, f.height), (40.0, 40.0));
    assert_eq!(f.min(), pt(-100.0, -100.0), "the opposite corner stays");
    assert_eq!(comp_of(&v, "S1"), "", "A is now outside");
}

#[test]
fn clicks_inside_reach_what_is_there() {
    let mut v = view();
    let c = draw_around_a(&mut v);
    v.handle(&ViewEvent::Cancel);
    // Empty space inside: a rubber band, not the compartment.
    assert_eq!(v.target_at(pt(60.0, 60.0)), Target::Empty);
    assert_eq!(v.target_at(pt(0.0, 0.0)), Target::Species("S1".into()));
    // The name, inside the top-left corner, selects it.
    assert_eq!(
        v.target_at(pt(-88.0, -85.0)),
        Target::Compartment(c.clone())
    );
    // A band inside it selects A, not the compartment; one around it selects both.
    drag(&mut v, pt(-60.0, -60.0), pt(60.0, 60.0));
    assert_eq!(v.selection().0, ["S1"]);
    assert!(v.selected_compartment_ids().is_empty());
    drag(&mut v, pt(-150.0, -150.0), pt(150.0, 150.0));
    assert_eq!(v.selected_compartment_ids(), std::slice::from_ref(&c));
    // Delete keeps the species, in the default compartment.
    v.select_item(&Target::Compartment(c.clone()), false);
    v.delete_selection();
    assert!(v.model().compartments.is_empty());
    assert_eq!(comp_of(&v, "S1"), "");
    assert_eq!(v.model().species().len(), 2);
}

#[test]
fn auto_layout_keeps_compartments_and_refits_the_boxes() {
    let mut v = view();
    let c = draw_around_a(&mut v);
    v.start_layout(
        LayoutParams {
            randomize: true,
            ..LayoutParams::default()
        },
        7,
    );
    while v.is_laying_out() {
        v.layout_step(100);
    }
    assert_eq!(comp_of(&v, "S1"), c);
    assert_eq!(comp_of(&v, "S2"), "");
    assert!(frame(&v, &c).contains(v.model().find_species("S1").unwrap().center));
}

#[test]
fn older_files_get_boxes_around_their_species() {
    let mut m = Model::new();
    m.add_species(pt(0.0, 0.0), 80.0, 36.0);
    m.find_species_mut("S1").unwrap().compartment = "cell".into();
    m.compartments.push(Compartment::new("cell"));
    m.compartments.push(Compartment::new("defaultCompartment"));
    let mut v = DiagramView::default();
    v.set_model(m);
    assert!(frame(&v, "cell").contains(pt(0.0, 0.0)));
    assert!(
        v.model()
            .find_compartment("defaultCompartment")
            .unwrap()
            .frame
            .is_none()
    );
    // Files without compartments are untouched by moves.
    let mut v = view();
    drag(&mut v, pt(0.0, 0.0), pt(10.0, 10.0));
    assert_eq!(comp_of(&v, "S1"), "");
}

#[test]
fn a_junction_inside_a_dragged_compartment_moves_with_it() {
    let mut v = view();
    // A box around A and R3's junction, but not B.
    v.set_tool(Tool::AddCompartment);
    drag(&mut v, pt(-100.0, -100.0), pt(200.0, 100.0));
    let c = v.model().compartments[0].id.clone();
    v.handle(&ViewEvent::Cancel);
    // Make R3 a Bézier with a smooth junction, so its handles must follow too.
    v.select_item(&Target::Leg("R3".into()), false);
    v.set_bezier();
    v.toggle_smooth();
    v.handle(&ViewEvent::Cancel);
    let s2_outer = v.model().find_reaction("R3").unwrap().products[0].ctrl2;
    drag(&mut v, pt(-100.0, 0.0), pt(-100.0, 50.0));
    assert_eq!(frame(&v, &c).center, pt(50.0, 50.0));
    let r = v.model().find_reaction("R3").unwrap();
    assert_eq!(r.junction, pt(150.0, 50.0));
    assert_eq!(v.model().find_species("S2").unwrap().center, pt(300.0, 0.0));
    // The inner handles were lined up round the new junction; B didn't move,
    // so its outer handle stays where it was.
    let mut again = v.model().clone();
    crate::bezier::apply_smooth_inner_handles(&mut again, "R3");
    assert_eq!(again.find_reaction("R3"), v.model().find_reaction("R3"));
    assert_eq!(r.products[0].ctrl2, s2_outer);
}

#[test]
fn a_hidden_box_is_not_drawn_or_hit_and_keeps_its_species() {
    use crate::render::{draw::Shown, svg::model_to_svg};
    let mut v = view();
    let c = draw_around_a(&mut v);
    v.set_tool(Tool::AddCompartment);
    drag(&mut v, pt(200.0, -100.0), pt(400.0, 100.0));
    let other = v.model().compartments.last().unwrap().id.clone();
    // Each box drawn is one fill of the compartment colour.
    let boxes = |v: &DiagramView| {
        model_to_svg(v.model(), 1.0, Shown::ALL)
            .matches("fill=\"#F0F6EA\"")
            .count()
    };
    assert_eq!(boxes(&v), 2);

    v.select_item(&Target::Compartment(c.clone()), false);
    v.set_compartment_hidden(true);
    assert_eq!(v.undo_description(), Some("Hide Compartment"));
    let hidden = v.model().find_compartment(&c).unwrap();
    assert!(hidden.frame.is_some() && hidden.drawn_frame().is_none());
    assert_eq!(boxes(&v), 1, "only the other one is drawn");
    // Still selected, its corners can't be dragged either.
    let before = frame(&v, &c);
    drag(&mut v, pt(-100.0, -100.0), pt(-150.0, -150.0));
    assert_eq!(frame(&v, &c), before);

    // Its border is not there to click, and Select All leaves it out.
    v.handle(&ViewEvent::Cancel);
    drag(&mut v, pt(-100.0, 0.0), pt(-100.0, 0.0));
    assert!(v.selected_compartment_ids().is_empty());
    v.select_all();
    assert_eq!(v.selected_compartment_ids(), std::slice::from_ref(&other));

    // Its species keeps it when moved, unless it lands in a box that is shown.
    v.handle(&ViewEvent::Cancel);
    drag(&mut v, pt(0.0, 0.0), pt(0.0, 300.0));
    assert_eq!(comp_of(&v, "S1"), c);
    drag(&mut v, pt(0.0, 300.0), pt(250.0, 50.0));
    assert_eq!(comp_of(&v, "S1"), other);

    // Shown again where it was; undo hides it again.
    v.select_item(&Target::Compartment(c.clone()), false);
    v.set_compartment_hidden(false);
    assert_eq!(v.undo_description(), Some("Show Compartment"));
    assert_eq!(
        v.model().find_compartment(&c).unwrap().drawn_frame(),
        Some(frame(&v, &c))
    );
    v.undo();
    assert!(v.model().find_compartment(&c).unwrap().hidden);
}

#[test]
fn renamed_to_the_default_compartment_its_box_goes() {
    let mut v = view();
    let c = draw_around_a(&mut v);
    v.rename_compartment(&c, "defaultCompartment").unwrap();
    let d = v.model().find_compartment("defaultCompartment").unwrap();
    assert_eq!(
        d.drawn_frame(),
        None,
        "the default compartment is never drawn"
    );
    assert_eq!(comp_of(&v, "S1"), "defaultCompartment");
    // Hiding or showing it changes nothing.
    v.select_item(&Target::Compartment("defaultCompartment".into()), false);
    v.set_compartment_hidden(true);
    assert!(
        !v.model()
            .find_compartment("defaultCompartment")
            .unwrap()
            .hidden
    );
}

#[test]
fn a_fill_can_be_left_out() {
    use crate::model::Rgba;
    use crate::render::draw::{compartment_colours, species_colours};
    let mut v = view();
    let c = draw_around_a(&mut v);
    v.select_item(&Target::Compartment(c.clone()), false);
    v.apply_colour(ColourField::CompartmentFill, Rgba::TRANSPARENT);
    assert_eq!(v.undo_description(), Some("Compartment Colour"));
    let comp = v.model().find_compartment(&c).unwrap();
    assert_eq!(compartment_colours(comp).0, Rgba::TRANSPARENT);
    assert_eq!(
        v.swatch_colour(ColourField::CompartmentFill),
        Some(Rgba::TRANSPARENT)
    );
    // Not the "use the default" sentinel, and the border stays.
    assert!(Rgba::TRANSPARENT.is_transparent());
    assert!(!Rgba::DEFAULT.is_transparent());
    assert!(!Rgba::rgb(1, 2, 3).is_transparent());
    let (_, border, _) = compartment_colours(comp);
    assert!(!border.is_transparent());
    // A species' fill too.
    v.select_item(&Target::Species("S1".into()), false);
    v.apply_colour(ColourField::Fill, Rgba::TRANSPARENT);
    let s = v.model().find_species("S1").unwrap();
    assert_eq!(species_colours(s, true).0, Rgba::TRANSPARENT);
    // The file keeps it as #FFFFFF00, not as the default.
    let text = crate::json::to_json(v.model());
    assert!(text.contains("\"fillColor\": \"#FFFFFF00\""), "{text}");
    let back = crate::json::from_json(&text).unwrap().model;
    assert_eq!(
        compartment_colours(back.find_compartment(&c).unwrap()).0,
        Rgba::TRANSPARENT
    );
    assert_eq!(
        species_colours(back.find_species("S1").unwrap(), true).0,
        Rgba::TRANSPARENT
    );
    // So does SBML, in the compartment's note.
    let xml = crate::sbml::export(v.model()).xml;
    assert!(xml.contains("fill=\"#FFFFFF00\""), "{xml}");
}

#[test]
fn a_compartment_name_can_be_bigger() {
    use crate::render::draw::{compartment_label_box, compartment_label_size};
    let mut v = view();
    let c = draw_around_a(&mut v);
    let comp = |v: &DiagramView| v.model().find_compartment(&c).unwrap().clone();
    assert_eq!(compartment_label_size(&comp(&v)), 12.0);
    let f = comp(&v).frame.unwrap();
    let (a, b) = compartment_label_box(&comp(&v), &f);
    v.select_item(&Target::Compartment(c.clone()), false);
    v.set_compartment_font_size(24.0);
    assert_eq!(v.undo_description(), Some("Font Size"));
    assert_eq!(compartment_label_size(&comp(&v)), 24.0);
    // The name's box grows from the corner, and a click there selects it.
    let (a2, b2) = compartment_label_box(&comp(&v), &f);
    assert_eq!(a2, a);
    assert!(b2.x > b.x && b2.y > b.y);
    let inside = pt((b.x + b2.x) / 2.0, (b.y + b2.y) / 2.0);
    v.model.clear_selection();
    assert_eq!(v.target_at(inside), Target::Compartment(c.clone()));
    // Bad sizes are ignored; undo goes back to 12.
    assert!(!v.set_compartment_font_size(f64::NAN).model_changed);
    v.undo();
    assert_eq!(compartment_label_size(&comp(&v)), 12.0);
    v.redo();
    // Files keep it: JSON in the style, SBML in the note.
    let back = crate::json::from_json(&crate::json::to_json(v.model()))
        .unwrap()
        .model;
    assert_eq!(
        compartment_label_size(back.find_compartment(&c).unwrap()),
        24.0
    );
    let xml = crate::sbml::export(v.model()).xml;
    assert!(xml.contains("fontSize=\"24\""), "{xml}");
    let opts = crate::antimony::ImportOptions {
        convert_catalytic: false,
        seed: 1,
    };
    let back = crate::sbml::import(&xml, &opts).unwrap().model;
    assert_eq!(
        compartment_label_size(back.find_compartment(&c).unwrap()),
        24.0
    );
    // Copy Style carries it to another compartment.
    v.set_tool(Tool::AddCompartment);
    drag(&mut v, pt(200.0, -100.0), pt(400.0, 100.0));
    let other = v.model().compartments.last().unwrap().id.clone();
    v.copy_style(&Target::Compartment(c.clone()));
    v.select_item(&Target::Compartment(other.clone()), false);
    v.paste_style();
    assert_eq!(
        compartment_label_size(v.model().find_compartment(&other).unwrap()),
        24.0
    );
}
