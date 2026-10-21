//! SBML import reads the look back from SBML Render (docs/render-design.md
//! §5): what PathwayDesigner writes comes back, with or without its own
//! notes, and a value equal to the default comes back as the default.

use pd_core::antimony::ImportOptions;
use pd_core::geometry::pt;
use pd_core::model::{
    BorderDash, Frame, LinePattern, Model, ModifierKind, ModifierLook, NodeShape, Participant,
    Rgba, SavedStyle, TextAlign, Typeface,
};
use pd_core::styles;

/// The SBML with every PathwayDesigner note taken out: what a file looks
/// like to this importer if only standard Layout and Render had carried it.
fn without_notes(xml: &str) -> String {
    let mut out = String::new();
    let mut rest = xml;
    while let Some(start) = rest.find("<pd:") {
        out.push_str(&rest[..start]);
        let end = rest[start..].find("/>").unwrap();
        rest = &rest[start + end + 2..];
    }
    out.push_str(rest);
    out
}

fn round_trip(model: &Model, notes: bool) -> Model {
    let xml = pd_core::sbml::export(model).xml;
    let xml = if notes { xml } else { without_notes(&xml) };
    let imported = pd_core::sbml::import(
        &xml,
        &ImportOptions {
            convert_catalytic: true,
            seed: 7,
        },
    )
    .unwrap();
    imported.model
}

const RED: Rgba = Rgba::rgb(0xC8, 0x00, 0x00);
const GOLD: Rgba = Rgba::rgb(0xFF, 0xD5, 0x4F);
const SLATE: Rgba = Rgba::rgb(0x54, 0x6E, 0x7A);

/// Every look Render carries, each set away from its default.
fn styled() -> Model {
    let mut m = Model::new();
    let a = m.add_species(pt(0.0, 0.0), 80.0, 36.0);
    let b = m.add_species(pt(400.0, 0.0), 60.0, 60.0);
    let e = m.add_species(pt(200.0, -150.0), 90.0, 40.0);
    let f = m.add_species(pt(200.0, 150.0), 90.0, 40.0);
    let r = m.add_reaction(pt(200.0, 0.0));
    {
        let rx = m.find_reaction_mut(&r).unwrap();
        rx.reactants.push(Participant::new(a.clone(), 1.0));
        rx.products.push(Participant::new(b.clone(), 1.0));
        let mut me = Participant::new(e.clone(), 1.0);
        me.kind = ModifierKind::Inhibitor;
        me.look = ModifierLook {
            colour: Some(RED),
            width: Some(3.0),
            pattern: LinePattern::Solid,
            symbol_size: Some(30.0),
            symbol_gap: Some(12.0),
        };
        rx.modifiers.push(me);
        let mut mf = Participant::new(f.clone(), 1.0);
        mf.kind = ModifierKind::Activator;
        mf.look.pattern = LinePattern::Dotted;
        mf.look.symbol_size = Some(20.0);
        rx.modifiers.push(mf);
        rx.style.has_custom_style = true;
        rx.style.line_color = SLATE;
        rx.style.line_width = 4.0;
        rx.style.junction_color = GOLD;
        rx.style.font_size = 18.0;
        rx.style.label_color = RED;
        rx.style.label_italic = true;
        rx.label_offset = Some(pt(0.0, -30.0));
    }
    {
        let s = m.find_species_mut(&a).unwrap();
        s.shape = NodeShape::Hexagon;
        s.style.has_custom_style = true;
        s.style.fill_color = GOLD;
        s.style.border_color = RED;
        s.style.border_width = 2.5;
        s.style.label_color = SLATE;
        s.style.font_size = 16.0;
        s.style.label_bold = true;
        s.border_dash = BorderDash::Dashed;
    }
    {
        // A boundary species: its border is drawn 2.4 times as wide.
        let s = m.find_species_mut(&b).unwrap();
        s.shape = NodeShape::Circle;
        s.is_boundary = true;
        s.style.has_custom_style = true;
        s.style.border_width = 2.0;
    }
    // An alias of it: drawn with its primary's boundary border.
    m.add_alias(&b, pt(500.0, 120.0)).unwrap();
    {
        let s = m.find_species_mut(&e).unwrap();
        s.shape = NodeShape::Rectangle;
        s.style.no_border = true;
    }
    m.find_species_mut(&f).unwrap().shape = NodeShape::Ellipse;
    let mut c = pd_core::model::Compartment::new("cell");
    c.frame = Some(Frame {
        center: pt(200.0, 0.0),
        width: 700.0,
        height: 500.0,
    });
    c.style.has_custom_style = true;
    c.style.fill_color = Rgba::rgb(0xF5, 0xF5, 0xF5);
    c.style.border_color = SLATE;
    c.style.border_width = 4.0;
    c.style.font_size = 20.0;
    m.compartments.push(c);
    let t = m.add_text(pt(-50.0, 300.0), "A note");
    {
        let t = m.find_text_mut(&t).unwrap();
        t.font.typeface = Typeface::Serif;
        t.font.bold = true;
        t.font.italic = true;
        t.size = 22.0;
        t.align = TextAlign::Center;
        t.color = SLATE;
    }
    m.background = Rgba::rgb(0xFF, 0xFF, 0xFF);
    for s in m.species().iter().map(|s| s.id.clone()).collect::<Vec<_>>() {
        m.find_species_mut(&s).unwrap().compartment = "cell".into();
    }
    m
}

