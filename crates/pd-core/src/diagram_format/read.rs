//! A diagram's statements applied to the model the Antimony text made
//! (docs/diagram-format-implementation-design.md §3).
//!
//! Order matters (see [`apply`]): constants, styles and the species' further
//! drawings first; then everything is placed; then the theme, which works from
//! where things are; then the looks; legs and modifier lines; ids last. An item
//! given one style follows it when it looks exactly like it. Nothing is fatal:
//! a statement that doesn't fit the model is skipped with a warning (format
//! §13).

use std::collections::HashMap;

use super::ast::*;
use super::colors::Constants;
use crate::antimony::{Diagnostic, Severity};
use crate::geometry::{Pt, pt};
use crate::model::{
    ALIAS_OFFSET, BorderDash, Frame, LinePattern, Look, Model, ModifierKind, ModifierLook,
    NodeShape, ReactionMode, Rgba, Role, StyleKind, TextAlign, Typeface, VisualStyle,
};
use crate::passes::NULL_OFFSET;
use crate::render::draw;
use crate::styles;

/// What applying a diagram did.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Applied {
    /// Species drawings the diagram gave no position, which were laid out
    /// automatically.
    pub laid_out: Vec<String>,
}

/// Applies `d` to `model`, adding to `warnings`.
///
/// In two passes round the theme. First everything is placed: species,
/// junctions (between their species when not given), stems, empty sets,
/// compartment boxes and texts. Then the theme, which works from where things
/// are (Escher's curves and the names it shows). Then each item's look, shape
/// and own settings, so they come after the theme's; then legs and modifier
/// lines; and ids last, since one placed on the clearest side looks at every
/// line.
pub fn apply(model: &mut Model, d: &Diagram, seed: u64, warnings: &mut Vec<Diagnostic>) -> Applied {
    let mut r = Reader {
        warnings,
        constants: Constants::default(),
        styles: HashMap::new(),
        drawings: HashMap::new(),
    };
    r.header(model, d);
    r.diagram_level(model, d);
    r.drawings(model, d);
    let mut placed = Vec::new();
    let mut with_junction = Vec::new();
    let mut nulls: Vec<(String, Pt)> = Vec::new();
    let mut links: Vec<(StyleKind, String, String)> = Vec::new();
    let mut texts = Vec::new();
    for s in &d.statements {
        match &s.kind {
            StatementKind::Compartment { id, clauses, props } => {
                r.compartment(model, Pass::Place, s.pos, id, clauses, props, &mut links)
            }
            StatementKind::Text { text, clauses, .. } => {
                texts.push(r.text_place(model, text, clauses))
            }
            StatementKind::Item {
                drawing, clauses, ..
            } => {
                if let Some(node) = r.node_of(model, drawing) {
                    if r.species_place(model, &node, clauses) {
                        placed.push(node);
                    }
                } else if drawing.copy == 1 && model.find_reaction(&drawing.id).is_some() {
                    if r.reaction_place(model, &drawing.id, clauses, &mut nulls) {
                        with_junction.push(drawing.id.clone());
                    }
                } else {
                    r.warn(
                        s.pos,
                        format!("the model has no species or reaction '{}'", drawing.id),
                    );
                }
            }
            _ => {}
        }
    }
    place_junctions_and_empty_sets(model, &with_junction, &nulls);
    // Species the diagram leaves out are laid out automatically, those it
    // places held where they are (design D5). Before the theme, which works
    // from where things are, and before the legs, whose handles the layout
    // would otherwise reset.
    let left: Vec<String> = model
        .species()
        .iter()
        .filter(|s| !s.is_null_node && !placed.contains(&s.id))
        .map(|s| s.id.clone())
        .collect();
    if !left.is_empty() {
        let given: Vec<(String, Pt)> = with_junction
            .iter()
            .filter_map(|id| model.find_reaction(id).map(|x| (id.clone(), x.junction)))
            .collect();
        let locks: Vec<(String, bool)> = model
            .species()
            .iter()
            .map(|s| (s.id.clone(), s.locked))
            .collect();
        for s in model.species_mut() {
            if placed.contains(&s.id) {
                s.locked = true;
            }
        }
        crate::layout::LayoutRun::new(model, crate::layout::LayoutParams::default(), seed)
            .finish(model);
        for (id, locked) in locks {
            if let Some(s) = model.find_species_mut(&id) {
                s.locked = locked;
            }
        }
        for (id, j) in given {
            if let Some(x) = model.find_reaction_mut(&id) {
                x.junction = j;
            }
        }
        place_junctions_and_empty_sets(model, &with_junction, &nulls);
    }
    let theme = d.statements.iter().find_map(|s| match &s.kind {
        StatementKind::Theme(t) => Some((s.pos, t.clone())),
        _ => None,
    });
    if let Some((pos, key)) = theme {
        match crate::themes::Theme::from_key(&key) {
            Some(t) => crate::themes::apply(model, t),
            None => r.warn(
                pos,
                format!("there is no theme '{key}'; the default look is used"),
            ),
        }
    }
    // A style the file defines with the name of one of the theme's replaces
    // it (format §6.4).
    let replacing: Vec<(StyleKind, String)> = r
        .styles
        .keys()
        .filter_map(|(kind, name)| Some((StyleKind::from_key(kind)?, name.clone())))
        .filter(|(kind, name)| styles::find(model, *kind, name).is_some())
        .collect();
    let defs: Vec<(StyleKind, String, Look)> = replacing
        .into_iter()
        .filter_map(|(kind, name)| r.saved_look(kind, &name).map(|look| (kind, name, look)))
        .collect();
    replace_theme_styles(model, &defs);
    let mut pending = Vec::new();
    let mut texts = texts.into_iter();
    for s in &d.statements {
        match &s.kind {
            StatementKind::Compartment { id, clauses, props } => {
                r.compartment(model, Pass::Look, s.pos, id, clauses, props, &mut links)
            }
            StatementKind::Text { clauses, props, .. } => {
                if let Some(id) = texts.next() {
                    r.text_look(model, &id, clauses, props, &mut links);
                }
            }
            StatementKind::Item {
                drawing,
                clauses,
                props,
            } => {
                if let Some(node) = r.node_of(model, drawing) {
                    r.species_look(model, &node, clauses, props, &mut pending, &mut links);
                } else if drawing.copy == 1 && model.find_reaction(&drawing.id).is_some() {
                    r.reaction_look(model, &drawing.id, clauses, props, &mut pending, &mut links);
                }
            }
            _ => {}
        }
    }
    for s in &d.statements {
        match &s.kind {
            StatementKind::Leg {
                from,
                to,
                clauses,
                props,
            } => r.leg(model, s.pos, from, to, clauses, props),
            StatementKind::Line {
                modifier,
                reaction,
                clauses,
                props,
            } => r.line(model, s.pos, modifier, reaction, clauses, props),
            _ => {}
        }
    }
    for p in pending {
        r.place_id(model, p);
    }
    r.link(model, &links);
    Applied { laid_out: left }
}

/// Junctions the diagram doesn't place go between their species; empty sets
/// it doesn't place go beside their junction, as the import puts them.
fn place_junctions_and_empty_sets(
    model: &mut Model,
    with_junction: &[String],
    nulls: &[(String, Pt)],
) {
    let ids: Vec<String> = model.reactions().iter().map(|x| x.id.clone()).collect();
    for id in &ids {
        if !with_junction.contains(id) {
            let j = crate::antimony_layout::default_junction(model, id);
            if let Some(x) = model.find_reaction_mut(id) {
                x.junction = j;
            }
        }
        let j = model
            .find_reaction(id)
            .map(|x| x.junction)
            .unwrap_or_default();
        for (null, dx) in [
            (format!("_source_{id}"), -NULL_OFFSET),
            (format!("_sink_{id}"), NULL_OFFSET),
        ] {
            let at = nulls.iter().find(|(n, _)| *n == null).map(|(_, p)| *p);
            if let Some(n) = model.find_species_mut(&null) {
                n.center = at.unwrap_or(j + pt(dx, 0.0));
            }
        }
    }
}

