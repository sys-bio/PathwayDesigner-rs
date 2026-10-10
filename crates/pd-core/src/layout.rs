//! Automatic layout (S §10, SPEC-Rust §7.2).
//!
//! A force-directed layout for networks with reaction junctions, run in steps
//! so the app can animate it and stay responsive without threads. `pd-core`
//! never reads a clock: the app decides how many iterations to run per frame.
//! The same model, parameters and seed always give the same result.

use std::collections::HashMap;

use crate::bezier::apply_smooth_handles;
use crate::geometry::{Pt, normalize, perp, pt};
use crate::model::{Id, Model, ReactionMode};
use crate::render::constants::HANDLE_FRACTION;

/// A small, fast, seeded random generator (SplitMix64). Written here rather
/// than taken from a crate so results are the same on every platform and
/// every version.
#[derive(Clone, Debug)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A number in [0, 1).
    pub fn next_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    /// A number in [lo, hi).
    pub fn range(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * self.next_f64()
    }

    /// An integer in [0, n).
    pub fn below(&mut self, n: usize) -> usize {
        (self.next_f64() * n as f64) as usize
    }
}

/// Layout settings, with the values the app uses (S §10.1, §17.1).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayoutParams {
    pub iterations: u32,
    /// Repulsion, spring and gravity constants.
    pub kr: f64,
    pub ka: f64,
    pub kg: f64,
    /// The step limit starts at `max(speed_start, 5 % of the starting
    /// bounding box's diagonal)` and is multiplied by `speed_decay` after
    /// every iteration.
    pub speed_start: f64,
    pub speed_decay: f64,
    /// Scatter the unlocked species at random first.
    pub randomize: bool,
    /// Run the Deckard & Bergmann handle pass afterwards (S §10.2).
    pub deckard: bool,
}

impl Default for LayoutParams {
    fn default() -> Self {
        Self {
            iterations: 800,
            kr: 8000.0,
            ka: 0.02,
            kg: 0.01,
            speed_start: 20.0,
            speed_decay: 0.98,
            randomize: false,
            deckard: false,
        }
    }
}

/// How far a run has got.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Progress {
    pub done: u32,
    pub total: u32,
}

impl Progress {
    pub fn finished(&self) -> bool {
        self.done >= self.total
    }
}

/// Randomize puts species in [100, 500)² (S §10.1).
const RANDOM_MIN: f64 = 100.0;
const RANDOM_MAX: f64 = 500.0;

#[derive(Clone, Debug)]
enum NodeKind {
    Species(Id),
    Junction(Id),
}

#[derive(Clone, Debug)]
struct Node {
    kind: NodeKind,
    pos: Pt,
    mass: f64,
    fixed: bool,
}

/// One end of a spring: a node, plus a fixed offset for an alias that rides
/// on its primary.
#[derive(Clone, Copy, Debug)]
struct End {
    node: usize,
    offset: Pt,
}

/// An alias that follows its primary rigidly.
#[derive(Clone, Debug)]
struct Rider {
    id: Id,
    primary: usize,
    offset: Pt,
}

/// A layout in progress (SPEC-Rust §7.2).
#[derive(Clone, Debug)]
pub struct LayoutRun {
    params: LayoutParams,
    nodes: Vec<Node>,
    /// (junction node, species end): one spring per unique species of a reaction.
    springs: Vec<(usize, End)>,
    riders: Vec<Rider>,
    /// Gravity pulls towards the centroid of the starting positions.
    centre: Pt,
    speed: f64,
    done: u32,
    rng: Rng,
}

