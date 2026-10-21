//! The paper's MAPK figure (use case 3), drawn by the app's own code: the
//! Gallery's MAPK cascade with negative feedback, its inhibitor routed back
//! round the diagram through corner points, written as a vector PDF for a
//! full-width figure; and the same model exported as SBML and Antimony, which
//! `Paper/figures/fig_mapk_timecourse.py` simulates for the second panel.
//!
//! For print at full width the names are enlarged (the boxes widened to fit
//! them, never narrowed), and the title and subtitle, which the caption
//! carries, are left out. The model is otherwise as in the Gallery.
//!
//! Also `mapk_as_exported.pdf`: the model drawn exactly as it is exported,
//! for the figure comparing it with COPASI's drawing of `mapk.xml`.
//!
//! ```text
//! cargo run -p pd-export --release --example figure_mapk -- <out>
//! ```

use std::path::Path;

use pd_core::render::draw::Shown;

/// Species names, the row labels and the reaction names, in world units: at
/// full width (about 61 %), roughly 8 pt.
const NAME: f64 = 18.0;
const REACTION_NAME: f64 = 15.0;

fn main() {
    let out = std::env::args().nth(1).expect("an output folder");
    let out = Path::new(&out);
    std::fs::create_dir_all(out).unwrap();
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ExampleModels/mapk_cascade.json");
    let model = pd_core::json::from_json(&std::fs::read_to_string(path).unwrap())
        .unwrap()
        .model;

    // The model files the time course is simulated from: exactly what
    // PathwayDesigner exports from the Gallery model.
    std::fs::write(out.join("mapk.xml"), pd_core::sbml::export(&model).xml).unwrap();
    std::fs::write(out.join("mapk.ant"), pd_core::antimony::export(&model)).unwrap();

    // The model drawn exactly as it is exported, titles and background
    // included: what COPASI draws from mapk.xml is compared with it.
    let pdf = pd_core::render::pdf::model_to_pdf(
        &model,
        pd_core::render::pdf::PageSize::Drawing,
        false,
        Shown::ALL,
    );
    std::fs::write(out.join("mapk_as_exported.pdf"), pdf).unwrap();

    // The drawing, for print.
    let mut m = model.clone();
    m.background = pd_core::model::Rgba::rgb(0xFF, 0xFF, 0xFF);
    let titles: Vec<String> = m
        .texts()
        .iter()
        .filter(|t| t.size >= 20.0 || t.text.starts_with("After "))
        .map(|t| t.id.clone())
        .collect();
    for id in titles {
        m.delete_text(&id);
    }
    for t in m.texts_mut() {
        t.size = NAME;
    }
    for s in m.species_mut() {
        s.style.has_custom_style = true;
        s.style.font_size = NAME;
        s.height = s.height.max(44.0);
        s.width = s.width.max(pd_core::render::draw::width_for_name(s).ceil());
    }
    for r in m.reactions_mut() {
        r.style.has_custom_style = true;
        r.style.font_size = REACTION_NAME;
    }
    let pdf = pd_core::render::pdf::model_to_pdf(
        &m,
        pd_core::render::pdf::PageSize::Drawing,
        false,
        Shown::ALL,
    );
    std::fs::write(out.join("mapk_diagram.pdf"), pdf).unwrap();
    let png = pd_export::model_to_png(&m, 300.0, Shown::ALL).unwrap();
    std::fs::write(out.join("mapk_diagram.png"), png).unwrap();
}
