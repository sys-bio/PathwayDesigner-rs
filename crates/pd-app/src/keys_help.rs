//! Help ▸ Mouse and Keyboard: the mouse actions and keyboard shortcuts, as
//! the README lists them (a test keeps the two the same).

use eframe::egui::{self, RichText};

/// Sections of (action, what it does).
pub const SECTIONS: &[(&str, &[(&str, &str)])] = &[
    (
        "On the canvas, with the Select tool",
        &[
            (
                "Click",
                "Select a species, reaction, compartment, text or name",
            ),
            ("Shift-click", "Add to or take from the selection"),
            (
                "Drag on empty canvas",
                "Select everything the rectangle touches",
            ),
            (
                "Drag a species, junction, text, name or compartment border",
                "Move it (a compartment carries what is inside it)",
            ),
            (
                "Drag a Bézier handle of a selected reaction",
                "Reshape that line",
            ),
            (
                "Drag a corner of a selected species or compartment",
                "Resize it; Shift keeps the proportions",
            ),
            (
                "Drag the small handle on a selected text's right edge",
                "Set where its lines wrap",
            ),
            (
                "Drag a square at an end of a selected reaction's stem",
                "Move that end of the stem; with Straight stem ticked (Properties), the stem pivots about the junction instead of bending",
            ),
            (
                "Drag the + by a selected reaction's junction onto a species",
                "Add the species to the reaction as a product; hold Shift when letting go for a reactant",
            ),
            (
                "Alt while dragging",
                "Place freely while Snap to grid is on",
            ),
            ("Double-click a species", "Rename it"),
            ("Double-click a text", "Edit its words"),
            (
                "Double-click a modifier line, or a reactant's or product's line",
                "Add a corner point there, so the line can turn a corner; drag the small square to move it, right-click it to remove it",
            ),
            (
                "Right-click",
                "The menu for what is under the pointer (aliases, Add Reactant…, Add Product…, Remove from Reaction, Add, Straighten or Remove Stem, Modifier Kind, corner points, styles, alignment, locking and more)",
            ),
            (
                "Rest the pointer on an item",
                "A tooltip: a species' value, a reaction's rate law, a compartment's volume",
            ),
        ],
    ),
    (
        "Adding things",
        &[
            ("Species, Text", "Click where it should go"),
            (
                "Compartment",
                "Drag out its box (a click makes a 300 × 200 box)",
            ),
            (
                "A → B, A + B → C, A → B + C, A + B → C + D",
                "Click the reactants, then the products, in turn",
            ),
            (
                "A+… → B+…",
                "Click the reactants, press Enter, click the products, press Enter again",
            ),
            (
                "Add Reactant…, Add Product… (a reaction's right-click menu)",
                "Click species to add them; Esc or a click on empty canvas stops",
            ),
            ("Any of these", "A click on empty canvas, or Esc, cancels"),
        ],
    ),
    (
        "Getting around",
        &[
            ("Wheel, or two-finger scroll", "Pan (Shift: sideways)"),
            (
                "Ctrl + wheel, or pinch",
                "Zoom, keeping the point under the pointer still",
            ),
            ("Middle-button drag, or Space + drag", "Pan"),
            (
                "The − / % / + at the right of the status bar",
                "Zoom out, zoom to fit, zoom in",
            ),
        ],
    ),
    (
        "In the model tree (left)",
        &[
            ("Click", "Select that item"),
            ("Ctrl-click", "Add to or take from the selection"),
            (
                "Shift-click",
                "Select everything from the last item clicked",
            ),
            (
                "↑ / ↓, Shift + ↑ / ↓",
                "Move through the list; extend the selection",
            ),
        ],
    ),
    (
        "Keys",
        &[
            ("Ctrl+Z", "Undo"),
            ("Ctrl+Shift+Z, Ctrl+Y", "Redo"),
            (
                "Ctrl+A",
                "Select all (Edit also has Select All Species and Select All Reactions)",
            ),
            ("Delete, Backspace", "Delete the selection"),
            (
                "Arrow keys, Shift + arrow keys",
                "Nudge the selection 1 unit, 10 units; with Snap to grid, one grid line, five (Alt for 1 and 10)",
            ),
            (
                "Arrow keys after clicking a corner point",
                "Move that point instead, by the same steps; a click elsewhere or Esc goes back to the selection",
            ),
            ("Ctrl+Alt+C, Ctrl+Alt+V", "Copy Style, Paste Style"),
            (
                "Enter",
                "With A+… → B+…: on to the products, then make the reaction",
            ),
            (
                "Esc",
                "Cancel what is in progress (a drag, a reaction being picked, a layout) and go back to the Select tool",
            ),
            ("Ctrl+S, Ctrl+Shift+S", "Save, Save As"),
            ("Ctrl+O", "Open"),
            (
                "Ctrl+N",
                "New (in a browser, Ctrl+N opens a new browser window: use File ▸ New)",
            ),
        ],
    ),
];

