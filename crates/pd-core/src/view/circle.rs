//! Arrange in Circle (S §7.5.1): puts the selected species evenly round a
//! circle and draws the reactions between neighbours as arcs of it.
//!
//! A cubic Bézier follows a circular arc of radius R and sweep θ almost
//! exactly when both handles lie on the tangents at its ends and are
//! (4/3)·tan(θ/4)·R long. Each reaction between neighbours is two such legs,
//! species to junction and junction to species, meeting on the circle with
//! the same tangent, so the reaction reads as one piece of the circle.

use std::f64::consts::{PI, TAU};

use super::DiagramView;
use crate::bezier::apply_smooth_handles;
use crate::geometry::{Pt, normalize, pt};
use crate::model::{Id, Model, ReactionMode};
use crate::render::constants::HANDLE_FRACTION;
use crate::view::ViewResponse;

/// Space between neighbouring species on the circle, on top of the larger
/// box diagonal, so there is room for the junction and the arrowhead.
const CIRCLE_GAP: f64 = 140.0;
/// How far outside the circle an outside species' box starts.
const OUTSIDE_DISTANCE: f64 = 70.0;
/// How far along the tangent, before or after the junction, the first
/// outside species' box starts.
const OUTSIDE_LEAD: f64 = 30.0;
/// Space between outside species of the same role on one reaction.
const OUTSIDE_GAP: f64 = 20.0;

const UNDO: &str = "Arrange in Circle";

/// The handle length that makes a cubic Bézier follow a circular arc of
/// `radius` and `sweep` radians: (4/3)·tan(θ/4)·R.
pub fn arc_handle_length(radius: f64, sweep: f64) -> f64 {
    4.0 / 3.0 * (sweep.abs() / 4.0).tan() * radius
}

/// The unit vector at `angle`.
fn unit(angle: f64) -> Pt {
    pt(angle.cos(), angle.sin())
}

/// A reaction drawn along an arc: from its reactant on the circle at
/// `start`, through the junction at the middle, to its product at
/// `start + sweep`.
#[derive(Clone, Debug)]
struct Arc {
    reaction: Id,
    /// The species on the circle.
    reactant: Id,
    product: Id,
    centre: Pt,
    radius: f64,
    start: f64,
    sweep: f64,
}

impl Arc {
    fn at(&self, angle: f64) -> Pt {
        self.centre + unit(angle) * self.radius
    }

    /// The unit tangent at `angle`, in the direction of travel.
    fn tangent(&self, angle: f64) -> Pt {
        let u = unit(angle);
        pt(-u.y, u.x) * self.sweep.signum()
    }

    fn mid(&self) -> f64 {
        self.start + self.sweep / 2.0
    }

    fn junction(&self) -> Pt {
        self.at(self.mid())
    }

    /// The handle length of each half: one leg spans half the sweep.
    fn handle(&self) -> f64 {
        arc_handle_length(self.radius, self.sweep / 2.0)
    }
}

impl DiagramView {
    /// The species Arrange in Circle puts on the circle: the selected ones
    /// that aren't locked.
    pub(super) fn circle_species(&self) -> Vec<Id> {
        self.model
            .selected_species()
            .filter(|s| !s.locked)
            .map(|s| s.id.clone())
            .collect()
    }

