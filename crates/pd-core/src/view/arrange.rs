//! Layout tools in the view: auto-layout, align and distribute, and the
//! random network (S §7.5, §7.7, §10, SPEC-Rust §7.2).

use super::{DiagramView, Gesture};
use crate::bezier::{Ctrl, apply_smooth_handles};
use crate::generate;
use crate::geometry::{Pt, pt};
use crate::layout::{LayoutParams, LayoutRun, Progress};
use crate::model::{Id, Model, ReactionMode, Role};
use crate::render::draw::{effective_mode, linear_junction_t, linear_line};
use crate::view::ViewResponse;

/// A layout in progress, and the model before it started.
#[derive(Clone, Debug)]
pub(super) struct LayoutState {
    run: LayoutRun,
    before: Box<Model>,
    /// Which compartment box sits in which at the start: boxes are refitted
    /// around their species as the layout moves them (S §7.9).
    nest: Vec<(Id, Id)>,
}

/// The undo step a run of keyboard nudges makes.
const NUDGE: &str = "Nudge";
/// How far an arrow key moves the selection without snapping, in diagram
/// units; with Shift.
pub const NUDGE_STEP: f64 = 1.0;
pub const NUDGE_STEP_SHIFT: f64 = 10.0;

/// Something align and distribute move: a species by its box, or a
/// junction as a point.
#[derive(Clone, Debug)]
struct Item {
    what: ItemKind,
    center: Pt,
    hw: f64,
    hh: f64,
    locked: bool,
}

#[derive(Clone, Debug, PartialEq)]
enum ItemKind {
    Species(Id),
    Junction(Id),
    /// Free text, by its box (S §7.8).
    Text(Id),
}

/// The align and distribute commands, and Arrange in Circle (S §7.5).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Arrange {
    Left,
    Center,
    Right,
    Top,
    Middle,
    Bottom,
    DistributeHorizontally,
    DistributeVertically,
    /// Arrange in Circle (S §7.5.1).
    Circle,
}

impl Arrange {
    pub const ALL: [Arrange; 9] = [
        Arrange::Left,
        Arrange::Center,
        Arrange::Right,
        Arrange::Top,
        Arrange::Middle,
        Arrange::Bottom,
        Arrange::DistributeHorizontally,
        Arrange::DistributeVertically,
        Arrange::Circle,
    ];

    /// True if a menu separator goes before this command.
    pub fn starts_group(self) -> bool {
        matches!(self, Arrange::DistributeHorizontally | Arrange::Circle)
    }

    pub fn label(self) -> &'static str {
        match self {
            Arrange::Left => "Align Left",
            Arrange::Center => "Align Center",
            Arrange::Right => "Align Right",
            Arrange::Top => "Align Top",
            Arrange::Middle => "Align Middle",
            Arrange::Bottom => "Align Bottom",
            Arrange::DistributeHorizontally => "Distribute Horizontally",
            Arrange::DistributeVertically => "Distribute Vertically",
            Arrange::Circle => "Arrange in Circle",
        }
    }

    /// How many selected species and junctions the command needs; for
    /// Arrange in Circle, how many unlocked selected species.
    pub fn minimum(self) -> usize {
        match self {
            Arrange::DistributeHorizontally | Arrange::DistributeVertically | Arrange::Circle => 3,
            _ => 2,
        }
    }
}

impl DiagramView {
    // ----- Auto-layout ------------------------------------------------------

    /// Starts an auto-layout. Each step is drawn as it goes; the model is
    /// only committed, as one undo step, when the run finishes.
    ///
    /// While it runs, curves keep their shape: stored handles travel with the
    /// end they belong to, and smooth junctions are recomputed every step.
    /// Randomize starts from automatic handles, as S §10.1 says. When the run
    /// finishes, handles are reset, recomputed or set by the Deckard pass
    /// (S §10.1, D-11).
    pub fn start_layout(&mut self, params: LayoutParams, seed: u64) -> ViewResponse {
        if self.layout.is_some() {
            return ViewResponse::default();
        }
        self.cancel_gesture();
        let before = Box::new(self.model.clone());
        if params.randomize {
            for r in self.model.reactions_mut() {
                for p in r.reactants.iter_mut().chain(&mut r.products) {
                    p.ctrl_pts_set = false;
                }
            }
        }
        let run = LayoutRun::new(&self.model, params, seed);
        run.apply_positions(&mut self.model);
        let nest = crate::compartments::nesting(&self.model);
        self.layout = Some(LayoutState { run, before, nest });
        self.follow_handles();
        ViewResponse {
            repaint: true,
            ..ViewResponse::default()
        }
    }

    pub fn is_laying_out(&self) -> bool {
        self.layout.is_some()
    }

    pub fn layout_progress(&self) -> Option<Progress> {
        self.layout.as_ref().map(|l| l.run.progress())
    }

