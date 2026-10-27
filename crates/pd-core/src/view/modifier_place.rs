//! Arrange ▸ Place Modifiers by Their Reactions (S §7.3): a modifier line
//! from a drawing far from its reaction gets a new drawing of the species
//! beside the reaction, so a model imported with its effectors in a far-off
//! column doesn't cross the map with long lines. Drawings left joined to
//! nothing are removed. The model doesn't change: an alias is the same
//! species.

use super::DiagramView;
use crate::geometry::{Pt, normalize, pt};
use crate::model::{Id, Model, Reaction, Role};
use crate::render::draw::{effective_junction, self_modifier_drawing};
use crate::view::ViewResponse;

/// How far a modifier's drawing may be from its reaction's junction before
/// it is given one nearby.
pub const MODIFIER_FAR: f64 = 150.0;
/// How far from the junction a new drawing goes: the first ring, then each
/// ring this much further out, tried in turn until a place overlaps no node.
const PLACE_FIRST_RING: f64 = 70.0;
const PLACE_RING_STEP: f64 = 30.0;
const PLACE_RINGS: usize = 10;
/// Directions tried around the junction.
const PLACE_DIRECTIONS: usize = 16;
/// Room kept between a new drawing and any other node.
const PLACE_CLEARANCE: f64 = 8.0;

/// What Place Modifiers by Their Reactions did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Placed {
    /// Modifier lines given a drawing beside their reaction.
    pub lines: usize,
    /// Drawings removed because they were left joined to nothing.
    pub removed: usize,
}

impl DiagramView {
    /// The modifier lines Place Modifiers by Their Reactions would move: in
    /// the selected reactions, or in all of them if none is selected, each
    /// one whose drawing is further than [`MODIFIER_FAR`] from the junction.
    /// A self modifier (a reactant or product of its reaction too) is left to
    /// its loop.
    fn far_modifiers(&self) -> Vec<(Id, usize)> {
        let any_selected = self.model.reactions().iter().any(|r| r.selected);
        let mut out = Vec::new();
        for r in self
            .model
            .reactions()
            .iter()
            .filter(|r| r.selected || !any_selected)
        {
            let j = effective_junction(&self.model, r);
            for (i, m) in r.modifiers.iter().enumerate() {
                if self_modifier_drawing(&self.model, r, m).is_some() {
                    continue;
                }
                if self
                    .model
                    .find_species(&m.species)
                    .is_some_and(|s| s.center.distance(j) > MODIFIER_FAR)
                {
                    out.push((r.id.clone(), i));
                }
            }
        }
        out
    }

    /// True if Place Modifiers by Their Reactions has a line to move.
    pub fn can_place_modifiers(&self) -> bool {
        !self.far_modifiers().is_empty()
    }

