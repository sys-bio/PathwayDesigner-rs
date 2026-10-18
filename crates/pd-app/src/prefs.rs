//! Preferences (S §12, SPEC-Rust §10.3).
//!
//! Kept in eframe's storage: a file in the app's data folder on the desktop,
//! `localStorage` in the browser. Keys start with `pathwayDesigner.`, because
//! every project page on a `github.io` site shares one `localStorage`. The
//! saved values are applied when the app starts (fixes D-2).

use pd_core::json::colour::{from_hex, to_hex};
use pd_core::model::Rgba;
use pd_core::render::constants::SELECTED_REACTION;

const KEY_CATALYTIC: &str = "pathwayDesigner.convertCatalyticReactions";
const KEY_NULL_SPECIES: &str = "pathwayDesigner.showNullSpecies";
const KEY_SELECTED_REACTION: &str = "pathwayDesigner.selectedReactionColour";
const KEY_MODIFIERS: &str = "pathwayDesigner.showModifiers";
const KEY_GRID: &str = "pathwayDesigner.showGrid";
const KEY_SNAP: &str = "pathwayDesigner.snapToGrid";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Preferences {
    /// During Antimony import, `A -> A + B` becomes `-> B` with `A` as a modifier.
    pub convert_catalytic: bool,
    /// Draw null nodes and the legs that touch them.
    pub show_null_species: bool,
    /// The colour a selected reaction is drawn in, to make it easy to see.
    pub selected_reaction: Rgba,
    /// View ▸ Modifier lines: draw modifier arcs. Set from the View menu,
    /// not the Preferences dialog, and remembered here.
    pub show_modifiers: bool,
    /// View ▸ Grid, remembered the same way.
    pub show_grid: bool,
    /// View ▸ Snap to grid, remembered the same way.
    pub snap_to_grid: bool,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            convert_catalytic: true,
            show_null_species: true,
            selected_reaction: SELECTED_REACTION,
            show_modifiers: true,
            show_grid: false,
            snap_to_grid: false,
        }
    }
}

impl Preferences {
    /// The saved preferences, or the defaults for anything not saved.
    pub fn load(storage: Option<&dyn eframe::Storage>) -> Self {
        let mut p = Self::default();
        if let Some(s) = storage {
            let flag = |key: &str, default: bool| match s.get_string(key).as_deref() {
                Some("true") => true,
                Some("false") => false,
                _ => default,
            };
            p.convert_catalytic = flag(KEY_CATALYTIC, p.convert_catalytic);
            p.show_null_species = flag(KEY_NULL_SPECIES, p.show_null_species);
            p.show_modifiers = flag(KEY_MODIFIERS, p.show_modifiers);
            p.show_grid = flag(KEY_GRID, p.show_grid);
            p.snap_to_grid = flag(KEY_SNAP, p.snap_to_grid);
            if let Some(c) = s
                .get_string(KEY_SELECTED_REACTION)
                .and_then(|h| from_hex(&h))
            {
                p.selected_reaction = c;
            }
        }
        p
    }

    pub fn save(&self, storage: &mut dyn eframe::Storage) {
        storage.set_string(KEY_CATALYTIC, self.convert_catalytic.to_string());
        storage.set_string(KEY_NULL_SPECIES, self.show_null_species.to_string());
        storage.set_string(KEY_SELECTED_REACTION, to_hex(self.selected_reaction));
        storage.set_string(KEY_MODIFIERS, self.show_modifiers.to_string());
        storage.set_string(KEY_GRID, self.show_grid.to_string());
        storage.set_string(KEY_SNAP, self.snap_to_grid.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[derive(Default)]
    struct Memory(HashMap<String, String>);

    impl eframe::Storage for Memory {
        fn get_string(&self, key: &str) -> Option<String> {
            self.0.get(key).cloned()
        }
        fn set_string(&mut self, key: &str, value: String) {
            self.0.insert(key.to_owned(), value);
        }
        fn remove_string(&mut self, key: &str) {
            self.0.remove(key);
        }
        fn flush(&mut self) {}
    }

    #[test]
    fn defaults_when_nothing_is_saved() {
        assert_eq!(Preferences::load(None), Preferences::default());
        assert_eq!(
            Preferences::load(Some(&Memory::default())),
            Preferences::default()
        );
    }

    #[test]
    fn saved_values_come_back() {
        let mut m = Memory::default();
        let p = Preferences {
            convert_catalytic: false,
            show_null_species: false,
            selected_reaction: Rgba([0xE0, 0x10, 0x80, 0xFF]),
            show_modifiers: false,
            show_grid: true,
            snap_to_grid: true,
        };
        p.save(&mut m);
        assert_eq!(m.0["pathwayDesigner.convertCatalyticReactions"], "false");
        assert_eq!(m.0["pathwayDesigner.showModifiers"], "false");
        assert_eq!(m.0["pathwayDesigner.showGrid"], "true");
        assert_eq!(m.0["pathwayDesigner.snapToGrid"], "true");
        assert_eq!(Preferences::load(Some(&m)), p);
    }
}
