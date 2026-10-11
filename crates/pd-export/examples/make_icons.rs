//! Renders `assets/icon/icon.svg` into the app icons: PNGs at the sizes the
//! installers and the window use, and a Windows `.ico` holding them.
//!
//! ```text
//! cargo run -p pd-export --example make_icons
//! ```

use std::path::Path;

use resvg::tiny_skia::{Pixmap, Transform};

const SIZES: [u32; 6] = [16, 32, 48, 64, 128, 256];

fn main() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/icon");
    let svg = std::fs::read_to_string(dir.join("icon.svg")).unwrap();
    let tree = resvg::usvg::Tree::from_str(&svg, &resvg::usvg::Options::default()).unwrap();
    let mut pngs = Vec::new();
    for size in SIZES.into_iter().chain([512]) {
        let mut pixmap = Pixmap::new(size, size).unwrap();
        let k = size as f32 / tree.size().width();
        resvg::render(&tree, Transform::from_scale(k, k), &mut pixmap.as_mut());
        let png = pixmap.encode_png().unwrap();
        std::fs::write(dir.join(format!("icon-{size}.png")), &png).unwrap();
        if SIZES.contains(&size) {
            pngs.push((size, png));
        }
    }
    // An ICO file: a header, one directory entry per image, then the images,
    // each stored as PNG (allowed since Windows Vista).
    let mut ico = Vec::new();
    ico.extend_from_slice(&[0, 0, 1, 0]);
    ico.extend_from_slice(&(pngs.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * pngs.len() as u32;
    for (size, png) in &pngs {
        let side = if *size >= 256 { 0 } else { *size as u8 };
        ico.extend_from_slice(&[side, side, 0, 0]);
        ico.extend_from_slice(&1u16.to_le_bytes()); // colour planes
        ico.extend_from_slice(&32u16.to_le_bytes()); // bits per pixel
        ico.extend_from_slice(&(png.len() as u32).to_le_bytes());
        ico.extend_from_slice(&offset.to_le_bytes());
        offset += png.len() as u32;
    }
    for (_, png) in &pngs {
        ico.extend_from_slice(png);
    }
    std::fs::write(dir.join("icon.ico"), ico).unwrap();
    println!("wrote the icons in {}", dir.display());
}
