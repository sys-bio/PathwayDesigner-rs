//! All of SBML core through PathwayDesigner and back
//! (docs/sbml-full-support-design.md): a file with one of everything is
//! read, every part is found in the model, and writing it and reading it
//! again gives the same model.

use pd_core::antimony::ImportOptions;
use pd_core::model::Model;
use pd_core::sbml;

const OPTS: ImportOptions = ImportOptions { seed: 7 };

const FULL: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<sbml xmlns="http://www.sbml.org/sbml/level3/version2/core" xmlns:fbc="http://www.sbml.org/sbml/level3/version1/fbc/version2" level="3" version="2" fbc:required="false">
  <model id="full" timeUnits="minute" substanceUnits="mole" conversionFactor="cf">
    <listOfFunctionDefinitions>
      <functionDefinition id="mm" name="Michaelis-Menten">
        <math xmlns="http://www.w3.org/1998/Math/MathML">
          <lambda><bvar><ci>s</ci></bvar><bvar><ci>v</ci></bvar>
            <apply><divide/><apply><times/><ci>v</ci><ci>s</ci></apply><apply><plus/><cn>1</cn><ci>s</ci></apply></apply>
          </lambda>
        </math>
      </functionDefinition>
    </listOfFunctionDefinitions>
    <listOfUnitDefinitions>
      <unitDefinition id="minute"><listOfUnits><unit kind="second" exponent="1" scale="0" multiplier="60"/></listOfUnits></unitDefinition>
      <unitDefinition id="per_min"><listOfUnits><unit kind="second" exponent="-1" scale="0" multiplier="60"/></listOfUnits></unitDefinition>
    </listOfUnitDefinitions>
    <listOfCompartments>
      <compartment id="cell" spatialDimensions="3" units="litre" constant="false"/>
      <compartment id="mem" spatialDimensions="2" size="1" constant="true"/>
    </listOfCompartments>
    <listOfSpecies>
      <species id="A" compartment="cell" initialConcentration="1" substanceUnits="mole" hasOnlySubstanceUnits="false" boundaryCondition="false" constant="false" conversionFactor="cf" fbc:charge="-2" fbc:chemicalFormula="C6H12O6"/>
      <species id="B" compartment="cell" hasOnlySubstanceUnits="true" boundaryCondition="false" constant="false"/>
      <species id="E" compartment="cell" initialAmount="0.1" hasOnlySubstanceUnits="false" boundaryCondition="false" constant="false"/>
    </listOfSpecies>
    <listOfParameters>
      <parameter id="k" name="rate" value="0.5" units="per_min" constant="true"/>
      <parameter id="Vm" value="9" constant="true"/>
      <parameter id="cf" value="1" constant="true"/>
      <parameter id="y" constant="false"/>
      <parameter id="z" value="0" constant="false"/>
      <parameter id="w" constant="false"/>
    </listOfParameters>
    <listOfInitialAssignments>
      <initialAssignment symbol="B"><math xmlns="http://www.w3.org/1998/Math/MathML"><apply><times/><cn type="integer">2</cn><ci>A</ci></apply></math></initialAssignment>
      <initialAssignment symbol="Vm"><math xmlns="http://www.w3.org/1998/Math/MathML"><apply><times/><ci>k</ci><cn>20</cn></apply></math></initialAssignment>
      <initialAssignment symbol="cell"><math xmlns="http://www.w3.org/1998/Math/MathML"><cn>2.5</cn></math></initialAssignment>
    </listOfInitialAssignments>
    <listOfRules>
      <assignmentRule variable="y" sboTerm="SBO:0000064">
        <notes><body xmlns="http://www.w3.org/1999/xhtml"><p>The total.</p></body></notes>
        <math xmlns="http://www.w3.org/1998/Math/MathML"><apply><plus/><ci>A</ci><ci>B</ci></apply></math>
      </assignmentRule>
      <rateRule variable="z"><math xmlns="http://www.w3.org/1998/Math/MathML"><apply><times/><ci>k</ci><ci>A</ci></apply></math></rateRule>
      <rateRule variable="cell"><math xmlns="http://www.w3.org/1998/Math/MathML"><cn>0.01</cn></math></rateRule>
      <algebraicRule><math xmlns="http://www.w3.org/1998/Math/MathML"><apply><minus/><ci>w</ci><apply><times/><cn type="integer">2</cn><ci>y</ci></apply></apply></math></algebraicRule>
    </listOfRules>
    <listOfConstraints>
      <constraint>
        <math xmlns="http://www.w3.org/1998/Math/MathML"><apply><geq/><ci>A</ci><cn type="integer">0</cn></apply></math>
        <message><p xmlns="http://www.w3.org/1999/xhtml">A went negative.</p></message>
      </constraint>
    </listOfConstraints>
    <listOfReactions>
      <reaction id="J1" name="first" reversible="false" compartment="cell">
        <listOfReactants>
          <speciesReference id="n1" name="the substrate" species="A" constant="false"/>
        </listOfReactants>
        <listOfProducts>
          <speciesReference species="B" stoichiometry="2" constant="true"/>
        </listOfProducts>
        <listOfModifiers>
          <modifierSpeciesReference id="m1" name="the enzyme" species="E" sboTerm="SBO:0000461"/>
        </listOfModifiers>
        <kineticLaw sboTerm="SBO:0000029">
          <math xmlns="http://www.w3.org/1998/Math/MathML"><apply><times/><ci>E</ci><apply><ci>mm</ci><ci>A</ci><ci>Vm</ci></apply><ci>k</ci></apply></math>
          <listOfLocalParameters>
            <localParameter id="k" name="its own k" value="3" units="per_min"/>
          </listOfLocalParameters>
        </kineticLaw>
      </reaction>
    </listOfReactions>
    <listOfEvents>
      <event id="Feed" name="feeding" useValuesFromTriggerTime="false">
        <trigger initialValue="false" persistent="false">
          <math xmlns="http://www.w3.org/1998/Math/MathML"><apply><gt/><csymbol encoding="text" definitionURL="http://www.sbml.org/sbml/symbols/time">t</csymbol><cn type="integer">10</cn></apply></math>
        </trigger>
        <delay><math xmlns="http://www.w3.org/1998/Math/MathML"><cn type="integer">2</cn></math></delay>
        <priority><math xmlns="http://www.w3.org/1998/Math/MathML"><cn type="integer">1</cn></math></priority>
        <listOfEventAssignments>
          <eventAssignment variable="A"><math xmlns="http://www.w3.org/1998/Math/MathML"><cn type="integer">5</cn></math></eventAssignment>
        </listOfEventAssignments>
      </event>
      <event id="Never" useValuesFromTriggerTime="true"/>
    </listOfEvents>
  </model>
