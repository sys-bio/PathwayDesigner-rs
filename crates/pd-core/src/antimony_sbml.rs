//! What Antimony can't say of a model, kept beside its text
//! (docs/antimony-full-support-design.md S3–S5).
//!
//! [`gather`] collects it from a model, as notes; [`write`] writes the notes
//! as the `/* sbml … end */` block that follows a model's diagram; [`read`]
//! reads them back; [`apply`] lays them on a model read from the Antimony
//! text. "Load Antimony" gathers them from the model it replaces when the
//! text has no block (S3). A note applies only where the text still says
//! what was written: a formula is put back as it was only if the text's
//! formula is what Antimony made of it, a local parameter only if its rate
//! law still uses it, a species reference's details only if the reaction
//! still has that species on that side.
//!
//! The block's lines (S5), each about one item, by the id the text uses:
//!
//! ```text
//! /* sbml glycolysis v1
//!   J1_k local J1 k                // J1_k is J1's own parameter k
//!   fast J1
//!   units substance mole, time minute
//!   S1 initial amount 5            // its starting value is an amount (or concentration)
//!   S2 unset initial               // the file gave none (also value, size)
//!   compartment_ id compartment    // Antimony reserves `compartment`
//!   unit time_ id time             // a unit's id (SBML keeps units apart)
//!   S1 sbml {"metaid":"_1"}        // the technical rest, as the native file has it
//!   rule y sbml {"annotation":{"sbo":64}}
//! end */
//! ```
//!
//! Items Antimony gives no name are named by kind: `rule y`, `rate y`,
//! `initial S1`, `algebraic 1`, `constraint 1`, `event 1` (the first event
//! without an id), and `model` for the model; units as `unit u`, since a
//! unit may have the id of something else.

use serde_json::{Map, Value, json};

use crate::model::{
    Annotation, Creator, History, Model, ModelUnits, PackageAttribute, PackageData,
    SpeciesReference,
};

