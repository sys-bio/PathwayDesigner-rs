//! Finding a species' ChEBI entry from its name (docs/chebi-lookup-design.md,
//! step 1). [`Search`] says which address to fetch next and is given each
//! answer; the app does the fetching, at most one request every
//! [`MIN_INTERVAL_MS`] (C8). All three services let web pages read them, so
//! the same search serves the browser and the desktop.
//!
//! For each species, in order (C1):
//!
//! 1. **BiGG**, by its name, then its id, lower case, with and without a
//!    compartment ending (`g6p_c`): a full name, its ChEBI entries, and its
//!    KEGG and MetaNetX links (C4). If none is a BiGG id, BiGG's search for
//!    the first, taking the result that is that word with a stereo ending
//!    (`asp` → `asp__L`, L-aspartate). Then **OLS4** for that full name, which
//!    names BiGG's ChEBI entries, so the one named as BiGG names it can be
//!    proposed (C2).
//! 2. Else **OLS4**, ChEBI searched for exactly the name, among molecular
//!    entities only (CHEBI:23367): roles (`… inhibitor`) and groups
//!    (`maleoyl group`) are left out.
//! 3. Else **PubChem**: the name's compound, whose synonyms include its ChEBI
//!    number.
//!
//! OLS4's free search is not used: tried on the Millard model, every match
//! it alone gave was wrong. Short abbreviations still find wrong things by
//! exact search (`MAL`, a maleoyl group), so a match is *sure*, and ticked
//! in the review, only when its name is the name searched for, or, from
//! BiGG, when one entry ChEBI names is named as BiGG names the compound.

use serde_json::Value;

use crate::model::{Id, Reference};

/// The least time between two requests (C8), for the app to keep to.
pub const MIN_INTERVAL_MS: u64 = 100;

const BIGG: &str = "https://bigg.ucsd.edu/api/v2/universal/metabolites";
const BIGG_SEARCH: &str = "https://bigg.ucsd.edu/api/v2/search";
/// ChEBI's molecular entities, the branch an exact search keeps to.
const MOLECULAR_ENTITY: &str = "http://purl.obolibrary.org/obo/CHEBI_23367";
const OLS: &str = "https://www.ebi.ac.uk/ols4/api/search";
const PUBCHEM: &str = "https://pubchem.ncbi.nlm.nih.gov/rest/pug/compound/name";

/// A species to look for: its id, and its display name if it has one.
#[derive(Clone, Debug, PartialEq)]
pub struct Query {
    pub species: Id,
    pub name: Option<String>,
}

/// Where an answer came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Bigg,
    Chebi,
    PubChem,
}

impl Source {
    pub fn text(self) -> &'static str {
        match self {
            Source::Bigg => "BiGG",
            Source::Chebi => "ChEBI",
            Source::PubChem => "PubChem",
        }
    }
}

/// A ChEBI entry found: `CHEBI:4170`, and its name if known.
#[derive(Clone, Debug, PartialEq)]
pub struct Candidate {
    pub chebi: String,
    pub name: Option<String>,
}

impl Candidate {
    /// The reference it makes: `bqbiol:is` its identifiers.org link.
    pub fn reference(&self) -> Reference {
        Reference {
            qualifier: "bqbiol:is".into(),
            resource: format!("https://identifiers.org/{}", self.chebi),
        }
    }
}

/// What was found for one species.
#[derive(Clone, Debug, PartialEq)]
pub struct Found {
    pub species: Id,
    /// `None` when nothing was found.
    pub source: Option<Source>,
    /// The compound's full name, as BiGG or ChEBI gives it.
    pub full_name: Option<String>,
    pub candidates: Vec<Candidate>,
    /// The candidate proposed (C2).
    pub chosen: Option<usize>,
    /// One clear match: the review ticks it (C2).
    pub sure: bool,
    /// From BiGG, its KEGG compound and MetaNetX chemical (C4).
    pub other_references: Vec<Reference>,
}

/// What the review accepted for one species: the references to add (its
/// ChEBI entry, and BiGG's KEGG and MetaNetX links, C4) and, if ticked, the
/// display name to give it (C3).
#[derive(Clone, Debug, PartialEq)]
pub struct Accepted {
    pub species: Id,
    pub references: Vec<Reference>,
    pub display_name: Option<String>,
}

