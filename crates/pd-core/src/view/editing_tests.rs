//! Tools and editing commands, driven with made-up events (S §6.5, §7, M4).

use super::*;
use crate::model::{Parameter, Participant, RenameError, Rgba};
use crate::render::SPt;
use crate::render::constants::{GUIDE_LINE, PENDING_PRODUCT, PENDING_REACTANT};

fn click(v: &mut DiagramView, x: f64, y: f64) -> ViewResponse {
    let mut r = v.handle(&ViewEvent::PointerDown {
        pos: pt(x, y),
        button: Button::Primary,
        modifiers: Modifiers::NONE,
    });
    r.merge(v.handle(&ViewEvent::PointerUp {
        pos: pt(x, y),
        button: Button::Primary,
    }));
    r
}

fn right_click(v: &mut DiagramView, x: f64, y: f64) -> ViewResponse {
    v.handle(&ViewEvent::PointerDown {
        pos: pt(x, y),
        button: Button::Secondary,
        modifiers: Modifiers::NONE,
    })
}

/// Four species made with the AddSpecies tool: S1 (0,0), S2 (300,0),
/// S3 (0,200), S4 (300,200).
fn four_species() -> DiagramView {
    let mut v = DiagramView::default();
    v.set_tool(Tool::AddSpecies);
    for (x, y) in [(0.0, 0.0), (300.0, 0.0), (0.0, 200.0), (300.0, 200.0)] {
        click(&mut v, x, y);
    }
    v.set_tool(Tool::Select);
    v
}

fn law(v: &DiagramView, id: &str) -> String {
    v.model().find_reaction(id).unwrap().kinetic_law.clone()
}

// ----- AddSpecies and AddReaction (S §6.5) ----------------------------------

#[test]
fn the_open_reaction_tool_takes_any_number_of_each() {
    let mut v = four_species();
    v.set_tool(Tool::AddSpecies);
    click(&mut v, 600.0, 0.0);
    v.set_tool(Tool::ANY_REACTION);
    let enter = |v: &mut DiagramView| v.handle(&ViewEvent::Confirm);
    // Enter before any reactant does nothing.
    assert!(!enter(&mut v).repaint);
    assert!(!v.picking_products());
    // Three reactants, one picked twice (ignored); Enter; two products.
    for (x, y) in [(0.0, 0.0), (300.0, 0.0), (300.0, 0.0), (0.0, 200.0)] {
        click(&mut v, x, y);
    }
    assert_eq!(v.picked(), (3, 0));
    assert!(enter(&mut v).repaint);
    assert!(v.picking_products());
    assert!(v.model().reactions().is_empty(), "nothing made yet");
    // Enter with no product yet does nothing.
    assert!(!enter(&mut v).model_changed);
    for (x, y) in [(300.0, 200.0), (600.0, 0.0)] {
        click(&mut v, x, y);
    }
    assert!(v.model().reactions().is_empty(), "still open for more");
    let r = enter(&mut v);
    assert!(r.model_changed);
    let rx = &v.model().reactions()[0];
    let ids = |l: &[crate::model::Participant]| -> Vec<String> {
        l.iter().map(|p| p.species.clone()).collect()
    };
    assert_eq!(ids(&rx.reactants), ["S1", "S2", "S3"]);
    assert_eq!(ids(&rx.products), ["S4", "S5"]);
    assert_eq!(
        law(&v, &rx.id.clone()),
        format!("k{}*S1*S2*S3", &rx.id[1..])
    );
    assert_eq!(v.undo_description(), Some("Add Reaction"));
    // The tool stays, ready for the next reaction's reactants.
    assert_eq!(v.tool(), Tool::ANY_REACTION);
    assert_eq!(v.picked(), (0, 0));
    assert!(!v.picking_products());
    // One reactant and one product is fine too; Esc drops the picks.
    click(&mut v, 0.0, 0.0);
    enter(&mut v);
    click(&mut v, 300.0, 0.0);
    enter(&mut v);
    assert_eq!(v.model().reactions().len(), 2);
    click(&mut v, 0.0, 0.0);
    v.handle(&ViewEvent::Cancel);
    assert_eq!((v.tool(), v.picked()), (Tool::Select, (0, 0)));
    // Enter does nothing with the other tools.
    v.set_tool(Tool::UNI_BI);
    click(&mut v, 0.0, 0.0);
    assert!(!enter(&mut v).repaint);
    assert_eq!(v.picked(), (1, 0));
}

