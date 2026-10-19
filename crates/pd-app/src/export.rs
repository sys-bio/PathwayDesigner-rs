//! Graphics output (S §11.6, SPEC-Rust §9): SVG, PNG and PDF files, and
//! printing. The drawing itself comes from `pd-export`; this module holds the
//! options dialog and hands the result to the files layer.

use eframe::egui;
use pd_core::model::Model;
use pd_core::render::draw::content_bounds;
use pd_export::PageSize;

use crate::app::{Dialog, PathwayApp};
use crate::document::with_extension;
use crate::files::Purpose;

/// Which output the options dialog is for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExportKind {
    Png,
    Pdf,
    Print,
}

/// The options last used, kept for the rest of the session.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ExportSettings {
    pub dpi: f64,
    pub page: PageSize,
    pub landscape: bool,
}

impl Default for ExportSettings {
    fn default() -> Self {
        Self {
            dpi: pd_export::DEFAULT_DPI,
            page: PageSize::Letter,
            landscape: false,
        }
    }
}

/// The resolutions offered for PNG.
const DPI_RANGE: std::ops::RangeInclusive<f64> = 36.0..=600.0;

impl PathwayApp {
    /// Saves the drawing as SVG, at its size on screen at 100%.
    pub(crate) fn export_svg(&mut self) {
        let svg = pd_export::model_to_svg(self.view.model(), 1.0, self.view.shown());
        let name = with_extension(&self.doc.name, "svg");
        self.files.save(
            &self.ctx,
            Purpose::ExportSvg,
            name,
            svg.into_bytes(),
            None,
            true,
        );
    }

    /// Opens the options dialog for PNG, PDF or printing.
    pub(crate) fn ask_export(&mut self, kind: ExportKind) {
        self.dialog = Some(Dialog::Export(kind, self.export_settings));
    }

    /// Writes the output the dialog was for, with its options.
    pub(crate) fn export(&mut self, kind: ExportKind, s: ExportSettings) {
        self.export_settings = s;
        let shown = self.view.shown();
        let model = self.view.model();
        match kind {
            ExportKind::Png => match pd_export::model_to_png(model, s.dpi, shown) {
                Ok(png) => {
                    let name = with_extension(&self.doc.name, "png");
                    self.files
                        .save(&self.ctx, Purpose::ExportPng, name, png, None, true);
                }
                Err(e) => self.warnings = vec![format!("The PNG couldn't be made: {e}.")],
            },
            ExportKind::Pdf => {
                let pdf = pd_export::model_to_pdf(model, s.page, s.landscape, shown);
                let name = with_extension(&self.doc.name, "pdf");
                self.files
                    .save(&self.ctx, Purpose::ExportPdf, name, pdf, None, true);
            }
            ExportKind::Print => {
                let pdf = pd_export::model_to_pdf(model, s.page, s.landscape, shown);
                self.print(pdf);
            }
        }
    }

    /// Opens the PDF where it can be printed (SPEC-Rust §9): a new browser
    /// tab, whose PDF viewer prints, or the system's PDF viewer.
    fn print(&mut self, pdf: Vec<u8>) {
        #[cfg(target_arch = "wasm32")]
        let result = crate::web::open_pdf(&pdf).map(|()| "a new tab");
        #[cfg(not(target_arch = "wasm32"))]
        let result = open_in_viewer(&self.doc.name, &pdf).map(|()| "your PDF viewer");
        match result {
            Ok(place) => {
                self.notice = Some(format!(
                    "Opened the drawing in {place}: print it from there"
                ));
                self.warnings.clear();
            }
            Err(e) => self.warnings = vec![e],
        }
    }
}

/// Writes the PDF to a temporary file and opens it with the system viewer.
#[cfg(not(target_arch = "wasm32"))]
fn open_in_viewer(doc_name: &str, pdf: &[u8]) -> Result<(), String> {
    let name = format!(
        "PathwayDesigner print - {}",
        with_extension(doc_name, "pdf")
    );
    let path = std::env::temp_dir().join(name);
    std::fs::write(&path, pdf)
        .map_err(|e| format!("The PDF for printing couldn't be written: {e}."))?;
    open::that_detached(&path).map_err(|e| format!("No program opened the PDF for printing: {e}."))
}

/// The options dialog. Returns whether it closed, and whether to go ahead.
pub(crate) fn dialog(
    ctx: &egui::Context,
    kind: ExportKind,
    s: &mut ExportSettings,
    model: &Model,
) -> (bool, bool) {
    let (mut go, mut cancel) = (false, false);
    let (title, button) = match kind {
        ExportKind::Png => ("Export PNG", "Export"),
        ExportKind::Pdf => ("Export PDF", "Export"),
        ExportKind::Print => ("Print", "Print"),
    };
    let modal = egui::Modal::new(egui::Id::unique("export")).show(ctx, |ui| {
        ui.heading(title);
        ui.add_space(4.0);
        if kind == ExportKind::Png {
            ui.horizontal(|ui| {
                ui.label("Resolution");
                ui.add(
                    egui::DragValue::new(&mut s.dpi)
                        .range(DPI_RANGE)
                        .speed(1.0)
                        .suffix(" DPI"),
                );
            });
            let (min, max) = content_bounds(model);
            let k = s.dpi / 96.0;
            ui.label(
                egui::RichText::new(format!(
                    "{} × {} pixels",
                    ((max.x - min.x) * k).ceil(),
                    ((max.y - min.y) * k).ceil()
                ))
                .small(),
            );
        } else {
            ui.horizontal(|ui| {
                ui.label("Page");
                egui::ComboBox::from_id_salt("page")
                    .selected_text(s.page.label())
                    .show_ui(ui, |ui| {
                        for page in PageSize::ALL {
                            ui.selectable_value(&mut s.page, page, page.label());
                        }
                    });
            });
            ui.add_enabled_ui(s.page != PageSize::Drawing, |ui| {
                ui.horizontal(|ui| {
                    ui.radio_value(&mut s.landscape, false, "Portrait");
                    ui.radio_value(&mut s.landscape, true, "Landscape");
                });
            });
            let note = match (kind, s.page) {
                (_, PageSize::Drawing) => "The page is the size of the drawing.",
                (ExportKind::Print, _) => {
                    "The drawing opens as a PDF, scaled to fit the page; print it from there."
                }
                _ => "The drawing is scaled to fit the page and centred.",
            };
            ui.label(egui::RichText::new(note).small());
        }
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            go = ui.button(button).clicked();
            cancel = ui.button("Cancel").clicked();
        });
    });
    (go || cancel || modal.should_close(), go)
}
