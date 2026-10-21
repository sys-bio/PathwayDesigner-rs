//! The diagram's look as SBML Render (docs/render-design.md §3; S §11.5.1):
//! one local render information on the layout, with a style for every
//! glyph, the colours they use, and the line endings for arrowheads and
//! modifier symbols.
//!
//! The look written is the look drawn: each item's colours, widths, shape and
//! fonts as the drawing resolves them (its own style, else the default), with
//! every optional part shown, as in the default view. Glyphs that look the
//! same share a style, and a style for items that follow a saved style takes
//! its name: the style of the item's own glyph, not of its name or lines.

use sbml_io::render as sr;

use crate::model::{
    BorderDash, Compartment, Frame, LinePattern, Model, ModifierKind, NodeShape, Participant,
    Reaction, ReactionMode, Rgba, Role, SpeciesNode, TextAlign, TextFont, TextItem, Typeface,
    VisualStyle,
};
use crate::render::constants::*;
use crate::render::draw;

/// What a glyph draws.
pub(super) enum Item<'a> {
    Species(&'a SpeciesNode),
    SpeciesName(&'a SpeciesNode),
    Compartment(&'a Compartment, Frame),
    CompartmentName(&'a Compartment),
    Text(&'a TextItem),
    Reaction(&'a Reaction),
    ReactionName(&'a Reaction),
    Leg(&'a Reaction, Role, &'a Participant),
}

/// The render information's id.
pub(super) const ID: &str = "pd_render";

/// The render list for the glyphs of a layout, each named by its glyph id.
pub(super) fn render_list(model: &Model, glyphs: &[(String, Item)]) -> sr::RenderList {
    let mut w = Writer {
        model,
        colours: Vec::new(),
        endings: Vec::new(),
        styles: Vec::new(),
    };
    for (id, item) in glyphs {
        let (group, name) = w.look(item);
        match w
            .styles
            .iter_mut()
            .find(|s| s.group.as_ref() == Some(&group) && s.name == name)
        {
            Some(s) => s.id_list.push(id.clone()),
            None => {
                let n = w.styles.len() + 1;
                w.styles.push(sr::Style {
                    id: Some(format!("pd_style_{n}")),
                    name,
                    id_list: vec![id.clone()],
                    group: Some(group),
                    ..Default::default()
                });
            }
        }
    }
    // A value, not a colour id: every reader understands one.
    let background = hex(draw::background_colour(model));
    sr::RenderList {
        items: vec![sr::RenderInformation {
            id: ID.into(),
            program_name: Some("PathwayDesigner".into()),
            program_version: Some(env!("CARGO_PKG_VERSION").into()),
            background_color: Some(background),
            color_definitions: w
                .colours
                .iter()
                .map(|c| sr::ColorDefinition {
                    id: colour_id(*c),
                    value: hex(*c),
                    ..Default::default()
                })
                .collect(),
            line_endings: w.endings.into_iter().map(|(_, e)| e).collect(),
            styles: w.styles,
            ..Default::default()
        }],
        ..Default::default()
    }
}

struct Writer<'m> {
    model: &'m Model,
    /// The colours used, in order of first use.
    colours: Vec<Rgba>,
    /// The line endings, each with what it was made for.
    endings: Vec<(Ending, sr::LineEnding)>,
    styles: Vec<sr::Style>,
}

/// What a line ending draws.
#[derive(Clone, Copy, PartialEq)]
enum Ending {
    /// A product's filled arrowhead: length, half base, colour.
    Arrow(f64, f64, Rgba),
    /// A modifier's symbol: kind, size, distance from the junction, colour,
    /// line width, the background (open symbols' insides).
    Symbol(ModifierKind, f64, f64, Rgba, f64, Rgba),
}

fn hex(c: Rgba) -> String {
    let [r, g, b, a] = c.0;
    if a == 255 {
        format!("#{r:02X}{g:02X}{b:02X}")
    } else {
        format!("#{r:02X}{g:02X}{b:02X}{a:02X}")
    }
}

fn colour_id(c: Rgba) -> String {
    format!("c_{}", &hex(c)[1..])
}

fn abs(x: f64) -> Option<sr::RelAbs> {
    Some(sr::RelAbs::abs(x))
}

fn rel(p: f64) -> Option<sr::RelAbs> {
    Some(sr::RelAbs::rel(p))
}

/// A custom field, unless it is the "use the default" sentinel.
fn custom_colour(style: &VisualStyle, f: impl Fn(&VisualStyle) -> Rgba) -> Option<Rgba> {
    style
        .has_custom_style
        .then(|| f(style))
        .filter(|c| !c.is_default())
}

fn font(p: &mut sr::Presentation, f: TextFont, size: f64) {
    p.font_family = Some(
        match f.typeface {
            Typeface::Sans => "sans-serif",
            Typeface::Serif => "serif",
        }
        .into(),
    );
    p.font_size = abs(size);
    p.font_weight = Some(if f.bold { "bold" } else { "normal" }.into());
    p.font_style = Some(if f.italic { "italic" } else { "normal" }.into());
}

fn point(x: f64, y: f64) -> sr::RenderPoint {
    sr::RenderPoint {
        x: rel(x),
        y: rel(y),
        ..Default::default()
    }
}

fn full_ellipse() -> sr::Ellipse {
    sr::Ellipse {
        cx: rel(50.0),
        cy: rel(50.0),
        rx: rel(50.0),
        ry: rel(50.0),
        ..Default::default()
    }
}

fn full_rect(radius: f64) -> sr::Rectangle {
    sr::Rectangle {
        x: rel(0.0),
        y: rel(0.0),
        width: rel(100.0),
        height: rel(100.0),
        rx: (radius > 0.0).then(|| sr::RelAbs::abs(radius)),
        ry: (radius > 0.0).then(|| sr::RelAbs::abs(radius)),
        ..Default::default()
    }
}

impl Writer<'_> {
    /// The id of a colour, defined once; `none` for no paint at all.
    fn colour(&mut self, c: Rgba) -> String {
        if c.is_transparent() {
            return "none".into();
        }
        if !self.colours.contains(&c) {
            self.colours.push(c);
        }
        colour_id(c)
    }

    /// An item's look as a group of drawing attributes and primitives, and the
    /// saved style it follows.
    fn look(&mut self, item: &Item) -> (sr::Group, Option<String>) {
        let mut g = sr::Group::default();
        let name = match item {
            Item::Species(s) if s.is_null_node => {
                self.null_node(&mut g);
                None
            }
            Item::Species(s) => {
                self.species(s, &mut g);
                s.style_name.clone()
            }
            Item::SpeciesName(s) => {
                let colour = custom_colour(&s.style, |st| st.label_color).unwrap_or(LABEL);
                g.presentation.stroke = Some(self.colour(colour));
                font(
                    &mut g.presentation,
                    draw::name_font(&s.style),
                    draw::label_size(s),
                );
                g.presentation.text_anchor = Some("middle".into());
                g.presentation.vtext_anchor = Some("middle".into());
                None
            }
            Item::Compartment(c, f) => {
                let (fill, border, width) = draw::compartment_colours(c);
                let p = &mut g.presentation;
                p.fill = Some(self.colour(fill));
                p.stroke = Some(if width > 0.0 {
                    self.colour(border)
                } else {
                    "none".into()
                });
                p.stroke_width = Some(width);
                let radius = COMPARTMENT_CORNER_RADIUS
                    .min(f.width / 2.0)
                    .min(f.height / 2.0);
                g.children.push(sr::Primitive::Rectangle(full_rect(radius)));
                c.style_name.clone()
            }
            Item::CompartmentName(c) => {
                // The name takes a custom border colour; otherwise its own.
                let colour =
                    custom_colour(&c.style, |st| st.border_color).unwrap_or(COMPARTMENT_LABEL);
                g.presentation.stroke = Some(self.colour(colour));
                font(
                    &mut g.presentation,
                    TextFont::default(),
                    draw::compartment_label_size(c),
                );
                g.presentation.text_anchor = Some("middle".into());
                g.presentation.vtext_anchor = Some("middle".into());
                None
            }
            Item::Text(t) => {
                let colour = if t.color.is_default() { LABEL } else { t.color };
                g.presentation.stroke = Some(self.colour(colour));
                font(&mut g.presentation, t.font, t.size);
                g.presentation.text_anchor = Some(
                    match t.align {
                        TextAlign::Left => "start",
                        TextAlign::Center => "middle",
                        TextAlign::Right => "end",
                    }
                    .into(),
                );
                g.presentation.vtext_anchor = Some("top".into());
                t.style_name.clone()
            }
            Item::Reaction(r) => {
                self.reaction(r, &mut g);
                r.style_name.clone()
            }
            Item::ReactionName(r) => {
                let colour = draw::reaction_label_colour(r);
                g.presentation.stroke = Some(self.colour(colour));
                font(
                    &mut g.presentation,
                    draw::name_font(&r.style),
                    draw::reaction_label_size(r),
                );
                g.presentation.text_anchor = Some("middle".into());
                g.presentation.vtext_anchor = Some("middle".into());
                None
            }
            Item::Leg(r, role, p) => {
                self.leg(r, *role, p, &mut g);
                None
            }
        };
        (g, name)
    }

    fn species(&mut self, s: &SpeciesNode, g: &mut sr::Group) {
        // As drawn with the alias indicator on, the default (S §5.3).
        let (fill, border) = draw::species_colours(s, true);
        let mut width = draw::species_border_width(s);
        if s.is_boundary {
            width *= BOUNDARY_BORDER_FACTOR;
        }
        let dashed = match s.border_dash {
            BorderDash::Automatic => s.is_alias(),
            BorderDash::Solid => false,
            BorderDash::Dashed => true,
        };
        let p = &mut g.presentation;
        p.fill = Some(self.colour(fill));
        if width > 0.0 {
            p.stroke = Some(self.colour(border));
            p.stroke_width = Some(width);
            if dashed {
                p.stroke_dasharray = Some(format!("{},{}", ALIAS_DASH.0, ALIAS_DASH.1));
            }
        } else {
            p.stroke = Some("none".into());
        }
        g.children.push(match s.shape {
            NodeShape::RoundedRect => sr::Primitive::Rectangle(full_rect(NODE_CORNER_RADIUS)),
            NodeShape::Rectangle => sr::Primitive::Rectangle(full_rect(0.0)),
            NodeShape::Ellipse | NodeShape::Circle => sr::Primitive::Ellipse(full_ellipse()),
            NodeShape::Hexagon => sr::Primitive::Polygon(sr::Polygon {
                elements: [
                    (0.0, 50.0),
                    (25.0, 0.0),
                    (75.0, 0.0),
                    (100.0, 50.0),
                    (75.0, 100.0),
                    (25.0, 100.0),
                ]
                .into_iter()
                .map(|(x, y)| point(x, y))
                .collect(),
                ..Default::default()
            }),
        });
    }

    /// A grey circle with an × (S §5.4).
    fn null_node(&mut self, g: &mut sr::Group) {
        let (fill, border) = (self.colour(NULL_FILL), self.colour(NULL_BORDER));
        g.presentation.fill = Some(fill);
        g.presentation.stroke = Some(border.clone());
        g.presentation.stroke_width = Some(DEFAULT_LINE_WIDTH);
        g.children.push(sr::Primitive::Ellipse(full_ellipse()));
        // Arms reaching 55 % of the radius, as a percentage of the box.
        let a = 50.0 * NULL_CROSS_ARM * std::f64::consts::FRAC_1_SQRT_2;
        for (y0, y1) in [(50.0 - a, 50.0 + a), (50.0 + a, 50.0 - a)] {
            g.children.push(sr::Primitive::Curve(sr::RenderCurve {
                presentation: sr::Presentation {
                    stroke: Some(border.clone()),
                    stroke_width: Some(NULL_CROSS_WIDTH),
                    ..Default::default()
                },
                elements: vec![point(50.0 - a, y0), point(50.0 + a, y1)],
                ..Default::default()
            }));
        }
    }

    /// The reaction's line (its stem, if it has one) and its junction dot,
    /// which fills the glyph's box.
    fn reaction(&mut self, r: &Reaction, g: &mut sr::Group) {
        let line = self.colour(draw::line_colour(r));
        g.presentation.stroke = Some(line);
        g.presentation.stroke_width = Some(draw::reaction_line_width(r));
        if junction_drawn(r) {
            let fill = custom_colour(&r.style, |s| s.junction_color).unwrap_or(JUNCTION_FILL);
            let mut dot = full_ellipse();
            dot.presentation = sr::Presentation {
                fill: Some(self.colour(fill)),
                stroke: Some(self.colour(JUNCTION_BORDER)),
                stroke_width: Some(JUNCTION_BORDER_WIDTH),
                ..Default::default()
            };
            g.children.push(sr::Primitive::Ellipse(dot));
        }
    }

    fn leg(&mut self, r: &Reaction, role: Role, p: &Participant, g: &mut sr::Group) {
        let colour = draw::line_colour(r);
        if role != Role::Modifier {
            g.presentation.stroke = Some(self.colour(colour));
            g.presentation.stroke_width = Some(draw::reaction_line_width(r));
            let size = draw::ArrowSize::of(r);
            let arrow = Ending::Arrow(size.length, size.half_base, colour);
            match role {
                Role::Product => g.presentation.end_head = Some(self.ending(arrow)),
                // A reversible reaction's reactants get one too (S §5.5).
                _ if r.is_reversible => g.presentation.start_head = Some(self.ending(arrow)),
                _ => {}
            }
            return;
        }
        // Modifier lines turned off for this diagram aren't drawn (S §7.6).
        if self.model.hide_modifier_lines {
            g.presentation.stroke = Some("none".into());
            return;
        }
        let look = &p.look;
        let colour = look.colour.unwrap_or(MODIFIER);
        let width = look.width.unwrap_or(DEFAULT_LINE_WIDTH);
        let (size, gap) = p.kind.symbol_defaults();
        let (size, gap) = (
            look.symbol_size.unwrap_or(size),
            look.symbol_gap.unwrap_or(gap),
        );
        g.presentation.stroke = Some(self.colour(colour));
        g.presentation.stroke_width = Some(width);
        g.presentation.stroke_dasharray = match look.pattern {
            LinePattern::Dashed => Some(format!("{},{}", MODIFIER_DASH.0, MODIFIER_DASH.1)),
            // Render has no round caps: short dashes, as long as the line is
            // wide (S §5.7.2).
            LinePattern::Dotted => Some(format!(
                "{},{}",
                (width.round() as i64).max(1),
                ((2.5 * width).round() as i64).max(1)
            )),
            LinePattern::Solid => None,
        };
        let back = draw::background_colour(self.model);
        g.presentation.end_head =
            Some(self.ending(Ending::Symbol(p.kind, size, gap, colour, width, back)));
    }

    /// The id of a line ending, made once.
    fn ending(&mut self, e: Ending) -> String {
        if let Some((_, le)) = self.endings.iter().find(|(k, _)| *k == e) {
            return le.id.clone();
        }
        let n = self.endings.len() + 1;
        let le = match e {
            // The tip at the end of the line, at the node's border.
            Ending::Arrow(length, half, colour) => {
                let fill = self.colour(colour);
                ending(
                    format!("pd_arrow_{n}"),
                    (-length, -half, length, 2.0 * half),
                    sr::Presentation {
                        fill: Some(fill),
                        stroke: Some("none".into()),
                        ..Default::default()
                    },
                    triangle(),
                )
            }
            // The line ends at the junction; the symbol's nearest point is
            // `gap` before it (S §5.7.2).
            Ending::Symbol(kind, size, gap, colour, width, back) => {
                let (c, b) = (self.colour(colour), self.colour(back));
                let solid = |fill: String| sr::Presentation {
                    fill: Some(fill),
                    stroke: Some("none".into()),
                    ..Default::default()
                };
                let open = sr::Presentation {
                    fill: Some(b),
                    stroke: Some(c.clone()),
                    stroke_width: Some(width),
                    ..Default::default()
                };
                let circle = (-(gap + size), -size / 2.0, size, size);
                match kind {
                    ModifierKind::Modifier => ending(
                        format!("pd_dot_{n}"),
                        circle,
                        solid(c),
                        sr::Primitive::Ellipse(full_ellipse()),
                    ),
                    ModifierKind::Catalyst => ending(
                        format!("pd_circle_{n}"),
                        circle,
                        open,
                        sr::Primitive::Ellipse(full_ellipse()),
                    ),
                    ModifierKind::Activator => {
                        let half = size * ARROW_HALF_BASE / ARROW_LENGTH;
                        ending(
                            format!("pd_open_arrow_{n}"),
                            (-(gap + size), -half, size, 2.0 * half),
                            open,
                            triangle(),
                        )
                    }
                    ModifierKind::Inhibitor => {
                        let thick = width + 1.0;
                        ending(
                            format!("pd_bar_{n}"),
                            (-gap - thick / 2.0, -size / 2.0, thick, size),
                            solid(c),
                            sr::Primitive::Rectangle(full_rect(0.0)),
                        )
                    }
                }
            }
        };
        let id = le.id.clone();
        self.endings.push((e, le));
        id
    }
}

/// Whether a reaction's junction dot is drawn (S §5.8).
pub(super) fn junction_drawn(r: &Reaction) -> bool {
    draw::effective_mode(r) != ReactionMode::Linear && r.style.junction_visible
}

/// A triangle pointing along the line, filling its box.
fn triangle() -> sr::Primitive {
    sr::Primitive::Polygon(sr::Polygon {
        elements: vec![point(0.0, 0.0), point(100.0, 50.0), point(0.0, 100.0)],
        ..Default::default()
    })
}

fn ending(
    id: String,
    (x, y, w, h): (f64, f64, f64, f64),
    presentation: sr::Presentation,
    shape: sr::Primitive,
) -> sr::LineEnding {
    sr::LineEnding {
        id,
        bounding_box: Some(sbml_io::BoundingBox::new(x, y, w, h)),
        group: Some(sr::Group {
            presentation,
            children: vec![shape],
            ..Default::default()
        }),
        ..Default::default()
    }
}
