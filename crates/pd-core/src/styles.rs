//! Looks and saved styles (S §8.5, §8.6): what Copy Style takes from an item
//! and Paste Style gives, and the links from items to the model's saved
//! styles.
//!
//! An item follows a saved style while it looks like it. Any change to its
//! look that doesn't come from the style drops the link, so the link never
//! claims a look the item doesn't have: [`drop_stale_links`] runs after every
//! edit.

use crate::model::{
    BorderDash, Look, Model, NodeShape, SavedStyle, SpeciesNode, StyleKind, VisualStyle,
};
use crate::render::constants::{ALIAS_BORDER, ALIAS_FILL, SPECIES_BORDER, SPECIES_FILL};
use crate::render::draw::clear_label_offset;

/// How the item `id` of `kind` looks, in standard form, or `None` if there
/// is no such item (or it is a null node, which has no look of its own).
pub fn look_of(model: &Model, kind: StyleKind, id: &str) -> Option<Look> {
    raw_look(model, kind, id).map(Look::canonical)
}

fn raw_look(model: &Model, kind: StyleKind, id: &str) -> Option<Look> {
    Some(match kind {
        StyleKind::Species => {
            let s = model.find_species(id).filter(|s| !s.is_null_node)?;
            species_look(s)
        }
        StyleKind::Reaction => Look::Reaction {
            style: model.find_reaction(id)?.style,
        },
        StyleKind::Compartment => Look::Compartment {
            style: model.find_compartment(id)?.style,
        },
        StyleKind::Text => {
            let t = model.find_text(id)?;
            Look::Text {
                font: t.font,
                align: t.align,
                size: t.size,
                color: t.color,
            }
        }
    })
}

fn species_look(s: &SpeciesNode) -> Look {
    Look::Species {
        // Whether a junction is shown is a reaction's setting.
        style: VisualStyle {
            junction_visible: true,
            ..s.style
        },
        shape: s.shape,
        border_dash: s.border_dash,
        label_outside: s.label_offset.is_some(),
        size: s.label_offset.map(|_| (s.width, s.height)),
        alias: s.is_alias(),
    }
}

/// The look as it is drawn, with nothing left to a default that differs
/// between a primary species and an alias: the fill and border colour drawn,
/// and the border solid or dashed (S §8.5). An alias's defaults are those
/// drawn with the alias indicator on. Other looks are as they are.
pub fn resolved(look: Look) -> Look {
    let Look::Species {
        style,
        shape,
        border_dash,
        label_outside,
        size,
        alias,
    } = look.canonical()
    else {
        return look.canonical();
    };
    let (fill, border) = if alias {
        (ALIAS_FILL, ALIAS_BORDER)
    } else {
        (SPECIES_FILL, SPECIES_BORDER)
    };
    let own = |c: crate::model::Rgba, default| if c.is_default() { default } else { c };
    Look::Species {
        style: VisualStyle {
            has_custom_style: true,
            fill_color: own(style.fill_color, fill),
            border_color: own(style.border_color, border),
            ..style
        },
        shape,
        border_dash: match border_dash {
            BorderDash::Automatic if alias => BorderDash::Dashed,
            BorderDash::Automatic => BorderDash::Solid,
            d => d,
        },
        label_outside,
        size,
        alias: false,
    }
}

/// True if two looks draw the same, wherever their defaults come from.
pub fn same_look(a: Look, b: Look) -> bool {
    resolved(a) == resolved(b)
}

/// True if an item with look `item` follows a style with look `style`: it
/// draws the same. An alias also follows a style when it draws as the style
/// does on an alias, with an alias's defaults where the style gives none
/// (a diagram's `ATP#2 style cofactor`, docs/diagram-format.md §6.2), as
/// well as when it took the style's look as a primary has it (S §8.5).
pub fn follows(item: Look, style: Look) -> bool {
    if same_look(item, style) {
        return true;
    }
    match (item, style) {
        (Look::Species { alias: true, .. }, Look::Species { .. }) => {
            let mut on_alias = style;
            if let Look::Species { alias, .. } = &mut on_alias {
                *alias = true;
            }
            same_look(item, on_alias)
        }
        _ => false,
    }
}

