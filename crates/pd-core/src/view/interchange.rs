//! Importing and exporting other formats through the view (S §11.2–§11.4).

use super::DiagramView;
use crate::antimony::{self, Diagnostic, ImportOptions};
use crate::view::ViewResponse;

impl DiagramView {
    /// Replaces the diagram with the model in Antimony `text`, as one undo
    /// step (S §11.4). Returns the warnings (statements skipped), or the
    /// errors, in which case the diagram is left exactly as it was.
    pub fn import_antimony(
        &mut self,
        text: &str,
        opts: &ImportOptions,
    ) -> Result<(ViewResponse, Vec<Diagnostic>), Vec<Diagnostic>> {
        let imported = antimony::import(text, opts)?;
        self.cancel_layout();
        self.cancel_gesture();
        let before = std::mem::replace(&mut self.model, imported.model);
        let mut r = self.record("Import Antimony", before);
        r.selection_changed = true;
        Ok((r, imported.warnings))
    }

    /// The diagram as Antimony text (S §11.3).
    pub fn export_antimony(&self) -> String {
        antimony::export(&self.model)
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
        let (r, warnings) = v.import_antimony("A -> B; k*A\nk = 0.1", &OPTS).unwrap();
        assert!(r.model_changed && r.selection_changed);
        assert!(warnings.is_empty());
        assert_eq!(v.model().reactions().len(), 1);
        assert!(!v.has_selection());
        assert_eq!(v.undo_description(), Some("Import Antimony"));
        v.undo();
        assert_eq!(v.model().species().len(), 1);
        assert_eq!(v.model().species()[0].id, "S1");
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
