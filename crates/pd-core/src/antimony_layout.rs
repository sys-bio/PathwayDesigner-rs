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
//! Lines (`J0.S1.b1`, step 3) and looks (`S1.color`, step 4) are recognised
//! here and left for those steps.

use std::collections::HashMap;

use antimony::Span;
use antimony::model::{LayoutStatement, LayoutValue, Module};

use crate::antimony::{Diagnostic, Severity};
use crate::geometry::{Pt, pt};
use crate::model::{ALIAS_OFFSET, Frame, Model};
use crate::passes::NULL_OFFSET;

/// libAntimony's species size when a file gives none.
pub const SPECIES_SIZE: (f64, f64) = (60.0, 36.0);
/// libAntimony's reaction size: the square at the reaction's centre.
pub const REACTION_SIZE: (f64, f64) = (20.0, 20.0);
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
}

/// What a dotted name refers to.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Item {
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

    let applied = place_species(model, &places, warnings);
    place_aliases(model, &places, warnings);
    place_reactions(model, &places);
    place_compartments(model, &places);
    applied
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
            } else if is_line_point(setting) || is_look(setting) {
                Ok(())
            } else {
                Err(unknown())
            }
        }
        // `S1.position.J3.J4`: an alias of S1 for those reactions.
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
        // `J0.S1.b1`, `J0.S1.arc2.seg3.species_end`: a line (step 3).
        [_, _, ..] if matches!(find(model, path[0]), Some((Item::Reaction, _))) => {
            match find(model, path[1]) {
                Some((Item::Species, _)) => Ok(()),
                _ => Err(format!(
                    "'{}' isn't a species, so '{name}' was skipped",
                    path[1]
                )),
            }
        }
        [_, ..] if find(model, path[0]).is_none() && !is_default_compartment(path[0]) => {
            Err(not_found(path[0], &name))
        }
        [_, ..] if is_default_compartment(path[0]) => Ok(()),
        _ => Err(unknown()),
    }
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
fn find(model: &Model, id: &str) -> Option<(Item, String)> {
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
    use crate::model::{Frame, Model};

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
        let i = load("compartment C = 1\nS1 in C\nJ0: S1 -> S2; k*S1\nk = 1\nC.size = {500, 400}\n");
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
    fn lines_and_looks_are_accepted_quietly_for_now() {
        let i = load(&format!(
            "{TWO}S1.pos = {{0, 0}}; S2.pos = {{100, 0}}\nJ0.S1.b1 = {{1, 2}}\n\
             J0.S1.arc2.seg2.species_end = {{1, 2}}\nS1.color = \"#FF0000\"\n\
             species.linecolor = black\nmodel.layout.background = white\n\
             model.layout = on\nmodel.layout.size = {{400, 300}}\n"
        ));
        assert!(i.warnings.is_empty(), "{:?}", messages(&i));
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
