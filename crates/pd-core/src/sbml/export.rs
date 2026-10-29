//! The model as SBML Level 3 Version 1 with the Layout package (S §11.5).

use std::collections::{HashMap, HashSet};

use sbml_io as sb;

use super::math::to_math;
use crate::bezier::leg_curve;
use crate::expr;
use crate::geometry::Pt;
use crate::model::{
    DEFAULT_COMPARTMENT, History, Model, ModifierKind, ReactionMode, Role, SpeciesNode,
};
use crate::render::constants::JUNCTION_RADIUS;
use crate::render::draw::{border, effective_junction, effective_mode, modifier_route};
use crate::textbox::lay_out;

use super::appearance::Item;

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
    // A shown compartment without a box yet is written with the box the app
    // gives it when the model is opened (S §7.9), so the file shows what is
    // seen.
    let fitted;
    let model = if model
        .compartments
        .iter()
        .any(|c| c.frame.is_none() && !c.hidden && !c.is_default())
    {
        let mut m = model.clone();
        crate::compartments::place_unplaced(&mut m);
        fitted = m;
        &fitted
    } else {
        model
    };
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
    let model_id = m.id.clone().unwrap_or_default();
    m.meta = meta(&model_id, &model.annotation, &model.history);

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
            meta: meta(&c.id, &c.annotation, &History::default()),
            id: c.id.clone(),
            name: c.display_name.clone(),
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
            meta: meta(&s.id, &s.annotation, &History::default()),
            id: s.id.clone(),
            name: s.display_name.clone(),
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
            meta: meta(&p.variable, &p.annotation, &History::default()),
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
                meta: meta(&r.variable, &r.annotation, &History::default()),
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
            meta: meta(&r.id, &r.annotation, &History::default()),
            id: r.id.clone(),
            name: r.display_name.clone(),
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
                                meta: sb::Meta {
                                    sbo_term: p.kind.sbo_term().map(str::to_owned),
                                    ..Default::default()
                                },
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
                    kind: p.kind,
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
    m.global_render = Some(super::appearance::global_render(model));
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
    /// A modifier's kind, for its glyph's role (S §5.7.1).
    kind: ModifierKind,
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
    for f in model.compartments.iter().filter_map(|c| c.frame) {
        max_x = max_x.max(f.max().x);
        max_y = max_y.max(f.max().y);
    }
    let texts: Vec<_> = model.texts().iter().map(|t| (t, lay_out(t))).collect();
    for (t, l) in &texts {
        max_x = max_x.max(t.position.x + l.width);
        max_y = max_y.max(t.position.y + l.height);
    }
    let mut l = sb::Layout {
        id: ids.unique("layout1".into()),
        dimensions: sb::Dimensions::new(max_x + LAYOUT_MARGIN, max_y + LAYOUT_MARGIN),
        ..Default::default()
    };

    // One glyph per node, aliases included, each species' own glyph before its
    // aliases' (a reader taking the first glyph of a species as the species
    // gets it right). A null node's glyph names no species, so its place
    // survives a round trip.
    let mut glyph_of: HashMap<&str, String> = HashMap::new();
    // Every glyph and what it draws, for the render information.
    let mut glyphs: Vec<(String, Item)> = Vec::new();
    let nodes = model.species();
    let in_order = nodes
        .iter()
        .filter(|s| !s.is_alias())
        .chain(nodes.iter().filter(|s| s.is_alias()));
    for s in in_order {
        let id = ids.unique(format!("sg_{}", s.id));
        glyph_of.insert(&s.id, id.clone());
        let mut glyph = sb::SpeciesGlyph {
            id: id.clone(),
            species: (!s.is_null_node).then(|| s.primary_id().to_owned()),
            bounding_box: Some(bbox(s)),
            ..Default::default()
        };
        // Its shape, in PathwayDesigner's note (S §5.10).
        glyph.meta.annotation = super::shape::species_note(s);
        l.species_glyphs.push(glyph);
        glyphs.push((id.clone(), Item::Species(s)));
        if !s.is_null_node {
            // A name outside the node has a box of its own (S §5.11).
            let text_box = match crate::render::draw::outside_label_box(s) {
                Some((a, b)) => sb::BoundingBox::new(a.x, a.y, b.x - a.x, b.y - a.y),
                None => bbox(s),
            };
            let tid = ids.unique(format!("tg_{}", s.id));
            l.text_glyphs.push(sb::TextGlyph {
                id: tid.clone(),
                graphical_object: Some(id),
                origin_of_text: Some(s.primary_id().to_owned()),
                bounding_box: Some(text_box),
                ..Default::default()
            });
            glyphs.push((tid, Item::SpeciesName(s)));
        }
    }

    // Drawn compartments (S §7.9), with a note for a custom style. A hidden
    // one gets no box, so no reader draws it; import reads a compartment the
    // layout doesn't draw as hidden.
    for c in model.compartments.iter().filter(|c| !c.hidden) {
        let Some(f) = c.frame else { continue };
        let mut g = sb::CompartmentGlyph {
            id: ids.unique(format!("cg_{}", c.id)),
            compartment: Some(c.id.clone()),
            bounding_box: Some(sb::BoundingBox::new(
                f.min().x,
                f.min().y,
                f.width,
                f.height,
            )),
            ..Default::default()
        };
        g.meta.annotation = super::shape::compartment_note(c);
        // Its name, in the box the app draws it in (S §7.9).
        let (a, b) = crate::render::draw::compartment_label_box(c, &f);
        let tid = ids.unique(format!("tg_{}", c.id));
        l.text_glyphs.push(sb::TextGlyph {
            id: tid.clone(),
            graphical_object: Some(g.id.clone()),
            origin_of_text: Some(c.id.clone()),
            bounding_box: Some(sb::BoundingBox::new(a.x, a.y, b.x - a.x, b.y - a.y)),
            ..Default::default()
        });
        glyphs.push((g.id.clone(), Item::Compartment(c, f)));
        glyphs.push((tid, Item::CompartmentName(c)));
        l.compartment_glyphs.push(g);
    }

    // Free text: a text glyph with its words, its box, and a note on how it
    // looks (S §7.8).
    for (t, lay) in &texts {
        let mut g = sb::TextGlyph {
            id: ids.unique(format!("tg_{}", t.id)),
            text: Some(t.text.clone()),
            bounding_box: Some(sb::BoundingBox::new(
                t.position.x,
                t.position.y,
                lay.width,
                lay.height,
            )),
            ..Default::default()
        };
        g.meta.annotation = Some(super::shape::text_note(t));
        glyphs.push((g.id.clone(), Item::Text(t)));
        l.text_glyphs.push(g);
    }

    for r in model.reactions() {
        let j = effective_junction(model, r);
        let mut g = sb::ReactionGlyph {
            id: ids.unique(format!("rg_{}", r.id)),
            reaction: Some(r.id.clone()),
            // The junction dot's box, so a reader can draw it (S §5.8); a
            // point where none is drawn.
            bounding_box: Some(if super::appearance::junction_drawn(r) {
                sb::BoundingBox::new(
                    j.x - JUNCTION_RADIUS,
                    j.y - JUNCTION_RADIUS,
                    2.0 * JUNCTION_RADIUS,
                    2.0 * JUNCTION_RADIUS,
                )
            } else {
                sb::BoundingBox::new(j.x, j.y, 0.0, 0.0)
            }),
            // Its stem (S §5.13) is the glyph's own curve: from the
            // reactants' end through the junction to the products'.
            curve: crate::render::draw::stem_line(r).map(|[a, m, b]| sb::Curve {
                segments: vec![
                    sb::Segment::Line {
                        start: point(a),
                        end: point(m),
                    },
                    sb::Segment::Line {
                        start: point(m),
                        end: point(b),
                    },
                ],
            }),
            ..Default::default()
        };
        // Linear, and smooth junctions, which the curves can't say.
        g.meta.annotation = super::shape::note(r);
        for leg in legs.iter().filter(|l| l.reaction == r.id) {
            let p = &r.participants(leg.role)[leg.index];
            let Some(node) = model.find_species(&p.species) else {
                continue;
            };
            let role = match leg.role {
                Role::Reactant => sb::Role::Substrate,
                Role::Product => sb::Role::Product,
                Role::Modifier => match leg.kind {
                    ModifierKind::Activator => sb::Role::Activator,
                    ModifierKind::Inhibitor => sb::Role::Inhibitor,
                    ModifierKind::Modifier | ModifierKind::Catalyst => sb::Role::Modifier,
                },
            };
            let sid = ids.unique(format!("srg_{}_{}_{}", r.id, leg.tag, node.id));
            glyphs.push((sid.clone(), Item::Leg(r, leg.role, p)));
            g.species_reference_glyphs.push(sb::SpeciesReferenceGlyph {
                id: sid,
                species_glyph: glyph_of[node.id.as_str()].clone(),
                species_reference: leg.reference.clone(),
                role: Some(role),
                // A modifier line's own look (S §5.7.2).
                meta: sb::Meta {
                    annotation: (leg.role == Role::Modifier)
                        .then(|| super::shape::modifier_note(&p.look))
                        .flatten(),
                    ..Default::default()
                },
                curve: Some(sb::Curve {
                    segments: leg_segments(model, r, leg.role, leg.index).unwrap_or_default(),
                }),
                ..Default::default()
            });
        }
        // A name shown: a text glyph of the reaction's id on its glyph, in
        // the name's own box (S §5.12).
        glyphs.push((g.id.clone(), Item::Reaction(r)));
        if let Some((a, b)) = crate::render::draw::reaction_label_box(model, r) {
            let tid = ids.unique(format!("tg_{}", r.id));
            glyphs.push((tid.clone(), Item::ReactionName(r)));
            l.text_glyphs.push(sb::TextGlyph {
                id: tid,
                graphical_object: Some(g.id.clone()),
                origin_of_text: Some(r.id.clone()),
                bounding_box: Some(sb::BoundingBox::new(a.x, a.y, b.x - a.x, b.y - a.y)),
                ..Default::default()
            });
        }
        l.reaction_glyphs.push(g);
    }
    // How it all looks (docs/render-design.md).
    l.render = Some(super::appearance::render_list(model, &glyphs));
    l
}

