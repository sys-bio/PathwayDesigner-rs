//! Antimony's layout statements (Antimony 3: `S1.pos = {28, 35}`,
//! `J0.position = {80, 153}`, `S1.position.J2 = {100, 300}`) turned into the
//! drawing: docs/antimony-layout-design.md §4.
//!
//! The `antimony` crate reads the statements and keeps them as data; what
//! they mean for a diagram is decided here. Positions in Antimony are the
//! top-left corners of SBML Layout bounding boxes, and libAntimony gives a
//! species 60 × 36 and a reaction 20 × 20 when the file gives no size, so a
//! centre is the corner plus half the size.
//!
//! A line between a reaction and a species (`J0.S1.b1`, §5) is the curve of
//! an SBML species reference glyph written as statements, and is read
//! through the same code as SBML import reads such curves. Looks
//! (`S1.color`, §6) are read in [`crate::antimony_looks`].

use std::collections::{BTreeMap, HashMap};

use antimony::Span;
use antimony::model::{LayoutStatement, LayoutValue, Module};

use crate::antimony::{Diagnostic, Severity};
use crate::geometry::{Pt, pt};
use crate::model::{ALIAS_OFFSET, Frame, Model, ReactionMode, Role};
use crate::passes::NULL_OFFSET;
use crate::sbml::import::{leg_from_curve, modifier_points, null_leg_handles};
use sbml_io as sb;

/// libAntimony's species size when a file gives none.
pub const SPECIES_SIZE: (f64, f64) = (60.0, 36.0);
/// libAntimony's reaction size: the square at the reaction's centre.
pub const REACTION_SIZE: (f64, f64) = (20.0, 20.0);
/// How near a node's centre a line, carried on, must pass to count as
/// drawn to it: libAntimony writes numbers to two decimals.
const ROUNDED: f64 = 0.1;
/// libAntimony's size for a reaction's null node.
pub const NULL_SIZE: (f64, f64) = (30.0, 30.0);
/// Antimony's name for the compartment of species given none, which this
/// app leaves undrawn.
const ANTIMONY_DEFAULT_COMPARTMENT: &str = "default_compartment";

/// What the statements did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Applied {
    /// Some species had a position.
    pub had_layout: bool,
    /// Species without one, left where the import put them (at random),
    /// when others had one.
    pub unplaced: usize,
}

/// A position and a size, each part optional: `S1.x` alone gives only `x`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Place {
    x: Option<f64>,
    y: Option<f64>,
    w: Option<f64>,
    h: Option<f64>,
}

impl Place {
    /// The top-left corner, if either coordinate is given; the other is 0,
    /// as the reference says.
    fn corner(&self) -> Option<Pt> {
        (self.x.is_some() || self.y.is_some())
            .then(|| pt(self.x.unwrap_or(0.0), self.y.unwrap_or(0.0)))
    }

    fn has_size(&self) -> bool {
        self.w.is_some() || self.h.is_some()
    }

    /// The size, each part from here, else from `fallback` (a category's),
    /// else `default`.
    fn size(&self, fallback: &Place, default: (f64, f64)) -> (f64, f64) {
        (
            self.w.or(fallback.w).unwrap_or(default.0),
            self.h.or(fallback.h).unwrap_or(default.1),
        )
    }
}

/// Everything the statements say about places, gathered first so that the
/// order of the statements doesn't matter.
#[derive(Default)]
struct Places {
    species: HashMap<String, Place>,
    reactions: HashMap<String, Place>,
    compartments: HashMap<String, Place>,
    /// A reaction's null node (`J0.--`).
    nulls: HashMap<String, Place>,
    /// `S1.position.J3.J4`: species, the reactions drawn to it, its place.
    aliases: Vec<(String, Vec<String>, Place, Span)>,
    species_default: Place,
    reaction_default: Place,
    /// The first `align_…` or `autolayout` statement, which only
    /// SBMLNetwork's automatic layout uses.
    autolayout: Option<Span>,
    /// Lines: (reaction, the species or `None` for its null node) to the
    /// line's segments by number (`seg2`), from the species.
    lines: BTreeMap<(String, Option<String>), BTreeMap<u32, Segment>>,
    /// The first `arc2` or later: a second line to the same species.
    further_arc: Option<Span>,
}

/// One segment of a line, as the statements give it: its ends (the species'
/// end, the reaction's end) and its two control points (`b1` on the
/// reaction's side, `b2` on the species').
#[derive(Clone, Copy, Debug, Default)]
struct Segment {
    species_end: Option<Pt>,
    rxn_end: Option<Pt>,
    b1: Option<Pt>,
    b2: Option<Pt>,
}

/// What a dotted name refers to.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Item {
    Species,
    Reaction,
    Compartment,
}

/// Applies `module`'s layout statements to `model`, the import's result.
/// Without any, the model is left exactly as it is.
pub fn apply(model: &mut Model, module: &Module, warnings: &mut Vec<Diagnostic>) -> Applied {
    if module.layout.is_empty() {
        return Applied::default();
    }
    let mut places = Places::default();
    for s in &module.layout {
        if let Err(message) = gather(model, s, &mut places) {
            warnings.push(Diagnostic::at(Severity::Warning, s.span, message));
        }
    }
    if let Some(span) = places.autolayout {
        warnings.push(Diagnostic::at(
            Severity::Warning,
            span,
            "Antimony's automatic-layout settings (align_…, autolayout) aren't used; \
             PathwayDesigner lays the model out itself",
        ));
    }

    if let Some(span) = places.further_arc {
        warnings.push(Diagnostic::at(
            Severity::Warning,
            span,
            "Second lines to a species (arc2, …) were skipped: PathwayDesigner draws one \
             line from each species of a reaction",
        ));
    }

    let applied = place_species(model, &places, warnings);
    place_aliases(model, &places, warnings);
    place_reactions(model, &places);
    place_compartments(model, &places);
    place_lines(model, &places, warnings);
    crate::antimony_looks::apply(model, module, warnings);
    applied
}

