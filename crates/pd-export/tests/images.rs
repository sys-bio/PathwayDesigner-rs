//! Image comparison tests (SPEC-Rust §4.1, §12).
//!
//! Every `Models/*.json` is drawn with `SvgRenderer` at zoom 1, turned into PNG
//! with resvg, and compared with a stored image in `tests/snapshots/`. The
//! whole path is pure Rust with the bundled font, so the result is the same on
//! every platform; a small tolerance covers rounding in anti-aliasing.
//!
//! After an intended change to the drawing, regenerate the stored images with
//! `PD_UPDATE_SNAPSHOTS=1 cargo test -p pd-export --test images`, look at
//! them, and commit them. When a comparison fails, the new image is written to
//! `target/image-diffs/` for a look.

use std::fs;
use std::path::{Path, PathBuf};

use pd_core::json::from_json;
use pd_core::render::svg::model_to_svg;
use pd_export::{Pixmap, svg_to_pixmap};

/// A channel may differ by this much before the pixel counts as different.
const CHANNEL_TOLERANCE: u8 = 2;
/// At most this many pixels may differ.
const MAX_DIFFERENT_PIXELS: usize = 50;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn model_files() -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = fs::read_dir(root().join("Models"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    files.sort();
    files
}

fn render(path: &Path) -> Pixmap {
    let model = from_json(&fs::read_to_string(path).unwrap()).unwrap().model;
    svg_to_pixmap(&model_to_svg(&model, 1.0, pd_export::Shown::ALL)).unwrap()
}

/// The number of pixels that differ by more than the tolerance in any channel.
fn different_pixels(a: &Pixmap, b: &Pixmap) -> usize {
    a.data()
        .as_chunks::<4>()
        .0
        .iter()
        .zip(b.data().as_chunks::<4>().0)
        .filter(|(x, y)| {
            x.iter()
                .zip(*y)
                .any(|(p, q)| p.abs_diff(*q) > CHANNEL_TOLERANCE)
        })
        .count()
}

#[test]
fn reference_models_draw_as_stored() {
    let update = std::env::var_os("PD_UPDATE_SNAPSHOTS").is_some();
    let snapshots = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/snapshots");
    let diffs = root().join("target/image-diffs");
    let mut failures = Vec::new();

    for path in model_files() {
        let name = path.file_stem().unwrap().to_string_lossy().into_owned();
        let stored_path = snapshots.join(format!("{name}.png"));
        let actual = render(&path);

        if update {
            fs::create_dir_all(&snapshots).unwrap();
            fs::write(&stored_path, actual.encode_png().unwrap()).unwrap();
            continue;
        }
        let Ok(bytes) = fs::read(&stored_path) else {
            failures.push(format!(
                "{name}: no stored image; run with PD_UPDATE_SNAPSHOTS=1"
            ));
            continue;
        };
        let stored = Pixmap::decode_png(&bytes).unwrap();
        let problem = if (stored.width(), stored.height()) != (actual.width(), actual.height()) {
            Some(format!(
                "size {}×{} instead of {}×{}",
                actual.width(),
                actual.height(),
                stored.width(),
                stored.height()
            ))
        } else {
            let n = different_pixels(&stored, &actual);
            (n > MAX_DIFFERENT_PIXELS).then(|| format!("{n} pixels differ"))
        };
        if let Some(problem) = problem {
            fs::create_dir_all(&diffs).unwrap();
            let out = diffs.join(format!("{name}.png"));
            fs::write(&out, actual.encode_png().unwrap()).unwrap();
            failures.push(format!("{name}: {problem} (new image: {})", out.display()));
        }
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n"));
}

#[test]
fn drawing_is_deterministic() {
    let path = &model_files()[0];
    assert_eq!(render(path).data(), render(path).data());
}
