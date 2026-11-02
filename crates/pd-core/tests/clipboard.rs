//! Cut, Copy and Paste (docs/cut-copy-paste-design.md): what Copy takes,
//! the ids Paste gives, and the undo steps.

use pd_core::clipboard::{self, Fragment};
use pd_core::geometry::{Pt, pt};
use pd_core::model::{
    AssignmentRule, Compartment, Frame, Model, Parameter, Participant, Reaction, SpeciesNode,
    TextItem,
};
use pd_core::view::DiagramView;

fn species(m: &mut Model, id: &str, at: Pt) {
    let mut s = SpeciesNode::new(id, at);
    s.initial_value = 2.5;
    s.display_name = Some(format!("{id} name"));
    m.push_species(s).unwrap();
}

fn reaction(m: &mut Model, id: &str, from: &str, to: &str, law: &str) {
    let a = m.find_species(from).unwrap().center;
    let b = m.find_species(to).unwrap().center;
    let mut r = Reaction::new(id, (a + b) / 2.0);
    r.reactants.push(Participant::new(from, 1.0));
    r.products.push(Participant::new(to, 1.0));
    r.kinetic_law = law.into();
    m.push_reaction(r).unwrap();
}

fn param(m: &mut Model, id: &str, value: &str) {
    m.parameters.push(Parameter {
        variable: id.into(),
        expression: value.into(),
        ..Parameter::default()
    });
}

/// `A -> B` by `R1` at `k1*A`, and `B -> C` by `R2` at `k2*B`.
fn chain() -> Model {
    let mut m = Model::new();
    species(&mut m, "A", pt(100.0, 100.0));
    species(&mut m, "B", pt(300.0, 100.0));
    species(&mut m, "C", pt(500.0, 100.0));
    reaction(&mut m, "R1", "A", "B", "k1*A");
    reaction(&mut m, "R2", "B", "C", "k2*B");
    param(&mut m, "k1", "0.1");
    param(&mut m, "k2", "0.2");
    m
}

fn select(m: &mut Model, ids: &[&str]) {
    m.clear_selection();
    for id in ids {
        if let Some(s) = m.find_species_mut(id) {
            s.selected = true;
        } else if let Some(r) = m.find_reaction_mut(id) {
            r.selected = true;
        } else if let Some(t) = m.find_text_mut(id) {
            t.selected = true;
        } else {
            m.find_compartment_mut(id).unwrap().selected = true;
        }
    }
}

fn copied(m: &mut Model, ids: &[&str]) -> Fragment {
    select(m, ids);
    clipboard::copy(m).unwrap()
}

fn ids(m: &Model) -> Vec<&str> {
    m.species().iter().map(|s| s.id.as_str()).collect()
}

fn selected(m: &Model) -> Vec<&str> {
    m.selected_species()
        .map(|s| s.id.as_str())
        .chain(m.selected_reactions().map(|r| r.id.as_str()))
        .collect()
}

#[test]
fn nothing_selected_copies_nothing() {
    let mut m = chain();
    m.clear_selection();
    assert!(clipboard::copy(&m).is_none());
}

#[test]
fn a_reaction_brings_its_species_and_parameters() {
    let mut m = chain();
    let f = copied(&mut m, &["R1"]);
    assert_eq!(ids(&f.model), ["A", "B"]);
    assert_eq!(f.model.reactions().len(), 1);
    let params: Vec<_> = f.model.parameters.iter().map(|p| &p.variable).collect();
    assert_eq!(params, ["k1"]);
}

#[test]
fn modifiers_come_too() {
    let mut m = chain();
    m.find_reaction_mut("R1")
        .unwrap()
        .modifiers
        .push(Participant::new("C", 1.0));
    let f = copied(&mut m, &["R1"]);
    assert_eq!(ids(&f.model), ["A", "B", "C"]);
}

