//! The model tree (SPEC-Rust §11.3): species and reactions as lists that can
//! be read by screen readers and used from the keyboard, sharing the
//! selection with the canvas.
//!
//! The usual list conventions:
//! - **click** selects one item, and centres it on the canvas;
//! - **Ctrl/Cmd-click** adds it to the selection, or takes it out;
//! - **Shift-click** selects everything from the last item clicked to this one;
//! - **↑ / ↓** move through the list, selecting as they go (with Shift, the
//!   selection grows instead).

use eframe::egui::{self, Key};
use pd_core::geometry::Pt;
use pd_core::render::draw::effective_junction;
use pd_core::view::{DiagramView, Target};

/// What the tree remembers between frames.
#[derive(Default)]
pub struct TreeState {
    /// The item last clicked or moved to: where a Shift range starts.
    anchor: Option<Target>,
}

/// One row: what it selects, where to centre on it, its label and tooltip.
struct Row {
    target: Target,
    at: Pt,
    label: String,
    tip: Option<String>,
    selected: bool,
}

pub fn show(ui: &mut egui::Ui, view: &mut DiagramView, state: &mut TreeState, canvas_centre: Pt) {
    let m = view.model();
    let species: Vec<Row> = m
        .species()
        .iter()
        .filter(|s| !s.is_null_node)
        .map(|s| Row {
            target: Target::Species(s.id.clone()),
            at: s.center,
            label: if s.is_alias() {
                format!("{} (alias)", s.primary_id())
            } else {
                s.id.clone()
            },
            tip: None,
            selected: s.selected,
        })
        .collect();
    let reactions: Vec<Row> = m
        .reactions()
        .iter()
        .map(|r| Row {
            target: Target::Leg(r.id.clone()),
            at: effective_junction(m, r),
            label: r.id.clone(),
            tip: (!r.kinetic_law.trim().is_empty()).then(|| format!("{}: {}", r.id, r.kinetic_law)),
            selected: r.selected,
        })
        .collect();

    let mut picked = None;
    for (title, rows, salt) in [
        ("Species", &species, "tree-species"),
        ("Reactions", &reactions, "tree-reactions"),
    ] {
        egui::CollapsingHeader::new(format!("{title} ({})", rows.len()))
            .id_salt(salt)
            .default_open(true)
            .show(ui, |ui| {
                if let Some(p) = list(ui, rows, state) {
                    picked = Some(p);
                }
            })
            .header_response
            .on_hover_text(
                "Click to select; Ctrl/Cmd-click to add; Shift-click for a range; ↑ ↓ to move",
            );
    }

    if let Some((targets, add, centre)) = picked {
        for (i, t) in targets.iter().enumerate() {
            view.select_item(t, add || i > 0);
        }
        if let Some(at) = centre {
            view.centre_on(at, canvas_centre);
        }
    }
}

/// Draws one list and handles clicks and arrow keys on it. Returns what to
/// select (the first replaces the selection unless adding), whether to add,
/// and where to centre.
fn list(
    ui: &mut egui::Ui,
    rows: &[Row],
    state: &mut TreeState,
) -> Option<(Vec<Target>, bool, Option<Pt>)> {
    let (shift, command) = ui.input(|i| (i.modifiers.shift, i.modifiers.command));
    let mut responses = Vec::with_capacity(rows.len());
    let mut result = None;
    for (i, row) in rows.iter().enumerate() {
        let mut resp = ui.selectable_label(row.selected, &row.label);
        if let Some(tip) = &row.tip {
            resp = resp.on_hover_text(tip);
        }
        if resp.clicked() {
            // egui doesn't focus a widget when it's clicked; the arrow keys
            // below need the clicked row to have the focus.
            resp.request_focus();
            result = Some(click(rows, i, state, shift, command));
        }
        if resp.has_focus() {
            // Keep egui from using ↑ / ↓ to move the focus by position.
            ui.memory_mut(|m| {
                m.set_focus_lock_filter(
                    resp.id,
                    egui::EventFilter {
                        vertical_arrows: true,
                        ..Default::default()
                    },
                )
            });
        }
        responses.push(resp);
    }

    // Arrow keys move the focus, and the selection with it.
    let focused = responses.iter().position(|r| r.has_focus());
    if let Some(i) = focused {
        let step = ui.input_mut(|inp| {
            if inp.consume_key(egui::Modifiers::NONE, Key::ArrowDown)
                || inp.consume_key(egui::Modifiers::SHIFT, Key::ArrowDown)
            {
                1
            } else if inp.consume_key(egui::Modifiers::NONE, Key::ArrowUp)
                || inp.consume_key(egui::Modifiers::SHIFT, Key::ArrowUp)
            {
                -1
            } else {
                0
            }
        });
        let next = i as isize + step;
        if step != 0 && (0..rows.len() as isize).contains(&next) {
            let next = next as usize;
            responses[next].request_focus();
            result = Some(if shift {
                range(rows, next, state)
            } else {
                state.anchor = Some(rows[next].target.clone());
                (vec![rows[next].target.clone()], false, Some(rows[next].at))
            });
        }
    }
    result
}

