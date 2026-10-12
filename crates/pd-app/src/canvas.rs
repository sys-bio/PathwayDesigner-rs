//! The diagram canvas: egui input to view events, and the view drawn with
//! [`EguiRenderer`] (SPEC-Rust §2.1, §6.1).

use eframe::egui::{self, CursorIcon, Event, Key, MouseWheelUnit, PointerButton, Sense, Vec2};
use pd_core::geometry::{Pt, pt};
use pd_core::render::constants::{TOOLTIP_DELAY_MS, ZOOM_STEP};
use pd_core::render::{SRect, spt};
use pd_core::view::{
    Arrange, Button, DiagramView, Modifiers, Request, Target, Tool, ViewEvent, ViewResponse,
};

use crate::renderer::EguiRenderer;

/// Points per wheel notch when the wheel reports points (the browser does).
const POINTS_PER_NOTCH: f32 = 100.0;
/// Points to pan per wheel line (native mouse wheels report lines).
const POINTS_PER_LINE: f32 = 40.0;

/// Canvas state the view doesn't own: panning and menus are the app's business.
#[derive(Default)]
pub struct CanvasState {
    /// A one-finger touch drag that started on empty canvas pans (SPEC-Rust §6.1).
    touch_pan: bool,
    /// What the open context menu is for.
    menu_target: Option<Target>,
    /// Where the pointer was last seen over the canvas, and since when
    /// (egui time, seconds) it has been still there.
    still_at: Option<(egui::Pos2, f64)>,
    /// What the tooltip on show is about.
    tooltip: Option<Target>,
}

/// A command chosen from the context menu (S §13.4). The app carries it out,
/// because some need a dialog.
#[derive(Clone, Debug, PartialEq)]
pub enum MenuAction {
    CreateAlias(String),
    SplitIntoAliases(String),
    Rename(String),
    GoToPrimary(String),
    DeleteSpecies(String),
    RemoveNullNode(String),
    ToggleLock,
    DeleteReaction(String),
    MakeNice(String),
    Arrange(Arrange),
}

/// What happened on the canvas this frame.
#[derive(Default)]
pub struct CanvasOutput {
    pub view: ViewResponse,
    pub action: Option<MenuAction>,
    /// The canvas's centre on screen, for centring things in it.
    pub centre: Pt,
}

/// Shows the canvas filling the available space, feeds input to the view and
/// draws it.
pub fn show(ui: &mut egui::Ui, view: &mut DiagramView, state: &mut CanvasState) -> CanvasOutput {
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
        let accepts_move = response.hovered();
        let events = ui.input(|i| i.events.clone());
        let touching = events.iter().any(|e| matches!(e, Event::Touch { .. }));
        for event in events {
            if let Some(ev) = to_view_event(&event, view, accepts_press, accepts_move) {
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
        if response.double_clicked_by(PointerButton::Primary)
            && let Some(pos) = response.interact_pointer_pos()
        {
            result.merge(view.handle(&ViewEvent::DoubleClick {
                pos: to_pt(pos.to_vec2()),
            }));
        }
        if !ui.rect_contains_pointer(rect) && !view.is_busy() {
            view.handle(&ViewEvent::PointerLeft);
        }
    }
    if let Some(Request::ContextMenu(target)) = &result.request {
        state.menu_target = Some(target.clone());
    }

    let mut action = None;
    let response = response.on_hover_cursor(cursor(view, hover, space));
    response.context_menu(|ui| match &state.menu_target {
        Some(target) => action = context_menu(ui, view, target),
        None => ui.close(),
    });

    let painter = painter.with_clip_rect(rect);
    let area = SRect {
        min: spt(rect.min.x, rect.min.y),
        max: spt(rect.max.x, rect.max.y),
    };
    let mut renderer = EguiRenderer::new(&painter);
    view.draw(&mut renderer, area);
    update_tooltip(ui, view, state, rect);
    if let (Some(target), Some(p)) = (&state.tooltip, hover)
        && let Some(text) = view.tooltip_text(target)
    {
        view.draw_tooltip(&mut renderer, &text, to_pt(p.to_vec2()), area);
    }
    CanvasOutput {
        view: result,
        action,
        centre: to_pt(rect.center().to_vec2()),
    }
}

/// Shows the tooltip once the pointer has been still for 600 ms over a
/// species, junction or leg; hides it when the pointer moves to another item
/// or leaves the canvas, or while dragging (S §6.6).
fn update_tooltip(ui: &egui::Ui, view: &DiagramView, state: &mut CanvasState, rect: egui::Rect) {
    let (now, pos) = ui.input(|i| (i.time, i.pointer.hover_pos()));
    let over = pos.filter(|p| rect.contains(*p) && ui.rect_contains_pointer(rect));
    let Some(pos) = over.filter(|_| !view.is_busy()) else {
        state.tooltip = None;
        state.still_at = None;
        return;
    };
    let target = view.target_at(to_pt(pos.to_vec2()));
    if state.tooltip.as_ref().is_some_and(|t| *t != target) {
        state.tooltip = None;
    }
    let since = match state.still_at {
        Some((p, since)) if p == pos => since,
        _ => {
            state.still_at = Some((pos, now));
            now
        }
    };
    if state.tooltip.is_none() && target != Target::Empty {
        let delay = TOOLTIP_DELAY_MS as f64 / 1000.0;
        let waited = now - since;
        if waited >= delay {
            state.tooltip = Some(target);
        } else {
            // Nothing else may cause a repaint while the pointer is still.
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_secs_f64(delay - waited));
        }
    }
}

