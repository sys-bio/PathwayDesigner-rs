//! Themes (S §8.7): Escher and Classic, applied to every item, the items
//! added afterwards, the file, and where names outside a node start.

use super::*;
use crate::model::{
    BorderDash, Compartment, Frame, NodeShape, Participant, ReactionMode, Rgba, TextItem, Typeface,
};
use crate::render::draw::{clear_label_offset, default_label_offset};
use crate::themes::{
    ESCHER_COFACTOR, ESCHER_COMPARTMENT, ESCHER_METABOLITE, ESCHER_REACTION, Theme, is_cofactor,
};

/// Glc + ATP → G6P + ADP (R1), a null node, a compartment `cell` around
/// them, and a text T1 in serif red.
fn view() -> DiagramView {
    let mut m = Model::new();
    for (id, x, y) in [
        ("Glc", 0.0, 0.0),
        ("ATP", 0.0, 100.0),
        ("G6P", 200.0, 0.0),
        ("ADP", 200.0, 100.0),
    ] {
        m.push_species(crate::model::SpeciesNode::new(id, pt(x, y)))
            .unwrap();
    }
    let mut null = crate::model::SpeciesNode::new("_null", pt(0.0, 300.0));
    null.is_null_node = true;
    m.push_species(null).unwrap();
    let mut r = crate::model::Reaction::new("R1", pt(100.0, 50.0));
    r.reactants = vec![Participant::new("Glc", 1.0), Participant::new("ATP", 1.0)];
    r.products = vec![Participant::new("G6P", 1.0), Participant::new("ADP", 1.0)];
    m.push_reaction(r).unwrap();
    let mut c = Compartment::new("cell");
    c.frame = Some(Frame {
        center: pt(100.0, 50.0),
        width: 500.0,
        height: 400.0,
    });
    m.compartments.push(c);
    let mut t = TextItem::new("T1", pt(-50.0, -100.0), "Title");
    t.font.typeface = Typeface::Serif;
    t.color = Rgba::rgb(0xCC, 0, 0);
    t.size = 24.0;
    m.push_text(t).unwrap();
    DiagramView::new(m)
}

fn rx(v: &DiagramView, id: &str) -> crate::model::Reaction {
    v.model().find_reaction(id).unwrap().clone()
}

fn sp<'a>(v: &'a DiagramView, id: &str) -> &'a crate::model::SpeciesNode {
    v.model().find_species(id).unwrap()
}

#[test]
fn cofactors_are_known_by_their_names() {
    for id in [
        "ATP", "adp", "NADH", "NAD+", "nadph", "H2O", "Pi", "CO2", "CoA", "Hin", "MgATP", "atp_c",
        "M_atp_c", "M_h_e", "M_nadh_c", "M_q8h2_c",
    ] {
        assert!(is_cofactor(id), "{id}");
    }
    for id in [
        "Glc",
        "G6P",
        "P",
        "M_glc__D_e",
        "M_accoa_c",
        "PYR",
        "M_pyr_c",
        "S1",
        "ATPase",
    ] {
        assert!(!is_cofactor(id), "{id}");
    }
}

