//! Reading Escher maps (escher.github.io; S §11.7).
//!
//! An Escher map is `[header, map]`. The map's `nodes` are metabolites (each
//! drawing of one is its own node), and each reaction's markers: one
//! `midmarker`, its centre, and `multimarker`s on its spine either side. A
//! reaction's `segments` join them: a curve (handles `b1` near the segment's
//! `from` node and `b2` near its `to` node) from a metabolite to the
//! multimarker on its side (or straight to the midmarker), and the straight
//! spine from one multimarker through the midmarker to the other.
//!
//! Here a metabolite becomes a species, and its other drawings aliases of
//! it. A reaction's junction is its midmarker, its stem (S §5.13) the spine,
//! and each leg Escher's own curve from the species to its side's end of the
//! stem: the drawing is the map's. Labels keep their places, and the diagram
//! takes the Escher theme (S §8.7), each drawing at the size the map gives it
//! (`node_is_primary`: a drawing that isn't primary is small).

use std::collections::HashMap;

use serde_json::Value;

use crate::geometry::{Pt, pt};
use crate::model::{Compartment, Id, Model, Participant, Reaction, ReactionMode, SpeciesNode};
use crate::passes::{NULL_OFFSET, NullPlace, add_null_nodes};
use crate::render::draw::{label_size, name_font, reaction_label_size};
use crate::text::{font_metrics_in, text_width_in};
use crate::themes;

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
    // Escher draws a drawing that isn't primary small.
    let mut primary: Vec<(Id, bool)> = Vec::new();
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
        if let Some(p) = n.get("node_is_primary").and_then(Value::as_bool) {
            primary.push((node_id.clone(), p));
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

        // The reaction's segments as a tree from its midmarker: each node's
        // next node on the way to the centre.
        let mut next: HashMap<&str, &str> = HashMap::new();
        {
            let mut joined: HashMap<&str, Vec<&str>> = HashMap::new();
            for s in &segments {
                if let (Some(a), Some(b)) = ends(s) {
                    joined.entry(a).or_default().push(b);
                    joined.entry(b).or_default().push(a);
                }
            }
            let mut queue = std::collections::VecDeque::from([mid]);
            let mut seen: std::collections::HashSet<&str> = [mid].into();
            while let Some(n) = queue.pop_front() {
                for &o in joined.get(n).into_iter().flatten() {
                    if seen.insert(o) {
                        next.insert(o, n);
                        queue.push_back(o);
                    }
                }
            }
        }
        // The places a metabolite's line passes on its way from the node it
        // joins to the centre (the midmarker itself left out): one, its side's
        // multimarker, in a map drawn in Escher; more where the line is routed
        // (S §5.6.5). Empty when it joins the midmarker.
        let way = |from: &str, at: Pt| -> Vec<Pt> {
            let mut out = Vec::new();
            let mut n = from;
            while n != mid {
                match nodes.get(n).and_then(|v| point(v, "x", "y")) {
                    Some(p) => out.push(p),
                    None => break,
                }
                match next.get(n) {
                    Some(&up) => n = up,
                    // Not joined to the centre: the node it joins is its end.
                    None => return vec![at],
                }
                if out.len() > nodes.len() {
                    break;
                }
            }
            out
        };

        // Each metabolite drawing in this reaction: its node, its way to the
        // centre, and the curve's handles by the metabolite and by the node
        // it joins.
        let mut legs: Vec<(&str, Vec<Pt>, Pt, Pt)> = Vec::new();
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
            // A segment with no handles is straight: handles a third of the
            // way along keep it so.
            let outer = near_m.unwrap_or(mp + (op - mp) / 3.0);
            let inner = near_other.unwrap_or(op + (mp - op) / 3.0);
            legs.push((m, way(other, op), outer, inner));
        }

        let mut rx = Reaction::new(id.clone(), j);
        rx.mode = ReactionMode::Bezier;
        rx.is_reversible = r
            .get("reversibility")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let mut used = vec![false; legs.len()];
        let (mut reactant_end, mut product_end) = (None, None);
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
            let (node, ref way, outer, inner) = legs[i];
            let Some(species) = species_of.get(node) else {
                continue;
            };
            let mut p = Participant::new(species.as_str(), coef.abs());
            p.ctrl_pts_set = true;
            // Its side's end of the stem is where the side's first line
            // meets the centre's marker.
            let joins = way.last().copied().unwrap_or(j);
            let side_end = if coef < 0.0 {
                *reactant_end.get_or_insert(joins)
            } else {
                *product_end.get_or_insert(joins)
            };
            // The rest of its way are its corner points, in order from the
            // metabolite; a line that reaches the centre by another marker
            // keeps its own, then runs straight to the side's end.
            p.points = match way.split_last() {
                Some((&last, rest)) if last == side_end => rest.to_vec(),
                _ => way.clone(),
            };
            if coef < 0.0 {
                (p.ctrl1, p.ctrl2) = (outer, inner);
                rx.reactants.push(p);
            } else {
                (p.ctrl1, p.ctrl2) = (inner, outer);
                rx.products.push(p);
            }
        }
        // The spine: each side's metabolites join one node (its multimarker,
        // or the midmarker itself); a stem only if one isn't the midmarker.
        let (r_end, p_end) = (reactant_end.unwrap_or(j), product_end.unwrap_or(j));
        if r_end != j || p_end != j {
            let stem = (r_end - j, p_end - j);
            rx.stem = Some(stem);
            rx.stem_straight = crate::model::stem_is_straight(stem);
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

    // Escher's look. The theme makes every curve smooth; the map's own
    // handles and stems are kept instead.
    let shapes: Vec<(Id, Reaction)> = model
        .reactions()
        .iter()
        .map(|r| (r.id.clone(), r.clone()))
        .collect();
    // At Escher's own sizes, so the map looks as it does in Escher (S §11.7).
    themes::apply_escher(&mut model, themes::EscherSizes::MAP);
    for (id, old) in shapes {
        if let Some(r) = model.find_reaction_mut(&id) {
            (
                r.reactants,
                r.products,
                r.mode,
                r.junction_smooth,
                r.stem,
                r.stem_straight,
            ) = (
                old.reactants,
                old.products,
                old.mode,
                false,
                old.stem,
                old.stem_straight,
            );
        }
    }
    // Each drawing at the size the map gives it; a drawing without the flag
    // keeps the theme's, which goes by its name.
    for (id, p) in primary {
        themes::give_escher_size(&mut model, &id, !p);
    }
    // Labels where Escher draws them: its point is the start of the text, on
    // its baseline; ours is the text's centre, measured in the face the
    // theme draws it in (bold italic, S §5.11.1).
    let centre = |text: &str, size: f64, font, at: Pt| {
        let m = font_metrics_in(font, size);
        pt(
            at.x + text_width_in(font, text, size) / 2.0,
            at.y - (m.ascent + m.descent) / 2.0,
        )
    };
    for (id, at) in labels {
        if let Some(s) = model.find_species_mut(&id) {
            let c = centre(s.primary_id(), label_size(s), name_font(&s.style), at);
            s.label_offset = Some(c - s.center);
        }
    }
    for (id, at) in reaction_labels {
        if let Some(r) = model.find_reaction_mut(&id) {
            let c = centre(&r.id, reaction_label_size(r), name_font(&r.style), at);
            r.label_offset = Some(c - r.junction);
        }
    }
    model.recompute_id_counter();
    Ok(Imported { model, warnings })
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
    use crate::model::Role;
    use crate::themes::{EscherSizes, Theme};

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
    fn the_spine_is_the_stem_and_each_leg_the_map_s_own_curve() {
        let m = map().model;
        let r = m.find_reaction("PGK").unwrap();
        // The multimarkers, from the midmarker at (100,200).
        assert_eq!(r.stem, Some((pt(0.0, -30.0), pt(0.0, 30.0))));
        assert_eq!(r.leg_anchor(Role::Reactant), pt(100.0, 170.0));
        assert_eq!(r.leg_anchor(Role::Product), pt(100.0, 230.0));
        for p in r.reactants.iter().chain(&r.products) {
            assert!(p.ctrl_pts_set);
        }
        // 3pg_c → its multimarker: b1 by the metabolite, b2 by the marker.
        let pg = &r.reactants[0];
        assert_eq!((pg.ctrl1, pg.ctrl2), (pt(100.0, 120.0), pt(100.0, 150.0)));
        let curve = crate::bezier::leg_curve(&m, r, Role::Reactant, 0).unwrap();
        assert_eq!((curve.p0, curve.p3), (pt(100.0, 100.0), pt(100.0, 170.0)));
        // A product, from the marker: the segment runs marker → 13dpg_c, so
        // b1 is by the marker.
        let dpg = &r.products[0];
        assert_eq!((dpg.ctrl1, dpg.ctrl2), (pt(100.0, 250.0), pt(100.0, 280.0)));
        // adp_c's segment has no handles: straight, a third of the way along.
        let adp = &r.products[1];
        let (k, a) = (pt(100.0, 230.0), pt(160.0, 270.0));
        assert!(adp.ctrl1.distance(k + (a - k) / 3.0) < 1e-9);
        assert!(adp.ctrl2.distance(a + (k - a) / 3.0) < 1e-9);
        // An exchange joins the midmarker itself: no stem.
        assert_eq!(m.find_reaction("EX_atp_c").unwrap().stem, None);
    }

    /// A routed reaction, as MetaCarto draws them: centre (100,200), the
    /// reactants' marker (100,170) up a chain through (100,100) to (0,100),
    /// where a_c's line starts; b_c joins the chain at (100,100); d_c reaches
    /// the centre by another marker, (70,200). The product c_c comes down
    /// from (100,230) through (100,300).
    const ROUTED: &str = r#"[
      {"map_name": "routed"},
      {"reactions": {
        "1": {"bigg_id": "R", "reversibility": false,
              "metabolites": [{"bigg_id": "a_c", "coefficient": -1},
                              {"bigg_id": "b_c", "coefficient": -1},
                              {"bigg_id": "d_c", "coefficient": -1},
                              {"bigg_id": "c_c", "coefficient": 1}],
              "segments": {
                "a": {"from_node_id": "11", "to_node_id": "10"},
                "b": {"from_node_id": "10", "to_node_id": "15"},
                "c": {"from_node_id": "12", "to_node_id": "11"},
                "d": {"from_node_id": "13", "to_node_id": "12"},
                "e": {"from_node_id": "20", "to_node_id": "13", "b1": {"x": 0, "y": 30}, "b2": {"x": 0, "y": 70}},
                "f": {"from_node_id": "21", "to_node_id": "12"},
                "g": {"from_node_id": "15", "to_node_id": "16"},
                "h": {"from_node_id": "16", "to_node_id": "22"},
                "i": {"from_node_id": "23", "to_node_id": "17"},
                "j": {"from_node_id": "17", "to_node_id": "10"}
              }}
      },
      "nodes": {
        "10": {"node_type": "midmarker", "x": 100, "y": 200},
        "11": {"node_type": "multimarker", "x": 100, "y": 170},
        "12": {"node_type": "multimarker", "x": 100, "y": 100},
        "13": {"node_type": "multimarker", "x": 0, "y": 100},
        "15": {"node_type": "multimarker", "x": 100, "y": 230},
        "16": {"node_type": "multimarker", "x": 100, "y": 300},
        "17": {"node_type": "multimarker", "x": 70, "y": 200},
        "20": {"node_type": "metabolite", "bigg_id": "a_c", "x": 0, "y": 0},
        "21": {"node_type": "metabolite", "bigg_id": "b_c", "x": 200, "y": 100},
        "22": {"node_type": "metabolite", "bigg_id": "c_c", "x": 200, "y": 300},
        "23": {"node_type": "metabolite", "bigg_id": "d_c", "x": -100, "y": 200}
      }
    }]"#;

    #[test]
    fn a_routed_reaction_s_lines_keep_their_routes() {
        let m = import(ROUTED).unwrap().model;
        let r = m.find_reaction("R").unwrap();
        // The stem runs between the markers next to the centre.
        assert_eq!(r.stem, Some((pt(0.0, -30.0), pt(0.0, 30.0))));
        let points = |list: &[Participant], i: usize| list[i].points.clone();
        // a_c: up its chain, in order from a_c, to the reactants' end.
        assert_eq!(points(&r.reactants, 0), [pt(0.0, 100.0), pt(100.0, 100.0)]);
        // b_c joins the chain: the shared run is its own too.
        assert_eq!(points(&r.reactants, 1), [pt(100.0, 100.0)]);
        // d_c keeps its own marker, then runs straight to the reactants' end.
        assert_eq!(points(&r.reactants, 2), [pt(70.0, 200.0)]);
        // c_c, the product: from c_c up to the products' end.
        assert_eq!(points(&r.products, 0), [pt(100.0, 300.0)]);
        // a_c's handles are its segment's, the inner one by its first corner.
        let a = &r.reactants[0];
        assert_eq!((a.ctrl1, a.ctrl2), (pt(0.0, 30.0), pt(0.0, 70.0)));
        let c = crate::bezier::leg_curve(&m, r, Role::Reactant, 0).unwrap();
        assert_eq!((c.p0, c.p3), (pt(0.0, 0.0), pt(0.0, 100.0)));
        // The routes survive SBML.
        let opts = crate::antimony::ImportOptions {
            convert_catalytic: false,
            seed: 1,
        };
        let back = crate::sbml::import(&crate::sbml::export(&m).xml, &opts)
            .unwrap()
            .model;
        let b = back.find_reaction("R").unwrap();
        for (p, q) in r
            .reactants
            .iter()
            .chain(&r.products)
            .zip(b.reactants.iter().chain(&b.products))
        {
            assert_eq!(p.points, q.points, "{}", p.species);
        }
        // A map drawn in Escher has none (the other tests' map).
        let plain = map().model;
        assert!(
            plain
                .reactions()
                .iter()
                .flat_map(|r| r.reactants.iter().chain(&r.products))
                .all(|p| p.points.is_empty())
        );
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
    fn a_map_is_drawn_at_escher_s_own_sizes() {
        use crate::render::draw::reaction_label_size;
        let m = map().model;
        let big = EscherSizes::MAP;
        let pg = m.find_species("M_3pg_c").unwrap();
        assert_eq!(
            (pg.width, label_size(pg)),
            (big.metabolite, big.metabolite_font)
        );
        let atp = m.find_species("atp_c").unwrap();
        assert_eq!(
            (atp.width, label_size(atp)),
            (big.cofactor, big.cofactor_font)
        );
        let r = &m.reactions()[0];
        assert_eq!(
            (r.style.line_width, reaction_label_size(r)),
            (big.line_width, big.reaction_font)
        );
        // The menu's Escher theme stays lighter.
        let mut light = m.clone();
        themes::apply(&mut light, Theme::Escher);
        let small = EscherSizes::DIAGRAM;
        assert_eq!(
            light.find_species("M_3pg_c").unwrap().width,
            small.metabolite
        );
        assert_eq!(light.reactions()[0].style.line_width, small.line_width);
        // Something added to the map takes the map's sizes.
        let before = m.clone();
        let mut more = m.clone();
        more.push_species(crate::model::SpeciesNode::new("glx_c", pt(0.0, 0.0)))
            .unwrap();
        themes::style_new_items(&before, &mut more);
        assert_eq!(more.find_species("glx_c").unwrap().width, big.metabolite);
    }

    #[test]
    fn each_drawing_is_the_size_the_map_gives_it() {
        // 3pg_c not primary (small, though not a cofactor); adp_c primary
        // (big, though a cofactor); atp_c small in one drawing, big in its
        // other; 13dpg_c without the flag, so by its name.
        let flagged = MAP
            .replace(
                r#""bigg_id": "3pg_c", "x""#,
                r#""bigg_id": "3pg_c", "node_is_primary": false, "x""#,
            )
            .replace(
                r#""bigg_id": "adp_c", "x""#,
                r#""bigg_id": "adp_c", "node_is_primary": true, "x""#,
            )
            .replace(
                r#""bigg_id": "atp_c", "x": 160"#,
                r#""bigg_id": "atp_c", "node_is_primary": false, "x": 160"#,
            )
            .replace(
                r#""bigg_id": "atp_c", "x": 400"#,
                r#""bigg_id": "atp_c", "node_is_primary": true, "x": 400"#,
            );
        assert_eq!(flagged.matches("node_is_primary").count(), 4);
        // Opened as the app opens it: through the view, which gives items an
        // edit adds the theme's look.
        let mut v = crate::view::DiagramView::new(Model::new());
        v.import_escher(&flagged).unwrap();
        let m = v.model().clone();
        let big = EscherSizes::MAP;
        let size = |id: &str| {
            let s = m.find_species(id).unwrap();
            (s.width, label_size(s), s.style_name.clone().unwrap())
        };
        let small = (
            big.cofactor,
            big.cofactor_font,
            themes::ESCHER_COFACTOR.to_owned(),
        );
        let large = (
            big.metabolite,
            big.metabolite_font,
            themes::ESCHER_METABOLITE.to_owned(),
        );
        assert_eq!(size("M_3pg_c"), small);
        assert_eq!(size("adp_c"), large);
        assert_eq!(size("atp_c"), small);
        let alias = m.aliases_of("atp_c").next().unwrap().id.clone();
        assert_eq!(size(&alias), large);
        assert_eq!(size("M_13dpg_c"), large);
        // The 3pg_c name starts at the map's label point at its small size.
        let pg = m.find_species("M_3pg_c").unwrap();
        let c = pg.center + pg.label_offset.unwrap();
        let w = text_width_in(name_font(&pg.style), "M_3pg_c", label_size(pg));
        assert!((c.x - w / 2.0 - 115.0).abs() < 1e-9);

        // The sizes travel through SBML.
        let opts = crate::antimony::ImportOptions {
            convert_catalytic: false,
            seed: 1,
        };
        let back = crate::sbml::import(&crate::sbml::export(&m).xml, &opts)
            .unwrap()
            .model;
        for id in ["M_3pg_c", "adp_c", "atp_c", alias.as_str(), "M_13dpg_c"] {
            let (a, b) = (m.find_species(id).unwrap(), back.find_species(id).unwrap());
            assert_eq!((a.width, label_size(a)), (b.width, label_size(b)), "{id}");
        }
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
        // x 115, its baseline at y 105, measured in its bold italic face.
        let (size, font) = (label_size(pg), name_font(&pg.style));
        assert!(font.bold && font.italic);
        let c = pg.center + pg.label_offset.unwrap();
        assert!((c.x - text_width_in(font, "M_3pg_c", size) / 2.0 - 115.0).abs() < 1e-9);
        let fm = font_metrics_in(font, size);
        assert!((c.y + (fm.ascent + fm.descent) / 2.0 - 105.0).abs() < 1e-9);
        // The reaction's name too.
        let r = m.find_reaction("PGK").unwrap();
        let rc = r.junction + r.label_offset.unwrap();
        let rw = text_width_in(name_font(&r.style), "PGK", reaction_label_size(r));
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
