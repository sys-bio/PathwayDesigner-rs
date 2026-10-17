//! Species styles (S §8.1, §8.5): the label's font size and colour, and Copy
//! Style / Paste Style.

use super::*;
use crate::model::{BorderDash, NodeShape, Rgba, StyleKind};
use crate::render::constants::LABEL;

/// S1 at (0,0) and S2 at (300,0), 80 × 36, and a null node.
fn view() -> DiagramView {
    let mut m = Model::new();
    m.add_species(pt(0.0, 0.0), 80.0, 36.0);
    m.add_species(pt(300.0, 0.0), 120.0, 40.0);
    let mut null = crate::model::SpeciesNode::new("_null", pt(0.0, 300.0));
    null.is_null_node = true;
    m.push_species(null).unwrap();
    DiagramView::new(m)
}

fn select(v: &mut DiagramView, ids: &[&str]) {
    for (i, id) in ids.iter().enumerate() {
        v.select_item(&Target::Species((*id).into()), i > 0);
    }
}

#[test]
fn the_font_size_and_label_colour_go_to_every_selected_species() {
    let mut v = view();
    select(&mut v, &["S1", "S2"]);
    assert_eq!(v.swatch_colour(ColourField::Label), Some(LABEL));
    v.set_font_size(16.0);
    let red = Rgba::rgb(0xCC, 0, 0);
    v.apply_colour(ColourField::Label, red);
    for id in ["S1", "S2"] {
        let s = v.model().find_species(id).unwrap();
        assert!(s.style.has_custom_style);
        assert_eq!((s.style.font_size, s.style.label_color), (16.0, red));
    }
    assert_eq!(v.swatch_colour(ColourField::Label), Some(red));
    // Each is one undo step.
    v.undo();
    assert_eq!(
        v.model().find_species("S1").unwrap().style.label_color,
        Rgba::DEFAULT
    );
    assert_eq!(v.model().find_species("S1").unwrap().style.font_size, 16.0);
    // A size that isn't one is refused.
    assert!(!v.set_font_size(-1.0).model_changed);
    // Both were always in the file, so the version stays 3.
    let text = crate::json::to_json(v.model());
    assert!(text.contains("\"version\": 3"));
    let back = crate::json::from_json(&text).unwrap().model;
    assert_eq!(back.find_species("S2").unwrap().style.font_size, 16.0);
}

#[test]
fn a_rename_widens_for_the_label_as_it_is_drawn() {
    let mut v = view();
    select(&mut v, &["S1"]);
    v.set_font_size(24.0);
    v.rename("S1", "glucose").unwrap();
    let s = v.model().find_species("glucose").unwrap();
    let want = crate::render::draw::width_for_name(s);
    assert_eq!(s.width, want);
    assert!(
        want > crate::text::text_width("glucose", 12.0) + 20.0,
        "wider than at the default size"
    );
}

#[test]
fn paste_style_copies_the_look_but_not_the_size() {
    let mut v = view();
    // S1: a red, dashed circle with a large blue label.
    select(&mut v, &["S1"]);
    v.apply_colour(ColourField::Fill, Rgba::rgb(0xFF, 0, 0));
    v.set_font_size(18.0);
    v.apply_colour(ColourField::Label, Rgba::rgb(0, 0, 0xFF));
    v.set_species_border_dash(BorderDash::Dashed);
    v.set_species_shape(NodeShape::Circle);
    assert!(!v.has_copied_style());
    let undo_before = v.undo_description().map(str::to_owned);
    v.copy_style(&Target::Species("S1".into()));
    assert!(v.has_copied_style());
    assert_eq!(
        v.undo_description().map(str::to_owned),
        undo_before,
        "copying is not an edit"
    );

    // Nothing selected: nowhere to paste.
    v.handle(&ViewEvent::Cancel);
    assert!(!v.can_paste_style());
    select(&mut v, &["S2", "_null"]);
    assert!(v.can_paste_style());
    v.paste_style();
    assert_eq!(v.undo_description(), Some("Paste Style"));
    let (s1, s2) = (
        v.model().find_species("S1").unwrap().clone(),
        v.model().find_species("S2").unwrap().clone(),
    );
    assert_eq!(s2.style.fill_color, s1.style.fill_color);
    assert_eq!(s2.style.font_size, 18.0);
    assert_eq!(s2.style.label_color, s1.style.label_color);
    assert_eq!(
        (s2.shape, s2.border_dash),
        (NodeShape::Circle, BorderDash::Dashed)
    );
    // Its size stays, made round by its larger side; the null node is untouched.
    assert_eq!((s2.width, s2.height), (120.0, 120.0));
    let null = v.model().find_species("_null").unwrap();
    assert_eq!(null.style, crate::model::VisualStyle::default());
    // One undo step puts it all back.
    v.undo();
    let s2 = v.model().find_species("S2").unwrap();
    assert_eq!((s2.width, s2.height), (120.0, 40.0));
    assert_eq!(s2.shape, NodeShape::RoundedRect);
    assert!(!s2.style.has_custom_style);
}