impl LayoutRun {
    /// Sets up a run on `model`. With `randomize`, unlocked species are
    /// scattered and junctions put at the centroid of their participants
    /// before the first iteration.
    pub fn new(model: &Model, params: LayoutParams, seed: u64) -> Self {
        let mut rng = Rng::new(seed);
        let mut nodes = Vec::new();
        let mut index: HashMap<&str, End> = HashMap::new();

        // Each species entry in a reaction adds to its primary's mass.
        let mut uses: HashMap<&str, usize> = HashMap::new();
        for r in model.reactions() {
            for p in r.reactants.iter().chain(&r.products) {
                let primary = model
                    .find_species(&p.species)
                    .map_or(p.species.as_str(), |s| s.primary_id());
                *uses.entry(primary).or_default() += 1;
            }
        }

        // Species positions, scattered first if asked.
        let mut start: HashMap<&str, Pt> = HashMap::new();
        for s in model.species() {
            let at = if params.randomize && !s.locked {
                pt(
                    rng.range(RANDOM_MIN, RANDOM_MAX),
                    rng.range(RANDOM_MIN, RANDOM_MAX),
                )
            } else {
                s.center
            };
            start.insert(s.id.as_str(), at);
        }

        // Primaries (and aliases that can't ride) are layout nodes.
        let mut riders_todo = Vec::new();
        for s in model.species() {
            let primary = s
                .alias_of
                .as_deref()
                .filter(|p| model.find_species(p).is_some());
            match primary {
                // A locked alias stays put, so it can't ride on its primary.
                Some(p) if !s.locked => riders_todo.push((s.id.as_str(), p)),
                _ => {
                    index.insert(
                        s.id.as_str(),
                        End {
                            node: nodes.len(),
                            offset: Pt::ZERO,
                        },
                    );
                    nodes.push(Node {
                        kind: NodeKind::Species(s.id.clone()),
                        pos: start[s.id.as_str()],
                        mass: 1.0 + *uses.get(s.id.as_str()).unwrap_or(&0) as f64,
                        fixed: s.locked,
                    });
                }
            }
        }
        let mut riders = Vec::new();
        for (alias, primary) in riders_todo {
            let host = index[primary];
            let offset = start[alias] - start[primary];
            index.insert(
                alias,
                End {
                    node: host.node,
                    offset,
                },
            );
            riders.push(Rider {
                id: alias.to_owned(),
                primary: host.node,
                offset,
            });
        }

        // Junctions, and one spring per unique species of each reaction.
        let mut springs = Vec::new();
        for r in model.reactions() {
            let ends: Vec<&str> = r
                .reactants
                .iter()
                .chain(&r.products)
                .map(|p| p.species.as_str())
                .filter(|id| index.contains_key(id))
                .collect();
            let junction = if params.randomize && !ends.is_empty() {
                let sum = ends.iter().fold(Pt::ZERO, |a, id| a + start[id]);
                sum / ends.len() as f64
            } else {
                r.junction
            };
            let j = nodes.len();
            nodes.push(Node {
                kind: NodeKind::Junction(r.id.clone()),
                pos: junction,
                mass: 1.0 + (r.reactants.len() + r.products.len()) as f64,
                fixed: false,
            });
            let mut seen: Vec<&str> = Vec::new();
            for id in ends {
                if !seen.contains(&id) {
                    seen.push(id);
                    springs.push((j, index[id]));
                }
            }
        }

        let centre = if nodes.is_empty() {
            Pt::ZERO
        } else {
            nodes.iter().fold(Pt::ZERO, |a, n| a + n.pos) / nodes.len() as f64
        };
        let (min, max) = nodes.iter().fold(
            (
                pt(f64::INFINITY, f64::INFINITY),
                pt(f64::NEG_INFINITY, f64::NEG_INFINITY),
            ),
            |(lo, hi), n| {
                (
                    pt(lo.x.min(n.pos.x), lo.y.min(n.pos.y)),
                    pt(hi.x.max(n.pos.x), hi.y.max(n.pos.y)),
                )
            },
        );
        let diagonal = if nodes.is_empty() {
            0.0
        } else {
            min.distance(max)
        };
        Self {
            params,
            speed: params.speed_start.max(0.05 * diagonal),
            nodes,
            springs,
            riders,
            centre,
            done: 0,
            rng,
        }
    }

    pub fn progress(&self) -> Progress {
        Progress {
            done: self.done,
            total: self.params.iterations,
        }
    }

    /// Runs up to `iterations` more iterations (S §10.1). Never reads a clock.
    pub fn step(&mut self, iterations: u32) -> Progress {
        for _ in 0..iterations {
            if self.done >= self.params.iterations {
                break;
            }
            self.iterate();
            self.done += 1;
        }
        self.progress()
    }