    /// Arrange in Circle (S §7.5.1). One undo step.
    pub(super) fn arrange_circle(&mut self) -> ViewResponse {
        let ids = self.circle_species();
        let n = ids.len();
        if n < 3 {
            return ViewResponse::default();
        }
        let before = self.model.clone();
        let index = |id: &str| ids.iter().position(|s| s == id);

        // The circle: centred on the species, big enough that neighbours
        // don't touch, and no smaller than they are spread now.
        let nodes: Vec<(Pt, f64)> = ids
            .iter()
            .map(|id| {
                let s = self
                    .model
                    .find_species(id)
                    .expect("selected species exists");
                (s.center, s.width.hypot(s.height))
            })
            .collect();
        let centre = nodes.iter().fold(Pt::ZERO, |a, (c, _)| a + *c) / n as f64;
        let spacing = nodes.iter().map(|(_, d)| *d).fold(0.0, f64::max) + CIRCLE_GAP;
        let needed = spacing / (2.0 * (PI / n as f64).sin());
        let spread = nodes.iter().map(|(c, _)| c.distance(centre)).sum::<f64>() / n as f64;
        let radius = needed.max(spread);

        // Reactions with exactly one reactant and one product on the circle.
        let links: Vec<(Id, usize, usize)> = self
            .model
            .reactions()
            .iter()
            .filter_map(|r| {
                let on = |list: &[crate::model::Participant]| {
                    let mut it = list.iter().filter_map(|p| index(&p.species));
                    match (it.next(), it.next()) {
                        (Some(i), None) => Some(i),
                        _ => None,
                    }
                };
                let (a, b) = (on(&r.reactants)?, on(&r.products)?);
                (a != b).then(|| (r.id.clone(), a, b))
            })
            .collect();

        let order = circle_order(
            &nodes.iter().map(|(c, _)| *c).collect::<Vec<_>>(),
            centre,
            &links.iter().map(|(_, a, b)| (*a, *b)).collect::<Vec<_>>(),
        );
        let angles_now: Vec<f64> = order
            .iter()
            .map(|&i| {
                let d = nodes[i].0 - centre;
                d.y.atan2(d.x)
            })
            .collect();
        let (first, sense) = best_rotation(&angles_now);
        let step = sense * TAU / n as f64;
        let mut place = vec![(0, 0.0); n];
        for (k, &i) in order.iter().enumerate() {
            place[i] = (k, first + step * k as f64);
        }

        // Reactions between neighbours become arcs. A second one between the
        // same two species takes the mirror-image arc, inside the circle;
        // any more are left to automatic handles.
        let mut arcs: Vec<Arc> = Vec::new();
        let mut pairs: Vec<(usize, usize)> = Vec::new();
        for (id, a, b) in &links {
            let (ka, kb) = (place[*a].0, place[*b].0);
            let forward = (kb + n - ka) % n == 1;
            if !forward && (ka + n - kb) % n != 1 {
                continue;
            }
            let pair = (*a.min(b), *a.max(b));
            let repeats = pairs.iter().filter(|p| **p == pair).count();
            pairs.push(pair);
            let sweep = if forward { step } else { -step };
            let arc = Arc {
                reaction: id.clone(),
                reactant: ids[*a].clone(),
                product: ids[*b].clone(),
                centre,
                radius,
                start: place[*a].1,
                sweep,
            };
            match repeats {
                0 => arcs.push(arc),
                1 => arcs.push(mirrored(arc)),
                _ => {}
            }
        }

        // Outside species: those on one arc reaction only, placed just
        // outside its junction, reactants before it and products after.
        let mut targets: Vec<(Id, Pt)> = ids
            .iter()
            .enumerate()
            .map(|(i, id)| (id.clone(), centre + unit(place[i].1) * radius))
            .collect();
        let extras_of = |m: &Model, arc: &Arc| -> (Vec<Id>, Vec<Id>) {
            let r = m.find_reaction(&arc.reaction).expect("arc reaction exists");
            let others = |list: &[crate::model::Participant], main: &Id| {
                list.iter()
                    .map(|p| p.species.clone())
                    .filter(|s| s != main)
                    .collect::<Vec<_>>()
            };
            (
                others(&r.reactants, &arc.reactant),
                others(&r.products, &arc.product),
            )
        };
        let mut extra_count: Vec<(Id, usize)> = Vec::new();
        for arc in &arcs {
            let (rs, ps) = extras_of(&self.model, arc);
            for s in rs.into_iter().chain(ps) {
                match extra_count.iter_mut().find(|(e, _)| *e == s) {
                    Some((_, c)) => *c += 1,
                    None => extra_count.push((s, 1)),
                }
            }
        }
        let movable = |m: &Model, s: &Id| {
            extra_count.iter().any(|(e, c)| e == s && *c == 1)
                && index(s).is_none()
                && m.find_species(s).is_some_and(|n| !n.locked)
        };
        for arc in &arcs {
            let (j, mid) = (arc.junction(), arc.mid());
            let (out, along) = (normalize(j - arc.centre), arc.tangent(mid));
            let (rs, ps) = extras_of(&self.model, arc);
            for (list, dir) in [(rs, -1.0), (ps, 1.0)] {
                let mut d = OUTSIDE_LEAD;
                for s in list.iter().filter(|s| movable(&self.model, s)) {
                    let half = {
                        let n = self.model.find_species(s).expect("checked");
                        n.width.hypot(n.height) / 2.0
                    };
                    d += half;
                    let at = j + out * (OUTSIDE_DISTANCE + half) + along * (dir * d);
                    d += half + OUTSIDE_GAP;
                    targets.push((s.clone(), at));
                }
            }
        }

        // Move everything, pass the movements on to other reactions as Align
        // does, then shape the arcs.
        let linear = self.linear_positions();
        let mut moved = Vec::new();
        for (id, at) in &targets {
            let s = self.model.find_species_mut(id).expect("species exists");
            moved.push((id.clone(), *at - s.center));
            s.center = *at;
        }
        self.moved_species(&moved);
        self.keep_linear(&linear, &moved);
        self.tidy_moved_reactions(&moved, &arcs);
        for arc in &arcs {
            shape_arc(&mut self.model, arc);
        }

        if self.model == before {
            return ViewResponse::default();
        }
        self.record(UNDO, before)
    }

