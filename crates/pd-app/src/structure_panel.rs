//! The Structure tab and the matrix panel (docs/structural-analysis-design.md
//! §6): the stoichiometry matrix, its flux null space and conservation laws,
//! from `pd_core::structure`. The tab holds the summary and the lists; the
//! matrix, which is wide, is a panel under the canvas.

use eframe::egui;
use pd_core::model::{Id, Model};
use pd_core::structure::{self, ConservationLaw, FluxVector, Matrix, Structure, WORK_LIMIT};
use web_time::Instant;

/// What the user picked in the lists: a null-space vector or a conservation
/// law, by its place in the list shown. Let go when the matrix or the basis
/// changes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Picked {
    Flux(usize),
    Law(usize),
}

/// A file the tab asks the app to save.
pub struct Export {
    pub name: String,
    pub csv: String,
}

/// A basis of the flux null space and the conservation laws.
#[derive(Clone)]
pub struct Bases {
    pub fluxes: Vec<FluxVector>,
    pub laws: Vec<ConservationLaw>,
}

/// The analysis of the model's current stoichiometry matrix, or why there is
/// none.
enum Analysis {
    Done {
        structure: Box<Structure>,
        millis: f64,
        /// From the factorization: the K matrix and the link matrix.
        k: Bases,
        /// The sparse bases, made when first asked for, and how long that
        /// took.
        sparse: Option<(Bases, f64)>,
    },
    TooLarge {
        rows: usize,
        cols: usize,
    },
}

/// A stoichiometry matrix with its species and reactions.
type Key = (Vec<Id>, Vec<Id>, Matrix);

#[derive(Default)]
pub struct StructurePanel {
    /// The matrix the analysis is of (species, reactions, entries), and the
    /// analysis: factorized again only when the matrix changes.
    cache: Option<(Key, Analysis)>,
    /// The matrix panel under the canvas is open.
    pub show_matrix: bool,
    /// The sparse bases are shown, not those of the factorization.
    pub sparse: bool,
    pub picked: Option<Picked>,
}

impl StructurePanel {
    /// Brings the analysis up to date with `model`: the matrix is rebuilt
    /// (cheap), and factorized only if it changed and isn't too large; the
    /// sparse bases are made if they are shown and not made yet.
    pub fn refresh(&mut self, model: &Model) {
        let key = structure::stoichiometry(model);
        if !self.cache.as_ref().is_some_and(|(k, _)| *k == key) {
            let (rows, cols) = (key.2.rows, key.2.cols);
            let analysis = if structure::work(rows, cols) > WORK_LIMIT {
                Analysis::TooLarge { rows, cols }
            } else {
                let start = Instant::now();
                let structure = Structure::from_matrix(key.0.clone(), key.1.clone(), key.2.clone());
                let k = Bases {
                    fluxes: structure.null_space(),
                    laws: structure.conservation_laws(),
                };
                let millis = start.elapsed().as_secs_f64() * 1000.0;
                log::info!("structure: {rows} x {cols} in {millis:.1} ms");
                Analysis::Done {
                    structure: Box::new(structure),
                    millis,
                    k,
                    sparse: None,
                }
            };
            self.cache = Some((key, analysis));
            self.picked = None;
        }
        if self.sparse
            && let Some((
                _,
                Analysis::Done {
                    structure, sparse, ..
                },
            )) = &mut self.cache
            && sparse.is_none()
        {
            let start = Instant::now();
            let bases = Bases {
                fluxes: structure.sparse_null_space(),
                laws: structure.sparse_conservation_laws(),
            };
            let millis = start.elapsed().as_secs_f64() * 1000.0;
            log::info!("structure: sparse bases in {millis:.1} ms");
            *sparse = Some((bases, millis));
        }
    }

    /// The current analysis, if there is one.
    pub fn structure(&self) -> Option<&Structure> {
        match &self.cache {
            Some((_, Analysis::Done { structure, .. })) => Some(structure),
            _ => None,
        }
    }

    /// The bases shown: the sparse ones when chosen (and made), else the
    /// factorization's.
    pub fn bases(&self) -> Option<&Bases> {
        match &self.cache {
            Some((_, Analysis::Done { k, sparse, .. })) => Some(match sparse {
                Some((b, _)) if self.sparse => b,
                _ => k,
            }),
            _ => None,
        }
    }

