//! How well each layout method draws models that come without a drawing
//! (docs/layout-algorithms-design.md L9). Each test model is taken through
//! Antimony without its drawing, so it arrives as a model read without one
//! does (species at random), then laid out; its own drawing, where it has
//! one, is measured beside it for comparison. Writes `report.md` (measures,
//! `pd_core::layout_measures`) and a picture of each drawing to the output
//! folder.
//!
//! The test set: models from `ExampleModels/`, BiGG's *E. coli* core map
//! (`Escher/`), BioModels without a drawing from `$BIOMODELS` (a folder of
//! SBML files; the first few with 10 to 60 reactions), and the Escher maps
//! named in `$ESCHER_MAPS` (paths separated by `;`), such as a Recon3D
//! subsystem. The force layout runs with three seeds.
//!
//! ```text
//! cargo run -p pd-export --release --example layout_quality -- <out>
//! ```

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use pd_core::antimony::ImportOptions;
use pd_core::layout::{LayoutParams, LayoutRun};
use pd_core::layout_measures::{Measures, measure};
use pd_core::model::Model;
use pd_core::render::draw::Shown;

const EXAMPLES: &[&str] = &[
    "mapk_cascade.json",
    "repressilator.json",
    "tca_cycle.json",
    "GepasiYeastGlycolysis.json",
    "E_coli_Millard2016.json",
];
const SEEDS: [u64; 3] = [1, 2, 3];
/// The methods compared, by name.
const METHODS: &[(&str, LayoutParams)] = &[
    (
        "organic",
        LayoutParams {
            iterations: 800,
            kr: 8000.0,
            ka: 0.02,
            kg: 0.01,
            speed_start: 20.0,
            speed_decay: 0.98,
            randomize: false,
            deckard: false,
            separate_cofactors: false,
        },
    ),
    (
        "organic + cofactors",
        LayoutParams {
            iterations: 800,
            kr: 8000.0,
            ka: 0.02,
            kg: 0.01,
            speed_start: 20.0,
            speed_decay: 0.98,
            randomize: false,
            deckard: false,
            separate_cofactors: true,
        },
    ),
];
/// BioModels taken: this many, with this many reactions.
const BIOMODELS: usize = 8;
const REACTIONS: std::ops::RangeInclusive<usize> = 10..=60;
/// Pictures are at most this wide or high, in pixels.
const PICTURE: f64 = 1400.0;

struct Case {
    name: String,
    model: Model,
    /// Whether `model` has a drawing of its own to compare with.
    drawn: bool,
}

fn main() {
    let out = PathBuf::from(std::env::args().nth(1).expect("an output folder"));
    std::fs::create_dir_all(&out).unwrap();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");

    let mut cases = Vec::new();
    for name in EXAMPLES {
        let text = std::fs::read_to_string(root.join("ExampleModels").join(name)).unwrap();
        let model = pd_core::json::from_json(&text).unwrap().model;
        cases.push(Case {
            name: name.trim_end_matches(".json").to_owned(),
            model,
            drawn: true,
        });
    }
    let escher = |path: &Path| {
        let text = std::fs::read_to_string(path).unwrap();
        Case {
            name: path.file_stem().unwrap().to_string_lossy().into_owned(),
            model: pd_core::escher::import(&text).unwrap().model,
            drawn: true,
        }
    };
    cases.push(escher(
        &root.join("Escher/e_coli_core.Core metabolism.json"),
    ));
    if let Ok(dir) = std::env::var("BIOMODELS") {
        cases.extend(biomodels(Path::new(&dir)));
    }
    if let Ok(paths) = std::env::var("ESCHER_MAPS") {
        cases.extend(
            paths
                .split(';')
                .filter(|p| !p.is_empty())
                .map(|p| escher(Path::new(p))),
        );
    }

    let mut report = String::from(
        "# Layout quality\n\n\
         Each model laid out as one read without a drawing; *as drawn* is its \
         own drawing. Crossings: pairs of lines that cross. Through: lines \
         through another species' box. Overlaps: species boxes that overlap. \
         Spread: how much line lengths differ (0 = all equal). Down: share of \
         reactions drawn with products below reactants. Area: bounding box \
         per species, in thousands of square units. Each method: the mean of \
         three seeds; *+ cofactors* separates the cofactors first.\n\n\
         | Model | Method | Species | Reactions | Crossings | Through | Overlaps | Spread | Down | Area | Aspect |\n\
         |---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|\n",
    );
    let mut pictures = String::from("\n## Pictures\n");
    for case in &cases {
        eprintln!("{}", case.name);
        let mut row = |method: &str, m: &Measures| {
            writeln!(
                report,
                "| {} | {method} | {} | {} | {:.1} | {:.1} | {:.1} | {:.2} | {} | {:.0} | {:.2} |",
                case.name,
                m.species,
                m.reactions,
                m.crossings as f64,
                m.lines_through_species as f64,
                m.overlaps as f64,
                m.length_spread,
                percent(m.downward),
                m.area_per_species / 1000.0,
                m.aspect,
            )
            .unwrap();
        };
        let _ = writeln!(pictures, "\n### {}\n", case.name);
        if case.drawn {
            row("as drawn", &measure(&case.model));
            picture(&out, &case.name, "drawn", &case.model, &mut pictures);
        }
        // Each method, from a model read without a drawing.
        for (method, params) in METHODS {
            let runs: Vec<(Model, Measures)> = SEEDS
                .iter()
                .map(|&seed| {
                    let mut bare = bare(&case.model, seed);
                    if params.separate_cofactors {
                        pd_core::layout::separate_cofactors(&mut bare);
                    }
                    let mut laid = bare.clone();
                    LayoutRun::new(&bare, *params, seed).finish(&mut laid);
                    let m = measure(&laid);
                    (laid, m)
                })
                .collect();
            let mean = mean(runs.iter().map(|(_, m)| m));
            writeln!(
                report,
                "| {} | {method} | {} | {} | {:.1} | {:.1} | {:.1} | {:.2} | {} | {:.0} | {:.2} |",
                case.name,
                mean.species,
                mean.reactions,
                mean.crossings,
                mean.through,
                mean.overlaps,
                mean.spread,
                percent(mean.down),
                mean.area / 1000.0,
                mean.aspect,
            )
            .unwrap();
            let file = method.replace([' ', '+'], "");
            picture(&out, &case.name, &file, &runs[0].0, &mut pictures);
        }
    }
    report.push_str(&pictures);
    std::fs::write(out.join("report.md"), report).unwrap();
    eprintln!("wrote {}", out.join("report.md").display());
}