    /// A reaction whose species all moved, and which isn't an arc, gets its
    /// junction at their centre and automatic (or smooth) handles: the mean
    /// shift Align gives it would put it anywhere.
    fn tidy_moved_reactions(&mut self, moved: &[(Id, Pt)], arcs: &[Arc]) {
        let mut smooth = Vec::new();
        let centres: Vec<(Id, Pt)> = self
            .model
            .reactions()
            .iter()
            .filter(|r| r.mode != ReactionMode::Linear)
            .filter(|r| !arcs.iter().any(|a| a.reaction == r.id))
            .filter_map(|r| {
                let ends: Vec<Pt> = r
                    .reactants
                    .iter()
                    .chain(&r.products)
                    .map(|p| {
                        moved
                            .iter()
                            .any(|(m, _)| *m == p.species)
                            .then(|| self.model.find_species(&p.species).map(|s| s.center))
                            .flatten()
                    })
                    .collect::<Option<_>>()?;
                (!ends.is_empty()).then(|| {
                    let c = ends.iter().fold(Pt::ZERO, |a, &p| a + p) / ends.len() as f64;
                    (r.id.clone(), c)
                })
            })
            .collect();
        for (id, c) in centres {
            let r = self.model.find_reaction_mut(&id).expect("found above");
            r.junction = c;
            for p in r.reactants.iter_mut().chain(&mut r.products) {
                p.ctrl_pts_set = false;
            }
            if r.mode == ReactionMode::Bezier && r.junction_smooth {
                smooth.push(id);
            }
        }
        for id in smooth {
            apply_smooth_handles(&mut self.model, &id);
        }
    }
}

/// The same arc reflected in the line between its ends: a circle of the
/// same size, bulging the other way, travelled the other way round.
fn mirrored(arc: Arc) -> Arc {
    let (a, b) = (arc.at(arc.start), arc.at(arc.start + arc.sweep));
    let d = normalize(b - a);
    let v = arc.centre - a;
    let centre = a + d * (2.0 * v.dot(d)) - v;
    let from = a - centre;
    Arc {
        centre,
        start: from.y.atan2(from.x),
        sweep: -arc.sweep,
        ..arc
    }
}

