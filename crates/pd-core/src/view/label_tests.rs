//! Names drawn outside their node (S §5.11): taking them out and back, the
//! name's own hit box and drag, what carries it along, Copy Style, and the
//! files. Driven with made-up events.

use super::*;
use crate::model::NodeShape;
use crate::render::constants::LABEL_GAP;
use crate::render::draw::{content_bounds, default_label_offset, label_size, outside_label_box};
use crate::text::text_width;

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

fn press(v: &mut DiagramView, at: Pt, modifiers: Modifiers) {
    v.handle(&ViewEvent::PointerDown {
        pos: at,
        button: Button::Primary,
        modifiers,
    });
}

fn drag(v: &mut DiagramView, from: Pt, to: Pt) {
    press(v, from, Modifiers::NONE);
    v.handle(&ViewEvent::PointerMove { pos: to });
    v.handle(&ViewEvent::PointerUp {
        pos: to,
        button: Button::Primary,
    });
}

fn offset(v: &DiagramView, id: &str) -> Option<Pt> {
    v.model().find_species(id).unwrap().label_offset
}

/// S1 with its name outside, and nothing selected.
fn outside_s1() -> DiagramView {
    let mut v = view();
    v.select_item(&Target::Species("S1".into()), false);
    v.set_species_label_outside(true);
    v.model.clear_selection();
    v
}

/// Where S1's name is drawn.
fn s1_label(v: &DiagramView) -> Pt {
    pt(0.0, 0.0) + offset(v, "S1").unwrap()
}

#[test]
fn a_name_taken_outside_starts_just_right_of_the_node() {
    let mut v = view();
    v.select_item(&Target::Species("S1".into()), false);
    v.select_item(&Target::Species("_null".into()), true);
    assert_eq!(v.selected_species_label_outside(), Some(false));
    v.set_species_label_outside(true);
    assert_eq!(v.undo_description(), Some("Label Outside"));
    // Its left edge LABEL_GAP right of the node, level with its centre.
    let w = text_width("S1", label_size(v.model().find_species("S1").unwrap()));
    assert_eq!(offset(&v, "S1"), Some(pt(40.0 + LABEL_GAP + w / 2.0, 0.0)));
    assert_eq!(offset(&v, "S2"), None, "not selected");
    assert_eq!(offset(&v, "_null"), None, "a null node has no name");
    assert_eq!(v.selected_species_label_outside(), Some(true));
    // Again: one already outside stays where it is.
    v.model.find_species_mut("S1").unwrap().label_offset = Some(pt(0.0, -30.0));
    let r = v.set_species_label_outside(true);
    assert!(!r.model_changed);
    assert_eq!(offset(&v, "S1"), Some(pt(0.0, -30.0)));
    // Back inside, and undo brings it out again.
    v.set_species_label_outside(false);
    assert_eq!(offset(&v, "S1"), None);
    assert_eq!(v.undo_description(), Some("Label Inside"));
    v.undo();
    assert_eq!(offset(&v, "S1"), Some(pt(0.0, -30.0)));
    // Mixed when they disagree.
    v.select_item(&Target::Species("S1".into()), false);
    v.select_item(&Target::Species("S2".into()), true);
    assert_eq!(v.selected_species_label_outside(), None);
}

#[test]
fn the_name_is_hit_on_its_own_and_selects_the_species() {
    let mut v = outside_s1();
    let at = s1_label(&v);
    assert_eq!(v.target_at(at), Target::SpeciesLabel("S1".into()));
    // Inside the node is still the node.
    assert_eq!(v.target_at(pt(0.0, 0.0)), Target::Species("S1".into()));
    // An inside name has no box of its own.
    let s2 = v.model().find_species("S2").unwrap();
    assert!(outside_label_box(s2).is_none());
    // A click selects the species; Shift-click adds and takes away.
    press(&mut v, at, Modifiers::NONE);
    v.handle(&ViewEvent::PointerUp {
        pos: at,
        button: Button::Primary,
    });
    assert_eq!(v.selection().0, vec!["S1".to_string()]);
    assert_eq!(
        v.undo_description(),
        Some("Label Outside"),
        "a click moves nothing"
    );
    v.select_item(&Target::Species("S2".into()), false);
    press(&mut v, at, Modifiers::SHIFT);
    assert_eq!(v.selection().0.len(), 2);
    press(&mut v, at, Modifiers::SHIFT);
    assert_eq!(v.selection().0, vec!["S2".to_string()]);
    // A double click renames; the tooltip is the species'.
    let r = v.handle(&ViewEvent::DoubleClick { pos: at });
    assert_eq!(r.request, Some(Request::Rename("S1".into())));
    assert_eq!(
        v.tooltip_text(&Target::SpeciesLabel("S1".into())),
        v.tooltip_text(&Target::Species("S1".into()))
    );
}

