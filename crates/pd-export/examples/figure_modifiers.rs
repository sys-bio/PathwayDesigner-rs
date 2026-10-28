//! The paper's modifier figure (evaluation E7), drawn by the app's own code,
//! as eight panels of the same size, four to a row:
//!
//! - A1-A3: one reaction whose rate law is edited three times through the
//!   diagram view; each edit brings the modifier lines in line with the law
//!   (S §8.4), and each state is drawn with its law beneath it;
//! - B1-B4: the four modifier kinds (S §5.7.1);
//! - C: a self modifier, a product marked as an activator of its own
//!   reaction with Mark as Modifier, drawn as a loop.
//!
//! Writes PNG, SVG and vector PDF files for each panel into a folder;
//! `Paper/figures/fig3_modifiers.tex` puts the PDFs together.
//!
//! ```text
//! cargo run -p pd-export --release --example figure_modifiers -- <out>
//! ```

use std::path::Path;

use pd_core::geometry::{Pt, pt};
use pd_core::model::{Model, ModifierKind, Participant, Reaction, SpeciesNode};
use pd_core::render::draw::Shown;
use pd_core::view::DiagramView;

const DPI: f64 = 300.0;

/// Names and text large enough to read once a panel is scaled to a quarter
/// of a page (PLOS asks for about 8 pt).
const FONT: f64 = 32.0;

/// Where every panel's reaction sits: S on the left, P on the right, the
/// junction between them, a modifier above and an inhibitor below.
const S: Pt = pt(0.0, 0.0);
const P: Pt = pt(400.0, 0.0);
const J: Pt = pt(200.0, 0.0);
const ABOVE: Pt = pt(200.0, -150.0);
const BELOW: Pt = pt(200.0, 150.0);
/// The line of text under every panel.
const FOOT: f64 = 230.0;

/// Thicker reaction and modifier lines, and modifier symbols 2.5 times their
/// usual size, so that they still show at a quarter of a page.
fn bold_lines(m: &mut Model) {
    for r in m.reactions_mut() {
        r.style.has_custom_style = true;
        r.style.line_width = 3.5;
        for p in &mut r.modifiers {
            let (size, gap) = p.kind.symbol_defaults();
            p.look.width = Some(3.0);
            p.look.symbol_size = Some(size * 2.5);
            p.look.symbol_gap = Some(gap * 2.0);
        }
    }
}

/// Species boxes sized for [`FONT`].
fn big(id: &str, at: Pt) -> SpeciesNode {
    let mut s = SpeciesNode::new(id, at);
    s.resize(130.0, 62.0);
    s.style.has_custom_style = true;
    s.style.font_size = FONT;
    s
}

/// S -> P, with the species in `others` placed too.
fn reaction(others: &[(&str, Pt)]) -> Model {
    let mut m = Model::new();
    m.push_species(big("S", S)).unwrap();
    m.push_species(big("P", P)).unwrap();
    for (id, at) in others {
        m.push_species(big(id, *at)).unwrap();
    }
    let mut r = Reaction::new("J1", J);
    r.reactants.push(Participant::new("S", 1.0));
    r.products.push(Participant::new("P", 1.0));
    m.push_reaction(r).unwrap();
    m
}

/// `text` under the panel, and empty texts at its corners, so every panel
/// covers the same area and all are drawn to the same scale.
fn frame(m: &mut Model, text: &str) {
    let width = text.chars().count() as f64 * FONT * 0.27;
    for (at, text) in [
        (pt(J.x - width, FOOT), text),
        (pt(S.x - 70.0, ABOVE.y - 50.0), " "),
        (pt(P.x + 50.0, FOOT), " "),
    ] {
        let t = m.add_text(at, text);
        m.find_text_mut(&t).unwrap().size = FONT;
    }
}

fn main() {
    let out = std::env::args().nth(1).expect("an output folder");
    let out = Path::new(&out);
    std::fs::create_dir_all(out).unwrap();
    // White, for print, rather than the app's background.
    let write = |name: &str, model: &Model| {
        let mut model = model.clone();
        model.background = pd_core::model::Rgba::rgb(0xFF, 0xFF, 0xFF);
        bold_lines(&mut model);
        let model = &model;
        let png = pd_export::model_to_png(model, DPI, Shown::ALL).unwrap();
        std::fs::write(out.join(format!("{name}.png")), png).unwrap();
        let svg = pd_core::render::svg::model_to_svg(model, 1.0, Shown::ALL);
        std::fs::write(out.join(format!("{name}.svg")), svg).unwrap();
        let pdf = pd_core::render::pdf::model_to_pdf(
            model,
            pd_core::render::pdf::PageSize::Drawing,
            false,
            Shown::ALL,
        );
        std::fs::write(out.join(format!("{name}.pdf")), pdf).unwrap();
    };

    // A: S -> P, with E and I nearby, and three rate laws in turn.
    let mut view = DiagramView::new(reaction(&[("E", ABOVE), ("I", BELOW)]));
    for (i, law) in ["k1*S", "k1*E*S/(Km + S)", "k1*S/(1 + I/Ki)"]
        .iter()
        .enumerate()
    {
        view.set_kinetic_law("J1", law);
        let mut drawn = view.model().clone();
        let lines: Vec<&str> = drawn
            .find_reaction("J1")
            .unwrap()
            .modifiers
            .iter()
            .map(|p| p.species.as_str())
            .collect();
        eprintln!("A{}: {law} -> modifier lines from {lines:?}", i + 1);
        frame(&mut drawn, &format!("v = {law}"));
        write(&format!("A{}", i + 1), &drawn);
    }

    // B: the four kinds, each S -> P with modifier M.
    let kinds = [
        ("Modifier", ModifierKind::Modifier),
        ("Activator", ModifierKind::Activator),
        ("Inhibitor", ModifierKind::Inhibitor),
        ("Catalyst", ModifierKind::Catalyst),
    ];
    for (i, (name, kind)) in kinds.iter().enumerate() {
        let mut m = reaction(&[("M", ABOVE)]);
        let r = m.find_reaction_mut("J1").unwrap();
        r.kinetic_law = "k1*M*S".to_owned();
        let mut modifier = Participant::new("M", 1.0);
        modifier.kind = *kind;
        r.modifiers.push(modifier);
        frame(&mut m, name);
        write(&format!("B{}", i + 1), &m);
    }

    // C: P activates its own production. The law names P, a product, so
    // modifier sync leaves it alone; Mark as Modifier makes it one, drawn
    // as a loop from P's line back to the junction.
    let law = "k1*S*P";
    let mut view = DiagramView::new(reaction(&[]));
    view.set_kinetic_law("J1", law);
    view.mark_as_modifier("J1", "P");
    let mut m = view.model().clone();
    let r = m.find_reaction_mut("J1").unwrap();
    assert_eq!(r.modifiers.len(), 1, "P marked as a modifier");
    r.modifiers[0].kind = ModifierKind::Activator;
    frame(&mut m, &format!("v = {law}"));
    write("C", &m);
}
