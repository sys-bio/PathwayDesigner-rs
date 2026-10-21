//! Corner points on modifier arcs (S §5.7.1): the route, hitting, dragging,
//! adding, removing, straightening, what carries them along, and files.
//! Driven through the view.

use super::*;
use crate::model::{ModifierKind, Participant, SpeciesNode};
use crate::render::draw::modifier_route;

/// A (0,0) → B (400,0), junction (200,0), with E (200,-200) as a modifier;
/// nodes 80 × 36. Screen = world.
fn view() -> DiagramView {
    let mut m = Model::new();
    for (id, x, y) in [("A", 0.0, 0.0), ("B", 400.0, 0.0), ("E", 200.0, -200.0)] {
        m.push_species(SpeciesNode::new(id, pt(x, y))).unwrap();
    }
    let mut r = crate::model::Reaction::new("R1", pt(200.0, 0.0));
    r.reactants = vec![Participant::new("A", 1.0)];
    r.products = vec![Participant::new("B", 1.0)];
    r.modifiers = vec![Participant::new("E", 1.0)];
    r.kinetic_law = "k*A*E".into();
    m.push_reaction(r).unwrap();
    DiagramView::new(m)
}

fn points(v: &DiagramView) -> Vec<Pt> {
    v.model().find_reaction("R1").unwrap().modifiers[0]
        .points
        .clone()
}

fn set_points(v: &mut DiagramView, pts: &[Pt]) {
    v.model.find_reaction_mut("R1").unwrap().modifiers[0].points = pts.to_vec();
}

fn press(v: &mut DiagramView, at: Pt, button: Button) -> ViewResponse {
    v.handle(&ViewEvent::PointerDown {
        pos: at,
        button,
        modifiers: Modifiers::NONE,
    })
}

fn drag(v: &mut DiagramView, from: Pt, to: Pt) {
    press(v, from, Button::Primary);
    v.handle(&ViewEvent::PointerMove { pos: to });
    v.handle(&ViewEvent::PointerUp {
        pos: to,
        button: Button::Primary,
    });
}

#[test]
fn an_arc_runs_through_its_corner_points() {
    let mut v = view();
    let route = |v: &DiagramView| {
        let r = v.model().find_reaction("R1").unwrap();
        modifier_route(v.model(), r, &r.modifiers[0]).unwrap()
    };
    // Straight: from E's bottom edge to the junction.
    assert_eq!(route(&v), [pt(200.0, -182.0), pt(200.0, 0.0)]);
    // It leaves E toward its first corner point, then runs through them all.
    set_points(&mut v, &[pt(360.0, -200.0), pt(360.0, 0.0)]);
    assert_eq!(
        route(&v),
        [
            pt(240.0, -200.0),
            pt(360.0, -200.0),
            pt(360.0, 0.0),
            pt(200.0, 0.0)
        ]
    );
}

#[test]
fn arcs_and_their_corner_points_are_hit() {
    let mut v = view();
    set_points(&mut v, &[pt(360.0, -200.0)]);
    // Each piece, counted from the node.
    assert_eq!(
        v.target_at(pt(300.0, -200.0)),
        Target::ModifierLine("R1".into(), 0, 0)
    );
    assert_eq!(
        v.target_at(pt(280.0, -100.0)),
        Target::ModifierLine("R1".into(), 0, 1)
    );
    // A click on the arc selects its reaction.
    press(&mut v, pt(300.0, -200.0), Button::Primary);
    v.handle(&ViewEvent::PointerUp {
        pos: pt(300.0, -200.0),
        button: Button::Primary,
    });
    assert!(v.model().find_reaction("R1").unwrap().selected);
    // Now its corner point can be grabbed.
    assert_eq!(
        v.target_at(pt(361.0, -199.0)),
        Target::CornerPoint("R1".into(), Role::Modifier, 0, 0)
    );
    // With modifier arcs hidden, neither is there.
    v.set_show_modifiers(false);
    assert_eq!(v.target_at(pt(300.0, -200.0)), Target::Empty);
    assert_eq!(v.target_at(pt(361.0, -199.0)), Target::Empty);
}

