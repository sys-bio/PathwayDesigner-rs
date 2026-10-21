//! A model's structure (docs/structural-analysis-design.md) as JSON, for
//! scripts/structure_check.py to compare with libRoadRunner: the species and
//! reactions, the stoichiometry matrix, the rank, the independent and
//! dependent species and reactions, L₀, K₀, and how long the analysis took.
//! With a second argument it also writes the model as SBML, every empty rate
//! law set to `1` first (libRoadRunner needs one; the structure doesn't
//! depend on it).
//!
//! ```text
//! cargo run -p pd-core --release --example structure_report -- <model> [<out.xml>]
//! ```
//! `<model>`: a PathwayDesigner `.json`, an Escher map (`.json`, an array) or
//! SBML (`.xml`/`.sbml`).

use pd_core::model::Model;
use pd_core::structure::{Matrix, Structure};
use serde_json::{Value, json};

fn read(path: &str) -> Model {
    let text = std::fs::read_to_string(path).expect("readable");
    if path.ends_with(".xml") || path.ends_with(".sbml") {
        let opts = pd_core::antimony::ImportOptions {
            convert_catalytic: false,
            seed: 1,
        };
        return pd_core::sbml::import(&text, &opts).expect("SBML").model;
    }
    if pd_core::escher::looks_like_escher(&text) {
        return pd_core::escher::import(&text).expect("Escher map").model;
    }
    pd_core::json::from_json(&text)
        .expect("PathwayDesigner JSON")
        .model
}

fn matrix(m: &Matrix) -> Value {
    (0..m.rows)
        .map(|r| (0..m.cols).map(|c| m.get(r, c)).collect::<Vec<_>>())
        .collect()
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut model = read(&args[0]);
    let start = std::time::Instant::now();
    let s = Structure::of(&model);
    let seconds = start.elapsed().as_secs_f64();
    let names = |list: &[String], idx: &[usize]| -> Vec<String> {
        idx.iter().map(|&i| list[i].clone()).collect()
    };
    // The sparse bases, as full vectors (reactions, species in model order).
    let start = std::time::Instant::now();
    let sparse_fluxes = s.sparse_null_space();
    let sparse_laws = s.sparse_conservation_laws();
    let sparse_seconds = start.elapsed().as_secs_f64();
    let full = |terms: &[(usize, f64)], n: usize| {
        let mut v = vec![0.0; n];
        for &(i, k) in terms {
            v[i] = k;
        }
        v
    };
    let k_support: Vec<usize> = s.null_space().iter().map(|v| v.terms.len()).collect();
    let out = json!({
        "species": s.species,
        "reactions": s.reactions,
        "n": matrix(&s.n),
        "rank": s.rank,
        "independent_species": names(&s.species, &s.independent_species),
        "dependent_species": names(&s.species, &s.dependent_species),
        "l0": matrix(&s.l0),
        "dependent_reactions": names(&s.reactions, &s.dependent_reactions),
        "independent_reactions": names(&s.reactions, &s.independent_reactions),
        "k0": matrix(&s.k0),
        "seconds": seconds,
        "sparse_null_space": sparse_fluxes.iter().map(|v| full(&v.terms, s.reactions.len())).collect::<Vec<_>>(),
        "sparse_laws": sparse_laws.iter().map(|l| full(&l.terms, s.species.len())).collect::<Vec<_>>(),
        "k_support": k_support,
        "sparse_seconds": sparse_seconds,
    });
    println!("{out}");
    if let Some(xml) = args.get(1) {
        for r in model.reactions_mut() {
            if r.kinetic_law.trim().is_empty() {
                r.kinetic_law = "1".into();
            }
        }
        std::fs::write(xml, pd_core::sbml::export(&model).xml).expect("writable");
    }
}
