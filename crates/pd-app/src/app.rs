//! The app shell: menu bar, tool palette, canvas, status bar and dialogs
//! (S §13).
//!
//! Opening and saving, unsaved changes and autosave are in `document.rs`.

use eframe::egui::{self, Key, KeyboardShortcut, Modifiers};
use pd_core::geometry::Pt;
use pd_core::geometry::pt;
use pd_core::layout::LayoutParams;
use pd_core::render::constants::ZOOM_STEP;
use pd_core::view::{Arrange, DiagramView, Request, Tool, ViewEvent};

use crate::antimony_panel::{AntimonyPanel, PanelAction};
use crate::autosave::{self, Autosave};
use crate::canvas::{self, CanvasState, MenuAction};
use crate::document::{Action, Document};
use crate::files::{Files, Filter, Purpose};
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
/// The licences of everything the app is built from (SPEC-Rust §8.5). The web
/// app ships the file next to itself; the desktop app links to the published one.
#[cfg(target_arch = "wasm32")]
const THIRD_PARTY_PAGE: &str = "THIRD-PARTY.html";
#[cfg(not(target_arch = "wasm32"))]
const THIRD_PARTY_PAGE: &str = "https://sys-bio.github.io/PathwayDesigner-rs/THIRD-PARTY.html";
const SELECT_ALL: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::A);
/// Find an item in the list on the left.
const FIND: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::F);
/// Ctrl+Alt (Cmd+Option), not Ctrl+Shift: Chrome keeps Ctrl+Shift+C for its
/// developer tools (S §8.5).
const COPY_STYLE: KeyboardShortcut =
    KeyboardShortcut::new(Modifiers::COMMAND.plus(Modifiers::ALT), Key::C);
const PASTE_STYLE: KeyboardShortcut =
    KeyboardShortcut::new(Modifiers::COMMAND.plus(Modifiers::ALT), Key::V);
const NEW: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::N);
const OPEN: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::O);
const SAVE_AS: KeyboardShortcut =
    KeyboardShortcut::new(Modifiers::COMMAND.plus(Modifiers::SHIFT), Key::S);
const SAVE: KeyboardShortcut = KeyboardShortcut::new(Modifiers::COMMAND, Key::S);

/// Where the project lives, for the About box.
const HOME_PAGE: &str = "https://github.com/sys-bio/PathwayDesigner-rs";

/// The tool palette, in the order of S §13.2, with tooltips.
const TOOLS: [(Tool, &str, &str); 9] = [
    (Tool::Select, "Select", "Select, move and edit (Esc)"),
    (
        Tool::AddSpecies,
        "Species",
        "Add species: click on the canvas",
    ),
    (
        Tool::AddText,
        "Text",
        "Add text: click where it should go, then type in the Text panel",
    ),
    (
        Tool::AddCompartment,
        "Compartment",
        "Add a compartment: drag out its box (species inside join it)",
    ),
    (
        Tool::ANY_REACTION,
        "A+… -> B+…",
        "Add a reaction with any number of reactants and products: pick the \
         reactants, press Enter, pick the products, then press Enter again",
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

/// A keyboard shortcut for a file command.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FileShortcut {
    New,
    Open,
    Save,
    SaveAs,
}

/// The dialog on screen, if any (SPEC-Rust §6.2).
pub(crate) enum Dialog {
    /// Asks for a name to save an item's look under (S §8.6).
    SaveStyle {
        target: pd_core::view::Target,
        text: String,
        error: Option<String>,
        focus: bool,
    },
    /// Asks for a new name for a saved style (S §8.6).
    RenameStyle {
        kind: pd_core::model::StyleKind,
        old: String,
        text: String,
        error: Option<String>,
        focus: bool,
    },
    /// File ▸ Open from KEGG…: asks for a map code.
    OpenKegg {
        text: String,
        error: Option<String>,
        focus: bool,
    },
    /// Asks for a new id for a species.
    Rename {
        id: String,
        text: String,
        error: Option<String>,
        focus: bool,
    },
    /// Unsaved changes: save them before `Action`? (S §13.1)
    ConfirmDiscard(Action),
    /// Changes were lost last time: get them back?
    Recover(Autosave),
    About,
    /// The preferences being edited (S §12).
    Preferences(Preferences),
    /// Options for PNG, PDF or printing (S §11.6).
    Export(crate::export::ExportKind, crate::export::ExportSettings),
}

/// The right panel's tabs (S §13.3).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum RightTab {
    /// The inspector: the groups for what is selected.
    #[default]
    Properties,
    /// The Antimony text, with Load Antimony and Get Antimony.
    Antimony,
    /// The saved styles of the model and of My Styles (S §8.6).
    Styles,
    /// The network's structure: its stoichiometry matrix, flux null space and
    /// conservation laws (docs/structural-analysis-design.md).
    Structure,
    /// Annotate Species from Names, while it searches and is reviewed
    /// (docs/chebi-lookup-design.md C6); there only then.
    Annotate,
}

pub struct PathwayApp {
    pub(crate) ctx: egui::Context,
    pub(crate) view: DiagramView,
    canvas: CanvasState,
    inspector: Inspector,
    /// Annotate Species from Names, under way (docs/chebi-lookup-design.md).
    pub(crate) annotate: Option<crate::annotate_tab::AnnotateTab>,
    pub(crate) antimony: AntimonyPanel,
    /// The Structure tab and the matrix panel.
    pub(crate) structure: crate::structure_panel::StructurePanel,
    /// Which of the right panel's tabs is shown.
    pub(crate) right_tab: RightTab,
    /// The Edit Style window, while it is open (S §8.6).
    pub(crate) style_editor: Option<crate::styles_panel::StyleEditor>,
    prefs: Preferences,
    pub(crate) dialog: Option<Dialog>,
    /// Help ▸ Mouse and Keyboard is open. Not modal: it can stay open while
    /// drawing.
    pub(crate) keys_help: bool,
    /// Help ▸ PathwayDesigner Help, a panel at the right.
    pub(crate) help: crate::help::Help,
    /// File ▸ Open from Gallery… (S §11.8).
    pub(crate) gallery: crate::gallery::Gallery,
    /// A KEGG map's compound names being looked up in PubChem.
    pub(crate) naming: Option<crate::kegg_names::Naming>,
    /// A KEGG map being fetched by File ▸ Open from KEGG… (desktop), by code.
    pub(crate) kegg_fetch: Option<(String, crate::gallery::Pending)>,
    pub(crate) files: Files,
    pub(crate) doc: Document,
    /// What to do once a Save requested by the unsaved-changes prompt is done.
    pub(crate) after_save: Option<Action>,
    /// The user chose to quit without saving: let the window close.
    pub(crate) allow_close: bool,
    pub(crate) last_title: String,
    /// Problems from the last operation, shown in the status bar.
    pub(crate) warnings: Vec<String>,
    /// Problems and notes about an SBML file being saved, shown once it is.
    pub(crate) export_messages: Vec<String>,
    /// The graphics output options last used.
    pub(crate) export_settings: crate::export::ExportSettings,
    /// Recently used files (desktop).
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) recent: crate::recent::Recent,
    /// The macOS menu bar, with About, Settings and Quit. Made in `main`,
    /// because macOS menus must be made on the main thread.
    #[cfg(target_os = "macos")]
    pub(crate) mac_menu: Option<crate::macos_menu::MacMenu>,
    /// What the last operation did, shown in the status bar.
    pub(crate) notice: Option<String>,
    /// Zoom to fit on the next frame, once the canvas size is known.
    pub(crate) fit_pending: bool,
    /// The canvas the last zoom to fit was for, while the fit is kept up to
    /// date with the canvas's size (until the user does anything).
    fit: Option<egui::Rect>,
    /// The canvas's centre on screen last frame, for centring things.
    canvas_centre: Pt,
    /// The file version the model needed last frame, to say when it starts
    /// needing version 4 (SPEC-Rust §4.2).
    pub(crate) needed_version: u32,
    /// A file read but not opened yet: it is opened on the next frame, after
    /// this one has shown the busy cursor.
    pub(crate) opening: Option<crate::files::FileEvent>,
    /// A file shortcut pressed while a text field had the keyboard: it runs
    /// next frame, once the field has let go and applied what was typed.
    pending_shortcut: Option<FileShortcut>,
    tree: crate::tree::TreeState,
    /// The Deckard and Randomize options for auto-layout (S §13.2).
    layout_params: LayoutParams,
    /// Layout iterations per frame, adjusted to take about 8 ms (SPEC-Rust §7.2).
    layout_iterations: u32,
}

/// Aim for this much layout work per frame.
const LAYOUT_FRAME_BUDGET_MS: f64 = 8.0;
const LAYOUT_START_ITERATIONS: u32 = 5;

