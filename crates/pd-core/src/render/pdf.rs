//! Vector PDF output (S §11.6, SPEC-Rust §9).
//!
//! [`PdfRenderer`] writes each drawing call as PDF path operators, and
//! [`model_to_pdf`] puts the drawing on one page. Labels are glyph outlines,
//! as in SVG, so the file needs no font. The PDF is written here directly,
//! with no dependencies: the drawing uses only paths, fills, strokes and
//! transparency.

use std::fmt::Write as _;

use crate::geometry::pt;
use crate::model::{Model, Rgba};
use crate::render::draw::{DrawOptions, Shown, content_bounds, draw_model};
use crate::render::{Renderer, SPt, SRect, Stroke, spt};
use crate::text::{PathSink, font_metrics, text_outline, text_width};

/// Points per inch, PDF's unit.
const PT_PER_INCH: f64 = 72.0;
/// Screen pixels (world units at zoom 1) per inch.
const PX_PER_INCH: f64 = 96.0;
/// Margin round the drawing on a Letter or A4 page: half an inch.
pub const PAGE_MARGIN: f64 = 36.0;
/// Bézier constant for a quarter circle.
const KAPPA: f32 = 0.552_284_8;

/// The page the drawing goes on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PageSize {
    /// US Letter, 8.5 × 11 in (S §11.6's default).
    #[default]
    Letter,
    /// A4, 210 × 297 mm.
    A4,
    /// A page exactly the size of the drawing, at the size it would print
    /// from the screen (96 world units to the inch).
    Drawing,
}

impl PageSize {
    pub const ALL: [PageSize; 3] = [PageSize::Letter, PageSize::A4, PageSize::Drawing];

    pub fn label(self) -> &'static str {
        match self {
            PageSize::Letter => "Letter",
            PageSize::A4 => "A4",
            PageSize::Drawing => "Drawing size",
        }
    }

    /// Width and height in points, portrait; `None` for [`PageSize::Drawing`].
    fn points(self) -> Option<(f64, f64)> {
        match self {
            PageSize::Letter => Some((612.0, 792.0)),
            PageSize::A4 => Some((595.276, 841.89)),
            PageSize::Drawing => None,
        }
    }
}

/// The model as a one-page vector PDF (S §11.6): the content bounds, scaled
/// to fit the page inside its margin and centred, with nothing that depends
/// on the selection, and the optional parts `shown`. `landscape` turns a
/// Letter or A4 page sideways.
pub fn model_to_pdf(model: &Model, page: PageSize, landscape: bool, shown: Shown) -> Vec<u8> {
    let (min, max) = content_bounds(model);
    let (cw, ch) = (max.x - min.x, max.y - min.y);
    // The drawing's place on the page: (scale, left, top, width, height).
    let (pw, ph, scale, area) = match page.points() {
        // Whole world units from the top left, as the SVG and PNG are.
        None => {
            let k = PT_PER_INCH / PX_PER_INCH;
            let (w, h) = (cw.ceil() * k, ch.ceil() * k);
            (w, h, k, (0.0, 0.0, w, h))
        }
        // Fitted inside the margin and centred.
        Some((w, h)) => {
            let (w, h) = if landscape { (h, w) } else { (w, h) };
            let k = ((w - 2.0 * PAGE_MARGIN) / cw).min((h - 2.0 * PAGE_MARGIN) / ch);
            let (dw, dh) = (cw * k, ch * k);
            (w, h, k, ((w - dw) / 2.0, (h - dh) / 2.0, dw, dh))
        }
    };
    let (ox, oy, dw, dh) = area;
    let mut pdf = PdfRenderer::new(pw as f32, ph as f32);
    let opts = DrawOptions {
        zoom: scale,
        scroll: pt(ox - min.x * scale, oy - min.y * scale),
        background: Some(SRect {
            min: spt(ox as f32, oy as f32),
            max: spt((ox + dw) as f32, (oy + dh) as f32),
        }),
        for_output: true,
        ..DrawOptions::default()
    }
    .with(shown);
    draw_model(model, &opts, &mut pdf);
    pdf.finish()
}

