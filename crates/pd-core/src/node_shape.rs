//! The shapes a species can be drawn as (S §5.10): the outline, what is
//! inside, and where a line from the centre crosses the border, so that
//! reaction lines end on the shape that is drawn.

use crate::geometry::{Pt, pt, rect_boundary_intersect};
use crate::model::{NodeShape, SpeciesNode};

/// How many straight pieces draw an ellipse.
const ELLIPSE_SEGMENTS: usize = 64;

/// The outline of a shape that isn't a rectangle, in world coordinates,
/// clockwise in screen terms from the left (y down), not closed. Rectangles
/// are drawn as rectangles and have none.
pub fn outline(node: &SpeciesNode) -> Option<Vec<Pt>> {
    let (c, a, b) = (node.center, node.width / 2.0, node.height / 2.0);
    match node.shape {
        NodeShape::RoundedRect | NodeShape::Rectangle => None,
        NodeShape::Ellipse | NodeShape::Circle => Some(
            (0..ELLIPSE_SEGMENTS)
                .map(|i| {
                    let t = std::f64::consts::TAU * i as f64 / ELLIPSE_SEGMENTS as f64;
                    pt(c.x - a * t.cos(), c.y - b * t.sin())
                })
                .collect(),
        ),
        NodeShape::Hexagon => Some(hexagon(c, a, b).to_vec()),
    }
}

/// A hexagon with points at the left and right and flat top and bottom,
/// filling the box: from the left point, clockwise in screen terms.
fn hexagon(c: Pt, a: f64, b: f64) -> [Pt; 6] {
    let inset = a / 2.0;
    [
        pt(c.x - a, c.y),
        pt(c.x - a + inset, c.y - b),
        pt(c.x + a - inset, c.y - b),
        pt(c.x + a, c.y),
        pt(c.x + a - inset, c.y + b),
        pt(c.x - a + inset, c.y + b),
    ]
}

/// Whether `p` is inside the shape (its border included).
pub fn contains(node: &SpeciesNode, p: Pt) -> bool {
    let (c, a, b) = (node.center, node.width / 2.0, node.height / 2.0);
    match node.shape {
        NodeShape::RoundedRect | NodeShape::Rectangle => {
            (p.x - c.x).abs() <= a && (p.y - c.y).abs() <= b
        }
        NodeShape::Ellipse | NodeShape::Circle => {
            if a <= 0.0 || b <= 0.0 {
                return false;
            }
            let (dx, dy) = ((p.x - c.x) / a, (p.y - c.y) / b);
            dx * dx + dy * dy <= 1.0
        }
        NodeShape::Hexagon => {
            let pts = hexagon(c, a, b);
            // Convex: inside if on the same side of every edge.
            (0..6).all(|i| {
                let (u, v) = (pts[i], pts[(i + 1) % 6]);
                (v.x - u.x) * (p.y - u.y) - (v.y - u.y) * (p.x - u.x) >= -1e-9
            })
        }
    }
}

/// The point where a line from the centre towards `toward` crosses the
/// border (a rounded rectangle's corners are taken as square, S §5.5).
pub fn border(node: &SpeciesNode, toward: Pt) -> Pt {
    let (c, a, b) = (node.center, node.width / 2.0, node.height / 2.0);
    let d = toward - c;
    if d.length() < 1e-12 {
        return c;
    }
    match node.shape {
        NodeShape::RoundedRect | NodeShape::Rectangle => rect_boundary_intersect(c, a, b, toward),
        NodeShape::Ellipse | NodeShape::Circle => {
            if a <= 0.0 || b <= 0.0 {
                return c;
            }
            let k = 1.0 / ((d.x / a).powi(2) + (d.y / b).powi(2)).sqrt();
            c + d * k
        }
        NodeShape::Hexagon => {
            // The nearest crossing of the ray from the centre with an edge.
            let pts = hexagon(c, a, b);
            let mut best = f64::INFINITY;
            for i in 0..6 {
                let (u, v) = (pts[i], pts[(i + 1) % 6]);
                let e = v - u;
                let denom = d.x * e.y - d.y * e.x;
                if denom.abs() < 1e-12 {
                    continue;
                }
                let w = u - c;
                let t = (w.x * e.y - w.y * e.x) / denom;
                let s = (w.x * d.y - w.y * d.x) / denom;
                if t > 0.0 && (-1e-9..=1.0 + 1e-9).contains(&s) {
                    best = best.min(t);
                }
            }
            if best.is_finite() { c + d * best } else { c }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(shape: NodeShape, w: f64, h: f64) -> SpeciesNode {
        let mut n = SpeciesNode::new("S", pt(10.0, 20.0));
        n.width = w;
        n.height = h;
        n.shape = shape;
        n
    }

    fn close(a: Pt, b: Pt) -> bool {
        a.distance(b) < 1e-9
    }

    #[test]
    fn rectangles_keep_the_box_border() {
        for shape in [NodeShape::RoundedRect, NodeShape::Rectangle] {
            let n = node(shape, 80.0, 36.0);
            assert!(close(border(&n, pt(110.0, 20.0)), pt(50.0, 20.0)));
            assert!(contains(&n, pt(49.0, 37.0)));
            assert!(!contains(&n, pt(51.0, 20.0)));
            assert!(outline(&n).is_none());
        }
    }

    #[test]
    fn an_ellipse_is_crossed_on_its_curve() {
        let n = node(NodeShape::Ellipse, 80.0, 40.0);
        // Along the axes, at the ends of the axes.
        assert!(close(border(&n, pt(110.0, 20.0)), pt(50.0, 20.0)));
        assert!(close(border(&n, pt(10.0, -80.0)), pt(10.0, 0.0)));
        // On the diagonal, a point on the curve, inside the box's corner.
        let p = border(&n, pt(110.0, 120.0));
        let (dx, dy) = ((p.x - 10.0) / 40.0, (p.y - 20.0) / 20.0);
        assert!((dx * dx + dy * dy - 1.0).abs() < 1e-9);
        assert!(p.x < 50.0 && p.y < 40.0);
        // A box corner is outside it.
        assert!(!contains(&n, pt(49.0, 39.0)));
        assert!(contains(&n, pt(10.0, 39.0)));
        assert_eq!(outline(&n).unwrap().len(), ELLIPSE_SEGMENTS);
    }

    #[test]
    fn a_hexagon_has_points_left_and_right() {
        let n = node(NodeShape::Hexagon, 80.0, 40.0);
        assert!(close(border(&n, pt(110.0, 20.0)), pt(50.0, 20.0)));
        assert!(close(border(&n, pt(10.0, -80.0)), pt(10.0, 0.0)));
        // The box's corners are cut off.
        assert!(!contains(&n, pt(48.0, 1.0)));
        assert!(contains(&n, pt(48.0, 20.0)));
        // A ray to the upper right lands on the slanted edge, inside the box.
        let p = border(&n, pt(110.0, -20.0));
        assert!(p.x < 50.0 && p.y > 0.0);
        assert!(contains(&n, p));
        assert_eq!(outline(&n).unwrap().len(), 6);
    }

    #[test]
    fn the_centre_itself_is_its_own_border() {
        for shape in NodeShape::ALL {
            let n = node(shape, 80.0, 40.0);
            assert!(close(border(&n, n.center), n.center));
        }
    }
}
