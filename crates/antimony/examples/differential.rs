//! The differential tier (SPEC.md §8): this crate against the Delphi
//! implementation, case by case over the corpus.
//!
//! ```text
//! cargo run --release -p antimony-rs --example differential
//! cargo run --release -p antimony-rs --example differential -- 00001 01234
//! ```
//!
//! Needs `ANTIMONY_CORPUS`, `ANTIMONY_CORPUS_INDEX` and `ANTIMONY_ORACLE`
//! (the Delphi `ant2sbml.exe`, so in practice Windows). The oracle's SBML is
//! cached under `target/antimony-oracle/`; delete that folder after
//! rebuilding the oracle.
//!
//! The oracle writes SBML, which `sbml-io` reads; this crate's side is its
//! parse of the same file. They are compared structurally, never as text.
//! Until the semantic pass exists (A4) only what the parser alone decides
//! is compared: each reaction's id, reversibility, participants,
//! stoichiometry and whether it has a law; which symbols have assignment
//! and rate rules; event ids and their targets; function definition ids;
//! and the number of algebraic rules. Every difference is a divergence.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::{env, fs};

use antimony::expr::ExprKind;
use antimony::model::{Module, Participant, RuleKind};
use antimony::parser::parse_document;

/// One case to compare.
struct Case {
    id: String,
    path: PathBuf,
}

fn var(name: &str) -> String {
    env::var(name).unwrap_or_else(|_| {
        eprintln!("{name} is not set; see the documentation at the top of this file");
        std::process::exit(2);
    })
}

/// The non-`comp` cases of the index, or those named on the command line.
fn cases(root: &Path, index: &Path, only: &[String]) -> Vec<Case> {
    let text = fs::read_to_string(index).expect("cannot read the index");
    let json: serde_json::Value = serde_json::from_str(&text).expect("index is not JSON");
    json.as_array()
        .expect("index is not a list")
        .iter()
        .filter(|item| {
            let packages = item["packages"].as_array();
            !packages.is_some_and(|p| p.iter().any(|v| v == "comp"))
                && item["expect_reject"]
                    .as_str()
                    .unwrap_or_default()
                    .is_empty()
        })
        .map(|item| {
            let id = item["case"].as_str().unwrap_or_default().to_string();
            Case {
                path: root.join(&id).join(format!("{id}-antimony.ant")),
                id,
            }
        })
        .filter(|case| only.is_empty() || only.contains(&case.id))
        .collect()
}

/// The oracle's SBML for a case, from the cache or by running it. `None`
/// when the oracle rejects the case.
fn oracle_sbml(oracle: &str, cache: &Path, case: &Case) -> Option<String> {
    let cached = cache.join(format!("{}.xml", case.id));
    let failed = cache.join(format!("{}.failed", case.id));
    if failed.exists() {
        return None;
    }
    if let Ok(text) = fs::read_to_string(&cached) {
        return Some(text);
    }
    let output = Command::new(oracle)
        .arg(&case.path)
        .arg("--quiet")
        .output()
        .expect("cannot run the oracle");
    if !output.status.success() {
        fs::write(&failed, &output.stderr).ok();
        return None;
    }
    let text = String::from_utf8(output.stdout).expect("the oracle wrote invalid UTF-8");
    fs::write(&cached, &text).ok();
    Some(text)
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
    facts
}

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
    let (mut agree, mut skipped, mut facts, mut divergent) = (0, 0, 0, Vec::new());
    for case in &cases {
        let Some(xml) = oracle_sbml(&oracle, &cache, case) else {
            skipped += 1;
            continue;
        };
        let model = match sbml_io::read(&xml) {
            Ok(read) => read.document.model.unwrap_or_default(),
            Err(e) => panic!("{}: sbml-io cannot read the oracle's SBML: {e:?}", case.id),
        };
        let source = fs::read_to_string(&case.path).expect("cannot read the case");
        let (document, _) = parse_document(&source);

        let ours = ours(document.main_module());
        let theirs = theirs(&model);
        facts += ours.len().max(theirs.len());
        let mut differences = Vec::new();
        for key in ours.keys().chain(theirs.keys()) {
            let (a, b) = (ours.get(key), theirs.get(key));
            if a != b && !differences.iter().any(|(k, _, _)| k == key) {
                differences.push((key.clone(), a.cloned(), b.cloned()));
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
        "{} cases: {agree} agree, {} diverge, {skipped} rejected by the oracle ({facts} facts compared)",
        cases.len(),
        divergent.len()
    );
    if !divergent.is_empty() {
        std::process::exit(1);
    }
}
