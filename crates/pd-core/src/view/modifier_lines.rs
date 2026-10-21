//! Corner points on modifier arcs (S §5.7.1) and on reactant and product
//! lines (S §5.6.5): adding, removing and straightening. Dragging one is a
//! gesture (`mod.rs`).

use super::DiagramView;
use crate::bezier::{Ctrl, leg_inner_end};
use crate::model::{LinePattern, ModifierLook, Reaction, Rgba, Role};

/// The undo step of moving a corner point, by dragging or with the keys.
const CORNER_NUDGE: &str = "Move Corner Point";
use crate::geometry::Pt;
use crate::view::ViewResponse;

/// A line of a reaction that may have corner points: its role and its index
/// in that list.
pub type LineRef = (Role, usize);

/// Changes the corner points of line `line` of `r`. On a reactant's or
/// product's line with stored handles, the handle by the end piece's far end
/// moves as much as that end does, so the end piece keeps its shape
/// (S §5.6.5).
pub(super) fn edit_points(
    r: &mut Reaction,
    (role, index): LineRef,
    change: impl FnOnce(&mut Vec<Pt>),
) {
    let Some(p) = r.participants(role).get(index) else {
        return;
    };
    let before = leg_inner_end(r, role, p);
    let p = &mut r.participants_mut(role)[index];
    change(&mut p.points);
    if role == Role::Modifier || !p.ctrl_pts_set {
        return;
    }
    let p = r.participants(role)[index].clone();
    let shift = leg_inner_end(r, role, &p) - before;
    let p = &mut r.participants_mut(role)[index];
    let inner = Ctrl::inner(role);
    inner.set(p, inner.get(p) + shift);
}

impl DiagramView {
    /// The current corner point, where it is (S §5.7.1, §5.6.5): the one
    /// last clicked or dragged, while its reaction is selected and its line
    /// shown.
    pub fn current_corner_point(&self) -> Option<Pt> {
        let (id, (role, index), point) = self.current_corner.as_ref()?;
        if *role == Role::Modifier && !self.show_modifiers() {
            return None;
        }
        let r = self.model.find_reaction(id).filter(|r| r.selected)?;
        r.participants(*role)
            .get(*index)?
            .points
            .get(*point)
            .copied()
    }

