//! The paper's modifier figure (evaluation E7), drawn by the app's own code:
//!
//! - A: one reaction whose rate law is edited three times through the
//!   diagram view; each edit brings the modifier lines in line with the law
//!   (S §8.4), and each state is drawn with its law beneath it;
//! - B: the four modifier kinds (S §5.7.1).
//!
//! Writes PNG, SVG and vector PDF files for each panel into a folder;
//! `Paper/figures/fig3_modifiers.tex` puts the PDFs together.
//!
//! ```text
//! cargo run -p pd-export --release --example figure_modifiers -- <out>
//! ```

use std::path::Path;

use pd_core::geometry::pt;
use pd_core::model::{Model, ModifierKind, Participant, Reaction, SpeciesNode};
use pd_core::render::draw::Shown;
use pd_core::view::DiagramView;

const DPI: f64 = 300.0;

/// Names and text large enough to read once the panel is scaled to a third
/// of a page (PLOS asks for about 8 pt).
const FONT: f64 = 26.0;

/// Thicker reaction and modifier lines, and modifier symbols 2.5 times their
/// usual size, so that they still show at a third of a page.
fn bold_lines(m: &mut Model) {
    for r in m.reactions_mut() {
        r.style.has_custom_style = true;
        r.style.line_width = 3.0;
        for p in &mut r.modifiers {
            let (size, gap) = p.kind.symbol_defaults();
            p.look.width = Some(2.5);
            p.look.symbol_size = Some(size * 2.5);
            p.look.symbol_gap = Some(gap * 2.0);
        }
    }
}

/// Species boxes sized for [`FONT`].
fn big(id: &str, at: pd_core::geometry::Pt) -> SpeciesNode {
    let mut s = SpeciesNode::new(id, at);
    s.resize(110.0, 54.0);
    s.style.has_custom_style = true;
    s.style.font_size = FONT;
    s
}

fn main() {
    let out = std::env::args().nth(1).expect("an output folder");
    let out = Path::new(&out);
    std::fs::create_dir_all(out).unwrap();
    // White, for print, rather than the app's background.
    let write = |name: &str, model: &Model, shown: Shown| {
        let mut model = model.clone();
        model.background = pd_core::model::Rgba::rgb(0xFF, 0xFF, 0xFF);
        let model = &model;
        let png = pd_export::model_to_png(model, DPI, shown).unwrap();
        std::fs::write(out.join(format!("{name}.png")), png).unwrap();
        let svg = pd_core::render::svg::model_to_svg(model, 1.0, shown);
        std::fs::write(out.join(format!("{name}.svg")), svg).unwrap();
        let pdf = pd_core::render::pdf::model_to_pdf(
            model,
            pd_core::render::pdf::PageSize::Drawing,
            false,
            shown,
        );
        std::fs::write(out.join(format!("{name}.pdf")), pdf).unwrap();
    };

    // A: S -> P, with E and I nearby, and three rate laws in turn.
    let mut m = Model::new();
    for (id, at) in [
        ("S", pt(0.0, 0.0)),
        ("P", pt(360.0, 0.0)),
        ("E", pt(180.0, -130.0)),
        ("I", pt(180.0, 130.0)),
    ] {
        m.push_species(big(id, at)).unwrap();
    }
    let mut r = Reaction::new("J1", pt(180.0, 0.0));
    r.reactants.push(Participant::new("S", 1.0));
    r.products.push(Participant::new("P", 1.0));
    m.push_reaction(r).unwrap();
    let mut view = DiagramView::new(m);
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
        bold_lines(&mut drawn);
        let t = drawn.add_text(pt(-40.0, 210.0), &format!("v = {law}"));
        drawn.find_text_mut(&t).unwrap().size = FONT;
        write(&format!("A{}", i + 1), &drawn, Shown::ALL);
    }

    // B: the four kinds, one reaction each, all S -> P with modifier M: the
    // first reaction has the species, the others aliases of them, drawn
    // without the alias indicator so all four look alike.
    let mut m = Model::new();
    let kinds = [
        ("Modifier", ModifierKind::Modifier),
        ("Activator", ModifierKind::Activator),
        ("Inhibitor", ModifierKind::Inhibitor),
        ("Catalyst", ModifierKind::Catalyst),
    ];
    for (i, (name, kind)) in kinds.iter().enumerate() {
        let x = i as f64 * 420.0;
        let mut node = |name: &str, at| {
            if i == 0 {
                m.push_species(big(name, at)).unwrap();
                return name.to_owned();
            }
            let id = m.add_alias(name, at).unwrap();
            let alias = m.find_species_mut(&id).unwrap();
            alias.resize(110.0, 54.0);
            alias.style = big(name, at).style;
            id
        };
        let a = node("S", pt(x, 0.0));
        let b = node("P", pt(x + 260.0, 0.0));
        let e = node("M", pt(x + 130.0, -120.0));
        let mut r = Reaction::new(format!("R{i}"), pt(x + 130.0, 0.0));
        r.reactants.push(Participant::new(&a, 1.0));
        r.products.push(Participant::new(&b, 1.0));
        r.kinetic_law = "k*M*S".to_owned();
        let mut modifier = Participant::new(&e, 1.0);
        modifier.kind = *kind;
        r.modifiers.push(modifier);
        m.push_reaction(r).unwrap();
        let t = m.add_text(pt(x + 70.0, 60.0), name);
        m.find_text_mut(&t).unwrap().size = FONT;
    }
    bold_lines(&mut m);
    let no_alias_marks = Shown {
        alias_indicator: false,
        ..Shown::ALL
    };
    write("B", &m, no_alias_marks);
}
