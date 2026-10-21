//! Reading a diagram's look from SBML Render (docs/render-design.md §5):
//! which style applies to each glyph, and what PathwayDesigner look it gives.
//!
//! Render says where it can; PathwayDesigner's notes, read before this, give
//! the rest (§6). A value equal to the default PathwayDesigner would draw is
//! kept as "the default", so a theme applied later still works on it.

use std::collections::HashMap;

use sbml_io as sb;
use sbml_io::render as sr;

use crate::model::{
    BorderDash, LinePattern, Model, ModifierKind, NodeShape, ReactionMode, Rgba, SavedStyle,
    StyleKind, TextAlign, Typeface, VisualStyle,
};
use crate::render::constants::*;
use crate::styles;

/// What a glyph draws, found while the model was built.
pub(super) enum Target {
    Species(String),
    SpeciesName(String),
    Compartment(String),
    CompartmentName(String),
    Text(String),
    Reaction(String),
    ReactionName(String),
    /// A modifier line: the reaction, and the node the line comes from.
    Modifier(String, String),
}

/// Gives the model the look its layout's render information describes.
pub(super) fn apply(
    model: &mut Model,
    layout: &sb::Layout,
    global: Option<&sr::RenderList>,
    targets: &[(String, Target)],
) {
    let Some(r) = Resolver::new(layout, global) else {
        return;
    };
    let types = glyph_types(layout);
    let mut hidden_lines = (0, 0);
    for (glyph, target) in targets {
        let (typ, role) = types.get(glyph.as_str()).copied().unwrap_or(("ANY", None));
        let Some((style, at)) = r.style(glyph, typ, role) else {
            continue;
        };
        let Some(g) = style.group.as_ref() else {
            continue;
        };
        let look = Look { r: &r, at, g };
        match target {
            Target::Species(id) => species(model, id, &look),
            Target::SpeciesName(id) => {
                if let Some(s) = model.find_species_mut(id) {
                    name(&mut s.style, &look);
                }
            }
            Target::Compartment(id) => compartment(model, id, &look),
            Target::CompartmentName(id) => {
                if let Some(c) = model.find_compartment_mut(id)
                    && let Some(size) = look.font_size()
                {
                    set_width(
                        &mut c.style,
                        |s| &mut s.font_size,
                        size,
                        COMPARTMENT_LABEL_SIZE,
                    );
                }
            }
            Target::Text(id) => text(model, id, &look),
            Target::Reaction(id) => reaction(model, id, &look),
            Target::ReactionName(id) => {
                if let Some(rx) = model.find_reaction_mut(id) {
                    name(&mut rx.style, &look);
                }
            }
            Target::Modifier(rid, node) => {
                hidden_lines.1 += 1;
                if look.stroke().as_deref() == Some("none") {
                    hidden_lines.0 += 1;
                } else {
                    modifier(model, rid, node, &look);
                }
            }
        }
    }
    // Every modifier line written with no stroke: the diagram's lines are
    // turned off (S §7.6).
    if hidden_lines.1 > 0 && hidden_lines.0 == hidden_lines.1 {
        model.hide_modifier_lines = true;
    }
    // The first background along the chain (SBMLNetwork gives it on the
    // global render information only).
    if let Some(c) = r
        .infos
        .iter()
        .enumerate()
        .find_map(|(k, i)| r.colour(k, i.background_color.as_deref()?))
        && c != BACKGROUND
    {
        model.background = c;
    }
    saved_styles(model, &r, targets);
}

/// Each glyph's Render type and, for a species reference glyph, its role.
fn glyph_types(l: &sb::Layout) -> HashMap<&str, (&'static str, Option<&str>)> {
    let mut out = HashMap::new();
    for g in &l.species_glyphs {
        out.insert(g.id.as_str(), ("SPECIESGLYPH", None));
    }
    for g in &l.compartment_glyphs {
        out.insert(g.id.as_str(), ("COMPARTMENTGLYPH", None));
    }
    for g in &l.text_glyphs {
        out.insert(g.id.as_str(), ("TEXTGLYPH", None));
    }
    for g in &l.reaction_glyphs {
        out.insert(g.id.as_str(), ("REACTIONGLYPH", None));
        for s in &g.species_reference_glyphs {
            out.insert(
                s.id.as_str(),
                ("SPECIESREFERENCEGLYPH", s.role.as_ref().map(|r| r.as_str())),
            );
        }
    }
    out
}

