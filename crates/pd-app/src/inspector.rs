//! The inspector panel (S §8.1, §8.2, SPEC-Rust §11.2).
//!
//! egui rebuilds it every frame from the selection, so there is no
//! feedback-loop guard to get wrong. Text fields keep a *draft* while the user
//! types; the draft is applied, as one undo step, when the field loses focus
//! or Enter is pressed. A draft remembers which item it was typed for: if the
//! selection changes first, it still goes to that item, not the new one.

use eframe::egui::{self, Color32, RichText, Sense, Vec2};
use pd_core::expr;
use pd_core::json::colour::{from_hex, to_hex};
use pd_core::model::Rgba;
use pd_core::render::constants::PICKER_SWATCHES;
use pd_core::view::{ColourField, DiagramView};

use crate::renderer::color;

/// Which item property a text field edits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Field {
    Concentration,
    BorderWidth,
    LineWidth,
    KineticLaw,
}

/// The text being typed into one field, and the item it is for.
#[derive(Default)]
struct Draft {
    target: Option<String>,
    text: String,
    dirty: bool,
}

#[derive(Default)]
pub struct Inspector {
    concentration: Draft,
    border_width: Draft,
    line_width: Draft,
    law: Draft,
    /// The colour picker's hex field.
    hex: String,
}

const ERROR: Color32 = Color32::from_rgb(0xCC, 0x33, 0x00);
const SWATCH: Vec2 = Vec2::new(28.0, 18.0);

impl Inspector {
    pub fn show(&mut self, ui: &mut egui::Ui, view: &mut DiagramView) {
        let (species, reactions) = view.selection();
        let one_species = (species.len() == 1).then(|| species[0].clone());
        let one_reaction = (reactions.len() == 1).then(|| reactions[0].clone());

        // Drafts typed for an item that is no longer the one shown go to that
        // item first (SPEC-Rust §11.2).
        for (field, target) in [
            (Field::Concentration, &one_species),
            (Field::BorderWidth, &one_species),
            (Field::LineWidth, &one_reaction),
            (Field::KineticLaw, &one_reaction),
        ] {
            if self.draft(field).target != *target {
                self.commit(field, view);
                let d = self.draft_mut(field);
                d.target = target.clone();
                d.dirty = false;
            }
        }

        ui.heading("Species");
        ui.add_enabled_ui(!species.is_empty(), |ui| {
            egui::Grid::new("species")
                .num_columns(2)
                .spacing([8.0, 6.0])
                .show(ui, |ui| {
                    ui.label("Id");
                    ui.label(match &one_species {
                        Some(id) => view.rename_initial(id).unwrap_or_default(),
                        None if species.is_empty() => String::new(),
                        None => format!("{} selected", species.len()),
                    });
                    ui.end_row();

                    ui.label("Fill");
                    self.swatch(ui, view, ColourField::Fill);
                    ui.end_row();
                    ui.label("Border");
                    self.swatch(ui, view, ColourField::Border);
                    ui.end_row();

                    ui.label("Border width");
                    self.number(
                        ui,
                        view,
                        Field::BorderWidth,
                        one_species.is_some(),
                        "0 = default",
                    );
                    ui.end_row();
                    ui.label("Concentration");
                    self.number(ui, view, Field::Concentration, one_species.is_some(), "");
                    ui.end_row();

                    ui.label("Boundary");
                    let all = !species.is_empty()
                        && species.iter().all(|id| {
                            let m = view.model();
                            let p = m.find_species(id).map(|s| s.primary_id().to_owned());
                            p.and_then(|p| m.find_species(&p))
                                .is_some_and(|s| s.is_boundary)
                        });
                    let mut boundary = all;
                    if ui.checkbox(&mut boundary, "").changed() {
                        view.set_boundary(boundary);
                    }
                    ui.end_row();
                });
        });

        ui.add_space(8.0);
        ui.separator();
        ui.heading("Reaction");
        ui.add_enabled_ui(!reactions.is_empty(), |ui| {
            egui::Grid::new("reaction")
                .num_columns(2)
                .spacing([8.0, 6.0])
                .show(ui, |ui| {
                    ui.label("Id");
                    ui.label(match &one_reaction {
                        Some(id) => id.clone(),
                        None if reactions.is_empty() => String::new(),
                        None => format!("{} selected", reactions.len()),
                    });
                    ui.end_row();

                    ui.label("Line");
                    self.swatch(ui, view, ColourField::Line);
                    ui.end_row();
                    ui.label("Line width");
                    self.number(
                        ui,
                        view,
                        Field::LineWidth,
                        one_reaction.is_some(),
                        "0 = default",
                    );
                    ui.end_row();

                    ui.label("Show junction");
                    let mut visible = !reactions.is_empty() && view.all_junctions_visible();
                    if ui.checkbox(&mut visible, "").changed() {
                        view.set_junction_visible(visible);
                    }
                    ui.end_row();
                });
            ui.label("Rate law");
            self.law_field(ui, view, one_reaction.is_some());
        });
    }

