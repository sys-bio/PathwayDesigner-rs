//! Antimony import and export (S §11.2, §11.3, SPEC-Rust §8.2).
//!
//! The bridge between `pd-antimony`, which reads and writes Antimony text as a
//! plain [`Description`], and the diagram [`Model`]. Everything specific to
//! PathwayDesigner lives here: case resolution, implied parameters, the
//! catalytic rewrite, modifiers from the rate law, null nodes and random
//! placement. To use a complete Antimony crate instead of `pd-antimony`, only
//! this module needs to change.

use std::collections::HashMap;

use pd_antimony::{
    Assignment as AssignmentText, Compartment as CompartmentText, Description,
    Reaction as ReactionText, Species as SpeciesText, Term, Text, line_column,
};
pub use pd_antimony::{Diagnostic, Severity};

use crate::expr;
use crate::geometry::{Pt, pt};
use crate::layout::Rng;
use crate::model::{
    AssignmentRule, Compartment, DEFAULT_COMPARTMENT, Model, NULL_NODE_SIZE, Parameter,
    Participant, ReactionMode, SpeciesNode,
};

/// New species go at random in [0, 600)² (S §11.2).
const PLACEMENT: f64 = 600.0;
/// Null nodes go this far left (source) or right (sink) of the junction.
const NULL_OFFSET: f64 = 70.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ImportOptions {
    /// `A -> A + B` becomes `-> B` with `A` as a modifier (S §12).
    pub convert_catalytic: bool,
    /// For the random placement of the species.
    pub seed: u64,
}

/// A successful import.
#[derive(Clone, Debug)]
pub struct Imported {
    pub model: Model,
    /// Statements that were skipped; the import went ahead anyway.
    pub warnings: Vec<Diagnostic>,
}

/// Reads Antimony text into a new model (S §11.2). Any error means no model:
/// the caller leaves the current diagram as it is.
pub fn import(src: &str, opts: &ImportOptions) -> Result<Imported, Vec<Diagnostic>> {
    let parsed = pd_antimony::parse(src);
    if parsed.has_errors() {
        return Err(parsed.errors().cloned().collect());
    }
    let warnings: Vec<Diagnostic> = parsed.warnings().cloned().collect();
    let mut b = Builder {
        src,
        errors: Vec::new(),
    };
    let model = b.build(&parsed.description, opts);
    if b.errors.is_empty() {
        Ok(Imported { model, warnings })
    } else {
        Err(b.errors)
    }
}

/// Writes the model as Antimony (S §11.3): aliases and null nodes left out,
/// numbers with a `.` whatever the locale.
pub fn export(model: &Model) -> String {
    let mut d = Description {
        name: Some(model.model_name.clone()),
        ..Description::default()
    };
    d.compartments = model
        .compartments
        .iter()
        .filter(|c| !c.id.eq_ignore_ascii_case(DEFAULT_COMPARTMENT))
        .map(|c| CompartmentText {
            id: c.id.clone(),
            size: Some(Text::new(number(c.size))),
        })
        .collect();
    let primaries = model
        .species()
        .iter()
        .filter(|s| !s.is_alias() && !s.is_null_node);
    d.species = primaries
        .map(|s| SpeciesText {
            id: s.id.clone(),
            boundary: s.is_boundary,
            compartment: (!s.compartment.is_empty()
                && !s.compartment.eq_ignore_ascii_case(DEFAULT_COMPARTMENT))
            .then(|| s.compartment.clone()),
            initial: Some(Text::new(number(s.initial_value))),
        })
        .collect();
    let term = |p: &Participant| {
        let node = model.find_species(&p.species)?;
        if node.is_null_node {
            return None;
        }
        let primary = model.find_species(node.primary_id()).unwrap_or(node);
        Some(Term {
            stoichiometry: p.stoichiometry,
            species: primary.id.clone(),
            boundary: primary.is_boundary,
        })
    };
    d.reactions = model
        .reactions()
        .iter()
        .map(|r| ReactionText {
            id: Some(r.id.clone()),
            reactants: r.reactants.iter().filter_map(term).collect(),
            products: r.products.iter().filter_map(term).collect(),
            rate: (!r.kinetic_law.trim().is_empty()).then(|| Text::new(r.kinetic_law.trim())),
            line: 0,
        })
        .collect();
    d.assignments = model
        .parameters
        .iter()
        .map(|p| AssignmentText {
            variable: p.variable.clone(),
            expression: Text::new(p.expression.trim()),
            line: 0,
        })
        .collect();
    d.rules = model
        .assignment_rules
        .iter()
        .map(|r| AssignmentText {
            variable: r.variable.clone(),
            expression: Text::new(r.expression.trim()),
            line: 0,
        })
        .collect();
    pd_antimony::write(&d)
}

