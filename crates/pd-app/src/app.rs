//! The app shell: menu bar, canvas and status bar (S §13).
//!
//! At R2 it can open the built-in sample models and show them with zoom and
//! pan. Panels, tools and file access arrive in later milestones.

use eframe::egui;
use pd_core::geometry::pt;
use pd_core::json::from_json;
use pd_core::view::DiagramView;

use crate::canvas;
use crate::fonts;
use crate::samples::SAMPLES;

/// The sample shown at start-up.
const START_SAMPLE: &str = "model9";

pub struct PathwayApp {
    view: DiagramView,
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
                let mut view = DiagramView::new(loaded.model);
                view.show_alias_indicator = self.view.show_alias_indicator;
                view.show_null_species = self.view.show_null_species;
                self.view = view;
                self.title = name.to_owned();
                self.warnings = loaded.warnings;
                self.fit_pending = true;
            }
            Err(e) => self.warnings = vec![format!("{name} could not be opened: {e}")],
        }
    }

    fn menu_bar(&mut self, ui: &mut egui::Ui) {
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("File", |ui| {
                if ui.button("New").clicked() {
                    self.view = DiagramView::default();
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
            let m = &self.view.model;
            ui.label(format!(
                "{} · {} species · {} reactions · {:.0}%",
                self.title,
                m.species().len(),
                m.reactions().len(),
                self.view.zoom() * 100.0
            ));
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
                canvas::show(ui, &mut self.view);
            });
    }
}
