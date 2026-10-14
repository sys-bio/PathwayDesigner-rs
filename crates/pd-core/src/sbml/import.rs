//! SBML Level 2 or 3 into a new model, with its layout when it has one
//! (S §16.2 F-1, SPEC-Rust §8.3).

use std::collections::{HashMap, HashSet};

use sbml_io as sb;

use super::math::from_math;
use crate::antimony::ImportOptions;
use crate::expr::{self, Ast};
use crate::geometry::{Pt, pt};
use crate::layout::Rng;
use crate::model::{
    AssignmentRule, Compartment, DEFAULT_COMPARTMENT, Model, Parameter, Participant, Reaction,
    ReactionMode, SpeciesNode, is_valid_sid,
};
use crate::passes::{self, NullPlace};

/// Species without a glyph go at random in [0, 600)², as for Antimony.
const PLACEMENT: f64 = 600.0;

/// A successful import.
#[derive(Clone, Debug)]
pub struct Imported {
    pub model: Model,
    /// What was left out or changed; the import went ahead anyway.
    pub warnings: Vec<String>,
    /// Whether the file had a layout.
    pub had_layout: bool,
    /// Species placed at random because the layout had no glyph for them
    /// (all of them, without a layout). The app offers auto-layout if any.
    pub unplaced: usize,
}

/// Reads SBML into a new model. An error means no model: the caller leaves
/// the current diagram as it is.
pub fn import(xml: &str, opts: &ImportOptions) -> Result<Imported, Vec<sb::Diagnostic>> {
    let read = sb::read(xml)?;
    let Some(m) = read.document.model.as_ref() else {
        return Err(vec![sb::Diagnostic {
            line: 1,
            column: 1,
            message: "the file has no model".into(),
        }]);
    };
    let mut b = Builder {
        level: read.document.level,
        warnings: read.warnings.iter().map(|d| d.to_string()).collect(),
        model: Model::new(),
        rng: Rng::new(opts.seed),
    };
    let (had_layout, unplaced) = b.build(m, opts);
    Ok(Imported {
        model: b.model,
        warnings: b.warnings,
        had_layout,
        unplaced,
    })
}

struct Builder {
    level: u32,
    warnings: Vec<String>,
    model: Model,
    rng: Rng,
}

/// What the layout says about a reaction's empty side: where its null node
/// goes, and its handles.
#[derive(Default)]
struct NullLeg {
    place: Option<NullPlace>,
    handles: Option<(Pt, Pt)>,
}

impl Builder {
    fn warn(&mut self, message: String) {
        self.warnings.push(message);
    }

    /// An expression from MathML, or a warning saying why not.
    fn expression(
        &mut self,
        math: Option<&sb::Math>,
        rename: &HashMap<String, String>,
        what: &str,
    ) -> Option<Ast> {
        match from_math(math?, rename) {
            Ok(ast) => Some(ast),
            Err(why) => {
                self.warn(format!("{what} was left out: it uses {why}"));
                None
            }
        }
    }

