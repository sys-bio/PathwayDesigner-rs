//! Resizing species (S §7.10): the corner handles of the selection, the
//! size fields and Fit to Name; and their shapes (S §5.10), in the view and
//! in files. Driven with made-up events.

use super::*;
use crate::render::constants::{FIT_MARGIN, HALO_OUTSET, MIN_SPECIES_SIZE};
use crate::render::draw::width_for_name;

/// S1 at (0,0) and S2 at (300,0), both 80 × 36, and a null node. Screen =
/// world.
fn view() -> DiagramView {
    let mut m = Model::new();
    m.add_species(pt(0.0, 0.0), 80.0, 36.0);
    m.add_species(pt(300.0, 0.0), 80.0, 36.0);
    let mut null = crate::model::SpeciesNode::new("_null", pt(0.0, 300.0));
    null.is_null_node = true;
    null.width = 28.0;
    null.height = 28.0;
    m.push_species(null).unwrap();
    DiagramView::new(m)
}

fn press(v: &mut DiagramView, at: Pt) {
    v.handle(&ViewEvent::PointerDown {
        pos: at,
        button: Button::Primary,
        modifiers: Modifiers::NONE,
    });
}

fn drag(v: &mut DiagramView, from: Pt, to: Pt) {
    press(v, from);
    v.handle(&ViewEvent::PointerMove { pos: to });
    v.handle(&ViewEvent::PointerUp {
        pos: to,
        button: Button::Primary,
    });
}

fn size(v: &DiagramView, id: &str) -> (f64, f64) {
    let s = v.model().find_species(id).unwrap();
    (s.width, s.height)
}

/// The bottom-right handle of S1 as it is (80 × 36 at the origin).
const S1_BOTTOM_RIGHT: Pt = pt(40.0 + HALO_OUTSET, 18.0 + HALO_OUTSET);

#[test]
fn a_corner_resizes_about_the_centre() {
    let mut v = view();
    // No handles until it is selected.
    assert!(!matches!(
        v.target_at(S1_BOTTOM_RIGHT),
        Target::SpeciesCorner(..)
    ));
    v.select_item(&Target::Species("S1".into()), false);
    assert_eq!(
        v.target_at(S1_BOTTOM_RIGHT),
        Target::SpeciesCorner("S1".into(), 2)
    );
    // Each side moves by the pointer's movement: 10 right and 5 down makes
    // it 20 wider and 10 taller, with the centre where it was.
    drag(&mut v, S1_BOTTOM_RIGHT, S1_BOTTOM_RIGHT + pt(10.0, 5.0));
    assert_eq!(size(&v, "S1"), (100.0, 46.0));
    assert_eq!(v.model().find_species("S1").unwrap().center, pt(0.0, 0.0));
    assert_eq!(v.undo_description(), Some("Resize Species"));
    v.undo();
    assert_eq!(size(&v, "S1"), (80.0, 36.0));
    // The top-left corner the other way round: up and left grows it.
    v.select_item(&Target::Species("S1".into()), false);
    let top_left = pt(-40.0 - HALO_OUTSET, -18.0 - HALO_OUTSET);
    drag(&mut v, top_left, top_left - pt(10.0, 5.0));
    assert_eq!(size(&v, "S1"), (100.0, 46.0));
}

#[test]
fn every_selected_species_grows_by_the_same_amount() {
    let mut v = view();
    v.select_item(&Target::Species("S1".into()), false);
    v.select_item(&Target::Species("S2".into()), true);
    v.select_item(&Target::Species("_null".into()), true);
    v.set_species_size(Some(60.0), None);
    assert_eq!(size(&v, "S2"), (60.0, 36.0));
    drag(
        &mut v,
        pt(30.0 + HALO_OUTSET, 18.0 + HALO_OUTSET),
        pt(50.0 + HALO_OUTSET, 18.0 + HALO_OUTSET),
    );
    assert_eq!(size(&v, "S1"), (100.0, 36.0));
    assert_eq!(size(&v, "S2"), (100.0, 36.0));
    // A null node keeps its size.
    assert_eq!(size(&v, "_null"), (28.0, 28.0));
}

