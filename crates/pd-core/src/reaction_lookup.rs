//! Finding a reaction's EC numbers and Rhea entry (docs/reaction-lookup-design.md,
//! step 1). As for species ([`crate::chebi`]), [`Search`] says which address
//! to fetch next and is given each answer; the app fetches, at most one
//! request every [`crate::chebi::MIN_INTERVAL_MS`].
//!
//! For each reaction, in order (R1):
//!
//! 1. **BiGG**, by the reaction's id, as it is and upper case, then its
//!    display name: a name, its metabolites, EC numbers, Rhea, KEGG and
//!    MetaNetX reactions.
//! 2. Else **Rhea**, when two or more of the reaction's species have ChEBI
//!    entries: the Rhea reactions that have all of them.
//! 3. Else **BiGG's search** for the id, the first few reactions it finds.
//!
//! A match is *sure*, and ticked in the review (R4), when the reaction's
//! species agree with it: from BiGG by id, each species is one of BiGG's
//! metabolites and each of those (H+ and water aside) one of the species;
//! from Rhea, it is the only match, every species has a ChEBI entry, and the
//! Rhea reaction has no other participants (H+ and water aside). A match
//! from BiGG's search is never sure.

use serde_json::Value;

use crate::chebi::{bigg_name, encode, link, same};
use crate::model::{Id, Reference};

const BIGG: &str = "https://bigg.ucsd.edu/api/v2/universal/reactions";
const BIGG_SEARCH: &str = "https://bigg.ucsd.edu/api/v2/search";
const RHEA: &str = "https://www.rhea-db.org/rhea";
/// How many of BiGG's search results are looked at.
const SEARCH_RESULTS: usize = 3;
/// How many Rhea reactions are listed.
const RHEA_ROWS: usize = 20;
/// H+ and water, which models often leave out: BiGG's ids and ChEBI's.
const BIGG_LEFT_OUT: [&str; 2] = ["h", "h2o"];
const CHEBI_LEFT_OUT: [&str; 3] = ["CHEBI:15378", "CHEBI:24636", "CHEBI:15377"];
/// BiGG's cofactors and other compounds in many reactions: sharing only
/// these says little about whether two reactions are the same.
const COMMON: [&str; 20] = [
    "h", "h2o", "atp", "adp", "amp", "gtp", "gdp", "nad", "nadh", "nadp", "nadph", "pi", "ppi",
    "co2", "coa", "o2", "nh4", "q8", "q8h2", "fad",
];

/// One of a reaction's reactants or products: its id, display name, and
/// its ChEBI entries.
#[derive(Clone, Debug, PartialEq)]
pub struct Participant {
    pub id: Id,
    pub name: Option<String>,
    pub chebi: Vec<String>,
}

/// A reaction to look for.
#[derive(Clone, Debug, PartialEq)]
pub struct Query {
    pub reaction: Id,
    pub name: Option<String>,
    /// Its reactants and products, each once.
    pub species: Vec<Participant>,
}

/// Where an answer came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Bigg,
    Rhea,
    BiggSearch,
}

impl Source {
    pub fn text(self) -> &'static str {
        match self {
            Source::Bigg => "BiGG",
            Source::Rhea => "Rhea",
            Source::BiggSearch => "BiGG's search",
        }
    }
}

/// A reaction found: BiGG's or Rhea's.
#[derive(Clone, Debug, PartialEq)]
pub struct Candidate {
    /// `PPC`, or `RHEA:28370`.
    pub id: String,
    /// BiGG's name; Rhea's reactions have none.
    pub name: Option<String>,
    /// To compare with the reaction: `co2 + pep ⇌ oaa + pi`.
    pub equation: String,
    /// `4.1.1.31`.
    pub ec: Vec<String>,
    /// Rhea's master reaction, `28370`.
    pub rhea: Option<String>,
    pub kegg: Option<String>,
    pub metanetx: Option<String>,
    /// The reaction's species agree with it.
    pub agrees: bool,
}