/// One thing Antimony can't say.
#[derive(Clone, Debug, PartialEq)]
pub enum Note {
    /// The model parameter `param` in the text is reaction's own `id`.
    Local {
        param: String,
        reaction: String,
        id: String,
    },
    Fast(String),
    Units(ModelUnits),
    /// The species' starting value is this amount (`true`) or
    /// concentration, where Antimony would read the other.
    Initial(String, bool, f64),
    /// The file gave no starting value, value or size.
    Unset(String, Unset),
    /// The text's `now` is the model's `was` (Antimony reserves `was`).
    Renamed {
        now: String,
        was: String,
    },
    /// [`Note::Renamed`] for a unit.
    RenamedUnit {
        now: String,
        was: String,
    },
    /// The technical rest of one item, as the native file keeps it.
    Sbml(Subject, Map<String, Value>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unset {
    Initial,
    Value,
    Size,
}

/// What a [`Note::Sbml`] is about.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Subject {
    Model,
    /// A species, compartment, parameter, reaction, function or event with
    /// an id.
    Item(String),
    Unit(String),
    Rule(String),
    Rate(String),
    Initial(String),
    /// 1-based, in the model's order.
    Algebraic(usize),
    Constraint(usize),
    /// An event without an id, 1-based among those.
    Event(usize),
}

// ----- Gathering ---------------------------------------------------------------

/// A formula as reading the Antimony written for it gives it back.
pub(crate) fn as_read(text: &str) -> String {
    crate::antimony::read_formula(&crate::antimony::antimony_formula(text))
}

/// A formula the file left out (the Test Suite has them), which Antimony
/// can't say.
fn no_math(map: &mut Map<String, Value>, text: &str) {
    if text.trim().is_empty() {
        map.insert("noMath".into(), json!(true));
    }
}

/// The formula as it was, when reading its Antimony wouldn't give it back.
fn exact(map: &mut Map<String, Value>, key: &str, text: &str) {
    if !text.trim().is_empty() && as_read(text) != text {
        map.insert(key.into(), json!(text));
    }
}

/// What Antimony can't say of an annotation: all of it on what Antimony
/// gives no name (`whole`), else the technical rest: metaid, other tools'
/// XML, other packages' parts, RDF, an item's history.
fn annotation_value(a: &Annotation, whole: bool) -> Option<Value> {
    annotation_value_on(a, whole, false)
}

/// [`annotation_value`]; `on_model` for the model's, whose qualifiers are
/// the model ones.
fn annotation_value_on(a: &Annotation, whole: bool, on_model: bool) -> Option<Value> {
    let mut m = Map::new();
    if !whole {
        // References Antimony has no word for.
        let more: Vec<Value> = a
            .references
            .iter()
            .filter(|r| crate::antimony::antimony_qualifier(&r.qualifier, on_model).is_none())
            .map(|r| json!([r.qualifier, r.resource]))
            .collect();
        if !more.is_empty() {
            m.insert("moreReferences".into(), Value::Array(more));
        }
    }
    if whole {
        if let Some(sbo) = a.sbo {
            m.insert("sbo".into(), json!(sbo));
        }
        if !a.references.is_empty() {
            let refs: Vec<Value> = a
                .references
                .iter()
                .map(|r| json!([r.qualifier, r.resource]))
                .collect();
            m.insert("references".into(), Value::Array(refs));
        }
        if let Some(n) = &a.notes {
            m.insert("notes".into(), json!(n));
        }
    }
    if let Some(id) = &a.metaid {
        m.insert("metaid".into(), json!(id));
    }
    if !a.other.is_empty() {
        m.insert("other".into(), json!(a.other));
    }
    if !a.rdf.is_empty() {
        m.insert("rdf".into(), json!(a.rdf));
    }
    if !a.package.is_empty() {
        let attributes: Vec<Value> = a
            .package
            .attributes
            .iter()
            .map(|x| json!([x.name, x.namespace, x.value]))
            .collect();
        m.insert(
            "package".into(),
            json!({"attributes": attributes, "elements": a.package.elements}),
        );
    }
    if !a.history.is_empty() {
        m.insert("history".into(), history_value(&a.history));
    }
    (!m.is_empty()).then_some(Value::Object(m))
}

fn history_value(h: &History) -> Value {
    let creators: Vec<Value> = h
        .creators
        .iter()
        .map(|c| json!([c.family, c.given, c.email, c.organization]))
        .collect();
    json!({"creators": creators, "created": h.created, "modified": h.modified})
}

/// A name Antimony's `is "…"` can't hold (double quotes, line breaks; found
/// by the BioModels round trip), as it is.
fn put_name(map: &mut Map<String, Value>, name: &Option<String>) {
    if let Some(n) = name
        && n.contains(['"', '\n', '\r', '\t'])
    {
        map.insert("name".into(), json!(n));
    }
}

/// The name and annotation of what has a unit's id, all of them: the
/// text's `is` and annotation lines can't tell the two apart.
fn put_beside_unit(
    model: &Model,
    map: &mut Map<String, Value>,
    id: &str,
    name: &Option<String>,
    a: &Annotation,
) -> bool {
    if !model.unit_definitions.iter().any(|u| u.id == id) {
        return false;
    }
    map.insert("name".into(), json!(name));
    map.insert("annotationAll".into(), json!(true));
    put_annotation(map, "annotation", a, true);
    true
}

fn put_annotation(map: &mut Map<String, Value>, key: &str, a: &Annotation, whole: bool) {
    if let Some(v) = annotation_value(a, whole) {
        map.insert(key.into(), v);
    }
}

/// The notes for `model`, whose Antimony was written from `text_model`
/// (the model with reserved ids renamed and local parameters made the
/// model's, `renamed`, `renamed_units` and `locals` saying which).
pub fn gather(
    model: &Model,
    renamed: &[(String, String)],
    renamed_units: &[(String, String)],
    locals: &[(String, String, String)],
    written_name: &str,
) -> Vec<Note> {
    let mut notes: Vec<Note> = Vec::new();
    for (was, now) in renamed {
        notes.push(Note::Renamed {
            now: now.clone(),
            was: was.clone(),
        });
    }
    for (was, now) in renamed_units {
        notes.push(Note::RenamedUnit {
            now: now.clone(),
            was: was.clone(),
        });
    }
    for (param, reaction, id) in locals {
        notes.push(Note::Local {
            param: param.clone(),
            reaction: reaction.clone(),
            id: id.clone(),
        });
    }
    if !model.units.is_empty() {
        notes.push(Note::Units(model.units.clone()));
    }
    let mut sbml = |subject: Subject, map: Map<String, Value>| {
        if !map.is_empty() {
            notes.push(Note::Sbml(subject, map));
        }
    };

    // The model itself.
    let mut m = Map::new();
    if let Some(id) = &model.model_id {
        m.insert("modelId".into(), json!(id));
    }
    if model.model_id_unset {
        m.insert("modelIdUnset".into(), json!(true));
    }
    if model.model_name_unset {
        m.insert("modelNameUnset".into(), json!(true));
    }
    // A name Antimony can't use as the model's.
    if model.model_name != written_name {
        m.insert("name".into(), json!(model.model_name));
    }
    if !model.sbml_namespaces.is_empty() {
        let ns: Vec<Value> = model
            .sbml_namespaces
            .iter()
            .map(|(p, u)| json!([p, u]))
            .collect();
        m.insert("namespaces".into(), Value::Array(ns));
    }
    if !model.sbml_document.is_empty() {
        let doc = Annotation {
            package: model.sbml_document.clone(),
            ..Annotation::default()
        };
        if let Some(Value::Object(mut v)) = annotation_value(&doc, false)
            && let Some(p) = v.remove("package")
        {
            m.insert("documentPackage".into(), p);
        }
    }
    if let Some(v) = model.sbml_version {
        m.insert("sbmlVersion".into(), json!(v));
    }
    if let Some(v) = annotation_value_on(&model.annotation, false, true) {
        m.insert("annotation".into(), v);
    }
    // Reactions with no rate law are written with the rate `v`, which then
    // is a parameter; this one wasn't the model's.
    if model
        .reactions()
        .iter()
        .any(|r| r.kinetic_law.trim().is_empty())
        && !model.parameters.iter().any(|p| p.variable == "v")
    {
        m.insert("noLawParameter".into(), json!(true));
    }
    // Genes (docs/gene-rules-design.md), where the fbc XML kept with the
    // model doesn't say them.
    if !model.genes.is_empty() && crate::genes::genes_as_read(model).as_ref() != Some(&model.genes)
    {
        let genes: Vec<Value> = model
            .genes
            .iter()
            .map(|g| json!([g.id, g.label, g.name, annotation_value(&g.annotation, true)]))
            .collect();
        m.insert("genes".into(), Value::Array(genes));
    }
    sbml(Subject::Model, m);

    for c in &model.compartments {
        let mut m = Map::new();
        if !put_beside_unit(model, &mut m, &c.id, &c.display_name, &c.annotation) {
            put_annotation(&mut m, "annotation", &c.annotation, false);
            put_name(&mut m, &c.display_name);
        }
        if c.dimensions != 3.0 {
            m.insert("dimensions".into(), json!(c.dimensions));
        }
        if c.sbml.dimensions_unset {
            m.insert("dimensionsUnset".into(), json!(true));
        }
        sbml(Subject::Item(c.id.clone()), m);
    }
    for s in model
        .species()
        .iter()
        .filter(|s| !s.is_alias() && !s.is_null_node)
    {
        let mut m = Map::new();
        if !put_beside_unit(model, &mut m, &s.id, &s.display_name, &s.annotation) {
            put_annotation(&mut m, "annotation", &s.annotation, false);
            put_name(&mut m, &s.display_name);
        }
        // Antimony writes `const` only for a boundary species.
        if s.is_constant && !s.is_boundary {
            m.insert("constant".into(), json!(true));
        }
        sbml(Subject::Item(s.id.clone()), m);
    }
    for p in &model.parameters {
        let mut m = Map::new();
        if !put_beside_unit(model, &mut m, &p.variable, &p.name, &p.annotation) {
            put_annotation(&mut m, "annotation", &p.annotation, false);
            put_name(&mut m, &p.name);
        }
        sbml(Subject::Item(p.variable.clone()), m);
    }
    for r in model.reactions() {
        let mut m = Map::new();
        put_annotation(&mut m, "annotation", &r.annotation, false);
        put_name(&mut m, &r.display_name);
        put_annotation(
            &mut m,
            "kineticLawAnnotation",
            &r.sbml.kinetic_law_annotation,
            true,
        );
        if r.kinetic_law.trim().is_empty() {
            m.insert("noLaw".into(), json!(true));
        }
        if let Some(c) = &r.sbml.compartment {
            m.insert("compartment".into(), json!(c));
        }
        exact(&mut m, "law", &law_as_written(model, r, locals));
        // Each species reference's details, by side, species and turn.
        let mut refs: Vec<Value> = Vec::new();
        for (role, list) in [
            ("reactant", &r.reactants),
            ("product", &r.products),
            ("modifier", &r.modifiers),
        ] {
            let mut seen: Vec<String> = Vec::new();
            for p in list {
                let Some(node) = model.find_species(&p.species) else {
                    continue;
                };
                if node.is_null_node {
                    continue;
                }
                let species = node.primary_id().to_owned();
                let turn = seen.iter().filter(|x| **x == species).count() + 1;
                seen.push(species.clone());
                // A modifier Antimony doesn't make from the rate law, which
                // only the diagram can say: one named twice, one the law
                // doesn't use (a local parameter with its id hides it), or
                // one also a reactant or product; with its kind (found by
                // the BioModels and Test Suite round trips).
                let local = |n: &str| r.sbml.local_parameters.iter().any(|l| l.id == n);
                let in_law = crate::model::names_used(&r.kinetic_law)
                    .iter()
                    .any(|n| (*n == species || *n == p.species) && !local(n));
                let on_a_side = r.reactants.iter().chain(&r.products).any(|q| {
                    model
                        .find_species(&q.species)
                        .is_some_and(|s| s.primary_id() == species)
                });
                let again = role == "modifier" && (turn > 1 || !in_law || on_a_side);
                if p.sbml.is_none() && !again {
                    continue;
                }
                let mut x = Map::new();
                x.insert("role".into(), json!(role));
                x.insert("species".into(), json!(species));
                x.insert("turn".into(), json!(turn));
                if again && let Some(k) = p.kind.key() {
                    x.insert("kind".into(), json!(k));
                }
                let Some(info) = &p.sbml else {
                    x.insert("noDetails".into(), json!(true));
                    refs.push(Value::Object(x));
                    continue;
                };
                if let Some(id) = &info.id {
                    x.insert("id".into(), json!(id));
                }
                if let Some(n) = &info.name {
                    x.insert("name".into(), json!(n));
                }
                if !info.constant {
                    x.insert("constant".into(), json!(false));
                }
                if info.stoichiometry_unset {
                    x.insert("stoichiometryUnset".into(), json!(true));
                }
                put_annotation(&mut x, "annotation", &info.annotation, true);
                refs.push(Value::Object(x));
            }
        }
        if !refs.is_empty() {
            m.insert("references".into(), Value::Array(refs));
        }
        // Its gene rule, where its fbc XML doesn't say it.
        if r.sbml.gene_rule.is_some()
            && crate::genes::rule_as_read(r, &model.sbml_namespaces) != r.sbml.gene_rule
        {
            m.insert("geneRule".into(), json!(r.sbml.gene_rule));
        }
        sbml(Subject::Item(r.id.clone()), m);
    }
    for f in &model.function_definitions {
        let mut m = Map::new();
        put_annotation(&mut m, "annotation", &f.annotation, false);
        put_name(&mut m, &f.name);
        if f.body.trim().is_empty() {
            m.insert("noBody".into(), json!(f.arguments));
        } else {
            exact(&mut m, "body", &f.body);
        }
        sbml(Subject::Item(f.id.clone()), m);
    }
    for u in &model.unit_definitions {
        let mut m = Map::new();
        // Antimony says too little of a unit's annotation to rely on.
        put_annotation(&mut m, "annotation", &u.annotation, true);
        // A unit with the id of something else has no Antimony name line,
        // which would be the other's (found by the BioModels round trip).
        if crate::antimony::unit_shares_id(model, &u.id) {
            m.insert("name".into(), json!(u.name));
        } else {
            put_name(&mut m, &u.name);
        }
        let read = crate::antimony::unit_factors(&crate::antimony::unit_text(u));
        if read.as_ref() != Some(&u.units) {
            let units: Vec<Value> = u
                .units
                .iter()
                .map(|x| json!([x.kind, x.exponent, x.scale, x.multiplier]))
                .collect();
            m.insert("units".into(), Value::Array(units));
        }
        sbml(Subject::Unit(u.id.clone()), m);
    }
    for a in &model.initial_assignments {
        let mut m = Map::new();
        put_annotation(&mut m, "annotation", &a.annotation, true);
        // A formula that is a number reads back as a value, not a formula.
        // (`INF` is a number only if nothing has that id: found by the Test
        // Suite round trip.)
        let number = as_read(&a.expression).parse::<f64>().is_ok()
            && !crate::model::names_used(&a.expression)
                .iter()
                .any(|n| model.is_id_used(n));
        // A species reference's, which the text has no line for (found by
        // the Test Suite round trip).
        let reference = model
            .reactions()
            .iter()
            .flat_map(|r| r.reactants.iter().chain(&r.products))
            .any(|p| p.reference_id() == Some(a.symbol.as_str()));
        if reference {
            m.insert("formula".into(), json!(a.expression));
            m.insert("notInText".into(), json!(true));
        } else if a.expression.trim().is_empty() {
            m.insert("noMath".into(), json!(true));
        } else if number {
            m.insert("formula".into(), json!(a.expression));
            m.insert("asValue".into(), json!(true));
        } else {
            exact(&mut m, "formula", &a.expression);
        }
        // The item's own value beside it, which Antimony can't say.
        if let Some(p) = model.parameters.iter().find(|p| p.variable == a.symbol) {
            if p.value.is_some() || number {
                m.insert("value".into(), json!(p.value));
            }
        } else if let Some(s) = model.find_species(&a.symbol) {
            if !s.sbml.initial_unset || number {
                let v = (!s.sbml.initial_unset).then_some(s.initial_value);
                m.insert("initial".into(), json!(v));
                m.insert("amount".into(), json!(s.initial_is_amount));
            }
        } else if let Some(c) = model.compartments.iter().find(|c| c.id == a.symbol)
            && (!c.sbml.size_unset || number)
        {
            let v = (!c.sbml.size_unset).then_some(c.size);
            m.insert("size".into(), json!(v));
        }
        sbml(Subject::Initial(a.symbol.clone()), m);
    }
    for r in &model.assignment_rules {
        let mut m = Map::new();
        put_annotation(&mut m, "annotation", &r.annotation, true);
        no_math(&mut m, &r.expression);
        exact(&mut m, "formula", &r.expression);
        sbml(Subject::Rule(r.variable.clone()), m);
    }
    for r in &model.rate_rules {
        let mut m = Map::new();
        put_annotation(&mut m, "annotation", &r.annotation, true);
        no_math(&mut m, &r.expression);
        exact(&mut m, "formula", &r.expression);
        sbml(Subject::Rate(r.variable.clone()), m);
    }
    for (i, r) in model.algebraic_rules.iter().enumerate() {
        let mut m = Map::new();
        put_annotation(&mut m, "annotation", &r.annotation, true);
        exact(&mut m, "formula", &r.expression);
        sbml(Subject::Algebraic(i + 1), m);
    }
    for (i, c) in model.constraints.iter().enumerate() {
        let mut m = Map::new();
        put_annotation(&mut m, "annotation", &c.annotation, true);
        exact(&mut m, "formula", &c.expression);
        if let Some(msg) = &c.message {
            m.insert("message".into(), json!(msg));
        }
        sbml(Subject::Constraint(i + 1), m);
    }
    let mut unnamed = 0;
    for e in &model.events {
        let mut m = Map::new();
        put_annotation(&mut m, "annotation", &e.annotation, e.id.is_none());
        if let Some(n) = &e.name {
            m.insert("name".into(), json!(n));
        }
        put_annotation(&mut m, "triggerAnnotation", &e.trigger.annotation, true);
        if e.no_trigger {
            m.insert("noTrigger".into(), json!(true));
        } else if e.trigger.expression.trim().is_empty() {
            m.insert("noTriggerMath".into(), json!(true));
        } else {
            exact(&mut m, "trigger", &e.trigger.expression);
        }
        for (key, part) in [("delay", &e.delay), ("priority", &e.priority)] {
            if let Some(p) = part {
                put_annotation(&mut m, &format!("{key}Annotation"), &p.annotation, true);
                exact(&mut m, key, &p.expression);
            }
        }
        let mut assignments = Map::new();
        for a in &e.assignments {
            let mut x = Map::new();
            put_annotation(&mut x, "annotation", &a.annotation, true);
            exact(&mut x, "formula", &a.expression);
            if !x.is_empty() {
                assignments.insert(a.variable.clone(), Value::Object(x));
            }
        }
        if !assignments.is_empty() {
            m.insert("assignments".into(), Value::Object(assignments));
        }
        let subject = match &e.id {
            Some(id) => Subject::Item(id.clone()),
            None => {
                unnamed += 1;
                Subject::Event(unnamed)
            }
        };
        sbml(subject, m);
    }
    // A rate law's own parameters: what Antimony can't say of their
    // annotations, under the names the text gives them.
    for (param, reaction, id) in locals {
        let lp = model
            .find_reaction(reaction)
            .and_then(|r| r.sbml.local_parameters.iter().find(|p| p.id == *id));
        if let Some(lp) = lp {
            let mut m = Map::new();
            put_annotation(&mut m, "annotation", &lp.annotation, false);
            sbml(Subject::Item(param.clone()), m);
        }
    }
    drop(sbml);

    for r in model.reactions() {
        if r.sbml.fast {
            notes.push(Note::Fast(r.id.clone()));
        }
    }
    for s in model
        .species()
        .iter()
        .filter(|s| !s.is_alias() && !s.is_null_node)
    {
        let formula = model.initial_assignment(&s.id).is_some();
        if s.sbml.initial_unset && !formula {
            notes.push(Note::Unset(s.id.clone(), Unset::Initial));
        } else if s.initial_is_amount != s.in_amounts && !formula {
            // Antimony reads a value as the species' amount if it is in
            // amounts, else as its concentration.
            notes.push(Note::Initial(
                s.id.clone(),
                s.initial_is_amount,
                s.initial_value,
            ));
        }
    }
    for p in &model.parameters {
        if p.value.is_none() && model.initial_assignment(&p.variable).is_none() {
            notes.push(Note::Unset(p.variable.clone(), Unset::Value));
        }
    }
    for c in &model.compartments {
        if c.sbml.size_unset && model.initial_assignment(&c.id).is_none() {
            notes.push(Note::Unset(c.id.clone(), Unset::Size));
        }
    }
    notes
}

/// A rate law as written, its local parameters under the names the text
/// gives them, for comparing with the text's.
fn law_as_written(
    _model: &Model,
    r: &crate::model::Reaction,
    locals: &[(String, String, String)],
) -> String {
    let mut law = r.kinetic_law.clone();
    for (param, reaction, id) in locals {
        if *reaction == r.id {
            law = crate::model::replace_whole_word(&law, id, param);
        }
    }
    law
}

/// What `notes` keep, in words, each once, in a fixed order: what plain
/// Antimony leaves out (S6), the model written under the name `written`. Renamed ids aren't among them: the text has
/// them, with `_` added.
pub fn left_out(notes: &[Note], written: &str) -> Vec<&'static str> {
    let mut found: Vec<&'static str> = Vec::new();
    let mut add = |what: &'static str| {
        if !found.contains(&what) {
            found.push(what);
        }
    };
    for n in notes {
        match n {
            Note::Local { .. } => add(LOCALS),
            Note::Fast(_) => add(FAST),
            Note::Units(_) => add(UNITS),
            Note::Initial(..) => add(AMOUNTS),
            Note::Unset(..) => add(UNSET),
            Note::Renamed { .. } | Note::RenamedUnit { .. } => {}
            Note::Sbml(subject, map) => {
                if let Some(Value::Array(refs)) = map.get("references") {
                    for x in refs {
                        add(if x.get("noDetails").is_some() {
                            MODIFIERS
                        } else {
                            REFERENCES
                        });
                    }
                }
                for (key, value) in map {
                    // What reading plain Antimony gives back anyway: a model
                    // with no name is written as `anonymous`, an id that is
                    // the name is read from it, and Level 3 Version 2 is
                    // what is written unless asked otherwise.
                    let model_name = notes.iter().find_map(|n| match n {
                        Note::Sbml(Subject::Model, m) => m.get("name"),
                        _ => None,
                    });
                    let anyway = match key.as_str() {
                        "name" => value == "",
                        "modelId" => {
                            Some(value) == model_name || model_name.is_none() && value == written
                        }
                        "sbmlVersion" => value == 2,
                        _ => false,
                    };
                    if *subject == Subject::Model && anyway {
                        continue;
                    }
                    if let Some(what) = in_words(subject, key) {
                        add(what);
                    }
                }
            }
        }
    }
    let order = [
        LOCALS,
        FAST,
        MODIFIERS,
        REFERENCES,
        NO_LAW,
        AMOUNTS,
        UNSET,
        INITIAL,
        FORMULAS,
        NO_MATH,
        CONSTANT,
        DIMENSIONS,
        PLACE,
        UNITS,
        EXACT_UNITS,
        NAMES,
        MESSAGES,
        GENES,
        ANNOTATIONS,
        MODEL,
        PACKAGES,
    ];
    found.sort_by_key(|w| order.iter().position(|o| o == w));
    found
}

