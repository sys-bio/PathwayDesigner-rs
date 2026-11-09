//! Genes and gene–reaction rules (docs/gene-rules-design.md): read from an
//! Escher map, written as SBML's fbc package and read back, and kept in the
//! native file.

use pd_core::antimony::ImportOptions;
use pd_core::model::Model;
use pd_core::sbml;

const OPTS: ImportOptions = ImportOptions { seed: 1 };
const MAP: &str = include_str!("../../../Escher/e_coli_core.Core metabolism.json");

fn rule(m: &Model, reaction: &str) -> Option<String> {
    m.find_reaction(reaction)?.sbml.gene_rule.clone()
}

fn map() -> Model {
    pd_core::escher::import(MAP).unwrap().model
}

#[test]
fn an_escher_maps_genes_are_read() {
    let m = map();
    assert_eq!(rule(&m, "PGK").as_deref(), Some("G_b2926"));
    assert_eq!(rule(&m, "PFK").as_deref(), Some("G_b3916 or G_b1723"));
    assert_eq!(
        rule(&m, "CYTBD").as_deref(),
        Some("(G_b0978 and G_b0979) or (G_b0733 and G_b0734)")
    );
    let pfka = m.genes.iter().find(|g| g.id == "G_b3916").unwrap();
    assert_eq!(
        (pfka.label.as_str(), pfka.name.as_deref()),
        ("b3916", Some("pfkA"))
    );
    // Each gene once.
    let mut ids: Vec<&str> = m.genes.iter().map(|g| g.id.as_str()).collect();
    let n = ids.len();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), n);
    assert!(n > 100, "{n}");
}

#[test]
fn genes_go_through_sbml_as_fbc() {
    let m = map();
    let xml = sbml::export(&m).xml;
    for part in [
        "fbc:listOfGeneProducts",
        r#"fbc:id="G_b3916" fbc:label="b3916" fbc:name="pfkA""#,
        "fbc:geneProductAssociation",
        r#"fbc:required="false""#,
        r#"fbc:strict="false""#,
    ] {
        assert!(xml.contains(part), "{part}");
    }
    let back = sbml::import(&xml, &OPTS).unwrap().model;
    assert_eq!(back.genes, m.genes);
    for r in m.reactions() {
        assert_eq!(rule(&back, &r.id), r.sbml.gene_rule, "{}", r.id);
    }
    // Written again, the same fbc: as read, it is kept. (The layout's
    // coordinates can differ in their last digit, as drawing recomputes them.)
    let fbc = |x: &str| -> Vec<String> {
        x.lines()
            .filter(|l| l.contains("fbc:"))
            .map(str::to_owned)
            .collect()
    };
    let again = sbml::export(&back).xml;
    assert!(again.matches("<fbc:geneProductAssociation>").count() > 50);
    assert_eq!(fbc(&again), fbc(&xml));
}

#[test]
fn an_edited_rule_is_written_new() {
    let m = map();
    let xml = sbml::export(&m).xml;
    let mut back = sbml::import(&xml, &OPTS).unwrap().model;
    back.find_reaction_mut("PFK").unwrap().sbml.gene_rule = Some("G_b3916".into());
    back.find_reaction_mut("PGK").unwrap().sbml.gene_rule = None;
    let again = sbml::import(&sbml::export(&back).xml, &OPTS).unwrap().model;
    assert_eq!(rule(&again, "PFK").as_deref(), Some("G_b3916"));
    assert_eq!(rule(&again, "PGK"), None);
}

#[test]
fn genes_are_kept_in_the_native_file() {
    let m = map();
    let json = pd_core::json::to_json(&m);
    assert!(
        json.contains("\"geneRule\": \"G_b3916 or G_b1723\""),
        "rule"
    );
    assert!(
        json.contains("\"version\": 24"),
        "the version that has genes"
    );
    let back = pd_core::json::from_json(&json).unwrap().model;
    assert_eq!(back.genes, m.genes);
    assert_eq!(rule(&back, "PFK"), rule(&m, "PFK"));
    // A model without genes stays in an older version.
    let plain = pd_core::antimony::import("A -> B; k*A\nk = 1", &OPTS)
        .unwrap()
        .model;
    assert!(pd_core::json::needed_version(&plain) < 24);
}

/// Antimony has no words for genes: they go in its `sbml` block, and come
/// back; plain Antimony says it leaves them out, and Load Antimony keeps them.
#[test]
fn genes_go_through_antimony() {
    let m = map();
    let text = pd_core::antimony::export_with_diagram(&m);
    assert!(text.contains("\"geneRule\""), "the block has the rules");
    let back = pd_core::antimony::import(&text, &OPTS).unwrap().model;
    assert_eq!(back.genes, m.genes);
    for r in m.reactions() {
        assert_eq!(rule(&back, &r.id), r.sbml.gene_rule, "{}", r.id);
    }
    let notes = pd_core::antimony::plain_notes(&m).join("\n");
    assert!(notes.contains("genes and reactions' gene rules"), "{notes}");
    let plain = pd_core::antimony::export(&m);
    let kept = pd_core::antimony::import_keeping(&plain, &OPTS, &m)
        .unwrap()
        .model;
    assert_eq!(kept.genes, m.genes);
    assert_eq!(rule(&kept, "PFK"), rule(&m, "PFK"));
}

