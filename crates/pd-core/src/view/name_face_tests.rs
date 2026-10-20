//! Bold and italic names (S §5.11.1): setting them, measuring and drawing
//! them in their face, styles carrying them, and files.

use super::*;
use crate::model::{Participant, Reaction, Rgba, SpeciesNode, TextFont};
use crate::render::draw::{name_font, outside_label_box, reaction_label_box, width_for_name};
use crate::text::text_width_in;

/// S1 (0,0) → S2 (300,0), R1 at (150,0) showing its name; S3 (0,200) spare.
fn view() -> DiagramView {
    let mut m = Model::new();
    for (id, x, y) in [("S1", 0.0, 0.0), ("S2", 300.0, 0.0), ("S3", 0.0, 200.0)] {
        m.push_species(SpeciesNode::new(id, pt(x, y))).unwrap();
    }
    let mut r = Reaction::new("R1", pt(150.0, 0.0));
    r.reactants = vec![Participant::new("S1", 1.0)];
    r.products = vec![Participant::new("S2", 1.0)];
    r.label_offset = Some(pt(0.0, -30.0));
    m.push_reaction(r).unwrap();
    DiagramView::new(m)
}

fn bold_italic() -> TextFont {
    TextFont {
        bold: true,
        italic: true,
        ..TextFont::default()
    }
}

#[test]
fn names_are_made_bold_and_italic_for_the_selection() {
    let mut v = view();
    v.select_item(&Target::Species("S1".into()), false);
    v.select_item(&Target::Species("S3".into()), true);
    assert_eq!(v.selected_names_bold_italic(true), (false, false));
    v.set_name_bold(true, true);
    assert_eq!(v.undo_description(), Some("Style"));
    v.set_name_italic(true, true);
    assert_eq!(v.selected_names_bold_italic(true), (true, true));
    let s1 = v.model().find_species("S1").unwrap();
    assert_eq!(name_font(&s1.style), bold_italic());
    // Only the selection, and only species.
    assert_eq!(
        name_font(&v.model().find_species("S2").unwrap().style),
        TextFont::default()
    );
    assert_eq!(
        name_font(&v.model().find_reaction("R1").unwrap().style),
        TextFont::default()
    );
    // Not all of a mixed selection: unticked.
    v.select_item(&Target::Species("S2".into()), true);
    assert_eq!(v.selected_names_bold_italic(true), (false, false));
    // Reactions on their own.
    v.select_item(&Target::Leg("R1".into()), false);
    v.set_name_italic(false, true);
    assert_eq!(v.selected_names_bold_italic(false), (false, true));
    // Undo, one step at a time.
    v.undo();
    assert_eq!(v.selected_names_bold_italic(false), (false, false));
    v.undo();
    assert!(!v.model().find_species("S1").unwrap().style.label_italic);
    // Setting what is already set is no step.
    let before = v.undo_description();
    v.select_item(&Target::Species("S1".into()), false);
    v.set_name_bold(true, true);
    assert_eq!(v.undo_description(), before);
}

#[test]
fn names_are_measured_in_their_face() {
    let mut v = view();
    let id = "S1";
    v.model.find_species_mut(id).unwrap().label_offset = Some(pt(0.0, 40.0));
    let width = |v: &DiagramView| {
        let (a, b) = outside_label_box(v.model().find_species(id).unwrap()).unwrap();
        b.x - a.x
    };
    let fit = |v: &DiagramView| width_for_name(v.model().find_species(id).unwrap());
    let rbox = |v: &DiagramView| {
        let m = v.model();
        let (a, b) = reaction_label_box(m, m.find_reaction("R1").unwrap()).unwrap();
        b.x - a.x
    };
    let (w0, f0, r0) = (width(&v), fit(&v), rbox(&v));
    v.select_item(&Target::Species(id.into()), false);
    v.set_name_bold(true, true);
    v.select_item(&Target::Leg("R1".into()), false);
    v.set_name_bold(false, true);
    // Bold Noto is wider than regular.
    let bold = TextFont {
        bold: true,
        ..TextFont::default()
    };
    let grow = text_width_in(bold, id, 12.0) - text_width_in(TextFont::default(), id, 12.0);
    assert!(grow > 0.0);
    assert!((width(&v) - w0 - grow).abs() < 1e-9);
    assert!((fit(&v) - f0 - grow).abs() < 1e-9);
    assert!(rbox(&v) > r0);
    // A rename widens the node by the bold name's growth.
    let s = v.model.find_species_mut(id).unwrap();
    s.label_offset = None;
    s.width = width_for_name(s);
    let before = v.model().find_species(id).unwrap().width;
    v.rename(id, "S1longer").unwrap();
    let after = v.model().find_species("S1longer").unwrap().width;
    let expected = text_width_in(bold, "S1longer", 12.0) - text_width_in(bold, id, 12.0);
    assert!(
        (after - before - expected).abs() < 1e-6,
        "{before} {after} {expected}"
    );
}

