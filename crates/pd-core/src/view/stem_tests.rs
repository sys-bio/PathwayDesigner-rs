//! Reaction stems (S §5.13): where the lines meet, drawing, hit-testing,
//! what carries a stem along and what takes it away, names, files and the
//! Escher theme. Driven through the view.

use super::*;
use crate::bezier::leg_curve;
use crate::model::Participant;
use crate::render::draw::{clear_reaction_label_offset, stem_line};

/// A at (0,0) → B at (400,0), Straight, junction (200,0), with a stem from
/// (160,0) to (240,0); C at (0,200) and D at (400,200) spare. Screen = world.
fn view() -> DiagramView {
    let mut m = Model::new();
    for (id, x, y) in [
        ("A", 0.0, 0.0),
        ("B", 400.0, 0.0),
        ("C", 0.0, 200.0),
        ("D", 400.0, 200.0),
    ] {
        m.push_species(crate::model::SpeciesNode::new(id, pt(x, y)))
            .unwrap();
    }
    let mut r = crate::model::Reaction::new("R1", pt(200.0, 0.0));
    r.reactants = vec![Participant::new("A", 1.0)];
    r.products = vec![Participant::new("B", 1.0)];
    r.stem = Some((pt(-40.0, 0.0), pt(40.0, 0.0)));
    m.push_reaction(r).unwrap();
    DiagramView::new(m)
}

fn r1(v: &DiagramView) -> crate::model::Reaction {
    v.model().find_reaction("R1").unwrap().clone()
}

fn drag(v: &mut DiagramView, from: Pt, to: Pt) {
    v.handle(&ViewEvent::PointerDown {
        pos: from,
        button: Button::Primary,
        modifiers: Modifiers::NONE,
    });
    v.handle(&ViewEvent::PointerMove { pos: to });
    v.handle(&ViewEvent::PointerUp {
        pos: to,
        button: Button::Primary,
    });
}

#[test]
fn the_lines_meet_the_stem_s_ends() {
    let v = view();
    let r = r1(&v);
    assert_eq!(r.leg_anchor(Role::Reactant), pt(160.0, 0.0));
    assert_eq!(r.leg_anchor(Role::Product), pt(240.0, 0.0));
    assert_eq!(
        r.leg_anchor(Role::Modifier),
        pt(200.0, 0.0),
        "arcs: the junction"
    );
    assert_eq!(
        stem_line(&r),
        Some([pt(160.0, 0.0), pt(200.0, 0.0), pt(240.0, 0.0)])
    );
    // A Bézier leg ends there too.
    let mut v = view();
    v.model.find_reaction_mut("R1").unwrap().mode = ReactionMode::Bezier;
    let r = r1(&v);
    let c = leg_curve(v.model(), &r, Role::Reactant, 0).unwrap();
    assert_eq!((c.p0, c.p3), (pt(0.0, 0.0), pt(160.0, 0.0)));
    let c = leg_curve(v.model(), &r, Role::Product, 0).unwrap();
    assert_eq!((c.p0, c.p3), (pt(200.0 + 40.0, 0.0), pt(400.0, 0.0)));
    // No stem: the junction, as before; and a Linear reaction has none.
    let mut plain = r.clone();
    plain.stem = None;
    assert_eq!(plain.leg_anchor(Role::Product), pt(200.0, 0.0));
    assert_eq!(stem_line(&plain), None);
    let mut linear = r1(&view());
    linear.mode = ReactionMode::Linear;
    assert_eq!(stem_line(&linear), None);
}

#[test]
fn the_stem_is_part_of_the_reaction_for_clicks() {
    let mut v = view();
    // Between the reactant's end of the stem and the junction.
    assert_eq!(v.target_at(pt(180.0, 0.0)), Target::Leg("R1".into()));
    // Without the stem, the straight line from A still reaches there, so
    // try off the line: the stem bends when its ends aren't in line.
    v.model.find_reaction_mut("R1").unwrap().stem = Some((pt(-40.0, -60.0), pt(40.0, 0.0)));
    assert_eq!(v.target_at(pt(180.0, -30.0)), Target::Leg("R1".into()));
    v.model.find_reaction_mut("R1").unwrap().stem = None;
    assert_eq!(v.target_at(pt(180.0, -30.0)), Target::Empty);
}

