//! The Antimony panel (S §11.4, SPEC-Rust §11.1).
//!
//! A text editor with **Load Antimony** (import the text, one undo step) and
//! **Get Antimony** (replace the text with an export of the diagram). Lines
//! with errors are highlighted, and the errors are listed below with their
//! line and column; clicking one puts the cursor there. The text last
//! imported is remembered, so it can be imported again when the catalytic
//! preference changes (S §12).

use std::sync::Arc;

use eframe::egui::{
    self, Color32, FontId, Galley, RichText, TextBuffer, TextFormat, text::CCursor,
    text::CCursorRange, text::LayoutJob,
};
use pd_core::antimony::{Diagnostic, ImportOptions};
use pd_core::view::DiagramView;

const ERROR: Color32 = Color32::from_rgb(0xCC, 0x33, 0x00);
const WARNING: Color32 = Color32::from_rgb(0xA0, 0x50, 0x00);
const ERROR_LINE_BACKGROUND: Color32 = Color32::from_rgb(0xFF, 0xE8, 0xE8);
const TEXT_SIZE: f32 = 13.0;

/// What the app should do after the panel was used.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PanelAction {
    /// A model was imported: fit it in the window.
    Imported,
    /// The user asked to lay out the imported model.
    LayOut,
}

#[derive(Default)]
pub struct AntimonyPanel {
    text: String,
    errors: Vec<Diagnostic>,
    warnings: Vec<Diagnostic>,
    /// A line after a successful import.
    imported: Option<String>,
    /// Whether to offer to lay out the import: not when the text placed
    /// every species itself (Antimony's layout statements).
    offer_layout: bool,
    /// The text last imported, for re-importing.
    last_import: Option<String>,
    /// Put the cursor at this (line, column) on the next frame.
    jump: Option<(usize, usize)>,
    /// Get Antimony writes the drawing too, as a diagram comment.
    with_layout: bool,
    /// Get Antimony writes the drawing as Antimony's layout statements too.
    with_statements: bool,
}

impl AntimonyPanel {
    pub fn last_import(&self) -> Option<&str> {
        self.last_import.as_deref()
    }

    /// The text in the panel.
    #[cfg(test)]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Imports `text` into the view. On errors the diagram is unchanged and
    /// they are listed.
    fn import(&mut self, text: &str, view: &mut DiagramView, opts: &ImportOptions) -> bool {
        match view.import_antimony(text, opts) {
            Ok((_, warnings, needs_layout)) => {
                self.errors.clear();
                self.warnings = warnings;
                let m = view.model();
                let species = m.species().iter().filter(|s| !s.is_null_node).count();
                let drawn = if needs_layout {
                    ""
                } else {
                    ", drawn as the text lays them out"
                };
                self.imported = Some(format!(
                    "Imported {species} species and {} reactions{drawn}.",
                    m.reactions().len()
                ));
                self.offer_layout = needs_layout;
                self.last_import = Some(text.to_owned());
                true
            }
            Err(errors) => {
                self.errors = errors;
                self.warnings.clear();
                self.imported = None;
                false
            }
        }
    }

    /// Puts `text` in the panel and imports it, as when a file is opened.
    pub fn load(&mut self, text: &str, view: &mut DiagramView, opts: &ImportOptions) -> bool {
        self.text = text.to_owned();
        self.import(text, view, opts)
    }