    fn draft(&self, field: Field) -> &Draft {
        match field {
            Field::Concentration => &self.concentration,
            Field::BorderWidth => &self.border_width,
            Field::LineWidth => &self.line_width,
            Field::KineticLaw => &self.law,
        }
    }

    fn draft_mut(&mut self, field: Field) -> &mut Draft {
        match field {
            Field::Concentration => &mut self.concentration,
            Field::BorderWidth => &mut self.border_width,
            Field::LineWidth => &mut self.line_width,
            Field::KineticLaw => &mut self.law,
        }
    }

    /// The field's value in the model, as text.
    fn model_text(field: Field, target: &str, view: &DiagramView) -> String {
        let m = view.model();
        let species = || {
            m.find_species(target)
                .and_then(|s| m.find_species(s.primary_id()).or(Some(s)))
        };
        match field {
            Field::Concentration => species().map(|s| s.initial_value.to_string()),
            Field::BorderWidth => m
                .find_species(target)
                .map(|s| s.style.border_width.to_string()),
            Field::LineWidth => m
                .find_reaction(target)
                .map(|r| r.style.line_width.to_string()),
            Field::KineticLaw => m.find_reaction(target).map(|r| r.kinetic_law.clone()),
        }
        .unwrap_or_default()
    }

    /// Applies a field's draft to the item it was typed for, if it changed.
    /// A number that doesn't read is dropped, and the field shows the model's
    /// value again.
    fn commit(&mut self, field: Field, view: &mut DiagramView) {
        let d = self.draft_mut(field);
        if !d.dirty {
            return;
        }
        d.dirty = false;
        let Some(target) = d.target.clone() else {
            return;
        };
        let text = d.text.trim().to_owned();
        let number = text.parse::<f64>().ok();
        match (field, number) {
            (Field::Concentration, Some(v)) => {
                view.set_initial_value(&target, v);
            }
            (Field::BorderWidth, Some(v)) => {
                view.set_border_width(&target, v);
            }
            (Field::LineWidth, Some(v)) => {
                view.set_line_width(&target, v);
            }
            (Field::KineticLaw, _) => {
                view.set_kinetic_law(&target, &text);
            }
            _ => {}
        }
    }

    /// Shows the model's value unless the user is typing.
    fn refresh(&mut self, field: Field, view: &DiagramView) {
        let d = self.draft_mut(field);
        if d.dirty {
            return;
        }
        d.text = match &d.target {
            Some(t) => Self::model_text(field, t, view),
            None => String::new(),
        };
    }

    fn number(
        &mut self,
        ui: &mut egui::Ui,
        view: &mut DiagramView,
        field: Field,
        enabled: bool,
        hint: &str,
    ) {
        self.refresh(field, view);
        let d = self.draft_mut(field);
        let valid = !d.dirty || d.text.trim().parse::<f64>().is_ok_and(f64::is_finite);
        let edit = egui::TextEdit::singleline(&mut d.text)
            .desired_width(90.0)
            .hint_text(hint)
            .text_color_opt((!valid).then_some(ERROR));
        let r = ui.add_enabled(enabled, edit);
        if r.changed() {
            d.dirty = true;
        }
        if r.lost_focus() {
            self.commit(field, view);
        }
    }

    /// The rate-law field: a parse error shows under it, but the law is
    /// still stored (S §8.4).
    fn law_field(&mut self, ui: &mut egui::Ui, view: &mut DiagramView, enabled: bool) {
        self.refresh(Field::KineticLaw, view);
        let d = &mut self.law;
        let r = ui.add_enabled(
            enabled,
            egui::TextEdit::multiline(&mut d.text)
                .desired_rows(2)
                .desired_width(f32::INFINITY)
                .code_editor()
                .hint_text(if enabled {
                    "e.g. k1*S1"
                } else {
                    "select one reaction"
                }),
        );
        if r.changed() {
            d.dirty = true;
        }
        // Enter applies; Shift+Enter would be a new line, which a law never needs.
        let enter = r.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        if enter {
            d.text = d.text.replace('\n', "");
            r.surrender_focus();
        }
        if r.lost_focus() || enter {
            self.commit(Field::KineticLaw, view);
        }
        let text = self.law.text.trim();
        if enabled
            && !text.is_empty()
            && let Err(e) = expr::parse(text)
        {
            ui.label(
                RichText::new(format!("Doesn't parse: {e}"))
                    .color(ERROR)
                    .small(),
            );
        }
    }

