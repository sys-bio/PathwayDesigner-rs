//! Cut, Copy and Paste of parts of a network (docs/cut-copy-paste-design.md).
//!
//! Copy makes a [`Fragment`]: the selected items, the species the selected
//! reactions join (C1), and the parameters, rules and compartments their rate
//! laws and rules use. On the clipboard it is a native JSON document with a
//! marker around it. Paste adds a fragment to a model: an id already used
//! gets `_2`, `_3`, … (C2), expressions follow the new ids, and everything
//! moves by an offset.

use std::collections::{HashMap, HashSet};

use serde_json::{Map, Value};

use crate::geometry::{Pt, pt};
use crate::model::{Annotation, Frame, Id, Model};

/// The key that says a JSON document is a copy from PathwayDesigner.
const MARKER: &str = "pathwayDesignerClipboard";

/// What Copy takes, ready to paste.
#[derive(Clone, Debug, PartialEq)]
pub struct Fragment {
    /// The copied items, as a model of their own, nothing selected.
    pub model: Model,
    /// Species made from aliases copied without their primary: the first
    /// such alias stands for the primary, under its id. Pasted where a
    /// species of that id is, they become aliases of it.
    pub from_aliases: Vec<Id>,
    /// Compartments the copied species are in, or the copied expressions
    /// use, that weren't selected themselves. Pasting uses the document's
    /// own compartment of that id if it has one.
    pub compartments_used: Vec<Id>,
}

/// The selection of `model` as a fragment, or `None` with nothing selected.
pub fn copy(model: &Model) -> Option<Fragment> {
    let reactions: Vec<_> = model.selected_reactions().collect();
    let mut wanted: HashSet<&str> = model.selected_species().map(|s| s.id.as_str()).collect();
    for r in &reactions {
        for p in r.reactants.iter().chain(&r.products).chain(&r.modifiers) {
            wanted.insert(&p.species);
        }
    }
    let selected_boxes: HashSet<&str> = model
        .selected_compartments()
        .map(|c| c.id.as_str())
        .collect();
    if wanted.is_empty() && selected_boxes.is_empty() && model.selected_texts().next().is_none() {
        return None;
    }

    let mut frag = Model::new();
    let mut stand_ins: HashMap<Id, Id> = HashMap::new();
    let mut from_aliases: Vec<Id> = Vec::new();
    for s in model
        .species()
        .iter()
        .filter(|s| wanted.contains(s.id.as_str()))
    {
        let mut node = s.clone();
        node.selected = false;
        if let Some(primary) = s.alias_of.as_ref().filter(|p| !wanted.contains(p.as_str()))
            && !from_aliases.contains(primary)
        {
            // The first alias copied without its primary stands for it.
            node.id = primary.clone();
            node.alias_of = None;
            if let Some(p) = model.find_species(primary) {
                node.initial_value = p.initial_value;
                node.in_amounts = p.in_amounts;
                node.initial_is_amount = p.initial_is_amount;
                node.is_boundary = p.is_boundary;
                node.is_constant = p.is_constant;
                node.compartment = p.compartment.clone();
                node.display_name = p.display_name.clone();
                node.annotation = p.annotation.clone();
            }
            stand_ins.insert(s.id.clone(), primary.clone());
            from_aliases.push(primary.clone());
        }
        frag.push_species(node).expect("ids are unique in a model");
    }
    let node_id = |id: &Id| stand_ins.get(id).unwrap_or(id).clone();

    // What the expressions use: words naming parameters, rules or
    // compartments, followed through the parameters' and rules' own
    // expressions.
    let mut words: Vec<String> = Vec::new();
    for r in &reactions {
        let mut r = (*r).clone();
        r.selected = false;
        for p in r
            .reactants
            .iter_mut()
            .chain(&mut r.products)
            .chain(&mut r.modifiers)
        {
            p.species = node_id(&p.species);
        }
        words.extend(words_in(&r.kinetic_law));
        frag.push_reaction(r).expect("ids are unique in a model");
    }
    let mut rules: HashSet<&str> = HashSet::new();
    for rule in &model.assignment_rules {
        if frag.find_species(&rule.variable).is_some() {
            rules.insert(&rule.variable);
            words.extend(words_in(&rule.expression));
        }
    }
    let mut params: HashSet<&str> = HashSet::new();
    let mut used_boxes: HashSet<&str> = HashSet::new();
    let mut seen: HashSet<String> = HashSet::new();
    while let Some(w) = words.pop() {
        if !seen.insert(w.clone()) {
            continue;
        }
        if let Some(p) = model.parameters.iter().find(|p| p.variable == w) {
            params.insert(&p.variable);
            words.extend(words_in(&p.expression));
        }
        if let Some(r) = model.assignment_rules.iter().find(|r| r.variable == w) {
            rules.insert(&r.variable);
            words.extend(words_in(&r.expression));
        }
        if let Some(c) = model.compartments.iter().find(|c| c.id == w) {
            used_boxes.insert(&c.id);
        }
    }
    for s in frag.species() {
        if let Some(c) = model.find_compartment(&s.compartment) {
            used_boxes.insert(&c.id);
        }
    }

    let mut compartments_used = Vec::new();
    for c in &model.compartments {
        let mut c = c.clone();
        if selected_boxes.contains(c.id.as_str()) {
            c.selected = false;
        } else if used_boxes.contains(c.id.as_str()) {
            c.frame = None;
            c.hidden = false;
            compartments_used.push(c.id.clone());
        } else {
            continue;
        }
        frag.compartments.push(c);
    }
    frag.parameters = model
        .parameters
        .iter()
        .filter(|p| params.contains(p.variable.as_str()))
        .cloned()
        .collect();
    frag.assignment_rules = model
        .assignment_rules
        .iter()
        .filter(|r| rules.contains(r.variable.as_str()))
        .cloned()
        .collect();
    for t in model.selected_texts() {
        let mut t = t.clone();
        t.selected = false;
        frag.push_text(t).expect("ids are unique in a model");
    }
    frag.styles = model.styles.clone();
    frag.reference_names = model.reference_names.clone();
    frag.show_display_names = model.show_display_names;
    Some(Fragment {
        model: frag,
        from_aliases,
        compartments_used,
    })
}

