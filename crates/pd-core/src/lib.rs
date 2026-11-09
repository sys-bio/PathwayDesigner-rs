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
pub mod antimony_sbml;
pub mod bezier;
pub mod chebi;
pub mod clipboard;
pub mod compartments;
pub mod diagram_format;
pub mod drafts;
pub mod entry_names;
pub mod escher;
pub mod expr;
pub mod generate;
pub mod genes;
pub mod geometry;
pub mod json;
pub mod kegg;
pub mod layout;
pub mod model;
pub mod model_core;
pub mod node_shape;
pub mod passes;
pub mod pubchem;
pub mod reaction_lookup;
pub mod render;
pub mod sbml;
pub mod sentences;
pub mod structure;
pub mod styles;
pub mod text;
pub mod textbox;
pub mod themes;
pub mod undo;
pub mod view;

/// A model from a file's text, by what it holds: an Escher map, SBML, or
/// PathwayDesigner's JSON. For the Gallery's checks and the tools that go
/// through every Gallery model; the app decides by the file's name too.
pub fn read_model(text: &str) -> Result<model::Model, String> {
    if escher::looks_like_escher(text) {
        return escher::import(text).map(|i| i.model);
    }
    let t = text.trim_start_matches('\u{FEFF}').trim_start();
    if t.starts_with('<') {
        let opts = antimony::ImportOptions { seed: 1 };
        return sbml::import(text, &opts).map(|i| i.model).map_err(|e| {
            e.iter()
                .map(|d| d.to_string())
                .collect::<Vec<_>>()
                .join("; ")
        });
    }
    json::from_json(text)
        .map(|l| l.model)
        .map_err(|e| e.to_string())
}
