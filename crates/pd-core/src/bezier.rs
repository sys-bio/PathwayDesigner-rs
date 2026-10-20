//! Bézier legs (S §5.6, §7.4.4).
//!
//! A leg is stored centre to centre: from the species' centre to the junction
//! for a reactant, from the junction to the species' centre for a product. It
//! is cut at the node border only when drawn, so moving or resizing a node
//! never requires its handles to be recomputed (invariant 5).
//!
//! Handle convention (S §5.6.1), used everywhere:
//!
//! | Leg      | curve            | `ctrl1`              | `ctrl2`              |
//! |----------|------------------|----------------------|----------------------|
//! | reactant | species → junction | outer (species side) | inner (junction side) |
//! | product  | junction → species | inner (junction side) | outer (species side) |

use crate::geometry::{CubicBezier, Pt, normalize, perp};
use crate::model::{Model, Participant, Reaction, Role, SpeciesNode};
use crate::render::constants::{FAN_SPREAD, HANDLE_FRACTION, REACTANT_GAP};

/// Smooth junction, looped species: outer handles move this far sideways, and
/// inner handles sit this far from the junction (S §7.4.4, §17.1).
pub const SMOOTH_LOOP_OUTER: f64 = 30.0;
pub const SMOOTH_LOOP_INNER: f64 = 25.0;

/// One of a participant's two handles.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Ctrl {
    One,
    Two,
}

impl Ctrl {
    /// The junction-side handle of a leg: `ctrl2` on a reactant, `ctrl1` on a
    /// product.
    pub fn inner(role: Role) -> Ctrl {
        if role == Role::Reactant {
            Ctrl::Two
        } else {
            Ctrl::One
        }
    }

    /// The species-side handle of a leg.
    pub fn outer(role: Role) -> Ctrl {
        if role == Role::Reactant {
            Ctrl::One
        } else {
            Ctrl::Two
        }
    }

    pub fn get(self, p: &Participant) -> Pt {
        match self {
            Ctrl::One => p.ctrl1,
            Ctrl::Two => p.ctrl2,
        }
    }

    pub fn set(self, p: &mut Participant, at: Pt) {
        match self {
            Ctrl::One => p.ctrl1 = at,
            Ctrl::Two => p.ctrl2 = at,
        }
    }
}

/// Automatic handles for a leg from `a` to `b` (S §5.6.2). `index` is the
/// leg's position among the reaction's legs of the same role, of which there
/// are `count`. The fan offset is applied at the junction end only, so legs
/// spread apart at the junction but leave each node cleanly.
pub fn auto_handles(a: Pt, b: Pt, role: Role, index: usize, count: usize) -> (Pt, Pt) {
    let l = a.distance(b);
    if l < 1.0 {
        return (a, b);
    }
    let d = (b - a) / l;
    let n = perp(d);
    let offset = if count <= 1 {
        0.0
    } else {
        (index as f64 - (count as f64 - 1.0) / 2.0) * FAN_SPREAD
    };
    let near_a = a + d * (HANDLE_FRACTION * l);
    let near_b = b - d * (HANDLE_FRACTION * l);
    match role {
        Role::Reactant => (near_a, near_b + n * offset),
        _ => (near_a + n * offset, near_b),
    }
}

/// The conceptual (uncut) curve of a reactant or product leg, with its stored
/// handles or, if they aren't set, its automatic ones. `None` for a modifier
/// or a missing species.
pub fn leg_curve(model: &Model, r: &Reaction, role: Role, index: usize) -> Option<CubicBezier> {
    if role == Role::Modifier {
        return None;
    }
    let list = r.participants(role);
    let p = list.get(index)?;
    let s = model.find_species(&p.species)?.center;
    let j = r.leg_anchor(role);
    let (a, b) = if role == Role::Reactant {
        (s, j)
    } else {
        (j, s)
    };
    let (c1, c2) = if p.ctrl_pts_set {
        (p.ctrl1, p.ctrl2)
    } else {
        auto_handles(a, b, role, index, list.len())
    };
    Some(CubicBezier::new(a, c1, c2, b))
}