/// A number as Antimony text: shortest form, `.` as the decimal separator.
fn number(v: f64) -> String {
    format!("{v}")
}

/// One side of a reaction: species ids with their stoichiometry.
type Side = Vec<(String, f64)>;

/// What is known about one species while building.
struct SpeciesInfo {
    id: String,
    boundary: bool,
    compartment: String,
    initial: f64,
}

struct Builder<'a> {
    src: &'a str,
    errors: Vec<Diagnostic>,
}

impl Builder<'_> {
    fn error(&mut self, offset: usize, message: impl Into<String>) {
        let (line, column) = line_column(self.src, offset);
        self.errors.push(Diagnostic {
            severity: Severity::Error,
            line,
            column,
            message: message.into(),
        });
    }

    /// A number written in the text, or an error.
    fn number(&mut self, t: &Text, what: &str) -> Option<f64> {
        match t.text.trim().parse::<f64>() {
            Ok(v) if v.is_finite() => Some(v),
            _ => {
                self.error(
                    t.offset,
                    format!("{what} must be a number, not '{}'", t.text.trim()),
                );
                None
            }
        }
    }

    /// An expression, parsed: the AST, or an error at the right place.
    fn expression(&mut self, t: &Text) -> Option<expr::Ast> {
        match expr::parse(&t.text) {
            Ok(ast) => Some(ast),
            Err(e) => {
                self.error(t.offset + e.span.start, e.message);
                None
            }
        }
    }

    fn build(&mut self, d: &Description, opts: &ImportOptions) -> Model {
        let mut model = Model::new();
        model.model_name = d.name.clone().unwrap_or_else(|| "anonymous".to_owned());

        // Species: declared ones first, then those only used in reactions.
        // Names typed in another case resolve to the declared spelling.
        let mut species: Vec<SpeciesInfo> = Vec::new();
        let mut by_lower: HashMap<String, usize> = HashMap::new();
        let mut resolve = |id: &str, boundary: bool, species: &mut Vec<SpeciesInfo>| -> String {
            let i = *by_lower.entry(id.to_ascii_lowercase()).or_insert_with(|| {
                species.push(SpeciesInfo {
                    id: id.to_owned(),
                    boundary: false,
                    compartment: String::new(),
                    initial: 0.0,
                });
                species.len() - 1
            });
            species[i].boundary |= boundary;
            species[i].id.clone()
        };
        for s in &d.species {
            let id = resolve(&s.id, s.boundary, &mut species);
            let initial = s
                .initial
                .as_ref()
                .and_then(|t| self.number(t, "A species' initial value"));
            let info = species
                .iter_mut()
                .find(|x| x.id == id)
                .expect("just resolved");
            if let Some(c) = &s.compartment {
                info.compartment = c.clone();
            }
            if let Some(v) = initial {
                info.initial = v;
            }
        }
        let reactions: Vec<(Side, Side)> = d
            .reactions
            .iter()
            .map(|r| {
                let mut side = |terms: &[Term]| {
                    terms
                        .iter()
                        .map(|t| {
                            (
                                resolve(&t.species, t.boundary, &mut species),
                                t.stoichiometry,
                            )
                        })
                        .collect::<Vec<_>>()
                };
                (side(&r.reactants), side(&r.products))
            })
            .collect();

        // Compartments.
        let mut compartments: Vec<Compartment> = Vec::new();
        for c in &d.compartments {
            let mut comp = Compartment::new(c.id.clone());
            if let Some(size) = c
                .size
                .as_ref()
                .and_then(|t| self.number(t, "A compartment's size"))
            {
                comp.size = size;
            }
            compartments.push(comp);
        }

        // `x = expr`: a species' initial value, a compartment's size, or a parameter.
        let mut parameters: Vec<Parameter> = Vec::new();
        for a in &d.assignments {
            if let Some(&i) = by_lower.get(&a.variable.to_ascii_lowercase()) {
                if let Some(v) = self.number(&a.expression, "A species' initial value") {
                    species[i].initial = v;
                }
            } else if let Some(c) = compartments
                .iter()
                .position(|c| c.id.eq_ignore_ascii_case(&a.variable))
            {
                if let Some(v) = self.number(&a.expression, "A compartment's size") {
                    compartments[c].size = v;
                }
            } else {
                self.expression(&a.expression);
                let value = a.expression.text.trim().to_owned();
                match parameters.iter_mut().find(|p| p.variable == a.variable) {
                    Some(p) => p.expression = value,
                    None => parameters.push(Parameter {
                        variable: a.variable.clone(),
                        expression: value,
                    }),
                }
            }
        }
        let rules: Vec<AssignmentRule> = d
            .rules
            .iter()
            .map(|r| {
                self.expression(&r.expression);
                AssignmentRule {
                    variable: r.variable.clone(),
                    expression: r.expression.text.trim().to_owned(),
                }
            })
            .collect();

        // Rate laws, tidied by the expression parser.
        let laws: Vec<Option<expr::Ast>> = d
            .reactions
            .iter()
            .map(|r| r.rate.as_ref().and_then(|t| self.expression(t)))
            .collect();

        // A name in a rate law that is nothing else is a parameter, 0.
        for ast in laws.iter().flatten() {
            for name in expr::identifiers(ast) {
                let known = species.iter().any(|s| s.id == name)
                    || parameters
                        .iter()
                        .any(|p| p.variable.eq_ignore_ascii_case(name))
                    || compartments.iter().any(|c| c.id.eq_ignore_ascii_case(name))
                    || rules.iter().any(|r| r.variable == name);
                if !known {
                    parameters.push(Parameter {
                        variable: name.to_owned(),
                        expression: "0".to_owned(),
                    });
                }
            }
        }

        model.compartments = compartments;
        model.parameters = parameters;
        model.assignment_rules = rules;

        // One node per species, at random.
        let mut rng = Rng::new(opts.seed);
        for s in &species {
            let mut node = SpeciesNode::new(
                s.id.clone(),
                pt(rng.range(0.0, PLACEMENT), rng.range(0.0, PLACEMENT)),
            );
            node.is_boundary = s.boundary;
            node.compartment = s.compartment.clone();
            node.initial_value = s.initial;
            if model.push_species(node).is_err() {
                // Unreachable: names were made unique above.
                continue;
            }
        }

        // Reactions.
        let declared: Vec<&str> = d.reactions.iter().filter_map(|r| r.id.as_deref()).collect();
        let mut next_generated = 1;
        for ((r, (reactants, products)), law) in d.reactions.iter().zip(reactions).zip(laws) {
            let id = match &r.id {
                Some(id) => id.clone(),
                None => loop {
                    let id = format!("R{next_generated}");
                    next_generated += 1;
                    if !declared.contains(&id.as_str()) && !model.is_id_used(&id) {
                        break id;
                    }
                },
            };
            if model.find_reaction(&id).is_some() {
                let offset = self.src.lines().take(r.line - 1).map(|l| l.len() + 1).sum();
                self.error(offset, format!("a second reaction is called '{id}'"));
                continue;
            }
            self.add_reaction(&mut model, &id, reactants, products, law, opts);
        }
        model.recompute_id_counter();
        model
    }

    fn add_reaction(
        &mut self,
        model: &mut Model,
        id: &str,
        mut reactants: Side,
        mut products: Side,
        law: Option<expr::Ast>,
        opts: &ImportOptions,
    ) {
        let centre = |list: &[(String, f64)]| {
            let pts: Vec<Pt> = list
                .iter()
                .filter_map(|(s, _)| model.find_species(s))
                .map(|s| s.center)
                .collect();
            (!pts.is_empty()).then(|| pts.iter().fold(Pt::ZERO, |a, &p| a + p) / pts.len() as f64)
        };
        let junction = match (centre(&reactants), centre(&products)) {
            (Some(a), Some(b)) => (a + b) / 2.0,
            (Some(a), None) | (None, Some(a)) => a,
            (None, None) => pt(PLACEMENT / 2.0, PLACEMENT / 2.0),
        };

        // Catalytic conversion: a species on both sides with the same total
        // stoichiometry comes off both, and becomes a modifier (S §11.2).
        let mut catalysts: Vec<String> = Vec::new();
        if opts.convert_catalytic {
            let total = |list: &[(String, f64)], s: &str| -> f64 {
                list.iter().filter(|(x, _)| x == s).map(|(_, n)| n).sum()
            };
            for (s, _) in &reactants {
                if total(&reactants, s) == total(&products, s) && !catalysts.contains(s) {
                    catalysts.push(s.clone());
                }
            }
            reactants.retain(|(s, _)| !catalysts.contains(s));
            products.retain(|(s, _)| !catalysts.contains(s));
        }

        let mut r = crate::model::Reaction::new(id, junction);
        r.mode = ReactionMode::Straight;
        r.reactants = reactants
            .iter()
            .map(|(s, n)| Participant::new(s.as_str(), *n))
            .collect();
        r.products = products
            .iter()
            .map(|(s, n)| Participant::new(s.as_str(), *n))
            .collect();
        // A species taken off by the catalytic rewrite is the catalyst: a
        // modifier, whether or not the rate law names it.
        r.modifiers = catalysts
            .iter()
            .map(|s| Participant::new(s.as_str(), 1.0))
            .collect();
        if let Some(ast) = &law {
            r.kinetic_law = expr::to_infix(ast);
            // Every species named in the law that isn't a participant is a modifier.
            for name in expr::identifiers(ast) {
                let is_species = model.find_species(name).is_some();
                let participant = r
                    .reactants
                    .iter()
                    .chain(&r.products)
                    .any(|p| p.species == name);
                let already = r.modifiers.iter().any(|p| p.species == name);
                if is_species && !participant && !already {
                    r.modifiers.push(Participant::new(name, 1.0));
                }
            }
        }

        // Null nodes for an empty side.
        for (empty, prefix, dx) in [
            (r.reactants.is_empty(), "_source_", -NULL_OFFSET),
            (r.products.is_empty(), "_sink_", NULL_OFFSET),
        ] {
            if !empty {
                continue;
            }
            let null_id = format!("{prefix}{id}");
            let mut node = SpeciesNode::new(null_id.clone(), junction + pt(dx, 0.0));
            node.is_null_node = true;
            node.width = NULL_NODE_SIZE;
            node.height = NULL_NODE_SIZE;
            if model.push_species(node).is_ok() {
                let p = Participant::new(null_id.as_str(), 1.0);
                if prefix == "_source_" {
                    r.reactants.push(p);
                } else {
                    r.products.push(p);
                }
            }
        }
        model.push_reaction(r).expect("checked unique above");
    }
}

