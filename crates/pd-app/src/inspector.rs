//! The inspector panel (S §8.1, §8.2, SPEC-Rust §11.2).
//!
//! egui rebuilds it every frame from the selection, so there is no
//! feedback-loop guard to get wrong. Text fields keep a *draft* while the user
//! types; the draft is applied, as one undo step, when the field loses focus
//! or Enter is pressed. A draft remembers which item it was typed for: if the
//! selection changes first, it still goes to that item, not the new one.

use eframe::egui::{self, Color32, RichText, Sense, Vec2, text::CCursor, text::CCursorRange};
use pd_core::expr;
use pd_core::json::colour::{from_hex, to_hex};
use pd_core::model::{BorderDash, NodeShape, Rgba, StyleKind, TextAlign, Typeface};
use pd_core::render::constants::PICKER_SWATCHES;
use pd_core::render::draw;
use pd_core::view::{ColourField, DiagramView};

use crate::renderer::color;

/// Which item property a text field edits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Field {
    Concentration,
    BorderWidth,
    SpeciesWidth,
    SpeciesHeight,
    FontSize,
    LineWidth,
    ReactionFontSize,
    KineticLaw,
    TextSize,
    TextWrap,
    CompartmentId,
    Volume,
    CompartmentBorderWidth,
    CompartmentFontSize,
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
    species_width: Draft,
    species_height: Draft,
    font_size: Draft,
    line_width: Draft,
    reaction_font_size: Draft,
    law: Draft,
    text_size: Draft,
    text_wrap: Draft,
    compartment_id: Draft,
    volume: Draft,
    compartment_border: Draft,
    compartment_font_size: Draft,
    /// Why the last compartment rename was refused.
    rename_error: Option<String>,
    /// The words of the one selected text, as shown or typed.
    words: String,
    /// The text `words` belongs to.
    words_for: Option<String>,
    /// Put the keyboard in the words field for this text (S §7.8).
    edit_text: Option<String>,
    /// The colour picker's hex field.
    hex: String,
    /// Only the rows that are part of a look (S §8.5): the Edit Style
    /// window's fields (S §8.6).
    look_only: bool,
}

/// A size as the field shows it: whole numbers as they are, others to one
/// decimal.
fn size_text(v: f64) -> String {
    let rounded = (v * 10.0).round() / 10.0;
    rounded.to_string()
}

/// A group's heading, after a gap and a line unless it is the first shown.
fn heading(ui: &mut egui::Ui, shown: &mut bool, title: &str) {
    if *shown {
        ui.add_space(8.0);
        ui.separator();
    }
    *shown = true;
    ui.heading(title);
}

/// The saved style the selected items of `kind` follow, and a list of the
/// styles of that kind, the model's and My Styles', to give them one
/// (S §8.6).
fn style_row(ui: &mut egui::Ui, view: &mut DiagramView, kind: StyleKind) {
    let names: Vec<String> = view
        .style_entries(kind)
        .into_iter()
        .map(|e| e.name)
        .collect();
    let m = view.model();
    let links: Vec<Option<String>> = pd_core::styles::selected_ids(m, kind)
        .iter()
        .map(|id| pd_core::styles::link(m, kind, id))
        .collect();
    let current = view.selected_style_name(kind);
    let shown = match &current {
        Some(name) => name.clone(),
        None if links.iter().all(Option::is_none) => "None".to_owned(),
        None => "Mixed".to_owned(),
    };
    let mut chosen = None;
    ui.horizontal(|ui| {
        ui.label("Style");
        ui.add_enabled_ui(!names.is_empty(), |ui| {
            egui::ComboBox::from_id_salt(("saved-style", kind.key()))
                .selected_text(shown)
                .show_ui(ui, |ui| {
                    for name in &names {
                        if ui
                            .selectable_label(current.as_deref() == Some(name), name)
                            .clicked()
                        {
                            chosen = Some(name.clone());
                        }
                    }
                })
                .response
                .on_disabled_hover_text("Save a style first: right-click an item, Save Style…");
        });
    });
    if let Some(name) = chosen {
        view.apply_style(kind, &name);
    }
}

