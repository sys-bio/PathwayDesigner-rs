//! A model's drawing as a diagram section in the canonical form
//! (docs/diagram-format.md §12; docs/diagram-format-implementation-design.md
//! §4).
//!
//! Only what differs from what a reader would draw anyway is written. That is
//! worked out against a *reference*: the same diagram with every look, shape
//! and handle as the Antimony import leaves them, the theme applied, which is
//! exactly where a reader stands before an item's own settings. An item that
//! follows one of the diagram's own styles writes `style <name>` and what
//! differs from that style over the reference.

use std::fmt::Write as _;

use super::colors;
use crate::geometry::Pt;
use crate::model::{
    BorderDash, DEFAULT_COMPARTMENT, Look, Model, ModifierKind, ModifierLook, NodeShape, Reaction,
    ReactionMode, Rgba, Role, SavedStyle, SpeciesNode, StyleKind, TextAlign, TextItem, Typeface,
    VisualStyle,
};
use crate::passes::NULL_OFFSET;
use crate::render::constants::*;
use crate::render::draw;
use crate::styles;

/// The diagram of `model`, which the Antimony text calls `name`, as a block
/// comment.
pub fn write(model: &Model, name: &str) -> String {
    let w = Writer::new(model);
    let mut out = format!("/* diagram {name} v1\n");
    let mut line = |s: String| {
        out.push_str("  ");
        out.push_str(&s);
        out.push('\n');
    };
    // 1. Diagram-level settings.
    if model.theme == Some(crate::themes::Theme::Escher) {
        line("theme escher".into());
    }
    if !model.background.is_default() {
        line(format!("background {}", color(model.background)));
    }
    if model.hide_modifier_lines {
        line("modifierLines hidden".into());
    }
    // 2. Styles some written item follows, by kind and name.
    for (kind, s) in &w.used_styles {
        line(w.style_def(*kind, s));
    }
    // 3. Compartments.
    for c in &model.compartments {
        if let Some(s) = w.compartment(c) {
            line(s);
        }
    }
    // 4. Texts, in the order they are drawn (a later one covers an earlier).
    for t in model.texts() {
        line(w.text(t));
    }
    // 5. Species drawings, each species' together.
    for s in model
        .species()
        .iter()
        .filter(|s| !s.is_alias() && !s.is_null_node)
    {
        line(w.species(s));
        for a in model.aliases_of(&s.id) {
            line(w.species(a));
        }
    }
    // 6. Reactions, each with its legs.
    for r in model.reactions() {
        if let Some(s) = w.reaction(r) {
            line(s);
        }
        for s in w.legs(r) {
            line(s);
        }
    }
    // 7. Modifier lines, by reaction.
    for r in model.reactions() {
        for s in w.modifier_lines(r) {
            line(s);
        }
    }
    out.push_str("end */\n");
    out
}

struct Writer<'m> {
    model: &'m Model,
    reference: Model,
    /// The styles the file defines: those some item names, and the diagram's
    /// copies of the theme's styles where they differ from the built-in ones
    /// (they replace the theme's, format §6.4). By kind, then name.
    used_styles: Vec<(StyleKind, SavedStyle)>,
}

impl<'m> Writer<'m> {
    fn new(model: &'m Model) -> Self {
        // The diagram's own versions of the theme's styles: an Escher map's,
        // at the map's sizes, or a file's from an older version of the theme.
        let mut replacing: Vec<(StyleKind, SavedStyle)> = Vec::new();
        if model.theme == Some(crate::themes::Theme::Escher) {
            for built_in in crate::themes::escher_styles() {
                if let Some(own) = styles::find(model, built_in.kind(), &built_in.name)
                    && own.look.canonical() != built_in.look.canonical()
                {
                    replacing.push((built_in.kind(), own.clone()));
                }
            }
        }
        let defs: Vec<(StyleKind, String, Look)> = replacing
            .iter()
            .map(|(k, s)| (*k, s.name.clone(), s.look))
            .collect();
        let mut w = Writer {
            model,
            reference: reference(model, &defs),
            used_styles: Vec::new(),
        };
        let mut used = replacing;
        for kind in StyleKind::ALL {
            for id in styles::item_ids(model, kind) {
                if let Some(s) = w.named_style(kind, &id)
                    && !used.iter().any(|(k, u)| *k == kind && u.name == s.name)
                {
                    used.push((kind, s.clone()));
                }
            }
        }
        used.sort_by(|a, b| (kind_order(a.0), &a.1.name).cmp(&(kind_order(b.0), &b.1.name)));
        w.used_styles = used;
        w
    }

    /// The style item `id` names in the file, if any: the one it follows,
    /// unless the theme gives it that style anyway.
    fn named_style(&self, kind: StyleKind, id: &str) -> Option<&SavedStyle> {
        let name = styles::link(self.model, kind, id)?;
        if self.model.theme.is_some()
            && styles::link(&self.reference, kind, id).as_deref() == Some(name.as_str())
        {
            return None;
        }
        styles::find(self.model, kind, &name)
    }

    /// `style <name>`, quoted when the name isn't a word.
    fn style_clause(&self, s: &SavedStyle) -> String {
        format!("style {}", style_ref(&s.name))
    }