    /// The Structure tab. Returns a file to save, if Export… was clicked.
    pub fn show(&mut self, ui: &mut egui::Ui, model_name: &str) -> Option<Export> {
        let mut export = None;
        let Some((_, analysis)) = &self.cache else {
            return None;
        };
        let (s, millis, sparse_millis) = match analysis {
            Analysis::TooLarge { rows, cols } => {
                ui.label(format!(
                    "The network is too large to analyse here: {rows} species by {cols} \
                     reactions. Networks up to about 600 by 600 can be analysed."
                ));
                return None;
            }
            Analysis::Done {
                structure,
                millis,
                sparse,
                ..
            } => (structure, *millis, sparse.as_ref().map(|(_, m)| *m)),
        };
        if s.reactions.is_empty() {
            ui.label("The model has no reactions.");
            return None;
        }
        egui::Grid::new("structure-summary")
            .num_columns(2)
            .show(ui, |ui| {
                for (label, value) in [
                    ("Floating species", s.species.len()),
                    ("Reactions", s.reactions.len()),
                    ("Rank", s.rank),
                    ("Independent fluxes", s.independent_reactions.len()),
                    ("Conservation laws", s.dependent_species.len()),
                ] {
                    ui.label(label);
                    ui.label(value.to_string());
                    ui.end_row();
                }
            });
        let timing = match (self.sparse, sparse_millis) {
            (true, Some(sm)) => format!("Computed in {millis:.1} ms, sparse bases {sm:.1} ms"),
            _ => format!("Computed in {millis:.1} ms"),
        };
        ui.label(egui::RichText::new(timing).weak().small());
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.toggle_value(&mut self.show_matrix, "Stoichiometry matrix")
                .on_hover_text("Show the matrix in a panel under the diagram");
            if ui
                .button("Copy")
                .on_hover_text("Copy the matrix, to paste into a spreadsheet")
                .clicked()
            {
                ui.ctx().copy_text(s.n_tsv());
            }
            if ui.button("Export…").clicked() {
                export = Some(Export {
                    name: format!("{model_name} stoichiometry.csv"),
                    csv: s.n_csv(),
                });
            }
        });
        // Which basis: the factorization's (as COPASI and libRoadRunner
        // report them) or the sparse one (each vector as few reactions or
        // species as the search finds).
        ui.horizontal(|ui| {
            ui.label("Basis:");
            let before = self.sparse;
            ui.selectable_value(&mut self.sparse, false, "K matrix")
                .on_hover_text(
                    "One vector per independent reaction, as COPASI and libRoadRunner give them",
                );
            ui.selectable_value(&mut self.sparse, true, "Sparse")
                .on_hover_text("The same space, each vector with as few reactions as can be found");
            if self.sparse != before {
                self.picked = None;
            }
        });
        ui.separator();
        // The sparse bases are made by the next refresh; until then, the
        // factorization's are shown.
        let Some(bases) = self.bases() else {
            return export;
        };
        let bases = bases.clone();
        let tag = if self.sparse && sparse_millis.is_some() {
            " sparse"
        } else {
            ""
        };

        // The flux null space, one row per basis vector.
        ui.horizontal(|ui| {
            ui.strong(format!("Flux null space ({})", bases.fluxes.len()));
            if !bases.fluxes.is_empty() {
                if ui.small_button("Copy").clicked() {
                    ui.ctx()
                        .copy_text(lines(bases.fluxes.iter().map(|v| s.flux_text(v))));
                }
                if ui.small_button("Export…").clicked() {
                    export = Some(Export {
                        name: format!("{model_name}{tag} null space.csv"),
                        csv: s.null_space_csv(&bases.fluxes),
                    });
                }
            }
        });
        ui.label(
            egui::RichText::new(
                "Each set of reactions can change together, in these proportions, \
                 with every species staying balanced.",
            )
            .weak()
            .small(),
        );
        if bases.fluxes.is_empty() {
            ui.label("None: no flux can change without unbalancing a species.");
        }
        for (i, v) in bases.fluxes.iter().enumerate() {
            let picked = self.picked == Some(Picked::Flux(i));
            if ui
                .add(egui::Button::selectable(picked, s.flux_text(v)).wrap())
                .clicked()
            {
                self.picked = (!picked).then_some(Picked::Flux(i));
            }
        }
        ui.separator();

        // The conservation laws.
        ui.horizontal(|ui| {
            ui.strong(format!("Conservation laws ({})", bases.laws.len()));
            if !bases.laws.is_empty() {
                if ui.small_button("Copy").clicked() {
                    ui.ctx()
                        .copy_text(lines(bases.laws.iter().map(|l| s.law_text(l))));
                }
                if ui.small_button("Export…").clicked() {
                    export = Some(Export {
                        name: format!("{model_name}{tag} conservation laws.csv"),
                        csv: s.laws_csv(&bases.laws),
                    });
                }
            }
        });
        if bases.laws.is_empty() {
            ui.label("None.");
        }
        for (i, law) in bases.laws.iter().enumerate() {
            let picked = self.picked == Some(Picked::Law(i));
            if ui
                .add(egui::Button::selectable(picked, s.law_text(law)).wrap())
                .clicked()
            {
                self.picked = (!picked).then_some(Picked::Law(i));
            }
        }
        export
    }

    /// The matrix panel: the stoichiometry matrix, its species and reaction
    /// names pinned to the left and top edges as it scrolls. Only the cells in
    /// view are drawn, so a large matrix scrolls smoothly. A picked vector's
    /// reactions and a picked law's species are shaded.
    pub fn matrix(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.strong("Stoichiometry matrix");
            ui.label(egui::RichText::new("species by reactions; blank is 0").weak());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .small_button("✕")
                    .on_hover_text("Close the matrix")
                    .clicked()
                {
                    self.show_matrix = false;
                }
            });
        });
        let (Some(s), Some(bases)) = (self.structure(), self.bases()) else {
            ui.label("Not available for this network (see the Structure tab).");
            return;
        };
        let (shaded_rows, shaded_cols): (Vec<usize>, Vec<usize>) = match self.picked {
            Some(Picked::Flux(i)) => (
                Vec::new(),
                bases
                    .fluxes
                    .get(i)
                    .map(|v| v.terms.iter().map(|t| t.0).collect())
                    .unwrap_or_default(),
            ),
            Some(Picked::Law(i)) => (
                bases
                    .laws
                    .get(i)
                    .map(|l| l.terms.iter().map(|t| t.0).collect())
                    .unwrap_or_default(),
                Vec::new(),
            ),
            None => (Vec::new(), Vec::new()),
        };
        grid(ui, s, &shaded_rows, &shaded_cols);
    }
}