/// Gives `look` to the item `id`, if it is of the look's kind (Paste Style,
/// S §8.5). A species keeps its own size unless the look has one (its name
/// outside), and one made a circle takes its larger side both ways; a name
/// already outside keeps its place, one taken outside starts by the node. A
/// look from a primary given to an alias, or the other way round, is first
/// [`resolved`], so the alias looks like the primary (S §8.5). A null node is
/// left alone. Doesn't touch the item's link.
pub fn give_look(model: &mut Model, id: &str, look: &Look) {
    let look = match (*look, model.find_species(id)) {
        (Look::Species { alias, .. }, Some(s)) if alias != s.is_alias() => resolved(*look),
        _ => *look,
    };
    match look {
        Look::Species {
            style,
            shape,
            border_dash,
            label_outside,
            size,
            alias: _,
        } => {
            let Some(s) = model.find_species_mut(id).filter(|s| !s.is_null_node) else {
                return;
            };
            s.style = VisualStyle {
                junction_visible: s.style.junction_visible,
                ..style
            };
            s.border_dash = border_dash;
            s.shape = shape;
            if let Some((w, h)) = size {
                s.resize(w, h);
            }
            if s.shape == NodeShape::Circle {
                let d = s.width.max(s.height);
                s.resize(d, d);
            }
            // Where the name goes depends on the node's lines, so it is
            // worked out with the whole model.
            let place = model
                .find_species(id)
                .and_then(|s| label_place(model, s, label_outside));
            if let Some(s) = model.find_species_mut(id) {
                s.label_offset = place;
            }
        }
        Look::Reaction { style } => {
            if let Some(r) = model.find_reaction_mut(id) {
                r.style = style;
            }
        }
        Look::Compartment { style } => {
            if let Some(c) = model.find_compartment_mut(id) {
                c.style = style;
            }
        }
        Look::Text {
            font,
            align,
            size,
            color,
        } => {
            if let Some(t) = model.find_text_mut(id) {
                (t.font, t.align, t.size, t.color) = (font, align, size, color);
            }
        }
    }
}

/// Where `s`'s name goes when it is put `outside` or not: inside, where it
/// already is outside, or beside the node on the side clearest of its lines
/// (S §5.11).
pub fn label_place(model: &Model, s: &SpeciesNode, outside: bool) -> Option<crate::geometry::Pt> {
    match (outside, s.label_offset) {
        (false, _) => None,
        (true, Some(o)) => Some(o),
        (true, None) => Some(clear_label_offset(model, s)),
    }
}

/// The ids of every item of `kind`, in drawing order. Null nodes are
/// included, though they never follow a style.
pub fn item_ids(model: &Model, kind: StyleKind) -> Vec<String> {
    match kind {
        StyleKind::Species => model.species().iter().map(|s| s.id.clone()).collect(),
        StyleKind::Reaction => model.reactions().iter().map(|r| r.id.clone()).collect(),
        StyleKind::Compartment => model.compartments.iter().map(|c| c.id.clone()).collect(),
        StyleKind::Text => model.texts().iter().map(|t| t.id.clone()).collect(),
    }
}

/// The ids of the selected items of `kind` (null nodes aside).
pub fn selected_ids(model: &Model, kind: StyleKind) -> Vec<String> {
    match kind {
        StyleKind::Species => model
            .selected_species()
            .filter(|s| !s.is_null_node)
            .map(|s| s.id.clone())
            .collect(),
        StyleKind::Reaction => model.selected_reactions().map(|r| r.id.clone()).collect(),
        StyleKind::Compartment => model
            .selected_compartments()
            .map(|c| c.id.clone())
            .collect(),
        StyleKind::Text => model.selected_texts().map(|t| t.id.clone()).collect(),
    }
}

/// The name of the saved style item `id` of `kind` follows.
pub fn link(model: &Model, kind: StyleKind, id: &str) -> Option<String> {
    match kind {
        StyleKind::Species => model.find_species(id)?.style_name.clone(),
        StyleKind::Reaction => model.find_reaction(id)?.style_name.clone(),
        StyleKind::Compartment => model.find_compartment(id)?.style_name.clone(),
        StyleKind::Text => model.find_text(id)?.style_name.clone(),
    }
}

/// Makes item `id` of `kind` follow the style `name`, or nothing.
pub fn set_link(model: &mut Model, kind: StyleKind, id: &str, name: Option<String>) {
    let slot = match kind {
        StyleKind::Species => model.find_species_mut(id).map(|s| &mut s.style_name),
        StyleKind::Reaction => model.find_reaction_mut(id).map(|r| &mut r.style_name),
        StyleKind::Compartment => model.find_compartment_mut(id).map(|c| &mut c.style_name),
        StyleKind::Text => model.find_text_mut(id).map(|t| &mut t.style_name),
    };
    if let Some(slot) = slot {
        *slot = name;
    }
}

/// The model's style `name` for `kind`.
pub fn find<'a>(model: &'a Model, kind: StyleKind, name: &str) -> Option<&'a SavedStyle> {
    model
        .styles
        .iter()
        .find(|s| s.kind() == kind && s.name == name)
}

/// The ids of the items of `kind` that follow the style `name`.
pub fn users(model: &Model, kind: StyleKind, name: &str) -> Vec<String> {
    item_ids(model, kind)
        .into_iter()
        .filter(|id| link(model, kind, id).as_deref() == Some(name))
        .collect()
}

