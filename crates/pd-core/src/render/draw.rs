//! The drawing passes (S §5.2), for Straight, Linear and Bézier reactions.
//!
//! The passes skip work the current options don't need, and never draw
//! anything that depends on the selection when drawing for output (S §11.6).

use crate::bezier::{Ctrl, leg_curve, leg_inner_end, leg_run};
use crate::geometry::{Pt, filled_arrowhead, normalize, pt, world_to_screen};
use crate::model::{
    BorderDash, Compartment, Frame, Id, LinePattern, Model, ModifierKind, NodeShape, Participant,
    Reaction, ReactionMode, Rgba, Role, SpeciesNode, TextFont, TextItem, VisualStyle,
};
use crate::render::constants::*;
use crate::render::{Renderer, SPt, SRect, Stroke, spt};
use crate::text::{font_metrics, font_metrics_in, text_width, text_width_in};
use crate::textbox::{lay_out, text_box};

/// What to draw and where.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DrawOptions {
    /// `screen = world × zoom + scroll` (S §5.1).
    pub zoom: f64,
    pub scroll: Pt,
    /// The screen area to fill with the background, or `None` to leave it.
    pub background: Option<SRect>,
    /// Leave out everything that depends on the current selection: halos, the
    /// thicker lines and larger junctions of selected items (S §11.6, D-21).
    pub for_output: bool,
    /// Draw aliases in purple with a dashed border (S §7.6). Default on.
    pub show_alias_indicator: bool,
    /// Draw null nodes and the legs that touch them (S §12). Default on.
    pub show_null_species: bool,
    /// Draw modifier arcs (S §7.6). Default on.
    pub show_modifiers: bool,
    /// Draw an arrowhead at the reactants' end of a reversible reaction too
    /// (S §5.5, §7.6). Default on.
    pub show_reversible: bool,
    /// The colour of a selected reaction's lines, arrowheads and modifier
    /// arcs (a preference; selection accent by default).
    pub selected_reaction: Rgba,
    /// Draw the grid on the background (S §7.6). Never drawn for output.
    pub grid: bool,
    /// The current corner point, which the arrow keys move (S §5.7.1):
    /// drawn filled.
    pub current_corner: Option<Pt>,
}

impl Default for DrawOptions {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            scroll: Pt::ZERO,
            background: None,
            for_output: false,
            show_alias_indicator: true,
            show_null_species: true,
            show_modifiers: true,
            show_reversible: true,
            selected_reaction: SELECTED_REACTION,
            grid: false,
            current_corner: None,
        }
    }
}

impl DrawOptions {
    /// These options with the optional parts shown as `shown` says.
    pub fn with(self, shown: Shown) -> Self {
        Self {
            show_alias_indicator: shown.alias_indicator,
            show_null_species: shown.null_species,
            show_modifiers: shown.modifiers,
            show_reversible: shown.reversible,
            ..self
        }
    }
}

/// Which optional parts of the drawing are shown (S §7.6, §12): the view's
/// settings, which output follows too.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Shown {
    pub alias_indicator: bool,
    pub null_species: bool,
    pub modifiers: bool,
    /// Arrowheads at both ends of a reversible reaction.
    pub reversible: bool,
}

impl Shown {
    /// Everything shown: the defaults.
    pub const ALL: Shown = Shown {
        alias_indicator: true,
        null_species: true,
        modifiers: true,
        reversible: true,
    };
}

/// Labels smaller than this many screen points are not drawn: at very small
/// zoom they would only be smudges.
const MIN_LABEL_SIZE_PX: f64 = 2.0;

/// Distance of the padlock's centre from a locked node's top-right corner.
const LOCK_INSET: f64 = 9.0;

/// Draws the whole model.
pub fn draw_model(model: &Model, opts: &DrawOptions, r: &mut dyn Renderer) {
    let d = Drawer { model, opts };
    if let Some(bg) = opts.background {
        r.round_rect(bg, 0.0, Some(background_colour(model)), None);
        if opts.grid && !opts.for_output {
            d.grid(bg, r);
        }
    }
    // Compartments first, largest first, so nested ones are drawn on top.
    let mut framed: Vec<(&Compartment, Frame)> = model
        .compartments
        .iter()
        .filter_map(|c| c.drawn_frame().map(|f| (c, f)))
        .collect();
    framed.sort_by(|a, b| b.1.area().total_cmp(&a.1.area()));
    for (c, f) in &framed {
        d.compartment(c, f, r);
    }
    for reaction in model.reactions() {
        d.reaction_legs(reaction, r);
    }
    for reaction in model.reactions() {
        d.modifier_arcs(reaction, r);
    }
    for node in model.species() {
        d.species(node, r);
    }
    if !opts.for_output {
        for node in model.selected_species() {
            d.halo(node, r);
        }
    }
    for reaction in model.reactions() {
        d.junction(reaction, r);
    }
    for reaction in model.reactions() {
        d.reaction_name(reaction, r);
    }
    for text in model.texts() {
        d.free_text(text, r);
    }
    if !opts.for_output {
        for (c, f) in framed.iter().filter(|(c, _)| c.selected) {
            d.compartment_selection(c, f, r);
        }
        for text in model.selected_texts() {
            d.text_selection(text, r);
        }
        for reaction in model.selected_reactions() {
            d.reaction_name_outline(reaction, r);
            d.bezier_handles(reaction, r);
            d.stem_ends(reaction, r);
            d.corner_points(reaction, r);
            d.add_line_handle(reaction, r);
        }
    }
}

/// A part of the network's structure picked in the Structure tab, drawn over
/// the rest (docs/structural-analysis-design.md §6): a flux null-space
/// vector's reactions or a conservation law's species, each with its
/// coefficient. A view, never saved.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Highlight {
    /// Reactions by id, with their coefficients.
    pub reactions: Vec<(Id, f64)>,
    /// Species by primary id (every drawing of each is outlined), with
    /// their coefficients.
    pub species: Vec<(Id, f64)>,
}

/// A highlighted item with a positive coefficient (blue) or a negative one
/// (orange): a pair that reads in colour-blind vision.
pub const HIGHLIGHT_POSITIVE: Rgba = Rgba([0x1F, 0x6F, 0xD1, 0xFF]);
pub const HIGHLIGHT_NEGATIVE: Rgba = Rgba([0xE0, 0x7B, 0x00, 0xFF]);
/// How opaque the background laid over the rest of the drawing is.
const HIGHLIGHT_FADE_ALPHA: u8 = 200;

/// [`draw_model`], then `highlight` over it: everything faded under a veil of
/// the background, then the highlighted reactions redrawn in the highlight
/// colours (their species with them), or the highlighted species redrawn and
/// outlined; a coefficient other than 1 beside each.
pub fn draw_model_highlighted(
    model: &Model,
    opts: &DrawOptions,
    highlight: Option<&Highlight>,
    r: &mut dyn Renderer,
) {
    draw_model(model, opts, r);
    if let Some(h) = highlight.filter(|h| !h.reactions.is_empty() || !h.species.is_empty()) {
        Drawer { model, opts }.highlight(h, r);
    }
}

/// A coefficient's label: `×2`, `×1/2`, `×0.25`; none for 1.
fn coefficient_label(k: f64) -> Option<String> {
    let m = k.abs();
    if (m - 1.0).abs() < 1e-9 {
        return None;
    }
    Some(match crate::structure::as_fraction(m) {
        Some((n, 1)) => format!("×{n}"),
        Some((n, d)) => format!("×{n}/{d}"),
        None => format!("×{m:.3}"),
    })
}

/// The colour of a coefficient's highlight.
fn highlight_colour(k: f64) -> Rgba {
    if k < 0.0 {
        HIGHLIGHT_NEGATIVE
    } else {
        HIGHLIGHT_POSITIVE
    }
}

impl Drawer<'_> {
    fn highlight(&self, h: &Highlight, out: &mut dyn Renderer) {
        // The veil: over the canvas, or for output the drawing's extent.
        let veil = self.opts.background.unwrap_or_else(|| {
            let (min, max) = content_bounds(self.model);
            let (a, b) = (self.to_screen(min), self.to_screen(max));
            SRect { min: a, max: b }
        });
        let mut fade = background_colour(self.model);
        fade.0[3] = HIGHLIGHT_FADE_ALPHA;
        out.round_rect(veil, 0.0, Some(fade), None);

        let label = |at: Pt, k: f64, out: &mut dyn Renderer| {
            if let Some(t) = coefficient_label(k) {
                let font = TextFont {
                    bold: true,
                    ..TextFont::default()
                };
                out.text(
                    self.to_screen(at),
                    &t,
                    self.len(13.0),
                    font,
                    highlight_colour(k),
                );
            }
        };
        // A null-space vector's reactions, in its colours and thicker, with
        // the species they join.
        for (id, k) in &h.reactions {
            let Some(reaction) = self.model.find_reaction(id) else {
                continue;
            };
            let mut shown = reaction.clone();
            shown.selected = false;
            shown.style.has_custom_style = true;
            shown.style.line_color = highlight_colour(*k);
            shown.style.line_width = reaction_line_width(reaction) * 1.8;
            self.reaction_legs(&shown, out);
            for p in reaction.reactants.iter().chain(&reaction.products) {
                if let Some(node) = self.visible(&p.species) {
                    self.species(node, out);
                }
            }
            self.junction(&shown, out);
            // Its name too, if shown, so it can be read through the veil.
            self.reaction_name(reaction, out);
            label(
                effective_junction(self.model, reaction) + pt(0.0, -16.0),
                *k,
                out,
            );
        }
        // A conservation law's species: every drawing of each, outlined.
        for (id, k) in &h.species {
            for node in self
                .model
                .species()
                .iter()
                .filter(|s| s.primary_id() == id.as_str())
            {
                self.species(node, out);
                let pad = 5.0;
                out.round_rect(
                    self.rect(node.center, node.width / 2.0 + pad, node.height / 2.0 + pad),
                    self.len(6.0),
                    None,
                    Some(Stroke::solid(self.len(3.0), highlight_colour(*k))),
                );
                label(
                    node.center + pt(node.width / 2.0 + pad + 10.0, -node.height / 2.0 - pad),
                    *k,
                    out,
                );
            }
        }
    }
}

/// The colour the diagram is drawn on (S §8.8): its own, or the default.
pub fn background_colour(model: &Model) -> Rgba {
    pick(Some(model.background), BACKGROUND)
}

/// A selected species' four corner handles, at the corners of its
/// selection halo, clockwise from the top left (S §7.10).
pub fn species_corners(s: &SpeciesNode) -> [Pt; 4] {
    let (hw, hh) = (s.width / 2.0 + HALO_OUTSET, s.height / 2.0 + HALO_OUTSET);
    let c = s.center;
    [
        pt(c.x - hw, c.y - hh),
        pt(c.x + hw, c.y - hh),
        pt(c.x + hw, c.y + hh),
        pt(c.x - hw, c.y + hh),
    ]
}

/// The width that fits a species' name: the label, as drawn, and
/// [`FIT_MARGIN`] each side (S §7.10).
pub fn width_for_name(s: &SpeciesNode) -> f64 {
    text_width_in(name_font(&s.style), s.primary_id(), label_size(s)) + 2.0 * FIT_MARGIN
}

/// The face a species' or reaction's name is drawn in (S §5.11.1): the
/// bundled sans-serif, bold and italic as its style says.
pub fn name_font(style: &VisualStyle) -> TextFont {
    custom(style).map_or_else(TextFont::default, |s| TextFont {
        bold: s.label_bold,
        italic: s.label_italic,
        ..TextFont::default()
    })
}

/// The size a species' name is drawn at.
pub fn label_size(s: &SpeciesNode) -> f64 {
    pick_width(custom(&s.style).map(|st| st.font_size), LABEL_FONT_SIZE)
}

/// The box round a species' name drawn outside its node, with
/// [`LABEL_BOX_PAD`] each side: what selects the name, and what is outlined
/// while the species is selected (S §5.11). `None` for a name drawn inside,
/// and for a null node. Returns (min, max).
pub fn outside_label_box(s: &SpeciesNode) -> Option<(Pt, Pt)> {
    let offset = s.label_offset.filter(|_| !s.is_null_node)?;
    let half = name_half(s.primary_id(), label_size(s), name_font(&s.style))
        + pt(LABEL_BOX_PAD, LABEL_BOX_PAD);
    let c = s.center + offset;
    Some((c - half, c + half))
}

/// Where a name taken outside its node starts when nothing is in the way:
/// just right of the node, [`LABEL_GAP`] away, level with its centre
/// (S §5.11).
pub fn default_label_offset(s: &SpeciesNode) -> Pt {
    label_offset_towards(s, pt(1.0, 0.0))
}

/// Where a name taken outside its node starts (S §5.11): on the side of the
/// node clearest of its lines. Of eight directions (right first, then left,
/// above, below, then the corners), the one furthest from every line that
/// leaves the node towards a junction; right when the node has none. The
/// name's box is [`LABEL_GAP`] from the node's box that way.
pub fn clear_label_offset(model: &Model, s: &SpeciesNode) -> Pt {
    let id = s.id.as_str();
    let legs: Vec<Pt> = model
        .reactions()
        .iter()
        .filter(|r| {
            [&r.reactants, &r.products, &r.modifiers]
                .iter()
                .any(|list| list.iter().any(|p| p.species == id))
        })
        .filter_map(|r| {
            let d = effective_junction(model, r) - s.center;
            let len = d.x.hypot(d.y);
            (len > 1e-9).then(|| pt(d.x / len, d.y / len))
        })
        .collect();
    label_offset_towards(s, clearest_direction(&legs))
}

/// Of eight directions (right first, then left, above, below, then the
/// corners), the one whose nearest line is furthest away: the smallest
/// largest cosine with `lines`, unit directions; the first such on a tie,
/// so right when there are none.
fn clearest_direction(lines: &[Pt]) -> Pt {
    let h = std::f64::consts::FRAC_1_SQRT_2;
    let directions = [
        pt(1.0, 0.0),
        pt(-1.0, 0.0),
        pt(0.0, -1.0),
        pt(0.0, 1.0),
        pt(h, -h),
        pt(-h, -h),
        pt(h, h),
        pt(-h, h),
    ];
    // The direction whose nearest line is furthest away: the smallest
    // largest cosine; the first such in the list on a tie.
    let closeness = |d: &Pt| {
        lines
            .iter()
            .map(|l| l.x * d.x + l.y * d.y)
            .fold(f64::NEG_INFINITY, f64::max)
    };
    let mut best = directions[0];
    let mut best_close = closeness(&best);
    for d in &directions[1..] {
        let c = closeness(d);
        if c < best_close - 1e-9 {
            (best, best_close) = (*d, c);
        }
    }
    best
}

/// Where `s`'s name, at `offset` while it was `old_size` high, goes now that
/// it is [`label_size`]: moved away from the node (or towards it) by half
/// the change in the name's width and height, along the axes of the side
/// it is on (to the nearest eighth of a turn), so the gap between the name
/// and the node stays the same.
pub fn label_offset_resized(s: &SpeciesNode, offset: Pt, old: (f64, TextFont)) -> Pt {
    name_offset_resized(
        s.primary_id(),
        offset,
        old,
        (label_size(s), name_font(&s.style)),
    )
}

/// [`label_offset_resized`] for a reaction's name (S §5.12), from its
/// junction.
pub fn reaction_label_offset_resized(r: &Reaction, offset: Pt, old: (f64, TextFont)) -> Pt {
    name_offset_resized(
        &r.id,
        offset,
        old,
        (reaction_label_size(r), name_font(&r.style)),
    )
}

/// A name `text` at `offset`, drawn at size and in face `old` and now `new`,
/// moved by half the change in its width and height along the axes of its
/// side.
fn name_offset_resized(text: &str, offset: Pt, old: (f64, TextFont), new: (f64, TextFont)) -> Pt {
    let grow = name_half(text, new.0, new.1) - name_half(text, old.0, old.1);
    let eighth = std::f64::consts::FRAC_PI_4;
    let a = (offset.y.atan2(offset.x) / eighth).round() * eighth;
    // A cosine of 6e-17 is zero: the name is centred along that axis.
    let side = |v: f64| if v.abs() < 1e-9 { 0.0 } else { v.signum() };
    offset + pt(side(a.cos()) * grow.x, side(a.sin()) * grow.y)
}

