//! Opening and saving files (SPEC-Rust §10.1).
//!
//! The one place, with `web.rs`, where code differs between the desktop and
//! the browser. File dialogs in the browser are asynchronous, so every
//! operation reports back through a channel that the app reads each frame
//! ([`Files::poll`]).
//!
//! - **Desktop:** native dialogs; Save writes back to the remembered path.
//! - **Browser:** opening uses a file input. Saving uses the File System
//!   Access API where the browser has it (Chromium), keeping the file handle
//!   so Save writes back to the same file; elsewhere it downloads a copy.

use std::sync::mpsc::{Receiver, Sender, channel};

use eframe::egui;

/// What kind of document a file holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileKind {
    Json,
    Antimony,
    Sbml,
}

impl FileKind {
    /// By extension, and if that doesn't decide it, by content: `{` means
    /// JSON, `<sbml` (after any XML declaration) means SBML, anything else
    /// Antimony (SPEC-Rust §10.1).
    pub fn of(name: &str, text: &str) -> FileKind {
        let ext = name.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase());
        match ext.as_deref() {
            Some("json") => FileKind::Json,
            Some("ant" | "txt" | "antimony") => FileKind::Antimony,
            Some("xml" | "sbml") => FileKind::Sbml,
            _ => {
                let t = text.trim_start_matches('\u{FEFF}').trim_start();
                if t.starts_with('{') {
                    FileKind::Json
                } else if t.starts_with("<?xml") || t.starts_with("<sbml") {
                    FileKind::Sbml
                } else {
                    FileKind::Antimony
                }
            }
        }
    }
}

/// File-dialog filters.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Filter {
    /// Anything PathwayDesigner can open.
    Any,
    Json,
    Antimony,
}

impl Filter {
    fn entries(self) -> &'static [(&'static str, &'static [&'static str])] {
        match self {
            Filter::Any => &[
                (
                    "Diagrams and models",
                    &["json", "ant", "txt", "xml", "sbml"],
                ),
                ("PathwayDesigner diagram", &["json"]),
                ("Antimony", &["ant", "txt"]),
                ("SBML", &["xml", "sbml"]),
            ],
            Filter::Json => &[("PathwayDesigner diagram", &["json"])],
            Filter::Antimony => &[("Antimony", &["ant", "txt"])],
        }
    }
}

/// Where Save writes: a remembered path on the desktop, a file handle (or
/// nothing, meaning download) in the browser.
#[derive(Clone, Debug)]
pub enum SaveTarget {
    #[cfg(not(target_arch = "wasm32"))]
    Path(std::path::PathBuf),
    #[cfg(target_arch = "wasm32")]
    Handle(crate::web::FileHandle),
}

/// Why a file is being opened or saved, so the app knows what to do with it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Purpose {
    /// Open a document (any format).
    Open,
    /// Import Antimony into the current diagram.
    ImportAntimony,
    /// Save the diagram as JSON.
    Save,
    /// Write Antimony.
    ExportAntimony,
    /// A file dropped on the window: open it, after the unsaved-changes prompt.
    Drop,
}

/// A finished file operation.
#[derive(Debug)]
pub enum FileEvent {
    Opened {
        purpose: Purpose,
        name: String,
        text: String,
        /// Where Save can write back to, if the platform knows (the desktop).
        target: Option<SaveTarget>,
    },
    Saved {
        purpose: Purpose,
        name: String,
        /// Where the next Save can write, if the platform allows it.
        target: Option<SaveTarget>,
    },
    Failed(String),
    /// The user closed the dialog.
    Cancelled,
}

pub struct Files {
    tx: Sender<FileEvent>,
    rx: Receiver<FileEvent>,
    /// An operation is waiting for a dialog or a write.
    busy: bool,
}

impl Default for Files {
    fn default() -> Self {
        let (tx, rx) = channel();
        Self {
            tx,
            rx,
            busy: false,
        }
    }
}

impl Files {
    /// The next finished operation, if any.
    pub fn poll(&mut self) -> Option<FileEvent> {
        let event = self.rx.try_recv().ok()?;
        // A drop isn't the dialog operation that may be waiting.
        let drop = matches!(
            event,
            FileEvent::Opened {
                purpose: Purpose::Drop,
                ..
            }
        );
        if !drop {
            self.busy = false;
        }
        Some(event)
    }