/// A native file from before version 24 has a model's fbc genes only as
/// the XML kept with it: they are read out of it, not lost on saving.
#[test]
fn an_older_files_fbc_genes_are_read() {
    let xml = sbml::export(&map()).xml;
    let m = sbml::import(&xml, &OPTS).unwrap().model;
    let mut old = m.clone();
    old.genes.clear();
    old.reactions_mut().for_each(|r| r.sbml.gene_rule = None);
    let json = pd_core::json::to_json(&old);
    assert!(json.contains("\"version\": 23"), "written as version 23");
    let back = pd_core::json::from_json(&json).unwrap().model;
    assert_eq!(back.genes, m.genes);
    assert_eq!(rule(&back, "PFK"), rule(&m, "PFK"));
}

/// A model read from SBML has its genes in the fbc XML it keeps, which
/// Antimony's block carries as it is: the genes come back out of it.
#[test]
fn genes_from_sbml_go_through_antimony() {
    let m = sbml::import(&sbml::export(&map()).xml, &OPTS)
        .unwrap()
        .model;
    let text = pd_core::antimony::export_with_diagram(&m);
    assert!(!text.contains("\"geneRule\""), "the fbc XML says them");
    let back = pd_core::antimony::import(&text, &OPTS).unwrap().model;
    assert_eq!(back.genes, m.genes);
    assert_eq!(rule(&back, "PFK"), rule(&m, "PFK"));
}

/// docs/uniprot-genes-design.md U1: a gene's annotation (its SBO term,
/// references and notes) goes through SBML, the native file and Antimony.
#[test]
fn a_genes_annotation_is_kept() {
    use pd_core::model::{Annotation, Reference};
    let mut m = map();
    let pfka = m.genes.iter_mut().find(|g| g.id == "G_b3916").unwrap();
    pfka.annotation = Annotation {
        sbo: Some(243),
        references: vec![Reference {
            qualifier: "bqbiol:is".into(),
            resource: "https://identifiers.org/uniprot:P0A796".into(),
        }],
        notes: Some("<p xmlns=\"http://www.w3.org/1999/xhtml\">Isozyme 1.</p>".into()),
        ..Annotation::default()
    };
    let gene = |m: &Model| {
        m.genes
            .iter()
            .find(|g| g.id == "G_b3916")
            .unwrap()
            .annotation
            .clone()
    };
    let want = gene(&m);
    let xml = sbml::export(&m).xml;
    assert!(xml.contains("uniprot:P0A796"), "the RDF is written");
    let back = sbml::import(&xml, &OPTS).unwrap().model;
    let got = gene(&back);
    assert_eq!(
        (got.sbo, &got.references, &got.notes),
        (want.sbo, &want.references, &want.notes)
    );
    let json = pd_core::json::from_json(&pd_core::json::to_json(&back))
        .unwrap()
        .model;
    assert_eq!(gene(&json), got);
    let ant = pd_core::antimony::import(&pd_core::antimony::export_with_diagram(&back), &OPTS)
        .unwrap()
        .model;
    assert_eq!(gene(&ant), got);
    // A gene from the map, whose annotation no fbc XML holds yet: Antimony's
    // block has it.
    let ant = pd_core::antimony::import(&pd_core::antimony::export_with_diagram(&m), &OPTS)
        .unwrap()
        .model;
    assert_eq!(gene(&ant), want);
    // Unchanged, written as read.
    assert_eq!(
        sbml::export(&back).xml.matches("uniprot:P0A796").count(),
        xml.matches("uniprot:P0A796").count()
    );
}

