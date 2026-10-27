//! Importing and exporting other formats through the view (S §11.2–§11.4).

use super::DiagramView;
use crate::antimony::{self, Diagnostic, ImportOptions};
use crate::sbml;
use crate::view::ViewResponse;

impl DiagramView {
    /// Replaces the diagram with the model in Antimony `text`, as one undo
    /// step (S §11.4). Returns the warnings (statements skipped) and whether
    /// the diagram still needs laying out (the text placed none or not all of
    /// its species with Antimony's layout statements), or the errors, in
    /// which case the diagram is left exactly as it was.
    pub fn import_antimony(
        &mut self,
        text: &str,
        opts: &ImportOptions,
    ) -> Result<(ViewResponse, Vec<Diagnostic>, bool), Vec<Diagnostic>> {
        let imported = antimony::import(text, opts)?;
        self.cancel_layout();
        self.cancel_gesture();
        let before = std::mem::replace(&mut self.model, imported.model);
        let mut r = self.record("Import Antimony", before);
        r.selection_changed = true;
        let needs_layout = !imported.had_layout || imported.unplaced > 0;
        Ok((r, imported.warnings, needs_layout))
    }

    /// The diagram as Antimony text (S §11.3).
    pub fn export_antimony(&self) -> String {
        antimony::export(&self.model)
    }

    /// The diagram as Antimony with its drawing as layout statements
    /// (docs/antimony-layout-export-design.md), and what those can't say.
    pub fn export_antimony_with_layout(&self) -> (String, Vec<String>) {
        (
            antimony::export_with_layout(&self.model),
            antimony::layout_notes(&self.model),
        )
    }

    /// The diagram as Antimony with its drawing as a diagram comment
    /// (docs/diagram-format.md), and with `layout`, Antimony's layout
    /// statements too; and what the export can't keep.
    pub fn export_antimony_with_diagram(&self, layout: bool) -> (String, Vec<String>) {
        let text = if layout {
            antimony::export_with_diagram_and_layout(&self.model)
        } else {
            antimony::export_with_diagram(&self.model)
        };
        (text, antimony::diagram_notes(&self.model, layout))
    }

    /// Replaces the diagram with the model in SBML `xml`, as one undo step
    /// (SPEC-Rust §8.3). Returns what the import reports, or the errors, in
    /// which case the diagram is left exactly as it was.
    pub fn import_sbml(
        &mut self,
        xml: &str,
        opts: &ImportOptions,
    ) -> Result<(ViewResponse, sbml::Imported), Vec<sbml::Diagnostic>> {
        let mut imported = sbml::import(xml, opts)?;
        self.cancel_layout();
        self.cancel_gesture();
        let model = std::mem::take(&mut imported.model);
        let before = std::mem::replace(&mut self.model, model);
        let mut r = self.record("Import SBML", before);
        r.selection_changed = true;
        Ok((r, imported))
    }

    /// Replaces the diagram with an Escher map's (S §11.7), as one undo
    /// step. Returns what couldn't be carried over, or why it isn't a map.
    pub fn import_escher(&mut self, text: &str) -> Result<Vec<String>, String> {
        let imported = crate::escher::import(text)?;
        self.cancel_layout();
        self.cancel_gesture();
        let before = std::mem::replace(&mut self.model, imported.model);
        self.record("Import Escher", before);
        Ok(imported.warnings)
    }

    /// Replaces the diagram with a KEGG map (KGML), as one undo step
    /// (docs/kegg-import-design.md). Returns what the map said besides its
    /// drawing: warnings, its code and title, and its compounds' KEGG numbers.
    /// An error leaves the diagram as it was.
    pub fn import_kegg(&mut self, text: &str) -> Result<crate::kegg::Map, String> {
        let imported = crate::kegg::import(text)?;
        self.cancel_layout();
        self.cancel_gesture();
        let before = std::mem::replace(&mut self.model, imported.model);
        self.record("Import KEGG map", before);
        Ok(imported.map)
    }

    /// Renames a KEGG map's compounds to the names found for them (see
    /// [`crate::kegg::apply_names`]), as one undo step. Returns how many.
    pub fn name_kegg_compounds(
        &mut self,
        compounds: &[(crate::model::Id, String)],
        names: &[(String, String)],
    ) -> usize {
        let before = self.model.clone();
        let n = crate::kegg::apply_names(&mut self.model, compounds, names);
        if n > 0 {
            self.record("Name compounds", before);
        }
        n
    }

