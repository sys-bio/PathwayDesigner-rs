//! The plain description of a model that the parser produces and the writer
//! reads. It mirrors what the Antimony text says, in the order it says it;
//! deciding what a name means (species, parameter, compartment) is left to the
//! caller.

/// A piece of the source: the text as written, and the byte offset where it
/// starts (for error positions). Text built for writing has offset 0.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Text {
    pub text: String,
    pub offset: usize,
}

impl Text {
    /// Text that didn't come from a source.
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            offset: 0,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Description {
    /// From `model <name>`; `None` without the wrapper.
    pub name: Option<String>,
    pub compartments: Vec<Compartment>,
    /// Declared with `species`, `var` or `$S = …`, in order.
    pub species: Vec<Species>,
    pub reactions: Vec<Reaction>,
    /// `x = expr`, in order. Whether `x` is a species, a compartment or a
    /// parameter is for the caller to decide.
    pub assignments: Vec<Assignment>,
    /// `x := expr`.
    pub rules: Vec<Assignment>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Compartment {
    pub id: String,
    /// `compartment c = size`.
    pub size: Option<Text>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Species {
    pub id: String,
    /// Written with `$`.
    pub boundary: bool,
    /// `in C`.
    pub compartment: Option<String>,
    /// `species S = value`.
    pub initial: Option<Text>,
}

/// One side's entry in a reaction: `[n[*]][$]S`.
#[derive(Clone, Debug, PartialEq)]
pub struct Term {
    pub stoichiometry: f64,
    pub species: String,
    pub boundary: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Reaction {
    /// `id:` before the reaction, if any.
    pub id: Option<String>,
    pub reactants: Vec<Term>,
    pub products: Vec<Term>,
    /// The rate law after `;`, if any.
    pub rate: Option<Text>,
    /// 1-based line the reaction starts on.
    pub line: usize,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Assignment {
    pub variable: String,
    pub expression: Text,
    pub line: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Severity {
    /// The text can't be used.
    Error,
    /// Something was skipped; the rest is fine.
    Warning,
}

/// A problem in the text, with a 1-based line and column.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    pub severity: Severity,
    pub line: usize,
    pub column: usize,
    pub message: String,
}

impl std::fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "line {}, column {}: {}",
            self.line, self.column, self.message
        )
    }
}
