//! Annotate Species from Names (docs/chebi-lookup-design.md, step 3): the
//! species are looked up in BiGG, ChEBI and PubChem, one request at a time,
//! then reviewed in a temporary Annotate tab in the right-hand panel (C6).
//! Nothing changes until Apply, which adds what is ticked as one undo step.

use eframe::egui::{self, RichText};
use pd_core::annotation::resource_url;
use pd_core::chebi::{Accepted, Found, MIN_INTERVAL_MS, Query, Search};

use crate::annotation_panel::Fetch;
use crate::gallery::{Pending, take};

/// What the tab asks the app to do.
#[derive(Debug, PartialEq)]
pub enum TabAction {
    /// Apply these, then close the tab.
    Apply(Vec<Accepted>),
    /// Close the tab, changing nothing.
    Close,
    /// Select this species and centre the diagram on it, the review staying.
    Show(String),
}

/// One species in the review.
struct Row {
    found: Found,
    /// Its display name when looked up, and its id.
    name: Option<String>,
    /// The candidate chosen.
    chosen: usize,
    /// Add the chosen entry (and BiGG's links): ticked when sure (C2).
    add: bool,
    /// Give it the full name found as its display name (C3).
    rename: bool,
}

pub struct AnnotateTab {
    search: Search,
    waiting: Option<Pending>,
    last: f64,
    /// The display name of each species looked for.
    names: Vec<(String, Option<String>)>,
    /// Filled in when the searching is done (or stopped).
    rows: Option<Vec<Row>>,
    unreachable: bool,
}

impl AnnotateTab {
    pub fn new(queries: Vec<Query>) -> Self {
        let names = queries
            .iter()
            .map(|q| (q.species.clone(), q.name.clone()))
            .collect();
        AnnotateTab {
            search: Search::new(queries),
            waiting: None,
            last: f64::NEG_INFINITY,
            names,
            rows: None,
            unreachable: false,
        }
    }

    /// Takes the search a step further, at most one request every
    /// `MIN_INTERVAL_MS` (C8). Runs whichever tab is shown.
    pub fn poll(&mut self, ctx: &egui::Context, fetch: Fetch) {
        if self.rows.is_some() {
            return;
        }
        if let Some(waiting) = &self.waiting {
            let Some(result) = take(waiting) else {
                return;
            };
            self.waiting = None;
            self.search.answer(result);
        }
        let now = ctx.input(|i| i.time);
        let wait = MIN_INTERVAL_MS as f64 / 1000.0;
        if now - self.last < wait {
            ctx.request_repaint_after(std::time::Duration::from_secs_f64(wait));
            return;
        }
        match self.search.next_url() {
            Some(url) => {
                self.last = now;
                self.waiting = Some(fetch(ctx, url));
                ctx.request_repaint();
            }
            None => self.review(),
        }
    }

    /// Ends the searching with what was found so far.
    fn review(&mut self) {
        self.waiting = None;
        self.unreachable = self.search.unreachable();
        let rows = self
            .search
            .results()
            .iter()
            .map(|f| {
                let name = self
                    .names
                    .iter()
                    .find(|(id, _)| *id == f.species)
                    .and_then(|(_, n)| n.clone());
                Row {
                    chosen: f.chosen.unwrap_or(0),
                    add: f.sure,
                    // C3: on for a species without a display name.
                    rename: f.full_name.is_some() && name.is_none(),
                    name,
                    found: f.clone(),
                }
            })
            .collect();
        self.rows = Some(rows);
    }

