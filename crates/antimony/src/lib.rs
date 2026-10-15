//! Read and write [Antimony](https://tellurium.readthedocs.io/en/latest/antimony.html),
//! the human-readable language for biochemical models, and convert it to SBML.
//! Pure Rust with no dependencies, so it also works in WebAssembly.
//!
//! **Status: under construction.** Reading and writing Antimony, and SBML
//! output with the `sbml` feature (`antimony::sbml`), are built (milestone
//! A7); the rest of the API arrives milestone by milestone (see `PLAN.md`).
//! The design is recorded in `SPEC.md`.
//!
//! ```
//! let loaded = antimony::parse("model *m()\n  J1: S1 -> S2; k1*S1\n  S1 = 10; k1 = 0.1\nend");
//! assert!(!loaded.has_errors());
//! let model = loaded.document.main_module();
//! assert_eq!(model.reactions[0].reactants[0].species, "S1");
//! assert!(model.reactions[0].is_reversible());
//! assert_eq!(model.symbol("S2").unwrap().kind, antimony::model::SymbolKind::Species);
//!
//! // Written back, it reads back as the same model.
//! let text = antimony::write_document(&loaded.document);
//! assert!(text.contains("J1: S1 -> S2; k1 * S1;"));
//!
//! // A typo is a warning, not a failure: the model still loads.
//! let loaded = antimony::parse("J1: A -> B; k1*typoo\nA = 1; B = 0; k1 = 2");
//! assert!(!loaded.has_errors());
//! assert!(loaded.diagnostics.has_any(antimony::Code::ImplicitParameter));
//! ```
//!
//! The lower levels are public too:
//!
//! ```
//! use antimony::lexer::{tokenize, TokenKind};
//!
//! let (tokens, diagnostics) = tokenize("J1: S1 -> S2; k1*S1");
//! assert!(diagnostics.is_empty());
//! assert_eq!(tokens[3].kind, TokenKind::ArrowRev);
//! assert_eq!(tokens[3].span.to_string(), "1:8-10");
//!
//! let (expr, _) = antimony::expr::parse_expression("(k1*S1^2)/(1 + S1)");
//! let expr = expr.unwrap();
//! assert_eq!(expr.to_string(), "k1 * S1 ^ 2 / (1 + S1)");
//! assert_eq!(expr.symbols(), ["k1", "S1"]);
//! assert_eq!(expr.evaluate_with(&|name| if name == "k1" { Some(2.0) } else { Some(3.0) }), Some(4.5));
//! ```

#![forbid(unsafe_code)]

pub mod builtins;
pub mod diagnostics;
pub mod expr;
pub mod flatten;
pub mod lexer;
pub mod messages;
pub mod model;
pub mod number;
pub mod parser;
#[cfg(feature = "sbml")]
pub mod sbml;
pub mod semantics;
pub mod span;
pub mod write;

pub use diagnostics::{Code, Diagnostic, Diagnostics, Severity};
pub use semantics::Options;
pub use span::{Pos, Span};
pub use write::{WriteOptions, write_document, write_module};

use model::Document;

/// This crate's version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// A source read into a model, and what was found on the way.
///
/// A malformed file is not an error to unwrap: the document holds as much
/// as could be read, and the problems are in `diagnostics`. Use
/// [`Loaded::into_result`] to treat any error as failure.
#[derive(Clone, Debug)]
pub struct Loaded {
    pub document: Document,
    /// In the order found: the parser's, then the semantic pass's.
    pub diagnostics: Diagnostics,
}

impl Loaded {
    pub fn has_errors(&self) -> bool {
        self.diagnostics.has_errors()
    }

    /// The document if nothing was an error, else the diagnostics.
    pub fn into_result(self) -> Result<Document, Diagnostics> {
        if self.has_errors() {
            Err(self.diagnostics)
        } else {
            Ok(self.document)
        }
    }
}

/// Reads Antimony: parses it, then resolves and checks it. Never fails and
/// never panics; see [`Loaded`].
pub fn parse(source: &str) -> Loaded {
    parse_with(source, &Options::default())
}

/// As [`parse`], with [`Options`]: strict mode, or filling in missing values.
pub fn parse_with(source: &str, options: &Options) -> Loaded {
    let (mut document, mut diagnostics) = parser::parse_document(source);
    let mut found = semantics::analyse(&mut document, options);
    diagnostics.append(&mut found);
    Loaded {
        document,
        diagnostics,
    }
}
