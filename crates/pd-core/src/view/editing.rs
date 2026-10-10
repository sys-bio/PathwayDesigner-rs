//! Tools and editing commands (S §6.5, §7.1–§7.3, §13.4).
//!
//! Each command changes the model and records one undo step.

use super::{DiagramView, Gesture, Pending};
use crate::bezier::apply_smooth_handles;
use crate::geometry::{Pt, pt};
use crate::model::{
    ALIAS_OFFSET, DEFAULT_SPECIES_HEIGHT, DEFAULT_SPECIES_WIDTH, Id, Model, Parameter, Participant,
    ReactionMode, RenameError,
};
use crate::render::constants::{
    GUIDE_LINE, LABEL_FONT_SIZE, LABEL_PADDING, NODE_CORNER_RADIUS, PENDING_PRODUCT,
    PENDING_REACTANT, PENDING_RING_OUTSET, PENDING_RING_WIDTH,
};
use crate::render::{Renderer, SRect, Stroke, spt};
use crate::text::text_width;
use crate::undo::Command;
use crate::view::{Modifiers, Request, Target, Tool, ViewResponse};

/// Halfway between a UniUni reaction's two species, or `None` for any other
/// reaction.
fn midpoint(m: &Model, id: &str) -> Option<Pt> {
    let r = m.find_reaction(id).filter(|r| r.is_uni_uni())?;
    let a = m.find_species(&r.reactants[0].species)?.center;
    let b = m.find_species(&r.products[0].species)?.center;
    Some((a + b) / 2.0)
}

/// The value of a parameter made for a new reaction's rate law (S §6.5).
const NEW_PARAMETER_VALUE: &str = "0.1";
/// Radius of the dot at the cursor while picking species, in screen points.
const PICK_DOT_RADIUS_PX: f32 = 5.0;
const GUIDE_DASH_PX: (f32, f32) = (6.0, 4.0);

impl DiagramView {
    // ----- Tools (S §6.1) ---------------------------------------------------

    pub fn tool(&self) -> Tool {
        self.tool
    }

    /// Switches tool. Entering an add tool clears the selection; any reaction
    /// being picked is dropped.
    pub fn set_tool(&mut self, tool: Tool) -> ViewResponse {
        self.cancel_gesture();
        self.tool = tool;
        self.pending = Pending::default();
        let before = self.selection();
        if tool != Tool::Select {
            self.model.clear_selection();
        }
        ViewResponse {
            repaint: true,
            selection_changed: self.selection() != before,
            ..ViewResponse::default()
        }
    }

    /// With an add-reaction tool: how many reactants and products have been
    /// picked so far.
    pub fn picked(&self) -> (usize, usize) {
        (self.pending.reactants.len(), self.pending.products.len())
    }

    /// Records a change made since `before` as one undo step.
    pub(super) fn record(&mut self, desc: &'static str, before: Model) -> ViewResponse {
        self.undo.push(Command::Snapshot {
            desc,
            before: Box::new(before),
            after: Box::new(self.model.clone()),
        });
        ViewResponse {
            repaint: true,
            model_changed: true,
            ..ViewResponse::default()
        }
    }

    /// AddSpecies: a click adds an 80 × 36 species centred on it (S §6.5).
    pub(super) fn add_species_at(&mut self, pos: Pt) -> ViewResponse {
        let before = self.model.clone();
        let world = self.screen_to_world(pos);
        self.model
            .add_species(world, DEFAULT_SPECIES_WIDTH, DEFAULT_SPECIES_HEIGHT);
        self.record("Add Species", before)
    }

    /// AddReaction: a click on a species picks it as the next reactant, then
    /// the next product; a species already picked is ignored; a click on empty
    /// canvas cancels (S §6.5).
    pub(super) fn pick_for_reaction(&mut self, pos: Pt) -> ViewResponse {
        let Tool::AddReaction {
            reactants,
            products,
        } = self.tool
        else {
            return ViewResponse::default();
        };
        let id = match self.target_at(pos) {
            Target::Species(id) => id,
            Target::Empty => return self.set_tool(Tool::Select),
            // Junctions and legs can't be picked.
            _ => return ViewResponse::default(),
        };
        let p = &mut self.pending;
        if p.reactants.contains(&id) || p.products.contains(&id) {
            return ViewResponse::default();
        }
        if p.reactants.len() < reactants {
            p.reactants.push(id);
        } else {
            p.products.push(id);
        }
        if p.products.len() < products {
            return ViewResponse {
                repaint: true,
                ..ViewResponse::default()
            };
        }
        // Everything picked: make the reaction, and stay in the tool.
        let picked = std::mem::take(&mut self.pending);
        self.create_reaction(&picked)
    }

