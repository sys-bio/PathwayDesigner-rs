//! How readable a drawing is, measured, to compare layout methods on the
//! same models (docs/layout-algorithms-design.md L9). Each reactant and
//! product line is taken as drawn: its curve (sampled) for a Bézier
//! reaction, its corner points, then its side's anchor; a stem is a line of
//! its reaction's own.

use crate::bezier::leg_curve;
use crate::geometry::{Pt, pt};
use crate::model::{Model, ReactionMode, Role};

/// Points a Bézier line is sampled at.
const SAMPLES: usize = 8;
/// A reaction is drawn downwards when its products' centre is at least this
/// far below its reactants'.
const DOWN: f64 = 1.0;

/// The measures of one drawing.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Measures {
    /// Species drawn, null nodes aside, and reactions.
    pub species: usize,
    pub reactions: usize,
    /// Pairs of lines of different reactions, to different species, that
    /// cross.
    pub crossings: usize,
    /// Times a line passes through the box of a species other than its own.
    pub lines_through_species: usize,
    /// Pairs of species boxes that overlap.
    pub overlaps: usize,
    /// How much the lines' lengths (species centre to junction) differ: their
    /// standard deviation over their mean; 0 when all are the same.
    pub length_spread: f64,
    /// The share of reactions, with reactants and products, drawn with their
    /// products below their reactants; NaN with none to judge.
    pub downward: f64,
    /// The drawing's bounding box (species and junctions) per species.
    pub area_per_species: f64,
    /// That box's width over its height.
    pub aspect: f64,
}

/// One drawn line: its reaction, the species node it goes to (none for a
/// stem), and its points.
struct Line {
    reaction: usize,
    species: Option<String>,
    points: Vec<Pt>,
}

/// A species' box: min and max corners.
type Box2 = (Pt, Pt);

/// Measures `model`'s drawing.
pub fn measure(model: &Model) -> Measures {
    let boxes: Vec<(&str, Box2)> = model
        .species()
        .iter()
        .filter(|s| !s.is_null_node)
        .map(|s| {
            let half = pt(s.width / 2.0, s.height / 2.0);
            (s.id.as_str(), (s.center - half, s.center + half))
        })
        .collect();
    let lines = lines(model);

    let mut crossings = 0;
    for (i, a) in lines.iter().enumerate() {
        for b in &lines[i + 1..] {
            let shared =
                a.reaction == b.reaction || (a.species.is_some() && a.species == b.species);
            if !shared && polylines_cross(&a.points, &b.points) {
                crossings += 1;
            }
        }
    }

    let mut lines_through_species = 0;
    for line in &lines {
        for (id, b) in &boxes {
            if line.species.as_deref() != Some(id) && polyline_enters(&line.points, *b) {
                lines_through_species += 1;
            }
        }
    }

    let mut overlaps = 0;
    for (i, (_, a)) in boxes.iter().enumerate() {
        for (_, b) in &boxes[i + 1..] {
            if a.0.x < b.1.x && b.0.x < a.1.x && a.0.y < b.1.y && b.0.y < a.1.y {
                overlaps += 1;
            }
        }
    }

    // Line lengths, species centre to junction.
    let mut lengths = Vec::new();
    let mut judged = 0usize;
    let mut down = 0usize;
    for r in model.reactions() {
        let j = crate::render::draw::effective_junction(model, r);
        let centres = |role: Role| -> Vec<Pt> {
            r.participants(role)
                .iter()
                .filter_map(|p| model.find_species(&p.species).map(|s| s.center))
                .collect()
        };
        let (reactants, products) = (centres(Role::Reactant), centres(Role::Product));
        lengths.extend(reactants.iter().chain(&products).map(|c| (*c - j).length()));
        if !reactants.is_empty() && !products.is_empty() {
            judged += 1;
            if mean_y(&products) > mean_y(&reactants) + DOWN {
                down += 1;
            }
        }
    }
    let length_spread = if lengths.is_empty() {
        0.0
    } else {
        let n = lengths.len() as f64;
        let mean = lengths.iter().sum::<f64>() / n;
        let var = lengths.iter().map(|l| (l - mean).powi(2)).sum::<f64>() / n;
        if mean > 0.0 { var.sqrt() / mean } else { 0.0 }
    };

    // The bounding box of the species and the junctions.
    let corners = boxes.iter().flat_map(|(_, b)| [b.0, b.1]).chain(
        model
            .reactions()
            .iter()
            .map(|r| crate::render::draw::effective_junction(model, r)),
    );
    let (mut lo, mut hi) = (
        pt(f64::INFINITY, f64::INFINITY),
        pt(f64::NEG_INFINITY, f64::NEG_INFINITY),
    );
    for c in corners {
        lo = pt(lo.x.min(c.x), lo.y.min(c.y));
        hi = pt(hi.x.max(c.x), hi.y.max(c.y));
    }
    let (w, h) = if lo.x.is_finite() {
        (hi.x - lo.x, hi.y - lo.y)
    } else {
        (0.0, 0.0)
    };

    Measures {
        species: boxes.len(),
        reactions: model.reactions().len(),
        crossings,
        lines_through_species,
        overlaps,
        length_spread,
        downward: if judged == 0 {
            f64::NAN
        } else {
            down as f64 / judged as f64
        },
        area_per_species: if boxes.is_empty() {
            0.0
        } else {
            w * h / boxes.len() as f64
        },
        aspect: if h > 0.0 { w / h } else { 0.0 },
    }
}