/// The lines (§5): each becomes an SBML curve, from the reaction to the
/// species as libAntimony writes it, and is read as SBML import reads one.
/// A reaction with handles on any line is drawn curved (D3).
fn place_lines(model: &mut Model, places: &Places, warnings: &mut Vec<Diagnostic>) {
    let mut bent = 0;
    for ((reaction, species), segments) in &places.lines {
        let Some(r) = model.find_reaction(reaction) else {
            continue;
        };
        let junction = r.junction;
        // Which participant, and where its node is.
        let found = match species {
            Some(id) => [Role::Reactant, Role::Product, Role::Modifier]
                .into_iter()
                .find_map(|role| {
                    r.participants(role)
                        .iter()
                        .position(|p| {
                            model
                                .find_species(&p.species)
                                .is_some_and(|n| n.primary_id() == id)
                        })
                        .map(|i| (role, i))
                }),
            None => [Role::Reactant, Role::Product]
                .into_iter()
                .find_map(|role| {
                    r.participants(role)
                        .iter()
                        .position(|p| {
                            model
                                .find_species(&p.species)
                                .is_some_and(|n| n.is_null_node)
                        })
                        .map(|i| (role, i))
                }),
        };
        let Some((role, index)) = found else {
            continue;
        };
        let node = r.participants(role)[index].species.clone();
        let Some((center, half)) = model
            .find_species(&node)
            .map(|n| (n.center, pt(n.width / 2.0, n.height / 2.0)))
        else {
            continue;
        };
        let Some(curve) = curve(segments, junction, center, half) else {
            continue;
        };
        // All but the piece at the species, at whichever end of the path.
        let segs = &curve.segments;
        let from = |q: sb::Point| pt(q.x, q.y).distance(center);
        let at_species_first = from(segs[0].start()) <= from(segs[segs.len() - 1].end());
        let rest = if at_species_first {
            &segs[1..]
        } else {
            &segs[..segs.len() - 1]
        };
        bent += rest.iter().any(is_bent) as usize;
        let r = model.find_reaction_mut(reaction).expect("found above");
        let p = &mut r.participants_mut(role)[index];
        let curved = match (species, role) {
            (None, _) => match null_leg_handles(
                &curve,
                junction,
                center,
                role == Role::Reactant,
                Some(ROUNDED),
            ) {
                Some((c1, c2)) => {
                    (p.ctrl1, p.ctrl2) = (c1, c2);
                    p.ctrl_pts_set = true;
                    true
                }
                None => false,
            },
            (Some(_), Role::Modifier) => {
                p.points = modifier_points(&curve, junction);
                false
            }
            (Some(_), _) => leg_from_curve(
                p,
                role == Role::Reactant,
                &curve,
                junction,
                center,
                Some(ROUNDED),
            ),
        };
        if curved {
            r.mode = ReactionMode::Bezier;
        }
    }
    if bent > 0 {
        let lines = if bent == 1 { "line was" } else { "lines were" };
        warnings.push(Diagnostic {
            severity: Severity::Warning,
            line: 0,
            column: 0,
            message: format!(
                "{bent} {lines} curved away from the species; PathwayDesigner draws only the \
                 piece at the species curved, and the rest straight"
            ),
        });
    }
}

/// A line's segments as one SBML curve, a path of joined segments. The
/// names are libAntimony's for an SBML segment's ends: `rxn_end` is its
/// start and `species_end` its end, whichever way the line was drawn (it
/// writes PathwayDesigner's reactant lines, drawn from the species, with
/// `rxn_end` at the species), and `b1` and `b2` its base points. libAntimony
/// lists the segments in path order, and the reference's example lists them
/// the other way round; which, is told by the ends they share. An end it
/// leaves out is one it can work out again: the reaction's centre, or where
/// the straight line from the species' centre to the line's next point
/// leaves the species' box. So a missing end of the path is the junction or
/// that border point, whichever is farther from the other end. With both
/// missing (as libAntimony writes a straight line, and as a person may write
/// only `b1` and `b2`), the path starts at the junction, as the reference
/// says, unless it is in libAntimony's order and its first segment's `b1`,
/// beside its start, is nearer the species than its `b2`. Which end is
/// the species' is then decided by where they are, as for SBML. `None` if a
/// segment is missing.
fn curve(
    segments: &BTreeMap<u32, Segment>,
    junction: Pt,
    center: Pt,
    half: Pt,
) -> Option<sb::Curve> {
    let n = *segments.keys().max()?;
    let mut segs: Vec<Segment> = (1..=n)
        .map(|k| segments.get(&k).copied())
        .collect::<Option<_>>()?;
    let same =
        |a: Option<Pt>, b: Option<Pt>| matches!((a, b), (Some(a), Some(b)) if a.distance(b) < 0.01);
    let forward = segs
        .windows(2)
        .filter(|w| same(w[0].species_end, w[1].rxn_end))
        .count();
    let backward = segs
        .windows(2)
        .filter(|w| same(w[0].rxn_end, w[1].species_end))
        .count();
    // Numbered from the species, as in the reference's example: then the
    // path, turned round, starts at the reaction.
    let from_species_numbering = backward > forward;
    if from_species_numbering {
        segs.reverse();
    }
    // Each segment's ends, from its neighbours' where it leaves one out.
    let len = segs.len();
    let mut ends: Vec<(Option<Pt>, Option<Pt>)> = segs
        .iter()
        .enumerate()
        .map(|(k, s)| {
            let start = s
                .rxn_end
                .or_else(|| (k > 0).then(|| segs[k - 1].species_end).flatten());
            let end = s
                .species_end
                .or_else(|| (k + 1 < len).then(|| segs[k + 1].rxn_end).flatten());
            (start, end)
        })
        .collect();
    // Where a line from the species' centre toward `toward` leaves its box:
    // the species end libAntimony leaves out.
    let border = |toward: Pt| {
        let d = toward - center;
        let t = [(half.x, d.x), (half.y, d.y)]
            .iter()
            .filter(|(_, d)| d.abs() > 1e-12)
            .map(|(h, d)| h / d.abs())
            .fold(f64::INFINITY, f64::min);
        if t.is_finite() && t < 1.0 {
            center + d * t
        } else {
            center
        }
    };
    // The missing end of a path whose other end is `given`: the junction, or
    // the species' border, whichever is farther from it.
    let missing = |given: Pt, next: Pt| {
        if given.distance(junction) >= given.distance(center) {
            junction
        } else {
            border(next)
        }
    };
    match (ends[0].0, ends[len - 1].1) {
        (None, None) => {
            // b1 is beside a segment's start, b2 beside its end.
            let first = segs[0];
            let from_species = !from_species_numbering
                && match (first.b1, first.b2) {
                    (Some(b1), Some(b2)) => b1.distance(center) < b2.distance(center),
                    (Some(b1), None) => b1.distance(center) < b1.distance(junction),
                    _ => false,
                };
            if from_species {
                let next = ends[0].1.unwrap_or(junction);
                (ends[0].0, ends[len - 1].1) = (Some(border(next)), Some(junction));
            } else {
                let next = ends[len - 1].0.unwrap_or(junction);
                (ends[0].0, ends[len - 1].1) = (Some(junction), Some(border(next)));
            }
        }
        (Some(a), None) => {
            let next = ends[len - 1].0.unwrap_or(a);
            ends[len - 1].1 = Some(missing(a, next));
        }
        (None, Some(b)) => {
            let next = ends[0].1.unwrap_or(b);
            ends[0].0 = Some(missing(b, next));
        }
        (Some(_), Some(_)) => {}
    }
    let q = |p: Pt| sb::Point::new(p.x, p.y);
    let mut out = Vec::new();
    for (s, (start, end)) in segs.iter().zip(ends) {
        let (start, end) = (start?, end?);
        // libAntimony writes a straight segment's missing base points as
        // (0, 0).
        let origin = Some(Pt::ZERO);
        let straight = s.b1 == origin && s.b2 == origin;
        out.push(if (s.b1.is_some() || s.b2.is_some()) && !straight {
            sb::Segment::Bezier {
                start: q(start),
                end: q(end),
                base1: q(s.b1.unwrap_or(start)),
                base2: q(s.b2.unwrap_or(end)),
            }
        } else {
            sb::Segment::Line {
                start: q(start),
                end: q(end),
            }
        });
    }
    Some(sb::Curve { segments: out })
}

