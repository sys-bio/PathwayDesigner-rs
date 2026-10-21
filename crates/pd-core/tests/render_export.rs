//! SBML export writes the diagram's look as SBML Render (docs/render-design.md
//! §3, S §11.5.1): a style for every glyph, holding the look as drawn.

use std::collections::HashMap;
use std::path::Path;

use pd_core::geometry::pt;
use pd_core::model::{
    BorderDash, LinePattern, Model, ModifierKind, ModifierLook, NodeShape, Participant,
    ReactionMode, Rgba, TextAlign, Typeface,
};
use sbml_io::render as sr;

/// The exported file's layout and its render information, read back.
fn export(model: &Model) -> (sbml_io::Layout, sr::RenderInformation) {
    let xml = pd_core::sbml::export(model).xml;
    let doc = sbml_io::read(&xml).unwrap().document;
    let mut l = doc.model.unwrap().layouts.remove(0);
    let ri = l.render.take().expect("render information").items.remove(0);
    (l, ri)
}

/// Each glyph's style.
fn styles(ri: &sr::RenderInformation) -> HashMap<String, &sr::Style> {
    let mut out = HashMap::new();
    for s in &ri.styles {
        for id in &s.id_list {
            assert!(out.insert(id.clone(), s).is_none(), "{id} has two styles");
        }
    }
    out
}

fn group<'a>(ri: &'a sr::RenderInformation, glyph: &str) -> &'a sr::Group {
    styles(ri)[glyph].group.as_ref().unwrap()
}

/// A colour id's value.
fn value(ri: &sr::RenderInformation, colour: &Option<String>) -> String {
    let id = colour.as_deref().unwrap();
    ri.color_definitions
        .iter()
        .find(|c| c.id == id)
        .map_or(id.to_owned(), |c| c.value.clone())
}

fn ending<'a>(ri: &'a sr::RenderInformation, id: &Option<String>) -> &'a sr::LineEnding {
    let id = id.as_deref().expect("a line ending");
    ri.line_endings.iter().find(|e| e.id == id).unwrap()
}

fn rel(s: &str) -> Option<sr::RelAbs> {
    Some(sr::RelAbs(s.to_owned()))
}

/// Two species, S1 → S2, and a third, E, as a modifier.
fn model() -> Model {
    let mut m = Model::new();
    let a = m.add_species(pt(0.0, 0.0), 80.0, 36.0);
    let b = m.add_species(pt(400.0, 0.0), 80.0, 36.0);
    let e = m.add_species(pt(200.0, -150.0), 80.0, 36.0);
    let r = m.add_reaction(pt(200.0, 0.0));
    let r = m.find_reaction_mut(&r).unwrap();
    r.reactants.push(Participant::new(a, 1.0));
    r.products.push(Participant::new(b, 1.0));
    r.modifiers.push(Participant::new(e, 1.0));
    m
}

#[test]
fn every_glyph_of_every_gallery_model_has_one_style() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let index: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(root.join("Gallery/index.json")).unwrap())
            .unwrap();
    for e in index["examples"].as_array().unwrap() {
        let text = std::fs::read_to_string(root.join(e["file"].as_str().unwrap())).unwrap();
        let model = if pd_core::escher::looks_like_escher(&text) {
            pd_core::escher::import(&text).unwrap().model
        } else {
            pd_core::json::from_json(&text).unwrap().model
        };
        let (l, ri) = export(&model);
        let styled = styles(&ri);
        let mut ids: Vec<&str> = Vec::new();
        ids.extend(l.species_glyphs.iter().map(|g| g.id.as_str()));
        ids.extend(l.compartment_glyphs.iter().map(|g| g.id.as_str()));
        ids.extend(l.text_glyphs.iter().map(|g| g.id.as_str()));
        for g in &l.reaction_glyphs {
            ids.push(&g.id);
            ids.extend(g.species_reference_glyphs.iter().map(|s| s.id.as_str()));
        }
        let title = e["title"].as_str().unwrap();
        for id in &ids {
            assert!(styled.contains_key(*id), "{title}: {id} has no style");
        }
        // Nothing styled that isn't there.
        assert_eq!(styled.len(), ids.len(), "{title}");
        // Every colour and ending a style names is defined.
        for s in &ri.styles {
            let p = &s.group.as_ref().unwrap().presentation;
            for c in [&p.fill, &p.stroke].into_iter().flatten() {
                assert!(
                    c == "none" || ri.color_definitions.iter().any(|d| &d.id == c),
                    "{title}: {c}"
                );
            }
            for h in [&p.start_head, &p.end_head].into_iter().flatten() {
                assert!(ri.line_endings.iter().any(|e| &e.id == h), "{title}: {h}");
            }
        }
    }
}