    /// The reaction from the picked species (S §6.5): junction halfway between
    /// the reactant and product centroids, stoichiometry 1, mass-action law
    /// `k<N>*…` with a parameter `k<N> = 0.1` if there is none, and the default
    /// render mode.
    fn create_reaction(&mut self, picked: &Pending) -> ViewResponse {
        let before = self.model.clone();
        let centroid = |ids: &[Id]| {
            let sum = ids
                .iter()
                .filter_map(|id| self.model.find_species(id))
                .fold(Pt::ZERO, |acc, s| acc + s.center);
            sum / ids.len().max(1) as f64
        };
        let junction = (centroid(&picked.reactants) + centroid(&picked.products)) / 2.0;
        let id = self.model.add_reaction(junction);
        // The rate law uses the species each node stands for: an alias's primary.
        let law_ids: Vec<String> = picked
            .reactants
            .iter()
            .filter_map(|r| self.model.find_species(r))
            .map(|s| s.primary_id().to_owned())
            .collect();
        let k = format!("k{}", &id[1..]);
        let law = std::iter::once(k.clone())
            .chain(law_ids)
            .collect::<Vec<_>>()
            .join("*");

        let mode = if self.default_bezier {
            ReactionMode::Bezier
        } else if picked.reactants.len() == 1 && picked.products.len() == 1 {
            ReactionMode::Linear
        } else {
            ReactionMode::Straight
        };
        let r = self.model.find_reaction_mut(&id).expect("just added");
        r.reactants = picked
            .reactants
            .iter()
            .map(|s| Participant::new(s, 1.0))
            .collect();
        r.products = picked
            .products
            .iter()
            .map(|s| Participant::new(s, 1.0))
            .collect();
        r.kinetic_law = law;
        r.mode = mode;
        r.junction_smooth = self.default_smooth && mode == ReactionMode::Bezier;
        if r.junction_smooth {
            apply_smooth_handles(&mut self.model, &id);
        }
        if self.model.find_parameter(&k).is_none() {
            self.model.parameters.push(Parameter {
                variable: k,
                expression: NEW_PARAMETER_VALUE.to_owned(),
            });
        }
        self.record("Add Reaction", before)
    }

    // ----- Reaction shape commands (S §7.4) ---------------------------------

    /// Runs `change` on each selected reaction and records one undo step if
    /// anything changed.
    fn on_selected_reactions(
        &mut self,
        desc: &'static str,
        change: impl Fn(&mut Model, &str),
    ) -> ViewResponse {
        let ids: Vec<Id> = self
            .model
            .selected_reactions()
            .map(|r| r.id.clone())
            .collect();
        self.on_reactions(desc, &ids, change)
    }

    fn on_reactions(
        &mut self,
        desc: &'static str,
        ids: &[Id],
        change: impl Fn(&mut Model, &str),
    ) -> ViewResponse {
        let before = self.model.clone();
        for id in ids {
            change(&mut self.model, id);
        }
        if self.model == before {
            return ViewResponse::default();
        }
        self.record(desc, before)
    }

    /// True if any selected reaction is UniUni (Toggle Linear applies).
    pub fn selection_has_uni_uni(&self) -> bool {
        self.model.selected_reactions().any(|r| r.is_uni_uni())
    }

    /// True if any selected reaction is Bézier (Toggle Smooth applies).
    pub fn selection_has_bezier(&self) -> bool {
        self.model
            .selected_reactions()
            .any(|r| r.mode == ReactionMode::Bezier)
    }