/// The two passes round the theme.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Pass {
    Place,
    Look,
}

/// Puts a file's styles in place of the theme's of the same name (format
/// §6.4): each such style of the model takes the file's look, and every item
/// following it is given that look.
pub(crate) fn replace_theme_styles(model: &mut Model, defs: &[(StyleKind, String, Look)]) {
    for (kind, name, look) in defs {
        let Some(s) = model
            .styles
            .iter_mut()
            .find(|s| s.kind() == *kind && s.name == *name)
        else {
            continue;
        };
        s.look = *look;
        for id in styles::users(model, *kind, name) {
            styles::give_look(model, &id, look);
        }
    }
}

/// Where an id goes (format §6.2).
#[derive(Clone, Copy, Debug, PartialEq)]
enum IdPlace {
    Inside,
    Hidden,
    /// Species: the clearest side; reactions: shown, clear of the lines.
    Clear,
    Side(Pt),
    Offset(Pt),
}

/// An id to place once everything else is drawn.
enum Pending {
    Species(String, IdPlace),
    Reaction(String, IdPlace),
}

/// A style as the file defines it: its properties, read when an item uses it.
struct StyleDef {
    kind: String,
    props: Vec<Property>,
}

struct Reader<'w> {
    warnings: &'w mut Vec<Diagnostic>,
    constants: Constants,
    /// By kind and name.
    styles: HashMap<(String, String), StyleDef>,
    /// `(species, copy)` to its node's id, for copies 2 and up.
    drawings: HashMap<(String, u32), String>,
}

/// The settings an item's styles and its own braces give, before they are
/// put on it.
#[derive(Clone, Copy)]
struct Edit {
    style: VisualStyle,
    shape: NodeShape,
    border_dash: BorderDash,
    size: Option<(f64, f64)>,
    id: Option<IdPlace>,
    text: TextEdit,
    modifier: ModifierLook,
    /// A reaction's empty sets' size.
    empty_set: Option<f64>,
}

#[derive(Clone, Copy, Default)]
struct TextEdit {
    size: Option<f64>,
    typeface: Option<Typeface>,
    bold: Option<bool>,
    italic: Option<bool>,
    align: Option<TextAlign>,
    color: Option<Rgba>,
    wrap: Option<f64>,
}