/// One request: what it is for, and its address.
#[derive(Clone, Debug, PartialEq)]
enum Step {
    /// BiGG, by one of the words to try.
    Bigg(String),
    /// BiGG's search, for a word that isn't a BiGG id.
    BiggSearch(String),
    /// OLS4 for the names of BiGG's entries, by BiGG's full name.
    BiggNames(String),
    Exact(String),
    PubChem(String),
}

impl Step {
    fn url(&self) -> String {
        match self {
            Step::Bigg(w) => format!("{BIGG}/{}", encode(w)),
            Step::BiggSearch(w) => {
                format!("{BIGG_SEARCH}?query={}&search_type=metabolites", encode(w))
            }
            Step::BiggNames(n) => {
                format!("{OLS}?q={}&ontology=chebi&exact=true&rows=20", encode(n))
            }
            Step::Exact(n) => format!(
                "{OLS}?q={}&ontology=chebi&exact=true&rows=20&allChildrenOf={}",
                encode(n),
                encode(MOLECULAR_ENTITY)
            ),
            Step::PubChem(n) => format!("{PUBCHEM}/{}/synonyms/JSON", encode(n)),
        }
    }
}

/// The search for some species, one request at a time.
#[derive(Clone, Debug)]
pub struct Search {
    queries: Vec<Query>,
    /// The species being looked for.
    current: usize,
    /// Its requests still to make, in order.
    plan: Vec<Step>,
    asked: Option<Step>,
    /// What has been found for it so far.
    found: Found,
    /// The name searched for in ChEBI and PubChem.
    searched: String,
    results: Vec<Found>,
    answered: usize,
    failed: usize,
}

impl Search {
    pub fn new(queries: Vec<Query>) -> Self {
        let mut s = Search {
            queries,
            current: 0,
            plan: Vec::new(),
            asked: None,
            found: nothing(""),
            searched: String::new(),
            results: Vec::new(),
            answered: 0,
            failed: 0,
        };
        s.start();
        s
    }

    /// Sets up the requests for the current species, if there is one.
    fn start(&mut self) {
        let Some(q) = self.queries.get(self.current) else {
            return;
        };
        self.found = nothing(&q.species);
        let name = q.name.clone().filter(|n| !n.trim().is_empty());
        let mut words: Vec<String> = Vec::new();
        for w in name.iter().chain(std::iter::once(&q.species)) {
            for w in bigg_words(w) {
                if !words.contains(&w) {
                    words.push(w);
                }
            }
        }
        // Reversed: steps are popped off the end.
        let text = name.unwrap_or_else(|| q.species.clone());
        self.searched = text.clone();
        self.plan = vec![Step::PubChem(text.clone()), Step::Exact(text)];
        if let Some(first) = words.first() {
            self.plan.push(Step::BiggSearch(first.clone()));
        }
        self.plan.extend(words.into_iter().rev().map(Step::Bigg));
    }

    /// The next address to fetch, or `None` when the search is over.
    pub fn next_url(&mut self) -> Option<String> {
        while self.current < self.queries.len() {
            if let Some(step) = self.plan.pop() {
                let url = step.url();
                self.asked = Some(step);
                return Some(url);
            }
            self.finish();
        }
        None
    }

    /// Ends the current species with what was found, and starts the next.
    fn finish(&mut self) {
        let mut found = std::mem::replace(&mut self.found, nothing(""));
        choose(&mut found, &self.searched);
        self.results.push(found);
        self.current += 1;
        self.start();
    }