const LOCALS: &str = "each rate law's own parameters (written as the model's, as J1_k)";
const FAST: &str = "fast reactions";
const MODIFIERS: &str = "modifiers the rate law doesn't use";
const REFERENCES: &str = "the ids, names and annotations of a reaction's species";
const NO_LAW: &str = "reactions and functions without a formula";
const AMOUNTS: &str = "whether starting values are amounts or concentrations";
const UNSET: &str = "values the file left out";
const INITIAL: &str = "starting formulas Antimony would read as numbers";
const FORMULAS: &str = "some formulas exactly as written";
const NO_MATH: &str = "rules and events without a formula";
const CONSTANT: &str = "constant species that aren't boundary species";
const DIMENSIONS: &str = "compartments' dimensions";
const PLACE: &str = "the compartment a reaction takes place in";
const UNITS: &str = "the model's units";
const EXACT_UNITS: &str = "units exactly as defined";
const NAMES: &str = "some names";
const MESSAGES: &str = "constraints' messages";
const GENES: &str = "genes and reactions' gene rules";
const ANNOTATIONS: &str = "metaids, and annotations Antimony has no words for";
const MODEL: &str = "the model's SBML id and name";
const PACKAGES: &str = "other SBML packages' parts and the SBML version";

/// What a key of a [`Note::Sbml`] keeps, in words.
fn in_words(subject: &Subject, key: &str) -> Option<&'static str> {
    Some(match key {
        "annotation"
        | "annotationAll"
        | "kineticLawAnnotation"
        | "triggerAnnotation"
        | "delayAnnotation"
        | "priorityAnnotation" => ANNOTATIONS,
        "modelId" | "modelIdUnset" | "modelNameUnset" => MODEL,
        "namespaces" | "documentPackage" | "sbmlVersion" => PACKAGES,
        "noLawParameter" | "noLaw" | "noBody" => NO_LAW,
        "dimensions" | "dimensionsUnset" => DIMENSIONS,
        "constant" => CONSTANT,
        "compartment" => PLACE,
        "units" => EXACT_UNITS,
        "message" => MESSAGES,
        "genes" | "geneRule" => GENES,
        "name" if *subject == Subject::Model => MODEL,
        "name" => NAMES,
        "noMath" | "noTrigger" | "noTriggerMath" => NO_MATH,
        "asValue" | "notInText" | "value" | "initial" | "amount" | "size" => INITIAL,
        "formula" | "law" | "body" | "trigger" | "delay" | "priority" | "assignments" => {
            match subject {
                Subject::Initial(_) => INITIAL,
                _ => FORMULAS,
            }
        }
        _ => return None,
    })
}

