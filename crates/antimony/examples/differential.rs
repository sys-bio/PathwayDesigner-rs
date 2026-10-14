//! The differential tier (SPEC.md §8): this crate against the Delphi
//! implementation, case by case over the corpus.
//!
//! ```text
//! cargo run --release -p antimony-rs --example differential
//! cargo run --release -p antimony-rs --example differential -- 00001 01234
//! ```
//!
//! Needs `ANTIMONY_CORPUS`, `ANTIMONY_CORPUS_INDEX` and `ANTIMONY_ORACLE`
//! (the Delphi `ant2sbml.exe`, so in practice Windows). The oracle's output
//! is cached under `target/antimony-oracle/`; delete that folder after
//! rebuilding the oracle.
//!
//! Two things are compared, on every case of the corpus:
//!
//! - **Diagnostics**, every one, by line and code: the oracle's on stderr
//!   against `antimony::parse`'s. This covers the `comp` cases and the ones
//!   the oracle rejects as well.
//! - **The model**, where the oracle wrote SBML and the case is outside the
//!   `comp` package. The oracle writes SBML, which `sbml-io` reads; this
//!   crate's side is `antimony::parse` of the same file, flattened. They are
//!   compared structurally, never as text.
//!
//! Until this crate writes SBML (A7) the model comparison is of what reading
//! decides: which ids are species, compartments and parameters, and each
//! species' compartment and boundary flag; each reaction's id,
//! reversibility, participants, stoichiometry and whether it has a law;
//! which symbols have assignment and rate rules; event ids and their
//! targets; function definition ids; and the number of algebraic rules.
//! Every difference is a divergence, except one the Delphi SBML writer is
//! known to add: a `default_compartment` for species that have none.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::{env, fs};

use antimony::expr::ExprKind;
use antimony::model::{Module, Participant, RuleKind, SymbolKind};

/// One case to compare.
struct Case {
    id: String,
    path: PathBuf,
    comp: bool,
}

/// What the oracle made of a case.
struct Oracle {
    /// `None` when it rejected the case.
    sbml: Option<String>,
    /// Its diagnostics, as "line code" (line 0 for none), sorted.
    diagnostics: Vec<String>,
}

fn var(name: &str) -> String {
    env::var(name).unwrap_or_else(|_| {
        eprintln!("{name} is not set; see the documentation at the top of this file");
        std::process::exit(2);
    })
}

/// Every case of the index, or those named on the command line.
fn cases(root: &Path, index: &Path, only: &[String]) -> Vec<Case> {
    let text = fs::read_to_string(index).expect("cannot read the index");
    let json: serde_json::Value = serde_json::from_str(&text).expect("index is not JSON");
    json.as_array()
        .expect("index is not a list")
        .iter()
        .map(|item| {
            let id = item["case"].as_str().unwrap_or_default().to_string();
            let packages = item["packages"].as_array();
            Case {
                path: root.join(&id).join(format!("{id}-antimony.ant")),
                comp: packages.is_some_and(|p| p.iter().any(|v| v == "comp")),
                id,
            }
        })
        .filter(|case| only.is_empty() || only.contains(&case.id))
        .collect()
}

/// A diagnostic line of the oracle's, `file(3:17): error: message [dcCode]`,
/// as "line code".
fn oracle_diagnostic(line: &str) -> Option<String> {
    let line = line.trim_end();
    let code = line.strip_suffix(']')?.rsplit_once('[')?.1;
    let at = line
        .find(".ant(")
        .and_then(|i| line[i + 5..].split_once(':'))
        .and_then(|(n, _)| n.parse::<u32>().ok())
        .unwrap_or(0);
    Some(format!("{at} {code}"))
}

