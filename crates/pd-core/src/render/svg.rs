//! SVG output (SPEC-Rust §9).
//!
//! [`SvgRenderer`] writes each drawing call as an SVG element. Labels are
//! written as glyph outlines, not text, so the file looks the same in any
//! viewer on any machine, with or without the font installed. PNG and PDF are
//! made from this SVG by `pd-export`.

use std::fmt::Write as _;

use crate::geometry::pt;
use crate::model::TextFont;
use crate::model::{Model, Rgba};
use crate::render::draw::{DrawOptions, Shown, content_bounds, draw_model};
use crate::render::{Renderer, SPt, SRect, Stroke, spt};
use crate::text::{fmt_num, font_metrics_in, text_path_in, text_width_in};

/// Collects SVG elements; [`SvgRenderer::finish`] wraps them in a document.
pub struct SvgRenderer {
    width: f32,
    height: f32,
    body: String,
}

impl SvgRenderer {
    /// A document `width × height` user units in size.
    pub fn new(width: f32, height: f32) -> Self {
        Self {
            width,
            height,
            body: String::new(),
        }
    }

    pub fn finish(self) -> String {
        let (w, h) = (num(self.width), num(self.height));
        format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}\" height=\"{h}\" \
             viewBox=\"0 0 {w} {h}\">\n{}</svg>\n",
            self.body
        )
    }

    fn fill_attrs(&mut self, fill: Option<Rgba>) {
        match fill {
            Some(c) => {
                let _ = write!(self.body, " fill=\"{}\"", rgb(c));
                if c.0[3] != 255 {
                    let _ = write!(self.body, " fill-opacity=\"{}\"", opacity(c));
                }
            }
            None => self.body.push_str(" fill=\"none\""),
        }
    }

    fn stroke_attrs(&mut self, stroke: Option<Stroke>) {
        let Some(s) = stroke else {
            return;
        };
        let _ = write!(
            self.body,
            " stroke=\"{}\" stroke-width=\"{}\"",
            rgb(s.color),
            num(s.width)
        );
        if s.color.0[3] != 255 {
            let _ = write!(self.body, " stroke-opacity=\"{}\"", opacity(s.color));
        }
        if let Some((on, off)) = s.dash {
            let _ = write!(self.body, " stroke-dasharray=\"{} {}\"", num(on), num(off));
        }
        if s.round_cap {
            self.body
                .push_str(" stroke-linecap=\"round\" stroke-linejoin=\"round\"");
        }
    }

    fn points(&mut self, pts: &[SPt]) {
        self.body.push_str(" points=\"");
        for (i, p) in pts.iter().enumerate() {
            if i > 0 {
                self.body.push(' ');
            }
            let _ = write!(self.body, "{},{}", num(p.x), num(p.y));
        }
        self.body.push('"');
    }
}

impl Renderer for SvgRenderer {
    fn round_rect(&mut self, r: SRect, radius: f32, fill: Option<Rgba>, stroke: Option<Stroke>) {
        let _ = write!(
            self.body,
            "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"",
            num(r.min.x),
            num(r.min.y),
            num(r.width()),
            num(r.height())
        );
        if radius > 0.0 {
            let _ = write!(self.body, " rx=\"{}\"", num(radius));
        }
        self.fill_attrs(fill);
        self.stroke_attrs(stroke);
        self.body.push_str("/>\n");
    }

    fn circle(&mut self, c: SPt, radius: f32, fill: Option<Rgba>, stroke: Option<Stroke>) {
        let _ = write!(
            self.body,
            "<circle cx=\"{}\" cy=\"{}\" r=\"{}\"",
            num(c.x),
            num(c.y),
            num(radius)
        );
        self.fill_attrs(fill);
        self.stroke_attrs(stroke);
        self.body.push_str("/>\n");
    }

    fn polyline(&mut self, pts: &[SPt], stroke: Stroke) {
        self.body.push_str("<polyline");
        self.points(pts);
        self.fill_attrs(None);
        self.stroke_attrs(Some(stroke));
        self.body.push_str("/>\n");
    }

    fn cubic(&mut self, p0: SPt, c1: SPt, c2: SPt, p3: SPt, stroke: Stroke) {
        let _ = write!(
            self.body,
            "<path d=\"M{} {}C{} {} {} {} {} {}\"",
            num(p0.x),
            num(p0.y),
            num(c1.x),
            num(c1.y),
            num(c2.x),
            num(c2.y),
            num(p3.x),
            num(p3.y)
        );
        self.fill_attrs(None);
        self.stroke_attrs(Some(stroke));
        self.body.push_str("/>\n");
    }

    fn polygon(&mut self, pts: &[SPt], fill: Rgba) {
        self.body.push_str("<polygon");
        self.points(pts);
        self.fill_attrs(Some(fill));
        self.body.push_str("/>\n");
    }

    fn text(&mut self, center: SPt, s: &str, size: f32, font: TextFont, color: Rgba) {
        let size = f64::from(size);
        let x = f64::from(center.x) - text_width_in(font, s, size) / 2.0;
        let baseline = f64::from(center.y) + font_metrics_in(font, size).cap_height / 2.0;
        let d = text_path_in(font, s, size, x, baseline);
        if d.is_empty() {
            return;
        }
        let _ = write!(self.body, "<path d=\"{d}\"");
        self.fill_attrs(Some(color));
        self.body.push_str("/>\n");
    }
}