#[test]
fn dragging_the_name_moves_only_the_name() {
    let mut v = outside_s1();
    let before = offset(&v, "S1").unwrap();
    let at = s1_label(&v);
    drag(&mut v, at, at + pt(-10.0, 50.0));
    let s1 = v.model().find_species("S1").unwrap();
    assert_eq!(s1.center, pt(0.0, 0.0), "the node stays");
    assert_eq!(s1.label_offset, Some(before + pt(-10.0, 50.0)));
    assert_eq!(v.undo_description(), Some("Move Label"));
    v.undo();
    assert_eq!(offset(&v, "S1"), Some(before));
    // Esc in the middle puts it back, with nothing to undo.
    let mut v = outside_s1();
    press(&mut v, at, Modifiers::NONE);
    v.handle(&ViewEvent::PointerMove {
        pos: at + pt(30.0, 0.0),
    });
    assert_eq!(offset(&v, "S1"), Some(before + pt(30.0, 0.0)));
    v.handle(&ViewEvent::Cancel);
    assert_eq!(offset(&v, "S1"), Some(before));
    assert_eq!(v.undo_description(), Some("Label Outside"));
}

#[test]
fn dragging_the_node_carries_its_name() {
    let mut v = outside_s1();
    let before = offset(&v, "S1").unwrap();
    drag(&mut v, pt(0.0, 0.0), pt(20.0, 10.0));
    let s1 = v.model().find_species("S1").unwrap();
    assert_eq!(s1.center, pt(20.0, 10.0));
    assert_eq!(
        s1.label_offset,
        Some(before),
        "the name keeps its place by the node"
    );
    assert_eq!(
        v.target_at(pt(20.0, 10.0) + before),
        Target::SpeciesLabel("S1".into())
    );
}

#[test]
fn a_rubber_band_over_the_name_selects_the_species() {
    let mut v = outside_s1();
    let (a, b) = outside_label_box(v.model().find_species("S1").unwrap()).unwrap();
    // Only the far end of the name, clear of the node.
    let from = pt(b.x - 2.0, a.y - 20.0);
    assert_eq!(v.target_at(from), Target::Empty);
    drag(&mut v, from, pt(b.x + 20.0, b.y + 20.0));
    assert_eq!(v.selection().0, vec!["S1".to_string()]);
}

#[test]
fn renaming_keeps_the_edge_by_the_node() {
    let mut v = outside_s1();
    let size = label_size(v.model().find_species("S1").unwrap());
    let near = |v: &DiagramView, id: &str| {
        outside_label_box(v.model().find_species(id).unwrap())
            .unwrap()
            .0
            .x
    };
    let left = near(&v, "S1");
    v.rename("S1", "glucose_6_phosphate").unwrap();
    let id = "glucose_6_phosphate";
    let s = v.model().find_species(id).unwrap();
    assert_eq!(s.width, 80.0, "the node isn't widened for a name outside");
    assert!((near(&v, id) - left).abs() < 1e-9, "the left edge stays");
    // On the left of the node it grows to the left.
    let grow = text_width("G", size) - text_width(id, size);
    v.model.find_species_mut(id).unwrap().label_offset = Some(pt(-60.0, 0.0));
    v.rename(id, "G").unwrap();
    assert_eq!(offset(&v, "G"), Some(pt(-60.0 - grow / 2.0, 0.0)));
    // Straight above, it grows both ways.
    v.model.find_species_mut("G").unwrap().label_offset = Some(pt(0.0, -30.0));
    v.rename("G", "Glc").unwrap();
    assert_eq!(offset(&v, "Glc"), Some(pt(0.0, -30.0)));
    // A name inside still widens its node.
    v.rename("S2", "a_much_longer_name_than_before").unwrap();
    assert!(
        v.model()
            .find_species("a_much_longer_name_than_before")
            .unwrap()
            .width
            > 80.0
    );
}