/// A Bézier segment whose control points lie off the straight line between
/// its ends: one PathwayDesigner draws straight when it isn't at the species.
fn is_bent(s: &sb::Segment) -> bool {
    let sb::Segment::Bezier {
        start,
        end,
        base1,
        base2,
    } = s
    else {
        return false;
    };
    let p = |v: &sb::Point| pt(v.x, v.y);
    let (a, b) = (p(start), p(end));
    let off = |c: Pt| {
        let d = b - a;
        let len = (d.x * d.x + d.y * d.y).sqrt();
        if len < 1e-9 {
            return c.distance(a);
        }
        ((c.x - a.x) * d.y - (c.y - a.y) * d.x).abs() / len
    };
    off(p(base1)) > 0.5 || off(p(base2)) > 0.5
}

/// Reads one statement into `places`, or says why it can't be used. Lines
/// and looks are accepted without effect (steps 3 and 4).
fn gather(model: &Model, s: &LayoutStatement, places: &mut Places) -> Result<(), String> {
    let path: Vec<&str> = s.path.iter().map(String::as_str).collect();
    let lower: Vec<String> = path.iter().map(|p| p.to_ascii_lowercase()).collect();
    let name = s.path.join(".");
    let unknown =
        || format!("'{name}' isn't a layout setting PathwayDesigner knows; it was skipped");

    match lower
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        ["model", "layout"] => Ok(()),
        ["model", "layout", setting] => {
            if setting.starts_with("align_") {
                places.autolayout.get_or_insert(s.span);
                Ok(())
            } else if matches!(
                *setting,
                "size" | "width" | "height" | "style" | "background"
            ) || is_look(setting)
            {
                // The layout's size has no use here; the rest are looks.
                Ok(())
            } else {
                Err(unknown())
            }
        }
        ["model", "autolayout", ..] => {
            places.autolayout.get_or_insert(s.span);
            Ok(())
        }
        [category @ ("species" | "reaction" | "compartment"), setting] => {
            if is_look(setting) {
                return Ok(());
            }
            let target = match *category {
                "species" => &mut places.species_default,
                "reaction" => &mut places.reaction_default,
                // A compartment has no default size: it is fitted around
                // its species.
                _ => return Ok(()),
            };
            match *setting {
                "size" | "width" | "height" => set(target, setting, &s.value, &name),
                _ => Err(unknown()),
            }
        }
        [_, setting] if is_place(setting) => {
            let (item, id) = find(model, path[0]).ok_or_else(|| not_found(path[0], &name))?;
            let map = match item {
                Item::Species => &mut places.species,
                Item::Reaction => &mut places.reactions,
                Item::Compartment => &mut places.compartments,
            };
            set(map.entry(id).or_default(), setting, &s.value, &name)
        }
        [_, setting] if is_look(setting) => Ok(()),
        // A reaction's null node, and its line (step 3) and look (step 4).
        [_, "--", setting] => {
            let (item, id) = find(model, path[0]).ok_or_else(|| not_found(path[0], &name))?;
            if item != Item::Reaction {
                return Err(format!(
                    "'{}' isn't a reaction, so '{name}' was skipped",
                    path[0]
                ));
            }
            if is_place(setting) {
                set(
                    places.nulls.entry(id).or_default(),
                    setting,
                    &s.value,
                    &name,
                )
            } else if is_line_point(setting) {
                line_point(places, (id, None), 1, 1, setting, &s.value, &name, s.span)
            } else if is_look(setting) {
                Ok(())
            } else {
                Err(unknown())
            }
        }
        // `S1.position.J3.J4`: an alias of S1 for those reactions.
        // An alias's look, `ATP.color.PFK`: read in antimony_looks.
        [_, setting, _, ..] if is_look(setting) => Ok(()),
        [_, setting @ ("pos" | "position" | "size"), ..] => {
            let species = match find(model, path[0]) {
                Some((Item::Species, id)) => id,
                _ => {
                    return Err(format!(
                        "'{}' isn't a species, so '{name}' was skipped",
                        path[0]
                    ));
                }
            };
            let mut reactions = Vec::new();
            for r in &path[2..] {
                match find(model, r) {
                    Some((Item::Reaction, id)) => reactions.push(id),
                    _ => return Err(format!("'{r}' isn't a reaction, so '{name}' was skipped")),
                }
            }
            reactions.sort();
            let at = match places
                .aliases
                .iter()
                .position(|(sp, rs, ..)| *sp == species && *rs == reactions)
            {
                Some(at) => at,
                None => {
                    places
                        .aliases
                        .push((species, reactions, Place::default(), s.span));
                    places.aliases.len() - 1
                }
            };
            set(&mut places.aliases[at].2, setting, &s.value, &name)
        }
        // `J0.S1.b1`, `J0.S1.arc2.seg3.species_end`: a line (§5).
        [_, _, rest @ ..] if matches!(find(model, path[0]), Some((Item::Reaction, _))) => {
            let Some((_, reaction)) = find(model, path[0]) else {
                unreachable!("matched above")
            };
            let species = match find(model, path[1]) {
                Some((Item::Species, id)) => id,
                _ => {
                    return Err(format!(
                        "'{}' isn't a species, so '{name}' was skipped",
                        path[1]
                    ));
                }
            };
            // [arcN] [segN] setting
            let (mut arc, mut seg) = (1, 1);
            let mut rest: &[&str] = rest;
            if let Some(n) = rest.first().and_then(|p| numbered(p, "arc")) {
                (arc, rest) = (n, &rest[1..]);
            }
            if let Some(n) = rest.first().and_then(|p| numbered(p, "seg")) {
                (seg, rest) = (n, &rest[1..]);
            }
            match rest {
                [setting] if is_line_point(setting) => line_point(
                    places,
                    (reaction, Some(species)),
                    arc,
                    seg,
                    setting,
                    &s.value,
                    &name,
                    s.span,
                ),
                [setting] if is_look(setting) => Ok(()),
                _ => Err(unknown()),
            }
        }
        [_, ..] if find(model, path[0]).is_none() && !is_default_compartment(path[0]) => {
            Err(not_found(path[0], &name))
        }
        [_, ..] if is_default_compartment(path[0]) => Ok(()),
        _ => Err(unknown()),
    }
}

/// `arcN` or `segN`: its number.
fn numbered(part: &str, prefix: &str) -> Option<u32> {
    part.strip_prefix(prefix)?.parse().ok().filter(|&n| n > 0)
}

/// Sets one point of a line's segment. A second arc to the same species is
/// noted and left out: this app draws one line per species of a reaction.
#[allow(clippy::too_many_arguments)]
fn line_point(
    places: &mut Places,
    key: (String, Option<String>),
    arc: u32,
    seg: u32,
    setting: &str,
    value: &LayoutValue,
    name: &str,
    span: Span,
) -> Result<(), String> {
    let LayoutValue::Point(x, y) = *value else {
        return Err(format!(
            "'{name}' needs a point such as {{10, 20}}; it was skipped"
        ));
    };
    if arc > 1 {
        places.further_arc.get_or_insert(span);
        return Ok(());
    }
    let segment = places.lines.entry(key).or_default().entry(seg).or_default();
    let at = Some(pt(x, y));
    match setting {
        "species_end" => segment.species_end = at,
        "rxn_end" => segment.rxn_end = at,
        "b1" => segment.b1 = at,
        "b2" => segment.b2 = at,
        _ => unreachable!("only line points come here"),
    }
    Ok(())
}

