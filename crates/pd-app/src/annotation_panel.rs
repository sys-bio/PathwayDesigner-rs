//! The Annotations part of an item's panel, and of the Diagram's for the
//! model (docs/annotations-design.md, step 4): its SBO term, its database
//! references and its notes, shown and edited. Folded until opened. Each
//! change is one undo step; the SBO term and the notes are applied when
//! their field loses focus, as the inspector's other fields are.
//!
//! A species' part can also find its ChEBI entry by name
//! (docs/chebi-lookup-design.md, step 2): BiGG, ChEBI and PubChem searched
//! a request at a time through the app's fetch, the matches listed to add.

use eframe::egui::{self, RichText};
use pd_core::annotation::{
    ITEM_QUALIFIERS, MODEL_QUALIFIERS, SboKind, notes_text, notes_xhtml, qualifier_text,
    resource_from_typed, resource_text, resource_url, sbo_id, sbo_name, sbo_url,
};
use pd_core::chebi::{Candidate, Query, Search};
use pd_core::model::{Annotation, Reference};
use pd_core::view::DiagramView;

use crate::gallery::{Pending, take};

/// How the app fetches an address (the Gallery's, swapped in tests).
pub type Fetch = fn(&egui::Context, String) -> Pending;

const ERROR: egui::Color32 = egui::Color32::from_rgb(0xC6, 0x28, 0x28);

/// One Annotations part, with what is being typed into it.
#[derive(Default)]
pub struct AnnotationPanel {
    /// The item the typing is for (`Some(None)`: the model).
    target: Option<Option<String>>,
    sbo: String,
    sbo_dirty: bool,
    notes: String,
    notes_dirty: bool,
    qualifier: String,
    resource: String,
    error: Option<String>,
    /// How to fetch, set by the inspector; without it there is no search.
    pub fetch: Option<Fetch>,
    /// The words to find in ChEBI.
    find: String,
    /// A search under way.
    finder: Option<Finder>,
    /// The entries the last search found, and what it said.
    found: Vec<Candidate>,
    find_note: Option<String>,
}

/// A Find in ChEBI under way: the search, the request waiting, and when the
/// last one was made (at most one every `MIN_INTERVAL_MS`, C8).
struct Finder {
    search: Search,
    waiting: Option<Pending>,
    last: f64,
}

impl AnnotationPanel {
    /// The part for item `id` (a species, reaction or compartment), or the
    /// model for `None`. `salt` keeps each panel's widgets apart.
    pub fn show(
        &mut self,
        ui: &mut egui::Ui,
        view: &mut DiagramView,
        id: Option<&str>,
        salt: &str,
    ) {
        let target = id.map(str::to_owned);
        if self.target.as_ref() != Some(&target) {
            // What was typed for the item shown before goes to it first.
            self.commit(view);
            // The words to find: the species' display name, else its id.
            let find = id
                .and_then(|i| view.model().find_species(i))
                .map(|s| {
                    let primary = view.model().find_species(s.primary_id()).unwrap_or(s);
                    primary
                        .display_name
                        .clone()
                        .unwrap_or_else(|| primary.id.clone())
                })
                .unwrap_or_default();
            *self = AnnotationPanel {
                target: Some(target),
                fetch: self.fetch,
                find,
                ..AnnotationPanel::default()
            };
        }
        self.poll(ui.ctx());
        let Some(current) = view.annotation(id).cloned() else {
            return;
        };
        if !self.sbo_dirty {
            self.sbo = current.sbo.map(|n| n.to_string()).unwrap_or_default();
        }
        if !self.notes_dirty {
            self.notes = current.notes.as_deref().map(notes_text).unwrap_or_default();
        }
        let count = usize::from(current.sbo.is_some())
            + current.references.len()
            + usize::from(current.notes.is_some());
        let title = if count == 0 {
            "Annotations".to_owned()
        } else {
            format!("Annotations ({count})")
        };
        egui::CollapsingHeader::new(title)
            .id_salt(("annotations", salt))
            .default_open(false)
            .show(ui, |ui| self.contents(ui, view, id, &current));
    }

