//! The domain model (S §3, §4), as plain Rust data.
//!
//! Everything refers to everything else by id, never by reference (SPEC-Rust
//! §3.1), so a cloned `Model` is a complete, valid snapshot for undo. All numbers
//! are `f64` (SPEC-Rust §3). The model knows nothing about pixels or egui.

use std::collections::HashMap;

use crate::geometry::Pt;

/// A species, reaction, parameter or compartment identifier. Kept exactly as
/// typed or imported, letter case included (S §4).
pub type Id = String;

/// Default species size (S §17.1).
pub const DEFAULT_SPECIES_WIDTH: f64 = 80.0;
pub const DEFAULT_SPECIES_HEIGHT: f64 = 36.0;
/// Null node size (S §17.1).
pub const NULL_NODE_SIZE: f64 = 28.0;
/// Offset of a new alias from its primary (S §7.3).
pub const ALIAS_OFFSET: Pt = Pt { x: 24.0, y: 24.0 };
/// The compartment id that is never shown as a real compartment (S §3.5).
pub const DEFAULT_COMPARTMENT: &str = "defaultCompartment";

/// A colour as straight (not premultiplied) RGBA. All zero, `#00000000`, is
/// the "use the default" sentinel (S §3.6).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Rgba(pub [u8; 4]);

impl Rgba {
    /// The "use the default" sentinel.
    pub const DEFAULT: Rgba = Rgba([0, 0, 0, 0]);

    pub const fn rgb(r: u8, g: u8, b: u8) -> Rgba {
        Rgba([r, g, b, 255])
    }

    pub fn is_default(self) -> bool {
        self == Rgba::DEFAULT
    }
}

/// A style override (S §3.6). Every field is applied on its own: a colour of
/// [`Rgba::DEFAULT`], or a width or font size of 0, means "use the default".
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VisualStyle {
    /// `false` means the whole record is ignored.
    pub has_custom_style: bool,
    pub fill_color: Rgba,
    pub border_color: Rgba,
    pub label_color: Rgba,
    pub border_width: f64,
    pub line_color: Rgba,
    pub line_width: f64,
    pub font_size: f64,
    pub junction_color: Rgba,
    /// A display toggle. Changing it does not make the style custom.
    pub junction_visible: bool,
}

impl Default for VisualStyle {
    fn default() -> Self {
        Self {
            has_custom_style: false,
            fill_color: Rgba::DEFAULT,
            border_color: Rgba::DEFAULT,
            label_color: Rgba::DEFAULT,
            border_width: 0.0,
            line_color: Rgba::DEFAULT,
            line_width: 0.0,
            font_size: 0.0,
            junction_color: Rgba::DEFAULT,
            junction_visible: true,
        }
    }
}

/// A species node (S §3.2). An alias is a second drawing of a primary species.
#[derive(Clone, Debug, PartialEq)]
pub struct SpeciesNode {
    pub id: Id,
    pub center: Pt,
    pub width: f64,
    pub height: f64,
    pub initial_value: f64,
    pub is_boundary: bool,
    pub is_constant: bool,
    /// Compartment id. Empty means the default compartment.
    pub compartment: String,
    /// The primary's id, when this node is an alias. Always a primary, never
    /// another alias.
    pub alias_of: Option<Id>,
    pub is_null_node: bool,
    /// Locked nodes are not moved by auto-layout.
    pub locked: bool,
    /// Transient: not saved.
    pub selected: bool,
    pub style: VisualStyle,
}

impl SpeciesNode {
    /// A plain species of the default size.
    pub fn new(id: impl Into<Id>, center: Pt) -> Self {
        Self {
            id: id.into(),
            center,
            width: DEFAULT_SPECIES_WIDTH,
            height: DEFAULT_SPECIES_HEIGHT,
            initial_value: 0.0,
            is_boundary: false,
            is_constant: false,
            compartment: String::new(),
            alias_of: None,
            is_null_node: false,
            locked: false,
            selected: false,
            style: VisualStyle::default(),
        }
    }

    pub fn is_alias(&self) -> bool {
        self.alias_of.is_some()
    }

    /// The id of the species this node stands for: its primary's id for an
    /// alias, its own otherwise. This is also its label (S §3.2).
    pub fn primary_id(&self) -> &str {
        self.alias_of.as_deref().unwrap_or(&self.id)
    }
}