#[test]
fn fit_to_name_leaves_names_outside_alone() {
    let mut v = outside_s1();
    v.model.find_species_mut("S1").unwrap().width = 20.0;
    v.select_item(&Target::Species("S1".into()), false);
    v.select_item(&Target::Species("S2".into()), true);
    v.model.find_species_mut("S2").unwrap().width = 20.0;
    v.fit_species_to_names();
    assert_eq!(v.model().find_species("S1").unwrap().width, 20.0);
    assert!(v.model().find_species("S2").unwrap().width > 20.0);
}

#[test]
fn copy_style_with_the_name_outside_takes_the_size_too() {
    let mut v = outside_s1();
    {
        let s1 = v.model.find_species_mut("S1").unwrap();
        (s1.width, s1.height, s1.shape) = (16.0, 16.0, NodeShape::Circle);
        s1.label_offset = Some(pt(0.0, -20.0));
    }
    v.copy_style(&Target::SpeciesLabel("S1".into())).unwrap();
    v.select_item(&Target::Species("S2".into()), false);
    v.paste_style();
    let s2 = v.model().find_species("S2").unwrap();
    assert_eq!(
        (s2.width, s2.height, s2.shape),
        (16.0, 16.0, NodeShape::Circle)
    );
    // Outside, but where is its own: it starts by its own node.
    assert_eq!(s2.label_offset, Some(default_label_offset(s2)));
    // A style with the name inside brings it in, and leaves the size.
    let mut w = view();
    w.copy_style(&Target::Species("S2".into())).unwrap();
    w.model.find_species_mut("S1").unwrap().label_offset = Some(pt(0.0, -20.0));
    w.model.find_species_mut("S1").unwrap().width = 16.0;
    w.select_item(&Target::Species("S1".into()), false);
    w.paste_style();
    let s1 = w.model().find_species("S1").unwrap();
    assert_eq!(s1.label_offset, None);
    assert_eq!(s1.width, 16.0);
}

#[test]
fn an_alias_starts_with_its_primarys_name_outside() {
    let mut v = outside_s1();
    let a = v.model.add_alias("S1", pt(0.0, 200.0)).unwrap();
    assert_eq!(offset(&v, &a), offset(&v, "S1"));
}

#[test]
fn the_name_is_inside_the_drawn_area() {
    let mut v = outside_s1();
    v.model.find_species_mut("S2").unwrap().label_offset = Some(pt(0.0, 500.0));
    let (_, max) = content_bounds(v.model());
    let (_, b) = outside_label_box(v.model().find_species("S2").unwrap()).unwrap();
    assert!(max.y >= b.y);
}

#[test]
fn names_outside_are_saved_only_when_used_and_need_version_6() {
    let mut v = view();
    v.select_item(&Target::Species("S1".into()), false);
    v.set_species_shape(NodeShape::Circle);
    let text = crate::json::to_json(v.model());
    assert!(text.contains("\"version\": 5") && !text.contains("labelD"));
    v.set_species_label_outside(true);
    v.model.find_species_mut("S1").unwrap().label_offset = Some(pt(12.5, -30.0));
    let text = crate::json::to_json(v.model());
    assert!(text.contains("\"version\": 6"));
    assert!(text.contains("\"labelDx\": 12.5") && text.contains("\"labelDy\": -30.0"));
    assert_eq!(text.matches("labelDx").count(), 1, "only S1 says");
    let back = crate::json::from_json(&text).unwrap();
    assert!(back.warnings.is_empty());
    assert!(!back.is_newer_than_supported());
    assert_eq!(
        back.model.find_species("S1").unwrap().label_offset,
        Some(pt(12.5, -30.0))
    );
    assert_eq!(back.model.find_species("S2").unwrap().label_offset, None);
    // Half a position is read as inside, with a word.
    let odd = text.replace("\"labelDy\": -30.0", "\"other\": 1");
    let back = crate::json::from_json(&odd).unwrap();
    assert_eq!(back.warnings.len(), 1);
    assert_eq!(back.model.find_species("S1").unwrap().label_offset, None);
}