/// How a leg is drawn: cut at the node border (S §5.6.3).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DrawnLeg {
    pub curve: CubicBezier,
    /// For a product leg: the arrowhead's tip and direction of travel.
    pub arrow: Option<(Pt, Pt)>,
}

/// Cuts a leg's curve at `node`'s border. A reactant leg keeps the part after
/// the crossing, its start moved forward by the reactant gap along the
/// tangent. A product leg keeps the part before it, ending the product-line
/// gap short of the border, with the arrowhead tip at the 3-unit inset,
/// pointing along the curve's tangent there.
pub fn cut_leg(curve: &CubicBezier, node: &SpeciesNode, role: Role) -> DrawnLeg {
    cut_leg_sized(curve, node, role, &crate::render::draw::ArrowSize::STANDARD)
}

/// [`cut_leg`] for an arrowhead of `size` (S §5.5).
pub fn cut_leg_sized(
    curve: &CubicBezier,
    node: &SpeciesNode,
    role: Role,
    size: &crate::render::draw::ArrowSize,
) -> DrawnLeg {
    // Cut where the curve leaves the shape that is drawn (S §5.10).
    let t = crate::geometry::bezier_crossing(curve, |p| crate::node_shape::contains(node, p));
    let (left, right) = curve.split(t);
    if role == Role::Reactant {
        let mut c = right;
        c.p0 += normalize(c.p1 - c.p0) * REACTANT_GAP;
        DrawnLeg {
            curve: c,
            arrow: None,
        }
    } else {
        let d = normalize(left.p3 - left.p2);
        let tip = left.p3 - d * size.tip_inset;
        let mut c = left;
        c.p3 = left.p3 - d * size.line_gap;
        DrawnLeg {
            curve: c,
            arrow: Some((tip, d)),
        }
    }
}

/// One leg's new handles: its role, its index, and `(ctrl1, ctrl2)` or `None`
/// for automatic handles.
pub type LegHandles = (Role, usize, Option<(Pt, Pt)>);

/// The handles a smooth junction gives each reactant and product (S §7.4.4):
/// `Some((ctrl1, ctrl2))`, or `None` for a leg shorter than 1, which goes back
/// to automatic handles. In the order reactants then products.
pub fn smooth_handles(model: &Model, r: &Reaction) -> Vec<LegHandles> {
    let j = r.junction;
    let centre = |list: &[Participant]| {
        let pts: Vec<Pt> = list
            .iter()
            .filter_map(|p| model.find_species(&p.species))
            .map(|s| s.center)
            .collect();
        (!pts.is_empty()).then(|| pts.iter().fold(Pt::ZERO, |a, &p| a + p) / pts.len() as f64)
    };
    let cent_r = centre(&r.reactants).unwrap_or(j);
    let cent_p = centre(&r.products).unwrap_or(cent_r);
    let axis = if cent_p.distance(cent_r) < 0.5 {
        Pt { x: 1.0, y: 0.0 }
    } else {
        normalize(cent_p - cent_r)
    };

    let mut out = Vec::new();
    for (role, list) in [(Role::Reactant, &r.reactants), (Role::Product, &r.products)] {
        for (index, p) in list.iter().enumerate() {
            let Some(s) = model.find_species(&p.species).map(|s| s.center) else {
                continue;
            };
            let l = j.distance(s);
            let handles = (l >= 1.0).then(|| {
                let outer = s + (j - s) * HANDLE_FRACTION;
                if role == Role::Reactant {
                    (outer, j - axis * (HANDLE_FRACTION * l))
                } else {
                    (j + axis * (HANDLE_FRACTION * l), outer)
                }
            });
            out.push((role, index, handles));
        }
    }

    // Looped species (A -> A + B): spread the two arcs to A apart, and set
    // every inner handle on the same sideways line.
    for (ri, rp) in r.reactants.iter().enumerate() {
        let Some(pi) = r.products.iter().position(|p| p.species == rp.species) else {
            continue;
        };
        let Some(a) = model.find_species(&rp.species).map(|s| s.center) else {
            continue;
        };
        let p = perp(normalize(j - a));
        let nr = r.reactants.len();
        if let Some((c1, _)) = &mut out[ri].2 {
            *c1 += p * SMOOTH_LOOP_OUTER;
        }
        if let Some((_, c2)) = &mut out[nr + pi].2 {
            *c2 -= p * SMOOTH_LOOP_OUTER;
        }
        for (role, _, handles) in &mut out {
            if let Some((c1, c2)) = handles {
                match role {
                    Role::Reactant => *c2 = j + p * SMOOTH_LOOP_INNER,
                    _ => *c1 = j - p * SMOOTH_LOOP_INNER,
                }
            }
        }
    }
    out
}

