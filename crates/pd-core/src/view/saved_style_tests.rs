//! Saved styles (S §8.6): saving, applying, updating, renaming and deleting
//! them, the links from items, which hand edits drop, and the file.

use super::*;
use crate::model::{
    BorderDash, Compartment, Frame, NodeShape, Participant, Rgba, StyleKind, TextAlign,
};
use crate::view::StyleError;

/// S1, S2, S3 (80 × 36) in a row, a null node, R1: S1 → S2 and R2: S2 → S3,
/// a drawn compartment `cell` and another `nucleus`, and texts T1 and T2.
fn view() -> DiagramView {
    let mut m = Model::new();
    for i in 0..3 {
        m.add_species(pt(200.0 * f64::from(i), 0.0), 80.0, 36.0);
    }
    let mut null = crate::model::SpeciesNode::new("_null", pt(0.0, 300.0));
    null.is_null_node = true;
    m.push_species(null).unwrap();
    for (id, a, b) in [("R1", "S1", "S2"), ("R2", "S2", "S3")] {
        let at = (m.find_species(a).unwrap().center + m.find_species(b).unwrap().center) * 0.5;
        let mut r = crate::model::Reaction::new(id, at);
        r.reactants.push(Participant::new(a, 1.0));
        r.products.push(Participant::new(b, 1.0));
        m.push_reaction(r).unwrap();
    }
    for (id, x) in [("cell", 200.0), ("nucleus", 900.0)] {
        let mut c = Compartment::new(id);
        c.frame = Some(Frame {
            center: pt(x, 0.0),
            width: 500.0,
            height: 200.0,
        });
        m.compartments.push(c);
    }
    for (id, y, words) in [("T1", 200.0, "one"), ("T2", 260.0, "two")] {
        m.push_text(crate::model::TextItem::new(id, pt(0.0, y), words))
            .unwrap();
    }
    DiagramView::new(m)
}

fn sp(id: &str) -> Target {
    Target::Species(id.into())
}

fn select(v: &mut DiagramView, items: &[Target]) {
    for (i, t) in items.iter().enumerate() {
        v.select_item(t, i > 0);
    }
}

fn link(v: &DiagramView, item: &Target) -> Option<String> {
    v.style_of(item)
}

/// S1 red with a dashed ellipse, saved as "Enzyme" (S1 follows it).
fn with_enzyme() -> DiagramView {
    let mut v = view();
    select(&mut v, &[sp("S1")]);
    v.apply_colour(ColourField::Fill, Rgba::rgb(0xCC, 0, 0));
    v.set_species_shape(NodeShape::Ellipse);
    v.set_species_border_dash(BorderDash::Dashed);
    v.save_style(&sp("S1"), "Enzyme").unwrap();
    v
}

#[test]
fn save_style_keeps_the_look_and_links_the_item() {
    let mut v = with_enzyme();
    assert_eq!(v.undo_description(), Some("Save Style"));
    assert_eq!(
        v.style_names(StyleKind::Species),
        vec!["Enzyme".to_string()]
    );
    assert!(v.style_names(StyleKind::Reaction).is_empty());
    assert_eq!(link(&v, &sp("S1")).as_deref(), Some("Enzyme"));
    // By its name drawn outside, too.
    assert_eq!(
        link(&v, &Target::SpeciesLabel("S1".into())).as_deref(),
        Some("Enzyme")
    );
    assert_eq!(link(&v, &sp("S2")), None);
    // Undo takes the style and the link away together.
    v.undo();
    assert!(v.style_names(StyleKind::Species).is_empty());
    assert_eq!(link(&v, &sp("S1")), None);
    v.redo();
    assert_eq!(link(&v, &sp("S1")).as_deref(), Some("Enzyme"));

    // The name is trimmed; an empty one, a taken one, or nothing to take
    // the look from, is refused.
    assert_eq!(v.save_style(&sp("S2"), "  "), Err(StyleError::EmptyName));
    assert_eq!(
        v.save_style(&sp("S2"), " Enzyme "),
        Err(StyleError::NameTaken("Enzyme".into()))
    );
    assert_eq!(v.save_style(&sp("_null"), "Null"), Err(StyleError::NoItem));
    assert_eq!(v.save_style(&Target::Empty, "X"), Err(StyleError::NoItem));
    // Each kind has its own names.
    v.save_style(&Target::Leg("R1".into()), "Enzyme").unwrap();
    assert_eq!(
        v.style_names(StyleKind::Reaction),
        vec!["Enzyme".to_string()]
    );
}

