//! The app shell: menu bar, canvas and status bar (S §13).
//!
//! It opens the built-in sample models and lets the user select, drag, delete
//! and undo. Panels, tools and file access arrive in later milestones.

use eframe::egui::{self, Key, KeyboardShortcut, Modifiers};
use pd_core::geometry::pt;
use pd_core::json::from_json;
use pd_core::model::Model;
use pd_core::view::{DiagramView, ViewEvent};

use crate::canvas::{self, CanvasState};
use crate::fonts;
use crate::samples::SAMPLES;

/// The sample shown at start-up.
const START_SAMPLE: &str = "model9";

const UNDO: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::Z);
const REDO: KeyboardShortcut =
    KeyboardShortcut::new(Modifiers::COMMAND.plus(Modifiers::SHIFT), Key::Z);
/// Ctrl+Y, also accepted for Redo (S §9.1).
const REDO_Y: KeyboardShortcut = KeyboardShortcut::new(Modifiers::CTRL, Key::Y);
const SELECT_ALL: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::A);

pub struct PathwayApp {
    view: DiagramView,
    canvas: CanvasState,
    /// Name of what is shown, for the status bar.
    title: String,
    /// Warnings from the last load, shown in the status bar.
    warnings: Vec<String>,
    /// Zoom to fit on the next frame, once the canvas size is known.
    fit_pending: bool,
}

impl PathwayApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        fonts::install(&cc.egui_ctx);
        cc.egui_ctx.set_visuals(egui::Visuals::light());
        let mut app = Self {
            view: DiagramView::default(),
            canvas: CanvasState::default(),
            title: "Untitled".to_owned(),
            warnings: Vec::new(),
            fit_pending: false,
        };
        if let Some((name, text)) = SAMPLES.iter().find(|(n, _)| *n == START_SAMPLE) {
            app.open(name, text);
        }
        app
    }

    fn open(&mut self, name: &str, text: &str) {
        match from_json(text) {
            Ok(loaded) => {
                self.view.set_model(loaded.model);
                self.title = name.to_owned();
                self.warnings = loaded.warnings;
                self.fit_pending = true;
            }
            Err(e) => self.warnings = vec![format!("{name} could not be opened: {e}")],
        }
    }

    /// Diagram shortcuts apply only while no text field is being edited, so a
    /// text field gets its own editing keys first (SPEC-Rust §6.1).
    fn shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.egui_wants_keyboard_input() {
            return;
        }
        // Redo before Undo: Cmd+Shift+Z would also match Cmd+Z.
        if ctx.input_mut(|i| i.consume_shortcut(&REDO) || i.consume_shortcut(&REDO_Y)) {
            self.view.redo();
        }
        if ctx.input_mut(|i| i.consume_shortcut(&UNDO)) {
            self.view.undo();
        }
        if ctx.input_mut(|i| i.consume_shortcut(&SELECT_ALL)) {
            self.view.select_all();
        }
        if ctx.input_mut(|i| {
            i.consume_key(Modifiers::NONE, Key::Delete)
                || i.consume_key(Modifiers::NONE, Key::Backspace)
        }) {
            self.view.delete_selection();
        }
        if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
            self.view.handle(&ViewEvent::Cancel);
        }
    }

    fn menu_bar(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("File", |ui| {
                if ui.button("New").clicked() {
                    self.view.set_model(Model::new());
                    self.title = "Untitled".to_owned();
                    self.warnings.clear();
                    ui.close();
                }
                ui.menu_button("Open sample", |ui| {
                    for (name, text) in SAMPLES {
                        if ui.button(*name).clicked() {
                            self.open(name, text);
                            ui.close();
                        }
                    }
                });
            });
            ui.menu_button("Edit", |ui| {
                let label = |verb: &str, what: Option<&str>| match what {
                    Some(w) => format!("{verb} {w}"),
                    None => verb.to_owned(),
                };
                let undo = egui::Button::new(label("Undo", self.view.undo_description()))
                    .shortcut_text(ctx.format_shortcut(&UNDO));
                if ui.add_enabled(self.view.can_undo(), undo).clicked() {
                    self.view.undo();
                    ui.close();
                }
                let redo = egui::Button::new(label("Redo", self.view.redo_description()))
                    .shortcut_text(ctx.format_shortcut(&REDO));
                if ui.add_enabled(self.view.can_redo(), redo).clicked() {
                    self.view.redo();
                    ui.close();
                }
                ui.separator();
                let delete = egui::Button::new("Delete").shortcut_text("Del");
                if ui.add_enabled(self.view.has_selection(), delete).clicked() {
                    self.view.delete_selection();
                    ui.close();
                }
                let all =
                    egui::Button::new("Select All").shortcut_text(ctx.format_shortcut(&SELECT_ALL));
                if ui.add(all).clicked() {
                    self.view.select_all();
                    ui.close();
                }
            });
            ui.menu_button("View", |ui| {
                if ui.button("Zoom to fit").clicked() {
                    self.fit_pending = true;
                    ui.close();
                }
                if ui.button("Actual size (100%)").clicked() {
                    self.view.set_view(1.0, self.view.scroll());
                    ui.close();
                }
                ui.separator();
                ui.checkbox(&mut self.view.show_alias_indicator, "Alias indicator");
                ui.checkbox(&mut self.view.show_null_species, "Show null species");
            });
        });
    }

    fn status_bar(&self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let m = self.view.model();
            let (species, reactions) = self.view.selection();
            let mut text = format!(
                "{} · {} species · {} reactions · {:.0}%",
                self.title,
                m.species().len(),
                m.reactions().len(),
                self.view.zoom() * 100.0
            );
            if !species.is_empty() || !reactions.is_empty() {
                text.push_str(&format!(
                    " · selected: {} species, {} reactions",
                    species.len(),
                    reactions.len()
                ));
            }
            ui.label(text);
            if let Some(w) = self.warnings.first() {
                ui.separator();
                let more = self.warnings.len() - 1;
                let text = if more > 0 {
                    format!("{w} (and {more} more)")
                } else {
                    w.clone()
                };
                ui.colored_label(egui::Color32::from_rgb(0xA0, 0x50, 0x00), text);
            }
        });
    }
}

impl eframe::App for PathwayApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.shortcuts(&ctx);
        egui::Panel::top("menu").show(ui, |ui| self.menu_bar(ui));
        egui::Panel::bottom("status").show(ui, |ui| self.status_bar(ui));
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ui, |ui| {
                if self.fit_pending {
                    let r = ui.available_rect_before_wrap();
                    self.view.zoom_to_fit(
                        pt(f64::from(r.min.x), f64::from(r.min.y)),
                        f64::from(r.width()),
                        f64::from(r.height()),
                    );
                    self.fit_pending = false;
                }
                canvas::show(ui, &mut self.view, &mut self.canvas);
            });
    }
}