/// Collects a page's content; [`PdfRenderer::finish`] writes the document.
/// Coordinates are in points with y pointing down, like the screen.
pub struct PdfRenderer {
    width: f32,
    height: f32,
    content: String,
    /// The (fill, stroke) opacities used, each an `ExtGState` resource.
    alphas: Vec<(u8, u8)>,
}

impl PdfRenderer {
    /// A page `width × height` points in size.
    pub fn new(width: f32, height: f32) -> Self {
        Self {
            width,
            height,
            // Flip y, so the drawing's y-down coordinates can be used as they are.
            content: format!("1 0 0 -1 0 {} cm\n", num(height)),
            alphas: Vec::new(),
        }
    }

    /// The finished PDF file.
    pub fn finish(self) -> Vec<u8> {
        let mut objects: Vec<String> = vec![
            "<< /Type /Catalog /Pages 2 0 R >>".into(),
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".into(),
        ];
        let first_gs = 5;
        let mut gs = String::new();
        for (i, _) in self.alphas.iter().enumerate() {
            let _ = write!(gs, " /GS{i} {} 0 R", first_gs + i);
        }
        objects.push(format!(
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {} {}] \
             /Resources << /ExtGState <<{gs} >> >> /Contents 4 0 R >>",
            num(self.width),
            num(self.height)
        ));
        objects.push(format!(
            "<< /Length {} >>\nstream\n{}endstream",
            self.content.len(),
            self.content
        ));
        for (fill, stroke) in &self.alphas {
            objects.push(format!(
                "<< /Type /ExtGState /ca {} /CA {} >>",
                alpha(*fill),
                alpha(*stroke)
            ));
        }
        objects.push("<< /Producer (PathwayDesigner) >>".into());
        let info = objects.len();

        let mut out: Vec<u8> = b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n".to_vec();
        let mut offsets = Vec::with_capacity(objects.len());
        for (i, body) in objects.iter().enumerate() {
            offsets.push(out.len());
            out.extend_from_slice(format!("{} 0 obj\n{body}\nendobj\n", i + 1).as_bytes());
        }
        let xref = out.len();
        let mut tail = format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1);
        for o in offsets {
            let _ = writeln!(tail, "{o:010} 00000 n ");
        }
        let _ = write!(
            tail,
            "trailer\n<< /Size {} /Root 1 0 R /Info {info} 0 R >>\nstartxref\n{xref}\n%%EOF\n",
            objects.len() + 1
        );
        out.extend_from_slice(tail.as_bytes());
        out
    }

    /// Starts an element: saves the graphics state and sets colours,
    /// opacities and the stroke.
    fn begin(&mut self, fill: Option<Rgba>, stroke: Option<Stroke>) {
        self.content.push_str("q\n");
        let fa = fill.map_or(255, |c| c.0[3]);
        let sa = stroke.map_or(255, |s| s.color.0[3]);
        if (fa, sa) != (255, 255) {
            let i = match self.alphas.iter().position(|a| *a == (fa, sa)) {
                Some(i) => i,
                None => {
                    self.alphas.push((fa, sa));
                    self.alphas.len() - 1
                }
            };
            let _ = writeln!(self.content, "/GS{i} gs");
        }
        if let Some(c) = fill {
            let _ = writeln!(self.content, "{} rg", rgb(c));
        }
        if let Some(s) = stroke {
            // SVG's miter limit, so joins look the same as in SVG and on screen.
            let _ = writeln!(self.content, "{} RG {} w 4 M", rgb(s.color), num(s.width));
            if let Some((on, off)) = s.dash {
                let _ = writeln!(self.content, "[{} {}] 0 d", num(on), num(off));
            }
            if s.round_cap {
                self.content.push_str("1 J 1 j\n");
            }
        }
    }

    /// Paints the path built since [`Self::begin`] and restores the state.
    fn end(&mut self, fill: bool, stroke: bool) {
        self.content.push_str(match (fill, stroke) {
            (true, true) => "B\nQ\n",
            (true, false) => "f\nQ\n",
            (false, true) => "S\nQ\n",
            (false, false) => "n\nQ\n",
        });
    }

    fn m(&mut self, p: SPt) {
        let _ = writeln!(self.content, "{} {} m", num(p.x), num(p.y));
    }

    fn l(&mut self, p: SPt) {
        let _ = writeln!(self.content, "{} {} l", num(p.x), num(p.y));
    }

    fn c(&mut self, c1: SPt, c2: SPt, p: SPt) {
        let _ = writeln!(
            self.content,
            "{} {} {} {} {} {} c",
            num(c1.x),
            num(c1.y),
            num(c2.x),
            num(c2.y),
            num(p.x),
            num(p.y)
        );
    }
}