#[test]
fn species_are_written_as_drawn() {
    let mut m = model();
    let alias = m.add_alias("S1", pt(0.0, 100.0)).unwrap();
    {
        let s = m.find_species_mut("S2").unwrap();
        s.shape = NodeShape::Hexagon;
        s.style.has_custom_style = true;
        s.style.fill_color = Rgba::rgb(0xFF, 0xD5, 0x4F);
        s.style.border_width = 3.0;
        s.style.font_size = 20.0;
        s.style.label_bold = true;
        s.is_boundary = true;
    }
    {
        let s = m.find_species_mut("S3").unwrap();
        s.shape = NodeShape::Ellipse;
        s.style.no_border = true;
    }
    let (_, ri) = export(&m);

    // The default: a rounded rectangle in the default colours.
    let g = group(&ri, "sg_S1");
    assert_eq!(value(&ri, &g.presentation.fill), "#EEF6FF");
    assert_eq!(value(&ri, &g.presentation.stroke), "#4A7FCB");
    assert_eq!(g.presentation.stroke_width, Some(1.5));
    assert_eq!(g.presentation.stroke_dasharray, None);
    let sr::Primitive::Rectangle(r) = &g.children[0] else {
        panic!("no rectangle")
    };
    assert_eq!((r.width.clone(), r.rx.clone()), (rel("100%"), rel("8")));

    // An alias: its colours and dashes, as the alias indicator draws them.
    let g = group(&ri, &format!("sg_{alias}"));
    assert_eq!(value(&ri, &g.presentation.fill), "#F5F0FF");
    assert_eq!(g.presentation.stroke_dasharray.as_deref(), Some("5,4"));

    // Its own look: fill, a boundary species' thicker border, a hexagon.
    let g = group(&ri, "sg_S2");
    assert_eq!(value(&ri, &g.presentation.fill), "#FFD54F");
    assert_eq!(g.presentation.stroke_width, Some(3.0 * 2.4));
    let sr::Primitive::Polygon(p) = &g.children[0] else {
        panic!("no polygon")
    };
    let xs: Vec<_> = p.elements.iter().map(|e| e.x.clone().unwrap().0).collect();
    assert_eq!(xs, ["0%", "25%", "75%", "100%", "75%", "25%"]);
    let name = group(&ri, "tg_S2");
    assert_eq!(name.presentation.font_size, rel("20"));
    assert_eq!(name.presentation.font_weight.as_deref(), Some("bold"));
    assert_eq!(value(&ri, &name.presentation.stroke), "#1A1A1A");

    // No border; an ellipse.
    let g = group(&ri, "sg_S3");
    assert_eq!(g.presentation.stroke.as_deref(), Some("none"));
    assert!(matches!(g.children[0], sr::Primitive::Ellipse(_)));

    // A border style set by hand wins over the alias rule.
    let mut m2 = m.clone();
    m2.find_species_mut(&alias).unwrap().border_dash = BorderDash::Solid;
    m2.find_species_mut("S1").unwrap().border_dash = BorderDash::Dashed;
    let (_, ri) = export(&m2);
    assert_eq!(
        group(&ri, &format!("sg_{alias}"))
            .presentation
            .stroke_dasharray,
        None
    );
    assert!(group(&ri, "sg_S1").presentation.stroke_dasharray.is_some());
}

#[test]
fn looks_are_shared_and_saved_styles_keep_their_names() {
    let mut m = model();
    for id in ["S1", "S2"] {
        let s = m.find_species_mut(id).unwrap();
        s.style.has_custom_style = true;
        s.style.fill_color = Rgba::rgb(0x11, 0x22, 0x33);
    }
    m.find_species_mut("S1").unwrap().style_name = Some("metabolite".into());
    let (_, ri) = export(&m);
    let st = styles(&ri);
    // The same look, but only S1 follows the saved style: two styles.
    assert_eq!(st["sg_S1"].name.as_deref(), Some("metabolite"));
    assert_eq!(st["sg_S2"].name, None);
    assert_eq!(st["sg_S2"].group, st["sg_S1"].group);
    // E keeps the default look, shared with nothing else here; the names
    // share one style.
    assert_eq!(st["tg_S1"].id_list.len(), 3);
}