    fn style_def(&self, kind: StyleKind, s: &SavedStyle) -> String {
        let kind_word = match kind {
            StyleKind::Species => "",
            StyleKind::Reaction => " reaction",
            StyleKind::Compartment => " compartment",
            StyleKind::Text => " text",
        };
        let props = match s.look {
            Look::Species {
                style,
                shape,
                border_dash,
                label_outside,
                size,
                ..
            } => {
                let mut p = Vec::new();
                if shape != NodeShape::default() {
                    p.push(format!("shape {}", shape.key()));
                }
                if let Some((w, h)) = size {
                    p.push(format!("size {}", size_text(w, h)));
                }
                p.extend(species_style_props(style, VisualStyle::default(), None));
                if border_dash != BorderDash::default() {
                    p.push(format!("borderStyle {}", border_dash.key()));
                }
                if label_outside {
                    p.push("id outside".into());
                }
                p
            }
            Look::Reaction { style } => {
                let mut p = reaction_style_props(style, VisualStyle::default());
                if !style.junction_visible {
                    p.push("junction hidden".into());
                }
                p
            }
            Look::Compartment { style } => compartment_style_props(style, VisualStyle::default()),
            Look::Text {
                font,
                align,
                size,
                color: c,
            } => {
                let base = TextItem::new("", Pt::ZERO, "");
                let mut t = base.clone();
                (t.font, t.align, t.size, t.color) = (font, align, size, c);
                text_props(&t, &base)
            }
        };
        format!("style {}{kind_word} {}", style_ref(&s.name), block(&props))
    }

    fn compartment(&self, c: &crate::model::Compartment) -> Option<String> {
        if c.id.eq_ignore_ascii_case(DEFAULT_COMPARTMENT) {
            return None;
        }
        let base = self.reference.find_compartment(&c.id)?;
        let mut clauses = Vec::new();
        if let Some(f) = c.frame {
            clauses.push(format!(
                "at {} size {}",
                point(f.min()),
                size_text(f.width, f.height)
            ));
        }
        if c.hidden {
            clauses.push("hidden".into());
        }
        let style = self.named_style(StyleKind::Compartment, &c.id);
        let base_style = match style.map(|s| s.look) {
            Some(Look::Compartment { style }) => overlay(base.style, style),
            _ => base.style,
        };
        if let Some(s) = style {
            clauses.push(self.style_clause(s));
        }
        let props = compartment_style_props(c.style, base_style);
        (!clauses.is_empty() || !props.is_empty())
            .then(|| statement(format!("compartment {}", c.id), clauses, props))
    }

    fn text(&self, t: &TextItem) -> String {
        let base = self
            .reference
            .find_text(&t.id)
            .cloned()
            .unwrap_or_else(|| TextItem::new("", Pt::ZERO, ""));
        let mut clauses = vec![format!("at {}", point(t.position))];
        let style = self.named_style(StyleKind::Text, &t.id);
        let mut base_t = base.clone();
        if let Some(s) = style {
            if let Look::Text {
                font,
                align,
                size,
                color: c,
            } = s.look
            {
                (base_t.font, base_t.align, base_t.size, base_t.color) = (font, align, size, c);
            }
            clauses.push(self.style_clause(s));
        }
        let mut props = text_props(t, &base_t);
        if t.wrap_width != base_t.wrap_width {
            props.push(format!("wrap {}", num(t.wrap_width)));
        }
        statement(format!("text {}", string(&t.text)), clauses, props)
    }

    /// `ATP` or `ATP#2`.
    fn drawing(&self, id: &str) -> String {
        let Some(s) = self.model.find_species(id) else {
            return id.to_owned();
        };
        let Some(primary) = &s.alias_of else {
            return s.id.clone();
        };
        let n = self
            .model
            .aliases_of(primary)
            .position(|a| a.id == s.id)
            .unwrap_or(0);
        format!("{primary}#{}", n + 2)
    }

    fn species(&self, s: &SpeciesNode) -> String {
        let base = self
            .reference
            .find_species(&s.id)
            .cloned()
            .unwrap_or_else(|| s.clone());
        let mut clauses = vec![format!("at {}", point(s.center))];
        if s.locked {
            clauses.push("locked".into());
        }
        // What the reader starts from: the reference, under the item's style.
        let mut b = base.clone();
        let mut base_label = base.label_offset;
        if let Some(st) = self.named_style(StyleKind::Species, &s.id) {
            if let Look::Species {
                style,
                shape,
                border_dash,
                label_outside,
                size,
                ..
            } = st.look
            {
                b.style = overlay(b.style, style);
                if shape != NodeShape::default() {
                    b.shape = shape;
                }
                if border_dash != BorderDash::default() {
                    b.border_dash = border_dash;
                }
                if let Some((w, h)) = size {
                    (b.width, b.height) = (w, h);
                }
                if label_outside {
                    base_label = Some(draw::clear_label_offset(self.model, s));
                }
            }
            clauses.push(self.style_clause(st));
        }
        if b.shape == NodeShape::Circle && b.width != b.height {
            let d = b.width.max(b.height);
            (b.width, b.height) = (d, d);
        }
        let mut props = Vec::new();
        if s.shape != b.shape {
            props.push(format!("shape {}", s.shape.key()));
        }
        if (s.width, s.height) != (b.width, b.height) {
            props.push(format!("size {}", size_text(s.width, s.height)));
        }
        let defaults = species_defaults(s);
        props.extend(species_style_props(s.style, b.style, Some(defaults)));
        if s.border_dash != b.border_dash {
            props.push(format!("borderStyle {}", s.border_dash.key()));
        }
        match (s.label_offset, base_label) {
            (None, None) => {}
            (None, Some(_)) => props.push("id inside".into()),
            (Some(o), Some(b)) if o == b => {}
            (Some(o), _) => props.push(match side_of(o, |d| draw::label_offset_on_side(s, d)) {
                Some(side) => format!("id {side}"),
                None => format!("id offset {}", point(o)),
            }),
        }
        statement(self.drawing(&s.id), clauses, props)
    }

