//! Round-trip tests on the reference files in `Models/` (SPEC-Rust §4.1, S §16 M1).
//!
//! For every `Models/*.json`:
//! 1. it loads with no warnings;
//! 2. saving gives the same content as the original file: key order and number
//!    formatting may differ, numbers may differ by 1e-9, and colours are
//!    compared by value (some v3 files still use legacy integer colours);
//! 3. saving and reloading gives the same model, field by field.

use std::fs;
use std::path::{Path, PathBuf};

use pd_core::json::{colour, from_json, to_json};
use serde_json::Value;

fn model_files() -> Vec<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../Models");
    let mut files: Vec<PathBuf> = fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("can't read {}: {e}", dir.display()))
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    files.sort();
    assert!(!files.is_empty(), "no reference files in {}", dir.display());
    files
}

fn read(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// Keys the writer may add or leave out because the value is the default:
/// a missing key on one side matches this value on the other.
fn default_for(key: &str) -> Option<Value> {
    match key {
        // Older Delphi versions wrote style objects without it.
        "junctionVisible" => Some(Value::Bool(true)),
        // Older Delphi versions left out empty modifier lists.
        "modifiers" => Some(Value::Array(Vec::new())),
        "compartment" => Some(Value::String(String::new())),
        "isNullNode" | "locked" => Some(Value::Bool(false)),
        _ => None,
    }
}

fn decode_colour(v: &Value) -> Option<pd_core::model::Rgba> {
    match v {
        Value::String(s) => colour::from_hex(s),
        Value::Number(n) => n.as_i64().and_then(colour::from_legacy_int),
        _ => None,
    }
}

/// Compares two JSON values by content. Pushes a description of each
/// difference onto `diffs`, with its path.
fn compare(path: &str, key: &str, a: &Value, b: &Value, diffs: &mut Vec<String>) {
    if key.ends_with("Color") {
        let (ca, cb) = (decode_colour(a), decode_colour(b));
        if ca.is_none() || ca != cb {
            diffs.push(format!("{path}: colour {a} vs {b}"));
        }
        return;
    }
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => {
            let (x, y) = (x.as_f64().unwrap(), y.as_f64().unwrap());
            if (x - y).abs() > 1e-9 {
                diffs.push(format!("{path}: {x} vs {y}"));
            }
        }
        (Value::Array(x), Value::Array(y)) => {
            if x.len() != y.len() {
                diffs.push(format!("{path}: {} items vs {}", x.len(), y.len()));
                return;
            }
            for (i, (xa, ya)) in x.iter().zip(y).enumerate() {
                compare(&format!("{path}[{i}]"), key, xa, ya, diffs);
            }
        }
        (Value::Object(x), Value::Object(y)) => {
            let mut keys: Vec<&String> = x.keys().chain(y.keys()).collect();
            keys.sort();
            keys.dedup();
            for k in keys {
                let sub = format!("{path}.{k}");
                match (x.get(k), y.get(k)) {
                    (Some(va), Some(vb)) => compare(&sub, k, va, vb, diffs),
                    (Some(v), None) | (None, Some(v)) => {
                        if default_for(k).as_ref() != Some(v) {
                            diffs.push(format!("{sub}: only on one side ({v})"));
                        }
                    }
                    (None, None) => unreachable!(),
                }
            }
        }
        _ => {
            if a != b {
                diffs.push(format!("{path}: {a} vs {b}"));
            }
        }
    }
}

#[test]
fn every_reference_file_loads_without_warnings() {
    for path in model_files() {
        let loaded = from_json(&read(&path)).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        assert!(
            loaded.warnings.is_empty(),
            "{}: {:?}",
            path.display(),
            loaded.warnings
        );
        let needed = pd_core::json::needed_version(&loaded.model);
        assert_eq!(loaded.version, needed, "{}", path.display());
        assert!(!loaded.model.species().is_empty(), "{}", path.display());
    }
}

#[test]
fn saving_gives_back_the_same_content() {
    let mut failures = Vec::new();
    for path in model_files() {
        let original_text = read(&path);
        let original: Value =
            serde_json::from_str(original_text.trim_start_matches('\u{FEFF}')).unwrap();
        let saved: Value =
            serde_json::from_str(&to_json(&from_json(&original_text).unwrap().model)).unwrap();
        let mut diffs = Vec::new();
        compare("$", "", &original, &saved, &mut diffs);
        if !diffs.is_empty() {
            failures.push(format!("{}:\n  {}", path.display(), diffs.join("\n  ")));
        }
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

#[test]
fn save_and_reload_gives_the_same_model() {
    for path in model_files() {
        let first = from_json(&read(&path)).unwrap().model;
        let text = to_json(&first);
        let second = from_json(&text).unwrap().model;
        assert_eq!(first, second, "{}", path.display());
        // And saving again is byte-for-byte stable.
        assert_eq!(text, to_json(&second), "{}", path.display());
    }
}

#[test]
fn the_comparison_catches_real_differences() {
    let original: Value = serde_json::from_str(
        r##"{ "species": [ { "id": "A", "x": 1.5, "style": { "fillColor": -2572328 } } ] }"##,
    )
    .unwrap();
    let same: Value = serde_json::from_str(
        r##"{ "species": [ { "x": 1.5000000000001, "id": "A",
             "style": { "fillColor": "#D8BFD8FF", "junctionVisible": true } } ] }"##,
    )
    .unwrap();
    let mut diffs = Vec::new();
    compare("$", "", &original, &same, &mut diffs);
    assert!(diffs.is_empty(), "{diffs:?}");

    for changed in [
        r##"{ "species": [ { "id": "B", "x": 1.5, "style": { "fillColor": -2572328 } } ] }"##,
        r##"{ "species": [ { "id": "A", "x": 1.6, "style": { "fillColor": -2572328 } } ] }"##,
        r##"{ "species": [ { "id": "A", "x": 1.5, "style": { "fillColor": "#D8BFD8FE" } } ] }"##,
        r##"{ "species": [ { "id": "A", "x": 1.5 } ] }"##,
        r##"{ "species": [ { "id": "A", "x": 1.5, "style": { "fillColor": -2572328 },
             "aliasOf": "Z" } ] }"##,
        r##"{ "species": [] }"##,
    ] {
        let mut diffs = Vec::new();
        compare(
            "$",
            "",
            &original,
            &serde_json::from_str(changed).unwrap(),
            &mut diffs,
        );
        assert!(!diffs.is_empty(), "not caught: {changed}");
    }
}
