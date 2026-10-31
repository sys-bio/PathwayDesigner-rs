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

/// Function definitions, as COPASI writes them: each rate law calls one.
const FUNCTIONS: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<sbml xmlns="http://www.sbml.org/sbml/level2/version4" level="2" version="4">
  <model id="functions">
    <listOfFunctionDefinitions>
      <functionDefinition id="mm">
        <math xmlns="http://www.w3.org/1998/Math/MathML">
          <lambda><bvar><ci>s</ci></bvar><bvar><ci>v</ci></bvar><bvar><ci>k</ci></bvar>
            <apply><divide/><apply><times/><ci>v</ci><ci>s</ci></apply>
              <apply><plus/><ci>k</ci><ci>s</ci></apply></apply>
          </lambda>
        </math>
      </functionDefinition>
      <functionDefinition id="sq">
        <math xmlns="http://www.w3.org/1998/Math/MathML">
          <lambda><bvar><ci>x</ci></bvar><apply><times/><ci>x</ci><ci>x</ci></apply></lambda>
        </math>
      </functionDefinition>
      <functionDefinition id="hill">
        <math xmlns="http://www.w3.org/1998/Math/MathML">
          <lambda><bvar><ci>s</ci></bvar><bvar><ci>k</ci></bvar>
            <apply><divide/><apply><ci>sq</ci><ci>s</ci></apply>
              <apply><plus/><apply><ci>sq</ci><ci>k</ci></apply><apply><ci>sq</ci><ci>s</ci></apply></apply></apply>
          </lambda>
        </math>
      </functionDefinition>
      <functionDefinition id="bad">
        <math xmlns="http://www.w3.org/1998/Math/MathML">
          <lambda><bvar><ci>x</ci></bvar><apply><times/><ci>x</ci><notanumber/></apply></lambda>
        </math>
      </functionDefinition>
    </listOfFunctionDefinitions>
    <listOfCompartments><compartment id="cell" size="1"/></listOfCompartments>
    <listOfSpecies>
      <species id="A" compartment="cell" initialConcentration="1"/>
      <species id="B" compartment="cell" initialConcentration="0"/>
      <species id="E" compartment="cell" initialConcentration="1"/>
    </listOfSpecies>
    <listOfParameters>
      <parameter id="k" value="0.5"/>
      <parameter id="K" value="2"/>
      <parameter id="y"/>
    </listOfParameters>
    <listOfRules>
      <assignmentRule variable="y">
        <math xmlns="http://www.w3.org/1998/Math/MathML"><apply><ci>sq</ci><ci>A</ci></apply></math>
      </assignmentRule>
    </listOfRules>
    <listOfReactions>
      <reaction id="J1">
        <listOfReactants><speciesReference species="A"/></listOfReactants>
        <listOfProducts><speciesReference species="B"/></listOfProducts>
        <kineticLaw>
          <math xmlns="http://www.w3.org/1998/Math/MathML">
            <apply><times/><ci>E</ci><apply><ci>mm</ci><ci>A</ci><ci>Vm</ci><ci>k</ci></apply></apply>
          </math>
          <listOfParameters><parameter id="k" value="3"/><parameter id="Vm" value="10"/></listOfParameters>
        </kineticLaw>
      </reaction>
      <reaction id="J2">
        <listOfReactants><speciesReference species="B"/></listOfReactants>
        <kineticLaw>
          <math xmlns="http://www.w3.org/1998/Math/MathML"><apply><ci>hill</ci><ci>B</ci><ci>K</ci></apply></math>
        </kineticLaw>
      </reaction>
      <reaction id="J3">
        <listOfReactants><speciesReference species="A"/></listOfReactants>
        <kineticLaw>
          <math xmlns="http://www.w3.org/1998/Math/MathML"><apply><ci>bad</ci><ci>A</ci></apply></math>
        </kineticLaw>
      </reaction>
    </listOfReactions>
  </model>
</sbml>"#;