    /// Asks for a file to open.
    pub fn open(&mut self, ctx: &egui::Context, purpose: Purpose, filter: Filter) {
        if self.busy {
            return;
        }
        self.busy = true;
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        // Where the browser can (Chromium), its own picker hands back a file
        // handle, so Save writes back to the opened file without asking.
        #[cfg(target_arch = "wasm32")]
        if crate::web::has_file_system_access() {
            wasm_bindgen_futures::spawn_local(async move {
                let event = match crate::web::pick_open_file(filter.entries()).await {
                    Ok(Some((handle, text))) => FileEvent::Opened {
                        purpose,
                        name: handle.name(),
                        text,
                        target: Some(SaveTarget::Handle(handle)),
                    },
                    Ok(None) => FileEvent::Cancelled,
                    Err(e) => FileEvent::Failed(format!("The file couldn't be opened: {e}.")),
                };
                let _ = tx.send(event);
                ctx.request_repaint();
            });
            return;
        }
        let mut dialog = rfd::AsyncFileDialog::new();
        for (name, exts) in filter.entries() {
            dialog = dialog.add_filter(*name, exts);
        }
        spawn(async move {
            let event = match dialog.pick_file().await {
                None => FileEvent::Cancelled,
                Some(file) => {
                    let name = file.file_name();
                    #[cfg(not(target_arch = "wasm32"))]
                    let target = Some(SaveTarget::Path(file.path().to_path_buf()));
                    #[cfg(target_arch = "wasm32")]
                    let target = None;
                    match String::from_utf8(file.read().await) {
                        Ok(text) => FileEvent::Opened {
                            purpose,
                            name,
                            text,
                            target,
                        },
                        Err(_) => FileEvent::Failed(format!("{name} isn't a text file.")),
                    }
                }
            };
            let _ = tx.send(event);
            ctx.request_repaint();
        });
    }

    /// Saves `contents` as `name`. With a `target` and `ask` false, writes
    /// straight back to it; otherwise asks where (or downloads, in browsers
    /// without the File System Access API).
    pub fn save(
        &mut self,
        ctx: &egui::Context,
        purpose: Purpose,
        name: String,
        contents: String,
        target: Option<SaveTarget>,
        ask: bool,
    ) {
        if self.busy {
            return;
        }
        self.busy = true;
        let tx = self.tx.clone();
        let ctx = ctx.clone();
        let filter = match purpose {
            Purpose::ExportAntimony => Filter::Antimony,
            _ => Filter::Json,
        };
        spawn(async move {
            let event = platform_save(purpose, name, contents, target, ask, filter).await;
            let _ = tx.send(event);
            ctx.request_repaint();
        });
    }

    /// Reads any files dropped on the window. They come back from
    /// [`Files::poll`] as `Opened` with [`Purpose::Drop`].
    pub fn take_dropped(&self, ctx: &egui::Context) {
        let dropped = ctx.input(|i| i.raw.dropped_files.clone());
        for f in dropped {
            let name = f.path().file_name().map_or_else(
                || "dropped file".to_owned(),
                |n| n.to_string_lossy().into_owned(),
            );
            let tx = self.tx.clone();
            let ctx = ctx.clone();
            let opened = move |bytes: Result<Vec<u8>, String>, target: Option<SaveTarget>| {
                let event = match bytes.map(String::from_utf8) {
                    Ok(Ok(text)) => FileEvent::Opened {
                        purpose: Purpose::Drop,
                        name,
                        text,
                        target,
                    },
                    Ok(Err(_)) => FileEvent::Failed(format!("{name} isn't a text file.")),
                    Err(e) => FileEvent::Failed(format!("{name} couldn't be read: {e}.")),
                };
                let _ = tx.send(event);
                ctx.request_repaint();
            };
            // The desktop reads at once; a browser can only read asynchronously.
            #[cfg(not(target_arch = "wasm32"))]
            opened(f.bytes(), Some(SaveTarget::Path(f.path().to_path_buf())));
            #[cfg(target_arch = "wasm32")]
            wasm_bindgen_futures::spawn_local(async move {
                let bytes = f.bytes_async().await;
                opened(bytes, None);
            });
        }
    }