/// The oracle's output for a case, from the cache or by running it.
fn run_oracle(oracle: &str, cache: &Path, case: &Case) -> Oracle {
    let sbml_file = cache.join(format!("{}.xml", case.id));
    let errors_file = cache.join(format!("{}.err", case.id));
    let (sbml, errors) = match fs::read_to_string(&errors_file) {
        Ok(errors) => (fs::read_to_string(&sbml_file).ok(), errors),
        Err(_) => {
            let output = Command::new(oracle)
                .arg(&case.path)
                .arg("--quiet")
                .output()
                .expect("cannot run the oracle");
            let errors = String::from_utf8_lossy(&output.stderr).into_owned();
            let sbml = output
                .status
                .success()
                .then(|| String::from_utf8(output.stdout).expect("the oracle wrote invalid UTF-8"));
            match &sbml {
                Some(text) => fs::write(&sbml_file, text).ok(),
                None => fs::remove_file(&sbml_file).ok(),
            };
            fs::write(&errors_file, &errors).ok();
            (sbml, errors)
        }
    };
    let mut diagnostics: Vec<String> = errors.lines().filter_map(oracle_diagnostic).collect();
    diagnostics.sort();
    Oracle { sbml, diagnostics }
}

/// A participant as the SBML gives it: species and stoichiometry, either
/// a number or the id of a named stoichiometry.
fn ours_participant(p: &Participant) -> String {
    match &p.stoichiometry {
        None => format!("1 {}", p.species),
        Some(e) => match (&e.kind, e.evaluate()) {
            (ExprKind::Symbol(name), _) => format!("[{name}] {}", p.species),
            (_, Some(value)) => format!("{value} {}", p.species),
            _ => format!("({e}) {}", p.species),
        },
    }
}

fn theirs_participant(r: &sbml_io::SpeciesReference) -> String {
    match (&r.id, r.stoichiometry) {
        (Some(id), _) => format!("[{id}] {}", r.species),
        (None, value) => format!("{} {}", value.unwrap_or(1.0), r.species),
    }
}

fn sorted(mut items: Vec<String>) -> Vec<String> {
    items.sort();
    items
}

/// What this crate's parse says, as comparable lines.
fn ours(module: &Module) -> BTreeMap<String, String> {
    let mut facts = BTreeMap::new();
    for r in &module.reactions {
        let side = |ps: &[Participant]| sorted(ps.iter().map(ours_participant).collect());
        facts.insert(
            format!("reaction {}", r.name),
            format!(
                "reversible={} {:?} -> {:?} law={}",
                r.is_reversible(),
                side(&r.reactants),
                side(&r.products),
                r.kinetic_law.is_some()
            ),
        );
    }
    for s in &module.symbols {
        if let Some(rule) = &s.rule {
            if rule.expr.is_some() {
                let kind = match rule.kind {
                    RuleKind::Assignment => "assignment",
                    RuleKind::Rate => "rate",
                };
                facts.insert(format!("rule {}", s.name), kind.to_string());
            }
        }
    }
    for e in &module.events {
        if let Some(name) = &e.name {
            let targets = sorted(e.assignments.iter().map(|a| a.target.clone()).collect());
            facts.insert(format!("event {name}"), format!("{targets:?}"));
        }
    }
    for f in &module.functions {
        facts.insert(format!("function {}", f.name), String::new());
    }
    let algebraic = module
        .algebraic_rules
        .iter()
        .filter(|r| r.expr.is_some())
        .count();
    facts.insert("algebraic rules".into(), algebraic.to_string());
    for kind in [
        SymbolKind::Species,
        SymbolKind::Compartment,
        SymbolKind::Parameter,
    ] {
        for s in module.symbols_of(kind) {
            let detail = match kind {
                SymbolKind::Species => format!(
                    "species in {} boundary={}",
                    s.compartment.as_deref().unwrap_or(DEFAULT_COMPARTMENT),
                    s.is_boundary
                ),
                _ => kind.name().to_string(),
            };
            facts.insert(format!("symbol {}", s.name), detail);
        }
    }
    facts
}

/// The compartment the Delphi SBML writer adds for species that have none.
const DEFAULT_COMPARTMENT: &str = "default_compartment";