/// The items for what was right-clicked (S §13.4). Returns the one chosen.
fn context_menu(ui: &mut egui::Ui, view: &DiagramView, target: &Target) -> Option<MenuAction> {
    let mut chosen = None;
    let mut item = |ui: &mut egui::Ui, label: &str, action: MenuAction| {
        if ui.button(label).clicked() {
            chosen = Some(action);
            ui.close();
        }
    };
    match target {
        Target::Species(id) => {
            let Some(node) = view.model().find_species(id) else {
                ui.close();
                return None;
            };
            if node.is_null_node {
                item(
                    ui,
                    "Remove Null Node",
                    MenuAction::RemoveNullNode(id.clone()),
                );
            } else if node.is_alias() {
                item(
                    ui,
                    "Rename (renames the primary)",
                    MenuAction::Rename(id.clone()),
                );
                item(ui, "Go to Primary", MenuAction::GoToPrimary(id.clone()));
                item(ui, "Delete Alias", MenuAction::DeleteSpecies(id.clone()));
            } else {
                item(ui, "Create Alias", MenuAction::CreateAlias(id.clone()));
                if view.can_split_into_aliases(id) {
                    item(
                        ui,
                        "Split into Aliases (one per reaction)",
                        MenuAction::SplitIntoAliases(id.clone()),
                    );
                } else {
                    ui.add_enabled(
                        false,
                        egui::Button::new("Split into Aliases (one per reaction)"),
                    )
                    .on_disabled_hover_text("Only for a species in two or more reactions");
                }
                item(ui, "Rename", MenuAction::Rename(id.clone()));
                item(ui, "Delete Species", MenuAction::DeleteSpecies(id.clone()));
            }
            alignment(ui, view, &mut item);
            ui.separator();
            let lock = if view.lock_would_lock() {
                "Lock"
            } else {
                "Unlock"
            };
            item(ui, lock, MenuAction::ToggleLock);
        }
        Target::Junction(id) | Target::Leg(id) => {
            item(
                ui,
                "Delete Reaction",
                MenuAction::DeleteReaction(id.clone()),
            );
            item(
                ui,
                "Adjust Reaction (Make Nice)",
                MenuAction::MakeNice(id.clone()),
            );
            alignment(ui, view, &mut item);
        }
        Target::Handle(_) | Target::Empty => ui.close(),
    }
    chosen
}