// ----- Writing and reading the block ---------------------------------------------

/// The block, or nothing when there are no notes.
pub fn write(notes: &[Note], model_name: &str) -> String {
    if notes.is_empty() {
        return String::new();
    }
    let mut out = format!("/* sbml {model_name} v1\n");
    for n in notes {
        let line = match n {
            Note::Local {
                param,
                reaction,
                id,
            } => format!("{param} local {reaction} {id}"),
            Note::Fast(r) => format!("fast {r}"),
            Note::Units(u) => {
                let parts: Vec<String> = [
                    ("substance", &u.substance),
                    ("time", &u.time),
                    ("volume", &u.volume),
                    ("area", &u.area),
                    ("length", &u.length),
                    ("extent", &u.extent),
                ]
                .iter()
                .filter_map(|(k, v)| Some(format!("{k} {}", v.as_ref()?)))
                .collect();
                format!("units {}", parts.join(", "))
            }
            Note::Initial(s, amount, v) => format!(
                "{s} initial {} {}",
                if *amount { "amount" } else { "concentration" },
                crate::model::format_number(*v)
            ),
            Note::Unset(id, what) => format!(
                "{id} unset {}",
                match what {
                    Unset::Initial => "initial",
                    Unset::Value => "value",
                    Unset::Size => "size",
                }
            ),
            Note::Renamed { now, was } => format!("{now} id {was}"),
            Note::RenamedUnit { now, was } => format!("unit {now} id {was}"),
            Note::Sbml(subject, map) => {
                let who = match subject {
                    Subject::Model => "model".to_owned(),
                    Subject::Item(id) => id.clone(),
                    Subject::Unit(id) => format!("unit {id}"),
                    Subject::Rule(v) => format!("rule {v}"),
                    Subject::Rate(v) => format!("rate {v}"),
                    Subject::Initial(v) => format!("initial {v}"),
                    Subject::Algebraic(i) => format!("algebraic {i}"),
                    Subject::Constraint(i) => format!("constraint {i}"),
                    Subject::Event(i) => format!("event {i}"),
                };
                // `*/` would end the comment: JSON may write `/` as `\/`.
                let data = Value::Object(map.clone()).to_string().replace("*/", "*\\/");
                format!("{who} sbml {data}")
            }
        };
        out.push_str("  ");
        out.push_str(&line);
        out.push('\n');
    }
    out.push_str("end */\n");
    out
}

