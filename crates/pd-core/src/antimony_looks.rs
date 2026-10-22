//! The looks Antimony's layout statements give (`S1.color = azure`,
//! `species.linewidth = 2`, `model.layout.style = escher`): §6 of
//! docs/antimony-layout-design.md. Positions and lines are read in
//! [`crate::antimony_layout`], which calls this last.
//!
//! An item's own setting wins over its kind's (`species.`), which wins over
//! the whole layout's (`model.layout.`), as the reference says. Each look
//! goes into the item's own style, as SBML Render import does, and a value
//! equal to PathwayDesigner's default is kept as the default, so a theme
//! applied later still works on it.

use std::collections::HashMap;

use antimony::Span;
use antimony::model::{LayoutStatement, LayoutValue, Module};

use crate::antimony::{Diagnostic, Severity};
use crate::antimony_layout::{Item, find};
use crate::model::{Model, NodeShape, Rgba, VisualStyle};
use crate::render::constants::*;
use crate::sbml::render_in::{set_colour, set_width, settle};
use crate::themes::{self, Theme};

/// The looks one statement, or a kind, or the whole layout, gives.
#[derive(Clone, Debug, Default)]
struct Looks {
    fill: Option<Rgba>,
    line: Option<Rgba>,
    width: Option<f64>,
    font_color: Option<Rgba>,
    font_size: Option<f64>,
    bold: Option<bool>,
    italic: Option<bool>,
    shape: Option<String>,
}

impl Looks {
    /// Each look from here, else from `kind`, else from `all`.
    fn over(&self, kind: &Looks, all: &Looks) -> Looks {
        Looks {
            fill: self.fill.or(kind.fill).or(all.fill),
            line: self.line.or(kind.line).or(all.line),
            width: self.width.or(kind.width).or(all.width),
            font_color: self.font_color.or(kind.font_color).or(all.font_color),
            font_size: self.font_size.or(kind.font_size).or(all.font_size),
            bold: self.bold.or(kind.bold).or(all.bold),
            italic: self.italic.or(kind.italic).or(all.italic),
            shape: (self.shape.clone())
                .or_else(|| kind.shape.clone())
                .or_else(|| all.shape.clone()),
        }
    }

    fn is_empty(&self) -> bool {
        self.fill.is_none()
            && self.line.is_none()
            && self.width.is_none()
            && self.font_color.is_none()
            && self.font_size.is_none()
            && self.bold.is_none()
            && self.italic.is_none()
            && self.shape.is_none()
    }
}

/// Everything the statements say about looks.
#[derive(Default)]
struct Gathered {
    all: Looks,
    species_kind: Looks,
    reaction_kind: Looks,
    compartment_kind: Looks,
    species: HashMap<String, Looks>,
    /// `ATP.color.PFK`: an alias's own look, by species and the reactions
    /// drawn to it (sorted), as `ATP.position.PFK` names the alias.
    aliases: HashMap<(String, Vec<String>), Looks>,
    reactions: HashMap<String, Looks>,
    compartments: HashMap<String, Looks>,
    /// `J0.S1.linecolor`: (reaction, species) to the line's looks.
    lines: HashMap<(String, String), Looks>,
    background: Option<Rgba>,
    theme: Option<Theme>,
    /// The first statement of each kind that is read but not drawn.
    font: Option<Span>,
    line_of_leg: Option<Span>,
}