/// Sets one part of a place from a statement's value.
fn set(place: &mut Place, setting: &str, value: &LayoutValue, name: &str) -> Result<(), String> {
    let number = || match value {
        LayoutValue::Number(n) => Ok(*n),
        _ => Err(format!("'{name}' needs a number; it was skipped")),
    };
    let point = || match value {
        LayoutValue::Point(x, y) => Ok((*x, *y)),
        _ => Err(format!(
            "'{name}' needs a point such as {{10, 20}}; it was skipped"
        )),
    };
    match setting {
        "pos" | "position" => {
            let (x, y) = point()?;
            (place.x, place.y) = (Some(x), Some(y));
        }
        "size" => {
            let (w, h) = point()?;
            if w <= 0.0 || h <= 0.0 {
                return Err(format!(
                    "'{name}' needs a width and height above 0; it was skipped"
                ));
            }
            (place.w, place.h) = (Some(w), Some(h));
        }
        "x" => place.x = Some(number()?),
        "y" => place.y = Some(number()?),
        "width" | "height" => {
            let n = number()?;
            if n <= 0.0 {
                return Err(format!("'{name}' needs a number above 0; it was skipped"));
            }
            if setting == "width" {
                place.w = Some(n);
            } else {
                place.h = Some(n);
            }
        }
        _ => unreachable!("only placement settings come here"),
    }
    Ok(())
}

fn not_found(id: &str, name: &str) -> String {
    format!("'{id}' isn't a species, reaction or compartment of the model, so '{name}' was skipped")
}

fn is_default_compartment(id: &str) -> bool {
    id == ANTIMONY_DEFAULT_COMPARTMENT
}

/// A placement setting.
fn is_place(setting: &str) -> bool {
    matches!(
        setting,
        "pos" | "position" | "x" | "y" | "size" | "width" | "height"
    )
}

/// A point of a line (step 3).
fn is_line_point(setting: &str) -> bool {
    matches!(setting, "species_end" | "rxn_end" | "b1" | "b2")
}

/// A look (step 4).
fn is_look(setting: &str) -> bool {
    matches!(
        setting,
        "color"
            | "fillcolor"
            | "linecolor"
            | "strokecolor"
            | "linewidth"
            | "strokewidth"
            | "fontcolor"
            | "font"
            | "fontsize"
            | "fontstyle"
            | "fontweight"
            | "shape"
    )
}

/// The species (never an alias or a null node), reaction or compartment an
/// id names, matched as written, else ignoring case, as the import resolves
/// names.
pub(crate) fn find(model: &Model, id: &str) -> Option<(Item, String)> {
    let species = |exact: bool| {
        model
            .species()
            .iter()
            .filter(|s| !s.is_alias() && !s.is_null_node)
            .find(|s| {
                if exact {
                    s.id == id
                } else {
                    s.id.eq_ignore_ascii_case(id)
                }
            })
            .map(|s| (Item::Species, s.id.clone()))
    };
    let reaction = |exact: bool| {
        model
            .reactions()
            .iter()
            .find(|r| {
                if exact {
                    r.id == id
                } else {
                    r.id.eq_ignore_ascii_case(id)
                }
            })
            .map(|r| (Item::Reaction, r.id.clone()))
    };
    let compartment = |exact: bool| {
        model
            .compartments
            .iter()
            .find(|c| {
                if exact {
                    c.id == id
                } else {
                    c.id.eq_ignore_ascii_case(id)
                }
            })
            .map(|c| (Item::Compartment, c.id.clone()))
    };
    species(true)
        .or_else(|| reaction(true))
        .or_else(|| compartment(true))
        .or_else(|| species(false))
        .or_else(|| reaction(false))
        .or_else(|| compartment(false))
}

/// Species: a centre from a corner and a size. A size alone, or a category
/// size, changes the size only.
fn place_species(model: &mut Model, places: &Places, warnings: &mut Vec<Diagnostic>) -> Applied {
    let mut applied = Applied::default();
    let default = &places.species_default;
    for node in model.species_mut() {
        if node.is_null_node || node.is_alias() {
            continue;
        }
        let place = places.species.get(&node.id).copied().unwrap_or_default();
        match place.corner() {
            Some(corner) => {
                let (w, h) = place.size(default, SPECIES_SIZE);
                (node.width, node.height) = (w, h);
                node.center = corner + pt(w / 2.0, h / 2.0);
                applied.had_layout = true;
            }
            None => {
                if place.has_size() || default.has_size() {
                    (node.width, node.height) = place.size(default, (node.width, node.height));
                }
                applied.unplaced += 1;
            }
        }
    }
    if !applied.had_layout {
        applied.unplaced = 0;
    } else if applied.unplaced > 0 {
        let n = applied.unplaced;
        let were = if n == 1 { "was" } else { "were" };
        warnings.push(Diagnostic {
            severity: Severity::Warning,
            line: 0,
            column: 0,
            message: format!("{n} species had no position and {were} placed at random"),
        });
    }
    applied
}

/// `S1.position.J3.J4`: a further drawing of S1, which J3's and J4's lines
/// go to.
fn place_aliases(model: &mut Model, places: &Places, warnings: &mut Vec<Diagnostic>) {
    for (species, reactions, place, span) in &places.aliases {
        let Some(primary) = model.find_species(species) else {
            continue;
        };
        let (pw, ph, pc) = (primary.width, primary.height, primary.center);
        let (w, h) = if place.has_size() {
            place.size(&places.species_default, (pw, ph))
        } else if place.corner().is_some() {
            // As for a species: libAntimony's size unless one is given.
            place.size(&places.species_default, SPECIES_SIZE)
        } else {
            (pw, ph)
        };
        let center = match place.corner() {
            Some(corner) => corner + pt(w / 2.0, h / 2.0),
            None => pc + ALIAS_OFFSET,
        };
        let Some(alias) = model.add_alias(species, center) else {
            continue;
        };
        if let Some(node) = model.find_species_mut(&alias) {
            (node.width, node.height) = (w, h);
        }
        for r in reactions {
            let Some(reaction) = model.find_reaction_mut(r) else {
                continue;
            };
            let mut found = false;
            for p in reaction
                .reactants
                .iter_mut()
                .chain(reaction.products.iter_mut())
                .chain(reaction.modifiers.iter_mut())
                .filter(|p| p.species == *species)
            {
                p.species = alias.clone();
                found = true;
            }
            if !found {
                warnings.push(Diagnostic::at(
                    Severity::Warning,
                    *span,
                    format!(
                        "'{species}' takes no part in '{r}', so no line was moved to its alias"
                    ),
                ));
            }
        }
    }
}