/// The species, reaction, parameter or compartment ids that a model's text
/// uses, for tests.
#[cfg(test)]
fn ids(m: &Model) -> Vec<String> {
    m.species().iter().map(|s| s.id.clone()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const ON: ImportOptions = ImportOptions {
        convert_catalytic: true,
        seed: 1,
    };
    const OFF: ImportOptions = ImportOptions {
        convert_catalytic: false,
        seed: 1,
    };

    fn ok(src: &str, opts: &ImportOptions) -> Model {
        match import(src, opts) {
            Ok(i) => i.model,
            Err(e) => panic!("{src}: {e:?}"),
        }
    }

    fn reaction<'a>(m: &'a Model, id: &str) -> &'a crate::model::Reaction {
        m.find_reaction(id).unwrap()
    }

    fn sides(m: &Model, id: &str) -> (Side, Side, Vec<String>) {
        let r = reaction(m, id);
        let f = |l: &[Participant]| {
            l.iter()
                .map(|p| (p.species.clone(), p.stoichiometry))
                .collect()
        };
        (
            f(&r.reactants),
            f(&r.products),
            r.modifiers.iter().map(|p| p.species.clone()).collect(),
        )
    }

    #[test]
    fn a_simple_model() {
        let m = ok(
            "model demo\n  S1 -> S2; k1*S1\n  k1 = 0.1\n  S1 = 10\nend",
            &ON,
        );
        assert_eq!(m.model_name, "demo");
        assert_eq!(ids(&m), ["S1", "S2"]);
        assert_eq!(m.find_species("S1").unwrap().initial_value, 10.0);
        assert_eq!(
            m.find_species("S2").unwrap().initial_value,
            0.0,
            "undeclared: 0"
        );
        assert_eq!(m.find_parameter("k1").unwrap().expression, "0.1");
        let r = reaction(&m, "R1");
        assert_eq!(r.kinetic_law, "k1*S1");
        assert_eq!(r.mode, ReactionMode::Straight);
        assert_eq!(m.species().iter().filter(|s| s.width == 80.0).count(), 2);
        assert!(
            m.species()
                .iter()
                .all(|s| (0.0..600.0).contains(&s.center.x) && (0.0..600.0).contains(&s.center.y))
        );
        let j = r.junction;
        let mid =
            (m.find_species("S1").unwrap().center + m.find_species("S2").unwrap().center) / 2.0;
        assert!(j.distance(mid) < 1e-9);
    }

    #[test]
    fn without_the_wrapper_the_name_is_anonymous() {
        assert_eq!(ok("A -> B", &ON).model_name, "anonymous");
    }

    #[test]
    fn catalytic_conversion_on_and_off() {
        // M8 acceptance: A -> A + B; k*A.
        let m = ok("R1: A -> A + B; k*A", &ON);
        let (r, p, mods) = sides(&m, "R1");
        assert_eq!(r, [("_source_R1".to_owned(), 1.0)]);
        assert_eq!(p, [("B".to_owned(), 1.0)]);
        assert_eq!(mods, ["A"]);
        assert!(m.find_species("_source_R1").unwrap().is_null_node);

        // Without a rate law, A is still the catalyst: a modifier.
        let m = ok("R1: A -> A + B", &ON);
        assert_eq!(sides(&m, "R1").2, ["A"]);

        let m = ok("R1: A -> A + B; k*A", &OFF);
        let (r, p, mods) = sides(&m, "R1");
        assert_eq!(r, [("A".to_owned(), 1.0)]);
        assert_eq!(p, [("A".to_owned(), 1.0), ("B".to_owned(), 1.0)]);
        assert!(mods.is_empty());
    }

    #[test]
    fn conversion_needs_equal_stoichiometry() {
        let m = ok("R1: A -> 2 A + B; k*A", &ON);
        let (r, _, _) = sides(&m, "R1");
        assert_eq!(r, [("A".to_owned(), 1.0)]);
        // `S4 + S4 -> S2 + S2` keeps everything; `S3 + S4 -> S4` loses S4.
        let m = ok("R1: S4 + S4 -> S2 + S2; k\nR2: S3 + S4 -> S4; k*S3*S4", &ON);
        assert_eq!(reaction(&m, "R1").reactants.len(), 2);
        let (r, p, mods) = sides(&m, "R2");
        assert_eq!(r, [("S3".to_owned(), 1.0)]);
        assert_eq!(p, [("_sink_R2".to_owned(), 1.0)]);
        assert_eq!(mods, ["S4"]);
    }

    #[test]
    fn null_nodes_for_empty_sides() {
        // M8 acceptance: `-> A` and `A ->`.
        let m = ok("R1: -> A; k\nR2: A -> ; k*A", &ON);
        let src = m.find_species("_source_R1").unwrap();
        assert!(src.is_null_node && src.width == 28.0 && src.height == 28.0);
        assert!(m.find_species("_sink_R2").unwrap().is_null_node);
        assert_eq!(reaction(&m, "R1").reactants[0].species, "_source_R1");
        assert_eq!(reaction(&m, "R2").products[0].species, "_sink_R2");
    }

    #[test]
    fn names_in_another_case_resolve_to_the_declared_spelling() {
        let m = ok("species Glucose\nR1: glucose -> G6P; k*Glucose", &ON);
        assert_eq!(ids(&m), ["Glucose", "G6P"]);
        assert_eq!(reaction(&m, "R1").reactants[0].species, "Glucose");
    }

    #[test]
    fn implied_parameters_and_modifiers() {
        let m = ok("R1: S -> P; Vm*S/(Km + S)*E\nspecies E\nKm = 0.5", &ON);
        assert_eq!(m.find_parameter("Vm").unwrap().expression, "0");
        assert_eq!(m.find_parameter("Km").unwrap().expression, "0.5");
        assert!(m.find_parameter("E").is_none(), "E is a species");
        assert_eq!(sides(&m, "R1").2, ["E"]);
        assert_eq!(reaction(&m, "R1").kinetic_law, "Vm*S/(Km + S)*E");
    }

    #[test]
    fn assignments_to_species_compartments_and_parameters() {
        let m = ok(
            "compartment cell\nspecies A in cell\ncell = 2\nA = 1.5\nk = 2*3\nT := A + 1\n$B = 4",
            &ON,
        );
        assert_eq!(m.find_compartment("cell").unwrap().size, 2.0);
        let a = m.find_species("A").unwrap();
        assert_eq!((a.initial_value, a.compartment.as_str()), (1.5, "cell"));
        assert_eq!(m.find_parameter("k").unwrap().expression, "2*3");
        assert_eq!(m.assignment_rules[0].expression, "A + 1");
        let b = m.find_species("B").unwrap();
        assert!(b.is_boundary && b.initial_value == 4.0);
    }

    #[test]
    fn reaction_ids_declared_or_generated() {
        let m = ok("A -> B; k\nR1: B -> C; k\nC -> D; k", &ON);
        let ids: Vec<&str> = m.reactions().iter().map(|r| r.id.as_str()).collect();
        assert_eq!(ids, ["R2", "R1", "R3"]);
    }

    #[test]
    fn errors_leave_no_model() {
        // M8 acceptance: a syntax error is reported with where it is.
        let e = import("A -> B; k\nC + -> D", &ON).unwrap_err();
        assert_eq!((e[0].line, e[0].column), (2, 5));
        // A bad rate law points into the law.
        let e = import("R1: A -> B; k*(A +\n", &ON).unwrap_err();
        assert_eq!(e[0].line, 1);
        assert!(e[0].column > 12);
        // A species' value must be a number.
        let e = import("species A\nA = k*2", &ON).unwrap_err();
        assert!(e[0].message.contains("must be a number"));
        assert_eq!((e[0].line, e[0].column), (2, 5));
        // Two reactions with one id.
        let e = import("R1: A -> B\nR1: B -> C", &ON).unwrap_err();
        assert_eq!(e[0].line, 2);
    }

    #[test]
    fn warnings_dont_stop_the_import() {
        let i = import("unit mM = 1e-3\nA -> B; k", &ON).unwrap();
        assert_eq!(i.warnings.len(), 1);
        assert_eq!(i.model.reactions().len(), 1);
    }

    #[test]
    fn export_follows_s_11_3() {
        let mut m = ok(
            "model demo\ncompartment cell = 2\nspecies $A in cell = 1.5\nR1: 2 A -> B; k1*A\nR2: B -> ; \nk1 = 0.1\nT := A + B",
            &ON,
        );
        m.add_alias("A", pt(0.0, 0.0));
        let text = export(&m);
        let expected = "\
model demo
  // Compartments
  compartment cell = 2;

  // Species
  species $A in cell;
  species B;

  // Reactions
  R1: 2 $A -> B; k1*A;
  R2: B -> ; v;

  // Parameters
  k1 = 0.1;

  // Initial values
  A = 1.5;
  B = 0;

  // Assignment rules
  T := A + B;
end
";
        assert_eq!(text, expected);
    }

    /// The same biochemistry: species with their values and boundary flags,
    /// reactions with participants and laws, parameters.
    fn biochemistry(m: &Model) -> String {
        let mut out = String::new();
        for s in m
            .species()
            .iter()
            .filter(|s| !s.is_null_node && !s.is_alias())
        {
            out += &format!("{} {} {}|", s.id, s.initial_value, s.is_boundary);
        }
        for r in m.reactions() {
            let side = |l: &[Participant]| {
                l.iter()
                    .filter(|p| !m.find_species(&p.species).unwrap().is_null_node)
                    .map(|p| format!("{}*{}", p.stoichiometry, p.species))
                    .collect::<Vec<_>>()
                    .join("+")
            };
            out += &format!(
                "{}: {} -> {}; {}|",
                r.id,
                side(&r.reactants),
                side(&r.products),
                r.kinetic_law
            );
        }
        let mut params: Vec<_> = m
            .parameters
            .iter()
            .map(|p| format!("{}={}", p.variable, p.expression))
            .collect();
        params.sort();
        out + &params.join(",")
    }

    #[test]
    fn every_reference_file_imports_and_round_trips() {
        // M8 acceptance.
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Models");
        let mut files: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p.extension().is_some_and(|x| x == "ant"))
            .collect();
        files.sort();
        assert_eq!(files.len(), 9);
        for path in files {
            let src = std::fs::read_to_string(&path).unwrap();
            for opts in [ON, OFF] {
                let first = import(&src, &opts)
                    .unwrap_or_else(|e| panic!("{}: {e:?}", path.display()))
                    .model;
                assert!(!first.reactions().is_empty(), "{}", path.display());
                let again = import(&export(&first), &opts)
                    .unwrap_or_else(|e| panic!("{} again: {e:?}", path.display()))
                    .model;
                assert_eq!(
                    biochemistry(&first),
                    biochemistry(&again),
                    "{}",
                    path.display()
                );
            }
        }
    }

    #[test]
    fn placement_is_seeded() {
        let a = ok("A -> B", &ON);
        let b = ok("A -> B", &ON);
        assert_eq!(a, b);
        let c = ok("A -> B", &ImportOptions { seed: 2, ..ON });
        assert_ne!(a.species()[0].center, c.species()[0].center);
    }
}