    fn build(&mut self, m: &sb::Model, opts: &ImportOptions) -> (bool, usize) {
        self.report_unsupported(m);
        self.model.model_name = m.name.clone().or_else(|| m.id.clone()).unwrap_or_default();
        let none = HashMap::new();

        // Compartments. The default one is implied, unless it was changed.
        for c in &m.compartments {
            let comp = Compartment {
                size: c.size.unwrap_or(1.0),
                dimensions: c.spatial_dimensions.map_or(3, |d| d as u32),
                ..Compartment::new(c.id.clone())
            };
            let implied = c.id == DEFAULT_COMPARTMENT && comp.size == 1.0 && comp.dimensions == 3;
            if !implied {
                self.model.compartments.push(comp);
            }
        }

        // Species: initialConcentration, or initialAmount if that is missing
        // or 0 (SPEC-Rust §8.3).
        for s in &m.species {
            let mut node = SpeciesNode::new(s.id.clone(), Pt::ZERO);
            node.initial_value = match (s.initial_concentration, s.initial_amount) {
                (Some(c), _) if c != 0.0 => c,
                (_, Some(a)) => a,
                (c, None) => c.unwrap_or(0.0),
            };
            node.is_boundary = s.boundary_condition.unwrap_or(false);
            node.is_constant = s.constant.unwrap_or(false);
            if s.compartment != DEFAULT_COMPARTMENT {
                node.compartment = s.compartment.clone();
            }
            if self.model.push_species(node).is_err() {
                self.warn(format!(
                    "a second species is called {}; it was left out",
                    s.id
                ));
            }
        }

        // Parameters. One set by an assignment rule is the rule's variable.
        let rule_vars: HashSet<&str> = m
            .rules
            .iter()
            .filter(|r| matches!(r, sb::Rule::Assignment { .. }))
            .filter_map(sb::Rule::variable)
            .collect();
        for p in &m.parameters {
            if !rule_vars.contains(p.id.as_str()) {
                self.model.parameters.push(Parameter {
                    variable: p.id.clone(),
                    expression: sb::fmt_f64(p.value.unwrap_or(0.0)),
                });
            }
        }
        for ia in &m.initial_assignments {
            let what = format!("The initial assignment to {}", ia.symbol);
            let Some(ast) = self.expression(ia.math.as_ref(), &none, &what) else {
                continue;
            };
            let text = expr::to_infix(&ast);
            let number = text.parse::<f64>().ok();
            if let Some(p) = self
                .model
                .parameters
                .iter_mut()
                .find(|p| p.variable == ia.symbol)
            {
                p.expression = text;
            } else if let (Some(v), Some(s)) = (number, self.model.find_species_mut(&ia.symbol)) {
                s.initial_value = v;
            } else if let (Some(v), Some(c)) = (
                number,
                self.model
                    .compartments
                    .iter_mut()
                    .find(|c| c.id == ia.symbol),
            ) {
                c.size = v;
            } else {
                self.warn(format!("{what} was left out: only parameters can have one"));
            }
        }
        for r in &m.rules {
            if let sb::Rule::Assignment { variable, math, .. } = r {
                let what = format!("The rule for {variable}");
                if let Some(ast) = self.expression(math.as_ref(), &none, &what) {
                    self.model.assignment_rules.push(AssignmentRule {
                        variable: variable.clone(),
                        expression: expr::to_infix(&ast),
                    });
                }
            }
        }

        // The layout: species first, so reactions can use their aliases. A
        // layout where no species glyph has a position counts as none.
        let layout = m.layouts.first().filter(|l| {
            l.species_glyphs
                .iter()
                .any(|g| positioned(g.bounding_box.as_ref()).is_some())
        });
        let (glyph_node, nulls) = match layout {
            Some(l) => self.place_species(l),
            None => (HashMap::new(), HashMap::new()),
        };
        let unplaced = self.place_the_rest(layout);

        for r in &m.reactions {
            self.reaction(m, r, layout, &glyph_node, &nulls, opts);
        }
        // Compartment boxes (S §7.9), and their style from PathwayDesigner's
        // note. Compartments without one are fitted around their species.
        if let Some(l) = m.layouts.first() {
            for g in &l.compartment_glyphs {
                let (Some(id), Some(b)) = (&g.compartment, positioned(g.bounding_box.as_ref()))
                else {
                    continue;
                };
                let (w, h) = (b.dimensions.width, b.dimensions.height);
                let style = g
                    .meta
                    .annotation
                    .as_deref()
                    .and_then(super::shape::read_compartment);
                if let Some(c) = self.model.find_compartment_mut(id)
                    && w >= 1.0
                    && h >= 1.0
                {
                    c.frame = Some(crate::model::Frame {
                        center: pt(b.position.x + w / 2.0, b.position.y + h / 2.0),
                        width: w,
                        height: h,
                    });
                    if let Some(s) = style {
                        c.style = s;
                    }
                }
            }
        }
        // Free text: a text glyph with words of its own and nothing it labels
        // (S §7.8). Its look comes from PathwayDesigner's note, if it has one.
        if let Some(l) = m.layouts.first() {
            for g in &l.text_glyphs {
                let (None, None, Some(words)) = (&g.graphical_object, &g.origin_of_text, &g.text)
                else {
                    continue;
                };
                let at = positioned(g.bounding_box.as_ref())
                    .map_or(Pt::ZERO, |b| pt(b.position.x, b.position.y));
                let id = self.model.add_text(at, words);
                let look = g
                    .meta
                    .annotation
                    .as_deref()
                    .and_then(super::shape::read_text);
                if let (Some(look), Some(t)) = (look, self.model.find_text_mut(&id)) {
                    t.font = look.font;
                    t.align = look.align;
                    t.size = look.size;
                    t.color = look.color;
                    t.wrap_width = look.wrap_width;
                }
            }
        }
        self.model.recompute_id_counter();
        (layout.is_some(), unplaced)
    }

