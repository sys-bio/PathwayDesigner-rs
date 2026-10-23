//! What survives an SBML round trip, for every model in `Gallery/index.json`
//! (the paper's evaluation E4). Each model is exported as SBML with Layout and
//! imported again, twice: once as written, and once with PathwayDesigner's own
//! notes (`<pd:…/>`) taken out, which is what a tool that reads only standard
//! SBML Layout sees. Both results are compared with the original, field by
//! field, through the native JSON form, so no field can be missed.
//!
//! Leg curves are compared as drawn, not as stored; state that isn't drawn
//! (handles of a reaction that isn't Bézier) and data that isn't the model's
//! (null nodes' and aliases' copies of biochemistry) are left out, and made-up
//! ids (null nodes, free text) are matched by what they belong to.
//!
//! With `--antimony-layout`, the model is exported as Antimony with its
//! drawing as layout statements (docs/antimony-layout-export-design.md) and
//! read back, beside the full SBML round trip for comparison. An alias is
//! matched by its species and the reactions drawn to it, which is all
//! Antimony names it by.
//!
//! With `--antimony` (evaluation E5), the model alone is compared instead,
//! after a trip through Antimony, and, for comparison, through SBML without its
//! layout: species (by their primary ids), compartments, reactions with their
//! species, modifiers and kinds, rate laws and reversibility, parameters and
//! rules. Neither carries a drawing.
//!
//! Prints a Markdown report: per model, then per field over all models. With
//! `DETAIL=<part of a model's title>`, also lists that model's every change on
//! stderr: in the full round trip, or with `DETAIL_STANDARD=1` in the one
//! through standard Layout and Render only.
//!
//! ```text
//! cargo run -p pd-core --release --example layout_report [-- --antimony | --antimony-layout] > report.md
//! ```

use std::collections::BTreeMap;
use std::path::Path;

use pd_core::antimony::ImportOptions;
use serde_json::Value;

const OPTS: ImportOptions = ImportOptions {
    convert_catalytic: false,
    seed: 1,
};

/// Numbers closer than this are the same (positions come back through text).
const TOLERANCE: f64 = 1e-6;
/// With `--antimony-layout`: numbers are written with two decimals.
const ROUNDED_TOLERANCE: f64 = 0.01;
static ROUNDED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Whether DETAIL lists the round trip without PathwayDesigner's notes.
fn detail_standard() -> bool {
    std::env::var("DETAIL_STANDARD").is_ok()
}

static DETAIL: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

thread_local! {
    /// The concrete path being compared, with list keys, for DETAIL.
    static AT: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };
}