/// The render information that applies, in the order styles are looked for:
/// the layout's own, the global one it refers to, then the first global one.
pub(super) struct Resolver<'a> {
    infos: Vec<&'a sr::RenderInformation>,
}

impl<'a> Resolver<'a> {
    pub(super) fn new(layout: &'a sb::Layout, global: Option<&'a sr::RenderList>) -> Option<Self> {
        let globals: &[sr::RenderInformation] = global.map_or(&[], |g| &g.items);
        let mut infos: Vec<&sr::RenderInformation> = Vec::new();
        if let Some(local) = layout.render.as_ref().and_then(|l| l.items.first()) {
            infos.push(local);
        }
        // References, followed once each.
        while let Some(next) = infos
            .last()
            .and_then(|i| i.reference_render_information.as_deref())
            .and_then(|id| globals.iter().find(|g| g.id == id))
            .filter(|g| !infos.iter().any(|i| std::ptr::eq(*i, *g)))
        {
            infos.push(next);
        }
        if let Some(first) = globals.first()
            && !infos.iter().any(|i| std::ptr::eq(*i, first))
        {
            infos.push(first);
        }
        (!infos.is_empty()).then_some(Resolver { infos })
    }

    /// The style for a glyph: in the first render information that has one,
    /// a style naming its id, else its role, else its type. With the index
    /// of the render information it came from.
    pub(super) fn style(
        &self,
        glyph: &str,
        typ: &str,
        role: Option<&str>,
    ) -> Option<(&'a sr::Style, usize)> {
        for (k, info) in self.infos.iter().enumerate() {
            let by = |f: &dyn Fn(&sr::Style) -> bool| info.styles.iter().find(|s| f(s));
            let found = by(&|s| s.id_list.iter().any(|i| i == glyph))
                .or_else(|| role.and_then(|r| by(&|s| s.role_list.iter().any(|x| x == r))))
                .or_else(|| by(&|s| s.type_list.iter().any(|t| t == typ || t == "ANY")));
            if let Some(s) = found {
                return Some((s, k));
            }
        }
        None
    }

    /// A colour as written: `none`, a value, or the id of a colour (or of a
    /// gradient, given as its colour nearest the middle) defined in the
    /// render information `at` or one it falls back on.
    pub(super) fn colour(&self, at: usize, value: &str) -> Option<Rgba> {
        let value = value.trim();
        if value.eq_ignore_ascii_case("none") {
            return Some(Rgba::TRANSPARENT);
        }
        if value.starts_with('#') {
            return parse_hex(value);
        }
        for info in &self.infos[at..] {
            if let Some(c) = info.color_definitions.iter().find(|c| c.id == value) {
                return parse_hex(&c.value);
            }
            if let Some(g) = info.gradients.iter().find(|g| g.id == value) {
                let middle = g.stops.iter().min_by(|a, b| {
                    let d = |s: &sr::GradientStop| {
                        let off = s
                            .offset
                            .as_ref()
                            .and_then(|o| o.resolve(100.0))
                            .unwrap_or(0.0);
                        (off - 50.0).abs()
                    };
                    d(a).total_cmp(&d(b))
                })?;
                return self.colour(at, middle.stop_color.as_deref()?);
            }
        }
        None
    }

    fn ending(&self, at: usize, id: &str) -> Option<&'a sr::LineEnding> {
        self.infos[at..]
            .iter()
            .find_map(|i| i.line_endings.iter().find(|e| e.id == id))
    }
}

/// `#RRGGBB`, `#RRGGBBAA` or `#RGB`.
fn parse_hex(s: &str) -> Option<Rgba> {
    let h = s.trim().strip_prefix('#')?;
    let byte = |i: usize| u8::from_str_radix(h.get(i..i + 2)?, 16).ok();
    match h.len() {
        6 => Some(Rgba([byte(0)?, byte(2)?, byte(4)?, 255])),
        8 => Some(Rgba([byte(0)?, byte(2)?, byte(4)?, byte(6)?])),
        3 => {
            let d = |i: usize| {
                u8::from_str_radix(h.get(i..i + 1)?, 16)
                    .ok()
                    .map(|v| v * 17)
            };
            Some(Rgba([d(0)?, d(1)?, d(2)?, 255]))
        }
        _ => None,
    }
}