    /// Says what the model has that a diagram can't hold.
    fn report_unsupported(&mut self, m: &sb::Model) {
        let rules = m
            .rules
            .iter()
            .filter(|r| !matches!(r, sb::Rule::Assignment { .. }))
            .count();
        for (n, one, many) in [
            (
                m.function_definitions.len(),
                "function definition",
                "function definitions",
            ),
            (m.events.len(), "event", "events"),
            (rules, "rate or algebraic rule", "rate and algebraic rules"),
            (m.constraints.len(), "constraint", "constraints"),
        ] {
            match n {
                0 => {}
                1 => self.warn(format!("1 {one} was left out")),
                n => self.warn(format!("{n} {many} were left out")),
            }
        }
        if m.layouts.len() > 1 {
            self.warn(format!(
                "{} layouts: only the first was used",
                m.layouts.len()
            ));
        }
    }

    /// Positions and sizes from species glyphs. A species' first glyph places
    /// it; each further glyph becomes an alias. Returns which node each glyph
    /// stands for, and the boxes of glyphs that name no species (null nodes).
    fn place_species(
        &mut self,
        l: &sb::Layout,
    ) -> (HashMap<String, String>, HashMap<String, NullPlace>) {
        let mut glyph_node = HashMap::new();
        let mut nulls = HashMap::new();
        let mut placed: HashSet<String> = HashSet::new();
        for g in &l.species_glyphs {
            let Some(bb) = positioned(g.bounding_box.as_ref()) else {
                continue;
            };
            let (w, h) = (bb.dimensions.width, bb.dimensions.height);
            let center = pt(bb.position.x + w / 2.0, bb.position.y + h / 2.0);
            let species = g
                .species
                .as_deref()
                .filter(|s| self.model.find_species(s).is_some_and(|n| !n.is_null_node));
            let Some(species) = species else {
                nulls.insert(
                    g.id.clone(),
                    NullPlace {
                        center,
                        size: Some((w, h)),
                    },
                );
                continue;
            };
            if placed.insert(species.to_owned()) {
                let node = self.model.find_species_mut(species).expect("checked above");
                (node.center, node.width, node.height) = (center, w, h);
                glyph_node.insert(g.id.clone(), species.to_owned());
                continue;
            }
            // An alias keeps the id it was exported with (`sg_<id>`) if it can.
            let wanted =
                g.id.strip_prefix("sg_")
                    .filter(|id| is_valid_sid(id) && !self.model.is_id_used(id));
            let id = match wanted {
                Some(id) => {
                    let mut node = SpeciesNode::new(id, center);
                    node.alias_of = Some(species.to_owned());
                    self.model.push_species(node).expect("checked unused");
                    id.to_owned()
                }
                None => self
                    .model
                    .add_alias(species, center)
                    .expect("species exists"),
            };
            let node = self.model.find_species_mut(&id).expect("just added");
            (node.width, node.height) = (w, h);
            glyph_node.insert(g.id.clone(), id);
        }
        (glyph_node, nulls)
    }