fn click(
    rows: &[Row],
    i: usize,
    state: &mut TreeState,
    shift: bool,
    command: bool,
) -> (Vec<Target>, bool, Option<Pt>) {
    let row = &rows[i];
    if shift {
        return range(rows, i, state);
    }
    state.anchor = Some(row.target.clone());
    // Ctrl/Cmd toggles one item without moving the view.
    if command {
        (vec![row.target.clone()], true, None)
    } else {
        (vec![row.target.clone()], false, Some(row.at))
    }
}

/// Everything from the anchor to row `i` in this list (from `i` alone if the
/// anchor isn't in it). Replaces the selection.
fn range(rows: &[Row], i: usize, state: &mut TreeState) -> (Vec<Target>, bool, Option<Pt>) {
    let anchor = state
        .anchor
        .as_ref()
        .and_then(|a| rows.iter().position(|r| r.target == *a));
    let Some(a) = anchor else {
        state.anchor = Some(rows[i].target.clone());
        return (vec![rows[i].target.clone()], false, Some(rows[i].at));
    };
    let (lo, hi) = (a.min(i), a.max(i));
    let targets = rows[lo..=hi].iter().map(|r| r.target.clone()).collect();
    (targets, false, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(n: usize) -> Vec<Row> {
        (0..n)
            .map(|i| Row {
                target: Target::Species(format!("S{i}")),
                at: Pt::ZERO,
                label: String::new(),
                tip: None,
                selected: false,
            })
            .collect()
    }

    fn ids(t: &[Target]) -> Vec<String> {
        t.iter()
            .map(|t| match t {
                Target::Species(id) => id.clone(),
                _ => String::new(),
            })
            .collect()
    }

    #[test]
    fn click_ctrl_click_and_shift_click() {
        let r = rows(6);
        let mut state = TreeState::default();
        let (t, add, centre) = click(&r, 1, &mut state, false, false);
        assert_eq!(
            (ids(&t), add, centre.is_some()),
            (vec!["S1".to_owned()], false, true)
        );
        // Ctrl/Cmd toggles one, adding, and doesn't move the view.
        let (t, add, centre) = click(&r, 4, &mut state, false, true);
        assert_eq!((ids(&t), add, centre), (vec!["S4".to_owned()], true, None));
        // Shift: from the anchor (now S4) to S2, replacing the selection.
        let (t, add, _) = click(&r, 2, &mut state, true, false);
        assert_eq!(
            (ids(&t), add),
            (
                vec!["S2".to_owned(), "S3".to_owned(), "S4".to_owned()],
                false
            )
        );
    }

    #[test]
    fn shift_without_an_anchor_selects_one() {
        let r = rows(3);
        let mut state = TreeState::default();
        let (t, _, _) = click(&r, 2, &mut state, true, false);
        assert_eq!(ids(&t), ["S2"]);
        // That became the anchor.
        let (t, _, _) = click(&r, 0, &mut state, true, false);
        assert_eq!(ids(&t), ["S0", "S1", "S2"]);
    }
}
