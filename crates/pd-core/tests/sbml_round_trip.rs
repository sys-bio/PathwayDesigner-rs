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

    // A hidden box is not written, so no other reader draws it; it comes
    // back hidden, its box to be fitted again if it is shown.
    m.find_compartment_mut(&id).unwrap().hidden = true;
    let xml = sbml::export(&m).xml;
    assert!(!xml.contains("compartmentGlyph"), "{xml}");
    let back = sbml::import(&xml, &OPTS).unwrap().model;
    let b = back.find_compartment(&id).unwrap();
    assert!(b.hidden && b.frame.is_none());
    assert_eq!(b.size, 2.5);
    assert_eq!(back.find_species(&s).unwrap().compartment, id);
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

/// A reactant drawn as an alias comes back as that alias, and its species is
/// not made a modifier because the rate law names it (E4 F1).
#[test]
fn a_reactant_drawn_as_an_alias_gets_no_modifier_line() {
    let mut m = Model::new();
    let s = m.add_species(Pt { x: 0.0, y: 0.0 }, 80.0, 36.0);
    let p = m.add_species(Pt { x: 300.0, y: 0.0 }, 80.0, 36.0);
    let s2 = m.add_alias(&s, Pt { x: 0.0, y: 200.0 }).unwrap();
    let r = m.add_reaction(Pt { x: 150.0, y: 100.0 });
    let rx = m.find_reaction_mut(&r).unwrap();
    rx.reactants
        .push(pd_core::model::Participant::new(&s2, 1.0));
    rx.products.push(pd_core::model::Participant::new(&p, 1.0));
    rx.kinetic_law = format!("k*{s}");
    let back = sbml::import(&sbml::export(&m).xml, &OPTS).unwrap().model;
    let q = back.find_reaction(&r).unwrap();
    assert!(q.modifiers.is_empty(), "{:?}", q.modifiers);
    let drawn = back.find_species(&q.reactants[0].species).unwrap();
    assert_eq!(drawn.primary_id(), s);
    assert!(close(drawn.center, Pt { x: 0.0, y: 200.0 }));
}

/// An alias listed before its species (E4 F2): export writes the species'
/// glyph first, and import takes the glyph named after the species as the
/// species whatever the order, so both keep their ids and places.
#[test]
fn an_alias_listed_first_keeps_its_role() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ExampleModels/repressilator.json");
    let m = from_json(&fs::read_to_string(path).unwrap()).unwrap().model;
    let order: Vec<&str> = m.species().iter().map(|s| s.id.as_str()).collect();
    let (alias, main) = (
        order.iter().position(|&s| s == "CI_2").unwrap(),
        order.iter().position(|&s| s == "CI").unwrap(),
    );
    assert!(alias < main, "the file lists the alias first");
    let xml = sbml::export(&m).xml;
    let at = |id: &str| xml.find(&format!("layout:id=\"{id}\"")).unwrap();
    assert!(at("sg_CI") < at("sg_CI_2"));

    let check = |xml: &str| {
        let back = sbml::import(xml, &OPTS).unwrap().model;
        let (ci, ci2) = (
            back.find_species("CI").unwrap(),
            back.find_species("CI_2").unwrap(),
        );
        assert!(!ci.is_alias() && ci2.alias_of.as_deref() == Some("CI"));
        assert!(close(ci.center, m.find_species("CI").unwrap().center));
        assert!(close(ci2.center, m.find_species("CI_2").unwrap().center));
        let decay = back.find_reaction("decay_CI").unwrap();
        assert_eq!(decay.reactants[0].species, "CI");
        assert!(decay.modifiers.is_empty());
        let lac = back.find_reaction("transcribe_lacI").unwrap();
        assert_eq!(lac.modifiers[0].species, "CI_2");
    };
    check(&xml);
    // The same file with the alias's glyph moved first, as another tool might
    // write it.
    let glyph = |id: &str| {
        let start = xml[..at(id)].rfind('<').unwrap();
        let end = start
            + xml[start..].find("</layout:speciesGlyph>").unwrap()
            + "</layout:speciesGlyph>".len();
        (start, end)
    };
    let (a0, a1) = glyph("sg_CI_2");
    let (m0, _) = glyph("sg_CI");
    let moved = format!(
        "{}{}{}{}",
        &xml[..m0],
        &xml[a0..a1],
        &xml[m0..a0],
        &xml[a1..]
    );
    assert!(moved.find("\"sg_CI_2\"").unwrap() < moved.find("\"sg_CI\"").unwrap());
    check(&moved);
}

