//! The paper's reaction-drawing figure (Fig 2), drawn by the app's own code:
//! one reaction drawn the ways PathwayDesigner offers (S §5.5, §5.6,
//! §5.13): straight legs, a single line, Bézier legs, a smooth junction, a
//! stem, and a stem with smooth curves. A + B -> C + D for every way but the single line, which is for a
//! reaction with one reactant and one product, A -> B.
//!
//! Writes PNG, SVG and vector PDF files for each panel into a folder;
//! `Paper/figures/fig_drawing.tex` puts the PDFs together.
//!
//! ```text
//! cargo run -p pd-export --release --example figure_drawing -- <out>
//! ```

use std::path::Path;

use pd_core::geometry::{Pt, pt};
use pd_core::model::{Model, Participant, Reaction, ReactionMode, SpeciesNode};
use pd_core::render::draw::Shown;

const DPI: f64 = 300.0;

/// Names large enough to read once a panel is scaled to a third of a page
/// (PLOS asks for about 8 pt), as in the modifier figure.
const FONT: f64 = 26.0;

/// Species boxes sized for [`FONT`].
fn big(id: &str, at: Pt) -> SpeciesNode {
    let mut s = SpeciesNode::new(id, at);
    s.resize(110.0, 54.0);
    s.style.has_custom_style = true;
    s.style.font_size = FONT;
    s
}

/// A + B -> C + D (or A -> B with `single`), the junction in the middle,
/// lines thick enough to show at a third of a page, and the panel's name
/// beneath.
fn reaction(single: bool, name: &str) -> Model {
    let mut m = Model::new();
    let mut r = Reaction::new("J1", pt(220.0, 0.0));
    if single {
        m.push_species(big("A", pt(0.0, 0.0))).unwrap();
        m.push_species(big("B", pt(440.0, 0.0))).unwrap();
        r.reactants.push(Participant::new("A", 1.0));
        r.products.push(Participant::new("B", 1.0));
    } else {
        for (id, at) in [
            ("A", pt(0.0, -90.0)),
            ("B", pt(0.0, 90.0)),
            ("C", pt(440.0, -90.0)),
            ("D", pt(440.0, 90.0)),
        ] {
            m.push_species(big(id, at)).unwrap();
        }
        r.reactants.push(Participant::new("A", 1.0));
        r.reactants.push(Participant::new("B", 1.0));
        r.products.push(Participant::new("C", 1.0));
        r.products.push(Participant::new("D", 1.0));
    }
    r.style.has_custom_style = true;
    r.style.line_width = 3.0;
    r.is_reversible = false;
    m.push_reaction(r).unwrap();
    // Every panel the same height, so they line up: the name at the foot.
    let t = m.add_text(pt(220.0 - name.len() as f64 * 7.0, 150.0), name);
    m.find_text_mut(&t).unwrap().size = FONT;
    // And the top of the tallest panel, kept by an empty text.
    let top = m.add_text(pt(220.0, -150.0), " ");
    m.find_text_mut(&top).unwrap().size = FONT;
    m
}

fn main() {
    let out = std::env::args().nth(1).expect("an output folder");
    let out = Path::new(&out);
    std::fs::create_dir_all(out).unwrap();
    // White, for print, rather than the app's background.
    let write = |file: &str, model: &Model| {
        let mut model = model.clone();
        model.background = pd_core::model::Rgba::rgb(0xFF, 0xFF, 0xFF);
        let model = &model;
        let png = pd_export::model_to_png(model, DPI, Shown::ALL).unwrap();
        std::fs::write(out.join(format!("{file}.png")), png).unwrap();
        let svg = pd_core::render::svg::model_to_svg(model, 1.0, Shown::ALL);
        std::fs::write(out.join(format!("{file}.svg")), svg).unwrap();
        let pdf = pd_core::render::pdf::model_to_pdf(
            model,
            pd_core::render::pdf::PageSize::Drawing,
            false,
            Shown::ALL,
        );
        std::fs::write(out.join(format!("{file}.pdf")), pdf).unwrap();
    };

    // A: straight legs meeting at the junction.
    let m = reaction(false, "Straight");
    write("A", &m);

    // B: a single straight line, for one reactant and one product.
    let mut m = reaction(true, "Single line");
    m.find_reaction_mut("J1").unwrap().mode = ReactionMode::Linear;
    write("B", &m);

    // C: Bézier legs, their handles dragged so each leg bows outwards and
    // meets the junction at an angle (unlike D).
    let mut m = reaction(false, "Bézier");
    {
        let r = m.find_reaction_mut("J1").unwrap();
        r.mode = ReactionMode::Bezier;
        // A reactant's first handle is by its species, a product's by the
        // junction (S §5.6.1).
        let leg = |p: &mut Participant, c1: Pt, c2: Pt| {
            (p.ctrl1, p.ctrl2, p.ctrl_pts_set) = (c1, c2, true);
        };
        leg(&mut r.reactants[0], pt(110.0, -160.0), pt(200.0, -70.0));
        leg(&mut r.reactants[1], pt(110.0, 160.0), pt(200.0, 70.0));
        leg(&mut r.products[0], pt(240.0, -70.0), pt(330.0, -160.0));
        leg(&mut r.products[1], pt(240.0, 70.0), pt(330.0, 160.0));
    }
    write("C", &m);

    // D: a smooth junction, the curves passing through it without a corner.
    let mut m = reaction(false, "Smooth junction");
    {
        let r = m.find_reaction_mut("J1").unwrap();
        r.mode = ReactionMode::Bezier;
        r.junction_smooth = true;
    }
    pd_core::bezier::apply_smooth_handles(&mut m, "J1");
    write("D", &m);

    // E: a stem, a straight centre line the reactants join at one end and
    // the products leave from the other, as Escher draws reactions.
    let mut m = reaction(false, "Stem");
    {
        let r = m.find_reaction_mut("J1").unwrap();
        r.mode = ReactionMode::Bezier;
        let stem = (pt(-60.0, 0.0), pt(60.0, 0.0));
        r.stem = Some(stem);
        r.stem_straight = pd_core::model::stem_is_straight(stem);
    }
    write("E", &m);

    // F: a stem with smooth curves, each joining the stem along its line,
    // as Escher's maps are drawn.
    let mut m = reaction(false, "Stem, smooth");
    {
        let r = m.find_reaction_mut("J1").unwrap();
        r.mode = ReactionMode::Bezier;
        r.junction_smooth = true;
        let stem = (pt(-60.0, 0.0), pt(60.0, 0.0));
        r.stem = Some(stem);
        r.stem_straight = pd_core::model::stem_is_straight(stem);
    }
    pd_core::bezier::apply_smooth_handles(&mut m, "J1");
    write("F", &m);
}