    fn reaction(&self, r: &Reaction) -> Option<String> {
        let base = self.reference.find_reaction(&r.id)?;
        let mut clauses = Vec::new();
        if r.junction != crate::antimony_layout::default_junction(self.model, &r.id) {
            clauses.push(format!("at {}", point(r.junction)));
        }
        if r.mode != base.mode {
            clauses.push(
                match r.mode {
                    ReactionMode::Straight => "straight",
                    ReactionMode::Linear => "linear",
                    ReactionMode::Bezier => "curved",
                }
                .into(),
            );
        }
        if r.mode == ReactionMode::Bezier && r.junction_smooth != base.junction_smooth {
            clauses.push(
                if r.junction_smooth {
                    "smooth"
                } else {
                    "smooth off"
                }
                .into(),
            );
        }
        if let Some((a, b)) = r.stem {
            clauses.push(format!("stem {} {}", point(a), point(b)));
            if !r.stem_straight {
                clauses.push("free".into());
            }
        }
        let uses: Vec<String> = r
            .reactants
            .iter()
            .chain(&r.products)
            .filter(|p| {
                self.model
                    .find_species(&p.species)
                    .is_some_and(|s| s.is_alias())
            })
            .map(|p| self.drawing(&p.species))
            .collect();
        if !uses.is_empty() {
            clauses.push(format!("uses {}", uses.join(", ")));
        }
        for (role, dx, word) in [
            (Role::Reactant, -NULL_OFFSET, "source"),
            (Role::Product, NULL_OFFSET, "sink"),
        ] {
            if let Some(n) = empty_set(self.model, r, role)
                && n.center != r.junction + crate::geometry::pt(dx, 0.0)
            {
                clauses.push(format!("{word} {}", point(n.center)));
            }
        }
        let style = self.named_style(StyleKind::Reaction, &r.id);
        let base_style = match style.map(|s| s.look) {
            Some(Look::Reaction { style }) => overlay(base.style, style),
            _ => base.style,
        };
        if let Some(s) = style {
            clauses.push(self.style_clause(s));
        }
        let mut props = reaction_style_props(r.style, base_style);
        let empty = |m: &Model| {
            let r = m.find_reaction(&r.id)?;
            [Role::Reactant, Role::Product]
                .into_iter()
                .find_map(|role| empty_set(m, r, role).map(|s| s.width))
        };
        if let (Some(a), Some(b)) = (empty(self.model), empty(&self.reference))
            && a != b
        {
            props.push(format!("emptySetSize {}", num(a)));
        }
        if r.style.junction_visible != base_style.junction_visible
            && !props.iter().any(|p| p.starts_with("junction"))
        {
            props.push(
                if r.style.junction_visible {
                    "junction shown"
                } else {
                    "junction hidden"
                }
                .into(),
            );
        }
        match (r.label_offset, base.label_offset) {
            (None, None) => {}
            (None, Some(_)) => props.push("id hidden".into()),
            (Some(o), Some(b)) if o == b => {}
            (Some(o), _) => props.push(
                match side_of(o, |d| draw::reaction_label_offset_on_side(r, d)) {
                    Some(side) => format!("id {side}"),
                    None => format!("id offset {}", point(o)),
                },
            ),
        }
        (!clauses.is_empty() || !props.is_empty()).then(|| statement(r.id.clone(), clauses, props))
    }

    /// A `leg` for each line with handles set by hand (unlike the
    /// reference's) or corner points.
    fn legs(&self, r: &Reaction) -> Vec<String> {
        let Some(base) = self.reference.find_reaction(&r.id) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for role in [Role::Reactant, Role::Product] {
            for (i, p) in r.participants(role).iter().enumerate() {
                if self
                    .model
                    .find_species(&p.species)
                    .is_none_or(|s| s.is_null_node)
                {
                    continue;
                }
                let b = base.participants(role).get(i);
                let handles = p.ctrl_pts_set
                    && !b.is_some_and(|b| {
                        b.ctrl_pts_set && (b.ctrl1, b.ctrl2) == (p.ctrl1, p.ctrl2)
                    });
                if !handles && p.points.is_empty() {
                    continue;
                }
                let d = self.drawing(&p.species);
                let mut s = match role {
                    Role::Reactant => format!("leg {d} -> {}", r.id),
                    _ => format!("leg {} -> {d}", r.id),
                };
                if !p.points.is_empty() {
                    let pts: Vec<String> = p.points.iter().map(|&q| point(q)).collect();
                    let _ = write!(s, " via {}", pts.join(" "));
                }
                if handles {
                    let _ = write!(s, " curve {} {}", point(p.ctrl1), point(p.ctrl2));
                }
                out.push(s);
            }
        }
        out
    }

    /// A `line` for each modifier whose line isn't plain: a kind the model
    /// doesn't state, a second drawing, corner points or a look of its own.
    /// Only modifiers the Antimony text keeps: named in the rate law (and not
    /// also a reactant or product, which the import never makes a modifier),
    /// or an activator or inhibitor (written as interactions).
    fn modifier_lines(&self, r: &Reaction) -> Vec<String> {
        let names = law_names(&r.kinetic_law);
        let mut out = Vec::new();
        for m in &r.modifiers {
            let Some(node) = self.model.find_species(&m.species) else {
                continue;
            };
            let primary = node.primary_id();
            let stated = matches!(m.kind, ModifierKind::Activator | ModifierKind::Inhibitor);
            let participant = r.reactants.iter().chain(&r.products).any(|p| {
                self.model
                    .find_species(&p.species)
                    .is_some_and(|s| s.primary_id() == primary)
            });
            if !stated && (participant || !names.iter().any(|n| n == primary)) {
                continue;
            }
            let kind = (m.kind == ModifierKind::Catalyst).then_some("catalyst");
            if kind.is_none() && !node.is_alias() && m.points.is_empty() && m.look.is_default() {
                continue;
            }
            let mut clauses = Vec::new();
            if !m.points.is_empty() {
                let pts: Vec<String> = m.points.iter().map(|&q| point(q)).collect();
                clauses.push(format!("via {}", pts.join(" ")));
            }
            if let Some(k) = kind {
                clauses.push(format!("kind {k}"));
            }
            out.push(statement(
                format!("line {} to {}", self.drawing(&m.species), r.id),
                clauses,
                modifier_props(&m.look),
            ));
        }
        out
    }
}

/// The empty set on `role`'s side of `r`, if it has one: a source among
/// its reactants, a sink among its products.
fn empty_set<'a>(m: &'a Model, r: &Reaction, role: Role) -> Option<&'a SpeciesNode> {
    r.participants(role)
        .iter()
        .find_map(|p| m.find_species(&p.species).filter(|s| s.is_null_node))
}

