//! Compound names for KEGG numbers, from PubChem (docs/kegg-import-design.md
//! §5, decision K2). KEGG deposits its compounds in PubChem under their
//! numbers, and PubChem lets web pages read it, so the same lookup serves the
//! browser and the desktop.
//!
//! [`Lookup`] says which address to fetch next and is given each answer; the
//! app does the fetching. Three kinds of request, each for a batch:
//!
//! 1. the KEGG substances' synonyms, which include the KEGG number, so each
//!    answer can be matched to its compound, and which name a compound PubChem
//!    has only as a substance;
//! 2. the substances' compounds (CIDs);
//! 3. the compounds' titles, PubChem's own name for each.
//!
//! PubChem refuses a whole batch if one number in it is unknown, so a batch
//! that fails is asked again one number at a time.

use std::collections::{HashMap, VecDeque};

use serde_json::Value;

const BASE: &str = "https://pubchem.ncbi.nlm.nih.gov/rest/pug";
/// Numbers or compounds in one request.
pub const BATCH: usize = 40;

#[derive(Clone, Debug, PartialEq)]
enum Request {
    Synonyms(Vec<String>),
    Cids(Vec<String>),
    Titles(Vec<u64>),
}

impl Request {
    fn url(&self) -> String {
        match self {
            Request::Synonyms(n) => {
                format!(
                    "{BASE}/substance/sourceid/KEGG/{}/synonyms/JSON",
                    n.join(",")
                )
            }
            Request::Cids(n) => format!("{BASE}/substance/sourceid/KEGG/{}/cids/JSON", n.join(",")),
            Request::Titles(c) => {
                let c: Vec<String> = c.iter().map(u64::to_string).collect();
                format!("{BASE}/compound/cid/{}/property/Title/JSON", c.join(","))
            }
        }
    }
}

/// A lookup of names for some KEGG numbers, a request at a time.
#[derive(Clone, Debug)]
pub struct Lookup {
    queue: VecDeque<Request>,
    asked: Option<Request>,
    /// Each number PubChem knows: its substance and synonyms.
    substances: HashMap<String, (u64, Vec<String>)>,
    /// Substance to compound.
    cids: HashMap<u64, u64>,
    /// Compound to title.
    titles: HashMap<u64, String>,
    /// Whether the compounds have been asked for (after every synonym).
    cids_asked: bool,
    /// The numbers asked about, in order.
    numbers: Vec<String>,
    answered: usize,
    failed: usize,
}

impl Lookup {
    pub fn new(numbers: &[String]) -> Self {
        let mut numbers: Vec<String> = numbers.to_vec();
        numbers.dedup();
        let queue = numbers
            .chunks(BATCH)
            .map(|c| Request::Synonyms(c.to_vec()))
            .collect();
        Lookup {
            queue,
            asked: None,
            substances: HashMap::new(),
            cids: HashMap::new(),
            titles: HashMap::new(),
            cids_asked: false,
            numbers,
            answered: 0,
            failed: 0,
        }
    }

    /// The address to fetch next, or `None` when the lookup is over. Ask again
    /// only after [`answer`](Self::answer).
    pub fn next_url(&mut self) -> Option<String> {
        if let Some(r) = &self.asked {
            return Some(r.url());
        }
        if self.queue.is_empty() && !self.cids_asked {
            self.queue_cids();
        }
        let r = self.queue.pop_front()?;
        let url = r.url();
        self.asked = Some(r);
        Some(url)
    }

    fn queue_cids(&mut self) {
        let known: Vec<String> = self
            .numbers
            .iter()
            .filter(|n| self.substances.contains_key(*n))
            .cloned()
            .collect();
        self.cids_asked = true;
        for c in known.chunks(BATCH) {
            self.queue.push_back(Request::Cids(c.to_vec()));
        }
    }