</sbml>"#;

/// What SBML holds, apart from the drawing, to compare two models by.
fn sbml_parts(m: &Model) -> String {
    let species: Vec<String> = m
        .species()
        .iter()
        .filter(|s| !s.is_alias() && !s.is_null_node)
        .map(|s| {
            format!(
                "{} {:?} {} {} {} {:?} {:?}",
                s.id,
                s.display_name,
                s.initial_value,
                s.initial_is_amount,
                s.in_amounts,
                s.sbml,
                s.annotation
            )
        })
        .collect();
    let reactions: Vec<String> = m
        .reactions()
        .iter()
        .map(|r| {
            let legs: Vec<String> = r
                .reactants
                .iter()
                .chain(&r.products)
                .chain(&r.modifiers)
                .map(|p| {
                    format!(
                        "{} {} {:?} {:?}",
                        p.species, p.stoichiometry, p.kind, p.sbml
                    )
                })
                .collect();
            format!(
                "{} {:?} {} {} {:?} {:?} {:?}",
                r.id, r.display_name, r.kinetic_law, r.is_reversible, r.sbml, legs, r.annotation
            )
        })
        .collect();
    let compartments: Vec<String> = m
        .compartments
        .iter()
        .map(|c| {
            format!(
                "{} {:?} {} {} {:?} {:?}",
                c.id, c.display_name, c.size, c.dimensions, c.sbml, c.annotation
            )
        })
        .collect();
    format!(
        "{:?}\n{:?}\n{:?}\n{:?}\n{:?}\n{:?}\n{:?}\n{:?}\n{:?}\n{:?}\n{:?}\n{:?}\n{species:?}\n{reactions:?}\n{compartments:?}\n{:?}",
        m.model_id,
        m.units,
        m.conversion_factor,
        m.function_definitions,
        m.unit_definitions,
        m.parameters,
        m.initial_assignments,
        m.assignment_rules,
        m.rate_rules,
        m.algebraic_rules,
        m.constraints,
        m.events,
        m.sbml_namespaces,
    )
}

