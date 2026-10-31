//! Fetching what references' entries are called, as they are wanted: the
//! hovered item's and the selected items' (pd_core::entry_names). One
//! request at a time, at most one every `MIN_INTERVAL_MS`; each reference
//! asked about once a session, found or not, so a missing entry or a lost
//! network costs one request, not one a frame.

use std::collections::HashSet;

use eframe::egui;
use pd_core::chebi::MIN_INTERVAL_MS;
use pd_core::entry_names::{name_url, parse_name};
use pd_core::view::DiagramView;

use crate::annotation_panel::Fetch;
use crate::gallery::{Pending, take};

#[derive(Default)]
pub struct NameFetcher {
    /// Asked about already.
    tried: HashSet<String>,
    /// The reference being asked about, and its answer to come.
    waiting: Option<(String, Pending)>,
    last: f64,
}

impl NameFetcher {
    /// Takes in an answer that has come, and asks about the next of
    /// `wanted` not yet tried, when the interval has passed.
    pub fn poll(
        &mut self,
        ctx: &egui::Context,
        view: &mut DiagramView,
        fetch: Fetch,
        wanted: &[String],
    ) {
        if let Some((resource, pending)) = &self.waiting {
            let Some(result) = take(pending) else {
                ctx.request_repaint_after(std::time::Duration::from_millis(50));
                return;
            };
            if let Some(name) = result.ok().and_then(|body| parse_name(resource, &body)) {
                view.name_references([(resource.clone(), name)]);
            }
            self.waiting = None;
            ctx.request_repaint();
        }
        let Some(next) = wanted.iter().find(|r| !self.tried.contains(*r)) else {
            return;
        };
        let now = ctx.input(|i| i.time);
        let wait = MIN_INTERVAL_MS as f64 / 1000.0;
        if now - self.last < wait {
            ctx.request_repaint_after(std::time::Duration::from_secs_f64(wait));
            return;
        }
        let Some(url) = name_url(next) else {
            self.tried.insert(next.clone());
            return;
        };
        self.tried.insert(next.clone());
        self.last = now;
        self.waiting = Some((next.clone(), fetch(ctx, url)));
        ctx.request_repaint();
    }
}