/// The model as one read without a drawing is: through Antimony, without
/// its drawing, its species at random (seeded).
fn bare(model: &Model, seed: u64) -> Model {
    let text = pd_core::antimony::export(model);
    pd_core::antimony::import(&text, &ImportOptions { seed })
        .unwrap()
        .model
}

/// The first BioModels in `dir` without a drawing, with 10 to 60 reactions:
/// `dir` holds `<id>.xml` files, or `<id>/<id>_url.xml` as BioModels'
/// curated archive unpacks.
fn biomodels(dir: &Path) -> Vec<Case> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter_map(|p| {
            if p.is_dir() {
                let id = p.file_name()?.to_string_lossy().into_owned();
                Some(p.join(format!("{id}_url.xml"))).filter(|f| f.exists())
            } else {
                p.extension().is_some_and(|x| x == "xml").then_some(p)
            }
        })
        .collect();
    files.sort();
    let mut out = Vec::new();
    for path in files {
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(imported) = pd_core::sbml::import(&text, &ImportOptions { seed: 1 }) else {
            continue;
        };
        if imported.had_layout || !REACTIONS.contains(&imported.model.reactions().len()) {
            continue;
        }
        out.push(Case {
            name: path
                .file_stem()
                .unwrap()
                .to_string_lossy()
                .trim_end_matches("_url")
                .to_owned(),
            model: imported.model,
            drawn: false,
        });
        if out.len() == BIOMODELS {
            break;
        }
    }
    out
}

/// The mean of several runs' measures.
struct Mean {
    species: usize,
    reactions: usize,
    crossings: f64,
    through: f64,
    overlaps: f64,
    spread: f64,
    down: f64,
    area: f64,
    aspect: f64,
}

fn mean<'a>(runs: impl Iterator<Item = &'a Measures>) -> Mean {
    let runs: Vec<&Measures> = runs.collect();
    let n = runs.len() as f64;
    let avg = |f: &dyn Fn(&Measures) -> f64| runs.iter().map(|m| f(m)).sum::<f64>() / n;
    Mean {
        species: runs[0].species,
        reactions: runs[0].reactions,
        crossings: avg(&|m| m.crossings as f64),
        through: avg(&|m| m.lines_through_species as f64),
        overlaps: avg(&|m| m.overlaps as f64),
        spread: avg(&|m| m.length_spread),
        down: avg(&|m| m.downward),
        area: avg(&|m| m.area_per_species),
        aspect: avg(&|m| m.aspect),
    }
}

fn percent(share: f64) -> String {
    if share.is_nan() {
        "–".to_owned()
    } else {
        format!("{:.0}%", share * 100.0)
    }
}

/// Writes the drawing as `<case>_<method>.png`, no wider or higher than
/// [`PICTURE`] pixels, and links it from the report.
fn picture(out: &Path, case: &str, method: &str, model: &Model, md: &mut String) {
    let m = measure(model);
    let (w, h) = if m.aspect > 0.0 {
        let area = m.area_per_species * m.species.max(1) as f64;
        let h = (area / m.aspect).sqrt();
        (h * m.aspect, h)
    } else {
        (1.0, 1.0)
    };
    // 96 dpi is one pixel a unit.
    let dpi = (96.0 * PICTURE / w.max(h).max(1.0)).min(96.0);
    let file = format!("{case}_{method}.png");
    match pd_export::model_to_png(model, dpi, Shown::ALL) {
        Ok(png) => {
            std::fs::write(out.join(&file), png).unwrap();
            let _ = writeln!(md, "{method}: ![{case} {method}]({file})\n");
        }
        Err(e) => {
            let _ = writeln!(md, "{method}: no picture ({e})\n");
        }
    }
}
