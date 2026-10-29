//! The document: opening, saving, unsaved changes, the window title and
//! autosave (S §13.1, M11; SPEC-Rust §10.1, §10.2).

use eframe::egui;
use pd_core::json::{from_json, to_json};
use pd_core::model::Model;
use pd_core::view::Tool;

use crate::app::{Dialog, PathwayApp};
use crate::autosave::{self, Autosave};
use crate::files::{FileEvent, FileKind, Filter, Purpose, SaveTarget};
use crate::samples::SAMPLES;

/// The current document's file.
#[derive(Clone, Debug)]
pub struct Document {
    /// Its file name, shown in the title and suggested when saving.
    pub name: String,
    /// Where Save writes, once it has been saved or opened from a file.
    pub target: Option<SaveTarget>,
    /// Opened from a newer version of the app: Save behaves as Save As, so
    /// the original isn't overwritten without what this version can't keep
    /// (SPEC-Rust §4).
    pub save_as_only: bool,
}

impl Default for Document {
    fn default() -> Self {
        Self {
            name: "Untitled.json".to_owned(),
            target: None,
            save_as_only: false,
        }
    }
}

/// Something that replaces the document, so it may need the user's say-so
/// first.
#[derive(Clone, Debug)]
pub enum Action {
    New,
    Open,
    OpenSample(&'static str),
    /// An example from the Gallery, to be fetched (S §11.8).
    OpenGallery(crate::gallery::Example),
    /// A KEGG map by its code, to be fetched (desktop;
    /// docs/kegg-import-design.md, K6).
    #[cfg(not(target_arch = "wasm32"))]
    OpenKegg(String),
    /// A file already read: dropped on the window.
    OpenText {
        name: String,
        text: String,
        target: Option<SaveTarget>,
    },
    /// A file from File ▸ Open Recent, or the command line (desktop).
    #[cfg(not(target_arch = "wasm32"))]
    OpenPath(std::path::PathBuf),
    Quit,
}

/// `name` with its extension replaced by `ext`.
pub fn with_extension(name: &str, ext: &str) -> String {
    let stem = name.rsplit_once('.').map_or(name, |(s, _)| s);
    format!("{stem}.{ext}")
}

impl PathwayApp {
    /// Does `action`, first asking to save unsaved changes (S §13.1).
    pub(crate) fn request(&mut self, action: Action) {
        if self.view.is_modified() {
            self.dialog = Some(Dialog::ConfirmDiscard(action));
        } else {
            self.perform(action);
        }
    }