#[test]
fn apply_style_gives_the_look_to_the_selected_items_of_its_kind() {
    let mut v = with_enzyme();
    // A mixed selection: S2, S3, a reaction, a text and the null node.
    select(
        &mut v,
        &[
            sp("S2"),
            sp("S3"),
            Target::Leg("R1".into()),
            Target::Text("T1".into()),
            sp("_null"),
        ],
    );
    let r1 = v.model().find_reaction("R1").unwrap().clone();
    v.apply_style(StyleKind::Species, "Enzyme");
    assert_eq!(v.undo_description(), Some("Apply Style"));
    let s1 = v.model().find_species("S1").unwrap().clone();
    for id in ["S2", "S3"] {
        let s = v.model().find_species(id).unwrap();
        assert_eq!(
            (s.style, s.shape, s.border_dash),
            (s1.style, s1.shape, s1.border_dash)
        );
        assert_eq!(link(&v, &sp(id)).as_deref(), Some("Enzyme"));
    }
    assert_eq!(
        v.model().find_reaction("R1").unwrap(),
        &r1,
        "other kinds alone"
    );
    assert_eq!(
        link(&v, &sp("_null")),
        None,
        "a null node never follows one"
    );
    assert_eq!(
        v.selected_style_name(StyleKind::Species).as_deref(),
        Some("Enzyme")
    );
    // With S1 too, still all Enzyme; with a plain one, none in common.
    select(&mut v, &[sp("S1"), sp("S2")]);
    assert_eq!(
        v.selected_style_name(StyleKind::Species).as_deref(),
        Some("Enzyme")
    );
    select(&mut v, &[sp("S3")]);
    v.set_species_border_dash(BorderDash::Solid);
    select(&mut v, &[sp("S2"), sp("S3")]);
    assert_eq!(v.selected_style_name(StyleKind::Species), None, "mixed");
    v.undo();
    // One undo step for all of them.
    v.undo();
    assert_eq!(link(&v, &sp("S2")), None);
    assert_eq!(
        v.model().find_species("S3").unwrap().shape,
        NodeShape::RoundedRect
    );
    // A style the model doesn't have does nothing.
    assert!(!v.apply_style(StyleKind::Species, "Nope").model_changed);
}

#[test]
fn a_hand_edit_drops_the_link_and_undo_brings_it_back() {
    let mut v = with_enzyme();
    select(&mut v, &[sp("S2")]);
    v.apply_style(StyleKind::Species, "Enzyme");
    // A colour (a style edit), then a shape (a snapshot).
    v.apply_colour(ColourField::Border, Rgba::rgb(0, 0x99, 0));
    assert_eq!(link(&v, &sp("S2")), None);
    assert_eq!(
        link(&v, &sp("S1")).as_deref(),
        Some("Enzyme"),
        "others keep theirs"
    );
    v.undo();
    assert_eq!(link(&v, &sp("S2")).as_deref(), Some("Enzyme"));
    v.redo();
    assert_eq!(link(&v, &sp("S2")), None);
    v.undo();
    // Undo clears the selection.
    select(&mut v, &[sp("S2")]);
    v.set_species_shape(NodeShape::Hexagon);
    assert_eq!(link(&v, &sp("S2")), None);
    v.undo();
    select(&mut v, &[sp("S2")]);
    // A change that isn't to its look keeps it: moving, renaming, its size
    // (with the name inside), its value.
    v.set_species_size(Some(150.0), None);
    v.set_initial_value("S2", 3.0);
    v.rename("S2", "ATP").unwrap();
    assert_eq!(link(&v, &sp("ATP")).as_deref(), Some("Enzyme"));
    // Pasting the very same look keeps it too: it still looks like it.
    v.copy_style(&sp("S1"));
    select(&mut v, &[sp("ATP")]);
    v.paste_style();
    assert_eq!(link(&v, &sp("ATP")).as_deref(), Some("Enzyme"));
}