#[test]
fn shift_keeps_the_proportions() {
    let mut v = view();
    v.select_item(&Target::Species("S1".into()), false);
    v.select_item(&Target::Species("S2".into()), true);
    v.set_species_size(None, Some(18.0));
    assert_eq!(size(&v, "S2"), (80.0, 18.0));
    let corner = pt(40.0 + HALO_OUTSET, 9.0 + HALO_OUTSET);
    press(&mut v, corner);
    v.handle(&ViewEvent::PointerMove {
        pos: corner + pt(40.0, 0.0),
    });
    // Without Shift, only the width changes.
    assert_eq!(size(&v, "S1"), (160.0, 18.0));
    // Shift held during the drag: twice the size both ways, and S2 the same.
    v.set_shift_held(true);
    assert_eq!(size(&v, "S1"), (160.0, 36.0));
    assert_eq!(size(&v, "S2"), (160.0, 36.0));
    v.set_shift_held(false);
    assert_eq!(size(&v, "S1"), (160.0, 18.0));
    v.set_shift_held(true);
    v.handle(&ViewEvent::PointerUp {
        pos: corner + pt(40.0, 0.0),
        button: Button::Primary,
    });
    assert_eq!(size(&v, "S1"), (160.0, 36.0));
}

#[test]
fn a_species_never_gets_smaller_than_the_least_size() {
    let mut v = view();
    v.select_item(&Target::Species("S1".into()), false);
    drag(&mut v, S1_BOTTOM_RIGHT, S1_BOTTOM_RIGHT - pt(500.0, 500.0));
    assert_eq!(size(&v, "S1"), (MIN_SPECIES_SIZE, MIN_SPECIES_SIZE));
    v.set_species_size(Some(1.0), Some(-5.0));
    assert_eq!(size(&v, "S1"), (MIN_SPECIES_SIZE, MIN_SPECIES_SIZE));
}

#[test]
fn escape_puts_the_size_back() {
    let mut v = view();
    v.select_item(&Target::Species("S1".into()), false);
    press(&mut v, S1_BOTTOM_RIGHT);
    v.handle(&ViewEvent::PointerMove {
        pos: S1_BOTTOM_RIGHT + pt(30.0, 30.0),
    });
    assert_ne!(size(&v, "S1"), (80.0, 36.0));
    v.handle(&ViewEvent::Cancel);
    assert_eq!(size(&v, "S1"), (80.0, 36.0));
    assert!(v.undo_description().is_none(), "nothing to undo");
}

#[test]
fn fit_to_name_takes_the_width_of_the_name() {
    let mut v = view();
    v.select_item(&Target::Species("S1".into()), false);
    v.fit_species_to_names();
    let s = v.model().find_species("S1").unwrap();
    let want =
        crate::text::text_width("S1", crate::render::constants::LABEL_FONT_SIZE) + 2.0 * FIT_MARGIN;
    assert_eq!(width_for_name(s), want);
    assert_eq!(s.width, want.ceil(), "in whole units");
    assert!(s.width < 80.0, "S1 is short: narrower than the default");
    assert_eq!(s.height, 36.0, "the height stays");
    assert_eq!(v.undo_description(), Some("Fit to Name"));
}

#[test]
fn a_resized_model_is_still_written_as_version_3() {
    let mut v = view();
    v.select_item(&Target::Species("S1".into()), false);
    v.set_species_size(Some(120.0), Some(50.0));
    let text = crate::json::to_json(v.model());
    assert!(text.contains("\"version\": 3"), "{}", &text[..80]);
    let back = crate::json::from_json(&text).unwrap().model;
    let s = back.find_species("S1").unwrap();
    assert_eq!((s.width, s.height), (120.0, 50.0));
}

// ----- Shapes (S §5.10) -------------------------------------------------------

use crate::model::NodeShape;