impl Reader<'_> {
    fn warn(&mut self, pos: Pos, message: impl Into<String>) {
        self.warnings.push(Diagnostic {
            severity: Severity::Warning,
            line: pos.line,
            column: pos.column,
            message: message.into(),
        });
    }

    fn header(&mut self, model: &Model, d: &Diagram) {
        if d.version > 1 {
            self.warn(
                d.pos,
                format!(
                    "the diagram is for version {} of the format; what this version knows is read",
                    d.version
                ),
            );
        }
        let name = model.model_name.as_str();
        if !["", "__main", "anonymous"].contains(&name) && name != d.name {
            self.warn(
                d.pos,
                format!("the diagram is of '{}', the model is '{name}'", d.name),
            );
        }
    }

    /// Colors, styles, the background and modifier lines.
    fn diagram_level(&mut self, model: &mut Model, d: &Diagram) {
        for s in &d.statements {
            match &s.kind {
                StatementKind::ColorDef { name, value } => {
                    let v = match value {
                        Color::Value(c) => Some(*c),
                        Color::None => Some(Rgba::TRANSPARENT),
                        Color::Name(n) => super::colors::basic(n),
                    };
                    match v {
                        Some(v) => {
                            if !self.constants.define(name, v) {
                                self.warn(
                                    s.pos,
                                    format!(
                                        "'{name}' is defined already; the first definition is used"
                                    ),
                                );
                            }
                        }
                        None => self.warn(s.pos, format!("the value of '{name}' isn't a color")),
                    }
                }
                StatementKind::Style { name, kind, props } => {
                    let kind = kind.clone().unwrap_or_else(|| "species".into());
                    if !["species", "reaction", "compartment", "text", "modifier"]
                        .contains(&kind.as_str())
                    {
                        self.warn(
                            s.pos,
                            format!("'{kind}' isn't a kind of style; the style is skipped"),
                        );
                        continue;
                    }
                    let key = (kind.clone(), name.clone());
                    if self.styles.contains_key(&key) {
                        self.warn(
                            s.pos,
                            format!(
                                "the {kind} style '{name}' is defined already; the first is used"
                            ),
                        );
                        continue;
                    }
                    self.styles.insert(
                        key,
                        StyleDef {
                            kind,
                            props: props.clone(),
                        },
                    );
                }
                _ => {}
            }
        }
        // After the constants, which a background may name.
        for s in &d.statements {
            match &s.kind {
                StatementKind::Background(c) => match self.color(c) {
                    Some(c) => model.background = c,
                    None => self.warn(s.pos, "the background isn't a color"),
                },
                StatementKind::ModifierLinesHidden(h) => model.hide_modifier_lines = *h,
                _ => {}
            }
        }
    }

    fn color(&self, c: &Color) -> Option<Rgba> {
        match c {
            Color::Value(v) => Some(*v),
            Color::None => Some(Rgba::TRANSPARENT),
            Color::Name(n) => self.constants.resolve(n),
        }
    }

    /// Makes the species' further drawings (`ATP#2`) the diagram names, in
    /// order of their numbers, each beside its species.
    fn drawings(&mut self, model: &mut Model, d: &Diagram) {
        let mut wanted: Vec<(Pos, Drawing)> = Vec::new();
        let mut want = |pos: Pos, dr: &Drawing| {
            if dr.copy > 1 && !wanted.iter().any(|(_, w)| w == dr) {
                wanted.push((pos, dr.clone()));
            }
        };
        for s in &d.statements {
            match &s.kind {
                StatementKind::Item {
                    drawing, clauses, ..
                } => {
                    want(s.pos, drawing);
                    for c in clauses {
                        if let ClauseKind::Uses(list) = &c.kind {
                            list.iter().for_each(|dr| want(c.pos, dr));
                        }
                    }
                }
                StatementKind::Leg { from, to, .. } => {
                    want(s.pos, from);
                    want(s.pos, to);
                }
                StatementKind::Line { modifier, .. } => want(s.pos, modifier),
                _ => {}
            }
        }
        wanted.sort_by(|a, b| (&a.1.id, a.1.copy).cmp(&(&b.1.id, b.1.copy)));
        for (pos, dr) in wanted {
            let primary = model
                .find_species(&dr.id)
                .filter(|s| !s.is_alias() && !s.is_null_node)
                .map(|s| s.center);
            let Some(center) = primary else {
                self.warn(
                    pos,
                    format!("the model has no species '{}' to draw again", dr.id),
                );
                continue;
            };
            let n = model.aliases_of(&dr.id).count() as f64 + 1.0;
            if let Some(id) = model.add_alias(&dr.id, center + ALIAS_OFFSET * n) {
                self.drawings.insert((dr.id.clone(), dr.copy), id);
            }
        }
    }

    /// The node a drawing names: the species itself, or its numbered alias.
    fn node_of(&self, model: &Model, dr: &Drawing) -> Option<String> {
        if dr.copy > 1 {
            return self.drawings.get(&(dr.id.clone(), dr.copy)).cloned();
        }
        model
            .find_species(&dr.id)
            .filter(|s| !s.is_alias() && !s.is_null_node)
            .map(|s| s.id.clone())
    }

    /// The styles an item names (`style a, b`), each known for `kind`.
    fn named_styles(&mut self, kind: &str, clauses: &[Clause]) -> Vec<String> {
        let mut out = Vec::new();
        for c in clauses {
            if let ClauseKind::Styles(names) = &c.kind {
                for n in names {
                    if self.styles.contains_key(&(kind.to_owned(), n.clone())) {
                        out.push(n.clone());
                    } else {
                        self.warn(c.pos, format!("there is no {kind} style '{n}'"));
                    }
                }
            }
        }
        out
    }

    /// The edit an item's styles and own braces make to `start`.
    fn edit(&mut self, kind: &str, start: Edit, names: &[String], own: &[Property]) -> Edit {
        let mut e = start;
        for n in names {
            let props = self.styles[&(kind.to_owned(), n.clone())].props.clone();
            for p in &props {
                self.property(kind, &mut e, p);
            }
        }
        for p in own {
            self.property(kind, &mut e, p);
        }
        e
    }

    /// Clauses `allowed` here, the others warned about.
    fn check_clauses(&mut self, what: &str, clauses: &[Clause], allowed: &[&str]) {
        for c in clauses {
            let name = clause_name(&c.kind);
            if let ClauseKind::Unknown(text) = &c.kind {
                self.warn(c.pos, format!("'{text}' isn't understood; skipped"));
            } else if !allowed.contains(&name) {
                self.warn(c.pos, format!("'{name}' doesn't apply to {what}; skipped"));
            }
        }
    }

    /// A compartment: its box when placing, its look after the theme.
    #[allow(clippy::too_many_arguments)]
    fn compartment(
        &mut self,
        model: &mut Model,
        pass: Pass,
        pos: Pos,
        id: &str,
        clauses: &[Clause],
        props: &[Property],
        links: &mut Vec<(StyleKind, String, String)>,
    ) {
        let Some(c) = model.find_compartment(id) else {
            if pass == Pass::Place {
                self.warn(pos, format!("the model has no compartment '{id}'"));
            }
            return;
        };
        if pass == Pass::Place {
            let (mut at, mut size, mut hidden) = (None, None, false);
            for cl in clauses {
                match cl.kind {
                    ClauseKind::At(p) => at = Some(p),
                    ClauseKind::Size(s) => size = Some(s),
                    ClauseKind::Hidden => hidden = true,
                    _ => {}
                }
            }
            let c = model.find_compartment_mut(id).expect("found above");
            c.hidden = hidden;
            match (at, size) {
                (Some(a), Some(s)) => c.frame = Some(Frame::from_corners(a, a + pt(s.w, s.h))),
                (None, None) => {}
                _ => self.warn(pos, "a compartment's box needs both 'at' and 'size'"),
            }
            return;
        }
        self.check_clauses("a compartment", clauses, &["at", "size", "hidden", "style"]);
        let start = Edit::of_style(c.style);
        let names = self.named_styles("compartment", clauses);
        let e = self.edit("compartment", start, &names, props);
        model.find_compartment_mut(id).expect("found above").style = e.style;
        if let [one] = names.as_slice() {
            links.push((StyleKind::Compartment, id.to_owned(), one.clone()));
        }
    }

    /// A text: made, at its place, before the theme (which gives texts its
    /// typeface and color); its look after.
    fn text_place(&mut self, model: &mut Model, text: &str, clauses: &[Clause]) -> String {
        let at = clauses
            .iter()
            .find_map(|c| match c.kind {
                ClauseKind::At(p) => Some(p),
                _ => None,
            })
            .unwrap_or_default();
        model.add_text(at, text)
    }

    fn text_look(
        &mut self,
        model: &mut Model,
        id: &str,
        clauses: &[Clause],
        props: &[Property],
        links: &mut Vec<(StyleKind, String, String)>,
    ) {
        self.check_clauses("a text", clauses, &["at", "style"]);
        let names = self.named_styles("text", clauses);
        let e = self.edit(
            "text",
            Edit::of_style(VisualStyle::default()),
            &names,
            props,
        );
        let Some(t) = model.find_text_mut(id) else {
            return;
        };
        let te = e.text;
        t.size = te.size.unwrap_or(t.size);
        t.font.typeface = te.typeface.unwrap_or(t.font.typeface);
        t.font.bold = te.bold.unwrap_or(t.font.bold);
        t.font.italic = te.italic.unwrap_or(t.font.italic);
        t.align = te.align.unwrap_or(t.align);
        t.color = te.color.unwrap_or(t.color);
        t.wrap_width = te.wrap.unwrap_or(t.wrap_width);
        if let [one] = names.as_slice() {
            links.push((StyleKind::Text, id.to_owned(), one.clone()));
        }
    }

    /// A species drawing's place; true if the statement gives one.
    fn species_place(&mut self, model: &mut Model, node: &str, clauses: &[Clause]) -> bool {
        let s = model.find_species_mut(node).expect("a node");
        let mut placed = false;
        for c in clauses {
            match c.kind {
                ClauseKind::At(p) => {
                    s.center = p;
                    placed = true;
                }
                ClauseKind::Locked => s.locked = true,
                _ => {}
            }
        }
        placed
    }

    /// A species drawing's look, size and id, after the theme.
    fn species_look(
        &mut self,
        model: &mut Model,
        node: &str,
        clauses: &[Clause],
        props: &[Property],
        pending: &mut Vec<Pending>,
        links: &mut Vec<(StyleKind, String, String)>,
    ) {
        self.check_clauses("a species", clauses, &["at", "locked", "style"]);
        let s = model.find_species(node).expect("a node");
        let start = Edit {
            shape: s.shape,
            border_dash: s.border_dash,
            ..Edit::of_style(s.style)
        };
        let names = self.named_styles("species", clauses);
        let e = self.edit("species", start, &names, props);
        let s = model.find_species_mut(node).expect("a node");
        s.style = VisualStyle {
            junction_visible: s.style.junction_visible,
            ..e.style
        };
        s.shape = e.shape;
        s.border_dash = e.border_dash;
        if let Some((w, h)) = e.size {
            (s.width, s.height) = (w, h);
        }
        if s.shape == NodeShape::Circle && s.width != s.height {
            let d = s.width.max(s.height);
            (s.width, s.height) = (d, d);
        }
        if let Some(place) = e.id {
            pending.push(Pending::Species(node.to_owned(), place));
        }
        if let [one] = names.as_slice() {
            links.push((StyleKind::Species, node.to_owned(), one.clone()));
        }
    }

    /// A reaction's junction, the drawings its lines go to, its stem and its
    /// empty sets; true if the statement places the junction.
    fn reaction_place(
        &mut self,
        model: &mut Model,
        id: &str,
        clauses: &[Clause],
        nulls: &mut Vec<(String, Pt)>,
    ) -> bool {
        let mut placed = false;
        let free = clauses.iter().any(|c| c.kind == ClauseKind::Free);
        for c in clauses {
            match &c.kind {
                ClauseKind::Uses(list) => {
                    for dr in list {
                        self.use_drawing(model, c.pos, id, dr);
                    }
                }
                ClauseKind::Source(p) => self.null_at(model, c.pos, id, "_source_", *p, nulls),
                ClauseKind::Sink(p) => self.null_at(model, c.pos, id, "_sink_", *p, nulls),
                _ => {}
            }
        }
        let r = model.find_reaction_mut(id).expect("a reaction");
        for c in clauses {
            match c.kind {
                ClauseKind::At(p) => {
                    r.junction = p;
                    placed = true;
                }
                ClauseKind::Stem(a, b) => {
                    r.stem = Some((a, b));
                    r.stem_straight = !free;
                }
                _ => {}
            }
        }
        placed
    }

    /// A reaction's shape, look and id, after the theme.
    fn reaction_look(
        &mut self,
        model: &mut Model,
        id: &str,
        clauses: &[Clause],
        props: &[Property],
        pending: &mut Vec<Pending>,
        links: &mut Vec<(StyleKind, String, String)>,
    ) {
        self.check_clauses(
            "a reaction",
            clauses,
            &[
                "at", "straight", "linear", "curved", "smooth", "stem", "free", "uses", "source",
                "sink", "style",
            ],
        );
        let r = model.find_reaction(id).expect("a reaction");
        let start = Edit::of_style(r.style);
        let names = self.named_styles("reaction", clauses);
        let e = self.edit("reaction", start, &names, props);
        let r = model.find_reaction_mut(id).expect("a reaction");
        for c in clauses {
            match c.kind {
                ClauseKind::Shape(Shape::Straight) => r.mode = ReactionMode::Straight,
                ClauseKind::Shape(Shape::Linear) => r.mode = ReactionMode::Linear,
                ClauseKind::Shape(Shape::Curved) => r.mode = ReactionMode::Bezier,
                ClauseKind::Smooth(on) => r.junction_smooth = on,
                _ => {}
            }
        }
        r.style = VisualStyle {
            junction_visible: e.style.junction_visible,
            ..e.style
        };
        if let Some(d) = e.empty_set {
            for null in [format!("_source_{id}"), format!("_sink_{id}")] {
                if let Some(n) = model.find_species_mut(&null) {
                    (n.width, n.height) = (d, d);
                }
            }
        }
        if let Some(place) = e.id {
            pending.push(Pending::Reaction(id.to_owned(), place));
        }
        if let [one] = names.as_slice() {
            links.push((StyleKind::Reaction, id.to_owned(), one.clone()));
        }
    }

    /// `uses ATP#2`: the reaction's line to ATP goes to that drawing.
    fn use_drawing(&mut self, model: &mut Model, pos: Pos, reaction: &str, dr: &Drawing) {
        let Some(node) = self.node_of(model, dr) else {
            self.warn(pos, format!("the model has no species '{}'", dr.id));
            return;
        };
        if !set_drawing(
            model,
            reaction,
            &[Role::Reactant, Role::Product],
            &dr.id,
            &node,
        ) {
            self.warn(
                pos,
                format!("'{}' isn't a reactant or product of '{reaction}'", dr.id),
            );
        }
    }

    fn null_at(
        &mut self,
        model: &Model,
        pos: Pos,
        reaction: &str,
        prefix: &str,
        p: Pt,
        nulls: &mut Vec<(String, Pt)>,
    ) {
        let id = format!("{prefix}{reaction}");
        if model.find_species(&id).is_some() {
            nulls.push((id, p));
        } else {
            let side = if prefix == "_source_" {
                "reactants"
            } else {
                "products"
            };
            self.warn(
                pos,
                format!("'{reaction}' has {side}, so no empty set to place"),
            );
        }
    }

    fn leg(
        &mut self,
        model: &mut Model,
        pos: Pos,
        from: &Drawing,
        to: &Drawing,
        clauses: &[Clause],
        props: &[Property],
    ) {
        let is_reaction = |d: &Drawing| d.copy == 1 && model.find_reaction(&d.id).is_some();
        let (role, drawing, reaction) = if is_reaction(to) && !is_reaction(from) {
            (Role::Reactant, from, to.id.clone())
        } else if is_reaction(from) && !is_reaction(to) {
            (Role::Product, to, from.id.clone())
        } else {
            self.warn(pos, "a leg joins a species' drawing and a reaction");
            return;
        };
        let Some(node) = self.node_of(model, drawing) else {
            self.warn(pos, format!("the model has no species '{}'", drawing.id));
            return;
        };
        if !set_drawing(model, &reaction, &[role], &drawing.id, &node) {
            let side = if role == Role::Reactant {
                "a reactant"
            } else {
                "a product"
            };
            self.warn(
                pos,
                format!("'{}' isn't {side} of '{reaction}'", drawing.id),
            );
            return;
        }
        self.check_clauses("a leg", clauses, &["via", "curve"]);
        for p in props {
            self.warn(
                p.pos,
                format!("'{}' isn't a property of a leg; skipped", p.name),
            );
        }
        let r = model.find_reaction_mut(&reaction).expect("a reaction");
        let part = r
            .participants_mut(role)
            .iter_mut()
            .find(|p| p.species == node)
            .expect("set above");
        for c in clauses {
            match &c.kind {
                ClauseKind::Via(points) => part.points = points.clone(),
                ClauseKind::Curve(a, b) => {
                    (part.ctrl1, part.ctrl2) = (*a, *b);
                    part.ctrl_pts_set = true;
                }
                _ => {}
            }
        }
    }

    fn line(
        &mut self,
        model: &mut Model,
        pos: Pos,
        modifier: &Drawing,
        reaction: &str,
        clauses: &[Clause],
        props: &[Property],
    ) {
        if model.find_reaction(reaction).is_none() {
            self.warn(pos, format!("the model has no reaction '{reaction}'"));
            return;
        }
        let Some(node) = self.node_of(model, modifier) else {
            self.warn(pos, format!("the model has no species '{}'", modifier.id));
            return;
        };
        if !set_drawing(model, reaction, &[Role::Modifier], &modifier.id, &node) {
            self.warn(
                pos,
                format!("'{}' doesn't modify '{reaction}'", modifier.id),
            );
            return;
        }
        self.check_clauses("a modifier's line", clauses, &["via", "kind", "style"]);
        let names = self.named_styles("modifier", clauses);
        let start = Edit {
            modifier: ModifierLook::default(),
            ..Edit::of_style(VisualStyle::default())
        };
        let e = self.edit("modifier", start, &names, props);
        let mut kind = None;
        for c in clauses {
            if let ClauseKind::Kind(k) = &c.kind {
                match modifier_kind(k) {
                    Some(k) => kind = Some((c.pos, k)),
                    None => self.warn(c.pos, format!("'{k}' isn't a kind of modifier")),
                }
            }
        }
        let r = model.find_reaction_mut(reaction).expect("checked");
        let part = r
            .modifiers
            .iter_mut()
            .find(|p| p.species == node)
            .expect("set above");
        let mut disagree = None;
        if let Some((kpos, k)) = kind {
            if part.kind == ModifierKind::Modifier {
                part.kind = k;
            } else if part.kind != k {
                disagree = Some(kpos);
            }
        }
        for c in clauses {
            if let ClauseKind::Via(points) = &c.kind {
                part.points = points.clone();
            }
        }
        part.look = e.modifier;
        if let Some(kpos) = disagree {
            self.warn(
                kpos,
                "the model's interaction gives this modifier another kind; the model's is used",
            );
        }
    }

    /// Reads one property into `e`, for an item of `kind`.
    fn property(&mut self, kind: &str, e: &mut Edit, p: &Property) {
        let v = &p.values;
        let ok = match (kind, p.name.as_str()) {
            ("species", "shape") => word(v).and_then(NodeShape::from_key).map(|s| e.shape = s),
            ("species", "size") => size(v).map(|s| e.size = Some(s)),
            ("species" | "compartment", "fill") => self.color_value(v).map(|c| {
                e.style.fill_color = c;
                e.style.has_custom_style = true;
            }),
            ("species" | "compartment", "border") => self.border(v, e),
            ("species", "borderStyle") => word(v)
                .and_then(BorderDash::from_key)
                .map(|d| e.border_dash = d),
            ("species", "id") => species_id(v).map(|i| e.id = Some(i)),
            ("reaction", "id") => reaction_id(v).map(|i| e.id = Some(i)),
            ("species" | "reaction", "idColor") => self.color_value(v).map(|c| {
                e.style.label_color = c;
                e.style.has_custom_style = true;
            }),
            ("species" | "reaction" | "compartment", "idSize") => number(v).map(|n| {
                e.style.font_size = n;
                e.style.has_custom_style = true;
            }),
            ("species" | "reaction", "idBold") => flag(v).map(|b| {
                e.style.label_bold = b;
                e.style.has_custom_style = true;
            }),
            ("species" | "reaction", "idItalic") => flag(v).map(|b| {
                e.style.label_italic = b;
                e.style.has_custom_style = true;
            }),
            ("reaction", "line") => self.line_look(v).map(|(c, w, _)| {
                if let Some(c) = c {
                    e.style.line_color = c;
                }
                if let Some(w) = w {
                    e.style.line_width = w;
                }
                e.style.has_custom_style = true;
            }),
            ("reaction", "emptySetSize") => number(v)
                .filter(|n| *n > 0.0)
                .map(|n| e.empty_set = Some(n)),
            ("reaction", "junction") => match word(v) {
                Some(w @ ("hidden" | "shown")) => {
                    e.style.junction_visible = w == "shown";
                    Some(())
                }
                _ => self.color_value(v).map(|c| {
                    e.style.junction_color = c;
                    e.style.junction_visible = true;
                    e.style.has_custom_style = true;
                }),
            },
            ("text", "size") => number(v).map(|n| e.text.size = Some(n)),
            ("text", "font") => match word(v) {
                Some("sans") => Some(Typeface::Sans),
                Some("serif") => Some(Typeface::Serif),
                _ => None,
            }
            .map(|t| e.text.typeface = Some(t)),
            ("text", "bold") => flag(v).map(|b| e.text.bold = Some(b)),
            ("text", "italic") => flag(v).map(|b| e.text.italic = Some(b)),
            ("text", "align") => match word(v) {
                Some("left") => Some(TextAlign::Left),
                Some("center") => Some(TextAlign::Center),
                Some("right") => Some(TextAlign::Right),
                _ => None,
            }
            .map(|a| e.text.align = Some(a)),
            ("text", "color") => self.color_value(v).map(|c| e.text.color = Some(c)),
            ("text", "wrap") => number(v).map(|n| e.text.wrap = Some(n)),
            ("modifier", "line") => self.line_look(v).map(|(c, w, pattern)| {
                e.modifier.colour = c.or(e.modifier.colour);
                e.modifier.width = w.or(e.modifier.width);
                e.modifier.pattern = pattern.unwrap_or(e.modifier.pattern);
            }),
            ("modifier", "symbol") => number(v).map(|n| e.modifier.symbol_size = Some(n)),
            ("modifier", "distance") => number(v).map(|n| e.modifier.symbol_gap = Some(n)),
            _ => {
                self.warn(
                    p.pos,
                    format!("'{}' isn't a property of a {kind}; skipped", p.name),
                );
                return;
            }
        };
        if ok.is_none() {
            self.warn(
                p.pos,
                format!("the value of '{}' doesn't read; skipped", p.name),
            );
        }
    }

    fn color_value(&self, v: &[Value]) -> Option<Rgba> {
        match v {
            [one] => self.value_color(one),
            _ => None,
        }
    }

    fn value_color(&self, v: &Value) -> Option<Rgba> {
        match v {
            Value::Hex(c) => Some(*c),
            Value::Word(w) if w == "none" => Some(Rgba::TRANSPARENT),
            Value::Word(w) => self.constants.resolve(w),
            _ => None,
        }
    }

    /// `border <color> <width>`, either alone, or `none`.
    fn border(&self, v: &[Value], e: &mut Edit) -> Option<()> {
        if let [Value::Word(w)] = v
            && w == "none"
        {
            e.style.no_border = true;
            return Some(());
        }
        let mut any = false;
        for x in v {
            match x {
                Value::Number(n) => e.style.border_width = *n,
                _ => e.style.border_color = self.value_color(x)?,
            }
            any = true;
        }
        e.style.has_custom_style = true;
        e.style.no_border = false;
        any.then_some(())
    }

    /// `line <color> <width> [pattern]`, each part optional.
    fn line_look(&self, v: &[Value]) -> Option<(Option<Rgba>, Option<f64>, Option<LinePattern>)> {
        let (mut c, mut w, mut pattern) = (None, None, None);
        for x in v {
            match x {
                Value::Number(n) => w = Some(*n),
                Value::Word(p) if ["dashed", "dotted", "solid"].contains(&p.as_str()) => {
                    pattern = Some(LinePattern::from_key(p))
                }
                _ => c = Some(self.value_color(x)?),
            }
        }
        (!v.is_empty()).then_some((c, w, pattern))
    }

    /// Puts an id where the diagram says, now that the lines are drawn.
    fn place_id(&mut self, model: &mut Model, p: Pending) {
        match p {
            Pending::Species(id, place) => {
                let Some(s) = model.find_species(&id) else {
                    return;
                };
                let offset = match place {
                    IdPlace::Inside | IdPlace::Hidden => None,
                    IdPlace::Clear => Some(draw::clear_label_offset(model, s)),
                    IdPlace::Side(d) => Some(draw::label_offset_on_side(s, d)),
                    IdPlace::Offset(o) => Some(o),
                };
                if let Some(s) = model.find_species_mut(&id) {
                    s.label_offset = offset;
                }
            }
            Pending::Reaction(id, place) => {
                let Some(r) = model.find_reaction(&id) else {
                    return;
                };
                let offset = match place {
                    IdPlace::Inside | IdPlace::Hidden => None,
                    IdPlace::Clear => Some(draw::clear_reaction_label_offset(model, r)),
                    IdPlace::Side(d) => Some(draw::reaction_label_offset_on_side(r, d)),
                    IdPlace::Offset(o) => Some(o),
                };
                if let Some(r) = model.find_reaction_mut(&id) {
                    r.label_offset = offset;
                }
            }
        }
    }

    /// Items given one style and nothing of their own follow it, if they look
    /// exactly like it (design D6). The style is added to the model's saved
    /// styles when some item follows it.
    fn link(&mut self, model: &mut Model, links: &[(StyleKind, String, String)]) {
        for (kind, item, name) in links {
            let Some(look) = self.saved_look(*kind, name) else {
                continue;
            };
            let Some(item_look) = styles::look_of(model, *kind, item) else {
                continue;
            };
            if !styles::same_look(item_look, look) {
                continue;
            }
            if styles::find(model, *kind, name).is_none() {
                model.styles.push(crate::model::SavedStyle {
                    name: name.clone(),
                    look,
                });
            }
            if styles::find(model, *kind, name)
                .is_some_and(|s| styles::same_look(s.look, item_look))
            {
                styles::set_link(model, *kind, item, Some(name.clone()));
            }
        }
        styles::drop_stale_links(model);
    }

    /// A file style as a saved style's look: its properties over the
    /// defaults.
    fn saved_look(&mut self, kind: StyleKind, name: &str) -> Option<Look> {
        let key = (kind.key().to_owned(), name.to_owned());
        let def = self.styles.get(&key)?;
        let (k, props) = (def.kind.clone(), def.props.clone());
        let mut e = Edit::of_style(VisualStyle::default());
        let warnings = std::mem::take(self.warnings);
        for p in &props {
            self.property(&k, &mut e, p);
        }
        // Already warned about when the items read them.
        *self.warnings = warnings;
        Some(match kind {
            StyleKind::Species => {
                let outside = !matches!(e.id, None | Some(IdPlace::Inside) | Some(IdPlace::Hidden));
                Look::Species {
                    style: e.style,
                    shape: e.shape,
                    border_dash: e.border_dash,
                    label_outside: outside,
                    size: if outside { e.size } else { None },
                    alias: false,
                }
            }
            StyleKind::Reaction => Look::Reaction { style: e.style },
            StyleKind::Compartment => Look::Compartment { style: e.style },
            StyleKind::Text => {
                let t = e.text;
                let base = crate::model::TextItem::new("", Pt::ZERO, "");
                Look::Text {
                    font: crate::model::TextFont {
                        typeface: t.typeface.unwrap_or_default(),
                        bold: t.bold.unwrap_or(false),
                        italic: t.italic.unwrap_or(false),
                    },
                    align: t.align.unwrap_or_default(),
                    size: t.size.unwrap_or(base.size),
                    color: t.color.unwrap_or(base.color),
                }
            }
        })
    }
}

