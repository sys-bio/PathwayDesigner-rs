//! The model as SBML Level 3 Version 1 with the Layout package (S §11.5).

use std::collections::{HashMap, HashSet};

use sbml_io as sb;

use super::math::to_math;
use crate::bezier::leg_curve;
use crate::expr;
use crate::geometry::Pt;
use crate::model::{DEFAULT_COMPARTMENT, Model, ReactionMode, Role, SpeciesNode};
use crate::render::draw::{border, effective_junction, effective_mode};

const LEVEL: u32 = 3;
const VERSION: u32 = 1;
/// The layout is this much bigger than the right- and bottom-most node edges.
const LAYOUT_MARGIN: f64 = 80.0;

/// An exported document, with what the user should know about it.
pub struct Exported {
    pub xml: String,
    /// What is wrong: expressions that don't parse (left out), and anything
    /// `sbml_io::validate` finds. Empty for a valid file.
    pub problems: Vec<String>,
    /// What was added to make the file valid: names used in expressions but
    /// defined nowhere, declared as parameters with value 0, as Antimony
    /// import does (S §11.2).
    pub notes: Vec<String>,
}

/// Writes the model as SBML (S §11.5).
pub fn export(model: &Model) -> Exported {
    let mut problems = Vec::new();
    let mut notes = Vec::new();
    let doc = document(model, &mut problems, &mut notes);
    problems.extend(sb::validate(&doc).into_iter().map(|i| i.message));
    Exported {
        xml: sb::write(&doc),
        problems,
        notes,
    }
}

/// Ids handed out, so every generated id is unique (D-13).
struct Ids(HashSet<String>);

impl Ids {
    fn unique(&mut self, base: String) -> String {
        let mut id = base.clone();
        let mut n = 2;
        while !self.0.insert(id.clone()) {
            id = format!("{base}_{n}");
            n += 1;
        }
        id
    }
}

/// `text` as a valid SId: other characters become `_`, and a leading digit
/// gets a `_` before it. Empty gives `model` (D-13).
fn sid(text: &str) -> String {
    let mut s: String = text
        .trim()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if s.is_empty() {
        return "model".into();
    }
    if s.starts_with(|c: char| c.is_ascii_digit()) {
        s.insert(0, '_');
    }
    s
}

