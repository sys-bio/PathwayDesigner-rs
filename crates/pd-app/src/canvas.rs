//! The diagram canvas: egui input to view events, and the view drawn with
//! [`EguiRenderer`] (SPEC-Rust §2.1, §6.1).

use eframe::egui::{self, CursorIcon, Event, Key, MouseWheelUnit, PointerButton, Sense, Vec2};
use pd_core::geometry::{Pt, pt};
use pd_core::render::constants::{TOOLTIP_DELAY_MS, ZOOM_STEP};
use pd_core::render::{SRect, spt};
use pd_core::view::{
    Arrange, Button, DiagramView, LineRef, Modifiers, Request, Target, Tool, ViewEvent,
    ViewResponse,
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
    MergeAliases(String),
    SplitOffSelected(String),
    MergeSelectedInto(String),
    MergeIntoPrimary(String),
    CopyStyle(Target),
    PasteStyle,
    /// Asks for a name to save the item's look under (S §8.6).
    SaveStyle(Target),
    /// The selected items of the kind take the style (S §8.6).
    ApplyStyle(pd_core::model::StyleKind, String),
    /// The style takes the item's look.
    UpdateStyle(String, Target),
    Rename(String),
    GoToPrimary(String),
    DeleteSpecies(String),
    RemoveNullNode(String),
    ToggleLock,
    DeleteReaction(String),
    /// Add Reactant… or Add Product… (S §7.11).
    AddParticipant(String, pd_core::model::Role),
    /// Remove from Reaction: the reaction and the species.
    RemoveParticipant(String, String),
    /// Modifier Kind ▸: the reaction, the modifier and its new kind
    /// (S §5.7.1).
    ModifierKind(String, String, pd_core::model::ModifierKind),
    /// Add Corner Point: the reaction, the line (its role and index), the
    /// piece of it and where (S §5.7.1, §5.6.5).
    AddCornerPoint(String, LineRef, usize, pd_core::geometry::Pt),
    /// Remove Corner Point: the reaction, the line, the point's index.
    RemoveCornerPoint(String, LineRef, usize),
    /// Straighten Modifier Line or Straighten Line: the reaction and the
    /// line.
    StraightenLine(String, LineRef),
    /// Add Stem, Straighten Stem and Remove Stem (S §5.13).
    AddStem(String),
    StraightenStem(String),
    RemoveStem(String),
    MakeNice(String),
    Arrange(Arrange),
    EditText(String),
    DeleteText(String),
    DeleteCompartment(String),
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
    // Shift during a species resize keeps proportions (S §7.10).
    result.merge(view.set_shift_held(ui.input(|i| i.modifiers.shift)));
    // Alt turns snapping to the grid off while it is held (S §7.6).
    result.merge(view.set_alt_held(ui.input(|i| i.modifiers.alt)));
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
    // A choice in a submenu, which `item` can't reach.
    let mut item_chosen = None;
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
                split_off(ui, view, id, &mut item);
                merge_selected(ui, view, id, &mut item);
                item(
                    ui,
                    "Merge into Primary",
                    MenuAction::MergeIntoPrimary(id.clone()),
                );
                item(
                    ui,
                    "Merge All Aliases into Primary",
                    MenuAction::MergeAliases(id.clone()),
                );
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
                split_off(ui, view, id, &mut item);
                if view.can_merge_aliases(id) {
                    item(ui, "Merge Aliases", MenuAction::MergeAliases(id.clone()));
                } else {
                    ui.add_enabled(false, egui::Button::new("Merge Aliases"))
                        .on_disabled_hover_text("Only for a species that has aliases");
                }
                merge_selected(ui, view, id, &mut item);
                item(ui, "Rename", MenuAction::Rename(id.clone()));
                item(ui, "Delete Species", MenuAction::DeleteSpecies(id.clone()));
            }
            if !node.is_null_node {
                ui.separator();
                style_items(ui, view, &Target::Species(id.clone()), &mut item);
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
            use pd_core::model::Role;
            item(
                ui,
                "Add Reactant…",
                MenuAction::AddParticipant(id.clone(), Role::Reactant),
            );
            item(
                ui,
                "Add Product…",
                MenuAction::AddParticipant(id.clone(), Role::Product),
            );
            // Each reactant and product, null nodes aside; the last one left
            // can't go.
            if let Some(r) = view.model().find_reaction(id) {
                let m = view.model();
                let listed: Vec<(String, &str)> =
                    [(&r.reactants, "reactant"), (&r.products, "product")]
                        .into_iter()
                        .flat_map(|(list, role)| {
                            list.iter().map(move |p| (p.species.clone(), role))
                        })
                        .filter(|(s, _)| m.find_species(s).is_some_and(|n| !n.is_null_node))
                        .collect();
                let mut removed = None;
                ui.menu_button("Remove from Reaction", |ui| {
                    for (species, role) in listed {
                        let label = match m.find_species(&species) {
                            Some(n) if n.is_alias() => {
                                format!("{} ({role}, alias)", n.primary_id())
                            }
                            _ => format!("{species} ({role})"),
                        };
                        let ok = view.can_remove_participant(id, &species);
                        let b = ui
                            .add_enabled(ok, egui::Button::new(label))
                            .on_disabled_hover_text("A reaction needs at least one species");
                        if b.clicked() {
                            removed = Some(MenuAction::RemoveParticipant(id.clone(), species));
                            ui.close();
                        }
                    }
                });
                if removed.is_some() {
                    item_chosen = removed;
                }
                // What each modifier does (S §5.7.1): the kinds straight
                // away for one modifier, a submenu each for several.
                if !r.modifiers.is_empty() {
                    let mut kind_chosen = None;
                    let kinds = |ui: &mut egui::Ui, species: &str, now, chosen: &mut Option<_>| {
                        for kind in pd_core::model::ModifierKind::ALL {
                            if ui.radio(now == kind, kind.label()).clicked() {
                                *chosen = Some(MenuAction::ModifierKind(
                                    id.clone(),
                                    species.to_owned(),
                                    kind,
                                ));
                                ui.close();
                            }
                        }
                    };
                    ui.menu_button("Modifier Kind", |ui| {
                        if let [only] = r.modifiers.as_slice() {
                            kinds(ui, &only.species, only.kind, &mut kind_chosen);
                        } else {
                            for mo in &r.modifiers {
                                let name = m
                                    .find_species(&mo.species)
                                    .map_or(mo.species.as_str(), |n| n.primary_id());
                                ui.menu_button(name, |ui| {
                                    kinds(ui, &mo.species, mo.kind, &mut kind_chosen);
                                });
                            }
                        }
                    });
                    if kind_chosen.is_some() {
                        item_chosen = kind_chosen;
                    }
                }
            }
            // The reactant's or product's line clicked (S §5.6.5): a corner
            // point there, or its corner points taken off.
            let on_line = match (target, view.context_point()) {
                (Target::Leg(_), Some(at)) => view
                    .line_piece_at(id, at)
                    .filter(|_| {
                        view.model().find_reaction(id).is_some_and(|r| {
                            pd_core::render::draw::effective_mode(r)
                                != pd_core::model::ReactionMode::Linear
                        })
                    })
                    .map(|(role, index, piece)| ((role, index), piece, at)),
                _ => None,
            };
            if let Some((line, piece, at)) = on_line {
                ui.separator();
                item(
                    ui,
                    "Add Corner Point",
                    MenuAction::AddCornerPoint(id.clone(), line, piece, at),
                );
                let b = ui
                    .add_enabled(
                        view.line_bent(id, line),
                        egui::Button::new("Straighten Line"),
                    )
                    .on_disabled_hover_text("It has no corner points");
                if b.clicked() {
                    item_chosen = Some(MenuAction::StraightenLine(id.clone(), line));
                    ui.close();
                }
                ui.separator();
            }
            if view
                .model()
                .find_reaction(id)
                .is_some_and(|r| r.stem.is_some())
            {
                let bent = view.straightened_stem(id).is_some();
                let b = ui
                    .add_enabled(bent, egui::Button::new("Straighten Stem"))
                    .on_disabled_hover_text("The stem is straight");
                if b.clicked() {
                    item_chosen = Some(MenuAction::StraightenStem(id.clone()));
                    ui.close();
                }
                item(ui, "Remove Stem", MenuAction::RemoveStem(id.clone()));
            } else {
                item(ui, "Add Stem", MenuAction::AddStem(id.clone()));
            }
            ui.separator();
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
            ui.separator();
            style_items(ui, view, &Target::Leg(id.clone()), &mut item);
            alignment(ui, view, &mut item);
        }
        Target::Text(id) => {
            item(ui, "Edit Text", MenuAction::EditText(id.clone()));
            item(ui, "Delete Text", MenuAction::DeleteText(id.clone()));
            ui.separator();
            style_items(ui, view, &Target::Text(id.clone()), &mut item);
            alignment(ui, view, &mut item);
        }
        // A modifier's arc (S §5.7.1): what the modifier does, and its
        // corner points.
        Target::ModifierLine(id, modifier, piece) => {
            let Some(mo) = view
                .model()
                .find_reaction(id)
                .and_then(|r| r.modifiers.get(*modifier))
            else {
                ui.close();
                return None;
            };
            let name = view
                .model()
                .find_species(&mo.species)
                .map_or(mo.species.clone(), |n| n.primary_id().to_owned());
            ui.label(egui::RichText::new(format!("Modifier {name}")).weak());
            let mut kind_chosen = None;
            ui.menu_button("Modifier Kind", |ui| {
                for kind in pd_core::model::ModifierKind::ALL {
                    if ui.radio(mo.kind == kind, kind.label()).clicked() {
                        kind_chosen = Some(MenuAction::ModifierKind(
                            id.clone(),
                            mo.species.clone(),
                            kind,
                        ));
                        ui.close();
                    }
                }
            });
            if kind_chosen.is_some() {
                item_chosen = kind_chosen;
            }
            let at = view.context_point().unwrap_or_default();
            let line = (pd_core::model::Role::Modifier, *modifier);
            item(
                ui,
                "Add Corner Point",
                MenuAction::AddCornerPoint(id.clone(), line, *piece, at),
            );
            let bent = view.line_bent(id, line);
            let b = ui
                .add_enabled(bent, egui::Button::new("Straighten Modifier Line"))
                .on_disabled_hover_text("It has no corner points");
            if b.clicked() {
                item_chosen = Some(MenuAction::StraightenLine(id.clone(), line));
                ui.close();
            }
        }
        Target::CornerPoint(id, role, index, point) => {
            item(
                ui,
                "Remove Corner Point",
                MenuAction::RemoveCornerPoint(id.clone(), (*role, *index), *point),
            );
        }
        Target::Compartment(id) => {
            item(
                ui,
                "Delete Compartment (keeps its species)",
                MenuAction::DeleteCompartment(id.clone()),
            );
            ui.separator();
            style_items(ui, view, &Target::Compartment(id.clone()), &mut item);
        }
        Target::Handle(_)
        | Target::TextWrap(_)
        | Target::CompartmentCorner(..)
        | Target::SpeciesCorner(..)
        | Target::SpeciesLabel(_)
        | Target::ReactionLabel(_)
        | Target::StemEnd(..)
        | Target::AddLine(_)
        | Target::Empty => ui.close(),
    }
    chosen.or(item_chosen)
}

