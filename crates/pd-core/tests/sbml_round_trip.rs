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
use pd_core::model::{Model, ReactionMode, Rgba, Role, TextFont, Typeface};
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
            close(effective_junction(a, r), effective_junction(b, q)),
            format!(
                "{}: junction {:?} vs {:?}",
                r.id,
                effective_junction(a, r),
                effective_junction(b, q)
            ),
        );
        // Every mode comes back, Linear and smooth junctions through
        // PathwayDesigner's note in the glyph's annotation; a Bézier reaction
        // keeps its curves exactly.
        check(
            r.mode == q.mode,
            format!("{}: mode {:?} vs {:?}", r.id, r.mode, q.mode),
        );
        let bezier = effective_mode(r) == ReactionMode::Bezier;
        check(
            !bezier || r.junction_smooth == q.junction_smooth,
            format!(
                "{}: smooth junction {} vs {}",
                r.id, r.junction_smooth, q.junction_smooth
            ),
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
        // A second round trip changes nothing more (within rounding: a Linear
        // junction is projected onto its line again each time).
        let again = sbml::import(&sbml::export(&back).xml, &OPTS).unwrap().model;
        for d in differences(&back, &again) {
            failures.push(format!("{name}: a second round trip changed {d}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn free_text_exports_as_a_text_glyph_and_comes_back() {
    let mut m = Model::new();
    m.add_species(Pt::ZERO, 80.0, 36.0);
    let id = m.add_text(Pt { x: 200.0, y: -50.0 }, "Glycolysis\nin yeast");
    let t = m.find_text_mut(&id).unwrap();
    t.font = TextFont {
        typeface: Typeface::Serif,
        bold: true,
        italic: false,
    };
    t.size = 20.0;
    t.color = Rgba::rgb(0x11, 0x22, 0x33);
    t.wrap_width = 150.0;
    let out = sbml::export(&m);
    assert!(out.problems.is_empty(), "{:?}", out.problems);
    assert!(
        out.xml.contains("text=\"Glycolysis\nin yeast\"")
            || out.xml.contains("Glycolysis&#10;in yeast")
    );
    let back = sbml::import(&out.xml, &OPTS).unwrap().model;
    assert_eq!(back.texts().len(), 1);
    let (a, b) = (&m.texts()[0], &back.texts()[0]);
    assert_eq!(b.text, a.text);
    assert!(close(b.position, a.position));
    assert_eq!(
        (b.font, b.size, b.color, b.wrap_width),
        (a.font, a.size, a.color, a.wrap_width)
    );
}

#[test]
fn compartment_boxes_and_styles_come_back() {
    let mut m = Model::new();
    let s = m.add_species(Pt { x: 0.0, y: 0.0 }, 80.0, 36.0);
    let id = m.add_compartment(pd_core::model::Frame {
        center: Pt { x: 10.0, y: 20.0 },
        width: 300.0,
        height: 200.0,
    });
    m.find_species_mut(&s).unwrap().compartment = id.clone();
    let c = m.find_compartment_mut(&id).unwrap();
    c.size = 2.5;
    c.style.has_custom_style = true;
    c.style.fill_color = Rgba::rgb(0xEE, 0xEE, 0xFF);
    c.style.border_width = 3.0;
    let out = sbml::export(&m);
    assert!(out.problems.is_empty(), "{:?}", out.problems);
    let back = sbml::import(&out.xml, &OPTS).unwrap().model;
    let (a, b) = (
        m.find_compartment(&id).unwrap(),
        back.find_compartment(&id).unwrap(),
    );
    assert_eq!(b.frame, a.frame);
    assert_eq!(b.size, 2.5);
    assert_eq!(b.style.fill_color, a.style.fill_color);
    assert_eq!(b.style.border_width, 3.0);
    assert_eq!(back.find_species(&s).unwrap().compartment, id);
    assert!(!b.hidden);

    // A hidden box comes back hidden, in its place.
    m.find_compartment_mut(&id).unwrap().hidden = true;
    let back = sbml::import(&sbml::export(&m).xml, &OPTS).unwrap().model;
    let b = back.find_compartment(&id).unwrap();
    assert!(b.hidden);
    assert_eq!(b.frame, m.find_compartment(&id).unwrap().frame);
    assert_eq!(b.style.fill_color, Rgba::rgb(0xEE, 0xEE, 0xFF));
}

/// Modifier kinds (S §5.7.1): the glyph's role and the reference's SBO term
/// carry them, and either alone brings them back.
#[test]
fn modifier_kinds_come_back_from_roles_and_sbo_terms() {
    use pd_core::model::ModifierKind as K;
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ExampleModels/mapk_cascade.json");
    let mut m = from_json(&fs::read_to_string(path).unwrap()).unwrap().model;
    for (r, k) in [
        ("J1", K::Inhibitor),
        ("J3", K::Activator),
        ("J4", K::Catalyst),
    ] {
        m.find_reaction_mut(r).unwrap().modifiers[0].kind = k;
    }
    let kinds = |m: &Model| -> Vec<(String, K)> {
        ["J1", "J3", "J4", "J7"]
            .iter()
            .map(|r| (r.to_string(), m.find_reaction(r).unwrap().modifiers[0].kind))
            .collect()
    };
    let want = |j1, j3, j4| {
        vec![
            ("J1".to_owned(), j1),
            ("J3".to_owned(), j3),
            ("J4".to_owned(), j4),
            ("J7".to_owned(), K::Modifier),
        ]
    };
    let exported = sbml::export(&m);
    assert!(exported.problems.is_empty(), "{:?}", exported.problems);
    let xml = exported.xml;
    assert!(xml.contains("role=\"inhibitor\""));
    assert!(xml.contains("role=\"activator\""));
    for term in ["SBO:0000020", "SBO:0000459", "SBO:0000013"] {
        assert_eq!(xml.matches(term).count(), 1, "{term}");
    }
    // A plain modifier gets no SBO term.
    assert!(!xml.contains("SBO:0000019"));
    let back = sbml::import(&xml, &OPTS).unwrap().model;
    assert_eq!(kinds(&back), want(K::Inhibitor, K::Activator, K::Catalyst));
    // Without the layout: the SBO terms alone.
    let start = xml.find("<layout:listOfLayouts").expect("a layout");
    let end = xml.find("</layout:listOfLayouts>").unwrap() + "</layout:listOfLayouts>".len();
    let bare = format!("{}{}", &xml[..start], &xml[end..]);
    let back = sbml::import(&bare, &OPTS).unwrap().model;
    assert_eq!(kinds(&back), want(K::Inhibitor, K::Activator, K::Catalyst));
    // Without the SBO terms: the roles alone (a catalyst has no role of its own).
    let no_sbo: String = xml
        .split(" sboTerm=\"")
        .enumerate()
        .map(|(i, part)| {
            if i == 0 {
                part
            } else {
                &part[part.find('"').unwrap() + 1..]
            }
        })
        .collect();
    assert!(!no_sbo.contains("sboTerm"));
    let back = sbml::import(&no_sbo, &OPTS).unwrap().model;
    assert_eq!(kinds(&back), want(K::Inhibitor, K::Activator, K::Modifier));
}