/// Applies `module`'s look statements to `model`.
pub(crate) fn apply(model: &mut Model, module: &Module, warnings: &mut Vec<Diagnostic>) {
    let mut g = Gathered::default();
    for s in &module.layout {
        if let Err(message) = gather(model, s, &mut g) {
            warnings.push(Diagnostic::at(Severity::Warning, s.span, message));
        }
    }
    if let Some(span) = g.font {
        warnings.push(Diagnostic::at(
            Severity::Warning,
            span,
            "Fonts (font = …) aren't used: PathwayDesigner draws names in its own typeface",
        ));
    }

    // A theme first, so the file's own looks come on top of it. It leaves
    // every node where the statements put it, at the size they gave.
    if let Some(theme) = g.theme {
        let places: Vec<_> = model
            .species()
            .iter()
            .map(|s| (s.id.clone(), s.center, s.width, s.height))
            .collect();
        themes::apply(model, theme);
        for (id, center, w, h) in places {
            if let Some(s) = model.find_species_mut(&id) {
                (s.center, s.width, s.height) = (center, w, h);
            }
        }
    }
    if let Some(c) = g.background {
        model.background = c;
    }

    let mut other_shapes = Vec::new();
    let ids: Vec<(String, String, bool)> = model
        .species()
        .iter()
        .filter(|s| !s.is_null_node)
        .map(|s| (s.id.clone(), s.primary_id().to_owned(), s.is_alias()))
        .collect();
    for (id, primary, alias) in ids {
        let mut own = g.species.get(&primary).cloned().unwrap_or_default();
        if alias {
            let key = (primary.clone(), reactions_drawn_to(model, &id));
            if let Some(a) = g.aliases.get(&key) {
                own = a.over(&own, &Looks::default());
            }
        }
        let looks = own.over(&g.species_kind, &g.all);
        if looks.is_empty() {
            continue;
        }
        species(model, &id, &looks, &mut other_shapes);
    }
    let ids: Vec<String> = model.reactions().iter().map(|r| r.id.clone()).collect();
    for id in ids {
        let own = g.reactions.get(&id).cloned().unwrap_or_default();
        let looks = own.over(&g.reaction_kind, &g.all);
        if !looks.is_empty() {
            reaction(model, &id, &looks);
        }
    }
    let ids: Vec<String> = model
        .compartments
        .iter()
        .filter(|c| !c.is_default())
        .map(|c| c.id.clone())
        .collect();
    for id in ids {
        let own = g.compartments.get(&id).cloned().unwrap_or_default();
        let looks = own.over(&g.compartment_kind, &g.all);
        if !looks.is_empty() {
            compartment(model, &id, &looks);
        }
    }
    for ((r, s), looks) in &g.lines {
        if !modifier_line(model, r, s, looks) {
            g.line_of_leg.get_or_insert(Span::default());
        }
    }

    if !other_shapes.is_empty() {
        other_shapes.sort();
        other_shapes.dedup();
        warnings.push(Diagnostic {
            severity: Severity::Warning,
            line: 0,
            column: 0,
            message: format!(
                "PathwayDesigner has no {} shape; those species are drawn as rectangles",
                other_shapes.join(", ")
            ),
        });
    }
    if g.line_of_leg.is_some() {
        warnings.push(Diagnostic {
            severity: Severity::Warning,
            line: 0,
            column: 0,
            message: "Colours and widths of single reactant and product lines aren't used: \
                      PathwayDesigner draws a reaction's lines alike"
                .into(),
        });
    }
}

/// Reads one statement's look, if it gives one; anything else is
/// [`crate::antimony_layout`]'s.
fn gather(model: &Model, s: &LayoutStatement, g: &mut Gathered) -> Result<(), String> {
    let lower: Vec<String> = s.path.iter().map(|p| p.to_ascii_lowercase()).collect();
    let lower: Vec<&str> = lower.iter().map(String::as_str).collect();
    let name = s.path.join(".");
    match lower.as_slice() {
        ["model", "layout", "style"] => {
            let style = word(&s.value).unwrap_or_default();
            if style.eq_ignore_ascii_case("escher") {
                g.theme = Some(Theme::Escher);
                Ok(())
            } else if style.eq_ignore_ascii_case("default") {
                Ok(())
            } else {
                Err(format!(
                    "The style '{style}' isn't one PathwayDesigner has; only 'escher' is used"
                ))
            }
        }
        ["model", "layout", "background"] => {
            g.background = Some(colour(&s.value, &name)?);
            Ok(())
        }
        ["model", "layout", setting] if is_look(setting) => {
            set(&mut g.all, setting, s, &mut g.font, &name)
        }
        [kind @ ("species" | "reaction" | "compartment"), setting] if is_look(setting) => {
            let target = match *kind {
                "species" => &mut g.species_kind,
                "reaction" => &mut g.reaction_kind,
                _ => &mut g.compartment_kind,
            };
            set(target, setting, s, &mut g.font, &name)
        }
        [_, setting] if is_look(setting) => {
            let target = match find(model, &s.path[0]) {
                Some((Item::Species, id)) => g.species.entry(id).or_default(),
                Some((Item::Reaction, id)) => g.reactions.entry(id).or_default(),
                Some((Item::Compartment, id)) => g.compartments.entry(id).or_default(),
                // Said by the layout reading.
                None => return Ok(()),
            };
            set(target, setting, s, &mut g.font, &name)
        }
        // An alias's look: `ATP.color.PFK.PGK`.
        [_, setting, _, ..]
            if is_look(setting) && matches!(find(model, &s.path[0]), Some((Item::Species, _))) =>
        {
            let Some((_, species)) = find(model, &s.path[0]) else {
                unreachable!("matched above")
            };
            let mut reactions = Vec::new();
            for r in &s.path[2..] {
                match find(model, r) {
                    Some((Item::Reaction, id)) => reactions.push(id),
                    _ => return Ok(()),
                }
            }
            reactions.sort();
            let target = g.aliases.entry((species, reactions)).or_default();
            set(target, setting, s, &mut g.font, &name)
        }
        // A line's look: `J0.S1.linecolor`, `J0.S1.arc2.linewidth`.
        [_, _, .., setting] if is_look(setting) => {
            let (Some((Item::Reaction, r)), Some((Item::Species, sp))) =
                (find(model, &s.path[0]), find(model, &s.path[1]))
            else {
                return Ok(());
            };
            set(
                g.lines.entry((r, sp)).or_default(),
                setting,
                s,
                &mut g.font,
                &name,
            )
        }
        _ => Ok(()),
    }
}

