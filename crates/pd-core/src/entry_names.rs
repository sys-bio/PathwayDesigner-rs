//! What a reference's entry is called, fetched when it isn't known
//! (`CHEBI:16452` → *oxaloacetate(2-)*). [`name_url`] says where to ask and
//! [`parse_name`] reads the answer; the app does the fetching, at most one
//! request every [`crate::chebi::MIN_INTERVAL_MS`]. Every service here lets
//! web pages read it, so the browser and the desktop ask the same way.
//!
//! - ChEBI, GO, NCBI Taxonomy, MAMO and SBO terms: EBI's Ontology Lookup
//!   Service, by the term's id.
//! - Rhea reactions: Rhea, which gives the equation.
//! - UniProt proteins: UniProt, the protein's recommended name.
//! - PubMed articles: NCBI, the title.
//!
//! Others (EC numbers, KEGG, MetaNetX) have no such service and are named only
//! when a lookup found the name.

use serde_json::Value;

use crate::annotation::resource_text;
use crate::chebi::encode;

const OLS: &str = "https://www.ebi.ac.uk/ols4/api/ontologies";
const RHEA: &str = "https://www.rhea-db.org/rhea";
const UNIPROT: &str = "https://rest.uniprot.org/uniprotkb";
const PUBMED: &str = "https://eutils.ncbi.nlm.nih.gov/entrez/eutils/esummary.fcgi";

/// Where an entry's name comes from.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Entry {
    /// An ontology in OLS, and the term's id there (`CHEBI:16452`).
    Ols(&'static str, String),
    Rhea(String),
    UniProt(String),
    PubMed(String),
}

/// The database and id of a reference, however it is written:
/// `https://identifiers.org/CHEBI:16452`, `http://identifiers.org/chebi/CHEBI:16452`,
/// `urn:miriam:chebi:CHEBI%3A16452`, `http://identifiers.org/taxonomy/562`.
fn entry(resource: &str) -> Option<Entry> {
    let text = resource_text(resource);
    let (db, id) = text.split_once('/').or_else(|| text.split_once(':'))?;
    let db = db.to_ascii_lowercase();
    let db = db.strip_prefix("obo.").unwrap_or(&db);
    let id = id.trim();
    // The number after an optional `PREFIX:` or `PREFIX_`.
    let number = |prefix: &str| -> Option<String> {
        let n = id
            .strip_prefix(&format!("{prefix}:"))
            .or_else(|| id.strip_prefix(&format!("{prefix}_")))
            .unwrap_or(id);
        (!n.is_empty() && n.chars().all(|c| c.is_ascii_digit())).then(|| n.to_owned())
    };
    Some(match db {
        "chebi" => Entry::Ols("chebi", format!("CHEBI:{}", number("CHEBI")?)),
        "go" => Entry::Ols("go", format!("GO:{}", number("GO")?)),
        "sbo" => Entry::Ols("sbo", format!("SBO:{}", number("SBO")?)),
        "mamo" => Entry::Ols("mamo", format!("MAMO:{}", number("MAMO")?)),
        "taxonomy" | "ncbitaxon" => {
            Entry::Ols("ncbitaxon", format!("NCBITaxon:{}", number("NCBITaxon")?))
        }
        "rhea" => Entry::Rhea(number("RHEA")?),
        "uniprot" => {
            let ok = !id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric());
            Entry::UniProt(ok.then(|| id.to_owned())?)
        }
        "pubmed" => Entry::PubMed(number("PMID")?),
        _ => return None,
    })
}

/// Where to ask what `resource`'s entry is called; `None` for an entry no
/// service names.
pub fn name_url(resource: &str) -> Option<String> {
    Some(match entry(resource)? {
        Entry::Ols(ontology, id) => format!("{OLS}/{ontology}/terms?obo_id={}", encode(&id)),
        Entry::Rhea(n) => {
            format!("{RHEA}?query=RHEA%3A{n}&columns=rhea-id,equation&format=tsv")
        }
        Entry::UniProt(acc) => format!("{UNIPROT}/{acc}.json?fields=protein_name"),
        Entry::PubMed(n) => format!("{PUBMED}?db=pubmed&id={n}&retmode=json"),
    })
}

