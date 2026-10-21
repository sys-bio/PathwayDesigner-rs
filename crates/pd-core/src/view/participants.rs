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

    /// Add Stem (S §5.13): a straight stem through the junction, along the
    /// way from the reactants to the products, [`STEM_HALF`] each side. A
    /// Linear reaction becomes Straight and a smooth junction stops being
    /// smooth; each line's handle by the junction moves to its end of the
    /// stem. A Linear reaction's junction is put where it was drawn on its
    /// line. One undo step. Nothing for a reaction that has a stem.
    pub fn add_stem(&mut self, id: &str) -> ViewResponse {
        let Some(r) = self.model.find_reaction(id).filter(|r| r.stem.is_none()) else {
            return ViewResponse::default();
        };
        let centre = |list: &[Participant]| {
            let pts: Vec<crate::geometry::Pt> = list
                .iter()
                .filter_map(|p| self.model.find_species(&p.species))
                .map(|s| s.center)
                .collect();
            (!pts.is_empty()).then(|| {
                pts.iter().fold(crate::geometry::Pt::ZERO, |a, &p| a + p) / pts.len() as f64
            })
        };
        let along = match (centre(&r.reactants), centre(&r.products)) {
            (Some(a), Some(b)) if a.distance(b) > 1e-9 => (b - a) / a.distance(b),
            _ => crate::geometry::pt(1.0, 0.0),
        };
        let before = self.model.clone();
        let stem = (along * -STEM_HALF, along * STEM_HALF);
        // A Linear reaction's junction is where it is drawn on its line.
        let drawn = crate::render::draw::effective_junction(&self.model, r);
        let r = self.model.find_reaction_mut(id).expect("found above");
        if r.mode == ReactionMode::Linear {
            r.mode = ReactionMode::Straight;
            r.junction = drawn;
        }
        r.junction_smooth = false;
        r.stem = Some(stem);
        r.stem_straight = true;
        shift_inner_handles(r, stem);
        self.record("Add Stem", before)
    }

    /// The straight stem Straighten Stem would give reaction `id` (S §5.13):
    /// each end as far from the junction as it is, both on the line through
    /// the junction along the way from the reactants' end to the products'.
    /// `None` without a stem, or when it is straight already.
    pub fn straightened_stem(
        &self,
        id: &str,
    ) -> Option<(crate::geometry::Pt, crate::geometry::Pt)> {
        let (a, b) = self.model.find_reaction(id)?.stem?;
        let len = |v: crate::geometry::Pt| v.x.hypot(v.y);
        let (la, lb) = (len(a), len(b));
        let ab = b - a;
        if len(ab) < 1e-9 {
            return None;
        }
        let d = ab / len(ab);
        let straight = (d * -la, d * lb);
        let off = |p: crate::geometry::Pt, q: crate::geometry::Pt| len(p - q) > 1e-6;
        (off(straight.0, a) || off(straight.1, b)).then_some(straight)
    }

    /// Straighten Stem (S §5.13): see [`Self::straightened_stem`]. Each line
    /// with stored handles has its handle by the stem moved as much as its
    /// end. One undo step; nothing for a stem already straight.
    pub fn straighten_stem(&mut self, id: &str) -> ViewResponse {
        let Some(straight) = self.straightened_stem(id) else {
            return ViewResponse::default();
        };
        let before = self.model.clone();
        let r = self.model.find_reaction_mut(id).expect("has a stem");
        let old = r.stem.expect("has a stem");
        r.stem = Some(straight);
        shift_inner_handles(r, (straight.0 - old.0, straight.1 - old.1));
        self.record("Straighten Stem", before)
    }

    /// Straight stem (S §5.13): keeps the selected reactions' stems
    /// straight, straightening a bent one first ([`Self::straightened_stem`]),
    /// or lets their ends move freely. One undo step.
    pub fn set_stem_straight(&mut self, straight: bool) -> ViewResponse {
        let before = self.model.clone();
        let ids: Vec<Id> = self
            .model
            .selected_reactions()
            .filter(|r| r.stem.is_some())
            .map(|r| r.id.clone())
            .collect();
        for id in ids {
            let bent = straight.then(|| self.straightened_stem(&id)).flatten();
            let r = self.model.find_reaction_mut(&id).expect("listed above");
            if let Some(s) = bent {
                let old = r.stem.expect("has a stem");
                r.stem = Some(s);
                shift_inner_handles(r, (s.0 - old.0, s.1 - old.1));
            }
            r.stem_straight = straight;
        }
        self.record_if_changed("Straight Stem", before)
    }

    /// Whether the selected reactions with a stem keep it straight, if they
    /// all agree; `None` also when none has a stem.
    pub fn selected_stems_straight(&self) -> Option<bool> {
        let mut s = self
            .model
            .selected_reactions()
            .filter(|r| r.stem.is_some())
            .map(|r| r.stem_straight);
        let first = s.next()?;
        s.all(|v| v == first).then_some(first)
    }

    /// Remove Stem (S §5.13): the lines meet at the junction again, each
    /// handle by the stem moving back with it. One undo step.
    pub fn remove_stem(&mut self, id: &str) -> ViewResponse {
        let Some(stem) = self.model.find_reaction(id).and_then(|r| r.stem) else {
            return ViewResponse::default();
        };
        let before = self.model.clone();
        let r = self.model.find_reaction_mut(id).expect("found above");
        r.stem = None;
        r.stem_straight = false;
        shift_inner_handles(r, (stem.0 * -1.0, stem.1 * -1.0));
        self.record("Remove Stem", before)
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

/// How far Add Stem puts each end of a new stem from the junction.
pub const STEM_HALF: f64 = 30.0;

/// Moves the stored handle by the junction of each reactant by `by.0` and of
/// each product by `by.1`.
fn shift_inner_handles(
    r: &mut crate::model::Reaction,
    by: (crate::geometry::Pt, crate::geometry::Pt),
) {
    use crate::bezier::Ctrl;
    for (role, d) in [(Role::Reactant, by.0), (Role::Product, by.1)] {
        let inner = Ctrl::inner(role);
        for p in r
            .participants_mut(role)
            .iter_mut()
            // A routed line's is by its first corner point (S §5.6.5).
            .filter(|p| p.ctrl_pts_set && p.points.is_empty())
        {
            let at = inner.get(p);
            inner.set(p, at + d);
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