/// One species on one side of a reaction (S §3.3).
#[derive(Clone, Debug, PartialEq)]
pub struct Participant {
    /// May be an alias; the leg is then drawn to the alias.
    pub species: Id,
    pub stoichiometry: f64,
    /// Bézier handles, used only when `ctrl_pts_set` (S §5.6.1).
    pub ctrl1: Pt,
    pub ctrl2: Pt,
    /// `false`: handles are computed when drawing and never saved.
    pub ctrl_pts_set: bool,
}

impl Participant {
    pub fn new(species: impl Into<Id>, stoichiometry: f64) -> Self {
        Self {
            species: species.into(),
            stoichiometry,
            ctrl1: Pt::ZERO,
            ctrl2: Pt::ZERO,
            ctrl_pts_set: false,
        }
    }
}

/// How a reaction is drawn (S §5.5). One value instead of the reference's
/// `isLinear`/`isBezier` pair, so both can never be set (SPEC-Rust §3.2).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ReactionMode {
    #[default]
    Straight,
    /// One straight line; only for UniUni reactions, drawn as Straight otherwise.
    Linear,
    Bezier,
}

/// Which list of a reaction a participant is in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Reactant,
    Product,
    Modifier,
}

/// A reaction (S §3.4).
#[derive(Clone, Debug, PartialEq)]
pub struct Reaction {
    pub id: Id,
    pub junction: Pt,
    /// Order matters: it sets the fan index of automatic handles (S §5.6.2).
    pub reactants: Vec<Participant>,
    pub products: Vec<Participant>,
    /// Only `species` is meaningful.
    pub modifiers: Vec<Participant>,
    /// Infix expression (S §8.3). Kept even if it doesn't parse.
    pub kinetic_law: String,
    pub is_reversible: bool,
    pub mode: ReactionMode,
    /// Only meaningful for Bézier reactions.
    pub junction_smooth: bool,
    /// Transient: not saved.
    pub selected: bool,
    pub style: VisualStyle,
}

impl Reaction {
    pub fn new(id: impl Into<Id>, junction: Pt) -> Self {
        Self {
            id: id.into(),
            junction,
            reactants: Vec::new(),
            products: Vec::new(),
            modifiers: Vec::new(),
            kinetic_law: String::new(),
            is_reversible: false,
            mode: ReactionMode::Straight,
            junction_smooth: false,
            selected: false,
            style: VisualStyle::default(),
        }
    }

    pub fn participants(&self, role: Role) -> &[Participant] {
        match role {
            Role::Reactant => &self.reactants,
            Role::Product => &self.products,
            Role::Modifier => &self.modifiers,
        }
    }

    pub fn participants_mut(&mut self, role: Role) -> &mut Vec<Participant> {
        match role {
            Role::Reactant => &mut self.reactants,
            Role::Product => &mut self.products,
            Role::Modifier => &mut self.modifiers,
        }
    }

    /// One reactant and one product.
    pub fn is_uni_uni(&self) -> bool {
        self.reactants.len() == 1 && self.products.len() == 1
    }

    /// True if `species` is a reactant or product (not just a modifier).
    pub fn uses_as_reactant_or_product(&self, species: &str) -> bool {
        self.reactants
            .iter()
            .chain(&self.products)
            .any(|p| p.species == species)
    }
}

/// A compartment (S §3.5). Stored and saved, not yet drawn.
#[derive(Clone, Debug, PartialEq)]
pub struct Compartment {
    pub id: Id,
    pub size: f64,
    pub dimensions: u32,
    pub style: VisualStyle,
}

impl Compartment {
    pub fn new(id: impl Into<Id>) -> Self {
        Self {
            id: id.into(),
            size: 1.0,
            dimensions: 3,
            style: VisualStyle::default(),
        }
    }
}

/// `variable = expression` (S §3.5).
#[derive(Clone, Debug, PartialEq)]
pub struct Parameter {
    pub variable: String,
    pub expression: String,
}

/// `variable := expression` (S §3.5).
#[derive(Clone, Debug, PartialEq)]
pub struct AssignmentRule {
    pub variable: String,
    pub expression: String,
}

/// Why a rename was refused (S §4).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RenameError {
    /// No species has the old id.
    NotFound,
    /// Null nodes cannot be renamed (S §7.1).
    NullNode,
    /// Empty, or not a valid SBML SId (`[A-Za-z_][A-Za-z0-9_]*`).
    InvalidId,
    /// Already used by another species, reaction, parameter or compartment.
    InUse,
}

