//! Annotate Species from Names (docs/chebi-lookup-design.md, step 3) and
//! Annotate Reactions from Names (docs/reaction-lookup-design.md, step 2):
//! the items are looked up one request at a time, then reviewed in a
//! temporary Annotate tab in the right-hand panel (C6). Nothing changes
//! until Apply, which adds what is ticked as one undo step.

use eframe::egui::{self, RichText};
use pd_core::annotation::resource_url;
use pd_core::chebi::{self, MIN_INTERVAL_MS};
use pd_core::reaction_lookup as rl;

use crate::annotation_panel::Fetch;
use crate::gallery::{Pending, take};

/// What the tab asks the app to do.
#[derive(Debug, PartialEq)]
pub enum TabAction {
    /// Apply these to species, then close the tab.
    Apply(Vec<chebi::Accepted>),
    /// Apply these to reactions, then close the tab.
    ApplyReactions(Vec<rl::Accepted>),
    /// Close the tab, changing nothing.
    Close,
    /// Select this species or reaction and centre the diagram on it, the
    /// review staying.
    Show(String),
}

/// The search under way: for species or for reactions.
enum Lookup {
    Species(chebi::Search),
    Reactions(rl::Search),
}

impl Lookup {
    fn next_url(&mut self) -> Option<String> {
        match self {
            Lookup::Species(s) => s.next_url(),
            Lookup::Reactions(s) => s.next_url(),
        }
    }

    fn answer(&mut self, result: Result<String, String>) {
        match self {
            Lookup::Species(s) => s.answer(result),
            Lookup::Reactions(s) => s.answer(result),
        }
    }

    fn progress(&self) -> (usize, usize) {
        match self {
            Lookup::Species(s) => s.progress(),
            Lookup::Reactions(s) => s.progress(),
        }
    }

    fn unreachable(&self) -> bool {
        match self {
            Lookup::Species(s) => s.unreachable(),
            Lookup::Reactions(s) => s.unreachable(),
        }
    }

    /// What it is looking for, and where.
    fn words(&self) -> Words {
        match self {
            Lookup::Species(_) => Words {
                items: "species",
                find: "Find a species",
                services: "BiGG, then ChEBI, then PubChem, one name at a time.",
                offline: "BiGG, ChEBI and PubChem couldn't be reached, so nothing was found.",
            },
            Lookup::Reactions(_) => Words {
                items: "reactions",
                find: "Find a reaction",
                services: "BiGG, then Rhea, then BiGG's search, one reaction at a time.",
                offline: "BiGG and Rhea couldn't be reached, so nothing was found.",
            },
        }
    }
}

struct Words {
    items: &'static str,
    find: &'static str,
    services: &'static str,
    offline: &'static str,
}

/// One item in the review: a species or a reaction.
struct Row<F> {
    found: F,
    /// Its id, and its display name when looked up.
    id: String,
    name: Option<String>,
    /// The candidate chosen.
    chosen: usize,
    /// Add the chosen candidate's references: ticked when sure (C2, R4).
    add: bool,
    /// Give it the name found as its display name (C3, R5).
    rename: bool,
}

impl<F> Row<F> {
    /// Whether the review's Find box keeps it: its id, its name or what was
    /// found holds the words, case aside.
    fn matches(&self, words: &str, found: &[Option<&str>]) -> bool {
        words.is_empty()
            || [Some(self.id.as_str()), self.name.as_deref()]
                .iter()
                .chain(found)
                .flatten()
                .any(|t| t.to_lowercase().contains(words))
    }

    /// Its name in the review: its display name, else its id.
    fn label(&self) -> String {
        self.name.clone().unwrap_or_else(|| self.id.clone())
    }
}

type SpeciesRow = Row<chebi::Found>;
type ReactionRow = Row<rl::Found>;

impl SpeciesRow {
    fn found_any(&self) -> bool {
        self.found.source.is_some()
    }

    fn keeps(&self, words: &str) -> bool {
        let f = &self.found;
        let chosen = f
            .candidates
            .get(self.chosen)
            .and_then(|c| c.name.as_deref());
        self.matches(words, &[f.full_name.as_deref(), chosen])
    }
}

impl ReactionRow {
    fn found_any(&self) -> bool {
        self.found.source.is_some()
    }

    fn keeps(&self, words: &str) -> bool {
        let c = self.found.candidates.get(self.chosen);
        self.matches(
            words,
            &[
                c.and_then(|c| c.name.as_deref()),
                c.map(|c| c.equation.as_str()),
            ],
        )
    }