impl Renderer for PdfRenderer {
    fn round_rect(&mut self, r: SRect, radius: f32, fill: Option<Rgba>, stroke: Option<Stroke>) {
        self.begin(fill, stroke);
        let rad = radius.max(0.0).min(r.width() / 2.0).min(r.height() / 2.0);
        let (x0, y0, x1, y1) = (r.min.x, r.min.y, r.max.x, r.max.y);
        if rad == 0.0 {
            let _ = writeln!(
                self.content,
                "{} {} {} {} re",
                num(x0),
                num(y0),
                num(r.width()),
                num(r.height())
            );
        } else {
            let k = rad * KAPPA;
            self.m(spt(x0 + rad, y0));
            self.l(spt(x1 - rad, y0));
            self.c(
                spt(x1 - rad + k, y0),
                spt(x1, y0 + rad - k),
                spt(x1, y0 + rad),
            );
            self.l(spt(x1, y1 - rad));
            self.c(
                spt(x1, y1 - rad + k),
                spt(x1 - rad + k, y1),
                spt(x1 - rad, y1),
            );
            self.l(spt(x0 + rad, y1));
            self.c(
                spt(x0 + rad - k, y1),
                spt(x0, y1 - rad + k),
                spt(x0, y1 - rad),
            );
            self.l(spt(x0, y0 + rad));
            self.c(
                spt(x0, y0 + rad - k),
                spt(x0 + rad - k, y0),
                spt(x0 + rad, y0),
            );
            self.content.push_str("h\n");
        }
        self.end(fill.is_some(), stroke.is_some());
    }

    fn circle(&mut self, c: SPt, radius: f32, fill: Option<Rgba>, stroke: Option<Stroke>) {
        self.begin(fill, stroke);
        let (r, k) = (radius, radius * KAPPA);
        self.m(spt(c.x + r, c.y));
        self.c(
            spt(c.x + r, c.y + k),
            spt(c.x + k, c.y + r),
            spt(c.x, c.y + r),
        );
        self.c(
            spt(c.x - k, c.y + r),
            spt(c.x - r, c.y + k),
            spt(c.x - r, c.y),
        );
        self.c(
            spt(c.x - r, c.y - k),
            spt(c.x - k, c.y - r),
            spt(c.x, c.y - r),
        );
        self.c(
            spt(c.x + k, c.y - r),
            spt(c.x + r, c.y - k),
            spt(c.x + r, c.y),
        );
        self.content.push_str("h\n");
        self.end(fill.is_some(), stroke.is_some());
    }

    fn polyline(&mut self, pts: &[SPt], stroke: Stroke) {
        let Some((first, rest)) = pts.split_first() else {
            return;
        };
        self.begin(None, Some(stroke));
        self.m(*first);
        for p in rest {
            self.l(*p);
        }
        self.end(false, true);
    }

    fn cubic(&mut self, p0: SPt, c1: SPt, c2: SPt, p3: SPt, stroke: Stroke) {
        self.begin(None, Some(stroke));
        self.m(p0);
        self.c(c1, c2, p3);
        self.end(false, true);
    }

    fn polygon(&mut self, pts: &[SPt], fill: Rgba) {
        let Some((first, rest)) = pts.split_first() else {
            return;
        };
        self.begin(Some(fill), None);
        self.m(*first);
        for p in rest {
            self.l(*p);
        }
        self.content.push_str("h\n");
        self.end(true, false);
    }