/// The Alignment submenu: the align and distribute commands for the
/// selected species and junctions (S §7.5).
fn alignment(
    ui: &mut egui::Ui,
    view: &DiagramView,
    item: &mut impl FnMut(&mut egui::Ui, &str, MenuAction),
) {
    ui.menu_button("Alignment", |ui| {
        for (i, cmd) in Arrange::ALL.into_iter().enumerate() {
            if i == 6 {
                ui.separator();
            }
            if view.can_arrange(cmd) {
                item(ui, cmd.label(), MenuAction::Arrange(cmd));
            } else {
                ui.add_enabled(false, egui::Button::new(cmd.label()));
            }
        }
    });
}

/// The view event for an egui event, if it is one the view cares about.
/// Presses only count over the canvas; moves and releases are passed on
/// while a gesture is in progress, even outside it.
fn to_view_event(
    event: &Event,
    view: &DiagramView,
    accepts_press: bool,
    accepts_move: bool,
) -> Option<ViewEvent> {
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
        Event::PointerMoved(pos) if view.is_busy() || accepts_move => {
            Some(ViewEvent::PointerMove {
                pos: to_pt(pos.to_vec2()),
            })
        }
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

/// A hand over things that can be dragged, a closed hand while dragging. A
/// Bézier handle is small, so over one, and while dragging it, the cursor is
/// a crosshair, which leaves the handle in view.
fn cursor(view: &DiagramView, hover: Option<egui::Pos2>, space: bool) -> CursorIcon {
    if view.is_dragging_handle() {
        return CursorIcon::Crosshair;
    }
    if view.is_busy() {
        return CursorIcon::Grabbing;
    }
    if space {
        return CursorIcon::Grab;
    }
    if view.tool() != Tool::Select {
        return CursorIcon::Crosshair;
    }
    match hover.map(|p| view.target_at(to_pt(p.to_vec2()))) {
        Some(Target::Handle(_)) => CursorIcon::Crosshair,
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

#[cfg(test)]
mod tests {
    use super::*;
    use pd_core::model::{Model, Participant, ReactionMode};
    use pd_core::view::{Button, Modifiers};

    /// A at (0,0) → B at (400,0), Bézier and selected, so its handles show;
    /// the outer handle of A's leg is at (70,0). Screen = world.
    fn view() -> DiagramView {
        let mut m = Model::new();
        let a = m.add_species(pt(0.0, 0.0), 80.0, 36.0);
        let b = m.add_species(pt(400.0, 0.0), 80.0, 36.0);
        let r = m.add_reaction(pt(200.0, 0.0));
        let r = m.find_reaction_mut(&r).unwrap();
        r.reactants.push(Participant::new(a, 1.0));
        r.products.push(Participant::new(b, 1.0));
        r.mode = ReactionMode::Bezier;
        r.selected = true;
        DiagramView::new(m)
    }

    fn at(x: f32, y: f32) -> Option<egui::Pos2> {
        Some(egui::Pos2::new(x, y))
    }

    #[test]
    fn the_cursor_leaves_handles_in_view() {
        let mut v = view();
        assert_eq!(cursor(&v, at(70.0, 0.0), false), CursorIcon::Crosshair);
        assert_eq!(cursor(&v, at(0.0, 0.0), false), CursorIcon::Grab);
        assert_eq!(cursor(&v, at(600.0, 300.0), false), CursorIcon::Default);
        // Dragging a handle keeps the crosshair; dragging a node closes the hand.
        v.handle(&ViewEvent::PointerDown {
            pos: pt(70.0, 0.0),
            button: Button::Primary,
            modifiers: Modifiers::NONE,
        });
        assert_eq!(cursor(&v, at(70.0, 0.0), false), CursorIcon::Crosshair);
        v.handle(&ViewEvent::PointerUp {
            pos: pt(70.0, 0.0),
            button: Button::Primary,
        });
        v.handle(&ViewEvent::PointerDown {
            pos: pt(400.0, 0.0),
            button: Button::Primary,
            modifiers: Modifiers::NONE,
        });
        assert_eq!(cursor(&v, at(400.0, 0.0), false), CursorIcon::Grabbing);
    }
}