    /// Runs up to `iterations` more iterations and shows the result. When the
    /// run is done it is finished and recorded as one undo step.
    pub fn layout_step(&mut self, iterations: u32) -> ViewResponse {
        let Some(state) = &mut self.layout else {
            return ViewResponse::default();
        };
        let progress = state.run.step(iterations);
        state.run.apply_positions(&mut self.model);
        self.follow_handles();
        if !progress.finished() {
            return ViewResponse {
                repaint: true,
                ..ViewResponse::default()
            };
        }
        let state = self.layout.take().expect("checked above");
        state.run.finish(&mut self.model);
        crate::compartments::refit(&mut self.model, &state.nest);
        self.record("Auto-layout", *state.before)
    }

    /// During a layout: puts every stored handle at its position before the
    /// layout plus the movement of the end it belongs to (the species for an
    /// outer handle, the junction for an inner one), and recomputes smooth
    /// junctions, so curves keep their shape while the nodes move.
    fn follow_handles(&mut self) {
        let Some(state) = self.layout.take() else {
            return;
        };
        // Species keep their compartments; the boxes follow them.
        crate::compartments::refit(&mut self.model, &state.nest);
        let before = &state.before;
        let mut smooth = Vec::new();
        let moved_species =
            |m: &Model, id: &str| match (m.find_species(id), before.find_species(id)) {
                (Some(now), Some(then)) => now.center - then.center,
                _ => Pt::ZERO,
            };
        let shifts: Vec<(Id, Pt, Vec<Pt>)> = self
            .model
            .reactions()
            .iter()
            .filter_map(|r| {
                let old = before.find_reaction(&r.id)?;
                let ends = r
                    .reactants
                    .iter()
                    .chain(&r.products)
                    .map(|p| moved_species(&self.model, &p.species))
                    .collect();
                Some((r.id.clone(), r.junction - old.junction, ends))
            })
            .collect();
        for (id, junction_shift, ends) in shifts {
            let old = before.find_reaction(&id).expect("found above");
            let r = self.model.find_reaction_mut(&id).expect("found above");
            if r.mode == ReactionMode::Bezier && r.junction_smooth {
                smooth.push(id);
                continue;
            }
            let nr = r.reactants.len();
            for (role, list, old_list) in [
                (Role::Reactant, &mut r.reactants, &old.reactants),
                (Role::Product, &mut r.products, &old.products),
            ] {
                for (i, (p, o)) in list.iter_mut().zip(old_list).enumerate() {
                    if !p.ctrl_pts_set {
                        continue;
                    }
                    let own = ends[if role == Role::Reactant { i } else { nr + i }];
                    let (outer, inner) = (Ctrl::outer(role), Ctrl::inner(role));
                    outer.set(p, outer.get(o) + own);
                    inner.set(p, inner.get(o) + junction_shift);
                }
            }
        }
        for id in smooth {
            apply_smooth_handles(&mut self.model, &id);
        }
        self.layout = Some(state);
    }

    /// Stops the layout and puts the model back as it was.
    pub fn cancel_layout(&mut self) -> ViewResponse {
        let Some(state) = self.layout.take() else {
            return ViewResponse::default();
        };
        self.model = *state.before;
        ViewResponse {
            repaint: true,
            ..ViewResponse::default()
        }
    }

    // ----- Random network (S §7.7) ------------------------------------------

    /// Replaces the model with a random network. One undo step.
    pub fn random_network(&mut self, seed: u64) -> ViewResponse {
        if self.layout.is_some() {
            return ViewResponse::default();
        }
        self.cancel_gesture();
        let before = std::mem::replace(&mut self.model, generate::random_network(seed));
        let mut r = self.record("Random Network", before);
        r.selection_changed = true;
        r
    }

    // ----- Align and distribute (S §7.5) ------------------------------------

    /// What align and distribute act on: the selected species and texts, by
    /// their boxes, and the junctions of the selected reactions, as points. A
    /// Linear reaction's junction is left out: it stays on its line.
    fn arrange_items(&self) -> Vec<Item> {
        let species = self.model.selected_species().map(|s| Item {
            what: ItemKind::Species(s.id.clone()),
            center: s.center,
            hw: s.width / 2.0,
            hh: s.height / 2.0,
            locked: s.locked,
        });
        let junctions = self
            .model
            .selected_reactions()
            .filter(|r| effective_mode(r) != ReactionMode::Linear)
            .map(|r| Item {
                what: ItemKind::Junction(r.id.clone()),
                center: r.junction,
                hw: 0.0,
                hh: 0.0,
                locked: false,
            });
        let texts = self.model.selected_texts().map(|t| {
            let (min, max) = crate::textbox::text_box(t);
            Item {
                what: ItemKind::Text(t.id.clone()),
                center: (min + max) / 2.0,
                hw: (max.x - min.x) / 2.0,
                hh: (max.y - min.y) / 2.0,
                locked: false,
            }
        });
        species.chain(junctions).chain(texts).collect()
    }