    /// The diagram as SBML with its layout (S §11.5).
    pub fn export_sbml(&self) -> sbml::Exported {
        sbml::export(&self.model)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::pt;

    const OPTS: ImportOptions = ImportOptions {
        convert_catalytic: true,
        seed: 1,
    };

    #[test]
    fn import_is_one_undo_step() {
        let mut v = DiagramView::default();
        let mut m = v.model().clone();
        m.add_species(pt(0.0, 0.0), 80.0, 36.0);
        m.find_species_mut("S1").unwrap().selected = true;
        v.set_model(m);
        let (r, warnings, needs_layout) = v.import_antimony("A -> B; k*A\nk = 0.1", &OPTS).unwrap();
        assert!(r.model_changed && r.selection_changed);
        assert!(warnings.is_empty());
        assert!(needs_layout);
        assert_eq!(v.model().reactions().len(), 1);
        assert!(!v.has_selection());
        assert_eq!(v.undo_description(), Some("Import Antimony"));
        v.undo();
        assert_eq!(v.model().species().len(), 1);
        assert_eq!(v.model().species()[0].id, "S1");
    }

    #[test]
    fn antimony_with_layout_carries_the_drawing() {
        let mut v = DiagramView::default();
        v.import_antimony(
            "A -> B; k*A\nk = 0.1\nA.pos = {0, 0}\nB.pos = {200, 0}",
            &OPTS,
        )
        .unwrap();
        let (text, notes) = v.export_antimony_with_layout();
        assert!(
            text.contains("  // Layout\n  model.layout = on\n"),
            "{text}"
        );
        assert!(text.contains("A.position = {0, 0}"), "{text}");
        assert!(notes.is_empty(), "{notes:?}");
        // The plain export is the model alone.
        assert!(!v.export_antimony().contains("model.layout"));
        // What the layout can't say comes with it.
        let mut m = v.model().clone();
        m.find_species_mut("A").unwrap().label_offset = Some(crate::geometry::pt(0.0, 30.0));
        v.set_model(m);
        let (_, notes) = v.export_antimony_with_layout();
        assert!(
            notes.iter().any(|n| n.starts_with("Names drawn outside")),
            "{notes:?}"
        );
    }

    #[test]
    fn a_text_that_places_its_species_needs_no_layout() {
        let mut v = DiagramView::default();
        let (_, _, needs_layout) = v
            .import_antimony(
                "A -> B; k*A\nk = 0.1\nA.pos = {0, 0}\nB.pos = {200, 0}",
                &OPTS,
            )
            .unwrap();
        assert!(!needs_layout, "the text placed every species");
        let (_, _, needs_layout) = v
            .import_antimony("A -> B; k*A\nk = 0.1\nA.pos = {0, 0}", &OPTS)
            .unwrap();
        assert!(needs_layout, "B was left at random");
    }

    #[test]
    fn sbml_import_is_one_undo_step() {
        let mut v = DiagramView::default();
        v.import_antimony(
            "A -> B; k*A
k = 0.1",
            &OPTS,
        )
        .unwrap();
        let xml = v.export_sbml().xml;
        v.import_antimony("X -> Y; 1", &OPTS).unwrap();
        let (r, imported) = v.import_sbml(&xml, &OPTS).unwrap();
        assert!(r.model_changed && imported.had_layout && imported.warnings.is_empty());
        assert!(v.model().find_species("A").is_some());
        assert_eq!(v.undo_description(), Some("Import SBML"));
        v.undo();
        assert!(v.model().find_species("X").is_some());
        // A bad file changes nothing.
        assert!(v.import_sbml("<sbml", &OPTS).is_err());
        assert!(v.model().find_species("X").is_some());
    }

    #[test]
    fn a_bad_import_leaves_the_diagram_alone() {
        // M8 acceptance.
        let mut v = DiagramView::default();
        let before = v.model().clone();
        let errors = v.import_antimony("A -> B; k*(", &OPTS).unwrap_err();
        assert_eq!(errors.len(), 1);
        assert_eq!(v.model(), &before);
        assert!(!v.can_undo());
    }

    #[test]
    fn export_round_trips_through_the_view() {
        let mut v = DiagramView::default();
        v.import_antimony("model m\nS1 -> S2; k1*S1\nk1 = 0.2\nend", &OPTS)
            .unwrap();
        let text = v.export_antimony();
        assert!(text.starts_with("model m\n"));
        assert!(text.contains("R1: S1 -> S2; k1*S1;"));
        assert!(text.contains("k1 = 0.2;"));
    }
}
