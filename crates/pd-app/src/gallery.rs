//! The Gallery (S §11.8): example models kept in the repository, not in the
//! app, listed by `Gallery/index.json` and fetched from `main` when wanted.

use std::sync::{Arc, Mutex};

use eframe::egui::{self, RichText};
use pd_core::json::FORMAT_VERSION;
use serde::Deserialize;

/// Where the index and the files are read from: the repository's `main`, or
/// for trying out a Gallery before it is pushed, wherever `PD_GALLERY_URL`
/// said when the app was built (ending in `/`).
pub const BASE_URL: &str = match option_env!("PD_GALLERY_URL") {
    Some(url) => url,
    None => "https://raw.githubusercontent.com/sys-bio/PathwayDesigner-rs/main/",
};

/// The index, from the repository's root.
pub const INDEX: &str = "Gallery/index.json";

/// One entry of the index.
#[derive(Clone, Debug, Deserialize, PartialEq)]
pub struct Example {
    /// A path from the repository's root.
    pub file: String,
    /// The heading it is listed under.
    pub group: String,
    pub title: String,
    #[serde(default)]
    pub description: String,
    /// The format version the file needs, or 0 for a file that isn't
    /// PathwayDesigner JSON.
    pub version: u32,
}

impl Example {
    /// The file's URL: its path with anything outside a URL's plain
    /// characters (a space, say) percent-encoded.
    pub fn url(&self) -> String {
        let mut url = BASE_URL.to_owned();
        for b in self.file.bytes() {
            if b.is_ascii_alphanumeric() || b"-._~/".contains(&b) {
                url.push(b as char);
            } else {
                url.push_str(&format!("%{b:02X}"));
            }
        }
        url
    }

    /// The file's name, which decides how it is opened (by its extension).
    pub fn name(&self) -> &str {
        self.file.rsplit('/').next().unwrap_or(&self.file)
    }

    /// True if it needs a newer PathwayDesigner than this one.
    pub fn too_new(&self) -> bool {
        self.version > FORMAT_VERSION
    }
}

/// The index, as read.
#[derive(Debug, Default, PartialEq)]
pub struct Index {
    pub examples: Vec<Example>,
    /// Entries that couldn't be read, left out.
    pub skipped: usize,
}

impl Index {
    /// The index's text. An entry that can't be read, or has no file, group or
    /// title, is skipped and the others kept; only an index that isn't an
    /// object with an `examples` list fails.
    pub fn parse(text: &str) -> Result<Index, String> {
        let value: serde_json::Value = serde_json::from_str(text.trim_start_matches('\u{FEFF}'))
            .map_err(|e| format!("the index isn't JSON ({e})"))?;
        let Some(entries) = value.get("examples").and_then(|e| e.as_array()) else {
            return Err("the index has no \"examples\" list".to_owned());
        };
        let mut index = Index::default();
        for entry in entries {
            match Example::deserialize(entry) {
                Ok(e) if !e.file.is_empty() && !e.group.is_empty() && !e.title.is_empty() => {
                    index.examples.push(e);
                }
                _ => index.skipped += 1,
            }
        }
        Ok(index)
    }

    /// The examples under their groups, the groups in the order they first
    /// appear.
    pub fn groups(&self) -> Vec<(&str, Vec<&Example>)> {
        let mut groups: Vec<(&str, Vec<&Example>)> = Vec::new();
        for e in &self.examples {
            match groups.iter_mut().find(|(g, _)| *g == e.group) {
                Some((_, list)) => list.push(e),
                None => groups.push((&e.group, vec![e])),
            }
        }
        groups
    }
}

/// A download under way, filled in when it is done.
pub type Pending = Arc<Mutex<Option<Result<String, String>>>>;

/// Fetches `url` (in the browser and on the desktop alike), waking the app
/// when it is done.
pub fn fetch(ctx: &egui::Context, url: String) -> Pending {
    let pending = Pending::default();
    let (slot, ctx) = (pending.clone(), ctx.clone());
    ehttp::fetch(ehttp::Request::get(url), move |result| {
        let result = result.and_then(|r| {
            if r.ok {
                String::from_utf8(r.bytes).map_err(|_| "it isn't text".to_owned())
            } else {
                Err(failure(r.status))
            }
        });
        *slot.lock().unwrap() = Some(result);
        ctx.request_repaint();
    });
    pending
}

/// A failed download's HTTP status, in words. Browsers often give no reason
/// text with it, so it is not used.
fn failure(status: u16) -> String {
    match status {
        404 => "not found (404)".to_owned(),
        s => format!("the server said {s}"),
    }
}

fn take(pending: &Pending) -> Option<Result<String, String>> {
    pending.lock().unwrap().take()
}

/// The index, as far as it has got.
enum Listing {
    NotFetched,
    Fetching(Pending),
    Ready(Index),
    Failed(String),
}

