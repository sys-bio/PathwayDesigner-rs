//! Read and write [Antimony](https://tellurium.readthedocs.io/en/latest/antimony.html),
//! the human-readable language for biochemical models, and convert it to SBML.
//! Pure Rust with no dependencies, so it also works in WebAssembly.
//!
//! **Status: under construction.** The lexer, diagnostics and expressions
//! are built (milestone A2); the statement parser, writers and API arrive
//! milestone by milestone (see `PLAN.md`). The design is recorded in
//! `SPEC.md`.
//!
//! ```
//! use antimony::lexer::{tokenize, TokenKind};
//!
//! let (tokens, diagnostics) = antimony::lexer::tokenize("J1: S1 -> S2; k1*S1");
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
pub mod lexer;
pub mod messages;
pub mod model;
pub mod number;
pub mod span;

pub use diagnostics::{Code, Diagnostic, Diagnostics, Severity};
pub use span::{Pos, Span};

/// This crate's version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