/// Sets one look from a statement.
fn set(
    looks: &mut Looks,
    setting: &str,
    s: &LayoutStatement,
    font: &mut Option<Span>,
    name: &str,
) -> Result<(), String> {
    let number = || match s.value {
        LayoutValue::Number(n) if n >= 0.0 => Ok(n),
        _ => Err(format!("'{name}' needs a number; it was skipped")),
    };
    match setting {
        "color" | "fillcolor" => looks.fill = Some(colour(&s.value, name)?),
        "linecolor" | "strokecolor" => looks.line = Some(colour(&s.value, name)?),
        "linewidth" | "strokewidth" => looks.width = Some(number()?),
        "fontcolor" => looks.font_color = Some(colour(&s.value, name)?),
        "fontsize" => looks.font_size = Some(number()?),
        "fontstyle" | "fontweight" => {
            let style = word(&s.value).unwrap_or_default().to_ascii_lowercase();
            (looks.bold, looks.italic) = match style.as_str() {
                "normal" => (Some(false), Some(false)),
                "bold" => (Some(true), Some(false)),
                "italic" => (Some(false), Some(true)),
                "bold_italic" | "bolditalic" => (Some(true), Some(true)),
                _ => {
                    return Err(format!(
                        "'{name}' needs normal, bold, italic or bold_italic; it was skipped"
                    ));
                }
            };
        }
        "font" => {
            font.get_or_insert(s.span);
        }
        "shape" => {
            let shape = word(&s.value).unwrap_or_default().to_ascii_lowercase();
            if !matches!(
                shape.as_str(),
                "rectangle"
                    | "square"
                    | "ellipse"
                    | "circle"
                    | "triangle"
                    | "diamond"
                    | "pentagon"
                    | "hexagon"
                    | "octagon"
            ) {
                return Err(format!(
                    "'{shape}' isn't a shape libAntimony knows; it was skipped"
                ));
            }
            looks.shape = Some(shape);
        }
        _ => unreachable!("only looks come here"),
    }
    Ok(())
}

/// A look setting (lower case).
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

/// A value of one or more words, or a string, as text.
fn word(v: &LayoutValue) -> Option<String> {
    match v {
        LayoutValue::Word(w) | LayoutValue::Text(w) => Some(w.clone()),
        _ => None,
    }
}

/// A colour: one of libAntimony's names (exactly as written, as libAntimony
/// reads them), `<name>_transparent`, or `"#RRGGBB"` / `"#RRGGBBAA"`.
fn colour(v: &LayoutValue, name: &str) -> Result<Rgba, String> {
    let text = word(v).ok_or_else(|| format!("'{name}' needs a colour; it was skipped"))?;
    parse_colour(&text).ok_or_else(|| {
        format!("'{text}' isn't a colour libAntimony knows, so '{name}' was skipped")
    })
}

pub(crate) fn parse_colour(text: &str) -> Option<Rgba> {
    if let Some(hex) = text.strip_prefix('#') {
        return parse_hex(hex);
    }
    let (base, transparent) = match text.strip_suffix("_transparent") {
        Some(base) => (base, true),
        None => (text, false),
    };
    let (_, value) = crate::antimony_colors::NAMED
        .iter()
        .find(|(n, _)| *n == base)?;
    let mut c = parse_hex(&value[1..])?;
    if transparent {
        c.0[3] = 0;
    }
    Some(c)
}