#[test]
fn reactions_their_junctions_and_arrowheads() {
    let mut m = model();
    let (l, ri) = export(&m);
    let g = group(&ri, "rg_R4");
    assert_eq!(value(&ri, &g.presentation.stroke), "#444444");
    let sr::Primitive::Ellipse(dot) = &g.children[0] else {
        panic!("no junction")
    };
    assert_eq!(value(&ri, &dot.presentation.fill), "#FF8800");
    // The glyph's box is the dot's, about the junction.
    let bb = l.reaction_glyphs[0].bounding_box.as_ref().unwrap();
    assert_eq!(
        (bb.position.x, bb.position.y, bb.dimensions.width),
        (196.0, -4.0, 8.0)
    );
    // The product's arrowhead, at the end of its line; no arrowhead at the
    // reactant of an irreversible reaction.
    let leg = group(&ri, "srg_R4_prod_S2");
    let arrow = ending(&ri, &leg.presentation.end_head);
    assert_eq!(
        value(&ri, &arrow.group.as_ref().unwrap().presentation.fill),
        "#444444"
    );
    // With a fill rule, or COPASI draws it unfilled.
    let rule = |g: &sr::Group| g.presentation.fill_rule.clone();
    assert_eq!(
        rule(arrow.group.as_ref().unwrap()).as_deref(),
        Some("nonzero")
    );
    assert_eq!(rule(group(&ri, "sg_S1")).as_deref(), Some("nonzero"));
    assert_eq!(rule(group(&ri, "srg_R4_sub_S1")), None);
    // On its shape too, or COPASI's diagram view draws only its outline.
    let sr::Primitive::Polygon(head) = &arrow.group.as_ref().unwrap().children[0] else {
        panic!("no polygon")
    };
    assert_eq!(value(&ri, &head.presentation.fill), "#444444");
    assert_eq!(head.presentation.fill_rule.as_deref(), Some("nonzero"));
    let b = arrow.bounding_box.as_ref().unwrap();
    assert_eq!((b.position.x, b.dimensions.width), (-12.0, 12.0));
    let sub = group(&ri, "srg_R4_sub_S1");
    assert_eq!(
        (
            sub.presentation.start_head.clone(),
            sub.presentation.end_head.clone()
        ),
        (None, None)
    );

    // Reversible: an arrowhead at the reactants' end too.
    m.find_reaction_mut("R4").unwrap().is_reversible = true;
    let (_, ri) = export(&m);
    assert!(
        group(&ri, "srg_R4_sub_S1")
            .presentation
            .start_head
            .is_some()
    );

    // A hidden junction, and a Linear reaction's: no dot, and a point.
    for linear in [false, true] {
        let mut m = model();
        let r = m.find_reaction_mut("R4").unwrap();
        if linear {
            r.modifiers.clear();
            r.mode = ReactionMode::Linear;
        } else {
            r.style.junction_visible = false;
        }
        let (l, ri) = export(&m);
        assert!(group(&ri, "rg_R4").children.is_empty(), "linear {linear}");
        let bb = l.reaction_glyphs[0].bounding_box.as_ref().unwrap();
        assert_eq!(bb.dimensions.width, 0.0, "linear {linear}");
    }
}