    /// Toggle Linear (S §7.4.1): flips UniUni reactions between Linear and
    /// Straight. Turning Linear off puts the junction halfway between the two
    /// species. Other reactions are left alone.
    pub fn toggle_linear(&mut self) -> ViewResponse {
        self.on_selected_reactions("Toggle Linear", |m, id| {
            let Some(r) = m.find_reaction(id).filter(|r| r.is_uni_uni()) else {
                return;
            };
            let linear = r.mode == ReactionMode::Linear;
            let mid = midpoint(m, id);
            let r = m.find_reaction_mut(id).expect("found above");
            if linear {
                r.mode = ReactionMode::Straight;
                if let Some(mid) = mid {
                    r.junction = mid;
                }
            } else {
                r.mode = ReactionMode::Linear;
            }
        })
    }

    /// Set Bézier (S §7.4.2).
    pub fn set_bezier(&mut self) -> ViewResponse {
        self.on_selected_reactions("Set Bézier", |m, id| {
            if let Some(r) = m.find_reaction_mut(id) {
                r.mode = ReactionMode::Bezier;
            }
        })
    }

    /// Set Straight (S §7.4.3). A UniUni reaction's junction goes halfway
    /// between its two species.
    pub fn set_straight(&mut self) -> ViewResponse {
        self.on_selected_reactions("Set Straight", |m, id| {
            let mid = midpoint(m, id);
            if let Some(r) = m.find_reaction_mut(id) {
                r.mode = ReactionMode::Straight;
                if let Some(mid) = mid {
                    r.junction = mid;
                }
            }
        })
    }

    /// Toggle Smooth Junction (S §7.4.4), Bézier reactions only. Turning it on
    /// computes and stores the smooth handles at once.
    pub fn toggle_smooth(&mut self) -> ViewResponse {
        self.on_selected_reactions("Smooth Junction", |m, id| {
            let Some(r) = m
                .find_reaction_mut(id)
                .filter(|r| r.mode == ReactionMode::Bezier)
            else {
                return;
            };
            r.junction_smooth = !r.junction_smooth;
            if r.junction_smooth {
                apply_smooth_handles(m, id);
            }
        })
    }

    /// Make Nice (S §7.4.5) on the selected reactions.
    pub fn make_nice(&mut self) -> ViewResponse {
        let ids: Vec<Id> = self
            .model
            .selected_reactions()
            .map(|r| r.id.clone())
            .collect();
        self.make_nice_reactions(&ids)
    }

    /// Make Nice on the given reactions ("Adjust Reaction" in the context
    /// menu): Bézier mode, the junction halfway between the reactant and
    /// product centres, and fresh handles: automatic, or smooth if the
    /// junction is smooth.
    pub fn make_nice_reactions(&mut self, ids: &[Id]) -> ViewResponse {
        self.on_reactions("Make Nice", ids, |m, id| {
            let Some(r) = m.find_reaction(id) else {
                return;
            };
            let centre = |list: &[crate::model::Participant]| {
                let pts: Vec<Pt> = list
                    .iter()
                    .filter_map(|p| m.find_species(&p.species))
                    .map(|s| s.center)
                    .collect();
                (!pts.is_empty())
                    .then(|| pts.iter().fold(Pt::ZERO, |a, &p| a + p) / pts.len() as f64)
            };
            let cent_r = centre(&r.reactants).unwrap_or(r.junction);
            let cent_p = centre(&r.products).unwrap_or(cent_r);
            let smooth = r.junction_smooth;
            let r = m.find_reaction_mut(id).expect("found above");
            r.mode = ReactionMode::Bezier;
            r.junction = (cent_r + cent_p) / 2.0;
            if smooth {
                apply_smooth_handles(m, id);
            } else {
                for p in r.reactants.iter_mut().chain(&mut r.products) {
                    p.ctrl_pts_set = false;
                }
            }
        })
    }

    // ----- Double click and context menu ------------------------------------

    /// A double click on a species asks for a new name (S §6.6). Null nodes
    /// can't be renamed.
    pub(super) fn double_click(&mut self, pos: Pt) -> ViewResponse {
        if self.tool != Tool::Select || self.is_busy() {
            return ViewResponse::default();
        }
        match self.target_at(pos) {
            Target::Species(id)
                if !self.model.find_species(&id).is_some_and(|s| s.is_null_node) =>
            {
                ViewResponse {
                    request: Some(Request::Rename(id)),
                    ..ViewResponse::default()
                }
            }
            _ => ViewResponse::default(),
        }
    }