#[test]
fn a_shape_goes_to_every_selected_species_and_a_circle_is_round() {
    let mut v = view();
    v.select_item(&Target::Species("S1".into()), false);
    v.select_item(&Target::Species("S2".into()), true);
    v.select_item(&Target::Species("_null".into()), true);
    assert_eq!(v.selected_species_shape(), Some(NodeShape::RoundedRect));
    v.set_species_shape(NodeShape::Hexagon);
    assert_eq!(v.undo_description(), Some("Change Shape"));
    let m = v.model();
    assert_eq!(m.find_species("S1").unwrap().shape, NodeShape::Hexagon);
    assert_eq!(m.find_species("S2").unwrap().shape, NodeShape::Hexagon);
    assert_eq!(
        m.find_species("_null").unwrap().shape,
        NodeShape::RoundedRect
    );
    assert_eq!(size(&v, "S1"), (80.0, 36.0), "the size stays");
    // A circle takes the larger side both ways, so the name still fits.
    v.set_species_shape(NodeShape::Circle);
    assert_eq!(size(&v, "S1"), (80.0, 80.0));
    v.undo();
    assert_eq!(size(&v, "S1"), (80.0, 36.0));
    // Two shapes selected: no single one to show.
    v.select_item(&Target::Species("S2".into()), false);
    v.set_species_shape(NodeShape::Ellipse);
    v.select_item(&Target::Species("S1".into()), true);
    assert_eq!(v.selected_species_shape(), None);
}

#[test]
fn a_circle_stays_round() {
    let mut v = view();
    v.select_item(&Target::Species("S1".into()), false);
    v.set_species_shape(NodeShape::Circle);
    // A drag keeps it round without Shift.
    let corner = pt(40.0 + HALO_OUTSET, 40.0 + HALO_OUTSET);
    drag(&mut v, corner, corner + pt(10.0, 0.0));
    assert_eq!(size(&v, "S1"), (100.0, 100.0));
    // Either field sets both.
    v.set_species_size(Some(60.0), None);
    assert_eq!(size(&v, "S1"), (60.0, 60.0));
    v.set_species_size(None, Some(70.0));
    assert_eq!(size(&v, "S1"), (70.0, 70.0));
    v.fit_species_to_names();
    let (w, h) = size(&v, "S1");
    assert_eq!(w, h);
}

#[test]
fn an_alias_is_drawn_as_its_primary_is() {
    let mut v = view();
    v.select_item(&Target::Species("S1".into()), false);
    v.set_species_shape(NodeShape::Ellipse);
    let alias = v.model.add_alias("S1", pt(0.0, 100.0)).unwrap();
    assert_eq!(
        v.model().find_species(&alias).unwrap().shape,
        NodeShape::Ellipse
    );
}

#[test]
fn shapes_are_saved_only_when_used_and_need_version_5() {
    let mut v = view();
    let plain = crate::json::to_json(v.model());
    assert!(plain.contains("\"version\": 3") && !plain.contains("\"shape\""));
    v.select_item(&Target::Species("S1".into()), false);
    v.set_species_shape(NodeShape::Hexagon);
    let text = crate::json::to_json(v.model());
    assert!(text.contains("\"version\": 5"));
    assert!(text.contains("\"shape\": \"hexagon\""));
    assert_eq!(text.matches("\"shape\"").count(), 1, "only S1 says");
    let back = crate::json::from_json(&text).unwrap();
    assert!(back.warnings.is_empty());
    assert_eq!(
        back.model.find_species("S1").unwrap().shape,
        NodeShape::Hexagon
    );
    // A shape this version doesn't know is drawn as the default, with a word.
    let odd = text.replace("\"hexagon\"", "\"star\"");
    let back = crate::json::from_json(&odd).unwrap();
    assert_eq!(back.warnings.len(), 1);
    assert_eq!(
        back.model.find_species("S1").unwrap().shape,
        NodeShape::RoundedRect
    );
}