/// What the oracle's SBML says, in the same form.
fn theirs(model: &sbml_io::Model) -> BTreeMap<String, String> {
    let mut facts = BTreeMap::new();
    for r in &model.reactions {
        let side =
            |rs: &[sbml_io::SpeciesReference]| sorted(rs.iter().map(theirs_participant).collect());
        let law = r.kinetic_law.as_ref().is_some_and(|k| k.math.is_some());
        facts.insert(
            format!("reaction {}", r.id),
            format!(
                "reversible={} {:?} -> {:?} law={law}",
                r.reversible.unwrap_or(false),
                side(&r.reactants),
                side(&r.products),
            ),
        );
    }
    for rule in &model.rules {
        let kind = match rule {
            sbml_io::Rule::Assignment { .. } => "assignment",
            sbml_io::Rule::Rate { .. } => "rate",
            sbml_io::Rule::Algebraic { .. } => continue,
        };
        facts.insert(
            format!("rule {}", rule.variable().unwrap_or_default()),
            kind.to_string(),
        );
    }
    for e in &model.events {
        if let Some(id) = &e.id {
            let targets = sorted(e.assignments.iter().map(|a| a.variable.clone()).collect());
            facts.insert(format!("event {id}"), format!("{targets:?}"));
        }
    }
    for f in &model.function_definitions {
        facts.insert(format!("function {}", f.id), String::new());
    }
    let algebraic = model
        .rules
        .iter()
        .filter(|r| matches!(r, sbml_io::Rule::Algebraic { .. }))
        .count();
    facts.insert("algebraic rules".into(), algebraic.to_string());
    for s in &model.species {
        facts.insert(
            format!("symbol {}", s.id),
            format!(
                "species in {} boundary={}",
                s.compartment,
                s.boundary_condition.unwrap_or(false)
            ),
        );
    }
    for c in &model.compartments {
        facts.insert(format!("symbol {}", c.id), "compartment".into());
    }
    for p in &model.parameters {
        facts.insert(format!("symbol {}", p.id), "parameter".into());
    }
    facts
}

fn main() {
    let root = PathBuf::from(var("ANTIMONY_CORPUS"));
    let index = PathBuf::from(var("ANTIMONY_CORPUS_INDEX"));
    let oracle = var("ANTIMONY_ORACLE");
    let only: Vec<String> = env::args().skip(1).collect();
    let cache = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/antimony-oracle");
    fs::create_dir_all(&cache).expect("cannot make the cache folder");

    let cases = cases(&root, &index, &only);
    let (mut agree, mut models, mut facts, mut messages) = (0, 0, 0, 0);
    let mut divergent = Vec::new();
    for case in &cases {
        let oracle = run_oracle(&oracle, &cache, case);
        let source = fs::read_to_string(&case.path).expect("cannot read the case");
        let loaded = antimony::parse(&source);
        let mut differences = Vec::new();

        let mut ours_diagnostics: Vec<String> = loaded
            .diagnostics
            .iter()
            .map(|d| format!("{} {}", d.span.start.line, d.code.name()))
            .collect();
        ours_diagnostics.sort();
        messages += ours_diagnostics.len().max(oracle.diagnostics.len());
        if ours_diagnostics != oracle.diagnostics {
            differences.push((
                "diagnostics".to_string(),
                Some(ours_diagnostics.join(", ")),
                Some(oracle.diagnostics.join(", ")),
            ));
        }

        if let (Some(xml), false) = (&oracle.sbml, case.comp) {
            models += 1;
            let model = match sbml_io::read(xml) {
                Ok(read) => read.document.model.unwrap_or_default(),
                Err(e) => panic!("{}: sbml-io cannot read the oracle's SBML: {e:?}", case.id),
            };
            let ours = ours(&antimony::flatten::flatten(&loaded.document));
            let mut theirs = theirs(&model);
            let default = format!("symbol {DEFAULT_COMPARTMENT}");
            if !ours.contains_key(&default) {
                theirs.remove(&default);
            }
            facts += ours.len().max(theirs.len());
            for key in ours.keys().chain(theirs.keys()) {
                let (a, b) = (ours.get(key), theirs.get(key));
                if a != b && !differences.iter().any(|(k, _, _)| k == key) {
                    differences.push((key.clone(), a.cloned(), b.cloned()));
                }
            }
        }

        if differences.is_empty() {
            agree += 1;
        } else {
            divergent.push((case.id.clone(), differences));
        }
    }

    for (id, differences) in &divergent {
        println!("{id}:");
        for (key, ours, theirs) in differences.iter().take(5) {
            println!("  {key}\n    ours:   {ours:?}\n    Delphi: {theirs:?}");
        }
    }
    println!(
        "{} cases: {agree} agree, {} diverge ({messages} diagnostics and, over {models} models, {facts} facts compared)",
        cases.len(),
        divergent.len()
    );
    if !divergent.is_empty() {
        std::process::exit(1);
    }
}