/// Stores the smooth-junction handles of reaction `id` (S §7.4.4).
pub fn apply_smooth_handles(model: &mut Model, id: &str) {
    let Some(r) = model.find_reaction(id) else {
        return;
    };
    let handles = smooth_handles(model, r);
    let r = model.find_reaction_mut(id).expect("found above");
    for (role, index, h) in handles {
        let p = &mut r.participants_mut(role)[index];
        match h {
            Some((c1, c2)) => {
                p.ctrl1 = c1;
                p.ctrl2 = c2;
                p.ctrl_pts_set = true;
            }
            None => p.ctrl_pts_set = false,
        }
    }
}

/// Lines up the inner (junction-side) handles of smooth reaction `id` again
/// after something moved (S §6.4, §7.5), keeping each stored outer handle:
/// the outer handles are the user's shaping, and only the inner ones make
/// the junction smooth. A leg with automatic handles is given both.
pub fn apply_smooth_inner_handles(model: &mut Model, id: &str) {
    let Some(r) = model.find_reaction(id) else {
        return;
    };
    let handles = smooth_handles(model, r);
    let r = model.find_reaction_mut(id).expect("found above");
    for (role, index, h) in handles {
        let p = &mut r.participants_mut(role)[index];
        match h {
            Some((c1, c2)) if p.ctrl_pts_set => match role {
                Role::Reactant => p.ctrl2 = c2,
                _ => p.ctrl1 = c1,
            },
            Some((c1, c2)) => {
                p.ctrl1 = c1;
                p.ctrl2 = c2;
                p.ctrl_pts_set = true;
            }
            None => p.ctrl_pts_set = false,
        }
    }
}

