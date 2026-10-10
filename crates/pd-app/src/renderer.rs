//! `EguiRenderer`: the diagram drawn with egui shapes (SPEC-Rust §5.2).

use eframe::egui::{
    self, Color32, CornerRadius, Painter, Pos2, Rect, Shape, StrokeKind, epaint::CubicBezierShape,
};
use pd_core::model::Rgba;
use pd_core::render::{Renderer, SPt, SRect, Stroke, rounded_rect_outline};
use pd_core::text::font_metrics;

use crate::fonts;

/// Segments per corner when a rounded outline has to be dashed by hand.
const CORNER_SEGMENTS: usize = 8;
/// Segments in a dashed circle.
const CIRCLE_SEGMENTS: usize = 48;
/// Dashes shorter than this are drawn at this length, so a dashed line at a
/// tiny zoom isn't thousands of specks.
const MIN_DASH: f32 = 1.0;
/// Lines thinner than this don't get round end caps: the difference is invisible.
const MIN_CAP_WIDTH: f32 = 2.0;

pub struct EguiRenderer<'a> {
    painter: &'a Painter,
}

impl<'a> EguiRenderer<'a> {
    pub fn new(painter: &'a Painter) -> Self {
        Self { painter }
    }

    fn dashed(&self, pts: &[Pos2], stroke: egui::Stroke, (on, off): (f32, f32)) {
        self.painter.extend(Shape::dashed_line(
            pts,
            stroke,
            on.max(MIN_DASH),
            off.max(MIN_DASH),
        ));
    }
}

pub fn color(c: Rgba) -> Color32 {
    let [r, g, b, a] = c.0;
    Color32::from_rgba_unmultiplied(r, g, b, a)
}

fn pos(p: SPt) -> Pos2 {
    Pos2::new(p.x, p.y)
}

fn egui_stroke(s: Stroke) -> egui::Stroke {
    egui::Stroke::new(s.width, color(s.color))
}

impl Renderer for EguiRenderer<'_> {
    fn round_rect(&mut self, r: SRect, radius: f32, fill: Option<Rgba>, stroke: Option<Stroke>) {
        let rect = Rect::from_min_max(pos(r.min), pos(r.max));
        let corner = CornerRadius::same(radius.round().clamp(0.0, 255.0) as u8);
        if let Some(f) = fill {
            self.painter.rect_filled(rect, corner, color(f));
        }
        match stroke {
            Some(s) if s.dash.is_some() => {
                // egui can't dash a rounded outline; dash a polyline of it instead.
                let outline: Vec<Pos2> = rounded_rect_outline(r, radius, CORNER_SEGMENTS)
                    .into_iter()
                    .map(pos)
                    .collect();
                self.dashed(&outline, egui_stroke(s), s.dash.unwrap());
            }
            Some(s) => {
                self.painter
                    .rect_stroke(rect, corner, egui_stroke(s), StrokeKind::Middle);
            }
            None => {}
        }
    }

    fn circle(&mut self, c: SPt, radius: f32, fill: Option<Rgba>, stroke: Option<Stroke>) {
        if let Some(f) = fill {
            self.painter.circle_filled(pos(c), radius, color(f));
        }
        match stroke {
            Some(s) if s.dash.is_some() => {
                let pts: Vec<Pos2> = (0..=CIRCLE_SEGMENTS)
                    .map(|i| {
                        let a = std::f32::consts::TAU * i as f32 / CIRCLE_SEGMENTS as f32;
                        Pos2::new(c.x + radius * a.cos(), c.y + radius * a.sin())
                    })
                    .collect();
                self.dashed(&pts, egui_stroke(s), s.dash.unwrap());
            }
            Some(s) => {
                self.painter.circle_stroke(pos(c), radius, egui_stroke(s));
            }
            None => {}
        }
    }

    fn polyline(&mut self, pts: &[SPt], stroke: Stroke) {
        let points: Vec<Pos2> = pts.iter().copied().map(pos).collect();
        let s = egui_stroke(stroke);
        match stroke.dash {
            Some(dash) => self.dashed(&points, s, dash),
            None => {
                // egui strokes have square ends; a dot at each end rounds them.
                if stroke.round_cap && stroke.width >= MIN_CAP_WIDTH && points.len() >= 2 {
                    let r = stroke.width / 2.0;
                    self.painter.circle_filled(points[0], r, s.color);
                    self.painter
                        .circle_filled(points[points.len() - 1], r, s.color);
                }
                self.painter.line(points, s);
            }
        }
    }

    fn cubic(&mut self, p0: SPt, c1: SPt, c2: SPt, p3: SPt, stroke: Stroke) {
        self.painter.add(CubicBezierShape::from_points_stroke(
            [pos(p0), pos(c1), pos(c2), pos(p3)],
            false,
            Color32::TRANSPARENT,
            egui_stroke(stroke),
        ));
    }

    fn polygon(&mut self, pts: &[SPt], fill: Rgba) {
        self.painter.add(Shape::convex_polygon(
            pts.iter().copied().map(pos).collect(),
            color(fill),
            egui::Stroke::NONE,
        ));
    }

    fn text(&mut self, center: SPt, s: &str, size: f32, c: Rgba) {
        let galley = self
            .painter
            .layout_no_wrap(s.to_owned(), fonts::diagram_font(size), color(c));
        let baseline_in_galley = galley
            .rows
            .first()
            .and_then(|row| row.row.glyphs.first().map(|g| row.pos.y + g.pos.y))
            .unwrap_or(0.0);
        let cap = font_metrics(f64::from(size)).cap_height as f32;
        let top_left = Pos2::new(
            center.x - galley.size().x / 2.0,
            center.y + cap / 2.0 - baseline_in_galley,
        );
        self.painter.galley(top_left, galley, color(c));
    }
}
