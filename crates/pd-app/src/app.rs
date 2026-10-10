//! The R0 test page: one species node whose label is drawn with the bundled font.
//!
//! It proves the font pipeline before anything depends on it. The node's width
//! comes from `pd_core::text` (label width + 2×10 padding, S §5.3), and a pale box
//! behind the label shows exactly the width pd-core measured. If egui drew the
//! text differently, the label would visibly over- or under-fill that box.

use eframe::egui::{self, Color32, CornerRadius, Pos2, Rect, Sense, Stroke, StrokeKind, Vec2};
use pd_core::geometry::{Pt, pt, world_to_screen};
use pd_core::text::{FONT_NAME, font_metrics, text_width};

use crate::fonts;

// S §17.1 and §17.2. These move to pd-core's render constants in R2.
const BACKGROUND: Color32 = Color32::from_rgb(0xF8, 0xF9, 0xFA);
const SPECIES_FILL: Color32 = Color32::from_rgb(0xEE, 0xF6, 0xFF);
const SPECIES_BORDER: Color32 = Color32::from_rgb(0x4A, 0x7F, 0xCB);
const LABEL: Color32 = Color32::from_rgb(0x1A, 0x1A, 0x1A);
const MEASURED_BOX: Color32 = Color32::from_rgb(0xFF, 0xE0, 0xB2);
const LABEL_SIZE: f64 = 12.0;
const LABEL_PADDING: f64 = 10.0;
const NODE_HEIGHT: f64 = 36.0;
const MIN_NODE_WIDTH: f64 = 80.0;
const CORNER_RADIUS: f64 = 8.0;
const BORDER_WIDTH: f64 = 1.5;

pub struct PathwayApp {
    label: String,
    zoom: f64,
    show_measured_box: bool,
}

impl PathwayApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        fonts::install(&cc.egui_ctx);
        cc.egui_ctx.set_visuals(egui::Visuals::light());
        Self {
            label: "Glucose_6_phosphate".to_owned(),
            zoom: 2.0,
            show_measured_box: true,
        }
    }

    fn controls(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_wrapped(|ui| {
            ui.strong("PathwayDesigner");
            ui.label(format!("R0 test page · v{}", env!("CARGO_PKG_VERSION")));
            ui.separator();
            ui.label("Label");
            ui.add(egui::TextEdit::singleline(&mut self.label).desired_width(180.0));
            ui.label("Zoom");
            ui.add(egui::Slider::new(&mut self.zoom, 0.1..=8.0).logarithmic(true));
            ui.checkbox(&mut self.show_measured_box, "Show measured width");
        });
        let measured = text_width(&self.label, LABEL_SIZE);
        let drawn = ui
            .painter()
            .layout_no_wrap(
                self.label.clone(),
                fonts::diagram_font(LABEL_SIZE as f32),
                LABEL,
            )
            .size()
            .x;
        ui.label(format!(
            "{FONT_NAME} at {LABEL_SIZE} — pd-core measures {measured:.2}, egui lays out {drawn:.2}"
        ));
    }

    fn canvas(&self, ui: &mut egui::Ui) {
        let (response, painter) = ui.allocate_painter(ui.available_size(), Sense::hover());
        let canvas = response.rect;
        painter.rect_filled(canvas, 0.0, BACKGROUND);

        // One node at world (0, 0), with the world origin at the canvas centre.
        let zoom = self.zoom;
        let offset = pt(f64::from(canvas.center().x), f64::from(canvas.center().y));
        let label_width = text_width(&self.label, LABEL_SIZE);
        let node_size = pt(
            (label_width + 2.0 * LABEL_PADDING).max(MIN_NODE_WIDTH),
            NODE_HEIGHT,
        );
        let centre = world_to_screen(Pt::ZERO, zoom, offset);

        let node = Rect::from_center_size(to_pos(centre), to_vec(node_size * zoom));
        painter.rect(
            node,
            CornerRadius::same((CORNER_RADIUS * zoom).round().clamp(0.0, 255.0) as u8),
            SPECIES_FILL,
            Stroke::new((BORDER_WIDTH * zoom) as f32, SPECIES_BORDER),
            StrokeKind::Middle,
        );

        // The label, centred horizontally and vertically on its cap height (S §5.3).
        let size = LABEL_SIZE * zoom;
        let metrics = font_metrics(size);
        let baseline = centre.y + metrics.cap_height / 2.0;
        let width = label_width * zoom;

        if self.show_measured_box {
            let measured = Rect::from_min_max(
                to_pos(pt(centre.x - width / 2.0, baseline - metrics.cap_height)),
                to_pos(pt(centre.x + width / 2.0, baseline)),
            );
            painter.rect_filled(measured, 0.0, MEASURED_BOX);
        }

        let galley =
            painter.layout_no_wrap(self.label.clone(), fonts::diagram_font(size as f32), LABEL);
        let baseline_in_galley = galley
            .rows
            .first()
            .and_then(|row| row.row.glyphs.first().map(|g| row.pos.y + g.pos.y))
            .unwrap_or(0.0);
        let top_left = Pos2::new(
            (centre.x - width / 2.0) as f32,
            baseline as f32 - baseline_in_galley,
        );
        painter.galley(top_left, galley, LABEL);
    }
}

impl eframe::App for PathwayApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::Panel::top("controls").show(ui, |ui| {
            ui.add_space(4.0);
            self.controls(ui);
            ui.add_space(4.0);
        });
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ui, |ui| self.canvas(ui));
    }
}

/// Model `f64` to screen `f32`, at the point of drawing (SPEC-Rust §5.1).
fn to_pos(p: Pt) -> Pos2 {
    Pos2::new(p.x as f32, p.y as f32)
}

fn to_vec(p: Pt) -> Vec2 {
    Vec2::new(p.x as f32, p.y as f32)
}