    fn iterate(&mut self) {
        let p = self.params;
        let n = self.nodes.len();
        let mut force = vec![Pt::ZERO; n];

        // Repulsion between every pair: Kr·mᵢ·mⱼ / d², with d ≥ 1.
        for i in 0..n {
            for k in i + 1..n {
                let mut delta = self.nodes[i].pos - self.nodes[k].pos;
                if delta.length() < 1e-9 {
                    // On top of each other: push apart in a random direction.
                    let a = self.rng.range(0.0, std::f64::consts::TAU);
                    delta = pt(a.cos(), a.sin());
                }
                let d = delta.length().max(1.0);
                let f =
                    normalize(delta) * (p.kr * self.nodes[i].mass * self.nodes[k].mass / (d * d));
                force[i] += f;
                force[k] -= f;
            }
        }
        // Springs: Ka·d, pulling a junction and its species together.
        for &(j, end) in &self.springs {
            let s = self.nodes[end.node].pos + end.offset;
            let pull = (s - self.nodes[j].pos) * p.ka;
            force[j] += pull;
            force[end.node] -= pull;
        }
        // Gravity towards the starting centroid, and the step limit.
        for (node, f) in self.nodes.iter_mut().zip(force) {
            if node.fixed {
                continue;
            }
            let mut step = f + (self.centre - node.pos) * (p.kg * node.mass);
            let len = step.length();
            if len > self.speed {
                step = step * (self.speed / len);
            }
            node.pos += step;
        }
        self.speed *= p.speed_decay;
    }

    /// Writes the current positions into `model`, for drawing the run as it
    /// goes. Handles are not touched.
    pub fn apply_positions(&self, model: &mut Model) {
        for node in &self.nodes {
            match &node.kind {
                NodeKind::Species(id) => {
                    if let Some(s) = model.find_species_mut(id) {
                        s.center = node.pos;
                    }
                }
                NodeKind::Junction(id) => {
                    if let Some(r) = model.find_reaction_mut(id) {
                        r.junction = node.pos;
                    }
                }
            }
        }
        for rider in &self.riders {
            if let Some(s) = model.find_species_mut(&rider.id) {
                s.center = self.nodes[rider.primary].pos + rider.offset;
            }
        }
    }

    /// Finishes the run: the remaining iterations, the final positions,
    /// re-centring, the Deckard pass if chosen, and handles (S §10.1):
    /// every participant's handles are reset to automatic (fix D-11), then the
    /// Deckard pass sets them for Bézier reactions, and smooth junctions get
    /// theirs recomputed.
    pub fn finish(mut self, model: &mut Model) {
        let remaining = self.params.iterations.saturating_sub(self.done);
        self.step(remaining);
        // Re-centre, unless locked nodes pin the drawing where it is.
        if !self.nodes.iter().any(|n| n.fixed) && !self.nodes.is_empty() {
            let now = self.nodes.iter().fold(Pt::ZERO, |a, n| a + n.pos) / self.nodes.len() as f64;
            let shift = self.centre - now;
            for n in &mut self.nodes {
                n.pos += shift;
            }
        }
        self.apply_positions(model);
        for r in model.reactions_mut() {
            for p in r.reactants.iter_mut().chain(&mut r.products) {
                p.ctrl_pts_set = false;
            }
        }
        if self.params.deckard {
            deckard_pass(model);
            if !self.nodes.iter().any(|n| n.fixed) {
                recentre(model, self.centre);
            }
        }
        let smooth: Vec<Id> = model
            .reactions()
            .iter()
            .filter(|r| r.mode == ReactionMode::Bezier && r.junction_smooth)
            .map(|r| r.id.clone())
            .collect();
        for id in smooth {
            apply_smooth_handles(model, &id);
        }
    }
}