    /// Place Modifiers by Their Reactions: each far modifier line (see
    /// `far_modifiers`) gets a drawing of its species beside its reaction, its
    /// corner points dropped. A drawing whose only lines are far modifier
    /// lines is itself moved beside the first of their reactions; every other
    /// line gets a new drawing, in the look of the one it came from. Then
    /// every alias joined to nothing is removed (a species' first drawing
    /// never is). The drawings placed are selected. One undo step.
    pub fn place_modifiers(&mut self) -> (ViewResponse, Placed) {
        let todo = self.far_modifiers();
        if todo.is_empty() {
            return (ViewResponse::default(), Placed::default());
        }
        self.cancel_gesture();
        let before = self.model.clone();
        self.model.clear_selection();
        let mut placed = Placed::default();
        // Drawings every line of which is moved: each goes itself, once.
        let lines_of = |m: &Model, id: &str| {
            m.reactions()
                .iter()
                .flat_map(|r| {
                    let rid = r.id.clone();
                    r.reactants
                        .iter()
                        .chain(&r.products)
                        .map(|p| (p.species.clone(), None))
                        .chain(
                            r.modifiers
                                .iter()
                                .enumerate()
                                .map(move |(i, p)| (p.species.clone(), Some((rid.clone(), i)))),
                        )
                        .collect::<Vec<_>>()
                })
                .filter(|(s, _)| s == id)
                .map(|(_, line)| line)
                .collect::<Vec<_>>()
        };
        let mut movable: Vec<Id> = Vec::new();
        for (rid, i) in &todo {
            let id = &self.model.find_reaction(rid).expect("listed").modifiers[*i].species;
            if !movable.contains(id)
                && lines_of(&self.model, id)
                    .iter()
                    .all(|l| l.as_ref().is_some_and(|l| todo.contains(l)))
            {
                movable.push(id.clone());
            }
        }
        for (rid, i) in todo {
            let Some(r) = self.model.find_reaction(&rid) else {
                continue;
            };
            let Some(old) = r
                .modifiers
                .get(i)
                .and_then(|m| self.model.find_species(&m.species))
                .cloned()
            else {
                continue;
            };
            let at = free_place(&self.model, r, &old);
            if let Some(k) = movable.iter().position(|m| *m == old.id) {
                // This drawing's own first line: it moves itself.
                movable.remove(k);
                if let Some(s) = self.model.find_species_mut(&old.id) {
                    s.center = at;
                }
                if let Some(m) = self
                    .model
                    .find_reaction_mut(&rid)
                    .and_then(|r| r.modifiers.get_mut(i))
                {
                    m.points.clear();
                }
                self.set_species_selected(&old.id, true);
                placed.lines += 1;
                continue;
            }
            let Some(alias) = self.model.add_alias(&old.id, at) else {
                continue;
            };
            if let Some(a) = self.model.find_species_mut(&alias) {
                a.style = old.style;
                a.border_dash = old.border_dash;
                a.style_name = old.style_name.clone();
                (a.shape, a.width, a.height) = (old.shape, old.width, old.height);
                a.label_offset = old.label_offset;
            }
            let m = &mut self
                .model
                .find_reaction_mut(&rid)
                .expect("found above")
                .modifiers[i];
            m.species = alias.clone();
            m.points.clear();
            self.set_species_selected(&alias, true);
            placed.lines += 1;
        }
        // Aliases that nothing draws any more, or ever did.
        let joined: Vec<Id> = self
            .model
            .reactions()
            .iter()
            .flat_map(|r| r.reactants.iter().chain(&r.products).chain(&r.modifiers))
            .map(|p| p.species.clone())
            .collect();
        let loose: Vec<Id> = self
            .model
            .species()
            .iter()
            .filter(|s| s.is_alias() && !joined.contains(&s.id))
            .map(|s| s.id.clone())
            .collect();
        for id in &loose {
            self.model.delete_species(id);
        }
        placed.removed = loose.len();
        let mut r = self.record("Place Modifiers by Their Reactions", before);
        r.selection_changed = true;
        (r, placed)
    }
}