    /// Puts species the layout didn't place at random, inside the layout if
    /// there is one. Returns how many.
    fn place_the_rest(&mut self, layout: Option<&sb::Layout>) -> usize {
        let placed: HashSet<String> = layout
            .map(|l| {
                l.species_glyphs
                    .iter()
                    .filter(|g| positioned(g.bounding_box.as_ref()).is_some())
                    .filter_map(|g| g.species.clone())
                    .collect()
            })
            .unwrap_or_default();
        let (w, h) = layout
            .map(|l| (l.dimensions.width.max(1.0), l.dimensions.height.max(1.0)))
            .unwrap_or((PLACEMENT, PLACEMENT));
        let mut n = 0;
        let ids: Vec<String> = self
            .model
            .species()
            .iter()
            .filter(|s| !s.is_alias() && !placed.contains(&s.id))
            .map(|s| s.id.clone())
            .collect();
        for id in ids {
            let at = pt(self.rng.range(0.0, w), self.rng.range(0.0, h));
            self.model.find_species_mut(&id).expect("listed").center = at;
            n += 1;
        }
        if layout.is_some() && n > 0 {
            let were = if n == 1 { "was" } else { "were" };
            self.warn(format!(
                "{n} species had no glyph and {were} placed at random"
            ));
        }
        n
    }