#[test]
fn corner_points_are_dragged_snapped_and_cancelled() {
    let mut v = view();
    set_points(&mut v, &[pt(360.0, -200.0)]);
    v.select_item(&Target::Leg("R1".into()), false);
    drag(&mut v, pt(360.0, -200.0), pt(380.0, -150.0));
    assert_eq!(points(&v), [pt(380.0, -150.0)]);
    assert_eq!(v.undo_description(), Some("Move Corner Point"));
    v.undo();
    assert_eq!(points(&v), [pt(360.0, -200.0)]);
    // Snapping: the point lands on the grid. (Undo clears the selection.)
    v.select_item(&Target::Leg("R1".into()), false);
    v.snap_to_grid = true;
    drag(&mut v, pt(360.0, -200.0), pt(372.0, -187.0));
    assert_eq!(points(&v), [pt(380.0, -180.0)]);
    v.snap_to_grid = false;
    // Esc puts it back, with nothing to undo.
    let before = v.undo_description();
    press(&mut v, pt(380.0, -180.0), Button::Primary);
    v.handle(&ViewEvent::PointerMove {
        pos: pt(500.0, -300.0),
    });
    assert_eq!(points(&v), [pt(500.0, -300.0)]);
    v.handle(&ViewEvent::Cancel);
    assert_eq!(points(&v), [pt(380.0, -180.0)]);
    assert_eq!(v.undo_description(), before);
}

#[test]
fn corner_points_are_added_removed_and_straightened() {
    let mut v = view();
    // Added on the piece clicked: the first piece, the last, then between.
    v.add_corner_point("R1", (Role::Modifier, 0), 0, pt(360.0, -200.0));
    assert_eq!(v.undo_description(), Some("Add Corner Point"));
    v.add_corner_point("R1", (Role::Modifier, 0), 1, pt(360.0, 0.0));
    v.add_corner_point("R1", (Role::Modifier, 0), 1, pt(380.0, -100.0));
    assert_eq!(
        points(&v),
        [pt(360.0, -200.0), pt(380.0, -100.0), pt(360.0, 0.0)]
    );
    assert!(v.line_bent("R1", (Role::Modifier, 0)));
    v.remove_corner_point("R1", (Role::Modifier, 0), 1);
    assert_eq!(v.undo_description(), Some("Remove Corner Point"));
    assert_eq!(points(&v), [pt(360.0, -200.0), pt(360.0, 0.0)]);
    v.straighten_line("R1", (Role::Modifier, 0));
    assert_eq!(v.undo_description(), Some("Straighten Modifier Line"));
    assert!(points(&v).is_empty());
    assert!(!v.line_bent("R1", (Role::Modifier, 0)));
    v.undo();
    assert_eq!(points(&v), [pt(360.0, -200.0), pt(360.0, 0.0)]);
    // Nothing to straighten: no undo step.
    let mut w = view();
    w.straighten_line("R1", (Role::Modifier, 0));
    assert_eq!(w.undo_description(), None);
    // A double-click on the arc adds one where it was clicked.
    w.handle(&ViewEvent::DoubleClick {
        pos: pt(200.0, -100.0),
    });
    assert_eq!(points(&w), [pt(200.0, -100.0)]);
}

#[test]
fn corner_points_move_only_with_both_ends() {
    let mut v = view();
    set_points(&mut v, &[pt(360.0, -200.0)]);
    // E alone: they stay.
    v.select_item(&Target::Species("E".into()), false);
    drag(&mut v, pt(200.0, -200.0), pt(220.0, -200.0));
    assert_eq!(points(&v), [pt(360.0, -200.0)]);
    // E with the reaction's species, so the junction moves too: they follow.
    for id in ["A", "B"] {
        v.select_item(&Target::Species(id.into()), true);
    }
    drag(&mut v, pt(0.0, 0.0), pt(10.0, 20.0));
    assert_eq!(points(&v), [pt(370.0, -180.0)]);
    v.undo();
    assert_eq!(points(&v), [pt(360.0, -200.0)]);
    // The arrow keys too.
    for id in ["A", "B", "E"] {
        v.select_item(&Target::Species(id.into()), true);
    }
    v.nudge(pt(5.0, 0.0));
    assert_eq!(points(&v), [pt(365.0, -200.0)]);
    // The junction dragged alone leaves them.
    drag(&mut v, pt(205.0, 0.0), pt(205.0, 40.0));
    assert_eq!(points(&v), [pt(365.0, -200.0)]);
}