#[test]
fn escher_restyles_every_item_in_one_step() {
    let mut v = view();
    // A species coloured by hand takes the theme too.
    v.model.find_species_mut("Glc").unwrap().style = crate::model::VisualStyle {
        has_custom_style: true,
        fill_color: Rgba::rgb(0, 0xCC, 0),
        ..Default::default()
    };
    let before = v.model().clone();
    v.apply_theme(Theme::Escher);
    assert_eq!(v.undo_description(), Some("Escher Theme"));
    assert_eq!(v.theme(), Some(Theme::Escher));
    for (id, style, d) in [
        ("Glc", ESCHER_METABOLITE, 26.0),
        ("G6P", ESCHER_METABOLITE, 26.0),
        ("ATP", ESCHER_COFACTOR, 16.0),
        ("ADP", ESCHER_COFACTOR, 16.0),
    ] {
        let s = sp(&v, id);
        assert_eq!(s.style_name.as_deref(), Some(style), "{id}");
        assert_eq!(
            (s.shape, s.width, s.height),
            (NodeShape::Circle, d, d),
            "{id}"
        );
        assert_eq!(s.border_dash, BorderDash::Solid);
        assert!(s.label_offset.is_some(), "{id}: name outside");
    }
    assert_ne!(
        sp(&v, "Glc").style.fill_color,
        Rgba::rgb(0, 0xCC, 0),
        "the hand colour is gone"
    );
    assert!(
        sp(&v, "_null").style_name.is_none(),
        "a null node has no look"
    );
    let r = v.model().find_reaction("R1").unwrap();
    assert_eq!(r.style_name.as_deref(), Some(ESCHER_REACTION));
    assert!(!r.style.junction_visible);
    assert!(r.style.line_width > crate::render::constants::DEFAULT_LINE_WIDTH);
    let c = v.model().find_compartment("cell").unwrap();
    assert_eq!(c.style_name.as_deref(), Some(ESCHER_COMPARTMENT));
    // A text keeps its size, and takes the theme's typeface and colour.
    let t = v.model().find_text("T1").unwrap();
    assert_eq!((t.font.typeface, t.size), (Typeface::Sans, 24.0));
    assert_ne!(t.color, Rgba::rgb(0xCC, 0, 0));
    // The theme's styles are the model's: the Styles tab and file have them.
    for name in [
        ESCHER_METABOLITE,
        ESCHER_COFACTOR,
        ESCHER_REACTION,
        ESCHER_COMPARTMENT,
    ] {
        assert!(v.model().styles.iter().any(|s| s.name == name), "{name}");
    }
    // One undo puts everything back.
    v.undo();
    let mut back = v.model().clone();
    back.clear_selection();
    let mut want = before;
    want.clear_selection();
    assert_eq!(back, want);
}

#[test]
fn a_name_already_outside_keeps_its_place() {
    // Above the node, 22 from its top edge; the node shrinks from 36 to 26
    // high, and the name stays 22 above the edge (S §5.11).
    let mut v = view();
    v.model.find_species_mut("G6P").unwrap().label_offset = Some(pt(0.0, -40.0));
    v.apply_theme(Theme::Escher);
    assert_eq!(sp(&v, "G6P").label_offset, Some(pt(0.0, -35.0)));
}

#[test]
fn items_added_under_a_theme_take_its_look() {
    let mut v = view();
    v.apply_theme(Theme::Escher);
    // A species from the Species tool.
    v.set_tool(Tool::AddSpecies);
    v.handle(&ViewEvent::PointerDown {
        pos: pt(400.0, 0.0),
        button: Button::Primary,
        modifiers: Modifiers::NONE,
    });
    let new = v.model().species().last().unwrap();
    assert_eq!(new.style_name.as_deref(), Some(ESCHER_METABOLITE));
    assert_eq!(new.shape, NodeShape::Circle);
    // An alias of a cofactor is a cofactor.
    v.set_tool(Tool::Select);
    v.create_alias("ATP");
    let alias = v.model().species().last().unwrap();
    assert!(alias.is_alias());
    assert_eq!(alias.style_name.as_deref(), Some(ESCHER_COFACTOR));
    // A reaction drawn with a reaction tool.
    v.set_tool(Tool::UNI_UNI);
    for at in [pt(0.0, 0.0), pt(200.0, 0.0)] {
        v.handle(&ViewEvent::PointerDown {
            pos: at,
            button: Button::Primary,
            modifiers: Modifiers::NONE,
        });
    }
    let r = v.model().reactions().last().unwrap();
    assert_ne!(r.id, "R1");
    assert_eq!(r.style_name.as_deref(), Some(ESCHER_REACTION));
    // A renamed species restyled by hand isn't themed again.
    v.set_tool(Tool::Select);
    v.select_item(&Target::Species("G6P".into()), false);
    v.apply_colour(ColourField::Fill, Rgba::rgb(0, 0, 0xCC));
    v.rename("G6P", "glucose6p").unwrap();
    assert_eq!(sp(&v, "glucose6p").style.fill_color, Rgba::rgb(0, 0, 0xCC));
    assert_eq!(sp(&v, "glucose6p").style_name, None);
}

