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
    /// A file already read: dropped on the window.
    OpenText {
        name: String,
        text: String,
        target: Option<SaveTarget>,
    },
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
            Action::OpenText { name, text, target } => self.open_text(&name, &text, target),
            Action::Quit => {
                self.allow_close = true;
                self.ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }

    /// Opens a file's text as the document, by its kind. A file that can't
    /// be read leaves the current diagram as it was (M11).
    pub(crate) fn open_text(&mut self, name: &str, text: &str, target: Option<SaveTarget>) {
        match FileKind::of(name, text) {
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
                }
                Err(e) => {
                    self.warnings = vec![format!(
                        "{name} couldn't be opened ({e}). The diagram wasn't changed."
                    )];
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
                } else {
                    self.warnings = vec![format!(
                        "{name} has errors, listed in the Antimony panel. The diagram wasn't changed."
                    )];
                }
            }
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
                }
            }
        }
    }

    /// Save, or Save As with `ask` (or always for a file from a newer version).
    pub(crate) fn save(&mut self, ask: bool) {
        let name = with_extension(&self.doc.name, "json");
        let json = to_json(self.view.model());
        self.files.save(
            &self.ctx,
            Purpose::Save,
            name,
            json,
            self.doc.target.clone(),
            ask || self.doc.save_as_only,
        );
    }

    pub(crate) fn export_antimony(&mut self) {
        let name = with_extension(&self.doc.name, "ant");
        let text = self.view.export_antimony();
        self.files
            .save(&self.ctx, Purpose::ExportAntimony, name, text, None, true);
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
            exported.xml,
            None,
            true,
        );
    }

    /// Handles finished file operations and dropped files.
    pub(crate) fn handle_files(&mut self) {
        self.files.take_dropped(&self.ctx);
        while let Some(event) = self.files.poll() {
            self.file_event(event);
        }
    }

    fn file_event(&mut self, event: FileEvent) {
        match event {
            FileEvent::Opened {
                purpose: Purpose::ImportAntimony,
                name,
                text,
                ..
            } => {
                let opts = self.import_options();
                if self.antimony.load(&text, &mut self.view, &opts) {
                    self.notice = Some(format!("Imported {name}"));
                    self.warnings.clear();
                    self.fit_pending = true;
                } else {
                    self.warnings =
                        vec![format!("{name} has errors, listed in the Antimony panel.")];
                }
            }
            FileEvent::Opened {
                purpose: Purpose::ImportSbml,
                name,
                text,
                ..
            } => {
                if let Some(had_layout) = self.import_sbml(&name, &text) {
                    self.notice = Some(format!("Imported {name}"));
                    if !had_layout {
                        self.start_layout();
                    }
                }
            }
            FileEvent::Opened {
                purpose: Purpose::Drop,
                name,
                text,
                target,
            } => self.request(Action::OpenText { name, text, target }),
            FileEvent::Opened {
                name, text, target, ..
            } => self.open_text(&name, &text, target),
            FileEvent::Saved {
                purpose: Purpose::ExportAntimony,
                name,
                ..
            } => {
                self.notice = Some(format!("Exported {name}"));
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