#[test]
fn a_right_click_on_an_arc_asks_for_its_menu() {
    let mut v = view();
    set_points(&mut v, &[pt(360.0, -200.0)]);
    let r = press(&mut v, pt(300.0, -200.0), Button::Secondary);
    assert_eq!(
        r.request,
        Some(Request::ContextMenu(Target::ModifierLine(
            "R1".into(),
            0,
            0
        )))
    );
    assert!(v.model().find_reaction("R1").unwrap().selected);
    assert_eq!(v.context_point(), Some(pt(300.0, -200.0)));
    // On a corner point of the selected reaction.
    let r = press(&mut v, pt(360.0, -200.0), Button::Secondary);
    assert_eq!(
        r.request,
        Some(Request::ContextMenu(Target::CornerPoint(
            "R1".into(),
            Role::Modifier,
            0,
            0
        )))
    );
}

#[test]
fn the_rate_law_keeps_corner_points() {
    let mut v = view();
    set_points(&mut v, &[pt(360.0, -200.0)]);
    v.set_kinetic_law("R1", "k2*A*E");
    assert_eq!(points(&v), [pt(360.0, -200.0)]);
}

#[test]
fn files_keep_corner_points() {
    use crate::json::{from_json, needed_version, to_json};
    let mut v = view();
    set_points(&mut v, &[pt(360.0, -200.0), pt(360.5, 0.25)]);
    assert_eq!(needed_version(v.model()), 11);
    let text = to_json(v.model());
    let json: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(
        json["reactions"][0]["modifiers"][0]["points"],
        serde_json::json!([360.0, -200.0, 360.5, 0.25])
    );
    assert!(json["reactions"][0]["reactants"][0].get("points").is_none());
    let back = from_json(&text).unwrap().model;
    assert_eq!(
        back.reactions()[0].modifiers[0].points,
        [pt(360.0, -200.0), pt(360.5, 0.25)]
    );
    // An odd number leaves the last out.
    let odd = text.replace("0.25\n", "0.25,\n 7\n");
    let odd = from_json(&odd).unwrap().model;
    assert_eq!(odd.reactions()[0].modifiers[0].points.len(), 2);
    // None: not written, and an older version.
    set_points(&mut v, &[]);
    assert!(!to_json(v.model()).contains("\"points\""));
    assert!(needed_version(v.model()) < 11);
}

#[test]
fn sbml_keeps_corner_points_and_kinds() {
    let opts = crate::antimony::ImportOptions {
        convert_catalytic: false,
        seed: 1,
    };
    let mut v = view();
    set_points(&mut v, &[pt(360.0, -200.0), pt(360.0, 0.0)]);
    v.model.find_reaction_mut("R1").unwrap().modifiers[0].kind = ModifierKind::Inhibitor;
    let xml = crate::sbml::export(v.model()).xml;
    let back = crate::sbml::import(&xml, &opts).unwrap().model;
    let m = &back.find_reaction("R1").unwrap().modifiers[0];
    assert_eq!(m.points, [pt(360.0, -200.0), pt(360.0, 0.0)]);
    assert_eq!(m.kind, ModifierKind::Inhibitor);
    // A curve another tool wrote from the junction end is turned round.
    let s = xml.find("srg_R1_mod_E").unwrap();
    let a = xml[s..].find("<layout:listOfCurveSegments>").unwrap() + s;
    let b = xml[a..].find("</layout:listOfCurveSegments>").unwrap() + a;
    let segs = &xml[a + "<layout:listOfCurveSegments>".len()..b];
    let pieces: Vec<&str> = segs
        .split("</layout:curveSegment>")
        .filter(|p| p.contains("curveSegment"))
        .collect();
    assert_eq!(pieces.len(), 3);
    let swap = |p: &str| {
        p.replace("layout:start", "layout:TMP")
            .replace("layout:end", "layout:start")
            .replace("layout:TMP", "layout:end")
    };
    let reversed: String = pieces
        .iter()
        .rev()
        .map(|p| format!("{}</layout:curveSegment>", swap(p)))
        .collect();
    let xml2 = format!(
        "{}{reversed}{}",
        &xml[..a + "<layout:listOfCurveSegments>".len()],
        &xml[b..]
    );
    let back = crate::sbml::import(&xml2, &opts).unwrap().model;
    assert_eq!(
        back.find_reaction("R1").unwrap().modifiers[0].points,
        [pt(360.0, -200.0), pt(360.0, 0.0)]
    );
}