/// A style's group, as one glyph sees it.
struct Look<'r, 'a> {
    r: &'r Resolver<'a>,
    at: usize,
    g: &'a sr::Group,
}

impl Look<'_, '_> {
    /// The first shape the group draws (a rectangle, ellipse or polygon).
    fn shape(&self) -> Option<&sr::Primitive> {
        self.g.children.iter().find(|p| {
            matches!(
                p,
                sr::Primitive::Rectangle(_) | sr::Primitive::Ellipse(_) | sr::Primitive::Polygon(_)
            )
        })
    }

    /// An attribute of the shape, else of the group.
    fn attr<T: Clone>(&self, f: impl Fn(&sr::Presentation) -> Option<T>) -> Option<T> {
        let own = self.shape().and_then(|p| match p {
            sr::Primitive::Rectangle(r) => f(&r.presentation),
            sr::Primitive::Ellipse(e) => f(&e.presentation),
            sr::Primitive::Polygon(p) => f(&p.presentation),
            _ => None,
        });
        own.or_else(|| f(&self.g.presentation))
    }

    fn stroke(&self) -> Option<String> {
        self.attr(|p| p.stroke.clone())
    }

    fn colour_of(&self, value: Option<String>) -> Option<Rgba> {
        self.r.colour(self.at, value.as_deref()?)
    }

    fn fill(&self) -> Option<Rgba> {
        self.colour_of(self.attr(|p| p.fill.clone()))
    }

    /// The border: `None` if the style doesn't say; `Some(None)` for none.
    fn border(&self) -> Option<Option<(Rgba, f64)>> {
        let stroke = self.stroke()?;
        let width = self.attr(|p| p.stroke_width).unwrap_or(1.0);
        let colour = self.r.colour(self.at, &stroke)?;
        Some((!colour.is_transparent() && width > 0.0).then_some((colour, width)))
    }

    fn dashed(&self) -> bool {
        self.attr(|p| p.stroke_dasharray.clone()).is_some()
    }

    /// The text's colour: its stroke, as Render has it, else its fill.
    fn text_colour(&self) -> Option<Rgba> {
        self.colour_of(self.g.presentation.stroke.clone())
            .filter(|c| !c.is_transparent())
            .or_else(|| self.colour_of(self.g.presentation.fill.clone()))
    }

    fn font_size(&self) -> Option<f64> {
        let (abs, rel) = self.g.presentation.font_size.as_ref()?.parse()?;
        (rel == 0.0 && abs > 0.0).then_some(abs)
    }

    fn bold(&self) -> Option<bool> {
        self.g
            .presentation
            .font_weight
            .as_deref()
            .map(|w| w == "bold")
    }

    fn italic(&self) -> Option<bool> {
        self.g
            .presentation
            .font_style
            .as_deref()
            .map(|w| w == "italic")
    }
}

/// A number as stored: rid of the last digits arithmetic adds (3.0 × 2.4 /
/// 2.4), so it reads back as written.
fn tidy(x: f64) -> f64 {
    (x * 1e9).round() / 1e9
}

/// Sets a colour field: the default sentinel if it is the default.
fn set_colour(
    style: &mut VisualStyle,
    f: impl Fn(&mut VisualStyle) -> &mut Rgba,
    c: Rgba,
    default: Rgba,
) {
    *f(style) = if c == default { Rgba::DEFAULT } else { c };
}

fn set_width(
    style: &mut VisualStyle,
    f: impl Fn(&mut VisualStyle) -> &mut f64,
    w: f64,
    default: f64,
) {
    let w = tidy(w);
    *f(style) = if (w - default).abs() < 1e-9 { 0.0 } else { w };
}

/// After setting fields: custom if any field is; otherwise as it was.
fn settle(style: &mut VisualStyle) {
    let plain = VisualStyle {
        junction_visible: style.junction_visible,
        no_border: style.no_border,
        ..VisualStyle::default()
    };
    let custom = VisualStyle {
        has_custom_style: false,
        ..*style
    } != plain;
    style.has_custom_style = custom;
}

