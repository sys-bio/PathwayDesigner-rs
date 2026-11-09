//! Finding genes' UniProt entries (docs/uniprot-genes-design.md, step 2).
//! [`GeneSearch`] says which address to fetch next and is given each
//! answer; the app does the fetching, at most one request every
//! [`MIN_INTERVAL_MS`]. UniProt lets web pages read it, so the same search
//! serves the browser and the desktop.
//!
//! With the model's organism (U2), the search first asks UniProt's taxonomy
//! for it, and searches within the nearest taxon that has reviewed proteins:
//! BiGG's *E. coli* K-12 substr. MG1655 (511145) has none, its parent K-12
//! (83333) has them all. Then (U3):
//!
//! 1. every gene by its **label**, a locus name such as a b-number
//!    (`gene:"b3916"`), many genes a request;
//! 2. those not found, by their **name** (`gene_exact:"pfkA"`), only with an
//!    organism, since a name alone matches thousands of entries.
//!
//! A gene's candidates are the entries that list its label (or name) among
//! their gene names, reviewed (Swiss-Prot) entries first. One is *sure*, and
//! ticked in the review, when it is the only one.

use crate::chebi::encode;
use crate::model::{Gene, Id, Model, Reference};

/// The least time between two requests, for the app to keep to.
pub const MIN_INTERVAL_MS: u64 = 100;

const SEARCH: &str = "https://rest.uniprot.org/uniprotkb/search";
const TAXONOMY: &str = "https://rest.uniprot.org/taxonomy";
const FIELDS: &str = "accession,gene_names,gene_oln,protein_name,reviewed";
/// Genes in one request: short enough for any address.
const BATCH: usize = 25;
/// Taxa to step up, at most, for one with reviewed proteins.
const STEPS_UP: usize = 3;

/// A gene to look for.
#[derive(Clone, Debug, PartialEq)]
pub struct GeneQuery {
    pub gene: Id,
    pub label: String,
    pub name: Option<String>,
}

impl GeneQuery {
    pub fn of(g: &Gene) -> Self {
        GeneQuery {
            gene: g.id.clone(),
            label: g.label.clone(),
            name: g.name.clone().filter(|n| !n.trim().is_empty()),
        }
    }
}

/// A UniProt entry.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub accession: String,
    pub protein: String,
    /// Its gene names, the main one first (`pfkA b3916 JW3887`).
    pub gene_names: Vec<String>,
    pub locus_names: Vec<String>,
    pub reviewed: bool,
}

impl Entry {
    /// The reference it makes: the gene product is this protein.
    pub fn reference(&self) -> Reference {
        Reference {
            qualifier: "bqbiol:is".into(),
            resource: format!("https://identifiers.org/uniprot:{}", self.accession),
        }
    }

    /// Its main gene name, `pfkA`.
    pub fn gene_name(&self) -> Option<&str> {
        self.gene_names.first().map(String::as_str)
    }

    fn names(&self, word: &str) -> bool {
        self.gene_names
            .iter()
            .chain(&self.locus_names)
            .any(|n| n.eq_ignore_ascii_case(word))
    }
}

/// How a gene was found.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum By {
    Label,
    Name,
}

/// What was found for one gene.
#[derive(Clone, Debug, PartialEq)]
pub struct GeneFound {
    pub gene: Id,
    pub by: Option<By>,
    pub candidates: Vec<Entry>,
    /// The candidate proposed: the first, reviewed ones first.
    pub chosen: Option<usize>,
    /// The only candidate: the review ticks it.
    pub sure: bool,
}

#[derive(Clone, Debug, PartialEq)]
enum Step {
    /// The taxonomy entry of a taxon, for its reviewed proteins and parent.
    Taxon(String, usize),
    /// Genes, by index, by label.
    Labels(Vec<usize>),
    /// Genes, by index, by name.
    Names(Vec<usize>),
}