/// The fragment as clipboard text: `{"pathwayDesignerClipboard": 1,
/// "fromAliases": [...], "compartmentsUsed": [...], "model": {...}}`, the
/// model as a native document.
pub fn to_text(fragment: &Fragment) -> String {
    let model: Value = serde_json::from_str(&crate::json::to_json(&fragment.model))
        .expect("the native writer writes JSON");
    let mut map = Map::new();
    map.insert(MARKER.into(), 1.into());
    map.insert("fromAliases".into(), fragment.from_aliases.clone().into());
    map.insert(
        "compartmentsUsed".into(),
        fragment.compartments_used.clone().into(),
    );
    map.insert("model".into(), model);
    let mut text = serde_json::to_string_pretty(&Value::Object(map)).expect("a JSON value");
    text.push('\n');
    text
}

/// The fragment in clipboard text, or `None` if the text isn't a copy from
/// PathwayDesigner.
pub fn from_text(text: &str) -> Option<Fragment> {
    let value: Value = serde_json::from_str(text.trim_start_matches('\u{FEFF}').trim()).ok()?;
    value.get(MARKER)?;
    let model = crate::json::from_json(&value.get("model")?.to_string())
        .ok()?
        .model;
    let ids = |key: &str| -> Vec<Id> {
        value
            .get(key)
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default()
    };
    Some(Fragment {
        from_aliases: ids("fromAliases"),
        compartments_used: ids("compartmentsUsed"),
        model,
    })
}

