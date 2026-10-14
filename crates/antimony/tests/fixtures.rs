//! The test data is in place and readable (milestone A0).
//!
//! `tests/invalid/` holds one Antimony file per diagnostic, each with a
//! `.expected` file naming the codes and lines it must produce. A0 only
//! checks the set is well formed; A3 and A4 check the parser against it.
//!
//! The corpus lives outside the repository and is found through
//! `ANTIMONY_CORPUS` (SPEC.md §8). Without it the corpus test is skipped
//! with a note, so a fresh clone still passes.

use std::fs;
use std::path::{Path, PathBuf};

/// The number of `NNNNN-antimony.ant` files the corpus generator wrote.
const CORPUS_SIZE: usize = 1788;

/// One expected diagnostic: its Delphi code name (`dcTypeConflict`) and the
/// 1-based line it is reported on.
#[derive(Debug, PartialEq)]
struct Expected {
    code: String,
    line: u32,
}

/// Reads a `.expected` file: `#` lines are comments, every other non-blank
/// line is `<code> <line>`.
fn read_expected(path: &Path) -> Result<Vec<Expected>, String> {
    let text = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut expected = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let bad = || {
            format!(
                "{}:{}: expected `<code> <line>`, got {line:?}",
                path.display(),
                i + 1
            )
        };
        let mut parts = line.split_whitespace();
        let (Some(code), Some(n), None) = (parts.next(), parts.next(), parts.next()) else {
            return Err(bad());
        };
        if !code.starts_with("dc") {
            return Err(bad());
        }
        let line = n.parse::<u32>().map_err(|_| bad())?;
        expected.push(Expected {
            code: code.to_string(),
            line,
        });
    }
    Ok(expected)
}

fn invalid_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/invalid")
}

/// The files in `dir` with extension `ext`, sorted by name.
fn files_with(dir: &Path, ext: &str) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = fs::read_dir(dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .map(|entry| entry.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == ext))
        .collect();
    files.sort();
    files
}

#[test]
fn invalid_fixtures_are_well_formed() {
    let dir = invalid_dir();
    let sources = files_with(&dir, "ant");
    let expectations = files_with(&dir, "expected");
    assert_eq!(sources.len(), 12, "the Delphi set has 12 fixtures");
    assert_eq!(
        sources
            .iter()
            .map(|p| p.with_extension(""))
            .collect::<Vec<_>>(),
        expectations
            .iter()
            .map(|p| p.with_extension(""))
            .collect::<Vec<_>>(),
        "every .ant needs a .expected, and the other way round"
    );
    for source in &sources {
        let lines = fs::read_to_string(source).unwrap().lines().count() as u32;
        let expected = read_expected(&source.with_extension("expected")).unwrap();
        assert!(
            !expected.is_empty(),
            "{}: names no diagnostic",
            source.display()
        );
        // One past the last line is end of file, where an unterminated block
        // is reported (expected-end.ant).
        for e in &expected {
            assert!(
                (1..=lines + 1).contains(&e.line),
                "{}: line {} is outside the file ({lines} lines)",
                source.display(),
                e.line
            );
        }
    }
}

#[test]
fn expected_file_format() {
    let dir = std::env::temp_dir().join(format!("antimony-a0-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let good = dir.join("good.expected");
    fs::write(&good, "# a comment\n\ndcTypeConflict 3\ndcExpectedEnd 4\n").unwrap();
    assert_eq!(
        read_expected(&good).unwrap(),
        vec![
            Expected {
                code: "dcTypeConflict".into(),
                line: 3
            },
            Expected {
                code: "dcExpectedEnd".into(),
                line: 4
            },
        ]
    );
    for bad in [
        "dcTypeConflict",
        "dcTypeConflict three",
        "TypeConflict 3",
        "dcA 1 2",
    ] {
        let path = dir.join("bad.expected");
        fs::write(&path, bad).unwrap();
        assert!(read_expected(&path).is_err(), "{bad:?} should be rejected");
    }
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn corpus_is_reachable() {
    let Some(root) = std::env::var_os("ANTIMONY_CORPUS") else {
        eprintln!("note: ANTIMONY_CORPUS is not set; corpus test skipped");
        return;
    };
    let root = PathBuf::from(root);
    let mut cases = 0;
    for entry in fs::read_dir(&root).unwrap_or_else(|e| panic!("{}: {e}", root.display())) {
        let case = entry.unwrap().path();
        let Some(id) = case.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if case.join(format!("{id}-antimony.ant")).is_file() {
            cases += 1;
        }
    }
    assert_eq!(
        cases,
        CORPUS_SIZE,
        "Antimony files under {}",
        root.display()
    );
}
