//! The app shell: menu bar, tool palette, canvas, status bar and dialogs
//! (S §13).
//!
//! It opens the built-in sample models, and lets the user add species and
//! reactions, select, drag, rename, delete and undo. Panels and file access
//! arrive in later milestones.

use eframe::egui::{self, Key, KeyboardShortcut, Modifiers};
use pd_core::geometry::pt;
use pd_core::json::from_json;
use pd_core::model::Model;
use pd_core::view::{DiagramView, Request, Tool, ViewEvent};

use crate::canvas::{self, CanvasState, MenuAction};
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

/// The tool palette, in the order of S §13.2, with tooltips.
const TOOLS: [(Tool, &str, &str); 6] = [
    (Tool::Select, "Select", "Select, move and edit (Esc)"),
    (
        Tool::AddSpecies,
        "Species",
        "Add species: click on the canvas",
    ),
    (
        Tool::BI_BI,
        "A+B -> C+D",
        "Add a BiBi reaction: pick 2 reactants, then 2 products",
    ),
    (
        Tool::UNI_BI,
        "A -> B+C",
        "Add a UniBi reaction: pick 1 reactant, then 2 products",
    ),
    (
        Tool::BI_UNI,
        "A+B -> C",
        "Add a BiUni reaction: pick 2 reactants, then 1 product",
    ),
    (
        Tool::UNI_UNI,
        "A -> B",
        "Add a UniUni reaction: pick 1 reactant, then 1 product",
    ),
];

/// The dialog on screen, if any (SPEC-Rust §6.2).
enum Dialog {
    /// Asks for a new id for a species.
    Rename {
        id: String,
        text: String,
        error: Option<String>,
        focus: bool,
    },
    /// Confirms New.
    ConfirmNew,
}

pub struct PathwayApp {
    view: DiagramView,
    canvas: CanvasState,
    dialog: Option<Dialog>,
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
            dialog: None,
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

    /// New: an empty diagram, with the undo history cleared (S §7.7).
    fn new_document(&mut self) {
        self.view.set_model(Model::new());
        self.view.set_tool(Tool::Select);
        self.title = "Untitled".to_owned();
        self.warnings.clear();
        self.fit_pending = true;
    }

    fn ask_rename(&mut self, id: &str) {
        if let Some(text) = self.view.rename_initial(id) {
            self.dialog = Some(Dialog::Rename {
                id: id.to_owned(),
                text,
                error: None,
                focus: true,
            });
        }
    }

    /// Carries out a context-menu command.
    fn act(&mut self, action: MenuAction, canvas_centre: pd_core::geometry::Pt) {
        match action {
            MenuAction::CreateAlias(id) => {
                self.view.create_alias(&id);
            }
            MenuAction::Rename(id) => self.ask_rename(&id),
            MenuAction::GoToPrimary(id) => {
                self.view.go_to_primary(&id, canvas_centre);
            }
            MenuAction::DeleteSpecies(id) => {
                self.view.delete_species(&id);
            }
            MenuAction::RemoveNullNode(id) => {
                self.view.remove_null_node(&id);
            }
            MenuAction::ToggleLock => {
                self.view.toggle_lock();
            }
            MenuAction::DeleteReaction(id) => {
                self.view.delete_reaction(&id);
            }
            MenuAction::MakeNice(id) => {
                self.view.make_nice_reactions(&[id]);
            }
        }
    }

