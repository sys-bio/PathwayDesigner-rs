//! SBML import of files this app didn't write (SPEC-Rust §8.3): layouts
//! written by python-libsbml in Level 3 and Level 2, and core-only models.

use pd_core::antimony::ImportOptions;
use pd_core::geometry::{Pt, pt};
use pd_core::model::ReactionMode;
use pd_core::sbml::{self, Imported};

const OPTS: ImportOptions = ImportOptions {
    convert_catalytic: false,
    seed: 7,
};

/// A layout file from sbml-io's tests, written by python-libsbml.
fn fixture(name: &str) -> Imported {
    let path = format!(
        "{}/../sbml-io/tests/data/{name}",
        env!("CARGO_MANIFEST_DIR")
    );
    sbml::import(&std::fs::read_to_string(path).unwrap(), &OPTS).unwrap()
}

fn close(a: Pt, b: Pt) -> bool {
    a.distance(b) < 1e-9
}

#[test]
fn a_libsbml_layout_is_read() {
    let i = fixture("layout-l3v1.xml");
    assert!(i.had_layout);
    assert_eq!(i.unplaced, 0);
    assert!(i.warnings.is_empty(), "{:?}", i.warnings);
    let m = &i.model;
    let glc = m.find_species("Glc").unwrap();
    // The glyph's box: (20, 20), 60 × 30.
    assert_eq!(
        (glc.center, glc.width, glc.height),
        (pt(50.0, 35.0), 60.0, 30.0)
    );
    assert_eq!(glc.initial_value, 10.0);

    let j0 = m.find_reaction("J0").unwrap();
    // No bounding box on the reaction glyph: the middle of its curve.
    assert_eq!(j0.junction, pt(190.0, 100.0));
    assert_eq!(j0.kinetic_law, "k1*HK*Glc*ATP");
    assert_eq!(j0.modifiers.len(), 1);
    assert_eq!(j0.modifiers[0].species, "HK");
    assert!(!j0.is_reversible);
    assert_eq!(j0.mode, ReactionMode::Bezier);
    // libsbml's file runs the substrate curve from the junction to Glc, the
    // other way round from this app, so the handles are swapped to match.
    let leg = &j0.reactants[0];
    assert_eq!(leg.species, "Glc");
    assert!(leg.ctrl_pts_set);
    assert!(close(leg.ctrl1, pt(50.0, 60.25)), "{:?}", leg.ctrl1);
    assert!(close(leg.ctrl2, pt(190.0, 60.5)), "{:?}", leg.ctrl2);
    // The product curve already runs junction → species: kept as stored.
    let g6p = &j0.products[0];
    assert!(close(g6p.ctrl1, pt(190.0, 60.5)) && close(g6p.ctrl2, pt(330.0, 60.25)));
    assert_eq!(m.find_parameter("k1").unwrap().expression, "0.1");
}

#[test]
fn level_2_reads_the_same_as_level_3() {
    let (a, b) = (
        fixture("layout-l3v1.xml").model,
        fixture("layout-l2v4.xml").model,
    );
    for s in a.species() {
        let t = b.find_species(&s.id).unwrap();
        assert_eq!(
            (s.center, s.width, s.height),
            (t.center, t.width, t.height),
            "{}",
            s.id
        );
    }
    for r in a.reactions() {
        let q = b.find_reaction(&r.id).unwrap();
        assert_eq!(r.junction, q.junction);
        assert_eq!(r.reactants, q.reactants);
        assert_eq!(r.products, q.products);
        assert_eq!(r.kinetic_law, q.kinetic_law);
    }
}

