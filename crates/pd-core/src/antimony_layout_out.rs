//! The drawing written as Antimony's layout statements, the inverse of
//! [`crate::antimony_layout`]: docs/antimony-layout-export-design.md.
//!
//! Positions in Antimony are the top-left corners of SBML Layout bounding
//! boxes, and libAntimony's default sizes (a species 60 × 36, a reaction
//! 20 × 20, a null node 30 × 30) are not PathwayDesigner's, so sizes are
//! written, once for most species (`species.size`) and for each drawing that
//! differs. A reaction's position puts libAntimony's 20 × 20 box centred on
//! the junction. Numbers have two decimals, as libAntimony writes them (D4).

use std::fmt::Write as _;

use std::collections::HashSet;

use sbml_io as sb;

use crate::antimony_layout::{NULL_SIZE, REACTION_SIZE};
use crate::geometry::{Pt, pt};
use crate::model::{Model, NodeShape, Rgba, Role, SpeciesNode, TextFont, VisualStyle};
use crate::render::constants::*;
use crate::render::draw;
use crate::sbml::export::leg_segments;

/// Raised so that libAntimony's automatic layout adds no drawings of its
/// own to a layout that places every species (libAntimony finding 5).
const MAX_CONNECTED_EDGES: u32 = 1000;

/// The layout statements for `model`, one per line, without indentation.
pub(crate) fn statements(model: &Model) -> Vec<String> {
    // A shown compartment without a box yet is written with the box the app
    // gives it when the model is opened (S §7.9), as SBML export does.
    let fitted;
    let model = if model
        .compartments
        .iter()
        .any(|c| c.frame.is_none() && !c.hidden && !c.is_default())
    {
        let mut m = model.clone();
        crate::compartments::place_unplaced(&mut m);
        fitted = m;
        &fitted
    } else {
        model
    };

    let mut out = vec![
        "model.layout = on".to_owned(),
        format!("model.autolayout.maxNumConnectedEdges = {MAX_CONNECTED_EDGES}"),
    ];

    // Sizes: the one most species are drawn at, once.
    let drawings: Vec<&SpeciesNode> = model.species().iter().filter(|s| !s.is_null_node).collect();
    let common = most_common_size(&drawings);
    if let Some((w, h)) = common {
        out.push(format!("species.size = {}", pair(w, h)));
    }

    for s in drawings.iter().filter(|s| !s.is_alias()) {
        out.push(format!("{}.position = {}", s.id, corner(s)));
        if common != Some((s.width, s.height)) {
            out.push(format!("{}.size = {}", s.id, pair(s.width, s.height)));
        }
    }
    // An alias is named by the reactions whose lines go to it.
    for s in drawings.iter().filter(|s| s.is_alias()) {
        let reactions = reactions_drawn_to(model, &s.id);
        if reactions.is_empty() {
            // Antimony can't name an alias no line goes to.
            continue;
        }
        let key = format!("{}.{{}}.{}", s.primary_id(), reactions.join("."));
        out.push(format!("{} = {}", key.replace("{}", "position"), corner(s)));
        // Always: libAntimony 3.2.1 doesn't give an alias `species.size`.
        out.push(format!(
            "{} = {}",
            key.replace("{}", "size"),
            pair(s.width, s.height)
        ));
    }

    for r in model.reactions() {
        let j = crate::render::draw::effective_junction(model, r);
        let (w, h) = REACTION_SIZE;
        out.push(format!(
            "{}.position = {}",
            r.id,
            pair(j.x - w / 2.0, j.y - h / 2.0)
        ));
        // Its null node (`J.--`): one per reaction in Antimony, so with both
        // sides empty the source is written.
        let null = r
            .reactants
            .iter()
            .chain(&r.products)
            .filter_map(|p| model.find_species(&p.species))
            .find(|n| n.is_null_node);
        if let Some(n) = null {
            out.push(format!("{}.--.position = {}", r.id, corner(n)));
            if (n.width, n.height) != NULL_SIZE {
                out.push(format!("{}.--.size = {}", r.id, pair(n.width, n.height)));
            }
        }
    }

    for r in model.reactions() {
        lines(model, r, &mut out);
    }

    for c in &model.compartments {
        let Some(f) = c.drawn_frame() else {
            continue;
        };
        let min = f.min();
        out.push(format!("{}.position = {}", c.id, pair(min.x, min.y)));
        out.push(format!("{}.size = {}", c.id, pair(f.width, f.height)));
    }

    looks(model, &mut out);
    out
}