fn parse_hex(hex: &str) -> Option<Rgba> {
    if !matches!(hex.len(), 6 | 8) || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
    let alpha = if hex.len() == 8 { byte(6)? } else { 255 };
    Some(Rgba([byte(0)?, byte(2)?, byte(4)?, alpha]))
}

/// The reactions whose lines go to the drawing `node`, sorted: what names
/// an alias in `ATP.position.PFK`.
fn reactions_drawn_to(model: &Model, node: &str) -> Vec<String> {
    let mut out: Vec<String> = model
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
        .collect();
    out.sort();
    out
}

fn species(model: &mut Model, id: &str, looks: &Looks, other_shapes: &mut Vec<String>) {
    let boundary = model
        .find_species(id)
        .is_some_and(|s| crate::render::draw::is_boundary(model, s));
    let Some(s) = model.find_species_mut(id) else {
        return;
    };
    let alias = s.is_alias();
    let (fill_default, border_default) = if alias {
        (ALIAS_FILL, ALIAS_BORDER)
    } else {
        (SPECIES_FILL, SPECIES_BORDER)
    };
    if let Some(c) = looks.fill {
        set_colour(&mut s.style, |st| &mut st.fill_color, c, fill_default);
    }
    if let Some(c) = looks.line {
        s.style.no_border = false;
        set_colour(&mut s.style, |st| &mut st.border_color, c, border_default);
    }
    if let Some(w) = looks.width {
        // A boundary species' border is drawn thicker (S §5.3).
        let w = if boundary {
            w / BOUNDARY_BORDER_FACTOR
        } else {
            w
        };
        set_width(
            &mut s.style,
            |st| &mut st.border_width,
            w,
            DEFAULT_LINE_WIDTH,
        );
    }
    name(&mut s.style, looks);
    if let Some(shape) = &looks.shape {
        s.shape = match shape.as_str() {
            // An ellipse in a square box is a circle, as SBML import reads it.
            "ellipse" if (s.width - s.height).abs() < 1e-9 => NodeShape::Circle,
            "ellipse" => NodeShape::Ellipse,
            "circle" => NodeShape::Circle,
            "hexagon" => NodeShape::Hexagon,
            "rectangle" | "square" => NodeShape::Rectangle,
            other => {
                other_shapes.push(other.to_owned());
                NodeShape::Rectangle
            }
        };
    }
    s.style_name = None;
}

fn reaction(model: &mut Model, id: &str, looks: &Looks) {
    let Some(r) = model.find_reaction_mut(id) else {
        return;
    };
    if let Some(c) = looks.line {
        set_colour(&mut r.style, |s| &mut s.line_color, c, REACTION_LINE);
    }
    if let Some(w) = looks.width {
        set_width(&mut r.style, |s| &mut s.line_width, w, DEFAULT_LINE_WIDTH);
    }
    // The fill of the square at the reaction's centre: its junction.
    if let Some(c) = looks.fill {
        set_colour(&mut r.style, |s| &mut s.junction_color, c, JUNCTION_FILL);
    }
    name(&mut r.style, looks);
    r.style_name = None;
}

fn compartment(model: &mut Model, id: &str, looks: &Looks) {
    let Some(c) = model.find_compartment_mut(id) else {
        return;
    };
    if let Some(f) = looks.fill {
        set_colour(&mut c.style, |s| &mut s.fill_color, f, COMPARTMENT_FILL);
    }
    if let Some(l) = looks.line {
        c.style.no_border = false;
        set_colour(&mut c.style, |s| &mut s.border_color, l, COMPARTMENT_BORDER);
    }
    if let Some(w) = looks.width {
        set_width(
            &mut c.style,
            |s| &mut s.border_width,
            w,
            COMPARTMENT_BORDER_WIDTH,
        );
    }
    settle(&mut c.style);
    c.style_name = None;
}