/// The search for some genes, one request at a time.
#[derive(Clone, Debug)]
pub struct GeneSearch {
    queries: Vec<GeneQuery>,
    /// The taxon searched within, once known.
    taxon: Option<String>,
    plan: Vec<Step>,
    asked: Option<Step>,
    found: Vec<GeneFound>,
    requests: usize,
    done: usize,
    answered: usize,
    failed: usize,
}

impl GeneSearch {
    /// A search for `queries`, within `organism` (an NCBI Taxonomy number),
    /// if known.
    pub fn new(queries: Vec<GeneQuery>, organism: Option<&str>) -> Self {
        let found = queries
            .iter()
            .map(|q| GeneFound {
                gene: q.gene.clone(),
                by: None,
                candidates: Vec::new(),
                chosen: None,
                sure: false,
            })
            .collect();
        let mut s = GeneSearch {
            queries,
            taxon: None,
            plan: Vec::new(),
            asked: None,
            found,
            requests: 0,
            done: 0,
            answered: 0,
            failed: 0,
        };
        match organism {
            Some(t) => s.plan.push(Step::Taxon(t.to_owned(), 0)),
            None => s.plan_labels(),
        }
        s
    }

    /// The label requests, in batches; each step is popped off the end.
    fn plan_labels(&mut self) {
        let all: Vec<usize> = (0..self.queries.len()).collect();
        let mut steps: Vec<Step> = all
            .chunks(BATCH)
            .map(|c| Step::Labels(c.to_vec()))
            .collect();
        steps.reverse();
        self.plan.extend(steps);
    }

    /// The name requests for the genes not found by label, with a name,
    /// only within a taxon.
    fn plan_names(&mut self) {
        if self.taxon.is_none() {
            return;
        }
        let left: Vec<usize> = (0..self.queries.len())
            .filter(|&i| self.found[i].candidates.is_empty() && self.queries[i].name.is_some())
            .collect();
        let mut steps: Vec<Step> = left
            .chunks(BATCH)
            .map(|c| Step::Names(c.to_vec()))
            .collect();
        steps.reverse();
        self.plan.extend(steps);
    }

    fn url(&self, step: &Step) -> String {
        let within = self
            .taxon
            .as_deref()
            .map(|t| format!(" AND taxonomy_id:{t}"))
            .unwrap_or_default();
        let any = |field: &str, words: Vec<&str>| {
            let parts: Vec<String> = words
                .iter()
                .map(|w| format!("{field}:\"{}\"", w.replace('"', "")))
                .collect();
            format!("({}){within}", parts.join(" OR "))
        };
        match step {
            Step::Taxon(t, _) => format!("{TAXONOMY}/{}?format=json", encode(t)),
            Step::Labels(ix) => {
                let q = any(
                    "gene",
                    ix.iter().map(|&i| self.queries[i].label.as_str()).collect(),
                );
                format!(
                    "{SEARCH}?query={}&fields={FIELDS}&format=tsv&size=500",
                    encode(&q)
                )
            }
            Step::Names(ix) => {
                let q = any(
                    "gene_exact",
                    ix.iter()
                        .filter_map(|&i| self.queries[i].name.as_deref())
                        .collect(),
                );
                format!(
                    "{SEARCH}?query={}&fields={FIELDS}&format=tsv&size=500",
                    encode(&q)
                )
            }
        }
    }

    /// The next address to fetch, or `None` when the search is over.
    pub fn next_url(&mut self) -> Option<String> {
        let step = self.plan.pop()?;
        let url = self.url(&step);
        self.asked = Some(step);
        self.requests += 1;
        Some(url)
    }

