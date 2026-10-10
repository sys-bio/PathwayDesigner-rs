//! The app shell: menu bar, tool palette, canvas, status bar and dialogs
//! (S §13).
//!
//! It opens the built-in sample models, and lets the user add species and
//! reactions, select, drag, rename, delete and undo. Panels and file access
//! arrive in later milestones.

use eframe::egui::{self, Key, KeyboardShortcut, Modifiers};
use pd_core::geometry::pt;
use pd_core::json::from_json;
use pd_core::layout::LayoutParams;
use pd_core::model::Model;
use pd_core::view::{Arrange, DiagramView, Request, Tool, ViewEvent};

use crate::antimony_panel::{AntimonyPanel, PanelAction};
use crate::canvas::{self, CanvasState, MenuAction};
use crate::fonts;
use crate::inspector::Inspector;
use crate::prefs::Preferences;
use crate::samples::SAMPLES;
use pd_core::antimony::ImportOptions;

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
    /// The preferences being edited (S §12).
    Preferences(Preferences),
}

pub struct PathwayApp {
    view: DiagramView,
    canvas: CanvasState,
    inspector: Inspector,
    antimony: AntimonyPanel,
    prefs: Preferences,
    dialog: Option<Dialog>,
    /// Name of what is shown, for the status bar.
    title: String,
    /// Warnings from the last load, shown in the status bar.
    warnings: Vec<String>,
    /// Zoom to fit on the next frame, once the canvas size is known.
    fit_pending: bool,
    /// The Deckard and Randomize options for auto-layout (S §13.2).
    layout_params: LayoutParams,
    /// Layout iterations per frame, adjusted to take about 8 ms (SPEC-Rust §7.2).
    layout_iterations: u32,
}

/// Aim for this much layout work per frame.
const LAYOUT_FRAME_BUDGET_MS: f64 = 8.0;
const LAYOUT_START_ITERATIONS: u32 = 5;

/// A seed from the clock, for anything random (SPEC-Rust §7.2).
fn clock_seed() -> u64 {
    web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos() as u64)
}

