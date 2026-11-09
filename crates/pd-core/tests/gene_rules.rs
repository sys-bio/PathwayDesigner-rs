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
