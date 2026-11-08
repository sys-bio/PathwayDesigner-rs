//! Editing the model's events, rules and the rest in the Events & Rules tab
//! (docs/sbml-app-design.md A1): a [`Draft`] of one part, as text in a form;
//! [`check`] says what is wrong with it as it is typed; [`apply`] puts it in
//! the model.
//!
//! A formula left as the form showed it keeps its text exactly (`5.0` stays
//! `5.0`); one typed is read as Antimony reads it, its whole numbers reals,
//! so `1/2` means a half in every simulator.

use crate::expr::{self, Ast};
use crate::model::{
    AlgebraicRule, AssignmentRule, Constraint, Event, EventAssignment, EventFormula,
    FunctionDefinition, InitialAssignment, Model, Parameter, RateRule, Trigger, UnitDefinition,
};
use crate::sentences::Part;

/// One part as the form holds it.
#[derive(Clone, Debug, PartialEq)]
pub enum Draft {
    Event(EventDraft),
    Rule(RuleDraft),
    Start {
        symbol: String,
        formula: String,
    },
    Function {
        id: String,
        arguments: String,
        body: String,
    },
    Unit {
        id: String,
        text: String,
    },
    Check {
        formula: String,
        message: String,
    },
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct EventDraft {
    pub name: String,
    pub trigger: String,
    pub delay: String,
    pub priority: String,
    /// What it sets, and to what.
    pub assignments: Vec<(String, String)>,
    /// SBML's `initialValue="false"`: the condition true at the start fires it.
    pub fires_at_start: bool,
    /// SBML's `persistent`.
    pub persistent: bool,
    /// SBML's `useValuesFromTriggerTime`.
    pub values_at_trigger: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuleKind {
    /// `y is always …`.
    Assignment,
    /// `y changes at the rate …`.
    Rate,
    /// `… is kept at 0`.
    Algebraic,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RuleDraft {
    pub kind: RuleKind,
    pub variable: String,
    pub formula: String,
}

/// A field of the form, for saying where a problem is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    Name,
    Trigger,
    Delay,
    Priority,
    /// What the `n`th assignment sets (0 for a rule's or a starting value's).
    Variable(usize),
    /// The `n`th assignment's formula (0 for any other formula).
    Formula(usize),
    Id,
    Arguments,
    Body,
    Text,
}

/// What [`check`] found: an error stops Done; a note doesn't.
#[derive(Clone, Debug, PartialEq)]
pub struct Problem {
    pub field: Field,
    pub message: String,
    pub error: bool,
}

/// A formula as the form shows it: whole numbers without `.0`.
pub fn editable(formula: &str) -> String {
    crate::sentences::whole_numbers(formula.trim())
}

/// The part as the form starts it.
pub fn draft_of(model: &Model, part: Part) -> Option<Draft> {
    let f = editable;
    Some(match part {
        Part::Event(i) => {
            let e = model.events.get(i)?;
            Draft::Event(EventDraft {
                name: e.name.clone().unwrap_or_default(),
                trigger: f(&e.trigger.expression),
                delay: e
                    .delay
                    .as_ref()
                    .map(|d| f(&d.expression))
                    .unwrap_or_default(),
                priority: e
                    .priority
                    .as_ref()
                    .map(|d| f(&d.expression))
                    .unwrap_or_default(),
                assignments: e
                    .assignments
                    .iter()
                    .map(|a| (a.variable.clone(), f(&a.expression)))
                    .collect(),
                fires_at_start: !e.trigger.initial_value,
                persistent: e.trigger.persistent,
                values_at_trigger: e.use_values_from_trigger_time,
            })
        }
        Part::AssignmentRule(i) => {
            let r = model.assignment_rules.get(i)?;
            Draft::Rule(RuleDraft {
                kind: RuleKind::Assignment,
                variable: r.variable.clone(),
                formula: f(&r.expression),
            })
        }
        Part::RateRule(i) => {
            let r = model.rate_rules.get(i)?;
            Draft::Rule(RuleDraft {
                kind: RuleKind::Rate,
                variable: r.variable.clone(),
                formula: f(&r.expression),
            })
        }
        Part::AlgebraicRule(i) => Draft::Rule(RuleDraft {
            kind: RuleKind::Algebraic,
            variable: String::new(),
            formula: f(&model.algebraic_rules.get(i)?.expression),
        }),
        Part::InitialAssignment(i) => {
            let a = model.initial_assignments.get(i)?;
            Draft::Start {
                symbol: a.symbol.clone(),
                formula: f(&a.expression),
            }
        }
        Part::Function(i) => {
            let x = model.function_definitions.get(i)?;
            Draft::Function {
                id: x.id.clone(),
                arguments: x.arguments.join(", "),
                body: f(&x.body),
            }
        }
        Part::Unit(i) => {
            let u = model.unit_definitions.get(i)?;
            Draft::Unit {
                id: u.id.clone(),
                text: crate::antimony::unit_text(u),
            }
        }
        Part::Constraint(i) => {
            let c = model.constraints.get(i)?;
            Draft::Check {
                formula: f(&c.expression),
                message: c
                    .message
                    .as_deref()
                    .map(crate::sentences::plain_text)
                    .unwrap_or_default(),
            }
        }
    })
}

/// An empty event, as Add an event starts it.
pub fn new_event() -> Draft {
    Draft::Event(EventDraft {
        assignments: vec![(String::new(), String::new())],
        persistent: true,
        values_at_trigger: true,
        ..EventDraft::default()
    })
}

/// An empty rule of `kind`.
pub fn new_rule(kind: RuleKind) -> Draft {
    Draft::Rule(RuleDraft {
        kind,
        variable: String::new(),
        formula: String::new(),
    })
}

// ----- Checking -------------------------------------------------------------------

/// What is wrong with `draft` as the form holds it, if it replaces `part`
/// (none for a new one). Errors stop Done; notes say what Done will do.
pub fn check(model: &Model, part: Option<Part>, draft: &Draft) -> Vec<Problem> {
    let mut out = Vec::new();
    let mut error = |field: Field, message: String| {
        out.push(Problem {
            field,
            message,
            error: true,
        })
    };
    let formula = |text: &str, own: &[String]| check_formula(model, text, own);
    match draft {
        Draft::Event(e) => {
            if let Err(m) = formula(&e.trigger, &[]) {
                error(Field::Trigger, m);
            }
            for (field, text) in [(Field::Delay, &e.delay), (Field::Priority, &e.priority)] {
                if !text.trim().is_empty()
                    && let Err(m) = formula(text, &[])
                {
                    error(field, m);
                }
            }
            for (n, (var, text)) in e.assignments.iter().enumerate() {
                if var.trim().is_empty() && text.trim().is_empty() {
                    continue;
                }
                match settable(model, var) {
                    Err(m) => error(Field::Variable(n), m),
                    Ok(()) => {
                        if model
                            .assignment_rules
                            .iter()
                            .any(|r| r.variable == var.trim())
                        {
                            error(
                                Field::Variable(n),
                                format!(
                                    "{} is always set by a rule, so an event can't change it",
                                    var.trim()
                                ),
                            );
                        } else if e.assignments[..n]
                            .iter()
                            .any(|(v, _)| v.trim() == var.trim())
                        {
                            error(Field::Variable(n), format!("{} is set twice", var.trim()));
                        }
                    }
                }
                if let Err(m) = formula(text, &[]) {
                    error(Field::Formula(n), m);
                }
            }
        }
        Draft::Rule(r) => {
            if r.kind != RuleKind::Algebraic {
                let var = r.variable.trim();
                match settable(model, var) {
                    Err(m) => error(Field::Variable(0), m),
                    Ok(()) => {
                        if let Some(m) = rule_conflict(model, part, r.kind, var) {
                            error(Field::Variable(0), m);
                        }
                    }
                }
            }
            if let Err(m) = formula(&r.formula, &[]) {
                error(Field::Formula(0), m);
            }
        }
        Draft::Start {
            symbol,
            formula: text,
        } => {
            let symbol = symbol.trim();
            match settable(model, symbol) {
                Err(m) => error(Field::Variable(0), m),
                Ok(()) => {
                    let other = model.initial_assignments.iter().enumerate().any(|(i, a)| {
                        a.symbol == symbol && part != Some(Part::InitialAssignment(i))
                    });
                    if other {
                        error(
                            Field::Variable(0),
                            format!("{symbol} already starts at a formula"),
                        );
                    } else if model.assignment_rules.iter().any(|r| r.variable == symbol) {
                        error(
                            Field::Variable(0),
                            format!("{symbol} is always set by a rule, so its start is the rule's"),
                        );
                    }
                }
            }
            if let Err(m) = formula(text, &[]) {
                error(Field::Formula(0), m);
            }
        }
        Draft::Function {
            id,
            arguments,
            body,
        } => {
            let id = id.trim();
            let mine = |other: &str| match part {
                Some(Part::Function(i)) => model
                    .function_definitions
                    .get(i)
                    .is_some_and(|f| f.id == other),
                _ => false,
            };
            if !crate::antimony::is_identifier(id) {
                error(
                    Field::Id,
                    "A function's name is a word: letters, digits and _".into(),
                );
            } else if !mine(id) && model.is_id_used(id) {
                error(Field::Id, format!("{id} is already used in the model"));
            }
            let args: Vec<String> = arguments
                .split(',')
                .map(|a| a.trim().to_owned())
                .filter(|a| !a.is_empty())
                .collect();
            if let Some(bad) = args.iter().find(|a| !crate::antimony::is_identifier(a)) {
                error(Field::Arguments, format!("{bad} isn't a word"));
            } else if args.iter().enumerate().any(|(i, a)| args[..i].contains(a)) {
                error(Field::Arguments, "An argument is named twice".into());
            }
            if let Err(m) = check_body(model, body, &args) {
                error(Field::Body, m);
            }
        }
        Draft::Unit { id, text } => {
            let id = id.trim();
            let mine = |other: &str| match part {
                Some(Part::Unit(i)) => model.unit_definitions.get(i).is_some_and(|u| u.id == other),
                _ => false,
            };
            if !crate::antimony::is_identifier(id) {
                error(
                    Field::Id,
                    "A unit's name is a word: letters, digits and _".into(),
                );
            } else if !mine(id) && model.unit_definitions.iter().any(|u| u.id == id) {
                error(Field::Id, format!("There is already a unit {id}"));
            }
            if crate::antimony::unit_factors(text).is_none() {
                error(
                    Field::Text,
                    "Write it from SBML's base units, such as 0.001 mole / litre".into(),
                );
            }
        }
        Draft::Check { formula: text, .. } => {
            if let Err(m) = formula(text, &[]) {
                error(Field::Formula(0), m);
            }
        }
    }
    // What Done does besides: new parameters for what isn't in the model.
    for (field, var) in variables(draft) {
        if !var.is_empty() && crate::antimony::is_identifier(var) && !exists(model, var) {
            out.push(Problem {
                field,
                message: format!("{var} isn't in the model; it will be a new parameter"),
                error: false,
            });
        }
    }
    out
}

/// What a draft sets, by field.
fn variables(draft: &Draft) -> Vec<(Field, &str)> {
    match draft {
        Draft::Event(e) => e
            .assignments
            .iter()
            .enumerate()
            .map(|(n, (v, _))| (Field::Variable(n), v.trim()))
            .collect(),
        Draft::Rule(r) if r.kind != RuleKind::Algebraic => {
            vec![(Field::Variable(0), r.variable.trim())]
        }
        _ => Vec::new(),
    }
}

fn exists(model: &Model, id: &str) -> bool {
    model.find_species(id).is_some()
        || model.compartments.iter().any(|c| c.id == id)
        || model.parameters.iter().any(|p| p.variable == id)
}

/// Whether a rule or an event may set `var`: a species, compartment or
/// parameter, or a word for a new parameter.
fn settable(model: &Model, var: &str) -> Result<(), String> {
    if var.is_empty() {
        return Err("Say what it sets".into());
    }
    if !crate::antimony::is_identifier(var) {
        return Err(format!("{var} isn't a name: letters, digits and _"));
    }
    if exists(model, var) {
        return Ok(());
    }
    if model.find_reaction(var).is_some() {
        return Err(format!("{var} is a reaction; its rate is its rate law's"));
    }
    if model.function_definitions.iter().any(|f| f.id == var) {
        return Err(format!("{var} is a function"));
    }
    if crate::sbml::math::is_constant_name(var) {
        return Err(format!("{var} is a constant"));
    }
    Ok(())
}

/// Why a rule of `kind` can't set `var`, if it can't: what else sets it,
/// and a species the reactions change (SBML lets a rule set a species in
/// reactions only if it is a boundary species).
fn rule_conflict(model: &Model, part: Option<Part>, kind: RuleKind, var: &str) -> Option<String> {
    let other_assignment = model
        .assignment_rules
        .iter()
        .enumerate()
        .any(|(i, r)| r.variable == var && part != Some(Part::AssignmentRule(i)));
    let other_rate = model
        .rate_rules
        .iter()
        .enumerate()
        .any(|(i, r)| r.variable == var && part != Some(Part::RateRule(i)));
    if other_assignment || other_rate {
        return Some(format!("{var} is already set by a rule"));
    }
    if kind == RuleKind::Assignment {
        if model.initial_assignments.iter().any(|a| a.symbol == var) {
            return Some(format!(
                "{var} starts at a formula; a rule would set it from the start"
            ));
        }
        if model
            .events
            .iter()
            .any(|e| e.assignments.iter().any(|a| a.variable == var))
        {
            return Some(format!(
                "An event changes {var}, so a rule can't always set it"
            ));
        }
    }
    if let Some(s) = model.find_species(var) {
        let id = s.primary_id().to_owned();
        let boundary = model.find_species(&id).is_some_and(|p| p.is_boundary);
        let in_reactions = model.reactions().iter().any(|r| {
            r.reactants.iter().chain(&r.products).any(|p| {
                model
                    .find_species(&p.species)
                    .is_some_and(|n| n.primary_id() == id)
            })
        });
        if in_reactions && !boundary {
            return Some(format!(
                "{var} takes part in reactions; a rule can set it only if it is a boundary species"
            ));
        }
    }
    None
}

/// Whether `text` is a formula of the model: it reads, and every name in it
/// is the model's, one of `own` (a function's arguments), or a constant.
fn check_formula(model: &Model, text: &str, own: &[String]) -> Result<(), String> {
    let text = text.trim();
    if text.is_empty() {
        return Err("Write a formula".into());
    }
    let ast = expr::parse(text).map_err(|e| format!("Can't read it: {}", e.message))?;
    let unknown: Vec<&str> = expr::identifiers(&ast)
        .into_iter()
        .filter(|n| {
            !(own.iter().any(|a| a == n)
                || exists(model, n)
                || model.find_reaction(n).is_some()
                || crate::sbml::math::is_constant_name(n))
        })
        .collect();
    if !unknown.is_empty() {
        return Err(format!("Not in the model: {}", unknown.join(", ")));
    }
    let mut calls = Vec::new();
    called(&ast, &mut calls);
    if let Some(f) = calls.iter().find(|f| !known_function(model, f)) {
        return Err(format!("No function called {f}"));
    }
    Ok(())
}

/// A function's body: it uses only its arguments, constants and functions.
fn check_body(model: &Model, text: &str, args: &[String]) -> Result<(), String> {
    let text = text.trim();
    if text.is_empty() {
        return Err("Write a formula".into());
    }
    let ast = expr::parse(text).map_err(|e| format!("Can't read it: {}", e.message))?;
    let unknown: Vec<&str> = expr::identifiers(&ast)
        .into_iter()
        .filter(|n| !(args.iter().any(|a| a == n) || crate::sbml::math::is_constant_name(n)))
        .collect();
    if !unknown.is_empty() {
        return Err(format!(
            "A function uses only its arguments; not one: {}",
            unknown.join(", ")
        ));
    }
    let mut calls = Vec::new();
    called(&ast, &mut calls);
    if let Some(f) = calls.iter().find(|f| !known_function(model, f)) {
        return Err(format!("No function called {f}"));
    }
    Ok(())
}

pub(crate) fn called(ast: &Ast, out: &mut Vec<String>) {
    match ast {
        Ast::Number(..) | Ast::Ident(..) => {}
        Ast::Unary(_, e, _) => called(e, out),
        Ast::Binary(_, l, r, _) => {
            called(l, out);
            called(r, out);
        }
        Ast::Call(name, args, _) => {
            out.push(name.clone());
            args.iter().for_each(|a| called(a, out));
        }
    }
}

fn known_function(model: &Model, name: &str) -> bool {
    expr::KNOWN_FUNCTIONS.contains(&name)
        || matches!(name, "piecewise" | "delay" | "rateOf")
        || sbml_io::Op::from_tag(name).is_some()
        || model.function_definitions.iter().any(|f| f.id == name)
}

// ----- Applying -------------------------------------------------------------------

/// `text` as stored: as it was if the form still shows it as it did,
/// else read as Antimony reads it.
fn stored(text: &str, was: Option<&str>) -> String {
    match was {
        Some(was) if editable(was) == text.trim() => was.to_owned(),
        _ => crate::antimony::read_formula(text),
    }
}

/// Puts `draft`, checked with no errors, in `model`, replacing `part` (or
/// adding it); what it sets is made a parameter if it is nothing yet, and
/// not constant. Returns where it now is.
pub fn apply(model: &mut Model, part: Option<Part>, draft: &Draft) -> Option<Part> {
    // Each set item: a new parameter if needed, and not constant.
    for (_, var) in variables(draft) {
        if var.is_empty() {
            continue;
        }
        let start = match draft {
            Draft::Rule(r) if r.kind == RuleKind::Assignment => None,
            _ => Some(0.0),
        };
        if !exists(model, var) {
            model.parameters.push(Parameter::new(var.to_owned(), start));
        }
        set_not_constant(model, var);
    }
    Some(match draft {
        Draft::Event(d) => {
            let i = match part {
                Some(Part::Event(i)) => i,
                _ => {
                    model.events.push(Event::default());
                    model.events.len() - 1
                }
            };
            let e = &mut model.events[i];
            e.name = Some(d.name.trim().to_owned()).filter(|n| !n.is_empty());
            e.no_trigger = false;
            e.trigger = Trigger {
                expression: stored(&d.trigger, Some(&e.trigger.expression)),
                initial_value: !d.fires_at_start,
                persistent: d.persistent,
                annotation: std::mem::take(&mut e.trigger.annotation),
            };
            e.use_values_from_trigger_time = d.values_at_trigger;
            let formula = |text: &str, was: Option<&EventFormula>| -> Option<EventFormula> {
                (!text.trim().is_empty()).then(|| EventFormula {
                    expression: stored(text, was.map(|w| w.expression.as_str())),
                    annotation: was.map(|w| w.annotation.clone()).unwrap_or_default(),
                })
            };
            e.delay = formula(&d.delay, e.delay.as_ref());
            e.priority = formula(&d.priority, e.priority.as_ref());
            let old = std::mem::take(&mut e.assignments);
            e.assignments = d
                .assignments
                .iter()
                .filter(|(v, t)| !v.trim().is_empty() || !t.trim().is_empty())
                .map(|(v, t)| {
                    let was = old.iter().find(|a| a.variable == v.trim());
                    EventAssignment {
                        variable: v.trim().to_owned(),
                        expression: stored(t, was.map(|a| a.expression.as_str())),
                        annotation: was.map(|a| a.annotation.clone()).unwrap_or_default(),
                    }
                })
                .collect();
            Part::Event(i)
        }
        Draft::Rule(d) => {
            // The rule as it was: its formula and annotation, and where it
            // was, which a change of kind leaves.
            let (was, annotation) = match part {
                Some(Part::AssignmentRule(i)) => {
                    let r = model.assignment_rules.remove(i);
                    (Some(r.expression), r.annotation)
                }
                Some(Part::RateRule(i)) => {
                    let r = model.rate_rules.remove(i);
                    (Some(r.expression), r.annotation)
                }
                Some(Part::AlgebraicRule(i)) => {
                    let r = model.algebraic_rules.remove(i);
                    (Some(r.expression), r.annotation)
                }
                _ => (None, Default::default()),
            };
            let expression = stored(&d.formula, was.as_deref());
            let variable = d.variable.trim().to_owned();
            let at = |old: Option<usize>, len: usize| old.filter(|&i| i <= len).unwrap_or(len);
            match d.kind {
                RuleKind::Assignment => {
                    let old = match part {
                        Some(Part::AssignmentRule(i)) => Some(i),
                        _ => None,
                    };
                    let i = at(old, model.assignment_rules.len());
                    model.assignment_rules.insert(
                        i,
                        AssignmentRule {
                            variable,
                            expression,
                            annotation,
                        },
                    );
                    Part::AssignmentRule(i)
                }
                RuleKind::Rate => {
                    let old = match part {
                        Some(Part::RateRule(i)) => Some(i),
                        _ => None,
                    };
                    let i = at(old, model.rate_rules.len());
                    model.rate_rules.insert(
                        i,
                        RateRule {
                            variable,
                            expression,
                            annotation,
                        },
                    );
                    Part::RateRule(i)
                }
                RuleKind::Algebraic => {
                    let old = match part {
                        Some(Part::AlgebraicRule(i)) => Some(i),
                        _ => None,
                    };
                    let i = at(old, model.algebraic_rules.len());
                    model.algebraic_rules.insert(
                        i,
                        AlgebraicRule {
                            expression,
                            annotation,
                        },
                    );
                    Part::AlgebraicRule(i)
                }
            }
        }
        Draft::Start { symbol, formula } => {
            let symbol = symbol.trim().to_owned();
            match part {
                Some(Part::InitialAssignment(i)) => {
                    let a = model.initial_assignments.get_mut(i)?;
                    a.expression = stored(formula, Some(&a.expression));
                    a.symbol = symbol;
                    Part::InitialAssignment(i)
                }
                _ => {
                    model.initial_assignments.push(InitialAssignment {
                        symbol,
                        expression: stored(formula, None),
                        annotation: Default::default(),
                    });
                    Part::InitialAssignment(model.initial_assignments.len() - 1)
                }
            }
        }
        Draft::Function {
            id,
            arguments,
            body,
        } => {
            let arguments: Vec<String> = arguments
                .split(',')
                .map(|a| a.trim().to_owned())
                .filter(|a| !a.is_empty())
                .collect();
            let id = id.trim().to_owned();
            let i = match part {
                Some(Part::Function(i)) => {
                    let old = model.function_definitions.get(i)?.id.clone();
                    if old != id {
                        // Calls follow the new name.
                        model.formulas_mut(|t, _| {
                            *t = crate::model::replace_whole_word(t, &old, &id)
                        });
                    }
                    i
                }
                _ => {
                    model
                        .function_definitions
                        .push(FunctionDefinition::default());
                    model.function_definitions.len() - 1
                }
            };
            let f = &mut model.function_definitions[i];
            f.body = stored(body, (f.arguments == arguments).then_some(f.body.as_str()));
            f.id = id;
            f.arguments = arguments;
            Part::Function(i)
        }
        Draft::Unit { id, text } => {
            let id = id.trim().to_owned();
            let units = crate::antimony::unit_factors(text)?;
            let i = match part {
                Some(Part::Unit(i)) => {
                    let old = model.unit_definitions.get(i)?.id.clone();
                    if old != id {
                        crate::antimony::rename_unit(model, &old, &id);
                    }
                    i
                }
                _ => {
                    model.unit_definitions.push(UnitDefinition {
                        id: id.clone(),
                        ..Default::default()
                    });
                    model.unit_definitions.len() - 1
                }
            };
            let u = &mut model.unit_definitions[i];
            // Its units as they were, if the text is still what they say.
            if crate::antimony::unit_text(u) != text.trim() {
                u.units = units;
            }
            u.id = id;
            Part::Unit(i)
        }
        Draft::Check { formula, message } => {
            let message = message.trim();
            let as_xhtml = |m: &str| {
                format!(
                    "<p xmlns=\"http://www.w3.org/1999/xhtml\">{}</p>",
                    m.replace('&', "&amp;")
                        .replace('<', "&lt;")
                        .replace('>', "&gt;")
                )
            };
            match part {
                Some(Part::Constraint(i)) => {
                    let c = model.constraints.get_mut(i)?;
                    c.expression = stored(formula, Some(&c.expression));
                    let old = c.message.as_deref().map(crate::sentences::plain_text);
                    if old.as_deref().unwrap_or("") != message {
                        c.message = (!message.is_empty()).then(|| as_xhtml(message));
                    }
                    Part::Constraint(i)
                }
                _ => {
                    model.constraints.push(Constraint {
                        expression: stored(formula, None),
                        message: (!message.is_empty()).then(|| as_xhtml(message)),
                        annotation: Default::default(),
                    });
                    Part::Constraint(model.constraints.len() - 1)
                }
            }
        }
    })
}

fn set_not_constant(model: &mut Model, var: &str) {
    if let Some(p) = model.parameters.iter_mut().find(|p| p.variable == var) {
        p.constant = false;
    }
    if let Some(c) = model.compartments.iter_mut().find(|c| c.id == var) {
        c.sbml.constant = false;
    }
    let primary = model.find_species(var).map(|s| s.primary_id().to_owned());
    if let Some(id) = primary
        && let Some(s) = model.find_species_mut(&id)
    {
        s.is_constant = false;
    }
}

/// Why `part` can't be deleted, if it can't: a function the model's
/// formulas call, or a unit something is measured in (either would leave the
/// model naming what isn't there).
pub fn still_used(model: &Model, part: Part) -> Option<String> {
    match part {
        Part::Function(i) => {
            let id = &model.function_definitions.get(i)?.id;
            let mut calls = Vec::new();
            for (_, text) in model.formulas() {
                if let Ok(ast) = expr::parse(&text) {
                    called(&ast, &mut calls);
                }
            }
            for f in &model.function_definitions {
                if let Ok(ast) = expr::parse(&f.body) {
                    called(&ast, &mut calls);
                }
            }
            calls
                .contains(id)
                .then(|| format!("Formulas still use the function {id}"))
        }
        Part::Unit(i) => {
            let id = model.unit_definitions.get(i)?.id.as_str();
            let is = |u: &Option<String>| u.as_deref() == Some(id);
            let u = &model.units;
            let used = [
                &u.substance,
                &u.time,
                &u.volume,
                &u.area,
                &u.length,
                &u.extent,
            ]
            .into_iter()
            .any(is)
                || model.compartments.iter().any(|c| is(&c.sbml.units))
                || model.species().iter().any(|s| is(&s.sbml.units))
                || model.parameters.iter().any(|p| is(&p.units))
                || model
                    .reactions()
                    .iter()
                    .any(|r| r.sbml.local_parameters.iter().any(|p| is(&p.units)));
            used.then(|| format!("Something is still measured in {id}"))
        }
        _ => None,
    }
}

/// Takes `part` out of the model.
pub fn delete(model: &mut Model, part: Part) {
    fn take<T>(list: &mut Vec<T>, i: usize) {
        if i < list.len() {
            list.remove(i);
        }
    }
    match part {
        Part::Event(i) => take(&mut model.events, i),
        Part::AssignmentRule(i) => take(&mut model.assignment_rules, i),
        Part::RateRule(i) => take(&mut model.rate_rules, i),
        Part::AlgebraicRule(i) => take(&mut model.algebraic_rules, i),
        Part::InitialAssignment(i) => take(&mut model.initial_assignments, i),
        Part::Function(i) => take(&mut model.function_definitions, i),
        Part::Unit(i) => take(&mut model.unit_definitions, i),
        Part::Constraint(i) => take(&mut model.constraints, i),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model() -> Model {
        crate::antimony::import(
            "model m\n  function mm(s, v)\n    v*s/(1 + s)\n  end\n  unit minute = 60 second\n  compartment cell = 1\n  species A in cell, B in cell\n  $X in cell\n  J1: A -> B; mm(A, Vm)\n  Vm = 2\n  Vm has minute\n  y := A + B\n  E1: at time > 10: A = 5\n  B = 2*A\nend",
            &crate::antimony::ImportOptions {
                seed: 1,
            },
        )
        .unwrap()
        .model
    }

    fn errors(m: &Model, part: Option<Part>, d: &Draft) -> Vec<String> {
        check(m, part, d)
            .into_iter()
            .filter(|p| p.error)
            .map(|p| p.message)
            .collect()
    }

    #[test]
    fn a_part_saved_as_shown_is_unchanged() {
        let mut m = model();
        // An integer, as SBML may have it, stays one.
        m.assignment_rules[0].expression = "A + 2".into();
        for part in [
            Part::Event(0),
            Part::AssignmentRule(0),
            Part::InitialAssignment(0),
            Part::Function(0),
            Part::Unit(0),
        ] {
            let d = draft_of(&m, part).unwrap();
            assert!(errors(&m, Some(part), &d).is_empty(), "{part:?}: {d:?}");
            let mut again = m.clone();
            assert_eq!(apply(&mut again, Some(part), &d), Some(part));
            assert_eq!(again, m, "{part:?}");
        }
        // The form shows whole numbers without `.0`; the model keeps them.
        let Some(Draft::Event(e)) = draft_of(&m, Part::Event(0)) else {
            panic!()
        };
        assert_eq!(e.assignments, [("A".to_owned(), "5".to_owned())]);
        assert_eq!(m.events[0].assignments[0].expression, "5.0");
    }

    #[test]
    fn a_typed_formula_is_read_as_antimony_reads_it() {
        let mut m = model();
        let d = Draft::Rule(RuleDraft {
            kind: RuleKind::Assignment,
            variable: "y".into(),
            formula: "1/2*A".into(),
        });
        apply(&mut m, Some(Part::AssignmentRule(0)), &d);
        assert_eq!(m.assignment_rules[0].expression, "1.0/2.0*A");
    }

    #[test]
    fn what_is_wrong_is_said_where_it_is() {
        let m = model();
        let rule = |kind, variable: &str, formula: &str| {
            Draft::Rule(RuleDraft {
                kind,
                variable: variable.into(),
                formula: formula.into(),
            })
        };
        let cases: Vec<(Draft, &str)> = vec![
            (
                rule(RuleKind::Assignment, "z", "A + q"),
                "Not in the model: q",
            ),
            (rule(RuleKind::Assignment, "z", "A +"), "Can't read it"),
            (
                rule(RuleKind::Assignment, "z", "f(A)"),
                "No function called f",
            ),
            (rule(RuleKind::Assignment, "", "A"), "Say what it sets"),
            (rule(RuleKind::Assignment, "J1", "A"), "J1 is a reaction"),
            (
                rule(RuleKind::Assignment, "y", "A"),
                "y is already set by a rule",
            ),
            (rule(RuleKind::Rate, "A", "1"), "A takes part in reactions"),
            (
                rule(RuleKind::Assignment, "B", "1"),
                "B starts at a formula",
            ),
            (
                Draft::Start {
                    symbol: "y".into(),
                    formula: "1".into(),
                },
                "y is always set by a rule",
            ),
            (
                Draft::Event(EventDraft {
                    trigger: "".into(),
                    assignments: vec![("y".into(), "1".into())],
                    ..EventDraft::default()
                }),
                "Write a formula",
            ),
            (
                Draft::Event(EventDraft {
                    trigger: "time > 1".into(),
                    assignments: vec![("y".into(), "1".into())],
                    ..EventDraft::default()
                }),
                "y is always set by a rule",
            ),
            (
                Draft::Function {
                    id: "g".into(),
                    arguments: "x".into(),
                    body: "x*Vm".into(),
                },
                "uses only its arguments; not one: Vm",
            ),
            (
                Draft::Function {
                    id: "A".into(),
                    arguments: "x".into(),
                    body: "x".into(),
                },
                "A is already used",
            ),
            (
                Draft::Unit {
                    id: "per_min".into(),
                    text: "1 / furlong".into(),
                },
                "SBML's base units",
            ),
        ];
        for (d, want) in cases {
            let found = errors(&m, None, &d);
            assert!(found.iter().any(|e| e.contains(want)), "{d:?}: {found:?}");
        }
        // A boundary species may be set by a rule; a new name is a note.
        assert!(errors(&m, None, &rule(RuleKind::Rate, "X", "1")).is_empty());
        let notes = check(&m, None, &rule(RuleKind::Rate, "w", "1"));
        assert_eq!(notes.len(), 1);
        assert!(!notes[0].error && notes[0].message.contains("new parameter"));
    }

    #[test]
    fn what_a_new_part_sets_becomes_a_parameter_that_changes() {
        let mut m = model();
        let at = apply(
            &mut m,
            None,
            &Draft::Rule(RuleDraft {
                kind: RuleKind::Rate,
                variable: "w".into(),
                formula: "Vm".into(),
            }),
        );
        assert_eq!(at, Some(Part::RateRule(0)));
        let w = m.parameters.iter().find(|p| p.variable == "w").unwrap();
        assert!(!w.constant);
        assert_eq!(w.value, Some(0.0));
        // The event's: Vm, constant until then.
        assert!(
            m.parameters
                .iter()
                .find(|p| p.variable == "Vm")
                .unwrap()
                .constant
        );
        let mut d = draft_of(&m, Part::Event(0)).unwrap();
        if let Draft::Event(e) = &mut d {
            e.assignments.push(("Vm".into(), "3".into()));
        }
        apply(&mut m, Some(Part::Event(0)), &d);
        assert!(
            !m.parameters
                .iter()
                .find(|p| p.variable == "Vm")
                .unwrap()
                .constant
        );
        assert_eq!(m.events[0].assignments[1].expression, "3.0");
    }

    #[test]
    fn a_rule_changes_kind() {
        let mut m = model();
        let at = apply(
            &mut m,
            Some(Part::AssignmentRule(0)),
            &Draft::Rule(RuleDraft {
                kind: RuleKind::Rate,
                variable: "y".into(),
                formula: "A".into(),
            }),
        );
        assert_eq!(at, Some(Part::RateRule(0)));
        assert!(m.assignment_rules.is_empty());
        assert_eq!(m.rate_rules[0].variable, "y");
    }

    #[test]
    fn what_is_still_used_isnt_deleted() {
        let mut m = model();
        assert!(still_used(&m, Part::Function(0)).unwrap().contains("mm"));
        assert!(still_used(&m, Part::Unit(0)).unwrap().contains("minute"));
        assert_eq!(still_used(&m, Part::Event(0)), None);
        delete(&mut m, Part::Event(0));
        assert!(m.events.is_empty());
        m.parameters.iter_mut().for_each(|p| p.units = None);
        assert_eq!(still_used(&m, Part::Unit(0)), None);
    }

    #[test]
    fn through_the_view_one_undo_step() {
        let m = model();
        let mut v = crate::view::DiagramView::new(m.clone());
        let bad = Draft::Rule(RuleDraft {
            kind: RuleKind::Assignment,
            variable: "z".into(),
            formula: "q".into(),
        });
        assert!(v.save_part(None, &bad).is_err());
        assert_eq!(v.model(), &m);
        let good = Draft::Rule(RuleDraft {
            kind: RuleKind::Assignment,
            variable: "z".into(),
            formula: "2*A".into(),
        });
        let (_, at) = v.save_part(None, &good).unwrap();
        assert_eq!(at, Part::AssignmentRule(1));
        assert!(v.delete_part(Part::Function(0)).is_err());
        v.delete_part(Part::Event(0)).unwrap();
        v.undo();
        v.undo();
        assert_eq!(v.model().assignment_rules, m.assignment_rules);
        assert_eq!(v.model().events, m.events);
    }
}