#[test]
fn a_style_with_the_name_outside_keeps_the_size_so_resizing_drops_it() {
    use crate::render::constants::HALO_OUTSET;
    let mut v = view();
    select(&mut v, &[sp("S1")]);
    v.set_species_label_outside(true);
    v.set_species_shape(NodeShape::Circle);
    v.set_species_size(Some(20.0), None);
    v.save_style(&sp("S1"), "Metabolite").unwrap();
    select(&mut v, &[sp("S2")]);
    v.apply_style(StyleKind::Species, "Metabolite");
    let s2 = v.model().find_species("S2").unwrap();
    assert_eq!((s2.width, s2.height), (20.0, 20.0));
    assert!(s2.label_offset.is_some());
    // Moving the name keeps the link: where it is isn't part of the look.
    let at = pt(200.0, 0.0) + s2.label_offset.unwrap();
    v.handle(&ViewEvent::PointerDown {
        pos: at,
        button: Button::Primary,
        modifiers: Modifiers::NONE,
    });
    v.handle(&ViewEvent::PointerUp {
        pos: at + pt(0.0, -20.0),
        button: Button::Primary,
    });
    assert_eq!(v.undo_description(), Some("Move Label"));
    assert_eq!(link(&v, &sp("S2")).as_deref(), Some("Metabolite"));
    // A corner drag changes its size, so it no longer follows it.
    let corner = pt(200.0 + 10.0 + HALO_OUTSET, 10.0 + HALO_OUTSET);
    v.handle(&ViewEvent::PointerDown {
        pos: corner,
        button: Button::Primary,
        modifiers: Modifiers::NONE,
    });
    v.handle(&ViewEvent::PointerUp {
        pos: corner + pt(10.0, 10.0),
        button: Button::Primary,
    });
    assert_eq!(v.undo_description(), Some("Resize Species"));
    assert_eq!(link(&v, &sp("S2")), None);
    v.undo();
    assert_eq!(link(&v, &sp("S2")).as_deref(), Some("Metabolite"));
}

#[test]
fn update_style_changes_every_item_that_follows_it() {
    let mut v = with_enzyme();
    select(&mut v, &[sp("S2")]);
    v.apply_style(StyleKind::Species, "Enzyme");
    // S3 is changed by hand, then made the new look of Enzyme.
    select(&mut v, &[sp("S3")]);
    let blue = Rgba::rgb(0, 0, 0xCC);
    v.apply_colour(ColourField::Fill, blue);
    v.set_species_shape(NodeShape::Hexagon);
    v.update_style("Enzyme", &sp("S3")).unwrap();
    assert_eq!(v.undo_description(), Some("Change Style"));
    for id in ["S1", "S2", "S3"] {
        let s = v.model().find_species(id).unwrap();
        assert_eq!(
            (s.style.fill_color, s.shape),
            (blue, NodeShape::Hexagon),
            "{id}"
        );
        assert_eq!(link(&v, &sp(id)).as_deref(), Some("Enzyme"));
    }
    // One undo step puts them all back, and the style.
    v.undo();
    let s1 = v.model().find_species("S1").unwrap();
    assert_eq!(s1.shape, NodeShape::Ellipse);
    assert_eq!(link(&v, &sp("S3")), None);
    // The wrong kind, or a style that isn't there, is refused.
    assert_eq!(
        v.update_style("Enzyme", &Target::Leg("R1".into())),
        Err(StyleError::WrongKind)
    );
    assert_eq!(
        v.update_style("Nope", &sp("S3")),
        Err(StyleError::NoStyle("Nope".into()))
    );
}

#[test]
fn rename_and_delete_a_style() {
    let mut v = with_enzyme();
    select(&mut v, &[sp("S2")]);
    v.apply_style(StyleKind::Species, "Enzyme");
    v.save_style(&sp("S3"), "Plain").unwrap();
    assert_eq!(
        v.rename_style(StyleKind::Species, "Enzyme", "Plain"),
        Err(StyleError::NameTaken("Plain".into()))
    );
    assert_eq!(
        v.rename_style(StyleKind::Species, "Enzyme", " "),
        Err(StyleError::EmptyName)
    );
    v.rename_style(StyleKind::Species, "Enzyme", "Kinase")
        .unwrap();
    assert_eq!(v.undo_description(), Some("Rename Style"));
    assert_eq!(link(&v, &sp("S1")).as_deref(), Some("Kinase"));
    assert_eq!(link(&v, &sp("S2")).as_deref(), Some("Kinase"));
    assert_eq!(link(&v, &sp("S3")).as_deref(), Some("Plain"));
    assert_eq!(
        v.style_names(StyleKind::Species),
        vec!["Kinase".to_string(), "Plain".to_string()]
    );
    // Deleting it unlinks its items, which keep their look.
    let s1 = v.model().find_species("S1").unwrap().clone();
    v.delete_style(StyleKind::Species, "Kinase");
    assert_eq!(v.undo_description(), Some("Delete Style"));
    assert_eq!(link(&v, &sp("S1")), None);
    assert_eq!(link(&v, &sp("S2")), None);
    let now = v.model().find_species("S1").unwrap();
    assert_eq!((now.style, now.shape), (s1.style, s1.shape));
    v.undo();
    assert_eq!(link(&v, &sp("S2")).as_deref(), Some("Kinase"));
}

