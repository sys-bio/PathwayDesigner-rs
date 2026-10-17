//! Looks and saved styles (S §8.5, §8.6): what Copy Style takes from an item
//! and Paste Style gives, and the links from items to the model's saved
//! styles.
//!
//! An item follows a saved style while it looks like it. Any change to its
//! look that doesn't come from the style drops the link, so the link never
//! claims a look the item doesn't have: [`drop_stale_links`] runs after every
//! edit.

use crate::model::{Look, Model, NodeShape, SavedStyle, SpeciesNode, StyleKind, VisualStyle};
use crate::render::draw::default_label_offset;

/// How the item `id` of `kind` looks, or `None` if there is no such item (or
/// it is a null node, which has no look of its own).
pub fn look_of(model: &Model, kind: StyleKind, id: &str) -> Option<Look> {
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
    }
}

/// Gives `look` to the item `id`, if it is of the look's kind (Paste Style,
/// S §8.5). A species keeps its own size unless the look has one (its name
/// outside), and one made a circle takes its larger side both ways; a name
/// already outside keeps its place, one taken outside starts by the node. A
/// null node is left alone. Doesn't touch the item's link.
pub fn give_look(model: &mut Model, id: &str, look: &Look) {
    match *look {
        Look::Species {
            style,
            shape,
            border_dash,
            label_outside,
            size,
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
            s.label_offset = label_place(s, label_outside);
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
/// already is outside, or just right of the node (S §5.11).
pub fn label_place(s: &SpeciesNode, outside: bool) -> Option<crate::geometry::Pt> {
    match (outside, s.label_offset) {
        (false, _) => None,
        (true, Some(o)) => Some(o),
        (true, None) => Some(default_label_offset(s)),
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
                .is_some_and(|st| look_of(model, kind, &id) == Some(st.look));
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
