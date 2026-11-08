//! The Events & Rules tab (docs/sbml-app-design.md): the model's events,
//! rules, starting values from formulas, functions, units and checks, each
//! as a plain sentence under its heading. Clicking a sentence opens it in
//! place as a small form (A1); Done is one undo step.

use eframe::egui::{self, Color32, RichText};
use pd_core::drafts::{self, Draft, EventDraft, Field, Problem, RuleKind};
use pd_core::sentences::{self, Part};
use pd_core::view::DiagramView;

const ERROR: Color32 = Color32::from_rgb(0xCC, 0x33, 0x00);

/// A part being edited, or added.
#[derive(Clone, Debug)]
pub struct Editing {
    /// `None` for a new part.
    pub part: Option<Part>,
    pub draft: Draft,
    /// The part as the form started, to notice it changed underneath (an
    /// undo) and let go of it.
    original: Option<Draft>,
    /// Why Delete didn't.
    refused: Option<String>,
}

#[derive(Default)]
pub struct EventsTab {
    pub editing: Option<Editing>,
}

/// Where a new part's form goes: under the heading it will be shown under.
fn heading_of(draft: &Draft) -> &'static str {
    match draft {
        Draft::Event(_) => "Events",
        Draft::Rule(_) => "Rules",
        Draft::Start { .. } => "Starting values from formulas",
        Draft::Function { .. } => "Functions",
        Draft::Unit { .. } => "Units",
        Draft::Check { .. } => "Checks",
    }
}

impl EventsTab {
    /// Starts editing `part`.
    pub fn edit(&mut self, view: &DiagramView, part: Part) {
        if let Some(draft) = drafts::draft_of(view.model(), part) {
            self.editing = Some(Editing {
                part: Some(part),
                original: Some(draft.clone()),
                draft,
                refused: None,
            });
        }
    }

    /// Starts adding a part, from `draft`.
    pub fn add(&mut self, draft: Draft) {
        self.editing = Some(Editing {
            part: None,
            draft,
            original: None,
            refused: None,
        });
    }

    /// Done: saves the form if nothing is wrong with it. Returns whether it
    /// did.
    pub fn done(&mut self, view: &mut DiagramView) -> bool {
        let Some(e) = &self.editing else {
            return false;
        };
        if view.save_part(e.part, &e.draft).is_ok() {
            self.editing = None;
            return true;
        }
        false
    }

    /// Delete: takes the part out, unless something still uses it.
    pub fn delete(&mut self, view: &mut DiagramView) {
        let Some(e) = &mut self.editing else {
            return;
        };
        let Some(part) = e.part else {
            self.editing = None;
            return;
        };
        match view.delete_part(part) {
            Ok(_) => self.editing = None,
            Err(why) => e.refused = Some(why),
        }
    }

