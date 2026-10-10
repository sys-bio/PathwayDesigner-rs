//! The Antimony writer (S §11.3).
//!
//! One statement per line, indented by two spaces, in sections introduced by
//! `// …` comments, wrapped in `model <name>` … `end`:
//! compartments, species, reactions, parameters (the assignments), initial
//! values (species' `initial`), then assignment rules. The caller decides
//! what goes in: the writer prints what it is given.

use std::fmt::Write as _;

use crate::description::{Description, Term};

/// The model as Antimony text. Reactions without a rate are written with the
/// rate `v`; species with `boundary` get a `$`; the arrow is always `->`.
pub fn write(d: &Description) -> String {
    let name = d
        .name
        .as_deref()
        .filter(|n| is_identifier(n))
        .unwrap_or("anonymous");
    let mut sections: Vec<(&str, Vec<String>)> = Vec::new();

    sections.push((
        "Compartments",
        d.compartments
            .iter()
            .map(|c| match &c.size {
                Some(s) => format!("compartment {} = {};", c.id, s.text),
                None => format!("compartment {};", c.id),
            })
            .collect(),
    ));
    sections.push((
        "Species",
        d.species
            .iter()
            .map(|s| {
                let mut line = format!("species {}{}", if s.boundary { "$" } else { "" }, s.id);
                if let Some(c) = &s.compartment {
                    let _ = write!(line, " in {c}");
                }
                line.push(';');
                line
            })
            .collect(),
    ));
    sections.push((
        "Reactions",
        d.reactions
            .iter()
            .map(|r| {
                let rate = r.rate.as_ref().map_or("v", |t| t.text.as_str());
                let id =
                    r.id.as_deref()
                        .map(|i| format!("{i}: "))
                        .unwrap_or_default();
                format!(
                    "{id}{} -> {}; {rate};",
                    side(&r.reactants),
                    side(&r.products)
                )
            })
            .collect(),
    ));
    sections.push((
        "Parameters",
        d.assignments
            .iter()
            .map(|a| format!("{} = {};", a.variable, a.expression.text))
            .collect(),
    ));
    sections.push((
        "Initial values",
        d.species
            .iter()
            .filter_map(|s| {
                s.initial
                    .as_ref()
                    .map(|v| format!("{} = {};", s.id, v.text))
            })
            .collect(),
    ));
    sections.push((
        "Assignment rules",
        d.rules
            .iter()
            .map(|a| format!("{} := {};", a.variable, a.expression.text))
            .collect(),
    ));

    let mut out = format!("model {name}\n");
    let mut first = true;
    for (title, lines) in sections {
        if lines.is_empty() {
            continue;
        }
        if !first {
            out.push('\n');
        }
        first = false;
        let _ = writeln!(out, "  // {title}");
        for line in lines {
            let _ = writeln!(out, "  {line}");
        }
    }
    out.push_str("end\n");
    out
}

/// `[n ]A + [n ]B`: a stoichiometry other than 1 goes in front of the name.
fn side(terms: &[Term]) -> String {
    terms
        .iter()
        .map(|t| {
            let n = if t.stoichiometry == 1.0 {
                String::new()
            } else {
                format!("{} ", t.stoichiometry)
            };
            format!("{n}{}{}", if t.boundary { "$" } else { "" }, t.species)
        })
        .collect::<Vec<_>>()
        .join(" + ")
}

fn is_identifier(s: &str) -> bool {
    let mut b = s.bytes();
    b.next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == b'_')
        && b.all(|c| c.is_ascii_alphanumeric() || c == b'_')
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::description::{Assignment, Compartment, Reaction, Species, Text};
    use crate::parse;

    fn sample() -> Description {
        Description {
            name: Some("demo".into()),
            compartments: vec![Compartment {
                id: "cell".into(),
                size: Some(Text::new("2")),
            }],
            species: vec![
                Species {
                    id: "A".into(),
                    boundary: true,
                    compartment: Some("cell".into()),
                    initial: Some(Text::new("1.5")),
                },
                Species {
                    id: "B".into(),
                    boundary: false,
                    compartment: None,
                    initial: Some(Text::new("0")),
                },
            ],
            reactions: vec![
                Reaction {
                    id: Some("R1".into()),
                    reactants: vec![Term {
                        stoichiometry: 2.0,
                        species: "A".into(),
                        boundary: true,
                    }],
                    products: vec![Term {
                        stoichiometry: 1.0,
                        species: "B".into(),
                        boundary: false,
                    }],
                    rate: Some(Text::new("k1*A")),
                    line: 0,
                },
                Reaction {
                    id: Some("R2".into()),
                    reactants: vec![],
                    products: vec![Term {
                        stoichiometry: 0.5,
                        species: "B".into(),
                        boundary: false,
                    }],
                    rate: None,
                    line: 0,
                },
            ],
            assignments: vec![Assignment {
                variable: "k1".into(),
                expression: Text::new("0.1"),
                line: 0,
            }],
            rules: vec![Assignment {
                variable: "T".into(),
                expression: Text::new("A + B"),
                line: 0,
            }],
        }
    }

    #[test]
    fn layout_of_the_text() {
        let expected = "\
model demo
  // Compartments
  compartment cell = 2;

  // Species
  species $A in cell;
  species B;

  // Reactions
  R1: 2 $A -> B; k1*A;
  R2:  -> 0.5 B; v;

  // Parameters
  k1 = 0.1;

  // Initial values
  A = 1.5;
  B = 0;

  // Assignment rules
  T := A + B;
end
";
        assert_eq!(write(&sample()), expected);
    }

    #[test]
    fn empty_sections_are_left_out_and_bad_names_become_anonymous() {
        let d = Description {
            name: Some("not a name".into()),
            ..Description::default()
        };
        assert_eq!(write(&d), "model anonymous\nend\n");
    }

    #[test]
    fn written_text_reads_back_the_same() {
        let d = sample();
        let back = parse(&write(&d));
        assert!(!back.has_errors(), "{:?}", back.diagnostics);
        let b = back.description;
        assert_eq!(b.name, d.name);
        assert_eq!(b.compartments.len(), 1);
        assert_eq!(b.reactions.len(), 2);
        assert_eq!(b.reactions[0].reactants, d.reactions[0].reactants);
        assert_eq!(b.reactions[1].products, d.reactions[1].products);
        assert_eq!(b.reactions[0].rate.as_ref().unwrap().text, "k1*A");
        assert_eq!(b.rules[0].expression.text, "A + B");
        // Initial values come back as assignments, after the declarations.
        assert_eq!(
            b.assignments
                .iter()
                .map(|a| a.variable.as_str())
                .collect::<Vec<_>>(),
            ["k1", "A", "B"]
        );
    }
}