/// The notes in `src`'s block, if it has one; with a warning for each line
/// that isn't one.
pub fn read(src: &str) -> Option<(Vec<Note>, Vec<String>)> {
    let start = src.find("/* sbml")?;
    let body = &src[start..];
    let end = body.find("end */")?;
    let mut notes = Vec::new();
    let mut warnings = Vec::new();
    for raw in body[..end].lines().skip(1) {
        let line = raw.trim();
        if line.is_empty() || line.starts_with("//") {
            continue;
        }
        match read_line(line) {
            Some(n) => notes.push(n),
            None => warnings.push(format!(
                "The sbml block's line \"{line}\" isn't one this version reads"
            )),
        }
    }
    Some((notes, warnings))
}

fn read_line(line: &str) -> Option<Note> {
    // `<who> sbml {…}`, `<kind> <key> sbml {…}`.
    if let Some(at) = line.find(" sbml {") {
        let who: Vec<&str> = line[..at].split_whitespace().collect();
        let data: Value = serde_json::from_str(&line[at + 6..]).ok()?;
        let Value::Object(map) = data else {
            return None;
        };
        let subject = match who.as_slice() {
            ["model"] => Subject::Model,
            [id] => Subject::Item((*id).to_owned()),
            ["unit", u] => Subject::Unit((*u).to_owned()),
            ["rule", v] => Subject::Rule((*v).to_owned()),
            ["rate", v] => Subject::Rate((*v).to_owned()),
            ["initial", v] => Subject::Initial((*v).to_owned()),
            ["algebraic", i] => Subject::Algebraic(i.parse().ok()?),
            ["constraint", i] => Subject::Constraint(i.parse().ok()?),
            ["event", i] => Subject::Event(i.parse().ok()?),
            _ => return None,
        };
        return Some(Note::Sbml(subject, map));
    }
    let words: Vec<&str> = line.split_whitespace().collect();
    Some(match words.as_slice() {
        [param, "local", reaction, id] => Note::Local {
            param: (*param).to_owned(),
            reaction: (*reaction).to_owned(),
            id: (*id).to_owned(),
        },
        ["fast", r] => Note::Fast((*r).to_owned()),
        ["units", ..] => {
            let mut u = ModelUnits::default();
            for part in line["units".len()..].split(',') {
                let kv: Vec<&str> = part.split_whitespace().collect();
                let [k, v] = kv.as_slice() else {
                    return None;
                };
                let v = Some((*v).to_owned());
                match *k {
                    "substance" => u.substance = v,
                    "time" => u.time = v,
                    "volume" => u.volume = v,
                    "area" => u.area = v,
                    "length" => u.length = v,
                    "extent" => u.extent = v,
                    _ => return None,
                }
            }
            Note::Units(u)
        }
        [s, "initial", what @ ("amount" | "concentration"), v] => {
            Note::Initial((*s).to_owned(), *what == "amount", v.parse().ok()?)
        }
        [id, "unset", what] => Note::Unset(
            (*id).to_owned(),
            match *what {
                "initial" => Unset::Initial,
                "value" => Unset::Value,
                "size" => Unset::Size,
                _ => return None,
            },
        ),
        ["unit", now, "id", was] => Note::RenamedUnit {
            now: (*now).to_owned(),
            was: (*was).to_owned(),
        },
        [now, "id", was] => Note::Renamed {
            now: (*now).to_owned(),
            was: (*was).to_owned(),
        },
        _ => return None,
    })
}

// ----- Applying ------------------------------------------------------------------

fn text(v: Option<&Value>) -> Option<String> {
    v.and_then(Value::as_str).map(str::to_owned)
}

/// Lays `v` (an annotation's value) on `a`.
fn merge_annotation(a: &mut Annotation, v: Option<&Value>) {
    let Some(Value::Object(m)) = v else {
        return;
    };
    if let Some(sbo) = m.get("sbo").and_then(Value::as_u64) {
        a.sbo = Some(sbo as u32);
    }
    if let Some(Value::Array(refs)) = m.get("references") {
        a.references = refs
            .iter()
            .filter_map(|r| {
                Some(crate::model::Reference {
                    qualifier: r.get(0)?.as_str()?.to_owned(),
                    resource: r.get(1)?.as_str()?.to_owned(),
                })
            })
            .collect();
    }
    if let Some(Value::Array(more)) = m.get("moreReferences") {
        for r in more {
            if let (Some(q), Some(res)) = (text(r.get(0)), text(r.get(1)))
                && !a
                    .references
                    .iter()
                    .any(|x| x.qualifier == q && x.resource == res)
            {
                a.references.push(crate::model::Reference {
                    qualifier: q,
                    resource: res,
                });
            }
        }
    }
    if let Some(n) = text(m.get("notes")) {
        a.notes = Some(n);
    }
    if let Some(id) = text(m.get("metaid")) {
        a.metaid = Some(id);
    }
    let strings = |v: Option<&Value>| -> Vec<String> {
        v.and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default()
    };
    if m.contains_key("other") {
        a.other = strings(m.get("other"));
    }
    if m.contains_key("rdf") {
        a.rdf = strings(m.get("rdf"));
    }
    if let Some(p) = m.get("package") {
        a.package = package(p);
    }
    if let Some(Value::Object(h)) = m.get("history") {
        a.history = History {
            creators: h
                .get("creators")
                .and_then(Value::as_array)
                .map(|cs| {
                    cs.iter()
                        .map(|c| Creator {
                            family: text(c.get(0)),
                            given: text(c.get(1)),
                            email: text(c.get(2)),
                            organization: text(c.get(3)),
                        })
                        .collect()
                })
                .unwrap_or_default(),
            created: text(h.get("created")),
            modified: strings(h.get("modified")),
        };
    }
}