    /// How Save behaves in this browser, for the status bar.
    #[cfg(target_arch = "wasm32")]
    pub fn save_mode() -> &'static str {
        if crate::web::has_file_system_access() {
            "Save writes back to the file"
        } else {
            "Save downloads a copy"
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn spawn(f: impl std::future::Future<Output = ()> + Send + 'static) {
    // Native dialogs block; run them off the UI thread.
    std::thread::spawn(move || pollster_block_on(f));
}

#[cfg(target_arch = "wasm32")]
fn spawn(f: impl std::future::Future<Output = ()> + 'static) {
    wasm_bindgen_futures::spawn_local(f);
}

/// Runs a future to completion on this thread. rfd's native futures finish
/// without needing a real executor, so a simple polling loop is enough.
#[cfg(not(target_arch = "wasm32"))]
fn pollster_block_on<F: std::future::Future>(f: F) -> F::Output {
    use std::sync::Arc;
    use std::task::{Context, Poll, Wake, Waker};
    struct Thread(std::thread::Thread);
    impl Wake for Thread {
        fn wake(self: Arc<Self>) {
            self.0.unpark();
        }
    }
    let waker = Waker::from(Arc::new(Thread(std::thread::current())));
    let mut cx = Context::from_waker(&waker);
    let mut f = std::pin::pin!(f);
    loop {
        match f.as_mut().poll(&mut cx) {
            Poll::Ready(v) => return v,
            Poll::Pending => std::thread::park(),
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
async fn platform_save(
    purpose: Purpose,
    name: String,
    contents: String,
    target: Option<SaveTarget>,
    ask: bool,
    filter: Filter,
) -> FileEvent {
    let path = match (target, ask) {
        (Some(SaveTarget::Path(p)), false) => p,
        _ => {
            let mut dialog = rfd::AsyncFileDialog::new().set_file_name(&name);
            for (label, exts) in filter.entries() {
                dialog = dialog.add_filter(*label, exts);
            }
            match dialog.save_file().await {
                Some(h) => h.path().to_path_buf(),
                None => return FileEvent::Cancelled,
            }
        }
    };
    match std::fs::write(&path, contents) {
        Ok(()) => FileEvent::Saved {
            purpose,
            name: path
                .file_name()
                .map_or(name, |n| n.to_string_lossy().into_owned()),
            target: Some(SaveTarget::Path(path)),
        },
        Err(e) => FileEvent::Failed(format!("{} couldn't be saved: {e}.", path.display())),
    }
}

#[cfg(target_arch = "wasm32")]
async fn platform_save(
    purpose: Purpose,
    name: String,
    contents: String,
    target: Option<SaveTarget>,
    ask: bool,
    filter: Filter,
) -> FileEvent {
    use crate::web;
    if !web::has_file_system_access() {
        return match web::download(&name, &contents) {
            Ok(()) => FileEvent::Saved {
                purpose,
                name,
                target: None,
            },
            Err(e) => FileEvent::Failed(format!("The download failed: {e}.")),
        };
    }
    let handle = match (target, ask) {
        (Some(SaveTarget::Handle(h)), false) => h,
        _ => {
            let (label, exts) = filter.entries()[0];
            match web::pick_save_file(&name, label, exts).await {
                Ok(Some(h)) => h,
                Ok(None) => return FileEvent::Cancelled,
                Err(e) => return FileEvent::Failed(format!("Saving failed: {e}.")),
            }
        }
    };
    match web::write_file(&handle, &contents).await {
        Ok(()) => FileEvent::Saved {
            purpose,
            name: handle.name(),
            target: Some(SaveTarget::Handle(handle)),
        },
        Err(e) => FileEvent::Failed(format!("Saving failed: {e}.")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_by_extension_then_content() {
        assert_eq!(FileKind::of("a.json", ""), FileKind::Json);
        assert_eq!(FileKind::of("a.JSON", ""), FileKind::Json);
        assert_eq!(FileKind::of("model.ant", "{"), FileKind::Antimony);
        assert_eq!(FileKind::of("m.txt", ""), FileKind::Antimony);
        assert_eq!(FileKind::of("m.xml", ""), FileKind::Sbml);
        assert_eq!(
            FileKind::of("noext", "\u{FEFF} {\"version\": 3}"),
            FileKind::Json
        );
        assert_eq!(
            FileKind::of("noext", "<?xml version=\"1.0\"?><sbml>"),
            FileKind::Sbml
        );
        assert_eq!(FileKind::of("noext", "<sbml>"), FileKind::Sbml);
        assert_eq!(FileKind::of("noext", "A -> B"), FileKind::Antimony);
    }
}
