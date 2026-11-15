//! Stress majorization (Gansner, Koren & North 2004; docs/layout-algorithms-design.md
//! L4): nodes are placed so that the distances between them on the screen
//! match their distances in the network. Started from classical scaling,
//! positions found from the distances alone, so the same network always
//! gives the same drawing. Each part of the network not joined to the rest
//! is laid out on its own, and the parts are packed side by side.

use crate::geometry::{Pt, pt};

/// The length of one line, a species to its reaction's junction, at 100%
/// spacing.
pub(super) const EDGE: f64 = 130.0;
/// Iterations at most; a run stops sooner once it no longer improves.
pub(super) const ITERATIONS: u32 = 300;
/// An iteration that lowers the stress by less than this share ends it.
const TOLERANCE: f64 = 1e-5;
/// Power iteration steps for classical scaling.
const POWER_STEPS: usize = 200;

/// The distances between nodes, and the run's progress.
#[derive(Clone, Debug)]
pub(super) struct Stress {
    n: usize,
    /// `d[i * n + j]`: the distance wanted between nodes `i` and `j`;
    /// infinite between parts the network does not join.
    d: Vec<f64>,
    /// Each node's part, numbered from the largest.
    part: Vec<usize>,
    /// The gap between packed parts.
    part_gap: f64,
    last: f64,
    pub converged: bool,
}

impl Stress {
    /// `n` nodes joined by `edges` of the given lengths; parts not joined
    /// are packed `part_gap` apart.
    pub fn new(n: usize, edges: &[(usize, usize, f64)], part_gap: f64) -> Self {
        let mut next: Vec<Vec<(usize, f64)>> = vec![Vec::new(); n];
        for &(a, b, len) in edges {
            if a != b {
                next[a].push((b, len));
                next[b].push((a, len));
            }
        }
        // Shortest distances from every node (Dijkstra).
        let mut d = vec![f64::INFINITY; n * n];
        for start in 0..n {
            let row = &mut d[start * n..(start + 1) * n];
            row[start] = 0.0;
            let mut done = vec![false; n];
            let mut heap = std::collections::BinaryHeap::new();
            heap.push((std::cmp::Reverse(Ordered(0.0)), start));
            while let Some((std::cmp::Reverse(Ordered(at)), i)) = heap.pop() {
                if done[i] {
                    continue;
                }
                done[i] = true;
                for &(k, len) in &next[i] {
                    let via = at + len;
                    if via < row[k] {
                        row[k] = via;
                        heap.push((std::cmp::Reverse(Ordered(via)), k));
                    }
                }
            }
        }
        // Parts, numbered from the largest (then by their first node).
        let mut part = vec![usize::MAX; n];
        let mut parts: Vec<Vec<usize>> = Vec::new();
        for i in 0..n {
            if part[i] == usize::MAX {
                let members: Vec<usize> = (0..n).filter(|&k| d[i * n + k].is_finite()).collect();
                for &k in &members {
                    part[k] = parts.len();
                }
                parts.push(members);
            }
        }
        let mut order: Vec<usize> = (0..parts.len()).collect();
        order.sort_by_key(|&p| (std::cmp::Reverse(parts[p].len()), parts[p][0]));
        let mut rank = vec![0; parts.len()];
        for (r, &p) in order.iter().enumerate() {
            rank[p] = r;
        }
        let part = part.iter().map(|&p| rank[p]).collect();
        Stress {
            n,
            d,
            part,
            part_gap,
            last: f64::INFINITY,
            converged: n < 2,
        }
    }

    fn dist(&self, i: usize, j: usize) -> f64 {
        self.d[i * self.n + j]
    }

