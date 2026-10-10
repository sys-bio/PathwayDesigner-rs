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
        fn text(&mut self, _: SPt, _: &str, _: f32, _: Rgba) {}
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
