//! Reaction names (S §5.12): showing and hiding them, where they start,
//! their hit box and drag, what carries them along, their size and colour,
//! the file, and the Escher and Classic themes. Driven with made-up events.

use super::*;
use crate::model::{Participant, Rgba};
use crate::render::constants::{JUNCTION_RADIUS, LABEL, LABEL_GAP};
use crate::render::draw::{
    clear_reaction_label_offset, content_bounds, reaction_label_box, reaction_label_colour,
    reaction_label_size,
};
use crate::text::font_metrics;
use crate::themes::Theme;

/// A at (0,0) and B at (300,0), 80 × 36, and R1: A → B, straight, its
/// junction at (150,0). Screen = world.
fn view() -> DiagramView {
    let mut m = Model::new();
    for (id, x) in [("A", 0.0), ("B", 300.0)] {
        m.push_species(crate::model::SpeciesNode::new(id, pt(x, 0.0)))
            .unwrap();
    }
    let mut r = crate::model::Reaction::new("R1", pt(150.0, 0.0));
    r.reactants = vec![Participant::new("A", 1.0)];
    r.products = vec![Participant::new("B", 1.0)];
    m.push_reaction(r).unwrap();
    DiagramView::new(m)
}

/// [`view`] with R1's name shown, and nothing selected.
fn named() -> DiagramView {
    let mut v = view();
    v.select_item(&Target::Leg("R1".into()), false);
    v.set_reaction_label_shown(true);
    v.model.clear_selection();
    v
}

fn offset(v: &DiagramView) -> Option<Pt> {
    v.model().find_reaction("R1").unwrap().label_offset
}