    pub fn show(&mut self, ui: &mut egui::Ui, view: &mut DiagramView) {
        // Let go of a part an undo or another edit changed underneath.
        if let Some(e) = &self.editing
            && let Some(part) = e.part
            && drafts::draft_of(view.model(), part) != e.original
        {
            self.editing = None;
        }
        let sections = sentences::sections(view.model());
        let mut start: Option<Part> = None;
        let mut add: Option<Draft> = None;
        let mut action: Option<FormAction> = None;
        let adding = self
            .editing
            .as_ref()
            .filter(|e| e.part.is_none())
            .map(|e| heading_of(&e.draft));
        for (n, section) in sections.iter().enumerate() {
            if n > 0 {
                ui.add_space(8.0);
            }
            ui.label(RichText::new(section.heading).strong());
            if section.items.is_empty() && adding != Some(section.heading) {
                ui.label(RichText::new("None").weak());
            }
            for item in &section.items {
                ui.add_space(2.0);
                if let Some(e) = self.editing.as_mut().filter(|e| e.part == Some(item.part)) {
                    action = form(ui, view, e).or(action);
                    continue;
                }
                let response = ui
                    .horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        let mut r = None;
                        if let Some(title) = &item.title {
                            r = Some(
                                ui.add(
                                    egui::Label::new(RichText::new(format!("{title}:")).strong())
                                        .sense(egui::Sense::click()),
                                ),
                            );
                        }
                        let s =
                            ui.add(egui::Label::new(&item.sentence).sense(egui::Sense::click()));
                        r.map_or(s.clone(), |r| r.union(s))
                    })
                    .inner
                    .on_hover_text("Click to change it");
                if response.clicked() {
                    start = Some(item.part);
                }
                for more in &item.more {
                    ui.label(RichText::new(format!("    {more}")).small().weak());
                }
            }
            if adding == Some(section.heading)
                && let Some(e) = self.editing.as_mut()
            {
                action = form(ui, view, e).or(action);
            }
            let new = match section.heading {
                "Events" => Some(("Add an event", drafts::new_event())),
                "Rules" => Some(("Add a rule", drafts::new_rule(RuleKind::Assignment))),
                _ => None,
            };
            if let Some((label, draft)) = new
                && self.editing.is_none()
                && ui.small_button(label).clicked()
            {
                add = Some(draft);
            }
        }
        // A new part whose heading isn't shown yet: its form at the end.
        if let Some(h) = adding
            && !sections.iter().any(|s| s.heading == h)
            && let Some(e) = self.editing.as_mut()
        {
            ui.add_space(8.0);
            ui.label(RichText::new(h).strong());
            action = form(ui, view, e).or(action);
        }
        if self.editing.is_none() {
            ui.add_space(8.0);
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new("Also add:").small());
                let empty = |kind: &str| match kind {
                    "a starting value" => Draft::Start {
                        symbol: String::new(),
                        formula: String::new(),
                    },
                    "a function" => Draft::Function {
                        id: String::new(),
                        arguments: String::new(),
                        body: String::new(),
                    },
                    "a unit" => Draft::Unit {
                        id: String::new(),
                        text: String::new(),
                    },
                    _ => Draft::Check {
                        formula: String::new(),
                        message: String::new(),
                    },
                };
                for kind in ["a starting value", "a function", "a unit", "a check"] {
                    if ui.small_button(kind).clicked() {
                        add = Some(empty(kind));
                    }
                }
            });
        }
        match action {
            Some(FormAction::Done) => {
                self.done(view);
            }
            Some(FormAction::Cancel) => self.editing = None,
            Some(FormAction::Delete) => self.delete(view),
            None => {}
        }
        if let Some(part) = start {
            self.edit(view, part);
        }
        if let Some(draft) = add {
            self.add(draft);
        }
    }
}

enum FormAction {
    Done,
    Cancel,
    Delete,
}

/// A text field filling the row.
fn field(ui: &mut egui::Ui, text: &mut String, hint: &str) {
    ui.add(
        egui::TextEdit::singleline(text)
            .hint_text(hint)
            .desired_width(f32::INFINITY),
    );
}

/// A narrow text field, for what a formula sets.
fn name_field(ui: &mut egui::Ui, text: &mut String, hint: &str) {
    ui.add(
        egui::TextEdit::singleline(text)
            .hint_text(hint)
            .desired_width(70.0),
    );
}

/// What `problems` say of `field`, under it.
fn say(ui: &mut egui::Ui, problems: &[Problem], field: Field) {
    for p in problems.iter().filter(|p| p.field == field) {
        let text = RichText::new(&p.message).small();
        ui.label(if p.error {
            text.color(ERROR)
        } else {
            text.weak()
        });
    }
}