#[test]
fn add_species_tool_adds_one_species_per_click() {
    let mut v = DiagramView::default();
    v.set_view(2.0, pt(10.0, 10.0));
    v.set_tool(Tool::AddSpecies);
    let r = click(&mut v, 110.0, 50.0);
    assert!(r.model_changed);
    let s = &v.model().species()[0];
    assert_eq!((s.id.as_str(), s.center), ("S1", pt(50.0, 20.0)));
    assert_eq!((s.width, s.height), (80.0, 36.0));
    // The tool stays active; each species is its own undo step.
    click(&mut v, 300.0, 300.0);
    assert_eq!(v.model().species().len(), 2);
    assert_eq!(v.tool(), Tool::AddSpecies);
    v.undo();
    assert_eq!(v.model().species().len(), 1);
}

#[test]
fn a_three_reaction_network_with_the_mouse_alone() {
    let mut v = four_species();
    v.default_bezier = false;
    // S1 → S2 (UniUni), S2 + S3 → S4 (BiUni), S4 → S1 + S3 (UniBi).
    v.set_tool(Tool::UNI_UNI);
    click(&mut v, 0.0, 0.0);
    click(&mut v, 300.0, 0.0);
    v.set_tool(Tool::BI_UNI);
    click(&mut v, 300.0, 0.0);
    click(&mut v, 0.0, 200.0);
    click(&mut v, 300.0, 200.0);
    v.set_tool(Tool::UNI_BI);
    click(&mut v, 300.0, 200.0);
    click(&mut v, 0.0, 0.0);
    click(&mut v, 0.0, 200.0);

    let m = v.model();
    assert_eq!(m.reactions().len(), 3);
    let ids: Vec<&str> = m.reactions().iter().map(|r| r.id.as_str()).collect();
    assert_eq!(ids, ["R5", "R6", "R7"]);
    assert_eq!(law(&v, "R5"), "k5*S1");
    assert_eq!(law(&v, "R6"), "k6*S2*S3");
    assert_eq!(law(&v, "R7"), "k7*S4");
    // k<N> parameters, each 0.1.
    for k in ["k5", "k6", "k7"] {
        assert_eq!(m.find_parameter(k).unwrap().expression, "0.1");
    }
    // Junction: halfway between the reactant and product centroids.
    let r6 = m.find_reaction("R6").unwrap();
    assert_eq!(
        r6.junction,
        ((pt(300.0, 0.0) + pt(0.0, 200.0)) / 2.0 + pt(300.0, 200.0)) / 2.0
    );
    assert!(
        r6.reactants
            .iter()
            .chain(&r6.products)
            .all(|p| p.stoichiometry == 1.0)
    );
    // Default modes with Bézier off: Linear for UniUni, Straight otherwise.
    assert_eq!(m.find_reaction("R5").unwrap().mode, ReactionMode::Linear);
    assert_eq!(r6.mode, ReactionMode::Straight);
}

#[test]
fn default_bezier_and_smooth_junction() {
    let mut v = four_species();
    v.set_tool(Tool::UNI_UNI);
    click(&mut v, 0.0, 0.0);
    click(&mut v, 300.0, 0.0);
    let r = &v.model().reactions()[0];
    assert_eq!(r.mode, ReactionMode::Bezier);
    assert!(r.junction_smooth);
    v.default_smooth = false;
    click(&mut v, 0.0, 200.0);
    click(&mut v, 300.0, 200.0);
    assert!(!v.model().reactions()[1].junction_smooth);
}

#[test]
fn an_existing_parameter_is_not_duplicated() {
    let mut v = four_species();
    let mut m = v.model().clone();
    m.parameters.push(Parameter {
        variable: "k5".into(),
        expression: "3".into(),
        annotation: Default::default(),
    });
    v.set_model(m);
    v.set_tool(Tool::UNI_UNI);
    click(&mut v, 0.0, 0.0);
    click(&mut v, 300.0, 0.0);
    let ks: Vec<_> = v
        .model()
        .parameters
        .iter()
        .filter(|p| p.variable == "k5")
        .collect();
    assert_eq!(ks.len(), 1);
    assert_eq!(ks[0].expression, "3");
}

#[test]
fn picking_rules() {
    let mut v = four_species();
    v.set_tool(Tool::BI_UNI);
    click(&mut v, 0.0, 0.0);
    // The same species again is ignored.
    click(&mut v, 0.0, 0.0);
    assert_eq!(v.pending.reactants, ["S1"]);
    assert!(v.model().reactions().is_empty());
    click(&mut v, 300.0, 0.0);
    // A third pick is the product: the reaction is made, and the tool stays.
    click(&mut v, 0.0, 200.0);
    assert_eq!(v.model().reactions().len(), 1);
    assert_eq!(v.tool(), Tool::BI_UNI);
    assert_eq!(v.pending, Pending::default());
    // Clicking a junction picks nothing; empty canvas cancels to Select.
    let j = v.model().reactions()[0].junction;
    click(&mut v, j.x, j.y);
    assert_eq!(v.tool(), Tool::BI_UNI);
    click(&mut v, 0.0, 0.0);
    click(&mut v, 1000.0, 1000.0);
    assert_eq!(v.tool(), Tool::Select);
    assert!(v.pending.reactants.is_empty());
}