/// Where R1's name is drawn.
fn name_at(v: &DiagramView) -> Pt {
    v.model().find_reaction("R1").unwrap().junction + offset(v).unwrap()
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

#[test]
fn a_name_shown_starts_clear_of_the_lines() {
    let mut v = view();
    v.select_item(&Target::Leg("R1".into()), false);
    assert_eq!(v.selected_reactions_label_shown(), Some(false));
    v.set_reaction_label_shown(true);
    assert_eq!(v.undo_description(), Some("Show Name"));
    assert_eq!(v.selected_reactions_label_shown(), Some(true));
    // Its lines go left and right, so the name goes above, centred, its
    // box LABEL_GAP clear of the junction's dot and the line.
    let m = font_metrics(12.0);
    let half_h = (m.ascent - m.descent) / 2.0;
    let gap = JUNCTION_RADIUS + 1.5 / 2.0 + LABEL_GAP;
    assert_eq!(offset(&v), Some(pt(0.0, -(gap + half_h))));
    // Again: a name already shown stays where it is.
    v.model.find_reaction_mut("R1").unwrap().label_offset = Some(pt(30.0, 30.0));
    assert!(!v.set_reaction_label_shown(true).model_changed);
    assert_eq!(offset(&v), Some(pt(30.0, 30.0)));
    // Hidden, and undo shows it again.
    v.set_reaction_label_shown(false);
    assert_eq!(v.undo_description(), Some("Hide Name"));
    assert_eq!(offset(&v), None);
    v.undo();
    assert_eq!(offset(&v), Some(pt(30.0, 30.0)));
}

#[test]
fn a_vertical_reaction_has_its_name_to_the_right() {
    let mut v = view();
    v.model.find_species_mut("B").unwrap().center = pt(0.0, 300.0);
    v.model.find_reaction_mut("R1").unwrap().junction = pt(0.0, 150.0);
    v.select_item(&Target::Leg("R1".into()), false);
    v.set_reaction_label_shown(true);
    let o = offset(&v).unwrap();
    assert!(o.x > 0.0 && o.y == 0.0, "{o:?}");
}

#[test]
fn a_curve_leaves_its_junction_along_its_handles() {
    // Straight, R1's lines go left and right, and its name goes above. As a
    // Bézier whose handles leave the junction up and down, it goes right.
    let mut v = view();
    let r = v.model.find_reaction_mut("R1").unwrap();
    r.mode = crate::model::ReactionMode::Bezier;
    let a = &mut r.reactants[0];
    (a.ctrl1, a.ctrl2, a.ctrl_pts_set) = (pt(40.0, 0.0), pt(150.0, -80.0), true);
    let b = &mut r.products[0];
    (b.ctrl1, b.ctrl2, b.ctrl_pts_set) = (pt(150.0, 80.0), pt(260.0, 0.0), true);
    let r = v.model().find_reaction("R1").unwrap();
    let o = clear_reaction_label_offset(v.model(), r);
    assert!(o.x > 0.0 && o.y == 0.0, "{o:?}");
}

#[test]
fn several_reactions_agree_or_are_mixed() {
    let mut v = named();
    let mut r2 = crate::model::Reaction::new("R2", pt(150.0, 200.0));
    r2.reactants = vec![Participant::new("A", 1.0)];
    r2.products = vec![Participant::new("B", 1.0)];
    v.model.push_reaction(r2).unwrap();
    v.select_item(&Target::Leg("R1".into()), false);
    v.select_item(&Target::Leg("R2".into()), true);
    assert_eq!(v.selected_reactions_label_shown(), None);
    v.set_reaction_label_shown(true);
    assert_eq!(v.selected_reactions_label_shown(), Some(true));
    assert!(
        v.model()
            .find_reaction("R2")
            .unwrap()
            .label_offset
            .is_some()
    );
}

#[test]
fn the_name_is_hit_and_selects_its_reaction() {
    let mut v = named();
    let at = name_at(&v);
    assert_eq!(v.target_at(at), Target::ReactionLabel("R1".into()));
    press(&mut v, at, Modifiers::NONE);
    assert_eq!(v.selection().1, vec!["R1".to_string()]);
    v.handle(&ViewEvent::PointerUp {
        pos: at,
        button: Button::Primary,
    });
    assert_eq!(
        v.undo_description(),
        Some("Show Name"),
        "a click moves nothing"
    );
    // Shift-click takes it out of the selection.
    press(&mut v, at, Modifiers::SHIFT);
    assert!(v.selection().1.is_empty());
    // Hidden, there is nothing there.
    let mut v = view();
    assert_eq!(v.target_at(at), Target::Empty);
    v.select_item(&Target::Leg("R1".into()), false);
    // Its tooltip and its menu are the reaction's.
    let v = named();
    assert_eq!(
        v.tooltip_text(&Target::ReactionLabel("R1".into())),
        Some("R1".to_owned())
    );
    let mut v = named();
    let r = v.handle(&ViewEvent::PointerDown {
        pos: at,
        button: Button::Secondary,
        modifiers: Modifiers::NONE,
    });
    assert_eq!(
        r.request,
        Some(Request::ContextMenu(Target::Leg("R1".into())))
    );
}

#[test]
fn dragging_the_name_moves_only_the_name() {
    let mut v = named();
    let before = offset(&v).unwrap();
    let at = name_at(&v);
    drag(&mut v, at, at + pt(-10.0, 50.0));
    assert_eq!(
        v.model().find_reaction("R1").unwrap().junction,
        pt(150.0, 0.0)
    );
    assert_eq!(offset(&v), Some(before + pt(-10.0, 50.0)));
    assert_eq!(v.undo_description(), Some("Move Label"));
    v.undo();
    assert_eq!(offset(&v), Some(before));
    // Esc in the middle puts it back.
    press(&mut v, at, Modifiers::NONE);
    v.handle(&ViewEvent::PointerMove {
        pos: at + pt(30.0, 0.0),
    });
    assert_eq!(offset(&v), Some(before + pt(30.0, 0.0)));
    v.handle(&ViewEvent::Cancel);
    assert_eq!(offset(&v), Some(before));
}

#[test]
fn the_name_snaps_to_the_grid() {
    let mut v = named();
    v.snap_to_grid = true;
    let at = name_at(&v);
    drag(&mut v, at, at + pt(23.0, -31.0));
    let c = name_at(&v);
    assert_eq!((c.x % 20.0, c.y % 20.0), (0.0, 0.0), "{c:?}");
    assert!(c.distance(at + pt(23.0, -31.0)) <= 15.0);
}

#[test]
fn moving_the_junction_or_a_species_carries_the_name() {
    let mut v = named();
    let before = offset(&v).unwrap();
    drag(&mut v, pt(150.0, 0.0), pt(170.0, 20.0));
    assert_eq!(
        v.model().find_reaction("R1").unwrap().junction,
        pt(170.0, 20.0)
    );
    assert_eq!(offset(&v), Some(before));
    assert_eq!(
        v.target_at(pt(170.0, 20.0) + before),
        Target::ReactionLabel("R1".into())
    );
    // A Linear reaction's junction is on its line: the name follows it.
    let mut v = named();
    v.model.find_reaction_mut("R1").unwrap().mode = crate::model::ReactionMode::Linear;
    drag(&mut v, pt(300.0, 0.0), pt(300.0, 100.0));
    let m = v.model();
    let j = crate::render::draw::effective_junction(m, m.find_reaction("R1").unwrap());
    assert_eq!(v.target_at(j + before), Target::ReactionLabel("R1".into()));
}

#[test]
fn a_rubber_band_over_the_name_selects_the_reaction() {
    let mut v = named();
    let (a, b) = reaction_label_box(v.model(), v.model().find_reaction("R1").unwrap()).unwrap();
    // Round the name only, not the junction.
    let from = pt(a.x - 5.0, a.y - 20.0);
    assert_eq!(v.target_at(from), Target::Empty);
    drag(&mut v, from, pt(b.x + 5.0, a.y + 2.0));
    assert_eq!(v.selection().1, vec!["R1".to_string()]);
}

#[test]
fn the_name_is_inside_the_drawn_area() {
    let mut v = named();
    v.model.find_reaction_mut("R1").unwrap().label_offset = Some(pt(0.0, -400.0));
    let (min, _) = content_bounds(v.model());
    let (a, _) = reaction_label_box(v.model(), v.model().find_reaction("R1").unwrap()).unwrap();
    assert!(min.y <= a.y - 60.0 + 1e-9);
}

#[test]
fn the_names_size_and_colour() {
    let mut v = named();
    let r = |v: &DiagramView| v.model().find_reaction("R1").unwrap().clone();
    assert_eq!(reaction_label_size(&r(&v)), 12.0);
    assert_eq!(reaction_label_colour(&r(&v)), LABEL);
    v.select_item(&Target::Leg("R1".into()), false);
    v.set_reaction_font_size(20.0);
    v.apply_colour(ColourField::ReactionLabel, Rgba::rgb(0xCC, 0, 0));
    assert_eq!(reaction_label_size(&r(&v)), 20.0);
    assert_eq!(reaction_label_colour(&r(&v)), Rgba::rgb(0xCC, 0, 0));
    assert_eq!(
        v.swatch_colour(ColourField::ReactionLabel),
        Some(Rgba::rgb(0xCC, 0, 0))
    );
    // The line keeps its colour.
    assert_eq!(
        v.swatch_colour(ColourField::Line),
        Some(crate::render::constants::REACTION_LINE)
    );
    // A bigger name has a bigger box.
    let (a, b) = reaction_label_box(v.model(), &r(&v)).unwrap();
    let wide = b.x - a.x;
    v.set_reaction_font_size(10.0);
    let (a, b) = reaction_label_box(v.model(), &r(&v)).unwrap();
    assert!(b.x - a.x < wide);
    // Species are left alone.
    assert!(!v.model().find_species("A").unwrap().style.has_custom_style);
}

#[test]
fn paste_style_carries_the_look_but_not_the_name() {
    let mut v = named();
    let mut r2 = crate::model::Reaction::new("R2", pt(150.0, 200.0));
    r2.reactants = vec![Participant::new("A", 1.0)];
    r2.products = vec![Participant::new("B", 1.0)];
    v.model.push_reaction(r2).unwrap();
    v.select_item(&Target::Leg("R1".into()), false);
    v.set_reaction_font_size(20.0);
    v.copy_style(&Target::ReactionLabel("R1".into()));
    v.select_item(&Target::Leg("R2".into()), false);
    v.paste_style();
    let r2 = v.model().find_reaction("R2").unwrap();
    assert_eq!(reaction_label_size(r2), 20.0);
    assert_eq!(r2.label_offset, None, "showing a name isn't part of a look");
}

#[test]
fn names_are_saved_only_when_shown_and_need_version_7() {
    let v = view();
    let text = crate::json::to_json(v.model());
    assert!(!text.contains("labelDx"));
    assert!(text.contains("\"version\": 3"));
    let v = named();
    let text = crate::json::to_json(v.model());
    assert!(text.contains("\"version\": 7"));
    let back = crate::json::from_json(&text).unwrap();
    assert!(back.warnings.is_empty(), "{:?}", back.warnings);
    assert_eq!(
        back.model.find_reaction("R1").unwrap().label_offset,
        offset(&v)
    );
    // Half a position is dropped, with a warning.
    let text = text.replace("\"labelDy\"", "\"labelDz\"");
    let back = crate::json::from_json(&text).unwrap();
    assert_eq!(back.model.find_reaction("R1").unwrap().label_offset, None);
    assert_eq!(back.warnings.len(), 1, "{:?}", back.warnings);
}

#[test]
fn escher_shows_every_name_and_classic_hides_them() {
    let mut v = view();
    v.apply_theme(Theme::Escher);
    let r = v.model().find_reaction("R1").unwrap();
    assert!(r.label_offset.is_some());
    assert_eq!(reaction_label_size(r), 16.0);
    assert_eq!(reaction_label_colour(r), Rgba::rgb(0x33, 0x4E, 0x75));
    // It starts clear of the curve's lines, for its size.
    assert_eq!(
        r.label_offset,
        Some(clear_reaction_label_offset(v.model(), r))
    );
    // A reaction drawn now shows its name too.
    v.set_tool(Tool::UNI_UNI);
    for at in [pt(0.0, 0.0), pt(300.0, 0.0)] {
        press(&mut v, at, Modifiers::NONE);
    }
    let new = v.model().reactions().last().unwrap();
    assert_ne!(new.id, "R1");
    assert!(new.label_offset.is_some());
    v.set_tool(Tool::Select);
    // Classic hides them; undo shows them again.
    v.apply_theme(Theme::Classic);
    assert!(
        v.model()
            .reactions()
            .iter()
            .all(|r| r.label_offset.is_none())
    );
    v.undo();
    assert!(
        v.model()
            .reactions()
            .iter()
            .all(|r| r.label_offset.is_some())
    );
}

#[test]
fn a_name_already_shown_keeps_its_gap_under_escher() {
    // R1's name dragged below its junction, at 12; Escher makes it 16, and
    // its top stays as far from the junction.
    let mut v = named();
    v.model.find_reaction_mut("R1").unwrap().label_offset = Some(pt(0.0, 40.0));
    let top = |v: &DiagramView| {
        let r = v.model().find_reaction("R1").unwrap();
        reaction_label_box(v.model(), r).unwrap().0.y - r.junction.y
    };
    let before = top(&v);
    v.apply_theme(Theme::Escher);
    assert!((top(&v) - before).abs() < 1e-9);
    assert_eq!(offset(&v).unwrap().x, 0.0);
}

#[test]
fn names_come_back_from_sbml() {
    let mut v = named();
    v.model.find_reaction_mut("R1").unwrap().label_offset = Some(pt(10.0, -30.0));
    // R2, Linear, its name placed from the junction drawn on its line.
    let mut r2 = crate::model::Reaction::new("R2", pt(150.0, 500.0));
    r2.mode = crate::model::ReactionMode::Linear;
    r2.reactants = vec![Participant::new("A", 1.0)];
    r2.products = vec![Participant::new("B", 1.0)];
    r2.label_offset = Some(pt(0.0, 25.0));
    v.model.push_reaction(r2).unwrap();
    // R3, its name not shown.
    let mut r3 = crate::model::Reaction::new("R3", pt(150.0, 200.0));
    r3.reactants = vec![Participant::new("B", 1.0)];
    r3.products = vec![Participant::new("A", 1.0)];
    v.model.push_reaction(r3).unwrap();
    let out = crate::sbml::export(v.model());
    assert!(out.xml.contains("layout:graphicalObject=\"rg_R1\""));
    assert!(!out.xml.contains("layout:graphicalObject=\"rg_R3\""));
    let opts = crate::antimony::ImportOptions {
        convert_catalytic: false,
        seed: 1,
    };
    let back = crate::sbml::import(&out.xml, &opts).unwrap().model;
    let close = |id: &str, want: Pt| {
        let o = back.find_reaction(id).unwrap().label_offset;
        assert!(o.is_some_and(|o| o.distance(want) < 1e-6), "{id}: {o:?}");
    };
    close("R1", pt(10.0, -30.0));
    close("R2", pt(0.0, 25.0));
    assert_eq!(back.find_reaction("R3").unwrap().label_offset, None);
    // Another tool may leave out originOfText: the glyph it labels is enough.
    let plain = out.xml.replace(" layout:originOfText=\"R1\"", "");
    assert_ne!(plain, out.xml);
    let back = crate::sbml::import(&plain, &opts).unwrap().model;
    let o = back.find_reaction("R1").unwrap().label_offset;
    assert!(o.is_some_and(|o| o.distance(pt(10.0, -30.0)) < 1e-6));
}
