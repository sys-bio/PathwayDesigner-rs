//! SBML export and import (SPEC-Rust §8.3, §12): every `Models/*.json`
//! exports with no problems, and imports back with the same biochemistry and
//! the same drawing: node boxes, aliases, junctions and every leg's curve.

use std::fs;
use std::path::{Path, PathBuf};

use pd_core::antimony::ImportOptions;
use pd_core::bezier::leg_curve;
use pd_core::expr;
use pd_core::geometry::Pt;
use pd_core::json::from_json;
use pd_core::model::{Model, ReactionMode, Role};
use pd_core::render::draw::{effective_junction, effective_mode};
use pd_core::sbml;

fn model_files() -> Vec<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Models");
    let mut files: Vec<PathBuf> = fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    files.sort();
    files
}

const OPTS: ImportOptions = ImportOptions {
    convert_catalytic: false,
    seed: 1,
};

fn close(a: Pt, b: Pt) -> bool {
    a.distance(b) < 1e-6
}

/// An expression in a canonical form, so `k1*S` and `k1 * S` compare equal.
fn canonical(text: &str) -> String {
    expr::parse(text.trim()).map_or_else(|_| text.trim().to_owned(), |a| expr::to_infix(&a))
}

/// What differs between a model and the same model after export and import.
fn differences(a: &Model, b: &Model) -> Vec<String> {
    let mut d = Vec::new();
    let mut check = |ok: bool, what: String| {
        if !ok {
            d.push(what);
        }
    };
    check(
        a.species().len() == b.species().len(),
        format!("{} species vs {}", a.species().len(), b.species().len()),
    );
    for s in a.species() {
        let Some(t) = b.find_species(&s.id) else {
            check(false, format!("species {} is missing", s.id));
            continue;
        };
        check(
            close(s.center, t.center),
            format!("{}: centre {:?} vs {:?}", s.id, s.center, t.center),
        );
        check(
            (s.width - t.width).abs() < 1e-9 && (s.height - t.height).abs() < 1e-9,
            format!("{}: size", s.id),
        );
        check(
            s.alias_of == t.alias_of,
            format!("{}: alias of {:?} vs {:?}", s.id, s.alias_of, t.alias_of),
        );
        check(
            s.is_null_node == t.is_null_node,
            format!("{}: null node", s.id),
        );
        if !s.is_null_node && !s.is_alias() {
            check(
                s.initial_value == t.initial_value,
                format!("{}: initial value", s.id),
            );
            check(
                s.is_boundary == t.is_boundary,
                format!("{}: boundary", s.id),
            );
            check(
                s.is_constant == t.is_constant,
                format!("{}: constant", s.id),
            );
        }
    }
    for r in a.reactions() {
        let Some(q) = b.find_reaction(&r.id) else {
            check(false, format!("reaction {} is missing", r.id));
            continue;
        };
        let names = |list: &[pd_core::model::Participant]| {
            list.iter()
                .map(|p| (p.species.clone(), p.stoichiometry))
                .collect::<Vec<_>>()
        };
        check(
            names(&r.reactants) == names(&q.reactants),
            format!(
                "{}: reactants {:?} vs {:?}",
                r.id,
                names(&r.reactants),
                names(&q.reactants)
            ),
        );
        check(
            names(&r.products) == names(&q.products),
            format!("{}: products", r.id),
        );
        let mods = |list: &[pd_core::model::Participant]| {
            let mut v: Vec<String> = list.iter().map(|p| p.species.clone()).collect();
            v.sort();
            v.dedup();
            v
        };
        check(
            mods(&r.modifiers) == mods(&q.modifiers),
            format!(
                "{}: modifiers {:?} vs {:?}",
                r.id,
                mods(&r.modifiers),
                mods(&q.modifiers)
            ),
        );
        check(
            r.is_reversible == q.is_reversible,
            format!("{}: reversible", r.id),
        );
        if expr::parse(r.kinetic_law.trim()).is_ok() {
            check(
                canonical(&r.kinetic_law) == canonical(&q.kinetic_law),
                format!("{}: law {:?} vs {:?}", r.id, r.kinetic_law, q.kinetic_law),
            );
        }
        check(
            close(effective_junction(a, r), q.junction),
            format!(
                "{}: junction {:?} vs {:?}",
                r.id,
                effective_junction(a, r),
                q.junction
            ),
        );
        // A Bézier reaction keeps its curves exactly; other modes come back
        // as Straight (SBML has no Linear).
        let bezier = effective_mode(r) == ReactionMode::Bezier;
        check(
            bezier == (q.mode == ReactionMode::Bezier),
            format!("{}: mode {:?} vs {:?}", r.id, r.mode, q.mode),
        );
        if bezier {
            for role in [Role::Reactant, Role::Product] {
                for i in 0..r.participants(role).len() {
                    let (ca, cb) = (leg_curve(a, r, role, i), leg_curve(b, q, role, i));
                    let same = match (ca, cb) {
                        (Some(x), Some(y)) => {
                            close(x.p0, y.p0)
                                && close(x.p1, y.p1)
                                && close(x.p2, y.p2)
                                && close(x.p3, y.p3)
                        }
                        _ => false,
                    };
                    check(
                        same,
                        format!("{}: {role:?} leg {i}: {ca:?} vs {cb:?}", r.id),
                    );
                }
            }
        }
    }
    for p in &a.parameters {
        let found = b.find_parameter(&p.variable);
        check(
            found.is_some_and(|q| canonical(&q.expression) == canonical(&p.expression)),
            format!(
                "parameter {}: {:?} vs {:?}",
                p.variable,
                p.expression,
                found.map(|q| &q.expression)
            ),
        );
    }
    for r in &a.assignment_rules {
        let found = b.assignment_rules.iter().find(|q| q.variable == r.variable);
        check(
            found.is_some_and(|q| canonical(&q.expression) == canonical(&r.expression)),
            format!("rule {}", r.variable),
        );
    }
    d
}

#[test]
fn every_model_exports_and_imports_back() {
    let mut failures = Vec::new();
    for path in model_files() {
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let model = from_json(&fs::read_to_string(&path).unwrap())
            .unwrap()
            .model;
        let out = sbml::export(&model);
        if !out.problems.is_empty() {
            failures.push(format!("{name}: export problems {:?}", out.problems));
        }
        let back = match sbml::import(&out.xml, &OPTS) {
            Ok(i) => {
                if i.unplaced > 0 || !i.had_layout {
                    failures.push(format!(
                        "{name}: {} unplaced, layout {}",
                        i.unplaced, i.had_layout
                    ));
                }
                if !i.warnings.is_empty() {
                    failures.push(format!("{name}: warnings {:?}", i.warnings));
                }
                i.model
            }
            Err(e) => {
                failures.push(format!("{name}: import failed {e:?}"));
                continue;
            }
        };
        for d in differences(&model, &back) {
            failures.push(format!("{name}: {d}"));
        }
        // A second round trip changes nothing more.
        let again = sbml::export(&back);
        if again.xml != sbml::export(&sbml::import(&again.xml, &OPTS).unwrap().model).xml {
            failures.push(format!("{name}: a second round trip changed the SBML"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
