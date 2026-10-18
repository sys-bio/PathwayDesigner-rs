//! PathwayDesigner core.
//!
//! Everything that doesn't need a window or a browser: geometry, the model, file
//! formats, layout and the drawing passes. It builds for native targets and for
//! `wasm32-unknown-unknown`, and its tests run with a plain `cargo test`
//! (SPEC-Rust §2).

pub mod antimony;
pub mod bezier;
pub mod compartments;
pub mod expr;
pub mod generate;
pub mod geometry;
pub mod json;
pub mod layout;
pub mod model;
pub mod node_shape;
pub mod passes;
pub mod render;
pub mod sbml;
pub mod styles;
pub mod text;
pub mod textbox;
pub mod themes;
pub mod undo;
pub mod view;