    /// True if `cmd` can do anything with the current selection.
    pub fn can_arrange(&self, cmd: Arrange) -> bool {
        if cmd == Arrange::Circle {
            return self.circle_species().len() >= cmd.minimum();
        }
        self.arrange_items().len() >= cmd.minimum()
    }

    /// Aligns or distributes the selected species and junctions. Locked
    /// species don't move, but still count towards the bounding box, so others
    /// can be aligned to a locked one; when distributing they keep their
    /// places. Junctions and handles follow moved species (see
    /// [`DiagramView::moved_species`]); a selected junction then goes to its
    /// own place. One undo step.
    pub fn arrange(&mut self, cmd: Arrange) -> ViewResponse {
        if self.layout.is_some() || !matches!(self.gesture, Gesture::Idle) || !self.can_arrange(cmd)
        {
            return ViewResponse::default();
        }
        if cmd == Arrange::Circle {
            return self.arrange_circle();
        }
        let sel = self.arrange_items();
        let left = sel
            .iter()
            .map(|s| s.center.x - s.hw)
            .fold(f64::INFINITY, f64::min);
        let right = sel
            .iter()
            .map(|s| s.center.x + s.hw)
            .fold(f64::NEG_INFINITY, f64::max);
        let top = sel
            .iter()
            .map(|s| s.center.y - s.hh)
            .fold(f64::INFINITY, f64::min);
        let bottom = sel
            .iter()
            .map(|s| s.center.y + s.hh)
            .fold(f64::NEG_INFINITY, f64::max);

        let mut targets: Vec<(ItemKind, Pt)> = Vec::new();
        match cmd {
            Arrange::DistributeHorizontally | Arrange::DistributeVertically => {
                let horizontal = cmd == Arrange::DistributeHorizontally;
                let key = |p: Pt| if horizontal { p.x } else { p.y };
                let mut order = sel.clone();
                order.sort_by(|a, b| key(a.center).total_cmp(&key(b.center)));
                let (first, last) = (key(order[0].center), key(order[order.len() - 1].center));
                let n = order.len() - 1;
                for (i, s) in order.iter().enumerate().skip(1).take(n - 1) {
                    let v = first + (last - first) * i as f64 / n as f64;
                    let at = if horizontal {
                        pt(v, s.center.y)
                    } else {
                        pt(s.center.x, v)
                    };
                    targets.push((s.what.clone(), at));
                }
            }
            _ => {
                for s in &sel {
                    let (c, hw, hh) = (s.center, s.hw, s.hh);
                    let at = match cmd {
                        Arrange::Left => pt(left + hw, c.y),
                        Arrange::Center => pt((left + right) / 2.0, c.y),
                        Arrange::Right => pt(right - hw, c.y),
                        Arrange::Top => pt(c.x, top + hh),
                        Arrange::Middle => pt(c.x, (top + bottom) / 2.0),
                        _ => pt(c.x, bottom - hh),
                    };
                    targets.push((s.what.clone(), at));
                }
            }
        }

        let before = self.model.clone();
        let linear = self.linear_positions();
        let locked: Vec<&ItemKind> = sel.iter().filter(|s| s.locked).map(|s| &s.what).collect();
        let mut moved = Vec::new();
        for (what, at) in &targets {
            let ItemKind::Species(id) = what else {
                continue;
            };
            if locked.contains(&what) {
                continue;
            }
            let Some(s) = self.model.find_species_mut(id) else {
                continue;
            };
            if s.center == *at {
                continue;
            }
            moved.push((id.clone(), *at - s.center));
            s.center = *at;
        }
        self.moved_species(&moved);
        self.keep_linear(&linear, &moved);
        let ids: Vec<Id> = moved.iter().map(|(id, _)| id.clone()).collect();
        crate::compartments::reassign(&mut self.model, &ids);
        for (what, at) in targets {
            if let ItemKind::Junction(id) = &what
                && let Some(j) = self.model.find_reaction(id).map(|r| r.junction)
                && j != at
            {
                self.shift_junction(id, at - j);
            }
            if let ItemKind::Text(id) = &what
                && let Some(t) = self.model.find_text_mut(id)
            {
                let (min, max) = crate::textbox::text_box(t);
                t.position += at - (min + max) / 2.0;
            }
        }
        if self.model == before {
            return ViewResponse::default();
        }
        self.record(cmd.label(), before)
    }

    /// Where each Linear junction is along its line, to keep it there.
    pub(super) fn linear_positions(&self) -> Vec<(Id, f64)> {
        self.model
            .reactions()
            .iter()
            .filter_map(|r| linear_junction_t(&self.model, r).map(|t| (r.id.clone(), t)))
            .collect()
    }

