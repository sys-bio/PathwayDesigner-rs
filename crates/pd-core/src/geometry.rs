//! Pure 2-D geometry (S §17.3).
//!
//! Nothing here knows about the model, the screen or egui. Every function works
//! the same on world or screen coordinates (S §5.1).

use std::ops::{Add, AddAssign, Div, Mul, Neg, Sub, SubAssign};

/// A point or vector, in `f64` like everything in the model (SPEC-Rust §3).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Pt {
    pub x: f64,
    pub y: f64,
}

/// Shorthand for `Pt { x, y }`.
pub const fn pt(x: f64, y: f64) -> Pt {
    Pt { x, y }
}

impl Pt {
    pub const ZERO: Pt = pt(0.0, 0.0);

    pub fn dot(self, other: Pt) -> f64 {
        self.x * other.x + self.y * other.y
    }

    pub fn length(self) -> f64 {
        self.x.hypot(self.y)
    }

    pub fn distance(self, other: Pt) -> f64 {
        (self - other).length()
    }
}

impl Add for Pt {
    type Output = Pt;
    fn add(self, o: Pt) -> Pt {
        pt(self.x + o.x, self.y + o.y)
    }
}

impl Sub for Pt {
    type Output = Pt;
    fn sub(self, o: Pt) -> Pt {
        pt(self.x - o.x, self.y - o.y)
    }
}

impl Mul<f64> for Pt {
    type Output = Pt;
    fn mul(self, k: f64) -> Pt {
        pt(self.x * k, self.y * k)
    }
}

impl Div<f64> for Pt {
    type Output = Pt;
    fn div(self, k: f64) -> Pt {
        pt(self.x / k, self.y / k)
    }
}

impl Neg for Pt {
    type Output = Pt;
    fn neg(self) -> Pt {
        pt(-self.x, -self.y)
    }
}

impl AddAssign for Pt {
    fn add_assign(&mut self, o: Pt) {
        *self = *self + o;
    }
}

impl SubAssign for Pt {
    fn sub_assign(&mut self, o: Pt) {
        *self = *self - o;
    }
}

/// Vectors shorter than this have no direction: [`normalize`] returns zero.
pub const NORMALIZE_EPSILON: f64 = 1e-7;

/// The unit vector along `v`, or (0, 0) if `|v| < 1e-7`.
pub fn normalize(v: Pt) -> Pt {
    let len = v.length();
    if len < NORMALIZE_EPSILON {
        Pt::ZERO
    } else {
        v / len
    }
}

/// `v` turned a quarter turn: `(-v.y, v.x)`. Not normalised.
pub fn perp(v: Pt) -> Pt {
    pt(-v.y, v.x)
}

/// Distance from `p` to the closest point on the segment `a`–`b`, with the
/// projection clamped to the segment. When `a == b` this is the distance to `a`.
pub fn point_to_segment_dist(p: Pt, a: Pt, b: Pt) -> f64 {
    let ab = b - a;
    let len2 = ab.dot(ab);
    if len2 == 0.0 {
        return p.distance(a);
    }
    let t = ((p - a).dot(ab) / len2).clamp(0.0, 1.0);
    p.distance(a + ab * t)
}

/// True if `p` is inside or on the border of the axis-aligned rectangle with
/// centre `c` and half-sizes `hw`, `hh`.
pub fn point_in_rect(p: Pt, c: Pt, hw: f64, hh: f64) -> bool {
    (p.x - c.x).abs() <= hw && (p.y - c.y).abs() <= hh
}

/// The point where the ray from the rectangle's centre `c` towards `ext` leaves
/// the axis-aligned rectangle with half-sizes `hw`, `hh` (S §5.5). Rounded
/// corners are ignored. Returns `c` if `ext` is at `c`.
pub fn rect_boundary_intersect(c: Pt, hw: f64, hh: f64, ext: Pt) -> Pt {
    let u = normalize(ext - c);
    if u == Pt::ZERO {
        return c;
    }
    // Distance along u to each pair of sides; a zero component never reaches them.
    let tx = if u.x != 0.0 {
        hw / u.x.abs()
    } else {
        f64::INFINITY
    };
    let ty = if u.y != 0.0 {
        hh / u.y.abs()
    } else {
        f64::INFINITY
    };
    c + u * tx.min(ty)
}

