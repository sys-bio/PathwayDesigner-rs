//! The Styles tab of the right panel (S §8.6, §13.3): the saved styles of
//! the model and of My Styles, by kind, each with a small picture. A click
//! gives a style to the selected items; a right click offers the rest.

use eframe::egui::{self, Color32, Sense, Vec2};
use pd_core::geometry::pt;
use pd_core::model::StyleKind;
use pd_core::render::constants::CONTENT_MARGIN;
use pd_core::render::draw::{DrawOptions, content_bounds, draw_model};
use pd_core::styles::StyleEntry;
use pd_core::view::{DiagramView, Target};

use crate::inspector::Inspector;
use crate::renderer::EguiRenderer;

/// The size of a style's picture.
const PREVIEW: Vec2 = Vec2::new(72.0, 34.0);
const MUTED: Color32 = Color32::from_rgb(0x70, 0x70, 0x70);

/// What the app must do for the tab.
#[derive(Clone, Debug, PartialEq)]
pub enum StylesAction {
    /// Ask for a new name for the style.
    Rename(StyleKind, String),
    /// Say something in the status bar.
    Notice(String),
    /// Open the Edit Style window for the style.
    Edit(StyleKind, String),
    /// Export My Styles… to a file.
    Export,
    /// Import Styles… from a file.
    Import,
}

/// Each kind's heading.
fn plural(kind: StyleKind) -> &'static str {
    match kind {
        StyleKind::Species => "Species",
        StyleKind::Reaction => "Reactions",
        StyleKind::Compartment => "Compartments",
        StyleKind::Text => "Texts",
    }
}

fn singular(kind: StyleKind) -> &'static str {
    match kind {
        StyleKind::Species => "species",
        StyleKind::Reaction => "reaction",
        StyleKind::Compartment => "compartment",
        StyleKind::Text => "text",
    }
}

/// The one selected item of `kind`, as a target, if exactly one is.
fn one_selected(view: &DiagramView, kind: StyleKind) -> Option<Target> {
    let ids = pd_core::styles::selected_ids(view.model(), kind);
    let [id] = ids.as_slice() else {
        return None;
    };
    let id = id.clone();
    Some(match kind {
        StyleKind::Species => Target::Species(id),
        StyleKind::Reaction => Target::Leg(id),
        StyleKind::Compartment => Target::Compartment(id),
        StyleKind::Text => Target::Text(id),
    })
}

/// Shows the tab. Returns what the app must do, if anything.
pub fn show(ui: &mut egui::Ui, view: &mut DiagramView) -> Option<StylesAction> {
    let mut action = None;
    ui.horizontal(|ui| {
        if ui
            .add_enabled(
                !view.my_styles().is_empty(),
                egui::Button::new("Export My Styles…"),
            )
            .on_hover_text("Save My Styles to a file, to share them or use them elsewhere")
            .on_disabled_hover_text("My Styles is empty")
            .clicked()
        {
            action = Some(StylesAction::Export);
        }
        if ui
            .button("Import Styles…")
            .on_hover_text(
                "Add the styles in a styles file, or a diagram's, to My Styles; \
                 none of yours are replaced",
            )
            .clicked()
        {
            action = Some(StylesAction::Import);
        }
    });
    ui.add_space(4.0);
    let all: Vec<(StyleKind, Vec<StyleEntry>)> = StyleKind::ALL
        .into_iter()
        .map(|k| (k, view.style_entries(k)))
        .collect();
    if all.iter().all(|(_, e)| e.is_empty()) {
        ui.label(
            "No saved styles yet. Right-click something on the diagram and choose \
             Save Style… to keep its look under a name.",
        );
        return action;
    }
    ui.label(
        egui::RichText::new(
            "Click a style to give it to the selected items. Right-click for more.",
        )
        .color(MUTED),
    );
    for (kind, entries) in all {
        if entries.is_empty() {
            continue;
        }
        ui.add_space(6.0);
        ui.strong(plural(kind));
        for e in entries {
            if let Some(a) = row(ui, view, kind, &e) {
                action = Some(a);
            }
        }
    }
    action
}