#[test]
fn names_are_drawn_in_their_face() {
    use crate::render::{Renderer, SPt, SRect, Stroke};
    /// The fonts each text is drawn in.
    struct Fonts(Vec<(String, TextFont)>);
    impl Renderer for Fonts {
        fn round_rect(&mut self, _: SRect, _: f32, _: Option<Rgba>, _: Option<Stroke>) {}
        fn circle(&mut self, _: SPt, _: f32, _: Option<Rgba>, _: Option<Stroke>) {}
        fn polyline(&mut self, _: &[SPt], _: Stroke) {}
        fn cubic(&mut self, _: SPt, _: SPt, _: SPt, _: SPt, _: Stroke) {}
        fn polygon(&mut self, _: &[SPt], _: Rgba) {}
        fn text(&mut self, _: SPt, s: &str, _: f32, font: TextFont, _: Rgba) {
            self.0.push((s.to_owned(), font));
        }
    }
    let mut v = view();
    v.select_item(&Target::Species("S1".into()), false);
    v.set_name_bold(true, true);
    v.set_name_italic(true, true);
    v.select_item(&Target::Leg("R1".into()), false);
    v.set_name_italic(false, true);
    let mut out = Fonts(Vec::new());
    crate::render::draw::draw_model(
        v.model(),
        &crate::render::draw::DrawOptions::default(),
        &mut out,
    );
    let font = |s: &str| out.0.iter().find(|(t, _)| t == s).unwrap().1;
    assert_eq!(font("S1"), bold_italic());
    assert_eq!(font("S2"), TextFont::default());
    assert_eq!(
        font("R1"),
        TextFont {
            italic: true,
            ..TextFont::default()
        }
    );
}

#[test]
fn styles_carry_bold_and_italic() {
    let mut v = view();
    v.select_item(&Target::Species("S1".into()), false);
    v.set_name_bold(true, true);
    // Copy Style and Paste Style.
    v.copy_style(&Target::Species("S1".into()));
    v.select_item(&Target::Species("S2".into()), false);
    v.paste_style();
    assert!(v.model().find_species("S2").unwrap().style.label_bold);
    // A saved style.
    v.save_style(&Target::Species("S1".into()), "Bold one")
        .unwrap();
    v.select_item(&Target::Species("S3".into()), false);
    v.apply_style(crate::model::StyleKind::Species, "Bold one");
    let s3 = v.model().find_species("S3").unwrap();
    assert!(s3.style.label_bold);
    assert_eq!(s3.style_name.as_deref(), Some("Bold one"));
    // Unbolding it leaves the style.
    v.set_name_bold(true, false);
    assert_eq!(v.model().find_species("S3").unwrap().style_name, None);
}

#[test]
fn files_keep_bold_and_italic_names_in_version_14() {
    use crate::json::{from_json, needed_version, to_json};
    let mut v = view();
    let plain = to_json(v.model());
    assert!(!plain.contains("\"bold\"") && !plain.contains("\"italic\""));
    assert!(needed_version(v.model()) < 14);
    v.select_item(&Target::Species("S1".into()), false);
    v.set_name_bold(true, true);
    v.select_item(&Target::Leg("R1".into()), false);
    v.set_name_italic(false, true);
    assert_eq!(needed_version(v.model()), 14);
    let text = to_json(v.model());
    let json: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(json["version"], 14);
    assert_eq!(json["species"][0]["style"]["bold"], true);
    assert!(json["species"][0]["style"].get("italic").is_none());
    assert_eq!(json["reactions"][0]["style"]["italic"], true);
    let back = from_json(&text).unwrap().model;
    assert!(back.find_species("S1").unwrap().style.label_bold);
    assert!(back.find_reaction("R1").unwrap().style.label_italic);
    // A saved style alone also needs version 14.
    let mut w = view();
    w.select_item(&Target::Species("S1".into()), false);
    w.set_name_bold(true, true);
    w.save_style(&Target::Species("S1".into()), "B").unwrap();
    w.model.find_species_mut("S1").unwrap().style = crate::model::VisualStyle::default();
    w.model.find_species_mut("S1").unwrap().style_name = None;
    assert_eq!(needed_version(w.model()), 14);
}

#[test]
fn a_style_that_isnt_custom_gives_the_plain_face() {
    let style = crate::model::VisualStyle {
        label_bold: true,
        label_italic: true,
        ..crate::model::VisualStyle::default()
    };
    assert!(!style.has_custom_style);
    assert_eq!(name_font(&style), TextFont::default());
}

#[test]
fn a_bold_name_outside_moves_by_its_bold_growth_on_rename() {
    let mut v = view();
    v.model.find_species_mut("S1").unwrap().label_offset = Some(pt(60.0, 0.0));
    v.select_item(&Target::Species("S1".into()), false);
    v.set_name_bold(true, true);
    v.rename("S1", "Glucose").unwrap();
    let bold = TextFont {
        bold: true,
        ..TextFont::default()
    };
    let grow = text_width_in(bold, "Glucose", 12.0) - text_width_in(bold, "S1", 12.0);
    let o = v
        .model()
        .find_species("Glucose")
        .unwrap()
        .label_offset
        .unwrap();
    assert!((o.x - 60.0 - grow / 2.0).abs() < 1e-9, "{o:?} {grow}");
}