/// The diagram as a reader stands before any item's own settings: positions,
/// sizes the import gives, junctions, stems, empty sets, boxes and texts as
/// they are; every look, shape, handle and name as the import leaves them;
/// then the theme.
fn reference(model: &Model, replacing: &[(StyleKind, String, Look)]) -> Model {
    let mut m = model.clone();
    for s in m.species_mut().filter(|s| s.is_null_node) {
        (s.width, s.height) = (crate::model::NULL_NODE_SIZE, crate::model::NULL_NODE_SIZE);
    }
    for s in m.species_mut().filter(|s| !s.is_null_node) {
        s.style = VisualStyle::default();
        s.shape = NodeShape::default();
        s.border_dash = BorderDash::default();
        s.label_offset = None;
        s.style_name = None;
        s.width = crate::model::DEFAULT_SPECIES_WIDTH;
        s.height = crate::model::DEFAULT_SPECIES_HEIGHT;
    }
    for r in m.reactions_mut() {
        r.style = VisualStyle::default();
        r.style_name = None;
        r.label_offset = None;
        r.mode = ReactionMode::Straight;
        r.junction_smooth = false;
        for p in r.reactants.iter_mut().chain(&mut r.products) {
            p.ctrl_pts_set = false;
            p.ctrl1 = Pt::ZERO;
            p.ctrl2 = Pt::ZERO;
            p.points.clear();
        }
        for p in &mut r.modifiers {
            p.points.clear();
            p.look = ModifierLook::default();
        }
    }
    for c in &mut m.compartments {
        c.style = VisualStyle::default();
        c.style_name = None;
    }
    let ids: Vec<String> = m.texts().iter().map(|t| t.id.clone()).collect();
    for id in ids {
        if let Some(t) = m.find_text_mut(&id) {
            let plain = TextItem::new("", Pt::ZERO, "");
            (t.font, t.align, t.size, t.color, t.wrap_width) = (
                plain.font,
                plain.align,
                plain.size,
                plain.color,
                plain.wrap_width,
            );
            t.style_name = None;
        }
    }
    m.styles.clear();
    m.theme = None;
    if let Some(theme) = model.theme {
        crate::themes::apply(&mut m, theme);
    }
    super::read::replace_theme_styles(&mut m, replacing);
    m
}

/// `base` with the fields `style` gives (those not "use the default").
fn overlay(base: VisualStyle, style: VisualStyle) -> VisualStyle {
    if !style.has_custom_style {
        return VisualStyle {
            junction_visible: if style.junction_visible {
                base.junction_visible
            } else {
                false
            },
            no_border: base.no_border || style.no_border,
            ..base
        };
    }
    let c = |b: Rgba, s: Rgba| if s.is_default() { b } else { s };
    let n = |b: f64, s: f64| if s == 0.0 { b } else { s };
    VisualStyle {
        has_custom_style: true,
        fill_color: c(base.fill_color, style.fill_color),
        border_color: c(base.border_color, style.border_color),
        label_color: c(base.label_color, style.label_color),
        border_width: n(base.border_width, style.border_width),
        line_color: c(base.line_color, style.line_color),
        line_width: n(base.line_width, style.line_width),
        font_size: n(base.font_size, style.font_size),
        junction_color: c(base.junction_color, style.junction_color),
        junction_visible: if style.junction_visible {
            base.junction_visible
        } else {
            false
        },
        no_border: base.no_border || style.no_border,
        label_bold: base.label_bold || style.label_bold,
        label_italic: base.label_italic || style.label_italic,
    }
}

/// A style's field as it counts: "use the default" without a custom style.
fn eff<T: Copy>(style: &VisualStyle, v: T, default: T) -> T {
    if style.has_custom_style { v } else { default }
}

/// What a species is drawn with where its style says "use the default".
#[derive(Clone, Copy)]
struct SpeciesDefaults {
    fill: Rgba,
    border: Rgba,
    border_width: f64,
}

fn species_defaults(s: &SpeciesNode) -> SpeciesDefaults {
    let mut plain = s.clone();
    plain.style = VisualStyle::default();
    let (fill, border) = draw::species_colours(&plain, true);
    SpeciesDefaults {
        fill,
        border,
        border_width: draw::species_border_width(&plain),
    }
}

/// A color property's value: `actual`, or what "use the default" draws.
fn shown(actual: Rgba, default: Rgba) -> Rgba {
    if actual.is_default() { default } else { actual }
}

fn species_style_props(a: VisualStyle, b: VisualStyle, d: Option<SpeciesDefaults>) -> Vec<String> {
    let d = d.unwrap_or(SpeciesDefaults {
        fill: SPECIES_FILL,
        border: SPECIES_BORDER,
        border_width: DEFAULT_LINE_WIDTH,
    });
    let mut p = Vec::new();
    let (fa, fb) = (
        eff(&a, a.fill_color, Rgba::DEFAULT),
        eff(&b, b.fill_color, Rgba::DEFAULT),
    );
    if fa != fb {
        p.push(format!("fill {}", color(shown(fa, d.fill))));
    }
    if a.no_border && !b.no_border {
        p.push("border none".into());
    } else {
        let (ca, cb) = (
            eff(&a, a.border_color, Rgba::DEFAULT),
            eff(&b, b.border_color, Rgba::DEFAULT),
        );
        let (wa, wb) = (eff(&a, a.border_width, 0.0), eff(&b, b.border_width, 0.0));
        let back = !a.no_border && b.no_border;
        let mut parts = Vec::new();
        if ca != cb || back {
            parts.push(color(shown(ca, d.border)));
        }
        if wa != wb || back {
            parts.push(num(if wa == 0.0 { d.border_width } else { wa }));
        }
        if !parts.is_empty() {
            p.push(format!("border {}", parts.join(" ")));
        }
    }
    p.extend(id_look_props(a, b));
    p
}

