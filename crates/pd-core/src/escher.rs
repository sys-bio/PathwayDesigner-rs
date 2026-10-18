//! Reading Escher maps (escher.github.io; S §11.7).
//!
//! An Escher map is `[header, map]`. The map's `nodes` are metabolites (each
//! drawing of one is its own node), and each reaction's markers: one
//! `midmarker`, its centre, and `multimarker`s on its spine either side. A
//! reaction's `segments` join them: a curve (handles `b1` near the segment's
//! `from` node and `b2` near its `to` node) from a metabolite to a multimarker,
//! then a short straight stub along the spine to the midmarker.
//!
//! Here a metabolite becomes a species, and its other drawings aliases of
//! it. A reaction's junction is its midmarker, and each leg is one curve from
//! the species to the junction: Escher's own handle by the metabolite, and
//! the multimarker as the handle by the junction, so the leg comes into the
//! junction along the spine (the stub merged into the curve). Labels keep
//! their places, and the diagram takes the Escher theme (S §8.7).

use std::collections::HashMap;

use serde_json::Value;

use crate::geometry::{Pt, pt};
use crate::model::{Compartment, Id, Model, Participant, Reaction, ReactionMode, SpeciesNode};
use crate::passes::{NULL_OFFSET, NullPlace, add_null_nodes};
use crate::render::draw::{label_size, reaction_label_size};
use crate::text::{font_metrics, text_width};
use crate::themes::{self, Theme};

/// A map read into a model, and what couldn't be carried over.
#[derive(Clone, Debug)]
pub struct Imported {
    pub model: Model,
    pub warnings: Vec<String>,
}

/// True if `text` looks like an Escher map: a JSON array (a PathwayDesigner
/// file is an object).
pub fn looks_like_escher(text: &str) -> bool {
    text.trim_start_matches('\u{FEFF}')
        .trim_start()
        .starts_with('[')
}

