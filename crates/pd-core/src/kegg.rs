//! Reading KEGG pathway maps, KGML files (docs/kegg-import-design.md).
//!
//! A KGML file draws a map's compounds and enzyme boxes, each an `entry` with
//! its `graphics` (centre, size, colours), and lists its `reaction`s, each by
//! the compound entries it turns into others. A reaction's own id is the id of
//! its enzyme box, so the box says where the reaction is drawn.
//!
//! Here a compound becomes a species, its further drawings aliases; an enzyme
//! box a species too, a rectangle at KEGG's place and size, catalysing each
//! reaction the box carries (design K1); a reaction is drawn straight, its
//! junction where the box projects onto the line from its substrates to its
//! products; and a box linking to another map a grey text (K4). Compounds are
//! named by their KEGG numbers here; the app looks up real names (K2).

use std::collections::HashMap;

use crate::geometry::{Pt, pt};
use crate::model::{
    BorderDash, Id, Model, ModifierKind, ModifierLook, NodeShape, Participant, Reaction, Rgba,
    SpeciesNode, VisualStyle, is_valid_sid,
};
use crate::render::draw;

/// A compound's circle, across: KEGG's 8 is too small to click (K3).
pub const COMPOUND_SIZE: f64 = 12.0;
/// The catalyst line's circle, across: well under a compound's.
const CATALYST_SYMBOL: f64 = 5.0;
/// The font size of names on the map.
const NAME_SIZE: f64 = 10.0;
const BLACK: Rgba = Rgba::rgb(0x00, 0x00, 0x00);
const WHITE: Rgba = Rgba::rgb(0xFF, 0xFF, 0xFF);
/// Reaction lines and catalyst lines.
const LINE: Rgba = Rgba::rgb(0x33, 0x33, 0x33);
/// Texts for links to other maps (K4).
pub const LINK_GREY: Rgba = Rgba::rgb(0x80, 0x80, 0x80);
/// An enzyme box KEGG gives no colour.
const ENZYME_FILL: Rgba = Rgba::rgb(0xBF, 0xFF, 0xBF);

/// A map read into a model.
#[derive(Clone, Debug)]
pub struct Imported {
    pub model: Model,
    pub map: Map,
}

/// What a map said besides its drawing.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Map {
    pub warnings: Vec<String>,
    /// The map's code (`eco00010`) and title (`Glycolysis / Gluconeogenesis`).
    pub code: String,
    pub title: String,
    /// Each compound species and its KEGG number (`C00031`), for looking up
    /// its name.
    pub compounds: Vec<(Id, String)>,
}

/// True if `text` is a KGML file: XML whose root element is `<pathway>`.
pub fn looks_like_kgml(text: &str) -> bool {
    let mut rest = text.trim_start_matches('\u{FEFF}');
    loop {
        rest = rest.trim_start();
        if rest.starts_with("<?") || rest.starts_with("<!--") || rest.starts_with("<!") {
            let close = if rest.starts_with("<!--") { "-->" } else { ">" };
            match rest.find(close) {
                Some(i) => rest = &rest[i + close.len()..],
                None => return false,
            }
        } else {
            return rest.starts_with("<pathway")
                && rest[8..].starts_with(|c: char| c.is_whitespace() || c == '>');
        }
    }
}

/// An entry's drawing.
#[derive(Clone, Debug)]
struct Graphics {
    name: String,
    center: Pt,
    width: f64,
    height: f64,
    bg: Option<Rgba>,
}

#[derive(Clone, Debug)]
struct Entry<'a> {
    kind: &'a str,
    /// `cpd:C00031`, `eco:b0114 eco:b0115`, `path:eco00020`.
    name: &'a str,
    graphics: Option<Graphics>,
}

