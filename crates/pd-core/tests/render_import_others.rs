//! SBML import reads other tools' Render (docs/render-design.md §5): styles
//! by type as COPASI writes them, text styled by what it labels as
//! SBMLNetwork writes it, and shapes PathwayDesigner can only approximate,
//! with one warning saying so.

use pd_core::antimony::ImportOptions;
use pd_core::model::{Model, NodeShape, Rgba};
use sbml_io::render as sr;
use sbml_io::*;

const OPTS: ImportOptions = ImportOptions {
    convert_catalytic: true,
    seed: 1,
};

const RED: Rgba = Rgba::rgb(0xFF, 0, 0);
const BLUE: Rgba = Rgba::rgb(0, 0, 0xFF);
const GREEN: Rgba = Rgba::rgb(0, 0xFF, 0);

fn rel(s: &str) -> Option<sr::RelAbs> {
    Some(sr::RelAbs(s.into()))
}

/// A style for glyphs of `types`, or, if `ids` isn't empty, for those ids.
fn style(types: &[&str], ids: &[&str], g: sr::Group) -> sr::Style {
    let v = |l: &[&str]| l.iter().map(|s| s.to_string()).collect();
    sr::Style {
        type_list: v(types),
        id_list: v(ids),
        group: Some(g),
        ..Default::default()
    }
}

fn shapes(fill: &str, stroke: &str, children: Vec<sr::Primitive>) -> sr::Group {
    sr::Group {
        presentation: sr::Presentation {
            fill: Some(fill.into()),
            stroke: Some(stroke.into()),
            ..Default::default()
        },
        children,
        ..Default::default()
    }
}

fn text(colour: &str, size: &str) -> sr::Group {
    sr::Group {
        presentation: sr::Presentation {
            stroke: Some(colour.into()),
            font_size: rel(size),
            ..Default::default()
        },
        ..Default::default()
    }
}

fn rect(at: &str, size: &str) -> sr::Primitive {
    sr::Primitive::Rectangle(sr::Rectangle {
        x: rel(at),
        y: rel(at),
        width: rel(size),
        height: rel(size),
        ..Default::default()
    })
}

/// Species A, B and C, each with a glyph and a text glyph on it, styled by
/// one render information: global (as COPASI writes it) or the layout's own
/// (needed for styles that name glyph ids).
fn import(
    styles: Vec<sr::Style>,
    gradients: Vec<sr::Gradient>,
    local: bool,
) -> (Model, Vec<String>) {
    let mut m = sbml_io::Model {
        id: Some("m".into()),
        ..Default::default()
    };
    m.compartments.push(Compartment {
        id: "c".into(),
        size: Some(1.0),
        constant: Some(true),
        ..Default::default()
    });
    let mut l = Layout {
        id: "l".into(),
        dimensions: Dimensions::new(500.0, 200.0),
        ..Default::default()
    };
    for (i, id) in ["A", "B", "C"].into_iter().enumerate() {
        m.species.push(Species {
            id: id.into(),
            compartment: "c".into(),
            initial_concentration: Some(1.0),
            has_only_substance_units: Some(false),
            boundary_condition: Some(false),
            constant: Some(false),
            ..Default::default()
        });
        let x = 20.0 + 150.0 * i as f64;
        l.species_glyphs.push(SpeciesGlyph {
            id: format!("sg_{id}"),
            species: Some(id.into()),
            bounding_box: Some(BoundingBox::new(x, 50.0, 80.0, 40.0)),
            ..Default::default()
        });
        l.text_glyphs.push(TextGlyph {
            id: format!("tg_{id}"),
            graphical_object: Some(format!("sg_{id}")),
            origin_of_text: Some(id.into()),
            bounding_box: Some(BoundingBox::new(x, 50.0, 80.0, 40.0)),
            ..Default::default()
        });
    }
    let colours = [
        ("red", "#FF0000"),
        ("blue", "#0000FF"),
        ("green", "#00FF00"),
    ];
    let list = sr::RenderList {
        items: vec![sr::RenderInformation {
            id: "r".into(),
            color_definitions: colours
                .iter()
                .map(|(id, v)| sr::ColorDefinition {
                    id: (*id).into(),
                    value: (*v).into(),
                    ..Default::default()
                })
                .collect(),
            gradients,
            styles,
            ..Default::default()
        }],
        ..Default::default()
    };
    if local {
        l.render = Some(list);
    } else {
        m.global_render = Some(list);
    }
    m.layouts.push(l);
    let mut doc = Document::new(3, 1);
    doc.model = Some(m);
    let i = pd_core::sbml::import(&write(&doc), &OPTS).unwrap();
    let w = i
        .warnings
        .into_iter()
        .filter(|w| w.starts_with("Render"))
        .collect();
    (i.model, w)
}