/// The filled arrowhead triangle with its point at `tip`, pointing along `dir`:
/// `{tip, tip − d·len + perp(d)·half_base, tip − d·len − perp(d)·half_base}`
/// with `d = normalize(dir)`. The app uses `len = 12`, `half_base = 5`. If `dir`
/// has no direction, all three corners are `tip`.
pub fn filled_arrowhead(tip: Pt, dir: Pt, len: f64, half_base: f64) -> [Pt; 3] {
    let d = normalize(dir);
    let base = tip - d * len;
    let side = perp(d) * half_base;
    [tip, base + side, base - side]
}

/// The end of a straight product line: the border point of the node (centre
/// `c`, half-sizes `hw`, `hh`) facing `junction`, moved `gap` back towards the
/// junction.
pub fn product_line_tip(c: Pt, hw: f64, hh: f64, junction: Pt, gap: f64) -> Pt {
    let border = rect_boundary_intersect(c, hw, hh, junction);
    border + normalize(junction - c) * gap
}

/// `screen = world × zoom + offset` (S §5.1). A zoom of 0 is treated as 1.
pub fn world_to_screen(w: Pt, zoom: f64, offset: Pt) -> Pt {
    w * effective_zoom(zoom) + offset
}

/// `world = (screen − offset) / zoom` (S §5.1). A zoom of 0 is treated as 1.
pub fn screen_to_world(s: Pt, zoom: f64, offset: Pt) -> Pt {
    (s - offset) / effective_zoom(zoom)
}

fn effective_zoom(zoom: f64) -> f64 {
    if zoom == 0.0 { 1.0 } else { zoom }
}

/// A cubic Bézier curve with end points `p0`, `p3` and control points `p1`, `p2`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CubicBezier {
    pub p0: Pt,
    pub p1: Pt,
    pub p2: Pt,
    pub p3: Pt,
}

impl CubicBezier {
    pub fn new(p0: Pt, p1: Pt, p2: Pt, p3: Pt) -> Self {
        Self { p0, p1, p2, p3 }
    }

    /// The point at parameter `t` (0 at `p0`, 1 at `p3`).
    pub fn eval(&self, t: f64) -> Pt {
        let u = 1.0 - t;
        self.p0 * (u * u * u)
            + self.p1 * (3.0 * u * u * t)
            + self.p2 * (3.0 * u * t * t)
            + self.p3 * (t * t * t)
    }

    /// Splits the curve at `t` with de Casteljau's construction. The halves
    /// cover `[0, t]` and `[t, 1]` and meet at `eval(t)`.
    pub fn split(&self, t: f64) -> (CubicBezier, CubicBezier) {
        let lerp = |a: Pt, b: Pt| a + (b - a) * t;
        let p01 = lerp(self.p0, self.p1);
        let p12 = lerp(self.p1, self.p2);
        let p23 = lerp(self.p2, self.p3);
        let p012 = lerp(p01, p12);
        let p123 = lerp(p12, p23);
        let mid = lerp(p012, p123);
        (
            CubicBezier::new(self.p0, p01, p012, mid),
            CubicBezier::new(mid, p123, p23, self.p3),
        )
    }
}

/// Bisection steps used by [`bezier_rect_crossing`] (S §17.1).
pub const BORDER_CROSSING_STEPS: u32 = 24;

/// The parameter `t` where `curve` crosses the border of the axis-aligned
/// rectangle with centre `c` and half-sizes `hw`, `hh` (S §5.6.3).
///
/// One end of the curve should be inside the rectangle and the other outside.
/// Bisection keeps the half of `[0, 1]` whose ends lie on opposite sides of the
/// border, for 24 steps. The result is clamped to `[0.001, 0.999]`, so splitting
/// there never leaves an empty piece. If both ends are on the same side the
/// answer is meaningless, but still in that range.
pub fn bezier_rect_crossing(curve: &CubicBezier, c: Pt, hw: f64, hh: f64) -> f64 {
    bezier_crossing(curve, |p| point_in_rect(p, c, hw, hh))
}