    /// The answer to the last address: its body, or why it failed.
    pub fn answer(&mut self, result: Result<String, String>) {
        let Some(step) = self.asked.take() else {
            return;
        };
        let body = match result {
            Ok(b) => {
                self.answered += 1;
                Some(b)
            }
            Err(_) => {
                self.failed += 1;
                None
            }
        };
        match step {
            Step::Taxon(t, up) => {
                let json = body.and_then(|b| serde_json::from_str::<serde_json::Value>(&b).ok());
                let reviewed = json
                    .as_ref()
                    .and_then(|j| j.pointer("/statistics/reviewedProteinCount"))
                    .and_then(serde_json::Value::as_u64)
                    .unwrap_or(1);
                let parent = json
                    .as_ref()
                    .and_then(|j| j.pointer("/parent/taxonId"))
                    .and_then(serde_json::Value::as_u64);
                match parent {
                    Some(p) if reviewed == 0 && up < STEPS_UP => {
                        self.plan.push(Step::Taxon(p.to_string(), up + 1));
                    }
                    _ => {
                        self.taxon = Some(t);
                        self.plan_labels();
                    }
                }
            }
            Step::Labels(ix) => {
                let entries = body.map(|b| entries(&b)).unwrap_or_default();
                for i in ix {
                    let label = self.queries[i].label.clone();
                    self.take(i, &entries, &label, By::Label);
                }
                self.done = self.labelled();
                if !self.plan.iter().any(|s| matches!(s, Step::Labels(_))) {
                    self.plan_names();
                }
            }
            Step::Names(ix) => {
                let entries = body.map(|b| entries(&b)).unwrap_or_default();
                for i in ix {
                    if let Some(name) = self.queries[i].name.clone() {
                        self.take(i, &entries, &name, By::Name);
                    }
                }
            }
        }
    }

    /// Gene `i`'s candidates: the entries naming `word`, reviewed first.
    fn take(&mut self, i: usize, entries: &[Entry], word: &str, by: By) {
        let mut mine: Vec<Entry> = entries.iter().filter(|e| e.names(word)).cloned().collect();
        if mine.is_empty() {
            return;
        }
        mine.sort_by_key(|e| !e.reviewed);
        let f = &mut self.found[i];
        f.sure = mine.len() == 1;
        f.chosen = Some(0);
        f.candidates = mine;
        f.by = Some(by);
    }

    fn labelled(&self) -> usize {
        self.found.iter().filter(|f| f.by.is_some()).count()
    }

    /// How many requests are made, and how many more there will be at least.
    pub fn progress(&self) -> (usize, usize) {
        (self.requests, self.requests + self.plan.len())
    }

    /// The taxon searched within, once the taxonomy has answered.
    pub fn taxon(&self) -> Option<&str> {
        self.taxon.as_deref()
    }

    /// What was found, one per gene, in the order asked.
    pub fn results(&self) -> &[GeneFound] {
        &self.found
    }

    /// UniProt didn't answer at all.
    pub fn unreachable(&self) -> bool {
        self.answered == 0 && self.failed > 0
    }
}

/// The entries in a TSV answer, its header first.
fn entries(tsv: &str) -> Vec<Entry> {
    tsv.lines()
        .skip(1)
        .filter_map(|line| {
            let cols: Vec<&str> = line.split('\t').collect();
            let words = |i: usize| -> Vec<String> {
                cols.get(i)
                    .map(|c| c.split_whitespace().map(str::to_owned).collect())
                    .unwrap_or_default()
            };
            let accession = cols.first()?.trim();
            if accession.is_empty() {
                return None;
            }
            Some(Entry {
                accession: accession.to_owned(),
                gene_names: words(1),
                locus_names: words(2),
                protein: cols.get(3).map(|p| protein_name(p)).unwrap_or_default(),
                reviewed: cols.get(4).is_some_and(|r| r.trim() == "reviewed"),
            })
        })
        .collect()
}