impl Candidate {
    /// The references it makes (R2): its EC numbers `bqbiol:isVersionOf`,
    /// its Rhea, KEGG and MetaNetX reactions `bqbiol:is`.
    pub fn references(&self) -> Vec<Reference> {
        let mut out: Vec<Reference> = self
            .ec
            .iter()
            .map(|e| Reference {
                qualifier: "bqbiol:isVersionOf".into(),
                ..link(&format!("ec-code:{e}"))
            })
            .collect();
        for (prefix, id) in [
            ("rhea", &self.rhea),
            ("kegg.reaction", &self.kegg),
            ("metanetx.reaction", &self.metanetx),
        ] {
            if let Some(id) = id {
                out.push(link(&format!("{prefix}:{id}")));
            }
        }
        out
    }

    /// What the entries of its references are called: Rhea's entry by its
    /// equation, the others by BiGG's name when there is one (a reaction
    /// Rhea found has none, so its EC numbers go unnamed).
    pub fn reference_names(&self) -> Vec<(String, String)> {
        self.references()
            .into_iter()
            .filter_map(|r| {
                let name = if r.resource.contains("/rhea:") {
                    Some(self.equation.clone()).filter(|e| !e.is_empty())
                } else {
                    self.name.clone()
                }?;
                Some((r.resource, name))
            })
            .collect()
    }

    /// The page to check it on.
    pub fn page(&self) -> String {
        match self.id.strip_prefix("RHEA:") {
            Some(n) => format!("https://www.rhea-db.org/rhea/{n}"),
            None => format!("http://bigg.ucsd.edu/universal/reactions/{}", self.id),
        }
    }
}

/// What was found for one reaction.
#[derive(Clone, Debug, PartialEq)]
pub struct Found {
    pub reaction: Id,
    /// `None` when nothing was found.
    pub source: Option<Source>,
    pub candidates: Vec<Candidate>,
    /// The candidate proposed: the first that agrees, else the first.
    pub chosen: Option<usize>,
    /// One clear match: the review ticks it (R4).
    pub sure: bool,
    /// Some of its species have no ChEBI entry, so Rhea couldn't be asked
    /// for all of them (R6).
    pub species_unannotated: bool,
}

/// What the review accepted for one reaction: the references to add, what
/// their entries are called, and, if ticked, the display name to give it
/// (R5).
#[derive(Clone, Debug, PartialEq)]
pub struct Accepted {
    pub reaction: Id,
    pub references: Vec<Reference>,
    pub names: Vec<(String, String)>,
    pub display_name: Option<String>,
}

/// One request.
#[derive(Clone, Debug, PartialEq)]
enum Step {
    /// BiGG, by one of the words to try.
    Bigg(String),
    /// Rhea, by the species' ChEBI entries.
    Rhea(Vec<String>),
    /// BiGG's search for the id.
    BiggSearch(String),
    /// One of the reactions BiGG's search found.
    BiggFound(String),
}

impl Step {
    fn url(&self) -> String {
        match self {
            Step::Bigg(w) | Step::BiggFound(w) => format!("{BIGG}/{}", encode(w)),
            Step::Rhea(chebi) => format!(
                "{RHEA}?query={}&columns=rhea-id,equation,ec,chebi-id&format=tsv&limit={RHEA_ROWS}",
                chebi.join("+AND+")
            ),
            Step::BiggSearch(w) => {
                format!("{BIGG_SEARCH}?query={}&search_type=reactions", encode(w))
            }
        }
    }
}