#[test]
fn parameters_and_rules_are_followed_through_expressions() {
    let mut m = chain();
    m.find_reaction_mut("R1").unwrap().kinetic_law = "v*A".into();
    m.assignment_rules.push(AssignmentRule {
        variable: "v".into(),
        expression: "k9*2".into(),
        ..AssignmentRule::default()
    });
    param(&mut m, "v", "0");
    param(&mut m, "k9", "3");
    param(&mut m, "unused", "4");
    let f = copied(&mut m, &["R1"]);
    let params: Vec<_> = f.model.parameters.iter().map(|p| &p.variable).collect();
    assert_eq!(params, ["v", "k9"]);
    assert_eq!(f.model.assignment_rules.len(), 1);
}

#[test]
fn pasting_into_the_same_model_gives_new_ids_and_rewrites_laws() {
    let mut m = chain();
    let f = copied(&mut m, &["R1"]);
    clipboard::paste(&mut m, &f, pt(20.0, 20.0));
    assert_eq!(ids(&m), ["A", "B", "C", "A_2", "B_2"]);
    let r = m.find_reaction("R1_2").unwrap();
    assert_eq!(r.kinetic_law, "k1_2*A_2");
    assert_eq!(r.reactants[0].species, "A_2");
    assert_eq!(m.find_parameter("k1_2").unwrap().expression, "0.1");
    // The originals are unchanged.
    assert_eq!(m.find_reaction("R1").unwrap().kinetic_law, "k1*A");
    // Values and display names are copied as they are.
    let a = m.find_species("A_2").unwrap();
    assert_eq!(a.initial_value, 2.5);
    assert_eq!(a.display_name.as_deref(), Some("A name"));
    // Only the pasted items are selected.
    assert_eq!(selected(&m), ["A_2", "B_2", "R1_2"]);
}

#[test]
fn pasting_into_another_model_keeps_ids() {
    let mut m = chain();
    let f = copied(&mut m, &["R2"]);
    let mut other = Model::new();
    clipboard::paste(&mut other, &f, Pt::ZERO);
    assert_eq!(ids(&other), ["B", "C"]);
    assert_eq!(other.find_reaction("R2").unwrap().kinetic_law, "k2*B");
    assert!(other.find_parameter("k2").is_some());
}

#[test]
fn pasting_again_gives_the_next_suffix() {
    let mut m = chain();
    let f = copied(&mut m, &["A"]);
    clipboard::paste(&mut m, &f, Pt::ZERO);
    clipboard::paste(&mut m, &f, Pt::ZERO);
    assert_eq!(ids(&m), ["A", "B", "C", "A_2", "A_3"]);
}

#[test]
fn ids_in_another_case_count_as_used() {
    let mut m = chain();
    let f = copied(&mut m, &["A"]);
    let mut other = Model::new();
    species(&mut other, "a", Pt::ZERO);
    clipboard::paste(&mut other, &f, Pt::ZERO);
    assert_eq!(ids(&other), ["a", "A_2"]);
}

#[test]
fn new_names_are_never_renamed_again() {
    // A becomes A_3, and A_2 becomes A_2_2: the law isn't rewritten twice.
    let mut m = Model::new();
    species(&mut m, "A", pt(0.0, 0.0));
    species(&mut m, "A_2", pt(200.0, 0.0));
    reaction(&mut m, "R1", "A", "A_2", "A*A_2");
    let f = copied(&mut m, &["R1"]);
    let mut target = m.clone();
    species(&mut target, "A_3", pt(0.0, 300.0));
    clipboard::paste(&mut target, &f, Pt::ZERO);
    assert_eq!(
        target.find_reaction("R1_2").unwrap().kinetic_law,
        "A_4*A_2_2"
    );
}

#[test]
fn everything_moves_by_the_offset() {
    let mut m = chain();
    {
        let r = m.find_reaction_mut("R1").unwrap();
        r.reactants[0].ctrl1 = pt(150.0, 80.0);
        r.reactants[0].ctrl_pts_set = true;
        r.products[0].points.push(pt(250.0, 50.0));
    }
    m.push_text(TextItem::new("T1", pt(10.0, 10.0), "note"))
        .unwrap();
    let f = copied(&mut m, &["R1", "T1"]);
    clipboard::paste(&mut m, &f, pt(5.0, 7.0));
    assert_eq!(m.find_species("A_2").unwrap().center, pt(105.0, 107.0));
    let r = m.find_reaction("R1_2").unwrap();
    assert_eq!(r.junction, pt(205.0, 107.0));
    assert_eq!(r.reactants[0].ctrl1, pt(155.0, 87.0));
    assert_eq!(r.products[0].points, [pt(255.0, 57.0)]);
    assert_eq!(m.find_text("T1_2").unwrap().position, pt(15.0, 17.0));
}