/// Reads a KGML file. An error, with no model, for a file that isn't one, or a
/// map this version can't draw (§6 of the design); each a sentence.
pub fn import(text: &str) -> Result<Imported, String> {
    let options = roxmltree::ParsingOptions {
        allow_dtd: true,
        ..Default::default()
    };
    let doc = roxmltree::Document::parse_with_options(text.trim_start_matches('\u{FEFF}'), options)
        .map_err(|e| format!("It isn't readable XML ({e})."))?;
    let root = doc.root_element();
    if root.tag_name().name() != "pathway" {
        return Err("It isn't a KEGG map (KGML).".into());
    }
    let code = root
        .attribute("name")
        .map(|n| n.trim_start_matches("path:").to_owned())
        .unwrap_or_else(|| "kegg".into());
    let title = root.attribute("title").unwrap_or(&code).to_owned();
    if root
        .attribute("number")
        .is_some_and(|n| n.starts_with("01"))
    {
        return Err("Global and overview KEGG maps aren't supported yet.".into());
    }

    let mut entries: HashMap<&str, Entry> = HashMap::new();
    // In the file's order, so ids are given as KEGG lists the entries.
    let mut order: Vec<&str> = Vec::new();
    for e in root.children().filter(|n| n.has_tag_name("entry")) {
        let Some(id) = e.attribute("id") else {
            continue;
        };
        let graphics = e
            .children()
            .find(|n| n.has_tag_name("graphics"))
            .and_then(graphics);
        entries.insert(
            id,
            Entry {
                kind: e.attribute("type").unwrap_or(""),
                name: e.attribute("name").unwrap_or(""),
                graphics,
            },
        );
        order.push(id);
    }
    let reactions: Vec<roxmltree::Node> = root
        .children()
        .filter(|n| n.has_tag_name("reaction"))
        .collect();
    if reactions.is_empty() {
        return Err(
            "This KEGG map has no reactions (signalling maps aren't supported yet).".into(),
        );
    }

    let mut model = Model::new();
    model.model_name = if is_valid_sid(&code) {
        code.clone()
    } else {
        "kegg".into()
    };
    let mut warnings = Vec::new();

    // Compounds: the first drawing of each the species, the others aliases.
    let mut drawing_of: HashMap<&str, Id> = HashMap::new();
    let mut compounds: Vec<(Id, String)> = Vec::new();
    for &eid in &order {
        let e = &entries[eid];
        let (true, Some(g)) = (e.kind == "compound", &e.graphics) else {
            continue;
        };
        let number = e
            .name
            .split_whitespace()
            .next()
            .unwrap_or(e.name)
            .split_once(':')
            .map_or(e.name, |(_, n)| n);
        let id = sid(number);
        let node = if let Some((primary, _)) = compounds.iter().find(|(_, n)| n == number) {
            model
                .add_alias(primary, g.center)
                .expect("the species exists")
        } else {
            let id = unused(&model, &id);
            let mut s = SpeciesNode::new(id.clone(), g.center);
            compound_look(&mut s);
            model.push_species(s).expect("checked unused");
            compounds.push((id.clone(), number.to_owned()));
            id
        };
        if let Some(s) = model.find_species_mut(&node) {
            compound_look(s);
        }
        drawing_of.insert(eid, node);
    }

    // Enzyme boxes that carry a reaction: one species for each enzyme, a box
    // of the same enzyme elsewhere an alias of it.
    let mut enzyme_of: HashMap<&str, Id> = HashMap::new();
    let mut enzyme_species: HashMap<&str, Id> = HashMap::new();
    for r in &reactions {
        let Some(eid) = r.attribute("id") else {
            continue;
        };
        if enzyme_of.contains_key(eid) {
            continue;
        }
        let Some(e) = entries.get(eid) else {
            continue;
        };
        let Some(g) = &e.graphics else {
            continue;
        };
        if !matches!(e.kind, "gene" | "ortholog" | "enzyme") {
            continue;
        }
        let node = match enzyme_species.get(e.name) {
            Some(primary) => model
                .add_alias(primary, g.center)
                .expect("the species exists"),
            None => {
                let id = unused(&model, &sid(&enzyme_name(&g.name, e.name)));
                model
                    .push_species(SpeciesNode::new(id.clone(), g.center))
                    .expect("checked unused");
                enzyme_species.insert(e.name, id.clone());
                id
            }
        };
        if let Some(s) = model.find_species_mut(&node) {
            enzyme_look(s, g);
        }
        enzyme_of.insert(eid, node);
    }

    // Reactions.
    for r in &reactions {
        let (Some(eid), Some(name)) = (r.attribute("id"), r.attribute("name")) else {
            continue;
        };
        let number = name
            .split_whitespace()
            .next()
            .unwrap_or(name)
            .trim_start_matches("rn:");
        let side = |tag: &str| -> Result<Vec<Id>, String> {
            r.children()
                .filter(|n| n.has_tag_name(tag))
                .map(|n| {
                    n.attribute("id")
                        .and_then(|id| drawing_of.get(id))
                        .cloned()
                        .ok_or_else(|| n.attribute("name").unwrap_or("?").to_owned())
                })
                .collect()
        };
        let (reactants, products) = match (side("substrate"), side("product")) {
            (Ok(a), Ok(b)) if !a.is_empty() && !b.is_empty() => (a, b),
            (Err(what), _) | (_, Err(what)) => {
                warnings.push(format!(
                    "Reaction {number} names {what}, which isn't a compound on the map; it was left out."
                ));
                continue;
            }
            _ => {
                warnings.push(format!(
                    "Reaction {number} has no substrates or no products; it was left out."
                ));
                continue;
            }
        };
        let centre = |ids: &[Id]| {
            let ps: Vec<Pt> = ids
                .iter()
                .filter_map(|id| model.find_species(id))
                .map(|s| s.center)
                .collect();
            ps.iter().fold(Pt::ZERO, |a, &p| a + p) / ps.len().max(1) as f64
        };
        let (from, to) = (centre(&reactants), centre(&products));
        let enzyme = enzyme_of.get(eid).cloned();
        let box_at = entries
            .get(eid)
            .and_then(|e| e.graphics.as_ref())
            .map(|g| g.center);
        let junction = match box_at {
            Some(b) => project(b, from, to),
            None => (from + to) / 2.0,
        };
        let id = unused_reaction(&model, &sid(number));
        let mut rx = Reaction::new(id, junction);
        rx.is_reversible = r.attribute("type") == Some("reversible");
        rx.reactants = reactants
            .iter()
            .map(|s| Participant::new(s.as_str(), 1.0))
            .collect();
        rx.products = products
            .iter()
            .map(|s| Participant::new(s.as_str(), 1.0))
            .collect();
        if let Some(enzyme) = enzyme {
            let mut m = Participant::new(enzyme.as_str(), 1.0);
            m.kind = ModifierKind::Catalyst;
            // A small open circle at the junction, so it isn't taken for a
            // compound's.
            m.look = ModifierLook {
                colour: Some(LINE),
                width: Some(1.0),
                pattern: crate::model::LinePattern::Solid,
                symbol_size: Some(CATALYST_SYMBOL),
                symbol_gap: Some(1.0),
            };
            rx.modifiers.push(m);
        }
        rx.style = VisualStyle {
            has_custom_style: true,
            line_color: LINE,
            line_width: 1.0,
            junction_visible: false,
            ..VisualStyle::default()
        };
        model.push_reaction(rx).expect("checked unused");
    }

    // Links to other maps, as grey texts at their boxes (K4); the map's own
    // title box, as its title, with where it came from (K5).
    for &eid in &order {
        let e = &entries[eid];
        let (true, Some(g)) = (e.kind == "map", &e.graphics) else {
            continue;
        };
        let top_left = pt(g.center.x - g.width / 2.0, g.center.y - g.height / 2.0);
        let tid = match g.name.strip_prefix("TITLE:") {
            Some(t) => {
                let id = model.add_text(top_left, &format!("{t} (KEGG {code})"));
                if let Some(text) = model.find_text_mut(&id) {
                    text.size = 16.0;
                    text.font.bold = true;
                }
                id
            }
            None => {
                let id = model.add_text(top_left, &g.name);
                if let Some(text) = model.find_text_mut(&id) {
                    text.size = NAME_SIZE;
                    text.color = LINK_GREY;
                    text.wrap_width = g.width.max(40.0);
                }
                id
            }
        };
        let _ = tid;
    }

    // Compound names beside their circles, clear of the lines (S §5.11).
    let ids: Vec<Id> = model
        .species()
        .iter()
        .filter(|s| s.shape == NodeShape::Circle)
        .map(|s| s.id.clone())
        .collect();
    for id in ids {
        let offset = model
            .find_species(&id)
            .map(|s| draw::clear_label_offset(&model, s));
        if let (Some(o), Some(s)) = (offset, model.find_species_mut(&id)) {
            s.label_offset = Some(o);
        }
    }
    model.recompute_id_counter();
    Ok(Imported {
        model,
        map: Map {
            warnings,
            code,
            title,
            compounds,
        },
    })
}