/// The name in the answer to [`name_url`] for `resource`, if it has one.
pub fn parse_name(resource: &str, body: &str) -> Option<String> {
    let name = match entry(resource)? {
        Entry::Rhea(_) => body.lines().nth(1)?.split('\t').nth(1).map(str::to_owned)?,
        Entry::Ols(..) => {
            let json: Value = serde_json::from_str(body).ok()?;
            json.pointer("/_embedded/terms/0/label")?
                .as_str()?
                .to_owned()
        }
        Entry::UniProt(_) => {
            let json: Value = serde_json::from_str(body).ok()?;
            let d = json.get("proteinDescription")?;
            d.pointer("/recommendedName/fullName/value")
                .or_else(|| d.pointer("/submissionNames/0/fullName/value"))?
                .as_str()?
                .to_owned()
        }
        Entry::PubMed(n) => {
            let json: Value = serde_json::from_str(body).ok()?;
            json.pointer(&format!("/result/{n}/title"))?
                .as_str()?
                .to_owned()
        }
    };
    let name = name.trim();
    (!name.is_empty()).then(|| name.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_way_of_writing_a_reference_is_read() {
        let chebi = Some(Entry::Ols("chebi", "CHEBI:16452".into()));
        for r in [
            "https://identifiers.org/CHEBI:16452",
            "http://identifiers.org/chebi/CHEBI:16452",
            "urn:miriam:chebi:CHEBI%3A16452",
        ] {
            assert_eq!(entry(r), chebi, "{r}");
        }
        let taxon = Some(Entry::Ols("ncbitaxon", "NCBITaxon:562".into()));
        assert_eq!(entry("http://identifiers.org/taxonomy/562"), taxon);
        assert_eq!(entry("https://identifiers.org/taxonomy:562"), taxon);
        let go = Some(Entry::Ols("go", "GO:0006096".into()));
        assert_eq!(entry("https://identifiers.org/GO:0006096"), go);
        assert_eq!(entry("http://identifiers.org/go/GO:0006096"), go);
        assert_eq!(entry("urn:miriam:obo.go:GO%3A0006096"), go);
        let mamo = Some(Entry::Ols("mamo", "MAMO:0000046".into()));
        assert_eq!(entry("http://identifiers.org/mamo/MAMO_0000046"), mamo);
        assert_eq!(entry("https://identifiers.org/mamo:MAMO_0000046"), mamo);
        assert_eq!(
            entry("https://identifiers.org/rhea:28370"),
            Some(Entry::Rhea("28370".into()))
        );
        assert_eq!(
            entry("http://identifiers.org/uniprot/P12345"),
            Some(Entry::UniProt("P12345".into()))
        );
        assert_eq!(
            entry("http://identifiers.org/pubmed/8983160"),
            Some(Entry::PubMed("8983160".into()))
        );
        // No service, or not an id.
        assert_eq!(entry("https://identifiers.org/ec-code:4.1.1.31"), None);
        assert_eq!(entry("https://identifiers.org/kegg.compound:C00036"), None);
        assert_eq!(entry("https://identifiers.org/CHEBI:abc"), None);
        assert_eq!(entry("https://example.org/page"), None);
        assert_eq!(name_url("https://identifiers.org/ec-code:4.1.1.31"), None);
    }

    #[test]
    fn addresses_and_answers() {
        let r = "https://identifiers.org/CHEBI:16452";
        assert_eq!(
            name_url(r).unwrap(),
            "https://www.ebi.ac.uk/ols4/api/ontologies/chebi/terms?obo_id=CHEBI%3A16452"
        );
        let ols = r#"{"_embedded": {"terms": [{"label": "oxaloacetate(2-)"}]}}"#;
        assert_eq!(parse_name(r, ols).as_deref(), Some("oxaloacetate(2-)"));
        assert_eq!(parse_name(r, r#"{"status": 404}"#), None);

        let r = "https://identifiers.org/rhea:28370";
        assert_eq!(
            name_url(r).unwrap(),
            "https://www.rhea-db.org/rhea?query=RHEA%3A28370&columns=rhea-id,equation&format=tsv"
        );
        let tsv = "Reaction identifier\tEquation\nRHEA:28370\toxaloacetate + phosphate = phosphoenolpyruvate + hydrogencarbonate\n";
        assert_eq!(
            parse_name(r, tsv).as_deref(),
            Some("oxaloacetate + phosphate = phosphoenolpyruvate + hydrogencarbonate")
        );
        assert_eq!(parse_name(r, "Reaction identifier\tEquation\n"), None);

        let r = "http://identifiers.org/uniprot/P12345";
        assert_eq!(
            name_url(r).unwrap(),
            "https://rest.uniprot.org/uniprotkb/P12345.json?fields=protein_name"
        );
        let up = r#"{"proteinDescription": {"recommendedName": {"fullName": {"value": "Aspartate aminotransferase, mitochondrial"}}}}"#;
        assert_eq!(
            parse_name(r, up).as_deref(),
            Some("Aspartate aminotransferase, mitochondrial")
        );
        let unreviewed =
            r#"{"proteinDescription": {"submissionNames": [{"fullName": {"value": "Kinase"}}]}}"#;
        assert_eq!(parse_name(r, unreviewed).as_deref(), Some("Kinase"));

        let r = "http://identifiers.org/pubmed/8983160";
        assert_eq!(
            name_url(r).unwrap(),
            "https://eutils.ncbi.nlm.nih.gov/entrez/eutils/esummary.fcgi?db=pubmed&id=8983160&retmode=json"
        );
        let pm = r#"{"result": {"8983160": {"title": "A kinetic mechanism."}}}"#;
        assert_eq!(parse_name(r, pm).as_deref(), Some("A kinetic mechanism."));
    }
}
