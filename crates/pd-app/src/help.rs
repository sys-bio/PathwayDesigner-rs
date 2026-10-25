//! Help ▸ PathwayDesigner Help: Markdown pages from the repository's `help/`
//! folder, built into the program and shown in a panel at the right, so the
//! diagram stays in view while reading.

use eframe::egui;
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};

/// A help page: its file name (what links in other pages name), its title in
/// the topic list, and its text.
struct Page {
    file: &'static str,
    title: &'static str,
    text: &'static str,
}

/// The first page is the contents.
const PAGES: &[Page] = &[
    Page {
        file: "contents.md",
        title: "Contents",
        text: include_str!("../../../help/contents.md"),
    },
    Page {
        file: "getting-started.md",
        title: "Getting started",
        text: include_str!("../../../help/getting-started.md"),
    },
    Page {
        file: "antimony.md",
        title: "Antimony",
        text: include_str!("../../../help/antimony.md"),
    },
    Page {
        file: "diagram-language.md",
        title: "The diagram language",
        text: include_str!("../../../help/diagram-language.md"),
    },
];

pub struct Help {
    pub open: bool,
    page: usize,
    /// Pages to go back to, the latest last.
    back: Vec<usize>,
    cache: CommonMarkCache,
}

impl Default for Help {
    fn default() -> Self {
        let mut cache = CommonMarkCache::default();
        for page in PAGES {
            cache.add_link_hook(page.file);
        }
        Self {
            open: false,
            page: 0,
            back: Vec::new(),
            cache,
        }
    }
}

impl Help {
    /// The title of the page shown (for a test of the in-window Help menu,
    /// which macOS doesn't have).
    #[cfg(all(test, not(target_os = "macos")))]
    pub fn title(&self) -> &'static str {
        PAGES[self.page].title
    }

    fn go(&mut self, page: usize) {
        if page != self.page {
            self.back.push(self.page);
            self.page = page;
        }
    }

    /// The panel's contents.
    pub fn show(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading("Help");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("✖").on_hover_text("Close help").clicked() {
                    self.open = false;
                }
            });
        });
        ui.horizontal(|ui| {
            if ui
                .add_enabled(!self.back.is_empty(), egui::Button::new("Back"))
                .clicked()
            {
                self.page = self.back.pop().unwrap_or(0);
            }
            if ui.button("Contents").clicked() {
                self.go(0);
            }
            let mut choice = self.page;
            egui::ComboBox::from_id_salt("help topic")
                .selected_text(PAGES[self.page].title)
                .show_ui(ui, |ui| {
                    for (i, page) in PAGES.iter().enumerate() {
                        ui.selectable_value(&mut choice, i, page.title);
                    }
                });
            self.go(choice);
        });
        ui.separator();
        egui::ScrollArea::vertical()
            .id_salt(("help page", self.page))
            .show(ui, |ui| {
                CommonMarkViewer::new().show(ui, &mut self.cache, PAGES[self.page].text);
            });
        if let Some(i) = PAGES
            .iter()
            .position(|p| self.cache.get_link_hook(p.file) == Some(true))
        {
            self.go(i);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every link to a `.md` file in a page names a page that exists (web
    /// addresses aside).
    #[test]
    fn links_name_pages() {
        for page in PAGES {
            for (n, _) in page.text.match_indices("](") {
                let rest = &page.text[n + 2..];
                let target = &rest[..rest.find(')').unwrap()];
                if target.ends_with(".md") && !target.contains("://") {
                    assert!(
                        PAGES.iter().any(|p| p.file == target),
                        "{} links to {target}",
                        page.file
                    );
                }
            }
        }
    }
}