/// Per field: how many times it occurs, and how many of those come back
/// different after the full round trip and after the standard-only one.
#[derive(Default, Clone, Copy)]
struct Tally {
    total: usize,
    lost: usize,
    lost_standard: usize,
}

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let index: Value =
        serde_json::from_str(&std::fs::read_to_string(root.join("Gallery/index.json")).unwrap())
            .unwrap();
    let antimony = std::env::args().any(|a| a == "--antimony");
    let antimony_layout = std::env::args().any(|a| a == "--antimony-layout");
    ROUNDED.store(antimony_layout, std::sync::atomic::Ordering::Relaxed);
    let (first, second) = if antimony_layout {
        ("Antimony with layout", "SBML")
    } else if antimony {
        ("Antimony", "SBML without layout")
    } else {
        ("full", "standard Layout only")
    };
    let mut by_field: BTreeMap<String, Tally> = BTreeMap::new();
    println!("| Model | Fields | Changed ({first}) | Changed ({second}) | Import warnings |");
    println!("|---|---|---|---|---|");
    for e in index["examples"].as_array().unwrap() {
        let file = e["file"].as_str().unwrap();
        let text = std::fs::read_to_string(root.join(file)).unwrap();
        let mut model = if pd_core::escher::looks_like_escher(&text) {
            pd_core::escher::import(&text).unwrap().model
        } else {
            pd_core::json::from_json(&text).unwrap().model
        };
        // As the app opens it: a compartment without a box gets one (S §7.9).
        pd_core::compartments::place_unplaced(&mut model);
        let title = e["title"].as_str().unwrap_or(file);
        DETAIL.store(
            std::env::var("DETAIL").is_ok_and(|d| title.contains(&d)),
            std::sync::atomic::Ordering::Relaxed,
        );
        let mut fields: BTreeMap<String, Tally> = BTreeMap::new();
        let xml = pd_core::sbml::export(&model).xml;
        let warnings: Vec<String> = if antimony_layout {
            let named = |m: &pd_core::model::Model| {
                let mut v = as_value(m);
                name_aliases(&mut v);
                v
            };
            let original = named(&model);
            let text = pd_core::antimony::export_with_layout(&model);
            let back = pd_core::antimony::import(&text, &OPTS).unwrap();
            let full = pd_core::sbml::import(&xml, &OPTS).unwrap();
            compare(&original, &named(&back.model), "", &mut fields, false);
            compare(&original, &named(&full.model), "", &mut fields, true);
            back.warnings.iter().map(|d| d.to_string()).collect()
        } else if antimony {
            let original = biochemistry(&model);
            let text = pd_core::antimony::export(&model);
            let back = pd_core::antimony::import(&text, &OPTS).unwrap();
            let core = pd_core::sbml::import(&without_layout(&xml), &OPTS).unwrap();
            compare(
                &original,
                &biochemistry(&back.model),
                "",
                &mut fields,
                false,
            );
            compare(&original, &biochemistry(&core.model), "", &mut fields, true);
            back.warnings.iter().map(|d| d.to_string()).collect()
        } else {
            let original = as_value(&model);
            let full = pd_core::sbml::import(&xml, &OPTS).unwrap();
            let standard = pd_core::sbml::import(&without_notes(&xml), &OPTS).unwrap();
            compare(&original, &as_value(&full.model), "", &mut fields, false);
            compare(&original, &as_value(&standard.model), "", &mut fields, true);
            full.warnings
        };
        let sum = fields.values().fold(Tally::default(), |a, t| Tally {
            total: a.total + t.total,
            lost: a.lost + t.lost,
            lost_standard: a.lost_standard + t.lost_standard,
        });
        let warnings = if warnings.is_empty() {
            "none".to_owned()
        } else {
            warnings.join("; ")
        };
        println!(
            "| {} | {} | {} | {} | {warnings} |",
            e["title"].as_str().unwrap_or(file),
            sum.total,
            sum.lost,
            sum.lost_standard
        );
        for (k, t) in fields {
            let f = by_field.entry(k).or_default();
            f.total += t.total;
            f.lost += t.lost;
            f.lost_standard += t.lost_standard;
        }
    }
    println!();
    println!("| Field | Occurrences | Changed ({first}) | Changed ({second}) |");
    println!("|---|---|---|---|");
    for (k, t) in &by_field {
        if t.lost > 0 || t.lost_standard > 0 {
            println!("| `{k}` | {} | {} | {} |", t.total, t.lost, t.lost_standard);
        }
    }
    println!();
    let kept: Vec<_> = by_field
        .iter()
        .filter(|(_, t)| t.lost == 0 && t.lost_standard == 0)
        .map(|(k, t)| format!("`{k}` ({})", t.total))
        .collect();
    println!("Unchanged in both: {}", kept.join(", "));
}

/// The model in its native JSON form, without the file's version number
/// (it says which features are used, not what the model is).
fn as_value(model: &pd_core::model::Model) -> Value {
    let mut v: Value = serde_json::from_str(&pd_core::json::to_json(model)).unwrap();
    v.as_object_mut().unwrap().remove("version");
    drawn_curves(model, &mut v);
    name_null_nodes(&mut v);
    // Free text has made-up ids too; it is matched by its place in the list.
    if let Some(texts) = v.get_mut("texts").and_then(Value::as_array_mut) {
        for (i, t) in texts.iter_mut().enumerate() {
            t["id"] = Value::String(format!("text {i}"));
        }
    }
    v
}