    /// The name the chosen reaction would give it: BiGG's (Rhea's have
    /// none).
    fn offered_name(&self) -> Option<String> {
        self.found.candidates.get(self.chosen)?.name.clone()
    }
}

/// The rows to review, once the searching is done.
enum Rows {
    Species(Vec<SpeciesRow>),
    Reactions(Vec<ReactionRow>),
}

pub struct AnnotateTab {
    search: Lookup,
    waiting: Option<Pending>,
    last: f64,
    /// The display name of each item looked for.
    names: Vec<(String, Option<String>)>,
    /// Filled in when the searching is done (or stopped).
    rows: Option<Rows>,
    unreachable: bool,
    /// The words in the review's Find box.
    filter: String,
}

impl AnnotateTab {
    /// Annotate Species from Names.
    pub fn new(queries: Vec<chebi::Query>) -> Self {
        let names = queries
            .iter()
            .map(|q| (q.species.clone(), q.name.clone()))
            .collect();
        Self::with(Lookup::Species(chebi::Search::new(queries)), names)
    }

    /// Annotate Reactions from Names.
    pub fn reactions(queries: Vec<rl::Query>) -> Self {
        let names = queries
            .iter()
            .map(|q| (q.reaction.clone(), q.name.clone()))
            .collect();
        Self::with(Lookup::Reactions(rl::Search::new(queries)), names)
    }

