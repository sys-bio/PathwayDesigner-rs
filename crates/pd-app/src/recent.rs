//! Recently used files, on the desktop (SPEC-Rust §15.1): File ▸ Open Recent.
//! The list is kept in eframe's storage (a file in the app's data folder),
//! newest first.

use std::path::{Path, PathBuf};

/// The storage key.
pub const KEY: &str = "pathwayDesigner.recent";
/// How many files the list keeps.
pub const MAX: usize = 8;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Recent {
    pub files: Vec<PathBuf>,
}

impl Recent {
    pub fn load(storage: Option<&dyn eframe::Storage>) -> Self {
        storage
            .and_then(|s| s.get_string(KEY))
            .map(|t| Self::decode(&t))
            .unwrap_or_default()
    }

    pub fn save(&self, storage: &mut dyn eframe::Storage) {
        storage.set_string(KEY, self.encode());
    }

    /// One path per line.
    fn encode(&self) -> String {
        self.files
            .iter()
            .map(|p| p.to_string_lossy())
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn decode(text: &str) -> Self {
        let mut r = Recent::default();
        for line in text.lines().rev().filter(|l| !l.trim().is_empty()) {
            r.add(Path::new(line));
        }
        r
    }

    /// Puts `path` at the top, once, keeping at most [`MAX`].
    pub fn add(&mut self, path: &Path) {
        self.remove(path);
        self.files.insert(0, path.to_path_buf());
        self.files.truncate(MAX);
    }

    pub fn remove(&mut self, path: &Path) {
        self.files.retain(|p| p != path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newest_first_once_and_at_most_max() {
        let mut r = Recent::default();
        for i in 0..10 {
            r.add(Path::new(&format!("/m/{i}.json")));
        }
        assert_eq!(r.files.len(), MAX);
        assert_eq!(r.files[0], Path::new("/m/9.json"));
        // Opening one again moves it to the top, without a second entry.
        r.add(Path::new("/m/5.json"));
        assert_eq!(r.files[0], Path::new("/m/5.json"));
        assert_eq!(r.files.iter().filter(|p| p.ends_with("5.json")).count(), 1);
        assert_eq!(r.files.len(), MAX);
    }

    #[test]
    fn kept_between_runs() {
        let mut r = Recent::default();
        r.add(Path::new("C:\\models\\a.json"));
        r.add(Path::new("/home/me/b.ant"));
        assert_eq!(Recent::decode(&r.encode()), r);
        assert_eq!(Recent::decode(""), Recent::default());
    }
}