#[test]
fn the_rate_law_uses_an_alias_s_primary() {
    let mut v = four_species();
    v.create_alias("S1");
    // The alias is at (24,24); S1 itself sits under it, so pick it at its far corner.
    v.set_tool(Tool::UNI_UNI);
    click(&mut v, 60.0, 40.0);
    click(&mut v, 300.0, 0.0);
    let r = &v.model().reactions()[0];
    assert_eq!(r.reactants[0].species, "S5", "the leg goes to the alias");
    assert_eq!(r.kinetic_law, "k6*S1");
}

#[test]
fn entering_an_add_tool_clears_the_selection_and_esc_returns_to_select() {
    let mut v = four_species();
    v.select_all();
    let r = v.set_tool(Tool::UNI_UNI);
    assert!(r.selection_changed);
    assert!(!v.has_selection());
    click(&mut v, 0.0, 0.0);
    v.handle(&ViewEvent::Cancel);
    assert_eq!(v.tool(), Tool::Select);
    assert!(v.pending.reactants.is_empty());
}

#[test]
fn picking_feedback_is_drawn() {
    #[derive(Default)]
    struct Rec {
        rings: Vec<Rgba>,
        guide: bool,
        dot: Option<Rgba>,
    }
    impl Renderer for Rec {
        fn round_rect(&mut self, _: SRect, _: f32, fill: Option<Rgba>, stroke: Option<Stroke>) {
            if let (None, Some(s)) = (fill, stroke) {
                self.rings.push(s.color);
            }
        }
        fn circle(&mut self, _: SPt, radius: f32, fill: Option<Rgba>, _: Option<Stroke>) {
            if radius == 5.0 {
                self.dot = fill;
            }
        }
        fn polyline(&mut self, _: &[SPt], stroke: Stroke) {
            self.guide |= stroke.color == GUIDE_LINE && stroke.dash.is_some();
        }
        fn cubic(&mut self, _: SPt, _: SPt, _: SPt, _: SPt, _: Stroke) {}
        fn polygon(&mut self, _: &[SPt], _: Rgba) {}
        fn text(&mut self, _: SPt, _: &str, _: f32, _: crate::model::TextFont, _: Rgba) {}
    }
    let mut v = four_species();
    v.set_tool(Tool::UNI_BI);
    click(&mut v, 0.0, 0.0);
    v.handle(&ViewEvent::PointerMove {
        pos: pt(150.0, 100.0),
    });
    let mut rec = Rec::default();
    v.draw(&mut rec, SRect::default());
    assert_eq!(rec.rings, [PENDING_REACTANT]);
    assert!(rec.guide);
    // Reactants done: products are next, so the dot is red.
    assert_eq!(rec.dot, Some(PENDING_PRODUCT));
    click(&mut v, 300.0, 0.0);
    let mut rec = Rec::default();
    v.draw(&mut rec, SRect::default());
    assert_eq!(rec.rings, [PENDING_REACTANT, PENDING_PRODUCT]);
}

// ----- Rename (S §7.1) ------------------------------------------------------

#[test]
fn double_click_asks_for_a_new_name() {
    let mut v = four_species();
    let r = v.handle(&ViewEvent::DoubleClick { pos: pt(0.0, 0.0) });
    assert_eq!(r.request, Some(Request::Rename("S1".into())));
    assert_eq!(
        v.handle(&ViewEvent::DoubleClick {
            pos: pt(900.0, 0.0)
        })
        .request,
        None
    );
    // Not while adding things.
    v.set_tool(Tool::AddSpecies);
    assert_eq!(
        v.handle(&ViewEvent::DoubleClick { pos: pt(0.0, 0.0) })
            .request,
        None
    );
}

#[test]
fn rename_updates_laws_widens_the_node_and_undoes() {
    let mut v = four_species();
    v.default_bezier = false;
    v.set_tool(Tool::UNI_UNI);
    click(&mut v, 0.0, 0.0);
    click(&mut v, 300.0, 0.0);
    v.set_tool(Tool::Select);
    let alias = {
        v.create_alias("S1");
        v.selection().0[0].clone()
    };
    let r = v.rename(&alias, "Glucose_6_phosphate").unwrap();
    assert!(r.model_changed);
    assert_eq!(law(&v, "R5"), "k5*Glucose_6_phosphate");
    // Widened to fit the label, primary and alias alike.
    let needed = crate::text::text_width("Glucose_6_phosphate", 12.0) + 20.0;
    for id in ["Glucose_6_phosphate", alias.as_str()] {
        assert!((v.model().find_species(id).unwrap().width - needed).abs() < 1e-9);
    }
    assert_eq!(v.undo_description(), Some("Rename"));
    v.undo();
    assert_eq!(law(&v, "R5"), "k5*S1");
    assert_eq!(v.model().find_species("S1").unwrap().width, 80.0);
}