/// `idColor`, `idSize`, `idBold`, `idItalic`.
fn id_look_props(a: VisualStyle, b: VisualStyle) -> Vec<String> {
    let mut p = Vec::new();
    let (la, lb) = (
        eff(&a, a.label_color, Rgba::DEFAULT),
        eff(&b, b.label_color, Rgba::DEFAULT),
    );
    if la != lb {
        p.push(format!("idColor {}", color(shown(la, LABEL))));
    }
    let (sa, sb) = (eff(&a, a.font_size, 0.0), eff(&b, b.font_size, 0.0));
    if sa != sb {
        p.push(format!(
            "idSize {}",
            num(if sa == 0.0 { LABEL_FONT_SIZE } else { sa })
        ));
    }
    for (name, on, base) in [
        (
            "idBold",
            eff(&a, a.label_bold, false),
            eff(&b, b.label_bold, false),
        ),
        (
            "idItalic",
            eff(&a, a.label_italic, false),
            eff(&b, b.label_italic, false),
        ),
    ] {
        if on != base {
            p.push(if on {
                name.to_owned()
            } else {
                format!("{name} off")
            });
        }
    }
    p
}

fn reaction_style_props(a: VisualStyle, b: VisualStyle) -> Vec<String> {
    let mut p = Vec::new();
    let (ca, cb) = (
        eff(&a, a.line_color, Rgba::DEFAULT),
        eff(&b, b.line_color, Rgba::DEFAULT),
    );
    let (wa, wb) = (eff(&a, a.line_width, 0.0), eff(&b, b.line_width, 0.0));
    let mut parts = Vec::new();
    if ca != cb {
        parts.push(color(shown(ca, REACTION_LINE)));
    }
    if wa != wb {
        parts.push(num(if wa == 0.0 { DEFAULT_LINE_WIDTH } else { wa }));
    }
    if !parts.is_empty() {
        p.push(format!("line {}", parts.join(" ")));
    }
    let (ja, jb) = (
        eff(&a, a.junction_color, Rgba::DEFAULT),
        eff(&b, b.junction_color, Rgba::DEFAULT),
    );
    if a.junction_visible && ja != jb {
        p.push(format!("junction {}", color(shown(ja, JUNCTION_FILL))));
    }
    p.extend(id_look_props(a, b));
    p
}

fn compartment_style_props(a: VisualStyle, b: VisualStyle) -> Vec<String> {
    let mut p = Vec::new();
    let (fa, fb) = (
        eff(&a, a.fill_color, Rgba::DEFAULT),
        eff(&b, b.fill_color, Rgba::DEFAULT),
    );
    if fa != fb {
        p.push(format!("fill {}", color(shown(fa, COMPARTMENT_FILL))));
    }
    if a.no_border && !b.no_border {
        p.push("border none".into());
    } else {
        let (ca, cb) = (
            eff(&a, a.border_color, Rgba::DEFAULT),
            eff(&b, b.border_color, Rgba::DEFAULT),
        );
        let (wa, wb) = (eff(&a, a.border_width, 0.0), eff(&b, b.border_width, 0.0));
        let back = !a.no_border && b.no_border;
        let mut parts = Vec::new();
        if ca != cb || back {
            parts.push(color(shown(ca, COMPARTMENT_BORDER)));
        }
        if wa != wb || back {
            parts.push(num(if wa == 0.0 {
                COMPARTMENT_BORDER_WIDTH
            } else {
                wa
            }));
        }
        if !parts.is_empty() {
            p.push(format!("border {}", parts.join(" ")));
        }
    }
    let (sa, sb) = (eff(&a, a.font_size, 0.0), eff(&b, b.font_size, 0.0));
    if sa != sb {
        p.push(format!(
            "idSize {}",
            num(if sa == 0.0 {
                COMPARTMENT_LABEL_SIZE
            } else {
                sa
            })
        ));
    }
    p
}

fn text_props(t: &TextItem, b: &TextItem) -> Vec<String> {
    let mut p = Vec::new();
    if t.size != b.size {
        p.push(format!("size {}", num(t.size)));
    }
    if t.font.typeface != b.font.typeface {
        p.push(
            match t.font.typeface {
                Typeface::Sans => "font sans",
                Typeface::Serif => "font serif",
            }
            .into(),
        );
    }
    for (name, on, base) in [
        ("bold", t.font.bold, b.font.bold),
        ("italic", t.font.italic, b.font.italic),
    ] {
        if on != base {
            p.push(if on {
                name.to_owned()
            } else {
                format!("{name} off")
            });
        }
    }
    if t.align != b.align {
        p.push(
            match t.align {
                TextAlign::Left => "align left",
                TextAlign::Center => "align center",
                TextAlign::Right => "align right",
            }
            .into(),
        );
    }
    if t.color != b.color {
        p.push(format!("color {}", color(shown(t.color, LABEL))));
    }
    p
}

fn modifier_props(l: &ModifierLook) -> Vec<String> {
    let mut p = Vec::new();
    let mut parts = Vec::new();
    if let Some(c) = l.colour {
        parts.push(color(c));
    }
    if let Some(w) = l.width {
        parts.push(num(w));
    }
    if let Some(k) = l.pattern.key() {
        parts.push(k.to_owned());
    }
    if !parts.is_empty() {
        p.push(format!("line {}", parts.join(" ")));
    }
    if let Some(s) = l.symbol_size {
        p.push(format!("symbol {}", num(s)));
    }
    if let Some(g) = l.symbol_gap {
        p.push(format!("distance {}", num(g)));
    }
    p
}

/// The side whose placement is exactly `o`, if any.
fn side_of(o: Pt, on: impl Fn(Pt) -> Pt) -> Option<&'static str> {
    use crate::geometry::pt;
    [
        ("above", pt(0.0, -1.0)),
        ("below", pt(0.0, 1.0)),
        ("left", pt(-1.0, 0.0)),
        ("right", pt(1.0, 0.0)),
    ]
    .into_iter()
    .find(|(_, d)| on(*d) == o)
    .map(|(n, _)| n)
}