/// As [`bezier_rect_crossing`], for any region `inside` says a point is in.
pub fn bezier_crossing(curve: &CubicBezier, inside_at: impl Fn(Pt) -> bool) -> f64 {
    let inside = |t: f64| inside_at(curve.eval(t));
    let start_inside = inside(0.0);
    let (mut lo, mut hi) = (0.0, 1.0);
    for _ in 0..BORDER_CROSSING_STEPS {
        let mid = 0.5 * (lo + hi);
        if inside(mid) == start_inside {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    (0.5 * (lo + hi)).clamp(0.001, 0.999)
}

#[cfg(test)]
mod tests {
    use super::*;

    const EPS: f64 = 1e-9;

    fn close(a: Pt, b: Pt) -> bool {
        a.distance(b) < EPS
    }

    #[test]
    fn vector_operations() {
        let a = pt(1.0, 2.0);
        let b = pt(3.0, -4.0);
        assert_eq!(a + b, pt(4.0, -2.0));
        assert_eq!(a - b, pt(-2.0, 6.0));
        assert_eq!(a * 2.0, pt(2.0, 4.0));
        assert_eq!(b / 2.0, pt(1.5, -2.0));
        assert_eq!(-a, pt(-1.0, -2.0));
        assert_eq!(a.dot(b), -5.0);
        assert_eq!(b.length(), 5.0);
        assert_eq!(a.distance(a), 0.0);
        let mut c = a;
        c += b;
        c -= a;
        assert_eq!(c, b);
    }

    #[test]
    fn normalize_gives_a_unit_vector() {
        assert!(close(normalize(pt(3.0, 4.0)), pt(0.6, 0.8)));
        assert!((normalize(pt(-1e-3, 2e-3)).length() - 1.0).abs() < EPS);
    }

    #[test]
    fn normalize_of_a_zero_length_vector_is_zero() {
        assert_eq!(normalize(Pt::ZERO), Pt::ZERO);
        assert_eq!(normalize(pt(5e-8, 5e-8)), Pt::ZERO);
        // Just above the threshold still has a direction.
        assert!((normalize(pt(2e-7, 0.0)).length() - 1.0).abs() < EPS);
    }

    #[test]
    fn perp_turns_a_quarter_turn_without_normalising() {
        assert_eq!(perp(pt(2.0, 1.0)), pt(-1.0, 2.0));
        assert_eq!(perp(pt(2.0, 1.0)).dot(pt(2.0, 1.0)), 0.0);
        assert_eq!(perp(Pt::ZERO), pt(-0.0, 0.0));
    }

    #[test]
    fn point_to_segment_distance() {
        let a = pt(0.0, 0.0);
        let b = pt(10.0, 0.0);
        // Projection inside the segment.
        assert_eq!(point_to_segment_dist(pt(5.0, 3.0), a, b), 3.0);
        // Projection clamped to each end.
        assert_eq!(point_to_segment_dist(pt(-3.0, 4.0), a, b), 5.0);
        assert_eq!(point_to_segment_dist(pt(13.0, -4.0), a, b), 5.0);
        // On the segment.
        assert_eq!(point_to_segment_dist(pt(7.0, 0.0), a, b), 0.0);
    }

    #[test]
    fn point_to_a_zero_length_segment_is_the_distance_to_its_point() {
        let a = pt(2.0, 2.0);
        assert_eq!(point_to_segment_dist(pt(5.0, 6.0), a, a), 5.0);
        assert_eq!(point_to_segment_dist(a, a, a), 0.0);
    }

    #[test]
    fn point_in_rect_includes_the_border() {
        let c = pt(10.0, 10.0);
        assert!(point_in_rect(c, c, 4.0, 2.0));
        assert!(point_in_rect(pt(14.0, 12.0), c, 4.0, 2.0));
        assert!(!point_in_rect(pt(14.1, 10.0), c, 4.0, 2.0));
        assert!(!point_in_rect(pt(10.0, 7.9), c, 4.0, 2.0));
    }

    #[test]
    fn rect_boundary_intersect_on_each_side() {
        let c = pt(100.0, 50.0);
        let (hw, hh) = (40.0, 18.0);
        assert!(close(
            rect_boundary_intersect(c, hw, hh, pt(300.0, 50.0)),
            pt(140.0, 50.0)
        ));
        assert!(close(
            rect_boundary_intersect(c, hw, hh, pt(-300.0, 50.0)),
            pt(60.0, 50.0)
        ));
        assert!(close(
            rect_boundary_intersect(c, hw, hh, pt(100.0, 500.0)),
            pt(100.0, 68.0)
        ));
        assert!(close(
            rect_boundary_intersect(c, hw, hh, pt(100.0, -500.0)),
            pt(100.0, 32.0)
        ));
    }

    #[test]
    fn rect_boundary_intersect_diagonal_hits_the_nearer_side() {
        let c = Pt::ZERO;
        // Steep ray: leaves through the top or bottom.
        assert!(close(
            rect_boundary_intersect(c, 40.0, 18.0, pt(10.0, 100.0)),
            pt(1.8, 18.0)
        ));
        // Shallow ray: leaves through the left or right.
        assert!(close(
            rect_boundary_intersect(c, 40.0, 18.0, pt(-100.0, 10.0)),
            pt(-40.0, 4.0)
        ));
        // Exactly through a corner.
        assert!(close(
            rect_boundary_intersect(c, 10.0, 10.0, pt(50.0, 50.0)),
            pt(10.0, 10.0)
        ));
    }

    #[test]
    fn rect_boundary_intersect_with_the_point_at_the_centre_returns_the_centre() {
        let c = pt(3.0, 4.0);
        assert_eq!(rect_boundary_intersect(c, 40.0, 18.0, c), c);
    }

    #[test]
    fn rect_boundary_intersect_of_an_empty_rectangle_is_its_centre() {
        let c = pt(3.0, 4.0);
        assert!(close(
            rect_boundary_intersect(c, 0.0, 0.0, pt(50.0, 9.0)),
            c
        ));
    }

    #[test]
    fn arrowhead_triangle() {
        let [tip, b1, b2] = filled_arrowhead(pt(100.0, 0.0), pt(5.0, 0.0), 12.0, 5.0);
        assert_eq!(tip, pt(100.0, 0.0));
        assert!(close(b1, pt(88.0, 5.0)));
        assert!(close(b2, pt(88.0, -5.0)));
        // The direction is normalised, so its length doesn't matter.
        let [_, c1, _] = filled_arrowhead(pt(0.0, 0.0), pt(0.0, -0.001), 12.0, 5.0);
        assert!(close(c1, pt(5.0, 12.0)));
    }

    #[test]
    fn arrowhead_with_no_direction_collapses_to_the_tip() {
        let tip = pt(1.0, 1.0);
        assert_eq!(filled_arrowhead(tip, Pt::ZERO, 12.0, 5.0), [tip, tip, tip]);
    }

    #[test]
    fn product_line_tip_stops_short_of_the_border() {
        // Node centred at the origin, junction to the left: border at x = −40,
        // pulled back 6 towards the junction.
        let tip = product_line_tip(Pt::ZERO, 40.0, 18.0, pt(-200.0, 0.0), 6.0);
        assert!(close(tip, pt(-46.0, 0.0)));
    }

    #[test]
    fn product_line_tip_with_the_junction_at_the_centre() {
        let c = pt(5.0, 5.0);
        assert_eq!(product_line_tip(c, 40.0, 18.0, c, 6.0), c);
    }

    #[test]
    fn world_screen_round_trip() {
        let offset = pt(30.0, -20.0);
        let w = pt(12.5, 7.25);
        let s = world_to_screen(w, 2.0, offset);
        assert_eq!(s, pt(55.0, -5.5));
        assert!(close(screen_to_world(s, 2.0, offset), w));
    }

    #[test]
    fn zero_zoom_is_treated_as_one() {
        let offset = pt(1.0, 2.0);
        assert_eq!(world_to_screen(pt(3.0, 4.0), 0.0, offset), pt(4.0, 6.0));
        assert_eq!(screen_to_world(pt(4.0, 6.0), 0.0, offset), pt(3.0, 4.0));
    }

    fn sample_curve() -> CubicBezier {
        CubicBezier::new(
            pt(0.0, 0.0),
            pt(0.0, 100.0),
            pt(100.0, 100.0),
            pt(100.0, 0.0),
        )
    }

    #[test]
    fn bezier_eval_at_the_ends_and_middle() {
        let b = sample_curve();
        assert_eq!(b.eval(0.0), b.p0);
        assert_eq!(b.eval(1.0), b.p3);
        assert!(close(b.eval(0.5), pt(50.0, 75.0)));
    }

    #[test]
    fn de_casteljau_split_matches_the_original_curve() {
        let b = sample_curve();
        let t = 0.3;
        let (left, right) = b.split(t);
        assert_eq!(left.p0, b.p0);
        assert_eq!(right.p3, b.p3);
        assert!(close(left.p3, b.eval(t)));
        assert!(close(right.p0, b.eval(t)));
        for i in 0..=10 {
            let s = f64::from(i) / 10.0;
            assert!(close(left.eval(s), b.eval(s * t)));
            assert!(close(right.eval(s), b.eval(t + s * (1.0 - t))));
        }
    }

    #[test]
    fn split_at_the_ends_leaves_a_point() {
        let b = sample_curve();
        let (left, right) = b.split(0.0);
        assert_eq!(left, CubicBezier::new(b.p0, b.p0, b.p0, b.p0));
        assert_eq!(right, b);
    }

    #[test]
    fn crossing_found_leaving_a_node() {
        // A straight "curve" from the node's centre out to the right: the border
        // at x = 40 is at t = 0.4.
        let b = CubicBezier::new(
            Pt::ZERO,
            pt(100.0 / 3.0, 0.0),
            pt(200.0 / 3.0, 0.0),
            pt(100.0, 0.0),
        );
        let t = bezier_rect_crossing(&b, Pt::ZERO, 40.0, 18.0);
        assert!((t - 0.4).abs() < 1e-6, "t = {t}");
    }

    #[test]
    fn crossing_found_entering_a_node() {
        // Product leg: from outside into a node at the end of the curve.
        let c = pt(100.0, 0.0);
        let b = CubicBezier::new(Pt::ZERO, pt(30.0, 60.0), pt(70.0, 60.0), c);
        let t = bezier_rect_crossing(&b, c, 40.0, 18.0);
        let p = b.eval(t);
        let on_border =
            ((p.x - c.x).abs() - 40.0).abs() < 1e-3 || ((p.y - c.y).abs() - 18.0).abs() < 1e-3;
        assert!(on_border, "t = {t}, p = {p:?}");
        assert!(point_in_rect(b.eval(t + 0.01), c, 40.0, 18.0));
        assert!(!point_in_rect(b.eval(t - 0.01), c, 40.0, 18.0));
    }

    #[test]
    fn crossing_is_clamped_away_from_the_ends() {
        // Both ends inside: no crossing, so the search runs to the end and is clamped.
        let b = CubicBezier::new(Pt::ZERO, pt(1.0, 0.0), pt(2.0, 0.0), pt(3.0, 0.0));
        assert_eq!(bezier_rect_crossing(&b, Pt::ZERO, 40.0, 18.0), 0.999);
        // A degenerate curve (all four points at one spot) behaves the same.
        let p = pt(5.0, 5.0);
        let d = CubicBezier::new(p, p, p, p);
        assert_eq!(bezier_rect_crossing(&d, Pt::ZERO, 40.0, 18.0), 0.999);
        // Crossing right at the start is clamped up to 0.001.
        let edge = CubicBezier::new(pt(40.0, 0.0), pt(70.0, 0.0), pt(80.0, 0.0), pt(90.0, 0.0));
        assert_eq!(bezier_rect_crossing(&edge, Pt::ZERO, 40.0, 18.0), 0.001);
    }
}