#[test]
fn rename_never_shrinks_and_reports_bad_ids() {
    let mut v = four_species();
    v.rename("S1", "A").unwrap();
    assert_eq!(v.model().find_species("A").unwrap().width, 80.0);
    assert_eq!(v.rename("A", "S2"), Err(RenameError::InUse));
    assert_eq!(v.rename("A", "1x"), Err(RenameError::InvalidId));
    // Same id: nothing to do, no undo step.
    let steps = v.undo.len();
    assert_eq!(v.rename("A", "A"), Ok(ViewResponse::default()));
    assert_eq!(v.undo.len(), steps);
    assert_eq!(v.rename_initial("A").as_deref(), Some("A"));
}

// ----- Aliases (S §7.3) -----------------------------------------------------

#[test]
fn create_alias_and_go_to_primary() {
    let mut v = four_species();
    let r = v.create_alias("S2");
    assert!(r.model_changed && r.selection_changed);
    let (sel, _) = v.selection();
    assert_eq!(sel, ["S5"]);
    let a = v.model().find_species("S5").unwrap();
    assert_eq!(a.alias_of.as_deref(), Some("S2"));
    assert_eq!(a.center, pt(324.0, 24.0));
    // An alias of the alias is placed off the primary and points at it.
    v.create_alias("S5");
    assert_eq!(
        v.model().find_species("S6").unwrap().alias_of.as_deref(),
        Some("S2")
    );

    v.set_view(2.0, Pt::ZERO);
    let r = v.go_to_primary("S5", pt(400.0, 300.0));
    assert!(r.selection_changed);
    assert_eq!(v.selection().0, ["S2"]);
    assert_eq!(
        v.world_to_screen(pt(300.0, 0.0)),
        pt(400.0, 300.0),
        "primary centred"
    );
    assert_eq!(v.undo_description(), Some("Create Alias"), "not an edit");
}

#[test]
fn deleting_a_primary_removes_its_aliases_and_their_reactions() {
    let mut v = four_species();
    v.create_alias("S1");
    v.set_tool(Tool::UNI_UNI);
    click(&mut v, 60.0, 40.0);
    click(&mut v, 300.0, 0.0);
    v.set_tool(Tool::Select);
    assert_eq!(v.model().reactions().len(), 1);
    v.delete_species("S1");
    assert!(v.model().find_species("S5").is_none());
    assert!(v.model().reactions().is_empty());
    v.undo();
    assert_eq!(v.model().reactions().len(), 1);
}

// ----- Context menu (S §13.4) -----------------------------------------------

#[test]
fn right_click_selects_and_asks_for_a_menu() {
    let mut v = four_species();
    let r = right_click(&mut v, 300.0, 0.0);
    assert_eq!(
        r.request,
        Some(Request::ContextMenu(Target::Species("S2".into())))
    );
    assert_eq!(v.selection().0, ["S2"]);
    // A right click on a selected species keeps the selection.
    v.select_all();
    right_click(&mut v, 0.0, 0.0);
    assert_eq!(v.selection().0.len(), 4);
    // Nothing on empty canvas.
    assert_eq!(right_click(&mut v, 900.0, 900.0).request, None);
}

#[test]
fn lock_and_unlock_the_selection() {
    let mut v = four_species();
    click(&mut v, 0.0, 0.0);
    assert!(v.lock_would_lock());
    v.toggle_lock();
    assert!(v.model().find_species("S1").unwrap().locked);
    assert!(!v.lock_would_lock());
    assert_eq!(v.undo_description(), Some("Lock"));
    v.toggle_lock();
    assert!(!v.model().find_species("S1").unwrap().locked);
    v.undo();
    assert!(v.model().find_species("S1").unwrap().locked);
}

#[test]
fn remove_a_null_node_from_its_reaction() {
    let mut v = four_species();
    let mut m = v.model().clone();
    let r = m.add_reaction(pt(300.0, 100.0));
    let mut null = crate::model::SpeciesNode::new("_source_R5", pt(300.0, 100.0));
    null.is_null_node = true;
    null.width = 28.0;
    null.height = 28.0;
    m.push_species(null).unwrap();
    let rr = m.find_reaction_mut(&r).unwrap();
    rr.reactants.push(Participant::new("_source_R5", 1.0));
    rr.products.push(Participant::new("S4", 1.0));
    v.set_model(m);
    // Null nodes can't be renamed.
    assert_eq!(
        v.handle(&ViewEvent::DoubleClick {
            pos: pt(300.0, 100.0)
        })
        .request,
        None
    );
    v.remove_null_node("_source_R5");
    let rr = v.model().find_reaction(&r).unwrap();
    assert!(rr.reactants.is_empty());
    assert_eq!(rr.products.len(), 1);
    v.undo();
    assert!(v.model().find_species("_source_R5").is_some());
}