/// One style: its picture, its name and where it is kept.
fn row(
    ui: &mut egui::Ui,
    view: &mut DiagramView,
    kind: StyleKind,
    e: &StyleEntry,
) -> Option<StylesAction> {
    let mut action = None;
    let where_kept = match (e.in_model, e.in_mine) {
        (true, true) if e.differs => "this model; differs from My Styles",
        (true, true) => "this model and My Styles",
        (true, false) => "this model",
        _ => "My Styles",
    };
    let users = match e.users {
        0 => String::new(),
        1 => " · 1 item".to_owned(),
        n => format!(" · {n} items"),
    };
    // The whole row takes the click; its words are not selectable text, which
    // would take it first.
    let response = ui
        .scope_builder(egui::UiBuilder::new().sense(Sense::click()), |ui| {
            ui.horizontal(|ui| {
                let (rect, _) = ui.allocate_exact_size(PREVIEW, Sense::hover());
                preview(ui, rect, &e.look);
                ui.vertical(|ui| {
                    ui.add(egui::Label::new(&e.name).selectable(false));
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(format!("{where_kept}{users}"))
                                .small()
                                .color(MUTED),
                        )
                        .selectable(false),
                    );
                });
            });
        })
        .response;
    let response = if response.hovered() {
        ui.painter().rect_stroke(
            response.rect.expand(2.0),
            4.0,
            egui::Stroke::new(1.0, Color32::from_gray(0xB0)),
            egui::StrokeKind::Outside,
        );
        response.on_hover_text(format!(
            "Give “{}” to the selected {}",
            e.name,
            plural(kind).to_lowercase()
        ))
    } else {
        response
    };
    if response.clicked() {
        action = apply(view, kind, &e.name);
    }
    response.context_menu(|ui| {
        let can_apply = view.can_apply_style(kind);
        if ui
            .add_enabled(can_apply, egui::Button::new("Apply to Selected"))
            .on_disabled_hover_text(format!("Select a {} first", singular(kind)))
            .clicked()
        {
            action = apply(view, kind, &e.name);
            ui.close();
        }
        let one = one_selected(view, kind);
        if ui
            .add_enabled(one.is_some(), egui::Button::new("Change Style to Match Selected"))
            .on_hover_text("The style takes the look of the selected item; everything following it changes too")
            .on_disabled_hover_text(format!("Select exactly one {}", singular(kind)))
            .clicked()
        {
            if let Some(item) = one {
                let _ = view.update_style(&e.name, &item);
            }
            ui.close();
        }
        if ui
            .add_enabled(e.users > 0, egui::Button::new("Select Items Using It"))
            .clicked()
        {
            view.select_style_users(kind, &e.name);
            ui.close();
        }
        ui.separator();
        if ui
            .button("Edit Style…")
            .on_hover_text("Change the style's colours, border, shape and so on")
            .clicked()
        {
            action = Some(StylesAction::Edit(kind, e.name.clone()));
            ui.close();
        }
        if ui.button("Rename…").clicked() {
            action = Some(StylesAction::Rename(kind, e.name.clone()));
            ui.close();
        }
        if e.in_model && !e.in_mine && ui.button("Keep in My Styles").clicked() {
            view.keep_in_my_styles(kind, &e.name);
            action = Some(StylesAction::Notice(format!("“{}” is now in My Styles", e.name)));
            ui.close();
        }
        if e.differs
            && ui
                .button("Replace My Style with This One")
                .on_hover_text("My Styles takes this model's look for it")
                .clicked()
        {
            view.keep_in_my_styles(kind, &e.name);
            ui.close();
        }
        if e.in_model
            && ui
                .button("Delete from This Model")
                .on_hover_text("Items following it keep their look")
                .clicked()
        {
            view.delete_style(kind, &e.name);
            ui.close();
        }
        if e.in_mine
            && ui
                .button("Remove from My Styles")
                .on_hover_text("Models that have it keep their own; this can't be undone")
                .clicked()
        {
            view.remove_from_my_styles(kind, &e.name);
            ui.close();
        }
    });
    action
}

/// Applies a style to the selected items of its kind, or says why not.
fn apply(view: &mut DiagramView, kind: StyleKind, name: &str) -> Option<StylesAction> {
    if !view.can_apply_style(kind) {
        return Some(StylesAction::Notice(format!(
            "Select a {} to give “{name}” to",
            singular(kind)
        )));
    }
    view.apply_style(kind, name);
    None
}