/// The search for some reactions, one request at a time.
#[derive(Clone, Debug)]
pub struct Search {
    queries: Vec<Query>,
    current: usize,
    /// Its requests still to make, last first.
    plan: Vec<Step>,
    asked: Option<Step>,
    found: Found,
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
            results: Vec::new(),
            answered: 0,
            failed: 0,
        };
        s.start();
        s
    }

    /// Sets up the requests for the current reaction, if there is one.
    fn start(&mut self) {
        let Some(q) = self.queries.get(self.current) else {
            return;
        };
        self.found = nothing(&q.reaction);
        let mut words: Vec<String> = Vec::new();
        let name = q.name.as_deref().map(str::trim).filter(|n| !n.is_empty());
        for w in [
            Some(q.reaction.trim().to_owned()),
            Some(q.reaction.trim().to_uppercase()),
            name.filter(|n| !n.contains(char::is_whitespace))
                .map(str::to_owned),
        ]
        .into_iter()
        .flatten()
        {
            if !w.is_empty() && !words.contains(&w) {
                words.push(w);
            }
        }
        let chebi: Vec<String> = q
            .species
            .iter()
            .filter_map(|s| s.chebi.first().cloned())
            .collect();
        self.found.species_unannotated = q.species.iter().any(|s| s.chebi.is_empty());
        // Reversed: steps are popped off the end.
        self.plan = vec![Step::BiggSearch(q.reaction.trim().to_owned())];
        if chebi.len() >= 2 {
            self.plan.push(Step::Rhea(chebi));
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

    /// Ends the current reaction with what was found, and starts the next.
    fn finish(&mut self) {
        let mut found = std::mem::replace(&mut self.found, nothing(""));
        if !found.candidates.is_empty() {
            let agreeing = found.candidates.iter().filter(|c| c.agrees).count();
            found.chosen = Some(found.candidates.iter().position(|c| c.agrees).unwrap_or(0));
            found.sure = match found.source {
                Some(Source::Bigg) => found.candidates[0].agrees,
                Some(Source::Rhea) => found.candidates.len() == 1 && agreeing == 1,
                _ => false,
            };
        }
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
                if why.contains("404") || why.contains("400") {
                    self.answered += 1;
                } else {
                    self.failed += 1;
                }
                return;
            }
        };
        let species = &self.queries[self.current].species;
        match step {
            Step::Rhea(_) => {
                let candidates = rhea_rows(&body, species);
                if !candidates.is_empty() {
                    self.found.source = Some(Source::Rhea);
                    self.found.candidates = candidates;
                    self.plan.clear();
                }
            }
            Step::Bigg(_) | Step::BiggFound(_) => {
                let Ok(json) = serde_json::from_str::<Value>(&body) else {
                    return;
                };
                let Some((c, shared)) = bigg_reaction(&json, species) else {
                    return;
                };
                // One BiGG's search found must share species with the
                // reaction: the search goes by letters, and most of what it
                // finds is another reaction entirely.
                if matches!(step, Step::BiggFound(_)) && !shared.enough(species.len()) {
                    return;
                }
                self.found.candidates.push(c);
                if let Step::Bigg(_) = step {
                    self.found.source = Some(Source::Bigg);
                    self.plan.clear();
                } else {
                    self.found.source = Some(Source::BiggSearch);
                }
            }
            Step::BiggSearch(_) => {
                let Ok(json) = serde_json::from_str::<Value>(&body) else {
                    return;
                };
                let ids: Vec<String> = json
                    .get("results")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(|r| r.get("bigg_id").and_then(Value::as_str))
                    .take(SEARCH_RESULTS)
                    .map(str::to_owned)
                    .collect();
                self.plan.extend(ids.into_iter().rev().map(Step::BiggFound));
            }
        }
    }

    /// How many reactions are done, and how many there are.
    pub fn progress(&self) -> (usize, usize) {
        (self.results.len(), self.queries.len())
    }

    /// What was found, one per reaction, in the order asked.
    pub fn results(&self) -> &[Found] {
        &self.results
    }

    /// No service answered at all.
    pub fn unreachable(&self) -> bool {
        self.answered == 0 && self.failed > 0
    }
}

/// Find in Rhea: Rhea's search for `words`, which may be names, ChEBI
/// entries or EC numbers.
pub fn rhea_find_url(words: &str) -> String {
    format!(
        "{RHEA}?query={}&columns=rhea-id,equation,ec,chebi-id&format=tsv&limit={RHEA_ROWS}",
        encode(words)
    )
}

/// The reactions in the answer to [`rhea_find_url`], in Rhea's order.
pub fn rhea_found(tsv: &str) -> Vec<Candidate> {
    rhea_rows(tsv, &[])
}

fn nothing(reaction: &str) -> Found {
    Found {
        reaction: reaction.to_owned(),
        source: None,
        candidates: Vec::new(),
        chosen: None,
        sure: false,
        species_unannotated: false,
    }
}

/// How many of a reaction's species a BiGG reaction has, all of them and
/// those not [`COMMON`].
#[derive(Clone, Copy, Debug, PartialEq)]
struct Shared {
    all: usize,
    main: usize,
}

impl Shared {
    /// Enough to offer a reaction BiGG's search found: all the species, or
    /// two of them (one if there is one) and one of those not common.
    fn enough(self, species: usize) -> bool {
        species > 0 && (self.all == species || (self.all >= species.min(2) && self.main >= 1))
    }
}