#[test]
fn clipboard_text_reads_back() {
    let mut m = chain();
    m.find_species_mut("A").unwrap().in_amounts = true;
    let f = copied(&mut m, &["R1"]);
    let text = clipboard::to_text(&f);
    let back = clipboard::from_text(&text).unwrap();
    assert_eq!(clipboard::to_text(&back), text);
    assert!(back.model.find_species("A").unwrap().in_amounts);
}

#[test]
fn other_text_is_not_pasted() {
    assert!(clipboard::from_text("A -> B; k1*A").is_none());
    assert!(clipboard::from_text("").is_none());
    // A model file isn't a copy.
    assert!(clipboard::from_text(&pd_core::json::to_json(&chain())).is_none());
}

/// `ATP` with an alias `S9`; `R1: S9 -> B`.
fn with_alias() -> Model {
    let mut m = Model::new();
    species(&mut m, "ATP", pt(0.0, 0.0));
    m.find_species_mut("ATP").unwrap().initial_value = 7.0;
    let mut alias = SpeciesNode::new("S9", pt(0.0, 200.0));
    alias.alias_of = Some("ATP".into());
    m.push_species(alias).unwrap();
    species(&mut m, "B", pt(200.0, 200.0));
    reaction(&mut m, "R1", "S9", "B", "k*ATP");
    m
}

#[test]
fn an_alias_without_its_primary_joins_the_species_of_that_id() {
    let mut m = with_alias();
    let f = copied(&mut m, &["R1"]);
    clipboard::paste(&mut m, &f, Pt::ZERO);
    let r = m.find_reaction("R1_2").unwrap();
    let joined = m.find_species(&r.reactants[0].species).unwrap();
    assert_eq!(joined.alias_of.as_deref(), Some("ATP"));
    assert_eq!(r.kinetic_law, "k*ATP");
    assert_eq!(r.products[0].species, "B_2");
    // No second ATP.
    assert_eq!(m.species().iter().filter(|s| !s.is_alias()).count(), 3);
}

#[test]
fn an_alias_without_its_primary_becomes_the_species_elsewhere() {
    let mut m = with_alias();
    let f = copied(&mut m, &["R1"]);
    let mut other = Model::new();
    clipboard::paste(&mut other, &f, Pt::ZERO);
    let atp = other.find_species("ATP").unwrap();
    assert!(!atp.is_alias());
    assert_eq!(atp.initial_value, 7.0);
    assert_eq!(atp.center, pt(0.0, 200.0));
    assert_eq!(
        other.find_reaction("R1").unwrap().reactants[0].species,
        "ATP"
    );
}

#[test]
fn an_alias_copied_with_its_primary_stays_an_alias_of_the_copy() {
    let mut m = with_alias();
    let f = copied(&mut m, &["R1", "ATP"]);
    clipboard::paste(&mut m, &f, Pt::ZERO);
    assert_eq!(
        m.find_species("S9_2").unwrap().alias_of.as_deref(),
        Some("ATP_2")
    );
    assert_eq!(m.find_reaction("R1_2").unwrap().kinetic_law, "k*ATP_2");
}

fn in_compartment() -> Model {
    let mut m = chain();
    let mut c = Compartment::new("cell");
    c.frame = Some(Frame::from_corners(pt(0.0, 0.0), pt(600.0, 200.0)));
    m.compartments.push(c);
    for s in m.species_mut() {
        s.compartment = "cell".into();
    }
    m
}