    /// Positions from the distances alone: each part by classical scaling,
    /// then the parts packed in rows, the largest first, about the origin.
    pub fn initial(&self) -> Vec<Pt> {
        let n = self.n;
        let mut x = vec![Pt::ZERO; n];
        let parts = self.part.iter().max().map_or(0, |p| p + 1);
        let mut boxes: Vec<(Pt, Pt)> = Vec::new();
        for p in 0..parts {
            let members: Vec<usize> = (0..n).filter(|&i| self.part[i] == p).collect();
            let at = self.scaling(&members);
            let (mut lo, mut hi) = (
                pt(f64::INFINITY, f64::INFINITY),
                pt(f64::NEG_INFINITY, f64::NEG_INFINITY),
            );
            for (&i, &q) in members.iter().zip(&at) {
                x[i] = q;
                lo = pt(lo.x.min(q.x), lo.y.min(q.y));
                hi = pt(hi.x.max(q.x), hi.y.max(q.y));
            }
            boxes.push((lo, hi));
        }
        // Rows about as wide as all the parts would be square.
        let area: f64 = boxes
            .iter()
            .map(|(lo, hi)| (hi.x - lo.x + self.part_gap) * (hi.y - lo.y + self.part_gap))
            .sum();
        let width = area
            .sqrt()
            .max(boxes.first().map_or(0.0, |(lo, hi)| hi.x - lo.x));
        let (mut cx, mut cy, mut row) = (0.0, 0.0, 0.0_f64);
        let mut shift = vec![Pt::ZERO; parts];
        for (p, (lo, hi)) in boxes.iter().enumerate() {
            let (w, h) = (hi.x - lo.x, hi.y - lo.y);
            if cx > 0.0 && cx + w > width {
                cx = 0.0;
                cy += row + self.part_gap;
                row = 0.0;
            }
            shift[p] = pt(cx, cy) - *lo;
            cx += w + self.part_gap;
            row = row.max(h);
        }
        for (i, q) in x.iter_mut().enumerate() {
            *q += shift[self.part[i]];
        }
        let centre = x.iter().fold(Pt::ZERO, |a, &q| a + q) / n.max(1) as f64;
        x.iter_mut().for_each(|q| *q -= centre);
        x
    }

    /// Classical scaling of one part: the two leading eigenvectors of the
    /// double-centred squared distances, by power iteration from fixed
    /// starts.
    fn scaling(&self, members: &[usize]) -> Vec<Pt> {
        let n = members.len();
        if n < 2 {
            return vec![Pt::ZERO; n];
        }
        let sq: Vec<f64> = members
            .iter()
            .flat_map(|&i| members.iter().map(move |&j| (i, j)))
            .map(|(i, j)| self.dist(i, j).powi(2))
            .collect();
        let row: Vec<f64> = (0..n)
            .map(|i| sq[i * n..(i + 1) * n].iter().sum::<f64>() / n as f64)
            .collect();
        let all = row.iter().sum::<f64>() / n as f64;
        let b = |i: usize, j: usize| -0.5 * (sq[i * n + j] - row[i] - row[j] + all);
        let times = |v: &[f64]| -> Vec<f64> {
            (0..n)
                .map(|i| (0..n).map(|j| b(i, j) * v[j]).sum())
                .collect()
        };
        let unit = |v: &mut Vec<f64>| {
            let len = v.iter().map(|x| x * x).sum::<f64>().sqrt();
            if len > 0.0 {
                v.iter_mut().for_each(|x| *x /= len);
            }
        };
        let dot = |a: &[f64], b: &[f64]| a.iter().zip(b).map(|(x, y)| x * y).sum::<f64>();
        let mut found: Vec<(Vec<f64>, f64)> = Vec::new();
        for k in 0..2 {
            // A fixed start, different for each vector.
            let mut v: Vec<f64> = (0..n)
                .map(|i| ((i * (k + 1) * 7 + k * 3) % 11) as f64 - 5.0 + 0.1 * i as f64)
                .collect();
            for _ in 0..POWER_STEPS {
                for (u, _) in &found {
                    let c = dot(&v, u);
                    v.iter_mut().zip(u).for_each(|(x, y)| *x -= c * y);
                }
                unit(&mut v);
                v = times(&v);
            }
            let before = dot(&v, &v).sqrt();
            for (u, _) in &found {
                let c = dot(&v, u);
                v.iter_mut().zip(u).for_each(|(x, y)| *x -= c * y);
            }
            // A direction the distances do not use (two nodes have only
            // one) is left with rounding errors alone: it gets no weight.
            let after = dot(&v, &v).sqrt();
            unit(&mut v);
            let lambda = if after <= 1e-9 * before {
                0.0
            } else {
                dot(&v, &times(&v))
            };
            found.push((v, lambda.max(0.0)));
        }
        let (x, lx) = &found[0];
        let (y, ly) = &found[1];
        (0..n)
            .map(|i| pt(x[i] * lx.sqrt(), y[i] * ly.sqrt()))
            .collect()
    }