#[test]
fn function_definitions_are_written_into_the_rate_laws() {
    let i = sbml::import(FUNCTIONS, &OPTS).unwrap();
    let m = &i.model;
    // The call becomes the body, with the arguments in place of the
    // parameters, and a local parameter renamed as it is everywhere else.
    let j1 = m.find_reaction("J1").unwrap();
    assert_eq!(j1.kinetic_law, "E*(Vm*A/(J1_k + A))");
    assert_eq!(m.find_parameter("J1_k").unwrap().expression, "3");
    // E is named only in the call's surroundings; A only inside the call.
    assert_eq!(j1.modifiers.len(), 1);
    assert_eq!(j1.modifiers[0].species, "E");
    // A function calling another is expanded all the way down.
    assert_eq!(
        m.find_reaction("J2").unwrap().kinetic_law,
        "B*B/(K*K + B*B)"
    );
    // Rules too.
    assert_eq!(m.assignment_rules[0].expression, "A*A");
    // A function that can't be read leaves out the law that calls it, and
    // says why; nothing calls an undefined function.
    assert_eq!(m.find_reaction("J3").unwrap().kinetic_law, "");
    assert_eq!(
        i.warnings,
        [
            "4 function definitions were written into the rate laws that use them",
            "The rate law of J3 was left out: it uses the function bad, which uses <notanumber/>",
        ]
    );
    // What is exported is valid: no calls left.
    let exported = sbml::export(m);
    assert!(exported.problems.is_empty(), "{:?}", exported.problems);
    assert!(!exported.xml.contains("<ci> mm </ci>") && !exported.xml.contains(">mm<"));
}

/// What the curated BioModels round trip found (docs: the BioModels report):
/// ids that differ only in case, species in amounts, initial amounts,
/// initial assignments to species, a rule's parameter beside a compartment
/// of the same name in another case, and `piecewise`. Read, written, read
/// again: nothing lost, and the written file has no problems.
const BIOMODELS_CASES: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<sbml xmlns="http://www.sbml.org/sbml/level3/version1/core" level="3" version="1">
  <model id="cases">
    <listOfCompartments>
      <compartment id="d" size="0.5" constant="true"/>
    </listOfCompartments>
    <listOfSpecies>
      <species id="A" compartment="d" initialAmount="5" hasOnlySubstanceUnits="true" boundaryCondition="false" constant="false"/>
      <species id="B" compartment="d" initialAmount="2" hasOnlySubstanceUnits="false" boundaryCondition="false" constant="false"/>
      <species id="C" compartment="d" initialConcentration="1" hasOnlySubstanceUnits="false" boundaryCondition="false" constant="false"/>
    </listOfSpecies>
    <listOfParameters>
      <parameter id="r1" value="0.3" constant="true"/>
      <parameter id="k" value="4" constant="true"/>
      <parameter id="D" constant="false"/>
      <parameter id="F" constant="false"/>
    </listOfParameters>
    <listOfInitialAssignments>
      <initialAssignment symbol="C">
        <math xmlns="http://www.w3.org/1998/Math/MathML"><apply><times/><ci>k</ci><cn>2</cn></apply></math>
      </initialAssignment>
    </listOfInitialAssignments>
    <listOfRules>
      <assignmentRule variable="D">
        <math xmlns="http://www.w3.org/1998/Math/MathML">
          <piecewise>
            <piece><cn>1</cn><apply><gt/><ci>A</ci><cn>3</cn></apply></piece>
            <otherwise><cn>0</cn></otherwise>
          </piecewise>
        </math>
      </assignmentRule>
      <assignmentRule variable="F">
        <math xmlns="http://www.w3.org/1998/Math/MathML"><apply><times/><ci>k</ci><cn>3</cn></apply></math>
      </assignmentRule>
    </listOfRules>
    <listOfReactions>
      <reaction id="R1" reversible="false">
        <listOfReactants><speciesReference species="A" stoichiometry="1" constant="true"/></listOfReactants>
        <listOfProducts><speciesReference species="B" stoichiometry="1" constant="true"/></listOfProducts>
        <kineticLaw><math xmlns="http://www.w3.org/1998/Math/MathML"><apply><times/><ci>r1</ci><ci>A</ci><ci>D</ci></apply></math></kineticLaw>
      </reaction>
      <reaction id="R2" reversible="false">
        <listOfReactants><speciesReference species="B" stoichiometry="1" constant="true"/></listOfReactants>
        <kineticLaw>
          <math xmlns="http://www.w3.org/1998/Math/MathML"><apply><times/><ci>F</ci><ci>B</ci></apply></math>
          <listOfLocalParameters><localParameter id="F" value="0.01"/></listOfLocalParameters>
        </kineticLaw>
      </reaction>
    </listOfReactions>
  </model>
</sbml>"#;