#[test]
fn a_rectangle_style_leaves_the_size_alone() {
    let mut v = view();
    select(&mut v, &["S1"]);
    v.set_species_shape(NodeShape::Hexagon);
    v.copy_style(&Target::Species("S1".into()));
    select(&mut v, &["S2"]);
    v.paste_style();
    let s2 = v.model().find_species("S2").unwrap();
    assert_eq!(
        (s2.shape, s2.width, s2.height),
        (NodeShape::Hexagon, 120.0, 40.0)
    );
}

/// S1 → R → S2 twice (R3, R4), two compartments and two texts, beside the
/// species of [`view`].
fn everything() -> DiagramView {
    use crate::model::{Frame, Participant, ReactionMode, TextItem};
    let mut v = view();
    let m = &mut v.model;
    for x in [150.0, 160.0] {
        let r = m.add_reaction(pt(x, 0.0));
        let r = m.find_reaction_mut(&r).unwrap();
        r.reactants.push(Participant::new("S1", 1.0));
        r.products.push(Participant::new("S2", 1.0));
    }
    m.find_reaction_mut("R4").unwrap().mode = ReactionMode::Bezier;
    for x in [0.0, 600.0] {
        m.add_compartment(Frame {
            center: pt(x, 600.0),
            width: 100.0 + x,
            height: 80.0,
        });
    }
    for (id, x) in [("T1", 0.0), ("T2", 300.0)] {
        let mut t = TextItem::new(id, pt(x, -200.0), id);
        t.wrap_width = 50.0 + x;
        m.push_text(t).unwrap();
    }
    v
}

#[test]
fn a_reaction_style_goes_to_the_selected_reactions_only() {
    let mut v = everything();
    v.select_item(&Target::Leg("R3".into()), false);
    let blue = Rgba::rgb(0, 0, 0xFF);
    v.apply_colour(ColourField::Line, blue);
    v.set_line_width("R3", 4.0);
    v.set_junction_visible(false);
    assert_eq!(
        v.copy_style(&Target::Junction("R3".into())).as_deref(),
        Some("R3")
    );
    // R4 and S2 selected: R4 takes it, S2 doesn't, and R4 stays a Bézier.
    v.select_item(&Target::Leg("R4".into()), false);
    v.select_item(&Target::Species("S2".into()), true);
    assert!(v.can_paste_style());
    v.paste_style();
    let r4 = v.model().find_reaction("R4").unwrap();
    assert_eq!((r4.style.line_color, r4.style.line_width), (blue, 4.0));
    assert!(!r4.style.junction_visible);
    assert_eq!(r4.mode, crate::model::ReactionMode::Bezier);
    assert!(!v.model().find_species("S2").unwrap().style.has_custom_style);
    // Only species selected: a reaction style has nowhere to go.
    v.select_item(&Target::Species("S1".into()), false);
    assert!(!v.can_paste_style());
}