#[test]
fn the_arrow_keys_move_the_current_corner_point() {
    let mut v = view();
    set_points(&mut v, &[pt(360.0, -200.0), pt(360.0, -20.0)]);
    v.select_item(&Target::Leg("R1".into()), false);
    assert_eq!(v.current_corner_point(), None, "none until one is clicked");
    // A click on the second point makes it current.
    press(&mut v, pt(360.0, -20.0), Button::Primary);
    v.handle(&ViewEvent::PointerUp {
        pos: pt(360.0, -20.0),
        button: Button::Primary,
    });
    assert_eq!(v.current_corner_point(), Some(pt(360.0, -20.0)));
    let junction = v.model().find_reaction("R1").unwrap().junction;
    // 1 unit, then 10 with Shift; the reaction stays put.
    v.nudge_key(pt(1.0, 0.0), false, false);
    v.nudge_key(pt(0.0, -1.0), true, false);
    assert_eq!(points(&v), [pt(360.0, -200.0), pt(361.0, -30.0)]);
    assert_eq!(v.model().find_reaction("R1").unwrap().junction, junction);
    assert_eq!(v.current_corner_point(), Some(pt(361.0, -30.0)));
    // One undo step for the run; undoing lets the point go.
    assert_eq!(v.undo_description(), Some("Move Corner Point"));
    v.undo();
    assert_eq!(points(&v), [pt(360.0, -200.0), pt(360.0, -20.0)]);
    assert_eq!(v.current_corner_point(), None);
    // While snapping: to the next grid line, five with Shift; Alt is free.
    v.select_item(&Target::Leg("R1".into()), false);
    press(&mut v, pt(360.0, -20.0), Button::Primary);
    v.handle(&ViewEvent::PointerUp {
        pos: pt(360.0, -20.0),
        button: Button::Primary,
    });
    v.snap_to_grid = true;
    set_points(&mut v, &[pt(360.0, -200.0), pt(363.0, -20.0)]);
    v.nudge_key(pt(1.0, 0.0), false, false);
    assert_eq!(points(&v)[1], pt(380.0, -20.0));
    v.nudge_key(pt(0.0, 1.0), true, false);
    assert_eq!(points(&v)[1], pt(380.0, 80.0));
    v.nudge_key(pt(-1.0, 0.0), false, true);
    assert_eq!(points(&v)[1], pt(379.0, 80.0));
    v.snap_to_grid = false;
    // A click elsewhere lets it go: the keys move the selection again.
    press(&mut v, pt(0.0, 0.0), Button::Primary);
    v.handle(&ViewEvent::PointerUp {
        pos: pt(0.0, 0.0),
        button: Button::Primary,
    });
    assert_eq!(v.current_corner_point(), None);
    v.nudge_key(pt(1.0, 0.0), false, false);
    assert_eq!(v.model().find_species("A").unwrap().center, pt(1.0, 0.0));
    assert_eq!(points(&v)[1], pt(379.0, 80.0));
}

