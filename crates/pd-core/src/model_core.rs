//! The parts of a model that SBML has beyond the network that is drawn
//! (docs/sbml-full-support-design.md): function definitions, units, initial
//! assignments, rate and algebraic rules, constraints and events, and what
//! reactions, species references and other SBML packages carry. Formulas are
//! infix text, as rate laws are; an empty one is none.
//!
//! Here too: every formula in the model, visited in one place, so that
//! renaming an item renames it everywhere, and deleting one takes with it
//! what would otherwise name an item that isn't there (F8).

use crate::model::{Annotation, Id, Model, replace_whole_word};

/// `f(x, y) = body`: a function the model defines (F1). Its body uses only
/// its arguments.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FunctionDefinition {
    pub id: Id,
    pub name: Option<String>,
    pub arguments: Vec<String>,
    pub body: String,
    pub annotation: Annotation,
}

/// A unit the model defines, as a product of units (F4).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct UnitDefinition {
    pub id: Id,
    pub name: Option<String>,
    pub units: Vec<Unit>,
    pub annotation: Annotation,
}

/// `(multiplier × 10^scale × kind)^exponent`, `kind` a base unit such as
/// `mole` or `second`.
#[derive(Clone, Debug, PartialEq)]
pub struct Unit {
    pub kind: String,
    pub exponent: f64,
    pub scale: i32,
    pub multiplier: f64,
    pub annotation: Annotation,
}

impl Unit {
    /// The base unit itself: exponent 1, scale 0, multiplier 1.
    pub fn new(kind: impl Into<String>) -> Self {
        Unit {
            kind: kind.into(),
            exponent: 1.0,
            scale: 0,
            multiplier: 1.0,
            annotation: Annotation::default(),
        }
    }
}

/// The units the model's values are in where an item doesn't say, by unit
/// id (F4). `None` where the model doesn't say either.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ModelUnits {
    pub substance: Option<String>,
    pub time: Option<String>,
    pub volume: Option<String>,
    pub area: Option<String>,
    pub length: Option<String>,
    pub extent: Option<String>,
}

impl ModelUnits {
    pub fn is_empty(&self) -> bool {
        *self == ModelUnits::default()
    }
}

/// The value `symbol` starts at, worked out from a formula (F3). It wins
/// over the value the item itself has.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct InitialAssignment {
    pub symbol: Id,
    pub expression: String,
    pub annotation: Annotation,
}

/// `variable` changes at the rate `expression`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RateRule {
    pub variable: Id,
    pub expression: String,
    pub annotation: Annotation,
}

/// `0 = expression`, holding at all times.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AlgebraicRule {
    pub expression: String,
    pub annotation: Annotation,
}

/// A condition the model should keep to while it runs (SBML's constraint;
/// a "check" to the user, F9), and what to say when it doesn't.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Constraint {
    pub expression: String,
    /// The message, XHTML as read.
    pub message: Option<String>,
    pub annotation: Annotation,
}

/// Something that happens when a condition becomes true: values change,
/// at once or after a delay.
#[derive(Clone, Debug, PartialEq)]
pub struct Event {
    /// SBML lets an event go without an id.
    pub id: Option<Id>,
    pub name: Option<String>,
    /// The new values are worked out when the event is triggered, not when
    /// it takes effect after its delay.
    pub use_values_from_trigger_time: bool,
    pub trigger: Trigger,
    /// The event has no trigger at all, which Level 3 Version 2 allows (not
    /// one with an empty condition).
    pub no_trigger: bool,
    pub delay: Option<EventFormula>,
    pub priority: Option<EventFormula>,
    pub assignments: Vec<EventAssignment>,
    pub annotation: Annotation,
}

impl Default for Event {
    fn default() -> Self {
        Event {
            id: None,
            name: None,
            use_values_from_trigger_time: true,
            trigger: Trigger::default(),
            no_trigger: false,
            delay: None,
            priority: None,
            assignments: Vec::new(),
            annotation: Annotation::default(),
        }
    }
}

impl Event {
    /// How a message names it: "the event Feed", or "an event".
    pub fn description(&self) -> String {
        match &self.id {
            Some(id) => format!("the event {id}"),
            None => "an event".to_owned(),
        }
    }
}