    /// Puts each Linear junction back at its place along its line, if one of
    /// its ends moved.
    pub(super) fn keep_linear(&mut self, linear: &[(Id, f64)], moved: &[(Id, Pt)]) {
        for (id, t) in linear {
            let ends_moved = self.model.find_reaction(id).is_some_and(|r| {
                r.reactants
                    .iter()
                    .chain(&r.products)
                    .any(|p| moved.iter().any(|(m, _)| *m == p.species))
            });
            let line = self
                .model
                .find_reaction(id)
                .and_then(|r| linear_line(&self.model, r));
            if let (true, Some((a, b))) = (ends_moved, line) {
                self.model
                    .find_reaction_mut(id)
                    .expect("found above")
                    .junction = a + (b - a) * *t;
            }
        }
    }

    /// Moves a junction as dragging it would: a smooth junction's inner
    /// handles go with it.
    fn shift_junction(&mut self, id: &str, delta: Pt) {
        let mut drag = self.begin_junction_drag(Pt::ZERO, id.to_owned());
        drag.delta = delta;
        self.apply_junction_drag(&drag);
    }

    // ----- Keyboard nudge ----------------------------------------------------

    /// An arrow key (S §7.5): moves the selection one step in direction
    /// `dir` (a unit vector along an axis), a big step with Shift. Without
    /// snapping, or with Alt (`free`), the steps are [`NUDGE_STEP`] and
    /// [`NUDGE_STEP_SHIFT`]. While snapping to the grid (S §7.6), the first
    /// selected item moves to the next grid line that way (five lines with
    /// Shift), and the rest by as much; across, nothing moves.
    pub fn nudge_key(&mut self, dir: Pt, big: bool, free: bool) -> ViewResponse {
        use crate::render::constants::{GRID_MAJOR_EVERY, GRID_SPACING};
        if !self.snap_to_grid || free {
            let step = if big { NUDGE_STEP_SHIFT } else { NUDGE_STEP };
            return self.nudge(dir * step);
        }
        let lines = if big { GRID_MAJOR_EVERY as f64 } else { 1.0 };
        let Some(a) = self.nudge_anchor() else {
            return self.nudge(dir * (GRID_SPACING * lines));
        };
        // The next grid line beyond `v` that way, then more lines with Shift.
        let along = |v: f64, d: f64| -> f64 {
            if d == 0.0 {
                return 0.0;
            }
            let k = v / GRID_SPACING;
            let next = if d > 0.0 {
                k.floor() + 1.0
            } else {
                k.ceil() - 1.0
            };
            (next + d * (lines - 1.0)) * GRID_SPACING - v
        };
        self.nudge(pt(along(a.x, dir.x), along(a.y, dir.y)))
    }

    /// The point of the first selected item that lands on the grid when
    /// arrow keys move the selection: a species' centre (not a locked one,
    /// which stays), else a text's top-left corner, a compartment's box's
    /// top-left corner, or a reaction's junction.
    fn nudge_anchor(&self) -> Option<Pt> {
        let m = &self.model;
        m.selected_species()
            .find(|s| !s.locked)
            .map(|s| s.center)
            .or_else(|| m.selected_texts().next().map(|t| t.position))
            .or_else(|| {
                m.selected_compartments()
                    .find_map(|c| c.drawn_frame())
                    .map(|f| f.min())
            })
            .or_else(|| m.selected_reactions().next().map(|r| r.junction))
    }

    /// Moves the selected species and the junctions of the selected reactions
    /// by `delta` (world units), as dragging them would: locked species stay,
    /// a junction follows when all its species move, stored handles follow
    /// their end. While the selection stays the same and nothing else
    /// happens, further nudges add to the same undo step.
    pub fn nudge(&mut self, delta: Pt) -> ViewResponse {
        if self.layout.is_some() || self.is_busy() || delta == Pt::ZERO {
            return ViewResponse::default();
        }
        let selection = self.selection_state();
        if !self.has_selection() {
            return ViewResponse::default();
        }
        let before = self.model.clone();
        let mut drag = self.begin_node_drag(Pt::ZERO);
        drag.delta = delta;
        self.apply_node_drag(&drag);
        crate::compartments::reassign(&mut self.model, &drag.reassign);
        for id in &selection.0.1 {
            let followed = drag.junctions.iter().any(|(j, _)| j == id);
            if !followed {
                self.shift_junction(id, delta);
            }
        }
        if self.model == before {
            return ViewResponse::default();
        }
        let same_run = self.nudged.as_ref() == Some(&(self.undo.version(), selection.clone()))
            && self.undo.undo_description() == Some(NUDGE);
        let response = if same_run && self.undo.amend_last(self.model.clone()) {
            ViewResponse {
                repaint: true,
                model_changed: true,
                ..ViewResponse::default()
            }
        } else {
            self.record(NUDGE, before)
        };
        self.nudged = Some((self.undo.version(), selection));
        response
    }