/// File ▸ Open from Gallery…: the window and the downloads.
pub struct Gallery {
    /// The window is open.
    pub open: bool,
    listing: Listing,
    /// The example being fetched, to be opened when it arrives.
    file: Option<(Example, Pending)>,
    /// What went wrong last, shown in the window.
    message: Option<String>,
    /// How to fetch: [`fetch`], or the tests' own.
    pub(crate) fetch: fn(&egui::Context, String) -> Pending,
}

impl Default for Gallery {
    fn default() -> Self {
        Self {
            open: false,
            listing: Listing::NotFetched,
            file: None,
            message: None,
            fetch,
        }
    }
}

impl Gallery {
    /// True while something is being fetched: the busy cursor.
    pub fn busy(&self) -> bool {
        self.file.is_some() || (self.open && matches!(self.listing, Listing::Fetching(_)))
    }

    /// Starts fetching `example`'s file.
    pub fn start(&mut self, ctx: &egui::Context, example: Example) {
        self.message = None;
        let pending = (self.fetch)(ctx, example.url());
        self.file = Some((example, pending));
    }

    /// The file being fetched, once it has arrived. If it couldn't be
    /// fetched, the window says why.
    pub fn poll(&mut self) -> Option<(Example, String)> {
        let result = take(&self.file.as_ref()?.1)?;
        let (example, _) = self.file.take()?;
        match result {
            Ok(text) => Some((example, text)),
            Err(why) => {
                self.failed(&example, &why);
                None
            }
        }
    }

    /// Says in the window that `example` couldn't be opened, and why.
    pub fn failed(&mut self, example: &Example, why: &str) {
        self.message = Some(format!("Couldn't open {}: {why}", example.title));
        self.open = true;
    }

    /// The window, while it is open. Returns the example whose Open was
    /// clicked.
    pub fn show(&mut self, ctx: &egui::Context) -> Option<Example> {
        if !self.open {
            return None;
        }
        if matches!(self.listing, Listing::NotFetched) {
            self.listing = Listing::Fetching((self.fetch)(ctx, format!("{BASE_URL}{INDEX}")));
        }
        if let Listing::Fetching(pending) = &self.listing
            && let Some(result) = take(pending)
        {
            self.listing = match result.and_then(|text| Index::parse(&text)) {
                Ok(index) => Listing::Ready(index),
                Err(why) => Listing::Failed(why),
            };
        }
        let (mut chosen, mut refresh) = (None, false);
        let fetching = self.file.as_ref().map(|(e, _)| e.title.clone());
        let mut open = self.open;
        egui::Window::new("Gallery")
            .open(&mut open)
            .resizable(true)
            .default_size([520.0, 560.0])
            .show(ctx, |ui| {
                ui.label(
                    RichText::new("Example models from the PathwayDesigner repository on GitHub.")
                        .weak(),
                );
                if let Some(message) = &self.message {
                    ui.colored_label(ui.visuals().warn_fg_color, message);
                }
                if let Some(title) = &fetching {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(format!("Fetching {title}…"));
                    });
                }
                match &self.listing {
                    Listing::NotFetched | Listing::Fetching(_) => {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label("Fetching the Gallery…");
                        });
                    }
                    Listing::Failed(why) => {
                        ui.colored_label(
                            ui.visuals().warn_fg_color,
                            format!("Couldn't reach the Gallery: {why}"),
                        );
                    }
                    Listing::Ready(index) => {
                        if index.skipped > 0 {
                            ui.label(
                                RichText::new(format!(
                                    "{} entries of the Gallery couldn't be read.",
                                    index.skipped
                                ))
                                .weak(),
                            );
                        }
                        egui::ScrollArea::vertical()
                            .max_height((ui.available_height() - 36.0).max(100.0))
                            .show(ui, |ui| {
                                for (group, examples) in index.groups() {
                                    ui.add_space(6.0);
                                    ui.label(RichText::new(group).strong());
                                    for e in examples {
                                        if entry(ui, e, fetching.is_none()) {
                                            chosen = Some(e.clone());
                                        }
                                    }
                                }
                            });
                    }
                }
                ui.separator();
                refresh = ui
                    .add_enabled(
                        !matches!(self.listing, Listing::Fetching(_)),
                        egui::Button::new("Refresh"),
                    )
                    .clicked();
            });
        self.open = open;
        if refresh {
            self.message = None;
            self.listing = Listing::NotFetched;
        }
        chosen
    }
}