impl std::fmt::Display for RenameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            RenameError::NotFound => "there is no species with that id",
            RenameError::NullNode => "null nodes cannot be renamed",
            RenameError::InvalidId => {
                "an id must start with a letter or _, followed by letters, digits or _"
            }
            RenameError::InUse => "that id is already used",
        })
    }
}

impl std::error::Error for RenameError {}

/// A species or reaction id that is already in the model.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DuplicateId(pub Id);

/// The whole document (S §3.1).
///
/// Species and reactions live in `Vec`s, because list order is z-order. The
/// id indexes are for looking things up, never for looping over: loops follow
/// `Vec` order so results are the same on every run (SPEC-Rust §3.1).
#[derive(Clone, Debug, PartialEq)]
pub struct Model {
    pub model_name: String,
    pub compartments: Vec<Compartment>,
    pub parameters: Vec<Parameter>,
    pub assignment_rules: Vec<AssignmentRule>,
    species: Vec<SpeciesNode>,
    reactions: Vec<Reaction>,
    species_index: HashMap<Id, usize>,
    reaction_index: HashMap<Id, usize>,
    /// Next numeric suffix for generated ids (S §4).
    next_id: u64,
}

impl Default for Model {
    fn default() -> Self {
        Self {
            model_name: String::new(),
            compartments: Vec::new(),
            parameters: Vec::new(),
            assignment_rules: Vec::new(),
            species: Vec::new(),
            reactions: Vec::new(),
            species_index: HashMap::new(),
            reaction_index: HashMap::new(),
            next_id: 1,
        }
    }
}

impl Model {
    pub fn new() -> Self {
        Self::default()
    }

    // ----- Reading ---------------------------------------------------------

    /// All species, in z-order (later ones are drawn on top).
    pub fn species(&self) -> &[SpeciesNode] {
        &self.species
    }

    pub fn reactions(&self) -> &[Reaction] {
        &self.reactions
    }

    /// The species with this exact id (case-sensitive).
    pub fn find_species(&self, id: &str) -> Option<&SpeciesNode> {
        self.species_index.get(id).map(|&i| &self.species[i])
    }

    /// The species with this id, for editing. Don't change its `id` here: use
    /// [`Model::rename_species`], which keeps every reference in step.
    pub fn find_species_mut(&mut self, id: &str) -> Option<&mut SpeciesNode> {
        self.species_index.get(id).map(|&i| &mut self.species[i])
    }

    pub fn find_reaction(&self, id: &str) -> Option<&Reaction> {
        self.reaction_index.get(id).map(|&i| &self.reactions[i])
    }

    /// The reaction with this id, for editing. Don't change its `id` here.
    pub fn find_reaction_mut(&mut self, id: &str) -> Option<&mut Reaction> {
        self.reaction_index.get(id).map(|&i| &mut self.reactions[i])
    }

    /// Every species, for editing in place. Don't change ids here.
    pub fn species_mut(&mut self) -> impl Iterator<Item = &mut SpeciesNode> {
        self.species.iter_mut()
    }

    /// Every reaction, for editing in place. Don't change ids here.
    pub fn reactions_mut(&mut self) -> impl Iterator<Item = &mut Reaction> {
        self.reactions.iter_mut()
    }

    /// The parameter called `name`, compared ignoring case as the reference does.
    pub fn find_parameter(&self, name: &str) -> Option<&Parameter> {
        self.parameters
            .iter()
            .find(|p| p.variable.eq_ignore_ascii_case(name))
    }

    /// The compartment called `name`, compared ignoring case as the reference does.
    pub fn find_compartment(&self, name: &str) -> Option<&Compartment> {
        self.compartments
            .iter()
            .find(|c| c.id.eq_ignore_ascii_case(name))
    }

    /// True if any compartment is not `defaultCompartment` (ignoring case).
    pub fn has_non_default_compartments(&self) -> bool {
        self.compartments
            .iter()
            .any(|c| !c.id.eq_ignore_ascii_case(DEFAULT_COMPARTMENT))
    }

