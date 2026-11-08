//! The model's events, rules and the rest as plain sentences, for the
//! Events & Rules tab (docs/sbml-app-design.md): "When time > 10: Glucose
//! becomes 5", "y is always A + B", "Volume changes at the rate k*Glucose".

use crate::model::Model;

/// One heading of the tab and what is under it.
#[derive(Clone, Debug, PartialEq)]
pub struct Section {
    pub heading: &'static str,
    pub items: Vec<Item>,
}

/// One part, as a sentence.
#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    /// Which part it is, to go to it and, later, to edit it.
    pub part: Part,
    /// A named event's name, shown before the sentence.
    pub title: Option<String>,
    pub sentence: String,
    /// What else there is to say of it, in small type: an event's delay,
    /// priority and options where they aren't the default.
    pub more: Vec<String>,
}

/// Where a part is in the model, by kind and position.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Part {
    Event(usize),
    AssignmentRule(usize),
    RateRule(usize),
    AlgebraicRule(usize),
    InitialAssignment(usize),
    Function(usize),
    Unit(usize),
    Constraint(usize),
}

/// The tab's sections, in order. Events and Rules always (empty when the
/// model has none); the others only when the model has some.
pub fn sections(model: &Model) -> Vec<Section> {
    let name = |id: &str| shown_name(model, id);
    let mut out = Vec::new();

    let events = model
        .events
        .iter()
        .enumerate()
        .map(|(i, e)| {
            let changes: Vec<String> = e
                .assignments
                .iter()
                .map(|a| format!("{} becomes {}", name(&a.variable), shown(&a.expression)))
                .collect();
            let changes = if changes.is_empty() {
                "nothing changes".to_owned()
            } else {
                changes.join(", ")
            };
            let sentence = if e.no_trigger {
                format!("Never (no condition): {changes}")
            } else if e.trigger.expression.trim().is_empty() {
                format!("When (no condition given): {changes}")
            } else {
                format!("When {}: {changes}", shown(&e.trigger.expression))
            };
            let mut more = Vec::new();
            if let Some(d) = &e.delay {
                more.push(format!("after a delay of {}", shown(&d.expression)));
            }
            if let Some(p) = &e.priority {
                more.push(format!(
                    "priority {} among events at the same time",
                    shown(&p.expression)
                ));
            }
            if e.delay.is_some() && !e.use_values_from_trigger_time {
                more.push("new values worked out when it takes effect".to_owned());
            }
            if !e.no_trigger && !e.trigger.initial_value {
                more.push("can fire at the start".to_owned());
            }
            if !e.no_trigger && !e.trigger.persistent {
                more.push(
                    "stops if its condition stops being true before it takes effect".to_owned(),
                );
            }
            Item {
                part: Part::Event(i),
                title: e.name.clone().or_else(|| e.id.clone()),
                sentence,
                more,
            }
        })
        .collect();
    out.push(Section {
        heading: "Events",
        items: events,
    });

    let mut rules: Vec<Item> = Vec::new();
    for (i, r) in model.assignment_rules.iter().enumerate() {
        rules.push(item(
            Part::AssignmentRule(i),
            format!("{} is always {}", name(&r.variable), shown(&r.expression)),
        ));
    }
    for (i, r) in model.rate_rules.iter().enumerate() {
        rules.push(item(
            Part::RateRule(i),
            format!(
                "{} changes at the rate {}",
                name(&r.variable),
                shown(&r.expression)
            ),
        ));
    }
    for (i, r) in model.algebraic_rules.iter().enumerate() {
        rules.push(item(
            Part::AlgebraicRule(i),
            format!("{} is kept at 0", shown(&r.expression)),
        ));
    }
    out.push(Section {
        heading: "Rules",
        items: rules,
    });

    let starts: Vec<Item> = model
        .initial_assignments
        .iter()
        .enumerate()
        .map(|(i, a)| {
            item(
                Part::InitialAssignment(i),
                format!("{} starts at {}", name(&a.symbol), shown(&a.expression)),
            )
        })
        .collect();
    let functions: Vec<Item> = model
        .function_definitions
        .iter()
        .enumerate()
        .map(|(i, f)| {
            let mut it = item(
                Part::Function(i),
                format!("{}({}) = {}", f.id, f.arguments.join(", "), shown(&f.body)),
            );
            it.title = f.name.clone();
            it
        })
        .collect();
    let units: Vec<Item> = model
        .unit_definitions
        .iter()
        .enumerate()
        .map(|(i, u)| {
            let mut it = item(
                Part::Unit(i),
                format!("{} = {}", u.id, crate::antimony::unit_text(u)),
            );
            it.title = u.name.clone();
            it
        })
        .collect();
    let checks: Vec<Item> = model
        .constraints
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let message = c
                .message
                .as_deref()
                .map(plain_text)
                .filter(|m| !m.is_empty());
            let sentence = match message {
                Some(m) => format!("{}, else: \u{201c}{m}\u{201d}", shown(&c.expression)),
                None => shown(&c.expression),
            };
            item(Part::Constraint(i), sentence)
        })
        .collect();
    for (heading, items) in [
        ("Starting values from formulas", starts),
        ("Functions", functions),
        ("Units", units),
        ("Checks", checks),
    ] {
        if !items.is_empty() {
            out.push(Section { heading, items });
        }
    }
    out
}