#[test]
fn moving_the_junction_carries_the_stem() {
    let mut v = view();
    drag(&mut v, pt(200.0, 0.0), pt(220.0, 50.0));
    let r = r1(&v);
    assert_eq!(r.junction, pt(220.0, 50.0));
    assert_eq!(r.stem, Some((pt(-40.0, 0.0), pt(40.0, 0.0))));
    assert_eq!(r.leg_anchor(Role::Reactant), pt(180.0, 50.0));
}

#[test]
fn reshaping_a_reaction_takes_its_stem_away() {
    let select = |v: &mut DiagramView| v.select_item(&Target::Leg("R1".into()), false);
    let mut v = view();
    select(&mut v);
    v.set_bezier();
    assert!(r1(&v).stem.is_some(), "Set Bézier keeps it");
    v.toggle_smooth();
    assert_eq!(r1(&v).stem, None, "a smooth junction has none");
    for cmd in [
        DiagramView::set_straight as fn(&mut DiagramView) -> ViewResponse,
        DiagramView::toggle_linear,
        DiagramView::make_nice,
    ] {
        let mut v = view();
        select(&mut v);
        cmd(&mut v);
        assert_eq!(r1(&v).stem, None);
        v.undo();
        assert!(r1(&v).stem.is_some(), "undo puts it back");
    }
}

#[test]
fn a_name_starts_clear_of_the_stem() {
    // The stem runs up and down through the junction, the legs go left and
    // right from its ends: the name goes beside the stem, to the right.
    let mut v = view();
    let r = v.model.find_reaction_mut("R1").unwrap();
    r.stem = Some((pt(0.0, -40.0), pt(0.0, 40.0)));
    let r = r1(&v);
    let o = clear_reaction_label_offset(v.model(), &r);
    assert!(o.x > 0.0 && o.y == 0.0, "{o:?}");
}

#[test]
fn the_file_keeps_the_stem_in_version_8() {
    let v = view();
    let text = crate::json::to_json(v.model());
    assert!(text.contains("\"version\": 8"));
    let back = crate::json::from_json(&text).unwrap();
    assert!(back.warnings.is_empty(), "{:?}", back.warnings);
    assert_eq!(back.model.find_reaction("R1").unwrap().stem, r1(&v).stem);
    // Without one, it isn't written and nothing needs version 8.
    let mut m = v.model().clone();
    m.find_reaction_mut("R1").unwrap().stem = None;
    let text = crate::json::to_json(&m);
    assert!(!text.contains("\"stem\""));
    assert!(!text.contains("\"version\": 8"));
    // Not four numbers: left out, with a warning.
    let bad = crate::json::to_json(v.model()).replace("\"stem\": [", "\"stem\": [1.0, ");
    let back = crate::json::from_json(&bad).unwrap();
    assert_eq!(back.model.find_reaction("R1").unwrap().stem, None);
    assert_eq!(back.warnings.len(), 1);
}

#[test]
fn the_escher_theme_keeps_a_stem_and_its_curves() {
    let mut v = view();
    {
        let r = v.model.find_reaction_mut("R1").unwrap();
        r.mode = ReactionMode::Bezier;
        let p = &mut r.reactants[0];
        (p.ctrl1, p.ctrl2, p.ctrl_pts_set) = (pt(40.0, -50.0), pt(120.0, 0.0), true);
    }
    let before = r1(&v);
    v.apply_theme(crate::themes::Theme::Escher);
    let r = r1(&v);
    assert_eq!(r.stem, before.stem);
    assert!(!r.junction_smooth);
    assert_eq!(r.reactants[0], before.reactants[0]);
    // A reaction with no stem is made smooth as before.
    let mut v = view();
    v.model.find_reaction_mut("R1").unwrap().stem = None;
    v.apply_theme(crate::themes::Theme::Escher);
    assert!(r1(&v).junction_smooth);
}