const CORE_ONLY: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<sbml xmlns="http://www.sbml.org/sbml/level2/version4" level="2" version="4">
  <model id="m" name="Core only">
    <listOfCompartments>
      <compartment id="cell" size="2"/>
    </listOfCompartments>
    <listOfSpecies>
      <species id="A" compartment="cell" initialAmount="5"/>
      <species id="B" compartment="cell" initialConcentration="0" initialAmount="3"/>
      <species id="E" compartment="cell" initialConcentration="1" boundaryCondition="true"/>
    </listOfSpecies>
    <listOfParameters>
      <parameter id="k" value="0.5"/>
      <parameter id="y"/>
    </listOfParameters>
    <listOfRules>
      <assignmentRule variable="y">
        <math xmlns="http://www.w3.org/1998/Math/MathML"><apply><times/><cn>2</cn><ci>A</ci></apply></math>
      </assignmentRule>
    </listOfRules>
    <listOfReactions>
      <reaction id="J1">
        <listOfReactants><speciesReference species="A"/></listOfReactants>
        <listOfProducts><speciesReference species="B" stoichiometry="2"/></listOfProducts>
        <kineticLaw>
          <math xmlns="http://www.w3.org/1998/Math/MathML">
            <apply><times/><ci>k</ci><ci>E</ci><ci>A</ci></apply>
          </math>
          <listOfParameters><parameter id="k" value="3"/></listOfParameters>
        </kineticLaw>
      </reaction>
      <reaction id="J2" reversible="false">
        <listOfReactants><speciesReference species="B"/></listOfReactants>
        <listOfProducts><speciesReference species="B"/><speciesReference species="A"/></listOfProducts>
      </reaction>
    </listOfReactions>
    <listOfEvents>
      <event><trigger><math xmlns="http://www.w3.org/1998/Math/MathML"><true/></math></trigger></event>
    </listOfEvents>
  </model>
</sbml>"#;

#[test]
fn a_core_only_model() {
    let i = sbml::import(CORE_ONLY, &OPTS).unwrap();
    assert!(!i.had_layout);
    assert_eq!(i.unplaced, 3);
    assert_eq!(i.warnings, ["1 event was left out"]);
    let m = &i.model;
    assert_eq!(m.model_name, "Core only");
    assert_eq!(m.find_compartment("cell").unwrap().size, 2.0);
    // initialAmount when there is no concentration, or it is 0.
    assert_eq!(m.find_species("A").unwrap().initial_value, 5.0);
    assert_eq!(m.find_species("B").unwrap().initial_value, 3.0);
    assert!(m.find_species("E").unwrap().is_boundary);
    // y is set by its rule, so it is the rule's variable, not a parameter.
    assert!(m.find_parameter("y").is_none());
    assert_eq!(m.assignment_rules[0].expression, "2*A");

    let j1 = m.find_reaction("J1").unwrap();
    // Level 2 reactions are reversible unless they say otherwise.
    assert!(j1.is_reversible);
    assert_eq!(j1.products[0].stoichiometry, 2.0);
    // The local k clashes with the global one, so it is renamed.
    assert_eq!(j1.kinetic_law, "J1_k*E*A");
    assert_eq!(m.find_parameter("J1_k").unwrap().expression, "3");
    assert_eq!(m.find_parameter("k").unwrap().expression, "0.5");
    // E is named in the law, so it is a modifier.
    assert_eq!(j1.modifiers[0].species, "E");

    // Without the catalytic rewrite, B stays on both sides of J2.
    let j2 = m.find_reaction("J2").unwrap();
    assert_eq!(j2.reactants.len(), 1);
    assert!(j2.modifiers.is_empty());
}

#[test]
fn the_catalytic_rewrite_and_null_nodes() {
    let opts = ImportOptions {
        convert_catalytic: true,
        ..OPTS
    };
    let m = sbml::import(CORE_ONLY, &opts).unwrap().model;
    let j2 = m.find_reaction("J2").unwrap();
    // B on both sides: a modifier now, and the empty side gets a null node.
    assert_eq!(j2.modifiers[0].species, "B");
    assert_eq!(j2.reactants[0].species, "_source_J2");
    assert!(m.find_species("_source_J2").unwrap().is_null_node);
    assert_eq!(j2.products[0].species, "A");
}