/// A species' or reaction's name: colour, size, bold and italic.
fn name(style: &mut VisualStyle, looks: &Looks) {
    if let Some(c) = looks.font_color {
        set_colour(style, |s| &mut s.label_color, c, LABEL);
    }
    if let Some(size) = looks.font_size {
        set_width(style, |s| &mut s.font_size, size, LABEL_FONT_SIZE);
    }
    if let Some(b) = looks.bold {
        style.label_bold = b;
    }
    if let Some(i) = looks.italic {
        style.label_italic = i;
    }
    settle(style);
}

/// A modifier's line colour and width. False if the species isn't a
/// modifier of the reaction: a reactant's or product's line has no look of
/// its own here.
fn modifier_line(model: &mut Model, rid: &str, species: &str, looks: &Looks) -> bool {
    let primary_of = |m: &Model, node: &str| {
        m.find_species(node)
            .map(|n| n.primary_id().to_owned())
            .unwrap_or_default()
    };
    let Some(r) = model.find_reaction(rid) else {
        return true;
    };
    let index = r
        .modifiers
        .iter()
        .position(|p| primary_of(model, &p.species) == species);
    let Some(index) = index else {
        return looks.line.is_none() && looks.width.is_none();
    };
    let p = &mut model.find_reaction_mut(rid).expect("found").modifiers[index];
    if let Some(c) = looks.line {
        p.look.colour = (c != MODIFIER).then_some(c);
    }
    if let Some(w) = looks.width {
        p.look.width = Some(w);
    }
    true
}

#[cfg(test)]
mod tests {
    use super::parse_colour;
    use crate::antimony::{ImportOptions, Imported, import};
    use crate::model::{Model, NodeShape, Rgba};
    use crate::render::constants::*;
    use crate::themes::Theme;

    fn load(text: &str) -> Imported {
        let opts = ImportOptions {
            convert_catalytic: false,
            seed: 1,
        };
        import(text, &opts).unwrap_or_else(|e| panic!("{e:?}"))
    }

    fn messages(i: &Imported) -> Vec<String> {
        i.warnings.iter().map(|w| w.message.clone()).collect()
    }

    fn rgb(hex: u32) -> Rgba {
        Rgba([(hex >> 16) as u8, (hex >> 8) as u8, hex as u8, 255])
    }

    fn style(m: &Model, id: &str) -> crate::model::VisualStyle {
        m.find_species(id).unwrap().style
    }

    const TWO: &str = "J0: S1 -> S2; k*S1\nk = 1\n";

    #[test]
    fn colours_are_libantimonys_names_or_hex() {
        assert_eq!(parse_colour("azure"), Some(rgb(0xF0FFFF)));
        // Not a CSS name: SBMLNetwork's.
        assert_eq!(parse_colour("fieryorange"), Some(rgb(0xA24510)));
        // SBMLNetwork's table has #DA70D, which libAntimony draws black.
        assert_eq!(parse_colour("orchid"), Some(rgb(0xDA70D6)));
        assert_eq!(
            parse_colour("#268F49CC"),
            Some(Rgba([0x26, 0x8F, 0x49, 0xCC]))
        );
        assert_eq!(parse_colour("#c77434"), Some(rgb(0xC77434)));
        assert_eq!(parse_colour("red_transparent"), Some(Rgba([255, 0, 0, 0])));
        assert_eq!(parse_colour("transparent"), Some(Rgba([255, 255, 255, 0])));
        // Matched exactly, as libAntimony does.
        assert_eq!(parse_colour("Red"), None);
        assert_eq!(parse_colour("#12345"), None);
        assert_eq!(parse_colour("notacolor"), None);
    }

    #[test]
    fn an_items_own_look_then_its_kinds_then_the_layouts() {
        let i = load(&format!(
            "{TWO}species S3\nS3 = 1\nmodel.layout.color = khaki\nspecies.color = azure\nS1.color = red\n\
             model.layout.linecolor = navy\n"
        ));
        let m = &i.model;
        assert_eq!(style(m, "S1").fill_color, rgb(0xFF0000));
        assert_eq!(style(m, "S2").fill_color, rgb(0xF0FFFF));
        assert_eq!(style(m, "S1").border_color, rgb(0x000080));
        // model.layout.color is every element's: the junction too.
        assert_eq!(
            m.find_reaction("J0").unwrap().style.junction_color,
            rgb(0xF0E68C)
        );
        assert!(i.warnings.is_empty(), "{:?}", messages(&i));
    }