    fn with(search: Lookup, names: Vec<(String, Option<String>)>) -> Self {
        AnnotateTab {
            search,
            waiting: None,
            last: f64::NEG_INFINITY,
            names,
            rows: None,
            unreachable: false,
            filter: String::new(),
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
        let name = |id: &str| {
            self.names
                .iter()
                .find(|(i, _)| i == id)
                .and_then(|(_, n)| n.clone())
        };
        self.rows = Some(match &self.search {
            Lookup::Species(s) => Rows::Species(
                s.results()
                    .iter()
                    .map(|f| {
                        let name = name(&f.species);
                        Row {
                            id: f.species.clone(),
                            chosen: f.chosen.unwrap_or(0),
                            add: f.sure,
                            // C3: on for a species without a display name.
                            rename: f.full_name.is_some() && name.is_none(),
                            name,
                            found: f.clone(),
                        }
                    })
                    .collect(),
            ),
            Lookup::Reactions(s) => Rows::Reactions(
                s.results()
                    .iter()
                    .map(|f| {
                        let mut row = Row {
                            id: f.reaction.clone(),
                            name: name(&f.reaction),
                            chosen: f.chosen.unwrap_or(0),
                            add: f.sure,
                            rename: false,
                            found: f.clone(),
                        };
                        // R5: on for a reaction without a display name.
                        row.rename = row.name.is_none() && row.offered_name().is_some();
                        row
                    })
                    .collect(),
            ),
        });
    }

    pub fn show(&mut self, ui: &mut egui::Ui) -> Option<TabAction> {
        let words = self.search.words();
        let Some(rows) = &mut self.rows else {
            let (done, total) = self.search.progress();
            ui.label(format!("Searching {} of {total}…", (done + 1).min(total)));
            ui.add(egui::ProgressBar::new(done as f32 / total.max(1) as f32));
            ui.label(RichText::new(words.services).small().weak());
            if ui
                .button("Stop")
                .on_hover_text("Review what has been found so far")
                .clicked()
            {
                self.review();
            }
            return None;
        };
        let (total, found, sure, ticked) = match rows {
            Rows::Species(r) => (
                r.len(),
                r.iter().filter(|r| r.found_any()).count(),
                r.iter().filter(|r| r.found.sure).count(),
                r.iter().filter(|r| r.add || r.rename).count(),
            ),
            Rows::Reactions(r) => (
                r.len(),
                r.iter().filter(|r| r.found_any()).count(),
                r.iter().filter(|r| r.found.sure).count(),
                r.iter().filter(|r| r.add || r.rename).count(),
            ),
        };
        if self.unreachable && found == 0 {
            ui.label(words.offline);
            return ui.button("Close").clicked().then_some(TabAction::Close);
        }
        // Long names are cut short to the panel's width, the whole shown on
        // hover, so a ChEBI name hundreds of letters long doesn't hold the
        // panel wide.
        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
        ui.label(format!(
            "Found {found} of {total} {}; {sure} ticked as clear matches.",
            words.items
        ));
        let hint = match rows {
            Rows::Species(_) => {
                "Check each before ticking it: a short name can find the wrong thing. \
                 Nothing changes until Apply."
            }
            Rows::Reactions(_) => {
                "Check each before ticking it: compare the equation found with the \
                 reaction. Nothing changes until Apply."
            }
        };
        ui.label(RichText::new(hint).small().weak());
        // R6: Rhea needs the species' ChEBI entries.
        if let Rows::Reactions(r) = rows
            && r.iter().any(|r| r.found.species_unannotated)
        {
            ui.label(
                RichText::new(
                    "Some reactions have species without a ChEBI entry; annotate the \
                     species first (Annotate Species from Names) to find more in Rhea.",
                )
                .small()
                .weak(),
            )
            .on_hover_text("Rhea is searched by the reaction's species' ChEBI entries");
        }
        let mut action = None;
        ui.horizontal(|ui| {
            if ui
                .add_enabled(ticked > 0, egui::Button::new(format!("Apply ({ticked})")))
                .clicked()
            {
                action = Some(match &*rows {
                    Rows::Species(r) => TabAction::Apply(accepted_species(r)),
                    Rows::Reactions(r) => TabAction::ApplyReactions(accepted_reactions(r)),
                });
            }
            if ui.button("Cancel").clicked() {
                action = Some(TabAction::Close);
            }
        });
        ui.add(
            egui::TextEdit::singleline(&mut self.filter)
                .hint_text(words.find)
                .desired_width(f32::INFINITY),
        );
        let find = self.filter.trim().to_lowercase();
        ui.separator();
        let missing: Vec<(String, String)> = match rows {
            Rows::Species(r) => {
                for (i, row) in r.iter_mut().enumerate() {
                    if row.found_any() && row.keeps(&find) {
                        if species_row(ui, i, row) {
                            action = Some(TabAction::Show(row.id.clone()));
                        }
                        ui.add_space(6.0);
                    }
                }
                r.iter()
                    .filter(|r| !r.found_any() && r.keeps(&find))
                    .map(|r| (r.label(), r.id.clone()))
                    .collect()
            }
            Rows::Reactions(r) => {
                for (i, row) in r.iter_mut().enumerate() {
                    if row.found_any() && row.keeps(&find) {
                        if reaction_row(ui, i, row) {
                            action = Some(TabAction::Show(row.id.clone()));
                        }
                        ui.add_space(6.0);
                    }
                }
                r.iter()
                    .filter(|r| !r.found_any() && r.keeps(&find))
                    .map(|r| (r.label(), r.id.clone()))
                    .collect()
            }
        };
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

/// A row's first line: its tick, its name (a link that shows it on the
/// diagram), where it was found and what. True if the name was clicked.
fn heading(
    ui: &mut egui::Ui,
    add: &mut bool,
    tick: &str,
    label: &str,
    source: &str,
    what: &str,
) -> bool {
    let mut show = false;
    ui.horizontal_wrapped(|ui| {
        ui.checkbox(add, "").on_hover_text(tick);
        show = ui
            .link(RichText::new(label).strong())
            .on_hover_text(format!("{label}: show it on the diagram"))
            .clicked();
        ui.label(RichText::new(format!("({source})")).small().weak());
        ui.label(format!("→ {what}")).on_hover_text(what);
    });
    show
}

/// The candidates to choose from, as wide as the panel allows, and a link
/// to the chosen one's page.
fn choice(ui: &mut egui::Ui, i: usize, chosen: &mut usize, texts: &[String], page: Option<String>) {
    ui.horizontal_wrapped(|ui| {
        ui.add_space(20.0);
        let shown = texts[*chosen].clone();
        // Leaving room for the link.
        let width = (ui.available_width() - 50.0).max(80.0);
        egui::ComboBox::from_id_salt(("annotate-choice", i))
            .selected_text(shown.clone())
            .width(width)
            .wrap_mode(egui::TextWrapMode::Truncate)
            .show_ui(ui, |ui| {
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
                for (k, t) in texts.iter().enumerate() {
                    ui.selectable_value(chosen, k, t.clone()).on_hover_text(t);
                }
            })
            .response
            .on_hover_text(shown);
        if let Some(url) = page {
            ui.hyperlink_to("open", url)
                .on_hover_text("The entry's page, to check it");
        }
    });
}

/// The tick to give it the name found as its display name.
fn rename(ui: &mut egui::Ui, on: &mut bool, name: &str) {
    ui.horizontal(|ui| {
        ui.add_space(20.0);
        ui.checkbox(on, format!("Display name “{name}”"))
            .on_hover_text(format!("Give it the display name “{name}”"));
    });
}

/// One species found. True if its name was clicked.
fn species_row(ui: &mut egui::Ui, i: usize, row: &mut SpeciesRow) -> bool {
    let f = &row.found;
    let source = f.source.map(|s| s.text()).unwrap_or("");
    let full = f.full_name.clone().unwrap_or_default();
    let label = row.label();
    let show = heading(
        ui,
        &mut row.add,
        "Add the ChEBI entry below as what this species is",
        &label,
        source,
        &full,
    );
    let texts: Vec<String> = row
        .found
        .candidates
        .iter()
        .map(|c| match &c.name {
            Some(n) => format!("{}  {n}", c.chebi),
            None => c.chebi.clone(),
        })
        .collect();
    let page = |k: usize| resource_url(&row.found.candidates[k].reference().resource);
    let url = page(row.chosen);
    choice(ui, i, &mut row.chosen, &texts, url);
    if !full.is_empty() {
        rename(ui, &mut row.rename, &full);
    }
    show
}

/// One reaction found: its tick and name, the reactions found to choose
/// from, the EC numbers and Rhea entry they give, and the equation, to
/// compare with the reaction. True if its name was clicked.
fn reaction_row(ui: &mut egui::Ui, i: usize, row: &mut ReactionRow) -> bool {
    let source = row.found.source.map(|s| s.text()).unwrap_or("");
    let c = &row.found.candidates[row.chosen];
    let what = c.name.clone().unwrap_or_else(|| c.equation.clone());
    let label = row.label();
    let show = heading(
        ui,
        &mut row.add,
        "Add the EC numbers and Rhea, KEGG and MetaNetX reactions below",
        &label,
        source,
        &what,
    );
    let texts: Vec<String> = row
        .found
        .candidates
        .iter()
        .map(|c| match &c.name {
            Some(n) => format!("{}  {n}", c.id),
            None => format!("{}  {}", c.id, c.equation),
        })
        .collect();
    let page = row.found.candidates[row.chosen].page();
    choice(ui, i, &mut row.chosen, &texts, Some(page));
    let c = &row.found.candidates[row.chosen];
    let mut gives: Vec<String> = Vec::new();
    if !c.ec.is_empty() {
        gives.push(format!("EC {}", c.ec.join(", ")));
    }
    if let Some(r) = &c.rhea {
        gives.push(format!("Rhea {r}"));
    }
    if let Some(k) = &c.kegg {
        gives.push(format!("KEGG {k}"));
    }
    if let Some(m) = &c.metanetx {
        gives.push(format!("MetaNetX {m}"));
    }
    let equation = c.equation.clone();
    ui.horizontal(|ui| {
        ui.add_space(20.0);
        ui.vertical(|ui| {
            if !gives.is_empty() {
                let gives = gives.join(" · ");
                ui.label(RichText::new(&gives).small()).on_hover_text(gives);
            }
            ui.label(RichText::new(&equation).small().weak())
                .on_hover_text(equation);
        });
    });
    if let Some(name) = row.offered_name() {
        rename(ui, &mut row.rename, &name);
    }
    show
}

/// What is ticked, as the view applies it to species.
fn accepted_species(rows: &[SpeciesRow]) -> Vec<chebi::Accepted> {
    rows.iter()
        .filter(|r| r.found_any() && (r.add || r.rename))
        .map(|r| {
            let mut references = Vec::new();
            if r.add {
                references.push(r.found.candidates[r.chosen].reference());
                references.extend(r.found.other_references.iter().cloned());
            }
            chebi::Accepted {
                species: r.id.clone(),
                references,
                display_name: r.rename.then(|| r.found.full_name.clone()).flatten(),
            }
        })
        .collect()
}

/// What is ticked, as the view applies it to reactions.
fn accepted_reactions(rows: &[ReactionRow]) -> Vec<rl::Accepted> {
    rows.iter()
        .filter(|r| r.found_any() && (r.add || r.rename))
        .map(|r| rl::Accepted {
            reaction: r.id.clone(),
            references: if r.add {
                r.found.candidates[r.chosen].references()
            } else {
                Vec::new()
            },
            display_name: r.rename.then(|| r.offered_name()).flatten(),
        })
        .collect()
}