/// A species' or reaction's name: colour, size, bold and italic.
fn name(style: &mut VisualStyle, look: &Look) {
    if let Some(c) = look.text_colour() {
        set_colour(style, |s| &mut s.label_color, c, LABEL);
    }
    if let Some(size) = look.font_size() {
        set_width(style, |s| &mut s.font_size, size, LABEL_FONT_SIZE);
    }
    if let Some(b) = look.bold() {
        style.label_bold = b;
    }
    if let Some(i) = look.italic() {
        style.label_italic = i;
    }
    settle(style);
}

fn species(model: &mut Model, id: &str, look: &Look) {
    let Some(s) = model.find_species_mut(id).filter(|s| !s.is_null_node) else {
        return;
    };
    let alias = s.is_alias();
    let (fill_default, border_default) = if alias {
        (ALIAS_FILL, ALIAS_BORDER)
    } else {
        (SPECIES_FILL, SPECIES_BORDER)
    };
    if let Some(fill) = look.fill() {
        set_colour(&mut s.style, |st| &mut st.fill_color, fill, fill_default);
    }
    match look.border() {
        Some(Some((colour, width))) => {
            s.style.no_border = false;
            set_colour(
                &mut s.style,
                |st| &mut st.border_color,
                colour,
                border_default,
            );
            let width = if s.is_boundary {
                width / BOUNDARY_BORDER_FACTOR
            } else {
                width
            };
            set_width(
                &mut s.style,
                |st| &mut st.border_width,
                width,
                DEFAULT_LINE_WIDTH,
            );
        }
        Some(None) => s.style.no_border = true,
        None => {}
    }
    settle(&mut s.style);
    // Dashed or not; the note, if it agrees, says whether by the alias rule.
    let dashed = look.dashed();
    let drawn = |d: BorderDash| match d {
        BorderDash::Automatic => alias,
        BorderDash::Solid => false,
        BorderDash::Dashed => true,
    };
    if drawn(s.border_dash) != dashed {
        s.border_dash = if dashed == alias {
            BorderDash::Automatic
        } else if dashed {
            BorderDash::Dashed
        } else {
            BorderDash::Solid
        };
    }
    if let Some(shape) = species_shape(look.shape(), s.shape, s.width, s.height) {
        s.shape = shape;
    }
}

/// The species shape a primitive draws, if it is one of PathwayDesigner's;
/// `current` (from the note) when it draws the same.
fn species_shape(
    p: Option<&sr::Primitive>,
    current: NodeShape,
    w: f64,
    h: f64,
) -> Option<NodeShape> {
    match p? {
        sr::Primitive::Rectangle(r) => {
            let radius = r.rx.as_ref().and_then(|v| v.resolve(w)).unwrap_or(0.0);
            Some(if radius > 0.0 {
                NodeShape::RoundedRect
            } else {
                NodeShape::Rectangle
            })
        }
        sr::Primitive::Ellipse(_) => Some(match current {
            NodeShape::Ellipse | NodeShape::Circle => current,
            _ if (w - h).abs() < 1e-9 => NodeShape::Circle,
            _ => NodeShape::Ellipse,
        }),
        sr::Primitive::Polygon(p) if is_hexagon(p) => Some(NodeShape::Hexagon),
        _ => None,
    }
}

/// PathwayDesigner's hexagon, to within 1 % of the box.
fn is_hexagon(p: &sr::Polygon) -> bool {
    const HEX: [(f64, f64); 6] = [
        (0.0, 50.0),
        (25.0, 0.0),
        (75.0, 0.0),
        (100.0, 50.0),
        (75.0, 100.0),
        (25.0, 100.0),
    ];
    p.elements.len() == 6
        && p.elements.iter().zip(HEX).all(|(e, (x, y))| {
            let at = |v: &Option<sb::render::RelAbs>| v.as_ref().and_then(|v| v.resolve(100.0));
            e.bezier.is_none()
                && at(&e.x).is_some_and(|v| (v - x).abs() <= 1.0)
                && at(&e.y).is_some_and(|v| (v - y).abs() <= 1.0)
        })
}