/// A protein's recommended name, without the other names UniProt lists in
/// brackets after it.
fn protein_name(all: &str) -> String {
    let mut depth = 0;
    let mut out = String::new();
    for c in all.chars() {
        match c {
            '(' => depth += 1,
            ')' if depth > 0 => depth -= 1,
            c if depth == 0 => out.push(c),
            _ => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Gives `gene` what the review accepted (U4): `entry`'s reference, the gene
/// SBO term (SBO:0000243) if it has none, and the entry's gene name if it
/// has none. Returns whether anything changed.
pub fn accept(model: &mut Model, gene: &str, entry: &Entry) -> bool {
    let Some(g) = model.genes.iter_mut().find(|g| g.id == gene) else {
        return false;
    };
    let before = g.clone();
    let r = entry.reference();
    if !g.annotation.references.contains(&r) {
        g.annotation.references.push(r);
    }
    g.annotation.sbo.get_or_insert(243);
    if g.name.is_none() {
        g.name = entry.gene_name().map(str::to_owned);
    }
    *g != before
}

// ----- The organism ---------------------------------------------------------------

/// A taxon found in UniProt's taxonomy.
#[derive(Clone, Debug, PartialEq)]
pub struct Taxon {
    pub id: String,
    pub name: String,
    pub common: Option<String>,
}

/// The address that searches UniProt's taxonomy for `name`.
pub fn taxonomy_search(name: &str) -> String {
    format!(
        "{TAXONOMY}/search?query={}&fields=id,scientific_name,common_name&format=tsv&size=10",
        encode(name)
    )
}

/// The taxa in a taxonomy search's answer.
pub fn taxa(tsv: &str) -> Vec<Taxon> {
    tsv.lines()
        .skip(1)
        .filter_map(|line| {
            let cols: Vec<&str> = line.split('\t').collect();
            let id = cols.first()?.trim();
            let name = cols.get(1)?.trim();
            (!id.is_empty() && !name.is_empty()).then(|| Taxon {
                id: id.to_owned(),
                name: name.to_owned(),
                common: cols
                    .get(2)
                    .map(|c| c.trim().to_owned())
                    .filter(|c| !c.is_empty()),
            })
        })
        .collect()
}

/// The BiGG model an Escher map was drawn for, from the map's name
/// (`e_coli_core.Core metabolism` → `e_coli_core`), if it looks like one.
pub fn bigg_model_of(model: &Model) -> Option<String> {
    let (id, _) = model.model_name.split_once('.')?;
    (crate::model::is_valid_sid(id) && id.len() > 2).then(|| id.to_owned())
}

/// The address of BiGG's record of model `id`.
pub fn bigg_model_url(id: &str) -> String {
    format!("https://bigg.ucsd.edu/api/v2/models/{}", encode(id))
}

/// The organism a BiGG model record names.
pub fn bigg_organism(json: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(json).ok()?;
    v.get("organism")?
        .as_str()
        .map(str::to_owned)
        .filter(|o| !o.trim().is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEAD: &str = "Entry\tGene Names\tGene Names (ordered locus)\tProtein names\tReviewed\n";

    fn q(gene: &str, label: &str, name: Option<&str>) -> GeneQuery {
        GeneQuery {
            gene: gene.into(),
            label: label.into(),
            name: name.map(str::to_owned),
        }
    }

    #[test]
    fn genes_by_label_then_name_within_the_organism() {
        let mut s = GeneSearch::new(
            vec![
                q("G_b3916", "b3916", Some("pfkA")),
                q("G_x", "x123", Some("pgk")),
                q("G_y", "y999", None),
            ],
            Some("511145"),
        );
        // The substrain has no reviewed proteins: its parent is searched.
        let url = s.next_url().unwrap();
        assert!(url.ends_with("/taxonomy/511145?format=json"), "{url}");
        s.answer(Ok(
            r#"{"statistics": {"reviewedProteinCount": 0}, "parent": {"taxonId": 83333}}"#.into(),
        ));
        let url = s.next_url().unwrap();
        assert!(url.ends_with("/taxonomy/83333?format=json"), "{url}");
        s.answer(Ok(
            r#"{"statistics": {"reviewedProteinCount": 6067}, "parent": {"taxonId": 562}}"#.into(),
        ));
        assert_eq!(s.taxon(), Some("83333"));
        // All three by label, in one request, within K-12.
        let url = s.next_url().unwrap();
        for part in [
            "gene%3A%22b3916%22",
            "gene%3A%22x123%22",
            "gene%3A%22y999%22",
            "taxonomy_id%3A83333",
        ] {
            assert!(url.contains(part), "{part} in {url}");
        }
        s.answer(Ok(format!(
            "{HEAD}P0A796\tpfkA b3916 JW3887\tb3916 JW3887\tATP-dependent 6-phosphofructokinase isozyme 1 (ATP-PFK 1) (EC 2.7.1.11)\treviewed\n"
        )));
        // The one with a name not found by label: by name.
        let url = s.next_url().unwrap();
        assert!(url.contains("gene_exact%3A%22pgk%22"), "{url}");
        assert!(!url.contains("pfkA"), "found by label, not by name");
        assert!(!url.contains("y999"), "no name, no name search");
        s.answer(Ok(format!(
            "{HEAD}A0A0X\tpgk\t\tPhosphoglycerate kinase\tunreviewed\nP0A799\tpgk b2926\tb2926\tPhosphoglycerate kinase (EC 2.7.2.3)\treviewed\n"
        )));
        assert_eq!(s.next_url(), None);
        let r = s.results();
        assert_eq!(r[0].by, Some(By::Label));
        assert!(r[0].sure);
        assert_eq!(r[0].candidates[0].accession, "P0A796");
        assert_eq!(
            r[0].candidates[0].protein,
            "ATP-dependent 6-phosphofructokinase isozyme 1"
        );
        assert_eq!(r[1].by, Some(By::Name));
        assert!(!r[1].sure, "two candidates");
        assert_eq!(r[1].candidates[0].accession, "P0A799", "reviewed first");
        assert_eq!(r[2].by, None);
        assert!(r[2].candidates.is_empty());
    }

    #[test]
    fn without_an_organism_by_label_only() {
        let mut s = GeneSearch::new(vec![q("G_a", "b3916", Some("pfkA"))], None);
        let url = s.next_url().unwrap();
        assert!(
            url.contains("gene%3A%22b3916%22") && !url.contains("taxonomy_id"),
            "{url}"
        );
        s.answer(Ok(HEAD.into()));
        assert_eq!(s.next_url(), None, "no name search without an organism");
        assert!(s.results()[0].candidates.is_empty());
    }

    #[test]
    fn many_genes_in_batches() {
        let qs: Vec<GeneQuery> = (0..60)
            .map(|i| q(&format!("G_{i}"), &format!("b{i}"), None))
            .collect();
        let mut s = GeneSearch::new(qs, None);
        assert_eq!(s.progress(), (0, 3));
        let mut n = 0;
        while s.next_url().is_some() {
            s.answer(Ok(HEAD.into()));
            n += 1;
        }
        assert_eq!(n, 3);
    }

    #[test]
    fn unreachable_when_nothing_answers() {
        let mut s = GeneSearch::new(vec![q("G_a", "b1", None)], None);
        s.next_url();
        s.answer(Err("offline".into()));
        assert!(s.unreachable());
        let mut s = GeneSearch::new(vec![q("G_a", "b1", None)], Some("83333"));
        s.next_url();
        s.answer(Err("offline".into()));
        s.next_url();
        s.answer(Ok(HEAD.into()));
        assert!(!s.unreachable(), "one answer is enough");
    }

    #[test]
    fn the_taxon_steps_up_a_little_only() {
        // Its count unknown: searched within it.
        let mut s = GeneSearch::new(vec![q("G_a", "b1", None)], Some("511145"));
        s.next_url();
        s.answer(Ok(r#"{"parent": {"taxonId": 83333}}"#.into()));
        assert_eq!(s.taxon(), Some("511145"));
        // No reviewed proteins up a long line: not past STEPS_UP.
        let mut s = GeneSearch::new(vec![q("G_a", "b1", None)], Some("10"));
        let mut taxa = 0;
        while let Some(url) = s.next_url() {
            if url.contains("/taxonomy/") {
                taxa += 1;
                let n = 10 + taxa;
                s.answer(Ok(format!(
                    r#"{{"statistics": {{"reviewedProteinCount": 0}}, "parent": {{"taxonId": {n}}}}}"#
                )));
            } else {
                s.answer(Ok(HEAD.into()));
            }
            assert!(taxa < 10);
        }
        assert_eq!(taxa, STEPS_UP + 1);
    }

    #[test]
    fn names_after_all_labels() {
        let qs: Vec<GeneQuery> = (0..30)
            .map(|i| q(&format!("G_{i}"), &format!("b{i}"), Some(&format!("n{i}"))))
            .collect();
        let mut s = GeneSearch::new(qs, Some("83333"));
        let mut urls = Vec::new();
        while let Some(url) = s.next_url() {
            if url.contains("/taxonomy/") {
                s.answer(Ok(r#"{"statistics": {"reviewedProteinCount": 5}}"#.into()));
            } else {
                s.answer(Ok(HEAD.into()));
            }
            urls.push(url);
        }
        // The taxon, two batches by label, two by name.
        assert_eq!(urls.len(), 5, "{urls:#?}");
        assert!(urls[3].contains("gene_exact"));
    }

    #[test]
    fn a_locus_name_finds_its_entry() {
        let mut s = GeneSearch::new(vec![q("G_a", "b3916", None)], None);
        s.next_url();
        s.answer(Ok(format!(
            "{HEAD}P0A796	pfkA	b3916	PFK	reviewed
"
        )));
        assert_eq!(s.results()[0].candidates.len(), 1);
        let empty = Gene {
            id: "G_a".into(),
            label: "a".into(),
            name: Some(" ".into()),
            ..Default::default()
        };
        assert_eq!(GeneQuery::of(&empty).name, None);
    }

    #[test]
    fn accepted_adds_the_entry_name_and_sbo() {
        let mut m = Model::new();
        m.genes.push(Gene {
            id: "G_b3916".into(),
            label: "b3916".into(),
            ..Default::default()
        });
        let e = entries(&format!(
            "{HEAD}P0A796\tpfkA b3916\tb3916\tPFK 1\treviewed\n"
        ))[0]
            .clone();
        assert!(accept(&mut m, "G_b3916", &e));
        let g = &m.genes[0];
        assert_eq!(
            g.annotation.references[0].resource,
            "https://identifiers.org/uniprot:P0A796"
        );
        assert_eq!(g.annotation.sbo, Some(243));
        assert_eq!(g.name.as_deref(), Some("pfkA"));
        assert!(!accept(&mut m, "G_b3916", &e), "twice: nothing more");
        m.genes[0].annotation.sbo = Some(252);
        m.genes[0].name = Some("own".into());
        let mut other = e.clone();
        other.accession = "P11111".into();
        assert!(accept(&mut m, "G_b3916", &other));
        let g = &m.genes[0];
        assert_eq!(g.annotation.sbo, Some(252), "its own SBO term kept");
        assert_eq!(g.name.as_deref(), Some("own"), "its own name kept");
    }

    #[test]
    fn the_organism_found_and_suggested() {
        let t = taxa(
            "Taxon Id\tScientific name\tCommon name\n83333\tEscherichia coli (strain K12)\t\n559292\tSaccharomyces cerevisiae\tBaker's yeast\n",
        );
        assert_eq!(t[0].id, "83333");
        assert_eq!(t[0].common, None);
        assert_eq!(t[1].common.as_deref(), Some("Baker's yeast"));
        assert!(taxonomy_search("E. coli").contains("query=E.%20coli"));
        let mut m = Model::new();
        m.model_name = "e_coli_core.Core metabolism".into();
        assert_eq!(bigg_model_of(&m).as_deref(), Some("e_coli_core"));
        m.model_name = "Glycolysis".into();
        assert_eq!(bigg_model_of(&m), None);
        m.model_name = "E. coli glycolysis".into();
        assert_eq!(bigg_model_of(&m), None);
        assert_eq!(
            bigg_organism(r#"{"organism": "Escherichia coli str. K-12 substr. MG1655"}"#)
                .as_deref(),
            Some("Escherichia coli str. K-12 substr. MG1655")
        );
    }
}