    /// Passes species movements on to the reactions (S §7.5):
    /// - if every reactant and product moved, the junction moves by the mean
    ///   of their movements, stored handles move with the end they belong to,
    ///   and a smooth junction gets its handles recomputed;
    /// - if only some moved, the junction stays, and the moved participants of
    ///   a Bézier reaction go back to automatic handles.
    pub(super) fn moved_species(&mut self, moved: &[(Id, Pt)]) {
        let delta_of = |id: &str| moved.iter().find(|(m, _)| m == id).map(|(_, d)| *d);
        let mut smooth = Vec::new();
        for r in self.model.reactions_mut() {
            let ends: Vec<Option<Pt>> = r
                .reactants
                .iter()
                .chain(&r.products)
                .map(|p| delta_of(&p.species))
                .collect();
            if ends.is_empty() || ends.iter().all(Option::is_none) {
                continue;
            }
            if ends.iter().all(Option::is_some) {
                let shift = ends.iter().flatten().fold(Pt::ZERO, |a, &d| a + d) / ends.len() as f64;
                r.junction += shift;
                for (role, list) in [
                    (Role::Reactant, &mut r.reactants),
                    (Role::Product, &mut r.products),
                ] {
                    for p in list.iter_mut().filter(|p| p.ctrl_pts_set) {
                        let own = delta_of(&p.species).unwrap_or(Pt::ZERO);
                        let outer = Ctrl::outer(role);
                        let inner = Ctrl::inner(role);
                        outer.set(p, outer.get(p) + own);
                        inner.set(p, inner.get(p) + shift);
                    }
                }
                if r.mode == ReactionMode::Bezier && r.junction_smooth {
                    smooth.push(r.id.clone());
                }
            } else if r.mode == ReactionMode::Bezier {
                for p in r.reactants.iter_mut().chain(&mut r.products) {
                    if delta_of(&p.species).is_some() {
                        p.ctrl_pts_set = false;
                    }
                }
            }
        }
        for id in smooth {
            apply_smooth_handles(&mut self.model, &id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Participant;
    use crate::view::{Button, Modifiers, ViewEvent};

    /// Three species in a ragged row: S1 (0,0) 80 wide, S2 (300,40) 120
    /// wide, S3 (100,-30) 80 wide; R4: S1 → S2, junction (150,20).
    fn view() -> DiagramView {
        let mut m = Model::new();
        let a = m.add_species(pt(0.0, 0.0), 80.0, 36.0);
        let b = m.add_species(pt(300.0, 40.0), 120.0, 36.0);
        m.add_species(pt(100.0, -30.0), 80.0, 36.0);
        let r = m.add_reaction(pt(150.0, 20.0));
        let r = m.find_reaction_mut(&r).unwrap();
        r.reactants.push(Participant::new(a, 1.0));
        r.products.push(Participant::new(b, 1.0));
        let mut v = DiagramView::new(m);
        select_species(&mut v);
        v
    }

    /// Selects every species and no reaction, so junctions aren't aligned.
    fn select_species(v: &mut DiagramView) {
        let ids: Vec<Id> = v.model().species().iter().map(|s| s.id.clone()).collect();
        v.handle(&ViewEvent::Cancel);
        for id in ids {
            v.select_item(&crate::view::Target::Species(id), true);
        }
    }

    fn at(v: &DiagramView, id: &str) -> Pt {
        v.model().find_species(id).unwrap().center
    }

    #[test]
    fn align_left_and_right() {
        let mut v = view();
        v.arrange(Arrange::Left);
        // Left edges at the box's left edge, −40.
        assert_eq!(at(&v, "S1"), pt(0.0, 0.0));
        assert_eq!(at(&v, "S2"), pt(20.0, 40.0));
        assert_eq!(at(&v, "S3"), pt(0.0, -30.0));
        assert_eq!(v.undo_description(), Some("Align Left"));
        v.undo();
        select_species(&mut v);
        v.arrange(Arrange::Right);
        // Right edges at 360.
        assert_eq!(at(&v, "S1"), pt(320.0, 0.0));
        assert_eq!(at(&v, "S2"), pt(300.0, 40.0));
    }

    #[test]
    fn align_center_middle_top_bottom() {
        let mut v = view();
        v.arrange(Arrange::Center);
        // Box x: −40 .. 360, centre 160.
        assert!(v.model().species().iter().all(|s| s.center.x == 160.0));
        v.arrange(Arrange::Top);
        // Box y: −48 .. 58; top edges at −48.
        assert!(v.model().species().iter().all(|s| s.center.y == -30.0));
        v.arrange(Arrange::Bottom);
        assert!(
            v.model().species().iter().all(|s| s.center.y == -30.0),
            "already aligned"
        );
        v.undo();
        v.undo();
        select_species(&mut v);
        v.arrange(Arrange::Middle);
        assert!(v.model().species().iter().all(|s| s.center.y == 5.0));
    }

    #[test]
    fn distribute_spaces_centres_evenly_between_the_ends() {
        let mut v = view();
        v.arrange(Arrange::DistributeHorizontally);
        // Sorted by x: S1 (0), S3 (100), S2 (300): S3 goes to 150.
        assert_eq!(at(&v, "S3"), pt(150.0, -30.0));
        assert_eq!(at(&v, "S1"), pt(0.0, 0.0));
        assert_eq!(at(&v, "S2"), pt(300.0, 40.0));
        v.arrange(Arrange::DistributeVertically);
        // By y: S3 (−30), S1 (0), S2 (40): S1 goes to 5.
        assert_eq!(at(&v, "S1"), pt(0.0, 5.0));
    }

    #[test]
    fn needs_enough_species() {
        let mut v = view();
        v.handle(&ViewEvent::Cancel);
        v.handle(&ViewEvent::PointerDown {
            pos: pt(0.0, 0.0),
            button: Button::Primary,
            modifiers: Modifiers::NONE,
        });
        v.handle(&ViewEvent::PointerUp {
            pos: pt(0.0, 0.0),
            button: Button::Primary,
        });
        assert!(!v.can_arrange(Arrange::Left));
        assert_eq!(v.arrange(Arrange::Left), ViewResponse::default());
    }

    #[test]
    fn locked_species_count_but_dont_move() {
        let mut v = view();
        let mut m = v.model().clone();
        m.find_species_mut("S2").unwrap().locked = true;
        v.set_model(m);
        select_species(&mut v);
        v.arrange(Arrange::Right);
        // S2 sets the right edge (360) and stays; the others line up on it.
        assert_eq!(at(&v, "S2"), pt(300.0, 40.0));
        assert_eq!(at(&v, "S1"), pt(320.0, 0.0));
    }

    #[test]
    fn junction_follows_when_all_its_species_move() {
        let mut v = view();
        v.arrange(Arrange::Top);
        // S1 moved by (0,−30), S2 by (0,−70): the junction by their mean.
        assert_eq!(
            v.model().find_reaction("R4").unwrap().junction,
            pt(150.0, -30.0)
        );
    }

    #[test]
    fn aligning_keeps_a_linear_junction_in_place_along_its_line() {
        let mut v = view();
        let mut m = v.model().clone();
        m.find_reaction_mut("R4").unwrap().mode = ReactionMode::Linear;
        v.set_model(m);
        select_species(&mut v);
        let before = {
            let m = v.model();
            linear_junction_t(m, m.find_reaction("R4").unwrap()).unwrap()
        };
        v.arrange(Arrange::Left);
        let m = v.model();
        let after = linear_junction_t(m, m.find_reaction("R4").unwrap()).unwrap();
        assert!((before - after).abs() < 1e-9);
    }

    #[test]
    fn partial_moves_reset_bezier_handles() {
        let mut v = view();
        let mut m = v.model().clone();
        let r = m.find_reaction_mut("R4").unwrap();
        r.mode = ReactionMode::Bezier;
        for p in r.reactants.iter_mut().chain(&mut r.products) {
            p.ctrl_pts_set = true;
        }
        v.set_model(m);
        select_species(&mut v);
        // Align Left moves S2 only (S1 is already at the left edge).
        v.arrange(Arrange::Left);
        let r = v.model().find_reaction("R4").unwrap();
        assert_eq!(r.junction, pt(150.0, 20.0), "junction stays");
        assert!(r.reactants[0].ctrl_pts_set, "S1 didn't move");
        assert!(!r.products[0].ctrl_pts_set, "S2 moved: automatic again");
    }

    /// The test view plus R5: S1 → S3 with its junction at (50,-80).
    fn two_reactions() -> DiagramView {
        let mut m = view().model().clone();
        let r = m.add_reaction(pt(50.0, -80.0));
        let r = m.find_reaction_mut(&r).unwrap();
        r.reactants.push(Participant::new("S1", 1.0));
        r.products.push(Participant::new("S3", 1.0));
        DiagramView::new(m)
    }

    fn junction(v: &DiagramView, id: &str) -> Pt {
        v.model().find_reaction(id).unwrap().junction
    }

    fn select(v: &mut DiagramView, items: &[crate::view::Target]) {
        v.handle(&ViewEvent::Cancel);
        for t in items {
            v.select_item(t, true);
        }
    }

    #[test]
    fn junctions_align_too() {
        use crate::view::Target;
        let mut v = two_reactions();
        // Two junctions alone are enough to align.
        select(
            &mut v,
            &[Target::Leg("R4".into()), Target::Leg("R5".into())],
        );
        assert!(v.can_arrange(Arrange::Middle));
        assert!(!v.can_arrange(Arrange::DistributeHorizontally));
        v.arrange(Arrange::Middle);
        assert_eq!(junction(&v, "R4"), pt(150.0, -30.0));
        assert_eq!(junction(&v, "R5"), pt(50.0, -30.0));
        assert_eq!(at(&v, "S1"), pt(0.0, 0.0), "species weren't selected");
        assert_eq!(v.undo_description(), Some("Align Middle"));

        // With a species: a junction is a point, a species its box.
        let mut v = two_reactions();
        select(
            &mut v,
            &[Target::Species("S2".into()), Target::Leg("R5".into())],
        );
        v.arrange(Arrange::Left);
        // The box's left edge is the junction's x, 50; S2 is 120 wide.
        assert_eq!(junction(&v, "R5"), pt(50.0, -80.0));
        assert_eq!(at(&v, "S2"), pt(110.0, 40.0));

        // Distribute: S1 (0), R5's junction (50), S2 (300) → the junction to 150.
        let mut v = two_reactions();
        select(
            &mut v,
            &[
                Target::Species("S1".into()),
                Target::Leg("R5".into()),
                Target::Species("S2".into()),
            ],
        );
        v.arrange(Arrange::DistributeHorizontally);
        assert_eq!(junction(&v, "R5"), pt(150.0, -80.0));
    }

    #[test]
    fn a_linear_junction_stays_on_its_line() {
        use crate::view::Target;
        let mut v = two_reactions();
        let mut m = v.model().clone();
        m.find_reaction_mut("R4").unwrap().mode = ReactionMode::Linear;
        v.set_model(m);
        select(
            &mut v,
            &[Target::Leg("R4".into()), Target::Leg("R5".into())],
        );
        // R4's junction doesn't count, so one junction isn't enough.
        assert!(!v.can_arrange(Arrange::Middle));
    }

    #[test]
    fn nudging_moves_the_selection_like_a_drag() {
        use crate::view::Target;
        let mut v = two_reactions();
        // S3 and R4's junction (R4's species, S1 and S2, aren't selected).
        select(
            &mut v,
            &[Target::Species("S3".into()), Target::Leg("R4".into())],
        );
        v.nudge(pt(1.0, 0.0));
        assert_eq!(at(&v, "S3"), pt(101.0, -30.0));
        assert_eq!(junction(&v, "R4"), pt(151.0, 20.0));
        assert_eq!(at(&v, "S1"), pt(0.0, 0.0));
        // R5's junction stays: only one of its species moved.
        assert_eq!(junction(&v, "R5"), pt(50.0, -80.0));

        // A run of nudges is one undo step…
        v.nudge(pt(0.0, 10.0));
        v.nudge(pt(0.0, 10.0));
        assert_eq!(at(&v, "S3"), pt(101.0, -10.0));
        assert_eq!(v.undo_description(), Some("Nudge"));
        v.undo();
        assert_eq!(at(&v, "S3"), pt(100.0, -30.0));
        assert_eq!(junction(&v, "R4"), pt(150.0, 20.0));
        assert!(!v.can_undo());

        // …until the selection changes.
        select(&mut v, &[Target::Species("S3".into())]);
        v.nudge(pt(1.0, 0.0));
        select(&mut v, &[Target::Species("S1".into())]);
        v.nudge(pt(1.0, 0.0));
        v.undo();
        assert_eq!(at(&v, "S1"), pt(0.0, 0.0));
        assert_eq!(
            at(&v, "S3"),
            pt(101.0, -30.0),
            "the first run is its own step"
        );
    }

    #[test]
    fn nudging_keeps_stored_handles_and_locked_species() {
        use crate::view::Target;
        let mut v = two_reactions();
        let mut m = v.model().clone();
        let r = m.find_reaction_mut("R4").unwrap();
        r.mode = ReactionMode::Bezier;
        r.reactants[0].ctrl1 = pt(30.0, -20.0);
        r.reactants[0].ctrl2 = pt(120.0, 20.0);
        r.reactants[0].ctrl_pts_set = true;
        m.find_species_mut("S3").unwrap().locked = true;
        v.set_model(m);
        select(
            &mut v,
            &[Target::Species("S1".into()), Target::Species("S3".into())],
        );
        v.nudge(pt(10.0, 0.0));
        let p = &v.model().find_reaction("R4").unwrap().reactants[0];
        // The species-side handle goes with S1; the other stays, still stored.
        assert!(p.ctrl_pts_set);
        assert_eq!((p.ctrl1, p.ctrl2), (pt(40.0, -20.0), pt(120.0, 20.0)));
        assert_eq!(at(&v, "S3"), pt(100.0, -30.0), "locked");

        // Nothing selected: nothing happens.
        v.handle(&ViewEvent::Cancel);
        let before = v.model().clone();
        assert!(!v.nudge(pt(1.0, 0.0)).model_changed);
        assert_eq!(v.model(), &before);
    }

    #[test]
    fn auto_layout_animates_and_is_one_undo_step() {
        let mut v = view();
        let before = v.model().clone();
        v.start_layout(LayoutParams::default(), 1);
        assert!(v.is_laying_out());
        // Events are ignored while it runs, except Esc.
        v.handle(&ViewEvent::PointerDown {
            pos: pt(0.0, 0.0),
            button: Button::Primary,
            modifiers: Modifiers::NONE,
        });
        assert!(!v.is_busy());
        let mut steps = 0;
        while v.is_laying_out() {
            v.layout_step(37);
            steps += 1;
        }
        assert_eq!(steps, 800_usize.div_ceil(37));
        assert_eq!(v.undo_description(), Some("Auto-layout"));
        let after = v.model().clone();
        // The same as running it in one go.
        let mut direct = before.clone();
        let mut prepared = before.clone();
        for r in prepared.reactions_mut() {
            for p in r.reactants.iter_mut().chain(&mut r.products) {
                p.ctrl_pts_set = false;
            }
        }
        LayoutRun::new(&prepared, LayoutParams::default(), 1).finish(&mut direct);
        assert_eq!(
            after.species().iter().map(|s| s.center).collect::<Vec<_>>(),
            direct
                .species()
                .iter()
                .map(|s| s.center)
                .collect::<Vec<_>>()
        );
        v.undo();
        let mut b = before;
        b.clear_selection();
        assert_eq!(v.model(), &b);
    }

    #[test]
    fn curves_keep_their_shape_while_a_layout_runs() {
        let mut v = view();
        let mut m = v.model().clone();
        let r = m.find_reaction_mut("R4").unwrap();
        r.mode = ReactionMode::Bezier;
        r.reactants[0].ctrl_pts_set = true;
        r.reactants[0].ctrl1 = pt(10.0, 30.0);
        r.reactants[0].ctrl2 = pt(120.0, 50.0);
        v.set_model(m);
        v.start_layout(LayoutParams::default(), 1);
        v.layout_step(50);
        let now = v.model();
        let r = now.find_reaction("R4").unwrap();
        let p = &r.reactants[0];
        assert!(p.ctrl_pts_set, "still a stored curve mid-layout");
        let a = now.find_species("S1").unwrap().center;
        // Outer handle keeps its offset from S1, inner from the junction.
        assert!(p.ctrl1.distance(a + pt(10.0, 30.0)) < 1e-9);
        assert!(
            p.ctrl2
                .distance(r.junction + pt(120.0, 50.0) - pt(150.0, 20.0))
                < 1e-9
        );
    }

    #[test]
    fn smooth_junctions_are_recomputed_while_a_layout_runs() {
        let mut v = view();
        let mut m = v.model().clone();
        let r = m.find_reaction_mut("R4").unwrap();
        r.mode = ReactionMode::Bezier;
        r.junction_smooth = true;
        crate::bezier::apply_smooth_handles(&mut m, "R4");
        v.set_model(m);
        v.start_layout(LayoutParams::default(), 1);
        v.layout_step(50);
        let mut expected = v.model().clone();
        crate::bezier::apply_smooth_handles(&mut expected, "R4");
        assert_eq!(v.model(), &expected);
    }

    #[test]
    fn randomize_starts_from_automatic_handles() {
        let mut v = view();
        let mut m = v.model().clone();
        let r = m.find_reaction_mut("R4").unwrap();
        r.mode = ReactionMode::Bezier;
        r.reactants[0].ctrl_pts_set = true;
        v.set_model(m);
        let params = LayoutParams {
            randomize: true,
            ..LayoutParams::default()
        };
        v.start_layout(params, 1);
        assert!(!v.model().find_reaction("R4").unwrap().reactants[0].ctrl_pts_set);
    }

    #[test]
    fn cancelling_a_layout_leaves_the_model_unchanged() {
        let mut v = view();
        let before = v.model().clone();
        v.start_layout(LayoutParams::default(), 1);
        v.layout_step(100);
        assert_ne!(v.model(), &before);
        v.handle(&ViewEvent::Cancel);
        assert!(!v.is_laying_out());
        assert_eq!(v.model().species(), before.species());
        assert!(!v.can_undo());
    }

    #[test]
    fn random_network_replaces_the_model_and_undoes() {
        let mut v = view();
        v.random_network(3);
        assert_eq!(v.model().species().len(), 10);
        assert_eq!(v.model().reactions().len(), 12);
        assert_eq!(v.undo_description(), Some("Random Network"));
        v.undo();
        assert_eq!(v.model().species().len(), 3);
    }
}