/// Reads an Escher map. An error if it isn't one.
pub fn import(text: &str) -> Result<Imported, String> {
    let v: Value = serde_json::from_str(text.trim_start_matches('\u{FEFF}'))
        .map_err(|e| format!("not JSON: {e}"))?;
    let (head, map) = match v.as_array().map(Vec::as_slice) {
        Some([head, map]) => (head, map),
        _ => return Err("not an Escher map (expected [header, map])".into()),
    };
    let nodes = map
        .get("nodes")
        .and_then(Value::as_object)
        .ok_or("the map has no nodes")?;
    let reactions = map
        .get("reactions")
        .and_then(Value::as_object)
        .ok_or("the map has no reactions")?;

    let mut model = Model::new();
    let mut warnings = Vec::new();
    model.model_name = head
        .get("map_name")
        .and_then(Value::as_str)
        .unwrap_or("escher")
        .to_owned();

    // Metabolites: the first drawing of each is the species, the others its
    // aliases. Nodes in the order of their ids, as Escher numbers them.
    let mut species_of: HashMap<&str, Id> = HashMap::new();
    let mut labels: Vec<(Id, Pt)> = Vec::new();
    for (key, n) in ordered(nodes) {
        if n.get("node_type").and_then(Value::as_str) != Some("metabolite") {
            continue;
        }
        let (Some(bigg), Some(at)) = (n.get("bigg_id").and_then(Value::as_str), point(n, "x", "y"))
        else {
            warnings.push(format!(
                "Node {key} has no metabolite or place; it was left out."
            ));
            continue;
        };
        let id = sid(bigg, 'M');
        let node_id = if model.find_species(&id).is_none() {
            let mut s = SpeciesNode::new(id.clone(), at);
            if let Some(c) = compartment_of(bigg) {
                if model.find_compartment(&c).is_none() {
                    // Escher has no boxes: the compartment is there, unseen.
                    let mut comp = Compartment::new(c.clone());
                    comp.hidden = true;
                    model.compartments.push(comp);
                }
                s.compartment = c;
            }
            model.push_species(s).expect("checked unused");
            id
        } else {
            model.add_alias(&id, at).expect("the species exists")
        };
        if let Some(l) = point(n, "label_x", "label_y") {
            labels.push((node_id.clone(), l));
        }
        species_of.insert(key, node_id);
    }

    // Reactions.
    let mut reaction_labels: Vec<(Id, Pt)> = Vec::new();
    for (key, r) in ordered(reactions) {
        let bigg = r.get("bigg_id").and_then(Value::as_str).unwrap_or(key);
        let mut id = sid(bigg, 'R');
        while model.find_reaction(&id).is_some() {
            id.push_str("_2");
        }
        let segments: Vec<&Value> = r
            .get("segments")
            .and_then(Value::as_object)
            .map(|s| s.values().collect())
            .unwrap_or_default();
        let kind = |id: &str| {
            nodes
                .get(id)
                .and_then(|n| n.get("node_type"))
                .and_then(Value::as_str)
        };
        let Some(mid) = segments
            .iter()
            .flat_map(|s| {
                let (a, b) = ends(s);
                [a, b]
            })
            .flatten()
            .find(|n| kind(n) == Some("midmarker"))
        else {
            warnings.push(format!("Reaction {bigg} has no centre; it was left out."));
            continue;
        };
        let Some(j) = nodes.get(mid).and_then(|n| point(n, "x", "y")) else {
            continue;
        };

        // Each metabolite drawing in this reaction: its node, and the leg's
        // handles by the species and by the junction.
        let mut legs: Vec<(&str, Pt, Pt)> = Vec::new();
        for s in &segments {
            let (Some(from), Some(to)) = ends(s) else {
                continue;
            };
            let (m, other, m_is_from) = match (kind(from), kind(to)) {
                (Some("metabolite"), _) => (from, to, true),
                (_, Some("metabolite")) => (to, from, false),
                _ => continue,
            };
            let (Some(mp), Some(op)) = (
                nodes.get(m).and_then(|n| point(n, "x", "y")),
                nodes.get(other).and_then(|n| point(n, "x", "y")),
            ) else {
                continue;
            };
            let (near_m, near_other) = if m_is_from {
                (handle(s, "b1"), handle(s, "b2"))
            } else {
                (handle(s, "b2"), handle(s, "b1"))
            };
            let outer = near_m.unwrap_or(mp + (op - mp) / 3.0);
            let (outer, inner) = if kind(other) == Some("midmarker") {
                (outer, near_other.unwrap_or(j + (mp - j) / 3.0))
            } else {
                // The multimarker: the leg comes in along the spine.
                let near_k = near_other.unwrap_or(op + (mp - op) / 3.0);
                merge(mp, outer, near_k, op, j)
            };
            legs.push((m, outer, inner));
        }

        let mut rx = Reaction::new(id.clone(), j);
        rx.mode = ReactionMode::Bezier;
        rx.is_reversible = r
            .get("reversibility")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let mut used = vec![false; legs.len()];
        for m in r
            .get("metabolites")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            let (Some(met), Some(coef)) = (
                m.get("bigg_id").and_then(Value::as_str),
                m.get("coefficient").and_then(Value::as_f64),
            ) else {
                continue;
            };
            if coef == 0.0 || !coef.is_finite() {
                continue;
            }
            let found = legs.iter().enumerate().position(|(i, (node, ..))| {
                !used[i]
                    && nodes
                        .get(*node)
                        .and_then(|n| n.get("bigg_id"))
                        .and_then(Value::as_str)
                        == Some(met)
            });
            let Some(i) = found else {
                warnings.push(format!(
                    "{met} in {bigg} isn't drawn; it was left out of it."
                ));
                continue;
            };
            used[i] = true;
            let (node, outer, inner) = legs[i];
            let Some(species) = species_of.get(node) else {
                continue;
            };
            let mut p = Participant::new(species.as_str(), coef.abs());
            p.ctrl_pts_set = true;
            if coef < 0.0 {
                (p.ctrl1, p.ctrl2) = (outer, inner);
                rx.reactants.push(p);
            } else {
                (p.ctrl1, p.ctrl2) = (inner, outer);
                rx.products.push(p);
            }
        }
        if rx.reactants.is_empty() && rx.products.is_empty() {
            warnings.push(format!(
                "Reaction {bigg} has no metabolites drawn; it was left out."
            ));
            continue;
        }
        // An exchange reaction has one side: a null node on the other, across
        // the junction from the species.
        let drawn: Vec<Pt> = rx
            .reactants
            .iter()
            .chain(&rx.products)
            .filter_map(|p| model.find_species(&p.species))
            .map(|s| s.center)
            .collect();
        let centre = drawn.iter().fold(Pt::ZERO, |a, &p| a + p) / drawn.len() as f64;
        let away = j - centre;
        let len = away.x.hypot(away.y);
        let place = (len > 1e-9).then(|| NullPlace {
            center: j + away / len * NULL_OFFSET,
            size: None,
        });
        add_null_nodes(&mut model, &mut rx, place, place);
        if let Some(l) = point(r, "label_x", "label_y") {
            reaction_labels.push((id.clone(), l));
        }
        model.push_reaction(rx).expect("checked unused");
    }

    for t in map
        .get("text_labels")
        .and_then(Value::as_object)
        .into_iter()
        .flat_map(|t| t.values())
    {
        if let (Some(words), Some(at)) = (t.get("text").and_then(Value::as_str), point(t, "x", "y"))
        {
            model.add_text(at, words);
        }
    }

    // Escher's look. The theme makes every curve smooth; Escher's own
    // handles are kept instead.
    let shapes: Vec<(Id, Reaction)> = model
        .reactions()
        .iter()
        .map(|r| (r.id.clone(), r.clone()))
        .collect();
    themes::apply(&mut model, Theme::Escher);
    for (id, old) in shapes {
        if let Some(r) = model.find_reaction_mut(&id) {
            (r.reactants, r.products, r.mode, r.junction_smooth) =
                (old.reactants, old.products, old.mode, false);
        }
    }
    // Labels where Escher draws them: its point is the start of the text, on
    // its baseline; ours is the text's centre.
    let centre = |text: &str, size: f64, at: Pt| {
        let m = font_metrics(size);
        pt(
            at.x + text_width(text, size) / 2.0,
            at.y - (m.ascent + m.descent) / 2.0,
        )
    };
    for (id, at) in labels {
        if let Some(s) = model.find_species_mut(&id) {
            let c = centre(s.primary_id(), label_size(s), at);
            s.label_offset = Some(c - s.center);
        }
    }
    for (id, at) in reaction_labels {
        if let Some(r) = model.find_reaction_mut(&id) {
            let c = centre(&r.id, reaction_label_size(r), at);
            r.label_offset = Some(c - r.junction);
        }
    }
    model.recompute_id_counter();
    Ok(Imported { model, warnings })
}

