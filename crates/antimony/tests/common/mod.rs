//! Shared by the integration tests: finding the external corpus.

#![allow(dead_code)]

use std::fs;
use std::path::PathBuf;

/// The number of `NNNNN-antimony.ant` files the corpus generator wrote.
pub const CORPUS_SIZE: usize = 1788;

/// The corpus folder from `ANTIMONY_CORPUS`, or `None` (with a note on
/// stderr) when it is not set, so the calling test can skip.
pub fn corpus_root(test: &str) -> Option<PathBuf> {
    match std::env::var_os("ANTIMONY_CORPUS") {
        Some(root) => Some(PathBuf::from(root)),
        None => {
            eprintln!("note: ANTIMONY_CORPUS is not set; {test} skipped");
            None
        }
    }
}

/// Every case's Antimony file, as (case id, path), sorted by case id.
pub fn corpus_files(root: &PathBuf) -> Vec<(String, PathBuf)> {
    let mut files = Vec::new();
    for entry in fs::read_dir(root).unwrap_or_else(|e| panic!("{}: {e}", root.display())) {
        let case = entry.unwrap().path();
        let Some(id) = case.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let file = case.join(format!("{id}-antimony.ant"));
        if file.is_file() {
            files.push((id.to_string(), file));
        }
    }
    files.sort();
    files
}

/// Reads a corpus file as UTF-8.
pub fn read(path: &PathBuf) -> String {
    fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}