#[test]
fn single_item_deletes() {
    let mut v = four_species();
    v.default_bezier = false;
    v.set_tool(Tool::UNI_UNI);
    click(&mut v, 0.0, 0.0);
    click(&mut v, 300.0, 0.0);
    v.set_tool(Tool::Select);
    v.delete_reaction("R5");
    assert!(v.model().reactions().is_empty());
    assert_eq!(v.model().species().len(), 4);
    v.undo();
    assert_eq!(v.model().reactions().len(), 1);
}

// ----- Undo depth (M4) ------------------------------------------------------

#[test]
fn twenty_steps_undo_and_redo() {
    let mut v = DiagramView::default();
    v.set_tool(Tool::AddSpecies);
    for i in 0..20 {
        click(&mut v, f64::from(i) * 100.0, 0.0);
    }
    let full = v.model().clone();
    for _ in 0..20 {
        assert!(v.undo().model_changed);
    }
    assert!(v.model().species().is_empty());
    assert!(!v.can_undo());
    for _ in 0..20 {
        v.redo();
    }
    assert_eq!(v.model(), &full);
}

// ----- Model tree (SPEC-Rust §11.3) -----------------------------------------

#[test]
fn selecting_from_a_list() {
    let mut v = four_species();
    let r = v.select_item(&Target::Species("S2".into()), false);
    assert!(r.selection_changed);
    assert_eq!(v.selection().0, ["S2"]);
    v.select_item(&Target::Species("S3".into()), true);
    assert_eq!(v.selection().0, ["S2", "S3"]);
    // Adding again takes it out.
    v.select_item(&Target::Species("S2".into()), true);
    assert_eq!(v.selection().0, ["S3"]);
    v.centre_on(pt(300.0, 0.0), pt(500.0, 400.0));
    assert_eq!(v.world_to_screen(pt(300.0, 0.0)), pt(500.0, 400.0));
}

#[test]
fn a_recovered_document_counts_as_unsaved() {
    let mut v = four_species();
    v.set_model(v.model().clone());
    assert!(!v.is_modified());
    v.mark_unsaved();
    assert!(v.is_modified());
    v.mark_saved();
    assert!(!v.is_modified());
}

/// ADP at (0,0), drawn by four reactions: R1 ADP → B (junction (60,0), the
/// nearest), R2 C → ADP (junction (0,300)), R3 D → E regulated by ADP
/// (junction (400,400)), R4 ADP + F → G, Bézier with stored handles
/// (junction (-300,0)).
fn busy_adp() -> DiagramView {
    let mut m = Model::new();
    for (id, x, y) in [
        ("ADP", 0.0, 0.0),
        ("B", 120.0, 0.0),
        ("C", 0.0, 400.0),
        ("D", 400.0, 300.0),
        ("E", 400.0, 500.0),
        ("F", -300.0, 100.0),
        ("G", -400.0, 0.0),
    ] {
        m.push_species(crate::model::SpeciesNode::new(id, pt(x, y)))
            .unwrap();
    }
    let reaction = |m: &mut Model, id: &str, j: Pt, reactants: &[&str], products: &[&str]| {
        let mut r = crate::model::Reaction::new(id, j);
        r.reactants = reactants
            .iter()
            .map(|s| Participant::new(*s, 1.0))
            .collect();
        r.products = products.iter().map(|s| Participant::new(*s, 1.0)).collect();
        m.push_reaction(r).unwrap();
    };
    reaction(&mut m, "R1", pt(60.0, 0.0), &["ADP"], &["B"]);
    reaction(&mut m, "R2", pt(0.0, 300.0), &["C"], &["ADP"]);
    reaction(&mut m, "R3", pt(400.0, 400.0), &["D"], &["E"]);
    reaction(&mut m, "R4", pt(-300.0, 0.0), &["ADP", "F"], &["G"]);
    let r3 = m.find_reaction_mut("R3").unwrap();
    r3.modifiers.push(Participant::new("ADP", 1.0));
    r3.kinetic_law = "k*D*ADP".into();
    let r4 = m.find_reaction_mut("R4").unwrap();
    r4.mode = ReactionMode::Bezier;
    r4.reactants[0].ctrl_pts_set = true;
    m.recompute_id_counter();
    DiagramView::new(m)
}