#[test]
fn a_compartment_style_keeps_each_box() {
    let mut v = everything();
    let (c1, c2) = (
        v.model().compartments[0].id.clone(),
        v.model().compartments[1].id.clone(),
    );
    v.select_item(&Target::Compartment(c1.clone()), false);
    let green = Rgba::rgb(0, 0x99, 0);
    v.apply_colour(ColourField::CompartmentFill, green);
    v.copy_style(&Target::Compartment(c1));
    v.select_item(&Target::Compartment(c2.clone()), false);
    v.paste_style();
    let c = v.model().find_compartment(&c2).unwrap();
    assert_eq!(c.style.fill_color, green);
    assert_eq!(c.frame.unwrap().width, 700.0, "its own box");
}

#[test]
fn a_text_style_keeps_the_words_and_the_wrap() {
    use crate::model::{TextAlign, TextFont, Typeface};
    let mut v = everything();
    {
        let t = v.model.find_text_mut("T1").unwrap();
        t.font = TextFont {
            typeface: Typeface::Serif,
            bold: true,
            italic: true,
        };
        (t.size, t.align, t.color) = (20.0, TextAlign::Center, Rgba::rgb(9, 9, 9));
    }
    v.copy_style(&Target::Text("T1".into()));
    v.select_item(&Target::Text("T2".into()), false);
    v.paste_style();
    let (t1, t2) = (
        v.model().find_text("T1").unwrap().clone(),
        v.model().find_text("T2").unwrap().clone(),
    );
    assert_eq!(
        (t2.font, t2.size, t2.align, t2.color),
        (t1.font, t1.size, t1.align, t1.color)
    );
    assert_eq!((t2.text.as_str(), t2.wrap_width), ("T2", 350.0));
    assert_eq!(v.undo_description(), Some("Paste Style"));
}

#[test]
fn the_edit_menu_copies_the_one_thing_selected() {
    let mut v = everything();
    v.select_item(&Target::Text("T2".into()), false);
    assert_eq!(v.selection_count(), 1);
    assert_eq!(v.copy_selected_style().as_deref(), Some("T2"));
    // Two things, or none: nothing to choose from.
    v.select_item(&Target::Leg("R3".into()), true);
    assert_eq!(v.copy_selected_style(), None);
    v.handle(&ViewEvent::Cancel);
    assert_eq!(v.copy_selected_style(), None);
    // A null node doesn't count.
    v.select_item(&Target::Species("_null".into()), false);
    assert_eq!(v.selection_count(), 0);
    assert_eq!(v.copy_style(&Target::Species("_null".into())), None);
}

#[test]
fn the_panel_values_are_what_is_drawn() {
    use crate::render::constants::{COMPARTMENT_BORDER_WIDTH, DEFAULT_LINE_WIDTH, LABEL_FONT_SIZE};
    use crate::render::draw::{
        compartment_colours, label_size, reaction_line_width, species_border_width,
    };
    let mut v = view();
    let s1 = v.model().find_species("S1").unwrap();
    assert_eq!(species_border_width(s1), DEFAULT_LINE_WIDTH);
    assert_eq!(label_size(s1), LABEL_FONT_SIZE);
    select(&mut v, &["S1"]);
    v.set_border_width("S1", 3.0);
    v.set_font_size(16.0);
    let s1 = v.model().find_species("S1").unwrap();
    assert_eq!((species_border_width(s1), label_size(s1)), (3.0, 16.0));
    let r = crate::model::Reaction::new("R9", pt(0.0, 0.0));
    assert_eq!(reaction_line_width(&r), DEFAULT_LINE_WIDTH);
    let c = crate::model::Compartment::new("cell");
    assert_eq!(compartment_colours(&c).2, COMPARTMENT_BORDER_WIDTH);
}