/// Gives participant `index` of `role` stored handles at its automatic
/// positions, if it doesn't have stored ones yet (S §6.4: handles are saved
/// on the first grab, so the other one never jumps).
pub fn store_auto_handles(model: &mut Model, id: &str, role: Role, index: usize) {
    let Some(r) = model.find_reaction(id) else {
        return;
    };
    if r.participants(role)
        .get(index)
        .is_none_or(|p| p.ctrl_pts_set)
    {
        return;
    }
    let Some(curve) = leg_curve(model, r, role, index) else {
        return;
    };
    let p = &mut model
        .find_reaction_mut(id)
        .expect("found above")
        .participants_mut(role)[index];
    p.ctrl1 = curve.p1;
    p.ctrl2 = curve.p2;
    p.ctrl_pts_set = true;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{bezier_rect_crossing, point_in_rect, pt};
    use crate::model::ReactionMode;
    use crate::render::constants::{ARROW_TIP_INSET, PRODUCT_LINE_GAP};

    fn close(a: Pt, b: Pt) -> bool {
        a.distance(b) < 1e-9
    }

    /// Reactants at (0,-100) and (0,100), products at (400,-100) and
    /// (400,100), junction at (200,0).
    fn bibi() -> Model {
        let mut m = Model::new();
        let ids: Vec<_> = [(0.0, -100.0), (0.0, 100.0), (400.0, -100.0), (400.0, 100.0)]
            .iter()
            .map(|&(x, y)| m.add_species(pt(x, y), 80.0, 36.0))
            .collect();
        let r = m.add_reaction(pt(200.0, 0.0));
        let r = m.find_reaction_mut(&r).unwrap();
        r.mode = ReactionMode::Bezier;
        r.reactants = vec![
            Participant::new(&ids[0], 1.0),
            Participant::new(&ids[1], 1.0),
        ];
        r.products = vec![
            Participant::new(&ids[2], 1.0),
            Participant::new(&ids[3], 1.0),
        ];
        m
    }

    #[test]
    fn automatic_handles_without_fan() {
        let (c1, c2) = auto_handles(pt(0.0, 0.0), pt(100.0, 0.0), Role::Reactant, 0, 1);
        assert!(close(c1, pt(35.0, 0.0)) && close(c2, pt(65.0, 0.0)));
        // Too short: the handles sit on the ends.
        assert_eq!(
            auto_handles(pt(0.0, 0.0), pt(0.5, 0.0), Role::Product, 0, 1),
            (pt(0.0, 0.0), pt(0.5, 0.0))
        );
    }

    #[test]
    fn fan_offset_is_at_the_junction_end_only() {
        // Two reactants: offsets −25 and +25 on the inner handle (ctrl2).
        let a = pt(0.0, 0.0);
        let j = pt(100.0, 0.0);
        let (c1, c2) = auto_handles(a, j, Role::Reactant, 0, 2);
        assert!(close(c1, pt(35.0, 0.0)), "outer handle untouched");
        assert!(close(c2, pt(65.0, -25.0)));
        let (_, c2) = auto_handles(a, j, Role::Reactant, 1, 2);
        assert!(close(c2, pt(65.0, 25.0)));
        // Product: junction is `a`, so the offset goes on ctrl1.
        let (c1, c2) = auto_handles(j, pt(200.0, 0.0), Role::Product, 0, 2);
        assert!(close(c1, pt(135.0, -25.0)) && close(c2, pt(165.0, 0.0)));
        // Three legs: −50, 0, +50.
        let (_, c2) = auto_handles(a, j, Role::Reactant, 2, 3);
        assert!(close(c2, pt(65.0, 50.0)));
    }

    #[test]
    fn leg_curve_uses_stored_handles_when_set() {
        let mut m = bibi();
        let r = m.find_reaction("R5").unwrap().clone();
        let c = leg_curve(&m, &r, Role::Reactant, 0).unwrap();
        assert_eq!((c.p0, c.p3), (pt(0.0, -100.0), pt(200.0, 0.0)));
        let p = &mut m.find_reaction_mut("R5").unwrap().reactants[0];
        p.ctrl_pts_set = true;
        p.ctrl1 = pt(1.0, 2.0);
        p.ctrl2 = pt(3.0, 4.0);
        let r = m.find_reaction("R5").unwrap();
        let c = leg_curve(&m, r, Role::Reactant, 0).unwrap();
        assert_eq!((c.p1, c.p2), (pt(1.0, 2.0), pt(3.0, 4.0)));
        // Products run from the junction to the species.
        let c = leg_curve(&m, r, Role::Product, 1).unwrap();
        assert_eq!((c.p0, c.p3), (pt(200.0, 0.0), pt(400.0, 100.0)));
        assert!(leg_curve(&m, r, Role::Modifier, 0).is_none());
    }

    #[test]
    fn cut_reactant_leg_starts_outside_its_node() {
        let m = bibi();
        let r = m.find_reaction("R5").unwrap();
        let node = m.find_species("S1").unwrap();
        let curve = leg_curve(&m, r, Role::Reactant, 0).unwrap();
        let drawn = cut_leg(&curve, node, Role::Reactant);
        assert!(drawn.arrow.is_none());
        assert_eq!(drawn.curve.p3, pt(200.0, 0.0), "ends at the junction");
        assert!(!point_in_rect(drawn.curve.p0, node.center, 40.0, 18.0));
        // About the reactant gap past the border.
        let (left, _) = curve.split(bezier_rect_crossing(&curve, node.center, 40.0, 18.0));
        assert!((drawn.curve.p0.distance(left.p3) - REACTANT_GAP).abs() < 1e-6);
    }

    #[test]
    fn a_bigger_arrowhead_on_a_curve() {
        let mut m = Model::new();
        let a = m.add_species(crate::geometry::pt(0.0, 0.0), 80.0, 36.0);
        let b = m.add_species(crate::geometry::pt(400.0, 100.0), 80.0, 36.0);
        let r = m.add_reaction(crate::geometry::pt(200.0, 50.0));
        let r = m.find_reaction_mut(&r).unwrap();
        r.mode = ReactionMode::Bezier;
        r.reactants.push(Participant::new(a, 1.0));
        r.products.push(Participant::new(b.clone(), 1.0));
        let r = m.reactions()[0].clone();
        let node = m.find_species(&b).unwrap();
        let curve = leg_curve(&m, &r, Role::Product, 0).unwrap();
        let size = crate::render::draw::ArrowSize::for_width(6.0);
        let plain = cut_leg(&curve, node, Role::Product);
        let big = cut_leg_sized(&curve, node, Role::Product, &size);
        let border = |leg: &DrawnLeg, inset: f64| {
            let (tip, d) = leg.arrow.unwrap();
            tip + d * inset
        };
        // Both measure from the same border point, at their own insets.
        let at = border(&plain, ARROW_TIP_INSET);
        assert!(border(&big, size.tip_inset).distance(at) < 1e-6);
        assert!((big.curve.p3.distance(at) - size.line_gap).abs() < 1e-6);
    }

    #[test]
    fn arrowhead_lies_along_the_curve_at_the_border() {
        let m = bibi();
        let r = m.find_reaction("R5").unwrap();
        for index in 0..2 {
            let node = m.find_species(&r.products[index].species).unwrap();
            let curve = leg_curve(&m, r, Role::Product, index).unwrap();
            let drawn = cut_leg(&curve, node, Role::Product);
            let (tip, dir) = drawn.arrow.unwrap();
            let t = bezier_rect_crossing(&curve, node.center, 40.0, 18.0);
            let border = curve.eval(t);
            // The tip is 3 short of the border, along the tangent there.
            assert!((tip.distance(border) - ARROW_TIP_INSET).abs() < 1e-6);
            let (left, _) = curve.split(t);
            assert!(close(dir, normalize(left.p3 - left.p2)));
            assert!(close(normalize(border - tip), dir));
            // The line stops 6 short of the border, under the arrowhead.
            assert!((drawn.curve.p3.distance(border) - PRODUCT_LINE_GAP).abs() < 1e-6);
            assert_eq!(drawn.curve.p0, pt(200.0, 0.0));
        }
    }

    #[test]
    fn bibi_legs_do_not_overlap() {
        // The two reactant legs (and the two product legs) stay apart along
        // their length, so they never draw on top of each other.
        let m = bibi();
        let r = m.find_reaction("R5").unwrap();
        for role in [Role::Reactant, Role::Product] {
            let a = leg_curve(&m, r, role, 0).unwrap();
            let b = leg_curve(&m, r, role, 1).unwrap();
            for i in 1..10 {
                let t = f64::from(i) / 10.0;
                assert!(a.eval(t).distance(b.eval(t)) > 5.0, "{role:?} t = {t}");
            }
        }
    }

    #[test]
    fn smooth_handles_line_up_through_the_junction() {
        let m = bibi();
        let r = m.find_reaction("R5").unwrap();
        let h = smooth_handles(&m, r);
        assert_eq!(h.len(), 4);
        // Axis from the reactant centre (0,0) to the product centre (400,0).
        let (_, _, Some((c1, c2))) = h[0] else {
            panic!()
        };
        // Leg length |J − S| = |(200,100)| ≈ 223.6; inner = J − axis·0.35·L.
        let l = pt(200.0, 100.0).length();
        assert!(close(c2, pt(200.0 - 0.35 * l, 0.0)));
        assert!(close(c1, pt(0.0, -100.0) + pt(200.0, 100.0) * 0.35));
        let (_, _, Some((c1, c2))) = h[2] else {
            panic!()
        };
        assert!(
            close(c1, pt(200.0 + 0.35 * l, 0.0)),
            "product inner on the axis"
        );
        assert!(close(c2, pt(400.0, -100.0) + pt(-200.0, 100.0) * 0.35));
    }

    #[test]
    fn smooth_handles_for_a_looped_species() {
        // A → A + B: A at (0,0), B at (400,0), junction at (200,100).
        let mut m = Model::new();
        let a = m.add_species(pt(0.0, 0.0), 80.0, 36.0);
        let b = m.add_species(pt(400.0, 0.0), 80.0, 36.0);
        let r = m.add_reaction(pt(200.0, 100.0));
        let rr = m.find_reaction_mut(&r).unwrap();
        rr.mode = ReactionMode::Bezier;
        rr.junction_smooth = true;
        rr.reactants = vec![Participant::new(&a, 1.0)];
        rr.products = vec![Participant::new(&a, 1.0), Participant::new(&b, 1.0)];
        apply_smooth_handles(&mut m, &r);
        let rr = m.find_reaction(&r).unwrap();
        let j = pt(200.0, 100.0);
        let p = perp(normalize(j - pt(0.0, 0.0)));
        // Every inner handle on the sideways line through the junction.
        assert!(close(rr.reactants[0].ctrl2, j + p * 25.0));
        assert!(close(rr.products[0].ctrl1, j - p * 25.0));
        assert!(close(rr.products[1].ctrl1, j - p * 25.0));
        // The two arcs to A leave it on different sides.
        let out_r = rr.reactants[0].ctrl1;
        let out_p = rr.products[0].ctrl2;
        assert!(out_r.distance(out_p) > 59.0);
        // So the two arcs are apart along their length.
        let arc_r = leg_curve(&m, rr, Role::Reactant, 0).unwrap();
        let arc_p = leg_curve(&m, rr, Role::Product, 0).unwrap();
        for i in 1..10 {
            let t = f64::from(i) / 10.0;
            // The product arc runs the other way: compare the matching points.
            assert!(arc_r.eval(t).distance(arc_p.eval(1.0 - t)) > 5.0, "t = {t}");
        }
        assert!(
            rr.reactants
                .iter()
                .chain(&rr.products)
                .all(|p| p.ctrl_pts_set)
        );
    }

    #[test]
    fn smooth_handles_reset_a_leg_too_short_to_curve() {
        let mut m = bibi();
        m.find_reaction_mut("R5").unwrap().junction = pt(0.0, -100.0);
        let r = m.find_reaction("R5").unwrap();
        let h = smooth_handles(&m, r);
        assert_eq!(h[0].2, None, "the junction is on S1");
        assert!(h[1].2.is_some());
    }

    #[test]
    fn storing_automatic_handles_keeps_the_curve() {
        let mut m = bibi();
        let before = leg_curve(&m, m.find_reaction("R5").unwrap(), Role::Product, 1).unwrap();
        store_auto_handles(&mut m, "R5", Role::Product, 1);
        let r = m.find_reaction("R5").unwrap();
        assert!(r.products[1].ctrl_pts_set);
        assert_eq!(leg_curve(&m, r, Role::Product, 1).unwrap(), before);
        // Already stored: left alone.
        m.find_reaction_mut("R5").unwrap().products[1].ctrl1 = pt(9.0, 9.0);
        store_auto_handles(&mut m, "R5", Role::Product, 1);
        assert_eq!(
            m.find_reaction("R5").unwrap().products[1].ctrl1,
            pt(9.0, 9.0)
        );
    }

    #[test]
    fn inner_and_outer() {
        assert_eq!(Ctrl::inner(Role::Reactant), Ctrl::Two);
        assert_eq!(Ctrl::inner(Role::Product), Ctrl::One);
        assert_eq!(Ctrl::outer(Role::Reactant), Ctrl::One);
        assert_eq!(Ctrl::outer(Role::Product), Ctrl::Two);
    }
}