    /// The stress of `x`: the weighted squared differences between the
    /// distances drawn and wanted, the weight 1/d², within each part.
    pub fn stress(&self, x: &[Pt]) -> f64 {
        let mut s = 0.0;
        for i in 0..self.n {
            for j in i + 1..self.n {
                let d = self.dist(i, j);
                if d.is_finite() {
                    let e = x[i].distance(x[j]) - d;
                    s += e * e / (d * d);
                }
            }
        }
        s
    }

    /// One iteration (the localized majorization, every node from the
    /// others' places before it), within each part: nodes `fixed` stay put.
    pub fn iterate(&mut self, x: &mut [Pt], fixed: &[bool]) {
        let old = x.to_vec();
        for i in 0..self.n {
            if fixed[i] {
                continue;
            }
            let (mut sum, mut weights) = (Pt::ZERO, 0.0);
            for j in 0..self.n {
                let d = self.dist(i, j);
                if i == j || !d.is_finite() {
                    continue;
                }
                let w = 1.0 / (d * d);
                let delta = old[i] - old[j];
                let len = delta.length();
                // Two nodes on one point: apart in a direction of their own.
                let dir = if len > 1e-9 {
                    delta / len
                } else {
                    let a = ((i * 7919 + j * 104_729) % 360) as f64 * std::f64::consts::PI / 180.0;
                    pt(a.cos(), a.sin())
                };
                sum += (old[j] + dir * d) * w;
                weights += w;
            }
            if weights > 0.0 {
                x[i] = sum / weights;
            }
        }
        let now = self.stress(x);
        if self.last.is_finite() && self.last - now <= TOLERANCE * self.last {
            self.converged = true;
        }
        self.last = now;
    }
}

/// An `f64` that orders, for the heap (distances are never NaN).
#[derive(Clone, Copy, PartialEq)]
struct Ordered(f64);

impl Eq for Ordered {}

impl PartialOrd for Ordered {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Ordered {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.total_cmp(&other.0)
    }
}