/// Copy Style and Paste Style (S §8.5), Save Style…, Apply Style and
/// Change Style to Match This (S §8.6), for the item right-clicked.
fn style_items(
    ui: &mut egui::Ui,
    view: &DiagramView,
    target: &Target,
    item: &mut impl FnMut(&mut egui::Ui, &str, MenuAction),
) {
    item(ui, "Copy Style", MenuAction::CopyStyle(target.clone()));
    if view.can_paste_style() {
        item(ui, "Paste Style", MenuAction::PasteStyle);
    } else {
        ui.add_enabled(false, egui::Button::new("Paste Style"))
            .on_disabled_hover_text("Copy the style of something of this kind first");
    }
    item(ui, "Save Style…", MenuAction::SaveStyle(target.clone()));
    let kind = style_kind(target);
    let names: Vec<String> = kind
        .map(|k| view.style_entries(k).into_iter().map(|e| e.name).collect())
        .unwrap_or_default();
    const CHANGE: &str = "Change Style to Match This";
    if let (Some(kind), false) = (kind, names.is_empty()) {
        ui.menu_button("Apply Style", |ui| {
            for name in &names {
                item(ui, name, MenuAction::ApplyStyle(kind, name.clone()));
            }
        })
        .response
        .on_hover_text("Give the selected items a saved style");
    } else {
        ui.add_enabled(false, egui::Button::new("Apply Style"))
            .on_disabled_hover_text("No saved style of this kind yet: use Save Style… first");
    }
    if names.is_empty() {
        ui.add_enabled(false, egui::Button::new(CHANGE))
            .on_disabled_hover_text("No saved style of this kind yet");
    } else {
        ui.menu_button(CHANGE, |ui| {
            for name in names {
                item(
                    ui,
                    &name,
                    MenuAction::UpdateStyle(name.clone(), target.clone()),
                );
            }
        })
        .response
        .on_hover_text(
            "Change a saved style to this item's look: every item that follows it changes too",
        );
    }
}