/// Compartments with no box yet (E4 F3). Export writes the box the app fits
/// round a shown one on opening, so it comes back shown in that box; a hidden
/// one, as an Escher map's are, gets no box and comes back hidden. A file from
/// another tool whose layout draws no box for a compartment brings it in
/// hidden. Without a layout nothing is hidden.
#[test]
fn compartments_without_a_box_come_back_as_they_were_seen() {
    let mut m = Model::new();
    let a = m.add_species(Pt { x: 0.0, y: 0.0 }, 80.0, 36.0);
    let b = m.add_species(Pt { x: 400.0, y: 0.0 }, 80.0, 36.0);
    for (id, hidden, s) in [("cytosol", true, &a), ("nucleus", false, &b)] {
        let mut c = pd_core::model::Compartment::new(id.to_owned());
        c.hidden = hidden;
        m.compartments.push(c);
        m.find_species_mut(s).unwrap().compartment = id.into();
    }
    let mut seen = m.clone();
    pd_core::compartments::place_unplaced(&mut seen);
    let xml = sbml::export(&m).xml;
    let back = sbml::import(&xml, &OPTS).unwrap().model;
    let (c, d) = (
        back.find_compartment("nucleus").unwrap(),
        seen.find_compartment("nucleus").unwrap(),
    );
    assert_eq!((c.hidden, c.frame), (false, d.frame));
    let c = back.find_compartment("cytosol").unwrap();
    assert!(c.hidden && c.frame.is_none());
    assert!(!xml.contains("cg_cytosol"));

    // The same file with no glyph for the nucleus, as another tool might
    // write it: the nucleus comes in hidden.
    let start = xml
        .find("<layout:compartmentGlyph layout:id=\"cg_nucleus\"")
        .unwrap();
    let end = start
        + xml[start..].find("</layout:compartmentGlyph>").unwrap()
        + "</layout:compartmentGlyph>".len();
    let other = format!("{}{}", &xml[..start], &xml[end..]);
    let c = sbml::import(&other, &OPTS).unwrap().model;
    let c = c.find_compartment("nucleus").unwrap();
    assert!(c.hidden && c.frame.is_none());

    // Without the layout, every compartment is shown, in a box fitted later.
    let start = xml.find("<layout:listOfLayouts").unwrap();
    let end = xml.find("</layout:listOfLayouts>").unwrap() + "</layout:listOfLayouts>".len();
    let bare = format!("{}{}", &xml[..start], &xml[end..]);
    let back = sbml::import(&bare, &OPTS).unwrap().model;
    assert!(back.compartments.iter().all(|c| !c.hidden));
}

/// A curved leg is written as the part that is drawn (S §11.5): it meets the
/// node at its border, and its last base point lies on the reaction's side,
/// so a reader draws the same curve and points an arrowhead along it into
/// the node. Checked on every Bézier leg of every model.
#[test]
fn curved_legs_are_written_as_drawn() {
    let mut legs = 0;
    for path in model_files() {
        let model = from_json(&fs::read_to_string(&path).unwrap())
            .unwrap()
            .model;
        let xml = sbml::export(&model).xml;
        let doc = sbml_io::read(&xml).unwrap().document;
        let layout = &doc.model.unwrap().layouts[0];
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        for g in &layout.reaction_glyphs {
            for srg in &g.species_reference_glyphs {
                let Some(curve) = &srg.curve else { continue };
                let [
                    sbml_io::Segment::Bezier {
                        start,
                        end,
                        base1,
                        base2,
                    },
                ] = curve.segments[..]
                else {
                    continue;
                };
                let p = |q: sbml_io::Point| Pt { x: q.x, y: q.y };
                let node_id = srg.species_glyph.strip_prefix("sg_").unwrap();
                let node = model.find_species(node_id).unwrap();
                // The node's end, the base point next to it, and the other end.
                let (at_node, next, far) = match srg.role {
                    Some(sbml_io::Role::Product) => (p(end), p(base2), p(start)),
                    _ => (p(start), p(base1), p(end)),
                };
                // At the border: a step further in is inside, a step back out
                // isn't.
                let inward = pd_core::geometry::normalize(node.center - at_node);
                assert!(
                    pd_core::node_shape::contains(node, at_node + inward * 0.5)
                        && !pd_core::node_shape::contains(node, at_node - inward * 0.5)
                        || at_node.distance(far) < 1.0,
                    "{name}: {} doesn't end at the border",
                    srg.id
                );
                // The curve arrives from outside: its last stretch points into
                // the node, not back out of it.
                assert!(
                    (at_node - next).dot(node.center - at_node) >= -1e-9,
                    "{name}: {} hooks back at its end",
                    srg.id
                );
                legs += 1;
            }
        }
    }
    assert!(legs > 50, "only {legs} curved legs");
}

/// A leg that meets its reaction at a stem's end on the far side of its
/// species from the junction still comes back with its own handles: which
/// end of the written curve is the species' is decided by the species alone.
#[test]
fn a_curved_leg_to_a_far_stem_end_comes_back() {
    use pd_core::geometry::pt;
    use pd_core::model::Participant;
    let mut m = Model::new();
    let a = m.add_species(pt(-150.0, 60.0), 40.0, 40.0);
    let b = m.add_species(pt(300.0, 0.0), 80.0, 36.0);
    let r = m.add_reaction(pt(0.0, 0.0));
    {
        let r = m.find_reaction_mut(&r).unwrap();
        r.mode = ReactionMode::Bezier;
        // The reactants' end of the stem lies beyond the reactant.
        r.stem = Some((pt(-400.0, 0.0), pt(100.0, 0.0)));
        let mut p = Participant::new(a.clone(), 1.0);
        p.ctrl1 = pt(-200.0, 120.0);
        p.ctrl2 = pt(-330.0, 40.0);
        p.ctrl_pts_set = true;
        r.reactants.push(p);
        r.products.push(Participant::new(b, 1.0));
    }
    let back = sbml::import(&sbml::export(&m).xml, &OPTS).unwrap().model;
    let (r0, r1) = (&m.reactions()[0], &back.reactions()[0]);
    let (c0, c1) = (
        leg_curve(&m, r0, Role::Reactant, 0).unwrap(),
        leg_curve(&back, r1, Role::Reactant, 0).unwrap(),
    );
    assert!(close(c0.p1, c1.p1) && close(c0.p2, c1.p2), "{c0:?}\n{c1:?}");
}