/// Moves every species, junction and stored handle so the centroid of the
/// species and junctions is at `centre`.
fn recentre(model: &mut Model, centre: Pt) {
    let pts: Vec<Pt> = model
        .species()
        .iter()
        .map(|s| s.center)
        .chain(model.reactions().iter().map(|r| r.junction))
        .collect();
    if pts.is_empty() {
        return;
    }
    let shift = centre - pts.iter().fold(Pt::ZERO, |a, &p| a + p) / pts.len() as f64;
    for s in model.species_mut() {
        s.center += shift;
    }
    for r in model.reactions_mut() {
        r.junction += shift;
        for p in r.reactants.iter_mut().chain(&mut r.products) {
            if p.ctrl_pts_set {
                p.ctrl1 += shift;
                p.ctrl2 += shift;
            }
        }
    }
}

/// Deckard & Bergmann: looped species get a guide this far sideways, and the
/// inner handles are pulled back to this distance from the junction (S §10.2).
const DECKARD_LOOP_GUIDE: f64 = 50.0;
const DECKARD_PULL_BACK: f64 = 25.0;
/// Looped species: the two outer handles move this far apart, each way.
const DECKARD_LOOP_OUTER: f64 = 30.0;
/// Repeated species on one side: outer handles turned ±20° and lengthened by 10.
const DECKARD_DUPLICATE_ANGLE_DEG: f64 = 20.0;
const DECKARD_DUPLICATE_LENGTHEN: f64 = 10.0;

/// The Bézier handle pass (S §10.2), for every Bézier reaction: places the
/// junction and gives every reactant and product stored handles.
pub fn deckard_pass(model: &mut Model) {
    let ids: Vec<Id> = model
        .reactions()
        .iter()
        .filter(|r| r.mode == ReactionMode::Bezier)
        .map(|r| r.id.clone())
        .collect();
    for id in ids {
        deckard_reaction(model, &id);
    }
}