/// Drops every link that no longer holds: to a style the model doesn't have,
/// or from an item that no longer looks like its style. Returns true if any
/// was dropped.
pub fn drop_stale_links(model: &mut Model) -> bool {
    let mut stale = Vec::new();
    for kind in StyleKind::ALL {
        for id in item_ids(model, kind) {
            let Some(name) = link(model, kind, &id) else {
                continue;
            };
            let holds = find(model, kind, &name)
                .is_some_and(|st| look_of(model, kind, &id).is_some_and(|l| follows(l, st.look)));
            if !holds {
                stale.push((kind, id));
            }
        }
    }
    for (kind, id) in &stale {
        set_link(model, *kind, id, None);
    }
    !stale.is_empty()
}

/// True if some item follows a saved style: the model needs format version 6
/// to keep its styles (SPEC-Rust §4.2).
pub fn any_links(model: &Model) -> bool {
    StyleKind::ALL.into_iter().any(|kind| {
        item_ids(model, kind)
            .iter()
            .any(|id| link(model, kind, id).is_some())
    })
}

/// One line of the Styles list for a kind (S §8.6): a style of the model,
/// of My Styles (the app's own), or of both under one name.
#[derive(Clone, Debug, PartialEq)]
pub struct StyleEntry {
    pub name: String,
    /// The model's look when the model has it, otherwise My Styles' look.
    pub look: Look,
    pub in_model: bool,
    pub in_mine: bool,
    /// Both have it, with different looks: in this model the model's wins.
    pub differs: bool,
    /// How many items of the model follow it.
    pub users: usize,
}

/// The styles of `kind` to choose from: the model's, in the order they were
/// saved, then those of My Styles (`mine`) the model doesn't have.
pub fn entries(model: &Model, mine: &[SavedStyle], kind: StyleKind) -> Vec<StyleEntry> {
    let mut out: Vec<StyleEntry> = model
        .styles
        .iter()
        .filter(|s| s.kind() == kind)
        .map(|s| {
            let my = find_in(mine, kind, &s.name);
            StyleEntry {
                name: s.name.clone(),
                look: s.look,
                in_model: true,
                in_mine: my.is_some(),
                // Saved styles are all kept as drawn, so they compare as they are.
                differs: my.is_some_and(|m| m.look != s.look),
                users: users(model, kind, &s.name).len(),
            }
        })
        .collect();
    for m in mine.iter().filter(|m| m.kind() == kind) {
        if find(model, kind, &m.name).is_none() {
            out.push(StyleEntry {
                name: m.name.clone(),
                look: m.look,
                in_model: false,
                in_mine: true,
                differs: false,
                users: 0,
            });
        }
    }
    out
}

/// The style `name` of `kind` in a list of styles.
pub fn find_in<'a>(list: &'a [SavedStyle], kind: StyleKind, name: &str) -> Option<&'a SavedStyle> {
    list.iter().find(|s| s.kind() == kind && s.name == name)
}

/// Adds `style` to a list, or replaces the one of its kind and name.
pub fn put(list: &mut Vec<SavedStyle>, style: SavedStyle) {
    match list
        .iter_mut()
        .find(|s| s.kind() == style.kind() && s.name == style.name)
    {
        Some(s) => *s = style,
        None => list.push(style),
    }
}

/// A tiny model that shows `look`, for the Styles list: a species named
/// "A"; a reaction from A to B; a compartment's box; a text "Text".
pub fn preview(look: &Look) -> Model {
    use crate::geometry::pt;
    let mut m = Model::new();
    match look.kind() {
        StyleKind::Species => {
            let mut s = SpeciesNode::new("A", pt(0.0, 0.0));
            s.width = 50.0;
            s.height = 26.0;
            m.push_species(s).expect("an empty model");
            give_look(&mut m, "A", look);
        }
        StyleKind::Reaction => {
            for (id, x) in [("A", -60.0), ("B", 60.0)] {
                let mut s = SpeciesNode::new(id, pt(x, 0.0));
                (s.width, s.height) = (24.0, 20.0);
                m.push_species(s).expect("an empty model");
            }
            let mut r = crate::model::Reaction::new("R", pt(0.0, 0.0));
            r.reactants.push(crate::model::Participant::new("A", 1.0));
            r.products.push(crate::model::Participant::new("B", 1.0));
            m.push_reaction(r).expect("an empty model");
            give_look(&mut m, "R", look);
        }
        StyleKind::Compartment => {
            let mut c = crate::model::Compartment::new("c");
            c.frame = Some(crate::model::Frame {
                center: pt(0.0, 0.0),
                width: 90.0,
                height: 40.0,
            });
            m.compartments.push(c);
            give_look(&mut m, "c", look);
        }
        StyleKind::Text => {
            m.push_text(crate::model::TextItem::new("T", pt(0.0, 0.0), "Text"))
                .expect("an empty model");
            give_look(&mut m, "T", look);
        }
    }
    m
}