    /// The answer to the last address: its body, or why it failed (a
    /// not-found counts as an answer, with nothing in it).
    pub fn answer(&mut self, result: Result<String, String>) {
        let Some(step) = self.asked.take() else {
            return;
        };
        let body = match result {
            Ok(b) => {
                self.answered += 1;
                b
            }
            Err(why) => {
                // A 404 is BiGG or PubChem saying it has no such name.
                if why.contains("404") || why.contains("400") {
                    self.answered += 1;
                } else {
                    self.failed += 1;
                }
                return;
            }
        };
        let Ok(json) = serde_json::from_str::<Value>(&body) else {
            return;
        };
        match step {
            Step::Bigg(_) => {
                let Some(name) = json.get("name").and_then(Value::as_str) else {
                    return;
                };
                let name = bigg_name(name);
                let name = name.as_str();
                let links = json.get("database_links");
                let ids = |key: &str| -> Vec<String> {
                    links
                        .and_then(|l| l.get(key))
                        .and_then(Value::as_array)
                        .map(|a| {
                            a.iter()
                                .filter_map(|x| x.get("id").and_then(Value::as_str))
                                .map(str::to_owned)
                                .collect()
                        })
                        .unwrap_or_default()
                };
                let chebi: Vec<String> = ids("CHEBI")
                    .into_iter()
                    .map(|c| {
                        if c.starts_with("CHEBI:") {
                            c
                        } else {
                            format!("CHEBI:{c}")
                        }
                    })
                    .collect();
                let mut other = Vec::new();
                for k in ids("KEGG Compound") {
                    other.push(link(&format!("kegg.compound:{k}")));
                }
                for m in ids("MetaNetX (MNX) Chemical") {
                    other.push(link(&format!("metanetx.chemical:{m}")));
                }
                self.found.full_name = Some(name.to_owned());
                self.found.other_references = other;
                if chebi.is_empty() {
                    // BiGG knows it but not its ChEBI entry: ChEBI by its
                    // full name.
                    self.searched = name.to_owned();
                    self.plan = vec![Step::PubChem(name.to_owned()), Step::Exact(name.to_owned())];
                    return;
                }
                self.found.source = Some(Source::Bigg);
                self.found.candidates = chebi
                    .into_iter()
                    .map(|c| Candidate {
                        chebi: c,
                        name: None,
                    })
                    .collect();
                // Their names, then this species is done.
                self.plan = vec![Step::BiggNames(name.to_owned())];
            }
            Step::BiggSearch(word) => {
                // The result that is this word with a stereo ending, in
                // BiGG's order: `asp__L` before `asp__D`.
                let found = json
                    .get("results")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(|r| r.get("bigg_id").and_then(Value::as_str))
                    .find(|id| {
                        id.split_once("__")
                            .is_some_and(|(base, _)| base.eq_ignore_ascii_case(&word))
                    });
                if let Some(id) = found {
                    self.plan.push(Step::Bigg(id.to_owned()));
                }
            }
            Step::BiggNames(full) => {
                let docs = ols_docs(&json);
                for (id, label) in &docs {
                    if let Some(c) = self.found.candidates.iter_mut().find(|c| c.chebi == *id) {
                        c.name = Some(label.clone());
                    }
                }
                // Entries ChEBI names exactly as BiGG names the compound,
                // which BiGG may not list, first; then BiGG's named ones;
                // its unnamed ones, often numbers ChEBI has retired, last.
                let mut ordered: Vec<Candidate> = docs
                    .into_iter()
                    .filter(|(id, label)| {
                        same(label, &full) && !self.found.candidates.iter().any(|c| c.chebi == *id)
                    })
                    .map(|(chebi, name)| Candidate {
                        chebi,
                        name: Some(name),
                    })
                    .collect();
                let (named, unnamed): (Vec<Candidate>, Vec<Candidate>) =
                    std::mem::take(&mut self.found.candidates)
                        .into_iter()
                        .partition(|c| c.name.is_some());
                ordered.extend(named);
                ordered.extend(unnamed);
                self.found.candidates = ordered;
            }
            Step::Exact(_) => {
                let docs = ols_docs(&json);
                if !docs.is_empty() {
                    self.found.source = Some(Source::Chebi);
                    if self.found.full_name.is_none() {
                        self.found.full_name = Some(docs[0].1.clone());
                    }
                    self.found.candidates = candidates(docs);
                    self.plan.clear();
                }
            }
            Step::PubChem(_) => {
                let synonyms: Vec<&str> = json
                    .pointer("/InformationList/Information/0/Synonym")
                    .and_then(Value::as_array)
                    .map(|a| a.iter().filter_map(Value::as_str).collect())
                    .unwrap_or_default();
                let chebi: Vec<Candidate> = synonyms
                    .iter()
                    .filter(|s| s.starts_with("CHEBI:"))
                    .map(|s| Candidate {
                        chebi: (*s).to_owned(),
                        name: None,
                    })
                    .collect();
                if !chebi.is_empty() {
                    self.found.source = Some(Source::PubChem);
                    if self.found.full_name.is_none() {
                        self.found.full_name = synonyms.first().map(|s| (*s).to_owned());
                    }
                    self.found.candidates = chebi;
                    self.plan.clear();
                }
            }
        }
    }

    /// How many species are done, and how many there are.
    pub fn progress(&self) -> (usize, usize) {
        (self.results.len(), self.queries.len())
    }

    /// What was found, one per species, in the order asked.
    pub fn results(&self) -> &[Found] {
        &self.results
    }

    /// No service answered at all (C9).
    pub fn unreachable(&self) -> bool {
        self.answered == 0 && self.failed > 0
    }
}