impl Fragment {
    /// The box around what is drawn: species, junctions, texts and the
    /// selected compartments' boxes. `None` if nothing is drawn.
    pub fn bounds(&self) -> Option<Frame> {
        let m = &self.model;
        let mut corners: Vec<(Pt, Pt)> = Vec::new();
        for s in m.species() {
            let half = pt(s.width / 2.0, s.height / 2.0);
            corners.push((s.center - half, s.center + half));
        }
        for r in m.reactions() {
            corners.push((r.junction, r.junction));
        }
        for t in m.texts() {
            corners.push(crate::textbox::text_box(t));
        }
        for c in &m.compartments {
            if let Some(f) = c.frame.filter(|_| !self.compartments_used.contains(&c.id)) {
                corners.push((f.min(), f.max()));
            }
        }
        let (first, rest) = corners.split_first()?;
        let (lo, hi) = rest.iter().fold(*first, |(lo, hi), (a, b)| {
            (
                pt(lo.x.min(a.x), lo.y.min(a.y)),
                pt(hi.x.max(b.x), hi.y.max(b.y)),
            )
        });
        Some(Frame::from_corners(lo, hi))
    }
}

/// Adds `fragment` to `model`, moved by `offset`, with only the pasted items
/// selected. Ids already used in `model` (in any case) get the first free
/// `_2`, `_3`, …; rate laws, parameters and rules follow. Species take the
/// compartment they land in, as when the user moves them.
pub fn paste(model: &mut Model, fragment: &Fragment, offset: Pt) {
    let f = &fragment.model;
    model.clear_selection();

    // Aliases copied without their primary join the species of that id here.
    let joined: HashSet<&Id> = fragment
        .from_aliases
        .iter()
        .filter(|id| {
            model
                .find_species(id)
                .is_some_and(|s| !s.is_alias() && !s.is_null_node)
        })
        .collect();

    // New names, for ids and in expressions.
    let mut names: HashMap<Id, Id> = HashMap::new();
    let mut nodes: HashMap<Id, Id> = HashMap::new();
    let mut taken: HashSet<Id> = HashSet::new();
    let mut fresh = |id: &str| -> Id {
        let free = |c: &str| !model.is_id_taken_for_rename(c) && !taken.contains(c);
        let new = if free(id) {
            id.to_owned()
        } else {
            (2..)
                .map(|n| format!("{id}_{n}"))
                .find(|c| free(c))
                .expect("some suffix is free")
        };
        taken.insert(new.clone());
        new
    };
    for s in f.species() {
        if s.alias_of.is_none() && joined.contains(&s.id) {
            names.insert(s.id.clone(), s.id.clone());
            nodes.insert(s.id.clone(), fresh(&s.id));
        } else {
            names.insert(s.id.clone(), fresh(&s.id));
        }
    }
    for r in f.reactions() {
        names.insert(r.id.clone(), fresh(&r.id));
    }
    for v in f
        .parameters
        .iter()
        .map(|p| &p.variable)
        .chain(f.assignment_rules.iter().map(|r| &r.variable))
    {
        if !names.contains_key(v) {
            let new = fresh(v);
            names.insert(v.clone(), new);
        }
    }
    let mut added_used: Vec<Id> = Vec::new();
    for c in &f.compartments {
        let new = match model.find_compartment(&c.id) {
            Some(own) if fragment.compartments_used.contains(&c.id) => own.id.clone(),
            _ => fresh(&c.id),
        };
        names.insert(c.id.clone(), new);
    }
    for t in f.texts() {
        names.insert(t.id.clone(), fresh(&t.id));
    }
    let name = |id: &str| names.get(id).cloned().unwrap_or_else(|| id.to_owned());
    let node_id = |id: &str| nodes.get(id).cloned().unwrap_or_else(|| name(id));

    let mut pasted_species = Vec::new();
    for s in f.species() {
        let mut node = s.clone();
        node.id = node_id(&s.id);
        node.alias_of = match &s.alias_of {
            Some(p) => Some(name(p)),
            None if joined.contains(&s.id) => {
                node.display_name = None;
                node.annotation = Annotation::default();
                Some(s.id.clone())
            }
            None => None,
        };
        if !node.compartment.is_empty() {
            node.compartment = name(&node.compartment);
        }
        node.center += offset;
        node.selected = true;
        pasted_species.push(node.id.clone());
        model.push_species(node).expect("fresh ids are unused");
    }
    for r in f.reactions() {
        let mut r = r.clone();
        r.id = name(&r.id);
        r.kinetic_law = rename_words(&r.kinetic_law, &names);
        r.junction += offset;
        for p in r
            .reactants
            .iter_mut()
            .chain(&mut r.products)
            .chain(&mut r.modifiers)
        {
            p.species = node_id(&p.species);
            p.ctrl1 += offset;
            p.ctrl2 += offset;
            for q in &mut p.points {
                *q += offset;
            }
        }
        r.selected = true;
        model.push_reaction(r).expect("fresh ids are unused");
    }
    for p in &f.parameters {
        let mut p = p.clone();
        p.variable = name(&p.variable);
        p.expression = rename_words(&p.expression, &names);
        model.parameters.push(p);
    }
    for r in &f.assignment_rules {
        let mut r = r.clone();
        r.variable = name(&r.variable);
        r.expression = rename_words(&r.expression, &names);
        model.assignment_rules.push(r);
    }
    for c in &f.compartments {
        let used = fragment.compartments_used.contains(&c.id);
        let mut c = c.clone();
        c.id = name(&c.id);
        if used {
            if model.find_compartment(&c.id).is_some() {
                continue;
            }
            added_used.push(c.id.clone());
        } else {
            if let Some(frame) = c.frame.as_mut() {
                frame.center += offset;
            }
            c.selected = true;
        }
        model.compartments.push(c);
    }
    for t in f.texts() {
        let mut t = t.clone();
        t.id = name(&t.id);
        t.position += offset;
        t.selected = true;
        model.push_text(t).expect("fresh ids are unused");
    }
    for st in &f.styles {
        if !model.styles.iter().any(|s| s.name == st.name) {
            model.styles.push(st.clone());
        }
    }
    for (k, v) in &f.reference_names {
        model
            .reference_names
            .entry(k.clone())
            .or_insert_with(|| v.clone());
    }

    crate::compartments::reassign(model, &pasted_species);
    // A compartment brought along only for its species, which landed in
    // others, and used by no expression, isn't wanted.
    let unwanted: Vec<Id> = added_used
        .into_iter()
        .filter(|id| {
            !model
                .species()
                .iter()
                .any(|s| s.compartment.eq_ignore_ascii_case(id))
                && !expressions(model).any(|e| words_in(e).iter().any(|w| w == id))
        })
        .collect();
    model.compartments.retain(|c| !unwanted.contains(&c.id));
    crate::styles::drop_stale_links(model);
    model.refresh_labels();
}