    /// A right click asks for the context menu of what is under it (S §13.4).
    /// An item that isn't selected becomes the only selection, so the menu's
    /// commands act on what the user sees highlighted. Nothing happens on
    /// empty canvas.
    pub(super) fn context_click(&mut self, pos: Pt, _modifiers: Modifiers) -> ViewResponse {
        if self.tool != Tool::Select || self.is_busy() {
            return ViewResponse::default();
        }
        let target = match self.target_at(pos) {
            // A handle stands for its reaction here.
            Target::Handle(h) => Target::Leg(h.reaction),
            t => t,
        };
        match &target {
            Target::Species(id) => {
                if !self.model.find_species(id).is_some_and(|s| s.selected) {
                    self.model.clear_selection();
                    self.set_species_selected(id, true);
                }
            }
            Target::Junction(id) | Target::Leg(id) => {
                if !self.model.find_reaction(id).is_some_and(|r| r.selected) {
                    self.model.clear_selection();
                    self.set_reaction_selected(id, true);
                }
            }
            Target::Handle(_) | Target::Empty => return ViewResponse::default(),
        }
        ViewResponse {
            repaint: true,
            request: Some(Request::ContextMenu(target)),
            ..ViewResponse::default()
        }
    }

    // ----- Rename (S §7.1) --------------------------------------------------

    /// Renames species `id` (its primary, for an alias) to `new`, rewriting
    /// every kinetic law, and widens the node and its aliases to fit the new
    /// label. One undo step.
    pub fn rename(&mut self, id: &str, new: &str) -> Result<ViewResponse, RenameError> {
        let before = self.model.clone();
        self.model.rename_species(id, new)?;
        if before == self.model {
            return Ok(ViewResponse::default());
        }
        let needed = text_width(new, LABEL_FONT_SIZE) + 2.0 * LABEL_PADDING;
        for s in self.model.species_mut() {
            if s.primary_id() == new && s.width < needed {
                s.width = needed;
            }
        }
        Ok(self.record("Rename", before))
    }

    /// The id a rename dialog for `id` starts with: the primary's id.
    pub fn rename_initial(&self, id: &str) -> Option<String> {
        self.model
            .find_species(id)
            .map(|s| s.primary_id().to_owned())
    }

    // ----- Aliases (S §7.3) -------------------------------------------------

    /// Adds an alias of `id`'s primary, 24 units right and down, and selects
    /// only the new alias.
    pub fn create_alias(&mut self, id: &str) -> ViewResponse {
        let Some(node) = self.model.find_species(id) else {
            return ViewResponse::default();
        };
        if node.is_null_node {
            return ViewResponse::default();
        }
        let primary = node.primary_id().to_owned();
        let at = self
            .model
            .find_species(&primary)
            .map_or(node.center, |p| p.center)
            + ALIAS_OFFSET;
        let before = self.model.clone();
        let Some(alias) = self.model.add_alias(&primary, at) else {
            return ViewResponse::default();
        };
        self.model.clear_selection();
        self.set_species_selected(&alias, true);
        let mut r = self.record("Create Alias", before);
        r.selection_changed = true;
        r
    }

    /// Selects only the primary of alias `id` and scrolls so it is centred on
    /// `screen_centre` (the middle of the canvas). Not an edit: no undo step.
    pub fn go_to_primary(&mut self, id: &str, screen_centre: Pt) -> ViewResponse {
        let Some(primary) = self
            .model
            .find_species(id)
            .and_then(|s| s.alias_of.clone())
            .and_then(|p| {
                self.model
                    .find_species(&p)
                    .map(|s| (s.id.clone(), s.center))
            })
        else {
            return ViewResponse::default();
        };
        self.model.clear_selection();
        self.set_species_selected(&primary.0, true);
        self.scroll = screen_centre - primary.1 * self.zoom;
        ViewResponse {
            repaint: true,
            selection_changed: true,
            ..ViewResponse::default()
        }
    }

    // ----- Context menu commands (S §13.4) ----------------------------------

    /// Deletes one species, with the cascade (S §7.2).
    pub fn delete_species(&mut self, id: &str) -> ViewResponse {
        if self.model.find_species(id).is_none() {
            return ViewResponse::default();
        }
        let before = self.model.clone();
        self.model.delete_species(id);
        let mut r = self.record("Delete", before);
        r.selection_changed = true;
        r
    }