/// The Edit Style window (S §8.6): a picture of the style on its own item,
/// in a small diagram of its own, with the Properties fields that are part of
/// a look. OK gives the style the item's look.
pub struct StyleEditor {
    kind: StyleKind,
    name: String,
    view: DiagramView,
    inspector: Inspector,
}

/// How the Edit Style window was closed.
pub enum EditorOutcome {
    Ok(StyleKind, String, pd_core::model::Look),
    Cancel,
}

impl StyleEditor {
    pub fn new(kind: StyleKind, name: String, look: pd_core::model::Look) -> Self {
        let mut view = DiagramView::new(pd_core::styles::preview(&look));
        view.select_item(&Self::item(kind), false);
        Self {
            kind,
            name,
            view,
            inspector: Inspector::for_styles(),
        }
    }

    /// The one item of the preview (see `pd_core::styles::preview`).
    fn item(kind: StyleKind) -> Target {
        match kind {
            StyleKind::Species => Target::Species("A".into()),
            StyleKind::Reaction => Target::Leg("R".into()),
            StyleKind::Compartment => Target::Compartment("c".into()),
            StyleKind::Text => Target::Text("T".into()),
        }
    }

    /// The window's own small diagram, for tests.
    #[cfg(test)]
    pub fn view_mut(&mut self) -> &mut DiagramView {
        &mut self.view
    }

    /// The look as edited so far.
    pub fn look(&self) -> Option<pd_core::model::Look> {
        let id = match Self::item(self.kind) {
            Target::Species(id) | Target::Leg(id) | Target::Compartment(id) | Target::Text(id) => {
                id
            }
            _ => return None,
        };
        pd_core::styles::look_of(self.view.model(), self.kind, &id)
    }

    /// The window. Returns how it was closed, once it is.
    pub fn show(&mut self, ctx: &egui::Context) -> Option<EditorOutcome> {
        let mut outcome = None;
        let mut open = true;
        egui::Window::new(format!("Edit Style: {}", self.name))
            .id(egui::Id::unique("edit-style"))
            .collapsible(false)
            .resizable(false)
            // Beside the right panel, clear of the diagram's top left.
            .default_pos(ctx.content_rect().right_top() + egui::vec2(-660.0, 60.0))
            .open(&mut open)
            .show(ctx, |ui| {
                if let Some(look) = self.look() {
                    let (rect, _) = ui.allocate_exact_size(Vec2::new(240.0, 90.0), Sense::hover());
                    preview(ui, rect, &look);
                }
                ui.add_space(4.0);
                self.inspector.show(ui, &mut self.view);
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if ui
                        .button("OK")
                        .on_hover_text(
                            "Give the style this look; everything following it changes too",
                        )
                        .clicked()
                        && let Some(look) = self.look()
                    {
                        outcome = Some(EditorOutcome::Ok(self.kind, self.name.clone(), look));
                    }
                    if ui.button("Cancel").clicked() {
                        outcome = Some(EditorOutcome::Cancel);
                    }
                });
            });
        if !open {
            outcome = Some(EditorOutcome::Cancel);
        }
        outcome
    }
}

/// The style drawn on a tiny model, scaled to fit `rect`.
fn preview(ui: &egui::Ui, rect: egui::Rect, look: &pd_core::model::Look) {
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 3.0, Color32::from_rgb(0xF8, 0xF9, 0xFA));
    let model = pd_core::styles::preview(look);
    let (a, b) = content_bounds(&model);
    let m = pt(CONTENT_MARGIN, CONTENT_MARGIN);
    let (a, b) = (a + m, b - m);
    let size = b - a;
    let zoom = (f64::from(rect.width() - 6.0) / size.x)
        .min(f64::from(rect.height() - 6.0) / size.y)
        .min(1.0);
    let centre = (a + b) * 0.5;
    let screen = pt(f64::from(rect.center().x), f64::from(rect.center().y));
    let opts = DrawOptions {
        zoom,
        scroll: screen - centre * zoom,
        background: None,
        for_output: true,
        ..DrawOptions::default()
    };
    draw_model(&model, &opts, &mut EguiRenderer::new(&painter));
}