fn mean_y(points: &[Pt]) -> f64 {
    points.iter().map(|p| p.y).sum::<f64>() / points.len() as f64
}

/// Every reactant and product line, and every stem, as drawn.
fn lines(model: &Model) -> Vec<Line> {
    let mut out = Vec::new();
    for (ri, r) in model.reactions().iter().enumerate() {
        for role in [Role::Reactant, Role::Product] {
            for (k, p) in r.participants(role).iter().enumerate() {
                let Some(s) = model.find_species(&p.species) else {
                    continue;
                };
                let mut points = vec![s.center];
                if r.mode == ReactionMode::Bezier
                    && let Some(c) = leg_curve(model, r, role, k)
                {
                    points.extend((1..=SAMPLES).map(|i| c.eval(i as f64 / SAMPLES as f64)));
                }
                points.extend(p.points.iter().copied());
                points.push(r.leg_anchor(role));
                points.dedup();
                out.push(Line {
                    reaction: ri,
                    species: Some(s.id.clone()),
                    points,
                });
            }
        }
        // A stem's ends are kept relative to the junction.
        if r.stem.is_some() {
            out.push(Line {
                reaction: ri,
                species: None,
                points: vec![
                    r.leg_anchor(Role::Reactant),
                    r.junction,
                    r.leg_anchor(Role::Product),
                ],
            });
        }
    }
    out
}

/// Whether two polylines cross: some segment of one properly crosses some
/// segment of the other.
fn polylines_cross(a: &[Pt], b: &[Pt]) -> bool {
    let bounds = |p: &[Pt]| {
        p.iter().fold(
            (
                pt(f64::INFINITY, f64::INFINITY),
                pt(f64::NEG_INFINITY, f64::NEG_INFINITY),
            ),
            |(lo, hi), q| {
                (
                    pt(lo.x.min(q.x), lo.y.min(q.y)),
                    pt(hi.x.max(q.x), hi.y.max(q.y)),
                )
            },
        )
    };
    let (alo, ahi) = bounds(a);
    let (blo, bhi) = bounds(b);
    if ahi.x < blo.x || bhi.x < alo.x || ahi.y < blo.y || bhi.y < alo.y {
        return false;
    }
    a.windows(2)
        .any(|s| b.windows(2).any(|t| segments_cross(s[0], s[1], t[0], t[1])))
}

/// Whether segments `ab` and `cd` cross at a point inside both.
fn segments_cross(a: Pt, b: Pt, c: Pt, d: Pt) -> bool {
    let side = |p: Pt, q: Pt, r: Pt| (q.x - p.x) * (r.y - p.y) - (q.y - p.y) * (r.x - p.x);
    let (d1, d2) = (side(a, b, c), side(a, b, d));
    let (d3, d4) = (side(c, d, a), side(c, d, b));
    d1 * d2 < 0.0 && d3 * d4 < 0.0
}

