//! Free text in the view (S §7.8): adding, typing, moving, wrapping,
//! selecting, deleting and aligning, driven with made-up events.

use super::*;
use crate::model::{Rgba, TextAlign, TextItem, Typeface};
use crate::render::draw::wrap_handle;
use crate::textbox::{lay_out, text_box};
use crate::view::{Arrange, ColourField};

/// One species, S1 at (0,0); view at zoom 1, so screen = world.
fn view() -> DiagramView {
    let mut m = Model::new();
    m.add_species(pt(0.0, 0.0), 80.0, 36.0);
    DiagramView::new(m)
}

fn press(v: &mut DiagramView, at: Pt, modifiers: Modifiers) -> ViewResponse {
    v.handle(&ViewEvent::PointerDown {
        pos: at,
        button: Button::Primary,
        modifiers,
    })
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

fn text<'a>(v: &'a DiagramView, id: &str) -> &'a TextItem {
    v.model().find_text(id).unwrap()
}

/// Adds a text at (200, 100) with the Text tool; returns its id.
fn add(v: &mut DiagramView) -> Id {
    v.set_tool(Tool::AddText);
    let r = press(v, pt(200.0, 100.0), Modifiers::NONE);
    let Some(Request::EditText(id)) = r.request else {
        panic!("adding text asks for its words: {r:?}");
    };
    id
}

#[test]
fn the_text_tool_places_selected_text_and_goes_back_to_select() {
    let mut v = view();
    let id = add(&mut v);
    let t = text(&v, &id);
    assert_eq!(t.position, pt(200.0, 100.0));
    assert_eq!(t.text, "Text");
    assert!(t.selected);
    assert_eq!(v.tool(), Tool::Select);
    assert_eq!(v.undo_description(), Some("Add Text"));
}

#[test]
fn typing_after_adding_is_one_undo_step() {
    let mut v = view();
    let id = add(&mut v);
    for words in ["G", "Gl", "Glycolysis"] {
        v.set_text(&id, words);
    }
    assert_eq!(text(&v, &id).text, "Glycolysis");
    v.undo();
    assert!(v.model().texts().is_empty(), "one undo takes it all away");
    v.redo();
    assert_eq!(text(&v, &id).text, "Glycolysis");

    // Typing later is its own step, and a run of it is one step.
    v.set_text(&id, "Glycolysis!");
    v.set_text(&id, "Glycolysis!!");
    assert_eq!(v.undo_description(), Some("Edit Text"));
    v.undo();
    assert_eq!(text(&v, &id).text, "Glycolysis");
}

#[test]
fn text_is_hit_on_top_and_dragged() {
    let mut v = view();
    let id = add(&mut v);
    v.handle(&ViewEvent::Cancel);
    let inside = pt(205.0, 105.0);
    assert_eq!(v.target_at(inside), Target::Text(id.clone()));
    drag(&mut v, inside, inside + pt(50.0, 20.0));
    assert_eq!(text(&v, &id).position, pt(250.0, 120.0));
    assert!(text(&v, &id).selected);
    v.undo();
    assert_eq!(text(&v, &id).position, pt(200.0, 100.0));

    // A text over a species is hit first: it is drawn on top.
    let s = v.model().find_species("S1").unwrap().center;
    v.set_tool(Tool::AddText);
    press(&mut v, s, Modifiers::NONE);
    let over = v.model().texts().last().unwrap().id.clone();
    assert_eq!(v.target_at(s + pt(2.0, 2.0)), Target::Text(over));
}

#[test]
fn the_wrap_handle_sets_the_wrap_width() {
    let mut v = view();
    let id = add(&mut v);
    v.set_text(&id, "the quick brown fox jumps over the lazy dog");
    let one_line = lay_out(text(&v, &id)).lines.len();
    assert_eq!(one_line, 1);
    let h = wrap_handle(text(&v, &id));
    assert_eq!(v.target_at(h), Target::TextWrap(id.clone()));
    let width = lay_out(text(&v, &id)).width;
    drag(&mut v, h, h - pt(width - 100.0, 0.0));
    let t = text(&v, &id);
    assert!((t.wrap_width - 100.0).abs() < 1e-9, "{}", t.wrap_width);
    assert!(lay_out(t).lines.len() > 1);
    assert_eq!(v.undo_description(), Some("Wrap Text"));
    v.undo();
    assert_eq!(text(&v, &id).wrap_width, 0.0);
    // Never narrower than the minimum.
    v.select_item(&Target::Text(id.clone()), false);
    let h = wrap_handle(text(&v, &id));
    drag(&mut v, h, h - pt(10_000.0, 0.0));
    assert_eq!(text(&v, &id).wrap_width, crate::textbox::MIN_WRAP_WIDTH);
}