/// The text as this platform names its keys: Cmd and Option on the macOS
/// desktop app. A browser can't tell for sure, so it keeps Ctrl and Alt and
/// the window says so.
fn keys(text: &str) -> String {
    if cfg!(target_os = "macos") {
        text.replace("Ctrl", "Cmd").replace("Alt", "Option")
    } else {
        text.to_owned()
    }
}

/// The window, while `open`.
pub fn show(ctx: &egui::Context, open: &mut bool) {
    egui::Window::new("Mouse and Keyboard")
        .open(open)
        .resizable(true)
        .default_size([680.0, 720.0])
        .show(ctx, |ui| {
            if !cfg!(target_os = "macos") {
                ui.label(
                    RichText::new("On a Mac, Cmd takes the place of Ctrl and Option of Alt.")
                        .weak(),
                );
            }
            egui::ScrollArea::vertical().show(ui, |ui| {
                for (title, rows) in SECTIONS {
                    ui.add_space(6.0);
                    ui.label(RichText::new(*title).strong());
                    egui::Grid::new(*title)
                        .num_columns(2)
                        .striped(true)
                        .spacing([16.0, 4.0])
                        .show(ui, |ui| {
                            for (action, effect) in *rows {
                                ui.label(keys(action));
                                ui.add(egui::Label::new(keys(effect)).wrap());
                                ui.end_row();
                            }
                        });
                }
            });
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The README's Mouse and keyboard section, without its Markdown marks.
    fn readme() -> String {
        include_str!("../../../README.md")
            .replace("**", "")
            .replace('*', "")
    }

    #[test]
    fn every_row_is_in_the_readme() {
        let readme = readme();
        for (title, rows) in SECTIONS {
            assert!(readme.contains(title), "section {title}");
            for (action, effect) in *rows {
                let row = format!("| {action} | {effect} |");
                assert!(readme.contains(&row), "not in the README: {row}");
            }
        }
    }

    #[test]
    fn every_readme_row_is_in_the_window() {
        let readme = readme();
        let start = readme.find("## Mouse and keyboard").unwrap();
        let end = readme[start..].find("\n## ").unwrap() + start;
        let rows: Vec<&str> = readme[start..end]
            .lines()
            .filter(|l| {
                l.starts_with("| ")
                    && !l.starts_with("| Action")
                    && !l.starts_with("| Tool")
                    && !l.starts_with("| Keys |")
            })
            .collect();
        let ours: usize = SECTIONS.iter().map(|(_, r)| r.len()).sum();
        assert_eq!(rows.len(), ours, "{rows:#?}");
    }

    #[test]
    fn mac_names_its_keys() {
        let s = keys("Ctrl+Alt+C");
        if cfg!(target_os = "macos") {
            assert_eq!(s, "Cmd+Option+C");
        } else {
            assert_eq!(s, "Ctrl+Alt+C");
        }
    }
}