/// Each leg's stored handles replaced by the curve actually drawn, for a
/// Bézier reaction; for any other reaction the handles and the smooth-junction
/// flag are not drawn at all, so they are left out of the comparison.
fn drawn_curves(model: &pd_core::model::Model, v: &mut Value) {
    use pd_core::model::{ReactionMode, Role};
    for r in v["reactions"].as_array_mut().unwrap() {
        let id = r["id"].as_str().unwrap().to_owned();
        let reaction = model.find_reaction(&id).unwrap();
        let bezier = pd_core::render::draw::effective_mode(reaction) == ReactionMode::Bezier;
        if !bezier {
            r.as_object_mut().unwrap().remove("isJunctionSmooth");
        }
        for (side, role) in [("reactants", Role::Reactant), ("products", Role::Product)] {
            let Some(list) = r.get_mut(side).and_then(Value::as_array_mut) else {
                continue;
            };
            for (i, p) in list.iter_mut().enumerate() {
                let p = p.as_object_mut().unwrap();
                for k in ["ctrl1x", "ctrl1y", "ctrl2x", "ctrl2y", "ctrlSet"] {
                    p.remove(k);
                }
                if bezier && let Some(c) = pd_core::bezier::leg_curve(model, reaction, role, i) {
                    let xs = [c.p0, c.p1, c.p2, c.p3]
                        .iter()
                        .flat_map(|q| [q.x, q.y])
                        .collect();
                    p.insert("curve".into(), xs_value(xs));
                }
            }
        }
    }
    // Null nodes are not SBML species, and an alias has no biochemistry of its
    // own (its species has): their biochemical fields mean nothing. That
    // includes an alias's compartment: a box's members are primary species, and
    // a dragged box carries whatever sits inside it (S §7.9).
    for s in v["species"].as_array_mut().unwrap() {
        let null = s["isNullNode"].as_bool() == Some(true);
        let alias = s.get("aliasOf").is_some();
        let s = s.as_object_mut().unwrap();
        if null || alias {
            for k in ["isBoundary", "isConstant", "initialValue", "compartment"] {
                s.remove(k);
            }
        }
    }
}

fn xs_value(xs: Vec<f64>) -> Value {
    Value::Array(xs.into_iter().map(|x| serde_json::json!(x)).collect())
}

/// Null nodes get made-up ids, which import makes up afresh, so each is
/// renamed after the reaction and side it belongs to (`J1 source 0`).
fn name_null_nodes(v: &mut Value) {
    let nulls: std::collections::HashSet<String> = v["species"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|s| s["isNullNode"].as_bool() == Some(true))
        .map(|s| s["id"].as_str().unwrap().to_owned())
        .collect();
    let mut names = std::collections::HashMap::new();
    for r in v["reactions"].as_array_mut().unwrap() {
        let id = r["id"].as_str().unwrap().to_owned();
        for (side, word) in [("reactants", "source"), ("products", "sink")] {
            if let Some(list) = r.get_mut(side).and_then(Value::as_array_mut) {
                for (i, p) in list.iter_mut().enumerate() {
                    let old = p["id"].as_str().unwrap().to_owned();
                    if nulls.contains(&old) {
                        let new = format!("{id} {word} {i}");
                        p["id"] = Value::String(new.clone());
                        names.insert(old, new);
                    }
                }
            }
        }
    }
    for s in v["species"].as_array_mut().unwrap() {
        if let Some(new) = names.get(s["id"].as_str().unwrap()) {
            s["id"] = Value::String(new.clone());
        }
    }
}

/// Aliases get made-up ids on reading Antimony, which names an alias only by
/// its species and the reactions drawn to it, so each is renamed so
/// (`S1 alias for J2 J3`).
fn name_aliases(v: &mut Value) {
    let aliases: Vec<(String, String)> = v["species"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|s| {
            let of = s.get("aliasOf")?.as_str()?.to_owned();
            Some((s["id"].as_str()?.to_owned(), of))
        })
        .collect();
    let mut names = std::collections::HashMap::new();
    for (id, of) in &aliases {
        let reactions: Vec<String> = v["reactions"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|r| {
                ["reactants", "products", "modifiers"].iter().any(|side| {
                    r.get(*side)
                        .and_then(Value::as_array)
                        .is_some_and(|l| l.iter().any(|p| p["id"].as_str() == Some(id)))
                })
            })
            .map(|r| r["id"].as_str().unwrap().to_owned())
            .collect();
        names.insert(
            id.clone(),
            format!("{of} alias for {}", reactions.join(" ")),
        );
    }
    for s in v["species"].as_array_mut().unwrap() {
        if let Some(new) = names.get(s["id"].as_str().unwrap()) {
            s["id"] = Value::String(new.clone());
        }
    }
    for r in v["reactions"].as_array_mut().unwrap() {
        for side in ["reactants", "products", "modifiers"] {
            if let Some(list) = r.get_mut(side).and_then(Value::as_array_mut) {
                for p in list {
                    if let Some(new) = p["id"].as_str().and_then(|id| names.get(id)) {
                        p["id"] = Value::String(new.clone());
                    }
                }
            }
        }
    }
}