impl Edit {
    fn of_style(style: VisualStyle) -> Edit {
        Edit {
            style,
            shape: NodeShape::default(),
            border_dash: BorderDash::default(),
            size: None,
            id: None,
            text: TextEdit::default(),
            modifier: ModifierLook::default(),
            empty_set: None,
        }
    }
}

/// Points the line of `role` from `reaction` to `species` (any of its
/// drawings) at `node`. False if the reaction has no such line.
fn set_drawing(
    model: &mut Model,
    reaction: &str,
    roles: &[Role],
    species: &str,
    node: &str,
) -> bool {
    let primary_of = |id: &str| {
        model
            .find_species(id)
            .map(|s| s.primary_id().to_owned())
            .unwrap_or_default()
    };
    let Some(r) = model.find_reaction(reaction) else {
        return false;
    };
    // The line already to this drawing, else the first to the species.
    let found = roles.iter().find_map(|&role| {
        let list = r.participants(role);
        list.iter()
            .position(|p| p.species == node)
            .or_else(|| list.iter().position(|p| primary_of(&p.species) == species))
            .map(|i| (role, i))
    });
    let Some((role, i)) = found else {
        return false;
    };
    let r = model.find_reaction_mut(reaction).expect("found above");
    r.participants_mut(role)[i].species = node.to_owned();
    true
}