/// When an event happens: the moment `expression` becomes true.
#[derive(Clone, Debug, PartialEq)]
pub struct Trigger {
    pub expression: String,
    /// The condition counts as false before the model starts, so one true
    /// at the start doesn't trigger the event.
    pub initial_value: bool,
    /// Once triggered, the event takes effect even if the condition is no
    /// longer true by then.
    pub persistent: bool,
    pub annotation: Annotation,
}

impl Default for Trigger {
    fn default() -> Self {
        Trigger {
            expression: String::new(),
            initial_value: true,
            persistent: true,
            annotation: Annotation::default(),
        }
    }
}

/// An event's delay or priority.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EventFormula {
    pub expression: String,
    pub annotation: Annotation,
}

/// `variable` becomes `expression` when the event takes effect.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EventAssignment {
    pub variable: Id,
    pub expression: String,
    pub annotation: Annotation,
}

/// A rate law's own parameter, seen by it alone (F2). Its id may be one the
/// model also uses; in the rate law it means this one.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct LocalParameter {
    pub id: Id,
    pub name: Option<String>,
    pub value: Option<f64>,
    pub units: Option<String>,
    pub annotation: Annotation,
}

/// What SBML says of a reaction beyond its participants and rate law.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ReactionSbml {
    /// SBML's `fast` (Level 3 Version 1 and Level 2).
    pub fast: bool,
    /// The compartment it takes place in, if one is named.
    pub compartment: Option<Id>,
    pub local_parameters: Vec<LocalParameter>,
    /// The `kineticLaw` element's own notes and annotation.
    pub kinetic_law_annotation: Annotation,
    /// Which genes' products catalyse it, as text: gene ids joined by `and`
    /// (a complex) and `or` (alternatives), with brackets
    /// (docs/gene-rules-design.md, [`crate::genes`]).
    pub gene_rule: Option<String>,
}

/// A gene, as SBML's fbc package has it (a gene product): what reactions'
/// gene rules name, by `id`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Gene {
    pub id: Id,
    /// The gene's own identifier, such as a b-number (`b3916`).
    pub label: String,
    /// Its name, such as `pfkA`.
    pub name: Option<String>,
}

/// What SBML says of one species in a reaction beyond the species and its
/// stoichiometry. Most participants have none of it.
#[derive(Clone, Debug, PartialEq)]
pub struct SpeciesReference {
    /// An id lets formulas use or set the stoichiometry.
    pub id: Option<Id>,
    pub name: Option<String>,
    /// SBML's `constant`: the stoichiometry doesn't change while the model
    /// runs.
    pub constant: bool,
    /// The file gave no stoichiometry (an initial assignment or rule may).
    pub stoichiometry_unset: bool,
    pub annotation: Annotation,
}

impl Default for SpeciesReference {
    fn default() -> Self {
        SpeciesReference {
            id: None,
            name: None,
            constant: true,
            stoichiometry_unset: false,
            annotation: Annotation::default(),
        }
    }
}

/// What SBML says of a compartment beyond its size and box.
#[derive(Clone, Debug, PartialEq)]
pub struct CompartmentSbml {
    pub units: Option<String>,
    /// SBML's `constant`: its size doesn't change while the model runs.
    pub constant: bool,
    /// The file gave no size (an initial assignment or rule may).
    pub size_unset: bool,
    /// The file gave no number of dimensions.
    pub dimensions_unset: bool,
}

impl Default for CompartmentSbml {
    fn default() -> Self {
        CompartmentSbml {
            units: None,
            constant: true,
            size_unset: false,
            dimensions_unset: false,
        }
    }
}

/// What SBML says of a species beyond what is drawn and its starting value.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SpeciesSbml {
    /// Its substance units.
    pub units: Option<String>,
    pub conversion_factor: Option<Id>,
    /// The file gave no initial amount or concentration.
    pub initial_unset: bool,
}

/// What another SBML package (fbc, qual, …) put on an item, kept to be
/// written back unchanged (F10).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PackageData {
    pub attributes: Vec<PackageAttribute>,
    /// Child elements, each as raw XML.
    pub elements: Vec<String>,
}

impl PackageData {
    pub fn is_empty(&self) -> bool {
        self.attributes.is_empty() && self.elements.is_empty()
    }
}

/// An attribute as written: `fbc:charge="-2"`.
#[derive(Clone, Debug, PartialEq)]
pub struct PackageAttribute {
    /// The qualified name, `fbc:charge`.
    pub name: String,
    pub namespace: Option<String>,
    pub value: String,
}

