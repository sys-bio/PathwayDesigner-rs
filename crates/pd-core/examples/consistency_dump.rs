//! Everything the shared core makes from each Gallery model, written to a
//! folder, so that a native build and a WebAssembly build can be compared
//! byte for byte (the paper's evaluation E9a): the native JSON, Antimony and
//! SBML exports, the SBML and Antimony read back, SVG and PDF drawings, and an
//! automatic layout from a fixed seed.
//!
//! ```text
//! cargo run -p pd-core --release --example consistency_dump -- <repository> <out>
//! cargo build -p pd-core --release --example consistency_dump --target wasm32-wasip1
//! ```
//!
//! The WebAssembly build runs under any WASI runtime, with the repository and
//! the output folder made visible to it.

use std::path::Path;

use pd_core::antimony::ImportOptions;
use pd_core::layout::{LayoutParams, LayoutRun};
use pd_core::render::draw::Shown;

const OPTS: ImportOptions = ImportOptions {
    convert_catalytic: false,
    seed: 1,
};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (root, out) = (Path::new(&args[1]), Path::new(&args[2]));
    std::fs::create_dir_all(out).unwrap();
    let index: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(root.join("Gallery/index.json")).unwrap())
            .unwrap();
    for e in index["examples"].as_array().unwrap() {
        let file = e["file"].as_str().unwrap();
        let text = std::fs::read_to_string(root.join(file)).unwrap();
        let model = if pd_core::escher::looks_like_escher(&text) {
            pd_core::escher::import(&text).unwrap().model
        } else {
            pd_core::json::from_json(&text).unwrap().model
        };
        let name = Path::new(file)
            .file_stem()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let write = |ext: &str, bytes: &[u8]| {
            std::fs::write(out.join(format!("{name}.{ext}")), bytes).unwrap()
        };

        write("json", pd_core::json::to_json(&model).as_bytes());
        let ant = pd_core::antimony::export(&model);
        write("ant", ant.as_bytes());
        let xml = pd_core::sbml::export(&model).xml;
        write("xml", xml.as_bytes());
        let back = pd_core::sbml::import(&xml, &OPTS).unwrap().model;
        write("sbml-back.json", pd_core::json::to_json(&back).as_bytes());
        let back = pd_core::antimony::import(&ant, &OPTS).unwrap().model;
        write("ant-back.json", pd_core::json::to_json(&back).as_bytes());
        write(
            "svg",
            pd_core::render::svg::model_to_svg(&model, 1.0, Shown::ALL).as_bytes(),
        );
        write(
            "pdf",
            &pd_core::render::pdf::model_to_pdf(&model, Default::default(), false, Shown::ALL),
        );
        let mut laid_out = model.clone();
        let params = LayoutParams {
            randomize: true,
            deckard: true,
            ..LayoutParams::default()
        };
        LayoutRun::new(&laid_out, params, 7).finish(&mut laid_out);
        write("layout.json", pd_core::json::to_json(&laid_out).as_bytes());
    }
}