const ERROR: Color32 = Color32::from_rgb(0xCC, 0x33, 0x00);
const SWATCH: Vec2 = Vec2::new(28.0, 18.0);

/// The words field's egui id, to focus it.
const WORDS_ID: &str = "text-words";

impl Inspector {
    /// The panel for the Edit Style window: only the rows that are part of a
    /// look, without ids, values, rate laws, words or saved styles.
    pub fn for_styles() -> Self {
        Self {
            look_only: true,
            ..Self::default()
        }
    }

    /// Asks for the keyboard to go to the words field once `id` is the one
    /// selected text, with its words selected so typing replaces them.
    pub fn edit_text(&mut self, id: &str) {
        self.edit_text = Some(id.to_owned());
    }

    /// The groups for what is selected, in the order Species, Reaction,
    /// Compartment, Text; with nothing selected, a hint (S §13.3).
    pub fn show(&mut self, ui: &mut egui::Ui, view: &mut DiagramView) {
        let (species, reactions) = view.selection();
        let one_species = (species.len() == 1).then(|| species[0].clone());
        let first_species = species.first().cloned();
        let one_reaction = (reactions.len() == 1).then(|| reactions[0].clone());
        let first_reaction = reactions.first().cloned();
        let texts = view.selected_text_ids();
        let one_text = (texts.len() == 1).then(|| texts[0].clone());
        let first_text = texts.first().cloned();
        let compartments = view.selected_compartment_ids();
        let one_compartment = (compartments.len() == 1).then(|| compartments[0].clone());
        let first_compartment = compartments.first().cloned();
        let look_only = self.look_only;

        // Drafts typed for an item that is no longer the one shown go to that
        // item first (SPEC-Rust §11.2).
        for (field, target) in [
            (Field::Concentration, &one_species),
            (Field::BorderWidth, &one_species),
            (Field::SpeciesWidth, &first_species),
            (Field::SpeciesHeight, &first_species),
            (Field::FontSize, &first_species),
            (Field::LineWidth, &one_reaction),
            (Field::ReactionFontSize, &first_reaction),
            (Field::KineticLaw, &one_reaction),
            (Field::TextSize, &first_text),
            (Field::TextWrap, &one_text),
            (Field::CompartmentId, &one_compartment),
            (Field::Volume, &one_compartment),
            (Field::CompartmentBorderWidth, &one_compartment),
            (Field::CompartmentFontSize, &first_compartment),
        ] {
            if self.draft(field).target != *target {
                self.commit(field, view);
                let d = self.draft_mut(field);
                d.target = target.clone();
                d.dirty = false;
            }
        }

        if one_compartment.is_none() {
            self.rename_error = None;
        }

        let mut shown = false;
        if !species.is_empty() {
            heading(ui, &mut shown, "Species");
            if !look_only {
                style_row(ui, view, StyleKind::Species);
            }
            ui.scope(|ui| {
                egui::Grid::new("species")
                    .num_columns(2)
                    .spacing([8.0, 6.0])
                    .show(ui, |ui| {
                        if !look_only {
                            ui.label("Id");
                            ui.label(match &one_species {
                                Some(id) => view.rename_initial(id).unwrap_or_default(),
                                None if species.is_empty() => String::new(),
                                None => format!("{} selected", species.len()),
                            });
                            ui.end_row();
                        }

                        ui.label("Fill");
                        self.swatch(ui, view, ColourField::Fill);
                        ui.end_row();
                        ui.label("Border");
                        self.swatch(ui, view, ColourField::Border);
                        ui.end_row();
                        ui.label("Label");
                        self.swatch(ui, view, ColourField::Label);
                        ui.end_row();

                        ui.label("Border width");
                        self.number(ui, view, Field::BorderWidth, one_species.is_some(), "")
                            .on_hover_text("0 for no border");
                        ui.end_row();

                        // Solid or dashed, for every selected species (S §5.3).
                        ui.label("Border style");
                        let current = view.selected_species_border_dash();
                        let mut chosen = None;
                        egui::ComboBox::from_id_salt("species-border-dash")
                            .selected_text(current.map_or("Mixed", BorderDash::label))
                            .show_ui(ui, |ui| {
                                for dash in BorderDash::ALL {
                                    let tip = match dash {
                                        BorderDash::Automatic => {
                                            "Dashed for an alias, solid otherwise"
                                        }
                                        BorderDash::Solid => "Always solid, even for an alias",
                                        BorderDash::Dashed => "Always dashed",
                                    };
                                    if ui
                                        .selectable_label(current == Some(dash), dash.label())
                                        .on_hover_text(tip)
                                        .clicked()
                                    {
                                        chosen = Some(dash);
                                    }
                                }
                            });
                        if let Some(dash) = chosen {
                            view.set_species_border_dash(dash);
                        }
                        ui.end_row();

                        // The label's size, for every selected species (S §8.1).
                        ui.label("Font size");
                        self.number(ui, view, Field::FontSize, true, "");
                        ui.end_row();

                        // Inside the node or beside it, for every selected
                        // species (S §5.11).
                        ui.label("Name");
                        let current = view.selected_species_label_outside();
                        let mut chosen = None;
                        egui::ComboBox::from_id_salt("species-label-place")
                            .selected_text(match current {
                                Some(false) => "Inside",
                                Some(true) => "Outside",
                                None => "Mixed",
                            })
                            .show_ui(ui, |ui| {
                                for (outside, label, tip) in [
                                    (false, "Inside", "The name is drawn inside the node"),
                                    (
                                        true,
                                        "Outside",
                                        "The name is drawn beside the node; drag it to move it",
                                    ),
                                ] {
                                    if ui
                                        .selectable_label(current == Some(outside), label)
                                        .on_hover_text(tip)
                                        .clicked()
                                    {
                                        chosen = Some(outside);
                                    }
                                }
                            });
                        if let Some(outside) = chosen {
                            view.set_species_label_outside(outside);
                        }
                        ui.end_row();

                        // For every selected species (S §7.10). Part of a look only
                        // with the name outside (S §5.11).
                        if !look_only || view.selected_species_label_outside() == Some(true) {
                            ui.label("Width");
                            self.number(ui, view, Field::SpeciesWidth, true, "")
                                .on_hover_text("Or drag a corner of the selection");
                            ui.end_row();
                            ui.label("Height");
                            self.number(ui, view, Field::SpeciesHeight, true, "")
                                .on_hover_text(
                                    "Shift while dragging a corner keeps the proportions",
                                );
                            ui.end_row();
                        }
                        if !look_only {
                            ui.label("");
                            // A name outside doesn't set the node's width.
                            let fits = view.selected_species_label_outside() != Some(true);
                            if ui
                                .add_enabled(fits, egui::Button::new("Fit to Name"))
                                .on_hover_text(
                                    "Make the selected species just wide enough for their names",
                                )
                                .on_disabled_hover_text("Their names are drawn outside the node")
                                .clicked()
                            {
                                view.fit_species_to_names();
                            }
                            ui.end_row();
                        }

                        // For every selected species (S §5.10).
                        ui.label("Shape");
                        let current = view.selected_species_shape();
                        let mut chosen = None;
                        egui::ComboBox::from_id_salt("species-shape")
                            .selected_text(current.map_or("Mixed", NodeShape::label))
                            .show_ui(ui, |ui| {
                                for shape in NodeShape::ALL {
                                    if ui
                                        .selectable_label(current == Some(shape), shape.label())
                                        .clicked()
                                    {
                                        chosen = Some(shape);
                                    }
                                }
                            });
                        if let Some(shape) = chosen {
                            view.set_species_shape(shape);
                        }
                        ui.end_row();
                        if !look_only {
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
                        }
                    });
            });
        }

        if !reactions.is_empty() {
            heading(ui, &mut shown, "Reaction");
            if !look_only {
                style_row(ui, view, StyleKind::Reaction);
            }
            ui.scope(|ui| {
                egui::Grid::new("reaction")
                    .num_columns(2)
                    .spacing([8.0, 6.0])
                    .show(ui, |ui| {
                        if !look_only {
                            ui.label("Id");
                            ui.label(match &one_reaction {
                                Some(id) => id.clone(),
                                None if reactions.is_empty() => String::new(),
                                None => format!("{} selected", reactions.len()),
                            });
                            ui.end_row();
                        }

                        ui.label("Line");
                        self.swatch(ui, view, ColourField::Line);
                        ui.end_row();
                        ui.label("Line width");
                        self.number(ui, view, Field::LineWidth, one_reaction.is_some(), "");
                        ui.end_row();

                        ui.label("Show junction");
                        let mut visible = !reactions.is_empty() && view.all_junctions_visible();
                        if ui.checkbox(&mut visible, "").changed() {
                            view.set_junction_visible(visible);
                        }
                        ui.end_row();

                        // For every selected reaction (S §8.1); drawn with
                        // arrowheads at both ends (S §5.5). Not part of a look.
                        if !look_only {
                            ui.label("Reversible");
                            let mut rev = view.selected_reactions_reversible() == Some(true);
                            if ui
                                .checkbox(&mut rev, "")
                                .on_hover_text(
                                    "A reversible reaction has an arrowhead at both ends",
                                )
                                .changed()
                            {
                                view.set_reversible(rev);
                            }
                            ui.end_row();
                        }

                        // Its id beside the junction, for every selected
                        // reaction (S §5.12). Where it is isn't part of a look.
                        if !look_only {
                            ui.label("Show name");
                            let mut named = view.selected_reactions_label_shown() == Some(true);
                            if ui
                                .checkbox(&mut named, "")
                                .on_hover_text(
                                    "Draw the reaction's id beside it; drag it to move it",
                                )
                                .changed()
                            {
                                view.set_reaction_label_shown(named);
                            }
                            ui.end_row();
                        }
                        ui.label("Name size");
                        self.number(ui, view, Field::ReactionFontSize, true, "");
                        ui.end_row();
                        ui.label("Name colour");
                        self.swatch(ui, view, ColourField::ReactionLabel);
                        ui.end_row();
                    });
                if !look_only {
                    ui.label("Rate law");
                    self.law_field(ui, view, one_reaction.is_some());
                }
            });
        }

        if !compartments.is_empty() {
            heading(ui, &mut shown, "Compartment");
            if !look_only {
                style_row(ui, view, StyleKind::Compartment);
            }
            ui.scope(|ui| {
                egui::Grid::new("compartment")
                    .num_columns(2)
                    .spacing([8.0, 6.0])
                    .show(ui, |ui| {
                        if !look_only {
                            ui.label("Id");
                            if one_compartment.is_some() {
                                self.number(ui, view, Field::CompartmentId, true, "");
                            } else if compartments.is_empty() {
                                ui.label("");
                            } else {
                                ui.label(format!("{} selected", compartments.len()));
                            }
                            ui.end_row();
                            ui.label("Volume");
                            self.number(ui, view, Field::Volume, one_compartment.is_some(), "");
                            ui.end_row();
                        }
                        ui.label("Fill");
                        self.swatch(ui, view, ColourField::CompartmentFill);
                        ui.end_row();
                        ui.label("Border");
                        self.swatch(ui, view, ColourField::CompartmentBorder);
                        ui.end_row();
                        ui.label("Border width");
                        self.number(
                            ui,
                            view,
                            Field::CompartmentBorderWidth,
                            one_compartment.is_some(),
                            "",
                        )
                        .on_hover_text("0 for no border");
                        ui.end_row();
                        // For every selected compartment (S §7.9).
                        ui.label("Name size");
                        self.number(ui, view, Field::CompartmentFontSize, true, "");
                        ui.end_row();

                        if !look_only {
                            // Ticked only if every selected box is shown. The default
                            // compartment is never drawn, so it has no box to show.
                            let m = view.model();
                            let boxes: Vec<_> = compartments
                                .iter()
                                .filter_map(|id| m.find_compartment(id))
                                .filter(|c| !c.is_default())
                                .collect();
                            let mut shown = !boxes.is_empty() && boxes.iter().all(|c| !c.hidden);
                            ui.label("Show box");
                            if ui
                                .add_enabled(
                                    !boxes.is_empty(),
                                    egui::Checkbox::without_text(&mut shown),
                                )
                                .on_hover_text(
                                    "Untick to hide the box; the compartment and its species stay",
                                )
                                .changed()
                            {
                                view.set_compartment_hidden(!shown);
                            }
                            ui.end_row();
                        }
                    });
                if let Some(e) = &self.rename_error {
                    ui.label(RichText::new(e).color(ERROR).small());
                }
            });
        }

        if !texts.is_empty() {
            heading(ui, &mut shown, "Text");
            if !look_only {
                style_row(ui, view, StyleKind::Text);
            }
            ui.scope(|ui| {
                if !look_only {
                    self.words_field(ui, view, one_text.as_deref());
                }
                let m = view.model();
                let selected: Vec<_> = texts.iter().filter_map(|id| m.find_text(id)).collect();
                let face = selected
                    .first()
                    .map(|t| t.font.typeface)
                    .unwrap_or_default();
                let all_bold = !selected.is_empty() && selected.iter().all(|t| t.font.bold);
                let all_italic = !selected.is_empty() && selected.iter().all(|t| t.font.italic);
                // Shown as chosen only if every selected text has it.
                let align = selected
                    .first()
                    .map(|t| t.align)
                    .filter(|a| selected.iter().all(|t| t.align == *a));
                egui::Grid::new("text")
                    .num_columns(2)
                    .spacing([8.0, 6.0])
                    .show(ui, |ui| {
                        ui.label("Typeface");
                        let mut chosen = None;
                        egui::ComboBox::from_id_salt("typeface")
                            .selected_text(typeface_name(face))
                            .show_ui(ui, |ui| {
                                for f in [Typeface::Sans, Typeface::Serif] {
                                    if ui.selectable_label(f == face, typeface_name(f)).clicked() {
                                        chosen = Some(f);
                                    }
                                }
                            });
                        if let Some(f) = chosen {
                            view.set_text_typeface(f);
                        }
                        ui.end_row();

                        ui.label("Size");
                        self.number(ui, view, Field::TextSize, !texts.is_empty(), "");
                        ui.end_row();

                        ui.label("Style");
                        ui.horizontal(|ui| {
                            let mut bold = all_bold;
                            if ui
                                .checkbox(&mut bold, RichText::new("Bold").strong())
                                .changed()
                            {
                                view.set_text_bold(bold);
                            }
                            let mut italic = all_italic;
                            if ui
                                .checkbox(&mut italic, RichText::new("Italic").italics())
                                .changed()
                            {
                                view.set_text_italic(italic);
                            }
                        });
                        ui.end_row();

                        ui.label("Align");
                        ui.horizontal(|ui| {
                            for (a, name, tip) in [
                                (TextAlign::Left, "Left", "Lines start at the left edge"),
                                (TextAlign::Center, "Center", "Lines are centred"),
                                (TextAlign::Right, "Right", "Lines end at the right edge"),
                            ] {
                                if ui
                                    .selectable_label(align == Some(a), name)
                                    .on_hover_text(tip)
                                    .clicked()
                                {
                                    view.set_text_align(a);
                                }
                            }
                        });
                        ui.end_row();

                        ui.label("Colour");
                        self.swatch(ui, view, ColourField::Text);
                        ui.end_row();

                        if !look_only {
                            ui.label("Wrap width");
                            self.number(
                                ui,
                                view,
                                Field::TextWrap,
                                one_text.is_some(),
                                "0 = no wrap",
                            )
                            .on_hover_text("Or drag the small square on the text's right edge");
                            ui.end_row();
                        }
                    });
            });
        }

        if !shown {
            ui.label(
                RichText::new(
                    "Select a species, reaction, compartment or text to see and change it here.",
                )
                .weak(),
            );
            // The diagram itself (S §8.8).
            if !look_only {
                ui.add_space(8.0);
                heading(ui, &mut shown, "Diagram");
                egui::Grid::new("diagram")
                    .num_columns(2)
                    .spacing([8.0, 6.0])
                    .show(ui, |ui| {
                        ui.label("Background");
                        ui.horizontal(|ui| {
                            self.swatch(ui, view, ColourField::Background);
                            let custom = !view.model().background.is_default();
                            if ui
                                .add_enabled(custom, egui::Button::new("Default"))
                                .on_hover_text("The usual light grey")
                                .clicked()
                            {
                                view.set_background(Rgba::DEFAULT);
                            }
                        });
                        ui.end_row();
                    });
            }
        }
    }