    /// The aliases of the primary `id`, in z-order.
    pub fn aliases_of<'a>(&'a self, id: &'a str) -> impl Iterator<Item = &'a SpeciesNode> + 'a {
        self.species
            .iter()
            .filter(move |s| s.alias_of.as_deref() == Some(id))
    }

    pub fn selected_species(&self) -> impl Iterator<Item = &SpeciesNode> {
        self.species.iter().filter(|s| s.selected)
    }

    pub fn selected_reactions(&self) -> impl Iterator<Item = &Reaction> {
        self.reactions.iter().filter(|r| r.selected)
    }

    /// The next numeric suffix for a generated id.
    pub fn next_id(&self) -> u64 {
        self.next_id
    }

    // ----- Building --------------------------------------------------------

    /// Adds a species built elsewhere (by a loader or importer) on top of the
    /// others. Fails if a species already has its id.
    pub fn push_species(&mut self, node: SpeciesNode) -> Result<(), DuplicateId> {
        if self.species_index.contains_key(&node.id) {
            return Err(DuplicateId(node.id));
        }
        self.species_index
            .insert(node.id.clone(), self.species.len());
        self.species.push(node);
        Ok(())
    }

    /// Adds a reaction built elsewhere. Fails if a reaction already has its id.
    pub fn push_reaction(&mut self, reaction: Reaction) -> Result<(), DuplicateId> {
        if self.reaction_index.contains_key(&reaction.id) {
            return Err(DuplicateId(reaction.id));
        }
        self.reaction_index
            .insert(reaction.id.clone(), self.reactions.len());
        self.reactions.push(reaction);
        Ok(())
    }

    /// A new species with a generated id `S<n>`, `width × height`, on top.
    pub fn add_species(&mut self, center: Pt, width: f64, height: f64) -> Id {
        let id = self.generate_id('S');
        let mut node = SpeciesNode::new(id.clone(), center);
        node.width = width;
        node.height = height;
        self.push_species(node).expect("generated ids are unused");
        id
    }

    /// A new alias of `primary` at `center`, copying the primary's size. If
    /// `primary` is itself an alias, the new alias points at its primary. Returns
    /// `None` if there is no such species.
    pub fn add_alias(&mut self, primary: &str, center: Pt) -> Option<Id> {
        let source = self.find_species(primary)?;
        let primary_id = source.primary_id().to_owned();
        let (width, height) = match self.find_species(&primary_id) {
            Some(p) => (p.width, p.height),
            None => (source.width, source.height),
        };
        let id = self.generate_id('S');
        let mut node = SpeciesNode::new(id.clone(), center);
        node.width = width;
        node.height = height;
        node.alias_of = Some(primary_id);
        self.push_species(node).expect("generated ids are unused");
        Some(id)
    }

    /// A new, empty reaction with a generated id `R<n>`.
    pub fn add_reaction(&mut self, junction: Pt) -> Id {
        let id = self.generate_id('R');
        self.push_reaction(Reaction::new(id.clone(), junction))
            .expect("generated ids are unused");
        id
    }

    /// `S<n>` or `R<n>` from the shared counter (S §4). Skips any id already
    /// taken, so a generated id never clashes with one typed by the user.
    fn generate_id(&mut self, prefix: char) -> Id {
        loop {
            let id = format!("{prefix}{}", self.next_id);
            self.next_id += 1;
            if !self.is_id_used(&id) {
                return id;
            }
        }
    }

    /// Sets the id counter to 1 + the largest numeric suffix of any species or
    /// reaction id (S §4). Call after a load, an import or an undo restore.
    pub fn recompute_id_counter(&mut self) {
        let max = self
            .species
            .iter()
            .map(|s| &s.id)
            .chain(self.reactions.iter().map(|r| &r.id))
            .filter_map(|id| numeric_suffix(id))
            .max()
            .unwrap_or(0);
        self.next_id = max + 1;
    }

    // ----- Changing --------------------------------------------------------

    /// Removes everything, as for New.
    pub fn clear(&mut self) {
        *self = Model::default();
    }

    pub fn clear_selection(&mut self) {
        for s in &mut self.species {
            s.selected = false;
        }
        for r in &mut self.reactions {
            r.selected = false;
        }
    }

    /// Deletes a species with the cascade of S §3.7:
    /// - a primary takes all of its aliases with it;
    /// - every reaction that has a removed node as a reactant or product is
    ///   deleted;
    /// - removed nodes are taken out of every remaining modifier list (D-6).
    ///
    /// Returns the ids of the deleted reactions. Nothing happens if there is no
    /// such species.
    pub fn delete_species(&mut self, id: &str) -> Vec<Id> {
        let Some(node) = self.find_species(id) else {
            return Vec::new();
        };
        let mut removed: Vec<Id> = vec![node.id.clone()];
        if !node.is_alias() {
            removed.extend(self.aliases_of(id).map(|a| a.id.clone()));
        }
        let is_removed = |s: &str| removed.iter().any(|r| r == s);

        let mut deleted_reactions = Vec::new();
        self.reactions.retain(|r| {
            let keep = !r
                .reactants
                .iter()
                .chain(&r.products)
                .any(|p| is_removed(&p.species));
            if !keep {
                deleted_reactions.push(r.id.clone());
            }
            keep
        });
        for r in &mut self.reactions {
            r.modifiers.retain(|m| !is_removed(&m.species));
        }
        self.species.retain(|s| !is_removed(&s.id));
        self.rebuild_indexes();
        deleted_reactions
    }

    /// Removes a null node and its legs, but keeps the reactions: a null node
    /// only draws an empty side, so the reaction is still `-> B` without it.
    /// Returns false if `id` is not a null node.
    pub fn remove_null_node(&mut self, id: &str) -> bool {
        if !self.find_species(id).is_some_and(|s| s.is_null_node) {
            return false;
        }
        for r in &mut self.reactions {
            for list in [&mut r.reactants, &mut r.products, &mut r.modifiers] {
                list.retain(|p| p.species != id);
            }
        }
        self.species.retain(|s| s.id != id);
        self.rebuild_indexes();
        true
    }

    /// Deletes a reaction. Returns false if there was none with that id.
    pub fn delete_reaction(&mut self, id: &str) -> bool {
        let Some(&i) = self.reaction_index.get(id) else {
            return false;
        };
        self.reactions.remove(i);
        self.rebuild_indexes();
        true
    }

    /// Renames a species (S §3.7, §4, §7.1). Renaming an alias renames its
    /// primary, because an alias shows its primary's id.
    ///
    /// Every reference follows: aliases, reaction participants and modifiers,
    /// and whole-word occurrences in kinetic laws (case-sensitive, D-12).
    /// Renaming to the same id does nothing. Auto-widening the node to fit the
    /// new label is the caller's job; it needs the font (S §5.3).
    pub fn rename_species(&mut self, old: &str, new: &str) -> Result<(), RenameError> {
        let node = self.find_species(old).ok_or(RenameError::NotFound)?;
        if node.is_null_node {
            return Err(RenameError::NullNode);
        }
        let old = node.primary_id().to_owned();
        if new == old {
            return Ok(());
        }
        if !is_valid_sid(new) {
            return Err(RenameError::InvalidId);
        }
        if self.is_id_used(new) {
            return Err(RenameError::InUse);
        }

        for s in &mut self.species {
            if s.id == old {
                s.id = new.to_owned();
            }
            if s.alias_of.as_deref() == Some(old.as_str()) {
                s.alias_of = Some(new.to_owned());
            }
        }
        for r in &mut self.reactions {
            for p in r
                .reactants
                .iter_mut()
                .chain(&mut r.products)
                .chain(&mut r.modifiers)
            {
                if p.species == old {
                    p.species = new.to_owned();
                }
            }
            r.kinetic_law = replace_whole_word(&r.kinetic_law, &old, new);
        }
        self.rebuild_indexes();
        Ok(())
    }

    /// True if any species, reaction, parameter or compartment uses `id`.
    /// Species and reactions match exactly; parameters and compartments ignore
    /// case, as the reference's lookups do.
    pub fn is_id_used(&self, id: &str) -> bool {
        self.species_index.contains_key(id)
            || self.reaction_index.contains_key(id)
            || self.find_parameter(id).is_some()
            || self.find_compartment(id).is_some()
    }

    fn rebuild_indexes(&mut self) {
        self.species_index = self
            .species
            .iter()
            .enumerate()
            .map(|(i, s)| (s.id.clone(), i))
            .collect();
        self.reaction_index = self
            .reactions
            .iter()
            .enumerate()
            .map(|(i, r)| (r.id.clone(), i))
            .collect();
    }
}

