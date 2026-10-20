//! The paper's Millard figure (use case 2), drawn by the app's own code: the
//! author's diagram of the Millard et al. (2017) E. coli model from the
//! Gallery, with modifier lines off as the file saves them, written as a
//! vector PDF (the whole diagram, also usable as supporting information); and,
//! for a close-up at a readable size, where a region of the diagram falls on
//! that PDF's page, as a LaTeX command.
//!
//! ```text
//! cargo run -p pd-export --release --example figure_millard -- <out>
//! ```

use std::path::Path;

use pd_core::render::draw::Shown;

/// The close-up: upper glycolysis and the oxidative pentose phosphate
/// pathway, in the diagram's coordinates (x0, y0, x1, y1).
const CLOSE_UP: (f64, f64, f64, f64) = (720.0, -640.0, 1540.0, -200.0);

fn main() {
    let out = std::env::args().nth(1).expect("an output folder");
    let out = Path::new(&out);
    std::fs::create_dir_all(out).unwrap();
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ExampleModels/E_coli_Millard2016.json");
    let mut m = pd_core::json::from_json(&std::fs::read_to_string(path).unwrap())
        .unwrap()
        .model;
    m.background = pd_core::model::Rgba::rgb(0xFF, 0xFF, 0xFF);
    // As the file shows it: modifier lines off.
    let shown = Shown {
        modifiers: !m.hide_modifier_lines,
        ..Shown::ALL
    };
    let pdf = pd_core::render::pdf::model_to_pdf(
        &m,
        pd_core::render::pdf::PageSize::Drawing,
        false,
        shown,
    );
    std::fs::write(out.join("millard_diagram.pdf"), pdf).unwrap();

    // Where the close-up falls on that page (as in figure_escher).
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
    std::fs::write(
        out.join("close_up.tex"),
        format!(
            "\\newcommand{{\\closeup}}[1]{{\\includegraphics[width=#1,viewport={viewport},clip]{{fig_millard_panels/millard_diagram.pdf}}}}\n"
        ),
    )
    .unwrap();
    println!("close-up viewport (bp): {viewport}");
}