    pub fn show(&mut self, ui: &mut egui::Ui) -> Option<TabAction> {
        let Some(rows) = &mut self.rows else {
            let (done, total) = self.search.progress();
            ui.label(format!("Searching {} of {total}…", (done + 1).min(total)));
            ui.add(egui::ProgressBar::new(done as f32 / total.max(1) as f32));
            ui.label(
                RichText::new("BiGG, then ChEBI, then PubChem, one name at a time.")
                    .small()
                    .weak(),
            );
            if ui
                .button("Stop")
                .on_hover_text("Review what has been found so far")
                .clicked()
            {
                self.review();
            }
            return None;
        };
        if self.unreachable && rows.iter().all(|r| r.found.source.is_none()) {
            ui.label("BiGG, ChEBI and PubChem couldn't be reached, so nothing was found.");
            return ui.button("Close").clicked().then_some(TabAction::Close);
        }
        let found = rows.iter().filter(|r| r.found.source.is_some()).count();
        let sure = rows.iter().filter(|r| r.found.sure).count();
        ui.label(format!(
            "Found {found} of {} species; {sure} ticked as clear matches.",
            rows.len()
        ));
        ui.label(
            RichText::new(
                "Check each before ticking it: a short name can find the wrong thing. \
                 Nothing changes until Apply.",
            )
            .small()
            .weak(),
        );
        let mut action = None;
        ui.horizontal(|ui| {
            let n = rows.iter().filter(|r| r.add || r.rename).count();
            if ui
                .add_enabled(n > 0, egui::Button::new(format!("Apply ({n})")))
                .clicked()
            {
                action = Some(TabAction::Apply(accepted(rows)));
            }
            if ui.button("Cancel").clicked() {
                action = Some(TabAction::Close);
            }
        });
        ui.separator();
        for (i, row) in rows.iter_mut().enumerate() {
            if row.found.source.is_some() {
                if found_row(ui, i, row) {
                    action = Some(TabAction::Show(row.found.species.clone()));
                }
                ui.add_space(6.0);
            }
        }
        let missing: Vec<(String, String)> = rows
            .iter()
            .filter(|r| r.found.source.is_none())
            .map(|r| {
                let id = r.found.species.clone();
                (r.name.clone().unwrap_or_else(|| id.clone()), id)
            })
            .collect();
        if !missing.is_empty() {
            ui.separator();
            ui.label(RichText::new("Nothing found for").weak());
            ui.horizontal_wrapped(|ui| {
                for (name, id) in &missing {
                    if ui
                        .link(name)
                        .on_hover_text("Show it on the diagram")
                        .clicked()
                    {
                        action = Some(TabAction::Show(id.clone()));
                    }
                }
            });
        }
        action
    }
}

/// One species found: tick, names, the ChEBI entry to add, and its name.
/// True if its name was clicked, to show it on the diagram.
fn found_row(ui: &mut egui::Ui, i: usize, row: &mut Row) -> bool {
    let mut show = false;
    let f = &row.found;
    let species = row.name.clone().unwrap_or_else(|| f.species.clone());
    let source = f.source.map(|s| s.text()).unwrap_or("");
    let full = f.full_name.clone().unwrap_or_default();
    ui.horizontal_wrapped(|ui| {
        ui.checkbox(&mut row.add, "")
            .on_hover_text("Add the ChEBI entry below as what this species is");
        show = ui
            .link(RichText::new(&species).strong())
            .on_hover_text("Show it on the diagram")
            .clicked();
        ui.label(format!("→ {full}"));
        ui.label(RichText::new(format!("({source})")).small().weak());
    });
    ui.horizontal_wrapped(|ui| {
        ui.add_space(20.0);
        let text = |c: &pd_core::chebi::Candidate| match &c.name {
            Some(n) => format!("{}  {n}", c.chebi),
            None => c.chebi.clone(),
        };
        let chosen = &row.found.candidates[row.chosen];
        egui::ComboBox::from_id_salt(("annotate-choice", i))
            .selected_text(text(chosen))
            .width(200.0)
            .show_ui(ui, |ui| {
                for (k, c) in row.found.candidates.iter().enumerate() {
                    ui.selectable_value(&mut row.chosen, k, text(c));
                }
            });
        let r = row.found.candidates[row.chosen].reference();
        if let Some(url) = resource_url(&r.resource) {
            ui.hyperlink_to("open", url)
                .on_hover_text("The entry's page, to check it");
        }
    });
    if !full.is_empty() {
        ui.horizontal(|ui| {
            ui.add_space(20.0);
            ui.checkbox(&mut row.rename, format!("Display name “{full}”"));
        });
    }
    show
}

/// What is ticked, as the view applies it.
fn accepted(rows: &[Row]) -> Vec<Accepted> {
    rows.iter()
        .filter(|r| r.found.source.is_some() && (r.add || r.rename))
        .map(|r| {
            let mut references = Vec::new();
            if r.add {
                references.push(r.found.candidates[r.chosen].reference());
                references.extend(r.found.other_references.iter().cloned());
            }
            Accepted {
                species: r.found.species.clone(),
                references,
                display_name: r.rename.then(|| r.found.full_name.clone()).flatten(),
            }
        })
        .collect()
}