#[test]
fn sbml_legs_end_at_the_stem() {
    let v = view();
    let xml = crate::sbml::export(v.model()).xml;
    // The reactant's line ends at (160,0), the product's starts at (240,0).
    assert!(
        xml.contains(r#"<layout:end layout:x="160" layout:y="0""#),
        "{xml}"
    );
    assert!(
        xml.contains(r#"<layout:start layout:x="240" layout:y="0""#),
        "{xml}"
    );
}

fn sbml_back(m: &Model) -> Model {
    let opts = crate::antimony::ImportOptions {
        convert_catalytic: false,
        seed: 1,
    };
    let xml = crate::sbml::export(m).xml;
    crate::sbml::import(&xml, &opts).unwrap().model
}

fn close(a: Option<(Pt, Pt)>, b: Option<(Pt, Pt)>) -> bool {
    match (a, b) {
        (Some((a1, a2)), Some((b1, b2))) => a1.distance(b1) < 1e-6 && a2.distance(b2) < 1e-6,
        (None, None) => true,
        _ => false,
    }
}

#[test]
fn a_stem_is_the_sbml_reaction_glyph_s_curve() {
    let v = view();
    let xml = crate::sbml::export(v.model()).xml;
    // The glyph's own curve: (160,0) → (200,0) → (240,0).
    let curve = &xml[xml.find("<layout:reactionGlyph").unwrap()..];
    let curve = &curve[..curve.find("<layout:listOfSpeciesReferenceGlyphs").unwrap()];
    assert!(curve.contains("<layout:curve>"), "{curve}");
    assert!(curve.contains(r#"<layout:start layout:x="160" layout:y="0""#));
    assert!(curve.contains(r#"<layout:end layout:x="240" layout:y="0""#));
    // Back again: the same stem, straight or Bézier.
    let back = sbml_back(v.model());
    assert!(close(back.find_reaction("R1").unwrap().stem, r1(&v).stem));
    let mut m = v.model().clone();
    {
        let r = m.find_reaction_mut("R1").unwrap();
        r.mode = ReactionMode::Bezier;
        r.stem = Some((pt(-40.0, -30.0), pt(40.0, 20.0)));
        let p = &mut r.reactants[0];
        (p.ctrl1, p.ctrl2, p.ctrl_pts_set) = (pt(40.0, -40.0), pt(120.0, -50.0), true);
    }
    let back = sbml_back(&m);
    let r = back.find_reaction("R1").unwrap();
    assert!(
        close(r.stem, m.find_reaction("R1").unwrap().stem),
        "{:?}",
        r.stem
    );
    assert_eq!(r.mode, ReactionMode::Bezier);
    let p = &r.reactants[0];
    assert!(p.ctrl1.distance(pt(40.0, -40.0)) < 1e-6 && p.ctrl2.distance(pt(120.0, -50.0)) < 1e-6);
    // No stem, no curve, and none comes back.
    m.find_reaction_mut("R1").unwrap().stem = None;
    let xml = crate::sbml::export(&m).xml;
    let glyph = &xml[xml.find("<layout:reactionGlyph").unwrap()..];
    let glyph = &glyph[..glyph.find("<layout:listOfSpeciesReferenceGlyphs").unwrap()];
    assert!(!glyph.contains("<layout:curve>"));
    assert_eq!(sbml_back(&m).find_reaction("R1").unwrap().stem, None);
}

#[test]
fn another_tool_s_reaction_curve_is_read_either_way_round() {
    // Written from the products' end to the reactants': still the stem the
    // right way round.
    let v = view();
    let xml = crate::sbml::export(v.model())
        .xml
        .replace(
            r#"<layout:start layout:x="160" layout:y="0""#,
            r#"<layout:start layout:x="TMP" layout:y="0""#,
        )
        .replace(
            r#"<layout:end layout:x="240" layout:y="0""#,
            r#"<layout:end layout:x="160" layout:y="0""#,
        )
        .replace(
            r#"<layout:start layout:x="TMP" layout:y="0""#,
            r#"<layout:start layout:x="240" layout:y="0""#,
        );
    let opts = crate::antimony::ImportOptions {
        convert_catalytic: false,
        seed: 1,
    };
    let back = crate::sbml::import(&xml, &opts).unwrap().model;
    assert!(close(back.find_reaction("R1").unwrap().stem, r1(&v).stem));
}

#[test]
fn an_escher_map_survives_sbml_with_its_stems() {
    let map = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../Escher/e_coli_core.Core metabolism.json"
    ))
    .unwrap();
    let m = crate::escher::import(&map).unwrap().model;
    let with_stems = m.reactions().iter().filter(|r| r.stem.is_some()).count();
    assert!(with_stems > 50, "{with_stems}");
    let back = sbml_back(&m);
    for r in m.reactions() {
        let b = back.find_reaction(&r.id).unwrap();
        assert!(
            close(b.stem, r.stem),
            "{}: {:?} vs {:?}",
            r.id,
            b.stem,
            r.stem
        );
    }
}