/// The kind of item a context-menu target is, for its styles.
fn style_kind(t: &Target) -> Option<pd_core::model::StyleKind> {
    use pd_core::model::StyleKind;
    match t {
        Target::Species(_) | Target::SpeciesCorner(..) | Target::SpeciesLabel(_) => {
            Some(StyleKind::Species)
        }
        Target::Junction(_)
        | Target::Leg(_)
        | Target::Handle(_)
        | Target::ReactionLabel(_)
        | Target::StemEnd(..)
        | Target::AddLine(_)
        | Target::ModifierLine(..)
        | Target::CornerPoint(..) => Some(StyleKind::Reaction),
        Target::Compartment(_) | Target::CompartmentCorner(..) => Some(StyleKind::Compartment),
        Target::Text(_) | Target::TextWrap(_) => Some(StyleKind::Text),
        Target::Empty => None,
    }
}

/// Split Off Selected Reactions (S §7.3), or why it is not offered.
fn split_off(
    ui: &mut egui::Ui,
    view: &DiagramView,
    id: &str,
    item: &mut impl FnMut(&mut egui::Ui, &str, MenuAction),
) {
    const LABEL: &str = "Split Off Selected Reactions";
    if view.can_split_off_selected(id) {
        item(ui, LABEL, MenuAction::SplitOffSelected(id.to_owned()));
    } else {
        ui.add_enabled(false, egui::Button::new(LABEL))
            .on_disabled_hover_text("Select a reaction this species is in");
    }
}