/// Where a drawing of `node` goes beside reaction `r`: around its junction,
/// on the nearest ring at which some direction overlaps no other node, in the
/// direction there furthest from the reaction's lines and modifiers. If every
/// place overlaps something, the clearest direction on the furthest ring.
fn free_place(model: &Model, r: &Reaction, node: &crate::model::SpeciesNode) -> Pt {
    let j = effective_junction(model, r);
    // The ways the reaction's lines leave the junction.
    let mut ways: Vec<Pt> = Vec::new();
    for role in [Role::Reactant, Role::Product] {
        for index in 0..r.participants(role).len() {
            if let Some(c) = crate::bezier::leg_curve(model, r, role, index) {
                let near = c.eval(if role == Role::Reactant { 0.8 } else { 0.2 });
                if near.distance(j) >= 1e-6 {
                    ways.push(normalize(near - j));
                }
            }
        }
    }
    for m in &r.modifiers {
        if let Some(s) = model
            .find_species(&m.species)
            .filter(|s| s.center.distance(j) <= MODIFIER_FAR && s.center.distance(j) >= 1.0)
        {
            ways.push(normalize(s.center - j));
        }
    }
    let crowding = |d: Pt| {
        ways.iter()
            .map(|w| w.x * d.x + w.y * d.y)
            .fold(-1.0, f64::max)
    };
    let (half_w, half_h) = (node.width / 2.0, node.height / 2.0);
    let overlaps = |c: Pt| {
        model.species().iter().any(|s| {
            !s.is_null_node
                && s.id != node.id
                && (s.center.x - c.x).abs() < s.width / 2.0 + half_w + PLACE_CLEARANCE
                && (s.center.y - c.y).abs() < s.height / 2.0 + half_h + PLACE_CLEARANCE
        })
    };
    let directions: Vec<Pt> = (0..PLACE_DIRECTIONS)
        .map(|k| {
            let a = std::f64::consts::TAU * k as f64 / PLACE_DIRECTIONS as f64;
            pt(a.cos(), a.sin())
        })
        .collect();
    let clearest = |cands: &mut dyn Iterator<Item = Pt>| {
        cands.min_by(|a, b| crowding(*a).total_cmp(&crowding(*b)))
    };
    let ring = |k: usize| PLACE_FIRST_RING + PLACE_RING_STEP * k as f64;
    for k in 0..PLACE_RINGS {
        let dist = ring(k);
        if let Some(d) = clearest(
            &mut directions
                .iter()
                .copied()
                .filter(|d| !overlaps(j + *d * dist)),
        ) {
            return j + d * dist;
        }
    }
    let d = clearest(&mut directions.iter().copied()).expect("sixteen directions");
    j + d * ring(PLACE_RINGS - 1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Participant;

    /// A -> B at (200, 0), and C -> D at (200, 400), with E a modifier of
    /// both from far off at (2000, 0), and F a near modifier of the first.
    fn model() -> Model {
        let mut m = Model::new();
        for (id, at) in [
            ("A", pt(0.0, 0.0)),
            ("B", pt(400.0, 0.0)),
            ("C", pt(0.0, 400.0)),
            ("D", pt(400.0, 400.0)),
            ("E", pt(2000.0, 0.0)),
            ("F", pt(200.0, -100.0)),
        ] {
            m.push_species(crate::model::SpeciesNode::new(id, at))
                .unwrap();
        }
        for (rid, a, b, y) in [("R1", "A", "B", 0.0), ("R2", "C", "D", 400.0)] {
            let mut r = Reaction::new(rid, pt(200.0, y));
            r.reactants.push(Participant::new(a, 1.0));
            r.products.push(Participant::new(b, 1.0));
            r.modifiers.push(Participant::new("E", 1.0));
            m.push_reaction(r).unwrap();
        }
        m.find_reaction_mut("R1")
            .unwrap()
            .modifiers
            .push(Participant::new("F", 1.0));
        m
    }

    fn view(m: Model) -> DiagramView {
        let mut v = DiagramView::default();
        v.set_model(m);
        v
    }

    fn modifier_at(v: &DiagramView, rid: &str, primary: &str) -> Pt {
        let r = v.model().find_reaction(rid).unwrap();
        r.modifiers
            .iter()
            .filter_map(|p| v.model().find_species(&p.species))
            .find(|s| s.primary_id() == primary)
            .unwrap()
            .center
    }

    #[test]
    fn far_modifiers_get_drawings_beside_their_reactions() {
        let mut v = view(model());
        assert!(v.can_place_modifiers());
        let (_, placed) = v.place_modifiers();
        assert_eq!(
            placed,
            Placed {
                lines: 2,
                removed: 0
            }
        );
        for (rid, y) in [("R1", 0.0), ("R2", 400.0)] {
            let at = modifier_at(&v, rid, "E");
            assert!(at.distance(pt(200.0, y)) <= MODIFIER_FAR, "{rid}: {at:?}");
        }
        // E's only lines were far: E itself went to R1, R2 has a new drawing.
        let e = v.model().find_species("E").unwrap();
        assert_eq!(e.center, modifier_at(&v, "R1", "E"));
        assert_eq!(v.model().aliases_of("E").count(), 1);
        // The near modifier stays where it was.
        assert_eq!(modifier_at(&v, "R1", "F"), pt(200.0, -100.0));
        // Nothing more to do; one undo step puts it all back.
        assert!(!v.can_place_modifiers());
        v.undo();
        assert_eq!(v.model().find_species("E").unwrap().center, pt(2000.0, 0.0));
        assert_eq!(v.model().aliases_of("E").count(), 0);
    }

    #[test]
    fn the_model_doesnt_change() {
        let before = model();
        let mut v = view(before.clone());
        v.place_modifiers();
        let primaries = |m: &Model| {
            let mut out: Vec<(String, Vec<String>)> = m
                .reactions()
                .iter()
                .map(|r| {
                    let mut ms: Vec<String> = r
                        .modifiers
                        .iter()
                        .map(|p| m.find_species(&p.species).unwrap().primary_id().to_owned())
                        .collect();
                    ms.sort();
                    (r.id.clone(), ms)
                })
                .collect();
            out.sort();
            out
        };
        assert_eq!(primaries(v.model()), primaries(&before));
    }

    #[test]
    fn a_placed_drawing_overlaps_no_node() {
        // Something already sits where the first ring would put E.
        let mut m = model();
        for k in 0..16 {
            let a = std::f64::consts::TAU * k as f64 / 16.0;
            let id = format!("X{k}");
            m.push_species(crate::model::SpeciesNode::new(
                id.as_str(),
                pt(200.0 + 70.0 * a.cos(), 70.0 * a.sin()),
            ))
            .unwrap();
        }
        let mut v = view(m);
        v.place_modifiers();
        let e = modifier_at(&v, "R1", "E");
        assert!(e.distance(pt(200.0, 0.0)) > 70.0 + 1.0, "{e:?}");
        let mine = v
            .model()
            .species()
            .iter()
            .find(|s| s.center == e)
            .unwrap()
            .id
            .clone();
        for s in v.model().species().iter().filter(|s| s.id != mine) {
            assert!(
                (s.center.x - e.x).abs() >= s.width / 2.0 + 40.0
                    || (s.center.y - e.y).abs() >= s.height / 2.0 + 18.0,
                "overlaps {}",
                s.id
            );
        }
    }

    #[test]
    fn only_the_selected_reactions_when_some_are() {
        let mut v = view(model());
        v.model.find_reaction_mut("R2").unwrap().selected = true;
        let (_, placed) = v.place_modifiers();
        assert_eq!(placed.lines, 1);
        assert!(modifier_at(&v, "R2", "E").distance(pt(200.0, 400.0)) <= MODIFIER_FAR);
        // R1's line still runs from far off.
        assert!(modifier_at(&v, "R1", "E").distance(pt(200.0, 0.0)) > MODIFIER_FAR);
    }

    #[test]
    fn self_modifiers_are_left_to_their_loops() {
        let mut m = model();
        m.find_species_mut("A").unwrap().center = pt(-1000.0, 0.0);
        m.find_reaction_mut("R1")
            .unwrap()
            .modifiers
            .push(Participant::new("A", 1.0));
        let mut v = view(m);
        v.place_modifiers();
        assert_eq!(modifier_at(&v, "R1", "A"), pt(-1000.0, 0.0));
        assert_eq!(v.model().aliases_of("A").count(), 0);
    }

    #[test]
    fn drawings_joined_to_nothing_go_but_never_a_species_first() {
        let mut m = model();
        let loose = m.add_alias("B", pt(900.0, 900.0)).unwrap();
        m.push_species(crate::model::SpeciesNode::new("G", pt(-500.0, -500.0)))
            .unwrap();
        let mut v = view(m);
        let (_, placed) = v.place_modifiers();
        assert_eq!(placed.removed, 1);
        assert!(v.model().find_species(&loose).is_none());
        assert!(
            v.model().find_species("G").is_some(),
            "a species' first drawing stays"
        );
    }
}
