//! The paper's Reactome figure: Reactome's glucose metabolism pathway
//! (R-HSA-70326), the SBML Reactome exports, opened as PathwayDesigner opens
//! it (its own layout, compartments and short labels, display names shown),
//! written as a vector PDF and a PNG; `Paper/figures/fig_reactome.tex` puts
//! the PDF on the page.
//!
//! ```text
//! cargo run -p pd-export --release --example figure_reactome -- <out>
//! ```

use std::path::Path;

use pd_core::render::draw::Shown;

const DPI: f64 = 300.0;

fn main() {
    let out = std::env::args().nth(1).expect("an output folder");
    let out = Path::new(&out);
    std::fs::create_dir_all(out).unwrap();
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ExampleModels/Reactome_Glycolysis.xml");
    let mut m = pd_core::read_model(&std::fs::read_to_string(path).unwrap()).unwrap();
    // As the app opens it (S §7.9), on white for print.
    pd_core::compartments::place_unplaced(&mut m);
    m.refresh_labels();
    m.background = pd_core::model::Rgba::rgb(0xFF, 0xFF, 0xFF);
    let pdf = pd_core::render::pdf::model_to_pdf(
        &m,
        pd_core::render::pdf::PageSize::Drawing,
        false,
        Shown::ALL,
    );
    std::fs::write(out.join("reactome_pd.pdf"), pdf).unwrap();
    let png = pd_export::model_to_png(&m, DPI, Shown::ALL).unwrap();
    std::fs::write(out.join("reactome_pd.png"), png).unwrap();
    println!(
        "{} species ({} drawings shown by label text), {} reactions, display names {}",
        m.species().iter().filter(|s| !s.is_alias()).count(),
        m.species()
            .iter()
            .filter(|s| s.label_text.is_some())
            .count(),
        m.reactions().len(),
        m.show_display_names
    );
}
