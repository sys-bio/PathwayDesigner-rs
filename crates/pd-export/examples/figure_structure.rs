//! The paper's structure figure (S §8.9), drawn by the app's own code: the
//! Gallery's MAPK cascade (A) with a conservation law highlighted, the total
//! of MAPK in its three forms, and (B) with a flux null-space vector
//! highlighted, the cycle of MAPK_P's phosphorylation and dephosphorylation
//! (J8, J9). Written as vector PDFs; the names enlarged and the titles left
//! out for print, as in the MAPK figure.
//!
//! ```text
//! cargo run -p pd-export --release --example figure_structure -- <out>
//! ```

use std::path::Path;

use pd_core::render::draw::Shown;
use pd_core::structure::Structure;

const NAME: f64 = 18.0;
const REACTION_NAME: f64 = 15.0;

fn main() {
    let out = std::env::args().nth(1).expect("an output folder");
    let out = Path::new(&out);
    std::fs::create_dir_all(out).unwrap();
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ExampleModels/mapk_cascade.json");
    let mut m = pd_core::json::from_json(&std::fs::read_to_string(path).unwrap())
        .unwrap()
        .model;
    // For print, as in figure_mapk: white, no titles, larger names.
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

    let s = Structure::of(&m);
    let law = s
        .conservation_laws()
        .into_iter()
        .find(|l| l.terms.iter().any(|t| s.species[t.0] == "MAPK"))
        .expect("the MAPK total");
    let cycle = s
        .null_space()
        .into_iter()
        .find(|v| v.terms.iter().any(|t| s.reactions[t.0] == "J9"))
        .expect("the J8/J9 cycle");
    println!("law: {}\nvector: {}", s.law_text(&law), s.flux_text(&cycle));
    for (name, highlight) in [
        ("structure_law.pdf", s.law_highlight(&law)),
        ("structure_vector.pdf", s.flux_highlight(&cycle)),
    ] {
        let pdf = pd_core::render::pdf::model_to_pdf_highlighted(
            &m,
            pd_core::render::pdf::PageSize::Drawing,
            false,
            Shown::ALL,
            Some(&highlight),
        );
        std::fs::write(out.join(name), pdf).unwrap();
    }
}
