//! PathwayDesigner graphics export (SPEC-Rust §9).
//!
//! Turns the SVG written by `pd-core`'s `SvgRenderer` into PNG through resvg,
//! and offers `pd-core`'s vector PDF and SVG next to it, so the app finds all
//! graphics output in one place. Pure Rust, no egui, builds for wasm32.

use pd_core::model::Model;
use pd_core::render::constants::PNG_DEFAULT_DPI;
pub use pd_core::render::pdf::{PageSize, model_to_pdf};
pub use pd_core::render::svg::model_to_svg;
pub use resvg::tiny_skia::Pixmap;

/// SVG user units per inch: CSS pixels.
const SVG_DPI: f64 = 96.0;

/// Something went wrong turning SVG into an image.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExportError(pub String);

impl std::fmt::Display for ExportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ExportError {}

/// Renders an SVG document at its own size, one pixel per user unit.
pub fn svg_to_pixmap(svg: &str) -> Result<Pixmap, ExportError> {
    let tree = resvg::usvg::Tree::from_str(svg, &resvg::usvg::Options::default())
        .map_err(|e| ExportError(format!("the SVG could not be read: {e}")))?;
    let size = tree.size().to_int_size();
    let mut pixmap = Pixmap::new(size.width(), size.height())
        .ok_or_else(|| ExportError(format!("the image size {size:?} is not usable")))?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::identity(),
        &mut pixmap.as_mut(),
    );
    Ok(pixmap)
}

/// The model as PNG bytes: the content bounds at `dpi` (S §11.6; 150 by
/// default, so a scale of 150/96), without anything that depends on the
/// selection.
pub fn model_to_png(
    model: &Model,
    dpi: f64,
    show_alias_indicator: bool,
    show_null_species: bool,
) -> Result<Vec<u8>, ExportError> {
    let svg = model_to_svg(
        model,
        dpi / SVG_DPI,
        show_alias_indicator,
        show_null_species,
    );
    svg_to_pixmap(&svg)?
        .encode_png()
        .map_err(|e| ExportError(format!("the PNG could not be written: {e}")))
}

/// The default PNG resolution (S §17.1).
pub const DEFAULT_DPI: f64 = PNG_DEFAULT_DPI;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn svg_renders_at_its_own_size() {
        let svg = r##"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="10" viewBox="0 0 20 10">
            <rect x="0" y="0" width="10" height="10" fill="#FF0000"/></svg>"##;
        let p = svg_to_pixmap(svg).unwrap();
        assert_eq!((p.width(), p.height()), (20, 10));
        let red = p.pixel(5, 5).unwrap();
        assert_eq!(
            (red.red(), red.green(), red.blue(), red.alpha()),
            (255, 0, 0, 255)
        );
        assert_eq!(p.pixel(15, 5).unwrap().alpha(), 0);
    }

    #[test]
    fn bad_svg_is_an_error() {
        assert!(svg_to_pixmap("<not svg").is_err());
    }

    #[test]
    fn png_size_follows_the_dpi() {
        let model = Model::new();
        // Empty model: 800 × 600 world units, at 150 DPI = 1250 × 937.5 → 938.
        let png = model_to_png(&model, DEFAULT_DPI, true, true).unwrap();
        let p = Pixmap::decode_png(&png).unwrap();
        assert_eq!((p.width(), p.height()), (1250, 938));
    }
}
