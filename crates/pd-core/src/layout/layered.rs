//! A layered layout (Sugiyama, Tagawa & Toda 1981; docs/layout-algorithms-design.md
//! L5): every line points down, from a reaction's reactants to its junction
//! and on to its products, so the network reads top to bottom.
//!
//! - **Cycles** are broken by turning round the fewest lines the greedy
//!   ordering of Eades, Lin & Smyth (1993) finds; those are drawn upwards.
//! - **Layers**: each node one below the lowest of its predecessors, then as
//!   low as its successors allow, so an input sits just above its use.
//! - A line across several layers gets a **corner point** in each between.
//! - **Order** in each layer: sweeps down and up by the barycentre of the
//!   neighbours, keeping the order with the fewest crossings.
//! - **Places** across: each node as near the mean of its neighbours as the
//!   order and the gaps allow, exactly (an ordered least-squares fit, pool
//!   adjacent violators), down and up a few times.
//! - Each part of the network not joined to the rest is laid out on its own,
//!   and the parts are packed in rows.
//!
//! All of it is deterministic: ties go by node number.

use crate::geometry::{Pt, pt};

/// The gap between neighbours in a layer, and between layers.
pub(super) const GAP: f64 = 40.0;
pub(super) const LAYER_GAP: f64 = 50.0;
/// The width given a corner point of a long line.
const DUMMY_WIDTH: f64 = 10.0;
/// Sweeps to order the layers, and rounds to place them across.
const SWEEPS: usize = 24;
const ROUNDS: usize = 8;
/// The gap between packed parts.
const PART_GAP: f64 = 80.0;

/// The network to lay out: nodes with their sizes, and lines from one to
/// another (reactant to junction, junction to product).
pub(super) struct Graph {
    pub size: Vec<Pt>,
    pub lines: Vec<(usize, usize)>,
}

/// The result: a centre for each node, and for each line its corner points
/// in order from its first node to its second.
#[derive(Debug, PartialEq)]
pub(super) struct Placed {
    pub pos: Vec<Pt>,
    pub corners: Vec<Vec<Pt>>,
    /// The lines drawn upwards, to break cycles.
    pub upward: Vec<bool>,
}

pub(super) fn lay_out(g: &Graph) -> Placed {
    let n = g.size.len();
    let mut placed = Placed {
        pos: vec![Pt::ZERO; n],
        corners: vec![Vec::new(); g.lines.len()],
        upward: vec![false; g.lines.len()],
    };
    // Parts, by their first node.
    let mut part = vec![usize::MAX; n];
    let mut parts: Vec<Vec<usize>> = Vec::new();
    let mut next: Vec<Vec<usize>> = vec![Vec::new(); n];
    for &(a, b) in &g.lines {
        next[a].push(b);
        next[b].push(a);
    }
    for start in 0..n {
        if part[start] != usize::MAX {
            continue;
        }
        let mut members = vec![start];
        part[start] = parts.len();
        let mut k = 0;
        while k < members.len() {
            for &m in &next[members[k]] {
                if part[m] == usize::MAX {
                    part[m] = parts.len();
                    members.push(m);
                }
            }
            k += 1;
        }
        members.sort_unstable();
        parts.push(members);
    }
    // Each part on its own, its box from the origin.
    let mut boxes: Vec<(Pt, Pt)> = Vec::new();
    for members in &parts {
        boxes.push(lay_out_part(g, members, &mut placed));
    }
    // The largest first, in rows about as wide as all would be square.
    let mut order: Vec<usize> = (0..parts.len()).collect();
    order.sort_by_key(|&p| (std::cmp::Reverse(parts[p].len()), parts[p][0]));
    let area: f64 = boxes
        .iter()
        .map(|(lo, hi)| (hi.x - lo.x + PART_GAP) * (hi.y - lo.y + PART_GAP))
        .sum();
    let width = area
        .sqrt()
        .max(order.first().map_or(0.0, |&p| boxes[p].1.x - boxes[p].0.x));
    let (mut cx, mut cy, mut row) = (0.0, 0.0, 0.0_f64);
    let mut shift = vec![Pt::ZERO; parts.len()];
    for &p in &order {
        let (lo, hi) = boxes[p];
        let (w, h) = (hi.x - lo.x, hi.y - lo.y);
        if cx > 0.0 && cx + w > width {
            cx = 0.0;
            cy += row + PART_GAP;
            row = 0.0;
        }
        shift[p] = pt(cx, cy) - lo;
        cx += w + PART_GAP;
        row = row.max(h);
    }
    for (i, q) in placed.pos.iter_mut().enumerate() {
        *q += shift[part[i]];
    }
    for (l, &(a, _)) in g.lines.iter().enumerate() {
        for q in &mut placed.corners[l] {
            *q += shift[part[a]];
        }
    }
    placed
}