    fn reaction(
        &mut self,
        m: &sb::Model,
        r: &sb::Reaction,
        layout: Option<&sb::Layout>,
        glyph_node: &HashMap<String, String>,
        nulls: &HashMap<String, NullPlace>,
        opts: &ImportOptions,
    ) {
        if self.model.is_id_used(&r.id) {
            self.warn(format!(
                "the reaction id {} is used twice; the second was left out",
                r.id
            ));
            return;
        }
        let mut rx = Reaction::new(r.id.clone(), Pt::ZERO);
        rx.mode = ReactionMode::Straight;
        // Level 2 makes reactions reversible unless they say otherwise.
        rx.is_reversible = r.reversible.unwrap_or(self.level == 2);

        // Species references, remembering each one's id for the layout.
        let mut refs: Vec<(Option<String>, crate::model::Role, usize)> = Vec::new();
        for (role, list) in [
            (crate::model::Role::Reactant, &r.reactants),
            (crate::model::Role::Product, &r.products),
        ] {
            for sr in list {
                if self.model.find_species(&sr.species).is_none() {
                    self.warn(format!(
                        "reaction {} uses {}, which isn't a species; it was left out",
                        r.id, sr.species
                    ));
                    continue;
                }
                let mut n = sr.stoichiometry.unwrap_or(1.0);
                if let Some(math) = &sr.stoichiometry_math {
                    match from_math(math, &HashMap::new())
                        .map(|a| expr::to_infix(&a).parse::<f64>())
                    {
                        Ok(Ok(v)) => n = v,
                        _ => self.warn(format!(
                            "reaction {}: a stoichiometry that isn't a number was read as 1",
                            r.id
                        )),
                    }
                }
                let list = rx.participants_mut(role);
                refs.push((sr.id.clone(), role, list.len()));
                list.push(Participant::new(sr.species.as_str(), n));
            }
        }
        for mr in &r.modifiers {
            if self.model.find_species(&mr.species).is_none() {
                continue;
            }
            if !rx.modifiers.iter().any(|p| p.species == mr.species) {
                refs.push((
                    mr.id.clone(),
                    crate::model::Role::Modifier,
                    rx.modifiers.len(),
                ));
                rx.modifiers
                    .push(Participant::new(mr.species.as_str(), 1.0));
            }
        }

        // Local parameters become global ones, renamed if the name is taken.
        let mut rename = HashMap::new();
        let mut law = None;
        if let Some(kl) = &r.kinetic_law {
            for lp in &kl.parameters {
                let taken = |id: &str, b: &Builder| {
                    b.model.is_id_used(id)
                        || b.model.find_parameter(id).is_some()
                        || b.model.find_compartment(id).is_some()
                        || m.reactions.iter().any(|x| x.id == id)
                };
                let mut name = lp.id.clone();
                if taken(&name, self) {
                    name = format!("{}_{}", r.id, lp.id);
                    let mut k = 2;
                    while taken(&name, self) {
                        name = format!("{}_{}_{k}", r.id, lp.id);
                        k += 1;
                    }
                    rename.insert(lp.id.clone(), name.clone());
                }
                self.model.parameters.push(Parameter {
                    variable: name,
                    expression: sb::fmt_f64(lp.value.unwrap_or(0.0)),
                });
            }
            law = self.expression(
                kl.math.as_ref(),
                &rename,
                &format!("The rate law of {}", r.id),
            );
        }
        if let Some(ast) = &law {
            rx.kinetic_law = expr::to_infix(ast);
        }

        // The layout: the junction, which node each leg is drawn to, handles.
        let glyph = layout.and_then(|l| {
            l.reaction_glyphs
                .iter()
                .find(|g| g.reaction.as_deref() == Some(r.id.as_str()))
        });
        rx.junction = match glyph.and_then(junction_of) {
            Some(j) => j,
            None => self.centroid(&rx),
        };
        let mut source = NullLeg::default();
        let mut sink = NullLeg::default();
        if let Some(g) = glyph {
            let mut matched: HashSet<(crate::model::Role, usize)> = HashSet::new();
            for srg in &g.species_reference_glyphs {
                let role = srg.role.as_ref();
                let handles = handles(srg.curve.as_ref());
                // A glyph that names no species: a null node.
                if let Some(place) = nulls.get(&srg.species_glyph) {
                    let is_source =
                        !matches!(role, Some(sb::Role::Product | sb::Role::SideProduct));
                    let leg = if is_source { &mut source } else { &mut sink };
                    leg.place = Some(*place);
                    leg.handles = handles.map(|(a, b, c1, c2)| {
                        orient(a, b, c1, c2, rx.junction, place.center, is_source)
                    });
                    continue;
                }
                let Some(node) = glyph_node.get(&srg.species_glyph) else {
                    continue;
                };
                let primary = self
                    .model
                    .find_species(node)
                    .map(|n| n.primary_id().to_owned())
                    .unwrap_or_default();
                let found = find_participant(&rx, &refs, &matched, srg, &primary);
                let Some((prole, index)) = found else {
                    continue;
                };
                matched.insert((prole, index));
                let center = self
                    .model
                    .find_species(node)
                    .map(|n| n.center)
                    .unwrap_or_default();
                let junction = rx.junction;
                let p = &mut rx.participants_mut(prole)[index];
                p.species = node.clone();
                if prole != crate::model::Role::Modifier
                    && let Some((a, b, c1, c2)) = handles
                {
                    let reactant = prole == crate::model::Role::Reactant;
                    (p.ctrl1, p.ctrl2) = orient(a, b, c1, c2, junction, center, reactant);
                    p.ctrl_pts_set = true;
                    rx.mode = ReactionMode::Bezier;
                }
            }
        }

        // PathwayDesigner's own note: Linear, and smooth junctions.
        if let Some(note) = glyph.and_then(|g| g.meta.annotation.as_deref()) {
            let (linear, smooth) = super::shape::read(note);
            if linear {
                rx.mode = ReactionMode::Linear;
            } else if smooth && rx.mode == ReactionMode::Bezier {
                rx.junction_smooth = true;
            }
        }

        // The passes every import gets (SPEC-Rust §8.4).
        if opts.convert_catalytic {
            passes::catalytic_rewrite(&mut rx);
        }
        if let Some(ast) = &law {
            passes::modifiers_from_law(&self.model, &mut rx, ast);
        }
        passes::add_null_nodes(&mut self.model, &mut rx, source.place, sink.place);
        for (leg, id) in [
            (&source, format!("_source_{}", rx.id)),
            (&sink, format!("_sink_{}", rx.id)),
        ] {
            let Some((c1, c2)) = leg.handles else {
                continue;
            };
            for p in rx
                .reactants
                .iter_mut()
                .chain(rx.products.iter_mut())
                .filter(|p| p.species == id)
            {
                (p.ctrl1, p.ctrl2, p.ctrl_pts_set) = (c1, c2, true);
                rx.mode = ReactionMode::Bezier;
            }
        }
        self.model.push_reaction(rx).expect("checked unused above");
    }

    /// Halfway between the reactants' and the products' centres.
    fn centroid(&self, r: &Reaction) -> Pt {
        let centre = |list: &[Participant]| {
            let pts: Vec<Pt> = list
                .iter()
                .filter_map(|p| self.model.find_species(&p.species))
                .map(|s| s.center)
                .collect();
            (!pts.is_empty()).then(|| pts.iter().fold(Pt::ZERO, |a, &p| a + p) / pts.len() as f64)
        };
        match (centre(&r.reactants), centre(&r.products)) {
            (Some(a), Some(b)) => (a + b) / 2.0,
            (Some(a), None) | (None, Some(a)) => a,
            (None, None) => pt(PLACEMENT / 2.0, PLACEMENT / 2.0),
        }
    }
}

