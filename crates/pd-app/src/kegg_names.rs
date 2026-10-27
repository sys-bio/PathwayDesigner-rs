//! Looking up a KEGG map's compound names in PubChem after it opens
//! (docs/kegg-import-design.md, decision K2): a request at a time, through
//! the same fetch as the Gallery, so it works in the browser and on the
//! desktop alike.

use eframe::egui;
use pd_core::kegg::Map;
use pd_core::model::Id;
use pd_core::pubchem::Lookup;

use crate::gallery::{Pending, take};

/// A lookup under way.
pub struct Naming {
    lookup: Lookup,
    /// Each compound species and its KEGG number.
    compounds: Vec<(Id, String)>,
    waiting: Option<Pending>,
}

/// How a lookup ended.
#[derive(Debug, PartialEq)]
pub enum Done {
    /// The names found, each a KEGG number and a name.
    Found(Vec<(String, String)>),
    /// PubChem never answered.
    Unreachable,
}

impl Naming {
    /// A lookup for `map`'s compounds, or none if it has none.
    pub fn new(map: &Map) -> Option<Self> {
        if map.compounds.is_empty() {
            return None;
        }
        let numbers: Vec<String> = map.compounds.iter().map(|(_, n)| n.clone()).collect();
        Some(Naming {
            lookup: Lookup::new(&numbers),
            compounds: map.compounds.clone(),
            waiting: None,
        })
    }

    pub fn compounds(&self) -> &[(Id, String)] {
        &self.compounds
    }

    /// Takes the lookup a step further: hands it an answer that has come,
    /// and asks for the next address. `Some` when it is over.
    pub fn poll(
        &mut self,
        ctx: &egui::Context,
        fetch: fn(&egui::Context, String) -> Pending,
    ) -> Option<Done> {
        if let Some(pending) = &self.waiting {
            let result = take(pending)?;
            self.waiting = None;
            self.lookup.answer(result);
        }
        match self.lookup.next_url() {
            Some(url) => {
                self.waiting = Some(fetch(ctx, url));
                None
            }
            None if self.lookup.unreachable() => Some(Done::Unreachable),
            None => Some(Done::Found(self.lookup.names())),
        }
    }
}
