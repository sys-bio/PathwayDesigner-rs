//! Passes applied to each reaction after any import, Antimony or SBML
//! (SPEC-Rust §8.4): the catalytic rewrite, modifiers from the rate law, and
//! null nodes for an empty side. Each is a plain function, so it can be
//! tested on its own and behaves the same for both formats.

use crate::expr;
use crate::geometry::{Pt, pt};
use crate::model::{Model, NULL_NODE_SIZE, Participant, Reaction, SpeciesNode};

/// Null nodes go this far left (source) or right (sink) of the junction.
pub const NULL_OFFSET: f64 = 70.0;

/// The catalytic rewrite (S §11.2): a species on both sides with the same
/// total stoichiometry comes off both, and becomes a modifier, whether or not
/// the rate law names it. `A -> A + B` becomes `-> B` with `A` as a modifier.
pub fn catalytic_rewrite(r: &mut Reaction) {
    let total = |list: &[Participant], s: &str| -> f64 {
        list.iter()
            .filter(|p| p.species == s)
            .map(|p| p.stoichiometry)
            .sum()
    };
    let mut catalysts: Vec<String> = Vec::new();
    for p in &r.reactants {
        if total(&r.reactants, &p.species) == total(&r.products, &p.species)
            && !catalysts.contains(&p.species)
        {
            catalysts.push(p.species.clone());
        }
    }
    r.reactants.retain(|p| !catalysts.contains(&p.species));
    r.products.retain(|p| !catalysts.contains(&p.species));
    for c in catalysts {
        if !r.modifiers.iter().any(|m| m.species == c) {
            r.modifiers.push(Participant::new(c, 1.0));
        }
    }
}

/// Every species the rate law names that isn't a reactant, product or
/// modifier already becomes a modifier (S §8.4).
pub fn modifiers_from_law(model: &Model, r: &mut Reaction, law: &expr::Ast) {
    for name in expr::identifiers(law) {
        let is_species = model.find_species(name).is_some();
        let listed = r
            .reactants
            .iter()
            .chain(&r.products)
            .chain(&r.modifiers)
            .any(|p| p.species == name);
        if is_species && !listed {
            r.modifiers.push(Participant::new(name, 1.0));
        }
    }
}

/// Where a null node goes: its centre, and its size if known.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NullPlace {
    pub center: Pt,
    pub size: Option<(f64, f64)>,
}

/// Adds a null node (`_source_<id>` or `_sink_<id>`) for an empty reactant or
/// product side. `source` and `sink` say where they go; by default
/// [`NULL_OFFSET`] left or right of the junction.
pub fn add_null_nodes(
    model: &mut Model,
    r: &mut Reaction,
    source: Option<NullPlace>,
    sink: Option<NullPlace>,
) {
    for (empty, prefix, dx, place) in [
        (r.reactants.is_empty(), "_source_", -NULL_OFFSET, source),
        (r.products.is_empty(), "_sink_", NULL_OFFSET, sink),
    ] {
        if !empty {
            continue;
        }
        let place = place.unwrap_or(NullPlace {
            center: r.junction + pt(dx, 0.0),
            size: None,
        });
        let null_id = format!("{prefix}{}", r.id);
        let mut node = SpeciesNode::new(null_id.clone(), place.center);
        node.is_null_node = true;
        (node.width, node.height) = place.size.unwrap_or((NULL_NODE_SIZE, NULL_NODE_SIZE));
        if model.push_species(node).is_ok() {
            let p = Participant::new(null_id.as_str(), 1.0);
            if prefix == "_source_" {
                r.reactants.push(p);
            } else {
                r.products.push(p);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reaction(reactants: &[(&str, f64)], products: &[(&str, f64)]) -> Reaction {
        let mut r = Reaction::new("J", pt(100.0, 100.0));
        r.reactants = reactants
            .iter()
            .map(|(s, n)| Participant::new(*s, *n))
            .collect();
        r.products = products
            .iter()
            .map(|(s, n)| Participant::new(*s, *n))
            .collect();
        r
    }

    fn ids(list: &[Participant]) -> Vec<&str> {
        list.iter().map(|p| p.species.as_str()).collect()
    }

    #[test]
    fn catalytic_rewrite_moves_catalysts_to_modifiers() {
        let mut r = reaction(&[("A", 1.0)], &[("A", 1.0), ("B", 1.0)]);
        catalytic_rewrite(&mut r);
        assert!(r.reactants.is_empty());
        assert_eq!(ids(&r.products), ["B"]);
        assert_eq!(ids(&r.modifiers), ["A"]);
        // Different totals: nothing moves.
        let mut r = reaction(&[("A", 1.0)], &[("A", 2.0)]);
        catalytic_rewrite(&mut r);
        assert_eq!(ids(&r.reactants), ["A"]);
        assert!(r.modifiers.is_empty());
    }

    #[test]
    fn law_species_become_modifiers_once() {
        let mut m = Model::new();
        for id in ["S", "P", "E"] {
            m.push_species(SpeciesNode::new(id, Pt::ZERO)).unwrap();
        }
        let mut r = reaction(&[("S", 1.0)], &[("P", 1.0)]);
        let law = expr::parse("k*E*S/(Km + S + E)").unwrap();
        modifiers_from_law(&m, &mut r, &law);
        modifiers_from_law(&m, &mut r, &law);
        assert_eq!(ids(&r.modifiers), ["E"]);
    }

    #[test]
    fn null_nodes_fill_empty_sides() {
        let mut m = Model::new();
        let mut r = reaction(&[], &[]);
        let sink = NullPlace {
            center: pt(5.0, 6.0),
            size: Some((10.0, 12.0)),
        };
        add_null_nodes(&mut m, &mut r, None, Some(sink));
        assert_eq!(ids(&r.reactants), ["_source_J"]);
        assert_eq!(ids(&r.products), ["_sink_J"]);
        let source = m.find_species("_source_J").unwrap();
        assert!(source.is_null_node);
        assert_eq!(source.center, pt(100.0 - NULL_OFFSET, 100.0));
        let sink = m.find_species("_sink_J").unwrap();
        assert_eq!(
            (sink.center, sink.width, sink.height),
            (pt(5.0, 6.0), 10.0, 12.0)
        );
    }
}