    fn contents(
        &mut self,
        ui: &mut egui::Ui,
        view: &mut DiagramView,
        id: Option<&str>,
        current: &Annotation,
    ) {
        // The SBO term: its number, its name, and the common ones to pick,
        // those for this kind of item. A model's says its framework.
        let m = view.model();
        let kind = match id {
            None => SboKind::Model,
            Some(i) if m.find_species(i).is_some() => SboKind::Species,
            Some(i) if m.find_reaction(i).is_some() => SboKind::Reaction,
            Some(_) => SboKind::Compartment,
        };
        ui.horizontal(|ui| {
            if kind == SboKind::Model {
                ui.label("Framework (SBO)").on_hover_text(
                    "The modelling framework the model's equations are for, as an SBO term",
                );
            } else {
                ui.label("SBO term");
            }
            let r = ui.add(
                egui::TextEdit::singleline(&mut self.sbo)
                    .desired_width(70.0)
                    .hint_text("none"),
            );
            if r.changed() {
                self.sbo_dirty = true;
            }
            if r.lost_focus() {
                self.commit_sbo(view);
            }
            let mut picked = None;
            ui.menu_button("Common", |ui| {
                for (n, name) in pd_core::annotation::sbo_terms(kind) {
                    if ui.button(format!("{n}  {name}")).clicked() {
                        picked = Some(*n);
                        ui.close();
                    }
                }
            });
            if let Some(n) = picked {
                self.sbo = n.to_string();
                self.sbo_dirty = true;
                self.commit_sbo(view);
            }
        });
        if let Some(n) = current.sbo {
            let name = sbo_name(n).unwrap_or("");
            ui.hyperlink_to(
                RichText::new(format!("{} {name}", sbo_id(n))).small(),
                sbo_url(n),
            )
            .on_hover_text("Open the term's definition");
        }

        // References: each a link, with a way to remove it.
        ui.add_space(4.0);
        ui.label("References");
        let mut remove = None;
        for (i, r) in current.references.iter().enumerate() {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(qualifier_text(&r.qualifier)).weak());
                let text = resource_text(&r.resource);
                match resource_url(&r.resource) {
                    Some(url) => {
                        ui.hyperlink_to(text, url).on_hover_text(&r.resource);
                    }
                    None => {
                        ui.label(text).on_hover_text(&r.resource);
                    }
                }
                if ui
                    .small_button("✕")
                    .on_hover_text("Remove this reference")
                    .clicked()
                {
                    remove = Some(i);
                }
            });
        }
        if let Some(i) = remove {
            let mut a = current.clone();
            a.references.remove(i);
            view.set_annotation(id, a);
        }
        // A new one: a qualifier and an entry.
        let qualifiers = if id.is_none() {
            MODEL_QUALIFIERS
        } else {
            ITEM_QUALIFIERS
        };
        if !qualifiers.contains(&self.qualifier.as_str()) {
            self.qualifier = qualifiers[0].to_owned();
        }
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt(("annotation-qualifier", id.unwrap_or("")))
                .selected_text(qualifier_text(&self.qualifier))
                .show_ui(ui, |ui| {
                    for q in qualifiers {
                        ui.selectable_value(
                            &mut self.qualifier,
                            (*q).to_owned(),
                            qualifier_text(q),
                        );
                    }
                });
            let r = ui.add(
                egui::TextEdit::singleline(&mut self.resource)
                    .desired_width(130.0)
                    .hint_text("CHEBI:15422 or a link"),
            );
            let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if ui.button("Add").clicked() || enter {
                match resource_from_typed(&self.resource) {
                    Some(resource) => {
                        let mut a = current.clone();
                        a.references.push(Reference {
                            qualifier: self.qualifier.clone(),
                            resource,
                        });
                        view.set_annotation(id, a);
                        self.resource.clear();
                        self.error = None;
                    }
                    None => {
                        self.error = Some(
                            "Type a link, or an identifier such as CHEBI:15422 or uniprot:P12345"
                                .into(),
                        )
                    }
                }
            }
        });
        if let Some(e) = &self.error {
            ui.label(RichText::new(e).color(ERROR).small());
        }
        if kind == SboKind::Species && self.fetch.is_some() {
            self.find_in_chebi(ui, view, id, current);
        }

        // Notes, as text.
        ui.add_space(4.0);
        ui.label("Notes");
        let r = ui.add(
            egui::TextEdit::multiline(&mut self.notes)
                .desired_rows(3)
                .desired_width(f32::INFINITY),
        );
        if r.changed() {
            self.notes_dirty = true;
        }
        if r.lost_focus() {
            self.commit_notes(view);
        }
    }

    /// Find in ChEBI: words, a Search button, and the entries found, each
    /// a link to its page and an Add button.
    fn find_in_chebi(
        &mut self,
        ui: &mut egui::Ui,
        view: &mut DiagramView,
        id: Option<&str>,
        current: &Annotation,
    ) {
        ui.add_space(4.0);
        ui.label("Find in ChEBI");
        let searching = self.finder.is_some();
        ui.horizontal(|ui| {
            let r = ui.add(
                egui::TextEdit::singleline(&mut self.find)
                    .desired_width(150.0)
                    .hint_text("a name, such as glucose"),
            );
            let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            let search = ui
                .add_enabled(!searching, egui::Button::new("Search"))
                .on_hover_text("Look the name up in BiGG, ChEBI and PubChem");
            if (search.clicked() || enter) && !searching && !self.find.trim().is_empty() {
                self.found.clear();
                self.find_note = None;
                self.finder = Some(Finder {
                    search: Search::new(vec![Query {
                        species: id.unwrap_or_default().to_owned(),
                        name: Some(self.find.trim().to_owned()),
                    }]),
                    waiting: None,
                    last: f64::NEG_INFINITY,
                });
            }
            if searching {
                ui.spinner();
            }
        });
        if let Some(note) = &self.find_note {
            ui.label(RichText::new(note).small().weak());
        }
        let mut add = None;
        // A long name is cut short, so it doesn't hold the panel wide.
        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
        for c in &self.found {
            ui.horizontal(|ui| {
                let r = c.reference();
                let have = current.references.contains(&r);
                if ui
                    .add_enabled(
                        !have,
                        egui::Button::new(if have { "Added" } else { "Add" }).small(),
                    )
                    .on_hover_text("Add as what this species is (bqbiol:is)")
                    .clicked()
                {
                    add = Some(r.clone());
                }
                ui.hyperlink_to(&c.chebi, resource_url(&r.resource).unwrap_or_default());
                if let Some(name) = &c.name {
                    ui.label(name).on_hover_text(name);
                }
            });
        }
        if let Some(r) = add {
            let mut a = current.clone();
            a.references.push(r);
            view.set_annotation(id, a);
        }
    }

    /// Takes a search a step further: hands it an answer that has come, and
    /// asks for the next address when the interval has passed.
    fn poll(&mut self, ctx: &egui::Context) {
        let (Some(fetch), Some(f)) = (self.fetch, self.finder.as_mut()) else {
            return;
        };
        if let Some(waiting) = &f.waiting {
            let Some(result) = take(waiting) else {
                return;
            };
            f.waiting = None;
            f.search.answer(result);
        }
        let now = ctx.input(|i| i.time);
        let wait = pd_core::chebi::MIN_INTERVAL_MS as f64 / 1000.0;
        if now - f.last < wait {
            ctx.request_repaint_after(std::time::Duration::from_secs_f64(wait));
            return;
        }
        match f.search.next_url() {
            Some(url) => {
                f.last = now;
                f.waiting = Some(fetch(ctx, url));
            }
            None => {
                let unreachable = f.search.unreachable();
                let result = f.search.results().first().cloned();
                self.finder = None;
                self.found = result.map(|r| r.candidates).unwrap_or_default();
                self.find_note = if unreachable {
                    Some("BiGG, ChEBI and PubChem couldn't be reached".into())
                } else if self.found.is_empty() {
                    Some("Nothing found: try another name".into())
                } else {
                    None
                };
            }
        }
        ctx.request_repaint();
    }

    /// Applies what was typed into the SBO term and notes fields.
    fn commit(&mut self, view: &mut DiagramView) {
        self.commit_sbo(view);
        self.commit_notes(view);
    }

    fn id(&self) -> Option<Option<String>> {
        self.target.clone()
    }

    /// The SBO term typed: a number, or `SBO:0000247`; empty for none. One
    /// that doesn't read is dropped, with a message.
    fn commit_sbo(&mut self, view: &mut DiagramView) {
        if !std::mem::take(&mut self.sbo_dirty) {
            return;
        }
        let Some(target) = self.id() else {
            return;
        };
        let id = target.as_deref();
        let Some(mut a) = view.annotation(id).cloned() else {
            return;
        };
        let t = self.sbo.trim();
        let digits = t.strip_prefix("SBO:").unwrap_or(t);
        a.sbo = if digits.is_empty() {
            None
        } else if let Ok(n) = digits.parse::<u32>() {
            Some(n)
        } else {
            self.error = Some("An SBO term is a number, such as 247".into());
            return;
        };
        self.error = None;
        view.set_annotation(id, a);
    }

    /// The notes typed. Untouched notes stay as they were read (A4).
    fn commit_notes(&mut self, view: &mut DiagramView) {
        if !std::mem::take(&mut self.notes_dirty) {
            return;
        }
        let Some(target) = self.id() else {
            return;
        };
        let id = target.as_deref();
        let Some(mut a) = view.annotation(id).cloned() else {
            return;
        };
        let was = a.notes.as_deref().map(notes_text).unwrap_or_default();
        if self.notes.trim() == was.trim() {
            return;
        }
        a.notes = notes_xhtml(&self.notes);
        view.set_annotation(id, a);
    }
}