/// Merge Selected Aliases into This (S §7.3), or why it is not offered.
fn merge_selected(
    ui: &mut egui::Ui,
    view: &DiagramView,
    id: &str,
    item: &mut impl FnMut(&mut egui::Ui, &str, MenuAction),
) {
    const LABEL: &str = "Merge Selected Aliases into This";
    if view.can_merge_selected_into(id) {
        item(ui, LABEL, MenuAction::MergeSelectedInto(id.to_owned()));
    } else {
        ui.add_enabled(false, egui::Button::new(LABEL))
            .on_disabled_hover_text("Select other aliases of this species");
    }
}

/// The Alignment submenu: the align and distribute commands for the
/// selected species and junctions (S §7.5).
fn alignment(
    ui: &mut egui::Ui,
    view: &DiagramView,
    item: &mut impl FnMut(&mut egui::Ui, &str, MenuAction),
) {
    ui.menu_button("Alignment", |ui| {
        for cmd in Arrange::ALL {
            if cmd.starts_group() {
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
        Some(
            Target::Handle(_) | Target::StemEnd(..) | Target::AddLine(_) | Target::CornerPoint(..),
        ) => CursorIcon::Crosshair,
        Some(
            Target::Species(_)
            | Target::SpeciesLabel(_)
            | Target::ReactionLabel(_)
            | Target::Junction(_)
            | Target::Text(_),
        ) => CursorIcon::Grab,
        Some(Target::TextWrap(_)) => CursorIcon::ResizeHorizontal,
        Some(Target::Compartment(_)) => CursorIcon::Grab,
        // Top-left and bottom-right corners, then the other two.
        Some(Target::CompartmentCorner(_, 0 | 2) | Target::SpeciesCorner(_, 0 | 2)) => {
            CursorIcon::ResizeNwSe
        }
        Some(Target::CompartmentCorner(..) | Target::SpeciesCorner(..)) => CursorIcon::ResizeNeSw,
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