#[test]
fn split_into_aliases_gives_each_reaction_its_own_drawing() {
    let mut v = busy_adp();
    let biochemistry = v.export_antimony();
    assert!(v.can_split_into_aliases("ADP"));
    let r = v.split_into_aliases("ADP");
    assert!(r.model_changed && r.selection_changed);
    let m = v.model();

    // R1, the nearest, stays with ADP; the others each get a new alias.
    assert_eq!(m.find_reaction("R1").unwrap().reactants[0].species, "ADP");
    let alias_of = |rid: &str, pick: &dyn Fn(&crate::model::Reaction) -> String| {
        let id = pick(m.find_reaction(rid).unwrap());
        let node = m.find_species(&id).unwrap();
        assert_eq!(node.alias_of.as_deref(), Some("ADP"), "{rid}");
        id
    };
    let a2 = alias_of("R2", &|r| r.products[0].species.clone());
    let a3 = alias_of("R3", &|r| r.modifiers[0].species.clone());
    let a4 = alias_of("R4", &|r| r.reactants[0].species.clone());
    let mut all = vec![a2.clone(), a3.clone(), a4.clone()];
    all.sort();
    all.dedup();
    assert_eq!(all.len(), 3, "one alias per reaction");
    assert_eq!(m.aliases_of("ADP").count(), 3);

    // Each alias sits between its junction and ADP, near the junction.
    let at = |id: &str| m.find_species(id).unwrap().center;
    assert_eq!(at(&a2), pt(0.0, 210.0));
    assert!((at(&a4).x - -210.0).abs() < 1e-9 && at(&a4).y.abs() < 1e-9);
    assert!(at(&a3).distance(pt(400.0, 400.0)) <= 90.0 + 1e-9);

    // A moved Bézier leg goes back to automatic handles.
    assert!(!m.find_reaction("R4").unwrap().reactants[0].ctrl_pts_set);
    // The new aliases are selected.
    let (sel, _) = v.selection();
    assert_eq!(sel.len(), 3);
    // The same species, so the same biochemistry.
    assert_eq!(v.export_antimony(), biochemistry);
    assert_eq!(v.undo_description(), Some("Split into Aliases"));

    // One undo step puts it all back.
    v.undo();
    assert_eq!(v.model().aliases_of("ADP").count(), 0);
    assert_eq!(
        v.model().find_reaction("R2").unwrap().products[0].species,
        "ADP"
    );
}

#[test]
fn split_into_aliases_needs_a_primary_in_two_reactions() {
    let mut v = busy_adp();
    // B is in one reaction only.
    assert!(!v.can_split_into_aliases("B"));
    assert!(!v.split_into_aliases("B").model_changed);
    // After a split, ADP itself draws one reaction, and an alias can't be split.
    v.split_into_aliases("ADP");
    assert!(!v.can_split_into_aliases("ADP"));
    let alias = v.model().aliases_of("ADP").next().unwrap().id.clone();
    assert!(!v.can_split_into_aliases(&alias));
}

#[test]
fn merge_aliases_undoes_a_split() {
    let mut v = busy_adp();
    let biochemistry = v.export_antimony();
    assert!(!v.can_merge_aliases("ADP"), "no aliases yet");
    v.split_into_aliases("ADP");
    let alias = v.model().aliases_of("ADP").next().unwrap().id.clone();
    // Offered on the primary and on any of its aliases.
    assert!(v.can_merge_aliases("ADP") && v.can_merge_aliases(&alias));
    // A leg to an alias whose handles were shaped by hand.
    v.model.find_reaction_mut("R4").unwrap().reactants[0].ctrl_pts_set = true;

    let r = v.merge_aliases(&alias);
    assert!(r.model_changed && r.selection_changed);
    let m = v.model();
    assert_eq!(m.aliases_of("ADP").count(), 0);
    assert!(m.find_species(&alias).is_none());
    // Every line is drawn to ADP again, and every reaction is still there.
    assert_eq!(m.reactions().len(), 4);
    assert_eq!(m.find_reaction("R2").unwrap().products[0].species, "ADP");
    assert_eq!(m.find_reaction("R3").unwrap().modifiers[0].species, "ADP");
    let r4 = &m.find_reaction("R4").unwrap().reactants[0];
    assert_eq!(r4.species, "ADP");
    assert!(!r4.ctrl_pts_set, "a moved leg gets automatic handles");
    // Only ADP is selected, and the biochemistry never changed.
    let (sel, _) = v.selection();
    assert_eq!(sel, ["ADP"]);
    assert_eq!(v.export_antimony(), biochemistry);
    assert_eq!(v.undo_description(), Some("Merge Aliases"));

    // One undo step brings the aliases back.
    v.undo();
    assert_eq!(v.model().aliases_of("ADP").count(), 3);
    assert_eq!(
        v.model().find_reaction("R2").unwrap().products[0].species,
        v.model().aliases_of("ADP").next().unwrap().id
    );
}