/// A bounding box that gives a position. libSBML writes an all-zero box
/// for a glyph that was never given one, so that counts as no box.
fn positioned(bb: Option<&sb::BoundingBox>) -> Option<&sb::BoundingBox> {
    bb.filter(|b| {
        let (p, d) = (&b.position, &b.dimensions);
        p.x != 0.0 || p.y != 0.0 || d.width != 0.0 || d.height != 0.0
    })
}

/// A reaction glyph's junction: the centre of its bounding box, or else the
/// middle of its curve.
fn junction_of(g: &sb::ReactionGlyph) -> Option<Pt> {
    if let Some(bb) = positioned(g.bounding_box.as_ref()) {
        let d = &bb.dimensions;
        return Some(pt(
            bb.position.x + d.width / 2.0,
            bb.position.y + d.height / 2.0,
        ));
    }
    let segs = &g.curve.as_ref()?.segments;
    let (a, b) = (segs.first()?.start(), segs.last()?.end());
    Some(pt((a.x + b.x) / 2.0, (a.y + b.y) / 2.0))
}

/// A curve's ends and handles: (start, end, base point 1, base point 2), or
/// `None` unless it has a Bézier segment. With several segments, the first
/// segment gives the first handle and the last the second.
fn handles(curve: Option<&sb::Curve>) -> Option<(Pt, Pt, Pt, Pt)> {
    let segs = &curve?.segments;
    if !segs.iter().any(|s| matches!(s, sb::Segment::Bezier { .. })) {
        return None;
    }
    let p = |q: sb::Point| pt(q.x, q.y);
    let (first, last) = (segs.first()?, segs.last()?);
    let c1 = match first {
        sb::Segment::Bezier { base1, .. } => p(*base1),
        s => p(s.start()),
    };
    let c2 = match last {
        sb::Segment::Bezier { base2, .. } => p(*base2),
        s => p(s.end()),
    };
    Some((p(first.start()), p(last.end()), c1, c2))
}

/// `(ctrl1, ctrl2)` for a leg whose curve runs from `start` to `end`. The
/// handles are taken as stored (S §11.5), unless the curve runs the other way
/// round from this app's convention (a reactant's from the species to the
/// junction, a product's from the junction to the species), as some tools
/// write it.
fn orient(
    start: Pt,
    end: Pt,
    c1: Pt,
    c2: Pt,
    junction: Pt,
    species: Pt,
    reactant: bool,
) -> (Pt, Pt) {
    let from_species = start.distance(species) + end.distance(junction)
        <= start.distance(junction) + end.distance(species);
    if from_species == reactant {
        (c1, c2)
    } else {
        (c2, c1)
    }
}

/// The participant a species reference glyph draws: the one its
/// `speciesReference` names, or else the first not yet matched with the
/// glyph's species, in the list its role says.
fn find_participant(
    rx: &Reaction,
    refs: &[(Option<String>, crate::model::Role, usize)],
    matched: &HashSet<(crate::model::Role, usize)>,
    srg: &sb::SpeciesReferenceGlyph,
    species: &str,
) -> Option<(crate::model::Role, usize)> {
    use crate::model::Role;
    if let Some(id) = &srg.species_reference
        && let Some((_, role, index)) = refs.iter().find(|(r, ..)| r.as_ref() == Some(id))
    {
        return Some((*role, *index));
    }
    let roles: &[Role] = match srg.role {
        Some(sb::Role::Substrate | sb::Role::SideSubstrate) => &[Role::Reactant],
        Some(sb::Role::Product | sb::Role::SideProduct) => &[Role::Product],
        Some(sb::Role::Modifier | sb::Role::Activator | sb::Role::Inhibitor) => &[Role::Modifier],
        _ => &[Role::Reactant, Role::Product, Role::Modifier],
    };
    roles.iter().find_map(|&role| {
        rx.participants(role)
            .iter()
            .enumerate()
            .find(|(i, p)| p.species == species && !matched.contains(&(role, *i)))
            .map(|(i, _)| (role, i))
    })
}
