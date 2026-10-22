//! A model's drawing as JSON: every node's centre, size and what it is a
//! drawing of, every reaction's junction and the nodes its lines go to, and
//! every compartment box. For comparing a layout read from Antimony's layout
//! statements with the same layout read from SBML Layout
//! (docs/antimony-layout-design.md §8; scripts/antimony_layout_check.py).
//!
//! ```text
//! cargo run -p pd-core --example drawing_report -- <model.ant | model.xml>
//! ```

use pd_core::model::Model;
use serde_json::json;

fn read(path: &str) -> (Model, Vec<String>) {
    let text = std::fs::read_to_string(path).expect("readable");
    if path.ends_with(".xml") || path.ends_with(".sbml") {
        let opts = pd_core::antimony::ImportOptions {
            convert_catalytic: false,
            seed: 1,
        };
        let i = pd_core::sbml::import(&text, &opts).expect("SBML");
        return (i.model, i.warnings);
    }
    let opts = pd_core::antimony::ImportOptions {
        convert_catalytic: false,
        seed: 1,
    };
    let i = pd_core::antimony::import(&text, &opts).expect("Antimony");
    let warnings = i.warnings.iter().map(|w| w.to_string()).collect();
    (i.model, warnings)
}

fn main() {
    let path = std::env::args().nth(1).expect("a model file");
    let (m, warnings) = read(&path);
    let nodes: Vec<_> = m
        .species()
        .iter()
        .map(|s| {
            json!({
                "id": s.id,
                "of": s.primary_id(),
                "null": s.is_null_node,
                "x": s.center.x, "y": s.center.y, "w": s.width, "h": s.height,
            })
        })
        .collect();
    let reactions: Vec<_> = m
        .reactions()
        .iter()
        .map(|r| {
            let ends = |list: &[pd_core::model::Participant]| -> Vec<String> {
                list.iter().map(|p| p.species.to_string()).collect()
            };
            // Each line's handles (when set) and corner points.
            let legs = |role: &str, list: &[pd_core::model::Participant]| -> Vec<serde_json::Value> {
                list.iter()
                    .map(|p| {
                        json!({
                            "role": role,
                            "node": p.species.to_string(),
                            "handles": p.ctrl_pts_set.then_some([p.ctrl1.x, p.ctrl1.y, p.ctrl2.x, p.ctrl2.y]),
                            "points": p.points.iter().map(|q| [q.x, q.y]).collect::<Vec<_>>(),
                        })
                    })
                    .collect()
            };
            let mut lines = legs("reactants", &r.reactants);
            lines.extend(legs("products", &r.products));
            lines.extend(legs("modifiers", &r.modifiers));
            json!({
                "id": r.id,
                "x": r.junction.x, "y": r.junction.y,
                "mode": format!("{:?}", r.mode),
                "reactants": ends(&r.reactants),
                "products": ends(&r.products),
                "modifiers": ends(&r.modifiers),
                "lines": lines,
            })
        })
        .collect();
    let compartments: Vec<_> = m
        .compartments
        .iter()
        .filter_map(|c| {
            let f = c.frame?;
            Some(json!({"id": c.id, "x": f.center.x, "y": f.center.y, "w": f.width, "h": f.height}))
        })
        .collect();
    println!(
        "{}",
        json!({"nodes": nodes, "reactions": reactions, "compartments": compartments, "warnings": warnings})
    );
}