/// The pointer's place on the diagram for the status bar, in whole units, or
/// nothing while it is off the canvas (S §13.5).
fn pointer_text(at: Option<Pt>) -> String {
    match at {
        // `+ 0.0` turns -0 into 0.
        Some(p) => format!("x {}, y {}", p.x.round() + 0.0, p.y.round() + 0.0),
        None => String::new(),
    }
}

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
            ctx: cc.egui_ctx.clone(),
            view: DiagramView::default(),
            canvas: CanvasState::default(),
            inspector: Inspector::default(),
            annotate: None,
            antimony: AntimonyPanel::default(),
            structure: Default::default(),
            right_tab: RightTab::default(),
            style_editor: None,
            prefs: Preferences::load(cc.storage),
            dialog: None,
            keys_help: false,
            help: Default::default(),
            gallery: Default::default(),
            naming: None,
            kegg_fetch: None,
            files: Files::default(),
            doc: Document::default(),
            after_save: None,
            allow_close: false,
            last_title: String::new(),
            warnings: Vec::new(),
            export_messages: Vec::new(),
            export_settings: crate::export::ExportSettings::default(),
            #[cfg(not(target_arch = "wasm32"))]
            recent: crate::recent::Recent::load(cc.storage),
            #[cfg(target_os = "macos")]
            mac_menu: None,
            notice: None,
            fit_pending: false,
            fit: None,
            canvas_centre: Pt::ZERO,
            needed_version: pd_core::json::DELPHI_VERSION,
            opening: None,
            pending_shortcut: None,
            tree: crate::tree::TreeState::default(),
            layout_params: LayoutParams::default(),
            layout_iterations: LAYOUT_START_ITERATIONS,
        };
        // Saved preferences apply from the start (fixes D-2).
        app.view.show_null_species = app.prefs.show_null_species;
        app.view.selected_reaction_colour = app.prefs.selected_reaction;
        app.view.show_reversible = app.prefs.show_reversible;
        app.view.show_grid = app.prefs.show_grid;
        app.view.snap_to_grid = app.prefs.snap_to_grid;
        app.view.set_my_styles(crate::my_styles::load(cc.storage));
        app.perform(Action::OpenSample(START_SAMPLE));
        app.notice = None;
        // Changes lost last time (a closed tab, a crash): offer them back.
        let lost = cc
            .storage
            .and_then(|s| s.get_string(autosave::KEY))
            .and_then(|t| Autosave::decode(&t));
        if let Some(a) = lost {
            app.dialog = Some(Dialog::Recover(a));
        }
        app
    }

    /// Zooms to fit when asked to (at startup, and after opening or
    /// importing). The canvas may not have its final size yet: in a browser
    /// tab it can get it well after the first frames. So until the user does
    /// anything (presses a button, scrolls, zooms, types), the fit is done
    /// again whenever the canvas changes size.
    fn fit_to_canvas(&mut self, ui: &egui::Ui) {
        let rect = ui.available_rect_before_wrap();
        if std::mem::take(&mut self.fit_pending) {
            self.fit = Some(egui::Rect::NOTHING);
        }
        let Some(fitted) = self.fit else {
            return;
        };
        let touched = ui.input(|i| {
            i.events.iter().any(|e| {
                matches!(
                    e,
                    egui::Event::PointerButton { pressed: true, .. }
                        | egui::Event::MouseWheel { .. }
                        | egui::Event::Zoom(_)
                        | egui::Event::Key { pressed: true, .. }
                        | egui::Event::Text(_)
                        | egui::Event::Touch { .. }
                )
            })
        });
        if touched {
            self.fit = None;
            return;
        }
        if rect != fitted && rect.width() > 0.0 && rect.height() > 0.0 {
            self.view.zoom_to_fit(
                pt(f64::from(rect.min.x), f64::from(rect.min.y)),
                f64::from(rect.width()),
                f64::from(rect.height()),
            );
            self.fit = Some(rect);
        }
    }

    /// File ▸ Open Recent (desktop).
    #[cfg(not(target_arch = "wasm32"))]
    fn recent_menu(&mut self, ui: &mut egui::Ui) {
        ui.add_enabled_ui(!self.recent.files.is_empty(), |ui| {
            ui.menu_button("Open Recent", |ui| {
                let mut chosen = None;
                for path in &self.recent.files {
                    let name = path.file_name().map_or_else(
                        || path.display().to_string(),
                        |n| n.to_string_lossy().into_owned(),
                    );
                    if ui
                        .button(name)
                        .on_hover_text(path.display().to_string())
                        .clicked()
                    {
                        chosen = Some(path.clone());
                        ui.close();
                    }
                }
                ui.separator();
                if ui.button("Clear the list").clicked() {
                    self.recent.files.clear();
                    ui.close();
                }
                if let Some(path) = chosen {
                    self.request(Action::OpenPath(path));
                }
            });
        });
    }

    /// Carries out what was chosen from the macOS menu bar.
    #[cfg(target_os = "macos")]
    fn mac_menu_commands(&mut self) {
        use crate::macos_menu::MenuCommand;
        let Some(command) = self.mac_menu.as_ref().and_then(|m| m.poll()) else {
            return;
        };
        match command {
            MenuCommand::About => self.dialog = Some(Dialog::About),
            MenuCommand::Help => self.help.open = true,
            MenuCommand::KeysHelp => self.keys_help = true,
            MenuCommand::Preferences => self.dialog = Some(Dialog::Preferences(self.prefs)),
            // Asks about unsaved changes first, as closing the window does.
            MenuCommand::Quit => self.request(Action::Quit),
        }
    }

    /// Starts an auto-layout with the toolbar's settings.
    pub(crate) fn start_layout(&mut self) {
        self.layout_iterations = LAYOUT_START_ITERATIONS;
        self.view.start_layout(self.layout_params, clock_seed());
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

    /// The first time an edit makes the document need a version newer than
    /// the Delphi app reads (free text, say), the status bar says it can no
    /// longer open it (SPEC-Rust §4.2). Opening such a file says nothing, nor
    /// does a later step up, which the Delphi app couldn't open either.
    fn note_version_change(&mut self) {
        use pd_core::json::DELPHI_VERSION;
        let needed = pd_core::json::needed_version(self.view.model());
        if needed > DELPHI_VERSION
            && self.needed_version <= DELPHI_VERSION
            && self.view.is_modified()
        {
            self.notice = Some("This file now needs PathwayDesigner (Rust) to open".to_owned());
        }
        self.needed_version = needed;
    }

    /// Copy Style for the one selected item, with a word in the status bar.
    fn copy_selected_style(&mut self) {
        if let Some(id) = self.view.copy_selected_style() {
            self.notice = Some(format!("Copied the style of {id}"));
        }
    }

    /// Carries out a context-menu command.
    fn act(&mut self, action: MenuAction, canvas_centre: pd_core::geometry::Pt) {
        match action {
            MenuAction::CreateAlias(id) => {
                self.view.create_alias(&id);
            }
            MenuAction::SplitIntoAliases(id) => {
                self.view.split_into_aliases(&id);
            }
            MenuAction::SelectAllDrawings(id) => {
                self.view.select_all_drawings(&id);
            }
            MenuAction::AnnotateSpecies(id) => self.start_annotating(Some(&id)),
            MenuAction::AnnotateReactions(id) => self.start_annotating_reactions(Some(&id)),
            MenuAction::MarkModifier(id, species, true) => {
                self.view.mark_as_modifier(&id, &species);
            }
            MenuAction::MarkModifier(id, species, false) => {
                self.view.unmark_as_modifier(&id, &species);
            }
            MenuAction::MergeAliases(id) => {
                self.view.merge_aliases(&id);
            }
            MenuAction::SplitOffSelected(id) => {
                self.view.split_off_selected(&id);
            }
            MenuAction::MergeSelectedInto(id) => {
                self.view.merge_selected_into(&id);
            }
            MenuAction::MergeIntoPrimary(id) => {
                self.view.merge_alias_into_primary(&id);
            }
            MenuAction::CopyStyle(target) => {
                if let Some(id) = self.view.copy_style(&target) {
                    self.notice = Some(format!("Copied the style of {id}"));
                }
            }
            MenuAction::PasteStyle => {
                self.view.paste_style();
            }
            MenuAction::SaveStyle(target) => {
                self.dialog = Some(Dialog::SaveStyle {
                    target,
                    text: String::new(),
                    error: None,
                    focus: true,
                });
            }
            MenuAction::ApplyStyle(kind, name) => {
                self.view.apply_style(kind, &name);
            }
            MenuAction::UpdateStyle(name, target) => {
                if let Err(e) = self.view.update_style(&name, &target) {
                    self.notice = Some(format!("Can't change the style: {e}."));
                }
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
            MenuAction::AddParticipant(id, role) => {
                self.view.begin_adding(&id, role);
            }
            MenuAction::RemoveParticipant(id, species) => {
                self.view.remove_participant(&id, &species);
            }
            MenuAction::AddStem(id) => {
                self.view.add_stem(&id);
            }
            MenuAction::ModifierKind(id, species, kind) => {
                self.view.set_modifier_kind(&id, &species, kind);
            }
            MenuAction::AddCornerPoint(id, line, piece, at) => {
                self.view.add_corner_point(&id, line, piece, at);
            }
            MenuAction::LoopToCornerPoints(id, modifier) => {
                self.view.loop_to_corner_points(&id, modifier);
            }
            MenuAction::RemoveCornerPoint(id, line, point) => {
                self.view.remove_corner_point(&id, line, point);
            }
            MenuAction::StraightenLine(id, line) => {
                self.view.straighten_line(&id, line);
            }
            MenuAction::StraightenStem(id) => {
                self.view.straighten_stem(&id);
            }
            MenuAction::RemoveStem(id) => {
                self.view.remove_stem(&id);
            }
            MenuAction::MakeNice(id) => {
                self.view.make_nice_reactions(&[id]);
            }
            MenuAction::Arrange(cmd) => {
                self.view.arrange(cmd);
            }
            MenuAction::EditText(id) => self.inspector.edit_text(&id),
            MenuAction::DeleteText(id) => {
                self.view.delete_text(&id);
            }
            MenuAction::DeleteCompartment(id) => {
                self.view.delete_compartment(&id);
            }
        }
    }

    fn file_shortcut(&mut self, cmd: FileShortcut) {
        match cmd {
            FileShortcut::Save => self.save(false),
            FileShortcut::SaveAs => self.save(true),
            FileShortcut::New => self.request(Action::New),
            FileShortcut::Open => self.request(Action::Open),
        }
    }

    /// Diagram shortcuts apply only while no text field is being edited and no
    /// dialog is open, so those get their own keys first (SPEC-Rust §6.1).
    /// The file shortcuts (New, Open, Save, Save As) mean nothing to a text
    /// field, so they work while typing too: the field lets go first, which
    /// applies what was typed, and the command runs on the next frame.
    fn shortcuts(&mut self, ctx: &egui::Context) {
        if self.dialog.is_some() {
            return;
        }
        if let Some(cmd) = self.pending_shortcut.take() {
            self.file_shortcut(cmd);
        }
        if ctx.egui_wants_keyboard_input() {
            let cmd = ctx.input_mut(|i| {
                if i.consume_shortcut(&SAVE_AS) {
                    Some(FileShortcut::SaveAs)
                } else if i.consume_shortcut(&SAVE) {
                    Some(FileShortcut::Save)
                } else if i.consume_shortcut(&NEW) {
                    Some(FileShortcut::New)
                } else if i.consume_shortcut(&OPEN) {
                    Some(FileShortcut::Open)
                } else {
                    None
                }
            });
            if let Some(cmd) = cmd {
                ctx.memory_mut(|m| {
                    if let Some(id) = m.focused() {
                        m.surrender_focus(id);
                    }
                });
                self.pending_shortcut = Some(cmd);
                ctx.request_repaint();
            }
            return;
        }
        // While a layout runs, only Esc does anything: it cancels the layout.
        if self.view.is_laying_out() {
            if ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
                self.view.cancel_layout();
            }
            return;
        }
        // Save As before Save, Redo before Undo: with Shift they would also
        // match the shorter shortcut.
        if ctx.input_mut(|i| i.consume_shortcut(&SAVE_AS)) {
            self.file_shortcut(FileShortcut::SaveAs);
        }
        if ctx.input_mut(|i| i.consume_shortcut(&SAVE)) {
            self.file_shortcut(FileShortcut::Save);
        }
        if ctx.input_mut(|i| i.consume_shortcut(&NEW)) {
            self.file_shortcut(FileShortcut::New);
        }
        if ctx.input_mut(|i| i.consume_shortcut(&OPEN)) {
            self.file_shortcut(FileShortcut::Open);
        }
        if ctx.input_mut(|i| i.consume_shortcut(&REDO) || i.consume_shortcut(&REDO_Y)) {
            self.view.redo();
        }
        if ctx.input_mut(|i| i.consume_shortcut(&UNDO)) {
            self.view.undo();
        }
        if ctx.input_mut(|i| i.consume_shortcut(&SELECT_ALL)) {
            self.view.select_all();
        }
        if ctx.input_mut(|i| i.consume_shortcut(&FIND)) {
            self.tree.focus_filter = true;
        }
        if ctx.input_mut(|i| i.consume_shortcut(&COPY_STYLE)) {
            self.copy_selected_style();
        }
        if ctx.input_mut(|i| i.consume_shortcut(&PASTE_STYLE)) {
            self.view.paste_style();
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
        if self.view.tool().is_open_reaction()
            && ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Enter))
        {
            self.view.handle(&ViewEvent::Confirm);
        }
        // Arrow keys nudge the selection, by grid lines while snapping
        // (S §7.5, §7.6). (A focused widget, such as a row of the model tree,
        // keeps them: shortcuts don't run while one has focus.)
        {
            let presses = ctx.input_mut(|i| {
                let mut presses = Vec::new();
                for (key, dir) in [
                    (Key::ArrowLeft, pt(-1.0, 0.0)),
                    (Key::ArrowRight, pt(1.0, 0.0)),
                    (Key::ArrowUp, pt(0.0, -1.0)),
                    (Key::ArrowDown, pt(0.0, 1.0)),
                ] {
                    // (big, free): Alt places freely, Shift takes big steps.
                    // Shift first: a plain pattern also matches the key with
                    // Shift.
                    for (mods, big, free) in [
                        (Modifiers::ALT.plus(Modifiers::SHIFT), true, true),
                        (Modifiers::ALT, false, true),
                        (Modifiers::SHIFT, true, false),
                        (Modifiers::NONE, false, false),
                    ] {
                        while i.consume_key(mods, key) {
                            presses.push((dir, big, free));
                        }
                    }
                }
                presses
            });
            for (dir, big, free) in presses {
                self.view.nudge_key(dir, big, free);
            }
        }
    }

    fn menu_bar(&mut self, ui: &mut egui::Ui) {
        let ctx = ui.ctx().clone();
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("File", |ui| {
                let item = |ui: &mut egui::Ui, label: &str, shortcut: Option<&KeyboardShortcut>| {
                    let mut b = egui::Button::new(label);
                    if let Some(s) = shortcut {
                        b = b.shortcut_text(ctx.format_shortcut(s));
                    }
                    ui.add(b).clicked()
                };
                if item(ui, "New", Some(&NEW)) {
                    self.request(Action::New);
                    ui.close();
                }
                if item(ui, "Open…", Some(&OPEN)) {
                    self.request(Action::Open);
                    ui.close();
                }
                #[cfg(not(target_arch = "wasm32"))]
                self.recent_menu(ui);
                if item(ui, "Open from Gallery…", None) {
                    self.gallery.open = true;
                    ui.close();
                }
                if item(ui, "Open from KEGG…", None) {
                    self.dialog = Some(Dialog::OpenKegg {
                        text: String::new(),
                        error: None,
                        focus: true,
                    });
                    ui.close();
                }
                ui.menu_button("Open sample", |ui| {
                    for (name, _) in SAMPLES {
                        if ui.button(*name).clicked() {
                            self.request(Action::OpenSample(name));
                            ui.close();
                        }
                    }
                });
                if item(ui, "Save", Some(&SAVE)) {
                    self.save(false);
                    ui.close();
                }
                if item(ui, "Save As…", Some(&SAVE_AS)) {
                    self.save(true);
                    ui.close();
                }
                ui.separator();
                if item(ui, "Import Antimony…", None) {
                    self.files
                        .open(&self.ctx, Purpose::ImportAntimony, Filter::Antimony);
                    ui.close();
                }
                if item(ui, "Export Antimony…", None) {
                    self.export_antimony(false);
                    ui.close();
                }
                if ui
                    .button("Export Antimony with Layout Statements…")
                    .on_hover_text(
                        "The drawing as a diagram comment, which PathwayDesigner reads, \
                         and as Antimony's layout statements, for tellurium and SBMLNetwork",
                    )
                    .clicked()
                {
                    self.export_antimony(true);
                    ui.close();
                }
                ui.separator();
                if item(ui, "Import SBML…", None) {
                    self.files
                        .open(&self.ctx, Purpose::ImportSbml, Filter::Sbml);
                    ui.close();
                }
                if item(ui, "Save to SBML…", None) {
                    self.export_sbml();
                    ui.close();
                }
                ui.separator();
                ui.menu_button("Export", |ui| {
                    if ui.button("SVG…").clicked() {
                        self.export_svg();
                        ui.close();
                    }
                    if ui.button("PNG…").clicked() {
                        self.ask_export(crate::export::ExportKind::Png);
                        ui.close();
                    }
                    if ui.button("PDF…").clicked() {
                        self.ask_export(crate::export::ExportKind::Pdf);
                        ui.close();
                    }
                });
                if item(ui, "Print…", None) {
                    self.ask_export(crate::export::ExportKind::Print);
                    ui.close();
                }
                // On macOS these are in the application menu (macos_menu.rs).
                #[cfg(not(target_os = "macos"))]
                {
                    ui.separator();
                    if item(ui, "Preferences…", None) {
                        self.dialog = Some(Dialog::Preferences(self.prefs));
                        ui.close();
                    }
                }
                #[cfg(not(any(target_arch = "wasm32", target_os = "macos")))]
                {
                    ui.separator();
                    if item(ui, "Quit", None) {
                        self.request(Action::Quit);
                        ui.close();
                    }
                }
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
                if ui.button("Select All Species").clicked() {
                    self.view.select_all_species();
                    ui.close();
                }
                if ui.button("Select All Reactions").clicked() {
                    self.view.select_all_reactions();
                    ui.close();
                }
                ui.separator();
                let one = self.view.selection_count() == 1;
                let copy =
                    egui::Button::new("Copy Style").shortcut_text(ctx.format_shortcut(&COPY_STYLE));
                if ui
                    .add_enabled(one, copy)
                    .on_disabled_hover_text("Select one item")
                    .clicked()
                {
                    self.copy_selected_style();
                    ui.close();
                }
                let paste = egui::Button::new("Paste Style")
                    .shortcut_text(ctx.format_shortcut(&PASTE_STYLE));
                if ui
                    .add_enabled(self.view.can_paste_style(), paste)
                    .on_disabled_hover_text("Copy an item's style, then select items of that kind")
                    .clicked()
                {
                    self.view.paste_style();
                    ui.close();
                }
                ui.separator();
                if ui
                    .add_enabled(
                        self.annotate.is_none() && !self.view.model().species().is_empty(),
                        egui::Button::new("Annotate Species from Names…"),
                    )
                    .on_hover_text(
                        "Look up the selected species (or all) in BiGG, ChEBI and PubChem, \
                         and review what is found",
                    )
                    .clicked()
                {
                    self.start_annotating(None);
                    ui.close();
                }
                if ui
                    .add_enabled(
                        self.annotate.is_none() && !self.view.model().reactions().is_empty(),
                        egui::Button::new("Annotate Reactions from Names…"),
                    )
                    .on_hover_text(
                        "Look up the selected reactions (or all) in BiGG and Rhea, \
                         and review what is found",
                    )
                    .clicked()
                {
                    self.start_annotating_reactions(None);
                    ui.close();
                }
            });
            // Align and distribute, also in the context menu (SPEC-Rust §6.3).
            ui.menu_button("Arrange", |ui| {
                for cmd in Arrange::ALL {
                    if cmd.starts_group() {
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
                if ui
                    .add_enabled(
                        self.view.can_place_modifiers(),
                        egui::Button::new("Place Modifiers by Their Reactions"),
                    )
                    .on_hover_text(
                        "Gives each modifier line from far off its own drawing beside its                          reaction (the selected reactions, or all), and removes drawings                          left joined to nothing",
                    )
                    .clicked()
                {
                    let (_, placed) = self.view.place_modifiers();
                    self.notice = Some(placed_notice(placed));
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
                    .on_hover_text("Locked nodes are not moved by dragging, alignment or layout")
                    .clicked()
                {
                    self.view.toggle_lock();
                    ui.close();
                }
            });
            ui.menu_button("View", |ui| {
                let zoom_in = egui::Button::new("Zoom In").shortcut_text("Ctrl+Wheel");
                if ui.add(zoom_in).clicked() {
                    self.zoom_step(1);
                    ui.close();
                }
                let zoom_out = egui::Button::new("Zoom Out").shortcut_text("Ctrl+Wheel");
                if ui.add(zoom_out).clicked() {
                    self.zoom_step(-1);
                    ui.close();
                }
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
                // A setting of the diagram, saved in its file (S §7.6).
                let mut modifiers = self.view.show_modifiers();
                if ui
                    .checkbox(&mut modifiers, "Modifier lines")
                    .on_hover_text(
                        "Show the dashed lines from modifiers to their reactions; saved with the diagram",
                    )
                    .changed()
                {
                    self.view.set_show_modifiers(modifiers);
                }
                // A setting of the diagram, saved in its file
                // (docs/display-names-design.md D4).
                let mut names = self.view.show_display_names();
                if ui
                    .checkbox(&mut names, "Display names")
                    .on_hover_text(
                        "Label items with their display names instead of their ids, where they have one; saved with the diagram",
                    )
                    .changed()
                {
                    self.view.set_show_display_names(names);
                }
                ui.checkbox(&mut self.view.show_reversible, "Reversible arrowheads")
                    .on_hover_text("Draw reversible reactions with an arrowhead at both ends");
                ui.checkbox(&mut self.view.show_grid, "Grid").on_hover_text(
                    "Lines every 20 units behind the diagram, as a guide; never exported",
                );
                ui.checkbox(&mut self.view.snap_to_grid, "Snap to grid")
                    .on_hover_text(
                        "Dragged and new items land on grid points; hold Alt to place freely",
                    );
                ui.separator();
                // Restyles the whole diagram, in one undo step (S §8.7).
                ui.menu_button("Theme", |ui| {
                    use pd_core::themes::Theme;
                    let current = self.view.theme().unwrap_or(Theme::Classic);
                    for (theme, tip) in [
                        (
                            Theme::Classic,
                            "The usual look: rounded boxes with the names inside",
                        ),
                        (
                            Theme::Escher,
                            "Like Escher's maps: small circles with names beside them, \
                             smaller cofactors, thick reaction lines",
                        ),
                    ] {
                        if ui
                            .selectable_label(current == theme, theme.label())
                            .on_hover_text(tip)
                            .clicked()
                        {
                            self.view.apply_theme(theme);
                            self.notice = Some(format!(
                                "{} theme: every item restyled (Undo puts it back)",
                                theme.label()
                            ));
                            ui.close();
                        }
                    }
                });
            });
            // On macOS, About is in the application menu (macos_menu.rs).
            #[cfg(not(target_os = "macos"))]
            ui.menu_button("Help", |ui| {
                if ui.button("PathwayDesigner Help").clicked() {
                    self.help.open = true;
                    ui.close();
                }
                if ui.button("Mouse and Keyboard").clicked() {
                    self.keys_help = true;
                    ui.close();
                }
                ui.separator();
                if ui.button("About PathwayDesigner").clicked() {
                    self.dialog = Some(Dialog::About);
                    ui.close();
                }
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
            // Shows pressed when the selected Bézier reactions are smooth.
            let smooth =
                egui::Button::selectable(self.view.selection_is_smooth(), "Smooth junction");
            let tip = "Smooth junction: legs pass through it in line. Click again to turn it off";
            if ui
                .add_enabled(bezier, smooth)
                .on_hover_text(tip)
                .on_disabled_hover_text(tip)
                .clicked()
            {
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
            self.start_layout();
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
        ui.separator();
        let found = egui::ScrollArea::vertical()
            .id_salt("tree")
            .show(ui, |ui| {
                crate::tree::show(ui, &mut self.view, &mut self.tree)
            })
            .inner;
        if let Some(at) = found {
            self.view.centre_on(at, self.canvas_centre);
        }
    }

    /// The status bar: the zoom control at the right, and the rest in the
    /// space left of it.
    fn status_bar(&mut self, ui: &mut egui::Ui) {
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            self.zoom_control(ui);
            ui.separator();
            // A fixed width, so the text beside it doesn't move as it changes.
            ui.add_sized(
                [118.0, ui.available_height()],
                egui::Label::new(pointer_text(self.view.pointer_world())),
            )
            .on_hover_text("Where the pointer is on the diagram");
            ui.separator();
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                // What doesn't fit is cut short with "…", never drawn under
                // the pointer's coordinates.
                ui.set_clip_rect(ui.max_rect().intersect(ui.clip_rect()));
                self.status_text(ui)
            });
        });
    }

    fn status_text(&self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            let m = self.view.model();
            let (species, reactions) = self.view.selection();
            let mark = if self.view.is_modified() {
                " (unsaved)"
            } else {
                ""
            };
            let mut text = format!(
                "{}{mark} · {} species · {} reactions",
                self.doc.name,
                m.species().len(),
                m.reactions().len(),
            );
            if !species.is_empty() || !reactions.is_empty() {
                text.push_str(&format!(
                    " · selected: {} species, {} reactions",
                    species.len(),
                    reactions.len()
                ));
            }
            let short = |text: String| egui::Label::new(text).truncate();
            ui.add(short(text));
            #[cfg(target_arch = "wasm32")]
            {
                ui.separator();
                ui.add(short(crate::files::Files::save_mode().to_string()));
            }
            if let Some(n) = &self.notice {
                ui.separator();
                ui.add(short(n.clone()));
            }
            if let Some(hint) = self.tool_hint() {
                ui.separator();
                ui.add(short(hint));
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

    /// The zoom control at the right of the status bar: − 95% +. Clicking
    /// the percentage zooms to fit.
    /// In a right-to-left layout, so the + comes first.
    fn zoom_control(&mut self, ui: &mut egui::Ui) {
        if ui
            .small_button("+")
            .on_hover_text("Zoom in (Ctrl+Wheel)")
            .clicked()
        {
            self.zoom_step(1);
        }
        let percent = format!("{:.0}%", self.view.zoom() * 100.0);
        let fit = egui::Button::new(percent)
            .frame(false)
            .min_size(egui::vec2(40.0, 0.0));
        if ui.add(fit).on_hover_text("Zoom to fit").clicked() {
            self.fit_pending = true;
        }
        if ui
            .small_button("−")
            .on_hover_text("Zoom out (Ctrl+Wheel)")
            .clicked()
        {
            self.zoom_step(-1);
        }
        ui.separator();
    }

    /// Zoom In (+1) or Zoom Out (−1) from a button or menu: two wheel
    /// notches about the centre of the canvas.
    /// Annotate Species from Names (docs/chebi-lookup-design.md): the
    /// species to look up, from a right-click on `only` or the selection,
    /// searched while the Annotate tab shows the progress.
    pub(crate) fn start_annotating(&mut self, only: Option<&str>) {
        if self.annotate.is_some() {
            return;
        }
        let queries = self.view.species_to_annotate(only);
        if queries.is_empty() {
            self.notice = Some("Each species already has a ChEBI reference".into());
            return;
        }
        self.annotate = Some(crate::annotate_tab::AnnotateTab::new(queries));
        self.right_tab = RightTab::Annotate;
    }

    /// Annotate Reactions from Names (docs/reaction-lookup-design.md): as
    /// [`Self::start_annotating`], for reactions.
    pub(crate) fn start_annotating_reactions(&mut self, only: Option<&str>) {
        if self.annotate.is_some() {
            return;
        }
        let queries = self.view.reactions_to_annotate(only);
        if queries.is_empty() {
            self.notice = Some("Each reaction already has an EC number or a Rhea reference".into());
            return;
        }
        self.annotate = Some(crate::annotate_tab::AnnotateTab::reactions(queries));
        self.right_tab = RightTab::Annotate;
    }

    fn finish_annotating(&mut self, action: crate::annotate_tab::TabAction) {
        if let crate::annotate_tab::TabAction::Show(id) = action {
            // Selected and brought into view; the review stays.
            let m = self.view.model();
            let found = match m.find_species(&id) {
                Some(s) => Some((pd_core::view::Target::Species(id), s.center)),
                None => m.find_reaction(&id).map(|r| {
                    let at = pd_core::render::draw::effective_junction(m, r);
                    (pd_core::view::Target::Leg(id), at)
                }),
            };
            if let Some((target, at)) = found {
                self.view.select_item(&target, false);
                self.view.centre_on(at, self.canvas_centre);
            }
            return;
        }
        match action {
            crate::annotate_tab::TabAction::Apply(items) => {
                let n = items.len();
                self.view.annotate_species(&items);
                self.notice = Some(format!("Annotated {n} species"));
            }
            crate::annotate_tab::TabAction::ApplyReactions(items) => {
                let n = items.len();
                self.view.annotate_reactions(&items);
                let s = if n == 1 { "" } else { "s" };
                self.notice = Some(format!("Annotated {n} reaction{s}"));
            }
            _ => {}
        }
        self.annotate = None;
        self.right_tab = RightTab::Properties;
    }

    fn zoom_step(&mut self, steps: i32) {
        self.view
            .zoom_at(ZOOM_STEP.powi(2 * steps), self.canvas_centre);
    }

    /// What the current tool expects next.
    fn tool_hint(&self) -> Option<String> {
        if let Some((_, role)) = self.view.new_line() {
            return Some(if role == pd_core::model::Role::Reactant {
                "Drop on a species to add it as a reactant · Esc to cancel".to_owned()
            } else {
                "Drop on a species to add it as a product (Shift: a reactant) · Esc to cancel"
                    .to_owned()
            });
        }
        if let Some((id, role)) = self.view.adding_to() {
            let side = if role == pd_core::model::Role::Reactant {
                "reactant"
            } else {
                "product"
            };
            return Some(format!(
                "Click a species to add it to {id} as a {side} · Esc to stop"
            ));
        }
        match self.view.tool() {
            Tool::Select => None,
            Tool::AddSpecies => Some("Click to add a species · Esc to stop".to_owned()),
            Tool::AddText => Some("Click where the text should go · Esc to stop".to_owned()),
            Tool::AddCompartment => {
                Some("Drag out the compartment's box (a click makes a 300 × 200 box)".to_owned())
            }
            Tool::ANY_REACTION => {
                let (r, p) = self.view.picked();
                // Short, so it doesn't run into the pointer's coordinates.
                Some(if self.view.picking_products() {
                    format!("Pick products ({p} so far) · Enter to finish · Esc to cancel")
                } else {
                    format!("Pick reactants ({r} so far) · Enter for products · Esc to cancel")
                })
            }
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

    /// The Edit Style window, if open; OK gives the style its look.
    fn style_editor_window(&mut self, ctx: &egui::Context) {
        use crate::styles_panel::EditorOutcome;
        let Some(editor) = &mut self.style_editor else {
            return;
        };
        match editor.show(ctx) {
            None => {}
            Some(EditorOutcome::Cancel) => self.style_editor = None,
            Some(EditorOutcome::Ok(kind, name, look)) => {
                match self.view.edit_style(kind, &name, look) {
                    Ok(_) => self.notice = Some(format!("Changed the style “{name}”")),
                    Err(e) => self.warnings = vec![format!("Can't change the style: {e}.")],
                }
                self.style_editor = None;
            }
        }
    }

    fn dialogs(&mut self, ctx: &egui::Context) {
        let Some(dialog) = &mut self.dialog else {
            return;
        };
        let mut close = false;
        let mut kegg_code = None;
        match dialog {
            Dialog::OpenKegg { text, error, focus } => {
                let mut submit = false;
                let modal = egui::Modal::new(egui::Id::unique("open-kegg")).show(ctx, |ui| {
                    ui.heading("Open from KEGG");
                    ui.label("A KEGG map code, such as eco00010 (E. coli glycolysis) or ko00010 (the reference map):");
                    ui.add_space(4.0);
                    let edit = ui.add(
                        egui::TextEdit::singleline(text)
                            .desired_width(240.0)
                            .hint_text("eco00010"),
                    );
                    if std::mem::take(focus) {
                        edit.request_focus();
                    }
                    if edit.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                        submit = true;
                    }
                    if cfg!(target_arch = "wasm32") {
                        ui.add_space(4.0);
                        ui.label(
                            egui::RichText::new(
                                "KEGG doesn't let web pages fetch its maps, so the map's file opens in a new tab: save it there (Ctrl+S), then open it with File ▸ Open.",
                            )
                            .small(),
                        );
                    }
                    if let Some(e) = error {
                        ui.colored_label(egui::Color32::from_rgb(0xCC, 0x33, 0x00), e.as_str());
                    }
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        submit |= ui.button("Open").clicked();
                        close |= ui.button("Cancel").clicked();
                    });
                });
                close |= modal.should_close();
                if submit {
                    match pd_core::kegg::map_code(text) {
                        Ok(code) => {
                            close = true;
                            kegg_code = Some(code);
                        }
                        Err(e) => *error = Some(e),
                    }
                }
            }
            Dialog::Rename {
                id,
                text,
                error,
                focus,
            } => {
                let mut submit = false;
                let modal = egui::Modal::new(egui::Id::unique("rename")).show(ctx, |ui| {
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
            Dialog::RenameStyle {
                kind,
                old,
                text,
                error,
                focus,
            } => {
                let mut submit = false;
                let modal = egui::Modal::new(egui::Id::unique("rename-style")).show(ctx, |ui| {
                    ui.heading("Rename style");
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
                    match self.view.rename_style(*kind, old, text) {
                        Ok(_) => close = true,
                        Err(e) => *error = Some(format!("Can't rename: {e}.")),
                    }
                }
            }
            Dialog::SaveStyle {
                target,
                text,
                error,
                focus,
            } => {
                let mut submit = false;
                let modal = egui::Modal::new(egui::Id::unique("save-style")).show(ctx, |ui| {
                    ui.heading("Save style");
                    ui.label("A name for this look:");
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
                    match self.view.save_style(target, text) {
                        Ok(_) => close = true,
                        Err(e) => *error = Some(format!("Can't save: {e}.")),
                    }
                }
            }
            Dialog::Preferences(draft) => {
                let mut ok = false;
                let catalytic_changed = draft.convert_catalytic != self.prefs.convert_catalytic;
                let will_reimport = catalytic_changed && self.antimony.last_import().is_some();
                let modal = egui::Modal::new(egui::Id::unique("preferences")).show(ctx, |ui| {
                    ui.heading("Preferences");
                    ui.add_space(4.0);
                    ui.checkbox(&mut draft.convert_catalytic, "Convert catalytic reactions")
                        .on_hover_text(
                            "On Antimony import, A -> A + B becomes -> B with A as a modifier",
                        );
                    ui.checkbox(&mut draft.show_null_species, "Show null species")
                        .on_hover_text("Draw the null nodes of reactions with an empty side");
                    ui.horizontal(|ui| {
                        let [r, g, b, a] = draft.selected_reaction.0;
                        let mut colour = egui::Color32::from_rgba_unmultiplied(r, g, b, a);
                        ui.label("Selected reaction colour");
                        if ui
                            .color_edit_button_srgba(&mut colour)
                            .on_hover_text("The colour a selected reaction is drawn in")
                            .changed()
                        {
                            draft.selected_reaction =
                                pd_core::model::Rgba(colour.to_srgba_unmultiplied());
                        }
                        if ui.small_button("Reset").clicked() {
                            draft.selected_reaction = pd_core::render::constants::SELECTED_REACTION;
                        }
                    });
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
                    self.view.selected_reaction_colour = draft.selected_reaction;
                    if will_reimport {
                        let opts = self.import_options();
                        if self.antimony.reimport(&mut self.view, &opts) {
                            self.fit_pending = true;
                        }
                    }
                }
            }
            Dialog::ConfirmDiscard(action) => {
                let (mut save, mut discard) = (false, false);
                let name = self.doc.name.clone();
                let modal = egui::Modal::new(egui::Id::unique("confirm-discard")).show(ctx, |ui| {
                    ui.heading(format!("Save changes to {name}?"));
                    ui.label("Your changes will be lost if you don't save them.");
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        save = ui.button("Save").clicked();
                        discard = ui.button("Don't Save").clicked();
                        close |= ui.button("Cancel").clicked();
                    });
                });
                close |= modal.should_close() || save || discard;
                let action = action.clone();
                if save {
                    self.after_save = Some(action);
                    self.save(false);
                } else if discard {
                    self.perform(action);
                }
            }
            Dialog::Recover(a) => {
                let (mut recover, mut discard) = (false, false);
                let (name, age) = (a.name.clone(), a.age(autosave::now()));
                let modal = egui::Modal::new(egui::Id::unique("recover")).show(ctx, |ui| {
                    ui.heading("Recover unsaved changes?");
                    ui.label(format!(
                        "{name} had unsaved changes when PathwayDesigner last closed ({age})."
                    ));
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        recover = ui.button("Recover").clicked();
                        discard = ui.button("Discard").clicked();
                    });
                });
                let a = a.clone();
                close |= modal.should_close() || recover || discard;
                if recover {
                    self.recover(&a);
                }
            }
            Dialog::Export(kind, settings) => {
                let kind = *kind;
                let (done, go) = crate::export::dialog(ctx, kind, settings, self.view.model());
                let settings = *settings;
                close |= done;
                if go {
                    self.export(kind, settings);
                }
            }
            Dialog::About => {
                let modal = egui::Modal::new(egui::Id::unique("about")).show(ctx, |ui| {
                    ui.heading("PathwayDesigner");
                    ui.label(format!(
                        "Version {} · built {}",
                        env!("CARGO_PKG_VERSION"),
                        env!("PD_BUILD_DATE")
                    ));
                    ui.label("A drawing tool for biochemical reaction networks.");
                    ui.hyperlink_to("Source code and documentation", HOME_PAGE);
                    ui.add(
                        egui::Hyperlink::from_label_and_url(
                            "Third-party licences",
                            THIRD_PARTY_PAGE,
                        )
                        .open_in_new_tab(true),
                    );
                    ui.add_space(4.0);
                    ui.label(
                        egui::RichText::new(
                            "MIT licence. Text uses Noto Sans and Noto Serif, under the SIL Open Font Licence.",
                        )
                        .small(),
                    );
                    ui.add_space(4.0);
                    close |= ui.button("Close").clicked();
                });
                close |= modal.should_close();
            }
        }
        if close {
            self.dialog = None;
        }
        if let Some(code) = kegg_code {
            self.open_from_kegg(code);
        }
    }
}

impl PathwayApp {
    /// How Antimony is imported, from the preferences.
    pub(crate) fn import_options(&self) -> ImportOptions {
        ImportOptions {
            convert_catalytic: self.prefs.convert_catalytic,
            seed: clock_seed(),
        }
    }
}

impl eframe::App for PathwayApp {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        // These View toggles change the view directly.
        self.prefs.show_reversible = self.view.show_reversible;
        self.prefs.show_grid = self.view.show_grid;
        self.prefs.snap_to_grid = self.view.snap_to_grid;
        self.prefs.save(storage);
        crate::my_styles::save(self.view.my_styles(), storage);
        self.autosave(storage);
        #[cfg(not(target_arch = "wasm32"))]
        self.recent.save(storage);
    }

    fn auto_save_interval(&self) -> std::time::Duration {
        autosave::INTERVAL
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        #[cfg(target_os = "macos")]
        self.mac_menu_commands();
        self.handle_files();
        self.poll_naming(&ctx);
        if let Some(t) = &mut self.annotate {
            t.poll(&ctx, self.gallery.fetch);
        }
        self.check_close();
        self.update_title();
        self.shortcuts(&ctx);
        self.run_layout(&ctx);
        let busy = self.view.is_laying_out();
        egui::Panel::top("menu").show(ui, |ui| self.menu_bar(ui));
        egui::Panel::top("toolbar").show(ui, |ui| self.toolbar(ui));
        egui::Panel::bottom("status").show(ui, |ui| self.status_bar(ui));
        // The stoichiometry matrix, under the canvas and the side panels
        // (docs/structural-analysis-design.md §6).
        let structure_open = self.right_tab == RightTab::Structure || self.structure.show_matrix;
        if structure_open {
            self.structure.refresh(self.view.model());
        }
        // The pick is drawn over the network while the tab or the matrix is
        // open (docs/structural-analysis-design.md §6).
        let highlight = structure_open.then(|| self.structure.highlight()).flatten();
        if self.view.set_highlight(highlight) {
            ui.ctx().request_repaint();
        }
        if self.structure.show_matrix {
            egui::Panel::bottom("matrix")
                .resizable(true)
                .default_size(220.0)
                .show(ui, |ui| self.structure.matrix(ui));
        }
        egui::Panel::left("tools").resizable(false).show(ui, |ui| {
            ui.add_enabled_ui(!busy, |ui| self.tool_palette(ui))
        });
        let opts = self.import_options();
        let mut antimony_action = None;
        let mut styles_action = None;
        let mut annotate_action = None;
        let mut structure_export = None;
        // Outside the inspector, at the window's right edge.
        if self.help.open {
            egui::Panel::right("help")
                .resizable(true)
                .default_size(340.0)
                .show(ui, |ui| self.help.show(ui));
        }
        egui::Panel::right("inspector")
            .resizable(true)
            .default_size(280.0)
            .show(ui, |ui| {
                // The tabs wrap onto a second row rather than hold the panel
                // wider than it is set (with Annotate there are five).
                ui.horizontal_wrapped(|ui| {
                    for (tab, name) in [
                        (RightTab::Properties, "Properties"),
                        (RightTab::Antimony, "Antimony"),
                        (RightTab::Styles, "Styles"),
                        (RightTab::Structure, "Structure"),
                    ] {
                        ui.selectable_value(&mut self.right_tab, tab, name);
                    }
                    if self.annotate.is_some() {
                        ui.selectable_value(&mut self.right_tab, RightTab::Annotate, "Annotate");
                    }
                });
                ui.separator();
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.add_enabled_ui(!busy, |ui| match self.right_tab {
                        RightTab::Properties => {
                            self.inspector.fetch = Some(self.gallery.fetch);
                            self.inspector.show(ui, &mut self.view)
                        }
                        RightTab::Antimony => {
                            antimony_action = self.antimony.show(ui, &mut self.view, &opts);
                        }
                        RightTab::Styles => {
                            styles_action = crate::styles_panel::show(ui, &mut self.view);
                        }
                        RightTab::Annotate => {
                            annotate_action = self.annotate.as_mut().and_then(|t| t.show(ui));
                        }
                        RightTab::Structure => {
                            let stem = self
                                .doc
                                .name
                                .rsplit_once('.')
                                .map_or(self.doc.name.as_str(), |(a, _)| a)
                                .to_owned();
                            structure_export = self.structure.show(ui, &stem);
                        }
                    });
                });
            });
        if let Some(e) = structure_export {
            self.export_csv(e.name, e.csv);
        }
        if let Some(a) = annotate_action {
            self.finish_annotating(a);
        }
        match styles_action {
            Some(crate::styles_panel::StylesAction::Rename(kind, old)) => {
                self.dialog = Some(Dialog::RenameStyle {
                    kind,
                    text: old.clone(),
                    old,
                    error: None,
                    focus: true,
                });
            }
            Some(crate::styles_panel::StylesAction::Notice(text)) => self.notice = Some(text),
            Some(crate::styles_panel::StylesAction::Export) => self.export_styles(),
            Some(crate::styles_panel::StylesAction::Edit(kind, name)) => {
                let look = self
                    .view
                    .style_entries(kind)
                    .into_iter()
                    .find(|e| e.name == name)
                    .map(|e| e.look);
                if let Some(look) = look {
                    self.style_editor =
                        Some(crate::styles_panel::StyleEditor::new(kind, name, look));
                }
            }
            Some(crate::styles_panel::StylesAction::Import) => {
                self.files.open(
                    &self.ctx,
                    crate::files::Purpose::ImportStyles,
                    crate::files::Filter::Styles,
                );
            }
            None => {}
        }
        match antimony_action {
            Some(PanelAction::Imported) => {
                self.warnings.clear();
                self.notice = Some("Imported Antimony".to_owned());
                self.fit_pending = true;
            }
            Some(PanelAction::LayOut) => self.start_layout(),
            None => {}
        }
        let mut output = None;
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ui, |ui| {
                self.fit_to_canvas(ui);
                output = Some(canvas::show(ui, &mut self.view, &mut self.canvas));
            });
        if let Some(out) = output {
            self.canvas_centre = out.centre;
            match &out.view.request {
                Some(Request::Rename(id)) => self.ask_rename(id),
                Some(Request::EditText(id)) => {
                    self.inspector.edit_text(id);
                    ctx.request_repaint();
                }
                _ => {}
            }
            if let Some(action) = out.action {
                self.act(action, out.centre);
            }
        }
        self.note_version_change();
        self.dialogs(&ctx);
        self.style_editor_window(&ctx);
        crate::keys_help::show(&ctx, &mut self.keys_help);
        if let Some(example) = self.gallery.show(&ctx) {
            self.request(Action::OpenGallery(example));
        }
        // Last, so nothing over the canvas sets another.
        if self.is_opening() || self.gallery.busy() || self.kegg_fetch.is_some() {
            ctx.set_cursor_icon(egui::CursorIcon::Wait);
        }
        links_in_new_tabs(&ctx);
    }
}