fn graphics(g: roxmltree::Node) -> Option<Graphics> {
    let num = |k: &str| {
        g.attribute(k)
            .and_then(|v| v.trim().parse::<f64>().ok())
            .filter(|v| v.is_finite())
    };
    Some(Graphics {
        name: g.attribute("name").unwrap_or("").to_owned(),
        center: pt(num("x")?, num("y")?),
        width: num("width").unwrap_or(COMPOUND_SIZE),
        height: num("height").unwrap_or(COMPOUND_SIZE),
        bg: g
            .attribute("bgcolor")
            .and_then(crate::diagram_format::colors::parse_hex),
    })
}

/// A white circle with a black border, its name beside it (K3).
fn compound_look(s: &mut SpeciesNode) {
    s.shape = NodeShape::Circle;
    (s.width, s.height) = (COMPOUND_SIZE, COMPOUND_SIZE);
    s.border_dash = BorderDash::Solid;
    s.style = VisualStyle {
        has_custom_style: true,
        fill_color: WHITE,
        border_color: BLACK,
        border_width: 1.0,
        font_size: NAME_SIZE,
        ..VisualStyle::default()
    };
}

/// A rectangle at KEGG's size and colour, its name inside (K3).
fn enzyme_look(s: &mut SpeciesNode, g: &Graphics) {
    s.shape = NodeShape::Rectangle;
    (s.width, s.height) = (g.width.max(10.0), g.height.max(10.0));
    s.border_dash = BorderDash::Solid;
    s.label_offset = None;
    s.style = VisualStyle {
        has_custom_style: true,
        fill_color: g.bg.unwrap_or(ENZYME_FILL),
        border_color: BLACK,
        border_width: 1.0,
        font_size: NAME_SIZE,
        ..VisualStyle::default()
    };
}