fn kind_order(k: StyleKind) -> usize {
    StyleKind::ALL.iter().position(|x| *x == k).unwrap_or(0)
}

/// The ids a rate law names.
fn law_names(law: &str) -> Vec<String> {
    law.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .filter(|w| !w.is_empty())
        .map(str::to_owned)
        .collect()
}

/// A style's name as written: as it is if it is a word (an Antimony id),
/// else quoted (format §6.1).
fn style_ref(name: &str) -> String {
    let word = name.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_')
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    if word { name.to_owned() } else { string(name) }
}

/// A statement: its head, clauses and own settings.
fn statement(head: String, clauses: Vec<String>, props: Vec<String>) -> String {
    let mut s = head;
    for c in clauses {
        s.push(' ');
        s.push_str(&c);
    }
    if !props.is_empty() {
        s.push(' ');
        s.push_str(&block(&props));
    }
    s
}

fn block(props: &[String]) -> String {
    if props.is_empty() {
        "{ }".into()
    } else {
        format!("{{ {} }}", props.join("; "))
    }
}

/// The shortest form that reads back the same.
/// Rounded to a thousandth of a unit, far below anything drawn, and written
/// in the shortest form that reads back as that (format §12).
fn num(v: f64) -> String {
    let v = (v * 1000.0).round() / 1000.0;
    let v = if v == 0.0 { 0.0 } else { v };
    format!("{v}")
}

fn point(p: Pt) -> String {
    format!("({}, {})", num(p.x), num(p.y))
}

fn size_text(w: f64, h: f64) -> String {
    if w == h {
        num(w)
    } else {
        format!("{} x {}", num(w), num(h))
    }
}

fn color(c: Rgba) -> String {
    if c.is_transparent() {
        return "none".into();
    }
    colors::basic_name(c).map_or_else(|| colors::to_hex(c), str::to_owned)
}

/// A string the format can hold: one line, no `"`, and no `*/`, which would
/// end the diagram's comment.
fn string(text: &str) -> String {
    let one_line: String = text
        .chars()
        .map(|c| match c {
            '\n' | '\r' => ' ',
            '"' => '\'',
            c => c,
        })
        .collect();
    format!("\"{}\"", one_line.replace("*/", "* /"))
}

#[cfg(test)]
mod tests {
    use crate::antimony::{ImportOptions, export_with_diagram, import};
    use crate::geometry::pt;
    use crate::model::{Model, ReactionMode};

    const OPTS: ImportOptions = ImportOptions {
        convert_catalytic: false,
        seed: 1,
    };

    const MODEL: &str = "model m
  species $Glc, G6P, $ATP, $ADP, $E
  hk: Glc + ATP -> G6P + ADP; E*k*Glc*ATP
  out: G6P -> ; k*G6P
  k = 1
end
";

    /// The model with `diagram` read, written, and only the diagram kept.
    fn rewritten(diagram: &str) -> (Model, String) {
        let text = format!("{MODEL}/* diagram m v1\n{diagram}\nend */\n");
        let m = import(&text, &OPTS).unwrap().model;
        let out = export_with_diagram(&m);
        let at = out.find("/* diagram").expect("a diagram");
        (m, out[at..].to_owned())
    }

    /// Writing then reading gives the same drawing, and the same text again.
    fn round_trip(m: &Model) -> Model {
        let text = export_with_diagram(m);
        let back = import(&text, &OPTS).unwrap();
        assert!(back.warnings.is_empty(), "{:?}\n{text}", back.warnings);
        assert_eq!(
            export_with_diagram(&back.model),
            text,
            "written again the same"
        );
        back.model
    }

