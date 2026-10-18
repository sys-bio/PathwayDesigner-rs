//! Changing an existing reaction's reactants and products (S §7.11): Add
//! Reactant… and Add Product… with the mouse, Remove from Reaction, null
//! nodes, the rate law and modifiers. Driven with made-up events.

use super::*;
use crate::model::{Participant, Reaction, SpeciesNode};

/// S1 (0,0), S2 (300,0), S3 (0,200), S4 (300,200), and R1: S1 → S2 with the
/// law a new reaction gets, `k1*S1`. Screen = world.
fn view() -> DiagramView {
    let mut m = Model::new();
    for (id, x, y) in [
        ("S1", 0.0, 0.0),
        ("S2", 300.0, 0.0),
        ("S3", 0.0, 200.0),
        ("S4", 300.0, 200.0),
    ] {
        m.push_species(SpeciesNode::new(id, pt(x, y))).unwrap();
    }
    let mut r = Reaction::new("R1", pt(150.0, 0.0));
    r.reactants = vec![Participant::new("S1", 1.0)];
    r.products = vec![Participant::new("S2", 1.0)];
    r.kinetic_law = "k1*S1".into();
    m.push_reaction(r).unwrap();
    DiagramView::new(m)
}

fn click(v: &mut DiagramView, at: Pt) -> ViewResponse {
    v.handle(&ViewEvent::PointerDown {
        pos: at,
        button: Button::Primary,
        modifiers: Modifiers::NONE,
    })
}

fn r1(v: &DiagramView) -> Reaction {
    v.model().find_reaction("R1").unwrap().clone()
}

fn ids(list: &[Participant]) -> Vec<String> {
    list.iter().map(|p| p.species.clone()).collect()
}

#[test]
fn add_reactant_picks_species_until_stopped() {
    let mut v = view();
    v.begin_adding("R1", Role::Reactant);
    assert_eq!(v.adding_to(), Some(("R1", Role::Reactant)));
    let r = click(&mut v, pt(0.0, 200.0));
    assert!(r.model_changed);
    assert_eq!(ids(&r1(&v).reactants), ["S1", "S3"]);
    assert_eq!(r1(&v).kinetic_law, "k1*S1*S3", "the law follows");
    assert_eq!(v.undo_description(), Some("Add Reactant"));
    // Still adding: another, then one already in it (ignored).
    click(&mut v, pt(300.0, 200.0));
    assert_eq!(ids(&r1(&v).reactants), ["S1", "S3", "S4"]);
    assert!(
        !click(&mut v, pt(300.0, 0.0)).model_changed,
        "S2 is its product"
    );
    assert!(
        !click(&mut v, pt(0.0, 0.0)).model_changed,
        "S1 is in already"
    );
    // A click on the junction does nothing; empty canvas stops.
    click(&mut v, pt(150.0, 0.0));
    assert!(v.adding_to().is_some());
    click(&mut v, pt(700.0, 700.0));
    assert_eq!(v.adding_to(), None);
    assert_eq!(v.tool(), Tool::Select);
    // Each species was one undo step.
    v.undo();
    assert_eq!(ids(&r1(&v).reactants), ["S1", "S3"]);
    v.undo();
    assert_eq!(ids(&r1(&v).reactants), ["S1"]);
    assert_eq!(r1(&v).kinetic_law, "k1*S1");
}

#[test]
fn add_product_and_esc() {
    let mut v = view();
    v.begin_adding("R1", Role::Product);
    click(&mut v, pt(300.0, 200.0));
    assert_eq!(ids(&r1(&v).products), ["S2", "S4"]);
    assert_eq!(r1(&v).kinetic_law, "k1*S1", "products aren't in the law");
    assert_eq!(v.undo_description(), Some("Add Product"));
    v.handle(&ViewEvent::Cancel);
    assert_eq!(v.adding_to(), None);
    // Choosing a tool stops it too, and nothing is added on a bad start.
    v.begin_adding("R1", Role::Product);
    v.set_tool(Tool::AddSpecies);
    assert_eq!(v.adding_to(), None);
    assert!(!v.begin_adding("R1", Role::Modifier).repaint);
    assert!(!v.begin_adding("nope", Role::Reactant).repaint);
    assert_eq!(v.adding_to(), None);
}

#[test]
fn a_modifier_added_becomes_a_participant_and_a_custom_law_stays() {
    let mut v = view();
    v.set_kinetic_law("R1", "Vm*S1*S4/(Km+S1)");
    assert_eq!(ids(&r1(&v).modifiers), ["S4"]);
    v.add_participant("R1", Role::Reactant, "S4");
    let r = r1(&v);
    assert_eq!(ids(&r.reactants), ["S1", "S4"]);
    assert!(r.modifiers.is_empty(), "no longer a modifier");
    assert_eq!(
        r.kinetic_law, "Vm*S1*S4/(Km+S1)",
        "a law written by hand stays"
    );
    // Taken out again while the law still names it: a modifier once more.
    v.remove_participant("R1", "S4");
    assert_eq!(ids(&r1(&v).modifiers), ["S4"]);
}