#[test]
fn errors_leave_no_model() {
    let e = sbml::import("<sbml", &OPTS).unwrap_err();
    assert_eq!(e[0].line, 1);
    let e = sbml::import(
        r#"<sbml xmlns="http://www.sbml.org/sbml/level3/version1/core" level="3" version="1"/>"#,
        &OPTS,
    )
    .unwrap_err();
    assert!(e[0].message.contains("no model"));
}

/// A layout whose glyphs have no positions, as libSBML writes them: every
/// bounding box all zeros.
const EMPTY_LAYOUT: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<sbml xmlns="http://www.sbml.org/sbml/level3/version1/core" xmlns:layout="http://www.sbml.org/sbml/level3/version1/layout/version1" level="3" version="1" layout:required="false">
  <model>
    <listOfCompartments><compartment id="c" constant="true"/></listOfCompartments>
    <listOfSpecies>
      <species id="A" compartment="c" hasOnlySubstanceUnits="false" boundaryCondition="false" constant="false"/>
      <species id="B" compartment="c" hasOnlySubstanceUnits="false" boundaryCondition="false" constant="false"/>
      <species id="C" compartment="c" hasOnlySubstanceUnits="false" boundaryCondition="false" constant="false"/>
    </listOfSpecies>
    <listOfReactions>
      <reaction id="J1" reversible="false">
        <listOfReactants><speciesReference species="A" constant="true"/></listOfReactants>
        <listOfProducts><speciesReference species="B" constant="true"/></listOfProducts>
      </reaction>
      <reaction id="J2" reversible="false">
        <listOfReactants><speciesReference species="B" constant="true"/></listOfReactants>
        <listOfProducts><speciesReference species="C" constant="true"/></listOfProducts>
      </reaction>
    </listOfReactions>
    <layout:listOfLayouts>
      <layout:layout layout:id="L">
        <layout:dimensions layout:width="400" layout:height="300"/>
        <layout:listOfSpeciesGlyphs>
          <layout:speciesGlyph layout:id="gA" layout:species="A">
            <layout:boundingBox><layout:position layout:x="0" layout:y="0"/><layout:dimensions layout:width="0" layout:height="0"/></layout:boundingBox>
          </layout:speciesGlyph>
        </layout:listOfSpeciesGlyphs>
        <layout:listOfReactionGlyphs>
          <layout:reactionGlyph layout:id="g1" layout:reaction="J1">
            <layout:boundingBox><layout:position layout:x="0" layout:y="0"/><layout:dimensions layout:width="0" layout:height="0"/></layout:boundingBox>
          </layout:reactionGlyph>
          <layout:reactionGlyph layout:id="g2" layout:reaction="J2">
            <layout:boundingBox><layout:position layout:x="0" layout:y="0"/><layout:dimensions layout:width="0" layout:height="0"/></layout:boundingBox>
          </layout:reactionGlyph>
        </layout:listOfReactionGlyphs>
      </layout:layout>
    </layout:listOfLayouts>
  </model>
</sbml>"#;

#[test]
fn all_zero_boxes_are_no_positions() {
    let i = sbml::import(EMPTY_LAYOUT, &OPTS).unwrap();
    // Nothing has a position, so it counts as no layout, and gets laid out.
    assert!(!i.had_layout);
    assert_eq!(i.unplaced, 3);
    let m = &i.model;
    let (j1, j2) = (
        m.find_reaction("J1").unwrap(),
        m.find_reaction("J2").unwrap(),
    );
    // Each junction sits between its own species, not at (0, 0).
    assert_ne!(j1.junction, j2.junction);
    let between = |a: &str, b: &str| {
        (m.find_species(a).unwrap().center + m.find_species(b).unwrap().center) / 2.0
    };
    assert!(close(j1.junction, between("A", "B")));
    assert!(close(j2.junction, between("B", "C")));
    assert!(m.find_species("A").unwrap().width > 0.0);
}
