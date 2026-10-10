//! The view: a model plus how it is shown (S §5.1, SPEC-Rust §2.1).
//!
//! `DiagramView` keeps its state between frames. It knows about zoom and
//! scroll but nothing about egui: the app turns egui input into calls here and
//! gives `draw` a renderer. Selection, dragging and editing arrive in R3 and R4.

use crate::geometry::{Pt, pt, screen_to_world, world_to_screen};
use crate::model::Model;
use crate::render::Renderer;
use crate::render::SRect;
use crate::render::constants::{ZOOM_MAX, ZOOM_MIN};
use crate::render::draw::{DrawOptions, content_bounds, draw_model};

#[derive(Clone, Debug)]
pub struct DiagramView {
    pub model: Model,
    zoom: f64,
    /// Screen position of the world origin (S §5.1 `scrollOffset`).
    scroll: Pt,
    /// Aliases drawn in purple with a dashed border (S §7.6).
    pub show_alias_indicator: bool,
    /// Null nodes and their legs drawn (S §12).
    pub show_null_species: bool,
}

impl Default for DiagramView {
    fn default() -> Self {
        Self::new(Model::new())
    }
}

impl DiagramView {
    pub fn new(model: Model) -> Self {
        Self {
            model,
            zoom: 1.0,
            scroll: Pt::ZERO,
            show_alias_indicator: true,
            show_null_species: true,
        }
    }

    pub fn zoom(&self) -> f64 {
        self.zoom
    }

    pub fn scroll(&self) -> Pt {
        self.scroll
    }

    pub fn world_to_screen(&self, w: Pt) -> Pt {
        world_to_screen(w, self.zoom, self.scroll)
    }

    pub fn screen_to_world(&self, s: Pt) -> Pt {
        screen_to_world(s, self.zoom, self.scroll)
    }

    /// Sets zoom (clamped to [0.1, 8]) and scroll directly.
    pub fn set_view(&mut self, zoom: f64, scroll: Pt) {
        self.zoom = zoom.clamp(ZOOM_MIN, ZOOM_MAX);
        self.scroll = scroll;
    }

    /// Multiplies the zoom by `factor`, clamped to [0.1, 8], keeping the world
    /// point under `anchor` (a screen point, usually the cursor) where it is
    /// (S §6.6).
    pub fn zoom_at(&mut self, factor: f64, anchor: Pt) {
        if !(factor.is_finite() && factor > 0.0) {
            return;
        }
        let world = self.screen_to_world(anchor);
        self.zoom = (self.zoom * factor).clamp(ZOOM_MIN, ZOOM_MAX);
        self.scroll = anchor - world * self.zoom;
    }

    /// Moves the drawing by `delta` screen points.
    pub fn pan(&mut self, delta: Pt) {
        self.scroll += delta;
    }

    /// Zooms and scrolls so the content bounds fill a `width × height` screen
    /// area with its top-left corner at `origin`, centred. Never zooms in past
    /// 100 %, so a small model isn't blown up.
    pub fn zoom_to_fit(&mut self, origin: Pt, width: f64, height: f64) {
        let (min, max) = content_bounds(&self.model);
        let size = max - min;
        let zoom = (width / size.x).min(height / size.y).min(1.0);
        self.zoom = zoom.clamp(ZOOM_MIN, ZOOM_MAX);
        let centre = (min + max) / 2.0;
        self.scroll = origin + pt(width / 2.0, height / 2.0) - centre * self.zoom;
    }

    /// Draws the diagram for the screen. `area` is the canvas, filled with the
    /// background first.
    pub fn draw(&self, r: &mut dyn Renderer, area: SRect) {
        let opts = DrawOptions {
            zoom: self.zoom,
            scroll: self.scroll,
            background: Some(area),
            for_output: false,
            show_alias_indicator: self.show_alias_indicator,
            show_null_species: self.show_null_species,
        };
        draw_model(&self.model, &opts, r);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: Pt, b: Pt) -> bool {
        a.distance(b) < 1e-9
    }

    #[test]
    fn zoom_keeps_the_point_under_the_cursor() {
        let mut v = DiagramView::default();
        v.set_view(1.3, pt(40.0, -25.0));
        let cursor = pt(320.0, 210.0);
        let before = v.screen_to_world(cursor);
        for factor in [1.15, 1.15, 1.0 / 1.15, 3.7, 0.2] {
            v.zoom_at(factor, cursor);
            assert!(close(v.screen_to_world(cursor), before));
        }
    }

    #[test]
    fn zoom_is_clamped_and_still_keeps_the_cursor_point() {
        let mut v = DiagramView::default();
        let cursor = pt(100.0, 100.0);
        let before = v.screen_to_world(cursor);
        v.zoom_at(1000.0, cursor);
        assert_eq!(v.zoom(), 8.0);
        assert!(close(v.screen_to_world(cursor), before));
        v.zoom_at(1e-6, cursor);
        assert_eq!(v.zoom(), 0.1);
        assert!(close(v.screen_to_world(cursor), before));
        // Nonsense factors are ignored.
        v.zoom_at(f64::NAN, cursor);
        v.zoom_at(0.0, cursor);
        v.zoom_at(-2.0, cursor);
        assert_eq!(v.zoom(), 0.1);
    }

    #[test]
    fn ctrl_wheel_steps_of_1_15() {
        let mut v = DiagramView::default();
        for _ in 0..3 {
            v.zoom_at(1.15, Pt::ZERO);
        }
        assert!((v.zoom() - 1.15f64.powi(3)).abs() < 1e-12);
    }

    #[test]
    fn pan_moves_the_drawing() {
        let mut v = DiagramView::default();
        v.pan(pt(10.0, -5.0));
        assert_eq!(v.world_to_screen(Pt::ZERO), pt(10.0, -5.0));
    }

    #[test]
    fn zoom_to_fit_centres_the_content() {
        let mut m = Model::new();
        m.add_species(pt(0.0, 0.0), 80.0, 36.0);
        m.add_species(pt(1000.0, 500.0), 80.0, 36.0);
        let mut v = DiagramView::new(m);
        v.zoom_to_fit(pt(0.0, 50.0), 600.0, 400.0);
        // Content is 1200 × 656 with margins: limited by width, 0.5.
        assert!((v.zoom() - 0.5).abs() < 1e-12);
        let (min, max) = content_bounds(&v.model);
        let centre = v.world_to_screen((min + max) / 2.0);
        assert!(close(centre, pt(300.0, 250.0)));
    }
}
