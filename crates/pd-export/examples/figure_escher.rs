//! The paper's Escher figure (use case 1), drawn by the app's own code: the
//! Escher map of E. coli core metabolism, opened as PathwayDesigner opens it,
//! written as a vector PDF; and, for a close-up beside Escher's own picture,
//! where a region of the map falls on that PDF's page, as a LaTeX
//! `\includegraphics` viewport.
//!
//! ```text
//! cargo run -p pd-export --release --example figure_escher -- <out>
//! ```

use std::path::Path;

use pd_core::render::draw::Shown;

/// The close-up: round citrate synthase (CS), from oxaloacetate to citrate,
/// in the map's own coordinates.
const CLOSE_UP: (f64, f64, f64, f64) = (2520.0, 3600.0, 3180.0, 4130.0);

fn main() {
    let out = std::env::args().nth(1).expect("an output folder");
    let out = Path::new(&out);
    std::fs::create_dir_all(out).unwrap();
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Escher/e_coli_core.Core metabolism.json");
    let mut m = pd_core::escher::import(&std::fs::read_to_string(path).unwrap())
        .unwrap()
        .model;
    // As the app opens it (S §7.9).
    pd_core::compartments::place_unplaced(&mut m);
    m.background = pd_core::model::Rgba::rgb(0xFF, 0xFF, 0xFF);
    let pdf = pd_core::render::pdf::model_to_pdf(
        &m,
        pd_core::render::pdf::PageSize::Drawing,
        false,
        Shown::ALL,
    );
    std::fs::write(out.join("escher_map_pd.pdf"), pdf).unwrap();

    // A drawing-sized page puts world (x, y) at ((x - min.x), (y - min.y))
    // in units of 0.75 pt from the top left; a viewport counts from the
    // bottom left.
    let (min, max) = pd_core::render::draw::content_bounds(&m);
    let k = 72.0 / 96.0;
    let page_h = (max.y - min.y).ceil() * k;
    let (x0, y0, x1, y1) = CLOSE_UP;
    let viewport = format!(
        "{:.2} {:.2} {:.2} {:.2}",
        (x0 - min.x) * k,
        page_h - (y1 - min.y) * k,
        (x1 - min.x) * k,
        page_h - (y0 - min.y) * k
    );
    // A whole command: graphicx does not expand a macro given as `viewport`.
    std::fs::write(
        out.join("close_up_viewport.tex"),
        format!(
            "\\newcommand{{\\closeup}}[1]{{\\includegraphics[width=#1,viewport={viewport},clip]{{fig_escher_panels/escher_map_pd.pdf}}}}\n"
        ),
    )
    .unwrap();
    println!("close-up viewport (bp): {viewport}");
}