    /// Moves the current corner point by `delta` (S §5.7.1), as an arrow key
    /// does. Further moves of the same point add to the same undo step.
    /// Returns `None` when there is no current point.
    pub(super) fn nudge_corner(&mut self, delta: Pt) -> Option<ViewResponse> {
        self.current_corner_point()?;
        let (id, line, point) = self.current_corner.clone()?;
        if delta == Pt::ZERO || self.is_busy() {
            return Some(ViewResponse::default());
        }
        let before = self.model.clone();
        if let Some(r) = self.model.find_reaction_mut(&id) {
            edit_points(r, line, |pts| {
                if let Some(p) = pts.get_mut(point) {
                    *p += delta;
                }
            });
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

    /// Puts a corner point at `at` on piece `piece` of line `line` of
    /// reaction `id` (pieces counted from the node), between the piece's two
    /// ends. One undo step.
    pub fn add_corner_point(
        &mut self,
        id: &str,
        line: LineRef,
        piece: usize,
        at: Pt,
    ) -> ViewResponse {
        self.current_corner = None;
        let before = self.model.clone();
        if let Some(r) = self.model.find_reaction_mut(id) {
            edit_points(r, line, |pts| {
                let index = piece.min(pts.len());
                pts.insert(index, at);
            });
        }
        self.record_if_changed("Add Corner Point", before)
    }

    /// Takes corner point `point` off line `line` of reaction `id`. One undo
    /// step.
    pub fn remove_corner_point(&mut self, id: &str, line: LineRef, point: usize) -> ViewResponse {
        self.current_corner = None;
        let before = self.model.clone();
        if let Some(r) = self.model.find_reaction_mut(id) {
            edit_points(r, line, |pts| {
                if point < pts.len() {
                    pts.remove(point);
                }
            });
        }
        self.record_if_changed("Remove Corner Point", before)
    }

    /// Takes every corner point off line `line` of reaction `id`, so it is
    /// straight again: Straighten Modifier Line (S §5.7.1), or Straighten
    /// Line for a reactant's or product's (S §5.6.5). One undo step.
    pub fn straighten_line(&mut self, id: &str, line: LineRef) -> ViewResponse {
        self.current_corner = None;
        let before = self.model.clone();
        if let Some(r) = self.model.find_reaction_mut(id) {
            edit_points(r, line, Vec::clear);
        }
        let desc = if line.0 == Role::Modifier {
            "Straighten Modifier Line"
        } else {
            "Straighten Line"
        };
        self.record_if_changed(desc, before)
    }

    /// True if line `line` of reaction `id` has corner points (Straighten
    /// has something to do).
    pub fn line_bent(&self, id: &str, (role, index): LineRef) -> bool {
        self.model
            .find_reaction(id)
            .and_then(|r| r.participants(role).get(index))
            .is_some_and(|p| !p.points.is_empty())
    }

    // ----- A modifier line's own look (S §5.7.2) ----------------------------

    /// The look of reaction `id`'s modifier `modifier`'s line.
    pub fn modifier_look(&self, id: &str, modifier: usize) -> Option<ModifierLook> {
        self.model
            .find_reaction(id)?
            .modifiers
            .get(modifier)
            .map(|m| m.look)
    }

    fn edit_modifier_look(
        &mut self,
        id: &str,
        modifier: usize,
        desc: &'static str,
        change: impl FnOnce(&mut ModifierLook),
    ) -> ViewResponse {
        let before = self.model.clone();
        if let Some(m) = self
            .model
            .find_reaction_mut(id)
            .and_then(|r| r.modifiers.get_mut(modifier))
        {
            change(&mut m.look);
        }
        self.record_if_changed(desc, before)
    }

    /// The line's colour; `None` puts the default back.
    pub fn set_modifier_colour(
        &mut self,
        id: &str,
        modifier: usize,
        colour: Option<Rgba>,
    ) -> ViewResponse {
        self.edit_modifier_look(id, modifier, "Modifier Line Colour", |l| l.colour = colour)
    }

    /// The line's width, kept within [`ModifierLook::WIDTH_RANGE`]; `None`
    /// puts the default back.
    pub fn set_modifier_width(
        &mut self,
        id: &str,
        modifier: usize,
        width: Option<f64>,
    ) -> ViewResponse {
        let (lo, hi) = ModifierLook::WIDTH_RANGE;
        let width = width.filter(|w| w.is_finite()).map(|w| w.clamp(lo, hi));
        self.edit_modifier_look(id, modifier, "Modifier Line Width", |l| l.width = width)
    }

    pub fn set_modifier_pattern(
        &mut self,
        id: &str,
        modifier: usize,
        pattern: LinePattern,
    ) -> ViewResponse {
        self.edit_modifier_look(id, modifier, "Modifier Line Pattern", |l| {
            l.pattern = pattern
        })
    }

    /// The symbol's size, kept within [`ModifierLook::SIZE_RANGE`]; `None`
    /// puts the kind's default back.
    pub fn set_modifier_symbol_size(
        &mut self,
        id: &str,
        modifier: usize,
        size: Option<f64>,
    ) -> ViewResponse {
        let (lo, hi) = ModifierLook::SIZE_RANGE;
        let size = size.filter(|v| v.is_finite()).map(|v| v.clamp(lo, hi));
        self.edit_modifier_look(id, modifier, "Modifier Symbol Size", |l| {
            l.symbol_size = size
        })
    }

    /// The symbol's distance from the junction, kept within
    /// [`ModifierLook::GAP_RANGE`]; `None` puts the kind's default back.
    pub fn set_modifier_symbol_gap(
        &mut self,
        id: &str,
        modifier: usize,
        gap: Option<f64>,
    ) -> ViewResponse {
        let (lo, hi) = ModifierLook::GAP_RANGE;
        let gap = gap.filter(|v| v.is_finite()).map(|v| v.clamp(lo, hi));
        self.edit_modifier_look(id, modifier, "Modifier Symbol Distance", |l| {
            l.symbol_gap = gap
        })
    }

    /// Apply to All: every other modifier line of reaction `id` takes this
    /// one's look (not its kind or corner points). One undo step.
    pub fn apply_modifier_look_to_all(&mut self, id: &str, modifier: usize) -> ViewResponse {
        let before = self.model.clone();
        if let Some(r) = self.model.find_reaction_mut(id)
            && let Some(look) = r.modifiers.get(modifier).map(|m| m.look)
        {
            for m in &mut r.modifiers {
                m.look = look;
            }
        }
        self.record_if_changed("Apply to All Modifier Lines", before)
    }
}