#[test]
fn the_current_corner_point_is_let_go() {
    let make = || {
        let mut v = view();
        set_points(&mut v, &[pt(360.0, -200.0)]);
        v.select_item(&Target::Leg("R1".into()), false);
        drag(&mut v, pt(360.0, -200.0), pt(360.0, -180.0));
        assert_eq!(
            v.current_corner_point(),
            Some(pt(360.0, -180.0)),
            "a drag too"
        );
        v
    };
    // Esc.
    let mut v = make();
    v.handle(&ViewEvent::Cancel);
    assert_eq!(v.current_corner_point(), None);
    // Another selection.
    let mut v = make();
    v.select_item(&Target::Species("A".into()), false);
    assert_eq!(v.current_corner_point(), None);
    // The reaction no longer selected.
    let mut v = make();
    v.model.find_reaction_mut("R1").unwrap().selected = false;
    assert_eq!(v.current_corner_point(), None);
    // Arcs hidden.
    let mut v = make();
    v.set_show_modifiers(false);
    assert_eq!(v.current_corner_point(), None);
    // Points added, removed or straightened away.
    let mut v = make();
    v.add_corner_point("R1", (Role::Modifier, 0), 0, pt(300.0, -200.0));
    assert_eq!(v.current_corner_point(), None);
    let mut v = make();
    v.remove_corner_point("R1", (Role::Modifier, 0), 0);
    assert_eq!(v.current_corner_point(), None);
    let mut v = make();
    v.straighten_line("R1", (Role::Modifier, 0));
    assert_eq!(v.current_corner_point(), None);
}

#[test]
fn removing_another_point_lets_the_current_one_go_too() {
    let mut v = view();
    set_points(&mut v, &[pt(360.0, -200.0), pt(360.0, -20.0)]);
    v.select_item(&Target::Leg("R1".into()), false);
    drag(&mut v, pt(360.0, -200.0), pt(360.0, -190.0));
    assert_eq!(v.current_corner_point(), Some(pt(360.0, -190.0)));
    v.remove_corner_point("R1", (Role::Modifier, 0), 1);
    assert_eq!(v.current_corner_point(), None);
}

/// Counts the squares the screen drawing fills with `fill`.
struct Squares {
    fill: crate::model::Rgba,
    count: usize,
}

impl crate::render::Renderer for Squares {
    fn round_rect(
        &mut self,
        _: crate::render::SRect,
        _: f32,
        fill: Option<crate::model::Rgba>,
        _: Option<crate::render::Stroke>,
    ) {
        self.count += usize::from(fill == Some(self.fill));
    }
    fn circle(
        &mut self,
        _: crate::render::SPt,
        _: f32,
        _: Option<crate::model::Rgba>,
        _: Option<crate::render::Stroke>,
    ) {
    }
    fn polyline(&mut self, _: &[crate::render::SPt], _: crate::render::Stroke) {}
    fn cubic(
        &mut self,
        _: crate::render::SPt,
        _: crate::render::SPt,
        _: crate::render::SPt,
        _: crate::render::SPt,
        _: crate::render::Stroke,
    ) {
    }
    fn polygon(&mut self, _: &[crate::render::SPt], _: crate::model::Rgba) {}
    fn text(
        &mut self,
        _: crate::render::SPt,
        _: &str,
        _: f32,
        _: crate::model::TextFont,
        _: crate::model::Rgba,
    ) {
    }
}

#[test]
fn the_screen_shows_the_current_corner_point_filled() {
    let mut v = view();
    set_points(&mut v, &[pt(360.0, -200.0)]);
    v.select_item(&Target::Leg("R1".into()), false);
    let filled = |v: &DiagramView| {
        let mut s = Squares {
            fill: v.selected_reaction_colour,
            count: 0,
        };
        let area = crate::render::SRect {
            min: crate::render::spt(0.0, 0.0),
            max: crate::render::spt(800.0, 600.0),
        };
        v.draw(&mut s, area);
        s.count
    };
    assert_eq!(filled(&v), 0);
    drag(&mut v, pt(360.0, -200.0), pt(360.0, -190.0));
    assert_eq!(filled(&v), 1);
}

