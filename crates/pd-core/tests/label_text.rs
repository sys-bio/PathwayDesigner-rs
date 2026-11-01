//! Label text (docs/layout-labels-design.md): a drawing's own words for its
//! label, as Reactome's layouts give short labels for long names. Shown in
//! place of the display name; kept by SBML (as Reactome writes it), the
//! native files and the diagram format.

use pd_core::antimony::ImportOptions;
use pd_core::geometry::pt;
use pd_core::model::{Compartment, Frame, LabelChoice, Model, Participant, Reaction};

const OPTS: ImportOptions = ImportOptions {
    convert_catalytic: false,
    seed: 1,
};

/// `S1 -> S2` by `R1` in the compartment `c1`, all drawn, with Reactome's
/// kind of display names.
fn model() -> Model {
    let mut m = Model::new();
    let mut c = Compartment::new("c1");
    c.frame = Some(Frame::from_corners(pt(0.0, 0.0), pt(500.0, 300.0)));
    c.display_name = Some("cytosol".into());
    m.compartments.push(c);
    for (id, x, name) in [
        ("S1", 100.0, "G6P [cytosol]"),
        ("S2", 300.0, "F6P [cytosol]"),
    ] {
        let s = m.add_species(pt(x, 100.0), 80.0, 36.0);
        m.rename_species(&s, id).unwrap();
        let s = m.find_species_mut(id).unwrap();
        s.display_name = Some(name.into());
        s.compartment = "c1".into();
    }
    let mut r = Reaction::new("R1", pt(200.0, 100.0));
    r.reactants.push(Participant::new("S1", 1.0));
    r.products.push(Participant::new("S2", 1.0));
    r.kinetic_law = "k1*S1".into();
    r.display_name = Some("glucose-6-phosphate isomerase".into());
    r.label_offset = Some(pt(0.0, -20.0));
    m.push_reaction(r).unwrap();
    m.parameters.push(pd_core::model::Parameter {
        variable: "k1".into(),
        expression: "1".into(),
        ..Default::default()
    });
    m
}

fn labels(m: &Model) -> Vec<String> {
    m.species().iter().map(|s| s.label().to_owned()).collect()
}

#[test]
fn label_text_stands_in_for_the_display_name() {
    let mut m = model();
    m.find_species_mut("S1").unwrap().label_text = Some("G6P".into());
    m.show_display_names = false;
    m.refresh_labels();
    assert_eq!(labels(&m), ["S1", "S2"]);
    m.show_display_names = true;
    m.refresh_labels();
    assert_eq!(labels(&m), ["G6P", "F6P [cytosol]"]);
    // An item's own choice: the id, whatever the diagram says.
    m.find_species_mut("S1").unwrap().label_choice = LabelChoice::Id;
    m.refresh_labels();
    assert_eq!(labels(&m)[0], "S1");
    // Label text without a display name is shown as a name would be.
    let s = m.find_species_mut("S2").unwrap();
    s.display_name = None;
    s.label_text = Some("F6P".into());
    m.refresh_labels();
    assert_eq!(labels(&m)[1], "F6P");
}

#[test]
fn reactions_and_compartments_have_label_text_too() {
    let mut m = model();
    m.show_display_names = true;
    m.find_reaction_mut("R1").unwrap().label_text = Some("GPI".into());
    m.compartments[0].label_text = Some("cyto".into());
    m.refresh_labels();
    assert_eq!(m.find_reaction("R1").unwrap().label(), "GPI");
    assert_eq!(m.compartments[0].label(), "cyto");
}

#[test]
fn a_new_alias_starts_with_its_primarys_words() {
    let mut m = model();
    m.find_species_mut("S1").unwrap().label_text = Some("G6P".into());
    let a = m.add_alias("S1", pt(100.0, 250.0)).unwrap();
    assert_eq!(
        m.find_species(&a).unwrap().label_text.as_deref(),
        Some("G6P")
    );
    // And can then have its own.
    m.find_species_mut(&a).unwrap().label_text = Some("glucose 6-P".into());
    assert_eq!(
        m.find_species("S1").unwrap().label_text.as_deref(),
        Some("G6P")
    );
}