/// The model as an SVG document for output: the content bounds (S §11.6) at
/// `scale` user units per world unit, with nothing that depends on the
/// selection, and the optional parts `shown`.
pub fn model_to_svg(model: &Model, scale: f64, shown: Shown) -> String {
    let (min, max) = content_bounds(model);
    let size = (max - min) * scale;
    let (w, h) = (size.x.ceil() as f32, size.y.ceil() as f32);
    let mut svg = SvgRenderer::new(w, h);
    let opts = DrawOptions {
        zoom: scale,
        scroll: pt(-min.x * scale, -min.y * scale),
        background: Some(SRect {
            min: spt(0.0, 0.0),
            max: spt(w, h),
        }),
        for_output: true,
        ..DrawOptions::default()
    }
    .with(shown);
    draw_model(model, &opts, &mut svg);
    svg.finish()
}

fn num(v: f32) -> String {
    fmt_num(f64::from(v))
}

fn rgb(c: Rgba) -> String {
    let [r, g, b, _] = c.0;
    format!("#{r:02X}{g:02X}{b:02X}")
}

fn opacity(c: Rgba) -> String {
    fmt_num(f64::from(c.0[3]) / 255.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Participant;

    #[test]
    fn elements_and_attributes() {
        let mut svg = SvgRenderer::new(100.0, 50.0);
        svg.round_rect(
            SRect {
                min: spt(1.0, 2.0),
                max: spt(81.0, 38.0),
            },
            8.0,
            Some(Rgba::rgb(0xEE, 0xF6, 0xFF)),
            Some(Stroke {
                width: 1.5,
                color: Rgba([0x7A, 0x6F, 0xC8, 128]),
                dash: Some((5.0, 4.0)),
                round_cap: false,
            }),
        );
        svg.polyline(
            &[spt(0.0, 0.0), spt(10.5, 3.25)],
            Stroke {
                width: 2.0,
                color: Rgba::rgb(0x44, 0x44, 0x44),
                dash: None,
                round_cap: true,
            },
        );
        svg.polygon(
            &[spt(0.0, 0.0), spt(1.0, 0.0), spt(0.0, 1.0)],
            Rgba::rgb(1, 2, 3),
        );
        svg.circle(spt(5.0, 5.0), 4.0, Some(Rgba::rgb(0xFF, 0x88, 0)), None);
        let doc = svg.finish();
        assert!(
            doc.starts_with(
                "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"100\" height=\"50\""
            )
        );
        assert!(doc.contains(
            "<rect x=\"1\" y=\"2\" width=\"80\" height=\"36\" rx=\"8\" fill=\"#EEF6FF\" \
             stroke=\"#7A6FC8\" stroke-width=\"1.5\" stroke-opacity=\"0.5\" stroke-dasharray=\"5 4\"/>"
        ));
        assert!(doc.contains(
            "<polyline points=\"0,0 10.5,3.25\" fill=\"none\" stroke=\"#444444\" stroke-width=\"2\" \
             stroke-linecap=\"round\" stroke-linejoin=\"round\"/>"
        ));
        assert!(doc.contains("<polygon points=\"0,0 1,0 0,1\" fill=\"#010203\"/>"));
        assert!(doc.contains("<circle cx=\"5\" cy=\"5\" r=\"4\" fill=\"#FF8800\"/>"));
        assert!(doc.ends_with("</svg>\n"));
    }

    #[test]
    fn labels_are_outlines_not_text() {
        let mut svg = SvgRenderer::new(100.0, 50.0);
        svg.text(
            spt(50.0, 25.0),
            "S1",
            12.0,
            TextFont::default(),
            Rgba::rgb(0x1A, 0x1A, 0x1A),
        );
        let doc = svg.finish();
        assert!(!doc.contains("<text"));
        assert!(doc.contains("<path d=\"M"));
        assert!(doc.contains("fill=\"#1A1A1A\""));
    }

    #[test]
    fn model_svg_covers_the_content_bounds() {
        let mut m = Model::new();
        let a = m.add_species(pt(100.0, 100.0), 80.0, 36.0);
        let b = m.add_species(pt(300.0, 100.0), 80.0, 36.0);
        let r = m.add_reaction(pt(200.0, 100.0));
        let r = m.find_reaction_mut(&r).unwrap();
        r.reactants.push(Participant::new(a, 1.0));
        r.products.push(Participant::new(b, 1.0));
        m.find_species_mut("S1").unwrap().selected = true;
        let doc = model_to_svg(&m, 1.0, Shown::ALL);
        // Bounds: x 60..340, y 82..118, plus 60 on each side.
        assert!(doc.contains("width=\"400\" height=\"156\""));
        // The background fills it; the first node's rectangle starts 60 in.
        assert!(
            doc.contains("<rect x=\"0\" y=\"0\" width=\"400\" height=\"156\" fill=\"#F8F9FA\"/>")
        );
        assert!(doc.contains("<rect x=\"60\" y=\"60\" width=\"80\" height=\"36\" rx=\"8\""));
        // No selection halo in output.
        assert!(!doc.contains("#FF0000"));
    }
}