/// Every link clicked this frame opens in a new tab: in the browser, one in
/// the same tab would replace PathwayDesigner (an annotation's reference, a
/// help page's link to the Antimony docs). On the desktop it makes no
/// difference.
fn links_in_new_tabs(ctx: &egui::Context) {
    ctx.output_mut(|o| {
        for c in &mut o.commands {
            if let egui::OutputCommand::OpenUrl(url) = c {
                url.new_tab = true;
            }
        }
    });
}

/// What the status bar says after Place Modifiers by Their Reactions.
fn placed_notice(placed: pd_core::view::Placed) -> String {
    let lines = match placed.lines {
        1 => "1 modifier line".to_owned(),
        n => format!("{n} modifier lines"),
    };
    match placed.removed {
        0 => format!("Placed {lines} beside their reactions"),
        1 => format!("Placed {lines} beside their reactions; removed 1 drawing joined to nothing"),
        n => {
            format!("Placed {lines} beside their reactions; removed {n} drawings joined to nothing")
        }
    }
}

#[cfg(test)]
mod tests {
    //! The whole app driven through egui with made-up input: the wiring
    //! between panels, canvas and view. Browser behaviour is out of reach
    //! here (the Select All drag bug lived there; see index.html).

    use super::*;
    use eframe::App as _;
    use eframe::egui::{Event, PointerButton, Pos2, RawInput, Rect, Vec2};

