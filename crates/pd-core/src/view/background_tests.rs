//! The diagram's background colour (S §8.8): setting it, undo, drawing,
//! output and the file.

use super::*;
use crate::model::Rgba;
use crate::render::constants::BACKGROUND;
use crate::render::draw::background_colour;

const NAVY: Rgba = Rgba::rgb(0x10, 0x20, 0x40);

#[test]
fn the_background_can_be_set_undone_and_put_back_to_the_default() {
    let mut v = DiagramView::default();
    assert_eq!(background_colour(v.model()), BACKGROUND);
    assert_eq!(v.swatch_colour(ColourField::Background), Some(BACKGROUND));
    // Through the colour picker, whatever is selected.
    let r = v.apply_colour(ColourField::Background, NAVY);
    assert!(r.model_changed);
    assert_eq!(v.undo_description(), Some("Background Colour"));
    assert_eq!(background_colour(v.model()), NAVY);
    assert_eq!(v.swatch_colour(ColourField::Background), Some(NAVY));
    // The same again changes nothing.
    assert!(!v.set_background(NAVY).model_changed);
    v.undo();
    assert_eq!(background_colour(v.model()), BACKGROUND);
    v.redo();
    // Default puts the usual one back.
    v.set_background(Rgba::DEFAULT);
    assert_eq!(background_colour(v.model()), BACKGROUND);
}

#[test]
fn it_is_drawn_on_screen_and_in_every_output() {
    let mut m = Model::new();
    m.background = NAVY;
    // Screen: the canvas is filled with it, under everything.
    let mut calls = Vec::new();
    struct Fills<'a>(&'a mut Vec<Rgba>);
    impl crate::render::Renderer for Fills<'_> {
        fn round_rect(
            &mut self,
            _: crate::render::SRect,
            _: f32,
            fill: Option<Rgba>,
            _: Option<crate::render::Stroke>,
        ) {
            self.0.extend(fill);
        }
        fn circle(
            &mut self,
            _: crate::render::SPt,
            _: f32,
            _: Option<Rgba>,
            _: Option<crate::render::Stroke>,
        ) {
        }
        fn polyline(&mut self, _: &[crate::render::SPt], _: crate::render::Stroke) {}
        fn cubic(
            &mut self,
            _: crate::render::SPt,
            _: crate::render::SPt,
            _: crate::render::SPt,
            _: crate::render::SPt,
            _: crate::render::Stroke,
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
    let v = DiagramView::new(m.clone());
    v.draw(
        &mut Fills(&mut calls),
        crate::render::SRect {
            min: crate::render::spt(0.0, 0.0),
            max: crate::render::spt(100.0, 100.0),
        },
    );
    assert_eq!(calls.first(), Some(&NAVY));
    let svg = crate::render::svg::model_to_svg(&m, 1.0, crate::render::draw::Shown::ALL);
    assert!(svg.contains("#102040"), "{svg}");
    let pdf = crate::render::pdf::model_to_pdf(
        &m,
        crate::render::pdf::PageSize::Drawing,
        false,
        crate::render::draw::Shown::ALL,
    );
    // 0x10/255, 0x20/255, 0x40/255 as the fill colour.
    let text = String::from_utf8_lossy(&pdf);
    assert!(
        text.contains("0.063 0.125 0.251 rg"),
        "no navy fill in the PDF"
    );
}

#[test]
fn the_file_keeps_it_in_version_9() {
    let mut m = Model::new();
    let text = crate::json::to_json(&m);
    assert!(!text.contains("background"));
    m.background = NAVY;
    let text = crate::json::to_json(&m);
    assert!(text.contains("\"background\": \"#102040FF\""), "{text}");
    assert!(text.contains("\"version\": 9"));
    let back = crate::json::from_json(&text).unwrap();
    assert!(back.warnings.is_empty());
    assert_eq!(back.model.background, NAVY);
}