/// Where a formula is.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FormulaPlace {
    /// A reaction's rate law, by reaction id.
    RateLaw(Id),
    InitialAssignment(usize),
    AssignmentRule(usize),
    RateRule(usize),
    AlgebraicRule(usize),
    Constraint(usize),
    Trigger(usize),
    Delay(usize),
    Priority(usize),
    /// Event, assignment.
    EventAssignment(usize, usize),
}

/// The names a formula uses: its identifiers if it parses, otherwise every
/// run of letters, digits and `_` that doesn't start with a digit.
pub fn names_used(formula: &str) -> Vec<String> {
    match crate::expr::parse(formula) {
        Ok(ast) => crate::expr::identifiers(&ast)
            .into_iter()
            .map(str::to_owned)
            .collect(),
        Err(_) => {
            let mut out: Vec<String> = Vec::new();
            let mut word = String::new();
            for c in formula.chars().chain(std::iter::once(' ')) {
                if c.is_alphanumeric() || c == '_' {
                    word.push(c);
                } else if !word.is_empty() {
                    let w = std::mem::take(&mut word);
                    if !w.starts_with(|c: char| c.is_ascii_digit()) && !out.contains(&w) {
                        out.push(w);
                    }
                }
            }
            out
        }
    }
}

impl Model {
    /// A copy with the model's functions written into the formulas that
    /// call them, and no functions (docs/sbml-full-support-design.md F1's
    /// preference, docs/sbml-app-design.md A4). A formula that calls none is
    /// left as it is; a function a call can't be written in for (its body
    /// doesn't read, or the call has the wrong number of arguments) is kept,
    /// with its calls.
    pub fn with_functions_written_in(&self) -> Model {
        use crate::expr;
        let defs: std::collections::HashMap<String, expr::FunctionDef> = self
            .function_definitions
            .iter()
            .filter_map(|f| {
                Some((
                    f.id.clone(),
                    expr::FunctionDef {
                        params: f.arguments.clone(),
                        body: expr::parse(&f.body).ok()?,
                    },
                ))
            })
            .collect();
        let mut m = self.clone();
        let mut still: Vec<String> = Vec::new();
        m.formulas_mut(|text, _| {
            let Ok(ast) = expr::parse(text) else {
                return;
            };
            let mut calls = Vec::new();
            crate::drafts::called(&ast, &mut calls);
            if !calls.iter().any(|c| defs.contains_key(c)) {
                return;
            }
            match expr::expand_functions(&ast, &defs) {
                Ok(x) => *text = expr::to_infix(&x),
                Err(_) => still.extend(calls),
            }
        });
        // What is still called, and what that calls.
        let mut keep: Vec<String> = Vec::new();
        while let Some(id) = still.pop() {
            if keep.contains(&id) {
                continue;
            }
            if let Some(f) = self.function_definitions.iter().find(|f| f.id == id) {
                if let Ok(ast) = expr::parse(&f.body) {
                    crate::drafts::called(&ast, &mut still);
                }
                keep.push(id);
            }
        }
        m.function_definitions.retain(|f| keep.contains(&f.id));
        m
    }

    /// Every formula in the model except function bodies (which use only
    /// their arguments), with where it is.
    pub fn formulas(&self) -> Vec<(FormulaPlace, &str)> {
        let mut out: Vec<(FormulaPlace, &str)> = Vec::new();
        for r in self.reactions() {
            out.push((FormulaPlace::RateLaw(r.id.clone()), &r.kinetic_law));
        }
        for (i, a) in self.initial_assignments.iter().enumerate() {
            out.push((FormulaPlace::InitialAssignment(i), &a.expression));
        }
        for (i, r) in self.assignment_rules.iter().enumerate() {
            out.push((FormulaPlace::AssignmentRule(i), &r.expression));
        }
        for (i, r) in self.rate_rules.iter().enumerate() {
            out.push((FormulaPlace::RateRule(i), &r.expression));
        }
        for (i, r) in self.algebraic_rules.iter().enumerate() {
            out.push((FormulaPlace::AlgebraicRule(i), &r.expression));
        }
        for (i, c) in self.constraints.iter().enumerate() {
            out.push((FormulaPlace::Constraint(i), &c.expression));
        }
        for (i, e) in self.events.iter().enumerate() {
            out.push((FormulaPlace::Trigger(i), &e.trigger.expression));
            if let Some(d) = &e.delay {
                out.push((FormulaPlace::Delay(i), &d.expression));
            }
            if let Some(p) = &e.priority {
                out.push((FormulaPlace::Priority(i), &p.expression));
            }
            for (j, a) in e.assignments.iter().enumerate() {
                out.push((FormulaPlace::EventAssignment(i, j), &a.expression));
            }
        }
        out
    }