/// One item's look as libAntimony settings, in the order written: each
/// `(setting, value)`, a value of `None` left unsaid.
type Looks = Vec<(&'static str, Option<String>)>;

/// The looks (design §6): every item's as drawn, so that libAntimony, which
/// fills what a text leaves out with SBMLNetwork's defaults, draws
/// PathwayDesigner's. Most of a kind share a look, which is written once
/// (`species.color = …`); then each item only where it differs, and an alias
/// only where it differs from its species.
fn looks(model: &Model, out: &mut Vec<String>) {
    out.push(format!(
        "model.layout.background = {}",
        colour(draw::background_colour(model))
    ));

    let species: Vec<&SpeciesNode> = model.species().iter().filter(|s| !s.is_null_node).collect();
    let species_look = |s: &SpeciesNode| -> Looks {
        let (fill, border) = draw::species_colours(s, true);
        let mut width = draw::species_border_width(s);
        if draw::is_boundary(model, s) {
            width *= BOUNDARY_BORDER_FACTOR;
        }
        let shape = match s.shape {
            // The default: libAntimony has no rounded rectangle.
            NodeShape::RoundedRect => None,
            NodeShape::Rectangle => Some("rectangle"),
            NodeShape::Ellipse => Some("ellipse"),
            NodeShape::Circle => Some("circle"),
            NodeShape::Hexagon => Some("hexagon"),
        };
        vec![
            ("color", Some(colour(fill))),
            ("linecolor", (width > 0.0).then(|| colour(border))),
            ("linewidth", Some(num(width))),
            ("fontcolor", Some(colour(label_colour(&s.style)))),
            ("fontsize", Some(num(draw::label_size(s)))),
            ("fontstyle", Some(font_style(draw::name_font(&s.style)))),
            ("shape", shape.map(str::to_owned)),
        ]
    };
    let primaries: Vec<&SpeciesNode> = species.iter().copied().filter(|s| !s.is_alias()).collect();
    let kind = by_kind(
        &primaries
            .iter()
            .map(|s| species_look(s))
            .collect::<Vec<_>>(),
    );
    write_kind(out, "species", &kind);
    for s in &primaries {
        write_own(out, &s.id, &species_look(s), &kind);
    }
    for s in species.iter().filter(|s| s.is_alias()) {
        let reactions = reactions_drawn_to(model, &s.id);
        let Some(primary) = model.find_species(s.primary_id()) else {
            continue;
        };
        if reactions.is_empty() {
            continue;
        }
        // Against what its species is drawn with, which it inherits.
        let base = species_look(primary);
        for ((setting, value), (_, inherited)) in species_look(s).iter().zip(&base) {
            if value != inherited
                && let Some(v) = value
            {
                out.push(format!(
                    "{}.{setting}.{} = {v}",
                    s.primary_id(),
                    reactions.join(".")
                ));
            }
        }
    }

    let reaction_look = |r: &crate::model::Reaction| -> Looks {
        // A junction not drawn is written transparent (no statement hides
        // it), which reading takes for hidden.
        let junction = if crate::sbml::appearance::junction_drawn(r) {
            colour(
                if custom(&r.style).is_some() && !r.style.junction_color.is_default() {
                    r.style.junction_color
                } else {
                    JUNCTION_FILL
                },
            )
        } else {
            "transparent".to_owned()
        };
        vec![
            ("linecolor", Some(colour(draw::line_colour(r)))),
            ("linewidth", Some(num(draw::reaction_line_width(r)))),
            ("color", Some(junction)),
            ("fontcolor", Some(colour(draw::reaction_label_colour(r)))),
            ("fontsize", Some(num(draw::reaction_label_size(r)))),
            ("fontstyle", Some(font_style(draw::name_font(&r.style)))),
        ]
    };
    let reactions: Vec<Looks> = model.reactions().iter().map(reaction_look).collect();
    let kind = by_kind(&reactions);
    write_kind(out, "reaction", &kind);
    for (r, look) in model.reactions().iter().zip(&reactions) {
        write_own(out, &r.id, look, &kind);
    }

    let compartment_look = |c: &crate::model::Compartment| -> Looks {
        let (fill, border, width) = draw::compartment_colours(c);
        vec![
            ("color", Some(colour(fill))),
            ("linecolor", (width > 0.0).then(|| colour(border))),
            ("linewidth", Some(num(width))),
            ("fontsize", Some(num(draw::compartment_label_size(c)))),
        ]
    };
    let shown: Vec<&crate::model::Compartment> = model
        .compartments
        .iter()
        .filter(|c| c.drawn_frame().is_some())
        .collect();
    let compartments: Vec<Looks> = shown.iter().map(|c| compartment_look(c)).collect();
    let kind = by_kind(&compartments);
    write_kind(out, "compartment", &kind);
    for (c, look) in shown.iter().zip(&compartments) {
        write_own(out, &c.id, look, &kind);
    }

    // Modifier lines: each its own colour and width, as drawn.
    for r in model.reactions() {
        let mut named: HashSet<String> = HashSet::new();
        for p in r.reactants.iter().chain(&r.products) {
            if let Some(n) = model.find_species(&p.species) {
                named.insert(n.primary_id().to_owned());
            }
        }
        for p in &r.modifiers {
            let Some(n) = model.find_species(&p.species) else {
                continue;
            };
            // A second line to one species can't be named (§5).
            if !named.insert(n.primary_id().to_owned()) {
                continue;
            }
            let key = format!("{}.{}", r.id, n.primary_id());
            out.push(format!(
                "{key}.linecolor = {}",
                colour(p.look.colour.unwrap_or(MODIFIER))
            ));
            out.push(format!(
                "{key}.linewidth = {}",
                num(p.look.width.unwrap_or(DEFAULT_LINE_WIDTH))
            ));
        }
    }
}

/// The value most items of a kind have, setting by setting (ties to the
/// first met). `None` where any item says nothing (a rounded rectangle has
/// no name in Antimony): saying nothing can't undo the kind's value, so
/// those items' neighbours say theirs each.
fn by_kind(looks: &[Looks]) -> Looks {
    let Some(first) = looks.first() else {
        return Vec::new();
    };
    first
        .iter()
        .enumerate()
        .map(|(i, (setting, _))| {
            let mut counts: Vec<(&Option<String>, usize)> = Vec::new();
            for look in looks {
                let v = &look[i].1;
                match counts.iter_mut().find(|(k, _)| *k == v) {
                    Some((_, n)) => *n += 1,
                    None => counts.push((v, 1)),
                }
            }
            if counts.iter().any(|(v, _)| v.is_none()) {
                return (*setting, None);
            }
            let best = counts.iter().map(|(_, n)| *n).max().unwrap_or(0);
            let value = counts
                .into_iter()
                .find(|(_, n)| *n == best)
                .and_then(|(v, _)| v.clone());
            (*setting, value)
        })
        .collect()
}

fn write_kind(out: &mut Vec<String>, kind: &str, look: &Looks) {
    for (setting, value) in look {
        if let Some(v) = value {
            out.push(format!("{kind}.{setting} = {v}"));
        }
    }
}

/// An item's settings where they differ from its kind's.
fn write_own(out: &mut Vec<String>, id: &str, look: &Looks, kind: &Looks) {
    for ((setting, value), (_, shared)) in look.iter().zip(kind) {
        if value != shared
            && let Some(v) = value
        {
            out.push(format!("{id}.{setting} = {v}"));
        }
    }
}

/// A style's own look, if it has one (the default look otherwise).
fn custom(style: &VisualStyle) -> Option<&VisualStyle> {
    style.has_custom_style.then_some(style)
}

/// A species' name colour as drawn.
fn label_colour(style: &VisualStyle) -> Rgba {
    match custom(style) {
        Some(s) if !s.label_color.is_default() => s.label_color,
        _ => LABEL,
    }
}

fn font_style(f: TextFont) -> String {
    match (f.bold, f.italic) {
        (false, false) => "normal",
        (true, false) => "bold",
        (false, true) => "italic",
        (true, true) => "bold_italic",
    }
    .to_owned()
}

/// A colour as libAntimony writes one (D5): a name from its table when one
/// is exactly this colour (never `orchid`, which libAntimony draws black),
/// else `"#RRGGBB"`, or `"#RRGGBBAA"` when not opaque.
fn colour(c: Rgba) -> String {
    let [r, g, b, a] = c.0;
    let hex = if a == 255 {
        format!("#{r:02X}{g:02X}{b:02X}")
    } else {
        format!("#{r:02X}{g:02X}{b:02X}{a:02X}")
    };
    crate::antimony_colors::NAMED
        .iter()
        .find(|(name, value)| *value == hex && *name != "orchid")
        .map(|(name, _)| (*name).to_owned())
        .unwrap_or_else(|| format!("\"{hex}\""))
}

/// A reaction's lines (design §5): each as SBML export draws it, written from
/// the reaction to the species, the way libAntimony's own layouts run, so
/// the names mean what the reference says. Both ends are always written:
/// libAntimony reads a line without `species_end` as ending at (0, 0), and in
/// a large model (the Millard model) one without `rxn_end` as starting far
/// from its reaction. A straight segment gets base points a third and two thirds along
/// it: libAntimony bends one without them towards the origin. Several
/// segments (corner points) are `seg1`, `seg2`, … from the reaction, each
/// with both ends. Antimony names a line by its reaction and species, so a
/// second line to one species (as a reactant and a modifier) can't be said.
fn lines(model: &Model, r: &crate::model::Reaction, out: &mut Vec<String>) {
    let mut named: HashSet<String> = HashSet::new();
    for role in [Role::Reactant, Role::Product, Role::Modifier] {
        for (index, p) in r.participants(role).iter().enumerate() {
            let Some(node) = model.find_species(&p.species) else {
                continue;
            };
            let key = if node.is_null_node {
                format!("{}.--", r.id)
            } else {
                format!("{}.{}", r.id, node.primary_id())
            };
            if !named.insert(key.clone()) {
                continue;
            }
            let Some(mut segments) = leg_segments(model, r, role, index) else {
                continue;
            };
            if segments.is_empty() {
                continue;
            }
            // From the reaction to the species: the path's far end from the
            // node first.
            let at = |q: sb::Point| pt(q.x, q.y);
            let first = at(segments[0].start());
            let last = at(segments[segments.len() - 1].end());
            if first.distance(node.center) < last.distance(node.center) {
                segments = segments.into_iter().rev().map(turned).collect();
            }
            let n = segments.len();
            for (k, s) in segments.iter().enumerate() {
                let prefix = if n == 1 {
                    key.clone()
                } else {
                    format!("{key}.seg{}", k + 1)
                };
                let (start, end) = (at(s.start()), at(s.end()));
                let (b1, b2) = match s {
                    sb::Segment::Bezier { base1, base2, .. } => (at(*base1), at(*base2)),
                    sb::Segment::Line { .. } => {
                        let d = end - start;
                        (start + d / 3.0, start + d * (2.0 / 3.0))
                    }
                };
                out.push(format!("{prefix}.rxn_end = {}", pair(start.x, start.y)));
                out.push(format!("{prefix}.species_end = {}", pair(end.x, end.y)));
                out.push(format!("{prefix}.b1 = {}", pair(b1.x, b1.y)));
                out.push(format!("{prefix}.b2 = {}", pair(b2.x, b2.y)));
            }
        }
    }
}

/// A segment run the other way.
fn turned(s: sb::Segment) -> sb::Segment {
    match s {
        sb::Segment::Line { start, end } => sb::Segment::Line {
            start: end,
            end: start,
        },
        sb::Segment::Bezier {
            start,
            end,
            base1,
            base2,
        } => sb::Segment::Bezier {
            start: end,
            end: start,
            base1: base2,
            base2: base1,
        },
    }
}

/// The size most of these drawings have, if any; ties go to the first met.
fn most_common_size(nodes: &[&SpeciesNode]) -> Option<(f64, f64)> {
    let mut counts: Vec<((f64, f64), usize)> = Vec::new();
    for s in nodes {
        let size = (s.width, s.height);
        match counts.iter_mut().find(|(k, _)| *k == size) {
            Some((_, n)) => *n += 1,
            None => counts.push((size, 1)),
        }
    }
    let best = counts.iter().map(|(_, n)| *n).max()?;
    counts.into_iter().find(|(_, n)| *n == best).map(|(k, _)| k)
}

/// The reactions whose lines go to the drawing `node`, in the model's order.
fn reactions_drawn_to(model: &Model, node: &str) -> Vec<String> {
    model
        .reactions()
        .iter()
        .filter(|r| {
            r.reactants
                .iter()
                .chain(&r.products)
                .chain(&r.modifiers)
                .any(|p| p.species == node)
        })
        .map(|r| r.id.clone())
        .collect()
}

/// A node's top-left corner as `{x, y}`.
fn corner(s: &SpeciesNode) -> String {
    let c: Pt = s.center;
    pair(c.x - s.width / 2.0, c.y - s.height / 2.0)
}

fn pair(x: f64, y: f64) -> String {
    format!("{{{}, {}}}", num(x), num(y))
}

/// Two decimals, without trailing zeros: `12`, `12.5`, `12.25`.
pub(crate) fn num(x: f64) -> String {
    let mut s = String::new();
    let _ = write!(s, "{:.2}", x);
    if s.contains('.') {
        while s.ends_with('0') {
            s.pop();
        }
        if s.ends_with('.') {
            s.pop();
        }
    }
    if s == "-0" { "0".to_owned() } else { s }
}

#[cfg(test)]
mod tests {
    use super::{num, statements};
    use crate::antimony::{ImportOptions, export, export_with_layout, import};
    use crate::geometry::{Pt, pt};
    use crate::model::{Frame, Model, NodeShape, ReactionMode, Rgba, Role, VisualStyle};

    const OPTS: ImportOptions = ImportOptions {
        convert_catalytic: false,
        seed: 1,
    };

    fn model(text: &str) -> Model {
        import(text, &OPTS).unwrap().model
    }

    /// A model placed by hand: S1 and S2 80 x 36, an alias of S1 for J2, a
    /// source on J0, and compartment C.
    fn placed() -> Model {
        let mut m = model(concat!(
            "compartment C = 1\nS1 in C; S2 in C\n",
            "J0: -> S1; k0\nJ1: S1 -> S2; k1*S1\nJ2: S1 -> S3; k2*S1\n",
            "k0 = 1; k1 = 1; k2 = 1\n",
        ));
        let place = |m: &mut Model, id: &str, x: f64, y: f64| {
            let s = m.find_species_mut(id).unwrap();
            s.center = pt(x, y);
        };
        place(&mut m, "S1", 100.0, 100.0);
        place(&mut m, "S2", 300.0, 100.0);
        place(&mut m, "S3", 300.0, 300.0);
        let alias = m.add_alias("S1", pt(100.0, 300.0)).unwrap();
        let j2 = m.find_reaction_mut("J2").unwrap();
        j2.reactants[0].species = alias;
        j2.junction = pt(200.0, 300.0);
        m.find_reaction_mut("J1").unwrap().junction = pt(200.0, 100.0);
        m.find_reaction_mut("J0").unwrap().junction = pt(100.0, 20.0);
        let src = m
            .species()
            .iter()
            .find(|s| s.is_null_node)
            .unwrap()
            .id
            .clone();
        place(&mut m, &src, 30.0, 20.0);
        m.compartments
            .iter_mut()
            .find(|c| c.id == "C")
            .unwrap()
            .frame = Some(Frame {
            center: pt(200.0, 200.0),
            width: 400.0,
            height: 360.0,
        });
        m
    }

    #[test]
    fn numbers_have_at_most_two_decimals() {
        assert_eq!(num(12.0), "12");
        assert_eq!(num(12.5), "12.5");
        assert_eq!(num(12.256), "12.26");
        assert_eq!(num(-0.001), "0");
        assert_eq!(num(-3.1), "-3.1");
    }

    #[test]
    fn the_statements() {
        let s = statements(&placed());
        for line in [
            "model.layout = on",
            "model.autolayout.maxNumConnectedEdges = 1000",
            "species.size = {80, 36}",
            "S1.position = {60, 82}",
            "S2.position = {260, 82}",
            "S1.position.J2 = {60, 282}",
            "S1.size.J2 = {80, 36}",
            "J1.position = {190, 90}",
            "J0.--.position = {16, 6}",
            "C.position = {0, 20}",
            "C.size = {400, 360}",
        ] {
            assert!(s.iter().any(|l| l == line), "{line} missing from {s:#?}");
        }
        // libAntimony's null node is 30 x 30; ours is 28 x 28, so it is said.
        assert!(s.iter().any(|l| l == "J0.--.size = {28, 28}"), "{s:#?}");
        // Every species is the common size: none has its own (an alias's is
        // always written, for libAntimony).
        assert!(!s.iter().any(|l| l.starts_with("S1.size =")), "{s:#?}");
    }

    #[test]
    fn a_species_of_another_size_has_its_own() {
        let mut m = placed();
        let s = m.find_species_mut("S2").unwrap();
        (s.width, s.height) = (120.0, 50.0);
        let s = statements(&m);
        assert!(s.iter().any(|l| l == "S2.size = {120, 50}"), "{s:#?}");
        assert!(s.iter().any(|l| l == "species.size = {80, 36}"), "{s:#?}");
    }

    #[test]
    fn hidden_and_default_compartments_are_not_written() {
        let mut m = placed();
        m.compartments
            .iter_mut()
            .find(|c| c.id == "C")
            .unwrap()
            .hidden = true;
        assert!(!statements(&m).iter().any(|l| l.starts_with("C.")));
    }

    #[test]
    fn the_layout_is_a_last_section_and_the_model_is_unchanged() {
        let m = placed();
        let plain = export(&m);
        let full = export_with_layout(&m);
        let (before, layout) = full
            .split_once("\n  // Layout\n")
            .expect("a Layout section");
        assert_eq!(format!("{before}end\n"), plain);
        assert!(layout.trim_end().ends_with("end"));
    }

    #[test]
    fn read_back_it_is_the_same_drawing() {
        let m = placed();
        let back = import(&export_with_layout(&m), &OPTS).unwrap();
        assert!(back.had_layout);
        assert_eq!(back.unplaced, 0);
        assert!(back.warnings.is_empty(), "{:?}", back.warnings);
        let b = &back.model;
        for s in m
            .species()
            .iter()
            .filter(|s| !s.is_alias() && !s.is_null_node)
        {
            let t = b.find_species(&s.id).unwrap();
            assert_eq!(
                (t.center, t.width, t.height),
                (s.center, s.width, s.height),
                "{}",
                s.id
            );
        }
        let alias = b.species().iter().find(|s| s.is_alias()).unwrap();
        assert_eq!(alias.center, pt(100.0, 300.0));
        assert_eq!(
            b.find_reaction("J2").unwrap().reactants[0].species,
            alias.id
        );
        for r in m.reactions() {
            assert_eq!(
                b.find_reaction(&r.id).unwrap().junction,
                r.junction,
                "{}",
                r.id
            );
        }
        let null = b.species().iter().find(|s| s.is_null_node).unwrap();
        assert_eq!((null.center, null.width), (pt(30.0, 20.0), 28.0));
        let c = b.compartments.iter().find(|c| c.id == "C").unwrap();
        assert_eq!(c.frame.unwrap().center, pt(200.0, 200.0));
        // The default background is written, and comes back as the default.
        assert_eq!(b.background, Rgba::DEFAULT);
    }

    // --- lines (step 2) -----------------------------------------------------------

    fn back(m: &Model) -> Model {
        let i = import(&export_with_layout(m), &OPTS).unwrap();
        assert!(i.warnings.is_empty(), "{:?}", i.warnings);
        i.model
    }

    fn near(a: Pt, b: Pt) -> bool {
        a.distance(b) < 0.02
    }

    #[test]
    fn a_straight_line_is_written_with_both_ends_and_base_points_at_thirds() {
        let s = statements(&placed());
        // J1's line to S1: from the junction (200, 100) to S1's border (140, 100).
        for line in [
            "J1.S1.rxn_end = {200, 100}",
            "J1.S1.species_end = {140, 100}",
            "J1.S1.b1 = {180, 100}",
            "J1.S1.b2 = {160, 100}",
        ] {
            assert!(s.iter().any(|l| l == line), "{line} missing from {s:#?}");
        }
        // The null node's line too.
        assert!(
            s.iter().any(|l| l.starts_with("J0.--.species_end")),
            "{s:#?}"
        );
    }

    #[test]
    fn a_straight_reaction_comes_back_straight() {
        let b = back(&placed());
        for r in b.reactions() {
            assert_eq!(r.mode, ReactionMode::Straight, "{}", r.id);
        }
    }

    #[test]
    fn a_curve_comes_back_as_drawn() {
        let mut m = placed();
        let r = m.find_reaction_mut("J1").unwrap();
        r.mode = ReactionMode::Bezier;
        r.reactants[0].ctrl1 = pt(130.0, 160.0);
        r.reactants[0].ctrl2 = pt(170.0, 160.0);
        r.reactants[0].ctrl_pts_set = true;
        let b = back(&m);
        let r1 = m.find_reaction("J1").unwrap();
        let r2 = b.find_reaction("J1").unwrap();
        assert_eq!(r2.mode, ReactionMode::Bezier);
        let c = |m: &Model, r| crate::bezier::leg_curve(m, r, Role::Reactant, 0).unwrap();
        let (u, v) = (c(&m, r1), c(&b, r2));
        for (p, q) in [(u.p0, v.p0), (u.p1, v.p1), (u.p2, v.p2), (u.p3, v.p3)] {
            assert!(p.distance(q) < 0.05, "{u:?} / {v:?}");
        }
    }

    #[test]
    fn corner_points_come_back_as_segments() {
        let mut m = placed();
        m.find_reaction_mut("J1").unwrap().products[0].points =
            vec![pt(250.0, 180.0), pt(300.0, 180.0)];
        let s = statements(&m);
        assert!(
            s.iter().any(|l| l == "J1.S2.seg1.rxn_end = {200, 100}"),
            "{s:#?}"
        );
        assert!(
            s.iter().any(|l| l.starts_with("J1.S2.seg3.species_end")),
            "{s:#?}"
        );
        let b = back(&m);
        let pts = &b.find_reaction("J1").unwrap().products[0].points;
        assert_eq!(pts.len(), 2, "{pts:?}");
        assert!(
            near(pts[0], pt(250.0, 180.0)) && near(pts[1], pt(300.0, 180.0)),
            "{pts:?}"
        );
    }

    #[test]
    fn a_modifiers_route_comes_back() {
        let mut m = model("J1: S1 -> S2; k*S1*E\nspecies E\nk = 1; E = 1\n");
        for (id, x, y) in [("S1", 0.0, 0.0), ("S2", 300.0, 0.0), ("E", 300.0, 300.0)] {
            m.find_species_mut(id).unwrap().center = pt(x, y);
        }
        let r = m.find_reaction_mut("J1").unwrap();
        r.junction = pt(150.0, 0.0);
        r.modifiers[0].points = vec![pt(150.0, 300.0)];
        let b = back(&m);
        let e = &b.find_reaction("J1").unwrap().modifiers[0];
        assert_eq!(e.points.len(), 1, "{:?}", e.points);
        assert!(near(e.points[0], pt(150.0, 300.0)), "{:?}", e.points);
    }

    #[test]
    fn a_stem_comes_back() {
        let mut m = placed();
        let r = m.find_reaction_mut("J1").unwrap();
        r.stem = Some((pt(-20.0, 0.0), pt(20.0, 0.0)));
        r.stem_straight = true;
        let b = back(&m);
        let stem = b.find_reaction("J1").unwrap().stem.expect("a stem");
        assert!(
            near(stem.0, pt(-20.0, 0.0)) && near(stem.1, pt(20.0, 0.0)),
            "{stem:?}"
        );
    }

    #[test]
    fn a_second_line_to_one_species_is_not_written() {
        // S1 is J1's reactant and, in the rate law, a modifier too: one name.
        let mut m = model("J1: S1 -> S2; k*S1\nk = 1\n");
        let r = m.find_reaction_mut("J1").unwrap();
        let mut p = r.reactants[0].clone();
        p.kind = crate::model::ModifierKind::Activator;
        r.modifiers.push(p);
        let s = statements(&m);
        assert_eq!(
            s.iter()
                .filter(|l| l.starts_with("J1.S1.species_end"))
                .count(),
            1
        );
    }

    // --- looks (step 3) ------------------------------------------------------------

    #[test]
    fn colours_are_libantimonys_names_or_hex() {
        use super::colour;
        assert_eq!(colour(Rgba([0xF0, 0xFF, 0xFF, 255])), "azure");
        assert_eq!(colour(Rgba([0xC7, 0x74, 0x34, 255])), "ochre");
        assert_eq!(colour(Rgba([0x12, 0x34, 0x56, 255])), "\"#123456\"");
        assert_eq!(colour(Rgba([0x12, 0x34, 0x56, 0x80])), "\"#12345680\"");
        assert_eq!(colour(Rgba([255, 255, 255, 0])), "transparent");
        // Never orchid: libAntimony draws it black.
        assert_eq!(colour(Rgba([0xDA, 0x70, 0xD6, 255])), "\"#DA70D6\"");
    }

    #[test]
    fn a_kinds_look_is_written_once_and_items_where_they_differ() {
        let mut m = placed();
        let s1 = m.find_species_mut("S1").unwrap();
        s1.style.has_custom_style = true;
        s1.style.fill_color = Rgba([255, 0, 0, 255]);
        let s = statements(&m);
        assert!(
            s.iter().any(|l| l == "species.color = \"#EEF6FF\""),
            "{s:#?}"
        );
        assert!(s.iter().any(|l| l == "S1.color = red"), "{s:#?}");
        assert!(!s.iter().any(|l| l.starts_with("S2.color")), "{s:#?}");
        assert!(
            s.iter()
                .any(|l| l.starts_with("model.layout.background = ")),
            "{s:#?}"
        );
    }

    fn style(m: &Model, id: &str) -> VisualStyle {
        m.find_species(id).unwrap().style
    }

    #[test]
    fn looks_come_back() {
        let mut m = placed();
        let s1 = m.find_species_mut("S1").unwrap();
        s1.style.has_custom_style = true;
        s1.style.fill_color = Rgba([255, 0, 0, 255]);
        s1.style.border_width = 3.0;
        s1.style.label_bold = true;
        s1.style.font_size = 16.0;
        s1.shape = NodeShape::Ellipse;
        let s2 = m.find_species_mut("S2").unwrap();
        s2.style.no_border = true;
        s2.shape = NodeShape::Hexagon;
        let j1 = m.find_reaction_mut("J1").unwrap();
        j1.style.has_custom_style = true;
        j1.style.line_color = Rgba([0, 0, 255, 255]);
        j1.style.line_width = 4.0;
        m.find_reaction_mut("J2").unwrap().style.junction_visible = false;
        let c = m.compartments.iter_mut().find(|c| c.id == "C").unwrap();
        c.style.has_custom_style = true;
        c.style.fill_color = Rgba([0xF5, 0xF5, 0xDC, 255]);
        c.style.font_size = 20.0;
        m.background = Rgba([0xFF, 0xFF, 0xF0, 255]);
        let b = back(&m);
        for id in ["S1", "S2", "S3"] {
            assert_eq!(style(&b, id).canonical(), style(&m, id).canonical(), "{id}");
            assert_eq!(
                b.find_species(id).unwrap().shape,
                m.find_species(id).unwrap().shape,
                "{id}"
            );
        }
        for id in ["J0", "J1", "J2"] {
            assert_eq!(
                b.find_reaction(id).unwrap().style.canonical(),
                m.find_reaction(id).unwrap().style.canonical(),
                "{id}"
            );
        }
        let cs = |m: &Model| {
            m.compartments
                .iter()
                .find(|c| c.id == "C")
                .unwrap()
                .style
                .canonical()
        };
        assert_eq!(cs(&b), cs(&m));
        assert_eq!(b.background, m.background);
    }

    #[test]
    fn a_rounded_rectangle_among_circles_stays_one() {
        // It has no name in Antimony: the kind can't be `circle` then.
        let mut m = placed();
        m.find_species_mut("S2").unwrap().shape = NodeShape::Circle;
        m.find_species_mut("S3").unwrap().shape = NodeShape::Circle;
        let s = statements(&m);
        assert!(!s.iter().any(|l| l.starts_with("species.shape")), "{s:#?}");
        let b = back(&m);
        assert_eq!(b.find_species("S1").unwrap().shape, NodeShape::RoundedRect);
        assert_eq!(b.find_species("S2").unwrap().shape, NodeShape::Circle);
    }

    #[test]
    fn an_aliass_own_look_and_a_boundary_border_come_back() {
        let mut m = placed();
        let alias = m
            .species()
            .iter()
            .find(|s| s.is_alias())
            .unwrap()
            .id
            .clone();
        let a = m.find_species_mut(&alias).unwrap();
        a.style.has_custom_style = true;
        a.style.fill_color = Rgba([0, 128, 0, 255]);
        m.find_species_mut("S3").unwrap().is_boundary = true;
        let s3 = m.find_species_mut("S3").unwrap();
        s3.style.has_custom_style = true;
        s3.style.border_width = 2.0;
        let s = statements(&m);
        assert!(s.iter().any(|l| l == "S1.color.J2 = green"), "{s:#?}");
        let b = back(&m);
        let ba = b.species().iter().find(|s| s.is_alias()).unwrap();
        assert_eq!(ba.style.fill_color, Rgba([0, 128, 0, 255]));
        assert_eq!(style(&b, "S3").border_width, 2.0);
    }

    #[test]
    fn a_modifier_lines_look_comes_back() {
        let mut m = model("J1: S1 -> S2; k*S1*E\nspecies E\nk = 1; E = 1\n");
        let e = &mut m.find_reaction_mut("J1").unwrap().modifiers[0];
        e.look.colour = Some(Rgba([255, 0, 0, 255]));
        e.look.width = Some(3.0);
        let s = statements(&m);
        assert!(s.iter().any(|l| l == "J1.E.linecolor = red"), "{s:#?}");
        let b = back(&m);
        let look = b.find_reaction("J1").unwrap().modifiers[0].look;
        assert_eq!(
            (look.colour, look.width),
            (Some(Rgba([255, 0, 0, 255])), Some(3.0))
        );
        // The default width comes back as the default.
        let m2 = model("J1: S1 -> S2; k*S1*E\nspecies E\nk = 1; E = 1\n");
        let b2 = back(&m2);
        assert_eq!(
            b2.find_reaction("J1").unwrap().modifiers[0].look.width,
            None
        );
    }

    #[test]
    fn an_alias_no_line_goes_to_is_not_written() {
        let mut m = placed();
        m.add_alias("S2", pt(500.0, 500.0)).unwrap();
        assert!(!statements(&m).iter().any(|l| l.starts_with("S2.position.")));
    }
}
