//! PathwayDesigner graphics export (SPEC-Rust §9).
//!
//! Turns the SVG written by `pd-core`'s `SvgRenderer` into PNG (through resvg)
//! and vector PDF (through svg2pdf). Pure Rust, no egui, builds for wasm32.
//!
//! Empty until milestone R10. The crate exists from R0 so the crate
//! boundaries and the CI checks are in place from the start.