fn document(model: &Model, problems: &mut Vec<String>, notes: &mut Vec<String>) -> sb::Document {
    let mut ids = Ids(HashSet::new());
    // Every id the model uses is taken before any is generated.
    for s in model.species() {
        ids.0.insert(s.id.clone());
    }
    for r in model.reactions() {
        ids.0.insert(r.id.clone());
    }
    for c in &model.compartments {
        ids.0.insert(c.id.clone());
    }
    for p in &model.parameters {
        ids.0.insert(p.variable.clone());
    }
    for r in &model.assignment_rules {
        ids.0.insert(r.variable.clone());
    }
    ids.0.insert(DEFAULT_COMPARTMENT.into());

    let mut parse = |text: &str, what: &str| -> Option<sb::Math> {
        let text = text.trim();
        if text.is_empty() {
            return None;
        }
        match expr::parse(text) {
            Ok(ast) => Some(to_math(&ast, LEVEL, VERSION)),
            Err(e) => {
                problems.push(format!("{what} doesn't parse, so it was left out: {e}"));
                None
            }
        }
    };

    let mut m = sb::Model {
        id: Some(ids.unique(sid(&model.model_name))),
        name: (!model.model_name.trim().is_empty()).then(|| model.model_name.clone()),
        ..Default::default()
    };

    // Compartments: the default one if it's missing, then the model's.
    if model.find_compartment(DEFAULT_COMPARTMENT).is_none() {
        m.compartments.push(sb::Compartment {
            id: DEFAULT_COMPARTMENT.into(),
            size: Some(1.0),
            spatial_dimensions: Some(3.0),
            constant: Some(true),
            ..Default::default()
        });
    }
    for c in &model.compartments {
        m.compartments.push(sb::Compartment {
            id: c.id.clone(),
            size: Some(c.size),
            spatial_dimensions: Some(f64::from(c.dimensions)),
            constant: Some(true),
            ..Default::default()
        });
    }

    // Species: primaries only, no null nodes (D-9).
    for s in model
        .species()
        .iter()
        .filter(|s| !s.is_alias() && !s.is_null_node)
    {
        m.species.push(sb::Species {
            id: s.id.clone(),
            compartment: if s.compartment.is_empty() {
                DEFAULT_COMPARTMENT.into()
            } else {
                s.compartment.clone()
            },
            initial_concentration: Some(s.initial_value),
            has_only_substance_units: Some(false),
            boundary_condition: Some(s.is_boundary),
            constant: Some(s.is_constant),
            ..Default::default()
        });
    }

    // Parameters: a number is the value; anything else is an initial
    // assignment (D-10).
    for p in &model.parameters {
        let value = p
            .expression
            .trim()
            .parse::<f64>()
            .ok()
            .filter(|v| v.is_finite());
        m.parameters.push(sb::Parameter {
            id: p.variable.clone(),
            value,
            constant: Some(true),
            ..Default::default()
        });
        if value.is_none()
            && let Some(math) = parse(&p.expression, &format!("The value of {}", p.variable))
        {
            m.initial_assignments.push(sb::InitialAssignment {
                symbol: p.variable.clone(),
                math: Some(math),
                meta: sb::Meta::default(),
            });
        }
    }
    // Assignment rules (D-10). A variable that is nothing else needs a
    // parameter to set.
    for r in &model.assignment_rules {
        let declared = model.find_species(&r.variable).is_some()
            || model.find_compartment(&r.variable).is_some()
            || model.find_parameter(&r.variable).is_some();
        if !declared && !m.parameters.iter().any(|p| p.id == r.variable) {
            m.parameters.push(sb::Parameter {
                id: r.variable.clone(),
                constant: Some(false),
                ..Default::default()
            });
        }
        if let Some(math) = parse(&r.expression, &format!("The rule for {}", r.variable)) {
            m.rules.push(sb::Rule::Assignment {
                variable: r.variable.clone(),
                math: Some(math),
                meta: sb::Meta::default(),
            });
        }
    }

    // Reactions. Each species reference gets an id, so its glyph can name it.
    let primary = |id: &str| -> Option<&SpeciesNode> {
        let node = model.find_species(id)?;
        if node.is_null_node {
            return None;
        }
        model.find_species(node.primary_id())
    };
    let mut legs: Vec<Leg> = Vec::new();
    for r in model.reactions() {
        let mut rx = sb::Reaction {
            id: r.id.clone(),
            reversible: Some(r.is_reversible),
            fast: Some(false),
            ..Default::default()
        };
        for (role, list, tag) in [
            (Role::Reactant, &r.reactants, "sub"),
            (Role::Product, &r.products, "prod"),
            (Role::Modifier, &r.modifiers, "mod"),
        ] {
            for (index, p) in list.iter().enumerate() {
                let reference = match primary(&p.species) {
                    Some(s) => {
                        let sr_id = ids.unique(format!("sr_{}_{tag}_{}", r.id, s.id));
                        if role == Role::Modifier {
                            if rx.modifiers.iter().any(|m| m.species == s.id) {
                                continue;
                            }
                            rx.modifiers.push(sb::ModifierSpeciesReference {
                                id: Some(sr_id.clone()),
                                species: s.id.clone(),
                                ..Default::default()
                            });
                        } else {
                            let sr = sb::SpeciesReference {
                                id: Some(sr_id.clone()),
                                species: s.id.clone(),
                                stoichiometry: Some(p.stoichiometry),
                                constant: Some(true),
                                ..Default::default()
                            };
                            if role == Role::Reactant {
                                rx.reactants.push(sr);
                            } else {
                                rx.products.push(sr);
                            }
                        }
                        Some(sr_id)
                    }
                    // A null node: drawn, but not a species.
                    None if model.find_species(&p.species).is_some() => None,
                    None => continue,
                };
                legs.push(Leg {
                    reaction: r.id.clone(),
                    role,
                    index,
                    tag,
                    reference,
                });
            }
        }
        if let Some(math) = parse(&r.kinetic_law, &format!("The rate law of {}", r.id)) {
            rx.kinetic_law = Some(sb::KineticLaw {
                math: Some(math),
                ..Default::default()
            });
        }
        m.reactions.push(rx);
    }

    // Names used in expressions but defined nowhere become parameters, 0.
    let mut defined: HashSet<String> = m.compartments.iter().map(|c| c.id.clone()).collect();
    defined.extend(m.species.iter().map(|s| s.id.clone()));
    defined.extend(m.parameters.iter().map(|p| p.id.clone()));
    defined.extend(m.reactions.iter().map(|r| r.id.clone()));
    let mut used = Vec::new();
    let maths = m
        .initial_assignments
        .iter()
        .filter_map(|ia| ia.math.as_ref())
        .chain(m.rules.iter().filter_map(|r| r.math()))
        .chain(
            m.reactions
                .iter()
                .filter_map(|r| r.kinetic_law.as_ref()?.math.as_ref()),
        );
    // Called function names aren't values, so they don't count.
    let called: HashSet<String> = maths.clone().flat_map(called_functions).collect();
    for math in maths {
        math.visit_idents(&mut |id| {
            if !defined.contains(id)
                && !called.contains(id)
                && !used.iter().any(|u: &String| u == id)
            {
                used.push(id.to_owned());
            }
        });
    }
    for id in &used {
        m.parameters.push(sb::Parameter {
            id: id.clone(),
            value: Some(0.0),
            constant: Some(true),
            ..Default::default()
        });
    }
    if !used.is_empty() {
        notes.push(format!(
            "{} {} used but not defined, so the file declares {} as a parameter with value 0",
            used.join(", "),
            if used.len() == 1 { "is" } else { "are" },
            if used.len() == 1 { "it" } else { "each" },
        ));
    }

    m.layouts.push(layout(model, &legs, &mut ids));
    let mut doc = sb::Document::new(LEVEL, VERSION);
    doc.model = Some(m);
    doc
}