/// The SBML text glyph of `S1`'s name, as written.
fn s1_text_glyph(xml: &str) -> &str {
    let at = xml
        .find(r#"layout:graphicalObject="sg_S1""#)
        .expect("S1's text glyph");
    let start = xml[..at].rfind('<').unwrap();
    let end = at + xml[at..].find('>').unwrap();
    &xml[start..=end]
}

#[test]
fn sbml_keeps_label_text_as_reactome_writes_it() {
    let mut m = model();
    let plain = pd_core::sbml::export(&m).xml;
    // Without label text, the name is the species', by its id.
    let glyph = s1_text_glyph(&plain);
    assert!(glyph.contains(r#"layout:originOfText="S1""#), "{glyph}");
    assert!(!glyph.contains("layout:text="), "{glyph}");

    // With it, the glyph holds the words, as Reactome's does.
    m.find_species_mut("S1").unwrap().label_text = Some("G6P".into());
    m.find_reaction_mut("R1").unwrap().label_text = Some("GPI".into());
    m.compartments[0].label_text = Some("cyto".into());
    let xml = pd_core::sbml::export(&m).xml;
    let glyph = s1_text_glyph(&xml);
    assert!(glyph.contains(r#"layout:text="G6P""#), "{glyph}");
    assert!(!glyph.contains("originOfText"), "{glyph}");

    // Read back: the same words, and the display names as they were.
    let back = pd_core::sbml::import(&xml, &OPTS).unwrap().model;
    let s1 = back.find_species("S1").unwrap();
    assert_eq!(s1.label_text.as_deref(), Some("G6P"));
    assert_eq!(s1.display_name.as_deref(), Some("G6P [cytosol]"));
    assert_eq!(back.find_species("S2").unwrap().label_text, None);
    assert_eq!(
        back.find_reaction("R1").unwrap().label_text.as_deref(),
        Some("GPI")
    );
    assert_eq!(
        back.compartments
            .iter()
            .find(|c| c.id == "c1")
            .unwrap()
            .label_text
            .as_deref(),
        Some("cyto")
    );
    // And written again, the same text glyphs.
    let glyphs = |xml: &str| {
        let a = xml.find("<layout:listOfTextGlyphs>").unwrap();
        let b = xml.find("</layout:listOfTextGlyphs>").unwrap();
        xml[a..b].to_owned()
    };
    assert_eq!(glyphs(&pd_core::sbml::export(&back).xml), glyphs(&xml));
}

#[test]
fn a_text_glyph_saying_the_name_or_id_keeps_nothing() {
    let xml = pd_core::sbml::export(&model()).xml;
    for words in ["G6P [cytosol]", "S1", "  "] {
        let file = xml.replace(
            r#"layout:originOfText="S1""#,
            &format!(r#"layout:text="{words}""#),
        );
        let back = pd_core::sbml::import(&file, &OPTS).unwrap().model;
        assert_eq!(
            back.find_species("S1").unwrap().label_text,
            None,
            "{words:?}"
        );
    }
}

#[test]
fn native_files_and_the_diagram_format_keep_it() {
    let mut m = model();
    m.find_species_mut("S1").unwrap().label_text = Some("G6P (hexose)".into());
    m.find_reaction_mut("R1").unwrap().label_text = Some("GPI".into());
    m.compartments[0].label_text = Some("cyto".into());
    let back = pd_core::json::from_json(&pd_core::json::to_json(&m))
        .unwrap()
        .model;
    assert_eq!(
        back.find_species("S1").unwrap().label_text,
        m.find_species("S1").unwrap().label_text
    );

    let text = pd_core::antimony::export_with_diagram(&m);
    assert!(text.contains("labelText"), "{text}");
    let back = pd_core::antimony::import(&text, &OPTS).unwrap().model;
    assert_eq!(
        back.find_species("S1").unwrap().label_text.as_deref(),
        Some("G6P (hexose)")
    );
    assert_eq!(
        back.find_reaction("R1").unwrap().label_text.as_deref(),
        Some("GPI")
    );
    assert_eq!(
        back.compartments
            .iter()
            .find(|c| c.id == "c1")
            .unwrap()
            .label_text
            .as_deref(),
        Some("cyto")
    );
    assert_eq!(back.find_species("S2").unwrap().label_text, None);
}

/// `labelText` belongs to one drawing: in a style it is skipped with a
/// word, as is one without its words in quotes.
#[test]
fn the_diagram_format_says_where_label_text_doesnt_belong() {
    let text = "model m\n  A -> B; k*A\n  k = 1\nend\n\n/* diagram m v1\n  style big species { labelText \"X\" }\n  A at (0, 0) style big\n  B at (200, 0) { labelText B2 }\n*/\n";
    let i = pd_core::antimony::import(text, &OPTS).unwrap();
    let said: Vec<&str> = i.warnings.iter().map(|w| w.message.as_str()).collect();
    assert!(
        said.iter()
            .any(|m| m.contains("'labelText' belongs to one drawing")),
        "{said:?}"
    );
    assert!(
        said.iter()
            .any(|m| m.contains("'labelText' needs its words in quotes")),
        "{said:?}"
    );
    assert_eq!(i.model.find_species("A").unwrap().label_text, None);
    assert_eq!(i.model.find_species("B").unwrap().label_text, None);
}
