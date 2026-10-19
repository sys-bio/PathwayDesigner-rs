//! Corner points on modifier arcs (S §5.7.1): adding, removing and
//! straightening. Dragging one is a gesture (`mod.rs`).

use super::DiagramView;

/// The undo step of moving a corner point, by dragging or with the keys.
const CORNER_NUDGE: &str = "Move Corner Point";
use crate::geometry::Pt;
use crate::view::ViewResponse;

impl DiagramView {
    /// The current corner point, where it is (S §5.7.1): the one last
    /// clicked or dragged, while its reaction is selected and its arc shown.
    pub fn current_corner_point(&self) -> Option<Pt> {
        let (id, modifier, point) = self.current_corner.as_ref()?;
        if !self.show_modifiers {
            return None;
        }
        let r = self.model.find_reaction(id).filter(|r| r.selected)?;
        r.modifiers.get(*modifier)?.points.get(*point).copied()
    }

    /// Moves the current corner point by `delta` (S §5.7.1), as an arrow key
    /// does. Further moves of the same point add to the same undo step.
    /// Returns `None` when there is no current point.
    pub(super) fn nudge_corner(&mut self, delta: Pt) -> Option<ViewResponse> {
        self.current_corner_point()?;
        let (id, modifier, point) = self.current_corner.clone()?;
        if delta == Pt::ZERO || self.is_busy() {
            return Some(ViewResponse::default());
        }
        let before = self.model.clone();
        if let Some(p) = self
            .model
            .find_reaction_mut(&id)
            .and_then(|r| r.modifiers.get_mut(modifier))
            .and_then(|m| m.points.get_mut(point))
        {
            *p += delta;
        }
        let selection = self.selection_state();
        let same_run = self.nudged.as_ref() == Some(&(self.undo.version(), selection.clone()))
            && self.undo.undo_description() == Some(CORNER_NUDGE);
        let response = if same_run && self.undo.amend_last(self.model.clone()) {
            ViewResponse {
                repaint: true,
                model_changed: true,
                ..ViewResponse::default()
            }
        } else {
            self.record(CORNER_NUDGE, before)
        };
        self.nudged = Some((self.undo.version(), selection));
        Some(response)
    }

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
        self.current_corner = None;
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
        self.current_corner = None;
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
        self.current_corner = None;
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