    #[test]
    fn a_species_look() {
        let i = load(&format!(
            "{TWO}S1.color = \"#268F49CC\"\nS1.linecolor = yellow\nS1.linewidth = 3\nS1.fontcolor = gray\n\
             S1.fontsize = 19\nS1.fontstyle = bold_italic\nS1.shape = ellipse\n"
        ));
        let s = i.model.find_species("S1").unwrap();
        assert!(s.style.has_custom_style);
        assert_eq!(s.style.fill_color, Rgba([0x26, 0x8F, 0x49, 0xCC]));
        assert_eq!(s.style.border_color, rgb(0xFFFF00));
        assert_eq!(s.style.border_width, 3.0);
        assert_eq!(s.style.label_color, rgb(0x808080));
        assert_eq!(s.style.font_size, 19.0);
        assert!(s.style.label_bold && s.style.label_italic);
        assert_eq!(s.shape, NodeShape::Ellipse);
    }

    #[test]
    fn a_value_equal_to_ours_is_kept_as_the_default() {
        let i = load(&format!(
            "{TWO}S1.color = \"#EEF6FF\"\nS1.linewidth = 1.5\n"
        ));
        let s = style(&i.model, "S1");
        assert_eq!(s.fill_color, Rgba::DEFAULT);
        assert_eq!(s.border_width, 0.0);
        assert!(!s.has_custom_style);
    }

    #[test]
    fn a_boundary_species_border_is_drawn_thicker_so_its_width_is_scaled() {
        let i = load(&format!("{TWO}const S1\n$S1 = 1\nS1.linewidth = 4.8\n"));
        assert!((style(&i.model, "S1").border_width - 4.8 / BOUNDARY_BORDER_FACTOR).abs() < 1e-9);
    }

    #[test]
    fn shapes_and_the_ones_we_dont_have() {
        let i = load(&format!(
            "{TWO}species S3, S4\nS3 = 1; S4 = 1\nS1.shape = circle\nS2.shape = octagon\nS3.shape = hexagon\n\
             S4.shape = triangle\n"
        ));
        let m = &i.model;
        assert_eq!(m.find_species("S1").unwrap().shape, NodeShape::Circle);
        assert_eq!(m.find_species("S2").unwrap().shape, NodeShape::Rectangle);
        assert_eq!(m.find_species("S3").unwrap().shape, NodeShape::Hexagon);
        let w = messages(&i);
        assert_eq!(w.len(), 1, "{w:?}");
        assert!(w[0].contains("no octagon, triangle shape"), "{w:?}");
    }

    #[test]
    fn an_alias_takes_its_species_look() {
        let i = load(concat!(
            "J1: S1 -> S2; k*S1\nJ2: S1 -> S3; k*S1\nk = 1\n",
            "S1.pos = {0, 0}\nS1.position.J2 = {0, 300}\nS1.color = red\n",
        ));
        let alias = i.model.species().iter().find(|s| s.is_alias()).unwrap();
        assert_eq!(alias.style.fill_color, rgb(0xFF0000));
    }

    #[test]
    fn an_alias_has_its_own_look() {
        // As libAntimony writes it: the alias's look named like its place.
        let i = load(concat!(
            "J1: S1 -> S2; k*S1
J2: S1 -> S3; k*S1
k = 1
",
            "S1.pos = {0, 0}
S1.position.J2 = {0, 300}
S1.color = red
S1.color.J2 = blue
",
        ));
        let m = &i.model;
        let alias = m.species().iter().find(|s| s.is_alias()).unwrap();
        assert_eq!(alias.style.fill_color, rgb(0x0000FF));
        assert_eq!(style(m, "S1").fill_color, rgb(0xFF0000));
    }

    #[test]
    fn a_reactions_look_goes_to_the_reaction_not_a_species_of_another_case() {
        // tca_cycle has a species Fum and a reaction FUM.
        let i = load(
            "FUM: Fum -> Mal; k*Fum
k = 1
FUM.color = black
Fum.color = red
",
        );
        assert_eq!(
            i.model.find_reaction("FUM").unwrap().style.junction_color,
            rgb(0x000000)
        );
        assert_eq!(style(&i.model, "Fum").fill_color, rgb(0xFF0000));
    }

    #[test]
    fn an_ellipse_in_a_square_box_is_a_circle() {
        let i = load(&format!(
            "{TWO}S1.size = {{30, 30}}
S1.shape = ellipse
S2.shape = ellipse
"
        ));
        assert_eq!(i.model.find_species("S1").unwrap().shape, NodeShape::Circle);
        assert_eq!(
            i.model.find_species("S2").unwrap().shape,
            NodeShape::Ellipse
        );
    }

