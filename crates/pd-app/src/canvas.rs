//! The diagram canvas: egui input to view events, and the view drawn with
//! [`EguiRenderer`] (SPEC-Rust §2.1, §6.1).

use eframe::egui::{self, CursorIcon, Event, Key, MouseWheelUnit, PointerButton, Sense, Vec2};
use pd_core::geometry::{Pt, pt};
use pd_core::render::constants::ZOOM_STEP;
use pd_core::render::{SRect, spt};
use pd_core::view::{Button, DiagramView, Modifiers, Target, ViewEvent, ViewResponse};

use crate::renderer::EguiRenderer;

/// Points per wheel notch when the wheel reports points (the browser does).
const POINTS_PER_NOTCH: f32 = 100.0;
/// Points to pan per wheel line (native mouse wheels report lines).
const POINTS_PER_LINE: f32 = 40.0;

/// Canvas state the view doesn't own: panning is the app's business.
#[derive(Default)]
pub struct CanvasState {
    /// A one-finger touch drag that started on empty canvas pans (SPEC-Rust §6.1).
    touch_pan: bool,
}

/// Shows the canvas filling the available space, feeds input to the view and
/// draws it. Returns what the view reported.
pub fn show(ui: &mut egui::Ui, view: &mut DiagramView, state: &mut CanvasState) -> ViewResponse {
    let (response, painter) = ui.allocate_painter(ui.available_size(), Sense::click_and_drag());
    let rect = response.rect;
    let mut result = ViewResponse::default();

    let hover = ui.input(|i| i.pointer.hover_pos());
    if ui.rect_contains_pointer(rect) {
        let anchor = hover.map_or(to_pt(rect.center().to_vec2()), |p| to_pt(p.to_vec2()));
        handle_wheel_and_pinch(ui, view, anchor, rect.height());
    }

    let space = ui.input(|i| i.key_down(Key::Space)) && !ui.ctx().egui_wants_keyboard_input();
    let multi_touch = ui.input(|i| i.multi_touch().is_some());
    let panning = response.dragged_by(PointerButton::Middle)
        || (space && response.dragged_by(PointerButton::Primary))
        || (state.touch_pan && response.dragged_by(PointerButton::Primary));
    if panning && !multi_touch {
        view.pan(to_pt(response.drag_delta()));
    }

    if !space {
        let accepts_press = response.hovered() && !multi_touch;
        let events = ui.input(|i| i.events.clone());
        let touching = events.iter().any(|e| matches!(e, Event::Touch { .. }));
        for event in events {
            if let Some(ev) = to_view_event(&event, view, accepts_press) {
                // A touch on empty canvas pans instead of starting a rubber band.
                if touching && is_press(&ev) && is_empty_at(view, &ev) {
                    state.touch_pan = true;
                    continue;
                }
                result.merge(view.handle(&ev));
            }
            if matches!(event, Event::PointerButton { pressed: false, .. }) {
                state.touch_pan = false;
            }
        }
    }

    response.on_hover_cursor(cursor(view, hover, space));

    let painter = painter.with_clip_rect(rect);
    let area = SRect {
        min: spt(rect.min.x, rect.min.y),
        max: spt(rect.max.x, rect.max.y),
    };
    view.draw(&mut EguiRenderer::new(&painter), area);
    result
}

/// The view event for an egui event, if it is one the view cares about.
/// Presses only count over the canvas; moves and releases are passed on
/// while a gesture is in progress, even outside it.
fn to_view_event(event: &Event, view: &DiagramView, accepts_press: bool) -> Option<ViewEvent> {
    match *event {
        Event::PointerButton {
            pos,
            button,
            pressed,
            modifiers,
        } => {
            let button = match button {
                PointerButton::Primary => Button::Primary,
                PointerButton::Secondary => Button::Secondary,
                PointerButton::Middle => Button::Middle,
                _ => return None,
            };
            let pos = to_pt(pos.to_vec2());
            if pressed {
                accepts_press.then_some(ViewEvent::PointerDown {
                    pos,
                    button,
                    modifiers: Modifiers {
                        shift: modifiers.shift,
                        command: modifiers.command,
                        alt: modifiers.alt,
                    },
                })
            } else {
                view.is_busy()
                    .then_some(ViewEvent::PointerUp { pos, button })
            }
        }
        Event::PointerMoved(pos) if view.is_busy() => Some(ViewEvent::PointerMove {
            pos: to_pt(pos.to_vec2()),
        }),
        Event::WindowFocused(false) => Some(ViewEvent::FocusLost),
        _ => None,
    }
}

fn is_press(ev: &ViewEvent) -> bool {
    matches!(
        ev,
        ViewEvent::PointerDown {
            button: Button::Primary,
            ..
        }
    )
}

fn is_empty_at(view: &DiagramView, ev: &ViewEvent) -> bool {
    match ev {
        ViewEvent::PointerDown { pos, .. } => view.target_at(*pos) == Target::Empty,
        _ => false,
    }
}

/// A hand over things that can be dragged, a closed hand while dragging.
fn cursor(view: &DiagramView, hover: Option<egui::Pos2>, space: bool) -> CursorIcon {
    if view.is_busy() {
        return CursorIcon::Grabbing;
    }
    if space {
        return CursorIcon::Grab;
    }
    match hover.map(|p| view.target_at(to_pt(p.to_vec2()))) {
        Some(Target::Species(_) | Target::Junction(_)) => CursorIcon::Grab,
        Some(Target::Leg(_)) => CursorIcon::PointingHand,
        _ => CursorIcon::Default,
    }
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

fn to_pt(v: Vec2) -> Pt {
    pt(f64::from(v.x), f64::from(v.y))
}