/// Lays out one part; returns its box (of the nodes' boxes).
fn lay_out_part(g: &Graph, members: &[usize], placed: &mut Placed) -> (Pt, Pt) {
    // Local numbers.
    let local: std::collections::HashMap<usize, usize> =
        members.iter().enumerate().map(|(k, &i)| (i, k)).collect();
    let n = members.len();
    let lines: Vec<(usize, usize, usize)> = g
        .lines
        .iter()
        .enumerate()
        .filter(|(_, (a, _))| local.contains_key(a))
        .map(|(l, &(a, b))| (l, local[&a], local[&b]))
        .collect();

    // Cycles: the greedy order; a line against it is turned round.
    let rank = greedy_order(n, &lines);
    let mut down: Vec<(usize, usize, usize)> = Vec::new(); // (line, upper, lower)
    for &(l, a, b) in &lines {
        if a == b {
            continue;
        }
        if rank[a] < rank[b] {
            down.push((l, a, b));
        } else {
            placed.upward[l] = true;
            down.push((l, b, a));
        }
    }

    // Layers: below the lowest predecessor, then as low as the successors
    // allow.
    let mut preds: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut succs: Vec<Vec<usize>> = vec![Vec::new(); n];
    for &(_, a, b) in &down {
        if !succs[a].contains(&b) {
            succs[a].push(b);
            preds[b].push(a);
        }
    }
    let topo = topological(n, &preds, &succs);
    let mut layer = vec![0usize; n];
    for &v in &topo {
        layer[v] = preds[v].iter().map(|&p| layer[p] + 1).max().unwrap_or(0);
    }
    for &v in topo.iter().rev() {
        if let Some(lowest) = succs[v].iter().map(|&s| layer[s]).min() {
            layer[v] = lowest - 1;
        }
    }

    // Corner points: a node in each layer a line crosses.
    let mut size: Vec<Pt> = members.iter().map(|&i| g.size[i]).collect();
    let mut chain: Vec<(usize, Vec<usize>)> = Vec::new(); // (line, nodes top to bottom)
    let mut up: Vec<Vec<usize>> = vec![Vec::new(); n]; // neighbours above
    let mut below: Vec<Vec<usize>> = vec![Vec::new(); n];
    for &(l, a, b) in &down {
        let mut path = vec![a];
        for k in layer[a] + 1..layer[b] {
            size.push(pt(DUMMY_WIDTH, 0.0));
            layer.push(k);
            up.push(Vec::new());
            below.push(Vec::new());
            path.push(size.len() - 1);
        }
        path.push(b);
        for w in path.windows(2) {
            if !below[w[0]].contains(&w[1]) {
                below[w[0]].push(w[1]);
                up[w[1]].push(w[0]);
            }
        }
        chain.push((l, path));
    }
    let total = size.len();
    let layers = layer.iter().max().map_or(0, |m| m + 1);

    // Order: first as found from the top, then sweeps by barycentre.
    let mut rows: Vec<Vec<usize>> = vec![Vec::new(); layers];
    let mut seen = vec![false; total];
    for v in topo.iter().copied().chain(n..total) {
        let mut stack = vec![v];
        while let Some(u) = stack.pop() {
            if seen[u] {
                continue;
            }
            seen[u] = true;
            rows[layer[u]].push(u);
            for &s in below[u].iter().rev() {
                stack.push(s);
            }
        }
    }
    let mut best = rows.clone();
    let mut fewest = crossings(&rows, &below);
    for sweep in 0..SWEEPS {
        let downwards = sweep % 2 == 0;
        let span: Vec<usize> = if downwards {
            (1..layers).collect()
        } else {
            (0..layers.saturating_sub(1)).rev().collect()
        };
        for l in span {
            if downwards {
                by_barycentre(&mut rows, l, l - 1, &up);
            } else {
                by_barycentre(&mut rows, l, l + 1, &below);
            }
        }
        transpose(&mut rows, &up, &below);
        let c = crossings(&rows, &below);
        if c < fewest {
            fewest = c;
            best = rows.clone();
        }
    }
    let rows = best;

    // Heights of the layers.
    let mut y = vec![0.0; layers];
    let height = |l: usize| rows[l].iter().map(|&v| size[v].y).fold(0.0, f64::max);
    for l in 1..layers {
        y[l] = y[l - 1] + height(l - 1) / 2.0 + height(l) / 2.0 + LAYER_GAP;
    }

    // Places across: packed first, then each layer towards its neighbours.
    let mut x = vec![0.0; total];
    for row in &rows {
        let at = fit(row, &size, &vec![0.0; row.len()]);
        for (&v, a) in row.iter().zip(at) {
            x[v] = a;
        }
        // Centred on zero.
        let mid = row.iter().map(|&v| x[v]).sum::<f64>() / row.len().max(1) as f64;
        row.iter().for_each(|&v| x[v] -= mid);
    }
    for round in 0..ROUNDS {
        for downwards in [true, false] {
            let span: Vec<usize> = if downwards {
                (1..layers).collect()
            } else {
                (0..layers.saturating_sub(1)).rev().collect()
            };
            for l in span {
                let by = if downwards { &up } else { &below };
                let last = round == ROUNDS - 1;
                let wanted: Vec<f64> = rows[l]
                    .iter()
                    .map(|&v| {
                        // The last round looks both ways.
                        let near: Vec<f64> = if last {
                            up[v].iter().chain(&below[v]).map(|&u| x[u]).collect()
                        } else {
                            by[v].iter().map(|&u| x[u]).collect()
                        };
                        if near.is_empty() {
                            x[v]
                        } else {
                            near.iter().sum::<f64>() / near.len() as f64
                        }
                    })
                    .collect();
                let at = fit(&rows[l], &size, &wanted);
                for (&v, a) in rows[l].iter().zip(at) {
                    x[v] = a;
                }
            }
        }
    }

    // Out: the part's nodes, the lines' corner points, the box.
    let (mut lo, mut hi) = (
        pt(f64::INFINITY, f64::INFINITY),
        pt(f64::NEG_INFINITY, f64::NEG_INFINITY),
    );
    for (k, &i) in members.iter().enumerate() {
        let c = pt(x[k], y[layer[k]]);
        placed.pos[i] = c;
        let h = g.size[i] * 0.5;
        lo = pt(lo.x.min(c.x - h.x), lo.y.min(c.y - h.y));
        hi = pt(hi.x.max(c.x + h.x), hi.y.max(c.y + h.y));
    }
    for (l, path) in chain {
        let mut corners: Vec<Pt> = path[1..path.len() - 1]
            .iter()
            .map(|&v| pt(x[v], y[layer[v]]))
            .collect();
        // Listed from the line's first node.
        if placed.upward[l] {
            corners.reverse();
        }
        placed.corners[l] = corners;
    }
    (lo, hi)
}