    #[test]
    fn a_reactions_and_a_compartments_look() {
        let i = load(concat!(
            "compartment C = 1\nS1 in C\nJ0: S1 -> S2; k*S1\nk = 1\n",
            "J0.linecolor = blue\nJ0.linewidth = 4\nJ0.color = black\nJ0.fontsize = 15\n",
            "C.color = beige\nC.linecolor = brown\nC.linewidth = 5\n",
        ));
        let r = i.model.find_reaction("J0").unwrap();
        assert_eq!(r.style.line_color, rgb(0x0000FF));
        assert_eq!(r.style.line_width, 4.0);
        assert_eq!(r.style.junction_color, rgb(0x000000));
        assert_eq!(r.style.font_size, 15.0);
        let c = i.model.compartments.iter().find(|c| c.id == "C").unwrap();
        assert_eq!(c.style.fill_color, rgb(0xF5F5DC));
        assert_eq!(c.style.border_color, rgb(0xA52A2A));
        assert_eq!(c.style.border_width, 5.0);
    }

    #[test]
    fn a_modifiers_line_look_and_a_reactants() {
        let i = load(concat!(
            "J0: S1 -> S2; k*S1*E\nspecies E\nk = 1; E = 1\n",
            "J0.E.linecolor = red\nJ0.E.linewidth = 3\nJ0.S1.linecolor = blue\n",
        ));
        let e = &i.model.find_reaction("J0").unwrap().modifiers[0];
        assert_eq!(e.look.colour, Some(rgb(0xFF0000)));
        assert_eq!(e.look.width, Some(3.0));
        let w = messages(&i);
        assert_eq!(w.len(), 1, "{w:?}");
        assert!(w[0].contains("single reactant and product lines"), "{w:?}");
    }

    #[test]
    fn the_escher_style_is_the_escher_theme_and_keeps_places() {
        let i = load(&format!(
            "{TWO}model.layout.style = escher\nS1.pos = {{0, 0}}\nS2.pos = {{300, 0}}\nS1.color = red\n"
        ));
        let m = &i.model;
        assert_eq!(m.theme, Some(Theme::Escher));
        let s1 = m.find_species("S1").unwrap();
        assert_eq!(
            (s1.center.x, s1.center.y, s1.width, s1.height),
            (30.0, 18.0, 60.0, 36.0)
        );
        // The file's own look on top of the theme's.
        assert_eq!(s1.style.fill_color, rgb(0xFF0000));
        assert!(i.warnings.is_empty(), "{:?}", messages(&i));
    }

    #[test]
    fn other_styles_and_fonts_are_one_warning_each() {
        let i = load(&format!(
            "{TWO}model.layout.style = calm\nS1.font = serif\nS2.font = arial\nmodel.layout.style = default\n"
        ));
        let w = messages(&i);
        assert_eq!(w.len(), 2, "{w:?}");
        assert!(w.iter().any(|m| m.contains("'calm'")), "{w:?}");
        assert!(w.iter().any(|m| m.contains("Fonts")), "{w:?}");
    }

    #[test]
    fn the_background() {
        let i = load(&format!("{TWO}model.layout.background = fuchsia\n"));
        assert_eq!(i.model.background, rgb(0xFF00FF));
    }

    #[test]
    fn what_cant_be_read_is_a_warning_each() {
        let i = load(&format!(
            "{TWO}S1.color = Red\nS1.linewidth = blue\nS1.fontstyle = heavy\nS1.shape = blob\n"
        ));
        let w = messages(&i);
        assert_eq!(w.len(), 4, "{w:?}");
        assert!(w[0].contains("'Red' isn't a colour"), "{w:?}");
        assert!(w[1].contains("needs a number"), "{w:?}");
        assert!(w[2].contains("bold_italic"), "{w:?}");
        assert!(w[3].contains("'blob' isn't a shape"), "{w:?}");
    }

    #[test]
    fn no_looks_leave_the_default_look() {
        let i = load(&format!("{TWO}S1.pos = {{0, 0}}\n"));
        assert!(!style(&i.model, "S1").has_custom_style);
        assert_eq!(i.model.find_species("S1").unwrap().style_name, None);
    }
}