/// One curve from `m` to the junction `j` for Escher's curve `m`, `c1`,
/// `c2`, `k` and its straight stub from `k` to `j`: the handles keep the
/// directions at both ends (towards `c1` from `m`, along the spine from `j`
/// to `k`), with the lengths that bring the curve closest to Escher's path.
/// Returns the handle by `m` and the one by `j`.
fn merge(m: Pt, c1: Pt, c2: Pt, k: Pt, j: Pt) -> (Pt, Pt) {
    const OUTER: [f64; 7] = [0.6, 0.8, 1.0, 1.2, 1.5, 1.8, 2.2];
    const INNER: [f64; 7] = [0.5, 1.0, 1.5, 2.0, 2.5, 3.0, 4.0];
    // Each curve as a polyline of this many pieces: as good a choice as 48
    // on the E. coli core map, and quick enough in the browser.
    const PIECES: u32 = 16;
    let along = |a: Pt, b: Pt, c: Pt, d: Pt| -> Vec<Pt> {
        (0..=PIECES)
            .map(|i| cubic(a, b, c, d, f64::from(i) / f64::from(PIECES)))
            .collect()
    };
    let mut path = along(m, c1, c2, k);
    path.push(j);
    let mut best = (f64::INFINITY, c1, k);
    for a in OUTER {
        for b in INNER {
            let (h1, h2) = (m + (c1 - m) * a, j + (k - j) * b);
            let ours = along(m, h1, h2, j);
            // The farther either path strays from the other, given up on as
            // soon as it is no better than the best so far.
            let mut d = 0.0_f64;
            for (from, to) in [(&ours, &path), (&path, &ours)] {
                for &p in from.iter() {
                    d = d.max(distance_to(p, to));
                    if d >= best.0 {
                        break;
                    }
                }
            }
            if d < best.0 - 1e-9 {
                best = (d, h1, h2);
            }
        }
    }
    (best.1, best.2)
}

fn cubic(a: Pt, b: Pt, c: Pt, d: Pt, t: f64) -> Pt {
    let u = 1.0 - t;
    a * (u * u * u) + b * (3.0 * u * u * t) + c * (3.0 * u * t * t) + d * (t * t * t)
}

/// How far `p` is from the polyline `line`.
fn distance_to(p: Pt, line: &[Pt]) -> f64 {
    line.windows(2)
        .map(|w| crate::geometry::point_to_segment_dist(p, w[0], w[1]))
        .fold(f64::INFINITY, f64::min)
}

