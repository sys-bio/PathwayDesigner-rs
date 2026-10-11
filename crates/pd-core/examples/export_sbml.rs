//! Writes every `Models/*.json` as SBML into a folder, for checking with
//! python-libsbml (`tests/scripts/check_sbml.py`, SPEC-Rust §8.3).
//!
//! ```text
//! cargo run -p pd-core --example export_sbml -- <out>
//! ```

use std::path::Path;

fn main() {
    let out = std::env::args().nth(1).expect("an output folder");
    std::fs::create_dir_all(&out).unwrap();
    let models = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Models");
    let mut failed = false;
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
        let exported = pd_core::sbml::export(&model);
        let name = p.file_stem().unwrap().to_string_lossy();
        for problem in &exported.problems {
            println!("{name}: {problem}");
            failed = true;
        }
        std::fs::write(Path::new(&out).join(format!("{name}.xml")), exported.xml).unwrap();
    }
    if failed {
        std::process::exit(1);
    }
}