    struct Harness {
        app: PathwayApp,
        frame: eframe::Frame,
        time: f64,
        /// The last accessibility tree, to find widgets by their label.
        access: Option<egui::accesskit::TreeUpdate>,
        /// The window's size.
        size: Vec2,
        /// The pointer's cursor after the last frame.
        cursor: egui::CursorIcon,
        /// The links the last frame opened.
        opened: Vec<egui::OpenUrl>,
    }

    impl Harness {
        fn new() -> Self {
            let cc = eframe::CreationContext::_new_kittest(egui::Context::default());
            cc.egui_ctx.enable_accesskit();
            let mut h = Harness {
                app: PathwayApp::new(&cc),
                frame: eframe::Frame::_new_kittest(),
                time: 0.0,
                access: None,
                size: egui::vec2(1400.0, 900.0),
                cursor: egui::CursorIcon::Default,
                opened: Vec::new(),
            };
            h.run(vec![]);
            h
        }

        /// One frame with these events.
        fn run(&mut self, events: Vec<Event>) {
            self.time += 0.05;
            let input = RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, self.size)),
                time: Some(self.time),
                events,
                ..Default::default()
            };
            let ctx = self.app.ctx.clone();
            let (app, frame) = (&mut self.app, &mut self.frame);
            let mut out = ctx.run_ui(input, |ui| app.ui(ui, frame));
            out.textures_delta.clear();
            self.access = out.platform_output.accesskit_update.take();
            self.cursor = out.platform_output.cursor_icon;
            self.opened = out
                .platform_output
                .commands
                .iter()
                .filter_map(|c| match c {
                    egui::OutputCommand::OpenUrl(u) => Some(u.clone()),
                    _ => None,
                })
                .collect();
        }

        /// The centre of the widget labelled `label`, from the accessibility
        /// tree.
        fn widget(&self, label: &str) -> Pos2 {
            let tree = self.access.as_ref().expect("accesskit is on");
            let node = tree
                .nodes
                .iter()
                .map(|(_, n)| n)
                .find(|n| n.label() == Some(label))
                .unwrap_or_else(|| panic!("no widget labelled {label:?}"));
            let b = node.bounds().expect("laid out");
            Pos2::new(((b.x0 + b.x1) / 2.0) as f32, ((b.y0 + b.y1) / 2.0) as f32)
        }

        /// The centre of the text `text` (a label's words are its value).
        fn text(&self, text: &str) -> Pos2 {
            let tree = self.access.as_ref().expect("accesskit is on");
            let node = tree
                .nodes
                .iter()
                .map(|(_, n)| n)
                .find(|n| n.value() == Some(text))
                .unwrap_or_else(|| panic!("no text {text:?}"));
            let b = node.bounds().expect("laid out");
            Pos2::new(((b.x0 + b.x1) / 2.0) as f32, ((b.y0 + b.y1) / 2.0) as f32)
        }

        fn pointer(&mut self, pos: Pos2, pressed: Option<bool>) {
            self.run(vec![match pressed {
                None => Event::PointerMoved(pos),
                Some(pressed) => Event::PointerButton {
                    pos,
                    button: PointerButton::Primary,
                    pressed,
                    modifiers: Modifiers::NONE,
                },
            }]);
        }

        fn click(&mut self, pos: Pos2) {
            self.pointer(pos, None);
            self.pointer(pos, Some(true));
            self.pointer(pos, Some(false));
        }

        fn key(&mut self, key: Key, modifiers: Modifiers) {
            for pressed in [true, false] {
                self.run(vec![Event::Key {
                    key,
                    physical_key: None,
                    pressed,
                    repeat: false,
                    modifiers,
                }]);
            }
        }

        fn centre(&self, id: &str) -> Pt {
            self.app.view.model().find_species(id).unwrap().center
        }

        fn on_screen(&self, id: &str) -> Pos2 {
            let p = self.app.view.world_to_screen(self.centre(id));
            Pos2::new(p.x as f32, p.y as f32)
        }
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn help_shows_the_mouse_and_keyboard() {
        let mut h = Harness::new();
        assert!(!h.app.keys_help);
        h.click(h.widget("Help"));
        h.run(vec![]);
        h.click(h.widget("Mouse and Keyboard"));
        assert!(h.app.keys_help);
        h.run(vec![]);
        // The window lists the shortcuts.
        h.text("Copy Style, Paste Style");
        h.text("A+… → B+…");
        // Its close button shuts it.
        h.click(h.widget("Close window"));
        h.run(vec![]);
        assert!(!h.app.keys_help);
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn help_opens_a_panel_with_pages() {
        let mut h = Harness::new();
        assert!(!h.app.help.open);
        h.click(h.widget("Help"));
        h.run(vec![]);
        h.click(h.widget("PathwayDesigner Help"));
        assert!(h.app.help.open);
        h.run(vec![]);
        assert_eq!(h.app.help.title(), "Contents");
        // A link opens its page, and Back returns.
        h.click(h.widget("Getting started"));
        h.run(vec![]);
        assert_eq!(h.app.help.title(), "Getting started");
        h.text("Write a model in Antimony");
        h.click(h.widget("Back"));
        h.run(vec![]);
        assert_eq!(h.app.help.title(), "Contents");
        // The close button shuts the panel.
        h.click(h.widget("✖"));
        h.run(vec![]);
        assert!(!h.app.help.open);
    }

    /// The Gallery fetched from this checkout instead of GitHub.
    fn local(_: &egui::Context, url: String) -> crate::gallery::Pending {
        let path = url
            .strip_prefix(crate::gallery::BASE_URL)
            .unwrap()
            .replace("%20", " ");
        let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../../");
        let text = std::fs::read_to_string(format!("{root}{path}")).map_err(|e| e.to_string());
        std::sync::Arc::new(std::sync::Mutex::new(Some(text)))
    }

    fn offline(_: &egui::Context, _: String) -> crate::gallery::Pending {
        std::sync::Arc::new(std::sync::Mutex::new(Some(Err("no network".into()))))
    }

    /// PubChem, for the two compounds of the KEGG map in the tests.
    fn pubchem(_: &egui::Context, url: String) -> crate::gallery::Pending {
        let text = if url.contains("/synonyms/") {
            r#"{"InformationList": {"Information": [
                {"SID": 3333, "Synonym": ["D-glucose", "C00031"]},
                {"SID": 3403, "Synonym": ["cori ester", "C00103"]}]}}"#
        } else if url.contains("/cids/") {
            r#"{"InformationList": {"Information": [
                {"SID": 3333, "CID": [5793]}, {"SID": 3403, "CID": [65533]}]}}"#
        } else {
            r#"{"PropertyTable": {"Properties": [
                {"CID": 5793, "Title": "D-Glucose"},
                {"CID": 65533, "Title": "alpha-D-glucose 1-phosphate"}]}}"#
        };
        std::sync::Arc::new(std::sync::Mutex::new(Some(Ok(text.into()))))
    }

    fn garbage(_: &egui::Context, _: String) -> crate::gallery::Pending {
        std::sync::Arc::new(std::sync::Mutex::new(Some(Ok("{ not a model".into()))))
    }

    /// A download that never finishes.
    fn waiting(_: &egui::Context, _: String) -> crate::gallery::Pending {
        Default::default()
    }

    /// The Open button on the row of the example titled `title`: where it
    /// is, and whether it is enabled.
    fn gallery_open(h: &Harness, title: &str) -> (Pos2, bool) {
        let title = h.text(title);
        let tree = h.access.as_ref().unwrap();
        let n = tree
            .nodes
            .iter()
            .map(|(_, n)| n)
            .filter(|n| n.label() == Some("Open"))
            .find(|n| {
                n.bounds()
                    .is_some_and(|b| b.y0 <= title.y as f64 && title.y as f64 <= b.y1 + 4.0)
            })
            .expect("an Open on its row");
        let b = n.bounds().unwrap();
        let at = Pos2::new(((b.x0 + b.x1) / 2.0) as f32, ((b.y0 + b.y1) / 2.0) as f32);
        (at, !n.is_disabled())
    }

    fn example(file: &str) -> crate::gallery::Example {
        let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../../");
        let text = std::fs::read_to_string(format!("{root}{}", crate::gallery::INDEX)).unwrap();
        crate::gallery::Index::parse(&text)
            .unwrap()
            .examples
            .into_iter()
            .find(|e| e.file == file)
            .unwrap()
    }

    #[test]
    fn opening_from_the_gallery() {
        let mut h = Harness::new();
        h.app.gallery.fetch = local;
        h.click(h.widget("File"));
        h.run(vec![]);
        h.click(h.widget("Open from Gallery…"));
        assert!(h.app.gallery.open);
        h.run(vec![]);
        h.run(vec![]);
        // The examples, under their groups.
        h.text("Large models");
        h.text("As in the README");
        h.text("E. coli central metabolism (Millard 2016)");
        let (open, _) = gallery_open(&h, "Upper glycolysis");
        h.click(open);
        h.run(vec![]);
        assert_eq!(h.app.doc.name, "upper_glycolysis.json");
        assert!(h.app.doc.target.is_none(), "new and unsaved");
        assert_eq!(h.app.view.model().reactions().len(), 4);
        assert!(!h.app.gallery.open, "the window closes once it has opened");
        // An Escher map goes through the import.
        h.app.perform(Action::OpenGallery(example(
            "Escher/e_coli_core.Core metabolism.json",
        )));
        h.run(vec![]);
        assert_eq!(
            h.app.doc.name,
            "e_coli_core.Core metabolism (from Escher).json"
        );
        assert_eq!(h.app.view.model().reactions().len(), 95);
    }

    #[test]
    fn the_gallery_asks_before_replacing_unsaved_changes() {
        let mut h = Harness::new();
        h.app.gallery.fetch = local;
        h.app.view.select_all();
        h.app.view.delete_selection();
        assert!(h.app.view.is_modified());
        h.app.gallery.open = true;
        // Twice: a new window takes a frame to find its size and place.
        h.run(vec![]);
        h.run(vec![]);
        let (open, _) = gallery_open(&h, "The TCA cycle");
        h.click(open);
        assert!(matches!(h.app.dialog, Some(Dialog::ConfirmDiscard(_))));
        h.run(vec![]);
        h.run(vec![]);
        assert_ne!(h.app.doc.name, "tca_cycle.json", "not without asking");
        h.click(h.widget("Don't Save"));
        h.run(vec![]);
        h.run(vec![]);
        assert_eq!(h.app.doc.name, "tca_cycle.json");
    }

    #[test]
    fn an_example_too_new_for_this_app_cant_be_opened() {
        fn index(_: &egui::Context, _: String) -> crate::gallery::Pending {
            let text = format!(
                r#"{{"examples": [
                    {{"file": "a.json", "group": "G", "title": "Old", "version": 3}},
                    {{"file": "b.json", "group": "G", "title": "New", "version": {}}}]}}"#,
                pd_core::json::FORMAT_VERSION + 1
            );
            std::sync::Arc::new(std::sync::Mutex::new(Some(Ok(text))))
        }
        let mut h = Harness::new();
        h.app.gallery.fetch = index;
        h.app.gallery.open = true;
        h.run(vec![]);
        h.run(vec![]);
        assert!(gallery_open(&h, "Old").1);
        assert!(!gallery_open(&h, "New").1);
        h.text("Needs a newer PathwayDesigner");
    }

    #[test]
    fn the_gallery_shows_the_busy_cursor_and_what_went_wrong() {
        let mut h = Harness::new();
        // Fetching the index: the busy cursor while the window is open.
        h.app.gallery.fetch = waiting;
        h.app.gallery.open = true;
        h.run(vec![]);
        h.run(vec![]);
        h.text("Fetching the Gallery…");
        assert_eq!(h.cursor, egui::CursorIcon::Wait);
        h.app.gallery.open = false;
        h.run(vec![]);
        assert_ne!(h.cursor, egui::CursorIcon::Wait);
        // Fetching a file.
        let before = h.app.view.model().clone();
        h.app
            .perform(Action::OpenGallery(example("ExampleModels/circle_1.json")));
        h.run(vec![]);
        assert_eq!(h.cursor, egui::CursorIcon::Wait);
        assert_eq!(h.app.view.model(), &before);
        // No network: the window says so, and the diagram stays.
        let mut h = Harness::new();
        let before = h.app.view.model().clone();
        h.app.gallery.fetch = offline;
        h.app.gallery.open = true;
        h.run(vec![]);
        h.run(vec![]);
        h.text("Couldn't reach the Gallery: no network");
        h.app
            .perform(Action::OpenGallery(example("ExampleModels/circle_1.json")));
        h.run(vec![]);
        h.run(vec![]);
        h.text("Couldn't open Circle 1: no network");
        assert_eq!(h.app.view.model(), &before);
        assert_ne!(h.cursor, egui::CursorIcon::Wait);
        // A file that arrives but can't be read.
        let mut h = Harness::new();
        let before = h.app.view.model().clone();
        let mut bad = example("ExampleModels/circle_1.json");
        bad.title = "Bad".into();
        h.app.gallery.fetch = garbage;
        h.app.perform(Action::OpenGallery(bad));
        h.run(vec![]);
        h.run(vec![]);
        assert!(h.app.gallery.open, "opened to say why");
        assert_eq!(h.app.view.model(), &before);
        h.run(vec![]);
        let tree = h.access.as_ref().unwrap();
        assert!(
            tree.nodes.iter().any(|(_, n)| n
                .value()
                .is_some_and(|v| v.starts_with("Couldn't open Bad:"))),
            "the window says why"
        );
    }

    #[test]
    fn the_structure_tab_and_the_matrix_panel() {
        let mut h = Harness::new();
        h.app.perform(Action::OpenText {
            name: "mapk.json".into(),
            text: include_str!("../../../ExampleModels/mapk_cascade.json").into(),
            target: None,
        });
        h.run(vec![]);
        // The tab: 8 species, 10 reactions, rank 5, three conservation laws.
        h.click(h.widget("Structure"));
        h.run(vec![]);
        assert_eq!(h.app.right_tab, RightTab::Structure);
        let s = h.app.structure.structure().expect("analysed");
        assert_eq!((s.species.len(), s.reactions.len(), s.rank), (8, 10, 5));
        assert_eq!(s.conservation_laws().len(), 3);
        let law = s.law_text(&s.conservation_laws()[0]);
        // The matrix panel opens from the tab and closes from its own button.
        assert!(!h.app.structure.show_matrix);
        h.click(h.widget("Matrices"));
        h.run(vec![]);
        assert!(h.app.structure.show_matrix);
        // Its tabs: K and L, then N again.
        use crate::structure_panel::MatrixTab;
        for (label, tab) in [
            ("K", MatrixTab::K),
            ("L", MatrixTab::L),
            ("N", MatrixTab::N),
        ] {
            h.click(h.widget(label));
            h.run(vec![]);
            assert_eq!(h.app.structure.matrix_tab, tab);
        }
        h.click(h.widget("✕"));
        h.run(vec![]);
        assert!(!h.app.structure.show_matrix);
        // A law is picked by clicking it, and let go by clicking again.
        h.click(h.widget(&law));
        assert!(matches!(
            h.app.structure.picked,
            Some(crate::structure_panel::Picked::Law(_))
        ));
        // The law is drawn over the network: its species, none of the
        // reactions.
        h.run(vec![]);
        let shown = h.app.view.highlight().expect("highlighted").clone();
        assert!(shown.reactions.is_empty());
        assert_eq!(
            shown.species.len(),
            h.app.structure.bases().unwrap().laws[0].terms.len()
        );
        // Not while another tab is shown; again when the tab is back.
        h.app.right_tab = RightTab::Properties;
        h.run(vec![]);
        assert!(h.app.view.highlight().is_none());
        h.app.right_tab = RightTab::Structure;
        h.run(vec![]);
        assert!(h.app.view.highlight().is_some());
        h.click(h.widget(&law));
        assert_eq!(h.app.structure.picked, None);
        h.run(vec![]);
        assert!(h.app.view.highlight().is_none(), "let go");
        // The null-space list folds away, leaving the laws in view; and
        // opens again.
        let has = |h: &Harness, label: &str| {
            h.access
                .as_ref()
                .is_some_and(|t| t.nodes.iter().any(|(_, n)| n.label() == Some(label)))
        };
        let first = {
            let s = h.app.structure.structure().unwrap();
            s.flux_text(&h.app.structure.bases().unwrap().fluxes[0])
        };
        assert!(has(&h, &first));
        h.click(h.widget("Flux null space (5)"));
        // Past the folding animation.
        for _ in 0..10 {
            h.run(vec![]);
        }
        assert!(!has(&h, &first), "folded");
        assert!(has(&h, &law), "the laws still there");
        h.click(h.widget("Flux null space (5)"));
        for _ in 0..10 {
            h.run(vec![]);
        }
        assert!(has(&h, &first), "open again");
        // The sparse basis: the same number of vectors and laws, each no
        // longer than the factorization's; the K matrix again on the other
        // button.
        let k = h.app.structure.bases().unwrap().clone();
        h.click(h.widget("Sparse"));
        h.run(vec![]);
        assert!(h.app.structure.sparse);
        let sparse = h.app.structure.bases().unwrap().clone();
        assert_eq!(sparse.fluxes.len(), k.fluxes.len());
        assert_eq!(sparse.laws.len(), k.laws.len());
        let size = |b: &crate::structure_panel::Bases| {
            b.fluxes.iter().map(|v| v.terms.len()).sum::<usize>()
                + b.laws.iter().map(|l| l.terms.len()).sum::<usize>()
        };
        assert!(size(&sparse) <= size(&k));
        h.click(h.widget("K matrix"));
        h.run(vec![]);
        assert!(!h.app.structure.sparse);
        // An edit is followed: with the first reaction gone, the analysis is
        // of 9 reactions.
        let first = h.app.view.model().reactions()[0].id.clone();
        h.app
            .view
            .select_item(&pd_core::view::Target::Leg(first), false);
        h.app.view.delete_selection();
        h.run(vec![]);
        let s = h.app.structure.structure().expect("analysed");
        assert_eq!(s.reactions.len(), 9);
    }

    #[test]
    fn opening_an_escher_map() {
        let map = r#"[{"map_name": "m"}, {"nodes": {
            "1": {"node_type": "midmarker", "x": 100, "y": 0},
            "2": {"node_type": "metabolite", "bigg_id": "a_c", "x": 0, "y": 0},
            "3": {"node_type": "metabolite", "bigg_id": "b_c", "x": 200, "y": 0}},
          "reactions": {"9": {"bigg_id": "R1", "metabolites": [
            {"bigg_id": "a_c", "coefficient": -1}, {"bigg_id": "b_c", "coefficient": 1}],
            "segments": {"s": {"from_node_id": "2", "to_node_id": "1"},
                         "t": {"from_node_id": "1", "to_node_id": "3"}}}}}]"#;
        let mut h = Harness::new();
        h.app.perform(Action::OpenText {
            name: "core.json".into(),
            text: map.into(),
            target: None,
        });
        h.run(vec![]);
        h.run(vec![]);
        // A new document, never saved over the map.
        assert_eq!(h.app.doc.name, "core (from Escher).json");
        assert!(h.app.doc.target.is_none());
        assert!(h.app.view.is_modified());
        assert!(!h.app.view.can_undo());
        assert_eq!(h.app.view.model().reactions().len(), 1);
        // Opening isn't an edit: the notice says what was opened.
        assert_eq!(
            h.app.notice.as_deref(),
            Some("Opened the Escher map core.json")
        );
        // Not a map: the diagram stays, and a warning says why.
        h.app.perform(Action::OpenText {
            name: "bad.json".into(),
            text: "[1]".into(),
            target: None,
        });
        h.run(vec![]);
        assert_eq!(h.app.view.model().reactions().len(), 1);
        assert!(h.app.warnings[0].contains("couldn't be read as an Escher map"));
    }

    #[test]
    fn mark_and_unmark_as_modifier_from_a_reactions_menu() {
        let mut h = Harness::new();
        let opts = pd_core::antimony::ImportOptions {
            convert_catalytic: false,
            seed: 1,
        };
        h.app
            .view
            .import_antimony(
                "J1: A -> B; k*A*B
k = 1
",
                &opts,
            )
            .unwrap();
        h.app.act(
            MenuAction::MarkModifier("J1".into(), "B".into(), true),
            pt(0.0, 0.0),
        );
        assert_eq!(h.app.view.marked_as_modifier("J1"), ["B"]);
        h.app.act(
            MenuAction::MarkModifier("J1".into(), "B".into(), false),
            pt(0.0, 0.0),
        );
        assert!(h.app.view.marked_as_modifier("J1").is_empty());
    }

    #[test]
    fn select_all_drawings_from_a_species_menu() {
        use pd_core::model::{Model, SpeciesNode};
        let mut m = Model::new();
        m.push_species(SpeciesNode::new("A", pt(0.0, 0.0))).unwrap();
        m.push_species(SpeciesNode::new("B", pt(300.0, 0.0)))
            .unwrap();
        let a2 = m.add_alias("A", pt(0.0, 200.0)).unwrap();
        let mut h = Harness::new();
        h.app.view.set_model(m);
        h.app
            .act(MenuAction::SelectAllDrawings(a2.clone()), pt(0.0, 0.0));
        let selected: Vec<String> = h
            .app
            .view
            .model()
            .species()
            .iter()
            .filter(|s| s.selected)
            .map(|s| s.id.clone())
            .collect();
        assert_eq!(selected, ["A".to_owned(), a2]);
    }

    #[test]
    fn a_loop_converts_to_corner_points_from_its_menu() {
        use pd_core::model::{Model, Participant, Reaction, SpeciesNode};
        let mut m = Model::new();
        for (id, x) in [("A", 0.0), ("B", 400.0)] {
            m.push_species(SpeciesNode::new(id, pt(x, 0.0))).unwrap();
        }
        let mut r = Reaction::new("R1", pt(200.0, 0.0));
        r.reactants.push(Participant::new("A", 1.0));
        r.products.push(Participant::new("B", 1.0));
        r.modifiers.push(Participant::new("A", 1.0));
        m.push_reaction(r).unwrap();
        let mut h = Harness::new();
        h.app.view.set_model(m);
        assert!(h.app.view.is_loop("R1", 0));
        h.app
            .act(MenuAction::LoopToCornerPoints("R1".into(), 0), pt(0.0, 0.0));
        assert!(!h.app.view.is_loop("R1", 0));
        let points = &h.app.view.model().find_reaction("R1").unwrap().modifiers[0].points;
        assert_eq!(points.len(), 2);
    }

    #[test]
    fn arrange_places_modifiers_by_their_reactions() {
        let mut h = Harness::new();
        let opts = pd_core::antimony::ImportOptions {
            convert_catalytic: false,
            seed: 1,
        };
        let text = "species E; E = 1
J1: A -> B; k*A*E
J2: C -> D; k*C*E
k = 1
/* diagram m v1
                    A at (0, 0)
B at (400, 0)
C at (0, 400)
D at (400, 400)
E at (2000, 0)
                    J1 at (200, 0)
J2 at (200, 400)
end */
";
        h.app.view.import_antimony(text, &opts).unwrap();
        h.run(vec![]);
        h.click(h.widget("Arrange"));
        h.run(vec![]);
        h.click(h.widget("Place Modifiers by Their Reactions"));
        h.run(vec![]);
        assert_eq!(
            h.app.notice.as_deref(),
            Some("Placed 2 modifier lines beside their reactions")
        );
        let m = h.app.view.model();
        assert!(
            m.find_species("E")
                .unwrap()
                .center
                .distance(pd_core::geometry::pt(200.0, 0.0))
                < 150.0
        );
        assert_eq!(m.aliases_of("E").count(), 1);
    }

    #[test]
    fn opening_a_kegg_map() {
        let map = r##"<?xml version="1.0"?>
<!DOCTYPE pathway SYSTEM "https://www.kegg.jp/kegg/xml/KGML_v0.7.2_.dtd">
<pathway name="path:eco00010" org="eco" number="00010" title="Glycolysis">
  <entry id="1" name="cpd:C00031" type="compound">
    <graphics name="C00031" type="circle" x="100" y="100" width="8" height="8"/>
  </entry>
  <entry id="2" name="cpd:C00103" type="compound">
    <graphics name="C00103" type="circle" x="300" y="100" width="8" height="8"/>
  </entry>
  <entry id="3" name="eco:b0001" type="gene" reaction="rn:R00001">
    <graphics name="glk" bgcolor="#BFFFBF" type="rectangle" x="200" y="90" width="46" height="17"/>
  </entry>
  <reaction id="3" name="rn:R00001" type="irreversible">
    <substrate id="1" name="cpd:C00031"/>
    <product id="2" name="cpd:C00103"/>
  </reaction>
</pathway>
"##;
        let mut h = Harness::new();
        h.app.gallery.fetch = pubchem;
        h.app.perform(Action::OpenText {
            name: "eco00010.xml".into(),
            text: map.into(),
            target: None,
        });
        h.run(vec![]);
        h.run(vec![]);
        // A new document, never saved over the map, named by the map's code.
        assert_eq!(h.app.doc.name, "eco00010 (from KEGG).json");
        assert!(h.app.doc.target.is_none());
        assert!(h.app.view.is_modified());
        assert!(!h.app.view.can_undo());
        assert_eq!(h.app.view.model().reactions().len(), 1);
        // KEGG's ids, labelled with names (docs/display-names-design.md D6).
        let glk = h
            .app
            .view
            .model()
            .find_species("b0001")
            .expect("the enzyme");
        assert_eq!(glk.label(), "glk");
        assert!(h.app.view.show_display_names());
        // The compounds take their names from PubChem, a request a frame.
        for _ in 0..6 {
            h.run(vec![]);
        }
        let m = h.app.view.model();
        let label = |id: &str| m.find_species(id).map(|s| s.label().to_owned());
        assert_eq!(label("C00031").as_deref(), Some("D-Glucose"), "named");
        // And referred to its PubChem compound (A7).
        assert!(
            m.find_species("C00031")
                .unwrap()
                .annotation
                .references
                .iter()
                .any(|r| r
                    .resource
                    .starts_with("https://identifiers.org/pubchem.compound:")),
            "{:?}",
            m.find_species("C00031").unwrap().annotation
        );
        assert_eq!(
            label("C00103").as_deref(),
            Some("alpha-D-glucose 1-phosphate")
        );
        assert!(h.app.naming.is_none(), "the lookup is over");
        assert_eq!(
            h.app.notice.as_deref(),
            Some("Named the 2 compounds from PubChem")
        );
        // Undo puts the numbers back.
        assert!(h.app.view.can_undo());
        // A map it can't draw: the diagram stays, and a warning says why.
        h.app.perform(Action::OpenText {
            name: "hsa04010.xml".into(),
            text: map
                .replace("<reaction ", "<x ")
                .replace("</reaction>", "</x>"),
            target: None,
        });
        h.run(vec![]);
        assert_eq!(h.app.view.model().reactions().len(), 1);
        assert!(
            h.app.warnings[0].starts_with("hsa04010.xml: This KEGG map has no reactions"),
            "{:?}",
            h.app.warnings
        );
    }

    /// KEGG for File ▸ Open from KEGG…: eco00010 is a one-reaction map, any
    /// other code isn't there; PubChem as [`pubchem`]. Built only where the
    /// test that uses it runs (not on macOS).
    #[cfg(not(target_os = "macos"))]
    fn kegg(ctx: &egui::Context, url: String) -> crate::gallery::Pending {
        if !url.contains("rest.kegg.jp") {
            return pubchem(ctx, url);
        }
        let result = if url.ends_with("/eco00010/kgml") {
            Ok(r#"<pathway name="path:eco00010" number="00010" title="Glycolysis">
  <entry id="1" name="cpd:C00031" type="compound"><graphics x="100" y="100"/></entry>
  <entry id="2" name="cpd:C00103" type="compound"><graphics x="300" y="100"/></entry>
  <reaction id="9" name="rn:R00001" type="irreversible"><substrate id="1"/><product id="2"/></reaction>
</pathway>"#
                .to_owned())
        } else {
            Err("not found (404)".to_owned())
        };
        std::sync::Arc::new(std::sync::Mutex::new(Some(result)))
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn open_from_kegg_fetches_a_map_by_its_code() {
        let mut h = Harness::new();
        h.app.gallery.fetch = kegg;
        h.click(h.widget("File"));
        h.run(vec![]);
        h.click(h.widget("Open from KEGG…"));
        h.run(vec![]);
        assert!(matches!(h.app.dialog, Some(Dialog::OpenKegg { .. })));
        let type_code = |h: &mut Harness, code: &str| {
            if let Some(Dialog::OpenKegg { text, .. }) = &mut h.app.dialog {
                *text = code.to_owned();
            }
            h.run(vec![]);
            h.click(h.widget("Open"));
            h.run(vec![]);
        };
        // Not a code: the dialog stays and says what one is.
        type_code(&mut h, "glycolysis");
        match &h.app.dialog {
            Some(Dialog::OpenKegg { error: Some(e), .. }) => {
                assert!(e.contains("five digits"), "{e}")
            }
            _ => panic!("the dialog should stay with an error"),
        }
        // A code, typed in capitals: fetched and opened as a new document.
        type_code(&mut h, "ECO00010");
        assert!(h.app.dialog.is_none());
        for _ in 0..8 {
            h.run(vec![]);
        }
        assert!(h.app.kegg_fetch.is_none());
        assert_eq!(h.app.doc.name, "eco00010 (from KEGG).json");
        assert_eq!(
            h.app.view.model().find_species("C00031").unwrap().label(),
            "D-Glucose",
            "named too"
        );
        // A map KEGG hasn't got: the diagram stays, and a warning says why.
        h.app.perform(Action::OpenKegg("eco99999".into()));
        h.run(vec![]);
        h.run(vec![]);
        assert_eq!(
            h.app.view.model().find_species("C00031").unwrap().label(),
            "D-Glucose"
        );
        assert_eq!(
            h.app.warnings,
            [
                "eco99999 couldn't be fetched from KEGG (not found (404)). The diagram wasn't changed."
            ]
        );
    }

    #[test]
    fn a_kegg_map_keeps_its_numbers_when_pubchem_cant_be_reached() {
        let map = r##"<pathway name="path:eco00010" number="00010" title="Glycolysis">
  <entry id="1" name="cpd:C00031" type="compound"><graphics x="100" y="100"/></entry>
  <entry id="2" name="cpd:C00103" type="compound"><graphics x="300" y="100"/></entry>
  <reaction id="9" name="rn:R00001" type="irreversible">
    <substrate id="1"/><product id="2"/>
  </reaction>
</pathway>"##;
        let mut h = Harness::new();
        h.app.gallery.fetch = offline;
        h.app.perform(Action::OpenText {
            name: "eco00010.xml".into(),
            text: map.into(),
            target: None,
        });
        for _ in 0..6 {
            h.run(vec![]);
        }
        assert!(h.app.view.model().find_species("C00031").is_some());
        assert_eq!(
            h.app.notice.as_deref(),
            Some("PubChem couldn't be reached, so the compounds keep their KEGG numbers")
        );
        // Opening another file stops a lookup still under way.
        h.app.gallery.fetch = waiting;
        h.app.perform(Action::OpenText {
            name: "eco00010.xml".into(),
            text: map.into(),
            target: None,
        });
        h.run(vec![]);
        h.run(vec![]);
        assert!(h.app.naming.is_some());
        h.app.perform(Action::OpenText {
            name: "other.ant".into(),
            text: "A -> B; k".into(),
            target: None,
        });
        h.run(vec![]);
        assert!(h.app.naming.is_none());
    }

    #[test]
    fn a_file_read_is_opened_a_frame_later_under_the_busy_cursor() {
        let mut h = Harness::new();
        let before = h.app.view.model().clone();
        let text = pd_core::json::to_json(&pd_core::model::Model::new());
        h.app.files.deliver(crate::files::FileEvent::Opened {
            purpose: crate::files::Purpose::Open,
            name: "next.json".into(),
            text,
            target: None,
        });
        // The frame it arrives: the busy cursor, and what is being opened.
        h.run(vec![]);
        assert!(h.app.is_opening());
        assert_eq!(h.app.notice.as_deref(), Some("Opening next.json…"));
        assert_eq!(h.app.view.model(), &before, "not opened yet");
        assert_eq!(h.cursor, egui::CursorIcon::Wait);
        // The next one opens it.
        h.run(vec![]);
        assert!(!h.app.is_opening());
        assert_eq!(h.app.doc.name, "next.json");
        assert_eq!(h.app.notice.as_deref(), Some("Opened next.json"));
        assert_ne!(h.cursor, egui::CursorIcon::Wait);
    }

    #[test]
    fn opening_sbml() {
        let mut h = Harness::new();
        let start = h.app.view.model().clone();
        let xml = h.app.view.export_sbml().xml;
        h.app.perform(Action::OpenText {
            name: "pathway.xml".into(),
            text: xml,
            target: None,
        });
        h.run(vec![]);
        // A new document, with the file's layout.
        assert_eq!(h.app.doc.name, "pathway.json");
        assert!(h.app.view.is_modified());
        assert!(!h.app.view.can_undo());
        assert!(!h.app.view.is_laying_out());
        for s in start.species() {
            let t = h.app.view.model().find_species(&s.id).unwrap();
            assert!(s.center.distance(t.center) < 1e-6, "{}", s.id);
        }

        // Without a layout, it is laid out at once.
        let core_only = r#"<sbml xmlns="http://www.sbml.org/sbml/level3/version1/core" level="3" version="1">
          <model><listOfCompartments><compartment id="c" constant="true"/></listOfCompartments>
          <listOfSpecies>
            <species id="A" compartment="c" hasOnlySubstanceUnits="false" boundaryCondition="false" constant="false"/>
            <species id="B" compartment="c" hasOnlySubstanceUnits="false" boundaryCondition="false" constant="false"/>
          </listOfSpecies>
          <listOfReactions><reaction id="J" reversible="false">
            <listOfReactants><speciesReference species="A" constant="true"/></listOfReactants>
            <listOfProducts><speciesReference species="B" constant="true"/></listOfProducts>
          </reaction></listOfReactions></model></sbml>"#;
        h.app.perform(Action::OpenText {
            name: "core.xml".into(),
            text: core_only.into(),
            target: None,
        });
        assert!(h.app.view.is_laying_out());

        // A file that isn't SBML leaves the diagram alone.
        let before = h.app.view.model().species().len();
        h.app.perform(Action::OpenText {
            name: "broken.xml".into(),
            text: "<sbml".into(),
            target: None,
        });
        assert_eq!(h.app.view.model().species().len(), before);
        assert!(h.app.warnings[0].contains("couldn't be read as SBML"));
    }

    #[test]
    fn arrow_keys_nudge_the_selection() {
        let mut h = Harness::new();
        h.app.perform(Action::New);
        h.run(vec![]);
        h.app.view.set_tool(Tool::AddSpecies);
        h.click(Pos2::new(400.0, 400.0));
        h.key(Key::Escape, Modifiers::NONE);
        // Long enough after the last click not to make a double-click, which
        // would open Rename.
        h.time += 1.0;
        h.click(h.on_screen("S1"));
        assert!(h.app.dialog.is_none());
        let start = h.centre("S1");
        h.key(Key::ArrowRight, Modifiers::NONE);
        h.key(Key::ArrowDown, Modifiers::SHIFT);
        assert_eq!(h.centre("S1") - start, pt(1.0, 10.0));
        // One undo step for the run.
        h.app.view.undo();
        assert_eq!(h.centre("S1"), start);
        // With a row of the model tree focused, the keys are the tree's.
        h.time += 1.0;
        h.click(h.widget("S1"));
        assert!(h.app.ctx.memory(|m| m.focused().is_some()));
        assert_eq!(h.app.view.selection().0, ["S1"]);
        let at = h.centre("S1");
        h.key(Key::ArrowRight, Modifiers::NONE);
        assert_eq!(h.centre("S1"), at);
    }

    #[test]
    fn the_reaction_menu_adds_and_removes_species() {
        use pd_core::model::Role;
        let mut h = Harness::new();
        h.app.perform(Action::New);
        h.run(vec![]);
        h.app.view.set_tool(Tool::AddSpecies);
        for x in [400.0, 550.0, 700.0] {
            h.time += 1.0;
            h.click(Pos2::new(x, 400.0));
        }
        h.app.view.set_tool(Tool::UNI_UNI);
        for id in ["S1", "S2"] {
            h.time += 1.0;
            h.click(h.on_screen(id));
        }
        h.app.view.set_tool(Tool::Select);
        let r = h.app.view.model().reactions()[0].id.clone();
        // Add Product…, then a click on S3.
        h.app.act(
            MenuAction::AddParticipant(r.clone(), Role::Product),
            pt(0.0, 0.0),
        );
        let hint = h.app.tool_hint().unwrap();
        assert!(
            hint.contains(&format!("add it to {r} as a product")),
            "{hint}"
        );
        h.time += 1.0;
        h.click(h.on_screen("S3"));
        let products = |h: &Harness| h.app.view.model().reactions()[0].products.len();
        assert_eq!(products(&h), 2);
        h.key(Key::Escape, Modifiers::NONE);
        assert_eq!(h.app.tool_hint(), None);
        // Remove from Reaction.
        h.app.act(
            MenuAction::RemoveParticipant(r.clone(), "S3".into()),
            pt(0.0, 0.0),
        );
        assert_eq!(products(&h), 1);
        // A new line from the + handle, with Shift: S3 joins as a reactant.
        h.app
            .view
            .select_item(&pd_core::view::Target::Leg(r.clone()), false);
        let plus = {
            let m = h.app.view.model();
            let rx = &m.reactions()[0];
            let at = pd_core::render::draw::add_line_handle(m, rx, h.app.view.zoom()).unwrap();
            let p = h.app.view.world_to_screen(at);
            Pos2::new(p.x as f32, p.y as f32)
        };
        h.run(vec![]);
        h.pointer(plus, None);
        h.pointer(plus, Some(true));
        h.pointer(h.on_screen("S3"), None);
        assert!(
            h.app
                .tool_hint()
                .unwrap()
                .contains("as a product (Shift: a reactant)")
        );
        // Shift goes down, and is still down as the button comes up.
        h.run(vec![Event::ModifiersChanged(Modifiers::SHIFT)]);
        assert!(h.app.tool_hint().unwrap().contains("as a reactant"));
        h.run(vec![Event::PointerButton {
            pos: h.on_screen("S3"),
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::SHIFT,
        }]);
        let reactants = h.app.view.model().reactions()[0].reactants.len();
        assert_eq!(reactants, 2);
        h.run(vec![Event::ModifiersChanged(Modifiers::NONE)]);
        // Add Stem, then Remove Stem.
        let stem = |h: &Harness| h.app.view.model().reactions()[0].stem;
        h.app.act(MenuAction::AddStem(r.clone()), pt(0.0, 0.0));
        assert!(stem(&h).is_some());
        // Bent, then straightened from the menu.
        {
            use pd_core::view::{Button, Modifiers as M, Target, ViewEvent};
            let v = &mut h.app.view;
            v.select_item(&Target::Leg(r.clone()), false);
            // Free to bend (Add Stem keeps it straight).
            v.set_stem_straight(false);
            let m = v.model();
            let rx = &m.reactions()[0];
            let end = v.world_to_screen(rx.leg_anchor(pd_core::model::Role::Reactant));
            let to = end + pt(0.0, -40.0);
            v.handle(&ViewEvent::PointerDown {
                pos: end,
                button: Button::Primary,
                modifiers: M::NONE,
            });
            v.handle(&ViewEvent::PointerMove { pos: to });
            v.handle(&ViewEvent::PointerUp {
                pos: to,
                button: Button::Primary,
            });
        }
        let (a, b) = stem(&h).unwrap();
        assert!((a.x * b.y - a.y * b.x).abs() > 1.0, "bent");
        h.app
            .act(MenuAction::StraightenStem(r.clone()), pt(0.0, 0.0));
        let (a2, b2) = stem(&h).unwrap();
        assert!((a2.x * b2.y - a2.y * b2.x).abs() < 1e-6, "in line again");
        h.app.act(MenuAction::RemoveStem(r), pt(0.0, 0.0));
        assert!(stem(&h).is_none());
    }

    #[test]
    fn bold_and_italic_names_from_the_panel() {
        use pd_core::view::Target;
        let mut h = Harness::new();
        let id = h.app.view.model().species()[0].id.clone();
        h.app.view.select_item(&Target::Species(id.clone()), false);
        h.run(vec![]);
        h.run(vec![]);
        h.click(h.widget("Bold"));
        h.run(vec![]);
        let style = |h: &Harness| h.app.view.model().find_species(&id).unwrap().style;
        assert!(style(&h).label_bold);
        assert!(!style(&h).label_italic);
        h.click(h.widget("Italic"));
        h.run(vec![]);
        assert!(style(&h).label_italic);
        // Ticked now, so a click unticks it.
        h.click(h.widget("Bold"));
        h.run(vec![]);
        assert!(!style(&h).label_bold);
    }

    #[test]
    fn modifier_kinds_from_the_panel_and_the_menu() {
        use pd_core::model::ModifierKind;
        use pd_core::view::Target;
        let mut h = Harness::new();
        let text = include_str!("../../../ExampleModels/mapk_cascade.json");
        h.app.perform(Action::OpenText {
            name: "mapk.json".into(),
            text: text.into(),
            target: None,
        });
        h.run(vec![]);
        let kind =
            |h: &Harness, r: &str| h.app.view.model().find_reaction(r).unwrap().modifiers[0].kind;
        // J3 has one modifier, MKKK_P: the Properties panel lists it.
        h.app.view.select_item(&Target::Leg("J3".into()), false);
        h.run(vec![]);
        h.run(vec![]);
        h.text("Modifiers");
        // One section, open: its name and kind, then its line's look.
        h.widget("MKKK_P: Modifier");
        for row in [
            "Line",
            "Line width",
            "Pattern",
            "Symbol size",
            "Symbol distance",
        ] {
            h.text(row);
        }
        h.click(h.text("Modifier"));
        h.run(vec![]);
        h.click(h.widget("Activator"));
        h.run(vec![]);
        assert_eq!(kind(&h, "J3"), ModifierKind::Activator);
        assert_eq!(h.app.view.undo_description(), Some("Modifier Kind"));
        // The right-click menu's choice.
        h.app.act(
            MenuAction::ModifierKind("J1".into(), "MAPK_PP".into(), ModifierKind::Catalyst),
            pt(0.0, 0.0),
        );
        assert_eq!(kind(&h, "J1"), ModifierKind::Catalyst);
        // Corner points from the arc's and the point's menus (S §5.7.1).
        let points = |h: &Harness| {
            h.app.view.model().find_reaction("J3").unwrap().modifiers[0]
                .points
                .clone()
        };
        let at = pt(800.0, 100.0);
        h.app.act(
            MenuAction::AddCornerPoint("J3".into(), (pd_core::model::Role::Modifier, 0), 0, at),
            pt(0.0, 0.0),
        );
        assert_eq!(points(&h), [at]);
        h.app.act(
            MenuAction::AddCornerPoint(
                "J3".into(),
                (pd_core::model::Role::Modifier, 0),
                1,
                pt(500.0, 100.0),
            ),
            pt(0.0, 0.0),
        );
        assert_eq!(points(&h).len(), 2);
        h.app.act(
            MenuAction::RemoveCornerPoint("J3".into(), (pd_core::model::Role::Modifier, 0), 0),
            pt(0.0, 0.0),
        );
        assert_eq!(points(&h), [pt(500.0, 100.0)]);
        h.app.act(
            MenuAction::StraightenLine("J3".into(), (pd_core::model::Role::Modifier, 0)),
            pt(0.0, 0.0),
        );
        assert!(points(&h).is_empty());
    }

    #[test]
    fn enter_moves_the_open_reaction_tool_on() {
        let mut h = Harness::new();
        h.app.perform(Action::New);
        h.run(vec![]);
        h.app.view.set_tool(Tool::AddSpecies);
        for x in [400.0, 550.0, 700.0] {
            h.time += 1.0;
            h.click(Pos2::new(x, 400.0));
        }
        h.key(Key::Escape, Modifiers::NONE);
        // The button, then two reactants, Enter, one product, Enter.
        h.time += 1.0;
        h.click(h.widget("A+… -> B+…"));
        assert_eq!(h.app.view.tool(), Tool::ANY_REACTION);
        assert!(h.app.tool_hint().unwrap().contains("Enter for products"));
        for id in ["S1", "S2"] {
            h.time += 1.0;
            h.click(h.on_screen(id));
        }
        h.key(Key::Enter, Modifiers::NONE);
        assert!(h.app.tool_hint().unwrap().contains("Enter to finish"));
        h.time += 1.0;
        h.click(h.on_screen("S3"));
        assert!(h.app.view.model().reactions().is_empty());
        h.key(Key::Enter, Modifiers::NONE);
        let r = &h.app.view.model().reactions()[0];
        assert_eq!((r.reactants.len(), r.products.len()), (2, 1));
    }

    #[test]
    fn the_delphi_notice_shows_only_when_a_file_first_outgrows_it() {
        let notice = "This file now needs PathwayDesigner (Rust) to open";
        let mut h = Harness::new();
        h.app.perform(Action::New);
        h.run(vec![]);
        // Two species and a reaction: version 3, which the Delphi app reads.
        h.app.view.set_tool(Tool::AddSpecies);
        h.click(Pos2::new(400.0, 400.0));
        h.time += 1.0;
        h.click(Pos2::new(700.0, 400.0));
        h.key(Key::Escape, Modifiers::NONE);
        h.app.view.set_tool(Tool::UNI_UNI);
        h.time += 1.0;
        h.click(h.on_screen("S1"));
        h.time += 1.0;
        h.click(h.on_screen("S2"));
        h.app.view.set_tool(Tool::Select);
        assert_eq!(h.app.view.model().reactions().len(), 1);
        h.app.notice = None;
        h.run(vec![]);
        assert_eq!(h.app.notice, None);
        // A name outside its node: version 6, past the Delphi app.
        h.app.view.select_all();
        h.app.view.set_species_label_outside(true);
        h.run(vec![]);
        assert_eq!(h.app.notice.as_deref(), Some(notice));
        // A reaction's name: version 7, but no news for the Delphi app.
        h.app.notice = None;
        h.app.view.set_reaction_label_shown(true);
        h.run(vec![]);
        assert_eq!(pd_core::json::needed_version(h.app.view.model()), 7);
        assert_eq!(h.app.notice, None);
    }

    #[test]
    fn arrow_keys_step_along_the_grid_while_snapping() {
        let mut h = Harness::new();
        h.app.perform(Action::New);
        h.run(vec![]);
        h.app.view.snap_to_grid = true;
        h.app.view.set_tool(Tool::AddSpecies);
        h.click(Pos2::new(400.0, 400.0));
        h.key(Key::Escape, Modifiers::NONE);
        h.time += 1.0;
        h.click(h.on_screen("S1"));
        // Placed on the grid, it moves a line at a time; Shift, five.
        let start = h.centre("S1");
        assert_eq!(pd_core::view::snap_point(start), start);
        h.key(Key::ArrowRight, Modifiers::NONE);
        h.key(Key::ArrowDown, Modifiers::SHIFT);
        assert_eq!(h.centre("S1") - start, pt(20.0, 100.0));
        // Alt: the fine steps, 1 and 10.
        h.key(Key::ArrowLeft, Modifiers::ALT);
        h.key(Key::ArrowUp, Modifiers::ALT.plus(Modifiers::SHIFT));
        assert_eq!(h.centre("S1") - start, pt(19.0, 90.0));
    }

    #[test]
    fn the_startup_fit_follows_the_canvas_until_touched() {
        let cc = eframe::CreationContext::_new_kittest(egui::Context::default());
        let mut h = Harness {
            app: PathwayApp::new(&cc),
            frame: eframe::Frame::_new_kittest(),
            time: 0.0,
            access: None,
            size: egui::vec2(1400.0, 900.0),
            cursor: egui::CursorIcon::Default,
            opened: Vec::new(),
        };
        // The first frames see a small canvas, as a new browser tab can.
        h.size = egui::vec2(300.0, 150.0);
        h.run(vec![]);
        let small = h.app.view.zoom();
        h.size = egui::vec2(1400.0, 900.0);
        h.run(vec![]);
        let big = h.app.view.zoom();
        assert!(
            big > small * 2.0,
            "fitted again for the real canvas: {small} -> {big}"
        );
        // Once the user has done something, a resize leaves the view alone.
        h.pointer(Pos2::new(700.0, 850.0), Some(true));
        h.pointer(Pos2::new(700.0, 850.0), Some(false));
        h.size = egui::vec2(1000.0, 700.0);
        h.run(vec![]);
        assert_eq!(h.app.view.zoom(), big);
    }

    #[test]
    fn opening_files_by_path_and_the_recent_list() {
        let mut h = Harness::new();
        let dir = std::env::temp_dir().join(format!("pd-recent-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let a = dir.join("first.json");
        let b = dir.join("second.ant");
        std::fs::write(&a, crate::samples::SAMPLES[0].1).unwrap();
        std::fs::write(
            &b,
            "A -> B; k*A
k = 0.1",
        )
        .unwrap();

        h.app.open_path(&a);
        assert_eq!(h.app.doc.name, "first.json");
        h.app.open_path(&b);
        assert_eq!(h.app.doc.name, "second.json");
        assert_eq!(h.app.recent.files, [b.clone(), a.clone()]);

        // Reopening from the list moves it to the top.
        h.app.perform(Action::OpenPath(a.clone()));
        assert_eq!(h.app.recent.files, [a.clone(), b.clone()]);

        // A file that is gone gives a warning and leaves the list.
        std::fs::remove_file(&b).unwrap();
        h.app.perform(Action::OpenPath(b.clone()));
        assert!(h.app.warnings[0].contains("couldn't be opened"));
        assert_eq!(h.app.recent.files, std::slice::from_ref(&a));
        assert_eq!(h.app.doc.name, "first.json", "the diagram is unchanged");
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// docs/display-names-design.md D3: the species panel shows the id and
    /// a display name that can be typed, as one undo step.
    #[test]
    fn a_display_name_is_typed_in_the_panel() {
        let mut h = Harness::new();
        let id = h.app.view.model().species()[0].id.clone();
        h.click(h.on_screen(&id));
        h.run(vec![]);
        // The field is in the column beside its label.
        let field = h.text("Display name") + egui::vec2(140.0, 0.0);
        h.click(field);
        h.run(vec![Event::Text("glucose".into())]);
        h.key(Key::Enter, Modifiers::NONE);
        h.run(vec![]);
        let s = h.app.view.model().find_species(&id).unwrap();
        assert_eq!(s.display_name.as_deref(), Some("glucose"));
        assert_eq!(h.app.view.undo_description(), Some("Edit Display Name"));
        // The id row still shows the id.
        h.text(&id);
    }

    /// docs/display-names-design.md D4: View ▸ Display names labels the
    /// diagram with names, and each panel's Label choice overrides it.
    #[test]
    fn view_display_names_and_the_label_choice() {
        use pd_core::model::{LabelChoice, StyleKind};
        let mut h = Harness::new();
        let id = h.app.view.model().species()[0].id.clone();
        h.app.view.set_display_name(&id, "glucose");
        h.run(vec![]);
        assert_eq!(h.app.view.model().species()[0].label(), id);
        h.click(h.widget("View"));
        h.run(vec![]);
        h.click(h.widget("Display names"));
        h.run(vec![]);
        assert!(h.app.view.show_display_names());
        assert_eq!(h.app.view.model().species()[0].label(), "glucose");
        // The species set to show its id, from its panel.
        h.key(Key::Escape, Modifiers::NONE);
        h.click(h.on_screen(&id));
        h.run(vec![]);
        h.click(h.text("As the diagram"));
        h.run(vec![]);
        h.click(h.widget("Id"));
        h.run(vec![]);
        assert_eq!(
            h.app.view.selected_label_choice(StyleKind::Species),
            Some(LabelChoice::Id)
        );
        assert_eq!(h.app.view.model().find_species(&id).unwrap().label(), id);
    }

    /// docs/annotations-design.md step 4: a species' Annotations part picks
    /// an SBO term, adds a reference typed as a compact identifier, and
    /// removes it, each one undo step.
    #[test]
    fn annotations_are_edited_in_the_panel() {
        let mut h = Harness::new();
        let id = h.app.view.model().species()[0].id.clone();
        h.click(h.on_screen(&id));
        h.run(vec![]);
        h.click(h.widget("Annotations"));
        h.run(vec![]);
        // An SBO term from the common ones.
        h.click(h.widget("Common"));
        h.run(vec![]);
        assert!(
            !h.access
                .as_ref()
                .unwrap()
                .nodes
                .iter()
                .any(|(_, n)| n.label() == Some("62  continuous framework")),
            "a species' list has no frameworks"
        );
        h.click(h.widget("247  simple chemical"));
        h.run(vec![]);
        let a = |h: &Harness| {
            h.app
                .view
                .model()
                .find_species(&id)
                .unwrap()
                .annotation
                .clone()
        };
        assert_eq!(a(&h).sbo, Some(247));
        assert_eq!(h.app.view.undo_description(), Some("Edit Annotation"));
        // A reference, typed in the field left of Add.
        let field = h.widget("Add") - egui::vec2(90.0, 0.0);
        h.click(field);
        h.run(vec![Event::Text("CHEBI:17234".into())]);
        h.click(h.widget("Add"));
        h.run(vec![]);
        let refs = a(&h).references;
        assert_eq!(refs.len(), 1, "{refs:?}");
        assert_eq!(refs[0].qualifier, "bqbiol:is");
        assert_eq!(refs[0].resource, "https://identifiers.org/CHEBI:17234");
        h.run(vec![]);
        // Shown as a link, and removed with its button.
        h.click(h.widget("✕"));
        h.run(vec![]);
        assert!(a(&h).references.is_empty());
        assert_eq!(a(&h).sbo, Some(247));
        // Notes, typed as text, applied when the field loses focus.
        let notes = h.text("Notes") + egui::vec2(0.0, 30.0);
        h.click(notes);
        h.run(vec![Event::Text("From yeast.".into())]);
        h.click(h.text("Notes"));
        h.run(vec![]);
        assert!(
            a(&h)
                .notes
                .as_deref()
                .is_some_and(|n| n.contains("<p>From yeast.</p>")),
            "{:?}",
            a(&h).notes
        );
        // With nothing selected, the Diagram's part is the model's.
        h.key(Key::Escape, Modifiers::NONE);
        h.click(h.on_screen(&id) + egui::vec2(0.0, 300.0));
        h.run(vec![]);
        h.click(h.widget("Annotations"));
        h.run(vec![]);
        // A model's SBO term is its framework, and its list holds frameworks.
        h.text("Framework (SBO)");
        h.click(h.widget("Common"));
        h.run(vec![]);
        let shown = |h: &Harness, label: &str| {
            h.access
                .as_ref()
                .unwrap()
                .nodes
                .iter()
                .any(|(_, n)| n.label() == Some(label))
        };
        assert!(shown(&h, "624  flux balance framework"));
        assert!(!shown(&h, "247  simple chemical"), "not a species' term");
        h.click(h.widget("62  continuous framework"));
        h.run(vec![]);
        assert_eq!(h.app.view.model().annotation.sbo, Some(62));
    }

    /// A link clicked in the app opens in a new tab, never over the app
    /// itself (in the browser): here, an annotation's reference.
    #[test]
    fn links_open_in_a_new_tab() {
        let mut h = Harness::new();
        let id = h.app.view.model().species()[0].id.clone();
        let a = pd_core::model::Annotation {
            references: vec![pd_core::model::Reference {
                qualifier: "bqbiol:is".into(),
                resource: "https://identifiers.org/CHEBI:17234".into(),
            }],
            ..Default::default()
        };
        h.app.view.set_annotation(Some(&id), a);
        h.click(h.on_screen(&id));
        h.run(vec![]);
        h.click(h.widget("Annotations (1)"));
        h.run(vec![]);
        let link = h.widget("CHEBI:17234");
        h.pointer(link, None);
        h.pointer(link, Some(true));
        h.pointer(link, Some(false));
        assert_eq!(h.opened.len(), 1, "{:?}", h.opened);
        assert_eq!(h.opened[0].url, "https://identifiers.org/CHEBI:17234");
        assert!(h.opened[0].new_tab);
    }

    /// docs/chebi-lookup-design.md step 2: Find in ChEBI lists what a name
    /// finds, and Add makes one a reference. BiGG knows no "glucose"; ChEBI
    /// does, exactly.
    fn chebi(_: &egui::Context, url: String) -> crate::gallery::Pending {
        let answer = if url.contains("ols4") && url.contains("q=glucose&") {
            Ok(r#"{"response": {"docs": [
                {"obo_id": "CHEBI:17234", "label": "glucose"},
                {"obo_id": "CHEBI:4167", "label": "D-glucopyranose"}]}}"#
                .to_owned())
        } else {
            Err("not found (404)".to_owned())
        };
        std::sync::Arc::new(std::sync::Mutex::new(Some(answer)))
    }

    #[test]
    fn find_in_chebi_adds_what_a_name_finds() {
        let mut h = Harness::new();
        h.app.gallery.fetch = chebi;
        let id = h.app.view.model().species()[0].id.clone();
        // The words to find start as the species' display name.
        h.app.view.set_display_name(&id, "glucose");
        h.click(h.on_screen(&id));
        h.run(vec![]);
        h.click(h.widget("Annotations"));
        h.run(vec![]);
        h.click(h.widget("Search"));
        for _ in 0..80 {
            h.run(vec![]);
        }
        let labels = |h: &Harness| -> Vec<String> {
            h.access
                .as_ref()
                .unwrap()
                .nodes
                .iter()
                .filter_map(|(_, n)| n.label().or(n.value()).map(str::to_owned))
                .collect()
        };
        let shown = labels(&h);
        assert!(shown.iter().any(|l| l == "CHEBI:17234"), "{shown:?}");
        assert!(shown.iter().any(|l| l == "D-glucopyranose"), "{shown:?}");
        // The first found's Add: the lowest "Add" but one (the references'
        // own Add is above them).
        let mut adds: Vec<Pos2> = h
            .access
            .as_ref()
            .unwrap()
            .nodes
            .iter()
            .filter(|(_, n)| n.label() == Some("Add"))
            .filter_map(|(_, n)| n.bounds())
            .map(|b| Pos2::new(((b.x0 + b.x1) / 2.0) as f32, ((b.y0 + b.y1) / 2.0) as f32))
            .collect();
        adds.sort_by(|a, b| a.y.total_cmp(&b.y));
        assert_eq!(adds.len(), 3, "the references' Add and one per entry");
        h.click(adds[1]);
        h.run(vec![]);
        let refs = &h
            .app
            .view
            .model()
            .find_species(&id)
            .unwrap()
            .annotation
            .references;
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].resource, "https://identifiers.org/CHEBI:17234");
        assert_eq!(h.app.view.undo_description(), Some("Edit Annotation"));
        // Added once: its button says so.
        assert!(labels(&h).iter().any(|l| l == "Added"));
    }

    /// BiGG knows g6p; ChEBI knows ATP exactly; nothing knows XYZ.
    fn annotate_services(_: &egui::Context, url: String) -> crate::gallery::Pending {
        let answer = if url.ends_with("metabolites/g6p") {
            Ok(r#"{"name": "D-Glucose 6-phosphate", "database_links": {
                "CHEBI": [{"id": "CHEBI:4170"}, {"id": "CHEBI:14314"}],
                "KEGG Compound": [{"id": "C00092"}]}}"#
                .to_owned())
        } else if url.contains("q=D-Glucose%206-phosphate&") {
            Ok(r#"{"response": {"docs": [
                {"obo_id": "CHEBI:4170", "label": "D-glucose 6-phosphate"}]}}"#
                .to_owned())
        } else if url.contains("ols4") && url.contains("q=ATP&") {
            Ok(r#"{"response": {"docs": [{"obo_id": "CHEBI:15422", "label": "ATP"}]}}"#.to_owned())
        } else {
            Err("not found (404)".to_owned())
        };
        std::sync::Arc::new(std::sync::Mutex::new(Some(answer)))
    }

    /// docs/chebi-lookup-design.md step 3: Edit ▸ Annotate Species from
    /// Names searches every species, the Annotate tab reviews what was
    /// found, ticking the clear matches, and Apply adds them as one step.
    #[test]
    fn annotate_species_from_names_reviews_then_applies() {
        let mut h = Harness::new();
        h.app.gallery.fetch = annotate_services;
        let opts = h.app.import_options();
        h.app
            .view
            .import_antimony(
                "G6P -> ATP; k*G6P
ATP -> XYZ; k*ATP
k = 1",
                &opts,
            )
            .unwrap();
        h.run(vec![]);
        h.click(h.widget("Edit"));
        h.run(vec![]);
        h.click(h.widget("Annotate Species from Names…"));
        h.run(vec![]);
        assert_eq!(h.app.right_tab, RightTab::Annotate);
        h.widget("Annotate");
        for _ in 0..120 {
            h.run(vec![]);
        }
        h.text("Found 2 of 3 species; 2 ticked as clear matches.");
        h.text("Nothing found for");
        // A name in the review shows its species: selected, centred.
        let centred = |h: &Harness, id: &str| {
            let at = h.on_screen(id);
            let c = h.app.canvas_centre;
            (f64::from(at.x) - c.x).abs() < 1.0 && (f64::from(at.y) - c.y).abs() < 1.0
        };
        h.click(h.widget("XYZ"));
        h.run(vec![]);
        assert!(h.app.view.model().find_species("XYZ").unwrap().selected);
        assert!(centred(&h, "XYZ"));
        h.click(h.widget("G6P"));
        h.run(vec![]);
        assert!(h.app.view.model().find_species("G6P").unwrap().selected);
        assert!(!h.app.view.model().find_species("XYZ").unwrap().selected);
        assert!(centred(&h, "G6P"));
        assert!(h.app.annotate.is_some(), "the review stays");
        h.click(h.widget("Apply (2)"));
        h.run(vec![]);
        let m = h.app.view.model();
        let g6p = m.find_species("G6P").unwrap();
        let refs: Vec<&str> = g6p
            .annotation
            .references
            .iter()
            .map(|r| r.resource.as_str())
            .collect();
        assert_eq!(
            refs,
            [
                "https://identifiers.org/CHEBI:4170",
                "https://identifiers.org/kegg.compound:C00092"
            ]
        );
        assert_eq!(g6p.display_name.as_deref(), Some("D-Glucose 6-phosphate"));
        let atp = m.find_species("ATP").unwrap();
        assert_eq!(
            atp.annotation.references[0].resource,
            "https://identifiers.org/CHEBI:15422"
        );
        assert!(m.find_species("XYZ").unwrap().annotation.is_empty());
        assert_eq!(h.app.view.undo_description(), Some("Annotate Species"));
        // The tab is gone.
        assert!(h.app.annotate.is_none());
        assert_eq!(h.app.right_tab, RightTab::Properties);
    }

    /// BiGG knows PPC.
    fn reaction_services(_: &egui::Context, url: String) -> crate::gallery::Pending {
        let answer = if url.ends_with("universal/reactions/PPC") {
            Ok(
                r#"{"bigg_id": "PPC", "name": "Phosphoenolpyruvate carboxylase",
                "metabolites": [
                    {"bigg_id": "pep", "name": "Phosphoenolpyruvate", "stoichiometry": -1.0},
                    {"bigg_id": "oaa", "name": "Oxaloacetate", "stoichiometry": 1.0}],
                "database_links": {"RHEA": [{"id": "23072"}],
                    "EC Number": [{"id": "4.1.1.31"}]}}"#
                    .to_owned(),
            )
        } else {
            Err("not found (404)".to_owned())
        };
        std::sync::Arc::new(std::sync::Mutex::new(Some(answer)))
    }

    /// docs/reaction-lookup-design.md step 2: Edit ▸ Annotate Reactions
    /// from Names, the review in the Annotate tab, Apply as one step.
    #[test]
    fn annotate_reactions_from_names_reviews_then_applies() {
        let mut h = Harness::new();
        h.app.gallery.fetch = reaction_services;
        let opts = h.app.import_options();
        h.app
            .view
            .import_antimony(
                "PPC: PEP -> OAA; k*PEP
X1: OAA -> Q; k*OAA
k = 1",
                &opts,
            )
            .unwrap();
        h.run(vec![]);
        h.click(h.widget("Edit"));
        h.run(vec![]);
        h.click(h.widget("Annotate Reactions from Names…"));
        h.run(vec![]);
        assert_eq!(h.app.right_tab, RightTab::Annotate);
        for _ in 0..120 {
            h.run(vec![]);
        }
        h.text("Found 1 of 2 reactions; 1 ticked as clear matches.");
        h.text("EC 4.1.1.31 · Rhea 23072");
        h.text("pep ⇌ oaa");
        // R6: no species has a ChEBI entry, so Rhea couldn't be asked.
        h.text(
            "Some reactions have species without a ChEBI entry; annotate the species first (Annotate Species from Names) to find more in Rhea.",
        );
        h.text("Nothing found for");
        // A reaction's name in the review (not the list on the left) shows
        // it: selected, the diagram centred on it, the review staying.
        let link = h
            .access
            .as_ref()
            .unwrap()
            .nodes
            .iter()
            .filter(|(_, n)| n.label() == Some("X1"))
            .filter_map(|(_, n)| n.bounds())
            .find(|b| b.x0 > f64::from(h.size.x) / 2.0)
            .map(|b| Pos2::new(((b.x0 + b.x1) / 2.0) as f32, ((b.y0 + b.y1) / 2.0) as f32))
            .expect("X1 in the review");
        h.click(link);
        h.run(vec![]);
        let m = h.app.view.model();
        let x1 = m.find_reaction("X1").unwrap();
        assert!(x1.selected);
        let at = h
            .app
            .view
            .world_to_screen(pd_core::render::draw::effective_junction(m, x1));
        let c = h.app.canvas_centre;
        assert!(
            (at.x - c.x).abs() < 1.0 && (at.y - c.y).abs() < 1.0,
            "centred on X1"
        );
        assert!(h.app.annotate.is_some());
        h.click(h.widget("Apply (1)"));
        h.run(vec![]);
        let r = h.app.view.model().find_reaction("PPC").unwrap();
        let refs: Vec<String> = r
            .annotation
            .references
            .iter()
            .map(|x| format!("{} {}", x.qualifier, x.resource))
            .collect();
        assert_eq!(
            refs,
            [
                "bqbiol:isVersionOf https://identifiers.org/ec-code:4.1.1.31",
                "bqbiol:is https://identifiers.org/rhea:23072"
            ]
        );
        assert_eq!(
            r.display_name.as_deref(),
            Some("Phosphoenolpyruvate carboxylase")
        );
        assert!(
            h.app
                .view
                .model()
                .find_reaction("X1")
                .unwrap()
                .annotation
                .is_empty()
        );
        assert_eq!(h.app.view.undo_description(), Some("Annotate Reactions"));
        assert!(h.app.annotate.is_none());

        // Again from the reaction's menu: PPC has an EC number now, so
        // there is nothing to look up.
        h.app
            .act(MenuAction::AnnotateReactions("PPC".into()), pt(0.0, 0.0));
        h.run(vec![]);
        assert!(h.app.annotate.is_none());
        // X1 alone, from its menu: offline, said so; Close changes nothing.
        h.app.gallery.fetch = offline;
        h.app
            .act(MenuAction::AnnotateReactions("X1".into()), pt(0.0, 0.0));
        for _ in 0..40 {
            h.run(vec![]);
        }
        h.text("BiGG and Rhea couldn't be reached, so nothing was found.");
        let before = h.app.view.model().clone();
        h.click(h.widget("Close"));
        h.run(vec![]);
        assert!(h.app.annotate.is_none());
        assert_eq!(*h.app.view.model(), before);
    }

    /// ChEBI knows LONG by a name hundreds of letters long, and ATP.
    fn long_names(_: &egui::Context, url: String) -> crate::gallery::Pending {
        let long = "beta-D-Glcp-(1->3)-[beta-D-Glcp-(1->6)]".repeat(10);
        let answer = if url.contains("ols4") && url.contains("q=LONG&") {
            Ok(format!(
                r#"{{"response": {{"docs": [{{"obo_id": "CHEBI:146098", "label": "{long}"}}]}}}}"#
            ))
        } else if url.contains("ols4") && url.contains("q=ATP&") {
            Ok(r#"{"response": {"docs": [{"obo_id": "CHEBI:15422", "label": "ATP"}]}}"#.to_owned())
        } else {
            Err("not found (404)".to_owned())
        };
        std::sync::Arc::new(std::sync::Mutex::new(Some(answer)))
    }

    /// A very long name found is cut short, so the panel keeps its width;
    /// and the review's Find box keeps only the species that match.
    #[test]
    fn the_review_keeps_its_width_and_can_be_searched() {
        let mut h = Harness::new();
        h.app.gallery.fetch = long_names;
        let opts = h.app.import_options();
        h.app
            .view
            .import_antimony(
                "LONG -> ATP; k*LONG
ATP -> XYZ; k*ATP
k = 1",
                &opts,
            )
            .unwrap();
        h.run(vec![]);
        let canvas = h.app.canvas_centre;
        h.app.start_annotating(None);
        for _ in 0..120 {
            h.run(vec![]);
        }
        h.text("Found 2 of 3 species; 1 ticked as clear matches.");
        let widest = |h: &Harness| {
            h.access
                .as_ref()
                .unwrap()
                .nodes
                .iter()
                .filter_map(|(_, n)| n.bounds())
                .filter(|b| b.x0 > f64::from(h.size.x) / 2.0)
                .map(|b| b.x1 - b.x0)
                .fold(0.0, f64::max)
        };
        assert!(
            widest(&h) < 400.0,
            "the panel wasn't held wide: {}",
            widest(&h)
        );
        assert!(
            (h.app.canvas_centre.x - canvas.x).abs() < 20.0,
            "the canvas wasn't squeezed"
        );
        // Find: "xy" keeps XYZ (in Nothing found) and hides LONG and ATP,
        // in the review (the list on the left has them all).
        let shown = |h: &Harness, label: &str| {
            h.access.as_ref().unwrap().nodes.iter().any(|(_, n)| {
                n.label() == Some(label)
                    && n.bounds().is_some_and(|b| b.x0 > f64::from(h.size.x) / 2.0)
            })
        };
        assert!(shown(&h, "LONG") && shown(&h, "ATP") && shown(&h, "XYZ"));
        // The review's Find box: the text field just below Apply.
        let apply = h.widget("Apply (2)");
        let field = h
            .access
            .as_ref()
            .unwrap()
            .nodes
            .iter()
            .filter(|(_, n)| n.role() == egui::accesskit::Role::TextInput)
            .filter_map(|(_, n)| n.bounds())
            .map(|b| Pos2::new(((b.x0 + b.x1) / 2.0) as f32, ((b.y0 + b.y1) / 2.0) as f32))
            .filter(|p| p.x > apply.x - 100.0 && p.y > apply.y)
            .min_by(|a, b| a.y.total_cmp(&b.y))
            .expect("the Find box");
        h.click(field);
        h.run(vec![Event::Text("xy".into())]);
        h.run(vec![]);
        assert!(shown(&h, "XYZ"));
        assert!(!shown(&h, "LONG") && !shown(&h, "ATP"));
    }

    /// From a species' menu: just that species. With no network the tab
    /// says so; Close, Stop and Cancel change nothing.
    #[test]
    fn annotate_species_can_be_stopped_or_cancelled() {
        let mut h = Harness::new();
        h.app.gallery.fetch = offline;
        let id = h.app.view.model().species()[0].id.clone();
        h.app
            .act(MenuAction::AnnotateSpecies(id.clone()), pt(0.0, 0.0));
        h.run(vec![]);
        assert_eq!(h.app.right_tab, RightTab::Annotate);
        for _ in 0..40 {
            h.run(vec![]);
        }
        h.text("BiGG, ChEBI and PubChem couldn't be reached, so nothing was found.");
        let before = h.app.view.model().clone();
        h.click(h.widget("Close"));
        h.run(vec![]);
        assert!(h.app.annotate.is_none());
        assert_eq!(*h.app.view.model(), before);

        // Stopped straight away: the review of what was found (nothing yet),
        // and Cancel.
        h.app.gallery.fetch = annotate_services;
        h.app.start_annotating(None);
        h.run(vec![]);
        h.click(h.widget("Stop"));
        h.run(vec![]);
        h.click(h.widget("Cancel"));
        h.run(vec![]);
        assert!(h.app.annotate.is_none());
        assert_eq!(h.app.right_tab, RightTab::Properties);
        assert_eq!(*h.app.view.model(), before);
    }

    /// The list on the left: Ctrl+F puts the cursor in its Find box; the
    /// words keep only the items that hold them, in their id or display
    /// name; Enter or a click selects one and centres the diagram on it.
    #[test]
    fn find_in_the_list_selects_and_centres() {
        let mut h = Harness::new();
        let opts = h.app.import_options();
        h.app
            .view
            .import_antimony(
                "G6P -> ATP; k*G6P
ATP -> ADP; k*ATP
k = 1",
                &opts,
            )
            .unwrap();
        h.app.view.set_display_name("ADP", "adenosine diphosphate");
        h.run(vec![]);
        let centred = |h: &Harness, id: &str| {
            let at = h.on_screen(id);
            let c = h.app.canvas_centre;
            (f64::from(at.x) - c.x).abs() < 1.0 && (f64::from(at.y) - c.y).abs() < 1.0
        };
        h.key(Key::F, Modifiers::COMMAND);
        h.run(vec![Event::Text("atp".into())]);
        h.run(vec![]);
        let listed = |h: &Harness| -> Vec<String> {
            h.access
                .as_ref()
                .unwrap()
                .nodes
                .iter()
                .filter_map(|(_, n)| n.label().map(str::to_owned))
                .filter(|l| ["G6P", "ATP", "ADP — adenosine diphosphate"].contains(&l.as_str()))
                .collect()
        };
        assert_eq!(listed(&h), ["ATP"], "only what holds the words");
        // Enter: the first found.
        h.key(Key::Enter, Modifiers::NONE);
        h.run(vec![]);
        assert!(h.app.view.model().find_species("ATP").unwrap().selected);
        assert!(centred(&h, "ATP"));
        // By display name, and picked with a click.
        let field = h.text("atp");
        h.click(field);
        h.key(Key::A, Modifiers::COMMAND);
        h.run(vec![Event::Text("diphos".into())]);
        h.run(vec![]);
        assert_eq!(listed(&h), ["ADP — adenosine diphosphate"]);
        h.click(h.widget("ADP — adenosine diphosphate"));
        h.run(vec![]);
        assert!(h.app.view.model().find_species("ADP").unwrap().selected);
        assert!(centred(&h, "ADP"));
        // Without words, everything again, and a click doesn't move the view.
        let field = h.text("diphos");
        h.click(field);
        h.key(Key::A, Modifiers::COMMAND);
        h.key(Key::Backspace, Modifiers::NONE);
        h.run(vec![]);
        assert_eq!(listed(&h).len(), 3);
        let scroll = h.app.view.scroll();
        h.click(h.widget("G6P"));
        h.run(vec![]);
        assert!(h.app.view.model().find_species("G6P").unwrap().selected);
        assert_eq!(h.app.view.scroll(), scroll);
    }

    /// C9: with no network, Find in ChEBI says so and adds nothing.
    #[test]
    fn find_in_chebi_without_a_network_says_so() {
        let mut h = Harness::new();
        h.app.gallery.fetch = offline;
        let id = h.app.view.model().species()[0].id.clone();
        h.click(h.on_screen(&id));
        h.run(vec![]);
        h.click(h.widget("Annotations"));
        h.run(vec![]);
        h.click(h.widget("Search"));
        for _ in 0..80 {
            h.run(vec![]);
        }
        h.text("BiGG, ChEBI and PubChem couldn't be reached");
        let a = &h.app.view.model().find_species(&id).unwrap().annotation;
        assert!(a.references.is_empty());
    }

    /// A4: notes the user only looks at stay as they were read.
    #[test]
    fn untouched_notes_stay_as_read() {
        let mut h = Harness::new();
        let id = h.app.view.model().species()[0].id.clone();
        let read = "<body xmlns=\"http://www.w3.org/1999/xhtml\"><p class=\"x\">Kept <b>as</b> read.</p></body>";
        let a = pd_core::model::Annotation {
            notes: Some(read.into()),
            ..Default::default()
        };
        h.app.view.set_annotation(Some(&id), a);
        h.click(h.on_screen(&id));
        h.run(vec![]);
        // Titled with how many it holds.
        h.click(h.widget("Annotations (1)"));
        h.run(vec![]);
        let notes = h.text("Notes") + egui::vec2(0.0, 30.0);
        h.click(notes);
        // Typed and taken back: the same words, so the notes stay as read.
        h.run(vec![Event::Text("x".into())]);
        h.key(Key::Backspace, Modifiers::NONE);
        h.click(h.text("Notes"));
        h.run(vec![]);
        let m = h.app.view.model();
        assert_eq!(
            m.find_species(&id).unwrap().annotation.notes.as_deref(),
            Some(read)
        );
    }

    #[test]
    fn save_works_while_typing_in_the_text_box() {
        let dir = std::env::temp_dir().join(format!("pd-save-typing-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("existing.json");
        let mut h = Harness::new();
        h.app.doc.target = Some(crate::files::SaveTarget::Path(path.clone()));
        // Place text below S7, where the canvas is empty.
        h.app.view.set_tool(Tool::AddText);
        let at = h.on_screen("S7") + egui::vec2(0.0, 90.0);
        h.click(at);
        h.run(vec![]);
        h.run(vec![Event::Text("Typed, not yet clicked away".into())]);
        assert!(
            h.app.ctx.egui_wants_keyboard_input(),
            "the keyboard is still in the text box"
        );
        h.key(Key::S, Modifiers::COMMAND);
        // Saving writes the file on another thread (files.rs), so wait for the
        // notice that it has finished before reading it.
        let saved_notice = |h: &Harness| {
            h.app
                .notice
                .as_deref()
                .is_some_and(|n| n.starts_with("Saved"))
        };
        for _ in 0..500 {
            h.run(vec![]);
            if saved_notice(&h) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(saved_notice(&h), "Save finished");
        let saved = std::fs::read_to_string(&path).expect("Save wrote the file");
        assert!(saved.contains("\"texts\""), "{saved}");
        assert!(saved.contains("Typed, not yet clicked away"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn importing_a_file_names_the_document_after_it() {
        use crate::files::{FileEvent, Purpose};
        let mut h = Harness::new();
        // The document came from a file, so Save would write back to it.
        assert_eq!(h.app.doc.name, "model9.json");
        h.app.doc.target = Some(crate::files::SaveTarget::Path("model9.json".into()));
        // A reaction's name shown: the SBML imported needs version 7.
        let mut named = h.app.view.model().clone();
        named.reactions_mut().next().unwrap().label_offset = Some(Pt::ZERO);
        let xml = pd_core::sbml::export(&named).xml;
        for (purpose, name, text) in [
            (Purpose::ImportSbml, "glycolysis.xml", xml),
            (
                Purpose::ImportAntimony,
                "feedback.ant",
                "A -> B; k*A
k = 0.1"
                    .to_owned(),
            ),
        ] {
            h.app.file_event(FileEvent::Opened {
                purpose,
                name: name.into(),
                text,
                target: None,
            });
            let json = crate::document::with_extension(name, "json");
            assert_eq!(h.app.doc.name, json, "{purpose:?}");
            // Save asks where, rather than writing over the previous file.
            assert!(h.app.doc.target.is_none(), "{purpose:?}");
            // Still one undo step.
            assert!(h.app.view.can_undo());
            // Importing isn't an edit: the status bar says what came in.
            h.run(vec![]);
            assert_eq!(
                h.app.notice,
                Some(format!("Imported {name}")),
                "{purpose:?}"
            );
        }
    }

    #[test]
    fn selecting_in_the_tree_leaves_the_diagram_where_it_is() {
        let mut h = Harness::new();
        h.run(vec![]);
        let origin = h.app.view.world_to_screen(Pt::ZERO);
        // Click a row, then step down the list with the arrow key.
        h.click(h.widget("S5"));
        assert_eq!(h.app.view.selection().0, ["S5"]);
        h.key(Key::ArrowDown, Modifiers::NONE);
        assert_eq!(h.app.view.selection().0, ["S6"]);
        assert_eq!(
            h.app.view.world_to_screen(Pt::ZERO),
            origin,
            "the diagram didn't move"
        );
    }

    #[test]
    fn species_follow_the_pointer_only_while_dragged() {
        let mut h = Harness::new();
        h.app.perform(Action::New);
        h.run(vec![]);
        h.app.view.set_tool(Tool::AddSpecies);
        for (x, y) in [
            (300.0, 300.0),
            (500.0, 300.0),
            (300.0, 500.0),
            (500.0, 500.0),
        ] {
            h.click(Pos2::new(x, y));
        }
        h.key(Key::Escape, Modifiers::NONE);
        assert_eq!(h.app.view.tool(), Tool::Select);
        // Select everything, then click empty canvas to clear it.
        h.key(Key::A, Modifiers::COMMAND);
        assert_eq!(h.app.view.selection().0.len(), 4);
        h.click(Pos2::new(800.0, 700.0));
        assert!(h.app.view.selection().0.is_empty());

        let zoom = h.app.view.zoom();
        for id in ["S1", "S2", "S3", "S4"] {
            let (start, at) = (h.centre(id), h.on_screen(id));
            let step = Vec2::new(30.0, 20.0);
            h.pointer(at, None);
            h.pointer(at, Some(true));
            h.pointer(at + step / 2.0, None);
            h.pointer(at + step, None);
            let moved = h.centre(id) - start;
            assert!(
                (moved.x * zoom - 30.0).abs() < 1e-6,
                "{id} moved {moved:?} while dragged"
            );
            assert!(
                (moved.y * zoom - 20.0).abs() < 1e-6,
                "{id} moved {moved:?} while dragged"
            );
            h.pointer(at + step, Some(false));
            assert!(!h.app.view.is_busy(), "{id}: the drag ended on release");
            // After the release it stays where it was dropped.
            h.pointer(at + step * 2.0, None);
            assert_eq!(
                h.centre(id) - start,
                moved,
                "{id} followed the pointer after release"
            );
        }
    }

    #[test]
    fn the_pointer_position_reads_in_whole_units() {
        assert_eq!(
            pointer_text(Some(Pt { x: 412.4, y: -86.6 })),
            "x 412, y -87"
        );
        assert_eq!(pointer_text(Some(Pt { x: -0.2, y: 0.4 })), "x 0, y 0");
        assert_eq!(pointer_text(None), "");
    }

    #[test]
    fn the_edit_style_window_changes_a_style() {
        use pd_core::model::{NodeShape, StyleKind};
        let mut h = Harness::new();
        // A species style, followed by the first species.
        let id = h.app.view.model().species()[0].id.clone();
        let target = pd_core::view::Target::Species(id.clone());
        h.app.view.save_style(&target, "Enzyme").unwrap();
        h.app.right_tab = RightTab::Styles;
        let look = h.app.view.model().styles[0].look;
        h.app.style_editor = Some(crate::styles_panel::StyleEditor::new(
            StyleKind::Species,
            "Enzyme".into(),
            look,
        ));
        h.run(vec![]);
        h.run(vec![]);
        // Only the rows that make a look.
        let labels: Vec<String> = h
            .access
            .as_ref()
            .unwrap()
            .nodes
            .iter()
            .filter_map(|(_, n)| n.value().map(str::to_owned))
            .collect();
        for row in ["Fill", "Border width", "Shape", "Name"] {
            assert!(labels.iter().any(|l| l == row), "{row}");
        }
        for row in ["Concentration", "Boundary", "Id", "Width"] {
            assert!(!labels.iter().any(|l| l == row), "{row}");
        }
        // A change in the window, then OK: the style and its species change.
        let ed = h.app.style_editor.as_mut().unwrap();
        ed.view_mut().set_species_shape(NodeShape::Hexagon);
        h.run(vec![]);
        h.click(h.widget("OK"));
        h.run(vec![]);
        assert!(h.app.style_editor.is_none(), "closed");
        assert_eq!(h.app.notice.as_deref(), Some("Changed the style “Enzyme”"));
        let s = h.app.view.model().find_species(&id).unwrap();
        assert_eq!(s.shape, NodeShape::Hexagon);
        assert_eq!(s.style_name.as_deref(), Some("Enzyme"));
        assert_eq!(h.app.view.undo_description(), Some("Edit Style"));
        // Cancel changes nothing.
        let look = h.app.view.model().styles[0].look;
        h.app.style_editor = Some(crate::styles_panel::StyleEditor::new(
            StyleKind::Species,
            "Enzyme".into(),
            look,
        ));
        h.app
            .style_editor
            .as_mut()
            .unwrap()
            .view_mut()
            .set_species_shape(NodeShape::Ellipse);
        h.run(vec![]);
        h.click(h.widget("Cancel"));
        h.run(vec![]);
        assert!(h.app.style_editor.is_none());
        assert_eq!(h.app.view.model().styles[0].look, look);
    }

    #[test]
    fn styles_are_imported_shown_and_applied_from_the_styles_tab() {
        use pd_core::model::{Look, NodeShape, SavedStyle, StyleKind, VisualStyle};
        let mut h = Harness::new();
        h.click(h.widget("Styles"));
        h.run(vec![]);
        assert_eq!(h.app.right_tab, RightTab::Styles);
        h.widget("Import Styles…");
        h.widget("Export My Styles…");

        // A styles file with one species style, kept as drawn, as saved
        // styles are.
        let style = SavedStyle {
            name: "Enzyme".into(),
            look: pd_core::styles::resolved(Look::Species {
                style: VisualStyle::default(),
                shape: NodeShape::Hexagon,
                border_dash: Default::default(),
                label_outside: false,
                size: None,
                alias: false,
            }),
        };
        let text = pd_core::json::styles_to_json(std::slice::from_ref(&style));
        h.app.right_tab = RightTab::Properties;
        h.app.file_event(crate::files::FileEvent::Opened {
            purpose: crate::files::Purpose::ImportStyles,
            name: "styles.json".into(),
            text: text.clone(),
            target: None,
        });
        assert_eq!(h.app.view.my_styles(), std::slice::from_ref(&style));
        assert_eq!(h.app.right_tab, RightTab::Styles, "shows what came in");
        assert!(
            h.app
                .notice
                .as_deref()
                .unwrap()
                .starts_with("Imported 1 style from styles.json")
        );
        // Again: nothing new.
        h.app.file_event(crate::files::FileEvent::Opened {
            purpose: crate::files::Purpose::ImportStyles,
            name: "styles.json".into(),
            text,
            target: None,
        });
        assert_eq!(h.app.view.my_styles().len(), 1);
        assert!(
            h.app
                .notice
                .as_deref()
                .unwrap()
                .contains("Imported 0 styles from styles.json; 1 style was already in My Styles")
        );

        // A click on the style gives it to the selected species.
        let id = h.app.view.model().species()[0].id.clone();
        h.app
            .view
            .select_item(&pd_core::view::Target::Species(id.clone()), false);
        h.run(vec![]);
        h.click(h.text("Enzyme"));
        h.run(vec![]);
        let s = h.app.view.model().find_species(&id).unwrap();
        assert_eq!(s.shape, NodeShape::Hexagon);
        assert_eq!(s.style_name.as_deref(), Some("Enzyme"));
        assert_eq!(h.app.view.style_names(StyleKind::Species), ["Enzyme"]);

        // Something that isn't JSON is a warning.
        h.app.file_event(crate::files::FileEvent::Opened {
            purpose: crate::files::Purpose::ImportStyles,
            name: "notes.txt".into(),
            text: "hello".into(),
            target: None,
        });
        assert_eq!(h.app.warnings.len(), 1);
        assert_eq!(h.app.view.my_styles().len(), 1);
        // Exporting says where it went.
        h.app.file_event(crate::files::FileEvent::Saved {
            purpose: crate::files::Purpose::ExportStyles,
            name: "my-styles.json".into(),
            target: None,
        });
        assert_eq!(
            h.app.notice.as_deref(),
            Some("Exported My Styles to my-styles.json")
        );
    }

    #[test]
    fn the_right_panel_has_two_tabs_and_shows_errors() {
        let mut h = Harness::new();
        assert_eq!(h.app.right_tab, RightTab::Properties);
        h.click(h.widget("Antimony"));
        h.run(vec![]);
        assert_eq!(h.app.right_tab, RightTab::Antimony);
        assert!(h.access.is_some());
        h.widget("Load Antimony");
        h.click(h.widget("Properties"));
        h.run(vec![]);
        assert_eq!(h.app.right_tab, RightTab::Properties);

        // An Antimony file with errors shows them, on the Antimony tab.
        h.app.file_event(crate::files::FileEvent::Opened {
            purpose: crate::files::Purpose::ImportAntimony,
            name: "bad.ant".into(),
            text: "J1: S1 => ~~~ ;".into(),
            target: None,
        });
        assert_eq!(h.app.right_tab, RightTab::Antimony);
    }

    #[test]
    fn get_antimony_writes_the_diagram_when_asked() {
        let mut h = Harness::new();
        h.app
            .view
            .import_antimony(
                "J1: S1 -> S2; k*S1\nk = 1\nS1.pos = {0, 0}\nS2.pos = {200, 0}",
                &h.app.import_options(),
            )
            .unwrap();
        h.click(h.widget("Antimony"));
        h.run(vec![]);
        h.click(h.widget("Get Antimony"));
        h.run(vec![]);
        assert!(!h.app.antimony.text().contains("/* diagram"));
        h.click(h.widget("Diagram"));
        h.run(vec![]);
        h.click(h.widget("Get Antimony"));
        h.run(vec![]);
        let text = h.app.antimony.text();
        assert!(
            text.contains("/* diagram") && text.contains("\n  S1 at ("),
            "{text}"
        );
        assert!(!text.contains("model.layout"), "{text}");
        // Antimony's own layout statements, alone or with the diagram.
        h.click(h.widget("Layout statements"));
        h.run(vec![]);
        h.click(h.widget("Get Antimony"));
        h.run(vec![]);
        let text = h.app.antimony.text();
        assert!(
            text.contains("model.layout") && text.contains("/* diagram"),
            "{text}"
        );
        h.click(h.widget("Diagram"));
        h.run(vec![]);
        h.click(h.widget("Get Antimony"));
        h.run(vec![]);
        let text = h.app.antimony.text();
        assert!(
            text.contains("model.layout") && !text.contains("/* diagram"),
            "{text}"
        );
    }

    #[test]
    fn exporting_antimony_writes_the_diagram_and_says_what_it_lost() {
        let mut h = Harness::new();
        h.app
            .view
            .import_antimony("J1: S1 -> S2; k*S1\nk = 1", &h.app.import_options())
            .unwrap();
        let mut m = h.app.view.model().clone();
        m.find_species_mut("S1").unwrap().label_offset = Some(pd_core::geometry::pt(0.0, 30.0));
        m.add_text(pd_core::geometry::pt(0.0, 0.0), "two\nlines");
        h.app.view.set_model(m);
        // The drawing as a diagram comment, which keeps the name outside.
        let (text, notes) = h.app.antimony_export(false);
        assert!(
            text.contains("/* diagram") && text.contains("id offset (0, 30)"),
            "{text}"
        );
        assert!(!text.contains("  // Layout\n"), "{text}");
        assert!(
            notes.iter().any(|n| n.contains("written on one line")),
            "{notes:?}"
        );
        // Asked for, Antimony's layout statements too, with what they lose.
        let (text, notes) = h.app.antimony_export(true);
        assert!(
            text.contains("  // Layout\n") && text.contains("/* diagram"),
            "{text}"
        );
        h.app.export_messages = notes;
        h.app.file_event(crate::files::FileEvent::Saved {
            purpose: crate::files::Purpose::ExportAntimony,
            name: "model.ant".into(),
            target: None,
        });
        assert_eq!(h.app.notice.as_deref(), Some("Exported model.ant"));
        assert!(
            h.app
                .warnings
                .iter()
                .any(|w| w.starts_with("Names drawn outside")),
            "{:?}",
            h.app.warnings
        );
        // Exporting is not saving: the document keeps its own name.
        assert_ne!(h.app.doc.name, "model.ant");
    }
}