    /// The words of the one selected text, applied as they are typed (one
    /// undo step for a run of typing). Enter starts a new line.
    fn words_field(&mut self, ui: &mut egui::Ui, view: &mut DiagramView, one: Option<&str>) {
        let id = egui::Id::new(WORDS_ID);
        let focused = ui.memory(|m| m.has_focus(id));
        // Another text selected: show its words, and stop typing into the old one.
        if self.words_for.as_deref() != one {
            self.words_for = one.map(str::to_owned);
            if focused {
                ui.memory_mut(|m| m.surrender_focus(id));
            }
        }
        if !focused || self.words_for.as_deref() != one {
            self.words = one
                .and_then(|t| view.model().find_text(t))
                .map(|t| t.text.clone())
                .unwrap_or_default();
        }
        let r = ui.add_enabled(
            one.is_some(),
            egui::TextEdit::multiline(&mut self.words)
                .id(id)
                .desired_rows(3)
                .desired_width(f32::INFINITY)
                .hint_text(if one.is_some() {
                    "Type the text here"
                } else {
                    "select one text"
                }),
        );
        if r.changed()
            && let Some(t) = one
        {
            view.set_text(t, &self.words);
        }
        if self.edit_text.is_some() && self.edit_text.as_deref() == one {
            self.edit_text = None;
            let mut state = egui::TextEdit::load_state(ui.ctx(), id).unwrap_or_default();
            let end = CCursor::new(self.words.chars().count());
            state
                .cursor
                .set_char_range(Some(CCursorRange::two(CCursor::new(0), end)));
            state.store(ui.ctx(), id);
            r.request_focus();
            r.scroll_to_me(Some(egui::Align::Center));
        }
    }