fn lines(items: impl Iterator<Item = String>) -> String {
    items.map(|t| t + "\n").collect()
}

/// A cell's text: blank for 0, a simple fraction as one (`1/2`), else four
/// significant figures.
fn cell(v: f64) -> String {
    match structure::as_fraction(v) {
        Some((0, _)) => String::new(),
        Some((n, 1)) => n.to_string(),
        Some((n, d)) => format!("{n}/{d}"),
        None => format!("{v:.4}"),
    }
}

/// The matrix as a grid in a scroll area, drawn by hand: only the visible
/// cells, with the header row and the name column painted over the edges of
/// the view so they stay in place.
fn grid(ui: &mut egui::Ui, s: &Structure, shaded_rows: &[usize], shaded_cols: &[usize]) {
    let font = egui::TextStyle::Monospace.resolve(ui.style());
    let char_w = ui.fonts_mut(|f| f.glyph_width(&font, '0'));
    let row_h = ui.text_style_height(&egui::TextStyle::Monospace) + 4.0;
    let longest = |names: &[Id]| names.iter().map(|n| n.chars().count()).max().unwrap_or(1);
    let name_w = (longest(&s.species).max(4) as f32 + 2.0) * char_w;
    let (rows, cols) = (s.n.rows, s.n.cols);
    // Each column as wide as its name or its widest entry; `left[c]` is
    // where column c starts, the name column before them all.
    let widths: Vec<f32> = (0..cols)
        .map(|c| {
            let entries = (0..rows).map(|r| cell(s.n.get(r, c)).chars().count());
            let chars = entries
                .max()
                .unwrap_or(0)
                .max(s.reactions[c].chars().count())
                .max(3);
            (chars as f32 + 2.0) * char_w
        })
        .collect();
    let mut left = vec![name_w];
    for w in &widths {
        left.push(left[left.len() - 1] + w);
    }
    // The columns that meet [from, to).
    let columns_in = |from: f32, to: f32| {
        let first = left[..cols]
            .partition_point(|&x| x <= from)
            .saturating_sub(1);
        let last = left[..cols].partition_point(|&x| x < to);
        first..last
    };
    let visuals = ui.visuals().clone();
    let shade = visuals.selection.bg_fill.gamma_multiply(0.35);
    egui::ScrollArea::both()
        .auto_shrink(false)
        .show_viewport(ui, |ui, view| {
            let size = egui::vec2(left[cols], row_h * (rows as f32 + 1.0));
            let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
            let painter = ui.painter_at(ui.clip_rect());
            let origin = rect.min;
            let in_view = columns_in(view.min.x, view.max.x);
            let first_row = ((view.min.y - row_h) / row_h).floor().max(0.0) as usize;
            let last_row = (((view.max.y - row_h) / row_h).ceil().max(0.0) as usize).min(rows);
            let text = |p: egui::Pos2, t: &str, align: egui::Align2, strong: bool| {
                let color = if strong {
                    visuals.strong_text_color()
                } else {
                    visuals.text_color()
                };
                painter.text(p, align, t, font.clone(), color);
            };
            // Cells.
            for r in first_row..last_row {
                let y = origin.y + row_h * (r as f32 + 1.0);
                for c in in_view.clone() {
                    let x = origin.x + left[c];
                    let cell_rect =
                        egui::Rect::from_min_size(egui::pos2(x, y), egui::vec2(widths[c], row_h));
                    if shaded_rows.contains(&r) || shaded_cols.contains(&c) {
                        painter.rect_filled(cell_rect, 0.0, shade);
                    }
                    let t = cell(s.n.get(r, c));
                    if !t.is_empty() {
                        text(
                            cell_rect.right_center() - egui::vec2(char_w, 0.0),
                            &t,
                            egui::Align2::RIGHT_CENTER,
                            false,
                        );
                    }
                }
            }
            // The header row, pinned to the top of the view.
            let top = origin.y + view.min.y;
            let band = egui::Rect::from_min_size(
                egui::pos2(origin.x + view.min.x, top),
                egui::vec2(view.width(), row_h),
            );
            painter.rect_filled(band, 0.0, visuals.panel_fill);
            for c in in_view {
                let x = origin.x + left[c];
                let r = egui::Rect::from_min_size(egui::pos2(x, top), egui::vec2(widths[c], row_h));
                if shaded_cols.contains(&c) {
                    painter.rect_filled(r, 0.0, shade);
                }
                text(
                    r.right_center() - egui::vec2(char_w, 0.0),
                    &s.reactions[c],
                    egui::Align2::RIGHT_CENTER,
                    true,
                );
            }
            // The name column, pinned to the left of the view.
            let left = origin.x + view.min.x;
            let column = egui::Rect::from_min_size(
                egui::pos2(left, origin.y + view.min.y),
                egui::vec2(name_w, view.height()),
            );
            painter.rect_filled(column, 0.0, visuals.panel_fill);
            for r in first_row..last_row {
                let y = origin.y + row_h * (r as f32 + 1.0);
                if y < top + row_h - 0.5 {
                    continue;
                }
                let rr = egui::Rect::from_min_size(egui::pos2(left, y), egui::vec2(name_w, row_h));
                if shaded_rows.contains(&r) {
                    painter.rect_filled(rr, 0.0, shade);
                }
                text(
                    rr.left_center() + egui::vec2(char_w, 0.0),
                    &s.species[r],
                    egui::Align2::LEFT_CENTER,
                    true,
                );
            }
            // The corner over both.
            painter.rect_filled(
                egui::Rect::from_min_size(egui::pos2(left, top), egui::vec2(name_w, row_h)),
                0.0,
                visuals.panel_fill,
            );
            painter.hline(
                left..=left + view.width(),
                top + row_h,
                visuals.widgets.noninteractive.bg_stroke,
            );
            painter.vline(
                left + name_w,
                top..=top + view.height(),
                visuals.widgets.noninteractive.bg_stroke,
            );
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cells_show_fractions_and_hide_zeros() {
        assert_eq!(cell(0.0), "");
        assert_eq!(cell(-1.0), "-1");
        assert_eq!(cell(0.5), "1/2");
        assert_eq!(cell(0.123456), "0.1235");
    }
}
