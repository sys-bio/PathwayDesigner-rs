//! Writes every `Models/*.json` as SBML into a folder, for checking with
//! python-libsbml (`tests/scripts/check_sbml.py`, SPEC-Rust §8.3). With
//! `--gallery`, writes every model in `Gallery/index.json` instead, Escher
//! maps included. With `--antimony`, also writes each as Antimony with its
//! drawing as layout statements (`<name>.ant`), for
//! `scripts/antimony_layout_check.py`.
//!
//! ```text
//! cargo run -p pd-core --example export_sbml -- <out> [--gallery] [--antimony]
//! ```

use std::path::{Path, PathBuf};

fn main() {
    let mut args = std::env::args().skip(1);
    let out = args.next().expect("an output folder");
    let (mut gallery, mut antimony) = (false, false);
    for a in args {
        match a.as_str() {
            "--gallery" => gallery = true,
            "--antimony" => antimony = true,
            other => panic!("unknown option {other}"),
        }
    }
    std::fs::create_dir_all(&out).unwrap();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let paths = if gallery {
        gallery_files(&root)
    } else {
        model_files(&root)
    };
    let mut failed = false;
    let mut names = std::collections::HashSet::new();
    for p in paths {
        let text = std::fs::read_to_string(&p).unwrap();
        let model = if pd_core::escher::looks_like_escher(&text) {
            pd_core::escher::import(&text).unwrap().model
        } else {
            pd_core::json::from_json(&text).unwrap().model
        };
        let exported = pd_core::sbml::export(&model);
        let name = p.file_stem().unwrap().to_string_lossy().into_owned();
        assert!(names.insert(name.clone()), "two models are called {name}");
        for problem in &exported.problems {
            println!("{name}: {problem}");
            failed = true;
        }
        std::fs::write(Path::new(&out).join(format!("{name}.xml")), exported.xml).unwrap();
        if antimony {
            let text = pd_core::antimony::export_with_layout(&model);
            std::fs::write(Path::new(&out).join(format!("{name}.ant")), text).unwrap();
        }
    }
    if failed {
        std::process::exit(1);
    }
}

/// `Models/*.json`, sorted.
fn model_files(root: &Path) -> Vec<PathBuf> {
    let mut paths: Vec<_> = std::fs::read_dir(root.join("Models"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    paths.sort();
    paths
}

/// Every file `Gallery/index.json` lists, in its order.
fn gallery_files(root: &Path) -> Vec<PathBuf> {
    let index: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(root.join("Gallery/index.json")).unwrap())
            .unwrap();
    index["examples"]
        .as_array()
        .expect("an examples list")
        .iter()
        .map(|e| root.join(e["file"].as_str().expect("a file")))
        .collect()
}