/// An enzyme's name: its box's label without KEGG's trailing dots (`pykF...`,
/// for more genes) or anything after a comma, else the entry's first id.
fn enzyme_name(label: &str, entry: &str) -> String {
    let label = label
        .split(',')
        .next()
        .unwrap_or("")
        .trim()
        .trim_end_matches('.');
    if !label.is_empty() {
        return label.to_owned();
    }
    entry
        .split_whitespace()
        .next()
        .map(|n| n.split_once(':').map_or(n, |(_, id)| id))
        .unwrap_or("enzyme")
        .to_owned()
}

/// `p` projected onto the segment from `a` to `b`, kept off its ends.
fn project(p: Pt, a: Pt, b: Pt) -> Pt {
    let d = b - a;
    let len2 = d.x * d.x + d.y * d.y;
    if len2 < 1e-9 {
        return p;
    }
    let t = (((p - a).x * d.x + (p - a).y * d.y) / len2).clamp(0.1, 0.9);
    a + d * t
}

/// `name` as an SBML id: what isn't allowed made `_`, and `_` in front of one
/// that starts with a digit.
pub(crate) fn sid(name: &str) -> Id {
    let mut s: String = name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    while s.contains("__") {
        s = s.replace("__", "_");
    }
    let s = s.trim_matches('_').to_owned();
    if s.is_empty() {
        return "S".into();
    }
    if s.starts_with(|c: char| c.is_ascii_digit()) {
        format!("_{s}")
    } else {
        s
    }
}