#[test]
fn merge_aliases_draws_a_modifier_once() {
    let mut v = busy_adp();
    let alias = v.model.add_alias("ADP", pt(500.0, 500.0)).unwrap();
    // R3 has ADP as a modifier already; give it the alias as one too.
    let r3 = v.model.find_reaction_mut("R3").unwrap();
    r3.modifiers.push(Participant::new(alias.as_str(), 1.0));
    v.merge_aliases("ADP");
    let mods: Vec<_> = v
        .model()
        .find_reaction("R3")
        .unwrap()
        .modifiers
        .iter()
        .map(|p| p.species.clone())
        .collect();
    assert_eq!(mods, ["ADP"]);
    // Nothing to do for a species without aliases, or for a null node.
    assert!(!v.can_merge_aliases("ADP"));
    assert!(!v.merge_aliases("B").model_changed);
}

#[test]
fn split_off_moves_only_the_selected_reactions() {
    let mut v = busy_adp();
    let biochemistry = v.export_antimony();
    // ADP alone selected: nothing to split off.
    v.select_item(&Target::Species("ADP".into()), false);
    assert!(!v.can_split_off_selected("ADP"));
    // ADP with R2 (it is a product there) and R3 (a modifier).
    v.select_item(&Target::Leg("R2".into()), true);
    v.select_item(&Target::Leg("R3".into()), true);
    assert!(v.can_split_off_selected("ADP"));
    // B is not in R2 or R3.
    assert!(!v.can_split_off_selected("B"));

    let r = v.split_off_selected("ADP");
    assert!(r.model_changed && r.selection_changed);
    let m = v.model();
    let a2 = m.find_reaction("R2").unwrap().products[0].species.clone();
    let a3 = m.find_reaction("R3").unwrap().modifiers[0].species.clone();
    for a in [&a2, &a3] {
        assert_eq!(m.find_species(a).unwrap().alias_of.as_deref(), Some("ADP"));
    }
    assert_ne!(a2, a3, "one alias per reaction");
    // Placed as Split into Aliases places them.
    assert_eq!(m.find_species(&a2).unwrap().center, pt(0.0, 210.0));
    // The others stay with ADP.
    assert_eq!(m.find_reaction("R1").unwrap().reactants[0].species, "ADP");
    assert_eq!(m.find_reaction("R4").unwrap().reactants[0].species, "ADP");
    assert_eq!(m.aliases_of("ADP").count(), 2);
    // The new aliases are selected, and the biochemistry is the same.
    let (mut sel, _) = v.selection();
    sel.sort();
    let mut want = vec![a2.clone(), a3.clone()];
    want.sort();
    assert_eq!(sel, want);
    assert_eq!(v.export_antimony(), biochemistry);
    assert_eq!(v.undo_description(), Some("Split Off Selected Reactions"));
    v.undo();
    assert_eq!(v.model().aliases_of("ADP").count(), 0);
    assert_eq!(
        v.model().find_reaction("R2").unwrap().products[0].species,
        "ADP"
    );
}

#[test]
fn split_off_works_from_an_alias_too() {
    let mut v = busy_adp();
    v.select_item(&Target::Species("ADP".into()), false);
    v.select_item(&Target::Leg("R2".into()), true);
    v.split_off_selected("ADP");
    let a2 = v.model().find_reaction("R2").unwrap().products[0]
        .species
        .clone();
    // From that alias, split R2 off again: a new alias of ADP, and a2 is
    // left drawing nothing, like one made with Create Alias.
    v.select_item(&Target::Species(a2.clone()), false);
    v.select_item(&Target::Leg("R2".into()), true);
    assert!(v.can_split_off_selected(&a2));
    v.split_off_selected(&a2);
    let m = v.model();
    let again = m.find_reaction("R2").unwrap().products[0].species.clone();
    assert_ne!(again, a2);
    assert_eq!(
        m.find_species(&again).unwrap().alias_of.as_deref(),
        Some("ADP")
    );
    assert!(m.find_species(&a2).is_some());
}

#[test]
fn an_alias_split_off_near_its_junction_goes_half_way() {
    // R1's junction is 60 from ADP, nearer than 180: half way, not 90 off.
    let mut v = busy_adp();
    v.select_item(&Target::Species("ADP".into()), false);
    v.select_item(&Target::Leg("R1".into()), true);
    v.split_off_selected("ADP");
    let m = v.model();
    let a1 = &m.find_reaction("R1").unwrap().reactants[0].species;
    assert_eq!(m.find_species(a1).unwrap().center, pt(30.0, 0.0));
}

/// busy_adp after Split into Aliases: the aliases drawing R2, R3 and R4.
fn three_adp_aliases() -> (DiagramView, Id, Id, Id) {
    let mut v = busy_adp();
    v.split_into_aliases("ADP");
    let m = v.model();
    let a2 = m.find_reaction("R2").unwrap().products[0].species.clone();
    let a3 = m.find_reaction("R3").unwrap().modifiers[0].species.clone();
    let a4 = m.find_reaction("R4").unwrap().reactants[0].species.clone();
    (v, a2, a3, a4)
}