    fn text(&mut self, center: SPt, s: &str, size: f32, color: Rgba) {
        let size = f64::from(size);
        let x = f64::from(center.x) - text_width(s, size) / 2.0;
        let baseline = f64::from(center.y) + font_metrics(size).cap_height / 2.0;
        let mut path = PdfPath {
            ops: String::new(),
            at: (0.0, 0.0),
        };
        text_outline(s, size, x, baseline, &mut path);
        if path.ops.is_empty() {
            return;
        }
        self.begin(Some(color), None);
        self.content.push_str(&path.ops);
        self.end(true, false);
    }
}

/// Glyph outlines as PDF path operators. PDF has no quadratic curves, so
/// they become the equivalent cubics.
struct PdfPath {
    ops: String,
    at: (f64, f64),
}

impl PathSink for PdfPath {
    fn move_to(&mut self, x: f64, y: f64) {
        let _ = writeln!(self.ops, "{} {} m", numf(x), numf(y));
        self.at = (x, y);
    }

    fn line_to(&mut self, x: f64, y: f64) {
        let _ = writeln!(self.ops, "{} {} l", numf(x), numf(y));
        self.at = (x, y);
    }

    fn quad_to(&mut self, cx: f64, cy: f64, x: f64, y: f64) {
        let (x0, y0) = self.at;
        let c1 = (x0 + 2.0 / 3.0 * (cx - x0), y0 + 2.0 / 3.0 * (cy - y0));
        let c2 = (x + 2.0 / 3.0 * (cx - x), y + 2.0 / 3.0 * (cy - y));
        self.cubic_to(c1.0, c1.1, c2.0, c2.1, x, y);
    }

    fn cubic_to(&mut self, c1x: f64, c1y: f64, c2x: f64, c2y: f64, x: f64, y: f64) {
        let _ = writeln!(
            self.ops,
            "{} {} {} {} {} {} c",
            numf(c1x),
            numf(c1y),
            numf(c2x),
            numf(c2y),
            numf(x),
            numf(y)
        );
        self.at = (x, y);
    }

    fn close(&mut self) {
        self.ops.push_str("h\n");
    }
}