fn nothing(species: &str) -> Found {
    Found {
        species: species.to_owned(),
        source: None,
        full_name: None,
        candidates: Vec::new(),
        chosen: None,
        sure: false,
        other_references: Vec::new(),
    }
}

/// Proposes a candidate (C2): the one named as what was looked for (BiGG's
/// full name, or the name searched for), else the first. Sure only when
/// exactly one is so named, and never from PubChem, whose entries have no
/// names to check.
fn choose(f: &mut Found, searched: &str) {
    if f.candidates.is_empty() {
        return;
    }
    let target = match f.source {
        Some(Source::Bigg) => f.full_name.clone().unwrap_or_default(),
        _ => searched.to_owned(),
    };
    let named: Vec<usize> = f
        .candidates
        .iter()
        .enumerate()
        .filter(|(_, c)| c.name.as_deref().is_some_and(|n| same(n, &target)))
        .map(|(i, _)| i)
        .collect();
    f.chosen = Some(named.first().copied().unwrap_or(0));
    f.sure = named.len() == 1 && f.source != Some(Source::PubChem);
}

/// The same name, case aside.
fn same(a: &str, b: &str) -> bool {
    a.trim().eq_ignore_ascii_case(b.trim())
}

/// BiGG's name without a formula it sometimes ends with: `ATP
/// C10H12N5O13P3` → `ATP`, `H2O H2O` → `H2O`.
fn bigg_name(name: &str) -> String {
    let name = name.trim();
    let Some((head, last)) = name.rsplit_once(' ') else {
        return name.to_owned();
    };
    let formula = last.chars().any(|c| c.is_ascii_digit())
        && last.starts_with(|c: char| c.is_ascii_uppercase())
        && last.chars().all(|c| c.is_ascii_alphanumeric());
    if formula && !head.trim().is_empty() {
        head.trim().to_owned()
    } else {
        name.to_owned()
    }
}

