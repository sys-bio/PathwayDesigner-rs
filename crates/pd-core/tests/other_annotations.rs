//! Other tools' annotations (docs/other-annotations-design.md): XML another
//! tool wrote in an item's SBML annotation is kept with the item and written
//! back as it was, with the item's metaid; CellDesigner's and RDF the reader
//! can't take apart are dropped, with a word; native files keep them;
//! Antimony says it can't.

use pd_core::antimony::ImportOptions;
use pd_core::model::Model;

const OPTS: ImportOptions = ImportOptions { seed: 1 };

/// A species `S` with COPASI's XML and an element in a namespace declared
/// only on `<sbml>`; a species `T` with CellDesigner's; a reaction `R` with
/// RDF the reader can't take apart.
const FILE: &str = r##"<?xml version="1.0" encoding="UTF-8"?>
<sbml xmlns="http://www.sbml.org/sbml/level3/version1/core" level="3" version="1"
      xmlns:jd="urn:example:jdesigner">
  <model id="m">
    <listOfCompartments>
      <compartment id="c" size="1" constant="true"/>
    </listOfCompartments>
    <listOfSpecies>
      <species id="S" metaid="meta_S" compartment="c" initialConcentration="1"
               hasOnlySubstanceUnits="false" boundaryCondition="false" constant="false">
        <annotation>
          <COPASI xmlns="http://www.copasi.org/static/sbml"><key>Metabolite_1</key></COPASI>
          <jd:display jd:colour="red"><jd:at x="1"/></jd:display>
        </annotation>
      </species>
      <species id="T" metaid="meta_T" compartment="c" initialConcentration="0"
               hasOnlySubstanceUnits="false" boundaryCondition="false" constant="false">
        <annotation>
          <celldesigner:extension xmlns:celldesigner="http://www.sbml.org/2001/ns/celldesigner"><celldesigner:positionToCompartment>inside</celldesigner:positionToCompartment></celldesigner:extension>
        </annotation>
      </species>
    </listOfSpecies>
    <listOfReactions>
      <reaction id="R" metaid="meta_R" reversible="false">
        <annotation>
          <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#" xmlns:x="urn:x">
            <rdf:Description rdf:about="#meta_R"><x:unknown>1</x:unknown></rdf:Description>
          </rdf:RDF>
        </annotation>
        <listOfReactants><speciesReference species="S" stoichiometry="1" constant="true"/></listOfReactants>
        <listOfProducts><speciesReference species="T" stoichiometry="1" constant="true"/></listOfProducts>
      </reaction>
    </listOfReactions>
  </model>
</sbml>"##;

fn opened() -> pd_core::sbml::Imported {
    pd_core::sbml::import(FILE, &OPTS).unwrap()
}

