//! Curated BioModels through PathwayDesigner and back: each model's SBML is
//! read as File ▸ Import SBML reads it and written as Save to SBML writes it.
//! `crates/pd-core/tests/scripts/biomodels_check.py` then compares each pair
//! with libSBML and libRoadRunner and writes the report.
//!
//! ```text
//! cargo run -p pd-core --release --example biomodels_roundtrip -- <BioModels> <out>
//! ```
//! `<BioModels>`: a folder of `<id>/<id>_url.xml` (as BioModels' curated
//! archive unpacks), or of `<id>.xml`. Writes `<out>/<id>.xml` for each model
//! read, and `<out>/roundtrip.jsonl`, a line per model: whether it was read,
//! what the import warned about, and what the export found wrong or added.

use std::io::Write;
use std::path::{Path, PathBuf};

use serde_json::json;

/// The SBML file of each model in `dir`, by id, in order.
fn models(dir: &Path) -> Vec<(String, PathBuf)> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir).expect("a readable folder").flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if path.is_dir() {
            // `<id>_url.xml`, else the folder's one SBML file, whatever it
            // is called (`<id>.xml`, `Philipson2015.xml`).
            let url = path.join(format!("{name}_url.xml"));
            let file = if url.exists() {
                Some(url)
            } else {
                std::fs::read_dir(&path)
                    .into_iter()
                    .flatten()
                    .flatten()
                    .map(|e| e.path())
                    .filter(|p| {
                        let f = p.file_name().unwrap_or_default().to_string_lossy();
                        f.ends_with(".xml") && f != "manifest.xml" && !f.ends_with("_urn.xml")
                    })
                    .find(|p| {
                        std::fs::read(p)
                            .map(|b| {
                                String::from_utf8_lossy(&b[..b.len().min(2000)]).contains("<sbml")
                            })
                            .unwrap_or(false)
                    })
            };
            if let Some(file) = file {
                out.push((name, file));
            }
        } else if let Some(id) = name.strip_suffix(".xml") {
            out.push((id.to_owned(), path));
        }
    }
    out.sort();
    out
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (src, out) = (Path::new(&args[1]), Path::new(&args[2]));
    std::fs::create_dir_all(out).expect("the output folder");
    let mut log = std::fs::File::create(out.join("roundtrip.jsonl")).expect("the log");
    let opts = pd_core::antimony::ImportOptions {
        convert_catalytic: false,
        seed: 1,
    };
    let all = models(src);
    let (mut read, mut failed) = (0, 0);
    for (id, file) in &all {
        let text = match std::fs::read(file) {
            Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
            Err(e) => {
                failed += 1;
                writeln!(
                    log,
                    "{}",
                    json!({"id": id, "read": false, "errors": [e.to_string()]})
                )
                .unwrap();
                continue;
            }
        };
        let line = match pd_core::sbml::import(&text, &opts) {
            Ok(imported) => {
                read += 1;
                let exported = pd_core::sbml::export(&imported.model);
                std::fs::write(out.join(format!("{id}.xml")), &exported.xml).expect("written");
                json!({
                    "id": id,
                    "read": true,
                    "warnings": imported.warnings,
                    "problems": exported.problems,
                    "notes": exported.notes,
                })
            }
            Err(diagnostics) => {
                failed += 1;
                let errors: Vec<String> = diagnostics.iter().map(|d| d.to_string()).collect();
                json!({"id": id, "read": false, "errors": errors})
            }
        };
        writeln!(log, "{line}").unwrap();
    }
    println!(
        "{} models: {read} read and written, {failed} not read",
        all.len()
    );
}