#[test]
fn shapes_come_back_from_sbml() {
    let mut v = view();
    v.select_item(&Target::Species("S1".into()), false);
    v.set_species_shape(NodeShape::Circle);
    let alias = v.model.add_alias("S1", pt(0.0, 200.0)).unwrap();
    v.model.find_species_mut(&alias).unwrap().shape = NodeShape::Hexagon;
    let out = crate::sbml::export(v.model());
    assert!(out.xml.contains("shape=\"circle\""));
    let opts = crate::antimony::ImportOptions {
        convert_catalytic: false,
        seed: 1,
    };
    let back = crate::sbml::import(&out.xml, &opts).unwrap().model;
    assert_eq!(back.find_species("S1").unwrap().shape, NodeShape::Circle);
    let a = back.aliases_of("S1").next().unwrap();
    assert_eq!(a.shape, NodeShape::Hexagon, "each drawing has its own");
    assert_eq!(
        back.find_species("S2").unwrap().shape,
        NodeShape::RoundedRect
    );
}

// ----- Border style (S §5.3) ---------------------------------------------------

use crate::model::BorderDash;

#[test]
fn a_border_style_goes_to_every_selected_species() {
    let mut v = view();
    let alias = v.model.add_alias("S1", pt(0.0, 100.0)).unwrap();
    v.select_item(&Target::Species("S1".into()), false);
    v.select_item(&Target::Species(alias.clone()), true);
    v.select_item(&Target::Species("_null".into()), true);
    assert_eq!(
        v.selected_species_border_dash(),
        Some(BorderDash::Automatic)
    );
    v.set_species_border_dash(BorderDash::Solid);
    assert_eq!(v.undo_description(), Some("Change Border Style"));
    let m = v.model();
    assert_eq!(
        m.find_species(&alias).unwrap().border_dash,
        BorderDash::Solid
    );
    assert_eq!(m.find_species("S1").unwrap().border_dash, BorderDash::Solid);
    assert_eq!(
        m.find_species("_null").unwrap().border_dash,
        BorderDash::Automatic
    );
    // A new alias starts dashed again, as aliases do.
    let another = v.model.add_alias("S1", pt(0.0, 200.0)).unwrap();
    assert_eq!(
        v.model().find_species(&another).unwrap().border_dash,
        BorderDash::Automatic
    );
    v.undo();
    assert_eq!(
        v.model().find_species(&alias).unwrap().border_dash,
        BorderDash::Automatic
    );
    v.select_item(&Target::Species("S2".into()), false);
    v.set_species_border_dash(BorderDash::Dashed);
    v.select_item(&Target::Species("S1".into()), true);
    assert_eq!(v.selected_species_border_dash(), None, "mixed");
}

#[test]
fn a_border_style_is_saved_only_when_set() {
    let mut v = view();
    assert!(!crate::json::to_json(v.model()).contains("borderDash"));
    let alias = v.model.add_alias("S1", pt(0.0, 100.0)).unwrap();
    v.select_item(&Target::Species(alias.clone()), false);
    v.set_species_border_dash(BorderDash::Solid);
    let text = crate::json::to_json(v.model());
    assert!(text.contains("\"version\": 5"));
    assert!(text.contains("\"borderDash\": \"solid\""));
    let back = crate::json::from_json(&text).unwrap().model;
    assert_eq!(
        back.find_species(&alias).unwrap().border_dash,
        BorderDash::Solid
    );
    // And in SBML, beside the shape.
    v.model.find_species_mut("S1").unwrap().shape = NodeShape::Circle;
    v.model.find_species_mut("S1").unwrap().border_dash = BorderDash::Dashed;
    let out = crate::sbml::export(v.model());
    let opts = crate::antimony::ImportOptions {
        convert_catalytic: false,
        seed: 1,
    };
    let back = crate::sbml::import(&out.xml, &opts).unwrap().model;
    let s1 = back.find_species("S1").unwrap();
    assert_eq!(
        (s1.shape, s1.border_dash),
        (NodeShape::Circle, BorderDash::Dashed)
    );
    let a = back.aliases_of("S1").next().unwrap();
    assert_eq!(a.border_dash, BorderDash::Solid);
}
