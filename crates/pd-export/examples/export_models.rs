//! Writes every `Models/*.json` as PDF (drawing size and Letter) and as PNG at
//! 96 DPI into a folder, for checking the PDFs against the PNGs with
//! pypdfium2 (`tests/scripts/compare_pdf.py`, SPEC-Rust §9).
//!
//! ```text
//! cargo run -p pd-export --example export_models -- <out>
//! ```

use std::path::Path;

use pd_core::render::pdf::PageSize;

fn main() {
    let out = std::env::args().nth(1).expect("an output folder");
    std::fs::create_dir_all(&out).unwrap();
    let models = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Models");
    let mut paths: Vec<_> = std::fs::read_dir(models)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    paths.sort();
    for p in paths {
        let model = pd_core::json::from_json(&std::fs::read_to_string(&p).unwrap())
            .unwrap()
            .model;
        let name = p.file_stem().unwrap().to_string_lossy();
        let file = |suffix: &str| Path::new(&out).join(format!("{name}{suffix}"));
        let pdf = pd_export::model_to_pdf(&model, PageSize::Drawing, false, pd_export::Shown::ALL);
        std::fs::write(file(".pdf"), pdf).unwrap();
        let letter =
            pd_export::model_to_pdf(&model, PageSize::Letter, false, pd_export::Shown::ALL);
        std::fs::write(file("-letter.pdf"), letter).unwrap();
        let png = pd_export::model_to_png(&model, 96.0, pd_export::Shown::ALL).unwrap();
        std::fs::write(file(".png"), png).unwrap();
    }
}