/// A BiGG reaction's answer as a candidate, and how many of the reaction's
/// species it has; `None` if it has no name.
fn bigg_reaction(json: &Value, species: &[Participant]) -> Option<(Candidate, Shared)> {
    let id = json.get("bigg_id").and_then(Value::as_str)?;
    let name = json.get("name").and_then(Value::as_str)?.trim();
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
    // Rhea's master reaction is the lowest of its four numbers.
    let rhea = ids("RHEA")
        .into_iter()
        .filter_map(|r| r.trim_start_matches("RHEA:").parse::<u64>().ok())
        .min()
        .map(|n| n.to_string());
    let mut left = Vec::new();
    let mut right = Vec::new();
    let mut metabolites = Vec::new();
    for m in json
        .get("metabolites")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let Some(mid) = m.get("bigg_id").and_then(Value::as_str) else {
            continue;
        };
        let mname = m.get("name").and_then(Value::as_str).map(bigg_name);
        let n = m
            .get("stoichiometry")
            .and_then(Value::as_f64)
            .unwrap_or(0.0);
        let term = if n.abs() == 1.0 {
            mid.to_owned()
        } else {
            format!("{} {mid}", n.abs())
        };
        if n < 0.0 {
            left.push(term);
        } else {
            right.push(term);
        }
        metabolites.push((mid.to_owned(), mname));
    }
    let (agrees, shared) = compare(&metabolites, species);
    let c = Candidate {
        id: id.to_owned(),
        name: (!name.is_empty()).then(|| name.to_owned()),
        equation: format!("{} ⇌ {}", left.join(" + "), right.join(" + ")),
        ec: ids("EC Number"),
        rhea,
        kegg: ids("KEGG Reaction").into_iter().next(),
        metanetx: ids("MetaNetX (MNX) Equation").into_iter().next(),
        agrees,
    };
    Some((c, shared))
}

/// Whether a reaction's species are BiGG's metabolites: each species one of
/// them, by BiGG id or name (case aside, `_c` endings dropped), and each of
/// them, H+ and water aside, one of the species. Also how many of the
/// species are among them.
fn compare(metabolites: &[(String, Option<String>)], species: &[Participant]) -> (bool, Shared) {
    let is = |s: &Participant, (id, name): &(String, Option<String>)| {
        [Some(s.id.as_str()), s.name.as_deref()]
            .into_iter()
            .flatten()
            .any(|w| {
                same(w, id)
                    || same(strip_compartment(w), id)
                    || name.as_deref().is_some_and(|n| same(w, n))
            })
    };
    let mut shared = Shared { all: 0, main: 0 };
    for s in species {
        if let Some((id, _)) = metabolites.iter().find(|m| is(s, m)) {
            shared.all += 1;
            if !COMMON.contains(&id.as_str()) {
                shared.main += 1;
            }
        }
    }
    let agrees = !species.is_empty()
        && shared.all == species.len()
        && metabolites
            .iter()
            .filter(|(id, _)| !BIGG_LEFT_OUT.contains(&id.as_str()))
            .all(|m| species.iter().any(|s| is(s, m)));
    (agrees, shared)
}

/// `g6p_c` → `g6p`: a one- or two-letter compartment ending, but not a
/// stereo ending (`mal__L`).
fn strip_compartment(w: &str) -> &str {
    match w.trim().rsplit_once('_') {
        Some((base, end))
            if !base.is_empty()
                && !base.ends_with('_')
                && end.len() <= 2
                && end.chars().all(|c| c.is_ascii_alphabetic()) =>
        {
            base
        }
        _ => w.trim(),
    }
}