#[test]
fn every_part_is_read() {
    let i = sbml::import(FULL, &OPTS).unwrap();
    assert!(i.warnings.is_empty(), "{:?}", i.warnings);
    let m = &i.model;
    assert_eq!(m.model_id.as_deref(), Some("full"));
    assert!(m.model_name_unset);
    assert_eq!(m.units.time.as_deref(), Some("minute"));
    assert_eq!(m.conversion_factor.as_deref(), Some("cf"));
    assert_eq!(m.function_definitions[0].body, "v*s/(1.0 + s)");
    assert_eq!(m.unit_definitions[1].units[0].exponent, -1.0);
    let cell = m.find_compartment("cell").unwrap();
    assert!(cell.sbml.size_unset && !cell.sbml.constant);
    assert_eq!(cell.sbml.units.as_deref(), Some("litre"));
    // Its size shown is what its initial assignment gives.
    assert_eq!(cell.size, 2.5);
    assert_eq!(m.find_compartment("mem").unwrap().dimensions, 2.0);
    let a = m.find_species("A").unwrap();
    assert_eq!(a.sbml.conversion_factor.as_deref(), Some("cf"));
    assert_eq!(a.annotation.package.attributes.len(), 2);
    assert!(m.find_species("B").unwrap().sbml.initial_unset);
    let k = m.find_parameter("k").unwrap();
    assert_eq!(
        (k.name.as_deref(), k.units.as_deref()),
        (Some("rate"), Some("per_min"))
    );
    assert_eq!(m.parameter_text("Vm"), "k*20.0");
    assert_eq!(m.find_parameter("Vm").unwrap().value, Some(9.0));
    assert_eq!(m.initial_assignment("B").unwrap().expression, "2*A");
    assert_eq!(m.assignment_rules[0].annotation.sbo, Some(64));
    assert!(m.assignment_rules[0].annotation.notes.is_some());
    assert_eq!(m.rate_rules.len(), 2);
    assert_eq!(m.algebraic_rules[0].expression, "w - 2*y");
    assert!(
        m.constraints[0]
            .message
            .as_deref()
            .unwrap()
            .contains("negative")
    );
    let j1 = m.find_reaction("J1").unwrap();
    assert_eq!(j1.kinetic_law, "E*mm(A, Vm)*k");
    assert_eq!(
        j1.sbml.local_parameters[0].name.as_deref(),
        Some("its own k")
    );
    assert_eq!(j1.sbml.compartment.as_deref(), Some("cell"));
    assert_eq!(j1.sbml.kinetic_law_annotation.sbo, Some(29));
    let n1 = j1.reactants[0].sbml.as_deref().unwrap();
    assert_eq!(n1.id.as_deref(), Some("n1"));
    assert!(n1.stoichiometry_unset && !n1.constant);
    let m1 = j1.modifiers[0].sbml.as_deref().unwrap();
    assert_eq!(m1.name.as_deref(), Some("the enzyme"));
    assert_eq!(m1.annotation.sbo, Some(461));
    let feed = &m.events[0];
    assert!(!feed.use_values_from_trigger_time && !feed.trigger.initial_value);
    assert_eq!(feed.trigger.expression, "time > 10");
    assert_eq!(feed.delay.as_ref().unwrap().expression, "2");
    assert_eq!(feed.priority.as_ref().unwrap().expression, "1");
    assert_eq!(feed.assignments[0].variable, "A");
    assert!(m.events[1].no_trigger);
}

