//! A small Antimony reader and writer (S §11.2, §11.3).
//!
//! This crate knows Antimony and nothing else. [`parse`] turns text into a
//! [`Description`] of the model, with errors and warnings that carry line and
//! column; [`write`] turns a description back into text. Expressions (rate
//! laws, initial values, assignments) are kept as the text that was written,
//! with where it starts, so the caller can parse them however it likes.
//!
//! PathwayDesigner uses it through `pd-core::antimony`, which maps a
//! description onto the diagram. To move to a complete Antimony crate, replace
//! this crate and adapt that bridge to read the new crate's output; nothing
//! else depends on this one.
//!
//! What is understood (S §11.2): `model name … end` (also `model name()`),
//! `compartment`, `species` / `var` with `$`, `in` and `= value`, reactions
//! with an optional `id:`, stoichiometry and `->` or `=>`, `x = expr` and
//! `x := expr`. `//` and `#` comments. Statements end with `;` or a new line.
//! Other Antimony statements (events, functions, units, modules, …) are
//! skipped with a warning.

mod description;
mod parse;
mod write;

pub use description::{
    Assignment, Compartment, Description, Diagnostic, Reaction, Severity, Species, Term, Text,
};
pub use parse::{Parsed, line_column, parse};
pub use write::write;
