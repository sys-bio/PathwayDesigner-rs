//! The Annotations part of an item's panel, and of the Diagram's for the
//! model (docs/annotations-design.md, step 4): its SBO term, its database
//! references and its notes, shown and edited. Folded until opened. Each
//! change is one undo step; the SBO term and the notes are applied when
//! their field loses focus, as the inspector's other fields are.

use eframe::egui::{self, RichText};
use pd_core::annotation::{
    ITEM_QUALIFIERS, MODEL_QUALIFIERS, SboKind, notes_text, notes_xhtml, qualifier_text,
    resource_from_typed, resource_text, resource_url, sbo_id, sbo_name, sbo_url,
};
use pd_core::model::{Annotation, Reference};
use pd_core::view::DiagramView;

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
            *self = AnnotationPanel {
                target: Some(target),
                ..AnnotationPanel::default()
            };
        }
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