#[test]
fn escher_draws_reactions_as_smooth_curves() {
    let mut v = view();
    let mut m = v.model().clone();
    // R2: Glc → G6P drawn as one straight line, its junction left elsewhere.
    let mut r2 = crate::model::Reaction::new("R2", pt(900.0, 900.0));
    r2.mode = ReactionMode::Linear;
    r2.reactants = vec![Participant::new("Glc", 1.0)];
    r2.products = vec![Participant::new("G6P", 1.0)];
    m.push_reaction(r2).unwrap();
    // R3: ATP → ADP, already a Bézier, with handles dragged by hand.
    let mut r3 = crate::model::Reaction::new("R3", pt(0.0, 50.0));
    r3.mode = ReactionMode::Bezier;
    let mut atp = Participant::new("ATP", 1.0);
    (atp.ctrl1, atp.ctrl2, atp.ctrl_pts_set) = (pt(-40.0, 70.0), pt(-20.0, 60.0), true);
    r3.reactants = vec![atp.clone()];
    r3.products = vec![Participant::new("ADP", 1.0)];
    m.push_reaction(r3).unwrap();
    v.set_model(m);
    v.apply_theme(Theme::Escher);
    for id in ["R1", "R2", "R3"] {
        let r = v.model().find_reaction(id).unwrap();
        assert_eq!(
            (r.mode, r.junction_smooth),
            (ReactionMode::Bezier, true),
            "{id}"
        );
    }
    assert_eq!(
        rx(&v, "R1").junction,
        pt(100.0, 50.0),
        "a junction in use stays"
    );
    assert_eq!(
        rx(&v, "R2").junction,
        pt(100.0, 0.0),
        "halfway, as Set Straight does"
    );
    // Every leg has the smooth junction's handles, those placed by hand too.
    for id in ["R1", "R2", "R3"] {
        let r = rx(&v, id);
        let smooth = crate::bezier::smooth_handles(v.model(), &r);
        for (role, i, h) in smooth {
            let p = &r.participants(role)[i];
            assert_eq!(Some((p.ctrl1, p.ctrl2)), h, "{id}");
            assert!(p.ctrl_pts_set);
        }
    }
    assert_ne!(rx(&v, "R3").reactants[0], atp);
    // Undo puts the straight lines back.
    v.undo();
    assert_eq!(rx(&v, "R1").mode, ReactionMode::Straight);
    assert_eq!(rx(&v, "R2").mode, ReactionMode::Linear);
    // A reaction drawn later under the theme is curved too.
    v.redo();
    v.set_tool(Tool::UNI_UNI);
    for at in [pt(0.0, 0.0), pt(200.0, 100.0)] {
        v.handle(&ViewEvent::PointerDown {
            pos: at,
            button: Button::Primary,
            modifiers: Modifiers::NONE,
        });
    }
    let new = v.model().reactions().last().unwrap();
    assert_eq!(
        (new.mode, new.junction_smooth),
        (ReactionMode::Bezier, true)
    );
    // Classic leaves the curves alone: a reaction's shape isn't part of its look.
    v.apply_theme(Theme::Classic);
    assert_eq!(rx(&v, "R1").mode, ReactionMode::Bezier);
}

#[test]
fn classic_puts_back_the_usual_look() {
    let mut v = view();
    v.apply_theme(Theme::Escher);
    v.apply_theme(Theme::Classic);
    assert_eq!(v.undo_description(), Some("Classic Theme"));
    assert_eq!(v.theme(), None);
    for id in ["Glc", "ATP", "G6P", "ADP"] {
        let s = sp(&v, id);
        assert_eq!(s.style_name, None);
        assert!(!s.style.has_custom_style);
        assert_eq!(
            (s.shape, s.border_dash),
            (NodeShape::RoundedRect, BorderDash::Automatic)
        );
        assert_eq!(s.label_offset, None);
        assert_eq!((s.width, s.height), (80.0, 36.0));
    }
    let r = v.model().find_reaction("R1").unwrap();
    assert_eq!((r.style, r.style_name.clone()), (Default::default(), None));
    let t = v.model().find_text("T1").unwrap();
    assert_eq!((t.color, t.size), (Rgba::DEFAULT, 24.0));
    assert!(v.model().styles.is_empty(), "Escher's styles are gone");
    // A long name gets a node wide enough for it.
    v.rename("Glc", "glucose_6_phosphate_long_name").unwrap();
    v.apply_theme(Theme::Escher);
    v.apply_theme(Theme::Classic);
    assert!(sp(&v, "glucose_6_phosphate_long_name").width > 80.0);
    // Items added now look as usual.
    v.set_tool(Tool::AddSpecies);
    v.handle(&ViewEvent::PointerDown {
        pos: pt(400.0, 0.0),
        button: Button::Primary,
        modifiers: Modifiers::NONE,
    });
    let new = v.model().species().last().unwrap();
    assert_eq!(
        (new.style_name.clone(), new.shape),
        (None, NodeShape::RoundedRect)
    );
}

