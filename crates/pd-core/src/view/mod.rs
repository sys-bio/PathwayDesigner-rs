//! The view: a model plus how it is shown and edited (S §5.1, §6,
//! SPEC-Rust §2.1).
//!
//! `DiagramView` keeps its state between frames. It knows about zoom, scroll,
//! the selection and the gesture in progress, but nothing about egui: the app
//! turns egui input into [`ViewEvent`]s and gives `draw` a renderer. Every
//! change to the model goes through the view, so every change is undoable.

mod arrange;
#[cfg(test)]
mod bezier_tests;
mod circle;
#[cfg(test)]
mod compartment_tests;
mod editing;
#[cfg(test)]
mod editing_tests;
pub mod hit;
pub mod input;
#[cfg(test)]
mod interaction_tests;
mod interchange;
#[cfg(test)]
mod label_tests;
mod properties;
#[cfg(test)]
mod resize_tests;
#[cfg(test)]
mod saved_style_tests;
mod saved_styles;
#[cfg(test)]
mod snap_tests;
#[cfg(test)]
mod style_tests;
#[cfg(test)]
mod text_tests;
mod tooltip;

use crate::bezier::{Ctrl as HandleCtrl, apply_smooth_handles, store_auto_handles};
use crate::geometry::{Pt, normalize, pt, screen_to_world, world_to_screen};
use crate::model::{Id, Model, ReactionMode, Role};
use crate::render::constants::{RUBBER_BAND, ZOOM_MAX, ZOOM_MIN};
use crate::render::draw::{
    DrawOptions, Shown, content_bounds, draw_model, linear_junction_t, linear_line,
};
use crate::render::{Renderer, SRect, Stroke, spt};
use crate::textbox::{MIN_WRAP_WIDTH, lay_out, text_box};
use crate::undo::{Command, CtrlState, UndoHistory};

pub use arrange::Arrange;
pub use hit::hit_test;
pub use input::{Button, HandleId, Modifiers, Request, Target, Tool, ViewEvent, ViewResponse};
pub use properties::{ColourField, sync_modifiers};
pub use saved_styles::{ImportReport, StyleError};

/// The rubber band's fill: its colour at 20 % (S §5.9).
const RUBBER_BAND_FILL_ALPHA: u8 = 51;

/// The selected species and reactions, by id.
pub type Selection = (Vec<Id>, Vec<Id>);

/// Everything selected, texts and compartments included, to tell when the
/// selection changes.
type SelectionState = (Selection, Vec<Id>, Vec<Id>);

#[derive(Clone, Debug)]
pub struct DiagramView {
    model: Model,
    zoom: f64,
    /// Screen position of the world origin (S §5.1 `scrollOffset`).
    scroll: Pt,
    /// Aliases drawn in purple with a dashed border (S §7.6).
    pub show_alias_indicator: bool,
    /// Null nodes and their legs drawn (S §12).
    pub show_null_species: bool,
    /// Modifier arcs drawn (S §7.6).
    pub show_modifiers: bool,
    /// The grid drawn behind the diagram (S §7.6). Never in output.
    pub show_grid: bool,
    /// Dragged and new items land on grid points (S §7.6), unless Alt is held.
    pub snap_to_grid: bool,
    /// The colour a selected reaction is drawn in (a preference).
    pub selected_reaction_colour: crate::model::Rgba,
    /// New reactions are Bézier (S §6.5). The reference starts with it on.
    pub default_bezier: bool,
    /// New Bézier reactions get a smooth junction (S §6.5). On at start.
    pub default_smooth: bool,
    undo: UndoHistory,
    gesture: Gesture,
    /// Shift is held: a species resize keeps proportions (S §7.10).
    shift_held: bool,
    /// Alt is held: drag freely, without snapping to the grid (S §7.6).
    alt_held: bool,
    /// What Copy Style took, for Paste Style (S §8.5). Not part of the model,
    /// so undo leaves it alone.
    copied_style: Option<crate::model::Look>,
    /// My Styles: the saved styles kept with the app, not the model (S §8.6).
    /// Not part of the model, so undo leaves them alone; the app loads and
    /// saves them with its preferences.
    my_styles: Vec<crate::model::SavedStyle>,
    tool: Tool,
    /// Species picked so far with an add-reaction tool.
    pending: Pending,
    /// Where the pointer is, in screen coordinates, while it is over the canvas.
    hover: Option<Pt>,
    /// An auto-layout in progress.
    layout: Option<arrange::LayoutState>,
    /// The undo history's version and the selection after the last keyboard
    /// nudge, so the next one can join its undo step.
    nudged: Option<(u64, SelectionState)>,
    /// The undo history's version after the last change to a text's words,
    /// and which text: typing more joins that undo step (S §7.8).
    text_edited: Option<(u64, Id)>,
}

/// The reaction being built with an add-reaction tool (S §6.5).
#[derive(Clone, Debug, Default, PartialEq)]
struct Pending {
    reactants: Vec<Id>,
    products: Vec<Id>,
}

/// The gesture in progress (S §6.1).
#[derive(Clone, Debug, Default)]
enum Gesture {
    #[default]
    Idle,
    Nodes(NodeDrag),
    Junction(JunctionDrag),
    Handle(HandleDrag),
    TextWrap(TextWrapDrag),
    Resize(ResizeDrag),
    SpeciesResize(SpeciesResizeDrag),
    Label(LabelDrag),
    /// The Compartment tool: a box being drawn, corners in world coordinates.
    NewCompartment {
        start: Pt,
        current: Pt,
    },
    /// Corners in screen coordinates.
    RubberBand {
        start: Pt,
        current: Pt,
    },
}

/// Which of a participant's two handles.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Ctrl {
    One,
    Two,
}

/// A stored handle that moves with a drag, and where it started.
#[derive(Clone, Debug)]
struct HandleRef {
    reaction: Id,
    role: Role,
    index: usize,
    ctrl: Ctrl,
    orig: Pt,
}

#[derive(Clone, Debug)]
struct NodeDrag {
    /// World point of the mouse-down.
    start: Pt,
    /// Moved species, junctions, texts and compartments (by their box's
    /// centre), with their original positions.
    species: Vec<(Id, Pt)>,
    junctions: Vec<(Id, Pt)>,
    texts: Vec<(Id, Pt)>,
    compartments: Vec<(Id, Pt)>,
    /// Species moved for themselves, not carried by a compartment: they take
    /// the compartment they end up in (S §7.9).
    reassign: Vec<Id>,
    handles: Vec<HandleRef>,
    /// Smooth reactions whose junction moves: their handles are recomputed
    /// as it goes (S §6.4).
    smooth: Vec<Id>,
    /// Linear reactions with one end moving: their junction keeps its place
    /// along the line, as a fraction `t` (with its original position).
    linear: Vec<(Id, Pt, f64)>,
    /// The model at the start, when the drag moves more than positions
    /// (stored handles, smooth junctions): it is undone as a snapshot (S §9.2).
    before: Option<Box<Model>>,
    /// The point of the item grabbed that snaps to the grid (S §7.6), where
    /// it was: a species' centre, a text's or compartment's top-left corner.
    anchor: Option<Pt>,
    /// The pointer's movement; `delta` is it snapped.
    raw: Pt,
    delta: Pt,
}

