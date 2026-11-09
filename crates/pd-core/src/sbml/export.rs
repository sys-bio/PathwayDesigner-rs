//! The model as SBML Level 3 with the Layout and Render packages (S §11.5),
//! everything it holds written as it was read
//! (docs/sbml-full-support-design.md).

use std::collections::{HashMap, HashSet};

use sbml_io as sb;

use super::math::to_math_with;
use crate::bezier::leg_curve;
use crate::expr;
use crate::geometry::Pt;
use crate::model::{
    Annotation, DEFAULT_COMPARTMENT, History, Model, ModifierKind, PackageData, ReactionMode, Role,
    SpeciesNode,
};
use crate::render::constants::JUNCTION_RADIUS;
use crate::render::draw::{border, effective_junction, effective_mode, modifier_route};
use crate::textbox::lay_out;

use super::appearance::Item;

const LEVEL: u32 = 3;

/// The SBML version a model is written in: Level 3 Version 2 (F5), but
/// Version 1 for what Version 2 forbids: a fast reaction (Version 2 has no
/// `fast`), or a rate law's local parameter named as a species of its
/// reaction (found by the SBML Test Suite round trip); and a model with
/// another package's XML kept as read (the `comp` package's model
/// definitions) in the version it was read from, whose core that XML uses.
fn version_for(model: &Model) -> u32 {
    let shadows = |r: &crate::model::Reaction| {
        r.sbml.local_parameters.iter().any(|p| {
            r.reactants
                .iter()
                .chain(&r.products)
                .chain(&r.modifiers)
                .filter_map(|x| model.find_species(&x.species))
                .any(|s| s.primary_id() == p.id)
        })
    };
    if model.reactions().iter().any(|r| r.sbml.fast || shadows(r)) {
        return 1;
    }
    let raw = !model.sbml_document.elements.is_empty()
        || model
            .annotations()
            .iter()
            .any(|a| !a.package.elements.is_empty());
    match model.sbml_version {
        Some(v) if raw => v,
        _ => 2,
    }
}
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
    // Genes and gene rules into the fbc XML kept with the model
    // (docs/gene-rules-design.md).
    let with_genes;
    let model = if !model.genes.is_empty()
        || model.reactions().iter().any(|r| r.sbml.gene_rule.is_some())
        || model
            .annotation
            .package
            .elements
            .iter()
            .any(|x| x.contains("listOfGeneProducts"))
    {
        let mut m = model.clone();
        crate::genes::to_packages(&mut m);
        with_genes = m;
        &with_genes
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
    let version = version_for(model);
    // A name the model uses for an item of its own means that item, not
    // SBML's constant of that name (`avogadro`, `pi`, …).
    let own: HashSet<&str> = model
        .species()
        .iter()
        .map(|s| s.id.as_str())
        .chain(model.reactions().iter().map(|r| r.id.as_str()))
        .chain(model.compartments.iter().map(|c| c.id.as_str()))
        .chain(model.parameters.iter().map(|p| p.variable.as_str()))
        .chain(model.function_definitions.iter().map(|f| f.id.as_str()))
        .collect();
    // A rate law's local parameter of that name too.
    let locals: HashSet<&str> = model
        .reactions()
        .iter()
        .flat_map(|r| r.sbml.local_parameters.iter().map(|p| p.id.as_str()))
        .collect();
    let taken = |n: &str| own.contains(n) || locals.contains(n);
    let mut ids = Ids(HashSet::new());
    // Every id the model uses is taken before any is generated.
    for id in &own {
        ids.0.insert((*id).to_owned());
    }
    for r in &model.assignment_rules {
        ids.0.insert(r.variable.clone());
    }
    for e in &model.events {
        if let Some(id) = &e.id {
            ids.0.insert(id.clone());
        }
    }
    for r in model.reactions() {
        for p in r.reactants.iter().chain(&r.products).chain(&r.modifiers) {
            if let Some(id) = p.reference_id() {
                ids.0.insert(id.to_owned());
            }
        }
    }
    ids.0.insert(DEFAULT_COMPARTMENT.into());

    // Function bodies, whose arguments mean themselves there, not SBML's
    // constants (a function with an argument `inf`, found by the BioModels
    // round trip).
    let mut bodies: Vec<Option<sb::Math>> = Vec::new();
    for f in &model.function_definitions {
        bodies.push(match expr::parse(f.body.trim()) {
            _ if f.body.trim().is_empty() => None,
            Ok(ast) => Some(to_math_with(&ast, LEVEL, version, &|n: &str| {
                taken(n) || f.arguments.iter().any(|a| a == n)
            })),
            Err(e) => {
                problems.push(format!(
                    "The function {} doesn't parse, so it was left out: {e}",
                    f.id
                ));
                None
            }
        });
    }

    let mut parse = |text: &str, what: &str| -> Option<sb::Math> {
        let text = text.trim();
        if text.is_empty() {
            return None;
        }
        match expr::parse(text) {
            Ok(ast) => Some(to_math_with(&ast, LEVEL, version, &taken)),
            Err(e) => {
                problems.push(format!("{what} doesn't parse, so it was left out: {e}"));
                None
            }
        }
    };

    // The id and name as read: a file may give the model neither.
    let model_id = match &model.model_id {
        Some(id) => {
            ids.0.insert(id.clone());
            Some(id.clone())
        }
        None if model.model_id_unset => None,
        None => Some(ids.unique(sid(&model.model_name))),
    };
    let name = (!model.model_name.trim().is_empty() && !model.model_name_unset)
        .then(|| model.model_name.clone());
    let mut m = sb::Model {
        id: model_id.clone(),
        name,
        substance_units: model.units.substance.clone(),
        time_units: model.units.time.clone(),
        volume_units: model.units.volume.clone(),
        area_units: model.units.area.clone(),
        length_units: model.units.length.clone(),
        extent_units: model.units.extent.clone(),
        conversion_factor: model.conversion_factor.clone(),
        ..Default::default()
    };
    let mut metaids = Metaids::new(model);
    m.meta = meta(
        &mut metaids,
        model_id.as_deref().unwrap_or("model"),
        &model.annotation,
        &model.history,
    );

    for f in &model.function_definitions {
        let body = bodies.remove(0);
        m.function_definitions.push(sb::FunctionDefinition {
            meta: meta(&mut metaids, &f.id, &f.annotation, &f.annotation.history),
            id: f.id.clone(),
            name: f.name.clone(),
            math: body.map(|body| sb::Math::Lambda {
                args: f.arguments.clone(),
                body: Box::new(body),
            }),
        });
    }
    for u in &model.unit_definitions {
        m.unit_definitions.push(sb::UnitDefinition {
            meta: meta(&mut metaids, &u.id, &u.annotation, &u.annotation.history),
            id: u.id.clone(),
            name: u.name.clone(),
            units: u
                .units
                .iter()
                .map(|x| sb::Unit {
                    meta: meta(&mut metaids, &u.id, &x.annotation, &x.annotation.history),
                    kind: x.kind.clone(),
                    exponent: Some(x.exponent),
                    scale: Some(x.scale),
                    multiplier: Some(x.multiplier),
                    offset: None,
                })
                .collect(),
        });
    }

    // What changes while the model runs, so isn't constant: what a rule or
    // an event sets (found by checking the BioModels round trip with libSBML).
    let changed = |id: &str| {
        model.assignment_rules.iter().any(|r| r.variable == id)
            || model.rate_rules.iter().any(|r| r.variable == id)
            || model
                .events
                .iter()
                .any(|e| e.assignments.iter().any(|a| a.variable == id))
    };

    // Compartments: the default one if a species is in it and the model
    // doesn't have it, then the model's.
    let in_default = model
        .species()
        .iter()
        .any(|s| !s.is_alias() && !s.is_null_node && s.compartment.is_empty());
    if in_default && model.find_compartment(DEFAULT_COMPARTMENT).is_none() {
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
            meta: meta(&mut metaids, &c.id, &c.annotation, &c.annotation.history),
            id: c.id.clone(),
            name: c.display_name.clone(),
            size: (!c.sbml.size_unset).then_some(c.size),
            spatial_dimensions: (!c.sbml.dimensions_unset).then_some(c.dimensions),
            units: c.sbml.units.clone(),
            constant: Some(c.sbml.constant && !changed(&c.id)),
            ..Default::default()
        });
    }

    // Species: primaries only, no null nodes (D-9).
    for s in model
        .species()
        .iter()
        .filter(|s| !s.is_alias() && !s.is_null_node)
    {
        let value = (!s.sbml.initial_unset).then_some(s.initial_value);
        m.species.push(sb::Species {
            meta: meta(&mut metaids, &s.id, &s.annotation, &s.annotation.history),
            id: s.id.clone(),
            name: s.display_name.clone(),
            compartment: if s.compartment.is_empty() {
                DEFAULT_COMPARTMENT.into()
            } else {
                s.compartment.clone()
            },
            initial_concentration: value.filter(|_| !s.initial_is_amount),
            initial_amount: value.filter(|_| s.initial_is_amount),
            substance_units: s.sbml.units.clone(),
            has_only_substance_units: Some(s.in_amounts),
            boundary_condition: Some(s.is_boundary),
            constant: Some(s.is_constant),
            conversion_factor: s.sbml.conversion_factor.clone(),
            ..Default::default()
        });
    }

    // Parameters, with their values (D-10).
    for p in &model.parameters {
        m.parameters.push(sb::Parameter {
            meta: meta(
                &mut metaids,
                &p.variable,
                &p.annotation,
                &p.annotation.history,
            ),
            id: p.variable.clone(),
            name: p.name.clone(),
            value: p.value,
            units: p.units.clone(),
            constant: Some(p.constant && !changed(&p.variable)),
        });
    }
    // A rule's variable that is nothing else, as an older model may have,
    // needs a parameter to set.
    for r in &model.assignment_rules {
        // Exactly: a compartment `d` doesn't declare a rule's `D` (found by
        // the BioModels round trip).
        let declared = model.find_species(&r.variable).is_some()
            || model.compartments.iter().any(|c| c.id == r.variable)
            || model.parameters.iter().any(|p| p.variable == r.variable)
            || model.reactions().iter().any(|x| {
                x.reactants
                    .iter()
                    .chain(&x.products)
                    .any(|p| p.reference_id() == Some(r.variable.as_str()))
            });
        if !declared && !m.parameters.iter().any(|p| p.id == r.variable) {
            m.parameters.push(sb::Parameter {
                id: r.variable.clone(),
                constant: Some(false),
                ..Default::default()
            });
        }
    }

    for a in &model.initial_assignments {
        let math = parse(&a.expression, &format!("The initial value of {}", a.symbol));
        m.initial_assignments.push(sb::InitialAssignment {
            meta: meta(&mut metaids, "metaid", &a.annotation, &a.annotation.history),
            symbol: a.symbol.clone(),
            math,
        });
    }
    for r in &model.assignment_rules {
        let math = parse(&r.expression, &format!("The rule for {}", r.variable));
        m.rules.push(sb::Rule::Assignment {
            meta: meta(&mut metaids, "metaid", &r.annotation, &r.annotation.history),
            variable: r.variable.clone(),
            math,
        });
    }
    for r in &model.rate_rules {
        let math = parse(&r.expression, &format!("The rate rule for {}", r.variable));
        m.rules.push(sb::Rule::Rate {
            meta: meta(&mut metaids, "metaid", &r.annotation, &r.annotation.history),
            variable: r.variable.clone(),
            math,
        });
    }
    for r in &model.algebraic_rules {
        let math = parse(&r.expression, "An algebraic rule");
        m.rules.push(sb::Rule::Algebraic {
            meta: meta(&mut metaids, "metaid", &r.annotation, &r.annotation.history),
            math,
        });
    }
    for c in &model.constraints {
        let math = parse(&c.expression, "A check");
        m.constraints.push(sb::Constraint {
            meta: meta(&mut metaids, "metaid", &c.annotation, &c.annotation.history),
            math,
            message: c.message.clone(),
        });
    }
    for e in &model.events {
        let what = e.description();
        let t = &e.trigger;
        let trigger_math = parse(&t.expression, &what);
        // An event SBML lets go without a trigger is written without one.
        let has_trigger = !e.no_trigger;
        let delay = e.delay.as_ref().map(|d| sb::Delay {
            meta: meta(&mut metaids, "metaid", &d.annotation, &d.annotation.history),
            math: parse(&d.expression, &what),
        });
        let priority = e.priority.as_ref().map(|p| sb::Priority {
            meta: meta(&mut metaids, "metaid", &p.annotation, &p.annotation.history),
            math: parse(&p.expression, &what),
        });
        let assignments = e
            .assignments
            .iter()
            .map(|a| sb::EventAssignment {
                meta: meta(&mut metaids, "metaid", &a.annotation, &a.annotation.history),
                variable: a.variable.clone(),
                math: parse(&a.expression, &what),
            })
            .collect();
        m.events.push(sb::Event {
            meta: meta(
                &mut metaids,
                e.id.as_deref().unwrap_or("metaid"),
                &e.annotation,
                &e.annotation.history,
            ),
            id: e.id.clone(),
            name: e.name.clone(),
            use_values_from_trigger_time: Some(e.use_values_from_trigger_time),
            time_units: None,
            trigger: has_trigger.then(|| sb::Trigger {
                meta: meta(&mut metaids, "metaid", &t.annotation, &t.annotation.history),
                initial_value: Some(t.initial_value),
                persistent: Some(t.persistent),
                math: trigger_math,
            }),
            delay,
            priority,
            assignments,
        });
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
            meta: meta(&mut metaids, &r.id, &r.annotation, &r.annotation.history),
            id: r.id.clone(),
            name: r.display_name.clone(),
            reversible: Some(r.is_reversible),
            // Version 2 has no `fast`; a fast reaction makes it Version 1.
            fast: (version == 1).then_some(r.sbml.fast),
            compartment: r.sbml.compartment.clone(),
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
                        // Its own id if it has one; else one made up, so its
                        // glyph can name it.
                        let info = p.sbml.as_deref().cloned().unwrap_or_default();
                        let sr_id = match &info.id {
                            Some(id) => id.clone(),
                            None => ids.unique(format!("sr_{}_{tag}_{}", r.id, s.id)),
                        };
                        let mut sr_meta = meta(
                            &mut metaids,
                            &sr_id,
                            &info.annotation,
                            &info.annotation.history,
                        );
                        if role == Role::Modifier {
                            // The SBO term as read while it still says the
                            // kind drawn; else the kind's own.
                            let read = info
                                .annotation
                                .sbo
                                .map(|n| format!("SBO:{n:07}"))
                                .filter(|t| ModifierKind::from_sbo(t) == p.kind);
                            sr_meta.sbo_term =
                                read.or_else(|| p.kind.sbo_term().map(str::to_owned));
                            rx.modifiers.push(sb::ModifierSpeciesReference {
                                id: Some(sr_id.clone()),
                                name: info.name.clone(),
                                species: s.id.clone(),
                                meta: sr_meta,
                            });
                        } else {
                            let sr = sb::SpeciesReference {
                                id: Some(sr_id.clone()),
                                name: info.name.clone(),
                                species: s.id.clone(),
                                stoichiometry: (!info.stoichiometry_unset)
                                    .then_some(p.stoichiometry),
                                constant: Some(info.constant),
                                meta: sr_meta,
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
        // The rate law, with its local parameters, if it has anything.
        let math = parse(&r.kinetic_law, &format!("The rate law of {}", r.id));
        let law_annotation = &r.sbml.kinetic_law_annotation;
        if math.is_some()
            || !r.sbml.local_parameters.is_empty()
            || !law_annotation.is_empty()
            || law_annotation.metaid.is_some()
        {
            rx.kinetic_law = Some(sb::KineticLaw {
                meta: meta(
                    &mut metaids,
                    "metaid",
                    law_annotation,
                    &law_annotation.history,
                ),
                math,
                parameters: r
                    .sbml
                    .local_parameters
                    .iter()
                    .map(|lp| sb::LocalParameter {
                        meta: meta(&mut metaids, &lp.id, &lp.annotation, &lp.annotation.history),
                        id: lp.id.clone(),
                        name: lp.name.clone(),
                        value: lp.value,
                        units: lp.units.clone(),
                    })
                    .collect(),
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
    defined.extend(m.reactions.iter().flat_map(|r| {
        r.reactants
            .iter()
            .chain(&r.products)
            .filter_map(|p| p.id.clone())
    }));
    let mut used = Vec::new();
    // Each formula, with the names that are its own: a rate law's local
    // parameters.
    let none: HashSet<String> = HashSet::new();
    let mut maths: Vec<(&sb::Math, HashSet<String>)> = Vec::new();
    for x in m
        .initial_assignments
        .iter()
        .filter_map(|ia| ia.math.as_ref())
        .chain(m.rules.iter().filter_map(|r| r.math()))
        .chain(m.constraints.iter().filter_map(|c| c.math.as_ref()))
        .chain(m.events.iter().flat_map(|e| {
            e.trigger
                .iter()
                .filter_map(|t| t.math.as_ref())
                .chain(e.delay.iter().filter_map(|d| d.math.as_ref()))
                .chain(e.priority.iter().filter_map(|p| p.math.as_ref()))
                .chain(e.assignments.iter().filter_map(|a| a.math.as_ref()))
        }))
    {
        maths.push((x, none.clone()));
    }
    for r in &m.reactions {
        if let Some(k) = &r.kinetic_law
            && let Some(x) = &k.math
        {
            maths.push((x, k.parameters.iter().map(|p| p.id.clone()).collect()));
        }
    }
    // Called function names aren't values, so they don't count.
    let called: HashSet<String> = maths
        .iter()
        .flat_map(|(x, _)| called_functions(x))
        .chain(m.function_definitions.iter().map(|f| f.id.clone()))
        .collect();
    for (math, own) in &maths {
        math.visit_idents(&mut |id| {
            if !defined.contains(id)
                && !own.contains(id)
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
    let mut doc = sb::Document::new(LEVEL, version);
    doc.namespaces = model.sbml_namespaces.clone();
    doc.meta.extra = extra(&model.sbml_document);
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
            let (text, origin_of_text) = words(&s.label_text, s.primary_id());
            l.text_glyphs.push(sb::TextGlyph {
                id: tid.clone(),
                graphical_object: Some(id),
                text,
                origin_of_text,
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
        let (text, origin_of_text) = words(&c.label_text, &c.id);
        l.text_glyphs.push(sb::TextGlyph {
            id: tid.clone(),
            graphical_object: Some(g.id.clone()),
            text,
            origin_of_text,
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
            let (text, origin_of_text) = words(&r.label_text, &r.id);
            l.text_glyphs.push(sb::TextGlyph {
                id: tid,
                graphical_object: Some(g.id.clone()),
                text,
                origin_of_text,
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

/// What a name's text glyph says: the drawing's label text as its own words,
/// as Reactome writes its short labels, or else the item's name, by its id
/// (docs/layout-labels-design.md L5).
fn words(label_text: &Option<String>, id: &str) -> (Option<String>, Option<String>) {
    match label_text {
        Some(t) => (Some(t.clone()), None),
        None => (None, Some(id.to_owned())),
    }
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

/// The metaids written: each item's own as read (docs/other-annotations-design.md
/// O3), the first item written keeping it when several share it (a pasted
/// copy); else one made from its id that no item read.
struct Metaids {
    /// Every metaid as read.
    read: HashSet<String>,
    used: HashSet<String>,
}

impl Metaids {
    fn new(model: &Model) -> Self {
        let mut read: HashSet<String> = HashSet::new();
        for a in model.annotations() {
            if let Some(m) = &a.metaid {
                read.insert(m.clone());
            }
        }
        Self {
            read,
            used: Default::default(),
        }
    }

    /// The metaid for the item `id` with annotation `a`.
    fn pick(&mut self, id: &str, a: &Annotation) -> String {
        if let Some(m) = &a.metaid
            && self.used.insert(m.clone())
        {
            return m.clone();
        }
        let free = |c: &str, used: &HashSet<String>| !used.contains(c) && !self.read.contains(c);
        let made = std::iter::once(id.to_owned())
            .chain((2..).map(|n| format!("{id}_{n}")))
            .find(|c| free(c, &self.used))
            .expect("some suffix is free");
        self.used.insert(made.clone());
        made
    }
}

/// An item's SBO term, notes and database references (and the model's
/// history) as SBML (docs/annotations-design.md), with another tool's own
/// XML written back as it was (docs/other-annotations-design.md). It has a
/// metaid when there is RDF, other XML, or one was read: its own as read,
/// else one made from its id ([`Metaids`]).
fn meta(metaids: &mut Metaids, id: &str, a: &Annotation, history: &History) -> sb::Meta {
    let rdf = !a.references.is_empty() || !history.is_empty() || !a.rdf.is_empty();
    let metaid = (rdf || !a.other.is_empty() || a.metaid.is_some()).then(|| metaids.pick(id, a));
    let annotation = metaid
        .as_deref()
        .and_then(|m| annotation_inner(m, a, history));
    sb::Meta {
        metaid,
        sbo_term: a.sbo.map(|n| format!("SBO:{n:07}")),
        notes: a.notes.clone(),
        annotation,
        extra: extra(&a.package),
    }
}

/// What goes inside an element's `<annotation>`, its metaid `metaid`: the
/// RDF of its references and `history`, then other tools' XML. Gene
/// products, written as fbc's raw XML, use it too
/// (docs/uniprot-genes-design.md U1).
pub(crate) fn annotation_inner(metaid: &str, a: &Annotation, history: &History) -> Option<String> {
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
    let mut annotation = sb::miriam::write_annotation(metaid, &terms, &history, &a.rdf);
    if !a.other.is_empty() {
        let mut all = annotation.unwrap_or_default();
        for x in &a.other {
            all.push_str(x);
        }
        annotation = Some(all);
    }
    annotation
}

/// What other packages put on an element, as `sbml-io` writes it (F10).
fn extra(p: &PackageData) -> sb::Extra {
    sb::Extra {
        attributes: p
            .attributes
            .iter()
            .map(|a| sb::Attribute {
                name: a.name.clone(),
                namespace: a.namespace.clone(),
                value: a.value.clone(),
            })
            .collect(),
        elements: p.elements.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Found by checking the BioModels round trip with libSBML: a compartment
    /// a rule sets is written not constant; a piece's condition that is a
    /// plain number (MathML's `<true/>` as import reads it) goes back as
    /// `<true/>` or `<false/>`; a whole number too large for 32 bits is
    /// written as a real, not an integer libSBML can't read.
    #[test]
    fn what_libsbml_found_in_the_biomodels_round_trip() {
        let mut m = Model::new();
        for id in ["fixed", "growing"] {
            m.compartments.push(crate::model::Compartment::new(id));
        }
        let rule = |variable: &str, expression: &str| crate::model::AssignmentRule {
            variable: variable.into(),
            expression: expression.into(),
            annotation: Default::default(),
        };
        m.assignment_rules.push(rule("growing", "1 + time"));
        m.assignment_rules
            .push(rule("p", "piecewise(1, 1, 0) + piecewise(2, 0, 3)"));
        m.assignment_rules
            .push(rule("q", "2000000000000 + 2147483647 + 2147483648"));
        let xml = export(&m).xml;
        let compartment = |id: &str| {
            let at = xml.find(&format!(r#"<compartment id="{id}""#)).unwrap();
            xml[at..at + xml[at..].find('>').unwrap()].to_owned()
        };
        assert!(compartment("fixed").contains(r#"constant="true""#));
        assert!(
            compartment("growing").contains(r#"constant="false""#),
            "{}",
            compartment("growing")
        );
        // The conditions: <true/> and <false/>, not numbers.
        assert_eq!(xml.matches("<true/>").count(), 1, "{xml}");
        assert_eq!(xml.matches("<false/>").count(), 1, "{xml}");
        // 2^31 - 1 is the largest integer; anything larger is real.
        assert!(
            xml.contains(r#"<cn type="integer"> 2147483647 </cn>"#),
            "{xml}"
        );
        assert!(
            !xml.contains(r#"<cn type="integer"> 2147483648 </cn>"#),
            "{xml}"
        );
        assert!(
            !xml.contains(r#"<cn type="integer"> 2000000000000 </cn>"#),
            "{xml}"
        );
        // Read back, the same values.
        let opts = crate::antimony::ImportOptions { seed: 1 };
        let back = crate::sbml::import(&xml, &opts).unwrap().model;
        let q = back
            .assignment_rules
            .iter()
            .find(|r| r.variable == "q")
            .unwrap();
        let v = crate::expr::evaluate(&crate::expr::parse(&q.expression).unwrap(), &|_| None);
        assert_eq!(
            v,
            Some(2000000000000.0 + 2147483647.0 + 2147483648.0),
            "{}",
            q.expression
        );
        // The first piecewise takes its piece, the second its otherwise.
        let p = back
            .assignment_rules
            .iter()
            .find(|r| r.variable == "p")
            .unwrap();
        let v = crate::expr::evaluate(&crate::expr::parse(&p.expression).unwrap(), &|_| None);
        assert_eq!(v, Some(1.0 + 3.0), "{}", p.expression);
    }

    /// Level 3 Version 2 (F5), with `rateOf` and `max` as SBML says them;
    /// Version 1 only for what Version 2 can't hold: a fast reaction, or
    /// another package's XML read from Version 1.
    #[test]
    fn version_1_only_when_needed() {
        let mut m = Model::new();
        m.parameters
            .push(crate::model::Parameter::new("k", Some(2.0)));
        m.parameters.push(crate::model::Parameter {
            constant: false,
            ..crate::model::Parameter::new("p", None)
        });
        m.assignment_rules.push(crate::model::AssignmentRule {
            variable: "p".into(),
            expression: "2*rate(k) + max(k, 1)".into(),
            annotation: Default::default(),
        });
        let v2 = export(&m);
        assert!(
            v2.xml.contains(r#"level="3" version="2""#),
            "{}",
            &v2.xml[..300]
        );
        assert!(v2.xml.contains("level3/version2/core"));
        assert!(v2.xml.contains("rateOf"));
        assert!(v2.problems.is_empty(), "{:?}", v2.problems);
        // And back: the same rule.
        let opts = crate::antimony::ImportOptions { seed: 1 };
        let back = crate::sbml::import(&v2.xml, &opts).unwrap().model;
        assert_eq!(back.assignment_rules[0].expression, "2*rate(k) + max(k, 1)");

        // A fast reaction: Version 1, with `fast` kept.
        let mut fast = Model::new();
        let a = fast.add_species(Pt::ZERO, 60.0, 30.0);
        let r = fast.add_reaction(Pt::ZERO);
        let rx = fast.find_reaction_mut(&r).unwrap();
        rx.reactants.push(crate::model::Participant::new(a, 1.0));
        rx.sbml.fast = true;
        let v1 = export(&fast);
        assert!(
            v1.xml.contains(r#"level="3" version="1""#),
            "{}",
            &v1.xml[..300]
        );
        assert!(v1.xml.contains(r#"fast="true""#));
        assert!(v1.problems.is_empty(), "{:?}", v1.problems);
        let back = crate::sbml::import(&v1.xml, &opts).unwrap().model;
        assert!(back.find_reaction(&r).unwrap().sbml.fast);

        // Another package's XML kept as read: the version it was read from,
        // whose core that XML uses; without such XML, Version 2.
        let mut comp = Model::new();
        comp.sbml_version = Some(1);
        assert!(export(&comp).xml.contains(r#"level="3" version="2""#));
        comp.sbml_document
            .elements
            .push(r#"<comp:listOfModelDefinitions xmlns:comp="urn:comp"/>"#.into());
        assert!(export(&comp).xml.contains(r#"level="3" version="1""#));
        comp.sbml_version = Some(2);
        assert!(export(&comp).xml.contains(r#"level="3" version="2""#));
    }

    /// A style names at most 100 glyphs; a model with 250 species of one
    /// look gets three styles like it, and comes back with that look.
    /// libSBML crashes on a large model whose styles name hundreds of
    /// glyphs each (found by the BioModels round trip).
    #[test]
    fn styles_name_at_most_a_hundred_glyphs() {
        let mut m = Model::new();
        for i in 0..250 {
            let id = m.add_species(
                crate::geometry::pt(f64::from(i % 25) * 100.0, f64::from(i / 25) * 60.0),
                80.0,
                36.0,
            );
            let s = m.find_species_mut(&id).unwrap();
            s.style.has_custom_style = true;
            s.style.fill_color = crate::model::Rgba([0x12, 0x34, 0x56, 0xFF]);
        }
        let xml = export(&m).xml;
        let lists: Vec<usize> = xml
            .split("idList=\"")
            .skip(1)
            .map(|rest| rest[..rest.find('"').unwrap()].split_whitespace().count())
            .collect();
        assert!(lists.iter().all(|&n| n <= 100), "{lists:?}");
        assert!(
            lists.iter().filter(|&&n| n == 100).count() >= 2,
            "{lists:?}"
        );
        let back = crate::sbml::import(&xml, &crate::antimony::ImportOptions { seed: 1 })
            .unwrap()
            .model;
        assert!(
            back.species()
                .iter()
                .all(|s| s.style.fill_color == crate::model::Rgba([0x12, 0x34, 0x56, 0xFF]))
        );
    }

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
        let opts = crate::antimony::ImportOptions { seed: 1 };
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
        <annotation>
          <celldesigner:extension xmlns:celldesigner="http://www.sbml.org/2001/ns/celldesigner"><celldesigner:x/></celldesigner:extension>
        </annotation>
        <listOfReactants><speciesReference species="S" stoichiometry="1" constant="true"/></listOfReactants>
        <listOfProducts><speciesReference species="P" stoichiometry="1" constant="true"/></listOfProducts>
      </reaction>
    </listOfReactions>
  </model>
</sbml>"##;
        let i = crate::sbml::import(sbml, &opts).unwrap();
        // COPASI's own XML is kept (docs/other-annotations-design.md);
        // only CellDesigner's, on one item, isn't (O1).
        let lost: Vec<&String> = i
            .warnings
            .iter()
            .filter(|w| w.contains("aren't kept"))
            .collect();
        assert_eq!(lost.len(), 1, "{:?}", i.warnings);
        assert!(
            lost[0].starts_with("CellDesigner's or JDesigner's own annotations")
                && lost[0].contains("on 1 item"),
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
            // COPASI's XML and the metaid it was read with are kept.
            assert_eq!(s.other.len(), 1);
            assert!(s.other[0].starts_with("<COPASI"), "{:?}", s.other);
            assert_eq!(s.metaid.as_deref(), Some("_S"));
            assert!(m.find_reaction("R").unwrap().annotation.other.is_empty());
            assert_eq!(m.find_compartment("c").unwrap().annotation.sbo, Some(290));
            assert_eq!(m.find_reaction("R").unwrap().annotation.sbo, Some(176));
            let k = &m.find_parameter("k").unwrap().annotation;
            assert_eq!(k.sbo, Some(35));
            assert!(k.notes.as_deref().unwrap().contains("A rate constant."));
            // A parameter set by a rule is kept, with its annotation.
            let kr = m.find_parameter("kr").unwrap();
            assert_eq!(kr.annotation.sbo, Some(186));
            assert!(!kr.constant);
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
        // hangs on each item's metaid: its own as read, else its id.
        let text = export(&i.model).xml;
        assert!(text.contains(r#"metaid="_S""#), "{text}");
        assert!(text.contains(r##"rdf:about="#_S""##), "{text}");
        assert!(
            text.contains(r#"<COPASI xmlns="http://www.copasi.org/static/sbml"><x/></COPASI>"#),
            "{text}"
        );
        assert!(!text.contains("celldesigner"), "{text}");
        assert!(text.contains(r##"rdf:about="#m""##), "{text}");
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
        let opts = crate::antimony::ImportOptions { seed: 1 };
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
