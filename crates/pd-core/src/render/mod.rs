//! Drawing (S §5, SPEC-Rust §5).
//!
//! The drawing passes in [`draw`] produce the whole diagram through the small
//! [`Renderer`] trait, so the screen (egui, in `pd-app`), SVG, PNG and PDF all
//! come from the same code. Renderers receive **screen** coordinates in `f32`:
//! the passes do their arithmetic in the model's `f64` and convert at the call
//! (SPEC-Rust §5.1).

pub mod constants;
pub mod draw;
pub mod svg;

use crate::model::Rgba;

/// A point in screen space (egui points, or SVG user units).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SPt {
    pub x: f32,
    pub y: f32,
}

pub const fn spt(x: f32, y: f32) -> SPt {
    SPt { x, y }
}

/// An axis-aligned rectangle in screen space.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SRect {
    pub min: SPt,
    pub max: SPt,
}

impl SRect {
    pub fn width(&self) -> f32 {
        self.max.x - self.min.x
    }

    pub fn height(&self) -> f32 {
        self.max.y - self.min.y
    }
}

/// How a line is drawn. Width and dash lengths are in screen units.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stroke {
    pub width: f32,
    pub color: Rgba,
    /// (on, off) lengths, or `None` for a solid line.
    pub dash: Option<(f32, f32)>,
    /// Round line ends; square (butt) ends otherwise.
    pub round_cap: bool,
}

impl Stroke {
    pub fn solid(width: f32, color: Rgba) -> Self {
        Self {
            width,
            color,
            dash: None,
            round_cap: false,
        }
    }
}

/// Everything the drawing passes need from a canvas. Kept this small on
/// purpose: every method is one more thing each renderer must get right.
pub trait Renderer {
    /// A rectangle with rounded corners (`radius` 0 for square corners).
    fn round_rect(&mut self, r: SRect, radius: f32, fill: Option<Rgba>, stroke: Option<Stroke>);
    fn circle(&mut self, c: SPt, radius: f32, fill: Option<Rgba>, stroke: Option<Stroke>);
    /// An open line through `pts`.
    fn polyline(&mut self, pts: &[SPt], stroke: Stroke);
    fn cubic(&mut self, p0: SPt, c1: SPt, c2: SPt, p3: SPt, stroke: Stroke);
    /// A filled convex polygon.
    fn polygon(&mut self, pts: &[SPt], fill: Rgba);
    /// One line of text in the bundled font, centred horizontally on
    /// `center.x` and vertically on its cap height at `center.y` (S §5.3).
    fn text(&mut self, center: SPt, s: &str, size: f32, color: Rgba);
}

/// The outline of a rounded rectangle as a closed polyline (the last point
/// repeats the first), with `segments` straight pieces per corner. Used to dash
/// rounded outlines where the canvas can't (SPEC-Rust §5.2).
pub fn rounded_rect_outline(r: SRect, radius: f32, segments: usize) -> Vec<SPt> {
    let radius = radius.max(0.0).min(r.width() / 2.0).min(r.height() / 2.0);
    let segments = segments.max(1);
    // Corner centres, clockwise from top-left in screen space (y down), with the
    // angle at which each corner's arc starts.
    let corners = [
        (
            spt(r.min.x + radius, r.min.y + radius),
            std::f32::consts::PI,
        ),
        (
            spt(r.max.x - radius, r.min.y + radius),
            1.5 * std::f32::consts::PI,
        ),
        (spt(r.max.x - radius, r.max.y - radius), 0.0),
        (
            spt(r.min.x + radius, r.max.y - radius),
            0.5 * std::f32::consts::PI,
        ),
    ];
    let mut pts = Vec::with_capacity(4 * (segments + 1) + 1);
    for (c, start) in corners {
        for i in 0..=segments {
            let a = start + std::f32::consts::FRAC_PI_2 * i as f32 / segments as f32;
            pts.push(spt(c.x + radius * a.cos(), c.y + radius * a.sin()));
        }
    }
    pts.push(pts[0]);
    pts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rounded_outline_is_closed_and_stays_on_the_rectangle() {
        let r = SRect {
            min: spt(0.0, 0.0),
            max: spt(80.0, 36.0),
        };
        let pts = rounded_rect_outline(r, 8.0, 8);
        assert_eq!(pts.first(), pts.last());
        assert_eq!(pts.len(), 4 * 9 + 1);
        for p in &pts {
            assert!((-1e-3..=80.001).contains(&p.x) && (-1e-3..=36.001).contains(&p.y));
        }
        // Starts at the left edge, just below the top-left corner.
        assert!((pts[0].x - 0.0).abs() < 1e-4 && (pts[0].y - 8.0).abs() < 1e-4);
    }

    #[test]
    fn rounded_outline_with_a_large_radius_is_clamped() {
        let r = SRect {
            min: spt(0.0, 0.0),
            max: spt(10.0, 10.0),
        };
        for p in rounded_rect_outline(r, 50.0, 4) {
            assert!(((p.x - 5.0).powi(2) + (p.y - 5.0).powi(2)).sqrt() <= 5.001);
        }
    }
}