#[test]
fn written_and_read_again_it_is_the_same() {
    let first = sbml::import(FULL, &OPTS).unwrap().model;
    let written = sbml::export(&first);
    assert!(written.problems.is_empty(), "{:?}", written.problems);
    assert!(written.notes.is_empty(), "{:?}", written.notes);
    assert!(written.xml.contains(r#"level="3" version="2""#));
    let again = sbml::import(&written.xml, &OPTS).unwrap();
    assert!(again.warnings.is_empty(), "{:?}", again.warnings);
    assert_eq!(sbml_parts(&again.model), sbml_parts(&first));
    // And through the native file.
    let native = pd_core::json::from_json(&pd_core::json::to_json(&first)).unwrap();
    assert_eq!(sbml_parts(&native.model), sbml_parts(&first));
}

/// Level 2 with what the BioModels round trip found: its default units, a
/// function that uses the time (allowed in Level 2 Version 1, not in a
/// Level 3 function), a Level 3 layout straight in the model, and Level 2's
/// `constant` on a rate law's parameter.
const LEVEL_2: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<sbml xmlns="http://www.sbml.org/sbml/level2" xmlns:layout="http://www.sbml.org/sbml/level3/version1/layout/version1" level="2" version="1">
  <model id="old">
    <listOfFunctionDefinitions>
      <functionDefinition id="wave">
        <math xmlns="http://www.w3.org/1998/Math/MathML">
          <lambda><bvar><ci>p</ci></bvar>
            <apply><sin/><apply><divide/><csymbol encoding="text" definitionURL="http://www.sbml.org/sbml/symbols/time">t</csymbol><ci>p</ci></apply></apply>
          </lambda>
        </math>
      </functionDefinition>
    </listOfFunctionDefinitions>
    <listOfUnitDefinitions>
      <unitDefinition id="substance"><listOfUnits><unit kind="mole" scale="-3"/></listOfUnits></unitDefinition>
    </listOfUnitDefinitions>
    <listOfCompartments><compartment id="c" size="1"/></listOfCompartments>
    <listOfSpecies><species id="A" compartment="c" initialConcentration="1"/></listOfSpecies>
    <listOfParameters><parameter id="T" value="2" units="time"/></listOfParameters>
    <listOfReactions>
      <reaction id="J" reversible="false">
        <listOfReactants><speciesReference species="A"/></listOfReactants>
        <kineticLaw>
          <math xmlns="http://www.w3.org/1998/Math/MathML"><apply><times/><ci>k</ci><apply><ci>wave</ci><ci>T</ci></apply></apply></math>
          <listOfParameters><parameter id="k" value="1" constant="true"/></listOfParameters>
        </kineticLaw>
      </reaction>
    </listOfReactions>
    <layout:listOfLayouts>
      <layout:layout layout:id="theirs"><layout:dimensions layout:width="10" layout:height="10"/></layout:layout>
    </layout:listOfLayouts>
  </model>
</sbml>"#;

#[test]
fn what_level_2_leaves_unsaid_is_said_in_level_3() {
    let i = sbml::import(LEVEL_2, &OPTS).unwrap();
    let m = &i.model;
    // Level 2's defaults, made explicit as libSBML does: `substance` is the
    // file's millimole; the rest as Level 2 has them.
    assert_eq!(m.units.substance.as_deref(), Some("substance"));
    assert_eq!(m.units.volume.as_deref(), Some("volume"));
    assert_eq!(m.units.time.as_deref(), Some("second"));
    assert_eq!(
        m.find_compartment("c").unwrap().sbml.units.as_deref(),
        Some("volume")
    );
    assert_eq!(
        m.find_species("A").unwrap().sbml.units.as_deref(),
        Some("substance")
    );
    let defined: Vec<&str> = m.unit_definitions.iter().map(|u| u.id.as_str()).collect();
    for id in ["substance", "volume", "area", "time"] {
        assert!(defined.contains(&id), "{id}: {defined:?}");
    }
    assert_eq!(
        m.unit_definitions
            .iter()
            .find(|u| u.id == "substance")
            .unwrap()
            .units[0]
            .scale,
        -3
    );
    // The function using the time is written into its call, with a word.
    assert!(m.function_definitions.is_empty());
    assert_eq!(m.find_reaction("J").unwrap().kinetic_law, "k*sin(time/T)");
    assert!(
        i.warnings.iter().any(|w| w.contains("The function wave")),
        "{:?}",
        i.warnings
    );
    // Their layout isn't kept as another package's part beside ours, and
    // Level 2's `constant` isn't written into Level 3.
    let written = sbml::export(m);
    assert_eq!(written.xml.matches("<layout:listOfLayouts").count(), 1);
    assert!(!written.xml.contains(r#"layout:id="theirs""#));
    assert!(
        !written
            .xml
            .contains(r#"<localParameter id="k" value="1" constant"#)
    );
    assert!(written.problems.is_empty(), "{:?}", written.problems);
}

/// A function whose argument is called `time` keeps it as its argument,
/// both ways (found by the BioModels round trip); and a rate law's local
/// parameter named as a species of its reaction, which Level 3 Version 2
/// forbids, makes the file Version 1 (found by the SBML Test Suite).
#[test]
fn names_that_mean_something_else_elsewhere() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<sbml xmlns="http://www.sbml.org/sbml/level3/version1/core" level="3" version="1">
  <model id="m">
    <listOfFunctionDefinitions>
      <functionDefinition id="decay">
        <math xmlns="http://www.w3.org/1998/Math/MathML">
          <lambda><bvar><ci>g</ci></bvar><bvar><ci>time</ci></bvar><apply><exp/><apply><times/><ci>g</ci><ci>time</ci></apply></apply></lambda>
        </math>
      </functionDefinition>
    </listOfFunctionDefinitions>
    <listOfCompartments><compartment id="c" size="1" spatialDimensions="3" constant="true"/></listOfCompartments>
    <listOfSpecies><species id="A" compartment="c" initialConcentration="1" hasOnlySubstanceUnits="false" boundaryCondition="false" constant="false"/></listOfSpecies>
    <listOfReactions>
      <reaction id="J" reversible="false" fast="false">
        <listOfReactants><speciesReference species="A" stoichiometry="1" constant="true"/></listOfReactants>
        <kineticLaw>
          <math xmlns="http://www.w3.org/1998/Math/MathML"><apply><times/><ci>A</ci><apply><ci>decay</ci><cn>1</cn><csymbol encoding="text" definitionURL="http://www.sbml.org/sbml/symbols/time">t</csymbol></apply></apply></math>
          <listOfLocalParameters><localParameter id="A" value="3"/></listOfLocalParameters>
        </kineticLaw>
      </reaction>
    </listOfReactions>
  </model>
</sbml>"#;
    let i = sbml::import(xml, &OPTS).unwrap();
    assert!(i.warnings.is_empty(), "{:?}", i.warnings);
    let f = &i.model.function_definitions[0];
    assert_eq!(
        (f.arguments.as_slice(), f.body.as_str()),
        (&["g".to_owned(), "time".to_owned()][..], "exp(g*time)")
    );
    let written = sbml::export(&i.model);
    assert!(written.problems.is_empty(), "{:?}", written.problems);
    assert!(written.xml.contains(r#"level="3" version="1""#));
    // In the function, `time` is its argument; in the rate law, the clock.
    let lambda =
        &written.xml[written.xml.find("<lambda>").unwrap()..written.xml.find("</lambda>").unwrap()];
    assert!(!lambda.contains("csymbol"), "{lambda}");
    let law = &written.xml[written.xml.find("<kineticLaw").unwrap()..];
    assert!(law.contains("symbols/time"));
}

/// A model its file gives no id, or no name (Level 3 Version 2 allows
/// both), is written back without them.
#[test]
fn a_model_without_id_or_name_stays_so() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<sbml xmlns="http://www.sbml.org/sbml/level3/version2/core" level="3" version="2">
  <model>
    <listOfCompartments><compartment id="c" size="1" spatialDimensions="3" constant="true"/></listOfCompartments>
  </model>
</sbml>"#;
    let m = sbml::import(xml, &OPTS).unwrap().model;
    assert!(m.model_id_unset && m.model_name_unset);
    let written = sbml::export(&m).xml;
    let model = &written[written.find("<model").unwrap()..];
    let tag = &model[..model.find('>').unwrap()];
    assert!(!tag.contains(" id=") && !tag.contains(" name="), "{tag}");
}

// ----- Through Antimony (docs/antimony-full-support-design.md) -------------------

/// [`sbml_parts`], parameters, initial assignments and each reaction's
/// modifiers in id order: Antimony may list them in another order, which
/// SBML gives no meaning.
fn sbml_parts_any_order(m: &Model) -> String {
    let mut m = m.clone();
    m.parameters.sort_by(|a, b| a.variable.cmp(&b.variable));
    m.initial_assignments
        .sort_by(|a, b| a.symbol.cmp(&b.symbol));
    let ids: Vec<String> = m.reactions().iter().map(|r| r.id.clone()).collect();
    for id in ids {
        let r = m.find_reaction_mut(&id).unwrap();
        r.modifiers.sort_by(|a, b| a.species.cmp(&b.species));
    }
    sbml_parts(&m)
}

/// Antimony with its `sbml` block (S4, S5) read back is the model again.
#[test]
fn through_antimony_with_its_block_it_is_the_same() {
    let first = sbml::import(FULL, &OPTS).unwrap().model;
    let text = pd_core::antimony::export_with_diagram(&first);
    assert!(text.contains("/* sbml full v1"), "{text}");
    let again = pd_core::antimony::import(&text, &OPTS).unwrap();
    assert_eq!(
        sbml_parts_any_order(&again.model),
        sbml_parts_any_order(&first)
    );
}

/// Plain Antimony, loaded over the model it was got from, keeps what the
/// text can't say (S3); read on its own it can't.
#[test]
fn plain_antimony_loaded_over_its_model_keeps_the_rest() {
    let first = sbml::import(FULL, &OPTS).unwrap().model;
    let text = pd_core::antimony::export(&first);
    assert!(!text.contains("/* sbml"), "{text}");
    let alone = pd_core::antimony::import(&text, &OPTS).unwrap().model;
    assert_ne!(sbml_parts_any_order(&alone), sbml_parts_any_order(&first));
    let mut view = pd_core::view::DiagramView::new(first.clone());
    view.load_antimony(&text, &OPTS).unwrap();
    assert_eq!(
        sbml_parts_any_order(view.model()),
        sbml_parts_any_order(&first)
    );
    // One undo step back to the model it replaced.
    view.undo();
    assert_eq!(
        sbml_parts_any_order(view.model()),
        sbml_parts_any_order(&first)
    );
}

/// What the text changes is changed; the rest of what it can't say is kept.
#[test]
fn plain_antimony_edited_keeps_what_it_left_alone() {
    let first = sbml::import(FULL, &OPTS).unwrap().model;
    let text = pd_core::antimony::export(&first).replace("cf = 1;", "cf = 2;");
    let mut view = pd_core::view::DiagramView::new(first.clone());
    view.load_antimony(&text, &OPTS).unwrap();
    let m = view.model();
    let cf = m.parameters.iter().find(|p| p.variable == "cf").unwrap();
    assert_eq!(cf.value, Some(2.0));
    // J1's own k, and the event's name, which plain Antimony can't say.
    let j1 = m.find_reaction("J1").unwrap();
    assert_eq!(j1.sbml.local_parameters[0].id, "k");
    assert_eq!(j1.sbml.local_parameters[0].value, Some(3.0));
    assert_eq!(m.events[0].name.as_deref(), Some("feeding"));
}

/// A text with a block of its own is read by it, not by the diagram it
/// replaces.
#[test]
fn a_texts_own_block_comes_first() {
    let first = sbml::import(FULL, &OPTS).unwrap().model;
    let text = pd_core::antimony::export_with_diagram(&first);
    let other = pd_core::antimony::import("model other\n  J1: A -> B; k2*A\n  k2 = 1\nend", &OPTS)
        .unwrap()
        .model;
    let mut view = pd_core::view::DiagramView::new(other);
    view.load_antimony(&text, &OPTS).unwrap();
    assert_eq!(
        sbml_parts_any_order(view.model()),
        sbml_parts_any_order(&first)
    );
}

/// What the BioModels and Test Suite round trips through Antimony found,
/// each once: ids Antimony reserves (a function, a unit, an event), a unit
/// with a parameter's id, ids differing only in case, infinities, a name
/// with a double quote, NaN given by a formula, a rate law's parameter it
/// doesn't use, species
/// references set by a rule and an initial assignment, modifiers Antimony
/// can't say, formulas the file left out, a parameter with no value, and a
/// reaction's rate in another's law.
const EDGES: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<sbml xmlns="http://www.sbml.org/sbml/level3/version2/core" level="3" version="2">
  <model id="edges">
    <listOfFunctionDefinitions>
      <functionDefinition id="max">
        <math xmlns="http://www.w3.org/1998/Math/MathML"><lambda><bvar><ci>x</ci></bvar><apply><times/><cn type="integer">2</cn><ci>x</ci></apply></lambda></math>
      </functionDefinition>
    </listOfFunctionDefinitions>
    <listOfUnitDefinitions>
      <unitDefinition id="time" name="hour"><listOfUnits><unit kind="second" exponent="1" scale="0" multiplier="3600"/></listOfUnits></unitDefinition>
      <unitDefinition id="BW" metaid="u_BW"><listOfUnits><unit kind="gram" exponent="1" scale="3" multiplier="1"/></listOfUnits></unitDefinition>
    </listOfUnitDefinitions>
    <listOfCompartments><compartment id="c" size="1" spatialDimensions="3" constant="true"/></listOfCompartments>
    <listOfSpecies>
      <species id="A" name="the &quot;A&quot;" compartment="c" initialConcentration="1" hasOnlySubstanceUnits="false" boundaryCondition="false" constant="false"/>
      <species id="B" compartment="c" initialConcentration="0" hasOnlySubstanceUnits="false" boundaryCondition="false" constant="false"/>
      <species id="E" compartment="c" initialConcentration="1" hasOnlySubstanceUnits="false" boundaryCondition="false" constant="false"/>
    </listOfSpecies>
    <listOfParameters>
      <parameter id="BW" name="weight" value="70" units="BW" constant="true" sboTerm="SBO:0000002"/>
      <parameter id="R" value="0" constant="true"/>
      <parameter id="INF" value="10" constant="true"/>
      <parameter id="Inf" value="1" constant="true"/>
      <parameter id="x" constant="true"/>
      <parameter id="up" value="INF" constant="true"/>
      <parameter id="nothing" value="NaN" constant="true"/>
      <parameter id="nv" constant="false"/>
      <parameter id="empty" value="3" constant="false"/>
      <parameter id="k" value="1" constant="true"/>
    </listOfParameters>
    <listOfInitialAssignments>
      <initialAssignment symbol="x"><math xmlns="http://www.w3.org/1998/Math/MathML"><ci>INF</ci></math></initialAssignment>
      <initialAssignment symbol="n2"><math xmlns="http://www.w3.org/1998/Math/MathML"><cn>3</cn></math></initialAssignment>
      <initialAssignment symbol="Inf"/>
      <initialAssignment symbol="R"><math xmlns="http://www.w3.org/1998/Math/MathML"><notanumber/></math></initialAssignment>
    </listOfInitialAssignments>
    <listOfRules>
      <assignmentRule variable="n1"><math xmlns="http://www.w3.org/1998/Math/MathML"><apply><ci>max</ci><ci>k</ci></apply></math></assignmentRule>
      <assignmentRule variable="empty"/>
      <rateRule variable="nv"><math xmlns="http://www.w3.org/1998/Math/MathML"><ci>k</ci></math></rateRule>
    </listOfRules>
    <listOfReactions>
      <reaction id="J1" reversible="false">
        <listOfReactants>
          <speciesReference id="n1" species="A" constant="false"/>
          <speciesReference species="B" stoichiometry="1" constant="true"/>
        </listOfReactants>
        <listOfProducts><speciesReference id="n2" species="B" constant="false"/></listOfProducts>
        <listOfModifiers>
          <modifierSpeciesReference species="E" sboTerm="SBO:0000461"/>
          <modifierSpeciesReference species="E" sboTerm="SBO:0000020"/>
          <modifierSpeciesReference species="B"/>
        </listOfModifiers>
        <kineticLaw>
          <math xmlns="http://www.w3.org/1998/Math/MathML"><apply><times/><ci>k</ci><ci>A</ci></apply></math>
          <listOfLocalParameters><localParameter id="unused" value="2"/></listOfLocalParameters>
        </kineticLaw>
      </reaction>
      <reaction id="J2" reversible="false">
        <listOfProducts><speciesReference species="A" stoichiometry="1" constant="true"/></listOfProducts>
        <kineticLaw><math xmlns="http://www.w3.org/1998/Math/MathML"><apply><plus/><ci>J1</ci><cn type="integer">1</cn></apply></math></kineticLaw>
      </reaction>
    </listOfReactions>
    <listOfEvents>
      <event id="event" useValuesFromTriggerTime="true">
        <trigger initialValue="true" persistent="true"/>
        <listOfEventAssignments><eventAssignment variable="k"><math xmlns="http://www.w3.org/1998/Math/MathML"><cn>2</cn></math></eventAssignment></listOfEventAssignments>
      </event>
    </listOfEvents>
  </model>
</sbml>"#;

#[test]
fn what_the_round_trips_found_comes_back_through_antimony() {
    let first = sbml::import(EDGES, &OPTS).unwrap();
    assert!(first.warnings.is_empty(), "{:?}", first.warnings);
    let first = first.model;
    // Each case is in the model.
    assert_eq!(first.find_reaction("J1").unwrap().modifiers.len(), 3);
    assert_eq!(first.events[0].id.as_deref(), Some("event"));
    let text = pd_core::antimony::export_with_diagram(&first);
    let again = pd_core::antimony::import(&text, &OPTS).unwrap();
    assert_eq!(
        sbml_parts_any_order(&again.model),
        sbml_parts_any_order(&first),
        "{text}"
    );
    // And as Load Antimony after Get Antimony.
    let plain = pd_core::antimony::export(&first);
    let kept = pd_core::antimony::import_keeping(&plain, &OPTS, &first).unwrap();
    assert_eq!(
        sbml_parts_any_order(&kept.model),
        sbml_parts_any_order(&first),
        "{plain}"
    );
    // Plain Antimony alone says what it leaves out.
    let notes = pd_core::antimony::plain_notes(&first).join("\n");
    for what in [
        "Ids Antimony reserves were written with `_` added",
        "each rate law's own parameters",
        "modifiers the rate law doesn't use",
        "rules and events without a formula",
        "some names",
    ] {
        assert!(notes.contains(what), "{what}: {notes}");
    }
}

/// docs/sbml-app-design.md A8: a model built from submodels says so on
/// opening, and its submodels are written back.
#[test]
fn a_model_of_submodels_says_so() {
    let xml = r#"<?xml version="1.0" encoding="UTF-8"?>
<sbml xmlns="http://www.sbml.org/sbml/level3/version1/core" xmlns:comp="http://www.sbml.org/sbml/level3/version1/comp/version1" level="3" version="1" comp:required="true">
  <model id="top">
    <listOfParameters><parameter id="p" value="1" constant="true"/></listOfParameters>
    <comp:listOfSubmodels><comp:submodel comp:id="sub1" comp:modelRef="inner"/></comp:listOfSubmodels>
  </model>
  <comp:listOfModelDefinitions>
    <comp:modelDefinition id="inner"><listOfParameters><parameter id="q" value="2" constant="true"/></listOfParameters></comp:modelDefinition>
  </comp:listOfModelDefinitions>
</sbml>"#;
    let i = sbml::import(xml, &OPTS).unwrap();
    assert!(
        i.warnings
            .iter()
            .any(|w| w.contains("built from submodels")),
        "{:?}",
        i.warnings
    );
    let written = sbml::export(&i.model).xml;
    assert!(
        written.contains("comp:submodel") && written.contains("modelDefinition"),
        "{written}"
    );
    // A model without them says nothing of it.
    let plain = sbml::import(FULL, &OPTS).unwrap();
    assert!(!plain.warnings.iter().any(|w| w.contains("submodels")));
}

/// docs/sbml-app-design.md A4: with the preference, SBML and Antimony are
/// written with the functions in the formulas that call them; the diagram
/// keeps them; without it, they are written as functions.
#[test]
fn functions_written_in_when_asked() {
    let first = sbml::import(FULL, &OPTS).unwrap().model;
    let inlined = first.with_functions_written_in();
    assert!(inlined.function_definitions.is_empty());
    let law = &inlined.find_reaction("J1").unwrap().kinetic_law;
    assert!(!law.contains("mm(") && law.contains("Vm"), "{law}");
    // What calls nothing is as it was.
    assert_eq!(inlined.assignment_rules, first.assignment_rules);

    let mut view = pd_core::view::DiagramView::new(first.clone());
    assert!(view.export_sbml().xml.contains("<functionDefinition"));
    assert!(view.export_antimony().contains("function mm("));
    view.write_functions_in = true;
    let xml = view.export_sbml().xml;
    assert!(!xml.contains("<functionDefinition"), "{xml}");
    assert!(!view.export_antimony().contains("function mm("));
    assert!(
        !view
            .export_antimony_with_diagram(false)
            .0
            .contains("function mm(")
    );
    assert_eq!(
        view.model().function_definitions.len(),
        1,
        "the diagram keeps it"
    );
    // The SBML means the same: read back, J1's rate is the same formula.
    let back = sbml::import(&xml, &OPTS).unwrap().model;
    assert_eq!(back.find_reaction("J1").unwrap().kinetic_law, *law);

    // A call that can't be written in keeps its function.
    let mut odd = first.clone();
    odd.find_reaction_mut("J1").unwrap().kinetic_law = "mm(A)*k".into();
    let kept = odd.with_functions_written_in();
    assert_eq!(kept.function_definitions.len(), 1);
    assert_eq!(kept.find_reaction("J1").unwrap().kinetic_law, "mm(A)*k");
}

/// A model drawn with no compartments, saved as SBML and opened again, still
/// has none: the default compartment the file needs is the implied one. A
/// file's own `defaultCompartment` that says more, or that a formula uses,
/// is kept.
#[test]
fn the_implied_default_compartment_stays_implied() {
    let m = pd_core::antimony::import("A -> B; k*A\nk = 1", &OPTS)
        .unwrap()
        .model;
    assert!(m.compartments.is_empty());
    let xml = sbml::export(&m).xml;
    assert!(xml.contains(r#"id="defaultCompartment""#), "{xml}");
    let back = sbml::import(&xml, &OPTS).unwrap().model;
    assert!(back.compartments.is_empty(), "{:?}", back.compartments);
    assert_eq!(sbml::export(&back).xml, xml);

    for own in [
        xml.replace(
            r#"id="defaultCompartment""#,
            r#"id="defaultCompartment" name="cell""#,
        ),
        xml.replace("<ci> k </ci>", "<ci> defaultCompartment </ci>"),
    ] {
        assert_ne!(own, xml);
        let kept = sbml::import(&own, &OPTS).unwrap().model;
        assert_eq!(kept.compartments.len(), 1, "{own}");
    }
}