/// The names of the functions an expression calls.
fn called_functions(m: &sb::Math) -> Vec<String> {
    let mut out = Vec::new();
    fn walk(m: &sb::Math, out: &mut Vec<String>) {
        use sb::Math::*;
        match m {
            Call(f, args) => {
                out.push(f.clone());
                args.iter().for_each(|a| walk(a, out));
            }
            Apply(_, args) | CallSymbol(_, args) => args.iter().for_each(|a| walk(a, out)),
            Root { degree: q, arg } | Log { base: q, arg } => {
                if let Some(q) = q {
                    walk(q, out);
                }
                walk(arg, out);
            }
            Piecewise { pieces, otherwise } => {
                for (v, c) in pieces {
                    walk(v, out);
                    walk(c, out);
                }
                if let Some(o) = otherwise {
                    walk(o, out);
                }
            }
            Lambda { body, .. } => walk(body, out),
            Semantics { math, .. } => walk(math, out),
            Number { .. } | Ident(_) | Symbol(_) | Constant(_) | Raw(_) => {}
        }
    }
    walk(m, &mut out);
    out
}

/// One drawn leg: which participant, and the id of its species reference
/// (`None` for a null node).
struct Leg {
    reaction: String,
    role: Role,
    index: usize,
    tag: &'static str,
    reference: Option<String>,
}

fn bbox(node: &SpeciesNode) -> sb::BoundingBox {
    sb::BoundingBox::new(
        node.center.x - node.width / 2.0,
        node.center.y - node.height / 2.0,
        node.width,
        node.height,
    )
}

fn point(p: Pt) -> sb::Point {
    sb::Point::new(p.x, p.y)
}

