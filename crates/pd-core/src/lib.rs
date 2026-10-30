//! PathwayDesigner core.
//!
//! Everything that doesn't need a window or a browser: geometry, the model, file
//! formats, layout and the drawing passes. It builds for native targets and for
//! `wasm32-unknown-unknown`, and its tests run with a plain `cargo test`
//! (SPEC-Rust §2).

pub mod annotation;
pub mod antimony;
pub(crate) mod antimony_colors;
pub mod antimony_layout;
pub(crate) mod antimony_layout_out;
pub(crate) mod antimony_looks;
pub mod bezier;
pub mod chebi;
pub mod compartments;
pub mod diagram_format;
pub mod escher;
pub mod expr;
pub mod generate;
pub mod geometry;
pub mod json;
pub mod kegg;
pub mod layout;
pub mod model;
pub mod node_shape;
pub mod passes;
pub mod reaction_lookup;
pub mod pubchem;
pub mod render;
pub mod sbml;
pub mod structure;
pub mod styles;
pub mod text;
pub mod textbox;
pub mod themes;
pub mod undo;
pub mod view;