    /// Calls `f` with every formula [`Model::formulas`] lists, to change it,
    /// and the names that mean something else there: a rate law's local
    /// parameters.
    pub fn formulas_mut(&mut self, mut f: impl FnMut(&mut String, &[String])) {
        for r in self.reactions_mut() {
            let locals: Vec<String> = r
                .sbml
                .local_parameters
                .iter()
                .map(|p| p.id.clone())
                .collect();
            f(&mut r.kinetic_law, &locals);
        }
        let none: &[String] = &[];
        for a in &mut self.initial_assignments {
            f(&mut a.expression, none);
        }
        for r in &mut self.assignment_rules {
            f(&mut r.expression, none);
        }
        for r in &mut self.rate_rules {
            f(&mut r.expression, none);
        }
        for r in &mut self.algebraic_rules {
            f(&mut r.expression, none);
        }
        for c in &mut self.constraints {
            f(&mut c.expression, none);
        }
        for e in &mut self.events {
            f(&mut e.trigger.expression, none);
            if let Some(d) = &mut e.delay {
                f(&mut d.expression, none);
            }
            if let Some(p) = &mut e.priority {
                f(&mut p.expression, none);
            }
            for a in &mut e.assignments {
                f(&mut a.expression, none);
            }
        }
    }

    /// Every annotation in the model: the model's own, its compartments',
    /// species' (primaries only), parameters', rules', reactions', and those
    /// of every other part of SBML it holds.
    pub fn annotations(&self) -> Vec<&Annotation> {
        let mut out: Vec<&Annotation> = vec![&self.annotation];
        out.extend(self.compartments.iter().map(|c| &c.annotation));
        out.extend(
            self.species()
                .iter()
                .filter(|s| !s.is_alias() && !s.is_null_node)
                .map(|s| &s.annotation),
        );
        out.extend(self.parameters.iter().map(|p| &p.annotation));
        out.extend(self.assignment_rules.iter().map(|r| &r.annotation));
        out.extend(self.function_definitions.iter().map(|f| &f.annotation));
        for u in &self.unit_definitions {
            out.push(&u.annotation);
            out.extend(u.units.iter().map(|x| &x.annotation));
        }
        out.extend(self.initial_assignments.iter().map(|a| &a.annotation));
        out.extend(self.rate_rules.iter().map(|r| &r.annotation));
        out.extend(self.algebraic_rules.iter().map(|r| &r.annotation));
        out.extend(self.constraints.iter().map(|c| &c.annotation));
        for e in &self.events {
            out.push(&e.annotation);
            out.push(&e.trigger.annotation);
            out.extend(e.delay.iter().map(|x| &x.annotation));
            out.extend(e.priority.iter().map(|x| &x.annotation));
            out.extend(e.assignments.iter().map(|x| &x.annotation));
        }
        for r in self.reactions() {
            out.push(&r.annotation);
            out.push(&r.sbml.kinetic_law_annotation);
            out.extend(r.sbml.local_parameters.iter().map(|p| &p.annotation));
            for p in r.reactants.iter().chain(&r.products).chain(&r.modifiers) {
                if let Some(x) = &p.sbml {
                    out.push(&x.annotation);
                }
            }
        }
        out
    }

    /// The initial assignment to `symbol`, if it has one.
    pub fn initial_assignment(&self, symbol: &str) -> Option<&InitialAssignment> {
        self.initial_assignments.iter().find(|a| a.symbol == symbol)
    }

    /// A parameter's value as Antimony and the native file write it: the
    /// formula its initial assignment gives, else its value, else empty.
    pub fn parameter_text(&self, variable: &str) -> String {
        // One with no formula (the Test Suite has them) gives no text.
        if let Some(a) = self
            .initial_assignment(variable)
            .filter(|a| !a.expression.trim().is_empty())
        {
            return a.expression.clone();
        }
        self.parameters
            .iter()
            .find(|p| p.variable == variable)
            .and_then(|p| p.value)
            .map(format_number)
            .unwrap_or_default()
    }