/// The ChEBI entries in an OLS4 answer: each its id and name.
fn ols_docs(json: &Value) -> Vec<(String, String)> {
    json.pointer("/response/docs")
        .and_then(Value::as_array)
        .map(|docs| {
            docs.iter()
                .filter_map(|d| {
                    let id = d.get("obo_id")?.as_str()?;
                    let label = d.get("label")?.as_str()?;
                    id.starts_with("CHEBI:")
                        .then(|| (id.to_owned(), label.to_owned()))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn candidates(docs: Vec<(String, String)>) -> Vec<Candidate> {
    docs.into_iter()
        .map(|(chebi, name)| Candidate {
            chebi,
            name: Some(name),
        })
        .collect()
}

fn link(compact: &str) -> Reference {
    Reference {
        qualifier: "bqbiol:is".into(),
        resource: format!("https://identifiers.org/{compact}"),
    }
}

/// The words to try in BiGG for a name or id: lower case, and without a
/// compartment ending (`g6p_c` → `g6p`). Only single words: BiGG's ids have
/// no spaces.
fn bigg_words(text: &str) -> Vec<String> {
    let w = text.trim().to_lowercase();
    if w.is_empty() || w.contains(char::is_whitespace) {
        return Vec::new();
    }
    let mut out = vec![w.clone()];
    if let Some((base, end)) = w.rsplit_once('_')
        && !base.is_empty()
        && end.len() <= 2
        && end.chars().all(|c| c.is_ascii_lowercase())
    {
        out.push(base.to_owned());
    }
    out
}

/// A name in a URL: letters, digits and `-._~` as they are, all else
/// percent-encoded (UTF-8).
fn encode(text: &str) -> String {
    let mut out = String::new();
    for b in text.trim().bytes() {
        if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Runs `search`, answering each address with the first canned answer
    /// whose key it contains, else a 404; returns the addresses asked.
    fn run(search: &mut Search, answers: &[(&str, &str)]) -> Vec<String> {
        let mut asked = Vec::new();
        while let Some(url) = search.next_url() {
            asked.push(url.clone());
            let a = answers
                .iter()
                .find(|(k, _)| url.contains(k))
                .map(|(_, v)| v.to_string())
                .ok_or_else(|| "HTTP 404".to_owned());
            search.answer(a);
            assert!(asked.len() < 100, "it stops");
        }
        asked
    }

    fn q(id: &str, name: Option<&str>) -> Query {
        Query {
            species: id.into(),
            name: name.map(str::to_owned),
        }
    }

    const BIGG_G6P: &str = r#"{"name": "D-Glucose 6-phosphate", "database_links": {
        "CHEBI": [{"id": "CHEBI:14314"}, {"id": "CHEBI:4170"}, {"id": "CHEBI:61548"}],
        "KEGG Compound": [{"id": "C00092"}],
        "MetaNetX (MNX) Chemical": [{"id": "MNXM160"}],
        "SEED Compound": [{"id": "cpd00079"}]}}"#;
    const OLS_G6P: &str = r#"{"response": {"docs": [
        {"obo_id": "CHEBI:4170", "label": "D-glucose 6-phosphate"},
        {"obo_id": "CHEBI:61548", "label": "D-glucopyranose 6-phosphate"}]}}"#;

    #[test]
    fn bigg_finds_an_abbreviation_and_chebi_names_its_entries() {
        let mut s = Search::new(vec![q("COPASI15", Some("G6P"))]);
        let asked = run(
            &mut s,
            &[
                ("metabolites/g6p", BIGG_G6P),
                (
                    "q=D-Glucose%206-phosphate&ontology=chebi&exact=true",
                    OLS_G6P,
                ),
            ],
        );
        assert_eq!(asked.len(), 2, "{asked:?}");
        let f = &s.results()[0];
        assert_eq!(f.species, "COPASI15");
        assert_eq!(f.source, Some(Source::Bigg));
        assert_eq!(f.full_name.as_deref(), Some("D-Glucose 6-phosphate"));
        assert_eq!(f.candidates.len(), 3);
        // The one named as BiGG names it, case aside, proposed and sure; the
        // ones ChEBI names first, the unnamed (CHEBI:14314) last.
        assert_eq!(f.candidates[f.chosen.unwrap()].chebi, "CHEBI:4170");
        assert!(f.sure);
        let order: Vec<&str> = f.candidates.iter().map(|c| c.chebi.as_str()).collect();
        assert_eq!(order, ["CHEBI:4170", "CHEBI:61548", "CHEBI:14314"]);
        // KEGG and MetaNetX (C4), not SEED.
        let other: Vec<&str> = f
            .other_references
            .iter()
            .map(|r| r.resource.as_str())
            .collect();
        assert_eq!(
            other,
            [
                "https://identifiers.org/kegg.compound:C00092",
                "https://identifiers.org/metanetx.chemical:MNXM160"
            ]
        );
        assert_eq!(
            f.candidates[0].reference().resource,
            "https://identifiers.org/CHEBI:4170"
        );
        assert_eq!(s.progress(), (1, 1));
    }

    #[test]
    fn a_full_name_goes_to_chebi_exactly_then_pubchem() {
        // ChEBI knows it exactly: one entry, sure.
        let mut s = Search::new(vec![q("S1", Some("ATP"))]);
        run(
            &mut s,
            &[(
                "q=ATP&ontology=chebi&exact=true",
                r#"{"response": {"docs": [{"obo_id": "CHEBI:15422", "label": "ATP"}]}}"#,
            )],
        );
        let f = &s.results()[0];
        assert_eq!((f.source, f.sure), (Some(Source::Chebi), true));
        assert_eq!(f.candidates[0].chebi, "CHEBI:15422");

        // Not in ChEBI by that name: PubChem's synonyms give it.
        let mut s = Search::new(vec![q("S2", Some("cori ester"))]);
        let asked = run(
            &mut s,
            &[(
                "name/cori%20ester/synonyms",
                r#"{"InformationList": {"Information": [{"Synonym": ["cori ester", "CHEBI:16077", "59-56-3"]}]}}"#,
            )],
        );
        // BiGG by the id (the name has a space), and searched; ChEBI
        // exactly, then PubChem.
        assert!(asked[0].ends_with("metabolites/s2"), "{asked:?}");
        assert!(asked[1].contains("search?query=s2"), "{asked:?}");
        assert!(asked[2].contains("exact=true"), "{asked:?}");
        assert!(asked[3].contains("pubchem"), "{asked:?}");
        assert_eq!(asked.len(), 4, "{asked:?}");
        let f = &s.results()[0];
        assert_eq!(f.source, Some(Source::PubChem));
        assert_eq!(f.candidates[0].chebi, "CHEBI:16077");
        assert!(!f.sure, "PubChem's have no names to check");

        // An abbreviation ChEBI matches by a synonym: offered, not sure.
        let mut s = Search::new(vec![q("S3", Some("MAL"))]);
        run(
            &mut s,
            &[(
                "q=MAL&ontology=chebi&exact=true",
                r#"{"response": {"docs": [{"obo_id": "CHEBI:25121", "label": "maleoyl group"}]}}"#,
            )],
        );
        let f = &s.results()[0];
        assert_eq!((f.source, f.sure), (Some(Source::Chebi), false));
    }

    /// Improvements 1 and 2: `ASP` isn't a BiGG id, but BiGG's search finds
    /// `asp__L`; ChEBI's exact search keeps to molecular entities.
    #[test]
    fn bigg_search_finds_a_stereo_form_and_chebi_keeps_to_molecules() {
        let mut s = Search::new(vec![q("ASP", None)]);
        let asked = run(
            &mut s,
            &[
                (
                    "search?query=asp&search_type=metabolites",
                    r#"{"results": [{"bigg_id": "aspsa"}, {"bigg_id": "asp__L"}, {"bigg_id": "asp__D"}]}"#,
                ),
                (
                    "metabolites/asp__L",
                    r#"{"name": "L-Aspartate", "database_links": {"CHEBI": [{"id": "CHEBI:29991"}]}}"#,
                ),
                (
                    "q=L-Aspartate&",
                    r#"{"response": {"docs": [{"obo_id": "CHEBI:29991", "label": "L-aspartate(1-)"}]}}"#,
                ),
            ],
        );
        assert!(asked[0].ends_with("metabolites/asp"), "{asked:?}");
        assert!(asked[1].contains("search?query=asp"), "{asked:?}");
        assert!(asked[2].ends_with("metabolites/asp__L"), "{asked:?}");
        let f = &s.results()[0];
        assert_eq!(f.source, Some(Source::Bigg));
        assert_eq!(f.full_name.as_deref(), Some("L-Aspartate"));
        assert_eq!(f.candidates[0].chebi, "CHEBI:29991");
        // Not BiGG's name exactly (an ion form): offered, not sure.
        assert!(!f.sure);

        // Nothing in BiGG: ChEBI's exact search, among molecules only.
        let mut s = Search::new(vec![q("S1", Some("glucose"))]);
        let asked = run(&mut s, &[]);
        let exact = asked.iter().find(|u| u.contains("exact=true")).unwrap();
        assert!(
            exact.ends_with("allChildrenOf=http%3A%2F%2Fpurl.obolibrary.org%2Fobo%2FCHEBI_23367"),
            "{exact}"
        );
    }

    #[test]
    fn nothing_found_is_said_and_each_species_has_a_result() {
        let mut s = Search::new(vec![q("X", None), q("glc_c", None)]);
        let asked = run(
            &mut s,
            &[(
                "metabolites/glc",
                r#"{"name": "D-Glucose", "database_links": {"CHEBI": [{"id": "CHEBI:4167"}]}}"#,
            )],
        );
        // X: BiGG, ChEBI, PubChem; glc_c: BiGG as glc_c, then glc.
        assert!(asked.iter().any(|u| u.ends_with("metabolites/glc_c")));
        assert_eq!(s.results().len(), 2);
        assert_eq!(s.results()[0].source, None);
        assert_eq!(s.results()[0].chosen, None);
        assert_eq!(s.results()[1].source, Some(Source::Bigg));
        // One entry, but ChEBI didn't name it: not sure.
        assert!(!s.results()[1].sure);
        assert!(!s.unreachable());
    }

    #[test]
    fn no_service_at_all_is_unreachable() {
        let mut s = Search::new(vec![q("S1", Some("ATP"))]);
        while s.next_url().is_some() {
            s.answer(Err("network error".into()));
        }
        assert!(s.unreachable());
        assert_eq!(s.results()[0].source, None);
    }

    #[test]
    fn words_and_addresses() {
        assert_eq!(bigg_name("ATP C10H12N5O13P3"), "ATP");
        assert_eq!(bigg_name("H2O H2O"), "H2O");
        assert_eq!(bigg_name("Coenzyme A"), "Coenzyme A");
        assert_eq!(bigg_name("D-Glucose 6-phosphate"), "D-Glucose 6-phosphate");
        assert_eq!(bigg_words("G6P"), ["g6p"]);
        assert_eq!(bigg_words("glc__D_e"), ["glc__d_e", "glc__d"]);
        assert!(bigg_words("glucose 6-phosphate").is_empty());
        assert_eq!(encode("α-D-glucose 1,6"), "%CE%B1-D-glucose%201%2C6");
    }
}
