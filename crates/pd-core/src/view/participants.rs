//! Changing an existing reaction's reactants and products (S §7.11): Add
//! Reactant… and Add Product… pick species with the mouse, one undo step
//! each; Remove from Reaction takes one out.

use super::{DiagramView, Pending};
use crate::bezier::apply_smooth_handles;
use crate::model::{Id, Model, Participant, ReactionMode, Role};
use crate::passes::add_null_nodes;
use crate::view::properties::sync_modifiers;
use crate::view::{Target, ViewResponse};

impl DiagramView {
    /// Add Reactant… or Add Product… on reaction `id` (S §7.11): from now
    /// on, each click on a species adds it in `role`, until Esc, a click on
    /// empty canvas or another tool.
    pub fn begin_adding(&mut self, id: &str, role: Role) -> ViewResponse {
        if role == Role::Modifier || self.model.find_reaction(id).is_none() {
            return ViewResponse::default();
        }
        self.set_tool(crate::view::Tool::Select);
        self.pending = Pending {
            adding_to: Some((id.to_owned(), role)),
            ..Pending::default()
        };
        ViewResponse {
            repaint: true,
            ..ViewResponse::default()
        }
    }

    /// The reaction, and the side, clicks are adding species to.
    pub fn adding_to(&self) -> Option<(&str, Role)> {
        self.pending
            .adding_to
            .as_ref()
            .map(|(id, role)| (id.as_str(), *role))
    }

    /// A click while adding: a species joins the reaction; empty canvas
    /// stops; anything else is ignored.
    pub(super) fn pick_for_adding(&mut self, pos: crate::geometry::Pt) -> ViewResponse {
        let Some((reaction, role)) = self.pending.adding_to.clone() else {
            return ViewResponse::default();
        };
        match self.target_at(pos) {
            Target::Species(s) | Target::SpeciesLabel(s) => {
                self.add_participant(&reaction, role, &s)
            }
            Target::Empty => self.set_tool(crate::view::Tool::Select),
            _ => ViewResponse::default(),
        }
    }

    /// Adds species `species` to reaction `id` as a reactant or a product,
    /// stoichiometry 1 (S §7.11). Nothing happens for a null node or a
    /// species already a reactant or product of it; a modifier becomes a
    /// participant. A side that only had a null node loses it. One undo step.
    pub fn add_participant(&mut self, id: &str, role: Role, species: &str) -> ViewResponse {
        let Some(r) = self.model.find_reaction(id) else {
            return ViewResponse::default();
        };
        let ok = role != Role::Modifier
            && self
                .model
                .find_species(species)
                .is_some_and(|s| !s.is_null_node)
            && !r
                .reactants
                .iter()
                .chain(&r.products)
                .any(|p| p.species == species);
        if !ok {
            return ViewResponse::default();
        }
        let before = self.model.clone();
        let auto = auto_law(&self.model, id);
        // The null node a side had for "nothing" goes.
        let nulls: Vec<Id> = r
            .participants(role)
            .iter()
            .filter(|p| self.is_null(&p.species))
            .map(|p| p.species.clone())
            .collect();
        for n in nulls {
            self.model.remove_null_node(&n);
        }
        let r = self.model.find_reaction_mut(id).expect("found above");
        r.modifiers.retain(|p| p.species != species);
        r.participants_mut(role)
            .push(Participant::new(species, 1.0));
        self.after_change(id, auto);
        self.record(
            if role == Role::Reactant {
                "Add Reactant"
            } else {
                "Add Product"
            },
            before,
        )
    }

    /// Whether `species` can be taken out of reaction `id`: it is one of its
    /// reactants or products, and not the only species left in it (null
    /// nodes aside).
    pub fn can_remove_participant(&self, id: &str, species: &str) -> bool {
        let Some(r) = self.model.find_reaction(id) else {
            return false;
        };
        let real: Vec<&str> = r
            .reactants
            .iter()
            .chain(&r.products)
            .map(|p| p.species.as_str())
            .filter(|s| !self.is_null(s))
            .collect();
        real.contains(&species) && real.len() > 1
    }

    /// Remove from Reaction (S §7.11): takes species `species` out of
    /// reaction `id`. A side left empty gets a null node. One undo step.
    pub fn remove_participant(&mut self, id: &str, species: &str) -> ViewResponse {
        if !self.can_remove_participant(id, species) {
            return ViewResponse::default();
        }
        let before = self.model.clone();
        let auto = auto_law(&self.model, id);
        let mut r = self.model.find_reaction(id).expect("checked").clone();
        r.reactants.retain(|p| p.species != species);
        r.products.retain(|p| p.species != species);
        add_null_nodes(&mut self.model, &mut r, None, None);
        *self.model.find_reaction_mut(id).expect("checked") = r;
        self.after_change(id, auto);
        self.record("Remove from Reaction", before)
    }

    fn is_null(&self, species: &str) -> bool {
        self.model
            .find_species(species)
            .is_some_and(|s| s.is_null_node)
    }

    /// After the participants changed: a rate law that was the automatic
    /// mass-action one (`auto`) follows the reactants; modifiers follow the
    /// law; a smooth junction gets its handles again.
    fn after_change(&mut self, id: &str, auto: bool) {
        if auto {
            let law = mass_action(&self.model, id);
            if let Some(r) = self.model.find_reaction_mut(id) {
                r.kinetic_law = law;
            }
        }
        sync_modifiers(&mut self.model, id);
        let smooth = self
            .model
            .find_reaction(id)
            .is_some_and(|r| r.mode == ReactionMode::Bezier && r.junction_smooth);
        if smooth {
            apply_smooth_handles(&mut self.model, id);
        }
    }
}

/// The mass-action law a new reaction gets (S §6.5), for reaction `id` as it
/// is now: `k<N>` times each reactant (an alias as its primary; null nodes
/// left out).
pub(super) fn mass_action(model: &Model, id: &str) -> String {
    let Some(r) = model.find_reaction(id) else {
        return String::new();
    };
    let k = format!("k{}", id.get(1..).unwrap_or(id));
    std::iter::once(k)
        .chain(
            r.reactants
                .iter()
                .filter_map(|p| model.find_species(&p.species))
                .filter(|s| !s.is_null_node)
                .map(|s| s.primary_id().to_owned()),
        )
        .collect::<Vec<_>>()
        .join("*")
}

/// True if reaction `id`'s law is still the one it was made with.
fn auto_law(model: &Model, id: &str) -> bool {
    model
        .find_reaction(id)
        .is_some_and(|r| r.kinetic_law == mass_action(model, id))
}