    /// Diagram shortcuts apply only while no text field is being edited and no
    /// dialog is open, so those get their own keys first (SPEC-Rust §6.1).
    fn shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.egui_wants_keyboard_input() || self.dialog.is_some() {
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
                    self.dialog = Some(Dialog::ConfirmNew);
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
                ui.separator();
                let has_species = self.view.model().selected_species().next().is_some();
                let lock = if self.view.lock_would_lock() {
                    "Lock"
                } else {
                    "Unlock"
                };
                if ui
                    .add_enabled(has_species, egui::Button::new(lock))
                    .clicked()
                {
                    self.view.toggle_lock();
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

    /// The top toolbar's reaction commands (S §13.2, §7.4). Each applies to
    /// the selected reactions and is greyed out when it can't do anything.
    fn toolbar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let any = self.view.model().selected_reactions().next().is_some();
            let uni_uni = self.view.selection_has_uni_uni();
            let bezier = self.view.selection_has_bezier();
            let button = |ui: &mut egui::Ui, enabled: bool, label: &str, tip: &str| {
                ui.add_enabled(enabled, egui::Button::new(label))
                    .on_hover_text(tip)
                    .on_disabled_hover_text(tip)
                    .clicked()
            };
            if button(
                ui,
                uni_uni,
                "Linear",
                "Toggle Linear: one straight line for a UniUni reaction",
            ) {
                self.view.toggle_linear();
            }
            if button(
                ui,
                any,
                "Set Bézier",
                "Draw the selected reactions as curves",
            ) {
                self.view.set_bezier();
            }
            if button(
                ui,
                any,
                "Set Straight",
                "Draw the selected reactions with straight legs",
            ) {
                self.view.set_straight();
            }
            if button(
                ui,
                bezier,
                "Smooth junction",
                "Toggle a smooth junction: legs pass through it in line",
            ) {
                self.view.toggle_smooth();
            }
            if button(
                ui,
                any,
                "Make Nice",
                "Centre the junction and reset the curves",
            ) {
                self.view.make_nice();
            }
        });
    }

    /// The left tool palette (S §13.2). The current tool is highlighted.
    fn tool_palette(&mut self, ui: &mut egui::Ui) {
        ui.add_space(4.0);
        for (tool, label, tip) in TOOLS {
            let selected = self.view.tool() == tool;
            let button = egui::Button::selectable(selected, label).min_size(egui::vec2(96.0, 0.0));
            if ui.add(button).on_hover_text(tip).clicked() {
                self.view.set_tool(tool);
            }
        }
        ui.separator();
        ui.label("New reactions:");
        ui.checkbox(&mut self.view.default_bezier, "Bézier")
            .on_hover_text("New reactions are drawn as curves");
        ui.add_enabled(
            self.view.default_bezier,
            egui::Checkbox::new(&mut self.view.default_smooth, "Smooth junction"),
        )
        .on_hover_text("New Bézier reactions get a smooth junction");
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
            if let Some(hint) = self.tool_hint() {
                ui.separator();
                ui.label(hint);
            }
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

    /// What the current tool expects next.
    fn tool_hint(&self) -> Option<String> {
        match self.view.tool() {
            Tool::Select => None,
            Tool::AddSpecies => Some("Click to add a species · Esc to stop".to_owned()),
            Tool::AddReaction {
                reactants,
                products,
            } => {
                let (r, p) = self.view.picked();
                Some(
                    if r < reactants {
                        format!("Pick reactant {} of {reactants}", r + 1)
                    } else {
                        format!("Pick product {} of {products}", p + 1)
                    } + " · click empty canvas to cancel",
                )
            }
        }
    }

    fn dialogs(&mut self, ctx: &egui::Context) {
        let Some(dialog) = &mut self.dialog else {
            return;
        };
        let mut close = false;
        match dialog {
            Dialog::Rename {
                id,
                text,
                error,
                focus,
            } => {
                let mut submit = false;
                let modal = egui::Modal::new(egui::Id::new("rename")).show(ctx, |ui| {
                    ui.heading("Rename species");
                    ui.add_space(4.0);
                    let edit = ui.add(egui::TextEdit::singleline(text).desired_width(240.0));
                    if std::mem::take(focus) {
                        edit.request_focus();
                    }
                    if edit.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                        submit = true;
                    }
                    if let Some(e) = error {
                        ui.colored_label(egui::Color32::from_rgb(0xCC, 0x33, 0x00), e.as_str());
                    }
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        submit |= ui.button("OK").clicked();
                        close |= ui.button("Cancel").clicked();
                    });
                });
                close |= modal.should_close();
                if submit {
                    match self.view.rename(id, text.trim()) {
                        Ok(_) => close = true,
                        Err(e) => *error = Some(format!("Can't rename: {e}.")),
                    }
                }
            }
            Dialog::ConfirmNew => {
                let mut confirmed = false;
                let modal = egui::Modal::new(egui::Id::new("confirm-new")).show(ctx, |ui| {
                    ui.heading("New diagram");
                    ui.label("Discard the current diagram? This can't be undone.");
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        confirmed = ui.button("Discard").clicked()
                            || ui.input(|i| i.key_pressed(Key::Enter));
                        close |= ui.button("Cancel").clicked();
                    });
                });
                close |= modal.should_close() || confirmed;
                if confirmed {
                    self.new_document();
                }
            }
        }
        if close {
            self.dialog = None;
        }
    }
}

impl eframe::App for PathwayApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.shortcuts(&ctx);
        egui::Panel::top("menu").show(ui, |ui| self.menu_bar(ui));
        egui::Panel::top("toolbar").show(ui, |ui| self.toolbar(ui));
        egui::Panel::bottom("status").show(ui, |ui| self.status_bar(ui));
        egui::Panel::left("tools")
            .resizable(false)
            .show(ui, |ui| self.tool_palette(ui));
        let mut output = None;
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
                output = Some(canvas::show(ui, &mut self.view, &mut self.canvas));
            });
        if let Some(out) = output {
            if let Some(Request::Rename(id)) = &out.view.request {
                self.ask_rename(id);
            }
            if let Some(action) = out.action {
                self.act(action, out.centre);
            }
        }
        self.dialogs(&ctx);
    }
}
