//! Preferences (S §12, SPEC-Rust §10.3).
//!
//! Kept in eframe's storage: a file in the app's data folder on the desktop,
//! `localStorage` in the browser. Keys start with `pathwayDesigner.`, because
//! every project page on a `github.io` site shares one `localStorage`. The
//! saved values are applied when the app starts (fixes D-2).

const KEY_CATALYTIC: &str = "pathwayDesigner.convertCatalyticReactions";
const KEY_NULL_SPECIES: &str = "pathwayDesigner.showNullSpecies";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Preferences {
    /// During Antimony import, `A -> A + B` becomes `-> B` with `A` as a modifier.
    pub convert_catalytic: bool,
    /// Draw null nodes and the legs that touch them.
    pub show_null_species: bool,
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            convert_catalytic: true,
            show_null_species: true,
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
        }
        p
    }

    pub fn save(&self, storage: &mut dyn eframe::Storage) {
        storage.set_string(KEY_CATALYTIC, self.convert_catalytic.to_string());
        storage.set_string(KEY_NULL_SPECIES, self.show_null_species.to_string());
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
        };
        p.save(&mut m);
        assert_eq!(m.0["pathwayDesigner.convertCatalyticReactions"], "false");
        assert_eq!(Preferences::load(Some(&m)), p);
    }
}