/// Checks every look Render carries.
fn check(got: &Model, want: &Model, how: &str) {
    for w in want.species() {
        let g = got.find_species(&w.id).unwrap();
        assert_eq!(g.style, w.style, "{how}: species {}", w.id);
        assert_eq!(g.shape, w.shape, "{how}: species {}", w.id);
    }
    // Dashed by hand: Render says dashed, and a primary's dashes can only be
    // by hand.
    assert_eq!(
        got.find_species("S1").unwrap().border_dash,
        BorderDash::Dashed,
        "{how}"
    );
    let (g, w) = (
        got.find_reaction("R5").unwrap(),
        want.find_reaction("R5").unwrap(),
    );
    assert_eq!(g.style, w.style, "{how}: reaction");
    for (gm, wm) in g.modifiers.iter().zip(&w.modifiers) {
        assert_eq!(
            (gm.kind, gm.look),
            (wm.kind, wm.look),
            "{how}: modifier {}",
            wm.species
        );
    }
    let (gc, wc) = (&got.compartments[0], &want.compartments[0]);
    assert_eq!(gc.style, wc.style, "{how}: compartment");
    let (gt, wt) = (&got.texts()[0], &want.texts()[0]);
    assert_eq!(
        (gt.font, gt.align, gt.size, gt.color),
        (wt.font, wt.align, wt.size, wt.color),
        "{how}: text"
    );
    assert_eq!(got.background, want.background, "{how}: background");
}

#[test]
fn every_look_comes_back_with_or_without_the_notes() {
    let m = styled();
    check(&round_trip(&m, true), &m, "with notes");
    check(&round_trip(&m, false), &m, "without notes");
}

#[test]
fn defaults_come_back_as_defaults() {
    let mut m = Model::new();
    let a = m.add_species(pt(0.0, 0.0), 80.0, 36.0);
    let b = m.add_species(pt(300.0, 0.0), 80.0, 36.0);
    let r = m.add_reaction(pt(150.0, 0.0));
    let rx = m.find_reaction_mut(&r).unwrap();
    rx.reactants.push(Participant::new(a, 1.0));
    rx.products.push(Participant::new(b, 1.0));
    m.add_alias("S1", pt(0.0, 100.0)).unwrap();
    let got = round_trip(&m, false);
    for s in got.species() {
        assert!(!s.style.has_custom_style, "{}: {:?}", s.id, s.style);
        assert_eq!(s.border_dash, BorderDash::Automatic, "{}", s.id);
        assert_eq!(s.shape, NodeShape::RoundedRect, "{}", s.id);
    }
    let r = &got.reactions()[0];
    assert!(
        !r.style.has_custom_style && r.style.junction_visible,
        "{:?}",
        r.style
    );
    assert_eq!(got.background, Rgba::DEFAULT);
    assert!(!got.hide_modifier_lines);
}

#[test]
fn hidden_junctions_and_hidden_modifier_lines() {
    let mut m = styled();
    m.find_reaction_mut("R5").unwrap().style.junction_visible = false;
    m.hide_modifier_lines = true;
    let got = round_trip(&m, false);
    assert!(!got.find_reaction("R5").unwrap().style.junction_visible);
    assert!(got.hide_modifier_lines);
}

#[test]
fn saved_styles_come_back_by_name() {
    let mut m = styled();
    let look = styles::look_of(&m, pd_core::model::StyleKind::Species, "S1").unwrap();
    m.styles.push(SavedStyle {
        name: "metabolite".into(),
        look,
    });
    m.find_species_mut("S1").unwrap().style_name = Some("metabolite".into());
    let got = round_trip(&m, false);
    assert_eq!(
        got.find_species("S1").unwrap().style_name.as_deref(),
        Some("metabolite")
    );
    let saved = styles::find(&got, pd_core::model::StyleKind::Species, "metabolite").unwrap();
    assert!(styles::same_look(saved.look, look));
    // No other item claims it.
    assert_eq!(
        styles::users(&got, pd_core::model::StyleKind::Species, "metabolite"),
        ["S1"]
    );
}