fn compartment(model: &mut Model, id: &str, look: &Look) {
    let Some(c) = model.find_compartment_mut(id) else {
        return;
    };
    if let Some(fill) = look.fill() {
        set_colour(&mut c.style, |s| &mut s.fill_color, fill, COMPARTMENT_FILL);
    }
    match look.border() {
        Some(Some((colour, width))) => {
            c.style.no_border = false;
            set_colour(
                &mut c.style,
                |s| &mut s.border_color,
                colour,
                COMPARTMENT_BORDER,
            );
            set_width(
                &mut c.style,
                |s| &mut s.border_width,
                width,
                COMPARTMENT_BORDER_WIDTH,
            );
        }
        Some(None) => c.style.no_border = true,
        None => {}
    }
    settle(&mut c.style);
}

fn text(model: &mut Model, id: &str, look: &Look) {
    let Some(t) = model.find_text_mut(id) else {
        return;
    };
    if let Some(c) = look.text_colour() {
        t.color = if c == LABEL { Rgba::DEFAULT } else { c };
    }
    if let Some(size) = look.font_size() {
        t.size = tidy(size);
    }
    if let Some(family) = look.g.presentation.font_family.as_deref() {
        let f = family.to_ascii_lowercase();
        t.font.typeface = if f.contains("serif") && !f.contains("sans") {
            Typeface::Serif
        } else {
            Typeface::Sans
        };
    }
    if let Some(b) = look.bold() {
        t.font.bold = b;
    }
    if let Some(i) = look.italic() {
        t.font.italic = i;
    }
    if let Some(a) = look.g.presentation.text_anchor.as_deref() {
        t.align = match a {
            "middle" => TextAlign::Center,
            "end" => TextAlign::Right,
            _ => TextAlign::Left,
        };
    }
}

fn reaction(model: &mut Model, id: &str, look: &Look) {
    let Some(rx) = model.find_reaction_mut(id) else {
        return;
    };
    if let Some(c) = look.colour_of(look.g.presentation.stroke.clone())
        && !c.is_transparent()
    {
        set_colour(&mut rx.style, |s| &mut s.line_color, c, REACTION_LINE);
    }
    if let Some(w) = look.g.presentation.stroke_width {
        set_width(&mut rx.style, |s| &mut s.line_width, w, DEFAULT_LINE_WIDTH);
    }
    // The junction dot: whatever the glyph's style draws (S §5.8).
    if rx.mode != ReactionMode::Linear {
        let dot = look.shape();
        rx.style.junction_visible = dot.is_some();
        let fill = match dot {
            Some(sr::Primitive::Ellipse(e)) => e.presentation.fill.clone(),
            Some(sr::Primitive::Rectangle(r)) => r.presentation.fill.clone(),
            _ => None,
        };
        if let Some(c) = look.colour_of(fill) {
            set_colour(&mut rx.style, |s| &mut s.junction_color, c, JUNCTION_FILL);
        }
    }
    settle(&mut rx.style);
}

fn modifier(model: &mut Model, rid: &str, node: &str, look: &Look) {
    let Some(rx) = model.find_reaction_mut(rid) else {
        return;
    };
    let Some(m) = rx.modifiers.iter_mut().find(|m| m.species == node) else {
        return;
    };
    let p = &look.g.presentation;
    if let Some(c) = look.colour_of(p.stroke.clone()) {
        m.look.colour = (c != MODIFIER).then_some(c);
    }
    let width = p.stroke_width.map(tidy);
    if let Some(w) = width {
        m.look.width = ((w - DEFAULT_LINE_WIDTH).abs() >= 1e-9).then_some(w);
    }
    let w = width.unwrap_or(DEFAULT_LINE_WIDTH);
    m.look.pattern = match dashes(p.stroke_dasharray.as_deref()) {
        None => LinePattern::Solid,
        Some(d) if d == [MODIFIER_DASH.0, MODIFIER_DASH.1] => LinePattern::Dashed,
        Some(d) if d.first().is_some_and(|&on| on <= w.round().max(1.0)) => LinePattern::Dotted,
        Some(_) => LinePattern::Dashed,
    };
    // The symbol's size and distance, from an ending that draws the kind's
    // symbol (S §5.7.2); the kind itself is the model's.
    let Some(e) = p
        .end_head
        .as_deref()
        .and_then(|id| look.r.ending(look.at, id))
    else {
        return;
    };
    let (Some(b), Some(shape)) = (
        e.bounding_box.as_ref(),
        e.group.as_ref().and_then(|g| g.children.first()),
    ) else {
        return;
    };
    let (x, w_box, h_box) = (b.position.x, b.dimensions.width, b.dimensions.height);
    let (size, gap) = match (m.kind, shape) {
        (ModifierKind::Modifier | ModifierKind::Catalyst, sr::Primitive::Ellipse(_))
        | (ModifierKind::Activator, sr::Primitive::Polygon(_)) => (w_box, -x - w_box),
        (ModifierKind::Inhibitor, sr::Primitive::Rectangle(_)) => (h_box, -(x + w_box / 2.0)),
        _ => return,
    };
    let (size, gap) = (tidy(size), tidy(gap));
    let (ds, dg) = m.kind.symbol_defaults();
    m.look.symbol_size = ((size - ds).abs() >= 1e-9).then_some(size);
    m.look.symbol_gap = ((gap - dg).abs() >= 1e-9).then_some(gap);
}

