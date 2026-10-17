//! The drawing passes (S §5.2), for Straight, Linear and Bézier reactions.
//!
//! The passes skip work the current options don't need, and never draw
//! anything that depends on the selection when drawing for output (S §11.6).

use crate::bezier::{Ctrl, cut_leg, leg_curve};
use crate::geometry::{Pt, filled_arrowhead, normalize, pt, world_to_screen};
use crate::model::{
    BorderDash, Compartment, Frame, Model, NodeShape, Reaction, ReactionMode, Rgba, Role,
    SpeciesNode, TextFont, TextItem, VisualStyle,
};
use crate::render::constants::*;
use crate::render::{Renderer, SPt, SRect, Stroke, spt};
use crate::text::{font_metrics, font_metrics_in, text_width};
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
    /// The colour of a selected reaction's lines, arrowheads and modifier
    /// arcs (a preference; selection accent by default).
    pub selected_reaction: Rgba,
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
            selected_reaction: SELECTED_REACTION,
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
}

impl Shown {
    /// Everything shown: the defaults.
    pub const ALL: Shown = Shown {
        alias_indicator: true,
        null_species: true,
        modifiers: true,
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
        r.round_rect(bg, 0.0, Some(BACKGROUND), None);
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
            d.bezier_handles(reaction, r);
        }
    }
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
    text_width(s.primary_id(), label_size(s)) + 2.0 * FIT_MARGIN
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
    let size = label_size(s);
    let m = font_metrics(size);
    let half = pt(
        text_width(s.primary_id(), size) / 2.0 + LABEL_BOX_PAD,
        (m.ascent - m.descent) / 2.0 + LABEL_BOX_PAD,
    );
    let c = s.center + offset;
    Some((c - half, c + half))
}

/// Where a name taken outside its node starts: just right of the node,
/// [`LABEL_GAP`] away, level with its centre (S §5.11).
pub fn default_label_offset(s: &SpeciesNode) -> Pt {
    let w = text_width(s.primary_id(), label_size(s));
    pt(s.width / 2.0 + LABEL_GAP + w / 2.0, 0.0)
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
    let m = font_metrics(COMPARTMENT_LABEL_SIZE);
    let w = text_width(&c.id, COMPARTMENT_LABEL_SIZE);
    let a = f.min() + pt(COMPARTMENT_LABEL_INSET, COMPARTMENT_LABEL_INSET);
    (a, a + pt(w, m.ascent - m.descent))
}

/// The fill, border colour and border width a compartment is drawn with.
pub fn compartment_colours(c: &Compartment) -> (Rgba, Rgba, f64) {
    let style = custom(&c.style);
    (
        pick(style.map(|s| s.fill_color), COMPARTMENT_FILL),
        pick(style.map(|s| s.border_color), COMPARTMENT_BORDER),
        pick_width(style.map(|s| s.border_width), COMPARTMENT_BORDER_WIDTH),
    )
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
        let style = custom(&r.style);
        let mut color = line_colour(r);
        let mut width = pick_width(style.map(|s| s.line_width), DEFAULT_LINE_WIDTH);
        if self.selected(r.selected) {
            width *= SELECTED_LINE_FACTOR;
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
            let start = leg_start(a, b.center);
            let (end, tip) = leg_end(b, a.center);
            out.polyline(&[self.to_screen(start), self.to_screen(end)], stroke);
            self.arrowhead(tip, b.center - a.center, stroke.color, out);
            return;
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
                    let leg = cut_leg(&curve, node, role);
                    let c = leg.curve;
                    out.cubic(
                        self.to_screen(c.p0),
                        self.to_screen(c.p1),
                        self.to_screen(c.p2),
                        self.to_screen(c.p3),
                        stroke,
                    );
                    if let Some((tip, dir)) = leg.arrow {
                        self.arrowhead(tip, dir, stroke.color, out);
                    }
                }
            }
            return;
        }

        let j = r.junction;
        for p in &r.reactants {
            if let Some(node) = self.visible(&p.species) {
                let start = leg_start(node, j);
                out.polyline(&[self.to_screen(start), self.to_screen(j)], stroke);
            }
        }
        for p in &r.products {
            if let Some(node) = self.visible(&p.species) {
                let (end, tip) = leg_end(node, j);
                out.polyline(&[self.to_screen(j), self.to_screen(end)], stroke);
                self.arrowhead(tip, node.center - j, stroke.color, out);
            }
        }
    }

    fn arrowhead(&self, tip: Pt, dir: Pt, color: Rgba, out: &mut dyn Renderer) {
        let tri = filled_arrowhead(tip, dir, ARROW_LENGTH, ARROW_HALF_BASE);
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
        let color = if self.selected(r.selected) {
            self.opts.selected_reaction
        } else {
            MODIFIER
        };
        let stroke = Stroke {
            width: self.len(DEFAULT_LINE_WIDTH),
            color,
            dash: self.dash(MODIFIER_DASH),
            round_cap: false,
        };
        for m in &r.modifiers {
            let Some(node) = self.visible(&m.species) else {
                continue;
            };
            let c = node.center;
            if c.distance(j) < 1.0 {
                continue;
            }
            // Starts inside the node, so the node hides the start.
            let toward_j = normalize(j - c);
            let start = border(node, j) - toward_j * MODIFIER_INSET;
            let dot = j - toward_j * MODIFIER_DOT_OFFSET;
            out.polyline(&[self.to_screen(start), self.to_screen(dot)], stroke);
            out.circle(
                self.to_screen(dot),
                self.len(MODIFIER_DOT_RADIUS),
                Some(color),
                None,
            );
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
        let mut border_width = pick_width(style.map(|s| s.border_width), DEFAULT_LINE_WIDTH);
        if node.is_boundary {
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
                pts.push(pts[0]);
                out.polyline(&pts, stroke);
            }
            None => {
                let radius = if node.shape == NodeShape::Rectangle {
                    0.0
                } else {
                    NODE_CORNER_RADIUS
                };
                let rect = self.rect(node.center, node.width / 2.0, node.height / 2.0);
                out.round_rect(rect, self.len(radius), Some(fill), Some(stroke));
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
                TextFont::default(),
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
            Some(Stroke::solid(self.len(width), border)),
        );
        let size = COMPARTMENT_LABEL_SIZE * self.opts.zoom;
        if size >= MIN_LABEL_SIZE_PX {
            let (a, b) = compartment_label_box(c, f);
            let cap = font_metrics(COMPARTMENT_LABEL_SIZE).cap_height;
            let m = font_metrics(COMPARTMENT_LABEL_SIZE);
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
    let b = border(node, from);
    let back = normalize(from - node.center);
    (b + back * PRODUCT_LINE_GAP, b + back * ARROW_TIP_INSET)
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
        fn cubic(&mut self, _: SPt, _: SPt, _: SPt, _: SPt, _: Stroke) {
            panic!("no cubics before R5");
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
                Call::Polyline(..) | Call::Polygon(..) => "leg",
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