#[test]
fn the_file_keeps_the_theme() {
    let mut v = view();
    let text = crate::json::to_json(v.model());
    assert!(!text.contains("\"theme\""));
    v.apply_theme(Theme::Escher);
    let text = crate::json::to_json(v.model());
    assert!(text.contains("\"version\": 6"));
    assert!(text.contains("\"theme\": \"escher\""));
    let back = crate::json::from_json(&text).unwrap();
    assert!(back.warnings.is_empty(), "{:?}", back.warnings);
    assert_eq!(back.model.theme, Some(Theme::Escher));
    assert_eq!(
        back.model
            .find_species("ATP")
            .unwrap()
            .style_name
            .as_deref(),
        Some(ESCHER_COFACTOR)
    );
    // Opened again, new items still take the theme.
    let mut w = DiagramView::new(back.model);
    w.set_tool(Tool::AddSpecies);
    w.handle(&ViewEvent::PointerDown {
        pos: pt(400.0, 0.0),
        button: Button::Primary,
        modifiers: Modifiers::NONE,
    });
    assert_eq!(
        w.model().species().last().unwrap().style_name.as_deref(),
        Some(ESCHER_METABOLITE)
    );
    // A theme this version doesn't know: a warning, and no theme.
    let odd = text.replace("\"theme\": \"escher\"", "\"theme\": \"neon\"");
    let back = crate::json::from_json(&odd).unwrap();
    assert_eq!(back.warnings.len(), 1);
    assert_eq!(back.model.theme, None);
}

#[test]
fn a_name_starts_on_the_side_clearest_of_its_lines() {
    let v = view();
    let m = v.model();
    // Glc's only line goes right and down, to the junction at (100, 50): its
    // name goes up and left, the other way.
    let glc = m.find_species("Glc").unwrap();
    let o = clear_label_offset(m, glc);
    assert!(o.x < 0.0 && o.y < 0.0, "{o:?}");
    // G6P's goes left and down: up and right.
    let g6p = m.find_species("G6P").unwrap();
    let o = clear_label_offset(m, g6p);
    assert!(o.x > 0.0 && o.y < 0.0, "{o:?}");
    // With no lines, right.
    let mut lone = crate::model::SpeciesNode::new("Lone", pt(900.0, 900.0));
    lone.width = 30.0;
    assert_eq!(clear_label_offset(m, &lone), default_label_offset(&lone));
    // Lines both ways along the row: above.
    let mut m2 = Model::new();
    for (id, x) in [("A", 0.0), ("B", 200.0), ("C", 400.0)] {
        m2.push_species(crate::model::SpeciesNode::new(id, pt(x, 0.0)))
            .unwrap();
    }
    for (rid, a, b, jx) in [("R1", "A", "B", 100.0), ("R2", "B", "C", 300.0)] {
        let mut r = crate::model::Reaction::new(rid, pt(jx, 0.0));
        r.reactants.push(Participant::new(a, 1.0));
        r.products.push(Participant::new(b, 1.0));
        m2.push_reaction(r).unwrap();
    }
    let b = m2.find_species("B").unwrap();
    let o = clear_label_offset(&m2, b);
    assert!(o.x == 0.0 && o.y < 0.0, "{o:?}");
    // The panel's Name ▸ Outside uses it too.
    let mut v = view();
    v.select_item(&Target::Species("Glc".into()), false);
    v.set_species_label_outside(true);
    assert!(sp(&v, "Glc").label_offset.unwrap().x < 0.0);
}
