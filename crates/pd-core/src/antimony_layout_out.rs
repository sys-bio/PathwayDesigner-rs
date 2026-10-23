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

use crate::antimony_layout::{NULL_SIZE, REACTION_SIZE};
use crate::geometry::Pt;
use crate::model::{Model, SpeciesNode};

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
        if common != Some((s.width, s.height)) {
            out.push(format!(
                "{} = {}",
                key.replace("{}", "size"),
                pair(s.width, s.height)
            ));
        }
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

    for c in &model.compartments {
        let Some(f) = c.drawn_frame() else {
            continue;
        };
        let min = f.min();
        out.push(format!("{}.position = {}", c.id, pair(min.x, min.y)));
        out.push(format!("{}.size = {}", c.id, pair(f.width, f.height)));
    }
    out
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
    use crate::geometry::pt;
    use crate::model::{Frame, Model};

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
            "J1.position = {190, 90}",
            "J0.--.position = {16, 6}",
            "C.position = {0, 20}",
            "C.size = {400, 360}",
        ] {
            assert!(s.iter().any(|l| l == line), "{line} missing from {s:#?}");
        }
        // libAntimony's null node is 30 x 30; ours is 28 x 28, so it is said.
        assert!(s.iter().any(|l| l == "J0.--.size = {28, 28}"), "{s:#?}");
        // Every species is the common size: none has its own.
        assert!(!s.iter().any(|l| l.starts_with("S1.size")), "{s:#?}");
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
    }

    #[test]
    fn an_alias_no_line_goes_to_is_not_written() {
        let mut m = placed();
        m.add_alias("S2", pt(500.0, 500.0)).unwrap();
        assert!(!statements(&m).iter().any(|l| l.starts_with("S2.position.")));
    }
}