#[test]
fn a_modifier_line_has_its_own_look() {
    use crate::model::{LinePattern, ModifierLook, Rgba};
    let mut v = view();
    let look = |v: &DiagramView| v.modifier_look("R1", 0).unwrap();
    assert!(look(&v).is_default());
    let red = Rgba::rgb(200, 0, 0);
    v.set_modifier_colour("R1", 0, Some(red));
    assert_eq!(v.undo_description(), Some("Modifier Line Colour"));
    v.set_modifier_width("R1", 0, Some(3.0));
    assert_eq!(v.undo_description(), Some("Modifier Line Width"));
    v.set_modifier_pattern("R1", 0, LinePattern::Dotted);
    assert_eq!(v.undo_description(), Some("Modifier Line Pattern"));
    v.set_modifier_symbol_size("R1", 0, Some(20.0));
    assert_eq!(v.undo_description(), Some("Modifier Symbol Size"));
    v.set_modifier_symbol_gap("R1", 0, Some(12.0));
    assert_eq!(v.undo_description(), Some("Modifier Symbol Distance"));
    assert_eq!(
        look(&v),
        ModifierLook {
            colour: Some(red),
            width: Some(3.0),
            pattern: LinePattern::Dotted,
            symbol_size: Some(20.0),
            symbol_gap: Some(12.0),
        }
    );
    // Kept within their ranges; None puts the default back.
    v.set_modifier_width("R1", 0, Some(500.0));
    assert_eq!(look(&v).width, Some(20.0));
    v.set_modifier_symbol_size("R1", 0, Some(0.1));
    assert_eq!(look(&v).symbol_size, Some(2.0));
    v.set_modifier_symbol_gap("R1", 0, Some(-5.0));
    assert_eq!(look(&v).symbol_gap, Some(0.0));
    v.set_modifier_colour("R1", 0, None);
    assert_eq!(look(&v).colour, None);
    // Undo, one setting at a time.
    v.undo();
    assert_eq!(look(&v).colour, Some(red));
    // The same value again is no undo step.
    let before = v.undo_description();
    v.set_modifier_pattern("R1", 0, LinePattern::Dotted);
    assert_eq!(v.undo_description(), before);
}

#[test]
fn apply_to_all_copies_the_look_but_not_the_kind_or_points() {
    use crate::model::{LinePattern, Rgba};
    let mut v = view();
    v.model
        .push_species(SpeciesNode::new("F", pt(100.0, -200.0)))
        .unwrap();
    {
        let r = v.model.find_reaction_mut("R1").unwrap();
        let mut f = Participant::new("F", 1.0);
        f.kind = ModifierKind::Activator;
        f.points = vec![pt(100.0, -100.0)];
        r.modifiers.push(f);
    }
    v.set_modifier_colour("R1", 0, Some(Rgba::rgb(1, 2, 3)));
    v.set_modifier_pattern("R1", 0, LinePattern::Solid);
    v.apply_modifier_look_to_all("R1", 0);
    assert_eq!(v.undo_description(), Some("Apply to All Modifier Lines"));
    let r = v.model().find_reaction("R1").unwrap();
    assert_eq!(r.modifiers[1].look, r.modifiers[0].look);
    assert_eq!(r.modifiers[1].kind, ModifierKind::Activator);
    assert_eq!(r.modifiers[1].points, [pt(100.0, -100.0)]);
    v.undo();
    assert!(v.modifier_look("R1", 1).unwrap().is_default());
}

#[test]
fn styles_leave_a_modifier_line_s_look_alone() {
    use crate::model::Rgba;
    let mut v = view();
    v.set_modifier_colour("R1", 0, Some(Rgba::rgb(9, 9, 9)));
    v.set_modifier_width("R1", 0, Some(4.0));
    v.apply_theme(crate::themes::Theme::Escher);
    v.apply_theme(crate::themes::Theme::Classic);
    let look = v.modifier_look("R1", 0).unwrap();
    assert_eq!(look.colour, Some(Rgba::rgb(9, 9, 9)));
    assert_eq!(look.width, Some(4.0));
}