#[derive(Clone, Debug)]
struct HandleDrag {
    start: Pt,
    handle: HandleId,
    /// The participant's handles before the grab (possibly automatic).
    from: CtrlState,
    /// Where the dragged handle was once stored.
    orig: Pt,
    /// On a smooth junction, dragging an inner handle moves the others too,
    /// so the drag is undone as a snapshot.
    smooth: bool,
    before: Option<Box<Model>>,
    delta: Pt,
}

/// Dragging a selected compartment's corner resizes it; the opposite corner
/// stays put (S §7.9).
#[derive(Clone, Debug)]
struct ResizeDrag {
    start: Pt,
    id: Id,
    corner: usize,
    orig: crate::model::Frame,
    before: Box<Model>,
    delta: Pt,
}

/// Dragging a selected species' corner resizes every selected species by
/// the same amount, each about its centre; with Shift, each keeps its
/// proportions (S §7.10).
#[derive(Clone, Debug)]
struct SpeciesResizeDrag {
    start: Pt,
    /// The species whose corner is dragged, and which corner.
    anchor: Id,
    corner: usize,
    /// Every species resized, and its size before the drag.
    orig: Vec<(Id, f64, f64)>,
    before: Box<Model>,
    delta: Pt,
}

/// Dragging a name drawn outside its node moves only the name (S §5.11).
#[derive(Clone, Debug)]
struct LabelDrag {
    start: Pt,
    id: Id,
    /// Its offset before the drag.
    orig: Pt,
    before: Box<Model>,
    delta: Pt,
}

/// The grid point nearest `p` (S §7.6).
pub fn snap_point(p: Pt) -> Pt {
    use crate::render::constants::GRID_SPACING;
    let to = |v: f64| (v / GRID_SPACING).round() * GRID_SPACING + 0.0;
    pt(to(p.x), to(p.y))
}

/// A drag's movement: the pointer's, or while snapping, the one that puts the
/// grabbed point `anchor` on the nearest grid point. Nothing moves until the
/// pointer does, so a click never moves what it selects.
fn snap_delta(snap: bool, anchor: Option<Pt>, raw: Pt) -> Pt {
    match anchor {
        Some(a) if snap && raw != Pt::ZERO => snap_point(a + raw) - a,
        _ => raw,
    }
}

/// Which way a corner moves as a box grows: x and y, clockwise from the top
/// left.
const CORNER_SIGNS: [(f64, f64); 4] = [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)];

/// Dragging a text's wrap handle sets the width its lines wrap at (S §7.8).
#[derive(Clone, Debug)]
struct TextWrapDrag {
    start: Pt,
    id: Id,
    /// The wrap width before the drag.
    from: f64,
    /// The text's width when the drag began.
    base: f64,
    before: Box<Model>,
    delta: Pt,
}