/// The model alone, without its drawing: what Antimony, or SBML without a
/// layout, can carry. Aliases are their species, and null nodes are left out.
fn biochemistry(model: &pd_core::model::Model) -> Value {
    use serde_json::json;
    let primary = |id: &str| {
        model
            .find_species(id)
            .map_or(id.to_owned(), |s| s.primary_id().to_owned())
    };
    let is_null = |id: &str| model.find_species(id).is_some_and(|s| s.is_null_node);
    let side = |list: &[pd_core::model::Participant]| -> Vec<Value> {
        list.iter()
            .filter(|p| !is_null(&p.species))
            .map(|p| json!({"id": primary(&p.species), "stoichiometry": p.stoichiometry}))
            .collect()
    };
    let species: Vec<Value> = model
        .species()
        .iter()
        .filter(|s| !s.is_alias() && !s.is_null_node)
        .map(|s| {
            json!({"id": s.id, "initialValue": s.initial_value, "isBoundary": s.is_boundary,
                   "isConstant": s.is_constant, "compartment": s.compartment})
        })
        .collect();
    let reactions: Vec<Value> = model
        .reactions()
        .iter()
        .map(|r| {
            let mut seen = std::collections::HashSet::new();
            let modifiers: Vec<Value> = r
                .modifiers
                .iter()
                .filter(|m| seen.insert(primary(&m.species)))
                .map(|m| json!({"id": primary(&m.species), "kind": format!("{:?}", m.kind)}))
                .collect();
            json!({"id": r.id, "isReversible": r.is_reversible, "kineticLaw": r.kinetic_law,
                   "reactants": side(&r.reactants), "products": side(&r.products),
                   "modifiers": modifiers})
        })
        .collect();
    let compartments: Vec<Value> = model
        .compartments
        .iter()
        .map(|c| json!({"id": c.id, "size": c.size, "dimensions": c.dimensions}))
        .collect();
    let parameters: Vec<Value> = model
        .parameters
        .iter()
        .map(|p| json!({"variable": p.variable, "expression": p.expression}))
        .collect();
    let rules: Vec<Value> = model
        .assignment_rules
        .iter()
        .map(|r| json!({"variable": r.variable, "expression": r.expression}))
        .collect();
    json!({"modelName": model.model_name, "compartments": compartments, "species": species,
           "reactions": reactions, "parameters": parameters, "assignmentRules": rules})
}

/// The SBML with its layout taken out: the model alone.
fn without_layout(xml: &str) -> String {
    match (
        xml.find("<layout:listOfLayouts"),
        xml.find("</layout:listOfLayouts>"),
    ) {
        (Some(start), Some(end)) => {
            format!(
                "{}{}",
                &xml[..start],
                &xml[end + "</layout:listOfLayouts>".len()..]
            )
        }
        _ => xml.to_owned(),
    }
}

/// The SBML with every PathwayDesigner note (`<pd:… />`, always one empty
/// element) taken out.
fn without_notes(xml: &str) -> String {
    let mut out = String::with_capacity(xml.len());
    let mut rest = xml;
    while let Some(start) = rest.find("<pd:") {
        out.push_str(&rest[..start]);
        let end = rest[start..]
            .find("/>")
            .expect("a pd note is an empty element");
        rest = &rest[start + end + 2..];
    }
    out.push_str(rest);
    out
}

/// What a list's items are matched by: an `id`, a `variable`, a `name`, or
/// failing those their position.
fn key_of(item: &Value, i: usize) -> String {
    for k in ["id", "variable", "name"] {
        if let Some(s) = item.get(k).and_then(Value::as_str) {
            return s.to_owned();
        }
    }
    format!("#{i}")
}