/// The greedy order of Eades, Lin & Smyth: sinks to the end, sources to the
/// front, else the node with the most more lines out than in. Returns each
/// node's place in it.
fn greedy_order(n: usize, lines: &[(usize, usize, usize)]) -> Vec<usize> {
    let mut outs: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut ins: Vec<Vec<usize>> = vec![Vec::new(); n];
    for &(_, a, b) in lines {
        if a != b {
            outs[a].push(b);
            ins[b].push(a);
        }
    }
    let mut left = vec![true; n];
    let count = |list: &Vec<usize>, left: &[bool]| list.iter().filter(|&&k| left[k]).count();
    let (mut front, mut back) = (Vec::new(), Vec::new());
    let mut remaining = n;
    while remaining > 0 {
        let mut changed = true;
        while changed {
            changed = false;
            for v in 0..n {
                if left[v] && count(&outs[v], &left) == 0 {
                    left[v] = false;
                    back.push(v);
                    remaining -= 1;
                    changed = true;
                }
            }
            for v in 0..n {
                if left[v] && count(&ins[v], &left) == 0 {
                    left[v] = false;
                    front.push(v);
                    remaining -= 1;
                    changed = true;
                }
            }
        }
        if let Some(v) = (0..n).filter(|&v| left[v]).max_by_key(|&v| {
            (
                count(&outs[v], &left) as i64 - count(&ins[v], &left) as i64,
                std::cmp::Reverse(v),
            )
        }) {
            left[v] = false;
            front.push(v);
            remaining -= 1;
        }
    }
    back.reverse();
    let mut rank = vec![0; n];
    for (k, &v) in front.iter().chain(&back).enumerate() {
        rank[v] = k;
    }
    rank
}