    fn draft(&self, field: Field) -> &Draft {
        match field {
            Field::Concentration => &self.concentration,
            Field::BorderWidth => &self.border_width,
            Field::SpeciesWidth => &self.species_width,
            Field::SpeciesHeight => &self.species_height,
            Field::FontSize => &self.font_size,
            Field::LineWidth => &self.line_width,
            Field::ReactionFontSize => &self.reaction_font_size,
            Field::KineticLaw => &self.law,
            Field::TextSize => &self.text_size,
            Field::TextWrap => &self.text_wrap,
            Field::CompartmentId => &self.compartment_id,
            Field::Volume => &self.volume,
            Field::CompartmentBorderWidth => &self.compartment_border,
            Field::CompartmentFontSize => &self.compartment_font_size,
        }
    }

    fn draft_mut(&mut self, field: Field) -> &mut Draft {
        match field {
            Field::Concentration => &mut self.concentration,
            Field::BorderWidth => &mut self.border_width,
            Field::SpeciesWidth => &mut self.species_width,
            Field::SpeciesHeight => &mut self.species_height,
            Field::FontSize => &mut self.font_size,
            Field::LineWidth => &mut self.line_width,
            Field::ReactionFontSize => &mut self.reaction_font_size,
            Field::KineticLaw => &mut self.law,
            Field::TextSize => &mut self.text_size,
            Field::TextWrap => &mut self.text_wrap,
            Field::CompartmentId => &mut self.compartment_id,
            Field::Volume => &mut self.volume,
            Field::CompartmentBorderWidth => &mut self.compartment_border,
            Field::CompartmentFontSize => &mut self.compartment_font_size,
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
            // What is drawn: the default unless set, 0 for no border.
            Field::BorderWidth => m
                .find_species(target)
                .map(|s| size_text(draw::species_border_width(s))),
            Field::SpeciesWidth => m.find_species(target).map(|s| size_text(s.width)),
            Field::SpeciesHeight => m.find_species(target).map(|s| size_text(s.height)),
            Field::FontSize => m
                .find_species(target)
                .map(|s| size_text(draw::label_size(s))),
            Field::LineWidth => m
                .find_reaction(target)
                .map(|r| size_text(draw::reaction_line_width(r))),
            Field::ReactionFontSize => m
                .find_reaction(target)
                .map(|r| size_text(draw::reaction_label_size(r))),
            Field::KineticLaw => m.find_reaction(target).map(|r| r.kinetic_law.clone()),
            Field::TextSize => m.find_text(target).map(|t| t.size.to_string()),
            Field::TextWrap => m.find_text(target).map(|t| t.wrap_width.to_string()),
            Field::CompartmentId => m.find_compartment(target).map(|c| c.id.clone()),
            Field::Volume => m.find_compartment(target).map(|c| c.size.to_string()),
            Field::CompartmentBorderWidth => m
                .find_compartment(target)
                .map(|c| size_text(draw::compartment_colours(c).2)),
            Field::CompartmentFontSize => m
                .find_compartment(target)
                .map(|c| size_text(draw::compartment_label_size(c))),
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
            // A size goes to every selected species.
            (Field::SpeciesWidth, Some(v)) => {
                view.set_species_size(Some(v), None);
            }
            (Field::SpeciesHeight, Some(v)) => {
                view.set_species_size(None, Some(v));
            }
            // 0 would be read as "the default"; a label or line needs a size.
            (Field::FontSize, Some(v)) if v > 0.0 => {
                view.set_font_size(v);
            }
            (Field::LineWidth, Some(v)) if v > 0.0 => {
                view.set_line_width(&target, v);
            }
            // The size goes to every selected reaction.
            (Field::ReactionFontSize, Some(v)) if v > 0.0 => {
                view.set_reaction_font_size(v);
            }
            (Field::KineticLaw, _) => {
                view.set_kinetic_law(&target, &text);
            }
            // The size goes to every selected text.
            (Field::TextSize, Some(v)) => {
                view.set_text_size(v);
            }
            (Field::TextWrap, Some(v)) => {
                view.set_text_wrap(&target, v);
            }
            (Field::CompartmentId, _) => {
                self.rename_error = view
                    .rename_compartment(&target, &text)
                    .err()
                    .map(|e| format!("Not renamed: {e}"));
            }
            (Field::Volume, Some(v)) => {
                view.set_compartment_volume(&target, v);
            }
            // The size goes to every selected compartment.
            (Field::CompartmentFontSize, Some(v)) if v > 0.0 => {
                view.set_compartment_font_size(v);
            }
            (Field::CompartmentBorderWidth, Some(v)) => {
                view.set_compartment_border_width(&target, v);
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
    ) -> egui::Response {
        self.refresh(field, view);
        let d = self.draft_mut(field);
        // The compartment's id is the one field here that isn't a number.
        let valid = field == Field::CompartmentId
            || !d.dirty
            || d.text.trim().parse::<f64>().is_ok_and(f64::is_finite);
        // An id can be long; a number is short.
        let width = if field == Field::CompartmentId {
            f32::INFINITY
        } else {
            90.0
        };
        let edit = egui::TextEdit::singleline(&mut d.text)
            .desired_width(width)
            .hint_text(hint)
            .text_color_opt((!valid).then_some(ERROR));
        let r = ui.add_enabled(enabled, edit);
        if r.changed() {
            d.dirty = true;
        }
        if r.lost_focus() {
            self.commit(field, view);
        }
        r
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
        paint_colour(ui.painter(), rect, current);
        ui.painter().rect_stroke(
            rect,
            3.0,
            ui.visuals().widgets.inactive.fg_stroke,
            egui::StrokeKind::Inside,
        );
        let tip = if current.is_some_and(Rgba::is_transparent) {
            "No fill. Choose a colour"
        } else {
            "Choose a colour"
        };
        let resp = resp.on_hover_text(tip);
        let none = can_leave_out(field);
        if resp.clicked() {
            self.hex = current
                .map(|c| to_hex(c)[..7].to_owned())
                .unwrap_or_default();
        }
        egui::Popup::from_toggle_button_response(&resp)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .show(|ui| {
                if let Some(c) = self.picker(ui, current, none) {
                    view.apply_colour(field, c);
                    ui.close();
                }
            });
    }

    /// The colour picker: 40 swatches, a preview with its `#RRGGBB` value, and
    /// a hex field (red while invalid). Returns the colour chosen, if any.
    fn picker(&mut self, ui: &mut egui::Ui, current: Option<Rgba>, none: bool) -> Option<Rgba> {
        let mut chosen = None;
        if none {
            let pressed = current.is_some_and(Rgba::is_transparent);
            ui.horizontal(|ui| {
                let (rect, r) = ui.allocate_exact_size(Vec2::splat(20.0), Sense::click());
                paint_colour(ui.painter(), rect, Some(Rgba::TRANSPARENT));
                let stroke = if pressed {
                    egui::Stroke::new(2.0, Color32::BLACK)
                } else {
                    egui::Stroke::new(1.0, Color32::from_gray(160))
                };
                ui.painter()
                    .rect_stroke(rect, 2.0, stroke, egui::StrokeKind::Inside);
                let b = ui.selectable_label(pressed, "No fill (transparent)");
                if r.clicked() || b.clicked() {
                    chosen = Some(Rgba::TRANSPARENT);
                }
            });
            ui.separator();
        }
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

/// Whether the picker offers "No fill" for `field`: only a fill can be left
/// out; a border, a line or text must be seen.
fn can_leave_out(field: ColourField) -> bool {
    matches!(field, ColourField::Fill | ColourField::CompartmentFill)
}

/// A colour in a swatch: no fill is white with a red slash across it.
fn paint_colour(painter: &egui::Painter, rect: egui::Rect, c: Option<Rgba>) {
    match c {
        Some(c) if c.is_transparent() => {
            painter.rect_filled(rect, 2.0, Color32::WHITE);
            let slash = egui::Stroke::new(1.5, Color32::from_rgb(0xD0, 0x30, 0x30));
            painter.line_segment([rect.left_bottom(), rect.right_top()], slash);
        }
        _ => {
            painter.rect_filled(rect, 2.0, c.map_or(Color32::TRANSPARENT, color));
        }
    }
}

fn typeface_name(f: Typeface) -> &'static str {
    match f {
        Typeface::Sans => "Sans (Noto Sans)",
        Typeface::Serif => "Serif (Noto Serif)",
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
    fn only_fills_can_be_left_out() {
        assert!(can_leave_out(ColourField::Fill));
        assert!(can_leave_out(ColourField::CompartmentFill));
        for f in [
            ColourField::Border,
            ColourField::Label,
            ColourField::Line,
            ColourField::ReactionLabel,
            ColourField::Text,
            ColourField::CompartmentBorder,
        ] {
            assert!(!can_leave_out(f), "{f:?}");
        }
    }

    #[test]
    fn hex_field_accepts_six_digits_only() {
        assert_eq!(parse_hex6("#FFE0B2"), Some(Rgba::rgb(0xFF, 0xE0, 0xB2)));
        assert_eq!(parse_hex6(" ffe0b2 "), Some(Rgba::rgb(0xFF, 0xE0, 0xB2)));
        assert_eq!(parse_hex6("#FFE0B2FF"), None);
        assert_eq!(parse_hex6("#FFE0B"), None);
        assert_eq!(parse_hex6("red"), None);
    }
}