fn expressions(model: &Model) -> impl Iterator<Item = &str> {
    model
        .reactions()
        .iter()
        .map(|r| r.kinetic_law.as_str())
        .chain(model.parameters.iter().map(|p| p.expression.as_str()))
        .chain(model.assignment_rules.iter().map(|r| r.expression.as_str()))
}

/// The names in an expression: runs of letters, digits and `_` that don't
/// start with a digit.
fn words_in(text: &str) -> Vec<String> {
    word_spans(text)
        .into_iter()
        .map(|(a, b)| text[a..b].to_owned())
        .collect()
}

/// `text` with every name in `names` replaced at once, so a name given to
/// one item is never renamed again for another.
fn rename_words(text: &str, names: &HashMap<Id, Id>) -> String {
    let mut out = String::with_capacity(text.len());
    let mut last = 0;
    for (a, b) in word_spans(text) {
        if let Some(new) = names.get(&text[a..b]) {
            out.push_str(&text[last..a]);
            out.push_str(new);
            last = b;
        }
    }
    out.push_str(&text[last..]);
    out
}

/// Where the names are in `text`, as byte ranges.
fn word_spans(text: &str) -> Vec<(usize, usize)> {
    let is_word = |b: u8| b.is_ascii_alphanumeric() || b == b'_' || b >= 0x80;
    let bytes = text.as_bytes();
    let mut spans = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if is_word(bytes[i]) {
            let start = i;
            while i < bytes.len() && is_word(bytes[i]) {
                i += 1;
            }
            if !bytes[start].is_ascii_digit() {
                spans.push((start, i));
            }
        } else {
            i += 1;
        }
    }
    spans
}