/// Reactions: a junction from a corner and a size; one without a position
/// goes between its species, as the import puts it, now that they have
/// moved. Then the null nodes.
fn place_reactions(model: &mut Model, places: &Places) {
    let ids: Vec<String> = model.reactions().iter().map(|r| r.id.clone()).collect();
    for id in ids {
        let place = places.reactions.get(&id).copied().unwrap_or_default();
        let junction = match place.corner() {
            Some(corner) => {
                let (w, h) = place.size(&places.reaction_default, REACTION_SIZE);
                corner + pt(w / 2.0, h / 2.0)
            }
            None => default_junction(model, &id),
        };
        let null = places.nulls.get(&id).copied().unwrap_or_default();
        let Some(r) = model.find_reaction_mut(&id) else {
            continue;
        };
        r.junction = junction;
        // A null node goes where the file says, else beside the junction,
        // as the import puts it. With two (`J0: -> ;`), `J0.--` is the source.
        let (source, sink) = (format!("_source_{id}"), format!("_sink_{id}"));
        let mut given = null.corner();
        for (null_id, dx) in [(source, -NULL_OFFSET), (sink, NULL_OFFSET)] {
            let Some(node) = model.find_species_mut(&null_id) else {
                continue;
            };
            match given.take() {
                Some(corner) => {
                    let (w, h) = null.size(&Place::default(), NULL_SIZE);
                    (node.width, node.height) = (w, h);
                    node.center = corner + pt(w / 2.0, h / 2.0);
                }
                None => node.center = junction + pt(dx, 0.0),
            }
        }
    }
}

/// Between the reactants' centre and the products' centre, as the import
/// places a reaction (null nodes left out: they are placed from it).
fn default_junction(model: &Model, id: &str) -> Pt {
    let Some(r) = model.find_reaction(id) else {
        return Pt::ZERO;
    };
    let centre = |list: &[crate::model::Participant]| {
        let pts: Vec<Pt> = list
            .iter()
            .filter_map(|p| model.find_species(&p.species))
            .filter(|s| !s.is_null_node)
            .map(|s| s.center)
            .collect();
        (!pts.is_empty()).then(|| pts.iter().fold(Pt::ZERO, |a, &p| a + p) / pts.len() as f64)
    };
    match (centre(&r.reactants), centre(&r.products)) {
        (Some(a), Some(b)) => (a + b) / 2.0,
        (Some(a), None) | (None, Some(a)) => a,
        (None, None) => r.junction,
    }
}