/// Counts every field of `a` and `b` under its path, with list positions left
/// out (`species[].style.fillColor`), and whether it differs.
fn compare(a: &Value, b: &Value, path: &str, out: &mut BTreeMap<String, Tally>, standard: bool) {
    if standard == detail_standard() && DETAIL.load(std::sync::atomic::Ordering::Relaxed) {
        match (a, b) {
            (Value::Object(_), Value::Object(_)) | (Value::Array(_), Value::Array(_)) => {}
            _ if !same_leaf(a, b, path) => eprintln!(
                "{path}: {a} -> {b}   @{}",
                AT.with(|t| t.borrow().join("/"))
            ),
            _ => {}
        }
    }
    let count = |out: &mut BTreeMap<String, Tally>, path: &str, same: bool| {
        let t = out.entry(path.to_owned()).or_default();
        if !standard {
            t.total += 1;
        }
        if !same {
            if standard {
                t.lost_standard += 1;
            } else {
                t.lost += 1;
            }
        }
    };
    match (a, b) {
        (Value::Object(x), Value::Object(y)) => {
            let mut keys: Vec<&String> = x.keys().chain(y.keys()).collect();
            keys.sort();
            keys.dedup();
            for k in keys {
                let p = if path.is_empty() {
                    k.clone()
                } else {
                    format!("{path}.{k}")
                };
                match (x.get(k), y.get(k)) {
                    (Some(u), Some(v)) => compare(u, v, &p, out, standard),
                    (Some(u), None) | (None, Some(u)) => {
                        if standard == detail_standard()
                            && DETAIL.load(std::sync::atomic::Ordering::Relaxed)
                        {
                            eprintln!(
                                "{p}: only on one side: {u}   @{}",
                                AT.with(|t| t.borrow().join("/"))
                            );
                        }
                        // Missing on one side: every field below it changed.
                        let mut leaves = Vec::new();
                        leaf_paths(u, &p, &mut leaves);
                        for l in leaves {
                            count(out, &l, false);
                        }
                    }
                    (None, None) => unreachable!(),
                }
            }
        }
        (Value::Array(x), Value::Array(y)) if x.iter().chain(y).all(|v| !v.is_object()) => {
            // A list of numbers (corner points): one field.
            let same = x.len() == y.len() && x.iter().zip(y).all(|(u, v)| same_leaf(u, v, path));
            if !same
                && standard == detail_standard()
                && DETAIL.load(std::sync::atomic::Ordering::Relaxed)
            {
                eprintln!(
                    "{path}: {a} -> {b}   @{}",
                    AT.with(|t| t.borrow().join("/"))
                );
            }
            count(out, path, same);
        }
        (Value::Array(x), Value::Array(y)) => {
            let p = format!("{path}[]");
            let ys: BTreeMap<String, &Value> = y
                .iter()
                .enumerate()
                .map(|(i, v)| (key_of(v, i), v))
                .collect();
            let mut seen = std::collections::HashSet::new();
            for (i, u) in x.iter().enumerate() {
                let k = key_of(u, i);
                seen.insert(k.clone());
                match ys.get(&k) {
                    Some(v) => {
                        AT.with(|t| t.borrow_mut().push(k.clone()));
                        compare(u, v, &p, out, standard);
                        AT.with(|t| t.borrow_mut().pop());
                    }
                    None => {
                        if standard == detail_standard()
                            && DETAIL.load(std::sync::atomic::Ordering::Relaxed)
                        {
                            eprintln!("{p}: {k} missing after");
                        }
                        let mut leaves = Vec::new();
                        leaf_paths(u, &p, &mut leaves);
                        for l in leaves {
                            count(out, &l, false);
                        }
                    }
                }
            }
            for (k, v) in &ys {
                if !seen.contains(k) {
                    if standard == detail_standard()
                        && DETAIL.load(std::sync::atomic::Ordering::Relaxed)
                    {
                        eprintln!("{p}: {k} new after");
                    }
                    let mut leaves = Vec::new();
                    leaf_paths(v, &p, &mut leaves);
                    for l in leaves {
                        count(out, &l, false);
                    }
                }
            }
        }
        _ => count(out, path, same_leaf(a, b, path)),
    }
}

fn leaf_paths(v: &Value, path: &str, out: &mut Vec<String>) {
    match v {
        Value::Object(m) => {
            for (k, x) in m {
                leaf_paths(x, &format!("{path}.{k}"), out);
            }
        }
        Value::Array(a) if a.iter().any(Value::is_object) => {
            for x in a {
                leaf_paths(x, &format!("{path}[]"), out);
            }
        }
        _ => out.push(path.to_owned()),
    }
}

/// Two values of one field the same: numbers within the tolerance,
/// expressions in a canonical form, everything else exactly.
fn same_leaf(a: &Value, b: &Value, path: &str) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => {
            (x.as_f64().unwrap() - y.as_f64().unwrap()).abs()
                < if ROUNDED.load(std::sync::atomic::Ordering::Relaxed) {
                    ROUNDED_TOLERANCE
                } else {
                    TOLERANCE
                }
        }
        (Value::String(x), Value::String(y))
            if path.ends_with("kineticLaw") || path.ends_with("expression") =>
        {
            match (x.trim().parse::<f64>(), y.trim().parse::<f64>()) {
                (Ok(u), Ok(w)) => u == w,
                _ => canonical(x) == canonical(y),
            }
        }
        _ => a == b,
    }
}

/// An expression with its spacing and brackets made canonical.
fn canonical(text: &str) -> String {
    pd_core::expr::parse(text.trim())
        .map_or_else(|_| text.trim().to_owned(), |a| pd_core::expr::to_infix(&a))
}