/// A topological order of an acyclic network, lowest number first.
fn topological(n: usize, preds: &[Vec<usize>], succs: &[Vec<usize>]) -> Vec<usize> {
    let mut waiting: Vec<usize> = preds.iter().map(Vec::len).collect();
    let mut ready: std::collections::BTreeSet<usize> =
        (0..n).filter(|&v| waiting[v] == 0).collect();
    let mut out = Vec::with_capacity(n);
    while let Some(v) = ready.pop_first() {
        out.push(v);
        for &s in &succs[v] {
            waiting[s] -= 1;
            if waiting[s] == 0 {
                ready.insert(s);
            }
        }
    }
    out
}

/// Orders layer `l` by the mean place of each node's neighbours (`by`) in
/// layer `fixed`; a node with none keeps its place, ties keep their order.
fn by_barycentre(rows: &mut [Vec<usize>], l: usize, fixed: usize, by: &[Vec<usize>]) {
    let at: std::collections::HashMap<usize, f64> = rows[fixed]
        .iter()
        .enumerate()
        .map(|(k, &v)| (v, k as f64))
        .collect();
    let mut keyed: Vec<(f64, usize, usize)> = rows[l]
        .iter()
        .enumerate()
        .map(|(k, &v)| {
            let near: Vec<f64> = by[v].iter().filter_map(|u| at.get(u).copied()).collect();
            let key = if near.is_empty() {
                k as f64
            } else {
                near.iter().sum::<f64>() / near.len() as f64
            };
            (key, k, v)
        })
        .collect();
    keyed.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    rows[l] = keyed.into_iter().map(|(_, _, v)| v).collect();
}