fn item(part: Part, sentence: String) -> Item {
    Item {
        part,
        title: None,
        sentence,
        more: Vec::new(),
    }
}

/// An item's name as the diagram shows it: a species' or compartment's
/// label, a parameter's display name when the diagram shows display names,
/// else the id.
pub fn shown_name(model: &Model, id: &str) -> String {
    if let Some(s) = model.find_species(id) {
        return s.label().to_owned();
    }
    if let Some(c) = model.compartments.iter().find(|c| c.id == id) {
        return c.label().to_owned();
    }
    if model.show_display_names
        && let Some(n) = model
            .parameters
            .iter()
            .find(|p| p.variable == id)
            .and_then(|p| p.name.as_deref())
            .filter(|n| !n.trim().is_empty())
    {
        return n.to_owned();
    }
    id.to_owned()
}

/// A formula as the tab shows it: the comparisons and logic in words and
/// signs a reader knows (`≥`, `and`), and whole numbers without `.0`; the
/// formula itself is unchanged.
pub fn shown(formula: &str) -> String {
    let f = formula.trim();
    if f.is_empty() {
        return "(no formula)".to_owned();
    }
    whole_numbers(f)
        .replace(">=", "\u{2265}")
        .replace("<=", "\u{2264}")
        .replace("!=", "\u{2260}")
        .replace("==", "=")
        .replace("&&", "and")
        .replace("||", "or")
}

/// `2.0*x` as `2*x`: a number's fraction of zeros left out. Digits inside
/// a name (`k2.0` can't occur, `x2` can) are left alone.
pub(crate) fn whole_numbers(f: &str) -> String {
    let b = f.as_bytes();
    let mut out = String::with_capacity(f.len());
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        let after_name = i > 0 && (b[i - 1].is_ascii_alphanumeric() || b[i - 1] == b'_');
        if c.is_ascii_digit() && !after_name {
            let start = i;
            while i < b.len() && b[i].is_ascii_digit() {
                i += 1;
            }
            out.push_str(&f[start..i]);
            if i < b.len() && b[i] == b'.' {
                let dot = i;
                i += 1;
                while i < b.len() && b[i].is_ascii_digit() {
                    i += 1;
                }
                if f[dot + 1..i].bytes().any(|d| d != b'0') {
                    out.push_str(&f[dot..i]);
                }
            }
        } else {
            let ch = f[i..].chars().next().expect("at a char boundary");
            out.push(ch);
            i += ch.len_utf8();
        }
    }
    out
}