    /// A colour swatch showing the effective colour; a click opens the picker
    /// below it (S §8.2).
    fn swatch(&mut self, ui: &mut egui::Ui, view: &mut DiagramView, field: ColourField) {
        let current = view.swatch_colour(field);
        let (rect, resp) = ui.allocate_exact_size(SWATCH, Sense::click());
        let fill = current.map_or(Color32::TRANSPARENT, color);
        ui.painter().rect_filled(rect, 3.0, fill);
        ui.painter().rect_stroke(
            rect,
            3.0,
            ui.visuals().widgets.inactive.fg_stroke,
            egui::StrokeKind::Inside,
        );
        let resp = resp.on_hover_text("Choose a colour");
        if resp.clicked() {
            self.hex = current
                .map(|c| to_hex(c)[..7].to_owned())
                .unwrap_or_default();
        }
        egui::Popup::from_toggle_button_response(&resp)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .show(|ui| {
                if let Some(c) = self.picker(ui, current) {
                    view.apply_colour(field, c);
                    ui.close();
                }
            });
    }

    /// The colour picker: 40 swatches, a preview with its `#RRGGBB` value, and
    /// a hex field (red while invalid). Returns the colour chosen, if any.
    fn picker(&mut self, ui: &mut egui::Ui, current: Option<Rgba>) -> Option<Rgba> {
        let mut chosen = None;
        egui::Grid::new("swatches")
            .spacing([3.0, 3.0])
            .show(ui, |ui| {
                for row in PICKER_SWATCHES {
                    for rgb in row {
                        let c = Rgba::rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8);
                        let (rect, r) = ui.allocate_exact_size(Vec2::splat(20.0), Sense::click());
                        ui.painter().rect_filled(rect, 2.0, color(c));
                        let stroke = if Some(c) == current {
                            egui::Stroke::new(2.0, Color32::BLACK)
                        } else {
                            egui::Stroke::new(1.0, Color32::from_gray(160))
                        };
                        ui.painter()
                            .rect_stroke(rect, 2.0, stroke, egui::StrokeKind::Inside);
                        if r.on_hover_text(&to_hex(c)[..7]).clicked() {
                            chosen = Some(c);
                        }
                    }
                    ui.end_row();
                }
            });
        ui.separator();
        let parsed = parse_hex6(&self.hex);
        ui.horizontal(|ui| {
            let preview = parsed.or(current).map_or(Color32::TRANSPARENT, color);
            let (rect, _) = ui.allocate_exact_size(Vec2::new(36.0, 20.0), Sense::hover());
            ui.painter().rect_filled(rect, 2.0, preview);
            let edit = ui.add(
                egui::TextEdit::singleline(&mut self.hex)
                    .desired_width(80.0)
                    .hint_text("#RRGGBB")
                    .text_color_opt(parsed.is_none().then_some(ERROR)),
            );
            let enter = edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if (ui
                .add_enabled(parsed.is_some(), egui::Button::new("Apply"))
                .clicked()
                || enter)
                && let Some(c) = parsed
            {
                chosen = Some(c);
            }
        });
        chosen
    }
}

/// `#RRGGBB` (the `#` optional), opaque. Only six digits: the picker never
/// sets transparency.
fn parse_hex6(s: &str) -> Option<Rgba> {
    let t = s.trim();
    let digits = t.strip_prefix('#').unwrap_or(t);
    if digits.len() != 6 {
        return None;
    }
    from_hex(digits)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_field_accepts_six_digits_only() {
        assert_eq!(parse_hex6("#FFE0B2"), Some(Rgba::rgb(0xFF, 0xE0, 0xB2)));
        assert_eq!(parse_hex6(" ffe0b2 "), Some(Rgba::rgb(0xFF, 0xE0, 0xB2)));
        assert_eq!(parse_hex6("#FFE0B2FF"), None);
        assert_eq!(parse_hex6("#FFE0B"), None);
        assert_eq!(parse_hex6("red"), None);
    }
}
