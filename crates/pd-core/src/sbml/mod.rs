//! SBML import and export (S §11.5, §16.2 F-1, SPEC-Rust §8.3).
//!
//! The bridge between `sbml-io`, which reads and writes SBML as plain data,
//! and the diagram [`Model`](crate::model::Model). Everything specific to
//! PathwayDesigner lives here: which parts of the model become which SBML
//! elements, the layout glyphs, MathML to and from `pd-core::expr`, and the
//! passes every import gets. A more complete SBML crate could replace
//! `sbml-io` by changing only this module.

mod appearance;
mod export;
mod import;
pub mod math;
mod render_in;
mod shape;

pub use export::{Exported, export};
pub use import::{Imported, import};
pub use sbml_io::Diagnostic;