    /// Does `action` without asking.
    pub(crate) fn perform(&mut self, action: Action) {
        match action {
            Action::New => {
                self.view.set_model(Model::new());
                self.view.set_tool(Tool::Select);
                self.doc = Document::default();
                self.warnings.clear();
                self.notice = None;
                self.fit_pending = true;
            }
            Action::Open => self.files.open(&self.ctx, Purpose::Open, Filter::Any),
            Action::OpenSample(name) => {
                if let Some((_, text)) = SAMPLES.iter().find(|(n, _)| *n == name) {
                    self.open_text(&format!("{name}.json"), text, None);
                }
            }
            Action::OpenGallery(example) => self.gallery.start(&self.ctx, example),
            #[cfg(not(target_arch = "wasm32"))]
            Action::OpenKegg(code) => {
                let url = pd_core::kegg::kgml_url(&code);
                self.notice = Some(format!("Fetching {code} from KEGG…"));
                self.kegg_fetch = Some((code, (self.gallery.fetch)(&self.ctx, url)));
            }
            Action::OpenText { name, text, target } => {
                self.open_text(&name, &text, target);
            }
            #[cfg(not(target_arch = "wasm32"))]
            Action::OpenPath(path) => self.open_path(&path),
            Action::Quit => {
                self.allow_close = true;
                self.ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }

    /// Opens a file from its path, as a new document (desktop). A file that
    /// can no longer be read is taken off the recent list.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn open_path(&mut self, path: &std::path::Path) {
        let name = path.file_name().map_or_else(
            || path.display().to_string(),
            |n| n.to_string_lossy().into_owned(),
        );
        match std::fs::read_to_string(path) {
            Ok(text) => {
                self.open_text(&name, &text, Some(SaveTarget::Path(path.to_path_buf())));
            }
            Err(e) => {
                self.recent.remove(path);
                self.warnings = vec![format!("{} couldn't be opened: {e}.", path.display())];
            }
        }
    }

    /// Opens a file's text as the document, by its kind. A file that can't
    /// be read leaves the current diagram as it was (M11). Returns whether it
    /// opened. On the desktop, a file opened from its path goes on the recent
    /// list.
    pub(crate) fn open_text(&mut self, name: &str, text: &str, target: Option<SaveTarget>) -> bool {
        #[cfg(not(target_arch = "wasm32"))]
        let path = target.as_ref().map(|SaveTarget::Path(p)| p.clone());
        let kind = FileKind::of(name, text);
        let opened = match kind {
            FileKind::Json => match from_json(text) {
                Ok(loaded) => {
                    let newer = loaded.is_newer_than_supported();
                    self.view.set_model(loaded.model);
                    self.doc = Document {
                        name: name.to_owned(),
                        target,
                        save_as_only: newer,
                    };
                    self.warnings = loaded.warnings;
                    self.notice = Some(format!("Opened {name}"));
                    self.fit_pending = true;
                    true
                }
                Err(e) => {
                    self.warnings = vec![format!(
                        "{name} couldn't be opened ({e}). The diagram wasn't changed."
                    )];
                    false
                }
            },
            FileKind::Antimony => {
                let opts = self.import_options();
                if self.antimony.load(text, &mut self.view, &opts) {
                    // Opening starts a new document: no undo back into the old one.
                    let model = self.view.model().clone();
                    self.view.set_model(model);
                    self.view.mark_unsaved();
                    self.doc = Document {
                        name: with_extension(name, "json"),
                        ..Document::default()
                    };
                    self.warnings.clear();
                    self.notice = Some(format!("Opened {name}"));
                    self.fit_pending = true;
                    true
                } else {
                    self.warnings = vec![format!(
                        "{name} has errors, listed in the Antimony panel. The diagram wasn't changed."
                    )];
                    self.right_tab = crate::app::RightTab::Antimony;
                    false
                }
            }
            FileKind::Escher => match self.view.import_escher(text) {
                Ok(warnings) => {
                    // Opening starts a new document: no undo back into the old one.
                    let model = self.view.model().clone();
                    self.view.set_model(model);
                    self.view.mark_unsaved();
                    // Saved as a PathwayDesigner file, never over the map.
                    let stem = name.strip_suffix(".json").unwrap_or(name);
                    self.doc = Document {
                        name: format!("{stem} (from Escher).json"),
                        ..Document::default()
                    };
                    self.warnings = warnings;
                    self.notice = Some(format!("Opened the Escher map {name}"));
                    self.fit_pending = true;
                    true
                }
                Err(e) => {
                    self.warnings = vec![format!(
                        "{name} couldn't be read as an Escher map ({e}). The diagram wasn't changed."
                    )];
                    false
                }
            },
            FileKind::Kegg => match self.view.import_kegg(text) {
                Ok(map) => {
                    // Opening starts a new document: no undo back into the old one.
                    let model = self.view.model().clone();
                    self.view.set_model(model);
                    self.view.mark_unsaved();
                    // Saved as a PathwayDesigner file, never over the map.
                    self.doc = Document {
                        name: format!("{} (from KEGG).json", map.code),
                        ..Document::default()
                    };
                    self.warnings = map.warnings.clone();
                    self.naming = crate::kegg_names::Naming::new(&map);
                    self.notice = Some(format!(
                        "Opened the KEGG map {} ({}); looking up the compounds' names in PubChem…",
                        map.title, map.code
                    ));
                    self.fit_pending = true;
                    true
                }
                Err(e) => {
                    self.warnings = vec![format!("{name}: {e} The diagram wasn't changed.")];
                    false
                }
            },
            FileKind::Sbml => {
                if let Some(had_layout) = self.import_sbml(name, text) {
                    // Opening starts a new document: no undo back into the old one.
                    let model = self.view.model().clone();
                    self.view.set_model(model);
                    self.view.mark_unsaved();
                    self.doc = Document {
                        name: with_extension(name, "json"),
                        ..Document::default()
                    };
                    self.notice = Some(format!("Opened {name}"));
                    // Without a layout every species is at random: lay it out.
                    if !had_layout {
                        self.start_layout();
                    }
                    true
                } else {
                    false
                }
            }
        };
        // Another file opened: a KEGG map's names no longer belong to it.
        if opened && kind != FileKind::Kegg {
            self.naming = None;
        }
        // What a file needs when opened is no news (an imported one is
        // marked unsaved, but it wasn't edited).
        if opened {
            self.needed_version = pd_core::json::needed_version(self.view.model());
        }
        #[cfg(not(target_arch = "wasm32"))]
        if let (true, Some(p)) = (opened, path) {
            self.recent.add(&p);
        }
        opened
    }

    /// Takes the lookup of a KEGG map's compound names a step further, and
    /// names the compounds when it is over (docs/kegg-import-design.md, K2).
    pub(crate) fn poll_naming(&mut self, ctx: &egui::Context) {
        use crate::kegg_names::Done;
        let Some(mut naming) = self.naming.take() else {
            return;
        };
        let Some(done) = naming.poll(ctx, self.gallery.fetch) else {
            self.naming = Some(naming);
            return;
        };
        let total = naming.compounds().len();
        self.notice = Some(match done {
            Done::Found(names, cids) => {
                let n = self
                    .view
                    .name_kegg_compounds(naming.compounds(), &names, &cids);
                if n == total {
                    format!("Named the {n} compounds from PubChem")
                } else {
                    format!(
                        "Named {n} of the {total} compounds from PubChem; the rest keep their KEGG numbers"
                    )
                }
            }
            Done::Unreachable => {
                "PubChem couldn't be reached, so the compounds keep their KEGG numbers".to_owned()
            }
        });
        ctx.request_repaint();
    }

    /// Save, or Save As with `ask` (or always for a file from a newer version).
    pub(crate) fn save(&mut self, ask: bool) {
        let name = with_extension(&self.doc.name, "json");
        let json = to_json(self.view.model());
        self.files.save(
            &self.ctx,
            Purpose::Save,
            name,
            json.into_bytes(),
            self.doc.target.clone(),
            ask || self.doc.save_as_only,
        );
    }

    /// Saves a matrix or list from the Structure tab as CSV.
    pub(crate) fn export_csv(&mut self, name: String, csv: String) {
        self.files.save(
            &self.ctx,
            Purpose::ExportCsv,
            name,
            csv.into_bytes(),
            None,
            true,
        );
    }

    /// File ▸ Export Antimony, and with `layout` the same with Antimony's
    /// layout statements too.
    pub(crate) fn export_antimony(&mut self, layout: bool) {
        let name = with_extension(&self.doc.name, "ant");
        let (text, notes) = self.antimony_export(layout);
        self.export_messages = notes;
        self.files.save(
            &self.ctx,
            Purpose::ExportAntimony,
            name,
            text.into_bytes(),
            None,
            true,
        );
    }

    /// What File ▸ Export Antimony writes: the model and its drawing as a
    /// diagram comment, with Antimony's layout statements too if asked (D4 of
    /// docs/diagram-format-implementation-design.md), and the notes on what
    /// the export loses.
    pub(crate) fn antimony_export(&self, layout: bool) -> (String, Vec<String>) {
        self.view.export_antimony_with_diagram(layout)
    }

    /// Replaces the diagram with the model in SBML `text`, as one undo step.
    /// Returns whether the file had a layout, or `None`, with the diagram
    /// unchanged, if it can't be read.
    fn import_sbml(&mut self, name: &str, text: &str) -> Option<bool> {
        let opts = self.import_options();
        match self.view.import_sbml(text, &opts) {
            Ok((_, imported)) => {
                self.warnings = imported.warnings;
                self.fit_pending = true;
                Some(imported.had_layout)
            }
            Err(errors) => {
                let first = errors.first().map(|e| e.to_string()).unwrap_or_default();
                self.warnings = vec![format!(
                    "{name} couldn't be read as SBML ({first}). The diagram wasn't changed."
                )];
                None
            }
        }
    }

    /// Writes the diagram as SBML. Problems and notes about the file are
    /// shown once it is written.
    pub(crate) fn export_sbml(&mut self) {
        let name = with_extension(&self.doc.name, "xml");
        let exported = self.view.export_sbml();
        self.export_messages = exported
            .problems
            .into_iter()
            .chain(exported.notes)
            .collect();
        self.files.save(
            &self.ctx,
            Purpose::ExportSbml,
            name,
            exported.xml.into_bytes(),
            None,
            true,
        );
    }

    /// File ▸ Open from KEGG… with map `code` (docs/kegg-import-design.md,
    /// K6). The desktop fetches the map and opens it. A web page may not read
    /// KEGG's files, so the browser opens the map's file in a new tab, to be
    /// saved and opened with File ▸ Open.
    pub(crate) fn open_from_kegg(&mut self, code: String) {
        #[cfg(not(target_arch = "wasm32"))]
        self.request(Action::OpenKegg(code));
        #[cfg(target_arch = "wasm32")]
        {
            self.ctx
                .open_url(egui::OpenUrl::new_tab(pd_core::kegg::kgml_url(&code)));
            self.notice = Some(format!(
                "KEGG's file for {code} is in a new tab: save it there (Ctrl+S), then open it with File ▸ Open"
            ));
        }
    }

    /// Handles finished file operations and dropped files. A file just read
    /// is opened on the next frame, so this one can show the busy cursor and
    /// say what is being opened first: reading a big file blocks the browser
    /// until it is done.
    pub(crate) fn handle_files(&mut self) {
        if let Some(result) = self
            .kegg_fetch
            .as_ref()
            .and_then(|(_, pending)| crate::gallery::take(pending))
        {
            let (code, _) = self.kegg_fetch.take().expect("checked");
            match result {
                Ok(text) => {
                    self.open_text(&format!("{code}.xml"), &text, None);
                }
                Err(why) => {
                    self.notice = None;
                    self.warnings = vec![format!(
                        "{code} couldn't be fetched from KEGG ({why}). The diagram wasn't changed."
                    )];
                }
            }
        }
        if let Some((example, text)) = self.gallery.poll() {
            if self.open_text(example.name(), &text, None) {
                self.gallery.open = false;
            } else {
                let why = self.warnings.first().cloned().unwrap_or_default();
                self.gallery.failed(&example, &why);
            }
        }
        if let Some(event) = self.opening.take() {
            self.file_event(event);
        }
        self.files.take_dropped(&self.ctx);
        while let Some(event) = self.files.poll() {
            match event {
                FileEvent::Opened { ref name, .. } if self.opening.is_none() => {
                    self.notice = Some(format!("Opening {name}…"));
                    self.opening = Some(event);
                    self.ctx.request_repaint();
                }
                event => self.file_event(event),
            }
        }
    }

    /// True while a file read is waiting to be opened on the next frame.
    pub(crate) fn is_opening(&self) -> bool {
        self.opening.is_some()
    }

    /// After the diagram was replaced by a file's model (Import Antimony or
    /// Import SBML), the document takes that file's name, so the status bar
    /// and the title show it, and Save asks where to save rather than writing
    /// the imported model over the previous file. The import stays one undo
    /// step.
    fn imported_from(&mut self, name: &str) {
        self.doc = Document {
            name: with_extension(name, "json"),
            ..Document::default()
        };
        self.notice = Some(format!("Imported {name}"));
        // What an imported file needs is no news either.
        self.needed_version = pd_core::json::needed_version(self.view.model());
    }

    pub(crate) fn file_event(&mut self, event: FileEvent) {
        match event {
            FileEvent::Opened {
                purpose: Purpose::ImportAntimony,
                name,
                text,
                ..
            } => {
                let opts = self.import_options();
                if self.antimony.load(&text, &mut self.view, &opts) {
                    self.imported_from(&name);
                    self.warnings.clear();
                    self.fit_pending = true;
                } else {
                    self.warnings =
                        vec![format!("{name} has errors, listed in the Antimony panel.")];
                    self.right_tab = crate::app::RightTab::Antimony;
                }
            }
            FileEvent::Opened {
                purpose: Purpose::ImportSbml,
                name,
                text,
                ..
            } => {
                if let Some(had_layout) = self.import_sbml(&name, &text) {
                    self.imported_from(&name);
                    if !had_layout {
                        self.start_layout();
                    }
                }
            }
            FileEvent::Opened {
                purpose: Purpose::ImportStyles,
                name,
                text,
                ..
            } => self.import_styles(&name, &text),
            FileEvent::Opened {
                purpose: Purpose::Drop,
                name,
                text,
                target,
            } => self.request(Action::OpenText { name, text, target }),
            FileEvent::Opened {
                name, text, target, ..
            } => {
                self.open_text(&name, &text, target);
            }
            FileEvent::Saved {
                purpose: Purpose::ExportAntimony,
                name,
                ..
            } => {
                self.notice = Some(format!("Exported {name}"));
                self.warnings = std::mem::take(&mut self.export_messages);
            }
            FileEvent::Saved {
                purpose:
                    Purpose::ExportCsv | Purpose::ExportSvg | Purpose::ExportPng | Purpose::ExportPdf,
                name,
                ..
            } => {
                self.notice = Some(format!("Exported {name}"));
            }
            FileEvent::Saved {
                purpose: Purpose::ExportStyles,
                name,
                ..
            } => {
                self.notice = Some(format!("Exported My Styles to {name}"));
            }
            FileEvent::Saved {
                purpose: Purpose::ExportSbml,
                name,
                ..
            } => {
                self.notice = Some(format!("Exported {name}"));
                self.warnings = std::mem::take(&mut self.export_messages);
            }
            FileEvent::Saved { name, target, .. } => {
                #[cfg(not(target_arch = "wasm32"))]
                if let Some(SaveTarget::Path(p)) = &target {
                    self.recent.add(p);
                }
                self.view.mark_saved();
                self.doc.name = name.clone();
                if target.is_some() {
                    self.doc.target = target;
                }
                self.doc.save_as_only = false;
                self.notice = Some(format!("Saved {name}"));
                if let Some(action) = self.after_save.take() {
                    self.perform(action);
                }
            }
            FileEvent::Failed(message) => {
                self.warnings = vec![message];
                self.after_save = None;
            }
            FileEvent::Cancelled => {
                self.after_save = None;
                self.export_messages.clear();
            }
        }
    }

    /// Export My Styles… (S §8.6): writes My Styles as a style list.
    pub(crate) fn export_styles(&mut self) {
        let text = pd_core::json::styles_to_json(self.view.my_styles());
        self.files.save(
            &self.ctx,
            Purpose::ExportStyles,
            "my-styles.json".to_owned(),
            text.into_bytes(),
            None,
            true,
        );
    }

    /// Import Styles… (S §8.6): adds the styles in `text`, a style list or a
    /// diagram, to My Styles, and says what happened.
    fn import_styles(&mut self, name: &str, text: &str) {
        match pd_core::json::styles_from_json(text) {
            Err(e) => {
                self.warnings = vec![format!("{name} couldn't be read as styles: {e}.")];
            }
            Ok((styles, warnings)) => {
                self.warnings = warnings;
                if styles.is_empty() {
                    self.notice = Some(format!("{name} has no saved styles"));
                    return;
                }
                let r = self.view.import_my_styles(styles);
                let count = |n: usize| match n {
                    1 => "1 style".to_owned(),
                    n => format!("{n} styles"),
                };
                let mut said = format!("Imported {} from {name}", count(r.added));
                if r.already > 0 {
                    let verb = if r.already == 1 { "was" } else { "were" };
                    said.push_str(&format!(
                        "; {} {verb} already in My Styles",
                        count(r.already)
                    ));
                }
                if !r.renamed.is_empty() {
                    let names: Vec<String> = r
                        .renamed
                        .iter()
                        .map(|(from, to)| format!("{from} as {to}"))
                        .collect();
                    said.push_str(&format!("; added {}", names.join(", ")));
                }
                self.notice = Some(said);
                self.right_tab = crate::app::RightTab::Styles;
            }
        }
    }

    /// The window title: the file name, marked while there are unsaved changes.
    pub(crate) fn update_title(&mut self) {
        let mark = if self.view.is_modified() { "*" } else { "" };
        let title = format!("{}{mark} — PathwayDesigner", self.doc.name);
        if title == self.last_title {
            return;
        }
        #[cfg(target_arch = "wasm32")]
        crate::web::set_title(&title);
        #[cfg(not(target_arch = "wasm32"))]
        self.ctx
            .send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
        self.last_title = title;
    }

    /// Closing the window with unsaved changes asks first (M11).
    pub(crate) fn check_close(&mut self) {
        #[cfg(target_arch = "wasm32")]
        crate::web::set_modified(self.view.is_modified());
        let close = self.ctx.input(|i| i.viewport().close_requested());
        if close && self.view.is_modified() && !self.allow_close {
            self.ctx
                .send_viewport_cmd(egui::ViewportCommand::CancelClose);
            if self.dialog.is_none() {
                self.dialog = Some(Dialog::ConfirmDiscard(Action::Quit));
            }
        }
    }

    /// Writes the autosave while there are unsaved changes, and removes it
    /// once there are none (SPEC-Rust §10.2).
    pub(crate) fn autosave(&self, storage: &mut dyn eframe::Storage) {
        if self.view.is_modified() && !self.view.is_laying_out() {
            let a = Autosave {
                name: self.doc.name.clone(),
                time: autosave::now(),
                json: to_json(self.view.model()),
            };
            storage.set_string(autosave::KEY, a.encode());
        } else {
            storage.remove_string(autosave::KEY);
        }
    }

    /// Puts a recovered autosave in place, as an unsaved document.
    pub(crate) fn recover(&mut self, a: &Autosave) {
        match from_json(&a.json) {
            Ok(loaded) => {
                self.view.set_model(loaded.model);
                self.view.mark_unsaved();
                self.doc = Document {
                    name: a.name.clone(),
                    ..Document::default()
                };
                self.notice = Some(format!("Recovered {}", a.name));
                self.fit_pending = true;
            }
            Err(e) => self.warnings = vec![format!("The autosave couldn't be read ({e}).")],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extensions() {
        assert_eq!(with_extension("model.ant", "json"), "model.json");
        assert_eq!(with_extension("a.b.json", "ant"), "a.b.ant");
        assert_eq!(with_extension("noext", "json"), "noext.json");
    }
}
