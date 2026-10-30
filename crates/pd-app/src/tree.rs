//! The model tree (SPEC-Rust §11.3): species, reactions and compartments as
//! lists that can be read by screen readers and used from the keyboard,
//! sharing the selection with the canvas.
//!
//! The usual list conventions:
//! - **click** selects one item;
//! - **Ctrl/Cmd-click** adds it to the selection, or takes it out;
//! - **Shift-click** selects everything from the last item clicked to this one;
//! - **↑ / ↓** move through the list, selecting as they go (with Shift, the
//!   selection grows instead).
//!
//! Selecting never moves the diagram: stepping through the list with the
//! arrow keys would otherwise scroll it at every step, and the selected item
//! would be hard to follow.
//!
//! A Find box above the lists keeps only the items whose id or display name
//! holds the words typed. Clicking one of those, or Enter for the first,
//! selects it and centres the diagram on it, since finding an item is
//! wanting to see it. Ctrl/Cmd+F puts the cursor in the box.

use eframe::egui::{self, Key};
use pd_core::geometry::Pt;
use pd_core::view::{DiagramView, Target};

/// What the tree remembers between frames.
#[derive(Default)]
pub struct TreeState {
    /// The item last clicked or moved to: where a Shift range starts.
    anchor: Option<Target>,
    /// The words in the Find box.
    pub filter: String,
    /// Put the cursor in the Find box (Ctrl/Cmd+F).
    pub focus_filter: bool,
}

/// One row: what it selects, its label and tooltip.
struct Row {
    target: Target,
    label: String,
    tip: Option<String>,
    selected: bool,
    /// What the Find box matches: its id and display name, lower case.
    words: String,
    /// Where it is, to centre the diagram on.
    at: Option<Pt>,
}

/// An item's label: its id, and its display name when that differs.
fn labelled(id: &str, name: Option<&str>) -> String {
    match name {
        Some(n) if n != id => format!("{id} — {n}"),
        _ => id.to_owned(),
    }
}

/// The lists, with the Find box above them. Returns where to centre the
/// diagram when an item was found and picked.
pub fn show(ui: &mut egui::Ui, view: &mut DiagramView, state: &mut TreeState) -> Option<Pt> {
    let m = view.model();
    let species: Vec<Row> = m
        .species()
        .iter()
        .filter(|s| !s.is_null_node)
        .map(|s| {
            let name = m
                .find_species(s.primary_id())
                .and_then(|p| p.display_name.as_deref());
            Row {
                target: Target::Species(s.id.clone()),
                label: if s.is_alias() {
                    format!("{} (alias)", labelled(s.primary_id(), name))
                } else {
                    labelled(&s.id, name)
                },
                tip: None,
                selected: s.selected,
                words: format!("{} {}", s.primary_id(), name.unwrap_or("")).to_lowercase(),
                at: Some(s.center),
            }
        })
        .collect();
    let reactions: Vec<Row> = m
        .reactions()
        .iter()
        .map(|r| Row {
            target: Target::Leg(r.id.clone()),
            label: labelled(&r.id, r.display_name.as_deref()),
            tip: (!r.kinetic_law.trim().is_empty()).then(|| format!("{}: {}", r.id, r.kinetic_law)),
            selected: r.selected,
            words: format!("{} {}", r.id, r.display_name.as_deref().unwrap_or("")).to_lowercase(),
            at: Some(pd_core::render::draw::effective_junction(m, r)),
        })
        .collect();

    // Listed here too because a hidden box can't be clicked (S §7.9).
    let compartments: Vec<Row> = m
        .compartments
        .iter()
        .map(|c| Row {
            target: Target::Compartment(c.id.clone()),
            label: if c.is_default() {
                format!("{} (not drawn)", c.id)
            } else if c.hidden {
                format!("{} (hidden)", labelled(&c.id, c.display_name.as_deref()))
            } else {
                labelled(&c.id, c.display_name.as_deref())
            },
            tip: None,
            selected: c.selected,
            words: format!("{} {}", c.id, c.display_name.as_deref().unwrap_or("")).to_lowercase(),
            at: c.frame.map(|f| f.center),
        })
        .collect();

    // The Find box: only the items holding the words are listed.
    let r = ui.add(
        egui::TextEdit::singleline(&mut state.filter)
            .hint_text("Find (Ctrl+F)")
            .desired_width(f32::INFINITY),
    );
    if std::mem::take(&mut state.focus_filter) {
        r.request_focus();
    }
    let enter = r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
    let words = state.filter.trim().to_lowercase();
    let finding = !words.is_empty();
    let keep = |rows: Vec<Row>| -> Vec<Row> {
        rows.into_iter()
            .filter(|r| r.words.contains(&words))
            .collect()
    };
    let (species, reactions, compartments) = if finding {
        (keep(species), keep(reactions), keep(compartments))
    } else {
        (species, reactions, compartments)
    };
    if finding && species.is_empty() && reactions.is_empty() && compartments.is_empty() {
        ui.label(egui::RichText::new("Nothing matches").weak());
    }
    // Enter: the first found.
    if enter && let Some(first) = species.iter().chain(&reactions).chain(&compartments).next() {
        let at = first.at;
        view.select_item(&first.target, false);
        return at;
    }

    let mut picked = None;
    for (title, rows, salt) in [
        ("Species", &species, "tree-species"),
        ("Reactions", &reactions, "tree-reactions"),
        ("Compartments", &compartments, "tree-compartments"),
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

    let mut centre = None;
    if let Some((targets, add)) = picked {
        // While finding, the one picked is brought into view.
        if finding && !add && targets.len() == 1 {
            centre = species
                .iter()
                .chain(&reactions)
                .chain(&compartments)
                .find(|r| r.target == targets[0])
                .and_then(|r| r.at);
        }
        for (i, t) in targets.iter().enumerate() {
            view.select_item(t, add || i > 0);
        }
    }
    centre
}

/// What to select (the first replaces the selection unless adding), and
/// whether to add.
type Pick = (Vec<Target>, bool);

/// Draws one list and handles clicks and arrow keys on it.
fn list(ui: &mut egui::Ui, rows: &[Row], state: &mut TreeState) -> Option<Pick> {
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
                (vec![rows[next].target.clone()], false)
            });
        }
    }
    result
}