/// A number for PDF: at most 3 decimals, never an exponent.
fn numf(v: f64) -> String {
    let s = format!("{v:.3}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" {
        "0".to_owned()
    } else {
        s.to_owned()
    }
}

fn num(v: f32) -> String {
    numf(f64::from(v))
}

fn rgb(c: Rgba) -> String {
    let [r, g, b, _] = c.0;
    let f = |v: u8| numf(f64::from(v) / 255.0);
    format!("{} {} {}", f(r), f(g), f(b))
}

fn alpha(a: u8) -> String {
    numf(f64::from(a) / 255.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Participant;

    /// The PDF as text, byte for byte: the binary comment's bytes become `?`,
    /// so offsets stay right.
    fn text(pdf: &[u8]) -> String {
        pdf.iter()
            .map(|&b| if b.is_ascii() { b as char } else { '?' })
            .collect()
    }

    /// Checks the cross-reference table: every entry points at its object.
    fn check_xref(pdf: &[u8]) {
        let t = text(pdf);
        let start: usize = t
            .rsplit("startxref\n")
            .next()
            .unwrap()
            .lines()
            .next()
            .unwrap()
            .parse()
            .unwrap();
        assert!(t[start..].starts_with("xref\n"));
        let lines: Vec<&str> = t[start..].lines().collect();
        let count: usize = lines[1].split(' ').nth(1).unwrap().parse().unwrap();
        for (n, line) in lines[3..2 + count].iter().enumerate() {
            assert_eq!(line.len(), 19, "entries are 20 bytes with the newline");
            let offset: usize = line[..10].parse().unwrap();
            assert!(
                t[offset..].starts_with(&format!("{} 0 obj", n + 1)),
                "object {}",
                n + 1
            );
        }
    }

    fn model() -> Model {
        let mut m = Model::new();
        let a = m.add_species(pt(100.0, 100.0), 80.0, 36.0);
        let b = m.add_species(pt(300.0, 100.0), 80.0, 36.0);
        let r = m.add_reaction(pt(200.0, 100.0));
        let r = m.find_reaction_mut(&r).unwrap();
        r.reactants.push(Participant::new(a, 1.0));
        r.products.push(Participant::new(b, 1.0));
        m
    }

    #[test]
    fn a_valid_document() {
        let pdf = model_to_pdf(&model(), PageSize::Letter, false, Shown::ALL);
        assert!(pdf.starts_with(b"%PDF-1.4\n"));
        assert!(pdf.ends_with(b"%%EOF\n"));
        check_xref(&pdf);
        let t = text(&pdf);
        assert!(t.contains("/MediaBox [0 0 612 792]"));
        // The stream's length is right.
        let len: usize = t
            .split("/Length ")
            .nth(1)
            .unwrap()
            .split(' ')
            .next()
            .unwrap()
            .parse()
            .unwrap();
        let start = t.find(">>\nstream\n").unwrap() + ">>\nstream\n".len();
        let body = &t[start..];
        assert_eq!(&body[len..len + 9], "endstream");
    }

    #[test]
    fn pages_and_fitting() {
        let t = text(&model_to_pdf(&model(), PageSize::A4, true, Shown::ALL));
        assert!(t.contains("/MediaBox [0 0 841.89 595.276]"));
        // The drawing (400 × 156 with its margin) fills the width inside the
        // page margin, and is centred vertically: the background's corner.
        let k = (841.89 - 72.0) / 400.0;
        let y = (595.276 - 156.0 * k) / 2.0;
        assert!(t.contains(&format!(
            "{} {} {} {} re",
            numf(36.0),
            numf(y),
            numf(400.0 * k),
            numf(156.0 * k)
        )));
        // Drawing size: 400 × 156 world units at 96 to the inch.
        let t = text(&model_to_pdf(
            &model(),
            PageSize::Drawing,
            false,
            Shown::ALL,
        ));
        assert!(t.contains("/MediaBox [0 0 300 117]"));
    }

    #[test]
    fn drawing_operators() {
        let mut p = PdfRenderer::new(100.0, 50.0);
        p.round_rect(
            SRect {
                min: spt(1.0, 2.0),
                max: spt(81.0, 38.0),
            },
            8.0,
            Some(Rgba::rgb(255, 0, 0)),
            Some(Stroke {
                width: 1.5,
                color: Rgba([0, 0, 255, 128]),
                dash: Some((5.0, 4.0)),
                round_cap: true,
            }),
        );
        p.text(spt(50.0, 25.0), "S1", 12.0, Rgba::rgb(0x1A, 0x1A, 0x1A));
        let pdf = p.finish();
        check_xref(&pdf);
        let t = text(&pdf);
        assert!(t.contains("1 0 0 -1 0 50 cm"), "y flipped to point down");
        assert!(t.contains("1 0 0 rg\n0 0 1 RG 1.5 w 4 M\n[5 4] 0 d\n1 J 1 j\n"));
        assert!(t.contains("/GS0 gs"));
        assert!(t.contains("<< /Type /ExtGState /ca 1 /CA 0.502 >>"));
        assert!(t.contains("B\nQ\n"));
        // The label is outlines, filled: no font, no text operators.
        assert!(!t.contains("BT") && !t.contains("/Font"));
        assert!(t.contains(" c\n") && t.contains("0.102 0.102 0.102 rg"));
    }

    #[test]
    fn nothing_that_depends_on_the_selection() {
        let mut m = model();
        let plain = model_to_pdf(&m, PageSize::Letter, false, Shown::ALL);
        m.find_species_mut("S1").unwrap().selected = true;
        m.find_reaction_mut("R3").unwrap().selected = true;
        assert_eq!(model_to_pdf(&m, PageSize::Letter, false, Shown::ALL), plain);
    }
}