/// The offset that puts `s`'s name beside its node in direction `d` (each
/// part -1, 0 or 1 in sign): the name's box [`LABEL_GAP`] from the node's
/// box along each axis `d` points along, centred along the others.
fn label_offset_towards(s: &SpeciesNode, d: Pt) -> Pt {
    beside(
        pt(s.width / 2.0, s.height / 2.0),
        name_half(s.primary_id(), label_size(s), name_font(&s.style)),
        d,
    )
}

/// Half the width and height of `text` drawn at `size` in `font`.
fn name_half(text: &str, size: f64, font: TextFont) -> Pt {
    let m = font_metrics_in(font, size);
    pt(
        text_width_in(font, text, size) / 2.0,
        (m.ascent - m.descent) / 2.0,
    )
}

/// The offset of a name `label_half` in half-size from the centre of a box
/// `node_half` in half-size, so the name is [`LABEL_GAP`] from the box in
/// direction `d` (each part -1, 0 or 1 in sign) along each axis `d` points
/// along, and centred along the others.
fn beside(node_half: Pt, label_half: Pt, d: Pt) -> Pt {
    let along = |v: f64, node_half: f64, label_half: f64| {
        if v.abs() < 1e-9 {
            0.0
        } else {
            v.signum() * (node_half + LABEL_GAP + label_half)
        }
    };
    pt(
        along(d.x, node_half.x, label_half.x),
        along(d.y, node_half.y, label_half.y),
    )
}

/// The size a reaction's name is drawn at (S §5.12).
pub fn reaction_label_size(r: &Reaction) -> f64 {
    pick_width(custom(&r.style).map(|st| st.font_size), LABEL_FONT_SIZE)
}

/// The colour a reaction's name is drawn in (S §5.12).
pub fn reaction_label_colour(r: &Reaction) -> Rgba {
    pick(custom(&r.style).map(|st| st.label_color), LABEL)
}

/// The box round a reaction's name, with [`LABEL_BOX_PAD`] each side: what
/// selects the reaction, and what is outlined while it is selected
/// (S §5.12). `None` when the name isn't shown. Returns (min, max).
pub fn reaction_label_box(model: &Model, r: &Reaction) -> Option<(Pt, Pt)> {
    let offset = r.label_offset?;
    let half = name_half(&r.id, reaction_label_size(r), name_font(&r.style))
        + pt(LABEL_BOX_PAD, LABEL_BOX_PAD);
    let c = effective_junction(model, r) + offset;
    Some((c - half, c + half))
}

/// The offset that puts `s`'s name beside its node on side `d` (a unit
/// axis: above, below, left or right), [`LABEL_GAP`] from the node's box and
/// centred on it, as the diagram format's `id above` and the others place it
/// (docs/diagram-format.md §6.2).
pub fn label_offset_on_side(s: &SpeciesNode, d: Pt) -> Pt {
    label_offset_towards(s, d)
}

/// The offset that puts `r`'s name beside its junction on side `d`, clear of
/// the junction's dot and the lines' width as [`clear_reaction_label_offset`]
/// keeps it.
pub fn reaction_label_offset_on_side(r: &Reaction, d: Pt) -> Pt {
    beside(
        reaction_label_clearance(r),
        name_half(&r.id, reaction_label_size(r), name_font(&r.style)),
        d,
    )
}

/// How far a reaction's name keeps from its junction, each way.
fn reaction_label_clearance(r: &Reaction) -> Pt {
    let clear = JUNCTION_RADIUS + reaction_line_width(r) / 2.0;
    pt(clear, clear)
}

/// Where a reaction's name starts when it is shown (S §5.12): beside its
/// junction on the side clearest of its lines, as for a species' name
/// (S §5.11). A line leaves the junction towards its handle on a Bézier
/// reaction, otherwise towards its species. The name keeps [`LABEL_GAP`]
/// from the junction's dot and the lines' width.
pub fn clear_reaction_label_offset(model: &Model, r: &Reaction) -> Pt {
    let j = effective_junction(model, r);
    let bezier = effective_mode(r) == ReactionMode::Bezier;
    let mut toward = Vec::new();
    // A stem's ends are where its lines leave the junction (S §5.13); a
    // side whose end is at the junction has its legs leave from there.
    let stem = stem_line(r);
    for role in [Role::Reactant, Role::Product, Role::Modifier] {
        if let Some(s) = stem {
            let end = if role == Role::Reactant { s[0] } else { s[2] };
            if role != Role::Modifier && end.distance(j) > 1e-9 {
                toward.push(end);
                continue;
            }
        }
        for (i, p) in r.participants(role).iter().enumerate() {
            // A leg with corner points leaves the junction towards the last
            // of them (S §5.6.5).
            if role != Role::Modifier
                && let Some(&last) = p.points.last()
            {
                toward.push(last);
                continue;
            }
            let curve = (bezier && role != Role::Modifier)
                .then(|| leg_curve(model, r, role, i))
                .flatten();
            let to = match (curve, role) {
                (Some(c), Role::Reactant) => Some(c.p2),
                (Some(c), _) => Some(c.p1),
                (None, _) => model.find_species(&p.species).map(|s| s.center),
            };
            toward.extend(to);
        }
    }
    let lines: Vec<Pt> = toward
        .into_iter()
        .filter_map(|t| {
            let d = t - j;
            let len = d.x.hypot(d.y);
            (len > 1e-9).then(|| pt(d.x / len, d.y / len))
        })
        .collect();
    beside(
        reaction_label_clearance(r),
        name_half(&r.id, reaction_label_size(r), name_font(&r.style)),
        clearest_direction(&lines),
    )
}

/// A compartment's four corner handles, clockwise from the top left
/// (S §7.9).
pub fn compartment_corners(f: &Frame) -> [Pt; 4] {
    let (a, b) = (f.min(), f.max());
    [a, pt(b.x, a.y), b, pt(a.x, b.y)]
}

/// The box of a compartment's name, inside its top-left corner: what
/// selects it, with its border (S §7.9).
pub fn compartment_label_box(c: &Compartment, f: &Frame) -> (Pt, Pt) {
    let size = compartment_label_size(c);
    let m = font_metrics(size);
    let w = text_width(&c.id, size);
    let a = f.min() + pt(COMPARTMENT_LABEL_INSET, COMPARTMENT_LABEL_INSET);
    (a, a + pt(w, m.ascent - m.descent))
}

/// The size a compartment's name (its id) is drawn at (S §7.9): its style's
/// font size, or 12.
pub fn compartment_label_size(c: &Compartment) -> f64 {
    pick_width(
        custom(&c.style).map(|s| s.font_size),
        COMPARTMENT_LABEL_SIZE,
    )
}

/// The fill, border colour and border width a compartment is drawn with; a
/// width of 0 for no border.
pub fn compartment_colours(c: &Compartment) -> (Rgba, Rgba, f64) {
    let style = custom(&c.style);
    let width = if c.style.no_border {
        0.0
    } else {
        pick_width(style.map(|s| s.border_width), COMPARTMENT_BORDER_WIDTH)
    };
    (
        pick(style.map(|s| s.fill_color), COMPARTMENT_FILL),
        pick(style.map(|s| s.border_color), COMPARTMENT_BORDER),
        width,
    )
}

/// Whether `node` is drawn as a boundary species, with a thicker border
/// (S §5.3): its primary's flag, which an alias draws too. An alias's own
/// copy of the flag isn't kept up to date, so it doesn't count.
pub fn is_boundary(model: &Model, node: &SpeciesNode) -> bool {
    match &node.alias_of {
        Some(p) => model.find_species(p).is_some_and(|p| p.is_boundary),
        None => node.is_boundary,
    }
}

/// The border width a species is drawn with, before a boundary species'
/// thicker border; 0 for no border (S §5.3).
pub fn species_border_width(s: &SpeciesNode) -> f64 {
    if s.style.no_border {
        0.0
    } else {
        pick_width(
            custom(&s.style).map(|st| st.border_width),
            DEFAULT_LINE_WIDTH,
        )
    }
}

/// The width a reaction's lines are drawn with, when it isn't selected.
pub fn reaction_line_width(r: &Reaction) -> f64 {
    pick_width(custom(&r.style).map(|s| s.line_width), DEFAULT_LINE_WIDTH)
}

/// How wide a selected reaction's line `width` wide is drawn (S §5.5):
/// 1.5 times, but at most 2 wider.
pub fn selected_line_width(width: f64) -> f64 {
    width + (width * (SELECTED_LINE_FACTOR - 1.0)).min(SELECTED_LINE_EXTRA_MAX)
}

/// A product leg's arrowhead and where the line ends under it (S §5.5).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ArrowSize {
    pub length: f64,
    pub half_base: f64,
    /// How far before the node's border the tip is.
    pub tip_inset: f64,
    /// How far before the node's border the line ends, under the arrowhead.
    pub line_gap: f64,
}

impl ArrowSize {
    /// The reference's arrowhead: for lines up to the default width.
    pub const STANDARD: ArrowSize = ArrowSize {
        length: ARROW_LENGTH,
        half_base: ARROW_HALF_BASE,
        tip_inset: ARROW_TIP_INSET,
        line_gap: PRODUCT_LINE_GAP,
    };

    /// The arrowhead for a line `width` wide: the standard one up to the
    /// default width; above it, every size times √(width / default), but
    /// never less than twice the line's width across (from about 16.7 wide),
    /// and the line ending halfway along the arrowhead, so a thick line's end
    /// stays inside it; or further back if its round end would show past the
    /// arrowhead's sides there (from about 14 wide).
    pub fn for_width(width: f64) -> ArrowSize {
        if width.is_nan() || width <= DEFAULT_LINE_WIDTH {
            return ArrowSize::STANDARD;
        }
        let k = (width / DEFAULT_LINE_WIDTH)
            .sqrt()
            .max(width / ARROW_HALF_BASE);
        let (length, half_base, tip_inset) =
            (ARROW_LENGTH * k, ARROW_HALF_BASE * k, ARROW_TIP_INSET * k);
        ArrowSize {
            length,
            half_base,
            tip_inset,
            line_gap: tip_inset + length / 2.0,
        }
        .under(width)
    }

    /// The same arrowhead over a line `width` wide: the line ends far enough
    /// back that its round end, radius width / 2, stays between the
    /// arrowhead's sides.
    pub fn under(self, width: f64) -> ArrowSize {
        let cap_inside = width / 2.0 * self.half_base.hypot(self.length) / self.half_base;
        ArrowSize {
            line_gap: self.line_gap.max(self.tip_inset + cap_inside),
            ..self
        }
    }

    /// The arrowheads of reaction `r`, from its own line width (not the
    /// wider one it is drawn with while selected, so they don't jump).
    pub fn of(r: &Reaction) -> ArrowSize {
        ArrowSize::for_width(reaction_line_width(r))
    }

    /// The arrowheads of reaction `r` as drawn, `selected` or not: the same
    /// size, over the line drawn.
    pub fn drawn(r: &Reaction, selected: bool) -> ArrowSize {
        let width = reaction_line_width(r);
        let size = ArrowSize::of(r);
        if selected {
            size.under(selected_line_width(width))
        } else {
            size
        }
    }
}

/// Where a selected text's wrap handle is: the middle of its box's right
/// edge (S §7.8).
pub fn wrap_handle(t: &TextItem) -> Pt {
    let (min, max) = text_box(t);
    pt(max.x, (min.y + max.y) / 2.0)
}

/// The world-space rectangle that output draws: every species rectangle and
/// junction, plus a 60-unit margin; 800 × 600 from the origin for an empty
/// model (S §11.6). Returns (min, max).
pub fn content_bounds(model: &Model) -> (Pt, Pt) {
    let mut min = pt(f64::INFINITY, f64::INFINITY);
    let mut max = pt(f64::NEG_INFINITY, f64::NEG_INFINITY);
    let mut grow = |p: Pt| {
        min = pt(min.x.min(p.x), min.y.min(p.y));
        max = pt(max.x.max(p.x), max.y.max(p.y));
    };
    for s in model.species() {
        let half = pt(s.width / 2.0, s.height / 2.0);
        grow(s.center - half);
        grow(s.center + half);
        if let Some((a, b)) = outside_label_box(s) {
            grow(a);
            grow(b);
        }
    }
    for r in model.reactions() {
        grow(r.junction);
        // A leg's corner points may be routed outside everything else
        // (S §5.6.5).
        for p in r.reactants.iter().chain(&r.products) {
            p.points.iter().for_each(|&q| grow(q));
        }
        if let Some((a, b)) = reaction_label_box(model, r) {
            grow(a);
            grow(b);
        }
    }
    for t in model.texts() {
        let (a, b) = text_box(t);
        grow(a);
        grow(b);
    }
    for f in model.compartments.iter().filter_map(|c| c.drawn_frame()) {
        grow(f.min());
        grow(f.max());
    }
    if min.x > max.x {
        return (Pt::ZERO, pt(800.0, 600.0));
    }
    let m = pt(CONTENT_MARGIN, CONTENT_MARGIN);
    (min - m, max + m)
}

/// How a reaction is actually drawn: Linear only applies to UniUni reactions,
/// and is drawn as Straight otherwise (S §5.5).
pub fn effective_mode(r: &Reaction) -> ReactionMode {
    match r.mode {
        ReactionMode::Linear if !r.is_uni_uni() => ReactionMode::Straight,
        mode => mode,
    }
}

/// A Linear reaction's junction stays at least this far outside both nodes'
/// borders, so the modifier dot next to it is never hidden inside a node.
pub const LINEAR_JUNCTION_CLEARANCE: f64 = 10.0;

/// Where a reaction's junction is drawn (S §5.5). For a Linear reaction it is
/// on the reactant–product line (see [`linear_junction_t`]); otherwise it is
/// the stored junction.
pub fn effective_junction(model: &Model, r: &Reaction) -> Pt {
    match linear_line(model, r) {
        Some((a, b)) => match linear_junction_t(model, r) {
            Some(t) => a + (b - a) * t,
            None => (a + b) / 2.0,
        },
        None => r.junction,
    }
}

/// The points a modifier's arc runs through (S §5.7.1): where it leaves the
/// modifier node's border, its corner points, then the junction. `None` when
/// its node is missing, or its first point (the junction, without corner
/// points) is on the node's centre.
pub fn modifier_route(model: &Model, r: &Reaction, m: &Participant) -> Option<Vec<Pt>> {
    let node = model.find_species(&m.species)?;
    let j = effective_junction(model, r);
    let first = m.points.first().copied().unwrap_or(j);
    if node.center.distance(first) < 1.0 {
        return None;
    }
    let mut route = Vec::with_capacity(m.points.len() + 2);
    route.push(border(node, first));
    route.extend(&m.points);
    route.push(j);
    Some(route)
}

/// A reaction's stem as drawn (S §5.13): from the reactants' end through
/// the junction to the products' end. `None` without one, and for a Linear
/// reaction, which is drawn as one line.
pub fn stem_line(r: &Reaction) -> Option<[Pt; 3]> {
    r.stem
        .filter(|_| effective_mode(r) != ReactionMode::Linear)?;
    Some([
        r.leg_anchor(Role::Reactant),
        r.junction,
        r.leg_anchor(Role::Product),
    ])
}

/// How far up and right of a selected reaction's junction its + handle is,
/// and its radius, in screen points (S §7.11).
pub const ADD_LINE_OFFSET_PX: f64 = 16.0;
pub const ADD_LINE_RADIUS_PX: f64 = 6.0;

/// Where a selected reaction's + handle is (S §7.11), at `zoom`: up and
/// right of its junction, the same distance on screen at any zoom. `None`
/// unless the reaction is selected.
pub fn add_line_handle(model: &Model, r: &Reaction, zoom: f64) -> Option<Pt> {
    r.selected
        .then(|| effective_junction(model, r) + pt(ADD_LINE_OFFSET_PX, -ADD_LINE_OFFSET_PX) / zoom)
}

/// A Linear reaction's reactant and product centres, or `None` for any other
/// reaction.
pub fn linear_line(model: &Model, r: &Reaction) -> Option<(Pt, Pt)> {
    if effective_mode(r) != ReactionMode::Linear {
        return None;
    }
    let a = model.find_species(&r.reactants[0].species)?.center;
    let b = model.find_species(&r.products[0].species)?.center;
    Some((a, b))
}