#[test]
fn files_keep_a_modifier_line_s_look_in_version_12() {
    use crate::json::{from_json, needed_version, to_json};
    use crate::model::{LinePattern, Rgba};
    let mut v = view();
    assert!(needed_version(v.model()) < 11);
    v.set_modifier_symbol_gap("R1", 0, Some(14.0));
    assert_eq!(needed_version(v.model()), 12);
    v.set_modifier_colour("R1", 0, Some(Rgba::rgb(0x12, 0x34, 0x56)));
    v.set_modifier_width("R1", 0, Some(2.5));
    v.set_modifier_pattern("R1", 0, LinePattern::Solid);
    v.set_modifier_symbol_size("R1", 0, Some(9.0));
    let text = to_json(v.model());
    let json: serde_json::Value = serde_json::from_str(&text).unwrap();
    let m = &json["reactions"][0]["modifiers"][0];
    assert_eq!(json["version"], 12);
    assert_eq!(m["lineColor"], "#123456FF");
    assert_eq!(m["lineWidth"], 2.5);
    assert_eq!(m["pattern"], "solid");
    assert_eq!(m["symbolSize"], 9.0);
    assert_eq!(m["symbolGap"], 14.0);
    let back = from_json(&text).unwrap().model;
    assert_eq!(
        back.reactions()[0].modifiers[0].look,
        v.modifier_look("R1", 0).unwrap()
    );
    // Defaults are left out; nonsense reads as the default.
    let plain = to_json(view().model());
    assert!(!plain.contains("lineColor") && !plain.contains("symbolGap"));
    let odd = text
        .replace("\"#123456FF\"", "\"glow\"")
        .replace("\"solid\"", "\"wavy\"")
        .replace("\"lineWidth\": 2.5", "\"lineWidth\": -1.0");
    let look = from_json(&odd).unwrap().model.reactions()[0].modifiers[0].look;
    assert_eq!(look.colour, None);
    assert_eq!(look.pattern, LinePattern::Dashed);
    assert_eq!(look.width, None);
}

#[test]
fn sbml_keeps_a_modifier_line_s_look() {
    use crate::model::{LinePattern, Rgba};
    let opts = crate::antimony::ImportOptions {
        convert_catalytic: false,
        seed: 1,
    };
    let mut v = view();
    // All default: no note at all.
    let xml = crate::sbml::export(v.model()).xml;
    assert!(!xml.contains("pd:modifier"));
    v.set_modifier_colour("R1", 0, Some(Rgba::rgb(200, 0, 0)));
    v.set_modifier_width("R1", 0, Some(3.0));
    v.set_modifier_pattern("R1", 0, LinePattern::Dotted);
    v.set_modifier_symbol_size("R1", 0, Some(30.0));
    v.set_modifier_symbol_gap("R1", 0, Some(12.5));
    let xml = crate::sbml::export(v.model()).xml;
    assert_eq!(xml.matches("<pd:modifier").count(), 1);
    let back = crate::sbml::import(&xml, &opts).unwrap().model;
    assert_eq!(
        back.find_reaction("R1").unwrap().modifiers[0].look,
        v.modifier_look("R1", 0).unwrap()
    );
}

#[test]
fn hidden_modifier_lines_are_saved_with_the_diagram() {
    use crate::json::{from_json, needed_version, to_json};
    let mut v = view();
    assert!(v.show_modifiers());
    assert!(!to_json(v.model()).contains("modifierLines"));
    // Hiding them is an edit of the diagram: one undo step.
    v.set_show_modifiers(false);
    assert!(!v.show_modifiers());
    assert_eq!(v.undo_description(), Some("Hide Modifier Lines"));
    assert_eq!(needed_version(v.model()), 13);
    let text = to_json(v.model());
    let json: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(json["modifierLines"], false);
    assert_eq!(json["version"], 13);
    // Opening the file keeps them hidden.
    let back = DiagramView::new(from_json(&text).unwrap().model);
    assert!(!back.show_modifiers());
    // Hidden lines can't be clicked.
    assert_eq!(v.target_at(pt(200.0, -100.0)), Target::Empty);
    v.undo();
    assert!(v.show_modifiers());
    assert_eq!(
        v.target_at(pt(200.0, -100.0)),
        Target::ModifierLine("R1".into(), 0, 0)
    );
    v.set_show_modifiers(true);
    assert_eq!(v.undo_description(), None, "already shown: no step");
}
