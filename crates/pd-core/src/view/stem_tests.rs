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