#[derive(Clone, Debug)]
struct JunctionDrag {
    start: Pt,
    reaction: Id,
    orig: Pt,
    /// A smooth junction's stored inner handles move with it (S §6.4).
    handles: Vec<HandleRef>,
    before: Option<Box<Model>>,
    /// The pointer's movement; `delta` is it snapped (S §7.6).
    raw: Pt,
    delta: Pt,
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
            show_modifiers: true,
            show_grid: false,
            snap_to_grid: false,
            show_null_species: true,
            selected_reaction_colour: crate::render::constants::SELECTED_REACTION,
            default_bezier: true,
            default_smooth: true,
            undo: UndoHistory::new(),
            gesture: Gesture::Idle,
            shift_held: false,
            alt_held: false,
            copied_style: None,
            my_styles: Vec::new(),
            tool: Tool::Select,
            pending: Pending::default(),
            hover: None,
            layout: None,
            nudged: None,
            text_edited: None,
        }
    }

    pub fn model(&self) -> &Model {
        &self.model
    }

    /// Replaces the model, as Open and New do: the undo history is cleared and
    /// any gesture dropped (S §9.1). Compartments without a box get one,
    /// fitted around their species (S §7.9).
    pub fn set_model(&mut self, model: Model) {
        self.model = model;
        crate::compartments::place_unplaced(&mut self.model);
        self.undo.clear();
        self.gesture = Gesture::Idle;
        self.pending = Pending::default();
        self.layout = None;
    }

    /// Which optional parts of the drawing are shown, for output too.
    pub fn shown(&self) -> Shown {
        Shown {
            alias_indicator: self.show_alias_indicator,
            null_species: self.show_null_species,
            modifiers: self.show_modifiers,
        }
    }

    // ----- Zoom and scroll --------------------------------------------------

    pub fn zoom(&self) -> f64 {
        self.zoom
    }

    pub fn scroll(&self) -> Pt {
        self.scroll
    }

    pub fn world_to_screen(&self, w: Pt) -> Pt {
        world_to_screen(w, self.zoom, self.scroll)
    }

    /// Where the pointer is on the diagram, while it is over the canvas
    /// (S §13.5).
    pub fn pointer_world(&self) -> Option<Pt> {
        self.hover.map(|p| self.screen_to_world(p))
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

    // ----- Drawing ----------------------------------------------------------

    /// Draws the diagram for the screen. `area` is the canvas, filled with the
    /// background first. The rubber band is drawn on top (S §5.2, pass 9).
    pub fn draw(&self, r: &mut dyn Renderer, area: SRect) {
        let opts = DrawOptions {
            zoom: self.zoom,
            scroll: self.scroll,
            background: Some(area),
            for_output: false,
            show_alias_indicator: self.show_alias_indicator,
            show_null_species: self.show_null_species,
            show_modifiers: self.show_modifiers,
            selected_reaction: self.selected_reaction_colour,
            grid: self.show_grid,
        };
        draw_model(&self.model, &opts, r);
        self.draw_overlays(r);
        if let Gesture::NewCompartment { start, current } = &self.gesture {
            let (a, b) = corners(self.world_to_screen(*start), self.world_to_screen(*current));
            r.round_rect(
                SRect {
                    min: spt(a.x as f32, a.y as f32),
                    max: spt(b.x as f32, b.y as f32),
                },
                0.0,
                None,
                Some(Stroke {
                    width: 1.5,
                    color: crate::render::constants::COMPARTMENT_BORDER,
                    dash: Some((6.0, 4.0)),
                    round_cap: false,
                }),
            );
        }
        if let Gesture::RubberBand { start, current } = &self.gesture {
            let (min, max) = corners(*start, *current);
            let mut fill = RUBBER_BAND;
            fill.0[3] = RUBBER_BAND_FILL_ALPHA;
            r.round_rect(
                SRect {
                    min: spt(min.x as f32, min.y as f32),
                    max: spt(max.x as f32, max.y as f32),
                },
                0.0,
                Some(fill),
                Some(Stroke::solid(1.0, RUBBER_BAND)),
            );
        }
    }

    // ----- Hit testing and selection ----------------------------------------

    /// What is under a screen point (S §6.2).
    pub fn target_at(&self, screen: Pt) -> Target {
        hit_test(
            &self.model,
            self.screen_to_world(screen),
            self.zoom,
            self.show_null_species,
        )
    }

    /// True while a drag or rubber band is in progress.
    pub fn is_busy(&self) -> bool {
        !matches!(self.gesture, Gesture::Idle)
    }

    /// True while a Bézier handle is being dragged.
    pub fn is_dragging_handle(&self) -> bool {
        matches!(self.gesture, Gesture::Handle(_))
    }

    pub fn has_selection(&self) -> bool {
        self.model.selected_species().next().is_some()
            || self.model.selected_reactions().next().is_some()
            || self.model.selected_texts().next().is_some()
            || self.model.selected_compartments().next().is_some()
    }

    /// The selected compartments' ids.
    pub fn selected_compartment_ids(&self) -> Vec<Id> {
        self.model
            .selected_compartments()
            .map(|c| c.id.clone())
            .collect()
    }

    /// The selected texts' ids, in z-order.
    pub fn selected_text_ids(&self) -> Vec<Id> {
        self.model.selected_texts().map(|t| t.id.clone()).collect()
    }

    fn selection_state(&self) -> SelectionState {
        (
            self.selection(),
            self.selected_text_ids(),
            self.selected_compartment_ids(),
        )
    }

    /// The selected species and reaction ids, in model order.
    /// How many items are selected, of every kind, null nodes aside.
    pub fn selection_count(&self) -> usize {
        let m = &self.model;
        m.selected_species().filter(|s| !s.is_null_node).count()
            + m.selected_reactions().count()
            + m.selected_compartments().count()
            + m.selected_texts().count()
    }

    pub fn selection(&self) -> Selection {
        (
            self.model
                .selected_species()
                .map(|s| s.id.clone())
                .collect(),
            self.model
                .selected_reactions()
                .map(|r| r.id.clone())
                .collect(),
        )
    }

    /// Selects every species, reaction and text (Ctrl/Cmd + A).
    pub fn select_all(&mut self) -> ViewResponse {
        self.select_only(true, true)
    }

    /// Selects every species, and no reaction.
    pub fn select_all_species(&mut self) -> ViewResponse {
        self.select_only(true, false)
    }

    /// Selects every reaction, and no species.
    pub fn select_all_reactions(&mut self) -> ViewResponse {
        self.select_only(false, true)
    }

    /// Makes the selection every species, every reaction, or both; texts
    /// only with both.
    fn select_only(&mut self, species: bool, reactions: bool) -> ViewResponse {
        let before = self.selection_state();
        for s in self.model.species_mut() {
            s.selected = species;
        }
        for r in self.model.reactions_mut() {
            r.selected = reactions;
        }
        for t in self.model.texts_mut() {
            t.selected = species && reactions;
        }
        for c in &mut self.model.compartments {
            c.selected = species && reactions && c.drawn_frame().is_some();
        }
        ViewResponse {
            repaint: true,
            selection_changed: self.selection_state() != before,
            model_changed: false,
            request: None,
        }
    }

    // ----- Events -----------------------------------------------------------

    /// Handles one input event (S §6). Every change to the selection is
    /// reported in the response (S §6.3).
    pub fn handle(&mut self, event: &ViewEvent) -> ViewResponse {
        // While a layout runs the drawing is moving: only Esc does anything,
        // and it cancels the layout.
        if self.layout.is_some() {
            return match event {
                ViewEvent::Cancel => self.cancel_layout(),
                _ => ViewResponse::default(),
            };
        }
        let before = self.selection_state();
        let mut response = match *event {
            ViewEvent::PointerDown {
                pos,
                button: Button::Primary,
                modifiers,
            } => self.pointer_down(pos, modifiers),
            ViewEvent::PointerDown {
                pos,
                button: Button::Secondary,
                modifiers,
            } => self.context_click(pos, modifiers),
            ViewEvent::PointerMove { pos } => {
                self.hover = Some(pos);
                let mut r = self.pointer_move(pos);
                // The add-reaction guide line follows the pointer.
                r.repaint |= matches!(self.tool, Tool::AddReaction { .. });
                r
            }
            ViewEvent::PointerLeft => {
                self.hover = None;
                ViewResponse {
                    repaint: true,
                    ..ViewResponse::default()
                }
            }
            ViewEvent::DoubleClick { pos } => self.double_click(pos),
            ViewEvent::PointerUp {
                pos,
                button: Button::Primary,
            } => {
                let mut r = self.pointer_move(pos);
                r.merge(self.pointer_up());
                r
            }
            ViewEvent::Cancel => {
                let mut r = self.cancel_gesture();
                self.set_tool(Tool::Select);
                self.model.clear_selection();
                r.repaint = true;
                r
            }
            ViewEvent::FocusLost => self.cancel_gesture(),
            _ => ViewResponse::default(),
        };
        if self.selection_state() != before {
            response.selection_changed = true;
            response.repaint = true;
        }
        response
    }

    fn pointer_down(&mut self, pos: Pt, modifiers: Modifiers) -> ViewResponse {
        if self.is_busy() {
            return ViewResponse::default();
        }
        match self.tool {
            Tool::Select => {}
            Tool::AddSpecies => return self.add_species_at(pos),
            Tool::AddReaction { .. } => return self.pick_for_reaction(pos),
            Tool::AddText => return self.add_text_at(pos),
            Tool::AddCompartment => {
                let at = self.place(self.screen_to_world(pos));
                self.gesture = Gesture::NewCompartment {
                    start: at,
                    current: at,
                };
                return ViewResponse {
                    repaint: true,
                    ..ViewResponse::default()
                };
            }
        }
        let world = self.screen_to_world(pos);
        let target = self.target_at(pos);
        let anchor = self.snap_anchor(&target);
        match target {
            Target::Species(id) => {
                let selected = self.model.find_species(&id).is_some_and(|s| s.selected);
                if modifiers.shift {
                    self.set_species_selected(&id, !selected);
                } else {
                    if !selected {
                        self.model.clear_selection();
                        self.set_species_selected(&id, true);
                    }
                    self.gesture = Gesture::Nodes(self.begin_node_drag(world));
                }
            }
            Target::Text(id) => {
                let selected = self.model.find_text(&id).is_some_and(|t| t.selected);
                if modifiers.shift {
                    self.set_text_selected(&id, !selected);
                } else {
                    if !selected {
                        self.model.clear_selection();
                        self.set_text_selected(&id, true);
                    }
                    self.gesture = Gesture::Nodes(self.begin_node_drag(world));
                }
            }
            Target::SpeciesLabel(id) => {
                let selected = self.model.find_species(&id).is_some_and(|s| s.selected);
                if modifiers.shift {
                    self.set_species_selected(&id, !selected);
                } else {
                    if !selected {
                        self.model.clear_selection();
                        self.set_species_selected(&id, true);
                    }
                    let orig = self.model.find_species(&id).and_then(|s| s.label_offset);
                    if let Some(orig) = orig {
                        self.gesture = Gesture::Label(LabelDrag {
                            start: world,
                            id,
                            orig,
                            before: Box::new(self.model.clone()),
                            delta: Pt::ZERO,
                        });
                    }
                }
            }
            Target::TextWrap(id) => {
                if let Some(drag) = self.begin_text_wrap(world, id) {
                    self.gesture = Gesture::TextWrap(drag);
                }
            }
            Target::Compartment(id) => {
                let selected = self.model.find_compartment(&id).is_some_and(|c| c.selected);
                if modifiers.shift {
                    self.set_compartment_selected(&id, !selected);
                } else {
                    if !selected {
                        self.model.clear_selection();
                        self.set_compartment_selected(&id, true);
                    }
                    self.gesture = Gesture::Nodes(self.begin_node_drag(world));
                }
            }
            Target::SpeciesCorner(id, corner) => {
                self.shift_held = modifiers.shift;
                let orig: Vec<(Id, f64, f64)> = self
                    .model
                    .species()
                    .iter()
                    .filter(|s| s.selected && !s.is_null_node)
                    .map(|s| (s.id.clone(), s.width, s.height))
                    .collect();
                self.gesture = Gesture::SpeciesResize(SpeciesResizeDrag {
                    start: world,
                    anchor: id,
                    corner,
                    orig,
                    before: Box::new(self.model.clone()),
                    delta: Pt::ZERO,
                });
            }
            Target::CompartmentCorner(id, corner) => {
                if let Some(orig) = self.model.find_compartment(&id).and_then(|c| c.frame) {
                    self.gesture = Gesture::Resize(ResizeDrag {
                        start: world,
                        id,
                        corner,
                        orig,
                        before: Box::new(self.model.clone()),
                        delta: Pt::ZERO,
                    });
                }
            }
            Target::Handle(h) => {
                self.gesture = Gesture::Handle(self.begin_handle_drag(world, h));
            }
            Target::Junction(id) => {
                if !modifiers.shift {
                    self.model.clear_selection();
                }
                self.set_reaction_selected(&id, true);
                self.gesture = Gesture::Junction(self.begin_junction_drag(world, id));
            }
            Target::Leg(id) => {
                if modifiers.shift {
                    let selected = self.model.find_reaction(&id).is_some_and(|r| r.selected);
                    self.set_reaction_selected(&id, !selected);
                } else {
                    self.model.clear_selection();
                    self.set_reaction_selected(&id, true);
                }
            }
            Target::Empty => {
                if !modifiers.shift {
                    self.model.clear_selection();
                }
                self.gesture = Gesture::RubberBand {
                    start: pos,
                    current: pos,
                };
            }
        }
        if let Gesture::Nodes(drag) = &mut self.gesture {
            drag.anchor = anchor;
        }
        ViewResponse {
            repaint: true,
            ..ViewResponse::default()
        }
    }

    fn set_species_selected(&mut self, id: &str, selected: bool) {
        if let Some(s) = self.model.find_species_mut(id) {
            s.selected = selected;
        }
    }

    fn set_reaction_selected(&mut self, id: &str, selected: bool) {
        if let Some(r) = self.model.find_reaction_mut(id) {
            r.selected = selected;
        }
    }

    fn set_text_selected(&mut self, id: &str, selected: bool) {
        if let Some(t) = self.model.find_text_mut(id) {
            t.selected = selected;
        }
    }

    fn set_compartment_selected(&mut self, id: &str, selected: bool) {
        if let Some(c) = self.model.find_compartment_mut(id) {
            c.selected = selected;
        }
    }

    fn pointer_move(&mut self, pos: Pt) -> ViewResponse {
        let world = self.screen_to_world(pos);
        let snap = self.snapping();
        let placed = self.place(world);
        match &mut self.gesture {
            Gesture::Idle => return ViewResponse::default(),
            Gesture::Nodes(drag) => {
                drag.raw = world - drag.start;
                drag.delta = snap_delta(snap, drag.anchor, drag.raw);
                let drag = drag.clone();
                self.apply_node_drag(&drag);
            }
            Gesture::Junction(drag) => {
                drag.raw = world - drag.start;
                drag.delta = snap_delta(snap, Some(drag.orig), drag.raw);
                let drag = drag.clone();
                self.apply_junction_drag(&drag);
            }
            Gesture::Handle(drag) => {
                drag.delta = world - drag.start;
                let drag = drag.clone();
                self.apply_handle_drag(&drag);
            }
            Gesture::TextWrap(drag) => {
                drag.delta = world - drag.start;
                let drag = drag.clone();
                self.apply_text_wrap(&drag);
            }
            Gesture::Resize(drag) => {
                drag.delta = world - drag.start;
                let drag = drag.clone();
                self.apply_resize(&drag);
            }
            Gesture::SpeciesResize(drag) => {
                drag.delta = world - drag.start;
                let drag = drag.clone();
                self.apply_species_resize(&drag);
            }
            Gesture::Label(drag) => {
                drag.delta = world - drag.start;
                let drag = drag.clone();
                self.apply_label_drag(&drag);
            }
            Gesture::NewCompartment { current, .. } => *current = placed,
            Gesture::RubberBand { current, .. } => *current = pos,
        }
        ViewResponse {
            repaint: true,
            ..ViewResponse::default()
        }
    }

    fn pointer_up(&mut self) -> ViewResponse {
        let gesture = std::mem::take(&mut self.gesture);
        // Species moved for themselves take the compartment they are now in.
        if let Gesture::Nodes(drag) = &gesture
            && drag.delta != Pt::ZERO
        {
            crate::compartments::reassign(&mut self.model, &drag.reassign);
        }
        // A resize can change a look a style keeps (a name outside: its
        // size), and then the item no longer follows it (S §8.6). Moves, the
        // only gestures not undone as snapshots, change no look.
        crate::styles::drop_stale_links(&mut self.model);
        let cmd = match gesture {
            Gesture::Idle => return ViewResponse::default(),
            Gesture::NewCompartment { start, current } => {
                return self.finish_new_compartment(start, current);
            }
            Gesture::SpeciesResize(drag) if drag.delta == Pt::ZERO => None,
            Gesture::SpeciesResize(drag) => Some(Command::Snapshot {
                desc: "Resize Species",
                before: drag.before,
                after: Box::new(self.model.clone()),
            }),
            Gesture::Label(drag) if drag.delta == Pt::ZERO => None,
            Gesture::Label(drag) => Some(Command::Snapshot {
                desc: "Move Label",
                before: drag.before,
                after: Box::new(self.model.clone()),
            }),
            Gesture::Resize(drag) if drag.delta == Pt::ZERO => None,
            Gesture::Resize(drag) => {
                let now = self.model.find_compartment(&drag.id).and_then(|c| c.frame);
                let mut ids = crate::compartments::members(&self.model, &drag.id);
                for f in [Some(drag.orig), now].into_iter().flatten() {
                    ids.extend(crate::compartments::contents(&self.model, &f).species);
                }
                crate::compartments::reassign(&mut self.model, &ids);
                Some(Command::Snapshot {
                    desc: "Resize Compartment",
                    before: drag.before,
                    after: Box::new(self.model.clone()),
                })
            }
            Gesture::RubberBand { start, current } => {
                self.finish_rubber_band(start, current);
                None
            }
            // A drag that ends where it started records nothing (S §9.1).
            Gesture::Nodes(drag)
                if drag.delta == Pt::ZERO
                    || (drag.species.is_empty()
                        && drag.texts.is_empty()
                        && drag.compartments.is_empty()) =>
            {
                None
            }
            Gesture::Nodes(drag) => Some(match drag.before {
                Some(before) => Command::Snapshot {
                    desc: "Move",
                    before,
                    after: Box::new(self.model.clone()),
                },
                None => {
                    let mut junctions = moved(&drag.junctions, drag.delta);
                    for (id, orig, _) in &drag.linear {
                        if let Some(r) = self.model.find_reaction(id) {
                            junctions.push((id.clone(), *orig, r.junction));
                        }
                    }
                    Command::MoveNodes {
                        species: moved(&drag.species, drag.delta),
                        junctions,
                    }
                }
            }),
            Gesture::Handle(drag) if drag.delta == Pt::ZERO => {
                // A press without a move changes nothing, not even storing
                // the automatic handles.
                self.revert_handle_drag(drag);
                None
            }
            Gesture::Handle(drag) => Some(match drag.before {
                Some(before) => Command::Snapshot {
                    desc: "Move Handle",
                    before,
                    after: Box::new(self.model.clone()),
                },
                None => {
                    let h = &drag.handle;
                    let r = self.model.find_reaction(&h.reaction).expect("dragged");
                    let p = &r.participants(h.role)[h.index];
                    Command::DragCtrlPt {
                        reaction: h.reaction.clone(),
                        role: h.role,
                        index: h.index,
                        from: drag.from,
                        to: CtrlState {
                            ctrl1: p.ctrl1,
                            ctrl2: p.ctrl2,
                            ctrl_pts_set: p.ctrl_pts_set,
                        },
                    }
                }
            }),
            Gesture::TextWrap(drag) if drag.delta == Pt::ZERO => None,
            Gesture::TextWrap(drag) => Some(Command::Snapshot {
                desc: "Wrap Text",
                before: drag.before,
                after: Box::new(self.model.clone()),
            }),
            Gesture::Junction(drag) if drag.delta == Pt::ZERO => None,
            Gesture::Junction(drag) => Some(match drag.before {
                Some(before) => Command::Snapshot {
                    desc: "Move Junction",
                    before,
                    after: Box::new(self.model.clone()),
                },
                None => Command::MoveJunction {
                    reaction: drag.reaction,
                    from: drag.orig,
                    to: drag.orig + drag.delta,
                },
            }),
        };
        let model_changed = cmd.is_some();
        if let Some(cmd) = cmd {
            self.undo.push(cmd);
        }
        ViewResponse {
            repaint: true,
            selection_changed: false,
            model_changed,
            request: None,
        }
    }

    /// Stops the gesture in progress and puts back whatever it moved.
    fn cancel_gesture(&mut self) -> ViewResponse {
        match std::mem::take(&mut self.gesture) {
            Gesture::Idle => return ViewResponse::default(),
            Gesture::Nodes(mut drag) => {
                drag.delta = Pt::ZERO;
                self.apply_node_drag(&drag);
            }
            Gesture::Junction(mut drag) => {
                drag.delta = Pt::ZERO;
                self.apply_junction_drag(&drag);
            }
            Gesture::Handle(drag) => self.revert_handle_drag(drag),
            Gesture::TextWrap(mut drag) => {
                drag.delta = Pt::ZERO;
                self.apply_text_wrap(&drag);
            }
            Gesture::Resize(mut drag) => {
                drag.delta = Pt::ZERO;
                self.apply_resize(&drag);
            }
            Gesture::SpeciesResize(mut drag) => {
                drag.delta = Pt::ZERO;
                self.apply_species_resize(&drag);
            }
            Gesture::Label(mut drag) => {
                drag.delta = Pt::ZERO;
                self.apply_label_drag(&drag);
            }
            Gesture::RubberBand { .. } | Gesture::NewCompartment { .. } => {}
        }
        ViewResponse {
            repaint: true,
            ..ViewResponse::default()
        }
    }

    // ----- Dragging (S §6.4) ------------------------------------------------

    fn begin_node_drag(&self, start: Pt) -> NodeDrag {
        let m = &self.model;
        // A selected compartment carries what is inside its box: species,
        // texts and the compartments nested in it (S §7.9).
        let mut comp_ids: Vec<Id> = Vec::new();
        let mut carried_species: Vec<Id> = Vec::new();
        let mut carried_texts: Vec<Id> = Vec::new();
        let mut carried_junctions: Vec<Id> = Vec::new();
        for c in m.selected_compartments() {
            let Some(f) = c.drawn_frame() else { continue };
            let inside = crate::compartments::contents(m, &f);
            comp_ids.push(c.id.clone());
            comp_ids.extend(inside.compartments);
            carried_species.extend(inside.species);
            carried_texts.extend(inside.texts);
            carried_junctions.extend(inside.junctions);
        }
        let mut compartments: Vec<(Id, Pt)> = Vec::new();
        for id in comp_ids {
            if compartments.iter().any(|(c, _)| *c == id) {
                continue;
            }
            if let Some(f) = m.find_compartment(&id).and_then(|c| c.frame) {
                compartments.push((id, f.center));
            }
        }
        // Locked species never move.
        let mut species: Vec<(Id, Pt)> = Vec::new();
        for s in m.species() {
            let picked = s.selected || carried_species.contains(&s.id);
            if picked && !s.locked {
                species.push((s.id.clone(), s.center));
            }
        }
        let reassign: Vec<Id> = m
            .selected_species()
            .filter(|s| !s.locked && !carried_species.contains(&s.id))
            .map(|s| s.id.clone())
            .collect();
        let is_moved = |id: &str| species.iter().any(|(s, _)| s == id);

        let mut junctions = Vec::new();
        let mut handles = Vec::new();
        let mut smooth = Vec::new();
        let mut linear = Vec::new();
        for r in m.reactions() {
            // The junction follows when every reactant and product moves, or
            // when a dragged compartment carries it (S §7.9).
            let mut ends = r.reactants.iter().chain(&r.products).peekable();
            let junction_moves = (ends.peek().is_some() && ends.all(|p| is_moved(&p.species)))
                || carried_junctions.contains(&r.id);
            if junction_moves {
                junctions.push((r.id.clone(), r.junction));
                if r.junction_smooth && r.mode == ReactionMode::Bezier {
                    smooth.push(r.id.clone());
                }
            } else if r
                .reactants
                .iter()
                .chain(&r.products)
                .any(|p| is_moved(&p.species))
                && let Some(t) = linear_junction_t(m, r)
            {
                linear.push((r.id.clone(), r.junction, t));
            }
            // Stored handles follow the end they belong to (S §5.6.1): the
            // species-side handle is ctrl1 on a reactant and ctrl2 on a product.
            for (role, list) in [(Role::Reactant, &r.reactants), (Role::Product, &r.products)] {
                let (species_side, junction_side) = match role {
                    Role::Reactant => (Ctrl::One, Ctrl::Two),
                    _ => (Ctrl::Two, Ctrl::One),
                };
                for (index, p) in list.iter().enumerate().filter(|(_, p)| p.ctrl_pts_set) {
                    let mut follow = |ctrl: Ctrl| {
                        handles.push(HandleRef {
                            reaction: r.id.clone(),
                            role,
                            index,
                            ctrl,
                            orig: if ctrl == Ctrl::One { p.ctrl1 } else { p.ctrl2 },
                        });
                    };
                    if is_moved(&p.species) {
                        follow(species_side);
                    }
                    if junction_moves {
                        follow(junction_side);
                    }
                }
            }
        }
        let texts: Vec<(Id, Pt)> = m
            .texts()
            .iter()
            .filter(|t| t.selected || carried_texts.contains(&t.id))
            .map(|t| (t.id.clone(), t.position))
            .collect();
        // Undo of a plain move restores positions only; a drag that moves more
        // needs a snapshot taken now (S §9.2). With compartments on the
        // diagram a move may change a species' compartment.
        let framed = m.compartments.iter().any(|c| c.drawn_frame().is_some());
        let before = (!handles.is_empty() || !smooth.is_empty() || !texts.is_empty() || framed)
            .then(|| Box::new(m.clone()));
        NodeDrag {
            start,
            species,
            junctions,
            texts,
            compartments,
            reassign,
            handles,
            smooth,
            linear,
            before,
            anchor: None,
            raw: Pt::ZERO,
            delta: Pt::ZERO,
        }
    }

    fn begin_junction_drag(&self, start: Pt, reaction: Id) -> JunctionDrag {
        let r = self
            .model
            .find_reaction(&reaction)
            .expect("hit reaction exists");
        let smooth = r.junction_smooth && r.mode == ReactionMode::Bezier;
        let mut handles = Vec::new();
        if smooth {
            // The stored inner (junction-side) handles move with the junction.
            for (role, list, ctrl) in [
                (Role::Reactant, &r.reactants, Ctrl::Two),
                (Role::Product, &r.products, Ctrl::One),
            ] {
                for (index, p) in list.iter().enumerate().filter(|(_, p)| p.ctrl_pts_set) {
                    handles.push(HandleRef {
                        reaction: reaction.clone(),
                        role,
                        index,
                        ctrl,
                        orig: if ctrl == Ctrl::One { p.ctrl1 } else { p.ctrl2 },
                    });
                }
            }
        }
        JunctionDrag {
            start,
            orig: r.junction,
            before: smooth.then(|| Box::new(self.model.clone())),
            reaction,
            handles,
            raw: Pt::ZERO,
            delta: Pt::ZERO,
        }
    }

    /// Puts every moved item at *original + delta*, never adding increments,
    /// so rounding errors can't build up (S §6.4).
    fn apply_node_drag(&mut self, drag: &NodeDrag) {
        for (id, orig) in &drag.species {
            if let Some(s) = self.model.find_species_mut(id) {
                s.center = *orig + drag.delta;
            }
        }
        for (id, orig) in &drag.junctions {
            if let Some(r) = self.model.find_reaction_mut(id) {
                r.junction = *orig + drag.delta;
            }
        }
        for (id, orig) in &drag.texts {
            if let Some(t) = self.model.find_text_mut(id) {
                t.position = *orig + drag.delta;
            }
        }
        for (id, orig) in &drag.compartments {
            if let Some(f) = self
                .model
                .find_compartment_mut(id)
                .and_then(|c| c.frame.as_mut())
            {
                f.center = *orig + drag.delta;
            }
        }
        self.move_handles(&drag.handles, drag.delta);
        // A Linear reaction's junction keeps its place along the line, so the
        // modifier arcs that end at it don't slide into a node.
        for (id, orig, t) in &drag.linear {
            let line = self
                .model
                .find_reaction(id)
                .and_then(|r| linear_line(&self.model, r));
            if let (Some((a, b)), Some(r)) = (line, self.model.find_reaction_mut(id)) {
                r.junction = if drag.delta == Pt::ZERO {
                    *orig
                } else {
                    a + (b - a) * *t
                };
            }
        }
        // A smooth junction that moved gets its handles recomputed (S §6.4).
        for id in &drag.smooth {
            apply_smooth_handles(&mut self.model, id);
        }
    }

    fn begin_text_wrap(&self, start: Pt, id: Id) -> Option<TextWrapDrag> {
        let t = self.model.find_text(&id)?;
        Some(TextWrapDrag {
            start,
            from: t.wrap_width,
            base: lay_out(t).width,
            id,
            before: Box::new(self.model.clone()),
            delta: Pt::ZERO,
        })
    }

    /// The text wraps at its width when the drag began plus how far the
    /// handle has moved, never less than [`MIN_WRAP_WIDTH`].
    fn apply_text_wrap(&mut self, drag: &TextWrapDrag) {
        if let Some(t) = self.model.find_text_mut(&drag.id) {
            t.wrap_width = if drag.delta == Pt::ZERO {
                drag.from
            } else {
                (drag.base + drag.delta.x).max(MIN_WRAP_WIDTH)
            };
        }
    }

    /// Whether Alt is held, from the app every frame: while it is, drags
    /// and new items don't snap to the grid; pressing or letting go of it
    /// during a drag takes effect at once (S §7.6).
    pub fn set_alt_held(&mut self, held: bool) -> ViewResponse {
        if self.alt_held == held {
            return ViewResponse::default();
        }
        self.alt_held = held;
        let snap = self.snapping();
        match &mut self.gesture {
            Gesture::Nodes(drag) => {
                drag.delta = snap_delta(snap, drag.anchor, drag.raw);
                let drag = drag.clone();
                self.apply_node_drag(&drag);
            }
            Gesture::Junction(drag) => {
                drag.delta = snap_delta(snap, Some(drag.orig), drag.raw);
                let drag = drag.clone();
                self.apply_junction_drag(&drag);
            }
            _ => return ViewResponse::default(),
        }
        ViewResponse {
            repaint: true,
            ..ViewResponse::default()
        }
    }

    /// True while dragged and new items snap to the grid.
    fn snapping(&self) -> bool {
        self.snap_to_grid && !self.alt_held
    }

    /// Where a new item goes for a click at world point `p`: the nearest grid
    /// point while snapping, otherwise `p` (S §7.6).
    fn place(&self, p: Pt) -> Pt {
        if self.snapping() { snap_point(p) } else { p }
    }

    /// The point of the item at `target` that snaps when it is dragged: a
    /// species' centre (unless it is locked, so doesn't move), a text's
    /// top-left corner, a compartment's box's top-left corner.
    fn snap_anchor(&self, target: &Target) -> Option<Pt> {
        let m = &self.model;
        match target {
            Target::Species(id) => m.find_species(id).filter(|s| !s.locked).map(|s| s.center),
            Target::Text(id) => m.find_text(id).map(|t| t.position),
            Target::Compartment(id) => m
                .find_compartment(id)
                .and_then(|c| c.frame)
                .map(|f| f.min()),
            _ => None,
        }
    }

    /// The dragged corner follows the pointer; the opposite one stays. Never
    /// smaller than the smallest box.
    /// Whether Shift is held, from the app every frame: pressing or letting
    /// go of it during a species resize switches between keeping and not
    /// keeping proportions at once.
    pub fn set_shift_held(&mut self, held: bool) -> ViewResponse {
        if self.shift_held == held {
            return ViewResponse::default();
        }
        self.shift_held = held;
        if let Gesture::SpeciesResize(drag) = &self.gesture {
            let drag = drag.clone();
            self.apply_species_resize(&drag);
            return ViewResponse {
                repaint: true,
                ..ViewResponse::default()
            };
        }
        ViewResponse::default()
    }

    /// Each species of the drag at its size before it, grown by twice the
    /// pointer's movement (each side moves by it, the centre stays); with
    /// Shift, scaled by the factor that the dragged species grows by, so each
    /// keeps its proportions. Never below [`MIN_SPECIES_SIZE`].
    fn apply_species_resize(&mut self, drag: &SpeciesResizeDrag) {
        use crate::render::constants::MIN_SPECIES_SIZE;
        let (sx, sy) = CORNER_SIGNS[drag.corner];
        let grow = pt(2.0 * sx * drag.delta.x, 2.0 * sy * drag.delta.y);
        let Some(&(_, w0, h0)) = drag.orig.iter().find(|(id, _, _)| *id == drag.anchor) else {
            return;
        };
        let factor = ((w0 + grow.x) / w0).max((h0 + grow.y) / h0);
        for (id, w, h) in &drag.orig {
            let (w, h) = (*w, *h);
            let circle = self
                .model
                .find_species(id)
                .is_some_and(|s| s.shape == crate::model::NodeShape::Circle);
            let size = if drag.delta == Pt::ZERO {
                (w, h)
            } else if self.shift_held || circle {
                // A circle always keeps its proportions.
                let f = factor.max(MIN_SPECIES_SIZE / w).max(MIN_SPECIES_SIZE / h);
                (w * f, h * f)
            } else {
                (
                    (w + grow.x).max(MIN_SPECIES_SIZE),
                    (h + grow.y).max(MIN_SPECIES_SIZE),
                )
            };
            // Whole units, so the sizes read cleanly in the panel and the file.
            let size = if drag.delta == Pt::ZERO {
                size
            } else {
                (size.0.round(), size.1.round())
            };
            if let Some(s) = self.model.find_species_mut(id) {
                // Its name, if outside, moves with the edge.
                s.resize(size.0, size.1);
            }
        }
    }

    fn apply_label_drag(&mut self, drag: &LabelDrag) {
        if let Some(s) = self.model.find_species_mut(&drag.id) {
            s.label_offset = Some(drag.orig + drag.delta);
        }
    }

    fn apply_resize(&mut self, drag: &ResizeDrag) {
        use crate::compartments::MIN_SIZE;
        let corners = crate::render::draw::compartment_corners(&drag.orig);
        let fixed = corners[(drag.corner + 2) % 4];
        let moved = corners[drag.corner] + drag.delta;
        // Keep the dragged corner on its own side of the fixed one.
        let sx = if corners[drag.corner].x >= fixed.x {
            1.0
        } else {
            -1.0
        };
        let sy = if corners[drag.corner].y >= fixed.y {
            1.0
        } else {
            -1.0
        };
        let w = ((moved.x - fixed.x) * sx).max(MIN_SIZE);
        let h = ((moved.y - fixed.y) * sy).max(MIN_SIZE);
        let other = fixed + pt(w * sx, h * sy);
        if let Some(c) = self.model.find_compartment_mut(&drag.id) {
            c.frame = Some(if drag.delta == Pt::ZERO {
                drag.orig
            } else {
                crate::model::Frame::from_corners(fixed, other)
            });
        }
    }

    /// The Compartment tool's drag is done: the new compartment, its box as
    /// drawn (or the default size, centred, for a click), with every species
    /// inside now in it. Selected; the tool goes back to Select (S §7.9).
    fn finish_new_compartment(&mut self, start: Pt, end: Pt) -> ViewResponse {
        use crate::compartments::{DEFAULT_SIZE, MIN_SIZE};
        let before = self.model.clone();
        let click =
            (end.x - start.x).abs() < MIN_SIZE / 4.0 && (end.y - start.y).abs() < MIN_SIZE / 4.0;
        let frame = if click {
            crate::model::Frame {
                center: start,
                width: DEFAULT_SIZE.x,
                height: DEFAULT_SIZE.y,
            }
        } else {
            let mut f = crate::model::Frame::from_corners(start, end);
            f.width = f.width.max(MIN_SIZE);
            f.height = f.height.max(MIN_SIZE);
            f
        };
        let id = self.model.add_compartment(frame);
        let inside = crate::compartments::contents(&self.model, &frame).species;
        crate::compartments::reassign(&mut self.model, &inside);
        self.model.clear_selection();
        self.set_compartment_selected(&id, true);
        self.tool = Tool::Select;
        let mut r = self.record("Add Compartment", before);
        r.selection_changed = true;
        r
    }

    fn apply_junction_drag(&mut self, drag: &JunctionDrag) {
        if let Some(r) = self.model.find_reaction_mut(&drag.reaction) {
            r.junction = drag.orig + drag.delta;
        }
        self.move_handles(&drag.handles, drag.delta);
    }

    fn begin_handle_drag(&mut self, start: Pt, handle: HandleId) -> HandleDrag {
        let r = self
            .model
            .find_reaction(&handle.reaction)
            .expect("hit reaction exists");
        let p = &r.participants(handle.role)[handle.index];
        let from = CtrlState {
            ctrl1: p.ctrl1,
            ctrl2: p.ctrl2,
            ctrl_pts_set: p.ctrl_pts_set,
        };
        let smooth = r.junction_smooth;
        let before = smooth.then(|| Box::new(self.model.clone()));
        // Save both automatic handles first, so the other one never jumps.
        store_auto_handles(&mut self.model, &handle.reaction, handle.role, handle.index);
        let r = self.model.find_reaction(&handle.reaction).expect("exists");
        let orig = handle.ctrl.get(&r.participants(handle.role)[handle.index]);
        HandleDrag {
            start,
            orig,
            handle,
            from,
            smooth,
            before,
            delta: Pt::ZERO,
        }
    }

    /// The handle follows the cursor. On a smooth junction, moving an inner
    /// handle swings every other inner handle into line through the junction,
    /// each keeping its distance from it (S §6.4).
    fn apply_handle_drag(&mut self, drag: &HandleDrag) {
        let h = &drag.handle;
        let at = drag.orig + drag.delta;
        let Some(r) = self.model.find_reaction_mut(&h.reaction) else {
            return;
        };
        h.ctrl.set(&mut r.participants_mut(h.role)[h.index], at);
        if !drag.smooth || h.ctrl != HandleCtrl::inner(h.role) {
            return;
        }
        let j = r.junction;
        let d = normalize(at - j);
        let dragged_distance = at.distance(j);
        let mut others = Vec::new();
        for role in [Role::Reactant, Role::Product] {
            for index in 0..r.participants(role).len() {
                if !(role == h.role && index == h.index) {
                    others.push((role, index));
                }
            }
        }
        for (role, index) in others {
            store_auto_handles(&mut self.model, &h.reaction, role, index);
            let r = self.model.find_reaction_mut(&h.reaction).expect("exists");
            let p = &mut r.participants_mut(role)[index];
            let inner = HandleCtrl::inner(role);
            let mut a = inner.get(p).distance(j);
            if a < 0.5 {
                a = dragged_distance;
            }
            let side = if role == h.role { 1.0 } else { -1.0 };
            inner.set(p, j + d * (side * a));
        }
    }

    /// Puts back everything a handle drag changed.
    fn revert_handle_drag(&mut self, drag: HandleDrag) {
        if let Some(before) = drag.before {
            self.model = *before;
            return;
        }
        let h = &drag.handle;
        if let Some(p) = self
            .model
            .find_reaction_mut(&h.reaction)
            .and_then(|r| r.participants_mut(h.role).get_mut(h.index))
        {
            p.ctrl1 = drag.from.ctrl1;
            p.ctrl2 = drag.from.ctrl2;
            p.ctrl_pts_set = drag.from.ctrl_pts_set;
        }
    }

    fn move_handles(&mut self, handles: &[HandleRef], delta: Pt) {
        for h in handles {
            if let Some(p) = self
                .model
                .find_reaction_mut(&h.reaction)
                .and_then(|r| r.participants_mut(h.role).get_mut(h.index))
            {
                match h.ctrl {
                    Ctrl::One => p.ctrl1 = h.orig + delta,
                    Ctrl::Two => p.ctrl2 = h.orig + delta,
                }
            }
        }
    }

    /// Adds every species whose rectangle meets the band, and every reaction
    /// whose stored junction is inside it (S §6.3).
    fn finish_rubber_band(&mut self, start: Pt, current: Pt) {
        let (a, b) = corners(self.screen_to_world(start), self.screen_to_world(current));
        let show_null = self.show_null_species;
        for s in self.model.species_mut() {
            if s.is_null_node && !show_null {
                continue;
            }
            let (hw, hh) = (s.width / 2.0, s.height / 2.0);
            let touches =
                |min: Pt, max: Pt| max.x >= a.x && min.x <= b.x && max.y >= a.y && min.y <= b.y;
            // The node, or its name drawn outside it (S §5.11).
            let on_label =
                crate::render::draw::outside_label_box(s).is_some_and(|(p, q)| touches(p, q));
            if touches(s.center - pt(hw, hh), s.center + pt(hw, hh)) || on_label {
                s.selected = true;
            }
        }
        for r in self.model.reactions_mut() {
            let j = r.junction;
            if (a.x..=b.x).contains(&j.x) && (a.y..=b.y).contains(&j.y) {
                r.selected = true;
            }
        }
        for t in self.model.texts_mut() {
            let (min, max) = text_box(t);
            if max.x >= a.x && min.x <= b.x && max.y >= a.y && min.y <= b.y {
                t.selected = true;
            }
        }
        // A compartment only when the band encloses it: a band drawn inside
        // one selects what is in it.
        let band = crate::model::Frame::from_corners(a, b);
        for c in &mut self.model.compartments {
            if c.drawn_frame().is_some_and(|f| band.encloses(&f)) {
                c.selected = true;
            }
        }
    }

    // ----- Editing commands -------------------------------------------------

    /// Deletes the selected species, with the cascade of S §3.7, then any
    /// selected reactions and texts that are left. One undo step (S §7.2).
    pub fn delete_selection(&mut self) -> ViewResponse {
        if self.is_busy() || self.layout.is_some() || !self.has_selection() {
            return ViewResponse::default();
        }
        let before = self.model.clone();
        let (species, reactions) = self.selection();
        for id in &species {
            self.model.delete_species(id);
        }
        for id in &reactions {
            self.model.delete_reaction(id);
        }
        for id in self.selected_text_ids() {
            self.model.delete_text(&id);
        }
        for id in self.selected_compartment_ids() {
            self.model.delete_compartment(&id);
        }
        self.undo.push(Command::Snapshot {
            desc: "Delete",
            before: Box::new(before),
            after: Box::new(self.model.clone()),
        });
        ViewResponse {
            repaint: true,
            selection_changed: true,
            model_changed: true,
            request: None,
        }
    }

    // ----- Undo -------------------------------------------------------------

    pub fn can_undo(&self) -> bool {
        self.undo.can_undo()
    }

    /// True if the document has changed since it was last saved or loaded
    /// (SPEC-Rust §10.1). Selecting, zooming and scrolling don't count.
    pub fn is_modified(&self) -> bool {
        self.undo.is_modified() || self.layout.is_some()
    }

    /// Notes that the document has just been saved.
    pub fn mark_saved(&mut self) {
        self.undo.mark_saved();
    }

    /// Notes that the document has unsaved changes, though the history is
    /// empty: after recovering an autosave.
    pub fn mark_unsaved(&mut self) {
        self.undo.mark_unsaved();
    }

    /// Selects one species or reaction, from a list rather than the canvas
    /// (the model tree, SPEC-Rust §11.3). With `add`, it is toggled in or out
    /// of the selection instead.
    pub fn select_item(&mut self, item: &Target, add: bool) -> ViewResponse {
        let before = self.selection_state();
        if !add {
            self.model.clear_selection();
        }
        match item {
            Target::Species(id) => {
                if let Some(s) = self.model.find_species_mut(id) {
                    s.selected = !(add && s.selected);
                }
            }
            Target::Junction(id) | Target::Leg(id) => {
                if let Some(r) = self.model.find_reaction_mut(id) {
                    r.selected = !(add && r.selected);
                }
            }
            Target::Text(id) | Target::TextWrap(id) => {
                if let Some(t) = self.model.find_text_mut(id) {
                    t.selected = !(add && t.selected);
                }
            }
            Target::SpeciesCorner(id, _) | Target::SpeciesLabel(id) => {
                if let Some(s) = self.model.find_species_mut(id) {
                    s.selected = !(add && s.selected);
                }
            }
            Target::Compartment(id) | Target::CompartmentCorner(id, _) => {
                if let Some(c) = self.model.find_compartment_mut(id) {
                    c.selected = !(add && c.selected);
                }
            }
            Target::Handle(_) | Target::Empty => {}
        }
        ViewResponse {
            repaint: true,
            selection_changed: self.selection_state() != before,
            ..ViewResponse::default()
        }
    }

    /// Scrolls so the world point `at` is at `screen_centre`.
    pub fn centre_on(&mut self, at: Pt, screen_centre: Pt) {
        self.scroll = screen_centre - at * self.zoom;
    }

    pub fn can_redo(&self) -> bool {
        self.undo.can_redo()
    }

    /// What Undo would undo ("Move"), for the Edit menu.
    pub fn undo_description(&self) -> Option<&'static str> {
        self.undo.undo_description()
    }

    pub fn redo_description(&self) -> Option<&'static str> {
        self.undo.redo_description()
    }

    /// Undoes the last change. Any drag is cancelled and the selection cleared
    /// (S §9.1).
    pub fn undo(&mut self) -> ViewResponse {
        self.undo_or_redo(true)
    }

    pub fn redo(&mut self) -> ViewResponse {
        self.undo_or_redo(false)
    }

    fn undo_or_redo(&mut self, undo: bool) -> ViewResponse {
        self.cancel_layout();
        self.cancel_gesture();
        // Typing after an undo or redo starts a new step.
        self.text_edited = None;
        let done = if undo {
            self.undo.undo(&mut self.model)
        } else {
            self.undo.redo(&mut self.model)
        };
        if done.is_none() {
            return ViewResponse::default();
        }
        self.model.clear_selection();
        ViewResponse {
            repaint: true,
            selection_changed: true,
            model_changed: true,
            request: None,
        }
    }
}

/// `(id, from, to)` for items that moved by `delta` from their originals.
fn moved(items: &[(Id, Pt)], delta: Pt) -> Vec<(Id, Pt, Pt)> {
    items
        .iter()
        .map(|(id, orig)| (id.clone(), *orig, *orig + delta))
        .collect()
}

/// The top-left and bottom-right corners of the box through `a` and `b`.
fn corners(a: Pt, b: Pt) -> (Pt, Pt) {
    (
        pt(a.x.min(b.x), a.y.min(b.y)),
        pt(a.x.max(b.x), a.y.max(b.y)),
    )
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