#[test]
fn every_kind_can_have_saved_styles() {
    let mut v = view();
    // Reactions.
    select(&mut v, &[Target::Leg("R1".into())]);
    v.apply_colour(ColourField::Line, Rgba::rgb(0xCC, 0, 0));
    v.set_junction_visible(false);
    v.save_style(&Target::Junction("R1".into()), "Hidden")
        .unwrap();
    select(&mut v, &[Target::Leg("R2".into())]);
    v.apply_style(StyleKind::Reaction, "Hidden");
    assert!(
        !v.model()
            .find_reaction("R2")
            .unwrap()
            .style
            .junction_visible
    );
    // Compartments.
    select(&mut v, &[Target::Compartment("cell".into())]);
    v.apply_colour(ColourField::CompartmentFill, Rgba::rgb(0xEE, 0xEE, 0));
    v.save_style(&Target::Compartment("cell".into()), "Yellow")
        .unwrap();
    select(&mut v, &[Target::Compartment("nucleus".into())]);
    v.apply_style(StyleKind::Compartment, "Yellow");
    assert_eq!(
        v.model().find_compartment("nucleus").unwrap().style,
        v.model().find_compartment("cell").unwrap().style
    );
    // Texts.
    select(&mut v, &[Target::Text("T1".into())]);
    v.set_text_size(30.0);
    v.set_text_align(TextAlign::Center);
    v.save_style(&Target::Text("T1".into()), "Title").unwrap();
    select(&mut v, &[Target::Text("T2".into())]);
    v.apply_style(StyleKind::Text, "Title");
    let t2 = v.model().find_text("T2").unwrap();
    assert_eq!((t2.size, t2.align), (30.0, TextAlign::Center));
    assert_eq!(t2.text, "two", "its words stay");
    // A hand edit drops each kind's link.
    v.set_text_bold(true);
    assert_eq!(link(&v, &Target::Text("T2".into())), None);
    select(&mut v, &[Target::Leg("R2".into())]);
    v.set_line_width("R2", 4.0);
    assert_eq!(link(&v, &Target::Leg("R2".into())), None);
    select(&mut v, &[Target::Compartment("nucleus".into())]);
    v.set_compartment_border_width("nucleus", 5.0);
    assert_eq!(link(&v, &Target::Compartment("nucleus".into())), None);
}

/// Every kind with a style followed by two items; reloaded.
fn styled() -> DiagramView {
    let mut v = with_enzyme();
    select(&mut v, &[sp("S2")]);
    v.apply_style(StyleKind::Species, "Enzyme");
    select(&mut v, &[sp("S3")]);
    v.set_species_label_outside(true);
    v.set_species_size(Some(20.0), Some(20.0));
    v.save_style(&sp("S3"), "Small").unwrap();
    select(&mut v, &[Target::Leg("R1".into())]);
    v.set_junction_visible(false);
    v.save_style(&Target::Leg("R1".into()), "Quiet").unwrap();
    select(&mut v, &[Target::Compartment("cell".into())]);
    v.apply_colour(ColourField::CompartmentFill, Rgba::rgb(0xEE, 0xEE, 0));
    v.save_style(&Target::Compartment("cell".into()), "Yellow")
        .unwrap();
    select(&mut v, &[Target::Text("T1".into())]);
    v.set_text_typeface(crate::model::Typeface::Serif);
    v.set_text_italic(true);
    v.apply_colour(ColourField::Text, Rgba::rgb(0x33, 0x66, 0x99));
    v.save_style(&Target::Text("T1".into()), "Note").unwrap();
    select(&mut v, &[Target::Text("T2".into())]);
    v.apply_style(StyleKind::Text, "Note");
    v
}