fn clause_name(c: &ClauseKind) -> &'static str {
    match c {
        ClauseKind::At(_) => "at",
        ClauseKind::Size(_) => "size",
        ClauseKind::Hidden => "hidden",
        ClauseKind::Locked => "locked",
        ClauseKind::Shape(Shape::Straight) => "straight",
        ClauseKind::Shape(Shape::Linear) => "linear",
        ClauseKind::Shape(Shape::Curved) => "curved",
        ClauseKind::Smooth(_) => "smooth",
        ClauseKind::Stem(..) => "stem",
        ClauseKind::Free => "free",
        ClauseKind::Uses(_) => "uses",
        ClauseKind::Source(_) => "source",
        ClauseKind::Sink(_) => "sink",
        ClauseKind::Styles(_) => "style",
        ClauseKind::Via(_) => "via",
        ClauseKind::Curve(..) => "curve",
        ClauseKind::Kind(_) => "kind",
        ClauseKind::Unknown(_) => "",
    }
}

fn word(v: &[Value]) -> Option<&str> {
    match v {
        [Value::Word(w)] => Some(w),
        _ => None,
    }
}

fn number(v: &[Value]) -> Option<f64> {
    match v {
        [Value::Number(n)] => Some(*n),
        _ => None,
    }
}

/// A flag: the word alone is on.
/// A flag: the word alone, or with `on`, is on; with `off`, off.
fn flag(v: &[Value]) -> Option<bool> {
    match v {
        [] => Some(true),
        [Value::Word(w)] if w == "on" => Some(true),
        [Value::Word(w)] if w == "off" => Some(false),
        _ => None,
    }
}