    #[test]
    fn a_small_diagram_in_the_canonical_form() {
        let (_, d) = rewritten(
            "style metabolite { shape circle; size 24; fill #FFD54F; id right }
style flux reaction { line navy 2.5 }
Glc at (100, 50) style metabolite
G6P at (100, 250) style metabolite
ATP at (200, 100)
ATP#2 at (220, 120)
ADP at (200, 200) locked
E at (300, 150) { fill none; border red 2; idBold }
hk at (100, 150) curved stem (0, -10) (0, 10) uses ATP#2 style flux { id below }
leg Glc -> hk curve (100, 80) (100, 120)
leg hk -> ADP via (150, 200)
out at (100, 300) sink (200, 300) { emptySetSize 18 }",
        );
        // The style puts ids on the clearest side: G6P's is its right, so
        // only Glc, whose clearest side is above, says where its id goes.
        assert_eq!(
            d,
            "/* diagram m v1
  style metabolite { shape circle; size 24; fill #FFD54F; id outside }
  style flux reaction { line navy 2.5 }
  Glc at (100, 50) style metabolite { id right }
  G6P at (100, 250) style metabolite
  ATP at (200, 100)
  ATP#2 at (220, 120)
  ADP at (200, 200) locked
  E at (300, 150) { fill none; border red 2; idBold }
  hk at (100, 150) curved stem (0, -10) (0, 10) uses ATP#2 style flux { id below }
  leg Glc -> hk curve (100, 80) (100, 120)
  leg hk -> ADP via (150, 200)
  out at (100, 300) sink (200, 300) { emptySetSize 18 }
end */
"
        );
    }

    #[test]
    fn a_junction_where_the_reader_puts_it_anyway_isnt_written() {
        let (_, d) = rewritten(
            "Glc at (0, 0)
ATP at (0, 100)
G6P at (200, 0)
ADP at (200, 100)
out at (300, 300)",
        );
        assert!(
            !d.contains("hk at") && d.contains("out at (300, 300)"),
            "{d}"
        );
    }

    #[test]
    fn what_is_read_back_is_what_was_written() {
        let (m, _) = rewritten(
            "style metabolite { shape circle; size 24; fill #FFD54F; id right }
Glc at (100, 50) style metabolite
G6P at (100, 250) style metabolite { id offset (30, -5) }
ATP at (200, 100)
ATP#2 at (220, 120)
ADP at (200, 200)
E at (300, 150)
hk at (100, 150) curved smooth uses ATP#2 { id shown }
leg Glc -> hk curve (100, 80) (100, 120)
line E to hk via (300, 100) { line red 3 solid }
out at (100, 300)",
        );
        let back = round_trip(&m);
        for s in m.species() {
            let b = back.species().iter().find(|b| {
                b.primary_id() == s.primary_id()
                    && b.is_alias() == s.is_alias()
                    && b.is_null_node == s.is_null_node
            });
            let b = b.unwrap_or_else(|| panic!("{} missing", s.id));
            assert_eq!(
                (
                    b.center,
                    b.width,
                    b.shape,
                    b.label_offset,
                    b.style,
                    &b.style_name
                ),
                (
                    s.center,
                    s.width,
                    s.shape,
                    s.label_offset,
                    s.style,
                    &s.style_name
                ),
                "{}",
                s.id
            );
        }
        for r in m.reactions() {
            let b = back.find_reaction(&r.id).unwrap();
            assert_eq!(
                (
                    b.junction,
                    b.mode,
                    b.junction_smooth,
                    b.label_offset,
                    b.style,
                    b.stem
                ),
                (
                    r.junction,
                    r.mode,
                    r.junction_smooth,
                    r.label_offset,
                    r.style,
                    r.stem
                ),
                "{}",
                r.id
            );
            for (p, q) in r
                .reactants
                .iter()
                .chain(&r.products)
                .zip(b.reactants.iter().chain(&b.products))
            {
                assert_eq!(
                    (p.ctrl_pts_set, p.ctrl1, p.ctrl2, &p.points),
                    (q.ctrl_pts_set, q.ctrl1, q.ctrl2, &q.points)
                );
            }
            for (p, q) in r.modifiers.iter().zip(&b.modifiers) {
                assert_eq!((p.kind, &p.points, p.look), (q.kind, &q.points, q.look));
            }
        }
    }

    #[test]
    fn under_a_theme_only_what_differs_from_it() {
        let (m, d) = rewritten(
            "theme escher\nGlc at (0, 0)\nG6P at (200, 0)\nATP at (0, 100)\nADP at (200, 100)\nE at (100, 200)",
        );
        assert!(d.contains("theme escher") && !d.contains("style"), "{d}");
        assert!(!d.contains("curved") && !d.contains("idBold"), "{d}");
        // A flag the theme turns on, switched off; a smooth junction made
        // sharp.
        let mut m2 = m.clone();
        m2.find_species_mut("Glc").unwrap().style.label_bold = false;
        let hk = m2.find_reaction_mut("hk").unwrap();
        hk.junction_smooth = false;
        let back = round_trip(&m2);
        let text = export_with_diagram(&m2);
        assert!(
            text.contains("idBold off") && text.contains("smooth off"),
            "{text}"
        );
        assert!(!back.find_species("Glc").unwrap().style.label_bold);
        let hk = back.find_reaction("hk").unwrap();
        assert!(hk.mode == ReactionMode::Bezier && !hk.junction_smooth);
        // The diagram's own version of a theme style replaces the theme's.
        let mut m3 = m.clone();
        let st = m3
            .styles
            .iter_mut()
            .find(|s| s.name == crate::themes::ESCHER_METABOLITE)
            .unwrap();
        if let crate::model::Look::Species { style, .. } = &mut st.look {
            style.fill_color = crate::model::Rgba::rgb(0, 0, 128);
        }
        let look = st.look;
        for id in crate::styles::users(
            &m3,
            crate::model::StyleKind::Species,
            crate::themes::ESCHER_METABOLITE,
        ) {
            crate::styles::give_look(&mut m3, &id, &look);
        }
        let text = export_with_diagram(&m3);
        assert!(text.contains("style \"Escher metabolite\" {"), "{text}");
        // Its items say nothing of their own: the style says it.
        assert!(text.contains("  Glc at (0, 0)\n"), "{text}");
        let back = round_trip(&m3);
        let glc = back.find_species("Glc").unwrap();
        assert_eq!(glc.style.fill_color, crate::model::Rgba::rgb(0, 0, 128));
        assert_eq!(
            glc.style_name.as_deref(),
            Some(crate::themes::ESCHER_METABOLITE)
        );
        // A replaced reaction style keeps the theme's hidden junctions; a
        // name the theme shows can be hidden.
        let mut m4 = m.clone();
        let st = m4
            .styles
            .iter_mut()
            .find(|s| s.name == crate::themes::ESCHER_REACTION)
            .unwrap();
        if let crate::model::Look::Reaction { style } = &mut st.look {
            assert!(!style.junction_visible);
            style.line_color = crate::model::Rgba::rgb(0, 128, 0);
        }
        let look = st.look;
        for id in crate::styles::users(
            &m4,
            crate::model::StyleKind::Reaction,
            crate::themes::ESCHER_REACTION,
        ) {
            crate::styles::give_look(&mut m4, &id, &look);
        }
        m4.find_reaction_mut("hk").unwrap().label_offset = None;
        let text = export_with_diagram(&m4);
        assert!(
            text.contains("junction hidden") && text.contains("id hidden"),
            "{text}"
        );
        let back = round_trip(&m4);
        let hk = back.find_reaction("hk").unwrap();
        assert!(!hk.style.junction_visible && hk.label_offset.is_none());
    }

    #[test]
    fn style_names_that_arent_words_are_quoted() {
        let (mut m, _) = rewritten("style metabolite { fill red }\nGlc at (0, 0) style metabolite");
        let st = m
            .styles
            .iter_mut()
            .find(|s| s.name == "metabolite")
            .unwrap();
        st.name = "My metabolites".into();
        m.find_species_mut("Glc").unwrap().style_name = Some("My metabolites".into());
        let text = export_with_diagram(&m);
        assert!(
            text.contains("style \"My metabolites\" {")
                && text.contains("style \"My metabolites\"\n"),
            "{text}"
        );
        let back = round_trip(&m);
        assert_eq!(
            back.find_species("Glc").unwrap().style_name.as_deref(),
            Some("My metabolites")
        );
    }

    #[test]
    fn modifier_lines_only_where_they_say_something_the_text_keeps() {
        let text = "model m
  species $Glc, G6P, $E, $C, $A
  hk: Glc -> G6P; E*C*k*Glc
  A -o hk
  k = 1
end
/* diagram m v1
line E to hk via (1, 2)
line C to hk kind catalyst
end */";
        let mut m = import(text, &OPTS).unwrap().model;
        let out = export_with_diagram(&m);
        assert!(out.contains("line E to hk via (1, 2)"), "{out}");
        assert!(out.contains("line C to hk kind catalyst"), "{out}");
        // The activator's kind is in the model (`A -o hk`), so no line.
        assert!(!out.contains("line A"), "{out}");
        // A modifier that is also a reactant can't come back through
        // Antimony, so its line isn't written.
        let hk = m.find_reaction_mut("hk").unwrap();
        let mut p = crate::model::Participant::new("Glc", 1.0);
        p.points = vec![pt(5.0, 5.0)];
        hk.modifiers.push(p);
        assert!(!export_with_diagram(&m).contains("line Glc"));
    }

    #[test]
    fn texts_keep_their_drawing_order_and_strings_stay_inside_the_comment() {
        let (mut m, _) = rewritten("text \"second\" at (0, 100)\ntext \"first\" at (0, 0)");
        assert_eq!(m.texts()[0].text, "second");
        let back = round_trip(&m);
        assert_eq!(back.texts()[0].text, "second");
        let id = m.texts()[0].id.clone();
        m.find_text_mut(&id).unwrap().text = "two\nlines with \"quotes\" and */".into();
        let text = export_with_diagram(&m);
        assert!(
            text.contains("text \"two lines with 'quotes' and * /\""),
            "{text}"
        );
        assert_eq!(text.matches("*/").count(), 1, "{text}");
    }

    #[test]
    fn with_antimonys_layout_statements_too() {
        let (m, _) = rewritten(
            "Glc at (0, 0)
G6P at (200, 0)
ATP at (0, 100)
ADP at (200, 100)
E at (100, 200)",
        );
        let text = crate::antimony::export_with_diagram_and_layout(&m);
        assert!(
            text.contains("model.layout = on") && text.contains("/* diagram m v1"),
            "{text}"
        );
        // The model and its diagram come before and after the statements as
        // in the plain export, and reading it back takes the diagram, quietly.
        let back = import(&text, &OPTS).unwrap();
        assert!(back.warnings.is_empty(), "{:?}", back.warnings);
        for s in m.species() {
            assert_eq!(
                back.model.find_species(&s.id).unwrap().center,
                s.center,
                "{}",
                s.id
            );
        }
        // What the export can't keep, for the user: a text's line breaks,
        // and with the statements, what they can't say.
        let mut m2 = m.clone();
        m2.add_text(
            pt(0.0, 0.0),
            "two
lines",
        );
        m2.find_species_mut("Glc").unwrap().label_offset = Some(pt(0.0, 30.0));
        let notes = crate::antimony::diagram_notes(&m2, false);
        assert!(
            notes.len() == 1 && notes[0].starts_with("1 text is written on one line"),
            "{notes:?}"
        );
        let notes = crate::antimony::diagram_notes(&m2, true);
        assert!(
            notes.iter().any(|n| n.starts_with("Names drawn outside")),
            "{notes:?}"
        );
        assert!(crate::antimony::diagram_notes(&m, false).is_empty());
    }

    #[test]
    fn numbers_in_their_shortest_form() {
        assert_eq!(super::num(100.0), "100");
        assert_eq!(super::num(0.1 + 0.2), "0.3");
        assert_eq!(super::num(81.03144262477514), "81.031");
        assert_eq!(super::num(56.65628709580387), "56.656");
        assert_eq!(super::num(-0.0004), "0");
        assert_eq!(super::num(2.0006), "2.001");
        assert_eq!(super::num(-0.0), "0");
        assert_eq!(super::point(pt(1.5, -2.0)), "(1.5, -2)");
        assert_eq!(super::size_text(24.0, 24.0), "24");
        assert_eq!(super::size_text(80.0, 36.0), "80 x 36");
        assert_eq!(super::color(crate::model::Rgba::rgb(0, 0, 128)), "navy");
        assert_eq!(super::color(crate::model::Rgba::rgb(1, 2, 3)), "#010203");
        assert_eq!(super::color(crate::model::Rgba::TRANSPARENT), "none");
    }

    /// Every Gallery model's diagram reads back without a warning about the
    /// diagram, and gives back every drawing in the same place.
    #[test]
    fn the_gallery_round_trips_without_diagram_warnings() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let index: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(root.join("Gallery/index.json")).unwrap(),
        )
        .unwrap();
        for e in index["examples"].as_array().unwrap() {
            let file = e["file"].as_str().unwrap();
            let text = std::fs::read_to_string(root.join(file)).unwrap();
            let model = if crate::escher::looks_like_escher(&text) {
                crate::escher::import(&text).unwrap().model
            } else {
                crate::json::from_json(&text).unwrap().model
            };
            let written = export_with_diagram(&model);
            let start = written.find("/* diagram").unwrap();
            let line_of = |w: &crate::antimony::Diagnostic| w.line;
            let first_diagram_line = written[..start].lines().count() + 1;
            let back = import(&written, &OPTS).unwrap();
            let in_diagram: Vec<String> = back
                .warnings
                .iter()
                .filter(|w| line_of(w) >= first_diagram_line)
                .map(|w| w.to_string())
                .collect();
            assert!(in_diagram.is_empty(), "{file}: {in_diagram:?}");
            assert_eq!(back.unplaced, 0, "{file}");
        }
    }
}