#[test]
fn the_file_keeps_the_styles_items_follow() {
    let mut v = styled();
    // One more, followed by nothing: not written.
    v.save_style(&sp("S1"), "Spare").unwrap();
    select(&mut v, &[sp("S1")]);
    v.apply_style(StyleKind::Species, "Enzyme");
    assert!(crate::styles::users(v.model(), StyleKind::Species, "Spare").is_empty());
    let text = crate::json::to_json(v.model());
    assert!(text.contains("\"version\": 6"));
    assert!(!text.contains("Spare"));
    assert_eq!(text.matches("\"styleName\": \"Enzyme\"").count(), 2);
    let back = crate::json::from_json(&text).unwrap();
    assert!(back.warnings.is_empty(), "{:?}", back.warnings);
    let m = back.model;
    let mut want = v.model().clone();
    want.styles.retain(|s| s.name != "Spare");
    want.clear_selection();
    // Everything comes back: looks, links and styles. A reaction's hidden
    // junction comes back as a custom style (fix D-8), in the style as on
    // the reaction, so they still match.
    for kind in StyleKind::ALL {
        for id in crate::styles::item_ids(&want, kind) {
            assert_eq!(
                crate::styles::link(&m, kind, &id),
                crate::styles::link(&want, kind, &id),
                "{id}"
            );
        }
        assert_eq!(
            m.styles.iter().filter(|s| s.kind() == kind).count(),
            want.styles.iter().filter(|s| s.kind() == kind).count()
        );
    }
    assert_eq!(
        crate::styles::find(&m, StyleKind::Species, "Small")
            .unwrap()
            .look,
        crate::styles::find(&want, StyleKind::Species, "Small")
            .unwrap()
            .look
    );
    assert_eq!(
        crate::styles::find(&m, StyleKind::Text, "Note")
            .unwrap()
            .look,
        crate::styles::find(&want, StyleKind::Text, "Note")
            .unwrap()
            .look
    );
    // Saving again gives the same file.
    assert_eq!(crate::json::to_json(&m), text);
}

#[test]
fn a_link_alone_needs_version_6() {
    let mut v = view();
    // Texts alone are version 4; a text following a style, 6.
    assert!(crate::json::to_json(v.model()).contains("\"version\": 4"));
    v.save_style(&Target::Text("T1".into()), "Plain").unwrap();
    assert!(crate::json::to_json(v.model()).contains("\"version\": 6"));
}

#[test]
fn a_model_without_links_keeps_its_old_version() {
    let mut v = with_enzyme();
    v.delete_style(StyleKind::Species, "Enzyme");
    // S1 is an ellipse: version 5, not 6.
    let text = crate::json::to_json(v.model());
    assert!(text.contains("\"version\": 5"));
    assert!(!text.contains("styles") && !text.contains("styleName"));
}

#[test]
fn a_damaged_file_loses_only_the_broken_links() {
    let v = styled();
    let text = crate::json::to_json(v.model());
    // A link to a style that isn't there: a warning, and the look stays.
    let odd = text.replacen("\"styleName\": \"Enzyme\"", "\"styleName\": \"Gone\"", 1);
    let back = crate::json::from_json(&odd).unwrap();
    assert_eq!(back.warnings.len(), 1, "{:?}", back.warnings);
    assert_eq!(back.model.find_species("S1").unwrap().style_name, None);
    assert_eq!(
        back.model.find_species("S2").unwrap().style_name.as_deref(),
        Some("Enzyme")
    );
    // An item edited by hand in the file no longer looks like its style: it
    // is unlinked quietly.
    let odd = text.replacen("\"shape\": \"ellipse\"", "\"shape\": \"hexagon\"", 1);
    let back = crate::json::from_json(&odd).unwrap();
    assert!(back.warnings.is_empty());
    let unlinked = ["S1", "S2"]
        .iter()
        .filter(|id| back.model.find_species(id).unwrap().style_name.is_none())
        .count();
    assert_eq!(unlinked, 1);
    // A kind this version doesn't know, a second style with a name its kind
    // has, and a style with its name outside but no size: skipped, each with
    // a word.
    let mut file: serde_json::Value = serde_json::from_str(&text).unwrap();
    let styles = file["styles"].as_array_mut().unwrap();
    let mut glow = styles[0].clone();
    glow["kind"] = "glow".into();
    let dup = styles[0].clone();
    let mut sizeless = styles[0].clone();
    sizeless["name"] = "Sizeless".into();
    sizeless["labelOutside"] = true.into();
    styles.extend([glow, dup, sizeless]);
    let back = crate::json::from_json(&file.to_string()).unwrap();
    assert_eq!(back.warnings.len(), 3, "{:?}", back.warnings);
    assert_eq!(
        back.model
            .styles
            .iter()
            .filter(|s| s.kind() == StyleKind::Species)
            .count(),
        2
    );
}