/// Swaps neighbours in a layer wherever that leaves fewer crossings with
/// the layers above and below (Gansner et al.'s transpose), until no swap
/// helps.
fn transpose(rows: &mut [Vec<usize>], up: &[Vec<usize>], below: &[Vec<usize>]) {
    // Crossings between the lines of `a` and of `b`, `a` left of `b`, to
    // the layer `other` (positions given).
    let between =
        |a: usize, b: usize, near: &[Vec<usize>], at: &std::collections::HashMap<usize, usize>| {
            let mut c = 0;
            for x in near[a].iter().filter_map(|u| at.get(u)) {
                for y in near[b].iter().filter_map(|u| at.get(u)) {
                    if x > y {
                        c += 1;
                    }
                }
            }
            c
        };
    let mut improved = true;
    let mut rounds = 0;
    while improved && rounds < 50 {
        improved = false;
        rounds += 1;
        for l in 0..rows.len() {
            let above: std::collections::HashMap<usize, usize> = if l > 0 {
                rows[l - 1]
                    .iter()
                    .enumerate()
                    .map(|(k, &v)| (v, k))
                    .collect()
            } else {
                Default::default()
            };
            let under: std::collections::HashMap<usize, usize> = if l + 1 < rows.len() {
                rows[l + 1]
                    .iter()
                    .enumerate()
                    .map(|(k, &v)| (v, k))
                    .collect()
            } else {
                Default::default()
            };
            for k in 0..rows[l].len().saturating_sub(1) {
                let (a, b) = (rows[l][k], rows[l][k + 1]);
                let now = between(a, b, up, &above) + between(a, b, below, &under);
                let swapped = between(b, a, up, &above) + between(b, a, below, &under);
                if swapped < now {
                    rows[l].swap(k, k + 1);
                    improved = true;
                }
            }
        }
    }
}

/// How many pairs of lines cross between each layer and the next.
fn crossings(rows: &[Vec<usize>], below: &[Vec<usize>]) -> usize {
    let mut total = 0;
    for l in 0..rows.len().saturating_sub(1) {
        let at: std::collections::HashMap<usize, usize> = rows[l + 1]
            .iter()
            .enumerate()
            .map(|(k, &v)| (v, k))
            .collect();
        let pairs: Vec<(usize, usize)> = rows[l]
            .iter()
            .enumerate()
            .flat_map(|(k, &v)| {
                below[v]
                    .iter()
                    .filter_map(|w| at.get(w).map(|&j| (k, j)))
                    .collect::<Vec<_>>()
            })
            .collect();
        // Listed left to right on top, so a pair crosses when the later
        // one ends further left below.
        for (i, a) in pairs.iter().enumerate() {
            for b in &pairs[i + 1..] {
                if a.0 < b.0 && a.1 > b.1 {
                    total += 1;
                }
            }
        }
    }
    total
}