/// Makes the reaction a Bézier along `arc`: its junction on the arc, the
/// two species on the circle joined by two arc-shaped legs, and any other
/// species joining at the junction along the arc's tangent. An other
/// species' own handle points straight towards the circle, or away from it
/// for a product, so its leg leaves and arrives square to the circle.
fn shape_arc(model: &mut Model, arc: &Arc) {
    let (j, mid, k) = (arc.junction(), arc.mid(), arc.handle());
    let t_mid = arc.tangent(mid);
    let out = normalize(j - arc.centre);
    let towards = |s: Pt| {
        let side = (j - s).dot(out);
        let dir = if side.abs() < 1.0 {
            normalize(j - s)
        } else {
            out * side.signum()
        };
        s + dir * (HANDLE_FRACTION * s.distance(j))
    };
    let (a, b) = (arc.at(arc.start), arc.at(arc.start + arc.sweep));
    let (t_a, t_b) = (arc.tangent(arc.start), arc.tangent(arc.start + arc.sweep));
    let centres: Vec<Option<Pt>> = {
        let r = model
            .find_reaction(&arc.reaction)
            .expect("arc reaction exists");
        r.reactants
            .iter()
            .chain(&r.products)
            .map(|p| model.find_species(&p.species).map(|s| s.center))
            .collect()
    };
    let r = model
        .find_reaction_mut(&arc.reaction)
        .expect("arc reaction exists");
    r.junction = j;
    r.mode = ReactionMode::Bezier;
    r.junction_smooth = false;
    let nr = r.reactants.len();
    for (i, p) in r.reactants.iter_mut().enumerate() {
        let Some(s) = centres[i] else { continue };
        p.ctrl_pts_set = true;
        if p.species == arc.reactant {
            (p.ctrl1, p.ctrl2) = (a + t_a * k, j - t_mid * k);
        } else {
            let l = s.distance(j);
            (p.ctrl1, p.ctrl2) = (towards(s), j - t_mid * (HANDLE_FRACTION * l));
        }
    }
    for (i, p) in r.products.iter_mut().enumerate() {
        let Some(s) = centres[nr + i] else { continue };
        p.ctrl_pts_set = true;
        if p.species == arc.product {
            (p.ctrl1, p.ctrl2) = (j + t_mid * k, b - t_b * k);
        } else {
            let l = s.distance(j);
            (p.ctrl1, p.ctrl2) = (j + t_mid * (HANDLE_FRACTION * l), towards(s));
        }
    }
}

/// The order of the species round the circle, as indices into `centres`.
/// If the links join every species into one chain or one loop, the order
/// follows them; otherwise it is the species' order round `centre` now.
fn circle_order(centres: &[Pt], centre: Pt, links: &[(usize, usize)]) -> Vec<usize> {
    let n = centres.len();
    let mut next: Vec<Vec<usize>> = vec![Vec::new(); n];
    for &(a, b) in links {
        if !next[a].contains(&b) {
            next[a].push(b);
            next[b].push(a);
        }
    }
    if next.iter().all(|v| !v.is_empty() && v.len() <= 2) {
        let start = next.iter().position(|v| v.len() == 1).unwrap_or(0);
        let mut order = vec![start];
        while let Some(&k) = next[*order.last().expect("not empty")]
            .iter()
            .find(|k| !order.contains(k))
        {
            order.push(k);
        }
        if order.len() == n {
            return order;
        }
    }
    let angle = |i: usize| {
        let d = centres[i] - centre;
        d.y.atan2(d.x)
    };
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&a, &b| angle(a).total_cmp(&angle(b)));
    order
}