    /// Gives parameter `variable` the value or formula `text`, as Antimony
    /// writes one (`k = 0.1`, `k = 2*Vm`): a number is its value, anything
    /// else its initial assignment. The parameter must exist.
    pub fn set_parameter_text(&mut self, variable: &str, text: &str) {
        let text = text.trim();
        let number = number_text(text);
        if let Some(p) = self.parameters.iter_mut().find(|p| p.variable == variable) {
            p.value = number;
        }
        self.initial_assignments.retain(|a| a.symbol != variable);
        if number.is_none() && !text.is_empty() {
            self.initial_assignments.push(InitialAssignment {
                symbol: variable.to_owned(),
                expression: text.to_owned(),
                annotation: Annotation::default(),
            });
        }
    }

    /// Renames `old` to `new` in every formula (but not where a rate law's
    /// local parameter of that name hides it), and wherever an item is
    /// named: what an initial assignment, rule or event sets, conversion
    /// factors, and the compartment a reaction takes place in. The item
    /// itself is the caller's to rename.
    pub(crate) fn rename_uses(&mut self, old: &str, new: &str) {
        self.formulas_mut(|text, own| {
            if !own.iter().any(|l| l == old) {
                *text = replace_whole_word(text, old, new);
            }
        });
        let swap = |id: &mut String| {
            if id == old {
                new.clone_into(id);
            }
        };
        self.initial_assignments
            .iter_mut()
            .for_each(|a| swap(&mut a.symbol));
        self.assignment_rules
            .iter_mut()
            .for_each(|r| swap(&mut r.variable));
        self.rate_rules
            .iter_mut()
            .for_each(|r| swap(&mut r.variable));
        for e in &mut self.events {
            e.assignments.iter_mut().for_each(|a| swap(&mut a.variable));
        }
        if let Some(c) = &mut self.conversion_factor {
            swap(c);
        }
        for s in self.species_mut() {
            if let Some(c) = &mut s.sbml.conversion_factor {
                swap(c);
            }
        }
        for r in self.reactions_mut() {
            if let Some(c) = &mut r.sbml.compartment {
                swap(c);
            }
        }
    }

    /// Takes out what would otherwise name an item that has been deleted,
    /// leaving the model valid (F8): an initial assignment, rule, constraint
    /// or event assignment that sets one of `gone` or whose formula uses
    /// one; an event whose trigger, delay or priority uses one; a conversion
    /// factor or reaction compartment naming one. Rate laws are left as they
    /// are. Returns what was taken, in words, for the user.
    pub(crate) fn remove_uses(&mut self, gone: &[Id]) -> Vec<String> {
        if gone.is_empty() {
            return Vec::new();
        }
        let is_gone = |id: &str| gone.iter().any(|g| g == id);
        let uses = |formula: &str| names_used(formula).iter().any(|n| is_gone(n));
        let mut removed: Vec<String> = Vec::new();

        self.initial_assignments.retain(|a| {
            let keep = !is_gone(&a.symbol) && !uses(&a.expression);
            if !keep {
                removed.push(format!("the starting formula of {}", a.symbol));
            }
            keep
        });
        self.assignment_rules.retain(|r| {
            let keep = !is_gone(&r.variable) && !uses(&r.expression);
            if !keep {
                removed.push(format!("the rule for {}", r.variable));
            }
            keep
        });
        self.rate_rules.retain(|r| {
            let keep = !is_gone(&r.variable) && !uses(&r.expression);
            if !keep {
                removed.push(format!("the rate rule for {}", r.variable));
            }
            keep
        });
        self.algebraic_rules.retain(|r| {
            let keep = !uses(&r.expression);
            if !keep {
                removed.push(format!("the rule 0 = {}", r.expression.trim()));
            }
            keep
        });
        self.constraints.retain(|c| {
            let keep = !uses(&c.expression);
            if !keep {
                removed.push(format!("the check {}", c.expression.trim()));
            }
            keep
        });
        self.events.retain_mut(|e| {
            let whole = uses(&e.trigger.expression)
                || e.delay.as_ref().is_some_and(|d| uses(&d.expression))
                || e.priority.as_ref().is_some_and(|p| uses(&p.expression));
            if whole {
                removed.push(e.description());
                return false;
            }
            let what = e.description();
            e.assignments.retain(|a| {
                let keep = !is_gone(&a.variable) && !uses(&a.expression);
                if !keep {
                    removed.push(format!("the change to {} in {what}", a.variable));
                }
                keep
            });
            true
        });
        if self.conversion_factor.as_deref().is_some_and(is_gone) {
            self.conversion_factor = None;
            removed.push("the model's conversion factor".to_owned());
        }
        for s in self.species_mut() {
            if s.sbml.conversion_factor.as_deref().is_some_and(is_gone) {
                s.sbml.conversion_factor = None;
                removed.push(format!("the conversion factor of {}", s.id));
            }
        }
        for r in self.reactions_mut() {
            if r.sbml.compartment.as_deref().is_some_and(is_gone) {
                r.sbml.compartment = None;
                removed.push(format!("the compartment of {}", r.id));
            }
        }
        removed
    }
}