#[test]
fn what_the_biomodels_round_trip_found_is_kept() {
    let check = |m: &pd_core::model::Model| {
        // R1 and r1 are two things.
        assert!(m.find_reaction("R1").is_some(), "the reaction R1 kept");
        assert!(m.parameters.iter().any(|p| p.variable == "r1"));
        let a = m.find_species("A").unwrap();
        assert!(a.in_amounts && a.initial_is_amount);
        assert_eq!(a.initial_value, 5.0);
        let b = m.find_species("B").unwrap();
        assert!(!b.in_amounts && b.initial_is_amount);
        assert_eq!(b.initial_value, 2.0);
        // k*2, worked out at the start.
        let c = m.find_species("C").unwrap();
        assert!(!c.initial_is_amount);
        assert_eq!(c.initial_value, 8.0);
        assert_eq!(m.assignment_rules[0].variable, "D");
        // R2's own F, renamed, not merged with the F a rule computes.
        assert_eq!(m.find_reaction("R2").unwrap().kinetic_law, "R2_F*B");
        assert!(
            m.parameters
                .iter()
                .any(|p| p.variable == "R2_F" && p.expression == "0.01")
        );
        assert!(!m.parameters.iter().any(|p| p.variable == "F"));
        assert_eq!(m.assignment_rules[0].expression, "piecewise(1, A > 3, 0)");
    };
    let first = sbml::import(BIOMODELS_CASES, &OPTS).unwrap();
    assert!(
        !first.warnings.iter().any(|w| w.contains("left out")),
        "{:?}",
        first.warnings
    );
    check(&first.model);
    let written = sbml::export(&first.model);
    assert!(written.problems.is_empty(), "{:?}", written.problems);
    // Nothing had to be declared to make the file whole.
    assert!(written.notes.is_empty(), "{:?}", written.notes);
    assert!(written.xml.contains("<piecewise>"));
    assert!(written.xml.contains(r#"hasOnlySubstanceUnits="true""#));
    assert!(written.xml.contains(r#"initialAmount="2""#));
    check(&sbml::import(&written.xml, &OPTS).unwrap().model);
}

/// Conversion factors, and a reaction's stoichiometry used in maths by its
/// id, have nowhere to go in a diagram: the import says so (found by the
/// SBML Test Suite round trip, where they went unannounced).
#[test]
fn what_a_diagram_cannot_hold_is_reported() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<sbml xmlns="http://www.sbml.org/sbml/level3/version1/core" level="3" version="1">
  <model id="m" conversionFactor="f">
    <listOfCompartments><compartment id="c" size="1" constant="true"/></listOfCompartments>
    <listOfSpecies>
      <species id="A" compartment="c" initialAmount="1" hasOnlySubstanceUnits="false" boundaryCondition="false" constant="false" conversionFactor="f"/>
    </listOfSpecies>
    <listOfParameters>
      <parameter id="f" value="2" constant="true"/>
      <parameter id="p" constant="false"/>
    </listOfParameters>
    <listOfRules>
      <assignmentRule variable="p"><math xmlns="http://www.w3.org/1998/Math/MathML"><ci>sr</ci></math></assignmentRule>
    </listOfRules>
    <listOfReactions>
      <reaction id="R" reversible="false">
        <listOfReactants><speciesReference id="sr" species="A" stoichiometry="2" constant="true"/></listOfReactants>
      </reaction>
    </listOfReactions>
  </model>
</sbml>"#;
    let w = sbml::import(xml, &OPTS).unwrap().warnings;
    assert!(
        w.iter().any(|w| w == "2 conversion factors were left out"),
        "{w:?}"
    );
    assert!(
        w.iter().any(|w| w.contains("stoichiometry by its id")),
        "{w:?}"
    );
}

/// An initial assignment may use a value an assignment rule gives
/// (`S1 = 2*k1`, `k1 := 0.75`): rules are read first (found by the SBML Test
/// Suite round trip, where these were left out).
#[test]
fn an_initial_assignment_can_use_a_rule() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<sbml xmlns="http://www.sbml.org/sbml/level3/version1/core" level="3" version="1">
  <model id="m">
    <listOfCompartments><compartment id="c" size="1" constant="true"/></listOfCompartments>
    <listOfSpecies>
      <species id="S1" compartment="c" hasOnlySubstanceUnits="false" boundaryCondition="false" constant="false"/>
    </listOfSpecies>
    <listOfParameters><parameter id="k1" constant="false"/></listOfParameters>
    <listOfInitialAssignments>
      <initialAssignment symbol="S1"><math xmlns="http://www.w3.org/1998/Math/MathML"><apply><times/><cn>2</cn><ci>k1</ci></apply></math></initialAssignment>
    </listOfInitialAssignments>
    <listOfRules>
      <assignmentRule variable="k1"><math xmlns="http://www.w3.org/1998/Math/MathML"><cn>0.75</cn></math></assignmentRule>
    </listOfRules>
  </model>
</sbml>"#;
    let i = sbml::import(xml, &OPTS).unwrap();
    assert!(
        !i.warnings.iter().any(|w| w.contains("left out")),
        "{:?}",
        i.warnings
    );
    assert_eq!(i.model.find_species("S1").unwrap().initial_value, 1.5);
}
