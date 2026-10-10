//! The diagram canvas: egui input to view calls, and the view drawn with
//! [`EguiRenderer`] (SPEC-Rust §2.1, §6.1).

use eframe::egui::{self, Event, MouseWheelUnit, PointerButton, Sense, Vec2};
use pd_core::geometry::{Pt, pt};
use pd_core::render::constants::ZOOM_STEP;
use pd_core::render::{SRect, spt};
use pd_core::view::DiagramView;

use crate::renderer::EguiRenderer;

/// Points per wheel notch when the wheel reports points (the browser does).
const POINTS_PER_NOTCH: f32 = 100.0;
/// Points to pan per wheel line (native mouse wheels report lines).
const POINTS_PER_LINE: f32 = 40.0;

/// Shows the canvas filling the available space, handles zoom and pan, and
/// draws the view.
pub fn show(ui: &mut egui::Ui, view: &mut DiagramView) -> egui::Response {
    let (response, painter) = ui.allocate_painter(ui.available_size(), Sense::click_and_drag());
    let rect = response.rect;

    if ui.rect_contains_pointer(rect) {
        let anchor = ui
            .input(|i| i.pointer.hover_pos())
            .map_or(to_pt(rect.center().to_vec2()), |p| to_pt(p.to_vec2()));
        handle_wheel_and_pinch(ui, view, anchor, rect.height());
    }
    handle_drag_pan(ui, &response, view);

    let painter = painter.with_clip_rect(rect);
    let area = SRect {
        min: spt(rect.min.x, rect.min.y),
        max: spt(rect.max.x, rect.max.y),
    };
    view.draw(&mut EguiRenderer::new(&painter), area);
    response
}

/// Ctrl/Cmd + wheel zooms by ×1.15 per notch about the cursor; a plain wheel or
/// two-finger scroll pans (Shift + wheel pans sideways); a trackpad or touch
/// pinch zooms (S §6.6, SPEC-Rust §6.1).
fn handle_wheel_and_pinch(ui: &egui::Ui, view: &mut DiagramView, anchor: Pt, page: f32) {
    let (events, multi_touch) = ui.input(|i| (i.events.clone(), i.multi_touch()));
    for event in events {
        match event {
            Event::MouseWheel {
                unit,
                delta,
                modifiers,
                ..
            } => {
                if modifiers.command || modifiers.ctrl {
                    let notches = match unit {
                        MouseWheelUnit::Line => delta.y,
                        MouseWheelUnit::Point => delta.y / POINTS_PER_NOTCH,
                        MouseWheelUnit::Page => delta.y * 3.0,
                    };
                    view.zoom_at(ZOOM_STEP.powf(f64::from(notches)), anchor);
                } else {
                    let mut d = match unit {
                        MouseWheelUnit::Line => delta * POINTS_PER_LINE,
                        MouseWheelUnit::Point => delta,
                        MouseWheelUnit::Page => delta * page,
                    };
                    if modifiers.shift && d.x == 0.0 {
                        d = Vec2::new(d.y, 0.0);
                    }
                    view.pan(to_pt(d));
                }
            }
            Event::Zoom(factor) => view.zoom_at(f64::from(factor), anchor),
            _ => {}
        }
    }
    // Two-finger pinch on a touch screen.
    if let Some(touch) = multi_touch {
        let centre = to_pt(touch.center_pos.to_vec2());
        view.zoom_at(f64::from(touch.zoom_delta), centre);
        view.pan(to_pt(touch.translation_delta));
    }
}

/// Middle-button drag and Space + drag pan. Until selection arrives in R3, a
/// plain left drag pans too.
fn handle_drag_pan(ui: &egui::Ui, response: &egui::Response, view: &mut DiagramView) {
    let multi_touch = ui.input(|i| i.multi_touch().is_some());
    if multi_touch {
        return;
    }
    if response.dragged_by(PointerButton::Middle) || response.dragged_by(PointerButton::Primary) {
        view.pan(to_pt(response.drag_delta()));
    }
}

fn to_pt(v: Vec2) -> Pt {
    pt(f64::from(v.x), f64::from(v.y))
}