    /// Imports the last imported text again, with new options (when the
    /// catalytic preference changes). Returns true if it did.
    pub fn reimport(&mut self, view: &mut DiagramView, opts: &ImportOptions) -> bool {
        match self.last_import.clone() {
            Some(text) => self.import(&text, view, opts),
            None => false,
        }
    }

    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        view: &mut DiagramView,
        opts: &ImportOptions,
    ) -> Option<PanelAction> {
        let mut action = None;
        ui.horizontal(|ui| {
            if ui
                .button("Load Antimony")
                .on_hover_text("Replace the diagram with the model in the text (can be undone)")
                .clicked()
            {
                let text = self.text.clone();
                if self.import(&text, view, opts) {
                    action = Some(PanelAction::Imported);
                }
            }
            if ui
                .button("Get Antimony")
                .on_hover_text("Replace the text with the diagram, written as Antimony")
                .clicked()
            {
                self.text = match (self.with_layout, self.with_statements) {
                    (true, statements) => view.export_antimony_with_diagram(statements).0,
                    (false, true) => view.export_antimony_with_layout().0,
                    (false, false) => view.export_antimony(),
                };
                self.errors.clear();
                self.warnings.clear();
                self.imported = None;
            }
            ui.checkbox(&mut self.with_layout, "Diagram").on_hover_text(
                "Get Antimony writes the drawing too, as a diagram comment after the model",
            );
            ui.checkbox(&mut self.with_statements, "Layout statements")
                .on_hover_text(
                    "Get Antimony writes the drawing as Antimony's own layout statements too, \
                     for tellurium and SBMLNetwork",
                );
        });

        let id = ui.make_persistent_id("antimony-text");
        let error_lines: Vec<usize> = self.errors.iter().map(|e| e.line).collect();
        let mut layouter = |ui: &egui::Ui, text: &dyn TextBuffer, wrap_width: f32| -> Arc<Galley> {
            let mut job = LayoutJob::default();
            for (i, line) in text.as_str().split_inclusive('\n').enumerate() {
                let background = if error_lines.contains(&(i + 1)) {
                    ERROR_LINE_BACKGROUND
                } else {
                    Color32::TRANSPARENT
                };
                job.append(
                    line,
                    0.0,
                    TextFormat {
                        font_id: FontId::monospace(TEXT_SIZE),
                        color: ui.visuals().text_color(),
                        background,
                        ..TextFormat::default()
                    },
                );
            }
            job.wrap.max_width = wrap_width;
            ui.fonts_mut(|f| f.layout_job(job))
        };
        let edit = egui::TextEdit::multiline(&mut self.text)
            .id(id)
            .code_editor()
            // It has its own tab, so it can be tall; errors go below it.
            .desired_rows(24)
            .desired_width(f32::INFINITY)
            .hint_text("S1 -> S2; k1*S1\nk1 = 0.1")
            .layouter(&mut layouter);
        let response = ui.add(edit);
        if response.changed() {
            self.imported = None;
        }

        if let Some((line, column)) = self.jump.take() {
            let index = char_index(&self.text, line, column);
            if let Some(mut state) = egui::TextEdit::load_state(ui.ctx(), id) {
                state
                    .cursor
                    .set_char_range(Some(CCursorRange::one(CCursor::new(index))));
                state.store(ui.ctx(), id);
            }
            ui.ctx().memory_mut(|m| m.request_focus(id));
        }

        for (list, colour) in [(&self.errors, ERROR), (&self.warnings, WARNING)] {
            for d in list {
                let text = RichText::new(d.to_string()).color(colour).small();
                if ui
                    .add(egui::Label::new(text).sense(egui::Sense::click()))
                    .on_hover_text("Go to this line")
                    .clicked()
                {
                    self.jump = Some((d.line, d.column));
                }
            }
        }
        if let Some(msg) = &self.imported {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(msg).small());
                if self.offer_layout
                    && ui
                        .small_button("Auto-layout")
                        .on_hover_text("Imported species are placed at random; lay them out")
                        .clicked()
                {
                    action = Some(PanelAction::LayOut);
                }
            });
        }
        action
    }
}

/// The character index of a 1-based line and column in `text`.
fn char_index(text: &str, line: usize, column: usize) -> usize {
    let mut index = 0;
    for (i, l) in text.split_inclusive('\n').enumerate() {
        if i + 1 == line {
            return index
                + (column.saturating_sub(1)).min(l.trim_end_matches('\n').chars().count());
        }
        index += l.chars().count();
    }
    index
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_and_column_to_a_character_index() {
        let t = "ab\ncdé f\nxy";
        assert_eq!(char_index(t, 1, 1), 0);
        assert_eq!(char_index(t, 2, 1), 3);
        assert_eq!(char_index(t, 2, 5), 7);
        // Past the end of a line: its end.
        assert_eq!(char_index(t, 1, 99), 2);
        // Past the last line: the end of the text.
        assert_eq!(char_index(t, 9, 1), t.chars().count());
    }

    #[test]
    fn failed_import_lists_errors_and_keeps_the_diagram() {
        let mut p = AntimonyPanel {
            text: "A -> B; k*(".into(),
            ..AntimonyPanel::default()
        };
        let mut view = DiagramView::default();
        let opts = ImportOptions {
            convert_catalytic: true,
            seed: 1,
        };
        let text = p.text.clone();
        assert!(!p.import(&text, &mut view, &opts));
        assert_eq!(p.errors.len(), 1);
        assert!(p.last_import().is_none());
        assert!(view.model().reactions().is_empty());
        // A good one is remembered, and can be imported again.
        assert!(p.import("A -> A + B; k*A", &mut view, &opts));
        assert_eq!(p.last_import(), Some("A -> A + B; k*A"));
        let off = ImportOptions {
            convert_catalytic: false,
            ..opts
        };
        assert!(p.reimport(&mut view, &off));
        assert_eq!(view.model().reactions()[0].reactants[0].species, "A");
    }
}