/// A participant's line as SBML curve segments, in the direction of flow: a
/// reactant's and a modifier's from the species to the reaction, a product's
/// from the reaction to the species (S §11.5). The drawn part only: the piece
/// at the species is cut at its border. `None` if its node is missing.
/// Antimony's layout statements are written from this too
/// (`antimony_layout_out`), so both say the same drawing.
pub(crate) fn leg_segments(
    model: &Model,
    r: &crate::model::Reaction,
    role: Role,
    index: usize,
) -> Option<Vec<sb::Segment>> {
    let p = &r.participants(role)[index];
    let node = model.find_species(&p.species)?;
    let j = effective_junction(model, r);
    let bezier = effective_mode(r) == ReactionMode::Bezier;
    // A modifier's arc through its corner points (S §5.7.1).
    if role == Role::Modifier
        && let Some(route) = modifier_route(model, r, p)
    {
        return Some(
            route
                .windows(2)
                .map(|w| sb::Segment::Line {
                    start: point(w[0]),
                    end: point(w[1]),
                })
                .collect(),
        );
    }
    // A leg meets the reaction at its end of the stem, if it has
    // one (S §5.13).
    let j = if crate::render::draw::stem_line(r).is_some() {
        r.leg_anchor(role)
    } else {
        j
    };
    // A line with corner points (S §5.6.5), unless the reaction is
    // drawn Linear: its end piece runs to the first of them, then
    // straight pieces run on to `j`.
    let corners: &[Pt] = if role != Role::Modifier && effective_mode(r) != ReactionMode::Linear {
        &p.points
    } else {
        &[]
    };
    let inner = corners.first().copied().unwrap_or(j);
    let edge = border(node, inner);
    let segment = match (
        role,
        bezier.then(|| leg_curve(model, r, role, index)).flatten(),
    ) {
        // The part of the curve that is drawn: it runs to the node's
        // centre and is cut where it crosses the node's border
        // (S §5.6.3), so a reader draws the same curve and points an
        // arrowhead along it. Its base points are the drawn part's;
        // import carries it on to the centre again (S §11.5).
        (role, Some(c)) => {
            let t = crate::geometry::bezier_crossing(&c, |p| crate::node_shape::contains(node, p));
            let (first, second) = c.split(t);
            let d = if role == Role::Reactant {
                second
            } else {
                first
            };
            sb::Segment::Bezier {
                start: point(d.p0),
                end: point(d.p3),
                base1: point(d.p1),
                base2: point(d.p2),
            }
        }
        (Role::Product, None) => sb::Segment::Line {
            start: point(inner),
            end: point(edge),
        },
        _ => sb::Segment::Line {
            start: point(edge),
            end: point(inner),
        },
    };
    // The whole line in the direction of flow: a reactant's from the
    // species through its corner points to `j`, a product's from `j`
    // back through them to the species.
    let run: Vec<sb::Segment> = {
        let mut pts: Vec<Pt> = corners.to_vec();
        pts.push(j);
        if role == Role::Product {
            pts.reverse();
        }
        pts.windows(2)
            .map(|w| sb::Segment::Line {
                start: point(w[0]),
                end: point(w[1]),
            })
            .collect()
    };
    let leg_segments: Vec<sb::Segment> = match role {
        Role::Reactant => std::iter::once(segment).chain(run).collect(),
        _ => run.into_iter().chain(std::iter::once(segment)).collect(),
    };
    Some(leg_segments)
}

