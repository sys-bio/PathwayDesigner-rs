//! Corner points on modifier arcs (S §5.7.1): adding, removing and
//! straightening. Dragging one is a gesture (`mod.rs`).

use super::DiagramView;
use crate::geometry::Pt;
use crate::view::ViewResponse;

impl DiagramView {
    /// Where the last context menu was asked for, in world coordinates.
    pub fn context_point(&self) -> Option<Pt> {
        self.context_point
    }

    /// Puts a corner point at `at` on piece `piece` of the arc of reaction
    /// `id`'s modifier `modifier` (pieces counted from the node), between the
    /// piece's two ends. One undo step.
    pub fn add_corner_point(
        &mut self,
        id: &str,
        modifier: usize,
        piece: usize,
        at: Pt,
    ) -> ViewResponse {
        let before = self.model.clone();
        if let Some(m) = self
            .model
            .find_reaction_mut(id)
            .and_then(|r| r.modifiers.get_mut(modifier))
        {
            let index = piece.min(m.points.len());
            m.points.insert(index, at);
        }
        self.record_if_changed("Add Corner Point", before)
    }

    /// Takes corner point `point` off the arc of reaction `id`'s modifier
    /// `modifier`. One undo step.
    pub fn remove_corner_point(&mut self, id: &str, modifier: usize, point: usize) -> ViewResponse {
        let before = self.model.clone();
        if let Some(m) = self
            .model
            .find_reaction_mut(id)
            .and_then(|r| r.modifiers.get_mut(modifier))
            && point < m.points.len()
        {
            m.points.remove(point);
        }
        self.record_if_changed("Remove Corner Point", before)
    }

    /// Takes every corner point off the arc of reaction `id`'s modifier
    /// `modifier`, so it is straight again. One undo step.
    pub fn straighten_modifier_line(&mut self, id: &str, modifier: usize) -> ViewResponse {
        let before = self.model.clone();
        if let Some(m) = self
            .model
            .find_reaction_mut(id)
            .and_then(|r| r.modifiers.get_mut(modifier))
        {
            m.points.clear();
        }
        self.record_if_changed("Straighten Modifier Line", before)
    }

    /// True if the arc of reaction `id`'s modifier `modifier` has corner
    /// points (Straighten Modifier Line has something to do).
    pub fn modifier_line_bent(&self, id: &str, modifier: usize) -> bool {
        self.model
            .find_reaction(id)
            .and_then(|r| r.modifiers.get(modifier))
            .is_some_and(|m| !m.points.is_empty())
    }
}