#[test]
fn a_selected_compartment_is_copied_with_its_box() {
    let mut m = in_compartment();
    let f = copied(&mut m, &["cell", "A"]);
    clipboard::paste(&mut m, &f, pt(0.0, 400.0));
    let c = m.find_compartment("cell_2").unwrap();
    assert_eq!(c.frame.unwrap().center, pt(300.0, 500.0));
    assert!(c.selected);
    assert_eq!(m.find_species("A_2").unwrap().compartment, "cell_2");
}

#[test]
fn a_compartment_used_but_not_selected_is_the_documents_own() {
    let mut m = in_compartment();
    let f = copied(&mut m, &["A"]);
    assert_eq!(f.compartments_used, ["cell"]);
    clipboard::paste(&mut m, &f, pt(20.0, 20.0));
    assert_eq!(m.compartments.len(), 1);
    assert_eq!(m.find_species("A_2").unwrap().compartment, "cell");
}

#[test]
fn a_compartment_used_comes_along_into_a_model_without_it() {
    let mut m = in_compartment();
    let f = copied(&mut m, &["A"]);
    let mut other = Model::new();
    clipboard::paste(&mut other, &f, Pt::ZERO);
    assert!(other.find_compartment("cell").is_some());
    assert_eq!(other.find_species("A").unwrap().compartment, "cell");
}

#[test]
fn a_species_takes_the_compartment_it_lands_in() {
    let mut m = in_compartment();
    let f = copied(&mut m, &["A"]);
    // Pasted below the box: outside it, so in the default compartment, and
    // nothing new is added.
    clipboard::paste(&mut m, &f, pt(0.0, 500.0));
    assert_eq!(m.find_species("A_2").unwrap().compartment, "");
    assert_eq!(m.compartments.len(), 1);
}

#[test]
fn cut_and_paste_are_one_undo_step_each() {
    let mut v = DiagramView::new(chain());
    let mut m = v.model().clone();
    select(&mut m, &["R1"]);
    v.set_model(m);
    let mut before = v.model().clone();
    before.clear_selection();
    let (text, _) = v.cut_selection();
    let text = text.unwrap();
    assert!(v.model().find_reaction("R1").is_none());
    v.paste(&text, None);
    assert!(v.model().find_reaction("R1").is_some());
    assert_eq!(v.undo_description(), Some("Paste"));
    v.undo();
    assert!(v.model().find_reaction("R1").is_none());
    v.undo();
    let mut back = v.model().clone();
    back.clear_selection();
    assert_eq!(back.reactions(), before.reactions());
}

#[test]
fn paste_without_a_place_steps_down_and_right() {
    let mut v = DiagramView::new(chain());
    let mut m = v.model().clone();
    select(&mut m, &["A"]);
    v.set_model(m);
    let text = v.copy_selection().unwrap();
    v.paste(&text, None);
    v.paste(&text, None);
    assert_eq!(
        v.model().find_species("A_2").unwrap().center,
        pt(120.0, 120.0)
    );
    assert_eq!(
        v.model().find_species("A_3").unwrap().center,
        pt(140.0, 140.0)
    );
}

#[test]
fn paste_at_a_point_centres_the_copy_there() {
    let mut v = DiagramView::new(chain());
    let mut m = v.model().clone();
    select(&mut m, &["A"]);
    v.set_model(m);
    let text = v.copy_selection().unwrap();
    let screen = v.world_to_screen(pt(1000.0, 800.0));
    v.paste(&text, Some(screen));
    let a = v.model().find_species("A_2").unwrap().center;
    assert!((a - pt(1000.0, 800.0)).length() < 1e-9, "{a:?}");
}