/// The form for `e`, in a frame; what its buttons asked for.
fn form(ui: &mut egui::Ui, view: &DiagramView, e: &mut Editing) -> Option<FormAction> {
    let problems = drafts::check(view.model(), e.part, &e.draft);
    let ok = !problems.iter().any(|p| p.error);
    let mut action = None;
    egui::Frame::group(ui.style()).show(ui, |ui| {
        match &mut e.draft {
            Draft::Event(d) => {
                // Its id, which formulas and other tools know it by.
                if let Some(Part::Event(i)) = e.part
                    && let Some(id) = view.model().events.get(i).and_then(|x| x.id.as_deref())
                {
                    ui.label(RichText::new(format!("Event {id}")).strong());
                }
                event_form(ui, d, &problems);
            }
            Draft::Rule(r) => {
                ui.horizontal(|ui| {
                    if r.kind != RuleKind::Algebraic {
                        name_field(ui, &mut r.variable, "what");
                    }
                    egui::ComboBox::from_id_salt("rule kind")
                        .selected_text(kind_words(r.kind))
                        .show_ui(ui, |ui| {
                            for k in [RuleKind::Assignment, RuleKind::Rate, RuleKind::Algebraic] {
                                ui.selectable_value(&mut r.kind, k, kind_words(k));
                            }
                        });
                });
                say(ui, &problems, Field::Variable(0));
                field(ui, &mut r.formula, "formula");
                say(ui, &problems, Field::Formula(0));
            }
            Draft::Start { symbol, formula } => {
                ui.horizontal(|ui| {
                    name_field(ui, symbol, "what");
                    ui.label("starts at");
                });
                say(ui, &problems, Field::Variable(0));
                field(ui, formula, "formula");
                say(ui, &problems, Field::Formula(0));
            }
            Draft::Function {
                id,
                arguments,
                body,
            } => {
                ui.horizontal(|ui| {
                    name_field(ui, id, "name");
                    ui.label("(");
                    name_field(ui, arguments, "x, y");
                    ui.label(") =");
                });
                say(ui, &problems, Field::Id);
                say(ui, &problems, Field::Arguments);
                field(ui, body, "formula of its arguments");
                say(ui, &problems, Field::Body);
            }
            Draft::Unit { id, text } => {
                ui.horizontal(|ui| {
                    name_field(ui, id, "name");
                    ui.label("=");
                });
                say(ui, &problems, Field::Id);
                field(ui, text, "0.001 mole / litre");
                say(ui, &problems, Field::Text);
            }
            Draft::Check { formula, message } => {
                field(ui, formula, "condition, such as A >= 0");
                say(ui, &problems, Field::Formula(0));
                ui.label(RichText::new("else say").small());
                field(ui, message, "message (optional)");
            }
        }
        ui.horizontal(|ui| {
            if ui.add_enabled(ok, egui::Button::new("Done")).clicked() {
                action = Some(FormAction::Done);
            }
            if ui.button("Cancel").clicked() {
                action = Some(FormAction::Cancel);
            }
            if e.part.is_some() && ui.button("Delete").clicked() {
                action = Some(FormAction::Delete);
            }
        });
        if let Some(why) = &e.refused {
            ui.label(
                RichText::new(format!("Not deleted: {why}."))
                    .small()
                    .color(ERROR),
            );
        }
    });
    action
}

fn kind_words(k: RuleKind) -> &'static str {
    match k {
        RuleKind::Assignment => "is always",
        RuleKind::Rate => "changes at the rate",
        RuleKind::Algebraic => "is kept at 0:",
    }
}

fn event_form(ui: &mut egui::Ui, d: &mut EventDraft, problems: &[Problem]) {
    ui.horizontal(|ui| {
        ui.label("Name");
        field(ui, &mut d.name, "optional");
    });
    ui.label("When");
    field(ui, &mut d.trigger, "condition, such as time > 10");
    say(ui, problems, Field::Trigger);
    let mut remove = None;
    for (n, (var, formula)) in d.assignments.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            name_field(ui, var, "what");
            ui.label("becomes");
            if ui
                .small_button("\u{2715}")
                .on_hover_text("Remove this change")
                .clicked()
            {
                remove = Some(n);
            }
        });
        say(ui, problems, Field::Variable(n));
        field(ui, formula, "formula");
        say(ui, problems, Field::Formula(n));
    }
    if let Some(n) = remove {
        d.assignments.remove(n);
    }
    if ui.small_button("Another change").clicked() {
        d.assignments.push((String::new(), String::new()));
    }
    ui.horizontal(|ui| {
        ui.label("After a delay of");
        field(ui, &mut d.delay, "none");
    });
    say(ui, problems, Field::Delay);
    ui.horizontal(|ui| {
        ui.label("Priority");
        field(ui, &mut d.priority, "none");
    });
    say(ui, problems, Field::Priority);
    ui.checkbox(&mut d.fires_at_start, "Can fire at the start")
        .on_hover_text("If its condition is already true at the start, it fires then");
    let mut stops = !d.persistent;
    if ui
        .checkbox(&mut stops, "Stops if its condition stops being true")
        .on_hover_text("Before it takes effect, after its delay")
        .changed()
    {
        d.persistent = !stops;
    }
    let mut later = !d.values_at_trigger;
    if ui
        .checkbox(&mut later, "New values worked out when it takes effect")
        .on_hover_text("Not when it fires; matters only with a delay")
        .changed()
    {
        d.values_at_trigger = !later;
    }
}