/// Where a Linear reaction's junction is along its line, from 0 at the
/// reactant's centre to 1 at the product's: the stored junction projected onto
/// the line, kept within [0.1, 0.9] (S §5.5) and at least 10 units outside both
/// nodes' borders. If the nodes are too close for that, halfway between their
/// borders. `None` if the centres are less than 1 apart, or the reaction isn't
/// Linear.
pub fn linear_junction_t(model: &Model, r: &Reaction) -> Option<f64> {
    let (a, b) = linear_line(model, r)?;
    let ab = b - a;
    let len = ab.length();
    if len < 1.0 {
        return None;
    }
    let t = ((r.junction - a).dot(ab) / (len * len)).clamp(0.1, 0.9);
    let na = model.find_species(&r.reactants[0].species)?;
    let nb = model.find_species(&r.products[0].species)?;
    let lo = (border(na, b).distance(a) + LINEAR_JUNCTION_CLEARANCE) / len;
    let hi = 1.0 - (border(nb, a).distance(b) + LINEAR_JUNCTION_CLEARANCE) / len;
    Some(if lo <= hi {
        t.clamp(lo, hi)
    } else {
        let lo = border(na, b).distance(a) / len;
        let hi = 1.0 - border(nb, a).distance(b) / len;
        (lo + hi) / 2.0
    })
}

struct Drawer<'a> {
    model: &'a Model,
    opts: &'a DrawOptions,
}

impl Drawer<'_> {
    fn to_screen(&self, p: Pt) -> SPt {
        let s = world_to_screen(p, self.opts.zoom, self.opts.scroll);
        spt(s.x as f32, s.y as f32)
    }

    /// A world-space length on screen.
    fn len(&self, l: f64) -> f32 {
        (l * self.opts.zoom) as f32
    }

    fn dash(&self, (on, off): (f64, f64)) -> Option<(f32, f32)> {
        Some((self.len(on), self.len(off)))
    }

    fn rect(&self, center: Pt, half_w: f64, half_h: f64) -> SRect {
        SRect {
            min: self.to_screen(center - pt(half_w, half_h)),
            max: self.to_screen(center + pt(half_w, half_h)),
        }
    }

    /// The node, unless it is missing or a null node that isn't shown.
    fn visible(&self, id: &str) -> Option<&SpeciesNode> {
        self.model
            .find_species(id)
            .filter(|s| self.opts.show_null_species || !s.is_null_node)
    }

    fn selected(&self, selected: bool) -> bool {
        selected && !self.opts.for_output
    }

    // ----- Pass 2: reaction legs and arrowheads (S §5.5) --------------------

    fn line_stroke(&self, r: &Reaction) -> Stroke {
        let mut color = line_colour(r);
        let mut width = reaction_line_width(r);
        if self.selected(r.selected) {
            width = selected_line_width(width);
            color = self.opts.selected_reaction;
        }
        Stroke {
            width: self.len(width),
            color,
            dash: None,
            round_cap: true,
        }
    }

    fn reaction_legs(&self, r: &Reaction, out: &mut dyn Renderer) {
        let stroke = self.line_stroke(r);
        if effective_mode(r) == ReactionMode::Linear {
            let (Some(a), Some(b)) = (
                self.visible(&r.reactants[0].species),
                self.visible(&r.products[0].species),
            ) else {
                return;
            };
            let size = ArrowSize::drawn(r, self.selected(r.selected));
            let start = if self.both_ends(r) {
                let (start, tip) = leg_end_sized(a, b.center, &size);
                self.arrowhead(tip, a.center - b.center, stroke.color, &size, out);
                start
            } else {
                leg_start(a, b.center)
            };
            let (end, tip) = leg_end_sized(b, a.center, &size);
            out.polyline(&[self.to_screen(start), self.to_screen(end)], stroke);
            self.arrowhead(tip, b.center - a.center, stroke.color, &size, out);
            return;
        }

        // The stem, if it has one (S §5.13): a straight line through the
        // junction.
        if let Some(stem) = stem_line(r) {
            let pts: Vec<SPt> = stem.iter().map(|&p| self.to_screen(p)).collect();
            out.polyline(&pts, stroke);
        }

        if effective_mode(r) == ReactionMode::Bezier {
            for (role, list) in [(Role::Reactant, &r.reactants), (Role::Product, &r.products)] {
                for (index, p) in list.iter().enumerate() {
                    let Some(node) = self.visible(&p.species) else {
                        continue;
                    };
                    let Some(curve) = leg_curve(self.model, r, role, index) else {
                        continue;
                    };
                    let size = ArrowSize::drawn(r, self.selected(r.selected));
                    // A reversible reaction's reactant leg ends in an
                    // arrowhead too: cut it as a product's, run backwards.
                    let leg = if role == Role::Reactant && self.both_ends(r) {
                        let back = crate::geometry::CubicBezier::new(
                            curve.p3, curve.p2, curve.p1, curve.p0,
                        );
                        crate::bezier::cut_leg_sized(&back, node, Role::Product, &size)
                    } else {
                        crate::bezier::cut_leg_sized(&curve, node, role, &size)
                    };
                    let c = leg.curve;
                    out.cubic(
                        self.to_screen(c.p0),
                        self.to_screen(c.p1),
                        self.to_screen(c.p2),
                        self.to_screen(c.p3),
                        stroke,
                    );
                    if let Some((tip, dir)) = leg.arrow {
                        self.arrowhead(tip, dir, stroke.color, &size, out);
                    }
                    self.leg_run(r, role, p, stroke, out);
                }
            }
            return;
        }

        let size = ArrowSize::drawn(r, self.selected(r.selected));
        for p in &r.reactants {
            if let Some(node) = self.visible(&p.species) {
                let j = leg_inner_end(r, Role::Reactant, p);
                let start = if self.both_ends(r) {
                    let (start, tip) = leg_end_sized(node, j, &size);
                    self.arrowhead(tip, node.center - j, stroke.color, &size, out);
                    start
                } else {
                    leg_start(node, j)
                };
                out.polyline(&[self.to_screen(start), self.to_screen(j)], stroke);
                self.leg_run(r, Role::Reactant, p, stroke, out);
            }
        }
        for p in &r.products {
            if let Some(node) = self.visible(&p.species) {
                let j = leg_inner_end(r, Role::Product, p);
                let (end, tip) = leg_end_sized(node, j, &size);
                out.polyline(&[self.to_screen(j), self.to_screen(end)], stroke);
                self.arrowhead(tip, node.center - j, stroke.color, &size, out);
                self.leg_run(r, Role::Product, p, stroke, out);
            }
        }
    }

    /// A leg's straight run through its corner points to its side's anchor
    /// (S §5.6.5), if it has corner points.
    fn leg_run(
        &self,
        r: &Reaction,
        role: Role,
        p: &Participant,
        stroke: Stroke,
        out: &mut dyn Renderer,
    ) {
        let run = leg_run(r, role, p);
        if run.len() >= 2 {
            let pts: Vec<SPt> = run.iter().map(|&q| self.to_screen(q)).collect();
            out.polyline(&pts, stroke);
        }
    }

    /// Whether `r` is drawn with arrowheads at both ends (S §5.5): it is
    /// reversible and they are shown.
    fn both_ends(&self, r: &Reaction) -> bool {
        r.is_reversible && self.opts.show_reversible
    }

    fn arrowhead(&self, tip: Pt, dir: Pt, color: Rgba, size: &ArrowSize, out: &mut dyn Renderer) {
        let tri = filled_arrowhead(tip, dir, size.length, size.half_base);
        out.polygon(&tri.map(|p| self.to_screen(p)), color);
    }

    // ----- Pass 7: Bézier handles (S §5.6.4) --------------------------------

    /// Every leg of a selected Bézier reaction shows its two handles, each
    /// joined to its anchor by a dashed grey line: the species centre for the
    /// outer handle, the junction for the inner one. A smooth junction's inner
    /// handles are outlined in teal.
    fn bezier_handles(&self, r: &Reaction, out: &mut dyn Renderer) {
        if effective_mode(r) != ReactionMode::Bezier {
            return;
        }
        let guide = Stroke {
            width: self.len(1.0),
            color: HANDLE_GUIDE,
            dash: self.dash(HANDLE_GUIDE_DASH),
            round_cap: false,
        };
        for (role, list) in [(Role::Reactant, &r.reactants), (Role::Product, &r.products)] {
            for (index, p) in list.iter().enumerate() {
                if self.visible(&p.species).is_none() {
                    continue;
                }
                let Some(curve) = leg_curve(self.model, r, role, index) else {
                    continue;
                };
                let (species_end, junction_end) = if role == Role::Reactant {
                    (curve.p0, curve.p3)
                } else {
                    (curve.p3, curve.p0)
                };
                for ctrl in [Ctrl::One, Ctrl::Two] {
                    let at = if ctrl == Ctrl::One {
                        curve.p1
                    } else {
                        curve.p2
                    };
                    let inner = ctrl == Ctrl::inner(role);
                    let anchor = if inner { junction_end } else { species_end };
                    out.polyline(&[self.to_screen(anchor), self.to_screen(at)], guide);
                    let border = if inner && r.junction_smooth {
                        Stroke::solid(self.len(1.5), HANDLE_SMOOTH_INNER)
                    } else {
                        Stroke::solid(self.len(1.0), HANDLE_BORDER)
                    };
                    out.circle(
                        self.to_screen(at),
                        self.len(HANDLE_RADIUS),
                        Some(HANDLE_FILL),
                        Some(border),
                    );
                }
            }
        }
    }

    // ----- Pass 3: modifier arcs (S §5.7) -----------------------------------

    fn modifier_arcs(&self, r: &Reaction, out: &mut dyn Renderer) {
        if r.modifiers.is_empty() || !self.opts.show_modifiers {
            return;
        }
        let j = effective_junction(self.model, r);
        for m in &r.modifiers {
            if self.visible(&m.species).is_none() {
                continue;
            }
            let Some(mut route) = modifier_route(self.model, r, m) else {
                continue;
            };
            // Its own look (S §5.7.2); the selected colour while selected.
            let color = if self.selected(r.selected) {
                self.opts.selected_reaction
            } else {
                m.look.colour.unwrap_or(MODIFIER)
            };
            let width = m.look.width.unwrap_or(DEFAULT_LINE_WIDTH);
            let (dash, round_cap) = match m.look.pattern {
                LinePattern::Dashed => (self.dash(MODIFIER_DASH), false),
                LinePattern::Dotted => (self.dash((0.1 * width, 2.5 * width)), true),
                LinePattern::Solid => (None, false),
            };
            let stroke = Stroke {
                width: self.len(width),
                color,
                dash,
                round_cap,
            };
            // Starts inside the node, so the node hides the start.
            route[0] = route[0] - normalize(route[1] - route[0]) * MODIFIER_INSET;
            // The symbol follows the last piece.
            let n = route.len();
            let toward_j = normalize(j - route[n - 2]);
            route[n - 1] = self.modifier_symbol(m, j, toward_j, color, width, out);
            let pts: Vec<SPt> = route.iter().map(|&p| self.to_screen(p)).collect();
            out.polyline(&pts, stroke);
        }
    }

    /// A selected reaction's modifiers' corner points, as small squares
    /// (S §5.7.1), while modifier arcs are shown.
    fn corner_points(&self, r: &Reaction, out: &mut dyn Renderer) {
        // Its lines' (S §5.6.5), unless it is drawn Linear, and its modifier
        // arcs' while they are shown (S §5.7.1).
        let mut lines: Vec<&Participant> = Vec::new();
        if effective_mode(r) != ReactionMode::Linear {
            lines.extend(r.reactants.iter().chain(&r.products));
        }
        if self.opts.show_modifiers {
            lines.extend(&r.modifiers);
        }
        for m in lines
            .into_iter()
            .filter(|m| self.visible(&m.species).is_some())
        {
            for &p in &m.points {
                let current = self.opts.current_corner == Some(p);
                out.round_rect(
                    self.rect(p, HANDLE_RADIUS, HANDLE_RADIUS),
                    0.0,
                    Some(if current {
                        self.opts.selected_reaction
                    } else {
                        HANDLE_FILL
                    }),
                    Some(Stroke::solid(self.len(1.0), HANDLE_BORDER)),
                );
            }
        }
    }

    /// The symbol at the junction end of a modifier's arc, for its kind
    /// (S §5.7.1) and look (S §5.7.2), its axis along `dir` (toward the
    /// junction `j`). Returns where the arc's line stops. Open symbols are
    /// filled with the background, so the line doesn't show through them.
    fn modifier_symbol(
        &self,
        m: &Participant,
        j: Pt,
        dir: Pt,
        color: Rgba,
        width: f64,
        out: &mut dyn Renderer,
    ) -> Pt {
        let (size, gap) = m.kind.symbol_defaults();
        let size = m.look.symbol_size.unwrap_or(size);
        let gap = m.look.symbol_gap.unwrap_or(gap);
        let solid = Stroke {
            width: self.len(width),
            color,
            dash: None,
            round_cap: false,
        };
        let back = background_colour(self.model);
        match m.kind {
            ModifierKind::Modifier => {
                let dot = j - dir * (gap + size / 2.0);
                out.circle(self.to_screen(dot), self.len(size / 2.0), Some(color), None);
                dot
            }
            ModifierKind::Catalyst => {
                let centre = j - dir * (gap + size / 2.0);
                out.circle(
                    self.to_screen(centre),
                    self.len(size / 2.0),
                    Some(back),
                    Some(solid),
                );
                centre - dir * (size / 2.0)
            }
            ModifierKind::Activator => {
                let tip = j - dir * gap;
                let tri = filled_arrowhead(tip, dir, size, size * ARROW_HALF_BASE / ARROW_LENGTH);
                let screen = tri.map(|p| self.to_screen(p));
                out.polygon(&screen, back);
                out.polyline(&[screen[0], screen[1], screen[2], screen[0]], solid);
                tip - dir * size
            }
            ModifierKind::Inhibitor => {
                let centre = j - dir * gap;
                let across = pt(-dir.y, dir.x) * (size / 2.0);
                out.polyline(
                    &[
                        self.to_screen(centre - across),
                        self.to_screen(centre + across),
                    ],
                    Stroke {
                        width: self.len(width + 1.0),
                        ..solid
                    },
                );
                centre
            }
        }
    }

    // ----- Pass 4: species nodes (S §5.3, §5.4) -----------------------------

    fn species(&self, node: &SpeciesNode, out: &mut dyn Renderer) {
        if node.is_null_node {
            if self.opts.show_null_species {
                self.null_node(node, out);
            }
            return;
        }
        let alias = node.is_alias() && self.opts.show_alias_indicator;
        let style = custom(&node.style);
        let (fill, border_color) = species_colours(node, self.opts.show_alias_indicator);
        let mut border_width = species_border_width(node);
        if is_boundary(self.model, node) {
            border_width *= BOUNDARY_BORDER_FACTOR;
        }
        let stroke = Stroke {
            width: self.len(border_width),
            color: border_color,
            // An alias keeps its dashed border even with a custom style, while
            // the alias indicator is shown; a border style set by hand
            // decides for itself (S §5.3).
            dash: match node.border_dash {
                BorderDash::Automatic => alias,
                BorderDash::Solid => false,
                BorderDash::Dashed => true,
            }
            .then(|| self.dash(ALIAS_DASH))
            .flatten(),
            round_cap: false,
        };
        match crate::node_shape::outline(node) {
            // Filled, then outlined as a closed line (S §5.10).
            Some(pts) => {
                let mut pts: Vec<SPt> = pts.into_iter().map(|p| self.to_screen(p)).collect();
                out.polygon(&pts, fill);
                if border_width > 0.0 {
                    pts.push(pts[0]);
                    out.polyline(&pts, stroke);
                }
            }
            None => {
                let radius = if node.shape == NodeShape::Rectangle {
                    0.0
                } else {
                    NODE_CORNER_RADIUS
                };
                let rect = self.rect(node.center, node.width / 2.0, node.height / 2.0);
                let stroke = (border_width > 0.0).then_some(stroke);
                out.round_rect(rect, self.len(radius), Some(fill), stroke);
            }
        }

        let size = label_size(node) * self.opts.zoom;
        if size >= MIN_LABEL_SIZE_PX {
            let color = pick(style.map(|s| s.label_color), LABEL);
            // Inside the node, or outside it at its offset (S §5.11).
            out.text(
                self.to_screen(node.center + node.label_offset.unwrap_or(Pt::ZERO)),
                node.primary_id(),
                size as f32,
                name_font(&node.style),
                color,
            );
        }
        if node.locked && !self.opts.for_output {
            self.padlock(node, out);
        }
    }

    /// A small padlock in a locked node's top-right corner (S §13.4: locked
    /// nodes look different). An editing aid, so it is not drawn for output.
    fn padlock(&self, node: &SpeciesNode, out: &mut dyn Renderer) {
        let c = node.center
            + pt(
                node.width / 2.0 - LOCK_INSET,
                -node.height / 2.0 + LOCK_INSET,
            );
        let body = SRect {
            min: self.to_screen(c + pt(-3.5, -0.5)),
            max: self.to_screen(c + pt(3.5, 5.0)),
        };
        out.round_rect(body, self.len(1.0), Some(NULL_BORDER), None);
        // The shackle: a half circle over the body, with short sides.
        let arc: Vec<SPt> = (0..=8)
            .map(|i| {
                let a = std::f64::consts::PI * (1.0 + f64::from(i) / 8.0);
                self.to_screen(c + pt(2.3 * a.cos(), -2.5 + 2.3 * a.sin()))
            })
            .collect();
        let mut shackle = vec![self.to_screen(c + pt(-2.3, -0.5))];
        shackle.extend(arc);
        shackle.push(self.to_screen(c + pt(2.3, -0.5)));
        out.polyline(&shackle, Stroke::solid(self.len(1.3), NULL_BORDER));
    }

    fn null_node(&self, node: &SpeciesNode, out: &mut dyn Renderer) {
        let c = self.to_screen(node.center);
        let radius = node.width / 2.0;
        out.circle(
            c,
            self.len(radius),
            Some(NULL_FILL),
            Some(Stroke::solid(self.len(DEFAULT_LINE_WIDTH), NULL_BORDER)),
        );
        // An × whose arms reach 55 % of the radius.
        let arm = self.len(radius * NULL_CROSS_ARM * std::f64::consts::FRAC_1_SQRT_2);
        let stroke = Stroke {
            width: self.len(NULL_CROSS_WIDTH),
            color: NULL_BORDER,
            dash: None,
            round_cap: true,
        };
        out.polyline(
            &[spt(c.x - arm, c.y - arm), spt(c.x + arm, c.y + arm)],
            stroke,
        );
        out.polyline(
            &[spt(c.x - arm, c.y + arm), spt(c.x + arm, c.y - arm)],
            stroke,
        );
    }

    // ----- Pass 5: selection halos (S §5.3) ---------------------------------

    fn halo(&self, node: &SpeciesNode, out: &mut dyn Renderer) {
        if node.is_null_node && !self.opts.show_null_species {
            return;
        }
        let stroke = Stroke {
            width: self.len(HALO_WIDTH),
            color: SELECTION_HALO,
            dash: self.dash(HALO_DASH),
            round_cap: false,
        };
        if node.is_null_node {
            let radius = node.width / 2.0 + HALO_OUTSET;
            out.circle(
                self.to_screen(node.center),
                self.len(radius),
                None,
                Some(stroke),
            );
        } else {
            let rect = self.rect(
                node.center,
                node.width / 2.0 + HALO_OUTSET,
                node.height / 2.0 + HALO_OUTSET,
            );
            out.round_rect(rect, self.len(HALO_CORNER_RADIUS), None, Some(stroke));
            // A name outside the node is outlined too, more lightly (S §5.11).
            if let Some((a, b)) = outside_label_box(node) {
                let label = SRect {
                    min: self.to_screen(a),
                    max: self.to_screen(b),
                };
                let thin = Stroke {
                    width: self.len(1.0),
                    ..stroke
                };
                out.round_rect(label, 0.0, None, Some(thin));
            }
            // Its corners resize it (S §7.10).
            for p in species_corners(node) {
                out.round_rect(
                    self.rect(p, HANDLE_RADIUS, HANDLE_RADIUS),
                    0.0,
                    Some(HANDLE_FILL),
                    Some(Stroke::solid(self.len(1.0), HANDLE_BORDER)),
                );
            }
        }
    }

    // ----- The grid (S §7.6) ------------------------------------------------

    /// Grid lines across `area`: every [`GRID_SPACING`] units, darker every
    /// [`GRID_MAJOR_EVERY`] spaces. Lines that would be closer than
    /// [`GRID_MIN_GAP_PX`] on screen are left out, finest first.
    fn grid(&self, area: SRect, out: &mut dyn Renderer) {
        let zoom = self.opts.zoom;
        let to_world = |x: f32, y: f32| {
            pt(
                (f64::from(x) - self.opts.scroll.x) / zoom,
                (f64::from(y) - self.opts.scroll.y) / zoom,
            )
        };
        let (a, b) = (
            to_world(area.min.x, area.min.y),
            to_world(area.max.x, area.max.y),
        );
        let major = GRID_SPACING * GRID_MAJOR_EVERY as f64;
        for (step, color, skip_majors) in
            [(GRID_SPACING, GRID_MINOR, true), (major, GRID_MAJOR, false)]
        {
            if step * zoom < GRID_MIN_GAP_PX {
                continue;
            }
            let stroke = Stroke::solid(1.0, color);
            let lines = |lo: f64, hi: f64| {
                let first = (lo / step).floor() as i64;
                let last = (hi / step).ceil() as i64;
                (first..=last).filter(move |k| !(skip_majors && k % GRID_MAJOR_EVERY == 0))
            };
            for k in lines(a.x, b.x) {
                let x = self.to_screen(pt(k as f64 * step, 0.0)).x;
                out.polyline(&[spt(x, area.min.y), spt(x, area.max.y)], stroke);
            }
            for k in lines(a.y, b.y) {
                let y = self.to_screen(pt(0.0, k as f64 * step)).y;
                out.polyline(&[spt(area.min.x, y), spt(area.max.x, y)], stroke);
            }
        }
    }

    // ----- Compartments (S §7.9) ---------------------------------------------

    /// A rounded box with the compartment's name inside its top-left corner.
    fn compartment(&self, c: &Compartment, f: &Frame, out: &mut dyn Renderer) {
        let (fill, border, width) = compartment_colours(c);
        out.round_rect(
            self.rect(f.center, f.width / 2.0, f.height / 2.0),
            self.len(
                COMPARTMENT_CORNER_RADIUS
                    .min(f.width / 2.0)
                    .min(f.height / 2.0),
            ),
            Some(fill),
            (width > 0.0).then(|| Stroke::solid(self.len(width), border)),
        );
        let label = compartment_label_size(c);
        let size = label * self.opts.zoom;
        if size >= MIN_LABEL_SIZE_PX {
            let (a, b) = compartment_label_box(c, f);
            let cap = font_metrics(label).cap_height;
            let m = font_metrics(label);
            let centre = pt((a.x + b.x) / 2.0, a.y + m.ascent - cap / 2.0);
            out.text(
                self.to_screen(centre),
                &c.id,
                size as f32,
                TextFont::default(),
                // The name takes a custom border colour; otherwise its own.
                if c.style.has_custom_style && !c.style.border_color.is_default() {
                    border
                } else {
                    COMPARTMENT_LABEL
                },
            );
        }
    }

    /// A selected compartment's dashed outline and corner handles (screen only).
    fn compartment_selection(&self, _c: &Compartment, f: &Frame, out: &mut dyn Renderer) {
        let stroke = Stroke {
            width: self.len(HALO_WIDTH),
            color: SELECTION_HALO,
            dash: self.dash(HALO_DASH),
            round_cap: false,
        };
        out.round_rect(
            self.rect(
                f.center,
                f.width / 2.0 + HALO_OUTSET,
                f.height / 2.0 + HALO_OUTSET,
            ),
            self.len(HALO_CORNER_RADIUS),
            None,
            Some(stroke),
        );
        for p in compartment_corners(f) {
            out.round_rect(
                self.rect(p, HANDLE_RADIUS, HANDLE_RADIUS),
                0.0,
                Some(HANDLE_FILL),
                Some(Stroke::solid(self.len(1.0), HANDLE_BORDER)),
            );
        }
    }

    // ----- Free text (S §7.8) -------------------------------------------------

    /// A text, line by line, each line from the text's left edge.
    fn free_text(&self, t: &TextItem, out: &mut dyn Renderer) {
        let size = t.size * self.opts.zoom;
        if size < MIN_LABEL_SIZE_PX {
            return;
        }
        let l = lay_out(t);
        let color = if t.color.is_default() { LABEL } else { t.color };
        let cap = font_metrics_in(t.font, t.size).cap_height;
        for (i, line) in l.lines.iter().enumerate() {
            if line.text.trim().is_empty() {
                continue;
            }
            let centre = t.position + pt(line.x + line.width / 2.0, l.baseline(i) - cap / 2.0);
            out.text(
                self.to_screen(centre),
                &line.text,
                size as f32,
                t.font,
                color,
            );
        }
    }

    /// A selected text's dashed outline and its wrap handle (screen only).
    fn text_selection(&self, t: &TextItem, out: &mut dyn Renderer) {
        let (min, max) = text_box(t);
        let stroke = Stroke {
            width: self.len(HALO_WIDTH),
            color: SELECTION_HALO,
            dash: self.dash(HALO_DASH),
            round_cap: false,
        };
        let rect = self.rect(
            (min + max) / 2.0,
            (max.x - min.x) / 2.0,
            (max.y - min.y) / 2.0,
        );
        out.round_rect(rect, 0.0, None, Some(stroke));
        let h = self.rect(wrap_handle(t), HANDLE_RADIUS, HANDLE_RADIUS);
        out.round_rect(
            h,
            0.0,
            Some(HANDLE_FILL),
            Some(Stroke::solid(self.len(1.0), HANDLE_BORDER)),
        );
    }

    // ----- The + handle (S §7.11) ------------------------------------------

    /// A white disc with a + in the selection colour, by the junction of a
    /// selected reaction, to drag a new line from.
    fn add_line_handle(&self, r: &Reaction, out: &mut dyn Renderer) {
        let Some(at) = add_line_handle(self.model, r, self.opts.zoom) else {
            return;
        };
        let c = self.to_screen(at);
        let radius = ADD_LINE_RADIUS_PX as f32;
        out.circle(
            c,
            radius,
            Some(HANDLE_FILL),
            Some(Stroke::solid(1.0, SELECTION_HALO)),
        );
        let arm = radius * 0.6;
        let plus = Stroke::solid(1.5, SELECTION_HALO);
        out.polyline(&[spt(c.x - arm, c.y), spt(c.x + arm, c.y)], plus);
        out.polyline(&[spt(c.x, c.y - arm), spt(c.x, c.y + arm)], plus);
    }

    // ----- Stem ends (S §5.13) ---------------------------------------------

    /// A selected reaction's stem has a square handle at each end, to drag.
    fn stem_ends(&self, r: &Reaction, out: &mut dyn Renderer) {
        let Some([a, _, b]) = stem_line(r) else {
            return;
        };
        for p in [a, b] {
            out.round_rect(
                self.rect(p, HANDLE_RADIUS, HANDLE_RADIUS),
                0.0,
                Some(HANDLE_FILL),
                Some(Stroke::solid(self.len(1.0), HANDLE_BORDER)),
            );
        }
    }

    // ----- Reaction names (S §5.12) -----------------------------------------

    fn reaction_name(&self, r: &Reaction, out: &mut dyn Renderer) {
        let Some(offset) = r.label_offset else {
            return;
        };
        let size = reaction_label_size(r) * self.opts.zoom;
        if size >= MIN_LABEL_SIZE_PX {
            out.text(
                self.to_screen(effective_junction(self.model, r) + offset),
                &r.id,
                size as f32,
                name_font(&r.style),
                reaction_label_colour(r),
            );
        }
    }

    /// A selected reaction's name is outlined, as a species' name outside
    /// its node is (S §5.11).
    fn reaction_name_outline(&self, r: &Reaction, out: &mut dyn Renderer) {
        let Some((a, b)) = reaction_label_box(self.model, r) else {
            return;
        };
        let stroke = Stroke {
            width: self.len(1.0),
            color: SELECTION_HALO,
            dash: self.dash(HALO_DASH),
            round_cap: false,
        };
        let rect = SRect {
            min: self.to_screen(a),
            max: self.to_screen(b),
        };
        out.round_rect(rect, 0.0, None, Some(stroke));
    }

    // ----- Pass 6: junction handles (S §5.8) --------------------------------

    fn junction(&self, r: &Reaction, out: &mut dyn Renderer) {
        if effective_mode(r) == ReactionMode::Linear || !r.style.junction_visible {
            return;
        }
        let selected = self.selected(r.selected);
        let fill = pick(custom(&r.style).map(|s| s.junction_color), JUNCTION_FILL);
        let (radius, border) = if selected {
            (JUNCTION_RADIUS_SELECTED, JUNCTION_BORDER_SELECTED)
        } else {
            (JUNCTION_RADIUS, JUNCTION_BORDER)
        };
        out.circle(
            self.to_screen(r.junction),
            self.len(radius),
            Some(fill),
            Some(Stroke::solid(self.len(JUNCTION_BORDER_WIDTH), border)),
        );
    }
}