#[test]
fn a_border_width_of_0_means_no_border() {
    use crate::render::draw::{compartment_colours, species_border_width};
    let mut v = view();
    select(&mut v, &["S1"]);
    v.set_border_width("S1", 3.0);
    v.set_border_width("S1", 0.0);
    let s1 = v.model().find_species("S1").unwrap();
    assert!(s1.style.no_border);
    assert_eq!(species_border_width(s1), 0.0);
    assert_eq!(s1.style.border_width, 3.0, "kept for when it has one again");
    // Undo brings the border back; a width gives it one again.
    v.undo();
    assert_eq!(
        species_border_width(v.model().find_species("S1").unwrap()),
        3.0
    );
    v.set_border_width("S1", 0.0);
    v.set_border_width("S1", 2.0);
    let s1 = v.model().find_species("S1").unwrap();
    assert!(!s1.style.no_border);
    assert_eq!(species_border_width(s1), 2.0);
    // A compartment too.
    let mut c = crate::model::Compartment::new("cell");
    c.frame = Some(crate::model::Frame {
        center: pt(0.0, 0.0),
        width: 400.0,
        height: 200.0,
    });
    v.model.compartments.push(c);
    v.set_compartment_border_width("cell", 0.0);
    let c = v.model().find_compartment("cell").unwrap();
    assert_eq!(compartment_colours(c).2, 0.0);
    v.set_compartment_border_width("cell", 4.0);
    assert_eq!(
        compartment_colours(v.model().find_compartment("cell").unwrap()).2,
        4.0
    );
}

#[test]
fn copy_style_and_saved_styles_carry_no_border() {
    let mut v = view();
    select(&mut v, &["S1"]);
    v.set_border_width("S1", 0.0);
    v.copy_style(&Target::Species("S1".into()));
    select(&mut v, &["S2"]);
    v.paste_style();
    assert!(v.model().find_species("S2").unwrap().style.no_border);
    // A saved style keeps it, in the file too.
    v.save_style(&Target::Species("S1".into()), "Bare").unwrap();
    let text = crate::json::to_json(v.model());
    let back = crate::json::from_json(&text).unwrap();
    assert!(back.warnings.is_empty());
    let look = crate::styles::find(&back.model, StyleKind::Species, "Bare")
        .unwrap()
        .look;
    assert!(matches!(look, crate::model::Look::Species { style, .. } if style.no_border));
    assert_eq!(
        back.model.find_species("S1").unwrap().style_name.as_deref(),
        Some("Bare")
    );
}

#[test]
fn no_border_is_saved_only_when_used_and_needs_version_6() {
    let mut v = view();
    select(&mut v, &["S1"]);
    v.set_border_width("S1", 3.0);
    let text = crate::json::to_json(v.model());
    assert!(text.contains("\"version\": 3") && !text.contains("noBorder"));
    v.set_border_width("S1", 0.0);
    let mut c = crate::model::Compartment::new("cell");
    c.style.no_border = true;
    c.frame = Some(crate::model::Frame {
        center: pt(0.0, 0.0),
        width: 400.0,
        height: 200.0,
    });
    v.model.compartments.push(c);
    let text = crate::json::to_json(v.model());
    assert!(text.contains("\"version\": 6"));
    assert_eq!(text.matches("\"noBorder\": true").count(), 2);
    let back = crate::json::from_json(&text).unwrap().model;
    let s1 = back.find_species("S1").unwrap();
    assert!(s1.style.no_border);
    assert_eq!(s1.style.border_width, 3.0);
    assert!(!back.find_species("S2").unwrap().style.no_border);
    assert!(back.find_compartment("cell").unwrap().style.no_border);
    // And SBML, by PathwayDesigner's notes.
    let out = crate::sbml::export(&back);
    assert_eq!(out.xml.matches("noBorder=\"true\"").count(), 2);
    let opts = crate::antimony::ImportOptions {
        convert_catalytic: false,
        seed: 1,
    };
    let m = crate::sbml::import(&out.xml, &opts).unwrap().model;
    assert!(m.find_species("S1").unwrap().style.no_border);
    assert!(!m.find_species("S2").unwrap().style.no_border);
    assert!(m.find_compartment("cell").unwrap().style.no_border);
}