/// The text of an XHTML message, tags left out and spaces closed up.
pub(crate) fn plain_text(xhtml: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for c in xhtml.chars() {
        match c {
            '<' => in_tag = true,
            '>' => {
                in_tag = false;
                out.push(' ');
            }
            c if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// What else changes `id` (a species' primary id, a compartment's or a
/// parameter's), each with words for it: its rule, its rate rule, the
/// events that set it (docs/sbml-app-design.md A3).
pub fn changed_by(model: &Model, id: &str) -> Vec<(Part, String)> {
    let mut out = Vec::new();
    for (i, r) in model.assignment_rules.iter().enumerate() {
        if r.variable == id {
            out.push((Part::AssignmentRule(i), "its rule".to_owned()));
        }
    }
    for (i, r) in model.rate_rules.iter().enumerate() {
        if r.variable == id {
            out.push((Part::RateRule(i), "its rate rule".to_owned()));
        }
    }
    for (i, e) in model.events.iter().enumerate() {
        if e.assignments.iter().any(|a| a.variable == id) {
            let words = match e.name.as_deref().or(e.id.as_deref()) {
                Some(n) => format!("the event {n}"),
                None => "an event".to_owned(),
            };
            out.push((Part::Event(i), words));
        }
    }
    out
}

/// `id`'s starting formula, if it has one: where it is, and the formula as
/// the tab shows it.
pub fn starts_at(model: &Model, id: &str) -> Option<(Part, String)> {
    let i = model
        .initial_assignments
        .iter()
        .position(|a| a.symbol == id)?;
    Some((
        Part::InitialAssignment(i),
        shown(&model.initial_assignments[i].expression),
    ))
}

/// The species (primary ids) and compartments that rules or events also
/// change, in the model's order: what the diagram marks (A2).
pub fn marked(model: &Model) -> (Vec<String>, Vec<String>) {
    let species = changed_otherwise(model);
    let compartments = model
        .compartments
        .iter()
        .filter(|c| !changed_by(model, &c.id).is_empty())
        .map(|c| c.id.clone())
        .collect();
    (species, compartments)
}

/// The species that rules or events also change, in the model's order:
/// what the stoichiometry alone doesn't account for (F11).
pub fn changed_otherwise(model: &Model) -> Vec<String> {
    let set: Vec<&str> = model
        .assignment_rules
        .iter()
        .map(|r| r.variable.as_str())
        .chain(model.rate_rules.iter().map(|r| r.variable.as_str()))
        .chain(
            model
                .events
                .iter()
                .flat_map(|e| e.assignments.iter().map(|a| a.variable.as_str())),
        )
        .collect();
    model
        .species()
        .iter()
        .filter(|s| !s.is_alias() && !s.is_null_node && set.contains(&s.id.as_str()))
        .map(|s| s.id.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{
        AlgebraicRule, AssignmentRule, Constraint, Event, EventAssignment, EventFormula,
        InitialAssignment, RateRule,
    };

    fn model() -> Model {
        let mut m = crate::antimony::import(
            "model m\n compartment cell = 1\n species Glc in cell, ATP in cell\n J1: Glc -> ATP; k*Glc\n k = 1\n y = 0\n Glc is \"glucose\"\nend",
            &crate::antimony::ImportOptions {
                seed: 1,
            },
        )
        .unwrap()
        .model;
        m.events.push(Event {
            id: Some("Feed".into()),
            trigger: crate::model::Trigger {
                expression: "time >= 10 && Glc < 1".into(),
                ..Default::default()
            },
            delay: Some(EventFormula {
                expression: "2".into(),
                ..Default::default()
            }),
            assignments: vec![EventAssignment {
                variable: "Glc".into(),
                expression: "5".into(),
                ..Default::default()
            }],
            ..Default::default()
        });
        m.assignment_rules.push(AssignmentRule {
            variable: "y".into(),
            expression: "Glc + ATP".into(),
            ..Default::default()
        });
        m.rate_rules.push(RateRule {
            variable: "cell".into(),
            expression: "0.01".into(),
            ..Default::default()
        });
        m.algebraic_rules.push(AlgebraicRule {
            expression: "y - 2*k".into(),
            ..Default::default()
        });
        m.initial_assignments.push(InitialAssignment {
            symbol: "ATP".into(),
            expression: "2*Glc".into(),
            ..Default::default()
        });
        m.constraints.push(Constraint {
            expression: "Glc >= 0".into(),
            message: Some(
                "<p xmlns=\"http://www.w3.org/1999/xhtml\">Glucose went negative.</p>".into(),
            ),
            ..Default::default()
        });
        m
    }

    #[test]
    fn each_part_is_a_sentence() {
        let s = sections(&model());
        let headings: Vec<&str> = s.iter().map(|x| x.heading).collect();
        assert_eq!(
            headings,
            ["Events", "Rules", "Starting values from formulas", "Checks"]
        );
        let event = &s[0].items[0];
        assert_eq!(event.title.as_deref(), Some("Feed"));
        assert_eq!(
            event.sentence,
            "When time \u{2265} 10 and Glc < 1: Glc becomes 5"
        );
        assert_eq!(event.more, ["after a delay of 2"]);
        let rules: Vec<&str> = s[1].items.iter().map(|i| i.sentence.as_str()).collect();
        assert_eq!(
            rules,
            [
                "y is always Glc + ATP",
                "cell changes at the rate 0.01",
                "y - 2*k is kept at 0"
            ]
        );
        assert_eq!(s[2].items[0].sentence, "ATP starts at 2*Glc");
        assert_eq!(
            s[3].items[0].sentence,
            "Glc \u{2265} 0, else: \u{201c}Glucose went negative.\u{201d}"
        );
    }

    #[test]
    fn names_are_as_the_diagram_shows_them() {
        let mut m = model();
        m.show_display_names = true;
        m.refresh_labels();
        let s = sections(&m);
        assert_eq!(
            s[0].items[0].sentence,
            "When time \u{2265} 10 and Glc < 1: glucose becomes 5"
        );
    }

    #[test]
    fn whole_numbers_without_their_zeros() {
        assert_eq!(
            shown("2.0*x2 + 1.50 - 3.0e5 + é1.0"),
            "2*x2 + 1.50 - 3e5 + é1"
        );
    }

    #[test]
    fn a_model_without_any_says_so_under_two_headings() {
        let s = sections(&Model::new());
        assert_eq!(s.len(), 2);
        assert!(s.iter().all(|x| x.items.is_empty()));
    }

    #[test]
    fn what_rules_and_events_also_change() {
        let m = model();
        assert_eq!(changed_otherwise(&m), ["Glc"]);
        assert_eq!(
            changed_by(&m, "Glc"),
            [(Part::Event(0), "the event Feed".to_owned())]
        );
        assert_eq!(
            changed_by(&m, "y"),
            [(Part::AssignmentRule(0), "its rule".to_owned())]
        );
        assert_eq!(
            changed_by(&m, "cell"),
            [(Part::RateRule(0), "its rate rule".to_owned())]
        );
        assert!(changed_by(&m, "ATP").is_empty());
        assert_eq!(
            starts_at(&m, "ATP"),
            Some((Part::InitialAssignment(0), "2*Glc".to_owned()))
        );
        assert_eq!(
            marked(&m),
            (vec!["Glc".to_owned()], vec!["cell".to_owned()])
        );
    }
}
