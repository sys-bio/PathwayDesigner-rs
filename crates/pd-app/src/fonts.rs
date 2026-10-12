//! Installing the bundled fonts into egui (SPEC-Rust §5.4).

use std::sync::Arc;

use eframe::egui::{self, FontData, FontDefinitions, FontFamily, FontId};
use pd_core::model::TextFont;
use pd_core::text::{all_fonts, font_data, font_key};

const NOTO: &str = "noto-sans";

/// The font family used for diagram labels. It holds only the bundled font, with
/// no fallbacks, so labels on screen always match what `pd_core::text` measured.
pub fn diagram_family() -> FontFamily {
    FontFamily::Name("diagram".into())
}

/// A diagram label font at `size` points.
pub fn diagram_font(size: f32) -> FontId {
    FontId::new(size, diagram_family())
}

/// One of the bundled faces (free text, S §7.8) at `size` points. Each face is
/// a family of its own holding only that font, so egui draws exactly what
/// `pd_core::text` measured.
pub fn face_font(font: TextFont, size: f32) -> FontId {
    if font == TextFont::default() {
        return diagram_font(size);
    }
    FontId::new(size, FontFamily::Name(font_key(font).into()))
}

/// The bundled font is the only font: for diagram labels and for the whole UI.
/// egui's own fonts are not built in (the `default_fonts` feature is off), so
/// monospace text uses the bundled font too until a monospace font is needed.
pub fn font_definitions() -> FontDefinitions {
    let mut fonts = FontDefinitions::empty();
    fonts.font_data.insert(
        NOTO.to_owned(),
        Arc::new(FontData::from_static(pd_core::text::FONT_DATA)),
    );
    for family in [
        FontFamily::Proportional,
        FontFamily::Monospace,
        diagram_family(),
    ] {
        fonts.families.insert(family, vec![NOTO.to_owned()]);
    }
    for font in all_fonts().into_iter().skip(1) {
        let key = font_key(font);
        fonts.font_data.insert(
            key.to_owned(),
            Arc::new(FontData::from_static(font_data(font))),
        );
        fonts
            .families
            .insert(FontFamily::Name(key.into()), vec![key.to_owned()]);
    }
    fonts
}

pub fn install(ctx: &egui::Context) {
    ctx.set_fonts(font_definitions());
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Lays out `text` with egui, the way the canvas will draw it, and returns its width.
    fn egui_width(text: &str, size: f32) -> f32 {
        egui_width_in(TextFont::default(), text, size)
    }

    fn egui_width_in(font: TextFont, text: &str, size: f32) -> f32 {
        let ctx = egui::Context::default();
        install(&ctx);
        let mut width = 0.0;
        // Fonts set with `set_fonts` take effect from the next frame.
        for _ in 0..2 {
            let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
                let galley = ui.painter().layout_no_wrap(
                    text.to_owned(),
                    face_font(font, size),
                    egui::Color32::BLACK,
                );
                width = galley.size().x;
            });
            // No renderer here to upload the font atlas to.
            output.textures_delta.clear();
        }
        width
    }

    #[test]
    fn pd_core_measures_what_egui_draws() {
        for label in ["S1", "AV", "Glucose_6_phosphate", "ATP + ADP", "x"] {
            let core = pd_core::text::text_width(label, 12.0);
            let ui = f64::from(egui_width(label, 12.0));
            // egui rounds glyph positions to whole pixels; allow for that.
            assert!(
                (core - ui).abs() <= 1.0,
                "{label}: pd-core {core}, egui {ui}"
            );
        }
    }

    #[test]
    fn every_face_measures_what_egui_draws() {
        for font in all_fonts() {
            let text = "Glucose 6-phosphate";
            let core = pd_core::text::text_width_in(font, text, 14.0);
            let ui = f64::from(egui_width_in(font, text, 14.0));
            assert!(
                (core - ui).abs() <= 1.0,
                "{}: pd-core {core}, egui {ui}",
                font_key(font)
            );
        }
    }
}