    pub fn delete_reaction(&mut self, id: &str) -> ViewResponse {
        let before = self.model.clone();
        if !self.model.delete_reaction(id) {
            return ViewResponse::default();
        }
        let mut r = self.record("Delete", before);
        r.selection_changed = true;
        r
    }

    /// Removes a null node and its leg, keeping the reaction.
    pub fn remove_null_node(&mut self, id: &str) -> ViewResponse {
        let before = self.model.clone();
        if !self.model.remove_null_node(id) {
            return ViewResponse::default();
        }
        let mut r = self.record("Remove Null Node", before);
        r.selection_changed = true;
        r
    }

    /// True if Lock/Unlock would lock: some selected species is unlocked.
    pub fn lock_would_lock(&self) -> bool {
        self.model.selected_species().any(|s| !s.locked)
    }

    /// Locks the selected species, or unlocks them if all are locked
    /// (S §13.4). Locked nodes are not moved by dragging, alignment or layout.
    pub fn toggle_lock(&mut self) -> ViewResponse {
        if self.model.selected_species().next().is_none() {
            return ViewResponse::default();
        }
        let lock = self.lock_would_lock();
        let before = self.model.clone();
        for s in self.model.species_mut().filter(|s| s.selected) {
            s.locked = lock;
        }
        self.record(if lock { "Lock" } else { "Unlock" }, before)
    }

    // ----- Feedback while picking (S §5.9) ----------------------------------

    /// Rings round the species picked so far, a dashed guide from the last
    /// one to the cursor, and a dot at the cursor: green while reactants are
    /// still needed, red after.
    pub(super) fn draw_overlays(&self, r: &mut dyn Renderer) {
        let Tool::AddReaction { reactants, .. } = self.tool else {
            return;
        };
        if !matches!(self.gesture, Gesture::Idle) {
            return;
        }
        let zoom = self.zoom as f32;
        let ring = |r: &mut dyn Renderer, id: &Id, color| {
            let Some(s) = self.model.find_species(id) else {
                return;
            };
            let stroke = Stroke::solid((PENDING_RING_WIDTH * self.zoom) as f32, color);
            let c = self.world_to_screen(s.center);
            if s.is_null_node {
                let radius = (s.width / 2.0 + PENDING_RING_OUTSET) * self.zoom;
                r.circle(
                    spt(c.x as f32, c.y as f32),
                    radius as f32,
                    None,
                    Some(stroke),
                );
            } else {
                let half = pt(
                    s.width / 2.0 + PENDING_RING_OUTSET,
                    s.height / 2.0 + PENDING_RING_OUTSET,
                );
                let (a, b) = (
                    self.world_to_screen(s.center - half),
                    self.world_to_screen(s.center + half),
                );
                r.round_rect(
                    SRect {
                        min: spt(a.x as f32, a.y as f32),
                        max: spt(b.x as f32, b.y as f32),
                    },
                    ((NODE_CORNER_RADIUS + PENDING_RING_OUTSET) as f32) * zoom,
                    None,
                    Some(stroke),
                );
            }
        };
        for id in &self.pending.reactants {
            ring(r, id, PENDING_REACTANT);
        }
        for id in &self.pending.products {
            ring(r, id, PENDING_PRODUCT);
        }
        let Some(cursor) = self.hover else {
            return;
        };
        let cursor = spt(cursor.x as f32, cursor.y as f32);
        let last = self
            .pending
            .products
            .last()
            .or(self.pending.reactants.last());
        if let Some(s) = last.and_then(|id| self.model.find_species(id)) {
            let c = self.world_to_screen(s.center);
            r.polyline(
                &[spt(c.x as f32, c.y as f32), cursor],
                Stroke {
                    width: 1.0,
                    color: GUIDE_LINE,
                    dash: Some(GUIDE_DASH_PX),
                    round_cap: false,
                },
            );
        }
        let color = if self.pending.reactants.len() < reactants {
            PENDING_REACTANT
        } else {
            PENDING_PRODUCT
        };
        r.circle(cursor, PICK_DOT_RADIUS_PX, Some(color), None);
    }
}