/// Pushes apart boxes that overlap (centre, half size), each pair along the
/// axis it overlaps less on, half each, or all of it for the one not fixed;
/// up to `passes` times over. Boxes with no size are left out.
pub(super) fn separate(x: &mut [Pt], half: &[Pt], fixed: &[bool], gap: f64, passes: usize) {
    let n = x.len();
    for _ in 0..passes {
        let mut moved = false;
        for i in 0..n {
            for j in i + 1..n {
                if half[i] == Pt::ZERO || half[j] == Pt::ZERO || (fixed[i] && fixed[j]) {
                    continue;
                }
                let delta = x[j] - x[i];
                let over_x = half[i].x + half[j].x + gap - delta.x.abs();
                let over_y = half[i].y + half[j].y + gap - delta.y.abs();
                if over_x <= 0.0 || over_y <= 0.0 {
                    continue;
                }
                // Along the axis it overlaps less on; the same way round
                // whichever is first.
                let push = if over_x < over_y {
                    pt(if delta.x >= 0.0 { over_x } else { -over_x }, 0.0)
                } else {
                    pt(0.0, if delta.y >= 0.0 { over_y } else { -over_y })
                };
                match (fixed[i], fixed[j]) {
                    (true, _) => x[j] += push,
                    (_, true) => x[i] -= push,
                    _ => {
                        x[i] -= push * 0.5;
                        x[j] += push * 0.5;
                    }
                }
                moved = true;
            }
        }
        if !moved {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unit(edges: &[(usize, usize)]) -> Vec<(usize, usize, f64)> {
        edges.iter().map(|&(a, b)| (a, b, EDGE)).collect()
    }

    fn converge(s: &mut Stress, x: &mut [Pt], fixed: &[bool]) {
        for _ in 0..ITERATIONS {
            if s.converged {
                break;
            }
            s.iterate(x, fixed);
        }
    }

    #[test]
    fn distances_follow_the_lines() {
        // 0 – 1 – 2, a longer line 2 – 3, and 4 on its own.
        let s = Stress::new(5, &[(0, 1, EDGE), (1, 2, EDGE), (2, 3, 2.5 * EDGE)], EDGE);
        assert_eq!(s.dist(0, 1), EDGE);
        assert_eq!(s.dist(0, 2), 2.0 * EDGE);
        assert_eq!(s.dist(3, 0), 4.5 * EDGE);
        assert!(s.dist(0, 4).is_infinite());
        // The larger part first.
        assert_eq!(s.part, vec![0, 0, 0, 0, 1]);
    }

    #[test]
    fn a_chain_comes_out_straight() {
        // A path is drawn exactly: each line EDGE long, the ends 4 apart.
        let edges: Vec<(usize, usize)> = (0..4).map(|i| (i, i + 1)).collect();
        let mut s = Stress::new(5, &unit(&edges), EDGE);
        let mut x = s.initial();
        converge(&mut s, &mut x, &[false; 5]);
        for (a, b) in &edges {
            assert!((x[*a].distance(x[*b]) - EDGE).abs() < 1.0, "{x:?}");
        }
        assert!((x[0].distance(x[4]) - 4.0 * EDGE).abs() < 1.0, "{x:?}");
        assert!(s.stress(&x) < 1e-4);
    }

    #[test]
    fn iterations_lower_the_stress_and_fixed_nodes_stay() {
        // A ring of six, started from scaling, with node 0 held.
        let edges: Vec<(usize, usize)> = (0..6).map(|i| (i, (i + 1) % 6)).collect();
        let mut s = Stress::new(6, &unit(&edges), EDGE);
        let mut x = s.initial();
        let start = s.stress(&x);
        let mut fixed = vec![false; 6];
        fixed[0] = true;
        let held = x[0];
        let mut last = start;
        for _ in 0..50 {
            s.iterate(&mut x, &fixed);
            let now = s.stress(&x);
            assert!(now <= last + 1e-9, "{now} after {last}");
            last = now;
        }
        assert_eq!(x[0], held);
    }

    #[test]
    fn parts_are_packed_side_by_side() {
        // Two chains of three, not joined: each drawn as a chain, and the
        // two apart, not on top of each other.
        let mut s = Stress::new(6, &unit(&[(0, 1), (1, 2), (3, 4), (4, 5)]), EDGE);
        let mut x = s.initial();
        converge(&mut s, &mut x, &[false; 6]);
        assert!((x[0].distance(x[2]) - 2.0 * EDGE).abs() < 1.0, "{x:?}");
        assert!((x[3].distance(x[5]) - 2.0 * EDGE).abs() < 1.0, "{x:?}");
        let nearest = (0..3)
            .flat_map(|i| (3..6).map(move |j| (i, j)))
            .map(|(i, j)| x[i].distance(x[j]))
            .fold(f64::INFINITY, f64::min);
        assert!(nearest >= EDGE - 1.0, "{nearest}: {x:?}");
        // Not far apart either: within a few lines of each other.
        assert!(nearest < 4.0 * EDGE, "{nearest}");
    }

    #[test]
    fn small_networks() {
        assert!(Stress::new(0, &[], EDGE).initial().is_empty());
        assert_eq!(Stress::new(1, &[], EDGE).initial(), vec![Pt::ZERO]);
        let s = Stress::new(2, &[(0, 1, EDGE)], EDGE);
        let x = s.initial();
        assert!((x[0].distance(x[1]) - EDGE).abs() < 1e-6, "{x:?}");
    }

    #[test]
    fn overlapping_boxes_are_pushed_apart() {
        let half = pt(40.0, 18.0);
        // Three boxes on one point, the first held.
        let mut x = vec![Pt::ZERO, pt(1.0, 0.0), pt(0.0, 1.0)];
        separate(&mut x, &[half; 3], &[true, false, false], 4.0, 50);
        assert_eq!(x[0], Pt::ZERO, "held");
        for i in 0..3 {
            for j in i + 1..3 {
                let d = x[j] - x[i];
                assert!(
                    d.x.abs() >= 2.0 * half.x + 4.0 - 1e-9
                        || d.y.abs() >= 2.0 * half.y + 4.0 - 1e-9,
                    "{i} {j}: {x:?}"
                );
            }
        }
        // Two boxes that overlap a little side by side move apart sideways
        // only.
        let mut x = vec![Pt::ZERO, pt(70.0, 5.0)];
        separate(&mut x, &[half; 2], &[false, false], 4.0, 50);
        assert_eq!((x[0].y, x[1].y), (0.0, 5.0), "{x:?}");
        assert!((x[1].x - x[0].x - 84.0).abs() < 1e-9, "{x:?}");
        // A point (a junction) is left where it is.
        let mut x = vec![Pt::ZERO, Pt::ZERO];
        separate(&mut x, &[half, Pt::ZERO], &[false, false], 4.0, 50);
        assert_eq!(x, vec![Pt::ZERO, Pt::ZERO]);
    }
}