/// Where the first species in the order goes, and which way round (+1 or
/// −1) the rest follow, so the species move as little as possible from
/// their angles now.
fn best_rotation(angles: &[f64]) -> (f64, f64) {
    let n = angles.len() as f64;
    let mut best = (0.0, 1.0, -1.0);
    for sense in [1.0, -1.0] {
        let sum = angles.iter().enumerate().fold(Pt::ZERO, |acc, (k, &a)| {
            acc + unit(a - sense * TAU * k as f64 / n)
        });
        let fit = sum.length();
        if fit > best.2 + 1e-9 {
            best = (sum.y.atan2(sum.x), sense, fit);
        }
    }
    (best.0, best.1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bezier::leg_curve;
    use crate::model::{Participant, Role};
    use crate::view::{Arrange, Target, ViewEvent};

    fn select(v: &mut DiagramView, ids: &[&str]) {
        v.handle(&ViewEvent::Cancel);
        for id in ids {
            v.select_item(&Target::Species((*id).into()), true);
        }
    }

    fn react(m: &mut Model, reactants: &[&Id], products: &[&Id]) -> Id {
        let id = m.add_reaction(Pt::ZERO);
        let r = m.find_reaction_mut(&id).unwrap();
        r.reactants = reactants
            .iter()
            .map(|s| Participant::new(*s, 1.0))
            .collect();
        r.products = products.iter().map(|s| Participant::new(*s, 1.0)).collect();
        id
    }

    fn centre_of(v: &DiagramView, ids: &[Id]) -> Pt {
        ids.iter()
            .map(|id| v.model().find_species(id).unwrap().center)
            .fold(Pt::ZERO, |a, p| a + p)
            / ids.len() as f64
    }

    /// The largest distance of the reaction's leg `role`/`index` from the
    /// circle, over the whole curve.
    fn off_circle(v: &DiagramView, rid: &str, role: Role, index: usize, c: Pt, r: f64) -> f64 {
        let reaction = v.model().find_reaction(rid).unwrap();
        let curve = leg_curve(v.model(), reaction, role, index).unwrap();
        (0..=50)
            .map(|i| (curve.eval(i as f64 / 50.0).distance(c) - r).abs())
            .fold(0.0, f64::max)
    }

    #[test]
    fn the_handle_length_is_the_standard_one() {
        let k = arc_handle_length(1.0, PI / 2.0);
        assert!((k - 0.552_284_75).abs() < 1e-6, "{k}");
    }

    #[test]
    fn species_only_go_evenly_round_a_circle() {
        let mut m = Model::new();
        let ids: Vec<Id> = [
            (0.0, 0.0),
            (10.0, 5.0),
            (20.0, -5.0),
            (5.0, 30.0),
            (-8.0, 12.0),
        ]
        .iter()
        .map(|&(x, y)| m.add_species(pt(x, y), 80.0, 36.0))
        .collect();
        let mut v = DiagramView::new(m);
        let names: Vec<&str> = ids.iter().map(|s| s.as_str()).collect();
        select(&mut v, &names);
        assert!(v.can_arrange(Arrange::Circle));
        v.arrange(Arrange::Circle);
        assert_eq!(v.undo_description(), Some(UNDO));
        let c = centre_of(&v, &ids);
        let at = |id: &Id| v.model().find_species(id).unwrap().center;
        let r = at(&ids[0]).distance(c);
        let mut angles: Vec<f64> = ids
            .iter()
            .map(|id| {
                assert!((at(id).distance(c) - r).abs() < 1e-6);
                let d = at(id) - c;
                d.y.atan2(d.x)
            })
            .collect();
        angles.sort_by(f64::total_cmp);
        for w in angles.windows(2) {
            assert!((w[1] - w[0] - TAU / 5.0).abs() < 1e-6);
        }
        // Neighbours are at least a diagonal plus the gap apart.
        let chord = 2.0 * r * (PI / 5.0).sin();
        assert!(chord >= 80f64.hypot(36.0) + CIRCLE_GAP - 1e-6);
    }

    #[test]
    fn a_loop_of_uni_uni_reactions_follows_the_circle() {
        let mut m = Model::new();
        // Placed out of loop order round their centre.
        let a = m.add_species(pt(0.0, 0.0), 80.0, 36.0);
        let b = m.add_species(pt(300.0, 300.0), 80.0, 36.0);
        let c = m.add_species(pt(300.0, 0.0), 80.0, 36.0);
        let d = m.add_species(pt(0.0, 300.0), 80.0, 36.0);
        let rs = [
            react(&mut m, &[&a], &[&b]),
            react(&mut m, &[&b], &[&c]),
            react(&mut m, &[&c], &[&d]),
            react(&mut m, &[&d], &[&a]),
        ];
        let ids = vec![a, b, c, d];
        let mut v = DiagramView::new(m);
        select(&mut v, &ids.iter().map(|s| s.as_str()).collect::<Vec<_>>());
        v.arrange(Arrange::Circle);
        let cen = centre_of(&v, &ids);
        let rad = v
            .model()
            .find_species(&ids[0])
            .unwrap()
            .center
            .distance(cen);
        for rid in &rs {
            let r = v.model().find_reaction(rid).unwrap();
            assert_eq!(r.mode, ReactionMode::Bezier);
            assert!((r.junction.distance(cen) - rad).abs() < 1e-6);
            for role in [Role::Reactant, Role::Product] {
                let err = off_circle(&v, rid, role, 0, cen, rad);
                assert!(err < rad * 1e-3, "{rid} {role:?} off by {err}");
            }
            // The two legs meet with the same direction.
            let (p, q) = (&r.reactants[0], &r.products[0]);
            let into = normalize(r.junction - p.ctrl2);
            let out = normalize(q.ctrl1 - r.junction);
            assert!(into.distance(out) < 1e-9);
        }
    }

    #[test]
    fn outside_species_sit_outside_and_join_along_the_tangent() {
        let mut m = Model::new();
        let a = m.add_species(pt(0.0, 0.0), 80.0, 36.0);
        let b = m.add_species(pt(200.0, 0.0), 80.0, 36.0);
        let c = m.add_species(pt(100.0, 200.0), 80.0, 36.0);
        let x = m.add_species(pt(500.0, 500.0), 80.0, 36.0);
        let y = m.add_species(pt(-500.0, 500.0), 80.0, 36.0);
        let bibi = react(&mut m, &[&a, &x], &[&b, &y]);
        react(&mut m, &[&b], &[&c]);
        react(&mut m, &[&c], &[&a]);
        let ids = vec![a.clone(), b.clone(), c.clone()];
        let mut v = DiagramView::new(m);
        select(&mut v, &["S1", "S2", "S3"]);
        v.arrange(Arrange::Circle);
        let cen = centre_of(&v, &ids);
        let rad = v.model().find_species(&a).unwrap().center.distance(cen);
        let r = v.model().find_reaction(&bibi).unwrap().clone();
        let (j, along) = (r.junction, normalize(r.products[0].ctrl1 - r.junction));
        for s in [&x, &y] {
            let at = v.model().find_species(s).unwrap().center;
            assert!(at.distance(cen) > rad + OUTSIDE_DISTANCE, "{s} not outside");
        }
        // X comes in before the junction and Y leaves after it.
        let xs = v.model().find_species(&x).unwrap().center;
        let ys = v.model().find_species(&y).unwrap().center;
        assert!((xs - j).dot(along) < 0.0 && (ys - j).dot(along) > 0.0);
        // Their junction-side handles lie on the tangent.
        assert!(normalize(j - r.reactants[1].ctrl2).distance(along) < 1e-9);
        assert!(normalize(r.products[1].ctrl1 - j).distance(along) < 1e-9);
        for role in [Role::Reactant, Role::Product] {
            assert!(off_circle(&v, &bibi, role, 0, cen, rad) < rad * 1e-3);
        }
    }

    #[test]
    fn a_shared_outside_species_stays_put() {
        let mut m = Model::new();
        let a = m.add_species(pt(0.0, 0.0), 80.0, 36.0);
        let b = m.add_species(pt(200.0, 0.0), 80.0, 36.0);
        let c = m.add_species(pt(100.0, 200.0), 80.0, 36.0);
        let atp = m.add_species(pt(900.0, 900.0), 80.0, 36.0);
        react(&mut m, &[&a, &atp], &[&b]);
        react(&mut m, &[&b, &atp], &[&c]);
        let mut v = DiagramView::new(m);
        select(&mut v, &["S1", "S2", "S3"]);
        v.arrange(Arrange::Circle);
        assert_eq!(
            v.model().find_species(&atp).unwrap().center,
            pt(900.0, 900.0)
        );
    }

    #[test]
    fn a_second_reaction_between_neighbours_bulges_inwards() {
        let mut m = Model::new();
        let a = m.add_species(pt(0.0, 0.0), 80.0, 36.0);
        let b = m.add_species(pt(200.0, 0.0), 80.0, 36.0);
        let c = m.add_species(pt(100.0, 200.0), 80.0, 36.0);
        let fwd = react(&mut m, &[&a], &[&b]);
        let back = react(&mut m, &[&b], &[&a]);
        let ids = vec![a, b, c];
        let mut v = DiagramView::new(m);
        select(&mut v, &["S1", "S2", "S3"]);
        v.arrange(Arrange::Circle);
        let cen = centre_of(&v, &ids);
        let rad = v
            .model()
            .find_species(&ids[0])
            .unwrap()
            .center
            .distance(cen);
        let jf = v.model().find_reaction(&fwd).unwrap().junction;
        let jb = v.model().find_reaction(&back).unwrap().junction;
        assert!((jf.distance(cen) - rad).abs() < 1e-6);
        assert!(jb.distance(cen) < rad - 10.0);
    }

    #[test]
    fn locked_species_are_left_out() {
        let mut m = Model::new();
        for i in 0..3 {
            m.add_species(pt(i as f64 * 100.0, 0.0), 80.0, 36.0);
        }
        m.find_species_mut("S1").unwrap().locked = true;
        let mut v = DiagramView::new(m);
        select(&mut v, &["S1", "S2", "S3"]);
        assert!(!v.can_arrange(Arrange::Circle), "only two unlocked");
        v.arrange(Arrange::Circle);
        assert_eq!(v.undo_description(), None);
    }
}