/// Places a layer's nodes, in their order, as near `wanted` as they can be
/// with each a gap from the next: the least squares fit under those
/// constraints, by pooling adjacent violators.
fn fit(row: &[usize], size: &[Pt], wanted: &[f64]) -> Vec<f64> {
    // x_k = y_k + offset_k, the y's in order.
    let mut offset = vec![0.0; row.len()];
    for k in 1..row.len() {
        offset[k] = offset[k - 1] + size[row[k - 1]].x / 2.0 + size[row[k]].x / 2.0 + GAP;
    }
    // Blocks of (sum, count).
    let mut blocks: Vec<(f64, f64, usize)> = Vec::new(); // (mean, weight, members)
    for (k, w) in wanted.iter().enumerate() {
        blocks.push((w - offset[k], 1.0, 1));
        while blocks.len() > 1 && blocks[blocks.len() - 2].0 > blocks[blocks.len() - 1].0 {
            let (m2, w2, c2) = blocks.pop().unwrap();
            let (m1, w1, c1) = blocks.pop().unwrap();
            blocks.push(((m1 * w1 + m2 * w2) / (w1 + w2), w1 + w2, c1 + c2));
        }
    }
    let mut out = Vec::with_capacity(row.len());
    for (mean, _, count) in blocks {
        for _ in 0..count {
            out.push(mean);
        }
    }
    out.iter().zip(&offset).map(|(y, o)| y + o).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn graph(n: usize, lines: &[(usize, usize)]) -> Graph {
        Graph {
            size: vec![pt(80.0, 36.0); n],
            lines: lines.to_vec(),
        }
    }

    #[test]
    fn a_chain_goes_straight_down() {
        let p = lay_out(&graph(4, &[(0, 1), (1, 2), (2, 3)]));
        for k in 0..3 {
            assert!(p.pos[k + 1].y > p.pos[k].y, "{:?}", p.pos);
            assert!((p.pos[k + 1].x - p.pos[k].x).abs() < 1e-9, "{:?}", p.pos);
        }
        assert!(p.upward.iter().all(|u| !u));
    }

    #[test]
    fn a_cycle_has_one_line_upwards() {
        let p = lay_out(&graph(3, &[(0, 1), (1, 2), (2, 0)]));
        assert_eq!(p.upward.iter().filter(|&&u| u).count(), 1);
        for (l, &(a, b)) in [(0, 1), (1, 2), (2, 0)].iter().enumerate() {
            assert_eq!(
                p.pos[b].y < p.pos[a].y,
                p.upward[l],
                "line {l}: {:?}",
                p.pos
            );
        }
    }

    #[test]
    fn a_branch_spreads_across_one_layer() {
        // 0 feeds 1 and 2: they share a layer, either side of 0, apart.
        let p = lay_out(&graph(3, &[(0, 1), (0, 2)]));
        assert_eq!(p.pos[1].y, p.pos[2].y);
        assert!((p.pos[2].x - p.pos[1].x).abs() >= 80.0 + GAP - 1e-9);
        assert!((p.pos[0].x - (p.pos[1].x + p.pos[2].x) / 2.0).abs() < 1e-9);
    }

    #[test]
    fn a_long_line_gets_corner_points() {
        // 0 → 1 → 2 → 3 and 0 → 3: the long line has a corner point in
        // each of the two layers it crosses, listed from 0.
        let p = lay_out(&graph(4, &[(0, 1), (1, 2), (2, 3), (0, 3)]));
        let c = &p.corners[3];
        assert_eq!(c.len(), 2);
        assert_eq!(c[0].y, p.pos[1].y);
        assert_eq!(c[1].y, p.pos[2].y);
        assert!(p.corners[..3].iter().all(Vec::is_empty));
        // Beside the chain, not through it.
        assert!((c[0].x - p.pos[1].x).abs() >= 40.0 + DUMMY_WIDTH / 2.0 - 1e-9);
    }

    #[test]
    fn an_input_sits_just_above_its_use() {
        // 0 → 1 → 2 → 3, and 4 → 3: 4 is one layer above 3, not at the top.
        let p = lay_out(&graph(5, &[(0, 1), (1, 2), (2, 3), (4, 3)]));
        assert_eq!(p.pos[4].y, p.pos[2].y);
    }

    #[test]
    fn crossings_are_counted_and_undone() {
        // Drawn crossed: 0 over 1 on top, 3 over 2 below, lines 0 → 2 and
        // 1 → 3: one crossing.
        let rows = vec![vec![0, 1], vec![3, 2]];
        let below = vec![vec![2], vec![3], vec![], vec![]];
        assert_eq!(crossings(&rows, &below), 1);
        // Laid out, each line keeps its side: no crossing.
        let p = lay_out(&graph(4, &[(0, 2), (1, 3), (0, 3)]));
        let left = |a: usize, b: usize| p.pos[a].x < p.pos[b].x;
        assert_eq!(left(0, 1), left(2, 3), "{:?}", p.pos);
    }

    /// Pairs of straight lines (between consecutive layers) that cross.
    fn crossed(p: &Placed, lines: &[(usize, usize)]) -> usize {
        let mut c = 0;
        for (i, &(a, b)) in lines.iter().enumerate() {
            for &(u, v) in &lines[i + 1..] {
                let same_layers = p.pos[a].y == p.pos[u].y && p.pos[b].y == p.pos[v].y;
                if same_layers && a != u && b != v {
                    let top = p.pos[a].x - p.pos[u].x;
                    let bottom = p.pos[b].x - p.pos[v].x;
                    if top * bottom < 0.0 {
                        c += 1;
                    }
                }
            }
        }
        c
    }

    #[test]
    fn the_order_found_first_is_uncrossed() {
        // Found from the top as 0, 1 over 2, 3: 1 → 2 crosses 0 → 3.
        let lines = [(0, 2), (0, 3), (1, 2)];
        let p = lay_out(&graph(4, &lines));
        assert_eq!(crossed(&p, &lines), 0, "{:?}", p.pos);
    }

    #[test]
    fn a_sweep_and_a_transpose_each_undo_a_crossing() {
        let up = vec![vec![], vec![], vec![1], vec![0]];
        let below = vec![vec![3], vec![2], vec![], vec![]];
        // 0 over 3 and 1 over 2, drawn 2, 3: crossed.
        let mut rows = vec![vec![0, 1], vec![2, 3]];
        assert_eq!(crossings(&rows, &below), 1);
        by_barycentre(&mut rows, 1, 0, &up);
        assert_eq!(rows[1], vec![3, 2]);
        let mut rows = vec![vec![0, 1], vec![2, 3]];
        transpose(&mut rows, &up, &below);
        assert_eq!(crossings(&rows, &below), 0);
    }

    #[test]
    fn a_node_moves_under_its_neighbour() {
        // 0 → 1 and 0 → 2 → 3: 3 sits under 2, not centred.
        let p = lay_out(&graph(4, &[(0, 1), (0, 2), (2, 3)]));
        assert!((p.pos[3].x - p.pos[2].x).abs() < 10.0, "{:?}", p.pos);
        assert!((p.pos[2].x - p.pos[1].x).abs() > 1.0);
    }

    #[test]
    fn an_upward_line_lists_its_corners_from_its_first_node() {
        // 0 → 1 → 2 → 3 → 0: 3 → 0 goes up across two layers' worth, its
        // corner points listed from 3, the lowest first.
        let lines = [(0, 1), (1, 2), (2, 3), (3, 0)];
        let p = lay_out(&graph(4, &lines));
        let up: Vec<usize> = (0..4).filter(|&l| p.upward[l]).collect();
        assert_eq!(up.len(), 1);
        let l = up[0];
        let c = &p.corners[l];
        assert!(c.len() >= 2, "{c:?}");
        let first = p.pos[lines[l].0];
        assert!(
            c[0].distance(first) < c[c.len() - 1].distance(first),
            "{c:?}"
        );
    }

    #[test]
    fn parts_sit_side_by_side() {
        let p = lay_out(&graph(4, &[(0, 1), (2, 3)]));
        let left = p.pos[0].x.max(p.pos[1].x);
        let right = p.pos[2].x.min(p.pos[3].x);
        assert!(
            (right - left).abs() >= 80.0 + PART_GAP - 1e-9,
            "{:?}",
            p.pos
        );
    }

    #[test]
    fn the_fit_keeps_order_and_gaps() {
        let size = vec![pt(80.0, 36.0); 3];
        // All want 0: spread evenly about it.
        let x = fit(&[0, 1, 2], &size, &[0.0, 0.0, 0.0]);
        assert_eq!(x, vec![-120.0, 0.0, 120.0]);
        // Far apart already: left alone.
        let x = fit(&[0, 1, 2], &size, &[0.0, 500.0, 1000.0]);
        assert_eq!(x, vec![0.0, 500.0, 1000.0]);
    }

    #[test]
    fn the_greedy_order_turns_round_one_line_of_a_cycle() {
        let rank = greedy_order(3, &[(0, 0, 1), (1, 1, 2), (2, 2, 0)]);
        let against = [(0, 1), (1, 2), (2, 0)]
            .iter()
            .filter(|&&(a, b)| rank[a] > rank[b])
            .count();
        assert_eq!(against, 1);
    }
}