/// A number as formulas and files write it, as SBML does: a whole number
/// without a decimal point, a very large or small one with an exponent
/// (`5.56368e-7`).
/// `text` as a number: Rust's numbers, and of its words for infinity and
/// NaN only Antimony's (`inf`, `-INF`, `NaN`; found by the BioModels round
/// trip), since `Inf` and `infinity` may be ids (the Test Suite has them).
pub fn number_text(text: &str) -> Option<f64> {
    let v = text.trim().parse::<f64>().ok()?;
    if v.is_finite() {
        return Some(v);
    }
    let word = text.trim().trim_start_matches(['-', '+']);
    matches!(word, "inf" | "INF" | "nan" | "NaN").then_some(v)
}

pub fn format_number(v: f64) -> String {
    sbml_io::fmt_f64(v)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Pt;
    use crate::model::{Parameter, Participant, SpeciesNode};

    /// S1 → R1 → S2, rate law `k*S1*V`, a parameter k, a compartment V,
    /// and one of each new part naming S1 or k.
    fn model() -> Model {
        let mut m = Model::new();
        m.push_species(SpeciesNode::new("S1", Pt::ZERO)).unwrap();
        m.push_species(SpeciesNode::new("S2", Pt::ZERO)).unwrap();
        let mut r = crate::model::Reaction::new("R1", Pt::ZERO);
        r.reactants.push(Participant::new("S1", 1.0));
        r.products.push(Participant::new("S2", 1.0));
        r.kinetic_law = "k*S1".into();
        m.push_reaction(r).unwrap();
        m.parameters.push(Parameter::new("k", Some(0.1)));
        m.initial_assignments.push(InitialAssignment {
            symbol: "S2".into(),
            expression: "2*S1".into(),
            ..Default::default()
        });
        m.rate_rules.push(RateRule {
            variable: "k".into(),
            expression: "-0.01*k".into(),
            ..Default::default()
        });
        m.algebraic_rules.push(AlgebraicRule {
            expression: "S1 + S2 - 10".into(),
            ..Default::default()
        });
        m.constraints.push(Constraint {
            expression: "S1 >= 0".into(),
            ..Default::default()
        });
        m.events.push(Event {
            id: Some("Feed".into()),
            trigger: Trigger {
                expression: "time > 10".into(),
                ..Default::default()
            },
            assignments: vec![
                EventAssignment {
                    variable: "S1".into(),
                    expression: "5".into(),
                    ..Default::default()
                },
                EventAssignment {
                    variable: "S2".into(),
                    expression: "k".into(),
                    ..Default::default()
                },
            ],
            ..Default::default()
        });
        m.events.push(Event {
            id: Some("Low".into()),
            trigger: Trigger {
                expression: "S1 < 1".into(),
                ..Default::default()
            },
            ..Default::default()
        });
        m
    }

    #[test]
    fn every_formula_is_visited() {
        let m = model();
        let texts: Vec<&str> = m.formulas().into_iter().map(|(_, t)| t).collect();
        assert_eq!(
            texts,
            [
                "k*S1",
                "2*S1",
                "-0.01*k",
                "S1 + S2 - 10",
                "S1 >= 0",
                "time > 10",
                "5",
                "k",
                "S1 < 1"
            ]
        );
    }

    #[test]
    fn a_species_is_renamed_everywhere() {
        let mut m = model();
        m.rename_species("S1", "Glc").unwrap();
        let texts: Vec<String> = m
            .formulas()
            .into_iter()
            .map(|(_, t)| t.to_owned())
            .collect();
        assert!(texts.iter().all(|t| !t.contains("S1")), "{texts:?}");
        assert_eq!(m.events[0].assignments[0].variable, "Glc");
        assert_eq!(m.events[1].trigger.expression, "Glc < 1");
        assert_eq!(m.algebraic_rules[0].expression, "Glc + S2 - 10");
    }

    #[test]
    fn a_local_parameter_hides_a_rename() {
        let mut m = model();
        m.find_reaction_mut("R1")
            .unwrap()
            .sbml
            .local_parameters
            .push(LocalParameter {
                id: "k".into(),
                value: Some(2.0),
                ..Default::default()
            });
        m.rename_parameter("k", "kdeg").unwrap();
        // The rate law's k is its own; the rate rule's and event's are the model's.
        assert_eq!(m.find_reaction("R1").unwrap().kinetic_law, "k*S1");
        assert_eq!(m.rate_rules[0].variable, "kdeg");
        assert_eq!(m.rate_rules[0].expression, "-0.01*kdeg");
        assert_eq!(m.events[0].assignments[1].expression, "kdeg");
        assert_eq!(m.parameters[0].variable, "kdeg");
    }

    #[test]
    fn deleting_a_species_takes_what_names_it() {
        let mut m = model();
        let deleted = m.delete_species("S1");
        // R1 goes with its reactant; everything naming S1 goes too.
        assert_eq!(deleted.reactions, vec!["R1".to_owned()]);
        assert!(m.initial_assignments.is_empty());
        assert!(m.algebraic_rules.is_empty());
        assert!(m.constraints.is_empty());
        // Feed loses its change to S1 and keeps its change to S2; Low, whose
        // trigger uses S1, goes whole.
        assert_eq!(m.events.len(), 1);
        assert_eq!(m.events[0].assignments.len(), 1);
        assert_eq!(m.events[0].assignments[0].variable, "S2");
        // The rate rule names only k: kept.
        assert_eq!(m.rate_rules.len(), 1);
        assert_eq!(
            deleted.removed,
            [
                "the starting formula of S2",
                "the rule 0 = S1 + S2 - 10",
                "the check S1 >= 0",
                "the change to S1 in the event Feed",
                "the event Low",
            ]
        );
    }

    #[test]
    fn deleting_a_reaction_takes_formulas_using_its_rate_or_stoichiometry() {
        let mut m = model();
        let r = m.find_reaction_mut("R1").unwrap();
        r.reactants[0].sbml = Some(Box::new(SpeciesReference {
            id: Some("n1".into()),
            ..Default::default()
        }));
        m.constraints.push(Constraint {
            expression: "R1 < 5".into(),
            ..Default::default()
        });
        m.initial_assignments.push(InitialAssignment {
            symbol: "n1".into(),
            expression: "2".into(),
            ..Default::default()
        });
        let deleted = m.delete_reaction("R1").unwrap();
        assert_eq!(
            deleted.removed,
            ["the starting formula of n1", "the check R1 < 5"]
        );
        assert_eq!(m.constraints.len(), 1);
    }

    #[test]
    fn deleting_a_compartment_takes_formulas_using_its_size() {
        let mut m = model();
        m.compartments.push(crate::model::Compartment::new("cell"));
        m.rate_rules.push(RateRule {
            variable: "cell".into(),
            expression: "0.1".into(),
            ..Default::default()
        });
        m.reactions_mut().next().unwrap().sbml.compartment = Some("cell".into());
        let deleted = m.delete_compartment("cell").unwrap();
        assert_eq!(
            deleted.removed,
            ["the rate rule for cell", "the compartment of R1"]
        );
        // A rate law naming it is left as it is (F8).
    }

    #[test]
    fn names_are_found_in_formulas_that_dont_parse() {
        assert_eq!(names_used("k*S1 + f(S2)"), ["k", "S1", "S2"]);
        assert_eq!(names_used("k*(S1 +"), ["k", "S1"]);
        assert_eq!(names_used("2e3 + x_1"), ["x_1"]);
    }

    #[test]
    fn parameter_text_is_its_formula_or_value() {
        let mut m = Model::new();
        m.parameters.push(Parameter::new("k", Some(0.5)));
        assert_eq!(m.parameter_text("k"), "0.5");
        m.set_parameter_text("k", "2*Vm");
        assert_eq!(m.parameters[0].value, None);
        assert_eq!(m.parameter_text("k"), "2*Vm");
        m.set_parameter_text("k", " 3 ");
        assert_eq!(m.parameters[0].value, Some(3.0));
        assert!(m.initial_assignments.is_empty());
        m.set_parameter_text("k", "");
        assert_eq!(m.parameter_text("k"), "");
    }
}