/// `80 x 36` or one number.
fn size(v: &[Value]) -> Option<(f64, f64)> {
    match v {
        [Value::Number(d)] => Some((*d, *d)),
        [Value::Number(w), Value::Word(x), Value::Number(h)] if x == "x" => Some((*w, *h)),
        _ => None,
    }
}

fn side(w: &str) -> Option<Pt> {
    Some(match w {
        "above" => pt(0.0, -1.0),
        "below" => pt(0.0, 1.0),
        "left" => pt(-1.0, 0.0),
        "right" => pt(1.0, 0.0),
        _ => return None,
    })
}

fn species_id(v: &[Value]) -> Option<IdPlace> {
    match v {
        [Value::Word(w)] => match w.as_str() {
            "inside" => Some(IdPlace::Inside),
            "outside" => Some(IdPlace::Clear),
            w => side(w).map(IdPlace::Side),
        },
        [Value::Word(o), Value::Point(p)] if o == "offset" => Some(IdPlace::Offset(*p)),
        _ => None,
    }
}

fn reaction_id(v: &[Value]) -> Option<IdPlace> {
    match v {
        [Value::Word(w)] => match w.as_str() {
            "hidden" => Some(IdPlace::Hidden),
            "shown" => Some(IdPlace::Clear),
            w => side(w).map(IdPlace::Side),
        },
        [Value::Word(o), Value::Point(p)] if o == "offset" => Some(IdPlace::Offset(*p)),
        _ => None,
    }
}