#[test]
fn styles_by_type_as_copasi_writes_them() {
    let (m, w) = import(
        vec![
            style(
                &["SPECIESGLYPH"],
                &[],
                shapes("red", "blue", vec![rect("0", "100%")]),
            ),
            style(&["TEXTGLYPH"], &[], text("green", "20")),
        ],
        Vec::new(),
        false,
    );
    let a = m.find_species("A").unwrap();
    assert_eq!((a.style.fill_color, a.style.border_color), (RED, BLUE));
    assert_eq!(a.shape, NodeShape::Rectangle);
    assert_eq!((a.style.label_color, a.style.font_size), (GREEN, 20.0));
    assert!(w.is_empty(), "{w:?}");
}

#[test]
fn text_is_styled_by_what_it_labels_as_sbmlnetwork_writes_it() {
    let (m, _) = import(
        vec![
            style(&["TEXTGLYPH"], &[], text("green", "20")),
            style(&["SPECIESGLYPH_TEXTGLYPH"], &[], text("blue", "24")),
        ],
        Vec::new(),
        false,
    );
    let a = m.find_species("A").unwrap();
    assert_eq!((a.style.label_color, a.style.font_size), (BLUE, 24.0));
}

#[test]
fn text_with_no_style_takes_its_owners() {
    let mut owner = shapes("red", "blue", vec![rect("0", "100%")]);
    owner.presentation.font_size = rel("18");
    let (m, _) = import(
        vec![style(&["SPECIESGLYPH"], &[], owner)],
        Vec::new(),
        false,
    );
    assert_eq!(m.find_species("A").unwrap().style.font_size, 18.0);
}

#[test]
fn shapes_it_cant_draw_are_approximated_and_reported() {
    // A: several shapes (the largest is drawn), a gradient (its middle
    // stop), an image (left out).
    let several = shapes(
        "fade",
        "blue",
        vec![
            sr::Primitive::Ellipse(sr::Ellipse {
                cx: rel("50%"),
                cy: rel("50%"),
                rx: rel("10%"),
                ..Default::default()
            }),
            rect("0", "100%"),
            sr::Primitive::Image(sr::Image {
                href: Some("x.png".into()),
                ..Default::default()
            }),
        ],
    );
    let stop = |off: &str, c: &str| sr::GradientStop {
        offset: rel(off),
        stop_color: Some(c.into()),
        ..Default::default()
    };
    let gradient = sr::Gradient {
        id: "fade".into(),
        name: None,
        spread_method: None,
        kind: sr::GradientKind::Linear {
            x1: None,
            y1: None,
            z1: None,
            x2: None,
            y2: None,
            z2: None,
        },
        stops: vec![
            stop("0%", "green"),
            stop("45%", "red"),
            stop("100%", "green"),
        ],
        meta: Default::default(),
    };
    // B: a rectangle inset in its box. C: a triangle.
    let inset = shapes("red", "blue", vec![rect("10%", "80%")]);
    let triangle = sr::Primitive::Polygon(sr::Polygon {
        elements: [("50%", "0%"), ("100%", "100%"), ("0%", "100%")]
            .into_iter()
            .map(|(x, y)| sr::RenderPoint {
                x: rel(x),
                y: rel(y),
                ..Default::default()
            })
            .collect(),
        ..Default::default()
    });
    let (m, w) = import(
        vec![
            style(&[], &["sg_A"], several),
            style(&[], &["sg_B"], inset),
            style(&[], &["sg_C"], shapes("red", "blue", vec![triangle])),
        ],
        vec![gradient],
        true,
    );
    let a = m.find_species("A").unwrap();
    assert_eq!(a.shape, NodeShape::Rectangle, "the largest shape");
    assert_eq!(a.style.fill_color, RED, "the stop nearest the middle");
    assert_eq!(m.find_species("B").unwrap().shape, NodeShape::Rectangle);
    assert_eq!(m.find_species("C").unwrap().shape, NodeShape::Rectangle);
    assert_eq!(
        w,
        ["Render: 1 gradient shown as one colour, 3 shapes approximated, 1 image left out"]
    );
}