/// Rhea's rows: id, equation, EC numbers, participants. One agrees when it
/// has as many participants as the reaction has species, H+ and water
/// aside (the search already makes it have all of theirs).
fn rhea_rows(tsv: &str, species: &[Participant]) -> Vec<Candidate> {
    let left_out = |c: &str| CHEBI_LEFT_OUT.contains(&c);
    let own = species
        .iter()
        .filter(|s| !s.chebi.iter().any(|c| left_out(c)))
        .count();
    tsv.lines()
        .skip(1)
        .filter_map(|line| {
            let cols: Vec<&str> = line.split('\t').collect();
            let id = cols.first()?.trim();
            let number = id.strip_prefix("RHEA:")?;
            let participants = cols
                .get(3)
                .map(|p| {
                    p.split(';')
                        .map(str::trim)
                        .filter(|c| !c.is_empty() && !left_out(c))
                        .count()
                })
                .unwrap_or(0);
            Some(Candidate {
                id: id.to_owned(),
                name: None,
                equation: cols.get(1).map(|e| e.trim().to_owned()).unwrap_or_default(),
                ec: cols
                    .get(2)
                    .into_iter()
                    .flat_map(|e| e.split(';'))
                    .map(|e| e.trim().trim_start_matches("EC:").to_owned())
                    .filter(|e| !e.is_empty())
                    .collect(),
                rhea: Some(number.to_owned()),
                kegg: None,
                metanetx: None,
                agrees: participants == own,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Runs `search`, answering each address with the first canned answer
    /// whose key it contains (a BiGG reaction's key, `reactions/FUM`, it
    /// must end with, so as not to answer `reactions/FUMA`), else a 404;
    /// returns the addresses asked.
    fn run(search: &mut Search, answers: &[(&str, &str)]) -> Vec<String> {
        let mut asked = Vec::new();
        while let Some(url) = search.next_url() {
            asked.push(url.clone());
            let a = answers
                .iter()
                .find(|(k, _)| {
                    if k.starts_with("reactions/") {
                        url.ends_with(k)
                    } else {
                        url.contains(k)
                    }
                })
                .map(|(_, v)| v.to_string())
                .ok_or_else(|| "HTTP 404".to_owned());
            search.answer(a);
            assert!(asked.len() < 100, "it stops");
        }
        asked
    }

    fn sp(id: &str, name: Option<&str>, chebi: &[&str]) -> Participant {
        Participant {
            id: id.into(),
            name: name.map(Into::into),
            chebi: chebi.iter().map(|c| c.to_string()).collect(),
        }
    }

    fn q(id: &str, species: Vec<Participant>) -> Query {
        Query {
            reaction: id.into(),
            name: None,
            species,
        }
    }

    const BIGG_PPC: &str = r#"{"bigg_id": "PPC", "name": "Phosphoenolpyruvate carboxylase",
        "metabolites": [
            {"bigg_id": "co2", "name": "CO2 CO2", "stoichiometry": -1.0},
            {"bigg_id": "h", "name": "H+", "stoichiometry": 1.0},
            {"bigg_id": "h2o", "name": "H2O H2O", "stoichiometry": -1.0},
            {"bigg_id": "oaa", "name": "Oxaloacetate", "stoichiometry": 1.0},
            {"bigg_id": "pep", "name": "Phosphoenolpyruvate", "stoichiometry": -1.0},
            {"bigg_id": "pi", "name": "Phosphate", "stoichiometry": 1.0}],
        "database_links": {
            "MetaNetX (MNX) Equation": [{"id": "MNXR103096"}],
            "RHEA": [{"id": "23073"}, {"id": "23072"}, {"id": "23075"}, {"id": "23074"}],
            "SEED Reaction": [{"id": "rxn00251"}],
            "KEGG Reaction": [{"id": "R00345"}],
            "EC Number": [{"id": "4.1.1.31"}]}}"#;

    fn ppc_species() -> Vec<Participant> {
        vec![
            sp("PEP", None, &[]),
            sp("CO2", None, &[]),
            sp("S7", Some("Oxaloacetate"), &[]),
            sp("pi_c", None, &[]),
        ]
    }

    #[test]
    fn bigg_by_id_gives_its_references_and_is_sure_when_the_species_agree() {
        let mut s = Search::new(vec![q("PPC", ppc_species())]);
        let asked = run(&mut s, &[("reactions/PPC", BIGG_PPC)]);
        assert_eq!(
            asked,
            ["https://bigg.ucsd.edu/api/v2/universal/reactions/PPC"]
        );
        let f = &s.results()[0];
        assert_eq!(f.source, Some(Source::Bigg));
        assert_eq!(f.chosen, Some(0));
        assert!(f.sure);
        let c = &f.candidates[0];
        assert_eq!(c.name.as_deref(), Some("Phosphoenolpyruvate carboxylase"));
        assert_eq!(c.equation, "co2 + h2o + pep ⇌ h + oaa + pi");
        // Rhea's master reaction, the lowest; SEED left out (R2).
        let refs: Vec<String> = c
            .references()
            .iter()
            .map(|r| format!("{} {}", r.qualifier, r.resource))
            .collect();
        assert_eq!(
            refs,
            [
                "bqbiol:isVersionOf https://identifiers.org/ec-code:4.1.1.31",
                "bqbiol:is https://identifiers.org/rhea:23072",
                "bqbiol:is https://identifiers.org/kegg.reaction:R00345",
                "bqbiol:is https://identifiers.org/metanetx.reaction:MNXR103096",
            ]
        );
        assert_eq!(c.page(), "http://bigg.ucsd.edu/universal/reactions/PPC");
        // Rhea's entry named by its equation, the rest by BiGG's name.
        let names = c.reference_names();
        assert_eq!(names.len(), 4);
        assert_eq!(names[0].1, "Phosphoenolpyruvate carboxylase");
        assert_eq!(
            names[1],
            (
                "https://identifiers.org/rhea:23072".to_owned(),
                "co2 + h2o + pep ⇌ h + oaa + pi".to_owned()
            )
        );

        // A species BiGG doesn't have: found, not sure.
        let mut species = ppc_species();
        species.push(sp("ATP", None, &[]));
        let mut s = Search::new(vec![q("PPC", species)]);
        run(&mut s, &[("reactions/PPC", BIGG_PPC)]);
        assert_eq!(s.results()[0].source, Some(Source::Bigg));
        assert!(!s.results()[0].sure);

        // One of BiGG's (not H+ or water) missing from the reaction: not sure.
        let mut species = ppc_species();
        species.pop();
        let mut s = Search::new(vec![q("PPC", species)]);
        run(&mut s, &[("reactions/PPC", BIGG_PPC)]);
        assert!(!s.results()[0].sure);
    }

    #[test]
    fn the_id_upper_case_and_a_one_word_display_name_are_tried() {
        let mut query = q("ppc", ppc_species());
        query.name = Some("Ppc1".into());
        let mut s = Search::new(vec![query]);
        let asked = run(&mut s, &[("reactions/PPC", BIGG_PPC)]);
        assert!(asked[0].ends_with("reactions/ppc"), "{asked:?}");
        assert!(asked[1].ends_with("reactions/PPC"), "{asked:?}");
        assert_eq!(asked.len(), 2, "found, so done: {asked:?}");
        assert_eq!(s.results()[0].source, Some(Source::Bigg));

        let mut query = q("R1", vec![]);
        query.name = Some("Ppc1".into());
        let mut s = Search::new(vec![query]);
        let asked = run(&mut s, &[]);
        assert!(asked[1].ends_with("reactions/Ppc1"), "{asked:?}");
        // No species with ChEBI entries: no Rhea, then BiGG's search.
        assert!(!asked.iter().any(|u| u.contains("rhea-db")), "{asked:?}");
        assert!(
            asked[2].contains("search?query=R1&search_type=reactions"),
            "{asked:?}"
        );
        assert_eq!(s.results()[0].source, None);
    }

    const RHEA_HEAD: &str = "Reaction identifier\tEquation\tEC number\tChEBI identifier\n";

    #[test]
    fn rhea_by_the_species_is_sure_only_for_one_match_without_more_participants() {
        let species = || {
            vec![
                sp("PEP", None, &["CHEBI:58702"]),
                sp("OAA", None, &["CHEBI:16452"]),
                sp("HCO3", None, &["CHEBI:17544"]),
                sp("P", None, &["CHEBI:43474"]),
            ]
        };
        let ppc_row = "RHEA:28370\toxaloacetate + phosphate = phosphoenolpyruvate + hydrogencarbonate\tEC:4.1.1.31\tCHEBI:16452;CHEBI:43474;CHEBI:58702;CHEBI:17544;CHEBI:15378\n";
        let rhea = format!("{RHEA_HEAD}{ppc_row}");
        let mut s = Search::new(vec![q("X1", species())]);
        let asked = run(&mut s, &[("rhea-db", &rhea)]);
        assert!(asked[0].ends_with("reactions/X1"), "{asked:?}");
        assert_eq!(
            asked[1],
            "https://www.rhea-db.org/rhea?query=CHEBI:58702+AND+CHEBI:16452+AND+CHEBI:17544+AND+CHEBI:43474&columns=rhea-id,equation,ec,chebi-id&format=tsv&limit=20"
        );
        assert_eq!(asked.len(), 2, "found, so no search: {asked:?}");
        let f = &s.results()[0];
        assert_eq!(f.source, Some(Source::Rhea));
        assert!(f.sure, "one match, H+ aside no more participants");
        let c = &f.candidates[0];
        assert_eq!(
            (c.id.as_str(), c.rhea.as_deref()),
            ("RHEA:28370", Some("28370"))
        );
        assert_eq!(c.ec, ["4.1.1.31"]);
        assert_eq!(c.page(), "https://www.rhea-db.org/rhea/28370");
        assert_eq!(
            c.references()[1].resource,
            "https://identifiers.org/rhea:28370"
        );
        // From Rhea: no name for its EC number, its equation for itself.
        assert_eq!(
            c.reference_names(),
            [(
                "https://identifiers.org/rhea:28370".to_owned(),
                "oxaloacetate + phosphate = phosphoenolpyruvate + hydrogencarbonate".to_owned()
            )]
        );

        // One match, but with a participant the reaction hasn't (diphosphate).
        let row = "RHEA:22356\toxaloacetate + diphosphate = phosphoenolpyruvate + CO2 + phosphate\tEC:4.1.1.38\tCHEBI:16452;CHEBI:33019;CHEBI:58702;CHEBI:16526;CHEBI:43474\n";
        let mut s = Search::new(vec![q("X1", species())]);
        run(&mut s, &[("rhea-db", &format!("{RHEA_HEAD}{row}"))]);
        assert_eq!(s.results()[0].source, Some(Source::Rhea));
        assert!(!s.results()[0].sure);

        // Two matches: the one that agrees proposed, not sure.
        let mut s = Search::new(vec![q("X1", species())]);
        run(
            &mut s,
            &[("rhea-db", &format!("{RHEA_HEAD}{row}{ppc_row}"))],
        );
        let f = &s.results()[0];
        assert_eq!(f.chosen, Some(1));
        assert!(!f.sure);

        // A species without a ChEBI entry: Rhea asked for the others, and
        // the review is told (R6); counted, so not sure.
        let mut sp4 = species();
        sp4.push(sp("Q", None, &[]));
        let mut s = Search::new(vec![q("X1", sp4)]);
        run(&mut s, &[("rhea-db", &rhea)]);
        let f = &s.results()[0];
        assert!(f.species_unannotated);
        assert!(!f.sure);
    }

    #[test]
    fn bigg_search_offers_its_first_reactions_never_sure() {
        let fum = r#"{"bigg_id": "FUM", "name": "Fumarase", "metabolites": [
            {"bigg_id": "fum", "name": "Fumarate", "stoichiometry": -1.0},
            {"bigg_id": "h2o", "name": "H2O H2O", "stoichiometry": -1.0},
            {"bigg_id": "mal__L", "name": "L-Malate", "stoichiometry": 1.0}],
            "database_links": {"EC Number": [{"id": "4.2.1.2"}]}}"#;
        let fumac = r#"{"bigg_id": "FUMAC", "name": "Fumarylacetoacetase", "metabolites": [
            {"bigg_id": "4fumacac", "name": "4-Fumarylacetoacetate", "stoichiometry": -1.0}]}"#;
        let species = vec![sp("FUM", None, &[]), sp("MAL", Some("L-Malate"), &[])];
        let mut s = Search::new(vec![q("FUMA", species)]);
        let asked = run(
            &mut s,
            &[
                (
                    "search?query=FUMA",
                    r#"{"results": [{"bigg_id": "FUMAC"}, {"bigg_id": "FUMAH"}, {"bigg_id": "FUM"}, {"bigg_id": "FUMm"}]}"#,
                ),
                ("reactions/FUMAC", fumac),
                ("reactions/FUM", fum),
            ],
        );
        // The first three, in BiGG's order; FUMAH not known.
        let tail: Vec<&str> = asked[asked.len() - 3..]
            .iter()
            .map(|u| u.rsplit('/').next().unwrap())
            .collect();
        assert_eq!(tail, ["FUMAC", "FUMAH", "FUM"]);
        let f = &s.results()[0];
        assert_eq!(f.source, Some(Source::BiggSearch));
        // Fumarylacetoacetase shares no species: left out. Fumarase is
        // offered, never sure.
        let ids: Vec<&str> = f.candidates.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(ids, ["FUM"]);
        assert!(!f.sure);

        // Sharing only ATP and ADP isn't enough: Millard's SK (succinyl-CoA
        // synthetase) isn't BiGG's SKK (shikimate kinase). Sharing all of
        // them is, common or not (ADK, adenylate kinase).
        let skk = r#"{"bigg_id": "SKK", "name": "Shikimate kinase", "metabolites": [
            {"bigg_id": "atp", "stoichiometry": -1.0}, {"bigg_id": "skm", "stoichiometry": -1.0},
            {"bigg_id": "adp", "stoichiometry": 1.0}, {"bigg_id": "skm5p", "stoichiometry": 1.0}]}"#;
        let adk = r#"{"bigg_id": "ADK1", "name": "Adenylate kinase", "metabolites": [
            {"bigg_id": "amp", "stoichiometry": -1.0}, {"bigg_id": "atp", "stoichiometry": -1.0},
            {"bigg_id": "adp", "stoichiometry": 2.0}]}"#;
        let answers = [
            ("search?query=SK", r#"{"results": [{"bigg_id": "SKK"}]}"#),
            ("search?query=ADK", r#"{"results": [{"bigg_id": "ADK1"}]}"#),
            ("reactions/SKK", skk),
            ("reactions/ADK1", adk),
        ];
        let sk = vec![
            sp("ATP", None, &[]),
            sp("ADP", None, &[]),
            sp("SUC", None, &[]),
        ];
        let adk_species = vec![
            sp("ATP", None, &[]),
            sp("AMP", None, &[]),
            sp("ADP", None, &[]),
        ];
        let mut s = Search::new(vec![q("SK", sk), q("ADK", adk_species)]);
        run(&mut s, &answers);
        assert_eq!(s.results()[0].source, None);
        assert_eq!(s.results()[1].source, Some(Source::BiggSearch));

        // Sharing one of two species isn't enough either; nothing found.
        let species = vec![sp("FUM", None, &[]), sp("X", None, &[])];
        let mut s = Search::new(vec![q("FUMA", species)]);
        run(
            &mut s,
            &[
                ("search?query=FUMA", r#"{"results": [{"bigg_id": "FUM"}]}"#),
                ("reactions/FUM", fum),
            ],
        );
        assert_eq!(s.results()[0].source, None);
    }

    #[test]
    fn nothing_found_and_unreachable() {
        let mut s = Search::new(vec![q("GROWTH", vec![]), q("PPC", ppc_species())]);
        run(&mut s, &[("reactions/PPC", BIGG_PPC)]);
        assert_eq!(s.progress(), (2, 2));
        assert_eq!(s.results()[0].source, None);
        assert_eq!(s.results()[0].chosen, None);
        assert_eq!(s.results()[1].source, Some(Source::Bigg));
        assert!(!s.unreachable());

        let mut s = Search::new(vec![q("PPC", ppc_species())]);
        while s.next_url().is_some() {
            s.answer(Err("network error".into()));
        }
        assert!(s.unreachable());
        assert_eq!(s.results()[0].source, None);
    }

    #[test]
    fn find_in_rhea() {
        assert_eq!(
            rhea_find_url("CHEBI:58702 pyruvate"),
            "https://www.rhea-db.org/rhea?query=CHEBI%3A58702%20pyruvate&columns=rhea-id,equation,ec,chebi-id&format=tsv&limit=20"
        );
        let tsv = format!(
            "{RHEA_HEAD}RHEA:18157\tpyruvate + ATP = phosphoenolpyruvate + ADP + H(+)\tEC:2.7.1.40\tCHEBI:15361\n"
        );
        let found = rhea_found(&tsv);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].id, "RHEA:18157");
        assert_eq!(found[0].ec, ["2.7.1.40"]);
        assert_eq!(
            found[0].equation,
            "pyruvate + ATP = phosphoenolpyruvate + ADP + H(+)"
        );
        assert!(rhea_found(RHEA_HEAD).is_empty());
    }

    #[test]
    fn compartment_endings() {
        assert_eq!(strip_compartment("g6p_c"), "g6p");
        assert_eq!(strip_compartment("mal__L"), "mal__L");
        assert_eq!(strip_compartment("GLC"), "GLC");
    }
}