fn modifier_kind(k: &str) -> Option<ModifierKind> {
    Some(match k {
        "modifier" => ModifierKind::Modifier,
        "activator" => ModifierKind::Activator,
        "inhibitor" => ModifierKind::Inhibitor,
        "catalyst" => ModifierKind::Catalyst,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use crate::antimony::{ImportOptions, Imported, import};
    use crate::geometry::pt;
    use crate::model::{Model, ModifierKind, NodeShape, ReactionMode, Rgba};

    const OPTS: ImportOptions = ImportOptions {
        convert_catalytic: false,
        seed: 1,
    };

    const MODEL: &str = "model m
  compartment cell = 1
  species $Glc in cell, G6P in cell, $ATP in cell, $ADP in cell, $E in cell, $I in cell
  hk: Glc + ATP -> G6P + ADP; E*k*Glc*ATP/(1 + I)
  out: G6P -> ; k*G6P
  k = 1
";

    /// The model above, ended, with `diagram` as its diagram.
    fn read(diagram: &str) -> Imported {
        let text = format!("{MODEL}end\n/* diagram m v1\n{diagram}\nend */\n");
        import(&text, &OPTS).unwrap_or_else(|e| panic!("{e:?}"))
    }

    /// Read, with no warnings.
    fn clean(diagram: &str) -> Model {
        let i = read(diagram);
        assert!(i.warnings.is_empty(), "{:?}", i.warnings);
        i.model
    }

    fn messages(i: &Imported) -> Vec<String> {
        i.warnings.iter().map(|w| w.message.clone()).collect()
    }

    #[test]
    fn positions_sizes_and_what_is_left_to_layout() {
        let i = read("Glc at (100, 50)\nG6P at (100, 250) locked { size 24; shape circle }");
        assert!(i.warnings.is_empty(), "{:?}", i.warnings);
        assert!(i.had_layout);
        let m = &i.model;
        let g = m.find_species("G6P").unwrap();
        assert_eq!(
            (g.center, g.width, g.height, g.shape, g.locked),
            (pt(100.0, 250.0), 24.0, 24.0, NodeShape::Circle, true)
        );
        // ATP, ADP, E and I have no place: they are laid out as the diagram
        // is read, with Glc and G6P held where it puts them (design D5).
        assert_eq!(i.unplaced, 0);
        assert_eq!(m.find_species("Glc").unwrap().center, pt(100.0, 50.0));
        // They were held for the layout, and are locked only if it says so.
        assert!(!m.find_species("Glc").unwrap().locked && m.find_species("G6P").unwrap().locked);
        let random = import(
            &format!(
                "{MODEL}end
"
            ),
            &OPTS,
        )
        .unwrap()
        .model;
        for id in ["ATP", "ADP", "E", "I"] {
            let (laid, at_random) = (
                m.find_species(id).unwrap().center,
                random.find_species(id).unwrap().center,
            );
            assert_ne!(laid, at_random, "{id}");
        }
        // A junction the diagram places stays where it says.
        let i = read(
            "Glc at (100, 50)
hk at (300, 300)",
        );
        assert_eq!(
            i.model.find_reaction("hk").unwrap().junction,
            pt(300.0, 300.0)
        );
        // The junction goes between its species, the sink beside it.
        let hk = m.find_reaction("hk").unwrap();
        assert_eq!(
            hk.junction,
            crate::antimony_layout::default_junction(m, "hk")
        );
        let out = m.find_reaction("out").unwrap();
        assert_eq!(
            m.find_species("_sink_out").unwrap().center,
            out.junction + pt(70.0, 0.0)
        );
    }

    #[test]
    fn a_text_without_a_diagram_is_read_as_before() {
        let i = import(&format!("{MODEL}end\n"), &OPTS).unwrap();
        assert!(!i.had_layout);
        // A diagram, which says more, replaces Antimony's layout statements;
        // a text with both is what PathwayDesigner writes for tellurium.
        let text = format!(
            "{MODEL}  Glc.position = {{0, 0}}
end
/* diagram m v1
Glc at (100, 50)
end */"
        );
        let i = import(&text, &OPTS).unwrap();
        assert_eq!(i.model.find_species("Glc").unwrap().center, pt(100.0, 50.0));
        assert!(i.warnings.is_empty(), "{:?}", i.warnings);
    }

    #[test]
    fn reactions_their_shape_stem_ends_and_ids() {
        let m = clean(
            "hk at (200, 150) curved smooth stem (-20, 0) (20, 0) free { id below }\nout at (300, 150) linear sink (400, 150) { id offset (0, -20) }",
        );
        let hk = m.find_reaction("hk").unwrap();
        assert_eq!(
            (hk.junction, hk.mode, hk.junction_smooth),
            (pt(200.0, 150.0), ReactionMode::Bezier, true)
        );
        assert_eq!(hk.stem, Some((pt(-20.0, 0.0), pt(20.0, 0.0))));
        assert!(!hk.stem_straight, "free");
        let below = crate::render::draw::reaction_label_offset_on_side(hk, pt(0.0, 1.0));
        assert_eq!(hk.label_offset, Some(below));
        // Below the junction by its dot, half the line, the gap and half the
        // name.
        use crate::render::constants::*;
        let (a, b) = crate::render::draw::reaction_label_box(&m, hk).unwrap();
        let half = (b.y - a.y) / 2.0 - LABEL_BOX_PAD;
        let want = JUNCTION_RADIUS + DEFAULT_LINE_WIDTH / 2.0 + LABEL_GAP + half;
        assert!(
            (below.y - want).abs() < 1e-9 && below.x == 0.0,
            "{below:?} {want}"
        );
        let out = m.find_reaction("out").unwrap();
        assert_eq!(
            (out.mode, out.label_offset),
            (ReactionMode::Linear, Some(pt(0.0, -20.0)))
        );
        assert_eq!(
            m.find_species("_sink_out").unwrap().center,
            pt(400.0, 150.0)
        );
        // A stem without `free` stays straight; `id shown` places it clear.
        let m = clean("hk at (0, 0) stem (-20, 0) (20, 0) { id shown }");
        let hk = m.find_reaction("hk").unwrap();
        assert!(hk.stem_straight);
        assert_eq!(
            hk.label_offset,
            Some(crate::render::draw::clear_reaction_label_offset(&m, hk))
        );
    }

    #[test]
    fn second_drawings_and_the_lines_that_go_to_them() {
        let m = clean("ATP#2 at (300, 300)\nhk uses ATP#2");
        let alias = m.aliases_of("ATP").next().expect("an alias");
        assert_eq!(alias.center, pt(300.0, 300.0));
        let hk = m.find_reaction("hk").unwrap();
        assert!(hk.reactants.iter().any(|p| p.species == alias.id));
        assert!(!hk.reactants.iter().any(|p| p.species == "ATP"));
        // Numbers need not be consecutive: ADP#5 is ADP's second drawing.
        let m = clean("hk uses ADP#5");
        assert_eq!(m.aliases_of("ADP").count(), 1);
    }

    #[test]
    fn legs_with_handles_and_corner_points() {
        let m = clean(
            "hk at (200, 150) curved\nleg Glc -> hk curve (110, 60) (180, 140)\nleg hk -> G6P via (250, 200) (250, 260)",
        );
        let hk = m.find_reaction("hk").unwrap();
        let glc = hk.reactants.iter().find(|p| p.species == "Glc").unwrap();
        assert!(glc.ctrl_pts_set);
        assert_eq!((glc.ctrl1, glc.ctrl2), (pt(110.0, 60.0), pt(180.0, 140.0)));
        let g6p = hk.products.iter().find(|p| p.species == "G6P").unwrap();
        assert_eq!(g6p.points, [pt(250.0, 200.0), pt(250.0, 260.0)]);
        // A leg naming a second drawing moves the line to it.
        let m = clean("leg ATP#2 -> hk via (1, 2)");
        let alias = m.aliases_of("ATP").next().unwrap().id.clone();
        let hk = m.find_reaction("hk").unwrap();
        let p = hk.reactants.iter().find(|p| p.species == alias).unwrap();
        assert_eq!(p.points, [pt(1.0, 2.0)]);
    }

    #[test]
    fn modifier_lines_their_kind_route_and_look() {
        let m = clean(
            "line E to hk kind catalyst via (10, 20) { line red 3 solid; symbol 14; distance 9 }",
        );
        let hk = m.find_reaction("hk").unwrap();
        let e = hk.modifiers.iter().find(|p| p.species == "E").unwrap();
        assert_eq!(
            (e.kind, e.points.clone()),
            (ModifierKind::Catalyst, vec![pt(10.0, 20.0)])
        );
        assert_eq!(e.look.colour, Some(Rgba::rgb(255, 0, 0)));
        assert_eq!(
            (e.look.width, e.look.symbol_size, e.look.symbol_gap),
            (Some(3.0), Some(14.0), Some(9.0))
        );
        assert_eq!(e.look.pattern, crate::model::LinePattern::Solid);
        // From a modifier style.
        let m = clean(
            "style feedback modifier { line #C80000 2 }\nline I to hk kind inhibitor style feedback",
        );
        let hk = m.find_reaction("hk").unwrap();
        let i = hk.modifiers.iter().find(|p| p.species == "I").unwrap();
        assert_eq!((i.kind, i.look.width), (ModifierKind::Inhibitor, Some(2.0)));
        assert_eq!(i.look.colour, Some(Rgba::rgb(0xC8, 0, 0)));
    }

    #[test]
    fn the_models_interaction_wins_over_a_lines_kind() {
        let text =
            format!("{MODEL}  I -| hk\nend\n/* diagram m v1\nline I to hk kind activator\nend */");
        let i = import(&text, &OPTS).unwrap();
        let hk = i.model.find_reaction("hk").unwrap();
        let kind = hk.modifiers.iter().find(|p| p.species == "I").unwrap().kind;
        assert_eq!(kind, ModifierKind::Inhibitor);
        assert!(
            messages(&i)
                .iter()
                .any(|m| m.contains("the model's is used")),
            "{:?}",
            i.warnings
        );
    }

    #[test]
    fn styles_constants_and_who_follows_them() {
        let m = clean(
            "color metab = #FFD54F
style metabolite { shape circle; size 24; fill metab; border gray 1.5; id right }
style flux reaction { line navy 2.5 }
Glc at (0, 0) style metabolite
G6P at (0, 100) style metabolite { idColor red }
hk style flux",
        );
        let glc = m.find_species("Glc").unwrap();
        assert_eq!((glc.shape, glc.width), (NodeShape::Circle, 24.0));
        assert_eq!(glc.style.fill_color, Rgba::rgb(0xFF, 0xD5, 0x4F));
        assert_eq!(
            (glc.style.border_color, glc.style.border_width),
            (Rgba::rgb(128, 128, 128), 1.5)
        );
        assert_eq!(
            glc.label_offset,
            Some(crate::render::draw::label_offset_on_side(glc, pt(1.0, 0.0)))
        );
        // Glc follows the style; G6P has a setting of its own, so it doesn't,
        // though it has the style's look and its own red name.
        assert_eq!(glc.style_name.as_deref(), Some("metabolite"));
        let g6p = m.find_species("G6P").unwrap();
        assert_eq!(g6p.style_name, None);
        assert_eq!(
            (g6p.shape, g6p.style.label_color),
            (NodeShape::Circle, Rgba::rgb(255, 0, 0))
        );
        let hk = m.find_reaction("hk").unwrap();
        assert_eq!(
            (hk.style.line_color, hk.style.line_width),
            (Rgba::rgb(0, 0, 128), 2.5)
        );
        assert_eq!(hk.style_name.as_deref(), Some("flux"));
        assert!(crate::styles::find(&m, crate::model::StyleKind::Species, "metabolite").is_some());
    }

    #[test]
    fn smaller_settings() {
        let i = read(
            "theme escher
style metabolite { fill red }
Glc at (0, 0) style metabolite
G6P at (0, 0) { shape circle; size 30 x 20; border none }
ATP#2 at (0, 0)
hk { idBold; idItalic; id shown }",
        );
        assert!(i.warnings.is_empty(), "{:?}", i.warnings);
        let m = &i.model;
        // Under a theme, an item given a style doesn't look exactly like the
        // style alone, so it doesn't follow it.
        let glc = m.find_species("Glc").unwrap();
        assert_eq!(glc.style.fill_color, Rgba::rgb(255, 0, 0));
        assert_ne!(glc.style_name.as_deref(), Some("metabolite"));
        // A circle takes its larger side both ways; no border.
        let g6p = m.find_species("G6P").unwrap();
        assert_eq!(
            (g6p.width, g6p.height, g6p.style.no_border),
            (30.0, 30.0, true)
        );
        // Nobody follows the style, so the diagram doesn't keep it.
        assert!(crate::styles::find(m, crate::model::StyleKind::Species, "metabolite").is_none());
        let hk = m.find_reaction("hk").unwrap();
        assert!(hk.style.label_bold && hk.style.label_italic && hk.label_offset.is_some());
        // Glc, G6P and ATP's second drawing stay where the diagram puts them.
        assert_eq!(i.unplaced, 0);
        assert_eq!(m.find_species("G6P").unwrap().center, pt(0.0, 0.0));
        assert_eq!(m.aliases_of("ATP").next().unwrap().center, pt(0.0, 0.0));
        // A hidden junction (the Escher theme hides them anyway).
        let m = clean(
            "hk { junction hidden }
out { junction red }",
        );
        assert!(!m.find_reaction("hk").unwrap().style.junction_visible);
        let out = m.find_reaction("out").unwrap();
        assert!(out.style.junction_visible && out.style.junction_color == Rgba::rgb(255, 0, 0));
        // An item follows its style while it looks exactly like it: settings
        // of its own that the style has too, or that aren't part of a look
        // (a size, an id's place), keep the link (design D6, as revised).
        let m = clean(
            "style metabolite { shape circle }
style cofactor { shape circle; size 16; id right }
Glc style metabolite { shape circle; size 40 }
G6P style metabolite { fill red }
ATP style cofactor { id offset (30, -5) }
ADP style cofactor { id inside }",
        );
        let glc = m.find_species("Glc").unwrap();
        assert_eq!(glc.style_name.as_deref(), Some("metabolite"));
        assert_eq!(glc.width, 40.0);
        assert_eq!(m.find_species("G6P").unwrap().style_name, None);
        let atp = m.find_species("ATP").unwrap();
        assert_eq!(atp.style_name.as_deref(), Some("cofactor"));
        assert_eq!(atp.label_offset, Some(pt(30.0, -5.0)));
        // Whether the id is outside is part of the look.
        assert_eq!(m.find_species("ADP").unwrap().style_name, None);
    }

    #[test]
    fn diagram_level_settings() {
        let m = clean("theme escher\nbackground navy\nmodifierLines hidden");
        assert_eq!(m.theme, Some(crate::themes::Theme::Escher));
        assert_eq!(m.background, Rgba::rgb(0, 0, 128));
        assert!(m.hide_modifier_lines);
        // The theme works from where the diagram puts things: Escher's smooth
        // handles and the names it shows are those of the final positions,
        // not of the import's random ones.
        let m = clean(
            "theme escher\nGlc at (0, 0)\nATP at (0, 100)\nG6P at (200, 0)\nADP at (200, 100)\nhk at (100, 50)",
        );
        let mut again = m.clone();
        crate::bezier::apply_smooth_handles(&mut again, "hk");
        let (a, b) = (
            m.find_reaction("hk").unwrap(),
            again.find_reaction("hk").unwrap(),
        );
        assert!(a.mode == ReactionMode::Bezier && a.junction_smooth);
        for (p, q) in a
            .reactants
            .iter()
            .chain(&a.products)
            .zip(b.reactants.iter().chain(&b.products))
        {
            assert!(
                p.ctrl_pts_set && (p.ctrl1, p.ctrl2) == (q.ctrl1, q.ctrl2),
                "{p:?} {q:?}"
            );
        }
        assert_eq!(
            a.label_offset,
            Some(crate::render::draw::clear_reaction_label_offset(&m, a))
        );
        // The theme's look reaches the items, and an item's own settings come
        // after it.
        let m = clean("theme escher\nGlc { fill red }");
        assert_eq!(
            m.find_species("Glc").unwrap().style.fill_color,
            Rgba::rgb(255, 0, 0)
        );
        assert!(m.find_species("G6P").unwrap().style_name.is_some());
    }

    #[test]
    fn compartments_and_texts() {
        let m = clean(
            "compartment cell at (10, 20) size 300 x 200 { fill none; border green 2; idSize 16 }\ntext \"Note\" at (5, 6) { size 20; bold; italic; font serif; align center; color gray; wrap 120 }",
        );
        let c = m.find_compartment("cell").unwrap();
        let f = c.frame.unwrap();
        assert_eq!((f.min(), f.width, f.height), (pt(10.0, 20.0), 300.0, 200.0));
        assert!(c.style.fill_color.is_transparent());
        assert_eq!(
            (
                c.style.border_color,
                c.style.border_width,
                c.style.font_size
            ),
            (Rgba::rgb(0, 128, 0), 2.0, 16.0)
        );
        let t = &m.texts()[0];
        assert_eq!(
            (t.text.as_str(), t.position, t.size, t.wrap_width),
            ("Note", pt(5.0, 6.0), 20.0, 120.0)
        );
        assert!(t.font.bold && t.font.italic && t.font.typeface == crate::model::Typeface::Serif);
        assert_eq!(
            (t.align, t.color),
            (crate::model::TextAlign::Center, Rgba::rgb(128, 128, 128))
        );
        let m = clean("compartment cell hidden");
        assert!(m.find_compartment("cell").unwrap().hidden);
    }

    #[test]
    fn species_ids_placed_by_side_offset_or_the_clearest_side() {
        let m = clean(
            "Glc at (0, 0) { id above }\nG6P at (0, 200) { id offset (40, -20) }\nATP at (200, 0) { id outside }",
        );
        let above = m.find_species("Glc").unwrap().label_offset.unwrap();
        assert!(above.x == 0.0 && above.y < -18.0, "{above:?}");
        assert_eq!(
            m.find_species("G6P").unwrap().label_offset,
            Some(pt(40.0, -20.0))
        );
        let atp = m.find_species("ATP").unwrap();
        assert_eq!(
            atp.label_offset,
            Some(crate::render::draw::clear_label_offset(&m, atp))
        );
    }

    #[test]
    fn what_doesnt_fit_the_model_is_skipped_with_a_warning() {
        let i = read(
            "Nothing at (1, 2)
Glc at (1, 2) curved glow
hk locked { fill red; arrowSize 14 }
leg Glc -> G6P via (1, 2)
leg E -> hk via (1, 2)
line Glc to out
Glc style missing
text \"t\" at (0, 0) { size big }
compartment nowhere hidden",
        );
        let m = messages(&i);
        for expect in [
            "no species or reaction 'Nothing'",
            "'curved' doesn't apply to a species",
            "'glow' isn't understood",
            "'locked' doesn't apply to a reaction",
            "'fill' isn't a property of a reaction",
            "'arrowSize' isn't a property of a reaction",
            "a leg joins a species' drawing and a reaction",
            "'E' isn't a reactant of 'hk'",
            "'Glc' doesn't modify 'out'",
            "there is no species style 'missing'",
            "the value of 'size' doesn't read",
            "no compartment 'nowhere'",
        ] {
            assert!(
                m.iter().any(|w| w.contains(expect)),
                "missing {expect:?} in {m:#?}"
            );
        }
        assert_eq!(m.len(), 12, "{m:#?}");
        // Still read: Glc's position.
        assert_eq!(i.model.find_species("Glc").unwrap().center, pt(1.0, 2.0));
    }

    #[test]
    fn the_header_is_checked_against_the_model() {
        let text = format!("{MODEL}end\n/* diagram other v2\nGlc at (1, 2)\nend */");
        let i = import(&text, &OPTS).unwrap();
        let m = messages(&i);
        assert!(m.iter().any(|w| w.contains("version 2")), "{m:?}");
        assert!(m.iter().any(|w| w.contains("'other'")), "{m:?}");
        assert_eq!(i.model.find_species("Glc").unwrap().center, pt(1.0, 2.0));
    }
}