impl PathwayApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        fonts::install(&cc.egui_ctx);
        cc.egui_ctx.set_visuals(egui::Visuals::light());
        let mut app = Self {
            view: DiagramView::default(),
            canvas: CanvasState::default(),
            inspector: Inspector::default(),
            antimony: AntimonyPanel::default(),
            prefs: Preferences::load(cc.storage),
            dialog: None,
            title: "Untitled".to_owned(),
            warnings: Vec::new(),
            fit_pending: false,
            layout_params: LayoutParams::default(),
            layout_iterations: LAYOUT_START_ITERATIONS,
        };
        // Saved preferences apply from the start (fixes D-2).
        app.view.show_null_species = app.prefs.show_null_species;
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
            MenuAction::Arrange(cmd) => {
                self.view.arrange(cmd);
            }
        }
    }

    /// Diagram shortcuts apply only while no text field is being edited and no
    /// dialog is open, so those get their own keys first (SPEC-Rust §6.1).
    fn shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.egui_wants_keyboard_input() || self.dialog.is_some() {
            return;
        }
        // While a layout runs, only Esc does anything: it cancels the layout.
        if self.view.is_laying_out() {
            if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
                self.view.cancel_layout();
            }
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
                if ui.button("Preferences…").clicked() {
                    self.dialog = Some(Dialog::Preferences(self.prefs));
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
            // Align and distribute, also in the context menu (SPEC-Rust §6.3).
            ui.menu_button("Arrange", |ui| {
                for (i, cmd) in Arrange::ALL.into_iter().enumerate() {
                    if i == 6 {
                        ui.separator();
                    }
                    let tip = format!("Needs {} or more selected species", cmd.minimum());
                    if ui
                        .add_enabled(self.view.can_arrange(cmd), egui::Button::new(cmd.label()))
                        .on_disabled_hover_text(tip)
                        .clicked()
                    {
                        self.view.arrange(cmd);
                        ui.close();
                    }
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
                    .on_hover_text("Locked nodes are not moved by dragging, alignment or layout")
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
            ui.separator();
            self.layout_controls(ui);
            ui.separator();
            if button(
                ui,
                true,
                "Random network",
                "Replace the diagram with a random 10-species, 12-reaction network (a test tool)",
            ) {
                self.view.random_network(clock_seed());
                self.title = "Random network".to_owned();
                self.fit_pending = true;
            }
        });
    }

    /// Auto-layout and its options, or the layout's progress and Cancel
    /// while it runs.
    fn layout_controls(&mut self, ui: &mut egui::Ui) {
        if let Some(p) = self.view.layout_progress() {
            ui.add(
                egui::ProgressBar::new(p.done as f32 / p.total.max(1) as f32)
                    .desired_width(160.0)
                    .text(format!("Layout {}/{}", p.done, p.total)),
            );
            if ui
                .button("Cancel")
                .on_hover_text("Stop and put everything back (Esc)")
                .clicked()
            {
                self.view.cancel_layout();
            }
            return;
        }
        if ui
            .button("Auto-layout")
            .on_hover_text("Force-directed layout; locked nodes stay where they are")
            .clicked()
        {
            self.layout_iterations = LAYOUT_START_ITERATIONS;
            self.view.start_layout(self.layout_params, clock_seed());
        }
        ui.checkbox(&mut self.layout_params.deckard, "Deckard")
            .on_hover_text("Afterwards, shape the Bézier curves with the Deckard & Bergmann pass");
        ui.checkbox(&mut self.layout_params.randomize, "Randomize")
            .on_hover_text("Scatter the unlocked species at random before the layout");
    }

    /// Runs this frame's share of the layout: as many iterations as take
    /// about 8 ms, adjusting the count by at most ×2 a frame.
    fn run_layout(&mut self, ctx: &egui::Context) {
        if !self.view.is_laying_out() {
            return;
        }
        let start = web_time::Instant::now();
        self.view.layout_step(self.layout_iterations);
        let ms = start.elapsed().as_secs_f64() * 1000.0;
        let factor = (LAYOUT_FRAME_BUDGET_MS / ms.max(0.01)).clamp(0.5, 2.0);
        self.layout_iterations =
            ((f64::from(self.layout_iterations) * factor).round() as u32).max(1);
        ctx.request_repaint();
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
            Dialog::Preferences(draft) => {
                let mut ok = false;
                let catalytic_changed = draft.convert_catalytic != self.prefs.convert_catalytic;
                let will_reimport = catalytic_changed && self.antimony.last_import().is_some();
                let modal = egui::Modal::new(egui::Id::new("preferences")).show(ctx, |ui| {
                    ui.heading("Preferences");
                    ui.add_space(4.0);
                    ui.checkbox(&mut draft.convert_catalytic, "Convert catalytic reactions")
                        .on_hover_text(
                            "On Antimony import, A -> A + B becomes -> B with A as a modifier",
                        );
                    ui.checkbox(&mut draft.show_null_species, "Show null species")
                        .on_hover_text("Draw the null nodes of reactions with an empty side");
                    if will_reimport {
                        ui.colored_label(
                            egui::Color32::from_rgb(0xA0, 0x50, 0x00),
                            "The last Antimony text will be imported again with the new setting. \
                             Changes made since that import will be lost (Undo brings them back).",
                        );
                    }
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ok = ui.button("OK").clicked();
                        close |= ui.button("Cancel").clicked();
                    });
                });
                close |= modal.should_close() || ok;
                if ok {
                    let draft = *draft;
                    self.prefs = draft;
                    self.view.show_null_species = draft.show_null_species;
                    if will_reimport {
                        let opts = self.import_options();
                        if self.antimony.reimport(&mut self.view, &opts) {
                            self.fit_pending = true;
                        }
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

impl PathwayApp {
    /// How Antimony is imported, from the preferences.
    fn import_options(&self) -> ImportOptions {
        ImportOptions {
            convert_catalytic: self.prefs.convert_catalytic,
            seed: clock_seed(),
        }
    }
}

impl eframe::App for PathwayApp {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        self.prefs.save(storage);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.shortcuts(&ctx);
        self.run_layout(&ctx);
        let busy = self.view.is_laying_out();
        egui::Panel::top("menu").show(ui, |ui| self.menu_bar(ui));
        egui::Panel::top("toolbar").show(ui, |ui| self.toolbar(ui));
        egui::Panel::bottom("status").show(ui, |ui| self.status_bar(ui));
        egui::Panel::left("tools").resizable(false).show(ui, |ui| {
            ui.add_enabled_ui(!busy, |ui| self.tool_palette(ui))
        });
        let opts = self.import_options();
        let mut antimony_action = None;
        egui::Panel::right("inspector")
            .resizable(true)
            .default_size(280.0)
            .show(ui, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.add_enabled_ui(!busy, |ui| {
                        let antimony = &mut self.antimony;
                        self.inspector.show(ui, &mut self.view, |ui, view| {
                            ui.heading("Antimony");
                            antimony_action = antimony.show(ui, view, &opts);
                        });
                    });
                });
            });
        match antimony_action {
            Some(PanelAction::Imported) => {
                self.title = "Antimony".to_owned();
                self.warnings.clear();
                self.fit_pending = true;
            }
            Some(PanelAction::LayOut) => {
                self.layout_iterations = LAYOUT_START_ITERATIONS;
                self.view.start_layout(self.layout_params, clock_seed());
            }
            None => {}
        }
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