#[test]
fn another_tools_xml_is_kept_and_written_back() {
    let i = opened();
    let s = &i.model.find_species("S").unwrap().annotation;
    assert_eq!(s.other.len(), 2, "{:?}", s.other);
    assert!(s.other[0].starts_with("<COPASI"));
    // The element that used a prefix declared on <sbml> declares it itself.
    assert!(
        s.other[1].starts_with(r#"<jd:display xmlns:jd="urn:example:jdesigner""#),
        "{}",
        s.other[1]
    );
    assert_eq!(s.metaid.as_deref(), Some("meta_S"));
    assert!(!s.is_empty(), "other tools' XML counts as an annotation");

    // Written back as it was, on the metaid it was read with, and read
    // again the same.
    let xml = pd_core::sbml::export(&i.model).xml;
    assert!(xml.contains(r#"metaid="meta_S""#), "{xml}");
    assert!(xml.contains(
        r#"<COPASI xmlns="http://www.copasi.org/static/sbml"><key>Metabolite_1</key></COPASI>"#
    ));
    assert!(
        xml.contains(r#"<jd:display xmlns:jd="urn:example:jdesigner" jd:colour="red">"#),
        "{xml}"
    );
    assert!(sbml_io::read(&xml).is_ok(), "the file reads");
    let back = pd_core::sbml::import(&xml, &OPTS).unwrap().model;
    assert_eq!(back.find_species("S").unwrap().annotation.other, s.other);
}

/// CellDesigner's own XML is dropped, with a word (O1). RDF the reader
/// can't take apart is kept as it is and written back inside the item's RDF
/// (docs/sbml-full-support-design.md: nothing is lost).
#[test]
fn celldesigners_xml_is_dropped_with_a_word_and_unread_rdf_kept() {
    let i = opened();
    assert!(
        i.model
            .find_species("T")
            .unwrap()
            .annotation
            .other
            .is_empty()
    );
    assert!(
        i.model
            .find_reaction("R")
            .unwrap()
            .annotation
            .other
            .is_empty()
    );
    let said: Vec<&String> = i
        .warnings
        .iter()
        .filter(|w| w.contains("aren't kept"))
        .collect();
    assert_eq!(said.len(), 1, "{:?}", i.warnings);
    assert!(said[0].contains("on 1 item"), "{}", said[0]);
    let r = &i.model.find_reaction("R").unwrap().annotation;
    assert_eq!(r.rdf.len(), 1, "{:?}", r.rdf);
    let xml = pd_core::sbml::export(&i.model).xml;
    assert!(!xml.contains("celldesigner"), "{xml}");
    assert!(xml.contains("x:unknown"), "{xml}");
    let back = pd_core::sbml::import(&xml, &OPTS).unwrap().model;
    assert_eq!(back.find_reaction("R").unwrap().annotation.rdf.len(), 1);
    // Their metaids are kept all the same.
    assert!(
        xml.contains(r#"metaid="meta_T""#) && xml.contains(r#"metaid="meta_R""#),
        "{xml}"
    );
}

/// Two items with one metaid (a pasted copy) can't both have it: the first
/// keeps it, the other gets one from its id; and an id is never made into a
/// metaid another item was read with.
#[test]
fn metaids_stay_unique() {
    let mut m = opened().model;
    let copy = pd_core::clipboard::copy(&{
        let mut sel = m.clone();
        sel.clear_selection();
        sel.find_species_mut("S").unwrap().selected = true;
        sel
    })
    .unwrap();
    pd_core::clipboard::paste(&mut m, &copy, pd_core::geometry::pt(0.0, 100.0));
    let s2 = m.find_species("S_2").unwrap();
    assert_eq!(
        s2.annotation.metaid.as_deref(),
        Some("meta_S"),
        "copied as it was"
    );
    // A species whose id is another's metaid.
    let mut n = pd_core::model::SpeciesNode::new("meta_T", pd_core::geometry::pt(0.0, 300.0));
    n.annotation.references.push(pd_core::model::Reference {
        qualifier: "bqbiol:is".into(),
        resource: "https://identifiers.org/CHEBI:17234".into(),
    });
    m.push_species(n).unwrap();
    // A compartment, written before the species, whose id is S's metaid.
    let mut c = pd_core::model::Compartment::new("meta_S");
    c.annotation.references.push(pd_core::model::Reference {
        qualifier: "bqbiol:is".into(),
        resource: "https://identifiers.org/GO:0005829".into(),
    });
    m.compartments.push(c);
    let xml = pd_core::sbml::export(&m).xml;
    let metaids: Vec<&str> = xml
        .match_indices("metaid=\"")
        .map(|(i, _)| {
            let rest = &xml[i + 8..];
            &rest[..rest.find('"').unwrap()]
        })
        .collect();
    let mut sorted = metaids.clone();
    sorted.sort();
    sorted.dedup();
    assert_eq!(sorted.len(), metaids.len(), "unique: {metaids:?}");
    assert_eq!(metaids.iter().filter(|m| **m == "meta_S").count(), 1);
    assert!(
        metaids.contains(&"meta_T"),
        "kept by the species read with it"
    );
    assert!(sbml_io::read(&xml).is_ok());
    // S and T keep theirs; the ones made from ids go round them.
    let back = pd_core::sbml::import(&xml, &OPTS).unwrap().model;
    let metaid = |id: &str| back.find_species(id).unwrap().annotation.metaid.clone();
    assert_eq!(metaid("S").as_deref(), Some("meta_S"));
    assert_eq!(metaid("T").as_deref(), Some("meta_T"));
}

#[test]
fn native_files_keep_them_in_version_22() {
    let m = opened().model;
    let text = pd_core::json::to_json(&m);
    let back = pd_core::json::from_json(&text).unwrap();
    // 22 brought them; a model read from SBML also has what 23 brought (its
    // model id, for one).
    assert!(back.version >= 22);
    let annotation = |m: &Model| m.find_species("S").unwrap().annotation.clone();
    assert_eq!(annotation(&back.model), annotation(&m));
    assert_eq!(
        back.model
            .find_species("T")
            .unwrap()
            .annotation
            .metaid
            .as_deref(),
        Some("meta_T")
    );
}

/// Antimony with the diagram keeps them, in its `sbml` block; without it,
/// it says it leaves them out (docs/antimony-full-support-design.md S6).
#[test]
fn antimony_says_where_it_cant_hold_them() {
    let m = opened().model;
    let notes = pd_core::antimony::diagram_notes(&m, false);
    assert!(
        !notes.iter().any(|n| n.contains("annotations")),
        "{notes:?}"
    );
    for notes in [
        pd_core::antimony::plain_notes(&m),
        pd_core::antimony::layout_notes(&m),
    ] {
        assert!(
            notes
                .iter()
                .any(|n| n.contains("annotations Antimony has no words for")),
            "{notes:?}"
        );
    }
    assert!(pd_core::antimony::diagram_notes(&Model::new(), false).is_empty());
    assert!(pd_core::antimony::plain_notes(&Model::new()).is_empty());
}

/// SBML Level 3 allows one top-level element per namespace in an
/// annotation; a Level 2 file can have more (JDesigner's `jd:display` beside
/// its `jd:listOfShadows`). The first is kept, the rest dropped with a word,
/// and the file written is valid on that count.
#[test]
fn one_element_per_namespace_is_kept() {
    let file = FILE.replace(
        r#"<jd:display jd:colour="red"><jd:at x="1"/></jd:display>"#,
        r#"<jd:display jd:colour="red"><jd:at x="1"/></jd:display><jd:listOfShadows><jd:shadow/></jd:listOfShadows>"#,
    );
    let i = pd_core::sbml::import(&file, &OPTS).unwrap();
    let other = &i.model.find_species("S").unwrap().annotation.other;
    assert_eq!(other.len(), 2, "{other:?}");
    assert!(other[1].starts_with("<jd:display"));
    assert!(other.iter().all(|x| !x.contains("listOfShadows")));
    let said: Vec<&String> = i
        .warnings
        .iter()
        .filter(|w| w.contains("aren't kept"))
        .collect();
    assert!(said[0].contains("on 2 items"), "{said:?}");
}

/// JDesigner's annotations, its own copy of its diagram, are dropped as
/// CellDesigner's are (O1).
#[test]
fn jdesigners_diagram_is_dropped() {
    let file = FILE.replace("urn:example:jdesigner", "http://www.sys-bio.org/sbml");
    let i = pd_core::sbml::import(&file, &OPTS).unwrap();
    let other = &i.model.find_species("S").unwrap().annotation.other;
    assert_eq!(other.len(), 1, "{other:?}");
    assert!(other[0].starts_with("<COPASI"));
    let said: Vec<&String> = i
        .warnings
        .iter()
        .filter(|w| w.contains("JDesigner"))
        .collect();
    assert!(said[0].contains("on 2 items"), "{said:?}");
}