/// One example: its Open button, title and description. Returns whether
/// Open was clicked.
fn entry(ui: &mut egui::Ui, e: &Example, enabled: bool) -> bool {
    let mut clicked = false;
    ui.horizontal(|ui| {
        clicked = ui
            .add_enabled(enabled && !e.too_new(), egui::Button::new("Open"))
            .clicked();
        ui.vertical(|ui| {
            ui.label(&e.title);
            if e.too_new() {
                ui.label(RichText::new("Needs a newer PathwayDesigner").weak());
            } else if !e.description.is_empty() {
                ui.add(egui::Label::new(RichText::new(&e.description).weak()).wrap());
            }
        });
    });
    clicked
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::files::FileKind;

    fn example(file: &str, group: &str) -> Example {
        Example {
            file: file.into(),
            group: group.into(),
            title: "t".into(),
            description: String::new(),
            version: 3,
        }
    }

    #[test]
    fn entries_that_cant_be_read_are_skipped() {
        let index = Index::parse(
            r#"{ "examples": [
                { "file": "a.json", "group": "G", "title": "A", "version": 3 },
                { "file": "b.json", "group": "G", "title": "B" },
                { "file": "", "group": "G", "title": "C", "version": 3 },
                { "file": "d.json", "group": "", "title": "D", "version": 3 },
                { "file": "e.json", "group": "G", "title": "", "version": 3 },
                "f.json",
                { "file": "g.json", "group": "H", "title": "G", "description": "d", "version": 99 }
            ] }"#,
        )
        .unwrap();
        assert_eq!(index.skipped, 5);
        let files: Vec<&str> = index.examples.iter().map(|e| e.file.as_str()).collect();
        assert_eq!(files, ["a.json", "g.json"]);
        assert_eq!(index.examples[0].description, "", "description is optional");
        assert_eq!(index.examples[1].description, "d");
        assert!(!index.examples[0].too_new());
        assert!(index.examples[1].too_new());
        let mut e = example("a.json", "G");
        e.version = FORMAT_VERSION;
        assert!(!e.too_new(), "this app's own version opens");
        e.version += 1;
        assert!(e.too_new());
    }

    #[test]
    fn an_index_that_isnt_one_fails() {
        assert!(Index::parse("not json").unwrap_err().contains("isn't JSON"));
        assert!(Index::parse("[]").unwrap_err().contains("no \"examples\""));
        assert!(Index::parse(r#"{"examples": 3}"#).is_err());
        assert_eq!(
            Index::parse("\u{FEFF}{\"examples\": []}").unwrap(),
            Index::default()
        );
    }

    #[test]
    fn groups_keep_the_order_they_first_appear_in() {
        let index = Index {
            examples: vec![
                example("a", "Small"),
                example("b", "Large"),
                example("c", "Small"),
            ],
            skipped: 0,
        };
        let groups: Vec<(&str, Vec<&str>)> = index
            .groups()
            .into_iter()
            .map(|(g, l)| (g, l.iter().map(|e| e.file.as_str()).collect()))
            .collect();
        assert_eq!(groups, [("Small", vec!["a", "c"]), ("Large", vec!["b"])]);
    }

    #[test]
    fn urls_are_encoded() {
        let e = example("Escher/e_coli_core.Core metabolism.json", "G");
        assert_eq!(
            e.url(),
            format!("{BASE_URL}Escher/e_coli_core.Core%20metabolism.json")
        );
        assert_eq!(e.name(), "e_coli_core.Core metabolism.json");
        assert_eq!(
            example("x(1).json", "G").url(),
            format!("{BASE_URL}x%281%29.json")
        );
        assert_eq!(example("a.json", "G").name(), "a.json");
    }

    /// A real download from GitHub, and a 404. Needs the network, so it only
    /// runs when asked: `cargo test -p pd-app -- --ignored`.
    #[test]
    #[ignore]
    fn fetches_from_github() {
        let ctx = egui::Context::default();
        let wait = |pending: Pending| loop {
            if let Some(result) = take(&pending) {
                break result;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        };
        let text = wait(fetch(&ctx, example("README.md", "G").url())).unwrap();
        assert!(text.contains("PathwayDesigner"));
        let missing = wait(fetch(&ctx, example("no such file", "G").url()));
        assert_eq!(missing.unwrap_err(), "not found (404)");
    }

    /// The repository's own index: every entry is there, opens, and says the
    /// version it needs, so a broken index fails CI on the push that breaks it.
    #[test]
    fn the_repositorys_index_is_sound() {
        let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../../");
        let text = std::fs::read_to_string(format!("{root}{INDEX}")).unwrap();
        let index = Index::parse(&text).unwrap();
        assert_eq!(index.skipped, 0, "an entry can't be read");
        assert!(!index.examples.is_empty());
        let mut files = std::collections::HashSet::new();
        for e in &index.examples {
            assert!(files.insert(&e.file), "{} is listed twice", e.file);
            let text = std::fs::read_to_string(format!("{root}{}", e.file))
                .unwrap_or_else(|err| panic!("{}: {err}", e.file));
            match FileKind::of(e.name(), &text) {
                FileKind::Json => {
                    let loaded = pd_core::json::from_json(&text)
                        .unwrap_or_else(|err| panic!("{}: {err}", e.file));
                    assert_eq!(loaded.version, e.version, "{}: version", e.file);
                    assert!(!e.too_new(), "{}: newer than this app", e.file);
                }
                FileKind::Escher => {
                    pd_core::escher::import(&text)
                        .unwrap_or_else(|err| panic!("{}: {err}", e.file));
                    assert_eq!(e.version, 0, "{}: not PathwayDesigner JSON", e.file);
                }
                kind => panic!("{}: add a check for {kind:?} files", e.file),
            }
        }
    }
}