/// The members of a JSON object in the order of their keys as numbers
/// (Escher numbers its nodes and reactions), others after.
fn ordered(o: &serde_json::Map<String, Value>) -> Vec<(&str, &Value)> {
    let mut v: Vec<(&str, &Value)> = o.iter().map(|(k, v)| (k.as_str(), v)).collect();
    v.sort_by_key(|(k, _)| (k.parse::<u64>().unwrap_or(u64::MAX), k.to_string()));
    v
}

/// A segment's `from` and `to` node ids.
fn ends(s: &Value) -> (Option<&str>, Option<&str>) {
    let end = |k: &str| s.get(k).and_then(Value::as_str);
    (end("from_node_id"), end("to_node_id"))
}

fn point(v: &Value, x: &str, y: &str) -> Option<Pt> {
    let x = v.get(x)?.as_f64()?;
    let y = v.get(y)?.as_f64()?;
    (x.is_finite() && y.is_finite()).then(|| pt(x, y))
}

/// A segment's handle `b1` or `b2`, if it has one.
fn handle(segment: &Value, key: &str) -> Option<Pt> {
    segment.get(key).and_then(|b| point(b, "x", "y"))
}

/// A BiGG id as an SBML id: as it is when it is one, otherwise with what
/// isn't allowed made `_` and BiGG's own prefix (`M_` for a metabolite, `R_`
/// for a reaction) in front, as BiGG's SBML writes them.
fn sid(bigg: &str, prefix: char) -> Id {
    if crate::model::is_valid_sid(bigg) {
        return bigg.to_owned();
    }
    let clean: String = bigg
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    format!("{prefix}_{clean}")
}

