//! The Events & Rules tab's edits through the view, each one undo step
//! (docs/sbml-app-design.md A1).

use super::DiagramView;
use crate::drafts::{self, Draft, Problem};
use crate::sentences::Part;
use crate::view::ViewResponse;

impl DiagramView {
    /// Puts `draft` in the model in place of `part`, or as a new part, as one
    /// undo step. Returns where it now is, or, leaving the model as it was,
    /// what is wrong with it.
    pub fn save_part(
        &mut self,
        part: Option<Part>,
        draft: &Draft,
    ) -> Result<(ViewResponse, Part), Vec<Problem>> {
        let errors: Vec<Problem> = drafts::check(&self.model, part, draft)
            .into_iter()
            .filter(|p| p.error)
            .collect();
        if !errors.is_empty() {
            return Err(errors);
        }
        let before = self.model.clone();
        let Some(at) = drafts::apply(&mut self.model, part, draft) else {
            self.model = before;
            return Err(Vec::new());
        };
        let desc = match (part.is_some(), draft) {
            (true, Draft::Event(_)) => "Edit Event",
            (false, Draft::Event(_)) => "Add Event",
            (true, Draft::Rule(_)) => "Edit Rule",
            (false, Draft::Rule(_)) => "Add Rule",
            (true, _) => "Edit",
            (false, _) => "Add",
        };
        Ok((self.record(desc, before), at))
    }

    /// Deletes `part`, as one undo step; not a function or unit still used,
    /// for which it says why.
    pub fn delete_part(&mut self, part: Part) -> Result<ViewResponse, String> {
        if let Some(why) = drafts::still_used(&self.model, part) {
            return Err(why);
        }
        let before = self.model.clone();
        drafts::delete(&mut self.model, part);
        Ok(self.record("Delete", before))
    }
}

#[cfg(test)]
mod tests {
    use crate::model::{Rgba, TextFont};
    use crate::render::{Renderer, SPt, SRect, Stroke};

    /// Counts the filled polygons drawn.
    #[derive(Default)]
    struct Polygons(usize);
    impl Renderer for Polygons {
        fn round_rect(&mut self, _: SRect, _: f32, _: Option<Rgba>, _: Option<Stroke>) {}
        fn circle(&mut self, _: SPt, _: f32, _: Option<Rgba>, _: Option<Stroke>) {}
        fn polyline(&mut self, _: &[SPt], _: Stroke) {}
        fn cubic(&mut self, _: SPt, _: SPt, _: SPt, _: SPt, _: Stroke) {}
        fn polygon(&mut self, _: &[SPt], _: Rgba) {
            self.0 += 1;
        }
        fn text(&mut self, _: SPt, _: &str, _: f32, _: TextFont, _: Rgba) {}
    }

    /// docs/sbml-app-design.md A2: the view draws the lightning bolts (two
    /// pieces each) unless View ▸ Rule and Event Marks is off.
    #[test]
    fn the_view_draws_bolts_unless_told_not_to() {
        let m = crate::antimony::import(
            "model m\n  species S1, S2\n  J1: S1 -> S2; k*S1\n  k = 1\n  E1: at time > 1: S1 = 5\nend",
            &crate::antimony::ImportOptions {
                seed: 1,
            },
        )
        .unwrap()
        .model;
        let mut v = super::DiagramView::new(m);
        let drawn = |v: &super::DiagramView| {
            let mut p = Polygons::default();
            v.draw(&mut p, SRect::default());
            p.0
        };
        let with = drawn(&v);
        v.show_rule_marks = false;
        assert_eq!(with - drawn(&v), 2);
    }
}