/// `id`, or `id_2`, `id_3` … if it is taken by a species or a reaction.
fn unused(model: &Model, id: &str) -> Id {
    let taken = |s: &str| model.find_species(s).is_some() || model.find_reaction(s).is_some();
    if !taken(id) {
        return id.to_owned();
    }
    (2..)
        .map(|n| format!("{id}_{n}"))
        .find(|s| !taken(s))
        .expect("some suffix is free")
}

fn unused_reaction(model: &Model, id: &str) -> Id {
    unused(model, id)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A small map written for the tests (KEGG's own files can't be committed,
    /// design §8): an isozyme pair between the same compounds, a box carrying
    /// two reactions, a compound drawn twice, an enzyme with two boxes, a link
    /// to another map, the title box, and a relation, which is skipped.
    const MAP: &str = r##"<?xml version="1.0"?>
<!DOCTYPE pathway SYSTEM "https://www.kegg.jp/kegg/xml/KGML_v0.7.2_.dtd">
<!-- Creation date: test -->
<pathway name="path:eco99999" org="eco" number="99999" title="Test pathway">
    <entry id="1" name="cpd:C00031" type="compound">
        <graphics name="C00031" fgcolor="#000000" bgcolor="#FFFFFF" type="circle" x="100" y="100" width="8" height="8"/>
    </entry>
    <entry id="2" name="cpd:C00103" type="compound">
        <graphics name="C00103" fgcolor="#000000" bgcolor="#FFFFFF" type="circle" x="300" y="100" width="8" height="8"/>
    </entry>
    <entry id="3" name="cpd:C00022" type="compound">
        <graphics name="C00022" fgcolor="#000000" bgcolor="#FFFFFF" type="circle" x="300" y="300" width="8" height="8"/>
    </entry>
    <entry id="4" name="cpd:C00031" type="compound">
        <graphics name="C00031" fgcolor="#000000" bgcolor="#FFFFFF" type="circle" x="100" y="300" width="8" height="8"/>
    </entry>
    <entry id="10" name="eco:b0001" type="gene" reaction="rn:R00001">
        <graphics name="glkA..." fgcolor="#000000" bgcolor="#BFFFBF" type="rectangle" x="160" y="90" width="46" height="17"/>
    </entry>
    <entry id="11" name="eco:b0002" type="gene" reaction="rn:R00002">
        <graphics name="glkB" fgcolor="#000000" bgcolor="#BFFFBF" type="rectangle" x="240" y="90" width="46" height="17"/>
    </entry>
    <entry id="12" name="eco:b0003" type="gene" reaction="rn:R00003 rn:R00004">
        <graphics name="pykF" fgcolor="#000000" bgcolor="#BFFFBF" type="rectangle" x="310" y="200" width="46" height="17"/>
    </entry>
    <entry id="13" name="eco:b0001" type="gene" reaction="rn:R00005">
        <graphics name="glkA..." fgcolor="#000000" bgcolor="#BFFFBF" type="rectangle" x="200" y="310" width="46" height="17"/>
    </entry>
    <entry id="14" name="ko:K99999" type="ortholog">
        <graphics name="K99999" fgcolor="#000000" bgcolor="#FFFFFF" type="rectangle" x="500" y="500" width="46" height="17"/>
    </entry>
    <entry id="20" name="path:eco00020" type="map">
        <graphics name="Citrate cycle (TCA cycle)" fgcolor="#000000" bgcolor="#FFFFFF" type="roundrectangle" x="400" y="400" width="120" height="25"/>
    </entry>
    <entry id="21" name="path:eco99999" type="map">
        <graphics name="TITLE:Test pathway" fgcolor="#000000" bgcolor="#FFFFFF" type="roundrectangle" x="150" y="40" width="200" height="25"/>
    </entry>
    <relation entry1="10" entry2="11" type="ECrel">
        <subtype name="compound" value="2"/>
    </relation>
    <reaction id="10" name="rn:R00001" type="reversible">
        <substrate id="1" name="cpd:C00031"/>
        <product id="2" name="cpd:C00103"/>
    </reaction>
    <reaction id="11" name="rn:R00002" type="reversible">
        <substrate id="1" name="cpd:C00031"/>
        <product id="2" name="cpd:C00103"/>
    </reaction>
    <reaction id="12" name="rn:R00003" type="irreversible">
        <substrate id="2" name="cpd:C00103"/>
        <product id="3" name="cpd:C00022"/>
    </reaction>
    <reaction id="12" name="rn:R00004" type="irreversible">
        <substrate id="2" name="cpd:C00103"/>
        <product id="3" name="cpd:C00022"/>
    </reaction>
    <reaction id="13" name="rn:R00005" type="irreversible">
        <substrate id="3" name="cpd:C00022"/>
        <product id="4" name="cpd:C00031"/>
    </reaction>
</pathway>
"##;

    fn map() -> Imported {
        import(MAP).unwrap()
    }

    #[test]
    fn kgml_is_recognised() {
        assert!(looks_like_kgml(MAP));
        assert!(looks_like_kgml("\u{FEFF}<pathway name=\"x\"/>"));
        assert!(!looks_like_kgml(
            "<?xml version=\"1.0\"?>\n<sbml level=\"3\"/>"
        ));
        assert!(!looks_like_kgml("<pathways/>"));
        assert!(!looks_like_kgml("S1 -> S2; k"));
    }

    #[test]
    fn compounds_are_species_at_keggs_places_and_a_second_drawing_an_alias() {
        let i = map();
        let m = &i.model;
        let glc = m.find_species("C00031").unwrap();
        assert_eq!(glc.center, pt(100.0, 100.0));
        assert_eq!(glc.shape, NodeShape::Circle);
        assert_eq!((glc.width, glc.height), (COMPOUND_SIZE, COMPOUND_SIZE));
        assert!(glc.label_offset.is_some(), "names beside the circles");
        let alias = m.aliases_of("C00031").next().expect("drawn twice");
        assert_eq!(alias.center, pt(100.0, 300.0));
        assert_eq!(alias.shape, NodeShape::Circle);
        assert_eq!(
            i.map.compounds,
            [
                ("C00031".into(), "C00031".into()),
                ("C00103".into(), "C00103".into()),
                ("C00022".into(), "C00022".into())
            ]
        );
        assert_eq!(m.model_name, "eco99999");
        assert_eq!(
            (i.map.code.as_str(), i.map.title.as_str()),
            ("eco99999", "Test pathway")
        );
        assert!(i.map.warnings.is_empty(), "{:?}", i.map.warnings);
    }

    #[test]
    fn reactions_join_the_drawings_kegg_names() {
        let m = map().model;
        let r = m.find_reaction("R00001").unwrap();
        assert!(r.is_reversible);
        assert_eq!(r.reactants[0].species, "C00031");
        assert_eq!(r.products[0].species, "C00103");
        assert!(!m.find_reaction("R00003").unwrap().is_reversible);
        // R00005 makes the second drawing of glucose.
        let r5 = m.find_reaction("R00005").unwrap();
        let alias = m.aliases_of("C00031").next().unwrap();
        assert_eq!(r5.products[0].species, alias.id);
        assert_eq!(m.reactions().len(), 5);
    }

    #[test]
    fn enzymes_are_boxes_that_catalyse_their_reactions() {
        let m = map().model;
        let glk = m.find_species("glkA").expect("the dots dropped");
        assert_eq!(glk.shape, NodeShape::Rectangle);
        assert_eq!((glk.width, glk.height), (46.0, 17.0));
        assert_eq!(glk.center, pt(160.0, 90.0));
        assert_eq!(glk.style.fill_color, Rgba::rgb(0xBF, 0xFF, 0xBF));
        let cat = &m.find_reaction("R00001").unwrap().modifiers[0];
        assert_eq!(
            (cat.species.as_str(), cat.kind),
            ("glkA", ModifierKind::Catalyst)
        );
        assert!(
            cat.look.symbol_size.unwrap() < COMPOUND_SIZE / 2.0,
            "not a compound's circle"
        );
        // One box carrying two reactions: one enzyme, catalysing both.
        for r in ["R00003", "R00004"] {
            assert_eq!(m.find_reaction(r).unwrap().modifiers[0].species, "pykF");
        }
        // The same enzyme's second box: an alias, catalysing R00005.
        let second = m.aliases_of("glkA").next().expect("a second box");
        assert_eq!(second.center, pt(200.0, 310.0));
        assert_eq!(
            m.find_reaction("R00005").unwrap().modifiers[0].species,
            second.id
        );
        // An ortholog box with no reaction isn't drawn.
        assert!(m.find_species("K99999").is_none());
    }

    #[test]
    fn a_junction_is_where_its_box_meets_the_line() {
        let m = map().model;
        // Isozymes between the same compounds: apart, each by its box.
        assert_eq!(
            m.find_reaction("R00001").unwrap().junction,
            pt(160.0, 100.0)
        );
        assert_eq!(
            m.find_reaction("R00002").unwrap().junction,
            pt(240.0, 100.0)
        );
        // The same reaction twice from one box: R00004 is its own reaction.
        assert_eq!(
            m.find_reaction("R00004").unwrap().junction,
            pt(300.0, 200.0)
        );
    }

    #[test]
    fn links_are_grey_texts_and_the_title_says_where_it_came_from() {
        let m = map().model;
        let texts: Vec<(&str, Rgba)> = m
            .texts()
            .iter()
            .map(|t| (t.text.as_str(), t.color))
            .collect();
        assert!(
            texts.contains(&("Citrate cycle (TCA cycle)", LINK_GREY)),
            "{texts:?}"
        );
        let title = m
            .texts()
            .iter()
            .find(|t| t.text == "Test pathway (KEGG eco99999)")
            .expect("a title");
        assert!(title.font.bold);
        assert_eq!(title.position, pt(50.0, 27.5));
    }

    #[test]
    fn maps_it_cant_draw_say_why() {
        let signalling = MAP
            .replace("<reaction ", "<notareaction ")
            .replace("</reaction>", "</notareaction>");
        assert!(import(&signalling).unwrap_err().contains("no reactions"));
        let global = MAP.replace("number=\"99999\"", "number=\"01100\"");
        assert!(import(&global).unwrap_err().contains("Global"));
        assert!(import("<sbml/>").unwrap_err().contains("isn't a KEGG map"));
        assert!(import("not xml").is_err());
    }

    #[test]
    fn a_reaction_naming_something_not_on_the_map_is_left_out() {
        let text = MAP.replace(
            "<product id=\"4\" name=\"cpd:C00031\"/>",
            "<product id=\"14\" name=\"ko:K99999\"/>",
        );
        let i = import(&text).unwrap();
        assert!(i.model.find_reaction("R00005").is_none());
        assert_eq!(i.map.warnings.len(), 1, "{:?}", i.map.warnings);
        assert!(i.map.warnings[0].contains("R00005"), "{:?}", i.map.warnings);
    }

    #[test]
    fn ids_are_words() {
        assert_eq!(sid("D-Glucose"), "D_Glucose");
        assert_eq!(
            sid("alpha-D-glucose 1-phosphate"),
            "alpha_D_glucose_1_phosphate"
        );
        assert_eq!(sid("2-Oxoglutarate"), "_2_Oxoglutarate");
        assert_eq!(sid("--"), "S");
        assert_eq!(enzyme_name("pykF...", "eco:b1854"), "pykF");
        assert_eq!(enzyme_name("PK, pyk", "ko:K00873"), "PK");
        assert_eq!(enzyme_name("", "ko:K00873 ko:K12406"), "K00873");
    }
}