/// The compartment a BiGG metabolite id ends with: `atp_c` is in `c`.
fn compartment_of(bigg: &str) -> Option<String> {
    let (_, suffix) = bigg.rsplit_once('_')?;
    (!suffix.is_empty() && suffix.len() <= 2 && suffix.chars().all(|c| c.is_ascii_alphanumeric()))
        .then(|| sid(suffix, 'C'))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// PGK-like: 3pg_c + atp_c → 13dpg_c + adp_c down a vertical spine
    /// (midmarker at (100,200), multimarkers at (100,170) and (100,230)),
    /// a second drawing of atp_c, and EX_atp_c, an exchange with no
    /// multimarker.
    const MAP: &str = r#"[
      {"map_name": "test map", "schema": "https://escher.github.io/escher/jsonschema/1-0-0#"},
      {"reactions": {
        "1": {"bigg_id": "PGK", "reversibility": true, "label_x": 120, "label_y": 205,
              "metabolites": [{"bigg_id": "3pg_c", "coefficient": -1},
                              {"bigg_id": "atp_c", "coefficient": -1},
                              {"bigg_id": "13dpg_c", "coefficient": 1},
                              {"bigg_id": "adp_c", "coefficient": 2}],
              "segments": {
                "a": {"from_node_id": "11", "to_node_id": "10"},
                "b": {"from_node_id": "10", "to_node_id": "12"},
                "c": {"from_node_id": "20", "to_node_id": "11", "b1": {"x": 100, "y": 120}, "b2": {"x": 100, "y": 150}},
                "d": {"from_node_id": "21", "to_node_id": "11", "b1": {"x": 160, "y": 150}, "b2": {"x": 100, "y": 150}},
                "e": {"from_node_id": "12", "to_node_id": "22", "b1": {"x": 100, "y": 250}, "b2": {"x": 100, "y": 280}},
                "f": {"from_node_id": "12", "to_node_id": "23"}
              }},
        "2": {"bigg_id": "EX_atp_c", "reversibility": false,
              "metabolites": [{"bigg_id": "atp_c", "coefficient": -1},
                              {"bigg_id": "glc_c", "coefficient": -1}],
              "segments": {"g": {"from_node_id": "24", "to_node_id": "30"}}}
      },
      "nodes": {
        "10": {"node_type": "midmarker", "x": 100, "y": 200},
        "11": {"node_type": "multimarker", "x": 100, "y": 170},
        "12": {"node_type": "multimarker", "x": 100, "y": 230},
        "20": {"node_type": "metabolite", "bigg_id": "3pg_c", "x": 100, "y": 100, "label_x": 115, "label_y": 105},
        "21": {"node_type": "metabolite", "bigg_id": "atp_c", "x": 160, "y": 130},
        "22": {"node_type": "metabolite", "bigg_id": "13dpg_c", "x": 100, "y": 300},
        "23": {"node_type": "metabolite", "bigg_id": "adp_c", "x": 160, "y": 270},
        "24": {"node_type": "metabolite", "bigg_id": "atp_c", "x": 400, "y": 100},
        "30": {"node_type": "midmarker", "x": 400, "y": 160}
      },
      "text_labels": {"t": {"text": "Glycolysis", "x": 10, "y": 20}},
      "canvas": {"x": 0, "y": 0, "width": 500, "height": 400}
    }]"#;

    fn map() -> Imported {
        import(MAP).unwrap()
    }

    fn dir(v: Pt) -> Pt {
        v / v.x.hypot(v.y)
    }

    #[test]
    fn metabolites_become_species_and_their_other_drawings_aliases() {
        let m = map().model;
        assert_eq!(m.model_name, "test map");
        let atp = m.find_species("atp_c").unwrap();
        assert_eq!(atp.center, pt(160.0, 130.0), "the first drawing");
        let aliases: Vec<_> = m.aliases_of("atp_c").collect();
        assert_eq!(aliases.len(), 1);
        assert_eq!(aliases[0].center, pt(400.0, 100.0));
        // An id that isn't an SBML id gets BiGG's prefix.
        assert!(m.find_species("M_13dpg_c").is_some());
        assert!(m.find_species("13dpg_c").is_none());
        // Compartments from the suffix, with no box shown.
        assert_eq!(atp.compartment, "c");
        assert!(m.find_compartment("c").unwrap().hidden);
        assert_eq!(m.texts().len(), 1);
    }

    #[test]
    fn a_reaction_keeps_its_centre_sides_and_stoichiometry() {
        let m = map().model;
        let r = m.find_reaction("PGK").unwrap();
        assert_eq!(r.junction, pt(100.0, 200.0));
        assert!(r.is_reversible);
        assert_eq!(r.mode, ReactionMode::Bezier);
        assert!(!r.junction_smooth, "the map's handles, not smooth ones");
        let ids = |l: &[Participant]| l.iter().map(|p| p.species.clone()).collect::<Vec<_>>();
        assert_eq!(ids(&r.reactants), ["M_3pg_c", "atp_c"]);
        assert_eq!(ids(&r.products), ["M_13dpg_c", "adp_c"]);
        assert_eq!(r.products[1].stoichiometry, 2.0);
        assert_eq!(r.reactants[0].stoichiometry, 1.0, "a reactant's -1 is 1");
        assert!(r.kinetic_law.is_empty(), "Escher has no rate laws");
    }

    #[test]
    fn each_leg_leaves_its_species_as_drawn_and_comes_in_along_the_spine() {
        let m = map().model;
        let r = m.find_reaction("PGK").unwrap();
        let j = r.junction;
        let down = pt(0.0, 1.0);
        for p in r.reactants.iter().chain(&r.products) {
            assert!(p.ctrl_pts_set);
        }
        // 3pg_c, a reactant: from (100,100) towards b1 (100,120), into the
        // junction from above, along the spine.
        let pg = &r.reactants[0];
        assert!(dir(pg.ctrl1 - pt(100.0, 100.0)).distance(down) < 1e-9);
        assert!(dir(j - pg.ctrl2).distance(down) < 1e-9);
        // atp_c leaves towards its b1 (160,150), straight down.
        let atp = &r.reactants[1];
        assert!(dir(atp.ctrl1 - pt(160.0, 130.0)).distance(down) < 1e-9);
        // 13dpg_c, a product: out of the junction downwards, and into the
        // species along b2 → (100,300).
        let dpg = &r.products[0];
        assert!(dir(dpg.ctrl1 - j).distance(down) < 1e-9);
        assert!(dir(pt(100.0, 300.0) - dpg.ctrl2).distance(down) < 1e-9);
        // adp_c's segment had no handles: it still leaves the junction along
        // the spine.
        let adp = &r.products[1];
        assert!(dir(adp.ctrl1 - j).distance(down) < 1e-9);
    }

    #[test]
    fn a_merged_curve_follows_the_drawn_path() {
        // A curve that bends, then a long stub: the merged curve stays close
        // to both, closer than with the handles as they come.
        let (m, c1, c2, k, j) = (
            pt(0.0, 0.0),
            pt(0.0, 80.0),
            pt(60.0, 100.0),
            pt(100.0, 100.0),
            pt(220.0, 100.0),
        );
        let (h1, h2) = merge(m, c1, c2, k, j);
        let mut path: Vec<Pt> = (0..=48)
            .map(|i| cubic(m, c1, c2, k, f64::from(i) / 48.0))
            .collect();
        path.push(j);
        let worst = |a: Pt, b: Pt| {
            (0..=48)
                .map(|i| distance_to(cubic(m, a, b, j, f64::from(i) / 48.0), &path))
                .fold(0.0, f64::max)
        };
        let fitted = worst(h1, h2);
        let plain = worst(c1, k);
        assert!(fitted < 8.0, "{fitted}");
        assert!(fitted < plain, "{fitted} vs {plain}");
        // The directions at both ends are kept.
        assert!(dir(h1 - m).distance(dir(c1 - m)) < 1e-9);
        assert!(dir(h2 - j).distance(dir(k - j)) < 1e-9);
    }

    #[test]
    fn an_exchange_has_a_null_node_across_its_centre() {
        let imported = map();
        let m = &imported.model;
        let r = m.find_reaction("EX_atp_c").unwrap();
        assert_eq!(r.reactants.len(), 1, "glc_c isn't drawn");
        let alias = &r.reactants[0].species;
        assert_eq!(
            m.find_species(alias).unwrap().alias_of.as_deref(),
            Some("atp_c")
        );
        let sink = m.find_species(&r.products[0].species).unwrap();
        assert!(sink.is_null_node);
        // Across the junction from the species, NULL_OFFSET away.
        assert!(sink.center.distance(pt(400.0, 160.0 + NULL_OFFSET)) < 1e-9);
        assert_eq!(imported.warnings.len(), 1, "{:?}", imported.warnings);
        assert!(imported.warnings[0].contains("glc_c"));
    }

    #[test]
    fn the_diagram_takes_the_escher_theme_and_the_map_s_labels() {
        let m = map().model;
        assert_eq!(m.theme, Some(Theme::Escher));
        let pg = m.find_species("M_3pg_c").unwrap();
        assert_eq!(pg.style_name.as_deref(), Some(themes::ESCHER_METABOLITE));
        assert_eq!(
            m.find_species("atp_c").unwrap().style_name.as_deref(),
            Some(themes::ESCHER_COFACTOR)
        );
        // Its name starts at the map's label point: the name's left edge at
        // x 115, its baseline at y 105.
        let size = label_size(pg);
        let c = pg.center + pg.label_offset.unwrap();
        assert!((c.x - text_width("M_3pg_c", size) / 2.0 - 115.0).abs() < 1e-9);
        let fm = font_metrics(size);
        assert!((c.y + (fm.ascent + fm.descent) / 2.0 - 105.0).abs() < 1e-9);
        // The reaction's name too.
        let r = m.find_reaction("PGK").unwrap();
        let rc = r.junction + r.label_offset.unwrap();
        let rw = text_width("PGK", reaction_label_size(r));
        assert!((rc.x - rw / 2.0 - 120.0).abs() < 1e-9);
        // Without a label point, the theme placed it.
        assert!(m.find_species("atp_c").unwrap().label_offset.is_some());
    }

    #[test]
    fn what_isnt_a_map_is_refused() {
        assert!(looks_like_escher("  [1, 2]"));
        assert!(!looks_like_escher("{\"version\": 7}"));
        assert!(import("{\"version\": 7}").is_err());
        assert!(import("[1]").is_err());
        assert!(import("[{}, {}]").is_err());
        assert!(import("not json").is_err());
        // A reaction with no centre is left out, with a warning.
        let text = MAP.replace(
            r#""node_type": "midmarker", "x": 400"#,
            r#""node_type": "multimarker", "x": 400"#,
        );
        let imported = import(&text).unwrap();
        assert!(imported.model.find_reaction("EX_atp_c").is_none());
        assert!(imported.warnings.iter().any(|w| w.contains("no centre")));
    }

    #[test]
    fn ids_are_made_sbml_ids() {
        assert_eq!(sid("atp_c", 'M'), "atp_c");
        assert_eq!(sid("13dpg_c", 'M'), "M_13dpg_c");
        assert_eq!(sid("glc-D", 'R'), "R_glc_D");
        assert_eq!(compartment_of("atp_c").as_deref(), Some("c"));
        assert_eq!(compartment_of("glc__D_e").as_deref(), Some("e"));
        assert_eq!(compartment_of("atp"), None);
    }
}