    /// The answer to the last address [`next_url`](Self::next_url) gave: the text
    /// fetched, or why it couldn't be.
    pub fn answer(&mut self, result: Result<String, String>) {
        let Some(asked) = self.asked.take() else {
            return;
        };
        let json = result
            .ok()
            .and_then(|t| serde_json::from_str::<Value>(&t).ok())
            .filter(|v| v.get("Fault").is_none());
        let Some(json) = json else {
            self.failed += 1;
            // A batch with a number PubChem doesn't know fails as a whole:
            // ask for each number alone.
            match asked {
                Request::Synonyms(n) if n.len() > 1 => self
                    .queue
                    .extend(n.into_iter().map(|one| Request::Synonyms(vec![one]))),
                Request::Cids(n) if n.len() > 1 => self
                    .queue
                    .extend(n.into_iter().map(|one| Request::Cids(vec![one]))),
                _ => {}
            }
            return;
        };
        self.answered += 1;
        let info = json
            .pointer("/InformationList/Information")
            .and_then(Value::as_array);
        match asked {
            Request::Synonyms(numbers) => {
                for i in info.into_iter().flatten() {
                    let Some(sid) = i.get("SID").and_then(Value::as_u64) else {
                        continue;
                    };
                    let synonyms: Vec<String> = i
                        .get("Synonym")
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                        .filter_map(Value::as_str)
                        .map(str::to_owned)
                        .collect();
                    // The KEGG number is among the synonyms; one asked alone
                    // is that one.
                    let number = numbers
                        .iter()
                        .find(|n| synonyms.contains(n))
                        .or((numbers.len() == 1).then(|| &numbers[0]));
                    if let Some(n) = number {
                        self.substances.entry(n.clone()).or_insert((sid, synonyms));
                    }
                }
            }
            Request::Cids(_) => {
                let mut wanted = Vec::new();
                for i in info.into_iter().flatten() {
                    let sid = i.get("SID").and_then(Value::as_u64);
                    let cid = i
                        .get("CID")
                        .and_then(Value::as_array)
                        .and_then(|c| c.first())
                        .and_then(Value::as_u64);
                    if let (Some(sid), Some(cid)) = (sid, cid) {
                        self.cids.insert(sid, cid);
                        wanted.push(cid);
                    }
                }
                wanted.dedup();
                for c in wanted.chunks(BATCH) {
                    self.queue.push_back(Request::Titles(c.to_vec()));
                }
            }
            Request::Titles(_) => {
                for p in json
                    .pointer("/PropertyTable/Properties")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                {
                    if let (Some(cid), Some(title)) = (
                        p.get("CID").and_then(Value::as_u64),
                        p.get("Title").and_then(Value::as_str),
                    ) {
                        self.titles.insert(cid, title.to_owned());
                    }
                }
            }
        }
    }

    /// True when no request ever got an answer: PubChem couldn't be reached.
    pub fn unreachable(&self) -> bool {
        self.answered == 0 && self.failed > 0
    }

    /// Each number with the PubChem compound (CID) found for it, in the
    /// order asked.
    pub fn compounds(&self) -> Vec<(String, u64)> {
        self.numbers
            .iter()
            .filter_map(|n| {
                let (sid, _) = self.substances.get(n)?;
                Some((n.clone(), *self.cids.get(sid)?))
            })
            .collect()
    }

    /// Each number with a name found for it, in the order asked: the
    /// compound's title, else the substance's first synonym that is a name.
    pub fn names(&self) -> Vec<(String, String)> {
        self.numbers
            .iter()
            .filter_map(|n| {
                let (sid, synonyms) = self.substances.get(n)?;
                let title = self
                    .cids
                    .get(sid)
                    .and_then(|cid| self.titles.get(cid))
                    .filter(|t| is_name(t, n));
                let name = title.or_else(|| synonyms.iter().find(|s| is_name(s, n)))?;
                Some((n.clone(), name.clone()))
            })
            .collect()
    }
}

