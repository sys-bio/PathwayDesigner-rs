//! Layout tools in the view: auto-layout, align and distribute, and the
//! random network (S §7.5, §7.7, §10, SPEC-Rust §7.2).

use super::{DiagramView, Gesture};
use crate::bezier::{Ctrl, apply_smooth_handles};
use crate::generate;
use crate::geometry::{Pt, pt};
use crate::layout::{LayoutParams, LayoutRun, Progress};
use crate::model::{Id, Model, ReactionMode, Role};
use crate::render::draw::{linear_junction_t, linear_line};
use crate::view::ViewResponse;

/// A layout in progress, and the model before it started.
#[derive(Clone, Debug)]
pub(super) struct LayoutState {
    run: LayoutRun,
    before: Box<Model>,
}

/// The eight align and distribute commands (S §7.5).
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
}

impl Arrange {
    pub const ALL: [Arrange; 8] = [
        Arrange::Left,
        Arrange::Center,
        Arrange::Right,
        Arrange::Top,
        Arrange::Middle,
        Arrange::Bottom,
        Arrange::DistributeHorizontally,
        Arrange::DistributeVertically,
    ];

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
        }
    }

    /// How many selected species the command needs.
    pub fn minimum(self) -> usize {
        match self {
            Arrange::DistributeHorizontally | Arrange::DistributeVertically => 3,
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
        self.layout = Some(LayoutState { run, before });
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

    /// True if `cmd` can do anything with the current selection.
    pub fn can_arrange(&self, cmd: Arrange) -> bool {
        self.model.selected_species().count() >= cmd.minimum()
    }

    /// Aligns or distributes the selected species. Locked species don't move,
    /// but still count towards the bounding box, so others can be aligned to
    /// a locked one; when distributing they keep their places. Junctions and
    /// handles follow (see [`DiagramView::moved_species`]). One undo step.
    pub fn arrange(&mut self, cmd: Arrange) -> ViewResponse {
        if self.layout.is_some() || !matches!(self.gesture, Gesture::Idle) || !self.can_arrange(cmd)
        {
            return ViewResponse::default();
        }
        let sel: Vec<(Id, Pt, f64, f64, bool)> = self
            .model
            .selected_species()
            .map(|s| {
                (
                    s.id.clone(),
                    s.center,
                    s.width / 2.0,
                    s.height / 2.0,
                    s.locked,
                )
            })
            .collect();
        let left = sel
            .iter()
            .map(|s| s.1.x - s.2)
            .fold(f64::INFINITY, f64::min);
        let right = sel
            .iter()
            .map(|s| s.1.x + s.2)
            .fold(f64::NEG_INFINITY, f64::max);
        let top = sel
            .iter()
            .map(|s| s.1.y - s.3)
            .fold(f64::INFINITY, f64::min);
        let bottom = sel
            .iter()
            .map(|s| s.1.y + s.3)
            .fold(f64::NEG_INFINITY, f64::max);

        let mut targets: Vec<(Id, Pt)> = Vec::new();
        match cmd {
            Arrange::DistributeHorizontally | Arrange::DistributeVertically => {
                let horizontal = cmd == Arrange::DistributeHorizontally;
                let key = |p: Pt| if horizontal { p.x } else { p.y };
                let mut order = sel.clone();
                order.sort_by(|a, b| key(a.1).total_cmp(&key(b.1)));
                let (first, last) = (key(order[0].1), key(order[order.len() - 1].1));
                let n = order.len() - 1;
                for (i, s) in order.iter().enumerate().skip(1).take(n - 1) {
                    let v = first + (last - first) * i as f64 / n as f64;
                    let at = if horizontal {
                        pt(v, s.1.y)
                    } else {
                        pt(s.1.x, v)
                    };
                    targets.push((s.0.clone(), at));
                }
            }
            _ => {
                for (id, c, hw, hh, _) in &sel {
                    let at = match cmd {
                        Arrange::Left => pt(left + hw, c.y),
                        Arrange::Center => pt((left + right) / 2.0, c.y),
                        Arrange::Right => pt(right - hw, c.y),
                        Arrange::Top => pt(c.x, top + hh),
                        Arrange::Middle => pt(c.x, (top + bottom) / 2.0),
                        _ => pt(c.x, bottom - hh),
                    };
                    targets.push((id.clone(), at));
                }
            }
        }

        let before = self.model.clone();
        // Where each Linear junction is along its line, to keep it there.
        let linear: Vec<(Id, f64)> = self
            .model
            .reactions()
            .iter()
            .filter_map(|r| linear_junction_t(&self.model, r).map(|t| (r.id.clone(), t)))
            .collect();
        let mut moved = Vec::new();
        for (id, at) in targets {
            let Some(s) = self.model.find_species_mut(&id) else {
                continue;
            };
            if s.locked || s.center == at {
                continue;
            }
            moved.push((id, at - s.center));
            s.center = at;
        }
        if moved.is_empty() {
            return ViewResponse::default();
        }
        self.moved_species(&moved);
        for (id, t) in linear {
            let ends_moved = self.model.find_reaction(&id).is_some_and(|r| {
                r.reactants
                    .iter()
                    .chain(&r.products)
                    .any(|p| moved.iter().any(|(m, _)| *m == p.species))
            });
            let line = self
                .model
                .find_reaction(&id)
                .and_then(|r| linear_line(&self.model, r));
            if let (true, Some((a, b))) = (ends_moved, line) {
                self.model
                    .find_reaction_mut(&id)
                    .expect("found above")
                    .junction = a + (b - a) * t;
            }
        }
        self.record(cmd.label(), before)
    }

    /// Passes species movements on to the reactions (S §7.5):
    /// - if every reactant and product moved, the junction moves by the mean
    ///   of their movements, stored handles move with the end they belong to,
    ///   and a smooth junction gets its handles recomputed;
    /// - if only some moved, the junction stays, and the moved participants of
    ///   a Bézier reaction go back to automatic handles.
    fn moved_species(&mut self, moved: &[(Id, Pt)]) {
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
        v.select_all();
        v
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
        v.select_all();
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
        v.select_all();
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
        v.select_all();
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
        v.select_all();
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
        v.select_all();
        // Align Left moves S2 only (S1 is already at the left edge).
        v.arrange(Arrange::Left);
        let r = v.model().find_reaction("R4").unwrap();
        assert_eq!(r.junction, pt(150.0, 20.0), "junction stays");
        assert!(r.reactants[0].ctrl_pts_set, "S1 didn't move");
        assert!(!r.products[0].ctrl_pts_set, "S2 moved: automatic again");
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