#[test]
fn selected_aliases_merge_into_the_one_right_clicked() {
    let (mut v, a2, a3, a4) = three_adp_aliases();
    let biochemistry = v.export_antimony();
    let at = v.model().find_species(&a2).unwrap().center;
    // One alias alone: nothing to merge.
    v.select_item(&Target::Species(a2.clone()), false);
    assert!(!v.can_merge_selected_into(&a2));
    // With a4 too, a4 merges into a2.
    v.select_item(&Target::Species(a4.clone()), true);
    assert!(v.can_merge_selected_into(&a2));
    let r = v.merge_selected_into(&a2);
    assert!(r.model_changed && r.selection_changed);
    let m = v.model();
    assert!(m.find_species(&a4).is_none());
    assert_eq!(m.find_reaction("R4").unwrap().reactants[0].species, a2);
    assert!(!m.find_reaction("R4").unwrap().reactants[0].ctrl_pts_set);
    // a2 stays where it was; a3, not selected, is left alone.
    assert_eq!(m.find_species(&a2).unwrap().center, at);
    assert_eq!(m.find_reaction("R3").unwrap().modifiers[0].species, a3);
    assert_eq!(m.aliases_of("ADP").count(), 2);
    let (sel, _) = v.selection();
    assert_eq!(sel, std::slice::from_ref(&a2));
    assert_eq!(v.export_antimony(), biochemistry);
    assert_eq!(v.undo_description(), Some("Merge Aliases"));
    v.undo();
    assert_eq!(v.model().aliases_of("ADP").count(), 3);
}

#[test]
fn the_primary_takes_only_the_selected_aliases_and_is_never_merged_away() {
    let (mut v, a2, a3, a4) = three_adp_aliases();
    // Right-clicking the primary with a3 selected merges only a3.
    v.select_item(&Target::Species("ADP".into()), false);
    v.select_item(&Target::Species(a3.clone()), true);
    v.merge_selected_into("ADP");
    let m = v.model();
    assert!(m.find_species(&a3).is_none());
    assert_eq!(m.find_reaction("R3").unwrap().modifiers[0].species, "ADP");
    assert!(m.find_species(&a2).is_some() && m.find_species(&a4).is_some());
    // An alias with only the primary selected besides: the primary stays.
    v.select_item(&Target::Species(a2.clone()), false);
    v.select_item(&Target::Species("ADP".into()), true);
    assert!(!v.can_merge_selected_into(&a2));
    assert!(!v.merge_selected_into(&a2).model_changed);
}

#[test]
fn one_alias_merges_into_the_primary() {
    let (mut v, a2, a3, a4) = three_adp_aliases();
    let biochemistry = v.export_antimony();
    let r = v.merge_alias_into_primary(&a3);
    assert!(r.model_changed && r.selection_changed);
    let m = v.model();
    assert!(m.find_species(&a3).is_none());
    assert_eq!(m.find_reaction("R3").unwrap().modifiers[0].species, "ADP");
    // The other aliases stay.
    assert!(m.find_species(&a2).is_some() && m.find_species(&a4).is_some());
    let (sel, _) = v.selection();
    assert_eq!(sel, ["ADP"]);
    assert_eq!(v.export_antimony(), biochemistry);
    assert_eq!(v.undo_description(), Some("Merge into Primary"));
    // Nothing to do for a primary.
    assert!(!v.merge_alias_into_primary("ADP").model_changed);
}

#[test]
fn select_all_drawings_selects_a_species_and_its_aliases_only() {
    let mut v = four_species();
    let a2 = v.model.add_alias("S1", pt(500.0, 500.0)).unwrap();
    let a3 = v.model.add_alias("S1", pt(600.0, 500.0)).unwrap();
    v.model.find_species_mut("S2").unwrap().selected = true;
    assert!(v.can_select_all_drawings(&a2));
    assert!(!v.can_select_all_drawings("S3"), "no aliases");
    let undo_before = v.undo_description();
    // Right-clicked on an alias: the whole species, nothing else.
    let r = v.select_all_drawings(&a2);
    assert!(r.selection_changed);
    let selected: Vec<&str> = v
        .model()
        .species()
        .iter()
        .filter(|s| s.selected)
        .map(|s| s.id.as_str())
        .collect();
    assert_eq!(selected, ["S1", a2.as_str(), a3.as_str()]);
    // Not an edit: no undo step of its own.
    assert_eq!(v.undo_description(), undo_before);
    // Then one size for all of them.
    v.set_species_size(Some(40.0), Some(20.0));
    for id in ["S1", a2.as_str(), a3.as_str()] {
        let s = v.model().find_species(id).unwrap();
        assert_eq!((s.width, s.height), (40.0, 20.0), "{id}");
    }
    assert_ne!(v.model().find_species("S2").unwrap().width, 40.0);
}