/// Compartment boxes: a corner and a size. A size alone puts the box at
/// (0, 0), as libAntimony does (its writer leaves out a zero position). One
/// without a size is fitted around its species later, as a compartment with
/// no box is.
fn place_compartments(model: &mut Model, places: &Places) {
    for c in &mut model.compartments {
        // The default compartment is never drawn (S §3.5), and SBML import
        // gives it no box either.
        if c.is_default() {
            continue;
        }
        let Some(place) = places.compartments.get(&c.id) else {
            continue;
        };
        let corner = place.corner().unwrap_or(Pt::ZERO);
        if let (Some(w), Some(h)) = (place.w, place.h) {
            c.frame = Some(Frame {
                center: corner + pt(w / 2.0, h / 2.0),
                width: w,
                height: h,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::antimony::{ImportOptions, Imported, import};
    use crate::geometry::pt;
    use crate::model::{Frame, Model, ReactionMode, Role};

    fn load(text: &str) -> Imported {
        let opts = ImportOptions {
            convert_catalytic: false,
            seed: 1,
        };
        import(text, &opts).unwrap_or_else(|e| panic!("{e:?}"))
    }

    fn node(m: &Model, id: &str) -> (f64, f64, f64, f64) {
        let s = m.find_species(id).unwrap_or_else(|| panic!("no {id}"));
        (s.center.x, s.center.y, s.width, s.height)
    }

    fn junction(m: &Model, id: &str) -> (f64, f64) {
        let r = m.find_reaction(id).unwrap();
        (r.junction.x, r.junction.y)
    }

    fn messages(i: &Imported) -> Vec<String> {
        i.warnings.iter().map(|w| w.message.clone()).collect()
    }

    const TWO: &str = "J0: S1 -> S2; k1*S1\nk1 = 1\n";

    #[test]
    fn no_statements_leave_the_import_as_it_was() {
        let i = load(TWO);
        assert!(!i.had_layout);
        assert_eq!(i.unplaced, 0);
        assert!(i.warnings.is_empty());
    }

    #[test]
    fn a_position_is_the_top_left_corner_of_libantimonys_size() {
        let i = load(&format!(
            "{TWO}S1.pos = {{100, 200}}\nS2.position = {{300, 200}}\n"
        ));
        assert_eq!(node(&i.model, "S1"), (130.0, 218.0, 60.0, 36.0));
        assert_eq!(node(&i.model, "S2"), (330.0, 218.0, 60.0, 36.0));
        assert!(i.had_layout);
        assert_eq!(i.unplaced, 0);
        assert!(i.warnings.is_empty(), "{:?}", messages(&i));
    }

    #[test]
    fn own_size_then_category_size_then_libantimonys() {
        let i = load(&format!(
            "{TWO}species.size = {{40, 50}}\nS1.pos = {{0, 0}}\nS1.width = 80\nS2.pos = {{100, 0}}\n"
        ));
        assert_eq!(node(&i.model, "S1"), (40.0, 25.0, 80.0, 50.0));
        assert_eq!(node(&i.model, "S2"), (120.0, 25.0, 40.0, 50.0));
    }

    #[test]
    fn x_alone_puts_y_at_zero() {
        let i = load(&format!("{TWO}S1.x = 28\nS2.x = 100; S2.y = 10\n"));
        assert_eq!(node(&i.model, "S1"), (58.0, 18.0, 60.0, 36.0));
        assert_eq!(node(&i.model, "S2"), (130.0, 28.0, 60.0, 36.0));
    }

    #[test]
    fn a_category_size_alone_sizes_every_species() {
        let i = load(&format!("{TWO}species.size = {{40, 50}}\n"));
        let s = i.model.find_species("S1").unwrap();
        assert_eq!((s.width, s.height), (40.0, 50.0));
        assert!(!i.had_layout);
    }

    #[test]
    fn a_reaction_position_is_its_boxs_corner() {
        let i = load(&format!("{TWO}J0.position = {{200, 210}}\n"));
        assert_eq!(junction(&i.model, "J0"), (210.0, 220.0));
        let i = load(&format!(
            "{TWO}reaction.size = {{4, 6}}\nJ0.pos = {{200, 210}}\n"
        ));
        assert_eq!(junction(&i.model, "J0"), (202.0, 213.0));
    }

    #[test]
    fn a_reaction_without_a_position_goes_between_its_moved_species() {
        let i = load(&format!("{TWO}S1.pos = {{0, 0}}\nS2.pos = {{200, 100}}\n"));
        // Centres (30, 18) and (230, 118).
        assert_eq!(junction(&i.model, "J0"), (130.0, 68.0));
    }

    #[test]
    fn species_without_a_position_are_counted_and_reported() {
        let i = load("J0: S1 -> S2; k1*S1\nJ1: S2 -> S3; k1*S2\nk1 = 1\nS1.pos = {0, 0}\n");
        assert!(i.had_layout);
        assert_eq!(i.unplaced, 2);
        assert!(
            messages(&i).contains(&"2 species had no position and were placed at random".into()),
            "{:?}",
            messages(&i)
        );
    }

    #[test]
    fn null_nodes_go_where_given_or_beside_the_junction() {
        let i = load(concat!(
            "J0: -> S1; k0\nJ1: S1 -> ; k1*S1\nk0 = 1; k1 = 1\nS1.pos = {100, 100}\n",
            "J0.position = {72.5, 66.5}\nJ0.--.position = {20, 20}\nJ0.--.size = {30, 30}\n",
            "J1.pos = {290, 108}\n",
        ));
        assert_eq!(node(&i.model, "_source_J0"), (35.0, 35.0, 30.0, 30.0));
        let sink = node(&i.model, "_sink_J1");
        assert_eq!((sink.2, sink.3), (28.0, 28.0), "ours when not placed");
        assert_eq!(
            (sink.0, sink.1),
            (300.0 + crate::passes::NULL_OFFSET, 118.0)
        );
    }

    #[test]
    fn a_placed_null_node_without_a_size_is_libantimonys_size() {
        let i = load("J1: S1 -> ; k1*S1\nk1 = 1\nS1.pos = {100, 100}\nJ1.--.position = {400, 0}\n");
        assert_eq!(node(&i.model, "_sink_J1"), (415.0, 15.0, 30.0, 30.0));
    }

    #[test]
    fn an_alias_takes_the_lines_of_the_reactions_named() {
        let i = load(concat!(
            "J1: S1 -> S2; k*S1\nJ2: S1 -> S3; k*S1\nk = 1\n",
            "S1.pos = {100, 100}\nS1.position.J2 = {100, 300}\n",
        ));
        let m = &i.model;
        let alias = m
            .species()
            .iter()
            .find(|s| s.alias_of.as_deref() == Some("S1"))
            .expect("an alias of S1");
        assert_eq!(
            (alias.center.x, alias.center.y, alias.width, alias.height),
            (130.0, 318.0, 60.0, 36.0)
        );
        assert_eq!(
            m.find_reaction("J2").unwrap().reactants[0].species,
            alias.id
        );
        assert_eq!(m.find_reaction("J1").unwrap().reactants[0].species, "S1");
    }

    #[test]
    fn an_alias_for_two_reactions_with_its_own_size() {
        let i = load(concat!(
            "J1: S1 -> S2; k*S1\nJ2: S1 -> S3; k*S1\nJ3: S1 -> S4; k*S1\nk = 1\n",
            "S1.position.J3.J2 = {0, 0}\nS1.size.J2.J3 = {80, 50}\n",
        ));
        let m = &i.model;
        let aliases: Vec<_> = m.species().iter().filter(|s| s.is_alias()).collect();
        assert_eq!(aliases.len(), 1, "position and size name the same alias");
        let a = aliases[0];
        assert_eq!(
            (a.center.x, a.center.y, a.width, a.height),
            (40.0, 25.0, 80.0, 50.0)
        );
        for r in ["J2", "J3"] {
            assert_eq!(m.find_reaction(r).unwrap().reactants[0].species, a.id);
        }
        assert_eq!(m.find_reaction("J1").unwrap().reactants[0].species, "S1");
    }

    #[test]
    fn a_compartment_box_needs_a_size() {
        let i = load(concat!(
            "compartment C = 1; compartment D = 1\nS1 in C; S2 in D\nJ0: S1 -> S2; k*S1\nk = 1\n",
            "C.pos = {10, 10}; C.size = {500, 400}\nD.pos = {600, 10}\n",
        ));
        let c = i.model.compartments.iter().find(|c| c.id == "C").unwrap();
        assert_eq!(
            c.frame,
            Some(Frame {
                center: pt(260.0, 210.0),
                width: 500.0,
                height: 400.0
            })
        );
        let d = i.model.compartments.iter().find(|c| c.id == "D").unwrap();
        assert_eq!(d.frame, None);
        // A size alone is at (0, 0), as libAntimony reads it.
        let i =
            load("compartment C = 1\nS1 in C\nJ0: S1 -> S2; k*S1\nk = 1\nC.size = {500, 400}\n");
        let c = i.model.compartments.iter().find(|c| c.id == "C").unwrap();
        assert_eq!(c.frame.unwrap().center, pt(250.0, 200.0));
    }

    #[test]
    fn the_default_compartment_gets_no_box() {
        let i = load(
            "compartment defaultCompartment = 1
S1 in defaultCompartment
J0: S1 -> S2; k*S1
k = 1
defaultCompartment.pos = {0, 0}; defaultCompartment.size = {500, 400}
",
        );
        let c = i
            .model
            .compartments
            .iter()
            .find(|c| c.is_default())
            .unwrap();
        assert_eq!(c.frame, None);
    }

    #[test]
    fn names_are_matched_ignoring_case_as_the_import_does() {
        let i = load(&format!("{TWO}s1.POS = {{100, 200}}\n"));
        assert_eq!(node(&i.model, "S1"), (130.0, 218.0, 60.0, 36.0));
    }

    #[test]
    fn looks_leave_the_layout_reading_alone() {
        // Read in antimony_looks; the layout reading says nothing of them.
        let i = load(&format!(
            "{TWO}S1.pos = {{0, 0}}; S2.pos = {{100, 0}}\nS1.color = \"#FF0000\"\n\
             species.linecolor = black\nmodel.layout.background = white\nS1.color.J0 = red\n\
             J0.--.color = white\nmodel.layout = on\nmodel.layout.size = {{400, 300}}\n"
        ));
        assert!(i.warnings.is_empty(), "{:?}", messages(&i));
    }

    // --- lines (§5) ------------------------------------------------------------

    fn leg(m: &Model, r: &str, role: Role, i: usize) -> crate::model::Participant {
        m.find_reaction(r).unwrap().participants(role)[i].clone()
    }

    /// S1 centred at (30, 18), S2 at (330, 18), J0's junction at (180, 18).
    const PLACED: &str =
        "J0: S1 -> S2; k1*S1\nk1 = 1\nS1.pos = {0, 0}\nS2.pos = {300, 0}\nJ0.pos = {170, 8}\n";

    #[test]
    fn b1_is_on_the_reactions_side_and_b2_on_the_species() {
        let i = load(&format!(
            "{PLACED}J0.S1.b1 = {{150, 60}}\nJ0.S1.b2 = {{60, 60}}\nJ0.S2.b1 = {{210, -40}}\nJ0.S2.b2 = {{300, -40}}\n"
        ));
        let m = &i.model;
        // A reactant's ctrl1 is on the species' side, a product's ctrl2.
        let r = leg(m, "J0", Role::Reactant, 0);
        assert!(r.ctrl_pts_set);
        assert_eq!((r.ctrl1, r.ctrl2), (pt(60.0, 60.0), pt(150.0, 60.0)));
        let p = leg(m, "J0", Role::Product, 0);
        assert_eq!((p.ctrl1, p.ctrl2), (pt(210.0, -40.0), pt(300.0, -40.0)));
        assert_eq!(m.find_reaction("J0").unwrap().mode, ReactionMode::Bezier);
        assert!(i.warnings.is_empty(), "{:?}", messages(&i));
    }

    #[test]
    fn a_curve_ending_at_the_border_is_carried_on_to_the_centre() {
        // A straight curve from the junction to S2's border, as libAntimony
        // writes one: carried on, it passes through S2's centre.
        let i = load(&format!(
            "{PLACED}J0.S2.species_end = {{300, 18}}\nJ0.S2.b1 = {{220, 18}}\nJ0.S2.b2 = {{260, 18}}\n"
        ));
        let p = leg(&i.model, "J0", Role::Product, 0);
        assert!(p.ctrl_pts_set);
        for c in [p.ctrl1, p.ctrl2] {
            assert!((c.y - 18.0).abs() < 1e-9, "{c:?}");
        }
        assert!(
            p.ctrl2.x > 260.0,
            "the handle moves out with the end: {:?}",
            p.ctrl2
        );
    }

    #[test]
    fn without_control_points_a_reaction_stays_straight() {
        let i = load(&format!("{PLACED}J0.S1.species_end = {{60, 18}}\n"));
        assert_eq!(
            i.model.find_reaction("J0").unwrap().mode,
            ReactionMode::Straight
        );
        assert!(!leg(&i.model, "J0", Role::Reactant, 0).ctrl_pts_set);
    }

    #[test]
    fn segments_become_corner_points_from_the_species() {
        let i = load(&format!(
            "{PLACED}J0.S1.species_end = {{30, 36}}\nJ0.S1.rxn_end = {{30, 100}}\n\
             J0.S1.b1 = {{30, 90}}\nJ0.S1.b2 = {{30, 50}}\n\
             J0.S1.seg2.species_end = {{30, 100}}\nJ0.S1.seg2.rxn_end = {{180, 100}}\n\
             J0.S1.seg3.species_end = {{180, 100}}\n"
        ));
        let r = leg(&i.model, "J0", Role::Reactant, 0);
        assert_eq!(r.points, vec![pt(30.0, 100.0), pt(180.0, 100.0)]);
        assert!(r.ctrl_pts_set, "the end piece keeps its handles");
        assert!(i.warnings.is_empty(), "{:?}", messages(&i));
    }

    #[test]
    fn a_bent_piece_away_from_the_species_is_drawn_straight_with_a_warning() {
        let i = load(&format!(
            "{PLACED}J0.S1.rxn_end = {{30, 100}}\n\
             J0.S1.seg2.species_end = {{30, 100}}\nJ0.S1.seg2.b1 = {{100, 160}}\n\
             J0.S1.seg2.b2 = {{60, 160}}\n"
        ));
        assert_eq!(
            leg(&i.model, "J0", Role::Reactant, 0).points,
            vec![pt(30.0, 100.0)]
        );
        let m = messages(&i);
        assert_eq!(m.len(), 1, "{m:?}");
        assert!(
            m[0].contains("1 line was curved away from the species"),
            "{m:?}"
        );
    }

    #[test]
    fn a_modifiers_segments_become_its_corner_points() {
        let i = load(concat!(
            "J0: S1 -> S2; k1*S1*E\nspecies E\nk1 = 1; E = 1\nE.pos = {0, 200}\nJ0.pos = {170, 8}\n",
            "J0.E.species_end = {30, 200}\nJ0.E.rxn_end = {30, 120}\n",
            "J0.E.seg2.species_end = {30, 120}\nJ0.E.seg2.rxn_end = {175, 120}\n",
            "J0.E.seg3.species_end = {175, 120}\n",
        ));
        let e = leg(&i.model, "J0", Role::Modifier, 0);
        assert_eq!(e.points, vec![pt(30.0, 120.0), pt(175.0, 120.0)]);
        assert_eq!(
            i.model.find_reaction("J0").unwrap().mode,
            ReactionMode::Straight
        );
    }

    #[test]
    fn a_null_nodes_line_gets_its_handles() {
        let i = load(concat!(
            "J0: -> S1; k0\nk0 = 1\nS1.pos = {300, 0}\nJ0.pos = {170, 8}\n",
            "J0.--.position = {0, 0}\nJ0.--.b1 = {150, 60}\nJ0.--.b2 = {60, 60}\n",
        ));
        let null = leg(&i.model, "J0", Role::Reactant, 0);
        assert!(i.model.find_species(&null.species).unwrap().is_null_node);
        assert!(null.ctrl_pts_set);
        assert_eq!((null.ctrl1, null.ctrl2), (pt(60.0, 60.0), pt(150.0, 60.0)));
        assert_eq!(
            i.model.find_reaction("J0").unwrap().mode,
            ReactionMode::Bezier
        );
    }

    #[test]
    fn a_second_arc_is_one_warning_and_the_first_is_kept() {
        let i = load(&format!(
            "{PLACED}J0.S1.b1 = {{150, 60}}\nJ0.S1.b2 = {{60, 60}}\n\
             J0.S1.arc2.b1 = {{1, 2}}\nJ0.S1.arc2.b2 = {{3, 4}}\n"
        ));
        let r = leg(&i.model, "J0", Role::Reactant, 0);
        assert_eq!((r.ctrl1, r.ctrl2), (pt(60.0, 60.0), pt(150.0, 60.0)));
        let m = messages(&i);
        assert_eq!(m.len(), 1, "{m:?}");
        assert!(m[0].contains("arc2"), "{m:?}");
    }

    #[test]
    fn a_line_to_an_alias_goes_to_the_alias() {
        let i = load(concat!(
            "J1: S1 -> S2; k*S1\nJ2: S1 -> S3; k*S1\nk = 1\n",
            "S1.pos = {0, 0}\nS1.position.J2 = {0, 300}\nJ2.pos = {170, 300}\n",
            "J2.S1.b1 = {150, 360}\nJ2.S1.b2 = {60, 360}\n",
        ));
        let r = leg(&i.model, "J2", Role::Reactant, 0);
        assert!(i.model.find_species(&r.species).unwrap().is_alias());
        assert_eq!((r.ctrl1, r.ctrl2), (pt(60.0, 360.0), pt(150.0, 360.0)));
        assert!(!leg(&i.model, "J1", Role::Reactant, 0).ctrl_pts_set);
    }

    // libAntimony's own conventions, found against libAntimony 3.2.1
    // (scripts/antimony_layout_check.py): its names are an SBML segment's
    // start and end, it leaves out an end it can work out again, writes a
    // straight segment's base points as (0, 0), and rounds to two decimals.

    /// MKKK of the MAPK cascade, centred at (205.7, 210), 63.4 x 36, and J1's
    /// junction at (360, 158); the line as PathwayDesigner's SBML has it,
    /// from MKKK's border to the junction, and libAntimony wrote it.
    const MKKK: &str = "J1: MKKK -> MKKK_P; k*MKKK\nk = 1\nMKKK.pos = {174, 192}\nMKKK.size = {63.41, 36}\nMKKK_P.pos = {482.59, 192}\nJ1.pos = {356, 154}\nJ1.size = {8, 8}\n";

    #[test]
    fn rxn_end_is_where_the_segment_starts_even_at_the_species() {
        let i = load(&format!(
            "{MKKK}J1.MKKK.rxn_end = {{237.41, 192}}\nJ1.MKKK.b1 = {{268.71, 165.28}}\nJ1.MKKK.b2 = {{308.48, 158}}\n"
        ));
        let r = leg(&i.model, "J1", Role::Reactant, 0);
        assert!(r.ctrl_pts_set);
        // b1, beside the segment's start, is the species' handle here: a
        // reactant's ctrl1. (Carried on, this curve misses MKKK's centre, so
        // the handles are the segment's own, as SBML import gives them.)
        assert_eq!((r.ctrl1, r.ctrl2), (pt(268.71, 165.28), pt(308.48, 158.0)));
    }

    #[test]
    fn a_straight_line_with_both_ends_left_out_runs_from_the_border() {
        // NegativeFeedback's R7: S2 is 80 x 36 at (7.36, 97.32); the line
        // ran from its border (87.36, 115.32) to the junction (144.86,
        // 115.32), and libAntimony wrote only its base points.
        let i = load(concat!(
            "R7: S2 -> S3; k*S2\nk = 1\nS2.pos = {7.36, 97.32}\nS2.size = {80, 36}\n",
            "S3.pos = {202.36, 97.32}\nS3.size = {80, 36}\nR7.pos = {140.86, 111.32}\nR7.size = {8, 8}\n",
            "R7.S2.b1 = {106.17, 115.32}\nR7.S2.b2 = {124.66, 115.32}\n",
        ));
        let r = leg(&i.model, "R7", Role::Reactant, 0);
        // SBML import of the original curve gives (81.48, …), (110.73, …).
        assert!((r.ctrl1.x - 81.48).abs() < 0.1, "{:?}", r.ctrl1);
        assert!((r.ctrl2.x - 110.73).abs() < 0.1, "{:?}", r.ctrl2);
    }

    #[test]
    fn a_missing_end_far_from_the_given_one_is_the_species_border() {
        // The given end is beside the junction, so the one left out is where
        // the line meets S2's box: (300, 18); carried on to S2's centre.
        let i = load(&format!(
            "{PLACED}J0.S2.rxn_end = {{185, 18}}\nJ0.S2.b1 = {{220, 18}}\nJ0.S2.b2 = {{260, 18}}\n"
        ));
        let p = leg(&i.model, "J0", Role::Product, 0);
        assert!(p.ctrl_pts_set);
        assert!(
            p.ctrl2.x > 270.0,
            "carried on past the border: {:?}",
            p.ctrl2
        );
    }

    #[test]
    fn base_points_at_the_origin_mean_a_straight_segment() {
        let i = load(&format!(
            "{PLACED}J0.S1.b1 = {{0, 0}}\nJ0.S1.b2 = {{0, 0}}\n"
        ));
        assert!(!leg(&i.model, "J0", Role::Reactant, 0).ctrl_pts_set);
        assert_eq!(
            i.model.find_reaction("J0").unwrap().mode,
            ReactionMode::Straight
        );
    }

    #[test]
    fn a_curve_rounded_to_two_decimals_is_still_carried_on_to_the_centre() {
        // A straight curve to S2's border, its numbers rounded as libAntimony
        // writes them: carried on, it misses the centre by about 0.003.
        let i = load(&format!(
            "{PLACED}J0.S2.species_end = {{300, 18.003}}\nJ0.S2.b1 = {{220, 18}}\nJ0.S2.b2 = {{260.01, 18}}\n"
        ));
        let p = leg(&i.model, "J0", Role::Product, 0);
        assert!(p.ctrl2.x > 270.0, "carried on: {:?}", p.ctrl2);
    }

    #[test]
    fn a_line_point_needs_a_point() {
        let i = load(&format!("{PLACED}J0.S1.b1 = 3\nJ0.S1.seg0.b1 = {{1, 2}}\n"));
        let m = messages(&i);
        assert_eq!(m.len(), 2, "{m:?}");
        assert!(m[0].contains("needs a point"), "{m:?}");
        assert!(m[1].contains("isn't a layout setting"), "{m:?}");
    }

    #[test]
    fn what_cant_be_used_is_a_warning_each() {
        let i = load(&format!(
            "{TWO}X.pos = {{0, 0}}\nS1.pos = 3\nS1.size = {{0, 10}}\nmodel.layout.foo = 1\n\
             J0.Q.b1 = {{1, 2}}\nS1.position.J9 = {{0, 0}}\n"
        ));
        let m = messages(&i);
        assert_eq!(m.len(), 6, "{m:?}");
        assert!(
            m[0].contains("'X' isn't a species, reaction or compartment"),
            "{m:?}"
        );
        assert!(m[1].contains("needs a point"), "{m:?}");
        assert!(m[2].contains("above 0"), "{m:?}");
        assert!(m[3].contains("isn't a layout setting"), "{m:?}");
        assert!(m[4].contains("'Q' isn't a species"), "{m:?}");
        assert!(m[5].contains("'J9' isn't a reaction"), "{m:?}");
    }

    #[test]
    fn automatic_layout_settings_are_one_warning() {
        let i = load(&format!(
            "{TWO}model.autolayout.maxNumConnectedEdges = 5\nmodel.layout.align_top = {{S1, S2}}\n"
        ));
        let m = messages(&i);
        assert_eq!(m.len(), 1, "{m:?}");
        assert!(m[0].contains("automatic-layout settings"), "{m:?}");
    }

    /// libAntimony 3.2.1's own output (2026-10-22), and the glyphs it made
    /// of it in SBML: S1 (100, 100) 60 x 36, its alias for J2 (100, 300),
    /// J0's box (72.5, 66.5) 20 x 20, J0's null node (20, 20) 30 x 30, and
    /// C (10, 10) 500 x 400.
    #[test]
    fn agrees_with_libantimony() {
        let i = load(concat!(
            "J0: -> S1; k0\nJ1: S1 -> S2; k1*S1\nJ2: S1 -> S3; k2*S1\n",
            "k0 = 1; k1 = 1; k2 = 1; S1 = 0; S2 = 0; S3 = 0\n",
            "compartment C = 1; S1 in C; S2 in C; S3 in C\n",
            "model.layout = on\nmodel.layout.size = {838.83, 410}\n",
            "C.position = {10, 10}\nC.size = {500, 400}\n",
            "S1.position = {100, 100}\nS1.position.J2 = {100, 300}\n",
            "S2.position = {30, 101.32}\nS3.position = {778.83, 30}\n",
            "J0.position = {72.5, 66.5}\n",
            "J0.--.position = {20, 20}\nJ0.--.size = {30, 30}\n",
        ));
        let m = &i.model;
        assert_eq!(node(m, "S1"), (130.0, 118.0, 60.0, 36.0));
        let alias = m.species().iter().find(|s| s.is_alias()).unwrap();
        assert_eq!((alias.center.x, alias.center.y), (130.0, 318.0));
        assert_eq!(junction(m, "J0"), (82.5, 76.5));
        assert_eq!(node(m, "_source_J0"), (35.0, 35.0, 30.0, 30.0));
        let c = m.compartments.iter().find(|c| c.id == "C").unwrap();
        assert_eq!(c.frame.unwrap().center, pt(260.0, 210.0));
        assert!(i.warnings.is_empty(), "{:?}", messages(&i));
    }
}