/// A dash array's numbers, or `None` if there is none.
fn dashes(s: Option<&str>) -> Option<Vec<f64>> {
    let v: Vec<f64> = s?
        .split([',', ' '])
        .filter_map(|t| t.trim().parse().ok())
        .collect();
    (!v.is_empty()).then_some(v)
}

/// A named style becomes a saved style of that name, followed by the items
/// its glyphs draw that look alike (§5.5).
fn saved_styles(model: &mut Model, r: &Resolver, targets: &[(String, Target)]) {
    let item = |glyph: &str| {
        targets.iter().find_map(|(g, t)| {
            (g == glyph).then(|| match t {
                Target::Species(id) => Some((StyleKind::Species, id.clone())),
                Target::Compartment(id) => Some((StyleKind::Compartment, id.clone())),
                Target::Text(id) => Some((StyleKind::Text, id.clone())),
                Target::Reaction(id) => Some((StyleKind::Reaction, id.clone())),
                _ => None,
            })?
        })
    };
    let named: Vec<&sr::Style> = r
        .infos
        .first()
        .map(|i| i.styles.iter().filter(|s| s.name.is_some()).collect())
        .unwrap_or_default();
    for s in named {
        let name = s.name.clone().expect("filtered");
        let items: Vec<(StyleKind, String)> = s.id_list.iter().filter_map(|g| item(g)).collect();
        let Some((kind, first)) = items.first().cloned() else {
            continue;
        };
        let Some(look) = styles::look_of(model, kind, &first) else {
            continue;
        };
        styles::put(
            &mut model.styles,
            SavedStyle {
                name: name.clone(),
                look,
            },
        );
        for (k, id) in items.into_iter().filter(|(k, _)| *k == kind) {
            if styles::look_of(model, k, &id).is_some_and(|l| styles::same_look(l, look)) {
                styles::set_link(model, k, &id, Some(name.clone()));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn style(id: &str, ids: &[&str], roles: &[&str], types: &[&str]) -> sr::Style {
        let v = |l: &[&str]| l.iter().map(|s| s.to_string()).collect();
        sr::Style {
            id: Some(id.into()),
            id_list: v(ids),
            role_list: v(roles),
            type_list: v(types),
            group: Some(sr::Group::default()),
            ..Default::default()
        }
    }

    fn info(id: &str, refers: Option<&str>, styles: Vec<sr::Style>) -> sr::RenderInformation {
        sr::RenderInformation {
            id: id.into(),
            reference_render_information: refers.map(str::to_string),
            styles,
            ..Default::default()
        }
    }

    fn found(r: &Resolver, glyph: &str, typ: &str, role: Option<&str>) -> Option<String> {
        r.style(glyph, typ, role).and_then(|(s, _)| s.id.clone())
    }

    #[test]
    fn a_style_is_found_by_id_then_role_then_type() {
        let l = sb::Layout {
            render: Some(sr::RenderList {
                items: vec![info(
                    "local",
                    None,
                    vec![
                        style(
                            "by_type",
                            &[],
                            &[],
                            &["SPECIESREFERENCEGLYPH", "SPECIESGLYPH"],
                        ),
                        style("by_role", &[], &["product"], &[]),
                        style("by_id", &["srg_1"], &[], &[]),
                        style("any", &[], &[], &["ANY"]),
                    ],
                )],
                ..Default::default()
            }),
            ..Default::default()
        };
        let r = Resolver::new(&l, None).unwrap();
        let typ = "SPECIESREFERENCEGLYPH";
        assert_eq!(
            found(&r, "srg_1", typ, Some("product")).as_deref(),
            Some("by_id")
        );
        assert_eq!(
            found(&r, "srg_2", typ, Some("product")).as_deref(),
            Some("by_role")
        );
        assert_eq!(
            found(&r, "srg_3", typ, Some("substrate")).as_deref(),
            Some("by_type")
        );
        assert_eq!(found(&r, "tg_1", "TEXTGLYPH", None).as_deref(), Some("any"));
    }

    #[test]
    fn local_render_information_comes_first_then_the_global_it_refers_to() {
        let l = sb::Layout {
            render: Some(sr::RenderList {
                items: vec![info(
                    "local",
                    Some("g2"),
                    vec![style("local_species", &["sg_A"], &[], &[])],
                )],
                ..Default::default()
            }),
            ..Default::default()
        };
        let global = sr::RenderList {
            items: vec![
                info("g1", None, vec![style("g1_any", &[], &[], &["ANY"])]),
                info(
                    "g2",
                    None,
                    vec![style("g2_species", &[], &[], &["SPECIESGLYPH"])],
                ),
            ],
            ..Default::default()
        };
        let r = Resolver::new(&l, Some(&global)).unwrap();
        assert_eq!(
            found(&r, "sg_A", "SPECIESGLYPH", None).as_deref(),
            Some("local_species")
        );
        // Not in the local one: the global it refers to, then the first.
        assert_eq!(
            found(&r, "sg_B", "SPECIESGLYPH", None).as_deref(),
            Some("g2_species")
        );
        assert_eq!(
            found(&r, "tg_B", "TEXTGLYPH", None).as_deref(),
            Some("g1_any")
        );
        // With no local one, the first global one (COPASI's files).
        let none = sb::Layout::default();
        let r = Resolver::new(&none, Some(&global)).unwrap();
        // g2 isn't referred to, so it isn't used; g1's ANY covers everything.
        assert_eq!(
            found(&r, "sg_B", "SPECIESGLYPH", None).as_deref(),
            Some("g1_any")
        );
        assert!(Resolver::new(&none, None).is_none());
    }

    #[test]
    fn colours_read_as_values_ids_and_gradients() {
        let mut ri = info("local", None, Vec::new());
        ri.color_definitions = vec![sr::ColorDefinition {
            id: "teal".into(),
            value: "#007777".into(),
            ..Default::default()
        }];
        let stop = |off: &str, c: &str| sr::GradientStop {
            offset: Some(sr::RelAbs(off.into())),
            stop_color: Some(c.into()),
            ..Default::default()
        };
        ri.gradients = vec![sr::Gradient {
            id: "fade".into(),
            name: None,
            spread_method: None,
            kind: sr::GradientKind::Linear {
                x1: None,
                y1: None,
                z1: None,
                x2: None,
                y2: None,
                z2: None,
            },
            stops: vec![
                stop("0%", "#FFFFFF"),
                stop("40%", "teal"),
                stop("100%", "#000000"),
            ],
            meta: Default::default(),
        }];
        let l = sb::Layout {
            render: Some(sr::RenderList {
                items: vec![ri],
                ..Default::default()
            }),
            ..Default::default()
        };
        let r = Resolver::new(&l, None).unwrap();
        assert_eq!(r.colour(0, "#FF8800"), Some(Rgba::rgb(0xFF, 0x88, 0x00)));
        assert_eq!(
            r.colour(0, "#ff880080"),
            Some(Rgba([0xFF, 0x88, 0x00, 0x80]))
        );
        assert_eq!(r.colour(0, "#F80"), Some(Rgba::rgb(0xFF, 0x88, 0x00)));
        assert_eq!(r.colour(0, "teal"), Some(Rgba::rgb(0, 0x77, 0x77)));
        assert_eq!(r.colour(0, "none"), Some(Rgba::TRANSPARENT));
        // A gradient: its stop nearest the middle.
        assert_eq!(r.colour(0, "fade"), Some(Rgba::rgb(0, 0x77, 0x77)));
        assert_eq!(r.colour(0, "nothing"), None);
        assert_eq!(r.colour(0, "#12345"), None);
    }
}