/// Whether a polyline goes into a box: a point inside it, or a segment
/// across one of its sides.
fn polyline_enters(points: &[Pt], (lo, hi): Box2) -> bool {
    let inside = |p: &Pt| p.x > lo.x && p.x < hi.x && p.y > lo.y && p.y < hi.y;
    if points.iter().any(inside) {
        return true;
    }
    let sides = [
        (lo, pt(hi.x, lo.y)),
        (pt(hi.x, lo.y), hi),
        (hi, pt(lo.x, hi.y)),
        (pt(lo.x, hi.y), lo),
    ];
    points.windows(2).any(|s| {
        sides
            .iter()
            .any(|(c, d)| segments_cross(s[0], s[1], *c, *d))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::antimony::{ImportOptions, import};

    /// Two one-to-one reactions, A → B and C → D, drawn straight, with their
    /// species at the given places and each junction halfway.
    fn two(a: Pt, b: Pt, c: Pt, d: Pt) -> Model {
        let mut m = import("J0: A -> B; 1\nJ1: C -> D; 1", &ImportOptions { seed: 1 })
            .unwrap()
            .model;
        for (id, at) in [("A", a), ("B", b), ("C", c), ("D", d)] {
            m.find_species_mut(id).unwrap().center = at;
        }
        for (id, from, to) in [("J0", a, b), ("J1", c, d)] {
            let r = m.find_reaction_mut(id).unwrap();
            r.mode = ReactionMode::Straight;
            r.junction = (from + to) * 0.5;
        }
        m
    }

    #[test]
    fn crossing_lines_are_counted() {
        // An X: A–B from top left to bottom right, C–D the other way, a
        // little lower, so the junctions are apart: C's line crosses B's.
        let x = measure(&two(
            pt(0.0, 0.0),
            pt(400.0, 400.0),
            pt(400.0, 40.0),
            pt(0.0, 440.0),
        ));
        assert_eq!(x.crossings, 1);
        // Lines that only meet at a point are not counted: the same X with
        // both junctions at its centre.
        let touching = measure(&two(
            pt(0.0, 0.0),
            pt(400.0, 400.0),
            pt(400.0, 0.0),
            pt(0.0, 400.0),
        ));
        assert_eq!(touching.crossings, 0);
        // Side by side.
        let apart = measure(&two(
            pt(0.0, 0.0),
            pt(0.0, 400.0),
            pt(300.0, 0.0),
            pt(300.0, 400.0),
        ));
        assert_eq!(apart.crossings, 0);
        assert_eq!(apart.reactions, 2);
        assert_eq!(apart.species, 4);
    }

    #[test]
    fn downward_reactions_overlaps_and_lines_through_species() {
        // Both drawn downwards, apart.
        let m = two(
            pt(0.0, 0.0),
            pt(0.0, 400.0),
            pt(300.0, 0.0),
            pt(300.0, 400.0),
        );
        let down = measure(&m);
        assert_eq!(down.downward, 1.0);
        assert_eq!(down.overlaps, 0);
        assert_eq!(down.lines_through_species, 0);
        assert_eq!(down.length_spread, 0.0, "every line 200 long");
        // J1 drawn upwards: half of them down.
        let half = measure(&two(
            pt(0.0, 0.0),
            pt(0.0, 400.0),
            pt(300.0, 400.0),
            pt(300.0, 0.0),
        ));
        assert_eq!(half.downward, 0.5);
        // C on top of A, D below B, all in one column: one overlap, and
        // J1's lines run through A's and B's boxes.
        let mut m = two(pt(0.0, 0.0), pt(0.0, 400.0), pt(0.0, 10.0), pt(0.0, 800.0));
        m.find_reaction_mut("J1").unwrap().junction = pt(0.0, 405.0);
        let on = measure(&m);
        assert_eq!(on.overlaps, 1, "A and C overlap");
        // A's line through C; C's through A and B; D's starts in B.
        assert_eq!(on.lines_through_species, 4, "{on:?}");
    }

    #[test]
    fn a_stem_is_drawn_about_its_junction() {
        // J0 down the left, J1 down the right: apart. J1's stem, kept
        // relative to its junction, stays on the right.
        let mut m = two(
            pt(0.0, 0.0),
            pt(0.0, 400.0),
            pt(300.0, 0.0),
            pt(300.0, 400.0),
        );
        m.find_reaction_mut("J1").unwrap().stem = Some((pt(0.0, -30.0), pt(0.0, 30.0)));
        assert_eq!(measure(&m).crossings, 0);
        // Its reactant end moved left of J0: the stem and C's line, which
        // runs to that end, both cross A's line.
        m.find_reaction_mut("J1").unwrap().stem = Some((pt(-400.0, -100.0), pt(0.0, 30.0)));
        assert_eq!(measure(&m).crossings, 2);
    }

    #[test]
    fn lines_that_share_a_reaction_or_a_species_are_not_crossings() {
        // A reaction's own lines, and lines to one species, meet where they
        // join; drawn through corner points they can cross, which is not
        // counted.
        let mut m = import("J0: A + B -> C; 1", &ImportOptions { seed: 1 })
            .unwrap()
            .model;
        for (id, at) in [
            ("A", pt(0.0, 0.0)),
            ("B", pt(200.0, 0.0)),
            ("C", pt(100.0, 400.0)),
        ] {
            m.find_species_mut(id).unwrap().center = at;
        }
        let r = m.find_reaction_mut("J0").unwrap();
        r.mode = ReactionMode::Straight;
        r.junction = pt(100.0, 200.0);
        // A's line swings right of B's: they cross.
        r.reactants[0].points = vec![pt(300.0, 100.0)];
        assert_eq!(measure(&m).crossings, 0, "one reaction's lines");

        let mut m = import("J0: A -> B; 1\nJ1: A -> C; 1", &ImportOptions { seed: 1 })
            .unwrap()
            .model;
        for (id, at) in [
            ("A", pt(0.0, 0.0)),
            ("B", pt(0.0, 400.0)),
            ("C", pt(400.0, 400.0)),
        ] {
            m.find_species_mut(id).unwrap().center = at;
        }
        for (id, j) in [("J0", pt(0.0, 200.0)), ("J1", pt(200.0, 200.0))] {
            let r = m.find_reaction_mut(id).unwrap();
            r.mode = ReactionMode::Straight;
            r.junction = j;
        }
        // A's line to J1 goes round the left of J0's, across it.
        m.find_reaction_mut("J1").unwrap().reactants[0].points = vec![pt(-100.0, 100.0)];
        assert_eq!(measure(&m).crossings, 0, "lines to one species");
    }

    #[test]
    fn a_line_inside_a_box() {
        // A's box made big enough to hold every other line whole: B's, C's
        // and D's lines are in it, and it overlaps their boxes.
        let mut m = two(
            pt(0.0, 0.0),
            pt(0.0, 400.0),
            pt(300.0, 0.0),
            pt(300.0, 400.0),
        );
        let a = m.find_species_mut("A").unwrap();
        a.width = 5000.0;
        a.height = 5000.0;
        let s = measure(&m);
        assert_eq!(s.lines_through_species, 3, "{s:?}");
        assert_eq!(s.overlaps, 3);
    }

    #[test]
    fn the_spread_of_line_lengths() {
        // Each junction a quarter of the way down: lines 100 and 300 long,
        // a mean of 200 and a standard deviation of 100.
        let mut m = two(
            pt(0.0, 0.0),
            pt(0.0, 400.0),
            pt(300.0, 0.0),
            pt(300.0, 400.0),
        );
        m.find_reaction_mut("J0").unwrap().junction = pt(0.0, 100.0);
        m.find_reaction_mut("J1").unwrap().junction = pt(300.0, 100.0);
        assert!((measure(&m).length_spread - 0.5).abs() < 1e-9);
    }

    #[test]
    fn a_curve_is_measured_as_drawn() {
        // J0 down the left; A's line bulges right, across J1's.
        let mut m = two(
            pt(0.0, 0.0),
            pt(0.0, 400.0),
            pt(300.0, 0.0),
            pt(300.0, 400.0),
        );
        assert_eq!(measure(&m).crossings, 0, "drawn straight");
        let r = m.find_reaction_mut("J0").unwrap();
        r.mode = ReactionMode::Bezier;
        let a = &mut r.reactants[0];
        a.ctrl1 = pt(600.0, 50.0);
        a.ctrl2 = pt(600.0, 150.0);
        a.ctrl_pts_set = true;
        assert_eq!(measure(&m).crossings, 1);
    }

    #[test]
    fn the_drawings_size() {
        let m = two(
            pt(0.0, 0.0),
            pt(0.0, 400.0),
            pt(300.0, 0.0),
            pt(300.0, 400.0),
        );
        let s = measure(&m);
        let a = m.find_species("A").unwrap();
        let (w, h) = (300.0 + a.width, 400.0 + a.height);
        assert!((s.area_per_species - w * h / 4.0).abs() < 1e-9, "{s:?}");
        assert!((s.aspect - w / h).abs() < 1e-9);
    }
}