#[test]
fn modifier_lines_and_their_symbols() {
    let mut m = model();
    let (_, ri) = export(&m);
    // The default: teal, dashed, a filled dot 8 across, 6 from the junction.
    let g = group(&ri, "srg_R4_mod_S3");
    assert_eq!(value(&ri, &g.presentation.stroke), "#007777");
    assert_eq!(g.presentation.stroke_dasharray.as_deref(), Some("6,4"));
    let dot = ending(&ri, &g.presentation.end_head);
    let b = dot.bounding_box.as_ref().unwrap();
    assert_eq!(
        (b.position.x, b.position.y, b.dimensions.width),
        (-14.0, -4.0, 8.0)
    );
    assert!(matches!(
        dot.group.as_ref().unwrap().children[0],
        sr::Primitive::Ellipse(_)
    ));

    let cases = [
        (ModifierKind::Activator, "Polygon"),
        (ModifierKind::Inhibitor, "Rectangle"),
        (ModifierKind::Catalyst, "Ellipse"),
    ];
    for (kind, shape) in cases {
        let mut m = model();
        let p = &mut m.find_reaction_mut("R4").unwrap().modifiers[0];
        p.kind = kind;
        p.look = ModifierLook {
            colour: Some(Rgba::rgb(0xC8, 0, 0)),
            width: Some(3.0),
            pattern: LinePattern::Solid,
            symbol_size: Some(30.0),
            symbol_gap: Some(10.0),
        };
        let (_, ri) = export(&m);
        let g = group(&ri, "srg_R4_mod_S3");
        assert_eq!(value(&ri, &g.presentation.stroke), "#C80000");
        assert_eq!(g.presentation.stroke_width, Some(3.0));
        assert_eq!(g.presentation.stroke_dasharray, None);
        let e = ending(&ri, &g.presentation.end_head);
        let drawn = format!("{:?}", e.group.as_ref().unwrap().children[0]);
        assert!(drawn.starts_with(shape), "{kind:?}: {drawn}");
        let b = e.bounding_box.as_ref().unwrap();
        match kind {
            // Its tip 10 from the junction, 30 long.
            ModifierKind::Activator => assert_eq!(b.position.x, -40.0),
            // Centred 10 from the junction, as thick as the line + 1.
            ModifierKind::Inhibitor => {
                assert_eq!((b.position.x, b.dimensions.width), (-12.0, 4.0))
            }
            _ => assert_eq!(b.position.x, -40.0),
        }
    }

    // Dotted: short dashes. Lines turned off: not drawn.
    m.find_reaction_mut("R4").unwrap().modifiers[0].look.pattern = LinePattern::Dotted;
    let (_, ri) = export(&m);
    assert_eq!(
        group(&ri, "srg_R4_mod_S3")
            .presentation
            .stroke_dasharray
            .as_deref(),
        Some("2,4")
    );
    m.hide_modifier_lines = true;
    let (_, ri) = export(&m);
    let g = group(&ri, "srg_R4_mod_S3");
    assert_eq!(g.presentation.stroke.as_deref(), Some("none"));
    assert_eq!(g.presentation.end_head, None);
}

#[test]
fn compartments_texts_and_the_background() {
    let mut m = model();
    let mut c = pd_core::model::Compartment::new("cell");
    c.frame = Some(pd_core::model::Frame {
        center: pt(200.0, 0.0),
        width: 600.0,
        height: 20.0,
    });
    m.compartments.push(c);
    let t = m.add_text(pt(0.0, 200.0), "A note");
    {
        let t = m.find_text_mut(&t).unwrap();
        t.font.typeface = Typeface::Serif;
        t.font.italic = true;
        t.size = 22.0;
        t.align = TextAlign::Right;
        t.color = Rgba::rgb(0x54, 0x6E, 0x7A);
    }
    m.background = Rgba::rgb(0xFF, 0xFF, 0xFF);
    let (l, ri) = export(&m);

    // The box, its corners rounded by 14 at most: half its height here.
    let g = group(&ri, "cg_cell");
    assert_eq!(value(&ri, &g.presentation.fill), "#F0F6EA");
    let sr::Primitive::Rectangle(r) = &g.children[0] else {
        panic!("no rectangle")
    };
    assert_eq!(r.rx, rel("10"));
    // Its name, in a text glyph of its own.
    let name = l
        .text_glyphs
        .iter()
        .find(|t| t.graphical_object.as_deref() == Some("cg_cell"))
        .expect("a name");
    assert_eq!(name.origin_of_text.as_deref(), Some("cell"));
    assert_eq!(
        value(&ri, &group(&ri, &name.id).presentation.stroke),
        "#3D5C2B"
    );

    let tg = l
        .text_glyphs
        .iter()
        .find(|g| g.text.as_deref() == Some("A note"))
        .unwrap();
    let p = &group(&ri, &tg.id).presentation;
    assert_eq!(
        (
            p.font_family.as_deref(),
            p.font_style.as_deref(),
            p.font_size.clone(),
            p.text_anchor.as_deref()
        ),
        (Some("serif"), Some("italic"), rel("22"), Some("end"))
    );
    assert_eq!(value(&ri, &p.stroke), "#546E7A");
    assert_eq!(ri.background_color.as_deref(), Some("#FFFFFF"));
}
