//! Cut, Copy and Paste of the diagram on the system clipboard
//! (docs/cut-copy-paste-design.md).
//!
//! Ctrl+X, Ctrl+C and Ctrl+V arrive as egui's Cut, Copy and Paste events (on
//! the web, from the browser's own clipboard events), so they reach the
//! system clipboard on both. The Edit menu and the context menu have no key
//! press behind them: on the desktop they ask eframe to read the clipboard,
//! which then sends a Paste event; on the web they read it through the
//! browser, and use the app's own last copy if the browser won't give it.

use eframe::egui;
use pd_core::geometry::Pt;

/// How long a paste asked for by a menu waits for the clipboard's text, in
/// seconds; after that a Paste event is taken as Ctrl+V.
const WAIT: f64 = 2.0;

/// What to do this frame.
pub enum Command {
    Cut,
    Copy,
    /// Paste `text`, centred on `at` (screen) or, without it, a step from
    /// where it was copied.
    Paste {
        text: String,
        at: Option<Pt>,
    },
}

pub struct Clipboard {
    /// The last copy made in this window.
    last: Option<String>,
    /// A paste a menu asked for: where it goes, and when it was asked (egui
    /// time).
    waiting: Option<(Option<Pt>, f64)>,
    /// The browser's answer to a read: its text, or `None` if it refused.
    #[cfg(target_arch = "wasm32")]
    read: std::rc::Rc<std::cell::RefCell<Option<Option<String>>>>,
}

impl Clipboard {
    pub fn new() -> Self {
        Self {
            last: None,
            waiting: None,
            #[cfg(target_arch = "wasm32")]
            read: Default::default(),
        }
    }

    /// Puts `text` on the clipboard.
    pub fn put(&mut self, ctx: &egui::Context, text: String) {
        ctx.copy_text(text.clone());
        self.last = Some(text);
    }

    /// Paste from a menu: asks for the clipboard's text, which
    /// [`Self::commands`] gives once it comes.
    pub fn request_paste(&mut self, ctx: &egui::Context, at: Option<Pt>) {
        self.waiting = Some((at, ctx.input(|i| i.time)));
        #[cfg(not(target_arch = "wasm32"))]
        ctx.send_viewport_cmd(egui::ViewportCommand::RequestPaste);
        #[cfg(target_arch = "wasm32")]
        {
            let slot = self.read.clone();
            let ctx = ctx.clone();
            wasm_bindgen_futures::spawn_local(async move {
                let text = match web_sys::window() {
                    Some(w) => {
                        let promise = w.navigator().clipboard().read_text();
                        wasm_bindgen_futures::JsFuture::from(promise)
                            .await
                            .ok()
                            .and_then(|v| v.as_string())
                    }
                    None => None,
                };
                *slot.borrow_mut() = Some(text);
                ctx.request_repaint();
            });
        }
    }

    /// This frame's commands: from the keys when `keys` (no text field has
    /// the keyboard), pasting at `pointer`; and a menu's paste once its text
    /// has come.
    pub fn commands(
        &mut self,
        ctx: &egui::Context,
        keys: bool,
        pointer: Option<Pt>,
    ) -> Vec<Command> {
        let now = ctx.input(|i| i.time);
        if self.waiting.is_some_and(|(_, since)| now - since > WAIT) {
            self.waiting = None;
        }
        let mut out = Vec::new();
        #[cfg(target_arch = "wasm32")]
        if let Some(read) = self.read.borrow_mut().take()
            && let Some((at, _)) = self.waiting.take()
            && let Some(text) = read.or_else(|| self.last.clone())
        {
            out.push(Command::Paste { text, at });
        }
        for event in ctx.input(|i| i.events.clone()) {
            match event {
                egui::Event::Cut if keys => out.push(Command::Cut),
                egui::Event::Copy if keys => out.push(Command::Copy),
                egui::Event::Paste(text) => {
                    if let Some((at, _)) = self.waiting.take() {
                        out.push(Command::Paste { text, at });
                    } else if keys {
                        out.push(Command::Paste { text, at: pointer });
                    }
                }
                _ => {}
            }
        }
        out
    }
}