/// The point where a line from `node`'s centre towards `toward` crosses its
/// border, for the shape it is drawn as (rounded corners ignored, S §5.5,
/// §5.10).
pub fn border(node: &SpeciesNode, toward: Pt) -> Pt {
    crate::node_shape::border(node, toward)
}

/// Where a reactant leg starts: the reactant gap outside the border, on the
/// way to `toward`.
pub fn leg_start(node: &SpeciesNode, toward: Pt) -> Pt {
    border(node, toward) + normalize(toward - node.center) * REACTANT_GAP
}

/// Where a product line ends and where its arrowhead's tip goes: the product
/// line gap and the tip inset short of `node`'s border, coming from `from`.
pub fn leg_end(node: &SpeciesNode, from: Pt) -> (Pt, Pt) {
    leg_end_sized(node, from, &ArrowSize::STANDARD)
}

/// [`leg_end`] for an arrowhead of `size`.
pub fn leg_end_sized(node: &SpeciesNode, from: Pt, size: &ArrowSize) -> (Pt, Pt) {
    let b = border(node, from);
    let back = normalize(from - node.center);
    (b + back * size.line_gap, b + back * size.tip_inset)
}

/// The fill and border colours a species is drawn with: its custom colours
/// where set, otherwise the defaults (the alias colours for an alias while the
/// alias indicator is on). The inspector's swatches show these (S §8.1).
pub fn species_colours(node: &SpeciesNode, show_alias_indicator: bool) -> (Rgba, Rgba) {
    let (fill, border) = if node.is_alias() && show_alias_indicator {
        (ALIAS_FILL, ALIAS_BORDER)
    } else {
        (SPECIES_FILL, SPECIES_BORDER)
    };
    let style = custom(&node.style);
    (
        pick(style.map(|s| s.fill_color), fill),
        pick(style.map(|s| s.border_color), border),
    )
}

/// The colour a reaction's lines are drawn with.
pub fn line_colour(r: &Reaction) -> Rgba {
    pick(custom(&r.style).map(|s| s.line_color), REACTION_LINE)
}

/// The style, if it is custom (`has_custom_style`); otherwise none of its
/// fields apply (S §3.6).
fn custom(style: &VisualStyle) -> Option<&VisualStyle> {
    style.has_custom_style.then_some(style)
}

/// A custom colour, unless it is the "use the default" sentinel.
fn pick(custom: Option<Rgba>, default: Rgba) -> Rgba {
    custom.filter(|c| !c.is_default()).unwrap_or(default)
}