/// An item's SBO term, notes and database references (and the model's
/// history) as SBML (docs/annotations-design.md). The metaid the RDF hangs
/// on is the item's id, which is unique among them.
fn meta(id: &str, a: &crate::model::Annotation, history: &History) -> sb::Meta {
    let mut terms: Vec<sb::miriam::CvTerm> = Vec::new();
    for r in &a.references {
        let (model, qualifier) = match r.qualifier.split_once(':') {
            Some(("bqmodel", q)) => (true, q),
            Some((_, q)) => (false, q),
            None => (false, r.qualifier.as_str()),
        };
        // References with one qualifier, one after another, share its Bag.
        match terms.last_mut() {
            Some(t) if t.model == model && t.qualifier == qualifier => {
                t.resources.push(r.resource.clone())
            }
            _ => terms.push(sb::miriam::CvTerm {
                model,
                qualifier: qualifier.to_owned(),
                resources: vec![r.resource.clone()],
            }),
        }
    }
    let history = sb::miriam::History {
        creators: history
            .creators
            .iter()
            .map(|c| sb::miriam::Creator {
                family: c.family.clone(),
                given: c.given.clone(),
                email: c.email.clone(),
                organization: c.organization.clone(),
            })
            .collect(),
        created: history.created.clone(),
        modified: history.modified.clone(),
    };
    let annotation = sb::miriam::write_annotation(id, &terms, &history);
    sb::Meta {
        metaid: annotation.is_some().then(|| id.to_owned()),
        sbo_term: a.sbo.map(|n| format!("SBO:{n:07}")),
        notes: a.notes.clone(),
        annotation,
        ..Default::default()
    }
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

    /// docs/annotations-design.md: SBO terms, database references and notes
    /// on species, reactions, compartments, parameters and the model, and the
    /// model's history, come back through SBML; another tool's own
    /// annotation doesn't, and the file says so once (A3, A8).
    #[test]
    fn annotations_round_trip_and_other_tools_are_reported() {
        let opts = crate::antimony::ImportOptions {
            convert_catalytic: false,
            seed: 1,
        };
        let sbml = r##"<?xml version="1.0" encoding="UTF-8"?>
<sbml xmlns="http://www.sbml.org/sbml/level3/version1/core" level="3" version="1"
      xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#">
  <model id="m" metaid="m" sboTerm="SBO:0000004">
    <annotation>
      <rdf:RDF xmlns:dcterms="http://purl.org/dc/terms/" xmlns:vCard="http://www.w3.org/2001/vcard-rdf/3.0#"
               xmlns:bqmodel="http://biomodels.net/model-qualifiers/">
        <rdf:Description rdf:about="#m">
          <dcterms:creator><rdf:Bag><rdf:li rdf:parseType="Resource">
            <vCard:N rdf:parseType="Resource"><vCard:Family>Elowitz</vCard:Family><vCard:Given>Michael</vCard:Given></vCard:N>
          </rdf:li></rdf:Bag></dcterms:creator>
          <dcterms:created rdf:parseType="Resource"><dcterms:W3CDTF>2005-02-02T14:56:11Z</dcterms:W3CDTF></dcterms:created>
          <bqmodel:isDescribedBy><rdf:Bag><rdf:li rdf:resource="http://identifiers.org/pubmed/10659856"/></rdf:Bag></bqmodel:isDescribedBy>
        </rdf:Description>
      </rdf:RDF>
    </annotation>
    <listOfCompartments>
      <compartment id="c" size="1" constant="true" sboTerm="SBO:0000290"/>
    </listOfCompartments>
    <listOfSpecies>
      <species id="S" metaid="_S" compartment="c" sboTerm="SBO:0000247" initialConcentration="1"
               hasOnlySubstanceUnits="false" boundaryCondition="false" constant="false">
        <notes><body xmlns="http://www.w3.org/1999/xhtml"><p>Glucose.</p></body></notes>
        <annotation>
          <rdf:RDF xmlns:bqbiol="http://biomodels.net/biology-qualifiers/">
            <rdf:Description rdf:about="#_S">
              <bqbiol:is><rdf:Bag>
                <rdf:li rdf:resource="http://identifiers.org/chebi/CHEBI:17234"/>
                <rdf:li rdf:resource="urn:miriam:kegg.compound:C00031"/>
              </rdf:Bag></bqbiol:is>
            </rdf:Description>
          </rdf:RDF>
          <COPASI xmlns="http://www.copasi.org/static/sbml"><x/></COPASI>
        </annotation>
      </species>
      <species id="P" compartment="c" initialConcentration="0"
               hasOnlySubstanceUnits="false" boundaryCondition="false" constant="false"/>
    </listOfSpecies>
    <listOfParameters>
      <parameter id="k" value="1" constant="true" sboTerm="SBO:0000035">
        <notes><body xmlns="http://www.w3.org/1999/xhtml"><p>A rate constant.</p></body></notes>
      </parameter>
      <parameter id="kr" constant="false" sboTerm="SBO:0000186"/>
    </listOfParameters>
    <listOfRules>
      <assignmentRule variable="kr">
        <math xmlns="http://www.w3.org/1998/Math/MathML"><apply><times/><cn>2</cn><ci>k</ci></apply></math>
      </assignmentRule>
    </listOfRules>
    <listOfReactions>
      <reaction id="R" reversible="false" sboTerm="SBO:0000176">
        <listOfReactants><speciesReference species="S" stoichiometry="1" constant="true"/></listOfReactants>
        <listOfProducts><speciesReference species="P" stoichiometry="1" constant="true"/></listOfProducts>
      </reaction>
    </listOfReactions>
  </model>
</sbml>"##;
        let i = crate::sbml::import(sbml, &opts).unwrap();
        // Only COPASI's own XML, on one item, isn't kept.
        let lost: Vec<&String> = i
            .warnings
            .iter()
            .filter(|w| w.contains("aren't kept"))
            .collect();
        assert_eq!(lost.len(), 1, "{:?}", i.warnings);
        assert!(
            lost[0].starts_with("another tool's own annotations on 1 item"),
            "{}",
            lost[0]
        );

        let check = |m: &Model| {
            let s = &m.find_species("S").unwrap().annotation;
            assert_eq!(s.sbo, Some(247));
            let refs: Vec<(&str, &str)> = s
                .references
                .iter()
                .map(|r| (r.qualifier.as_str(), r.resource.as_str()))
                .collect();
            assert_eq!(
                refs,
                [
                    ("bqbiol:is", "http://identifiers.org/chebi/CHEBI:17234"),
                    ("bqbiol:is", "urn:miriam:kegg.compound:C00031")
                ]
            );
            assert!(s.notes.as_deref().unwrap().contains("Glucose."));
            assert_eq!(m.find_compartment("c").unwrap().annotation.sbo, Some(290));
            assert_eq!(m.find_reaction("R").unwrap().annotation.sbo, Some(176));
            let k = &m.find_parameter("k").unwrap().annotation;
            assert_eq!(k.sbo, Some(35));
            assert!(k.notes.as_deref().unwrap().contains("A rate constant."));
            // A parameter set by a rule is kept as the rule, with its annotation.
            let kr = m
                .assignment_rules
                .iter()
                .find(|r| r.variable == "kr")
                .unwrap();
            assert_eq!(kr.annotation.sbo, Some(186));
            assert_eq!(m.annotation.sbo, Some(4));
            assert_eq!(
                m.annotation.references[0].qualifier,
                "bqmodel:isDescribedBy"
            );
            assert_eq!(m.history.creators[0].family.as_deref(), Some("Elowitz"));
            assert_eq!(m.history.created.as_deref(), Some("2005-02-02T14:56:11Z"));
        };
        check(&i.model);
        // Written and read again: the same, and nothing more lost. The RDF
        // hangs on each item's metaid, its id.
        let text = export(&i.model).xml;
        assert!(text.contains(r#"metaid="S""#), "{text}");
        assert!(text.contains(r##"rdf:about="#S""##), "{text}");
        let back = crate::sbml::import(&text, &opts).unwrap();
        assert!(
            back.warnings.iter().all(|w| !w.contains("aren't kept")),
            "{:?}",
            back.warnings
        );
        check(&back.model);
    }

    /// Display names go out as SBML `name` and come back (D1 of
    /// docs/display-names-design.md); an alias has none of its own.
    #[test]
    fn display_names_round_trip() {
        use crate::geometry::pt;
        use crate::model::{Compartment, Participant, Reaction};
        let mut m = Model::new();
        let mut c = Compartment::new("comp_0");
        c.display_name = Some("astrocyte".into());
        m.compartments.push(c);
        let a = m.add_species(pt(0.0, 0.0), 60.0, 30.0);
        let b = m.add_species(pt(200.0, 0.0), 60.0, 30.0);
        m.find_species_mut(&a).unwrap().display_name = Some("L-glutamate".into());
        let alias = m.add_alias(&a, pt(0.0, 100.0)).unwrap();
        let mut r = Reaction::new("re12", pt(100.0, 0.0));
        r.reactants.push(Participant::new(a.as_str(), 1.0));
        r.products.push(Participant::new(b.as_str(), 1.0));
        r.display_name = Some("glutamate uptake".into());
        m.push_reaction(r).unwrap();

        let text = export(&m).xml;
        assert!(text.contains("name=\"L-glutamate\""), "{text}");
        let opts = crate::antimony::ImportOptions {
            convert_catalytic: false,
            seed: 1,
        };
        let back = crate::sbml::import(&text, &opts).unwrap().model;
        let name = |id: &str| back.find_species(id).unwrap().display_name.clone();
        assert_eq!(name(&a).as_deref(), Some("L-glutamate"));
        assert_eq!(name(&b), None);
        assert_eq!(name(&alias), None);
        assert_eq!(
            back.find_reaction("re12").unwrap().display_name.as_deref(),
            Some("glutamate uptake")
        );
        assert_eq!(
            back.find_compartment("comp_0")
                .unwrap()
                .display_name
                .as_deref(),
            Some("astrocyte")
        );
    }
}