#[test]
fn names_outside_come_back_from_sbml() {
    let mut v = outside_s1();
    v.model.find_species_mut("S1").unwrap().label_offset = Some(pt(0.0, -40.0));
    let a = v.model.add_alias("S1", pt(0.0, 200.0)).unwrap();
    v.model.find_species_mut(&a).unwrap().label_offset = Some(pt(-70.0, 0.0));
    // A name moved back over its node: only the note says it is outside.
    let near = v.model.add_species(pt(600.0, 0.0), 80.0, 36.0);
    v.model.find_species_mut(&near).unwrap().label_offset = Some(pt(5.0, 0.0));
    let out = crate::sbml::export(v.model());
    assert!(out.xml.contains("label=\"outside\""));
    let opts = crate::antimony::ImportOptions {
        convert_catalytic: false,
        seed: 1,
    };
    let back = crate::sbml::import(&out.xml, &opts).unwrap().model;
    let close = |o: Option<Pt>, want: Pt| o.is_some_and(|o| o.distance(want) < 1e-6);
    assert!(close(
        back.find_species("S1").unwrap().label_offset,
        pt(0.0, -40.0)
    ));
    let alias = back.aliases_of("S1").next().unwrap();
    assert!(
        close(alias.label_offset, pt(-70.0, 0.0)),
        "each drawing has its own"
    );
    assert_eq!(back.find_species("S2").unwrap().label_offset, None);
    assert!(close(
        back.find_species(&near).unwrap().label_offset,
        pt(5.0, 0.0)
    ));

    // Another tool's file has no note: a text box away from the node is
    // a name outside it, and one on the node is inside.
    let plain = out.xml.replace(" label=\"outside\"", "");
    let back = crate::sbml::import(&plain, &opts).unwrap().model;
    assert!(close(
        back.find_species("S1").unwrap().label_offset,
        pt(0.0, -40.0)
    ));
    assert_eq!(back.find_species("S2").unwrap().label_offset, None);
    assert_eq!(back.find_species(&near).unwrap().label_offset, None);
}

#[test]
fn resizing_keeps_the_name_by_the_edge() {
    use crate::render::constants::HALO_OUTSET;
    let mut v = outside_s1();
    let gap = |v: &DiagramView| {
        let s = v.model().find_species("S1").unwrap();
        outside_label_box(s).unwrap().0.x - (s.center.x + s.width / 2.0)
    };
    let before = gap(&v);
    // The Width field: 80 to 24, and the name follows the right edge in.
    v.select_item(&Target::Species("S1".into()), false);
    v.set_species_size(Some(24.0), None);
    assert!((gap(&v) - before).abs() < 1e-9);
    // A corner drag, the same.
    let corner = pt(12.0 + HALO_OUTSET, 18.0 + HALO_OUTSET);
    drag(&mut v, corner, corner + pt(8.0, 0.0));
    assert_eq!(v.model().find_species("S1").unwrap().width, 40.0);
    assert!((gap(&v) - before).abs() < 1e-9);
    // Esc in the middle of a drag puts the name back with the size.
    let o = offset(&v, "S1");
    let corner = pt(20.0 + HALO_OUTSET, 18.0 + HALO_OUTSET);
    press(&mut v, corner, Modifiers::NONE);
    v.handle(&ViewEvent::PointerMove {
        pos: corner + pt(30.0, 0.0),
    });
    assert_ne!(offset(&v, "S1"), o);
    v.handle(&ViewEvent::Cancel);
    assert_eq!(offset(&v, "S1"), o);
    // Above the node, a taller node pushes it up; it stays centred across.
    v.model.find_species_mut("S1").unwrap().label_offset = Some(pt(0.0, -30.0));
    v.select_item(&Target::Species("S1".into()), false);
    v.set_species_size(None, Some(56.0));
    assert_eq!(offset(&v, "S1"), Some(pt(0.0, -40.0)));
    v.set_species_size(Some(60.0), None);
    assert_eq!(
        offset(&v, "S1"),
        Some(pt(0.0, -40.0)),
        "still centred across"
    );
    v.set_species_size(Some(40.0), None);
    // Made a circle: the larger side both ways, and the name moves out.
    v.model.find_species_mut("S1").unwrap().label_offset = Some(pt(40.0, 0.0));
    v.set_species_shape(NodeShape::Circle);
    let s1 = v.model().find_species("S1").unwrap();
    assert_eq!((s1.width, s1.height), (56.0, 56.0));
    assert_eq!(
        s1.label_offset,
        Some(pt(48.0, 0.0)),
        "level with the centre"
    );
    // Taller and wider: beside the node it moves out, and stays level.
    v.set_species_size(None, Some(70.0));
    assert_eq!(offset(&v, "S1"), Some(pt(55.0, 0.0)));
}
