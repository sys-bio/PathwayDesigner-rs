//! What the annotation lookups left to check, marked on the diagram
//! (docs/annotation-marks-design.md): the marks kept after Apply, for the
//! session only (M3, M5), and the two ticks that show each kind, shared by
//! the Annotate tab, the Diagram group and View ▸ Annotation Marks (M4).

use eframe::egui::{self, RichText};
use pd_core::model::{Id, Model};
use pd_core::render::draw::{Mark, Marks, mark_colour};

/// A review under way: what it marks, the items it looked up, and whether
/// they are reactions.
pub struct Review {
    pub reactions: bool,
    pub looked_up: Vec<Id>,
    pub marks: Marks,
}

pub struct MarkState {
    /// Kept after Apply, until cleared or their items are annotated.
    pub kept: Marks,
    pub not_sure: bool,
    pub nothing_found: bool,
}

impl Default for MarkState {
    fn default() -> Self {
        MarkState {
            kept: Marks::default(),
            not_sure: true,
            nothing_found: true,
        }
    }
}

impl MarkState {
    /// What the diagram shows: the kept marks, those of the items a review
    /// under way looked up replaced by the review's; each kind only if its
    /// tick is on.
    pub fn shown(&self, review: Option<&Review>) -> Marks {
        let mut marks = self.kept.clone();
        if let Some(r) = review {
            let (kept, new) = if r.reactions {
                (&mut marks.reactions, &r.marks.reactions)
            } else {
                (&mut marks.species, &r.marks.species)
            };
            kept.retain(|(id, _)| !r.looked_up.contains(id));
            kept.extend(new.iter().cloned());
        }
        let on = |m: &Mark| match m {
            Mark::NotSure => self.not_sure,
            Mark::NothingFound => self.nothing_found,
        };
        marks.species.retain(|(_, m)| on(m));
        marks.reactions.retain(|(_, m)| on(m));
        marks
    }

    /// After Apply: the review's marks kept in place of those of the items
    /// it looked up (M3). After Cancel nothing is kept (M7).
    pub fn keep(&mut self, review: Review) {
        self.kept = self.shown_all(Some(&review));
    }

    /// [`Self::shown`] whatever the ticks say.
    fn shown_all(&self, review: Option<&Review>) -> Marks {
        let all = MarkState {
            kept: self.kept.clone(),
            not_sure: true,
            nothing_found: true,
        };
        all.shown(review)
    }

    /// Drops the marks of items that are gone, or have since got what was
    /// looked for: a species a ChEBI reference, a reaction an EC number or
    /// a Rhea reference (M3).
    pub fn prune(&mut self, model: &Model) {
        let has = |refs: &[pd_core::model::Reference], words: &[&str]| {
            refs.iter().any(|r| {
                let r = r.resource.to_ascii_lowercase();
                words.iter().any(|w| r.contains(w))
            })
        };
        self.kept.species.retain(|(id, _)| {
            model
                .find_species(id)
                .is_some_and(|s| !has(&s.annotation.references, &["chebi"]))
        });
        self.kept.reactions.retain(|(id, _)| {
            model
                .find_reaction(id)
                .is_some_and(|r| !has(&r.annotation.references, &["ec-code", "rhea"]))
        });
    }

    /// How many items are marked of each kind, kept and in `review`.
    pub fn counts(&self, review: Option<&Review>) -> (usize, usize) {
        let all = self.shown_all(review);
        let n = |m: Mark| {
            all.species
                .iter()
                .chain(&all.reactions)
                .filter(|(_, k)| *k == m)
                .count()
        };
        (n(Mark::NotSure), n(Mark::NothingFound))
    }

    /// The two ticks, each with its count and in its colour.
    pub fn ticks(&mut self, ui: &mut egui::Ui, (not_sure, nothing_found): (usize, usize)) {
        let swatch = |m: Mark, text: String| {
            let [r, g, b, _] = mark_colour(m).0;
            RichText::new(text).color(egui::Color32::from_rgb(r, g, b))
        };
        ui.checkbox(
            &mut self.not_sure,
            swatch(Mark::NotSure, format!("Not sure ({not_sure})")),
        )
        .on_hover_text("Found, but not ticked as a clear match: an orange ring");
        ui.checkbox(
            &mut self.nothing_found,
            swatch(
                Mark::NothingFound,
                format!("Nothing found ({nothing_found})"),
            ),
        )
        .on_hover_text("Nothing found: a blue ring");
    }