fn deckard_reaction(model: &mut Model, id: &str) {
    let Some(r) = model.find_reaction(id) else {
        return;
    };
    let centre_of = |ids: &[String]| {
        let pts: Vec<Pt> = ids
            .iter()
            .filter_map(|s| model.find_species(s))
            .map(|s| s.center)
            .collect();
        (!pts.is_empty()).then(|| pts.iter().fold(Pt::ZERO, |a, &p| a + p) / pts.len() as f64)
    };
    let reactants: Vec<String> = r.reactants.iter().map(|p| p.species.clone()).collect();
    let products: Vec<String> = r.products.iter().map(|p| p.species.clone()).collect();
    if reactants
        .iter()
        .chain(&products)
        .any(|s| model.find_species(s).is_none())
    {
        return;
    }
    let looped = reactants.iter().find(|s| products.contains(s)).cloned();

    // 1. Junction.
    let j = if looped.is_some() {
        let mut unique: Vec<String> = Vec::new();
        for s in reactants.iter().chain(&products) {
            if !unique.contains(s) {
                unique.push(s.clone());
            }
        }
        centre_of(&unique).unwrap_or(r.junction)
    } else {
        match (centre_of(&reactants), centre_of(&products)) {
            (Some(a), Some(b)) => (a + b) / 2.0,
            (Some(a), None) | (None, Some(a)) => a,
            (None, None) => r.junction,
        }
    };
    // 2. Substrate guide: the mean of the reactant centres and J.
    let mut sum = j;
    for s in &reactants {
        sum += model.find_species(s).expect("checked").center;
    }
    let mut g = sum / (reactants.len() + 1) as f64;
    // 3. Looped and UniUni cases.
    let a_centre = looped
        .as_deref()
        .map(|a| model.find_species(a).expect("checked").center);
    if let Some(a) = a_centre {
        g = j + perp(normalize(j - a)) * DECKARD_LOOP_GUIDE;
    } else if reactants.len() == 1 && products.len() == 1 {
        let rc = model.find_species(&reactants[0]).expect("checked").center;
        let pc = model.find_species(&products[0]).expect("checked").center;
        g = j + normalize(rc - pc) * j.distance(g);
    }
    // 4. Pull back: the inner handles are always 25 from J.
    g = j + normalize(g - j) * DECKARD_PULL_BACK;

    let centres: Vec<Pt> = reactants
        .iter()
        .chain(&products)
        .map(|s| model.find_species(s).expect("checked").center)
        .collect();
    let nr = reactants.len();
    let r = model.find_reaction_mut(id).expect("found above");
    r.junction = j;
    // 5. Handles.
    for (i, p) in r.reactants.iter_mut().enumerate() {
        let s = centres[i];
        p.ctrl1 = s + (j - s) * HANDLE_FRACTION;
        p.ctrl2 = g;
        p.ctrl_pts_set = true;
    }
    for (i, p) in r.products.iter_mut().enumerate() {
        let s = centres[nr + i];
        p.ctrl1 = j * 2.0 - g;
        p.ctrl2 = s + (j - s) * HANDLE_FRACTION;
        p.ctrl_pts_set = true;
    }
    // 6. A species repeated on one side: turn the two outer handles apart.
    let angle = DECKARD_DUPLICATE_ANGLE_DEG.to_radians();
    for (list, outer_is_ctrl1) in [(&mut r.reactants, true), (&mut r.products, false)] {
        let n = list.len();
        for a in 0..n {
            for b in a + 1..n {
                if list[a].species != list[b].species {
                    continue;
                }
                let s = centres[if outer_is_ctrl1 { a } else { nr + a }];
                for (k, sign) in [(a, 1.0), (b, -1.0)] {
                    let h = if outer_is_ctrl1 {
                        list[k].ctrl1
                    } else {
                        list[k].ctrl2
                    };
                    let v = h - s;
                    let (sin, cos) = (sign * angle).sin_cos();
                    let turned = pt(v.x * cos - v.y * sin, v.x * sin + v.y * cos);
                    let out = s + turned + normalize(turned) * DECKARD_DUPLICATE_LENGTHEN;
                    if outer_is_ctrl1 {
                        list[k].ctrl1 = out;
                    } else {
                        list[k].ctrl2 = out;
                    }
                }
            }
        }
    }
    // 7. Looped species: move the two arcs to A apart.
    if let (Some(a_id), Some(a)) = (looped.as_deref(), a_centre) {
        let p = perp(normalize(j - a));
        for rp in r.reactants.iter_mut().filter(|x| x.species == a_id) {
            rp.ctrl1 += p * DECKARD_LOOP_OUTER;
        }
        for pp in r.products.iter_mut().filter(|x| x.species == a_id) {
            pp.ctrl2 -= p * DECKARD_LOOP_OUTER;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Participant;

    #[test]
    fn rng_is_deterministic_and_in_range() {
        let mut a = Rng::new(42);
        let mut b = Rng::new(42);
        for _ in 0..1000 {
            let x = a.next_f64();
            assert_eq!(x, b.next_f64());
            assert!((0.0..1.0).contains(&x));
        }
        assert_ne!(Rng::new(1).next_u64(), Rng::new(2).next_u64());
        let mut r = Rng::new(7);
        for _ in 0..1000 {
            assert!(r.below(5) < 5);
            assert!((3.0..4.0).contains(&r.range(3.0, 4.0)));
        }
    }

    /// A → A + B (looped), with B and a third species C linked by C → B.
    fn looped() -> Model {
        let mut m = Model::new();
        let a = m.add_species(pt(100.0, 100.0), 80.0, 36.0);
        let b = m.add_species(pt(110.0, 105.0), 80.0, 36.0);
        let c = m.add_species(pt(120.0, 95.0), 80.0, 36.0);
        let r = m.add_reaction(pt(105.0, 100.0));
        let rr = m.find_reaction_mut(&r).unwrap();
        rr.mode = ReactionMode::Bezier;
        rr.reactants = vec![Participant::new(&a, 1.0)];
        rr.products = vec![Participant::new(&a, 1.0), Participant::new(&b, 1.0)];
        let r2 = m.add_reaction(pt(115.0, 100.0));
        let rr = m.find_reaction_mut(&r2).unwrap();
        rr.reactants = vec![Participant::new(&c, 1.0)];
        rr.products = vec![Participant::new(&b, 1.0)];
        m
    }

    fn run(m: &Model, params: LayoutParams, seed: u64) -> Model {
        let mut out = m.clone();
        LayoutRun::new(m, params, seed).finish(&mut out);
        out
    }

    fn overlapping(m: &Model) -> bool {
        let s = m.species();
        (0..s.len()).any(|i| {
            (i + 1..s.len()).any(|k| {
                (s[i].center.x - s[k].center.x).abs() < (s[i].width + s[k].width) / 2.0
                    && (s[i].center.y - s[k].center.y).abs() < (s[i].height + s[k].height) / 2.0
            })
        })
    }

    #[test]
    fn layout_pulls_crowded_nodes_apart() {
        let m = looped();
        assert!(overlapping(&m));
        let out = run(&m, LayoutParams::default(), 1);
        assert!(!overlapping(&out));
    }

    #[test]
    fn layout_is_deterministic_and_steps_dont_matter() {
        let m = looped();
        let params = LayoutParams {
            randomize: true,
            ..LayoutParams::default()
        };
        let once = run(&m, params, 99);
        // Same seed, run in uneven chunks.
        let mut run2 = LayoutRun::new(&m, params, 99);
        for n in [1, 7, 50, 3, 1000] {
            run2.step(n);
        }
        let mut chunked = m.clone();
        run2.finish(&mut chunked);
        assert_eq!(once, chunked);
        // A different seed scatters differently.
        assert_ne!(once, run(&m, params, 100));
    }

    #[test]
    fn locked_nodes_do_not_move() {
        let mut m = looped();
        m.find_species_mut("S1").unwrap().locked = true;
        let out = run(
            &m,
            LayoutParams {
                randomize: true,
                ..LayoutParams::default()
            },
            5,
        );
        assert_eq!(out.find_species("S1").unwrap().center, pt(100.0, 100.0));
        assert_ne!(out.find_species("S2").unwrap().center, pt(110.0, 105.0));
    }

    #[test]
    fn aliases_ride_on_their_primary() {
        let mut m = looped();
        let alias = m.add_alias("S2", pt(150.0, 150.0)).unwrap();
        let offset = pt(150.0, 150.0) - pt(110.0, 105.0);
        let out = run(&m, LayoutParams::default(), 3);
        let primary = out.find_species("S2").unwrap().center;
        let a = out.find_species(&alias).unwrap().center;
        assert!((a - primary).distance(offset) < 1e-9);
    }

    #[test]
    fn a_locked_alias_stays_put_while_its_primary_moves() {
        let mut m = looped();
        let alias = m.add_alias("S2", pt(150.0, 150.0)).unwrap();
        m.find_species_mut(&alias).unwrap().locked = true;
        let out = run(&m, LayoutParams::default(), 3);
        assert_eq!(out.find_species(&alias).unwrap().center, pt(150.0, 150.0));
    }

    #[test]
    fn running_twice_does_not_drift() {
        // A layout that has settled stays where it is: its centroid doesn't wander.
        let m = looped();
        let once = run(&m, LayoutParams::default(), 1);
        let twice = run(&once, LayoutParams::default(), 1);
        let centroid = |m: &Model| {
            let pts: Vec<Pt> = m
                .species()
                .iter()
                .map(|s| s.center)
                .chain(m.reactions().iter().map(|r| r.junction))
                .collect();
            pts.iter().fold(Pt::ZERO, |a, &p| a + p) / pts.len() as f64
        };
        assert!(centroid(&once).distance(centroid(&twice)) < 1e-6);
        for (a, b) in once.species().iter().zip(twice.species()) {
            assert!(a.center.distance(b.center) < 5.0, "{} moved", a.id);
        }
    }

    #[test]
    fn handles_are_reset_after_a_layout_without_deckard() {
        // Fix D-11: stored handles don't stay at their old absolute positions.
        let mut m = looped();
        for p in &mut m.find_reaction_mut("R4").unwrap().reactants {
            p.ctrl_pts_set = true;
        }
        let out = run(&m, LayoutParams::default(), 1);
        assert!(out.reactions().iter().all(|r| {
            r.reactants
                .iter()
                .chain(&r.products)
                .all(|p| !p.ctrl_pts_set)
        }));
    }

    #[test]
    fn deckard_keeps_the_arcs_of_a_looped_species_apart() {
        let m = looped();
        let out = run(
            &m,
            LayoutParams {
                deckard: true,
                ..LayoutParams::default()
            },
            1,
        );
        let r = out.find_reaction("R4").unwrap();
        assert!(
            r.reactants
                .iter()
                .chain(&r.products)
                .all(|p| p.ctrl_pts_set)
        );
        let a = &out.find_species("S1").unwrap().center;
        let arc_r = crate::bezier::leg_curve(&out, r, crate::model::Role::Reactant, 0).unwrap();
        let arc_p = crate::bezier::leg_curve(&out, r, crate::model::Role::Product, 0).unwrap();
        for i in 1..10 {
            let t = f64::from(i) / 10.0;
            assert!(arc_r.eval(t).distance(arc_p.eval(1.0 - t)) > 5.0, "t = {t}");
        }
        // The junction is at the centroid of the unique species, A and B.
        let b = out.find_species("S2").unwrap().center;
        assert!(r.junction.distance((*a + b) / 2.0) < 1e-6);
        // Non-Bézier reactions keep automatic handles.
        assert!(
            out.find_reaction("R5")
                .unwrap()
                .reactants
                .iter()
                .all(|p| !p.ctrl_pts_set)
        );
    }

    #[test]
    fn deckard_inner_handles_are_25_from_the_junction() {
        let mut m = Model::new();
        let a = m.add_species(pt(0.0, 0.0), 80.0, 36.0);
        let b = m.add_species(pt(400.0, 0.0), 80.0, 36.0);
        let c = m.add_species(pt(400.0, 200.0), 80.0, 36.0);
        let r = m.add_reaction(pt(0.0, 0.0));
        let rr = m.find_reaction_mut(&r).unwrap();
        rr.mode = ReactionMode::Bezier;
        rr.reactants = vec![Participant::new(&a, 1.0)];
        rr.products = vec![Participant::new(&b, 1.0), Participant::new(&c, 1.0)];
        deckard_pass(&mut m);
        let rr = m.find_reaction(&r).unwrap();
        let j = rr.junction;
        // Junction between the reactant centre and the product centre.
        assert!(j.distance((pt(0.0, 0.0) + pt(400.0, 100.0)) / 2.0) < 1e-9);
        assert!((rr.reactants[0].ctrl2.distance(j) - 25.0).abs() < 1e-9);
        // Products' inner handle is the reflection of G through J.
        assert!((rr.products[0].ctrl1 - j + (rr.reactants[0].ctrl2 - j)).length() < 1e-9);
        // Outer handles 35 % of the way from the species to J.
        assert!(rr.reactants[0].ctrl1.distance(j * 0.35) < 1e-9);
    }

    #[test]
    fn deckard_spreads_a_repeated_species() {
        // A + A → B: the two outer handles turn ±20° apart and get 10 longer.
        let mut m = Model::new();
        let a = m.add_species(pt(0.0, 0.0), 80.0, 36.0);
        let b = m.add_species(pt(400.0, 0.0), 80.0, 36.0);
        let r = m.add_reaction(pt(200.0, 0.0));
        let rr = m.find_reaction_mut(&r).unwrap();
        rr.mode = ReactionMode::Bezier;
        rr.reactants = vec![Participant::new(&a, 1.0), Participant::new(&a, 1.0)];
        rr.products = vec![Participant::new(&b, 1.0)];
        deckard_pass(&mut m);
        let rr = m.find_reaction(&r).unwrap();
        let (h0, h1) = (rr.reactants[0].ctrl1, rr.reactants[1].ctrl1);
        assert!(h0.distance(h1) > 30.0);
        // Both started at 0.35 × 200 = 70 from A; now 80.
        assert!((h0.length() - 80.0).abs() < 1e-9 && (h1.length() - 80.0).abs() < 1e-9);
    }

    #[test]
    fn empty_model_is_fine() {
        let m = Model::new();
        let out = run(&m, LayoutParams::default(), 1);
        assert_eq!(out, m);
    }
}
