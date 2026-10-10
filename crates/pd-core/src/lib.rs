//! PathwayDesigner core.
//!
//! Everything that doesn't need a window or a browser: geometry, the model, file
//! formats, layout and the drawing passes. It builds for native targets and for
//! `wasm32-unknown-unknown`, and its tests run with a plain `cargo test`
//! (SPEC-Rust §2).

pub mod antimony;
pub mod bezier;
pub mod expr;
pub mod generate;
pub mod geometry;
pub mod json;
pub mod layout;
pub mod model;
pub mod render;
pub mod text;
pub mod undo;
pub mod view;