    /// The kept marks' line in the Diagram group and View ▸ Annotation
    /// Marks: the ticks and Clear Marks (M4). Nothing when none are kept.
    pub fn controls(&mut self, ui: &mut egui::Ui) {
        if self.kept.is_empty() {
            ui.label(RichText::new("No annotation marks").weak());
            return;
        }
        ui.label("Marked on the diagram:");
        let counts = self.counts(None);
        ui.horizontal_wrapped(|ui| self.ticks(ui, counts));
        if ui
            .button("Clear Marks")
            .on_hover_text("Take every annotation mark off the diagram")
            .clicked()
        {
            self.kept = Marks::default();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pd_core::model::Reference;

    fn marks(species: &[(&str, Mark)], reactions: &[(&str, Mark)]) -> Marks {
        let v = |x: &[(&str, Mark)]| x.iter().map(|(i, m)| (i.to_string(), *m)).collect();
        Marks {
            species: v(species),
            reactions: v(reactions),
        }
    }

    #[test]
    fn a_review_replaces_the_marks_of_what_it_looked_up() {
        let mut s = MarkState {
            kept: marks(
                &[("A", Mark::NotSure), ("B", Mark::NothingFound)],
                &[("R1", Mark::NotSure)],
            ),
            ..MarkState::default()
        };
        // A species review of B and C: A kept, B and C as the review says,
        // the reaction untouched.
        let review = Review {
            reactions: false,
            looked_up: vec!["B".into(), "C".into()],
            marks: marks(&[("C", Mark::NothingFound)], &[]),
        };
        let want = marks(
            &[("A", Mark::NotSure), ("C", Mark::NothingFound)],
            &[("R1", Mark::NotSure)],
        );
        assert_eq!(s.shown(Some(&review)), want);
        assert_eq!(s.counts(Some(&review)), (2, 1));
        // The ticks.
        s.not_sure = false;
        assert_eq!(
            s.shown(Some(&review)),
            marks(&[("C", Mark::NothingFound)], &[])
        );
        // Kept whatever the ticks say.
        s.keep(review);
        assert_eq!(s.kept, want);
        s.not_sure = true;
        s.nothing_found = false;
        assert_eq!(
            s.shown(None),
            marks(&[("A", Mark::NotSure)], &[("R1", Mark::NotSure)])
        );
        // A reaction review replaces only reactions.
        let review = Review {
            reactions: true,
            looked_up: vec!["R1".into()],
            marks: Marks::default(),
        };
        s.keep(review);
        assert_eq!(
            s.kept,
            marks(&[("A", Mark::NotSure), ("C", Mark::NothingFound)], &[])
        );
    }

    #[test]
    fn a_mark_goes_when_its_item_gets_what_was_looked_for() {
        let mut m = Model::new();
        let a = m.add_species(pd_core::geometry::pt(0.0, 0.0), 80.0, 36.0);
        let b = m.add_species(pd_core::geometry::pt(200.0, 0.0), 80.0, 36.0);
        let r = m.add_reaction(pd_core::geometry::pt(100.0, 0.0));
        let mut s = MarkState {
            kept: marks(
                &[
                    (a.as_str(), Mark::NotSure),
                    (b.as_str(), Mark::NotSure),
                    ("gone", Mark::NotSure),
                ],
                &[(r.as_str(), Mark::NothingFound), ("gone", Mark::NotSure)],
            ),
            ..MarkState::default()
        };
        let reference = |x: &str| Reference {
            qualifier: "bqbiol:is".into(),
            resource: format!("https://identifiers.org/{x}"),
        };
        // A KEGG reference isn't what was looked for; ChEBI is.
        m.find_species_mut(&a)
            .unwrap()
            .annotation
            .references
            .push(reference("kegg.compound:C00092"));
        m.find_species_mut(&b)
            .unwrap()
            .annotation
            .references
            .push(reference("CHEBI:4170"));
        s.prune(&m);
        assert_eq!(
            s.kept,
            marks(
                &[(a.as_str(), Mark::NotSure)],
                &[(r.as_str(), Mark::NothingFound)]
            )
        );
        m.find_reaction_mut(&r)
            .unwrap()
            .annotation
            .references
            .push(reference("ec-code:2.7.1.11"));
        s.prune(&m);
        assert!(s.kept.reactions.is_empty());
    }
}