/// Every Gallery model, everything selected, copied and pasted into an
/// empty model: the same species and reactions, the same rate laws, and
/// every name a law uses is there. Pasted into itself, every law still
/// finds its names.
#[test]
fn gallery_models_copy_whole() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let index: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(root.join("Gallery/index.json")).unwrap())
            .unwrap();
    for e in index["examples"].as_array().unwrap() {
        let text = std::fs::read_to_string(root.join(e["file"].as_str().unwrap())).unwrap();
        let mut m = pd_core::read_model(&text).unwrap();
        let title = e["title"].as_str().unwrap();
        let all: Vec<String> = m
            .species()
            .iter()
            .map(|s| s.id.clone())
            .chain(m.reactions().iter().map(|r| r.id.clone()))
            .chain(m.compartments.iter().map(|c| c.id.clone()))
            .collect();
        select(&mut m, &all.iter().map(String::as_str).collect::<Vec<_>>());
        let f = clipboard::from_text(&clipboard::to_text(&clipboard::copy(&m).unwrap())).unwrap();
        let mut other = Model::new();
        clipboard::paste(&mut other, &f, Pt::ZERO);
        assert_eq!(ids(&other), ids(&m), "{title}");
        let laws = |m: &Model| -> Vec<String> {
            m.reactions()
                .iter()
                .map(|r| r.kinetic_law.clone())
                .collect()
        };
        assert_eq!(laws(&other), laws(&m), "{title}");
        let mut same = m.clone();
        clipboard::paste(&mut same, &f, pt(0.0, 2000.0));
        for target in [&other, &same] {
            for r in target.reactions() {
                let Ok(ast) = pd_core::expr::parse(&r.kinetic_law) else {
                    continue;
                };
                for name in pd_core::expr::identifiers(&ast) {
                    assert!(
                        target.is_id_used(name)
                            || target.assignment_rules.iter().any(|a| a.variable == name)
                            || ["time", "avogadro", "pi"].contains(&name),
                        "{title}: {} uses {name}, which isn't there",
                        r.id
                    );
                }
            }
        }
    }
}

#[test]
fn a_parameter_brings_the_parameters_its_value_uses() {
    let mut m = chain();
    m.parameters[0].expression = "2*kbase".into();
    param(&mut m, "kbase", "0.05");
    let f = copied(&mut m, &["R1"]);
    clipboard::paste(&mut m, &f, Pt::ZERO);
    assert_eq!(m.find_parameter("k1_2").unwrap().expression, "2*kbase_2");
    assert_eq!(m.find_parameter("kbase_2").unwrap().expression, "0.05");
}

#[test]
fn a_rule_setting_a_copied_species_comes_too() {
    let mut m = chain();
    m.assignment_rules.push(AssignmentRule {
        variable: "B".into(),
        expression: "k3*2".into(),
        ..AssignmentRule::default()
    });
    param(&mut m, "k3", "1");
    let f = copied(&mut m, &["R1"]);
    clipboard::paste(&mut m, &f, Pt::ZERO);
    let rule = m
        .assignment_rules
        .iter()
        .find(|r| r.variable == "B_2")
        .unwrap();
    assert_eq!(rule.expression, "k3_2*2");
}

#[test]
fn a_new_name_is_not_given_twice() {
    // Copied: A and A_2. Pasted where only A is: A becomes A_2, so the
    // copied A_2 must become something else.
    let mut m = Model::new();
    species(&mut m, "A", pt(0.0, 0.0));
    species(&mut m, "A_2", pt(200.0, 0.0));
    let f = copied(&mut m, &["A", "A_2"]);
    let mut target = Model::new();
    species(&mut target, "A", pt(0.0, 0.0));
    clipboard::paste(&mut target, &f, Pt::ZERO);
    assert_eq!(ids(&target), ["A", "A_2", "A_2_2"]);
}

#[test]
fn a_compartment_used_is_not_added_when_its_species_land_elsewhere() {
    let mut m = in_compartment();
    let f = copied(&mut m, &["A"]);
    let mut other = Model::new();
    let mut c = Compartment::new("nucleus");
    c.frame = Some(Frame::from_corners(pt(0.0, 0.0), pt(100.0, 100.0)));
    other.compartments.push(c);
    clipboard::paste(&mut other, &f, pt(0.0, 500.0));
    assert!(other.find_compartment("cell").is_none());
    assert_eq!(other.find_species("A").unwrap().compartment, "");
}

#[test]
fn json_without_the_marker_is_not_pasted() {
    let mut m = chain();
    let f = copied(&mut m, &["A"]);
    let text = clipboard::to_text(&f).replace("pathwayDesignerClipboard", "somethingElse");
    assert!(clipboard::from_text(&text).is_none());
}