/// A gene product with references in its RDF, as BiGG writes them, is read
/// with them.
#[test]
fn a_bigg_gene_products_references_are_read() {
    let xml = r##"<?xml version="1.0" encoding="UTF-8"?>
<sbml xmlns="http://www.sbml.org/sbml/level3/version1/core" xmlns:fbc="http://www.sbml.org/sbml/level3/version1/fbc/version2" level="3" version="1" fbc:required="false">
  <model id="m" fbc:strict="false">
    <fbc:listOfGeneProducts>
      <fbc:geneProduct metaid="G_b3916" sboTerm="SBO:0000243" fbc:id="G_b3916" fbc:label="b3916" fbc:name="pfkA">
        <annotation>
          <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#" xmlns:bqbiol="http://biomodels.net/biology-qualifiers/">
            <rdf:Description rdf:about="#G_b3916">
              <bqbiol:is><rdf:Bag><rdf:li rdf:resource="http://identifiers.org/uniprot/P0A796"/></rdf:Bag></bqbiol:is>
            </rdf:Description>
          </rdf:RDF>
        </annotation>
      </fbc:geneProduct>
    </fbc:listOfGeneProducts>
  </model>
</sbml>"##;
    let m = sbml::import(xml, &OPTS).unwrap().model;
    let a = &m.genes[0].annotation;
    assert_eq!(a.sbo, Some(243));
    assert_eq!(a.metaid.as_deref(), Some("G_b3916"));
    assert_eq!(
        a.references[0].resource,
        "http://identifiers.org/uniprot/P0A796"
    );
}

/// docs/uniprot-genes-design.md U2: the model's organism, a taxonomy
/// reference, read in its spellings, set, and kept through SBML, the native
/// file and Antimony.
#[test]
fn the_organism_is_kept() {
    use pd_core::genes::{organism, set_organism};
    let mut m = map();
    assert_eq!(organism(&m), None);
    for spelling in [
        "https://identifiers.org/taxonomy:562",
        "http://identifiers.org/taxonomy/562",
        "urn:miriam:taxonomy:562",
    ] {
        m.annotation.references = vec![pd_core::model::Reference {
            qualifier: "bqbiol:hasTaxon".into(),
            resource: spelling.into(),
        }];
        assert_eq!(organism(&m).as_deref(), Some("562"), "{spelling}");
    }
    set_organism(&mut m, "83333");
    assert_eq!(organism(&m).as_deref(), Some("83333"));
    assert_eq!(m.annotation.references.len(), 1, "in place of the other");
    let back = sbml::import(&sbml::export(&m).xml, &OPTS).unwrap().model;
    assert_eq!(organism(&back).as_deref(), Some("83333"));
    let json = pd_core::json::from_json(&pd_core::json::to_json(&m))
        .unwrap()
        .model;
    assert_eq!(organism(&json).as_deref(), Some("83333"));
    let ant = pd_core::antimony::import(&pd_core::antimony::export_with_diagram(&m), &OPTS)
        .unwrap()
        .model;
    assert_eq!(organism(&ant).as_deref(), Some("83333"));
}

/// docs/uniprot-genes-design.md step 3: the genes looked up, the organism
/// set, what is accepted applied, each one undo step.
#[test]
fn genes_to_annotate_and_applied() {
    use pd_core::view::DiagramView;
    let mut m = map();
    let all = m.genes.len();
    m.find_reaction_mut("PFK").unwrap().selected = true;
    let mut v = DiagramView::new(m);
    let ids = |v: &DiagramView| -> Vec<String> {
        v.genes_to_annotate().into_iter().map(|q| q.gene).collect()
    };
    let mut picked = ids(&v);
    picked.sort();
    assert_eq!(picked, ["G_b1723", "G_b3916"]);
    assert_eq!(v.reaction_of_gene("G_b3916").as_deref(), Some("PFK"));
    assert_eq!(v.reaction_of_gene("G_none"), None);

    assert_eq!(v.organism(), None);
    v.set_organism("83333");
    assert_eq!(v.organism().as_deref(), Some("83333"));
    assert_eq!(v.undo_description(), Some("Set Organism"));
    // Another reference after it: the same organism again adds no step.
    let mut a = v.model().annotation.clone();
    a.references.push(pd_core::model::Reference {
        qualifier: "bqbiol:is".into(),
        resource: "https://identifiers.org/bigg.model:e_coli_core".into(),
    });
    v.set_annotation(None, a);
    v.set_organism("83333");
    assert_eq!(v.undo_description(), Some("Edit Annotation"));

    let entry = pd_core::uniprot::Entry {
        accession: "P0A796".into(),
        protein: "PFK 1".into(),
        gene_names: vec!["pfkA".into()],
        locus_names: vec!["b3916".into()],
        reviewed: true,
    };
    v.annotate_genes(&[("G_b3916".into(), entry)]);
    assert_eq!(v.undo_description(), Some("Annotate Genes"));
    let g = v.model().genes.iter().find(|g| g.id == "G_b3916").unwrap();
    assert_eq!(
        pd_core::uniprot::entry_of(&g.annotation).as_deref(),
        Some("P0A796")
    );
    // Annotated, it isn't looked up again.
    assert_eq!(ids(&v), ["G_b1723"]);
    // Nothing selected: every gene not yet annotated.
    let mut m = v.model().clone();
    m.find_reaction_mut("PFK").unwrap().selected = false;
    assert_eq!(ids(&DiagramView::new(m)).len(), all - 1);
}