/// A custom width or size, unless it is 0 ("use the default").
fn pick_width(custom: Option<f64>, default: f64) -> f64 {
    custom.filter(|&w| w > 0.0).unwrap_or(default)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Participant;

    /// Records every call, in screen coordinates, so tests can check what was
    /// drawn without a canvas.
    #[derive(Default)]
    struct Recorder {
        calls: Vec<Call>,
        /// Each rounded rectangle's corner radius, in the order drawn.
        radii: Vec<f32>,
    }

    #[derive(Debug, Clone, PartialEq)]
    enum Call {
        RoundRect(SRect, Option<Rgba>, Option<Stroke>),
        Circle(SPt, f32, Option<Rgba>, Option<Stroke>),
        Polyline(Vec<SPt>, Stroke),
        Cubic([SPt; 4], Stroke),
        Polygon(Vec<SPt>, Rgba),
        Text(SPt, String, f32, Rgba),
        TextIn(SPt, String, f32, TextFont, Rgba),
    }

    impl Renderer for Recorder {
        fn round_rect(
            &mut self,
            r: SRect,
            radius: f32,
            fill: Option<Rgba>,
            stroke: Option<Stroke>,
        ) {
            self.radii.push(radius);
            self.calls.push(Call::RoundRect(r, fill, stroke));
        }
        fn circle(&mut self, c: SPt, radius: f32, fill: Option<Rgba>, stroke: Option<Stroke>) {
            self.calls.push(Call::Circle(c, radius, fill, stroke));
        }
        fn polyline(&mut self, pts: &[SPt], stroke: Stroke) {
            self.calls.push(Call::Polyline(pts.to_vec(), stroke));
        }
        fn cubic(&mut self, p0: SPt, c1: SPt, c2: SPt, p3: SPt, stroke: Stroke) {
            self.calls.push(Call::Cubic([p0, c1, c2, p3], stroke));
        }
        fn polygon(&mut self, pts: &[SPt], fill: Rgba) {
            self.calls.push(Call::Polygon(pts.to_vec(), fill));
        }
        fn text(&mut self, center: SPt, s: &str, size: f32, font: TextFont, color: Rgba) {
            // Labels are always in the label font.
            if font == TextFont::default() {
                self.calls
                    .push(Call::Text(center, s.to_owned(), size, color));
            } else {
                self.calls
                    .push(Call::TextIn(center, s.to_owned(), size, font, color));
            }
        }
    }

    fn draw(model: &Model, opts: &DrawOptions) -> Vec<Call> {
        let mut rec = Recorder::default();
        draw_model(model, opts, &mut rec);
        rec.calls
    }

    fn close(a: SPt, b: SPt) -> bool {
        (a.x - b.x).abs() < 1e-3 && (a.y - b.y).abs() < 1e-3
    }

    /// A at (0,0) → R at (200,0) → B at (400,0), all 80 × 36.
    fn uni_uni() -> Model {
        let mut m = Model::new();
        let a = m.add_species(pt(0.0, 0.0), 80.0, 36.0);
        let b = m.add_species(pt(400.0, 0.0), 80.0, 36.0);
        let r = m.add_reaction(pt(200.0, 0.0));
        let r = m.find_reaction_mut(&r).unwrap();
        r.reactants.push(Participant::new(a, 1.0));
        r.products.push(Participant::new(b, 1.0));
        m
    }

    #[test]
    fn straight_legs_leave_the_gaps() {
        let calls = draw(&uni_uni(), &DrawOptions::default());
        let lines: Vec<&Vec<SPt>> = calls
            .iter()
            .filter_map(|c| match c {
                Call::Polyline(p, _) => Some(p),
                _ => None,
            })
            .collect();
        // Reactant: starts 6 outside A's right border (x = 40 + 6).
        assert!(close(lines[0][0], spt(46.0, 0.0)));
        assert!(close(lines[0][1], spt(200.0, 0.0)));
        // Product: from the junction to 6 before B's left border (x = 360 − 6).
        assert!(close(lines[1][0], spt(200.0, 0.0)));
        assert!(close(lines[1][1], spt(354.0, 0.0)));
        // Arrowhead tip 3 before the border, pointing right.
        let arrow = calls
            .iter()
            .find_map(|c| match c {
                Call::Polygon(p, _) => Some(p),
                _ => None,
            })
            .unwrap();
        assert!(close(arrow[0], spt(357.0, 0.0)));
        assert!(close(arrow[1], spt(345.0, -5.0)) || close(arrow[1], spt(345.0, 5.0)));
    }

    /// [`uni_uni`] routed round the junction (S §5.6.5): A's line goes down
    /// through (0,100) and (200,100) and up to the junction; B's leaves the
    /// junction up through (200,-100) and (400,-100) and comes down into B.
    fn routed(mode: ReactionMode) -> Model {
        let mut m = uni_uni();
        let r = m.reactions_mut().next().unwrap();
        r.mode = mode;
        r.reactants[0].points = vec![pt(0.0, 100.0), pt(200.0, 100.0)];
        r.products[0].points = vec![pt(400.0, -100.0), pt(200.0, -100.0)];
        m
    }

    fn polylines(calls: &[Call]) -> Vec<Vec<SPt>> {
        calls
            .iter()
            .filter_map(|c| match c {
                Call::Polyline(p, _) => Some(p.clone()),
                _ => None,
            })
            .collect()
    }

    fn arrow_tip(calls: &[Call]) -> SPt {
        calls
            .iter()
            .find_map(|c| match c {
                Call::Polygon(p, _) => Some(p[0]),
                _ => None,
            })
            .unwrap()
    }

    #[test]
    fn a_straight_leg_runs_through_its_corner_points() {
        let calls = draw(&routed(ReactionMode::Straight), &DrawOptions::default());
        let lines = polylines(&calls);
        let same = |got: &Vec<SPt>, want: &[(f32, f32)]| {
            got.len() == want.len()
                && got
                    .iter()
                    .zip(want)
                    .all(|(g, &(x, y))| close(*g, spt(x, y)))
        };
        // A's end piece leaves A's bottom (y 18) 6 out, to the first corner.
        assert!(
            same(&lines[0], &[(0.0, 24.0), (0.0, 100.0)]),
            "{:?}",
            lines[0]
        );
        assert!(same(
            &lines[1],
            &[(0.0, 100.0), (200.0, 100.0), (200.0, 0.0)]
        ));
        // B's end piece comes down from its first corner to 6 short of B's
        // top (y -18), and the arrowhead points down into B.
        assert!(
            same(&lines[2], &[(400.0, -100.0), (400.0, -24.0)]),
            "{:?}",
            lines[2]
        );
        assert!(same(
            &lines[3],
            &[(400.0, -100.0), (200.0, -100.0), (200.0, 0.0)]
        ));
        assert!(close(arrow_tip(&calls), spt(400.0, -21.0)));
    }

    #[test]
    fn a_bezier_leg_curves_only_into_its_species() {
        let calls = draw(&routed(ReactionMode::Bezier), &DrawOptions::default());
        let lines = polylines(&calls);
        // The runs are straight; the end pieces are curves (not polylines).
        assert_eq!(lines.len(), 2, "{lines:?}");
        assert_eq!(lines[0][0], spt(0.0, 100.0));
        assert_eq!(lines[1][0], spt(400.0, -100.0));
        // Without stored handles the end piece is straight, not fanned: the
        // arrowhead points straight down into B.
        assert!(close(arrow_tip(&calls), spt(400.0, -21.0)));
    }

    #[test]
    fn a_linear_reaction_ignores_corner_points() {
        let calls = draw(&routed(ReactionMode::Linear), &DrawOptions::default());
        let lines = polylines(&calls);
        assert_eq!(lines.len(), 1, "one line from A to B: {lines:?}");
        assert!(lines[0].iter().all(|p| p.y.abs() < 1e-3));
    }

    #[test]
    fn a_highlight_veils_the_rest_and_draws_its_items_in_their_colours() {
        let mut m = uni_uni();
        // Its name shown (S §5.12), to be drawn again over the veil.
        m.reactions_mut().next().unwrap().label_offset = Some(pt(0.0, -30.0));
        let id = m.reactions()[0].id.clone();
        let plain = draw(&m, &DrawOptions::default());
        let mut rec = Recorder::default();
        draw_model_highlighted(&m, &DrawOptions::default(), None, &mut rec);
        assert_eq!(rec.calls, plain, "no highlight: the plain drawing");

        // A vector with the reaction at −2: orange, thicker, labelled ×2,
        // over a veil of the background.
        let h = Highlight {
            reactions: vec![(id, -2.0)],
            species: Vec::new(),
        };
        let mut rec = Recorder::default();
        draw_model_highlighted(&m, &DrawOptions::default(), Some(&h), &mut rec);
        let extra = &rec.calls[plain.len()..];
        assert!(
            matches!(&extra[0], Call::RoundRect(_, Some(c), None) if c.0[3] == HIGHLIGHT_FADE_ALPHA),
            "the veil first: {:?}",
            extra[0]
        );
        let orange: Vec<&Call> = extra
            .iter()
            .filter(|c| matches!(c, Call::Polyline(_, s) if s.color == HIGHLIGHT_NEGATIVE))
            .collect();
        assert_eq!(orange.len(), 2, "both legs");
        if let Call::Polyline(_, s) = orange[0] {
            assert!(s.width > 1.5 * reaction_line_width(&m.reactions()[0]) as f32);
        }
        assert!(extra.iter().any(|c| matches!(c,
            Call::TextIn(_, t, _, _, col) if t == "×2" && *col == HIGHLIGHT_NEGATIVE)));
        let name = m.reactions()[0].id.clone();
        assert!(
            extra
                .iter()
                .any(|c| matches!(c, Call::Text(_, t, ..) | Call::TextIn(_, t, ..) if *t == name)),
            "its name over the veil"
        );

        // A law: every drawing of its species outlined, aliases too.
        let mut m = uni_uni();
        let a = m.species()[0].id.clone();
        m.add_alias(&a, pt(0.0, 200.0)).unwrap();
        let plain = draw(&m, &DrawOptions::default());
        let h = Highlight {
            reactions: Vec::new(),
            species: vec![(a, 1.0)],
        };
        let mut rec = Recorder::default();
        draw_model_highlighted(&m, &DrawOptions::default(), Some(&h), &mut rec);
        let outlines = rec.calls[plain.len()..]
            .iter()
            .filter(
                |c| matches!(c, Call::RoundRect(_, None, Some(s)) if s.color == HIGHLIGHT_POSITIVE),
            )
            .count();
        assert_eq!(outlines, 2, "the species and its alias");
        assert!(
            !rec.calls[plain.len()..]
                .iter()
                .any(|c| matches!(c, Call::TextIn(_, t, ..) if t.starts_with('×'))),
            "no label for 1"
        );
        assert_eq!(coefficient_label(0.5).as_deref(), Some("×1/2"));
        assert_eq!(coefficient_label(-1.0), None);
    }

    #[test]
    fn output_draws_the_highlight() {
        let m = uni_uni();
        let h = Highlight {
            reactions: vec![(m.reactions()[0].id.clone(), 1.0)],
            species: Vec::new(),
        };
        let shown = Shown::ALL;
        let with = crate::render::svg::model_to_svg_highlighted(&m, 1.0, shown, Some(&h));
        let without = crate::render::svg::model_to_svg(&m, 1.0, shown);
        assert!(with.contains("#1F6FD1"), "blue in the SVG");
        assert!(!without.contains("#1F6FD1"));
        let pdf = crate::render::pdf::model_to_pdf_highlighted(
            &m,
            crate::render::pdf::PageSize::Drawing,
            false,
            shown,
            Some(&h),
        );
        assert_ne!(
            pdf,
            crate::render::pdf::model_to_pdf(
                &m,
                crate::render::pdf::PageSize::Drawing,
                false,
                shown
            )
        );
    }

    #[test]
    fn a_selected_reaction_shows_its_lines_corner_points() {
        let squares = |m: &Model| -> Vec<SPt> {
            draw(m, &DrawOptions::default())
                .iter()
                .filter_map(|c| match c {
                    Call::RoundRect(r, _, _) => {
                        Some(spt((r.min.x + r.max.x) / 2.0, (r.min.y + r.max.y) / 2.0))
                    }
                    _ => None,
                })
                .collect()
        };
        let corners = [
            (0.0, 100.0),
            (200.0, 100.0),
            (400.0, -100.0),
            (200.0, -100.0),
        ];
        let shown = |m: &Model| {
            let s = squares(m);
            corners
                .iter()
                .filter(|&&(x, y)| s.iter().any(|q| close(*q, spt(x, y))))
                .count()
        };
        let mut m = routed(ReactionMode::Straight);
        assert_eq!(shown(&m), 0, "not while it isn't selected");
        m.reactions_mut().next().unwrap().selected = true;
        assert_eq!(shown(&m), 4);
        // A Linear reaction draws its line without them.
        m.reactions_mut().next().unwrap().mode = ReactionMode::Linear;
        assert_eq!(shown(&m), 0);
    }

    #[test]
    fn corner_points_count_in_the_drawing_s_extent() {
        let mut m = routed(ReactionMode::Straight);
        m.reactions_mut().next().unwrap().reactants[0].points[1] = pt(200.0, 900.0);
        let (_, max) = content_bounds(&m);
        assert!(max.y >= 900.0, "{max:?}");
    }

    #[test]
    fn a_name_keeps_clear_of_lines_that_leave_through_corner_points() {
        // Unrouted, the lines leave the junction left and right: the name
        // goes above or below. Routed, they leave up and down: it goes to a
        // side.
        let plain = uni_uni();
        let o = clear_reaction_label_offset(&plain, &plain.reactions()[0]);
        assert!(o.y.abs() > o.x.abs(), "{o:?}");
        let m = routed(ReactionMode::Straight);
        let o = clear_reaction_label_offset(&m, &m.reactions()[0]);
        assert!(o.x.abs() > o.y.abs(), "{o:?}");
    }

    #[test]
    fn each_shape_is_drawn_as_its_outline() {
        let mut m = Model::new();
        for (i, shape) in NodeShape::ALL.into_iter().enumerate() {
            let id = m.add_species(pt(200.0 * i as f64, 0.0), 80.0, 40.0);
            m.find_species_mut(&id).unwrap().shape = shape;
        }
        let calls = draw(&m, &DrawOptions::default());
        let fill = SPECIES_FILL;
        let rects = calls
            .iter()
            .filter(|c| matches!(c, Call::RoundRect(_, Some(f), _) if *f == fill))
            .count();
        let polygons: Vec<usize> = calls
            .iter()
            .filter_map(|c| match c {
                Call::Polygon(p, f) if *f == fill => Some(p.len()),
                _ => None,
            })
            .collect();
        let outlines: Vec<usize> = calls
            .iter()
            .filter_map(|c| match c {
                Call::Polyline(p, s) if s.color == SPECIES_BORDER => Some(p.len()),
                _ => None,
            })
            .collect();
        // The two rectangles as rectangles, the second with square corners;
        // the ellipse, circle and hexagon as a filled polygon and a closed
        // outline.
        assert_eq!(rects, 2);
        let mut rec = Recorder::default();
        draw_model(&m, &DrawOptions::default(), &mut rec);
        let radii: Vec<f32> = rec
            .calls
            .iter()
            .filter(|c| matches!(c, Call::RoundRect(..)))
            .zip(&rec.radii)
            .filter(|(c, _)| matches!(c, Call::RoundRect(_, Some(f), _) if *f == fill))
            .map(|(_, r)| *r)
            .collect();
        assert_eq!(radii, [NODE_CORNER_RADIUS as f32, 0.0]);
        assert_eq!(polygons, [64, 64, 6]);
        assert_eq!(outlines, [65, 65, 7]);
    }

    #[test]
    fn a_border_style_set_by_hand_wins_over_the_alias_rule() {
        // A primary and an alias of it, side by side, both rectangles.
        let mut m = Model::new();
        let p = m.add_species(pt(0.0, 0.0), 80.0, 36.0);
        let a = m.add_alias(&p, pt(200.0, 0.0)).unwrap();
        let dashes = |m: &Model, indicator: bool| -> Vec<bool> {
            let opts = DrawOptions {
                show_alias_indicator: indicator,
                ..DrawOptions::default()
            };
            draw(m, &opts)
                .iter()
                .filter_map(|c| match c {
                    // A node: filled and outlined (an alias's fill differs).
                    Call::RoundRect(_, Some(f), Some(s)) if *f != BACKGROUND => {
                        Some(s.dash.is_some())
                    }
                    _ => None,
                })
                .collect()
        };
        // Automatic: the alias dashed, while the indicator is shown.
        assert_eq!(dashes(&m, true), [false, true]);
        assert_eq!(dashes(&m, false), [false, false]);
        // Solid alias, dashed primary: as set, indicator or not.
        m.find_species_mut(&a).unwrap().border_dash = BorderDash::Solid;
        m.find_species_mut(&p).unwrap().border_dash = BorderDash::Dashed;
        assert_eq!(dashes(&m, true), [true, false]);
        assert_eq!(dashes(&m, false), [true, false]);
    }

    #[test]
    fn a_line_ends_on_the_shape_drawn() {
        // A product at 45 degrees: the ellipse's border is nearer its centre
        // than the box's corner.
        let mut node = SpeciesNode::new("B", pt(0.0, 0.0));
        node.width = 80.0;
        node.height = 80.0;
        let from = pt(-200.0, -200.0);
        let boxed = leg_end(&node, from).0;
        node.shape = NodeShape::Circle;
        let round = leg_end(&node, from).0;
        assert!(round.distance(node.center) < boxed.distance(node.center) - 10.0);
        let edge = border(&node, from);
        assert!((edge.distance(node.center) - 40.0).abs() < 1e-9);
    }

    #[test]
    fn draw_order_follows_s_5_2() {
        let mut m = uni_uni();
        m.find_species_mut("S1").unwrap().selected = true;
        // Free text, bold, selected: drawn over everything, then its outline
        // and wrap handle.
        let t = m.add_text(pt(0.0, 100.0), "Note");
        let t = m.find_text_mut(&t).unwrap();
        t.font.bold = true;
        t.selected = true;
        let opts = DrawOptions {
            background: Some(SRect {
                min: spt(0.0, 0.0),
                max: spt(100.0, 100.0),
            }),
            ..DrawOptions::default()
        };
        let kinds: Vec<&str> = draw(&m, &opts)
            .iter()
            .map(|c| match c {
                Call::RoundRect(_, Some(f), _) if *f == BACKGROUND => "bg",
                Call::RoundRect(_, Some(f), _) if *f == HANDLE_FILL => "handle",
                Call::RoundRect(_, None, _) => "halo",
                Call::RoundRect(..) => "node",
                Call::Polyline(..) | Call::Polygon(..) | Call::Cubic(..) => "leg",
                Call::Text(..) => "label",
                Call::TextIn(..) => "text",
                Call::Circle(..) => "junction",
            })
            .collect();
        assert_eq!(
            kinds,
            [
                // A selected species' halo has its four corner handles (S §7.10).
                "bg", "leg", "leg", "leg", "node", "label", "node", "label", "halo", "handle",
                "handle", "handle", "handle", "junction", "text", "halo", "handle"
            ]
        );
    }

    #[test]
    fn zoom_and_scroll_apply_to_points_widths_and_text() {
        let opts = DrawOptions {
            zoom: 2.0,
            scroll: pt(10.0, 20.0),
            ..DrawOptions::default()
        };
        let calls = draw(&uni_uni(), &opts);
        let Call::Polyline(pts, stroke) = &calls[0] else {
            panic!()
        };
        assert!(close(pts[0], spt(46.0 * 2.0 + 10.0, 20.0)));
        assert_eq!(stroke.width, 3.0);
        assert!(stroke.round_cap);
        let text = calls.iter().find_map(|c| match c {
            Call::Text(c, s, size, _) => Some((*c, s.clone(), *size)),
            _ => None,
        });
        assert_eq!(text, Some((spt(10.0, 20.0), "S1".to_owned(), 24.0)));
    }

    #[test]
    fn linear_reaction_is_one_line_with_no_junction() {
        let mut m = uni_uni();
        m.find_reaction_mut("R3").unwrap().mode = ReactionMode::Linear;
        let calls = draw(&m, &DrawOptions::default());
        let lines: Vec<_> = calls
            .iter()
            .filter(|c| matches!(c, Call::Polyline(..)))
            .collect();
        assert_eq!(lines.len(), 1);
        let Call::Polyline(p, _) = lines[0] else {
            panic!()
        };
        assert!(close(p[0], spt(46.0, 0.0)) && close(p[1], spt(354.0, 0.0)));
        assert!(!calls.iter().any(|c| matches!(c, Call::Circle(..))));
    }

    #[test]
    fn linear_flag_is_ignored_unless_uni_uni() {
        let mut m = uni_uni();
        let c = m.add_species(pt(400.0, 200.0), 80.0, 36.0);
        let r = m.find_reaction_mut("R3").unwrap();
        r.mode = ReactionMode::Linear;
        r.products.push(Participant::new(c, 1.0));
        assert_eq!(
            effective_mode(m.find_reaction("R3").unwrap()),
            ReactionMode::Straight
        );
        // Straight: a junction is drawn.
        assert!(
            draw(&m, &DrawOptions::default())
                .iter()
                .any(|c| matches!(c, Call::Circle(..)))
        );
    }

    #[test]
    fn effective_junction_of_a_linear_reaction() {
        let mut m = uni_uni();
        let r = m.find_reaction_mut("R3").unwrap();
        r.mode = ReactionMode::Linear;
        r.junction = pt(100.0, 50.0);
        let r = m.find_reaction("R3").unwrap().clone();
        // Projected onto the A–B line.
        assert_eq!(effective_junction(&m, &r), pt(100.0, 0.0));
        // Clamped along the line. The 0.1 limit alone would put it at x = 40,
        // on A's border; it is kept 10 outside it instead.
        let mut far = r.clone();
        far.junction = pt(-500.0, 0.0);
        assert_eq!(
            effective_junction(&m, &far),
            pt(40.0 + LINEAR_JUNCTION_CLEARANCE, 0.0)
        );
        let mut far = r.clone();
        far.junction = pt(900.0, 0.0);
        assert_eq!(
            effective_junction(&m, &far),
            pt(360.0 - LINEAR_JUNCTION_CLEARANCE, 0.0)
        );
        // Nodes too close for the clearance: halfway between their borders.
        let mut close = m.clone();
        close.find_species_mut("S2").unwrap().center = pt(90.0, 0.0);
        let rc = close.find_reaction("R3").unwrap().clone();
        assert_eq!(effective_junction(&close, &rc), pt(45.0, 0.0));
        // Straight reactions use the stored junction.
        let mut straight = r;
        straight.mode = ReactionMode::Straight;
        assert_eq!(effective_junction(&m, &straight), pt(100.0, 50.0));
    }

    #[test]
    fn modifier_arc_is_dashed_and_ends_at_a_dot() {
        let mut m = uni_uni();
        let e = m.add_species(pt(200.0, -200.0), 80.0, 36.0);
        m.find_reaction_mut("R3")
            .unwrap()
            .modifiers
            .push(Participant::new(e, 1.0));
        let calls = draw(&m, &DrawOptions::default());
        let arc = calls
            .iter()
            .find_map(|c| match c {
                Call::Polyline(p, s) if s.dash.is_some() => Some((p.clone(), *s)),
                _ => None,
            })
            .unwrap();
        // Starts 6 inside the bottom border of E (y = −200 + 18 − 6), ends at
        // the dot 10 above the junction.
        assert!(close(arc.0[0], spt(200.0, -188.0)));
        assert!(close(arc.0[1], spt(200.0, -10.0)));
        assert_eq!(arc.1.color, MODIFIER);
        assert!(calls.contains(&Call::Circle(spt(200.0, -10.0), 4.0, Some(MODIFIER), None)));
        // Hidden: no arc and no dot, and nothing else changes.
        let hidden = DrawOptions::default().with(Shown {
            modifiers: false,
            ..Shown::ALL
        });
        let without = draw(&m, &hidden);
        assert!(
            !without
                .iter()
                .any(|c| matches!(c, Call::Polyline(_, s) if s.dash.is_some()))
        );
        assert!(!without.contains(&Call::Circle(spt(200.0, -10.0), 4.0, Some(MODIFIER), None)));
        assert_eq!(without.len(), calls.len() - 2);
    }

    #[test]
    fn modifier_kinds_end_in_their_symbols() {
        use crate::model::ModifierKind;
        let mut m = uni_uni();
        let e = m.add_species(pt(200.0, -200.0), 80.0, 36.0);
        m.find_reaction_mut("R3")
            .unwrap()
            .modifiers
            .push(Participant::new(e, 1.0));
        let solid = Stroke {
            width: 1.5,
            color: MODIFIER,
            dash: None,
            round_cap: false,
        };
        let back = background_colour(&m);
        // The junction is at (200, 0); the arc comes down from E.
        let draw_kind = |m: &mut Model, k| {
            m.find_reaction_mut("R3").unwrap().modifiers[0].kind = k;
            let calls = draw(m, &DrawOptions::default());
            let arc_end = calls
                .iter()
                .find_map(|c| match c {
                    Call::Polyline(p, s) if s.dash.is_some() => Some(p[1]),
                    _ => None,
                })
                .unwrap();
            (calls, arc_end)
        };
        // Catalyst: an open circle where the dot was; the arc stops at it.
        let (calls, end) = draw_kind(&mut m, ModifierKind::Catalyst);
        assert!(calls.contains(&Call::Circle(
            spt(200.0, -10.0),
            5.0,
            Some(back),
            Some(solid)
        )));
        assert!(close(end, spt(200.0, -15.0)), "{end:?}");
        assert!(!calls.contains(&Call::Circle(spt(200.0, -10.0), 4.0, Some(MODIFIER), None)));
        // Activator: an open arrowhead, tip 8 above the junction, pointing at it.
        let (calls, end) = draw_kind(&mut m, ModifierKind::Activator);
        let tri = calls
            .iter()
            .find_map(|c| match c {
                Call::Polygon(p, f) if *f == back => Some(p.clone()),
                _ => None,
            })
            .expect("a triangle filled with the background");
        assert!(tri.iter().any(|p| close(*p, spt(200.0, -8.0))), "{tri:?}");
        // Its base is behind the tip, on the modifier's side: it points at the junction.
        assert!(
            tri.iter()
                .filter(|p| !close(**p, spt(200.0, -8.0)))
                .all(|p| close(spt(200.0, p.y), spt(200.0, -20.0))),
            "{tri:?}"
        );
        assert!(calls.contains(&Call::Polyline(vec![tri[0], tri[1], tri[2], tri[0]], solid)));
        assert!(close(end, spt(200.0, -20.0)), "{end:?}");
        // Inhibitor: a bar across the arc, 8 above the junction.
        let (calls, end) = draw_kind(&mut m, ModifierKind::Inhibitor);
        let bar = calls
            .iter()
            .find_map(|c| match c {
                Call::Polyline(p, s) if s.dash.is_none() && s.width == 2.5 => Some(p.clone()),
                _ => None,
            })
            .expect("a bar");
        assert!(close(bar[0], spt(192.0, -8.0)) || close(bar[0], spt(208.0, -8.0)));
        assert!(close(bar[1], spt(192.0, -8.0)) || close(bar[1], spt(208.0, -8.0)));
        assert_eq!(bar.len(), 2);
        assert!(close(end, spt(200.0, -8.0)), "{end:?}");
        // Plain modifier: the dot, as before.
        let (calls, end) = draw_kind(&mut m, ModifierKind::Modifier);
        assert!(calls.contains(&Call::Circle(spt(200.0, -10.0), 4.0, Some(MODIFIER), None)));
        assert!(close(end, spt(200.0, -10.0)));
        // Open symbols take the diagram's own background.
        m.background = Rgba::rgb(0x10, 0x20, 0x30);
        let (calls, _) = draw_kind(&mut m, ModifierKind::Catalyst);
        assert!(calls.contains(&Call::Circle(
            spt(200.0, -10.0),
            5.0,
            Some(Rgba::rgb(0x10, 0x20, 0x30)),
            Some(solid)
        )));
    }

    #[test]
    fn modifier_arcs_bend_at_their_corner_points() {
        let mut m = uni_uni();
        let e = m.add_species(pt(200.0, -200.0), 80.0, 36.0);
        let mut p = Participant::new(e, 1.0);
        p.points = vec![pt(360.0, -200.0), pt(360.0, -10.0)];
        m.find_reaction_mut("R3").unwrap().modifiers.push(p);
        let arc = |calls: &[Call]| {
            calls
                .iter()
                .find_map(|c| match c {
                    Call::Polyline(p, s) if s.dash.is_some() => Some(p.clone()),
                    _ => None,
                })
                .unwrap()
        };
        let calls = draw(&m, &DrawOptions::default());
        // From inside E's right edge, through both points, to the dot on the
        // last piece: 10 from the junction (200, 0), coming from (360, -10).
        let a = arc(&calls);
        assert_eq!(a.len(), 4);
        assert!(close(a[0], spt(234.0, -200.0)), "{a:?}");
        assert!(close(a[1], spt(360.0, -200.0)));
        assert!(close(a[2], spt(360.0, -10.0)));
        let d = normalize(pt(200.0, 0.0) - pt(360.0, -10.0));
        let dot = pt(200.0, 0.0) - d * 10.0;
        assert!(close(a[3], spt(dot.x as f32, dot.y as f32)), "{a:?}");
        // The squares show only on a selected reaction.
        let square = |calls: &[Call]| {
            calls
                .iter()
                .filter(|c| matches!(c, Call::RoundRect(_, Some(f), _) if *f == HANDLE_FILL))
                .count()
        };
        assert_eq!(square(&calls), 0);
        m.find_reaction_mut("R3").unwrap().selected = true;
        let calls = draw(&m, &DrawOptions::default());
        assert_eq!(square(&calls), 2);
        // The current one is filled with the selected reaction's colour.
        let opts = DrawOptions {
            current_corner: Some(pt(360.0, -10.0)),
            ..DrawOptions::default()
        };
        let calls = draw(&m, &opts);
        assert_eq!(square(&calls), 1);
        let filled = calls
            .iter()
            .filter(|c| matches!(c, Call::RoundRect(_, Some(f), _) if *f == opts.selected_reaction))
            .count();
        assert_eq!(filled, 1);
        // Hidden arcs hide their points too; output never shows them.
        let hidden = DrawOptions::default().with(Shown {
            modifiers: false,
            ..Shown::ALL
        });
        assert_eq!(square(&draw(&m, &hidden)), 0);
        let output = DrawOptions {
            for_output: true,
            ..DrawOptions::default()
        };
        assert_eq!(square(&draw(&m, &output)), 0);
    }

    #[test]
    fn a_modifier_line_is_drawn_in_its_own_look() {
        use crate::model::{LinePattern, ModifierKind, ModifierLook};
        let mut m = uni_uni();
        let e = m.add_species(pt(200.0, -200.0), 80.0, 36.0);
        m.find_reaction_mut("R3")
            .unwrap()
            .modifiers
            .push(Participant::new(e, 1.0));
        let red = Rgba::rgb(200, 0, 0);
        let set = |m: &mut Model, kind, look| {
            let p = &mut m.find_reaction_mut("R3").unwrap().modifiers[0];
            p.kind = kind;
            p.look = look;
            draw(m, &DrawOptions::default())
        };
        let line = |calls: &[Call]| {
            calls
                .iter()
                .find_map(|c| match c {
                    Call::Polyline(p, s) if p.len() == 2 && p[0].y < -100.0 => {
                        Some((p.clone(), *s))
                    }
                    _ => None,
                })
                .unwrap()
        };
        // Colour, width and pattern; the junction is at (200, 0).
        let base = ModifierLook {
            colour: Some(red),
            width: Some(3.0),
            ..ModifierLook::default()
        };
        for (pattern, dash, round) in [
            (LinePattern::Dashed, Some((6.0, 4.0)), false),
            (LinePattern::Dotted, Some((0.3, 7.5)), true),
            (LinePattern::Solid, None, false),
        ] {
            let calls = set(
                &mut m,
                ModifierKind::Modifier,
                ModifierLook { pattern, ..base },
            );
            let (_, s) = line(&calls);
            assert_eq!((s.color, s.width, s.round_cap), (red, 3.0, round));
            match (s.dash, dash) {
                (Some((a, b)), Some((x, y))) => {
                    assert!((a - x as f32).abs() < 1e-4 && (b - y as f32).abs() < 1e-4)
                }
                (None, None) => {}
                other => panic!("{pattern:?}: {other:?}"),
            }
            // The dot takes the line's colour.
            assert!(calls.contains(&Call::Circle(spt(200.0, -10.0), 4.0, Some(red), None)));
        }
        // Symbol size and distance, kind by kind.
        let look = ModifierLook {
            symbol_size: Some(20.0),
            symbol_gap: Some(15.0),
            ..ModifierLook::default()
        };
        // Dot: diameter 20, nearest point 15 from the junction.
        let calls = set(&mut m, ModifierKind::Modifier, look);
        assert!(calls.contains(&Call::Circle(spt(200.0, -25.0), 10.0, Some(MODIFIER), None)));
        // Bar: 20 long, 15 from the junction, 1 wider than the line.
        let calls = set(&mut m, ModifierKind::Inhibitor, look);
        let bar = calls
            .iter()
            .find_map(|c| match c {
                Call::Polyline(p, s) if s.dash.is_none() && s.width == 2.5 => Some(p.clone()),
                _ => None,
            })
            .unwrap();
        assert!(close(bar[0], spt(190.0, -15.0)) || close(bar[0], spt(210.0, -15.0)));
        assert!((bar[0].x - bar[1].x).abs() > 19.9);
        assert!(close(line(&calls).0[1], spt(200.0, -15.0)));
        // Arrowhead: 20 long, tip 15 from the junction; the line stops at its base.
        let calls = set(&mut m, ModifierKind::Activator, look);
        assert!(close(line(&calls).0[1], spt(200.0, -35.0)));
        // Circle: diameter 20, nearest point 15 away.
        let calls = set(&mut m, ModifierKind::Catalyst, look);
        assert!(calls.iter().any(|c| matches!(c,
            Call::Circle(p, r, _, Some(_)) if close(*p, spt(200.0, -25.0)) && *r == 10.0)));
        // Selected: the selected reaction colour, whatever its own.
        m.find_reaction_mut("R3").unwrap().selected = true;
        let calls = set(&mut m, ModifierKind::Modifier, base);
        assert_eq!(
            line(&calls).1.color,
            DrawOptions::default().selected_reaction
        );
    }

    #[test]
    fn styles_apply_field_by_field() {
        let mut m = uni_uni();
        let s = m.find_species_mut("S1").unwrap();
        s.style.has_custom_style = true;
        s.style.fill_color = Rgba::rgb(1, 2, 3);
        s.is_boundary = true;
        let calls = draw(&m, &DrawOptions::default());
        let Some(Call::RoundRect(_, fill, Some(stroke))) =
            calls.iter().find(|c| matches!(c, Call::RoundRect(..)))
        else {
            panic!()
        };
        assert_eq!(*fill, Some(Rgba::rgb(1, 2, 3)));
        // Border colour not set: default. Boundary: width × 2.4.
        assert_eq!(stroke.color, SPECIES_BORDER);
        assert!((stroke.width - 1.5 * 2.4).abs() < 1e-5);

        // Without has_custom_style the fields are ignored.
        m.find_species_mut("S1").unwrap().style.has_custom_style = false;
        let calls = draw(&m, &DrawOptions::default());
        assert!(matches!(
            calls.iter().find(|c| matches!(c, Call::RoundRect(..))),
            Some(Call::RoundRect(_, Some(f), _)) if *f == SPECIES_FILL
        ));
    }

    #[test]
    fn an_alias_of_a_boundary_species_has_its_thick_border() {
        // The border width drawn round the node centred at (0, y).
        fn border_at(m: &Model, y: f32) -> f32 {
            draw(m, &DrawOptions::default())
                .iter()
                .find_map(|c| match c {
                    Call::RoundRect(r, _, Some(s))
                        if ((r.min.y + r.max.y) / 2.0 - y).abs() < 1e-3 =>
                    {
                        Some(s.width)
                    }
                    _ => None,
                })
                .unwrap()
        }
        let mut m = uni_uni();
        let alias = m.add_alias("S1", pt(0.0, 100.0)).unwrap();
        m.find_species_mut("S1").unwrap().is_boundary = true;
        assert!((border_at(&m, 0.0) - 1.5 * 2.4).abs() < 1e-5);
        assert!((border_at(&m, 100.0) - 1.5 * 2.4).abs() < 1e-5);
        // The alias's own copy of the flag is not used.
        m.find_species_mut("S1").unwrap().is_boundary = false;
        m.find_species_mut(&alias).unwrap().is_boundary = true;
        assert!((border_at(&m, 100.0) - 1.5).abs() < 1e-5);
    }

    #[test]
    fn aliases_show_the_primary_id_with_a_dashed_border() {
        let mut m = uni_uni();
        let alias = m.add_alias("S1", pt(0.0, 100.0)).unwrap();
        let calls = draw(&m, &DrawOptions::default());
        let node = calls
            .iter()
            .rposition(|c| matches!(c, Call::RoundRect(..)))
            .unwrap();
        let Call::RoundRect(_, Some(fill), Some(stroke)) = &calls[node] else {
            panic!()
        };
        assert_eq!((*fill, stroke.color), (ALIAS_FILL, ALIAS_BORDER));
        assert_eq!(stroke.dash, Some((5.0, 4.0)));
        assert!(matches!(&calls[node + 1], Call::Text(_, s, _, _) if s == "S1"));

        // With the indicator off, an alias looks like any other node.
        let opts = DrawOptions {
            show_alias_indicator: false,
            ..DrawOptions::default()
        };
        let calls = draw(&m, &opts);
        assert!(
            calls
                .iter()
                .all(|c| !matches!(c, Call::RoundRect(_, Some(f), _) if *f == ALIAS_FILL))
        );
        assert!(m.find_species(&alias).is_some());
    }

    #[test]
    fn null_nodes_and_their_legs_can_be_hidden() {
        let mut m = uni_uni();
        m.find_species_mut("S2").unwrap().is_null_node = true;
        let shown = draw(&m, &DrawOptions::default());
        assert!(
            shown.iter().any(
                |c| matches!(c, Call::Circle(_, r, Some(f), _) if *f == NULL_FILL && *r == 40.0)
            )
        );
        let hidden = draw(
            &m,
            &DrawOptions {
                show_null_species: false,
                ..DrawOptions::default()
            },
        );
        assert!(
            !hidden.iter().any(|c| matches!(c, Call::Polygon(..))),
            "no product leg"
        );
        assert!(
            !hidden
                .iter()
                .any(|c| matches!(c, Call::Circle(_, _, Some(f), _) if *f == NULL_FILL))
        );
    }

    #[test]
    fn output_leaves_out_the_selection() {
        let mut m = uni_uni();
        m.find_species_mut("S1").unwrap().selected = true;
        m.find_reaction_mut("R3").unwrap().selected = true;
        let screen = draw(&m, &DrawOptions::default());
        let output = draw(
            &m,
            &DrawOptions {
                for_output: true,
                ..DrawOptions::default()
            },
        );
        assert!(
            screen
                .iter()
                .any(|c| matches!(c, Call::RoundRect(_, None, _)))
        );
        assert!(
            !output
                .iter()
                .any(|c| matches!(c, Call::RoundRect(_, None, _)))
        );
        // Selected line is 1.5× wide on screen only; selected junction is larger.
        let width = |calls: &[Call]| match &calls[0] {
            Call::Polyline(_, s) => s.width,
            _ => panic!(),
        };
        assert!((width(&screen) - 2.25).abs() < 1e-5);
        assert!((width(&output) - 1.5).abs() < 1e-5);
        assert!(
            screen
                .iter()
                .any(|c| matches!(c, Call::Circle(_, r, _, _) if (*r - 6.4).abs() < 1e-5))
        );
        assert!(
            output
                .iter()
                .any(|c| matches!(c, Call::Circle(_, r, _, _) if (*r - 4.0).abs() < 1e-5))
        );
    }

    #[test]
    fn a_selected_reaction_is_drawn_in_the_selection_colour() {
        let mut m = uni_uni();
        let colour = |calls: &[Call]| match &calls[0] {
            Call::Polyline(_, s) => s.color,
            _ => panic!(),
        };
        let plain = draw(&m, &DrawOptions::default());
        assert_eq!(colour(&plain), REACTION_LINE);
        m.find_reaction_mut("R3").unwrap().selected = true;
        // The default selection colour, and one from the preferences.
        assert_eq!(
            colour(&draw(&m, &DrawOptions::default())),
            SELECTED_REACTION
        );
        let pink = Rgba([0xE0, 0x10, 0x80, 0xFF]);
        let chosen = DrawOptions {
            selected_reaction: pink,
            ..DrawOptions::default()
        };
        let calls = draw(&m, &chosen);
        assert_eq!(colour(&calls), pink);
        // The arrowhead too.
        assert!(
            calls
                .iter()
                .any(|c| matches!(c, Call::Polygon(_, f) if *f == pink))
        );
        // Never in output.
        let output = DrawOptions {
            for_output: true,
            ..chosen
        };
        assert_eq!(colour(&draw(&m, &output)), REACTION_LINE);
    }

    #[test]
    fn a_name_outside_is_drawn_at_its_offset_and_outlined_on_screen_only() {
        let mut m = uni_uni();
        let s1 = m.find_species_mut("S1").unwrap();
        s1.label_offset = Some(pt(0.0, -40.0));
        s1.selected = true;
        let calls = draw(&m, &DrawOptions::default());
        let at = calls.iter().find_map(|c| match c {
            Call::Text(at, s, _, _) if s == "S1" => Some(*at),
            _ => None,
        });
        assert!(close(at.unwrap(), spt(0.0, -40.0)));
        // The halo, then the name's box, outlined in the halo colour.
        let (a, b) = outside_label_box(m.find_species("S1").unwrap()).unwrap();
        let outlined = |calls: &[Call]| {
            calls.iter().any(|c| {
                matches!(c, Call::RoundRect(r, None, Some(st))
                    if st.color == SELECTION_HALO
                        && close(r.min, spt(a.x as f32, a.y as f32))
                        && close(r.max, spt(b.x as f32, b.y as f32)))
            })
        };
        assert!(outlined(&calls));
        let output = DrawOptions {
            for_output: true,
            ..DrawOptions::default()
        };
        assert!(!outlined(&draw(&m, &output)));
        // Inside, it is at the node's centre.
        m.find_species_mut("S1").unwrap().label_offset = None;
        let calls = draw(&m, &DrawOptions::default());
        assert!(calls.iter().any(
            |c| matches!(c, Call::Text(at, s, _, _) if s == "S1" && close(*at, spt(0.0, 0.0)))
        ));
    }

    #[test]
    fn no_border_draws_no_outline() {
        let mut m = uni_uni();
        let fill_only = |calls: &[Call]| {
            calls
                .iter()
                .filter(|c| matches!(c, Call::RoundRect(_, Some(f), None) if *f == SPECIES_FILL))
                .count()
        };
        assert_eq!(fill_only(&draw(&m, &DrawOptions::default())), 0);
        m.find_species_mut("S1").unwrap().style.no_border = true;
        let calls = draw(&m, &DrawOptions::default());
        assert_eq!(fill_only(&calls), 1, "S1 filled, not outlined");
        // A shape drawn as a polygon: filled, with no closed line round it.
        let lines = |m: &Model| {
            draw(m, &DrawOptions::default())
                .iter()
                .filter(|c| matches!(c, Call::Polyline(p, _) if p.len() > 4))
                .count()
        };
        m.find_species_mut("S1").unwrap().shape = NodeShape::Hexagon;
        assert_eq!(lines(&m), 0);
        m.find_species_mut("S1").unwrap().style.no_border = false;
        assert_eq!(lines(&m), 1);
        // A compartment.
        let mut c = Compartment::new("cell");
        c.frame = Some(Frame {
            center: pt(200.0, 0.0),
            width: 600.0,
            height: 200.0,
        });
        c.style.no_border = true;
        m.compartments.push(c);
        let calls = draw(&m, &DrawOptions::default());
        assert!(
            calls
                .iter()
                .any(|c| matches!(c, Call::RoundRect(_, Some(f), None) if *f == COMPARTMENT_FILL))
        );
    }

    #[test]
    fn the_grid_is_drawn_behind_everything_on_screen_only() {
        let m = uni_uni();
        let area = SRect {
            min: spt(0.0, 0.0),
            max: spt(400.0, 300.0),
        };
        let opts = DrawOptions {
            background: Some(area),
            grid: true,
            ..DrawOptions::default()
        };
        // Vertical grid lines, by colour: where they are.
        let verticals = |calls: &[Call], color: Rgba| -> Vec<f32> {
            calls
                .iter()
                .filter_map(|c| match c {
                    Call::Polyline(p, s) if s.color == color && p[0].x == p[1].x => Some(p[0].x),
                    _ => None,
                })
                .collect()
        };
        let calls = draw(&m, &opts);
        let minor = verticals(&calls, GRID_MINOR);
        let major = verticals(&calls, GRID_MAJOR);
        assert!(minor.contains(&20.0) && minor.contains(&380.0));
        assert!(!minor.contains(&100.0), "no fine line under a darker one");
        assert_eq!(major, vec![0.0, 100.0, 200.0, 300.0, 400.0]);
        // Behind everything: right after the background, before any leg.
        let first_grid = calls
            .iter()
            .position(|c| matches!(c, Call::Polyline(_, s) if s.color == GRID_MINOR))
            .unwrap();
        assert_eq!(first_grid, 1);
        let last_grid = calls
            .iter()
            .rposition(|c| matches!(c, Call::Polyline(_, s) if s.color == GRID_MINOR || s.color == GRID_MAJOR))
            .unwrap();
        let first_node = calls
            .iter()
            .position(|c| matches!(c, Call::RoundRect(_, Some(f), _) if *f == SPECIES_FILL))
            .unwrap();
        assert!(last_grid < first_node);
        // It follows zoom and scroll: at 0.2 the fine lines would be 4 px
        // apart, so only the darker ones, 20 px apart, are drawn.
        let far = DrawOptions {
            zoom: 0.2,
            scroll: pt(7.0, 0.0),
            ..opts
        };
        let calls = draw(&m, &far);
        assert!(verticals(&calls, GRID_MINOR).is_empty());
        let major = verticals(&calls, GRID_MAJOR);
        assert!(major.contains(&27.0) && major.contains(&47.0));
        // Never for output, and not unless asked for.
        for o in [
            DrawOptions {
                for_output: true,
                ..opts
            },
            DrawOptions {
                grid: false,
                ..opts
            },
        ] {
            let calls = draw(&m, &o);
            assert!(verticals(&calls, GRID_MINOR).is_empty());
            assert!(verticals(&calls, GRID_MAJOR).is_empty());
        }
    }

    #[test]
    fn a_stem_is_drawn_through_the_junction_and_the_lines_meet_its_ends() {
        // A at (0,0), junction (200,0), B at (400,0); the stem from 160 to 240.
        let mut m = uni_uni();
        let r = m.reactions_mut().next().unwrap();
        r.mode = ReactionMode::Straight;
        r.stem = Some((pt(-40.0, 0.0), pt(40.0, 0.0)));
        let lines: Vec<Vec<SPt>> = draw(&m, &DrawOptions::default())
            .into_iter()
            .filter_map(|c| match c {
                Call::Polyline(p, _) => Some(p),
                _ => None,
            })
            .collect();
        assert!(
            lines.contains(&vec![spt(160.0, 0.0), spt(200.0, 0.0), spt(240.0, 0.0)]),
            "{lines:?}"
        );
        // The reactant's line ends at the stem, the product's starts there.
        assert!(
            lines
                .iter()
                .any(|l| l.len() == 2 && l[1] == spt(160.0, 0.0))
        );
        assert!(
            lines
                .iter()
                .any(|l| l.len() == 2 && l[0] == spt(240.0, 0.0))
        );
        // Selected, each end has a square handle; never in output.
        let handles = |m: &Model, opts: &DrawOptions| {
            draw(m, opts)
                .into_iter()
                .filter(|c| matches!(c, Call::RoundRect(_, Some(f), _) if *f == HANDLE_FILL))
                .count()
        };
        assert_eq!(handles(&m, &DrawOptions::default()), 0);
        m.reactions_mut().next().unwrap().selected = true;
        assert_eq!(handles(&m, &DrawOptions::default()), 2);
        let output = DrawOptions {
            for_output: true,
            ..DrawOptions::default()
        };
        assert_eq!(handles(&m, &output), 0);
        m.reactions_mut().next().unwrap().selected = false;
        // Linear: one line, no stem.
        m.reactions_mut().next().unwrap().mode = ReactionMode::Linear;
        let calls = draw(&m, &DrawOptions::default());
        assert!(
            !calls
                .iter()
                .any(|c| matches!(c, Call::Polyline(p, _) if p.len() == 3))
        );
    }

    #[test]
    fn a_reversible_reaction_has_an_arrowhead_at_both_ends() {
        // A at (0,0) → B at (400,0); A's border is at x 40.
        let arrows = |mode: ReactionMode, reversible: bool, shown: bool| {
            let mut m = uni_uni();
            let r = m.reactions_mut().next().unwrap();
            r.mode = mode;
            r.is_reversible = reversible;
            let opts = DrawOptions {
                show_reversible: shown,
                ..DrawOptions::default()
            };
            draw(&m, &opts)
                .into_iter()
                .filter_map(|c| match c {
                    Call::Polygon(p, _) if p.len() == 3 => Some(p),
                    _ => None,
                })
                .collect::<Vec<_>>()
        };
        for mode in [
            ReactionMode::Straight,
            ReactionMode::Linear,
            ReactionMode::Bezier,
        ] {
            assert_eq!(arrows(mode, false, true).len(), 1, "{mode:?}");
            assert_eq!(arrows(mode, true, false).len(), 1, "{mode:?}: hidden");
            let both = arrows(mode, true, true);
            assert_eq!(both.len(), 2, "{mode:?}");
            // One points at A: its tip 3 right of A's border, its base
            // further right.
            let at_a = both
                .iter()
                .find(|t| (t[0].x - 43.0).abs() < 0.5)
                .unwrap_or_else(|| panic!("{mode:?}: {both:?}"));
            assert!(at_a[1].x > at_a[0].x && at_a[2].x > at_a[0].x, "{mode:?}");
            // And the line from A starts under it, not at the reactant gap.
            let lines: Vec<SPt> = {
                let mut m = uni_uni();
                let r = m.reactions_mut().next().unwrap();
                r.mode = mode;
                r.is_reversible = true;
                draw(&m, &DrawOptions::default())
                    .into_iter()
                    .filter_map(|c| match c {
                        Call::Polyline(p, _) => Some(p[0]),
                        _ => None,
                    })
                    .collect()
            };
            if mode != ReactionMode::Bezier {
                assert!(
                    lines.iter().any(|p| (p.x - (40.0 + 6.0)).abs() < 0.5),
                    "{mode:?}: {lines:?}"
                );
            }
        }
        // With a thick line the line stops halfway under the bigger head:
        // 6 wide, the head is twice the size, so 6 + 24/2 from A's border.
        let mut m = uni_uni();
        let r = m.reactions_mut().next().unwrap();
        r.mode = ReactionMode::Straight;
        r.is_reversible = true;
        r.style.has_custom_style = true;
        r.style.line_width = 6.0;
        let starts: Vec<SPt> = draw(&m, &DrawOptions::default())
            .into_iter()
            .filter_map(|c| match c {
                Call::Polyline(p, _) => Some(p[0]),
                _ => None,
            })
            .collect();
        assert!(
            starts.iter().any(|p| (p.x - (40.0 + 18.0)).abs() < 0.5),
            "{starts:?}"
        );
        // Output follows what is shown.
        let shown = Shown {
            reversible: false,
            ..Shown::ALL
        };
        assert!(!DrawOptions::default().with(shown).show_reversible);
        assert!(DrawOptions::default().with(Shown::ALL).show_reversible);
    }

    #[test]
    fn a_selected_reaction_has_a_plus_handle_on_screen_only() {
        let mut m = uni_uni();
        let plus = |m: &Model, opts: &DrawOptions| {
            draw(m, opts)
                .into_iter()
                .filter(|c| {
                    matches!(c, Call::Circle(_, r, Some(f), _)
                        if *r == ADD_LINE_RADIUS_PX as f32 && *f == HANDLE_FILL)
                })
                .count()
        };
        assert_eq!(plus(&m, &DrawOptions::default()), 0);
        m.reactions_mut().next().unwrap().selected = true;
        assert_eq!(plus(&m, &DrawOptions::default()), 1);
        let output = DrawOptions {
            for_output: true,
            ..DrawOptions::default()
        };
        assert_eq!(plus(&m, &output), 0);
    }

    #[test]
    fn a_compartment_name_is_drawn_at_its_size() {
        let mut m = Model::new();
        let mut c = Compartment::new("cell");
        c.frame = Some(Frame {
            center: pt(100.0, 100.0),
            width: 200.0,
            height: 200.0,
        });
        m.compartments.push(c);
        let size = |m: &Model| {
            draw(m, &DrawOptions::default())
                .into_iter()
                .find_map(|c| match c {
                    Call::Text(_, s, size, _) if s == "cell" => Some(size),
                    _ => None,
                })
                .unwrap()
        };
        assert_eq!(size(&m), 12.0);
        let c = &mut m.compartments[0];
        c.style.has_custom_style = true;
        c.style.font_size = 20.0;
        assert_eq!(size(&m), 20.0);
    }

    #[test]
    fn a_reaction_name_is_drawn_beside_its_junction() {
        let mut m = uni_uni();
        let id = m.reactions()[0].id.clone();
        let names = |m: &Model, opts: &DrawOptions| -> Vec<Call> {
            draw(m, opts)
                .into_iter()
                .filter(|c| matches!(c, Call::Text(_, s, ..) if *s == id))
                .collect()
        };
        assert!(names(&m, &DrawOptions::default()).is_empty(), "hidden");
        let r = m.find_reaction_mut(&id).unwrap();
        r.label_offset = Some(pt(10.0, -20.0));
        assert_eq!(
            names(&m, &DrawOptions::default()),
            vec![Call::Text(spt(210.0, -20.0), id.clone(), 12.0, LABEL)]
        );
        let r = m.find_reaction_mut(&id).unwrap();
        r.style.has_custom_style = true;
        r.style.font_size = 16.0;
        r.style.label_color = Rgba::rgb(0xCC, 0, 0);
        assert_eq!(
            names(&m, &DrawOptions::default()),
            vec![Call::Text(
                spt(210.0, -20.0),
                id.clone(),
                16.0,
                Rgba::rgb(0xCC, 0, 0)
            )]
        );
        // Drawn over the lines and the junction.
        let calls = draw(&m, &DrawOptions::default());
        let name = calls
            .iter()
            .position(|c| matches!(c, Call::Text(_, s, ..) if *s == id))
            .unwrap();
        let last_line = calls
            .iter()
            .rposition(|c| matches!(c, Call::Polyline(..) | Call::Polygon(..)))
            .unwrap();
        assert!(name > last_line);
    }

    #[test]
    fn a_selected_reactions_name_is_outlined_on_screen_only() {
        let mut m = uni_uni();
        let r = m.reactions_mut().next().unwrap();
        r.label_offset = Some(pt(0.0, -20.0));
        let outlines = |m: &Model, opts: &DrawOptions| {
            draw(m, opts)
                .into_iter()
                .filter(|c| {
                    matches!(c, Call::RoundRect(_, None, Some(s))
                        if s.color == SELECTION_HALO && s.width == 1.0)
                })
                .count()
        };
        assert_eq!(outlines(&m, &DrawOptions::default()), 0);
        m.reactions_mut().next().unwrap().selected = true;
        assert_eq!(outlines(&m, &DrawOptions::default()), 1);
        let output = DrawOptions {
            for_output: true,
            ..DrawOptions::default()
        };
        assert_eq!(outlines(&m, &output), 0);
        // Hidden, nothing to outline.
        m.reactions_mut().next().unwrap().label_offset = None;
        assert_eq!(outlines(&m, &DrawOptions::default()), 0);
    }

    #[test]
    fn arrowheads_grow_with_lines_thicker_than_the_default() {
        assert_eq!(
            ArrowSize::for_width(DEFAULT_LINE_WIDTH),
            ArrowSize::STANDARD
        );
        assert_eq!(ArrowSize::for_width(1.0), ArrowSize::STANDARD);
        assert_eq!(ArrowSize::for_width(f64::NAN), ArrowSize::STANDARD);
        // Four times as wide: twice the size, the line ending halfway along.
        let big = ArrowSize::for_width(4.0 * DEFAULT_LINE_WIDTH);
        assert_eq!(
            (big.length, big.half_base, big.tip_inset),
            (
                2.0 * ARROW_LENGTH,
                2.0 * ARROW_HALF_BASE,
                2.0 * ARROW_TIP_INSET
            )
        );
        assert_eq!(big.line_gap, big.tip_inset + big.length / 2.0);
        assert_eq!(ArrowSize::STANDARD.line_gap, PRODUCT_LINE_GAP);
        // Very thick lines: never less than twice the line's width across,
        // in proportion; up to about 16.7 wide the square root still rules.
        let k = (16.0 / DEFAULT_LINE_WIDTH).sqrt();
        assert_eq!(ArrowSize::for_width(16.0).half_base, ARROW_HALF_BASE * k);
        let huge = ArrowSize::for_width(32.0);
        assert_eq!(huge.half_base, 32.0);
        let k = 32.0 / ARROW_HALF_BASE;
        assert_eq!(
            (huge.length, huge.tip_inset),
            (ARROW_LENGTH * k, ARROW_TIP_INSET * k)
        );
        // Its line ends further back than halfway, so its round end stays
        // inside the arrowhead.
        assert!(huge.line_gap > huge.tip_inset + huge.length / 2.0);
        for w in [2.0, 6.0, 10.0, 14.0, 16.0, 20.0, 32.0, 100.0] {
            let a = ArrowSize::for_width(w);
            // From the tip, the end's centre is `d` along; the arrowhead's
            // sides are d·sinθ from it.
            let d = a.line_gap - a.tip_inset;
            let sin = a.half_base / a.half_base.hypot(a.length);
            assert!(d * sin >= w / 2.0 - 1e-9, "width {w}: the end shows");
            assert!(d < a.length, "width {w}: the end is past the base");
            // Halfway, unless that is too far forward.
            assert!(d >= a.length / 2.0);
        }
        assert_eq!(big.line_gap, big.tip_inset + big.length / 2.0);
    }

    /// The arrowhead drawn for A → B, Straight or Linear, with lines `width`
    /// wide (0 for the default), selected or not: its triangle, and the end of
    /// the product line under it.
    fn drawn_arrow(width: f64, selected: bool, mode: ReactionMode) -> ([SPt; 3], SPt, f32) {
        let mut m = uni_uni();
        let r = m.reactions_mut().next().unwrap();
        r.mode = mode;
        if width > 0.0 {
            r.style.has_custom_style = true;
            r.style.line_width = width;
        }
        r.selected = selected;
        let calls = draw(&m, &DrawOptions::default());
        let tri = calls
            .iter()
            .find_map(|c| match c {
                Call::Polygon(p, _) if p.len() == 3 => Some([p[0], p[1], p[2]]),
                _ => None,
            })
            .unwrap();
        // The product leg: the line that ends furthest right (from the
        // junction, or from A for a Linear reaction).
        let (end, w) = calls
            .iter()
            .filter_map(|c| match c {
                Call::Polyline(p, s) if p.len() == 2 => Some((p[1], s.width)),
                _ => None,
            })
            .max_by(|a, b| a.0.x.total_cmp(&b.0.x))
            .unwrap();
        (tri, end, w)
    }

    #[test]
    fn a_thick_line_ends_inside_its_bigger_arrowhead() {
        for mode in [ReactionMode::Straight, ReactionMode::Linear] {
            thick_line_ends_inside(mode);
        }
    }

    fn thick_line_ends_inside(mode: ReactionMode) {
        let (small, ..) = drawn_arrow(0.0, false, mode);
        let (tri, end, width) = drawn_arrow(6.0, false, mode);
        let len = |t: &[SPt; 3]| t[0].x - (t[1].x + t[2].x) / 2.0;
        let half = |t: &[SPt; 3]| (t[1].y - t[2].y).abs() / 2.0;
        assert!((len(&small) - 12.0).abs() < 1e-4, "the default stays");
        assert!(
            (len(&tri) - 24.0).abs() < 1e-4,
            "twice as long at 4x the width"
        );
        assert!((half(&tri) - 10.0).abs() < 1e-4);
        // The tip: 6 before B's border (at x 360).
        assert!((tri[0].x - 354.0).abs() < 1e-4);
        // The line's end, with its round cap, is inside the triangle: at
        // that point the triangle is wider than the line.
        let from_tip = tri[0].x - end.x;
        assert!(from_tip > width / 2.0, "the cap doesn't reach the tip");
        let tri_half_there = half(&tri) * from_tip / len(&tri);
        assert!(tri_half_there > width / 2.0, "{tri_half_there} vs {width}");
        // Selected, the line is drawn wider but the arrowhead stays the same.
        let (sel, ..) = drawn_arrow(6.0, true, mode);
        assert_eq!(sel, tri);
    }

    #[test]
    fn a_selected_very_thick_line_is_only_a_little_wider() {
        assert_eq!(selected_line_width(1.5), 2.25);
        assert_eq!(selected_line_width(4.0), 6.0);
        assert_eq!(selected_line_width(32.0), 34.0);
        for mode in [ReactionMode::Straight, ReactionMode::Linear] {
            let (tri, ..) = drawn_arrow(32.0, false, mode);
            let (sel, end, width) = drawn_arrow(32.0, true, mode);
            assert_eq!(width, 34.0, "{mode:?}");
            assert_eq!(sel, tri, "{mode:?}: the arrowhead stays the same");
            // The wider line's round end is still between the arrowhead's
            // sides.
            let len = sel[0].x - (sel[1].x + sel[2].x) / 2.0;
            let half = (sel[1].y - sel[2].y).abs() / 2.0;
            let sin = half / half.hypot(len);
            let from_tip = sel[0].x - end.x;
            assert!(from_tip * sin >= width / 2.0 - 1e-3, "{mode:?}");
            assert!(from_tip < len, "{mode:?}: the end is before the base");
        }
        // Curved: the product curve's end, as wide, is as far from the tip.
        let curved_end = |selected: bool| {
            let mut m = uni_uni();
            let r = m.reactions_mut().next().unwrap();
            r.mode = ReactionMode::Bezier;
            r.style.has_custom_style = true;
            r.style.line_width = 32.0;
            r.selected = selected;
            draw(&m, &DrawOptions::default())
                .iter()
                .filter_map(|c| match c {
                    Call::Cubic(p, s) => Some((p[3], s.width)),
                    _ => None,
                })
                .max_by(|a, b| a.0.x.total_cmp(&b.0.x))
                .unwrap()
        };
        let (end, width) = curved_end(true);
        let (straight_tri, straight_end, _) = drawn_arrow(32.0, true, ReactionMode::Straight);
        assert_eq!(width, 34.0);
        assert!(
            (end.x - straight_end.x).abs() < 1e-3,
            "{end:?} {straight_end:?}"
        );
        assert!(
            curved_end(false).0.x > end.x,
            "selected, it ends further back"
        );
        assert_eq!(
            straight_tri,
            drawn_arrow(32.0, false, ReactionMode::Straight).0
        );
        // At the default width, selecting changes nothing about the arrowhead.
        let mut m = uni_uni();
        let r = m.reactions_mut().next().unwrap();
        assert_eq!(ArrowSize::drawn(r, true), ArrowSize::STANDARD);
        r.style.has_custom_style = true;
        r.style.line_width = 32.0;
        assert_eq!(ArrowSize::drawn(r, false), ArrowSize::of(r));
    }

    #[test]
    fn locked_nodes_show_a_padlock_on_screen_only() {
        let mut m = uni_uni();
        let plain = draw(&m, &DrawOptions::default()).len();
        m.find_species_mut("S1").unwrap().locked = true;
        let calls = draw(&m, &DrawOptions::default());
        // A body and a shackle, after the node's label.
        assert_eq!(calls.len(), plain + 2);
        let label = calls
            .iter()
            .position(|c| matches!(c, Call::Text(_, s, _, _) if s == "S1"))
            .unwrap();
        assert!(matches!(calls[label + 1], Call::RoundRect(_, Some(f), None) if f == NULL_BORDER));
        assert!(matches!(&calls[label + 2], Call::Polyline(_, s) if s.color == NULL_BORDER));
        let output = DrawOptions {
            for_output: true,
            ..DrawOptions::default()
        };
        assert_eq!(draw(&m, &output).len(), plain);
    }

    #[test]
    fn hidden_junction_is_not_drawn() {
        let mut m = uni_uni();
        m.find_reaction_mut("R3").unwrap().style.junction_visible = false;
        assert!(
            !draw(&m, &DrawOptions::default())
                .iter()
                .any(|c| matches!(c, Call::Circle(..)))
        );
    }

    #[test]
    fn tiny_labels_are_skipped() {
        let opts = DrawOptions {
            zoom: 0.1,
            ..DrawOptions::default()
        };
        assert!(
            !draw(&uni_uni(), &opts)
                .iter()
                .any(|c| matches!(c, Call::Text(..)))
        );
    }

    #[test]
    fn content_bounds_include_nodes_junctions_and_margin() {
        assert_eq!(content_bounds(&Model::new()), (Pt::ZERO, pt(800.0, 600.0)));
        let (min, max) = content_bounds(&uni_uni());
        assert_eq!(min, pt(-40.0 - 60.0, -18.0 - 60.0));
        assert_eq!(max, pt(440.0 + 60.0, 18.0 + 60.0));
    }
}