fn package(v: &Value) -> PackageData {
    PackageData {
        attributes: v
            .get("attributes")
            .and_then(Value::as_array)
            .map(|xs| {
                xs.iter()
                    .filter_map(|x| {
                        Some(PackageAttribute {
                            name: x.get(0)?.as_str()?.to_owned(),
                            namespace: text(x.get(1)),
                            value: x.get(2)?.as_str()?.to_owned(),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default(),
        elements: v
            .get("elements")
            .and_then(Value::as_array)
            .map(|xs| {
                xs.iter()
                    .filter_map(|x| x.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default(),
    }
}

/// `formula` as it was, if the text's `now` is what Antimony made of it.
fn restore(now: &mut String, was: Option<String>) {
    if let Some(was) = was
        && as_read(&was) == *now
    {
        *now = was;
    }
}

/// Lays the notes on `model`, read from the text they were written beside.
pub fn apply(model: &mut Model, notes: &[Note]) {
    // Ids first: every other note uses the model's own.
    for n in notes {
        match n {
            Note::Renamed { now, was } => rename(model, now, was),
            Note::RenamedUnit { now, was } => crate::antimony::rename_unit(model, now, was),
            _ => {}
        }
    }
    for n in notes {
        match n {
            Note::Sbml(subject, map) => apply_sbml(model, subject, map),
            Note::Units(u) => model.units = u.clone(),
            Note::Fast(r) => {
                if let Some(r) = model.find_reaction_mut(r) {
                    r.sbml.fast = true;
                }
            }
            Note::Initial(s, amount, v) => {
                if let Some(s) = model.find_species_mut(s) {
                    s.initial_value = *v;
                    s.initial_is_amount = *amount;
                }
            }
            Note::Unset(id, what) => match what {
                Unset::Initial => {
                    if let Some(s) = model.find_species_mut(id) {
                        s.sbml.initial_unset = true;
                    }
                }
                Unset::Value => {
                    if let Some(p) = model.parameters.iter_mut().find(|p| p.variable == *id) {
                        p.value = None;
                    }
                }
                Unset::Size => {
                    if let Some(c) = model.compartments.iter_mut().find(|c| c.id == *id) {
                        c.sbml.size_unset = true;
                    }
                }
            },
            Note::Local { .. } | Note::Renamed { .. } | Note::RenamedUnit { .. } => {}
        }
    }
    // Local parameters last, once the parameter has its annotation.
    for n in notes {
        if let Note::Local {
            param,
            reaction,
            id,
        } = n
        {
            make_local(model, param, reaction, id);
        }
    }
    // Genes: out of the fbc XML the notes gave back, then those the notes
    // say themselves (docs/gene-rules-design.md).
    crate::genes::from_packages(model);
    for n in notes {
        let Note::Sbml(subject, map) = n else {
            continue;
        };
        match subject {
            Subject::Model => {
                if let Some(Value::Array(genes)) = map.get("genes") {
                    model.genes = genes
                        .iter()
                        .filter_map(|g| {
                            let mut annotation = Annotation::default();
                            merge_annotation(&mut annotation, g.get(3));
                            Some(crate::model::Gene {
                                id: g.get(0)?.as_str()?.to_owned(),
                                label: g.get(1)?.as_str()?.to_owned(),
                                name: text(g.get(2)),
                                annotation,
                            })
                        })
                        .collect();
                }
            }
            Subject::Item(id) if map.contains_key("geneRule") => {
                if let Some(r) = model.find_reaction_mut(id) {
                    r.sbml.gene_rule = text(map.get("geneRule"));
                }
            }
            _ => {}
        }
    }
    // A rule's variable that is a reaction's species reference again, not
    // a parameter (found by the BioModels round trip).
    let references: Vec<String> = model
        .reactions()
        .iter()
        .flat_map(|r| r.reactants.iter().chain(&r.products).chain(&r.modifiers))
        .filter_map(|p| p.reference_id().map(str::to_owned))
        .collect();
    model
        .parameters
        .retain(|p| !references.contains(&p.variable));
    // The rate `v` written for reactions with none, if nothing uses it now.
    let no_law = notes
        .iter()
        .any(|n| matches!(n, Note::Sbml(Subject::Model, m) if m.contains_key("noLawParameter")));
    if no_law
        && !model
            .formulas()
            .iter()
            .any(|(_, t)| crate::model::names_used(t).iter().any(|n| n == "v"))
    {
        model.parameters.retain(|p| p.variable != "v");
    }
}

fn rename(model: &mut Model, now: &str, was: &str) {
    if model.rename_back(now, was) {
        return;
    }
    if model.function_definitions.iter().any(|f| f.id == now) {
        let swap = |t: &str| crate::model::replace_whole_word(t, now, was);
        model.formulas_mut(|t, _| *t = swap(t));
        for f in &mut model.function_definitions {
            f.body = swap(&f.body);
            if f.id == now {
                f.id = was.to_owned();
            }
        }
    } else if let Some(e) = model
        .events
        .iter_mut()
        .find(|e| e.id.as_deref() == Some(now))
    {
        e.id = Some(was.to_owned());
    }
}

/// The model parameter `param` back into `reaction`'s rate law as its own
/// `id`, if nothing else uses it.
fn make_local(model: &mut Model, param: &str, reaction: &str, id: &str) {
    let Some(r) = model.find_reaction(reaction) else {
        return;
    };
    let uses = |t: &str| crate::model::names_used(t).iter().any(|n| n == param);
    // A law may leave its own parameter unused (found by the BioModels
    // round trip), but it mustn't use another `id`, which the local would hide.
    if id != param
        && crate::model::names_used(&r.kinetic_law)
            .iter()
            .any(|n| n == id)
    {
        return;
    }
    let elsewhere = model.formulas().iter().any(|(place, t)| {
        *place != crate::model::FormulaPlace::RateLaw(reaction.to_owned()) && uses(t)
    });
    let Some(i) = model.parameters.iter().position(|p| p.variable == param) else {
        return;
    };
    if elsewhere || model.initial_assignment(param).is_some() {
        return;
    }
    let p = model.parameters.remove(i);
    let r = model.find_reaction_mut(reaction).expect("found above");
    r.kinetic_law = crate::model::replace_whole_word(&r.kinetic_law, param, id);
    r.sbml.local_parameters.push(crate::model::LocalParameter {
        id: id.to_owned(),
        name: p.name,
        value: p.value,
        units: p.units,
        annotation: p.annotation,
    });
}

fn apply_sbml(model: &mut Model, subject: &Subject, map: &Map<String, Value>) {
    let annotation = map.get("annotation");
    match subject {
        Subject::Model => {
            if let Some(id) = text(map.get("modelId")) {
                model.model_id = Some(id);
            }
            model.model_id_unset |= map.get("modelIdUnset").is_some();
            model.model_name_unset |= map.get("modelNameUnset").is_some();
            if let Some(n) = text(map.get("name")) {
                model.model_name = n;
            }
            if let Some(Value::Array(ns)) = map.get("namespaces") {
                model.sbml_namespaces = ns
                    .iter()
                    .filter_map(|x| Some((text(x.get(0)), x.get(1)?.as_str()?.to_owned())))
                    .collect();
            }
            if let Some(p) = map.get("documentPackage") {
                model.sbml_document = package(p);
            }
            if let Some(v) = map.get("sbmlVersion").and_then(Value::as_u64) {
                model.sbml_version = Some(v as u32);
            }
            merge_annotation(&mut model.annotation, annotation);
        }
        Subject::Item(id) => apply_item(model, id, map),
        Subject::Unit(id) => apply_unit(model, id, map),
        Subject::Rule(v) => {
            if map.get("noMath").is_some()
                && !model.assignment_rules.iter().any(|r| r.variable == *v)
            {
                model.assignment_rules.push(crate::model::AssignmentRule {
                    variable: v.clone(),
                    expression: String::new(),
                    annotation: Annotation::default(),
                });
                changed_by_a_rule(model, v);
            }
            if let Some(r) = model.assignment_rules.iter_mut().find(|r| r.variable == *v) {
                merge_annotation(&mut r.annotation, annotation);
                restore(&mut r.expression, text(map.get("formula")));
            }
        }
        Subject::Rate(v) => {
            if map.get("noMath").is_some() && !model.rate_rules.iter().any(|r| r.variable == *v) {
                model.rate_rules.push(crate::model::RateRule {
                    variable: v.clone(),
                    expression: String::new(),
                    annotation: Annotation::default(),
                });
                changed_by_a_rule(model, v);
            }
            if let Some(r) = model.rate_rules.iter_mut().find(|r| r.variable == *v) {
                merge_annotation(&mut r.annotation, annotation);
                restore(&mut r.expression, text(map.get("formula")));
            }
        }
        Subject::Initial(v) => apply_initial(model, v, map),
        Subject::Algebraic(i) => {
            if let Some(r) = model.algebraic_rules.get_mut(i.wrapping_sub(1)) {
                merge_annotation(&mut r.annotation, annotation);
                restore(&mut r.expression, text(map.get("formula")));
            }
        }
        Subject::Constraint(i) => {
            if let Some(c) = model.constraints.get_mut(i.wrapping_sub(1)) {
                merge_annotation(&mut c.annotation, annotation);
                restore(&mut c.expression, text(map.get("formula")));
                if let Some(m) = text(map.get("message")) {
                    c.message = Some(m);
                }
            }
        }
        Subject::Event(i) => {
            let e = model
                .events
                .iter_mut()
                .filter(|e| e.id.is_none())
                .nth(i.wrapping_sub(1));
            if let Some(e) = e {
                apply_event(e, map);
            }
        }
    }
}

/// A parameter or compartment a rule added back sets isn't constant.
fn changed_by_a_rule(model: &mut Model, variable: &str) {
    if let Some(p) = model.parameters.iter_mut().find(|p| p.variable == variable) {
        p.constant = false;
    }
    if let Some(c) = model.compartments.iter_mut().find(|c| c.id == variable) {
        c.sbml.constant = false;
    }
}

/// An initial assignment's note: its formula as it was (back as a formula if
/// the text read it as a value), and its item's own value beside it.
fn apply_initial(model: &mut Model, symbol: &str, map: &Map<String, Value>) {
    let formula = text(map.get("formula"));
    if map.get("noMath").is_some() && model.initial_assignment(symbol).is_none() {
        model
            .initial_assignments
            .push(crate::model::InitialAssignment {
                symbol: symbol.to_owned(),
                expression: String::new(),
                annotation: Annotation::default(),
            });
    }
    if map.get("notInText").is_some() {
        if let Some(f) = &formula
            && model.initial_assignment(symbol).is_none()
        {
            model
                .initial_assignments
                .push(crate::model::InitialAssignment {
                    symbol: symbol.to_owned(),
                    expression: f.clone(),
                    annotation: Annotation::default(),
                });
        }
    } else if map.get("asValue").is_some() {
        let Some(f) = &formula else {
            return;
        };
        // Only if the text still gives the item that value.
        let now = model
            .parameters
            .iter()
            .find(|p| p.variable == symbol)
            .and_then(|p| p.value)
            .or_else(|| model.find_species(symbol).map(|s| s.initial_value))
            .or_else(|| {
                model
                    .compartments
                    .iter()
                    .find(|c| c.id == symbol)
                    .map(|c| c.size)
            });
        // NaN is the same NaN (found by the Test Suite round trip).
        let same = |v: f64| {
            as_read(f)
                .parse::<f64>()
                .is_ok_and(|x| x == v || (x.is_nan() && v.is_nan()))
        };
        if !now.is_some_and(same) || model.initial_assignment(symbol).is_some() {
            return;
        }
        model
            .initial_assignments
            .push(crate::model::InitialAssignment {
                symbol: symbol.to_owned(),
                expression: f.clone(),
                annotation: Annotation::default(),
            });
    }
    let Some(a) = model
        .initial_assignments
        .iter_mut()
        .find(|a| a.symbol == symbol)
    else {
        return;
    };
    merge_annotation(&mut a.annotation, map.get("annotation"));
    restore(&mut a.expression, formula);
    if let Some(v) = map.get("value")
        && let Some(p) = model.parameters.iter_mut().find(|p| p.variable == symbol)
    {
        p.value = v.as_f64();
    }
    if let Some(v) = map.get("initial")
        && let Some(s) = model.find_species_mut(symbol)
    {
        match v.as_f64() {
            Some(x) => {
                s.initial_value = x;
                s.sbml.initial_unset = false;
            }
            None => s.sbml.initial_unset = true,
        }
        if let Some(amount) = map.get("amount").and_then(Value::as_bool) {
            s.initial_is_amount = amount;
        }
    }
    if let Some(v) = map.get("size")
        && let Some(c) = model.compartments.iter_mut().find(|c| c.id == symbol)
    {
        match v.as_f64() {
            Some(x) => {
                c.size = x;
                c.sbml.size_unset = false;
            }
            None => c.sbml.size_unset = true,
        }
    }
}

fn apply_event(e: &mut crate::model::Event, map: &Map<String, Value>) {
    merge_annotation(&mut e.annotation, map.get("annotation"));
    if let Some(n) = text(map.get("name")) {
        e.name = Some(n);
    }
    merge_annotation(&mut e.trigger.annotation, map.get("triggerAnnotation"));
    if map.get("noTrigger").is_some() && e.trigger.expression == "false" {
        e.no_trigger = true;
        e.trigger.expression.clear();
    }
    if map.get("noTriggerMath").is_some() && e.trigger.expression == "false" {
        e.trigger.expression.clear();
    }
    restore(&mut e.trigger.expression, text(map.get("trigger")));
    if let Some(d) = &mut e.delay {
        merge_annotation(&mut d.annotation, map.get("delayAnnotation"));
        restore(&mut d.expression, text(map.get("delay")));
    }
    if let Some(p) = &mut e.priority {
        merge_annotation(&mut p.annotation, map.get("priorityAnnotation"));
        restore(&mut p.expression, text(map.get("priority")));
    }
    if let Some(Value::Object(xs)) = map.get("assignments") {
        for a in &mut e.assignments {
            if let Some(x) = xs.get(&a.variable) {
                merge_annotation(&mut a.annotation, x.get("annotation"));
                restore(&mut a.expression, text(x.get("formula")));
            }
        }
    }
}

fn apply_item(model: &mut Model, id: &str, map: &Map<String, Value>) {
    let annotation = map.get("annotation");
    let name = text(map.get("name"));
    let has_name = map.contains_key("name");
    // All of it here: what the text gave it may be a unit's.
    let all = map.contains_key("annotationAll");
    let merge_annotation = |a: &mut Annotation, v: Option<&Value>| {
        if all {
            *a = Annotation::default();
        }
        merge_annotation(a, v);
    };
    if let Some(c) = model.compartments.iter_mut().find(|c| c.id == id) {
        merge_annotation(&mut c.annotation, annotation);
        if has_name {
            c.display_name = name;
        }
        if let Some(d) = map.get("dimensions").and_then(Value::as_f64) {
            c.dimensions = d;
        }
        c.sbml.dimensions_unset |= map.get("dimensionsUnset").is_some();
        return;
    }
    if let Some(s) = model.find_species_mut(id) {
        merge_annotation(&mut s.annotation, annotation);
        if has_name {
            s.display_name = name;
        }
        if map.get("constant").is_some() {
            s.is_constant = true;
        }
        return;
    }
    if let Some(p) = model.parameters.iter_mut().find(|p| p.variable == id) {
        merge_annotation(&mut p.annotation, annotation);
        if has_name {
            p.name = name;
        }
        return;
    }
    if let Some(r) = model.find_reaction_mut(id) {
        if has_name {
            r.display_name = name;
        }
        apply_reaction(model, id, map);
        return;
    }
    if let Some(f) = model.function_definitions.iter_mut().find(|f| f.id == id) {
        merge_annotation(&mut f.annotation, annotation);
        if has_name {
            f.name = name;
        }
        restore(&mut f.body, text(map.get("body")));
        return;
    }
    if let Some(Value::Array(args)) = map.get("noBody") {
        model
            .function_definitions
            .push(crate::model::FunctionDefinition {
                id: id.to_owned(),
                arguments: args
                    .iter()
                    .filter_map(|a| a.as_str().map(str::to_owned))
                    .collect(),
                ..Default::default()
            });
        let f = model.function_definitions.last_mut().expect("just pushed");
        merge_annotation(&mut f.annotation, annotation);
        return;
    }
    if let Some(e) = model
        .events
        .iter_mut()
        .find(|e| e.id.as_deref() == Some(id))
    {
        apply_event(e, map);
    }
}

fn apply_unit(model: &mut Model, id: &str, map: &Map<String, Value>) {
    if let Some(u) = model.unit_definitions.iter_mut().find(|u| u.id == id) {
        merge_annotation(&mut u.annotation, map.get("annotation"));
        if map.contains_key("name") {
            u.name = text(map.get("name"));
        }
        if let Some(Value::Array(units)) = map.get("units") {
            let exact: Vec<crate::model::Unit> = units
                .iter()
                .filter_map(|x| {
                    Some(crate::model::Unit {
                        kind: x.get(0)?.as_str()?.to_owned(),
                        exponent: x.get(1)?.as_f64()?,
                        scale: x.get(2)?.as_i64()? as i32,
                        multiplier: x.get(3)?.as_f64()?,
                        annotation: Annotation::default(),
                    })
                })
                .collect();
            // Only if the text's unit is still the one written for it.
            let as_written = crate::model::UnitDefinition {
                units: exact.clone(),
                ..u.clone()
            };
            if crate::antimony::unit_factors(&crate::antimony::unit_text(&as_written)).as_ref()
                == Some(&u.units)
            {
                u.units = exact;
            }
        }
    }
}

fn apply_reaction(model: &mut Model, id: &str, map: &Map<String, Value>) {
    let primaries: Vec<(String, String)> = model
        .species()
        .iter()
        .map(|s| (s.id.clone(), s.primary_id().to_owned()))
        .collect();
    let primary = |node: &str| {
        primaries
            .iter()
            .find(|(n, _)| n == node)
            .map(|(_, p)| p.clone())
            .unwrap_or_default()
    };
    let r = model.find_reaction_mut(id).expect("checked");
    merge_annotation(&mut r.annotation, map.get("annotation"));
    merge_annotation(
        &mut r.sbml.kinetic_law_annotation,
        map.get("kineticLawAnnotation"),
    );
    if let Some(c) = text(map.get("compartment")) {
        r.sbml.compartment = Some(c);
    }
    restore(&mut r.kinetic_law, text(map.get("law")));
    // A reaction with no rate law was written with the rate `v`.
    if map.get("noLaw").is_some() && r.kinetic_law == "v" {
        r.kinetic_law.clear();
        r.modifiers.retain(|_| true);
    }
    if let Some(Value::Array(refs)) = map.get("references") {
        for x in refs {
            let (Some(role), Some(species)) = (text(x.get("role")), text(x.get("species"))) else {
                continue;
            };
            let turn = x.get("turn").and_then(Value::as_u64).unwrap_or(1) as usize;
            let list = match role.as_str() {
                "reactant" => &mut r.reactants,
                "product" => &mut r.products,
                _ => &mut r.modifiers,
            };
            // A modifier Antimony can't say: added back.
            let have = list
                .iter()
                .filter(|p| primary(&p.species) == species)
                .count();
            if role == "modifier"
                && have + 1 == turn
                && primaries.iter().any(|(n, _)| *n == species)
            {
                let mut again = crate::model::Participant::new(species.clone(), 1.0);
                again.kind = crate::model::ModifierKind::ALL
                    .into_iter()
                    .find(|k| k.key().is_some() && k.key() == x.get("kind").and_then(Value::as_str))
                    .unwrap_or_default();
                list.push(again);
            }
            if x.get("noDetails").is_some() {
                continue;
            }
            let Some(p) = list
                .iter_mut()
                .filter(|p| primary(&p.species) == species)
                .nth(turn - 1)
            else {
                continue;
            };
            let mut info = SpeciesReference {
                id: text(x.get("id")),
                name: text(x.get("name")),
                constant: x.get("constant").and_then(Value::as_bool).unwrap_or(true),
                stoichiometry_unset: x.get("stoichiometryUnset").is_some(),
                annotation: Annotation::default(),
            };
            merge_annotation(&mut info.annotation, x.get("annotation"));
            // A modifier's kind from its SBO term, where the text gave it
            // none (plain Antimony says no kinds).
            if role == "modifier"
                && p.kind == crate::model::ModifierKind::Modifier
                && let Some(n) = info.annotation.sbo
            {
                p.kind = crate::model::ModifierKind::from_sbo(&format!("SBO:{n:07}"));
            }
            p.sbml = Some(Box::new(info));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_line_reads_back() {
        let notes = vec![
            Note::Local {
                param: "J1_k".into(),
                reaction: "J1".into(),
                id: "k".into(),
            },
            Note::Fast("J1".into()),
            Note::Units(ModelUnits {
                substance: Some("mole".into()),
                time: Some("minute".into()),
                ..ModelUnits::default()
            }),
            Note::Initial("S1".into(), true, 5.0),
            Note::Initial("S3".into(), false, 0.43),
            Note::Unset("S2".into(), Unset::Initial),
            Note::Unset("k".into(), Unset::Value),
            Note::Unset("c".into(), Unset::Size),
            Note::Renamed {
                now: "compartment_".into(),
                was: "compartment".into(),
            },
            Note::Sbml(
                Subject::Model,
                json!({"modelId": "m1"}).as_object().unwrap().clone(),
            ),
            Note::Sbml(
                Subject::Rule("y".into()),
                json!({"annotation": {"sbo": 64}})
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
            Note::Sbml(
                Subject::Event(2),
                json!({"noTrigger": true, "x": "a */ b"})
                    .as_object()
                    .unwrap()
                    .clone(),
            ),
        ];
        let text = write(&notes, "m");
        assert!(
            text.starts_with("/* sbml m v1\n") && text.ends_with("end */\n"),
            "{text}"
        );
        // `*/` inside the JSON doesn't end the comment.
        assert_eq!(text.matches("*/").count(), 1, "{text}");
        let (back, warnings) = read(&format!("model m\nend\n{text}")).unwrap();
        assert!(warnings.is_empty(), "{warnings:?}");
        assert_eq!(back, notes);
    }
}