fn click(rows: &[Row], i: usize, state: &mut TreeState, shift: bool, command: bool) -> Pick {
    let row = &rows[i];
    if shift {
        return range(rows, i, state);
    }
    state.anchor = Some(row.target.clone());
    // Ctrl/Cmd toggles one item, adding to the selection.
    (vec![row.target.clone()], command)
}

/// Everything from the anchor to row `i` in this list (from `i` alone if the
/// anchor isn't in it). Replaces the selection.
fn range(rows: &[Row], i: usize, state: &mut TreeState) -> Pick {
    let anchor = state
        .anchor
        .as_ref()
        .and_then(|a| rows.iter().position(|r| r.target == *a));
    let Some(a) = anchor else {
        state.anchor = Some(rows[i].target.clone());
        return (vec![rows[i].target.clone()], false);
    };
    let (lo, hi) = (a.min(i), a.max(i));
    let targets = rows[lo..=hi].iter().map(|r| r.target.clone()).collect();
    (targets, false)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(n: usize) -> Vec<Row> {
        (0..n)
            .map(|i| Row {
                target: Target::Species(format!("S{i}")),
                label: String::new(),
                tip: None,
                selected: false,
                words: String::new(),
                at: None,
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
        let (t, add) = click(&r, 1, &mut state, false, false);
        assert_eq!((ids(&t), add), (vec!["S1".to_owned()], false));
        // Ctrl/Cmd toggles one, adding.
        let (t, add) = click(&r, 4, &mut state, false, true);
        assert_eq!((ids(&t), add), (vec!["S4".to_owned()], true));
        // Shift: from the anchor (now S4) to S2, replacing the selection.
        let (t, add) = click(&r, 2, &mut state, true, false);
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
        let (t, _) = click(&r, 2, &mut state, true, false);
        assert_eq!(ids(&t), ["S2"]);
        // That became the anchor.
        let (t, _) = click(&r, 0, &mut state, true, false);
        assert_eq!(ids(&t), ["S0", "S1", "S2"]);
    }
}