#[test]
fn rubber_band_select_all_and_delete_include_text() {
    let mut v = view();
    let id = add(&mut v);
    v.handle(&ViewEvent::Cancel);
    // A band around the text only.
    drag(&mut v, pt(150.0, 60.0), pt(300.0, 160.0));
    assert_eq!(v.selected_text_ids(), std::slice::from_ref(&id));
    assert!(v.selection().0.is_empty());
    v.select_all();
    assert_eq!(v.selected_text_ids(), std::slice::from_ref(&id));
    v.select_all_species();
    assert!(v.selected_text_ids().is_empty());
    v.select_item(&Target::Text(id.clone()), false);
    v.delete_selection();
    assert!(v.model().texts().is_empty());
    v.undo();
    assert_eq!(v.model().texts().len(), 1);
}

#[test]
fn double_click_edits_and_right_click_selects() {
    let mut v = view();
    let id = add(&mut v);
    v.handle(&ViewEvent::Cancel);
    let inside = pt(205.0, 105.0);
    let r = v.handle(&ViewEvent::DoubleClick { pos: inside });
    assert_eq!(r.request, Some(Request::EditText(id.clone())));
    let r = v.handle(&ViewEvent::PointerDown {
        pos: inside,
        button: Button::Secondary,
        modifiers: Modifiers::NONE,
    });
    assert_eq!(
        r.request,
        Some(Request::ContextMenu(Target::Text(id.clone())))
    );
    assert!(text(&v, &id).selected);
}

#[test]
fn style_edits_apply_to_the_selected_texts() {
    let mut v = view();
    let a = add(&mut v);
    let b = add(&mut v);
    v.select_all();
    v.set_text_typeface(Typeface::Serif);
    v.set_text_bold(true);
    v.set_text_italic(true);
    v.set_text_size(24.0);
    v.set_text_align(TextAlign::Right);
    v.apply_colour(ColourField::Text, Rgba::rgb(0xCC, 0, 0));
    for id in [&a, &b] {
        let t = text(&v, id);
        assert_eq!(t.font.typeface, Typeface::Serif);
        assert!(t.font.bold && t.font.italic);
        assert_eq!(t.size, 24.0);
        assert_eq!(t.align, TextAlign::Right);
        assert_eq!(t.color, Rgba::rgb(0xCC, 0, 0));
    }
    assert_eq!(
        v.swatch_colour(ColourField::Text),
        Some(Rgba::rgb(0xCC, 0, 0))
    );
    // A size below 1 is ignored; one undo per edit.
    v.set_text_size(0.0);
    assert_eq!(text(&v, &a).size, 24.0);
    v.undo();
    assert_eq!(text(&v, &a).color, Rgba::DEFAULT);
    // Wrap width: 0 turns wrapping off; tiny widths go up to the minimum.
    v.set_text_wrap(&a, 3.0);
    assert_eq!(text(&v, &a).wrap_width, crate::textbox::MIN_WRAP_WIDTH);
    v.set_text_wrap(&a, 0.0);
    assert_eq!(text(&v, &a).wrap_width, 0.0);
}

#[test]
fn text_aligns_and_nudges_with_species() {
    let mut v = view();
    let id = add(&mut v);
    v.select_all();
    v.arrange(Arrange::Left);
    // Left edges together: the species' at −40, the text box's too.
    let (min, _) = text_box(text(&v, &id));
    let s = v.model().find_species("S1").unwrap();
    assert!((min.x - (s.center.x - s.width / 2.0)).abs() < 1e-9);
    let before = text(&v, &id).position;
    v.nudge(pt(1.0, 0.0));
    assert_eq!(text(&v, &id).position, before + pt(1.0, 0.0));
}