#[test]
fn removing_the_last_on_a_side_leaves_a_null_node() {
    let mut v = view();
    v.add_participant("R1", Role::Reactant, "S3");
    // S1 out: the law follows.
    assert!(v.can_remove_participant("R1", "S1"));
    v.remove_participant("R1", "S1");
    assert_eq!(v.undo_description(), Some("Remove from Reaction"));
    assert_eq!(ids(&r1(&v).reactants), ["S3"]);
    assert_eq!(r1(&v).kinetic_law, "k1*S3");
    // S2, the only product, out: a sink takes its place.
    v.remove_participant("R1", "S2");
    assert_eq!(ids(&r1(&v).products), ["_sink_R1"]);
    assert!(v.model().find_species("_sink_R1").unwrap().is_null_node);
    // S3 is now the only species left: it can't go.
    assert!(!v.can_remove_participant("R1", "S3"));
    assert!(!v.remove_participant("R1", "S3").model_changed);
    assert!(
        !v.can_remove_participant("R1", "_sink_R1"),
        "not a species of it"
    );
    assert!(!v.can_remove_participant("R1", "S4"), "not in it");
    // A product added replaces the sink.
    v.add_participant("R1", Role::Product, "S4");
    assert_eq!(ids(&r1(&v).products), ["S4"]);
    assert!(v.model().find_species("_sink_R1").is_none());
    // And a null node can't be added.
    v.remove_participant("R1", "S4");
    assert!(
        !v.add_participant("R1", Role::Reactant, "_sink_R1")
            .model_changed
    );
    // Undo, step by step, back to the start.
    for _ in 0..6 {
        v.undo();
    }
    let r = r1(&v);
    assert_eq!(
        (ids(&r.reactants), ids(&r.products)),
        (vec!["S1".to_string()], vec!["S2".to_string()])
    );
    assert!(v.model().find_species("_sink_R1").is_none());
}

#[test]
fn a_reaction_with_no_reactant_left_has_a_law_of_its_constant() {
    let mut v = view();
    v.add_participant("R1", Role::Product, "S3");
    v.remove_participant("R1", "S1");
    let r = r1(&v);
    assert_eq!(ids(&r.reactants), ["_source_R1"]);
    assert_eq!(r.kinetic_law, "k1", "a source is not in the law");
    // A null node that belongs to nothing can't be added either.
    let mut null = SpeciesNode::new("_null", pt(600.0, 0.0));
    null.is_null_node = true;
    v.model.push_species(null).unwrap();
    assert!(
        !v.add_participant("R1", Role::Product, "_null")
            .model_changed
    );
    v.begin_adding("R1", Role::Product);
    assert!(!click(&mut v, pt(600.0, 0.0)).model_changed);
}

#[test]
fn a_smooth_junction_gets_its_handles_again() {
    let mut v = view();
    let r = v.model.find_reaction_mut("R1").unwrap();
    r.mode = crate::model::ReactionMode::Bezier;
    r.junction_smooth = true;
    v.add_participant("R1", Role::Product, "S4");
    let r = r1(&v);
    let smooth = crate::bezier::smooth_handles(v.model(), &r);
    for (role, i, h) in smooth {
        let p = &r.participants(role)[i];
        assert!(p.ctrl_pts_set);
        assert_eq!(Some((p.ctrl1, p.ctrl2)), h);
    }
}

#[test]
fn an_alias_is_added_as_drawn() {
    let mut v = view();
    let a = v.model.add_alias("S3", pt(0.0, 400.0)).unwrap();
    v.begin_adding("R1", Role::Reactant);
    click(&mut v, pt(0.0, 400.0));
    assert_eq!(ids(&r1(&v).reactants), ["S1".to_string(), a.clone()]);
    // The law names the species, not the alias.
    assert_eq!(r1(&v).kinetic_law, "k1*S1*S3");
    // Nodes are compared, as the reaction tools compare them: the alias
    // is in, the primary node is not.
    assert!(!click(&mut v, pt(0.0, 400.0)).model_changed);
}

#[test]
fn reactions_can_be_marked_reversible() {
    let mut v = view();
    let mut r2 = Reaction::new("R2", pt(150.0, 200.0));
    r2.reactants = vec![Participant::new("S3", 1.0)];
    r2.products = vec![Participant::new("S4", 1.0)];
    r2.is_reversible = true;
    v.model.push_reaction(r2).unwrap();
    assert_eq!(v.selected_reactions_reversible(), None, "nothing selected");
    v.select_item(&Target::Leg("R1".into()), false);
    assert_eq!(v.selected_reactions_reversible(), Some(false));
    v.select_item(&Target::Leg("R2".into()), true);
    assert_eq!(v.selected_reactions_reversible(), None, "they disagree");
    v.set_reversible(true);
    assert_eq!(v.undo_description(), Some("Reversible"));
    assert!(r1(&v).is_reversible);
    assert_eq!(v.selected_reactions_reversible(), Some(true));
    assert!(!v.set_reversible(true).model_changed);
    v.undo();
    assert!(!r1(&v).is_reversible);
    // The file and SBML keep it, as they always have.
    v.redo();
    let back = crate::json::from_json(&crate::json::to_json(v.model()))
        .unwrap()
        .model;
    assert!(back.find_reaction("R1").unwrap().is_reversible);
    // View ▸ Reversible arrowheads is on by default, and output follows it.
    assert!(v.show_reversible);
    v.show_reversible = false;
    assert!(!v.shown().reversible);
}