/// The numeric suffix of an id: everything after its first character, as an
/// integer (S §4). `S12` → 12, `r3` → 3, `S1a` → none.
fn numeric_suffix(id: &str) -> Option<u64> {
    let mut chars = id.chars();
    chars.next()?;
    let rest = chars.as_str();
    if rest.is_empty() || !rest.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    rest.parse().ok()
}

/// A valid SBML SId: `[A-Za-z_][A-Za-z0-9_]*`.
pub fn is_valid_sid(id: &str) -> bool {
    let mut bytes = id.bytes();
    match bytes.next() {
        Some(b) if b.is_ascii_alphabetic() || b == b'_' => {}
        _ => return false,
    }
    bytes.all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

/// Replaces every whole-word occurrence of `from` in `text` with `to`. A whole
/// word is a match with no letter, digit or `_` directly before or after it.
/// Case-sensitive (D-12).
pub fn replace_whole_word(text: &str, from: &str, to: &str) -> String {
    if from.is_empty() {
        return text.to_owned();
    }
    let is_word = |c: char| c.is_alphanumeric() || c == '_';
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    let mut prev: Option<char> = None;
    while let Some(pos) = rest.find(from) {
        let before = rest[..pos].chars().next_back().or(prev);
        let after = rest[pos + from.len()..].chars().next();
        out.push_str(&rest[..pos]);
        if before.is_some_and(is_word) || after.is_some_and(is_word) {
            out.push_str(from);
        } else {
            out.push_str(to);
        }
        prev = from.chars().next_back();
        rest = &rest[pos + from.len()..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::pt;

    /// S1 → R2 → S3, with S4 as a modifier and an alias of S1.
    fn small_model() -> Model {
        let mut m = Model::new();
        let s1 = m.add_species(pt(0.0, 0.0), 80.0, 36.0);
        let r = m.add_reaction(pt(100.0, 0.0));
        let s3 = m.add_species(pt(200.0, 0.0), 80.0, 36.0);
        let s4 = m.add_species(pt(100.0, -100.0), 80.0, 36.0);
        let reaction = m.find_reaction_mut(&r).unwrap();
        reaction.reactants.push(Participant::new(&s1, 1.0));
        reaction.products.push(Participant::new(&s3, 1.0));
        reaction.modifiers.push(Participant::new(&s4, 1.0));
        reaction.kinetic_law = "k1*S1*S4".into();
        m
    }

    #[test]
    fn ids_come_from_one_shared_counter() {
        let mut m = Model::new();
        assert_eq!(m.add_species(Pt::ZERO, 80.0, 36.0), "S1");
        assert_eq!(m.add_species(Pt::ZERO, 80.0, 36.0), "S2");
        assert_eq!(m.add_reaction(Pt::ZERO), "R3");
        assert_eq!(m.add_species(Pt::ZERO, 80.0, 36.0), "S4");
    }

    #[test]
    fn generated_ids_skip_ids_already_taken() {
        let mut m = Model::new();
        m.push_species(SpeciesNode::new("S1", Pt::ZERO)).unwrap();
        assert_eq!(m.add_species(Pt::ZERO, 80.0, 36.0), "S2");
    }

    #[test]
    fn id_counter_after_load() {
        let mut m = Model::new();
        for id in ["S3", "glucose", "S1a", "_source_R9"] {
            m.push_species(SpeciesNode::new(id, Pt::ZERO)).unwrap();
        }
        m.push_reaction(Reaction::new("r11", Pt::ZERO)).unwrap();
        m.recompute_id_counter();
        // 1 + max(3, 11); "glucose", "S1a" and "_source_R9" have no numeric suffix.
        assert_eq!(m.next_id(), 12);

        let mut empty = Model::new();
        empty.recompute_id_counter();
        assert_eq!(empty.next_id(), 1);
    }

    #[test]
    fn duplicate_ids_are_refused() {
        let mut m = Model::new();
        m.push_species(SpeciesNode::new("A", Pt::ZERO)).unwrap();
        assert_eq!(
            m.push_species(SpeciesNode::new("A", Pt::ZERO)),
            Err(DuplicateId("A".into()))
        );
        assert_eq!(m.species().len(), 1);
    }

    #[test]
    fn lookups_are_case_sensitive_for_species_but_not_parameters() {
        let mut m = Model::new();
        m.push_species(SpeciesNode::new("Glc", Pt::ZERO)).unwrap();
        m.parameters.push(Parameter {
            variable: "Km".into(),
            expression: "0.5".into(),
        });
        m.compartments.push(Compartment::new("Cell"));
        assert!(m.find_species("Glc").is_some());
        assert!(m.find_species("glc").is_none());
        assert!(m.find_parameter("KM").is_some());
        assert!(m.find_compartment("cell").is_some());
    }

    #[test]
    fn alias_points_at_the_primary_and_copies_its_size() {
        let mut m = Model::new();
        let p = m.add_species(Pt::ZERO, 120.0, 40.0);
        let a1 = m.add_alias(&p, pt(24.0, 24.0)).unwrap();
        // An alias of an alias points at the primary.
        let a2 = m.add_alias(&a1, pt(48.0, 48.0)).unwrap();
        for a in [&a1, &a2] {
            let node = m.find_species(a).unwrap();
            assert_eq!(node.alias_of.as_deref(), Some(p.as_str()));
            assert_eq!((node.width, node.height), (120.0, 40.0));
            assert_eq!(node.primary_id(), p);
        }
        assert_eq!(m.aliases_of(&p).count(), 2);
        assert!(m.add_alias("nope", Pt::ZERO).is_none());
    }

    #[test]
    fn deleting_a_species_cascades() {
        let mut m = small_model();
        let deleted = m.delete_species("S1");
        assert_eq!(deleted, vec!["R2".to_owned()]);
        assert!(m.find_species("S1").is_none());
        assert!(m.reactions().is_empty());
        // Indexes are rebuilt: the remaining species are still found.
        assert!(m.find_species("S3").is_some());
        assert!(m.find_species("S4").is_some());
    }

    #[test]
    fn deleting_a_modifier_keeps_the_reaction_and_cleans_its_modifier_list() {
        let mut m = small_model();
        let deleted = m.delete_species("S4");
        assert!(deleted.is_empty());
        let r = m.find_reaction("R2").unwrap();
        assert!(
            r.modifiers.is_empty(),
            "D-6: no reference to a deleted node"
        );
    }

    #[test]
    fn deleting_a_primary_removes_its_aliases_and_their_reactions() {
        let mut m = small_model();
        let alias = m.add_alias("S3", pt(0.0, 200.0)).unwrap();
        let r = m.add_reaction(pt(0.0, 100.0));
        m.find_reaction_mut(&r)
            .unwrap()
            .reactants
            .push(Participant::new(&alias, 1.0));
        let mut deleted = m.delete_species("S3");
        deleted.sort();
        assert_eq!(deleted, vec!["R2".to_owned(), r]);
        assert!(m.find_species(&alias).is_none());
    }

    #[test]
    fn deleting_an_alias_leaves_the_primary() {
        let mut m = small_model();
        let alias = m.add_alias("S1", pt(0.0, 200.0)).unwrap();
        let deleted = m.delete_species(&alias);
        assert!(deleted.is_empty());
        assert!(m.find_species("S1").is_some());
        assert!(m.find_species(&alias).is_none());
        assert_eq!(m.reactions().len(), 1);
    }

    #[test]
    fn removing_a_null_node_keeps_its_reaction() {
        let mut m = small_model();
        let mut null = SpeciesNode::new("_source_R2", pt(-100.0, 0.0));
        null.is_null_node = true;
        m.push_species(null).unwrap();
        m.find_reaction_mut("R2")
            .unwrap()
            .reactants
            .push(Participant::new("_source_R2", 1.0));
        assert!(!m.remove_null_node("S1"), "only null nodes");
        assert!(m.remove_null_node("_source_R2"));
        assert!(m.find_species("_source_R2").is_none());
        let r = m.find_reaction("R2").unwrap();
        assert_eq!(r.reactants.len(), 1);
        assert_eq!(r.reactants[0].species, "S1");
    }

    #[test]
    fn deleting_a_reaction() {
        let mut m = small_model();
        assert!(m.delete_reaction("R2"));
        assert!(!m.delete_reaction("R2"));
        assert!(m.find_reaction("R2").is_none());
        assert_eq!(m.species().len(), 3);
    }

    #[test]
    fn rename_updates_every_reference() {
        let mut m = small_model();
        let alias = m.add_alias("S1", pt(0.0, 200.0)).unwrap();
        m.rename_species("S1", "Glucose").unwrap();
        assert!(m.find_species("S1").is_none());
        assert!(m.find_species("Glucose").is_some());
        assert_eq!(
            m.find_species(&alias).unwrap().alias_of.as_deref(),
            Some("Glucose")
        );
        let r = m.find_reaction("R2").unwrap();
        assert_eq!(r.reactants[0].species, "Glucose");
        assert_eq!(r.kinetic_law, "k1*Glucose*S4");
    }

    #[test]
    fn renaming_an_alias_renames_its_primary() {
        let mut m = small_model();
        let alias = m.add_alias("S1", pt(0.0, 200.0)).unwrap();
        m.rename_species(&alias, "A").unwrap();
        assert!(m.find_species("A").is_some());
        assert_eq!(
            m.find_species(&alias).unwrap().alias_of.as_deref(),
            Some("A")
        );
    }

    #[test]
    fn modifiers_follow_a_rename() {
        let mut m = small_model();
        m.rename_species("S4", "E").unwrap();
        let r = m.find_reaction("R2").unwrap();
        assert_eq!(r.modifiers[0].species, "E");
        assert_eq!(r.kinetic_law, "k1*S1*E");
    }

    #[test]
    fn rename_is_refused_when_invalid() {
        let mut m = small_model();
        m.parameters.push(Parameter {
            variable: "k1".into(),
            expression: "0.1".into(),
        });
        assert_eq!(m.rename_species("S1", ""), Err(RenameError::InvalidId));
        assert_eq!(m.rename_species("S1", "2x"), Err(RenameError::InvalidId));
        assert_eq!(m.rename_species("S1", "a-b"), Err(RenameError::InvalidId));
        assert_eq!(m.rename_species("S1", "S3"), Err(RenameError::InUse));
        assert_eq!(m.rename_species("S1", "R2"), Err(RenameError::InUse));
        assert_eq!(m.rename_species("S1", "K1"), Err(RenameError::InUse));
        assert_eq!(m.rename_species("S9", "A"), Err(RenameError::NotFound));
        // Same id: nothing to do.
        assert_eq!(m.rename_species("S1", "S1"), Ok(()));
    }

    #[test]
    fn null_nodes_cannot_be_renamed() {
        let mut m = Model::new();
        let mut node = SpeciesNode::new("_source_R1", Pt::ZERO);
        node.is_null_node = true;
        m.push_species(node).unwrap();
        assert_eq!(
            m.rename_species("_source_R1", "A"),
            Err(RenameError::NullNode)
        );
    }

    #[test]
    fn whole_word_replacement() {
        assert_eq!(replace_whole_word("k1*S1*S10", "S1", "X"), "k1*X*S10");
        assert_eq!(replace_whole_word("S1+S1", "S1", "X"), "X+X");
        assert_eq!(replace_whole_word("aS1 S1_b S1", "S1", "X"), "aS1 S1_b X");
        // Case-sensitive (D-12): renaming `a` leaves `A` alone.
        assert_eq!(replace_whole_word("a*A", "a", "b"), "b*A");
        // A match right after a previous match is still checked against it.
        assert_eq!(replace_whole_word("aa", "a", "b"), "aa");
        assert_eq!(replace_whole_word("", "a", "b"), "");
        assert_eq!(replace_whole_word("abc", "", "b"), "abc");
    }

    #[test]
    fn sid_validity() {
        for ok in ["A", "_x", "S1", "glucose_6_P"] {
            assert!(is_valid_sid(ok), "{ok}");
        }
        for bad in ["", "1A", "a b", "a-b", "é"] {
            assert!(!is_valid_sid(bad), "{bad}");
        }
    }

    #[test]
    fn selection() {
        let mut m = small_model();
        m.find_species_mut("S1").unwrap().selected = true;
        m.find_reaction_mut("R2").unwrap().selected = true;
        assert_eq!(m.selected_species().count(), 1);
        assert_eq!(m.selected_reactions().count(), 1);
        m.clear_selection();
        assert_eq!(m.selected_species().count(), 0);
        assert_eq!(m.selected_reactions().count(), 0);
    }

    #[test]
    fn default_compartment_is_not_a_real_one() {
        let mut m = Model::new();
        m.compartments.push(Compartment::new("DefaultCompartment"));
        assert!(!m.has_non_default_compartments());
        m.compartments.push(Compartment::new("cytosol"));
        assert!(m.has_non_default_compartments());
    }

    #[test]
    fn clear_resets_everything() {
        let mut m = small_model();
        m.model_name = "x".into();
        m.clear();
        assert_eq!(m, Model::new());
    }
}