/// True if `s` reads as a name, not a number: not the KEGG number itself, a
/// CAS number (`59-56-3`) or a bare number.
fn is_name(s: &str, number: &str) -> bool {
    let s = s.trim();
    !s.is_empty()
        && s != number
        && !s.chars().all(|c| c.is_ascii_digit() || c == '-')
        && s.chars().any(|c| c.is_alphabetic())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn numbers(n: &[&str]) -> Vec<String> {
        n.iter().map(|s| s.to_string()).collect()
    }

    /// Runs `lookup`, answering each address from `answers` (by a piece of
    /// the address), or with a failure.
    fn run(lookup: &mut Lookup, answers: &[(&str, &str)]) -> Vec<String> {
        let mut asked = Vec::new();
        while let Some(url) = lookup.next_url() {
            asked.push(url.clone());
            let a = answers
                .iter()
                .find(|(k, _)| url.contains(k))
                .map(|(_, v)| v.to_string())
                .ok_or_else(|| "the server said 400".to_owned());
            lookup.answer(a);
            assert!(asked.len() < 50, "it stops");
        }
        asked
    }

    const SYNONYMS: &str = r#"{"InformationList": {"Information": [
        {"SID": 3333, "Synonym": ["D-glucose", "glucose", "C00031"]},
        {"SID": 3403, "Synonym": ["cori ester", "59-56-3", "C00103"]},
        {"SID": 47205285, "Synonym": ["C15972", "Lipoamide-E"]}]}}"#;
    const CIDS: &str = r#"{"InformationList": {"Information": [
        {"SID": 3333, "CID": [5793]}, {"SID": 3403, "CID": [65533]}, {"SID": 47205285}]}}"#;
    const TITLES: &str = r#"{"PropertyTable": {"Properties": [
        {"CID": 5793, "Title": "D-Glucose"}, {"CID": 65533, "Title": "alpha-D-glucose 1-phosphate"}]}}"#;

    #[test]
    fn titles_first_then_synonyms() {
        let mut l = Lookup::new(&numbers(&["C00031", "C00103", "C15972"]));
        let asked = run(
            &mut l,
            &[
                ("C00031,C00103,C15972/synonyms", SYNONYMS),
                ("C00031,C00103,C15972/cids", CIDS),
                ("cid/5793,65533/property/Title", TITLES),
            ],
        );
        assert_eq!(asked.len(), 3, "{asked:?}");
        assert_eq!(
            l.names(),
            [
                ("C00031".to_owned(), "D-Glucose".to_owned()),
                (
                    "C00103".to_owned(),
                    "alpha-D-glucose 1-phosphate".to_owned()
                ),
                // No compound: the substance's first synonym that is a name.
                ("C15972".to_owned(), "Lipoamide-E".to_owned()),
            ]
        );
        // The compounds found, for the references (A7); C15972 has none.
        assert_eq!(
            l.compounds(),
            [("C00031".to_owned(), 5793), ("C00103".to_owned(), 65533)]
        );
        assert!(!l.unreachable());
    }

    #[test]
    fn a_batch_pubchem_refuses_is_asked_one_at_a_time() {
        let fault = r#"{"Fault": {"Code": "PUGREST.BadRequest"}}"#;
        let mut l = Lookup::new(&numbers(&["C00031", "C99999"]));
        let asked = run(
            &mut l,
            &[
                ("C00031,C99999/synonyms", fault),
                (
                    "KEGG/C00031/synonyms",
                    r#"{"InformationList": {"Information": [{"SID": 3333, "Synonym": ["D-glucose"]}]}}"#,
                ),
                (
                    "KEGG/C00031/cids",
                    r#"{"InformationList": {"Information": [{"SID": 3333, "CID": [5793]}]}}"#,
                ),
                ("cid/5793/property", TITLES),
            ],
        );
        assert!(
            asked.iter().any(|u| u.contains("KEGG/C99999/synonyms")),
            "{asked:?}"
        );
        // C99999 is unknown: no name, and the rest still found.
        assert_eq!(l.names(), [("C00031".to_owned(), "D-Glucose".to_owned())]);
        assert!(!l.unreachable());
    }

    #[test]
    fn no_answer_at_all_is_unreachable() {
        let mut l = Lookup::new(&numbers(&["C00031"]));
        run(&mut l, &[]);
        assert!(l.unreachable());
        assert!(l.names().is_empty());
    }

    #[test]
    fn numbers_are_not_names() {
        assert!(is_name("D-Glucose", "C00031"));
        assert!(!is_name("C00031", "C00031"));
        assert!(!is_name("59-56-3", "C00103"));
        assert!(!is_name("  ", "C00103"));
        assert!(!is_name("12345", "C00103"));
    }
}