fn layout(model: &Model, legs: &[Leg], ids: &mut Ids) -> sb::Layout {
    let (mut max_x, mut max_y) = (0.0f64, 0.0f64);
    for s in model.species() {
        max_x = max_x.max(s.center.x + s.width / 2.0);
        max_y = max_y.max(s.center.y + s.height / 2.0);
    }
    let mut l = sb::Layout {
        id: ids.unique("layout1".into()),
        dimensions: sb::Dimensions::new(max_x + LAYOUT_MARGIN, max_y + LAYOUT_MARGIN),
        ..Default::default()
    };

    // One glyph per node, aliases included. A null node's glyph names no
    // species, so its place survives a round trip.
    let mut glyph_of: HashMap<&str, String> = HashMap::new();
    for s in model.species() {
        let id = ids.unique(format!("sg_{}", s.id));
        glyph_of.insert(&s.id, id.clone());
        l.species_glyphs.push(sb::SpeciesGlyph {
            id: id.clone(),
            species: (!s.is_null_node).then(|| s.primary_id().to_owned()),
            bounding_box: Some(bbox(s)),
            ..Default::default()
        });
        if !s.is_null_node {
            l.text_glyphs.push(sb::TextGlyph {
                id: ids.unique(format!("tg_{}", s.id)),
                graphical_object: Some(id),
                origin_of_text: Some(s.primary_id().to_owned()),
                bounding_box: Some(bbox(s)),
                ..Default::default()
            });
        }
    }

    for r in model.reactions() {
        let j = effective_junction(model, r);
        let bezier = effective_mode(r) == ReactionMode::Bezier;
        let mut g = sb::ReactionGlyph {
            id: ids.unique(format!("rg_{}", r.id)),
            reaction: Some(r.id.clone()),
            bounding_box: Some(sb::BoundingBox::new(j.x, j.y, 0.0, 0.0)),
            ..Default::default()
        };
        for leg in legs.iter().filter(|l| l.reaction == r.id) {
            let p = &r.participants(leg.role)[leg.index];
            let Some(node) = model.find_species(&p.species) else {
                continue;
            };
            let edge = border(node, j);
            let segment = match (
                leg.role,
                bezier
                    .then(|| leg_curve(model, r, leg.role, leg.index))
                    .flatten(),
            ) {
                // basePoint1 = ctrl1 and basePoint2 = ctrl2 on both sides
                // (S §5.6.1, §11.5).
                (Role::Reactant, Some(c)) => sb::Segment::Bezier {
                    start: point(edge),
                    end: point(j),
                    base1: point(c.p1),
                    base2: point(c.p2),
                },
                (Role::Product, Some(c)) => sb::Segment::Bezier {
                    start: point(j),
                    end: point(edge),
                    base1: point(c.p1),
                    base2: point(c.p2),
                },
                (Role::Product, None) => sb::Segment::Line {
                    start: point(j),
                    end: point(edge),
                },
                _ => sb::Segment::Line {
                    start: point(edge),
                    end: point(j),
                },
            };
            let role = match leg.role {
                Role::Reactant => sb::Role::Substrate,
                Role::Product => sb::Role::Product,
                Role::Modifier => sb::Role::Modifier,
            };
            g.species_reference_glyphs.push(sb::SpeciesReferenceGlyph {
                id: ids.unique(format!("srg_{}_{}_{}", r.id, leg.tag, node.id)),
                species_glyph: glyph_of[node.id.as_str()].clone(),
                species_reference: leg.reference.clone(),
                role: Some(role),
                curve: Some(sb::Curve {
                    segments: vec![segment],
                }),
                ..Default::default()
            });
        }
        l.reaction_glyphs.push(g);
    }
    l
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_valid_and_unique() {
        assert_eq!(sid(""), "model");
        assert_eq!(sid("my model-2"), "my_model_2");
        assert_eq!(sid("3cells"), "_3cells");
        let mut ids = Ids(HashSet::from(["a".to_string()]));
        assert_eq!(ids.unique("a".into()), "a_2");
        assert_eq!(ids.unique("a".into()), "a_3");
        assert_eq!(ids.unique("b".into()), "b");
    }
}
