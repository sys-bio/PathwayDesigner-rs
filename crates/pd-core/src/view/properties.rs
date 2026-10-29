//! Property and style edits from the inspector (S §8.1, §8.2, §8.4).
//!
//! Every edit is one undo step (fixes D-4): colours, widths and junction
//! visibility as a StyleEdit command, the rest as snapshots.

use super::DiagramView;
use crate::expr;
use crate::model::{
    BorderDash, Compartment, Id, LabelChoice, Model, ModifierKind, NodeShape, RenameError, Rgba,
    StyleKind, TextAlign, TextItem, Typeface, VisualStyle,
};
use crate::render::constants::LABEL;
use crate::render::draw::compartment_colours;
use crate::render::draw::{line_colour, species_colours};
use crate::undo::Command;
use crate::view::{Target, ViewResponse};

/// Which colour a swatch sets (S §8.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColourField {
    /// Fill of the selected species.
    Fill,
    /// Border of the selected species.
    Border,
    /// Label of the selected species (S §8.1).
    Label,
    /// Lines of the selected reactions.
    Line,
    /// Names of the selected reactions (S §5.12).
    ReactionLabel,
    /// The selected texts (S §7.8).
    Text,
    /// Fill of the selected compartments (S §7.9).
    CompartmentFill,
    /// The diagram's background (S §8.8), whatever is selected.
    Background,
    /// Border of the selected compartments.
    CompartmentBorder,
}

/// Brings a reaction's modifiers in line with its rate law (S §8.4):
/// - an empty law removes every modifier;
/// - a law that doesn't parse leaves them alone;
/// - otherwise, modifiers no longer named in the law are removed, and every
///   species the law names that isn't a reactant or product is added (once).
///
/// Species are matched by their primary's id, so aliases count as their
/// primary.
pub fn sync_modifiers(model: &mut Model, id: &str) {
    let Some(r) = model.find_reaction(id) else {
        return;
    };
    let primary = |sp: &str| {
        model
            .find_species(sp)
            .map_or(sp.to_owned(), |s| s.primary_id().to_owned())
    };
    let named: Vec<String> = if r.kinetic_law.trim().is_empty() {
        Vec::new()
    } else {
        match expr::parse(&r.kinetic_law) {
            Ok(ast) => expr::identifiers(&ast)
                .into_iter()
                .map(str::to_owned)
                .collect(),
            Err(_) => return,
        }
    };
    let ends: Vec<String> = r
        .reactants
        .iter()
        .chain(&r.products)
        .map(|p| primary(&p.species))
        .collect();
    let mut modifiers: Vec<_> = r
        .modifiers
        .iter()
        .filter(|m| named.contains(&primary(&m.species)))
        .cloned()
        .collect();
    for name in &named {
        let is_species = model
            .find_species(name)
            .is_some_and(|s| !s.is_alias() && !s.is_null_node);
        let present = modifiers.iter().any(|m| primary(&m.species) == *name);
        if is_species && !ends.contains(name) && !present {
            modifiers.push(crate::model::Participant::new(name.as_str(), 1.0));
        }
    }
    model.find_reaction_mut(id).expect("found above").modifiers = modifiers;
}

/// The kind and id of the item a target stands for, for its look: a
/// species by its node, corner or name, a reaction by its junction, a leg or
/// a handle, a compartment by its border or corner, a text by itself or its
/// wrap handle.
pub(super) fn style_target(t: &Target) -> Option<(StyleKind, Id)> {
    Some(match t {
        Target::Species(id) | Target::SpeciesCorner(id, _) | Target::SpeciesLabel(id) => {
            (StyleKind::Species, id.clone())
        }
        Target::Junction(id)
        | Target::Leg(id)
        | Target::ReactionLabel(id)
        | Target::StemEnd(id, _)
        | Target::AddLine(id)
        | Target::ModifierLine(id, ..)
        | Target::CornerPoint(id, ..) => (StyleKind::Reaction, id.clone()),
        Target::Handle(h) => (StyleKind::Reaction, h.reaction.clone()),
        Target::Compartment(id) | Target::CompartmentCorner(id, _) => {
            (StyleKind::Compartment, id.clone())
        }
        Target::Text(id) | Target::TextWrap(id) => (StyleKind::Text, id.clone()),
        Target::Empty => return None,
    })
}

impl DiagramView {
    /// Sets a reaction's rate law and syncs its modifiers. The law is kept
    /// even if it doesn't parse (S §8.4). One undo step.
    pub fn set_kinetic_law(&mut self, id: &str, law: &str) -> ViewResponse {
        let before = self.model.clone();
        let Some(r) = self.model.find_reaction_mut(id) else {
            return ViewResponse::default();
        };
        r.kinetic_law = law.to_owned();
        sync_modifiers(&mut self.model, id);
        self.record_if_changed("Edit Rate Law", before)
    }

    /// Sets a species' initial value. For an alias, its primary's. One undo step.
    pub fn set_initial_value(&mut self, id: &str, value: f64) -> ViewResponse {
        if !value.is_finite() {
            return ViewResponse::default();
        }
        let Some(primary) = self
            .model
            .find_species(id)
            .map(|s| s.primary_id().to_owned())
        else {
            return ViewResponse::default();
        };
        let before = self.model.clone();
        if let Some(s) = self.model.find_species_mut(&primary) {
            s.initial_value = value;
        }
        self.record_if_changed("Edit Concentration", before)
    }

    /// Sets the display name of a species (its primary's, for an alias), a
    /// reaction or a compartment (docs/display-names-design.md D3). A blank
    /// name removes it. Neither the id nor any rate law changes. One undo
    /// step.
    pub fn set_display_name(&mut self, id: &str, name: &str) -> ViewResponse {
        let name = crate::model::display_name(Some(name.trim().to_owned()));
        let before = self.model.clone();
        let primary = self
            .model
            .find_species(id)
            .map(|s| s.primary_id().to_owned());
        if let Some(primary) = primary {
            if let Some(s) = self.model.find_species_mut(&primary) {
                s.display_name = name;
            }
        } else if let Some(r) = self.model.find_reaction_mut(id) {
            r.display_name = name;
        } else if let Some(c) = self.model.find_compartment_mut(id) {
            c.display_name = name;
        }
        self.record_if_changed("Edit Display Name", before)
    }

    /// Whether labels show display names (View ▸ Show Display Names,
    /// docs/display-names-design.md D4). A setting of the diagram, saved in
    /// its file.
    pub fn show_display_names(&self) -> bool {
        self.model.show_display_names
    }

    /// Shows display names in the labels, or ids. One undo step.
    pub fn set_show_display_names(&mut self, show: bool) -> ViewResponse {
        let before = self.model.clone();
        self.model.show_display_names = show;
        let desc = if show {
            "Show Display Names"
        } else {
            "Hide Display Names"
        };
        self.record_if_changed(desc, before)
    }

    /// The label choice every selected item of `kind` has, if they all have
    /// the same one (D4). Species, reactions and compartments only.
    pub fn selected_label_choice(&self, kind: StyleKind) -> Option<LabelChoice> {
        let m = &self.model;
        let choices: Vec<LabelChoice> = match kind {
            StyleKind::Species => m
                .species()
                .iter()
                .filter(|s| s.selected && !s.is_null_node)
                .map(|s| s.label_choice)
                .collect(),
            StyleKind::Reaction => m
                .reactions()
                .iter()
                .filter(|r| r.selected)
                .map(|r| r.label_choice)
                .collect(),
            StyleKind::Compartment => m
                .compartments
                .iter()
                .filter(|c| c.selected)
                .map(|c| c.label_choice)
                .collect(),
            StyleKind::Text => Vec::new(),
        };
        let first = *choices.first()?;
        choices.iter().all(|c| *c == first).then_some(first)
    }

    /// Sets what the labels of the selected items of `kind` show (D4): as
    /// the diagram says, the id, or the display name. Each drawing of a
    /// species has its own. One undo step.
    pub fn set_label_choice(&mut self, kind: StyleKind, choice: LabelChoice) -> ViewResponse {
        let before = self.model.clone();
        match kind {
            StyleKind::Species => {
                for s in self.model.species_mut().filter(|s| s.selected) {
                    s.label_choice = choice;
                }
            }
            StyleKind::Reaction => {
                for r in self.model.reactions_mut().filter(|r| r.selected) {
                    r.label_choice = choice;
                }
            }
            StyleKind::Compartment => {
                for c in self.model.compartments.iter_mut().filter(|c| c.selected) {
                    c.label_choice = choice;
                }
            }
            StyleKind::Text => {}
        }
        self.record_if_changed("Label", before)
    }

    /// Makes the selected species boundary species, or not (their primaries,
    /// for aliases). One undo step.
    pub fn set_boundary(&mut self, boundary: bool) -> ViewResponse {
        let ids: Vec<Id> = self
            .model
            .selected_species()
            .map(|s| s.primary_id().to_owned())
            .collect();
        let before = self.model.clone();
        for id in ids {
            if let Some(s) = self.model.find_species_mut(&id) {
                s.is_boundary = boundary;
            }
        }
        self.record_if_changed("Boundary Species", before)
    }

    /// The colour a swatch shows: the first selected item's effective colour
    /// (S §8.1). `None` if nothing of that kind is selected.
    pub fn swatch_colour(&self, field: ColourField) -> Option<Rgba> {
        match field {
            ColourField::Fill | ColourField::Border => {
                let s = self.model.selected_species().next()?;
                let (fill, border) = species_colours(s, self.show_alias_indicator);
                Some(if field == ColourField::Fill {
                    fill
                } else {
                    border
                })
            }
            ColourField::Label => self.model.selected_species().next().map(|s| {
                let custom = s.style.has_custom_style && !s.style.label_color.is_default();
                if custom { s.style.label_color } else { LABEL }
            }),
            ColourField::Line => self.model.selected_reactions().next().map(line_colour),
            ColourField::ReactionLabel => self
                .model
                .selected_reactions()
                .next()
                .map(crate::render::draw::reaction_label_colour),
            ColourField::CompartmentFill | ColourField::CompartmentBorder => {
                let c = self.model.selected_compartments().next()?;
                let (fill, border, _) = compartment_colours(c);
                Some(if field == ColourField::CompartmentFill {
                    fill
                } else {
                    border
                })
            }
            ColourField::Background => Some(crate::render::draw::background_colour(&self.model)),
            ColourField::Text => self
                .model
                .selected_texts()
                .next()
                .map(|t| if t.color.is_default() { LABEL } else { t.color }),
        }
    }

    /// Applies a colour to every selected item of the matching kind, and
    /// makes their styles custom. One undo step (S §8.2).
    pub fn apply_colour(&mut self, field: ColourField, colour: Rgba) -> ViewResponse {
        if field == ColourField::Background {
            return self.set_background(colour);
        }
        if field == ColourField::Text {
            return self.on_selected_texts("Text Colour", |t| t.color = colour);
        }
        if field == ColourField::CompartmentFill {
            return self.on_selected_compartments("Compartment Colour", |c| {
                c.style.has_custom_style = true;
                c.style.fill_color = colour;
            });
        }
        if field == ColourField::CompartmentBorder {
            return self.on_selected_compartments("Compartment Colour", |c| {
                c.style.has_custom_style = true;
                c.style.border_color = colour;
            });
        }
        self.style_edit(
            |s| {
                match field {
                    ColourField::Fill => Some(&mut s.fill_color),
                    ColourField::Border => Some(&mut s.border_color),
                    ColourField::Label => Some(&mut s.label_color),
                    _ => None,
                }
                .into_iter()
                .for_each(|c| {
                    *c = colour;
                    s.has_custom_style = true;
                })
            },
            |r| {
                match field {
                    ColourField::Line => Some(&mut r.line_color),
                    ColourField::ReactionLabel => Some(&mut r.label_color),
                    _ => None,
                }
                .into_iter()
                .for_each(|c| {
                    *c = colour;
                    r.has_custom_style = true;
                })
            },
            !matches!(field, ColourField::Line | ColourField::ReactionLabel),
            matches!(field, ColourField::Line | ColourField::ReactionLabel),
        )
    }

    /// Sets the border width of species `id` (0 means the default). One undo step.
    pub fn set_border_width(&mut self, id: &str, width: f64) -> ViewResponse {
        if !(width.is_finite() && width >= 0.0) {
            return ViewResponse::default();
        }
        // 0: no border (S §5.3); its width is kept for when it gets one.
        self.style_edit_one(id, true, |s| {
            s.no_border = width == 0.0;
            if width > 0.0 {
                s.border_width = width;
            }
            s.has_custom_style = true;
        })
    }

    /// Marks the selected reactions reversible or not (S §8.1): a reversible
    /// one is drawn with an arrowhead at both ends (S §5.5). One undo step.
    pub fn set_reversible(&mut self, reversible: bool) -> ViewResponse {
        let before = self.model.clone();
        for r in self.model.reactions_mut().filter(|r| r.selected) {
            r.is_reversible = reversible;
        }
        self.record_if_changed("Reversible", before)
    }

    /// Sets what `species` does as a modifier of reaction `id` (S §5.7.1):
    /// the symbol at the end of its arc. One undo step.
    pub fn set_modifier_kind(
        &mut self,
        id: &str,
        species: &str,
        kind: ModifierKind,
    ) -> ViewResponse {
        let before = self.model.clone();
        if let Some(r) = self.model.find_reaction_mut(id) {
            for m in r.modifiers.iter_mut().filter(|m| m.species == species) {
                m.kind = kind;
            }
        }
        self.record_if_changed("Modifier Kind", before)
    }

    /// Whether the selected reactions are reversible, if they all agree.
    pub fn selected_reactions_reversible(&self) -> Option<bool> {
        let mut rev = self.model.selected_reactions().map(|r| r.is_reversible);
        let first = rev.next()?;
        rev.all(|v| v == first).then_some(first)
    }

    /// Sets the diagram's background colour (S §8.8); [`Rgba::DEFAULT`] puts
    /// the usual one back. One undo step.
    /// Whether the diagram's modifier lines are drawn (View ▸ Modifier lines,
    /// S §7.6). A setting of the diagram, saved in its file.
    pub fn show_modifiers(&self) -> bool {
        !self.model.hide_modifier_lines
    }

    /// Shows or hides the diagram's modifier lines. One undo step.
    pub fn set_show_modifiers(&mut self, show: bool) -> ViewResponse {
        let before = self.model.clone();
        self.model.hide_modifier_lines = !show;
        let desc = if show {
            "Show Modifier Lines"
        } else {
            "Hide Modifier Lines"
        };
        self.record_if_changed(desc, before)
    }

    pub fn set_background(&mut self, colour: Rgba) -> ViewResponse {
        let before = self.model.clone();
        self.model.background = colour;
        self.record_if_changed("Background Colour", before)
    }

    /// Sets the label font size of every selected species (0 means the
    /// default, 12). One undo step (S §8.1).
    pub fn set_font_size(&mut self, size: f64) -> ViewResponse {
        if !(size.is_finite() && size >= 0.0) {
            return ViewResponse::default();
        }
        self.style_edit(
            |s| {
                s.font_size = size;
                s.has_custom_style = true;
            },
            |_| {},
            true,
            false,
        )
    }

    /// Makes the names of the selected species (`species`), or of the
    /// selected reactions, bold or not (S §5.11.1). One undo step.
    pub fn set_name_bold(&mut self, species: bool, bold: bool) -> ViewResponse {
        self.name_face_edit(species, move |s| s.label_bold = bold)
    }

    /// Makes the names of the selected species (`species`), or of the
    /// selected reactions, italic or not (S §5.11.1). One undo step.
    pub fn set_name_italic(&mut self, species: bool, italic: bool) -> ViewResponse {
        self.name_face_edit(species, move |s| s.label_italic = italic)
    }

    fn name_face_edit(&mut self, species: bool, change: impl Fn(&mut VisualStyle)) -> ViewResponse {
        let change = |s: &mut VisualStyle| {
            change(s);
            s.has_custom_style = true;
        };
        if species {
            self.style_edit(change, |_| {}, true, false)
        } else {
            self.style_edit(|_| {}, change, false, true)
        }
    }

    /// Whether every selected species' (`species`) or reaction's name is
    /// bold, and whether every one is italic; `(false, false)` with none
    /// selected.
    pub fn selected_names_bold_italic(&self, species: bool) -> (bool, bool) {
        let styles: Vec<VisualStyle> = if species {
            self.model.selected_species().map(|s| s.style).collect()
        } else {
            self.model.selected_reactions().map(|r| r.style).collect()
        };
        let all = |f: fn(&VisualStyle) -> bool| {
            !styles.is_empty() && styles.iter().all(|s| s.has_custom_style && f(s))
        };
        (all(|s| s.label_bold), all(|s| s.label_italic))
    }

    /// Sets the name size of every selected reaction (0 means the default,
    /// 12). One undo step (S §5.12).
    pub fn set_reaction_font_size(&mut self, size: f64) -> ViewResponse {
        if !(size.is_finite() && size >= 0.0) {
            return ViewResponse::default();
        }
        self.style_edit(
            |_| {},
            |r| {
                r.font_size = size;
                r.has_custom_style = true;
            },
            false,
            true,
        )
    }

    /// Shows or hides the names of the selected reactions. A name shown
    /// starts beside its junction on the side clearest of its lines; one
    /// already shown stays where it is. One undo step (S §5.12).
    pub fn set_reaction_label_shown(&mut self, shown: bool) -> ViewResponse {
        let before = self.model.clone();
        let places: Vec<(String, Option<crate::geometry::Pt>)> = self
            .model
            .selected_reactions()
            .map(|r| {
                let place = match (shown, r.label_offset) {
                    (false, _) => None,
                    (true, Some(o)) => Some(o),
                    (true, None) => Some(crate::render::draw::clear_reaction_label_offset(
                        &self.model,
                        r,
                    )),
                };
                (r.id.clone(), place)
            })
            .collect();
        for (id, place) in places {
            if let Some(r) = self.model.find_reaction_mut(&id) {
                r.label_offset = place;
            }
        }
        let desc = if shown { "Show Label" } else { "Hide Label" };
        self.record_if_changed(desc, before)
    }

    /// Whether the selected reactions show their names, if they all agree.
    pub fn selected_reactions_label_shown(&self) -> Option<bool> {
        let mut shown = self
            .model
            .selected_reactions()
            .map(|r| r.label_offset.is_some());
        let first = shown.next()?;
        shown.all(|s| s == first).then_some(first)
    }

    /// Copy Style (S §8.5): remembers how `item` looks, a species, a reaction
    /// (by its junction or a leg), a compartment or a text, for Paste Style.
    /// Returns the id copied, or `None` if there is nothing there to copy. Not
    /// an edit: no undo step.
    pub fn copy_style(&mut self, item: &Target) -> Option<Id> {
        let (kind, id) = style_target(item)?;
        self.copied_style = Some(crate::styles::look_of(&self.model, kind, &id)?);
        Some(id)
    }

    /// Copy Style for the one item selected (null nodes aside), whatever its
    /// kind, as the Edit menu and its shortcut do. `None` unless exactly one
    /// is selected.
    pub fn copy_selected_style(&mut self) -> Option<Id> {
        let m = &self.model;
        let mut items: Vec<Target> = m
            .selected_species()
            .filter(|s| !s.is_null_node)
            .map(|s| Target::Species(s.id.clone()))
            .collect();
        items.extend(m.selected_reactions().map(|r| Target::Leg(r.id.clone())));
        items.extend(
            m.selected_compartments()
                .map(|c| Target::Compartment(c.id.clone())),
        );
        items.extend(m.selected_texts().map(|t| Target::Text(t.id.clone())));
        match items.as_slice() {
            [one] => {
                let one = one.clone();
                self.copy_style(&one)
            }
            _ => None,
        }
    }

    /// True if Paste Style has something to paste and something selected of
    /// the same kind to paste it on.
    pub fn can_paste_style(&self) -> bool {
        self.copied_style
            .is_some_and(|look| !crate::styles::selected_ids(&self.model, look.kind()).is_empty())
    }

    /// True if a style has been copied.
    pub fn has_copied_style(&self) -> bool {
        self.copied_style.is_some()
    }

    /// Paste Style (S §8.5): every selected item of the kind that was copied
    /// takes the copied look; selected items of other kinds are left alone. A
    /// species keeps its size, except that one made a circle takes its larger
    /// side both ways; a null node is left alone. One undo step.
    pub fn paste_style(&mut self) -> ViewResponse {
        let Some(look) = self.copied_style else {
            return ViewResponse::default();
        };
        let before = self.model.clone();
        for id in crate::styles::selected_ids(&self.model, look.kind()) {
            crate::styles::give_look(&mut self.model, &id, &look);
        }
        self.record_if_changed("Paste Style", before)
    }

    /// Sets the line width of reaction `id` (0 means the default). One undo step.
    pub fn set_line_width(&mut self, id: &str, width: f64) -> ViewResponse {
        if !(width.is_finite() && width >= 0.0) {
            return ViewResponse::default();
        }
        self.style_edit_one(id, false, |s| {
            s.line_width = width;
            s.has_custom_style = true;
        })
    }

    /// Shows or hides the junction of every selected reaction. A display
    /// toggle: it doesn't make the style custom (S §3.6). One undo step.
    pub fn set_junction_visible(&mut self, visible: bool) -> ViewResponse {
        self.style_edit(|_| {}, |s| s.junction_visible = visible, false, true)
    }

    /// True if every selected reaction shows its junction (S §8.1).
    pub fn all_junctions_visible(&self) -> bool {
        self.model
            .selected_reactions()
            .all(|r| r.style.junction_visible)
    }

    /// Changes the styles of the selected species and/or reactions, recording
    /// one StyleEdit with the styles before and after.
    fn style_edit(
        &mut self,
        species: impl Fn(&mut VisualStyle),
        reactions: impl Fn(&mut VisualStyle),
        do_species: bool,
        do_reactions: bool,
    ) -> ViewResponse {
        let mut sp = Vec::new();
        if do_species {
            for s in self.model.species_mut().filter(|s| s.selected) {
                let before = s.style;
                species(&mut s.style);
                if s.style != before {
                    sp.push((s.id.clone(), before, s.style));
                }
            }
        }
        let mut rx = Vec::new();
        if do_reactions {
            for r in self.model.reactions_mut().filter(|r| r.selected) {
                let before = r.style;
                reactions(&mut r.style);
                if r.style != before {
                    rx.push((r.id.clone(), before, r.style));
                }
            }
        }
        self.push_style_edit(sp, rx)
    }

    fn style_edit_one(
        &mut self,
        id: &str,
        species: bool,
        change: impl Fn(&mut VisualStyle),
    ) -> ViewResponse {
        let (mut sp, mut rx) = (Vec::new(), Vec::new());
        if species {
            if let Some(s) = self.model.find_species_mut(id) {
                let before = s.style;
                change(&mut s.style);
                if s.style != before {
                    sp.push((id.to_owned(), before, s.style));
                }
            }
        } else if let Some(r) = self.model.find_reaction_mut(id) {
            let before = r.style;
            change(&mut r.style);
            if r.style != before {
                rx.push((id.to_owned(), before, r.style));
            }
        }
        self.push_style_edit(sp, rx)
    }

    fn push_style_edit(
        &mut self,
        species: Vec<(Id, VisualStyle, VisualStyle)>,
        reactions: Vec<(Id, VisualStyle, VisualStyle)>,
    ) -> ViewResponse {
        if species.is_empty() && reactions.is_empty() {
            return ViewResponse::default();
        }
        // An item following a saved style stops following it (S §8.6). The
        // link is not part of a style edit, so the step is a snapshot.
        let linked = species.iter().any(|(id, ..)| {
            self.model
                .find_species(id)
                .is_some_and(|s| s.style_name.is_some())
        }) || reactions.iter().any(|(id, ..)| {
            self.model
                .find_reaction(id)
                .is_some_and(|r| r.style_name.is_some())
        });
        if linked {
            let mut before = self.model.clone();
            for (id, old, _) in &species {
                if let Some(s) = before.find_species_mut(id) {
                    s.style = *old;
                }
            }
            for (id, old, _) in &reactions {
                if let Some(r) = before.find_reaction_mut(id) {
                    r.style = *old;
                }
            }
            return self.record("Style", before);
        }
        self.undo.push(Command::StyleEdit { species, reactions });
        ViewResponse {
            repaint: true,
            model_changed: true,
            ..ViewResponse::default()
        }
    }

    // ----- Compartments (S §7.9) ---------------------------------------------

    /// Sets a compartment's volume (finite, not negative). One undo step.
    pub fn set_compartment_volume(&mut self, id: &str, volume: f64) -> ViewResponse {
        if !(volume.is_finite() && volume >= 0.0) {
            return ViewResponse::default();
        }
        let before = self.model.clone();
        if let Some(c) = self.model.find_compartment_mut(id) {
            c.size = volume;
        }
        self.record_if_changed("Edit Volume", before)
    }

    /// Sets a compartment's border width (0 means the default). One undo step.
    pub fn set_compartment_border_width(&mut self, id: &str, width: f64) -> ViewResponse {
        if !(width.is_finite() && width >= 0.0) {
            return ViewResponse::default();
        }
        let before = self.model.clone();
        // 0: no border (S §7.9); its width is kept for when it gets one.
        if let Some(c) = self.model.find_compartment_mut(id) {
            c.style.no_border = width == 0.0;
            if width > 0.0 {
                c.style.border_width = width;
                c.style.has_custom_style = true;
            }
        }
        self.record_if_changed("Border Width", before)
    }

    /// Sets the name size of every selected compartment (S §7.9; 0 means the
    /// default, 12). One undo step.
    pub fn set_compartment_font_size(&mut self, size: f64) -> ViewResponse {
        if !(size.is_finite() && size >= 0.0) {
            return ViewResponse::default();
        }
        self.on_selected_compartments("Font Size", |c| {
            c.style.font_size = size;
            c.style.has_custom_style = true;
        })
    }

    /// Renames a compartment, and every reference to it (S §7.9). One undo
    /// step.
    pub fn rename_compartment(&mut self, id: &str, new: &str) -> Result<ViewResponse, RenameError> {
        let before = self.model.clone();
        self.model.rename_compartment(id, new)?;
        Ok(self.record_if_changed("Rename Compartment", before))
    }

    /// Sets the width, the height, or both, of every selected species (null
    /// nodes aside), each about its centre, never below the smallest size. A
    /// circle takes either for both. One undo step (S §7.10).
    pub fn set_species_size(&mut self, width: Option<f64>, height: Option<f64>) -> ViewResponse {
        use crate::render::constants::MIN_SPECIES_SIZE;
        let before = self.model.clone();
        for s in self.model.species_mut() {
            if !s.selected || s.is_null_node {
                continue;
            }
            if let Some(w) = width.filter(|w| w.is_finite()) {
                let w = w.max(MIN_SPECIES_SIZE);
                let h = if s.shape == NodeShape::Circle {
                    w
                } else {
                    s.height
                };
                s.resize(w, h);
            }
            if let Some(h) = height.filter(|h| h.is_finite()) {
                let h = h.max(MIN_SPECIES_SIZE);
                let w = if s.shape == NodeShape::Circle {
                    h
                } else {
                    s.width
                };
                s.resize(w, h);
            }
        }
        self.record_if_changed("Resize Species", before)
    }

    /// Draws every selected species (null nodes aside) as `shape`. Made a
    /// circle, a species takes the larger of its width and height both ways,
    /// so its name still fits. One undo step (S §5.10).
    pub fn set_species_shape(&mut self, shape: NodeShape) -> ViewResponse {
        let before = self.model.clone();
        for s in self.model.species_mut() {
            if !s.selected || s.is_null_node {
                continue;
            }
            s.shape = shape;
            if shape == NodeShape::Circle {
                let d = s.width.max(s.height);
                s.resize(d, d);
            }
        }
        self.record_if_changed("Change Shape", before)
    }

    /// Gives every selected species (null nodes aside) this border style. One
    /// undo step (S §5.3).
    pub fn set_species_border_dash(&mut self, dash: BorderDash) -> ViewResponse {
        let before = self.model.clone();
        for s in self.model.species_mut() {
            if s.selected && !s.is_null_node {
                s.border_dash = dash;
            }
        }
        self.record_if_changed("Change Border Style", before)
    }

    /// The border style every selected species (null nodes aside) has, if
    /// they all have the same one.
    pub fn selected_species_border_dash(&self) -> Option<BorderDash> {
        let mut dashes = self
            .model
            .species()
            .iter()
            .filter(|s| s.selected && !s.is_null_node)
            .map(|s| s.border_dash);
        let first = dashes.next()?;
        dashes.all(|d| d == first).then_some(first)
    }

    /// Draws the names of the selected species (null nodes aside) outside
    /// their nodes or inside them. A name taken outside starts just right of
    /// its node; one already outside stays where it is. One undo step
    /// (S §5.11).
    pub fn set_species_label_outside(&mut self, outside: bool) -> ViewResponse {
        let before = self.model.clone();
        let places: Vec<(String, Option<crate::geometry::Pt>)> = self
            .model
            .species()
            .iter()
            .filter(|s| s.selected && !s.is_null_node)
            .map(|s| {
                let place = crate::styles::label_place(&self.model, s, outside);
                (s.id.clone(), place)
            })
            .collect();
        for (id, place) in places {
            if let Some(s) = self.model.find_species_mut(&id) {
                s.label_offset = place;
            }
        }
        let desc = if outside {
            "Label Outside"
        } else {
            "Label Inside"
        };
        self.record_if_changed(desc, before)
    }

    /// Whether the selected species (null nodes aside) have their names
    /// outside, if they all agree.
    pub fn selected_species_label_outside(&self) -> Option<bool> {
        let mut outside = self
            .model
            .species()
            .iter()
            .filter(|s| s.selected && !s.is_null_node)
            .map(|s| s.label_offset.is_some());
        let first = outside.next()?;
        outside.all(|o| o == first).then_some(first)
    }

    /// The shape every selected species (null nodes aside) has, if they all
    /// have the same one.
    pub fn selected_species_shape(&self) -> Option<NodeShape> {
        let mut shapes = self
            .model
            .species()
            .iter()
            .filter(|s| s.selected && !s.is_null_node)
            .map(|s| s.shape);
        let first = shapes.next()?;
        shapes.all(|s| s == first).then_some(first)
    }

    /// Fit to Name: each selected species (null nodes aside, and those with
    /// their name outside) is made as wide as its name and a small margin
    /// each side, in whole units; the height stays. One undo step (S §7.10).
    pub fn fit_species_to_names(&mut self) -> ViewResponse {
        use crate::render::constants::MIN_SPECIES_SIZE;
        let before = self.model.clone();
        for s in self.model.species_mut() {
            if s.selected && !s.is_null_node && s.label_offset.is_none() {
                s.width = crate::render::draw::width_for_name(s)
                    .ceil()
                    .max(MIN_SPECIES_SIZE);
                if s.shape == NodeShape::Circle {
                    s.height = s.width;
                }
            }
        }
        self.record_if_changed("Fit to Name", before)
    }

    /// Shows or hides the boxes of the selected compartments (S §7.9). The
    /// default compartment is never drawn, so it is left alone.
    pub fn set_compartment_hidden(&mut self, hidden: bool) -> ViewResponse {
        let desc = if hidden {
            "Hide Compartment"
        } else {
            "Show Compartment"
        };
        self.on_selected_compartments(desc, |c| {
            if !c.is_default() {
                c.hidden = hidden;
            }
        })
    }

    /// Runs `change` on every selected compartment: one undo step, if anything
    /// changed.
    fn on_selected_compartments(
        &mut self,
        desc: &'static str,
        change: impl Fn(&mut Compartment),
    ) -> ViewResponse {
        let before = self.model.clone();
        self.model
            .compartments
            .iter_mut()
            .filter(|c| c.selected)
            .for_each(change);
        self.record_if_changed(desc, before)
    }

    // ----- Free text (S §7.8) ------------------------------------------------

    /// Sets the words of text `id`, as they are typed. A run of changes to
    /// the same text, with nothing else in between, is one undo step; so is
    /// adding a text and then typing its words.
    pub fn set_text(&mut self, id: &str, text: &str) -> ViewResponse {
        let before = self.model.clone();
        let Some(t) = self.model.find_text_mut(id) else {
            return ViewResponse::default();
        };
        t.text = text.to_owned();
        if self.model == before {
            return ViewResponse::default();
        }
        let same_run = self.text_edited.as_ref() == Some(&(self.undo.version(), id.to_owned()))
            && matches!(self.undo.undo_description(), Some("Edit Text" | "Add Text"));
        let response = if same_run && self.undo.amend_last(self.model.clone()) {
            ViewResponse {
                repaint: true,
                model_changed: true,
                ..ViewResponse::default()
            }
        } else {
            self.record("Edit Text", before)
        };
        self.text_edited = Some((self.undo.version(), id.to_owned()));
        response
    }

    /// Sets the width text `id` wraps at (0: no wrapping; otherwise at least
    /// the narrowest wrap). One undo step.
    pub fn set_text_wrap(&mut self, id: &str, width: f64) -> ViewResponse {
        if !width.is_finite() {
            return ViewResponse::default();
        }
        let width = if width <= 0.0 {
            0.0
        } else {
            width.max(crate::textbox::MIN_WRAP_WIDTH)
        };
        let before = self.model.clone();
        if let Some(t) = self.model.find_text_mut(id) {
            t.wrap_width = width;
        }
        self.record_if_changed("Wrap Text", before)
    }

    /// Sets the size of the selected texts. Sizes below 1 are ignored.
    pub fn set_text_size(&mut self, size: f64) -> ViewResponse {
        if !(size.is_finite() && size >= 1.0) {
            return ViewResponse::default();
        }
        self.on_selected_texts("Text Size", |t| t.size = size)
    }

    pub fn set_text_typeface(&mut self, typeface: Typeface) -> ViewResponse {
        self.on_selected_texts("Typeface", |t| t.font.typeface = typeface)
    }

    pub fn set_text_align(&mut self, align: TextAlign) -> ViewResponse {
        self.on_selected_texts("Align Text", |t| t.align = align)
    }

    pub fn set_text_bold(&mut self, bold: bool) -> ViewResponse {
        self.on_selected_texts("Bold", |t| t.font.bold = bold)
    }

    pub fn set_text_italic(&mut self, italic: bool) -> ViewResponse {
        self.on_selected_texts("Italic", |t| t.font.italic = italic)
    }

    /// Runs `change` on every selected text: one undo step, if anything changed.
    fn on_selected_texts(
        &mut self,
        desc: &'static str,
        change: impl Fn(&mut TextItem),
    ) -> ViewResponse {
        let before = self.model.clone();
        self.model
            .texts_mut()
            .filter(|t| t.selected)
            .for_each(change);
        self.record_if_changed(desc, before)
    }

    /// Records a change since `before` as one undo step, or nothing if the
    /// model is the same.
    pub(super) fn record_if_changed(&mut self, desc: &'static str, before: Model) -> ViewResponse {
        if self.model == before {
            return ViewResponse::default();
        }
        self.record(desc, before)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::pt;
    use crate::model::Participant;
    use crate::render::constants::{ALIAS_BORDER, ALIAS_FILL, REACTION_LINE, SPECIES_BORDER};
    use crate::view::{Button, Modifiers, ViewEvent};

    /// S → P with E and Km-free species X around; R5 selected.
    fn view() -> DiagramView {
        let mut m = Model::new();
        let s = m.add_species(pt(0.0, 0.0), 80.0, 36.0);
        let p = m.add_species(pt(400.0, 0.0), 80.0, 36.0);
        m.add_species(pt(200.0, -200.0), 80.0, 36.0);
        m.rename_species("S3", "E").unwrap();
        m.add_species(pt(200.0, 200.0), 80.0, 36.0);
        let r = m.add_reaction(pt(200.0, 0.0));
        let rr = m.find_reaction_mut(&r).unwrap();
        rr.reactants.push(Participant::new(s, 1.0));
        rr.products.push(Participant::new(p, 1.0));
        rr.selected = true;
        DiagramView::new(m)
    }

    fn modifiers(v: &DiagramView) -> Vec<String> {
        v.model().reactions()[0]
            .modifiers
            .iter()
            .map(|m| m.species.clone())
            .collect()
    }

    #[test]
    fn a_display_name_is_set_on_the_primary_and_undone_in_one_step() {
        let mut v = view();
        let alias = v.model.add_alias("S1", pt(0.0, 100.0)).unwrap();
        v.set_kinetic_law("R5", "k*S1");
        let before = v.model().clone();
        v.set_display_name(&alias, "  glucose ");
        assert_eq!(v.undo_description(), Some("Edit Display Name"));
        let m = v.model();
        assert_eq!(
            m.find_species("S1").unwrap().display_name.as_deref(),
            Some("glucose")
        );
        assert_eq!(m.find_species(&alias).unwrap().display_name, None);
        // Only the name changed: not the id, not the law.
        assert_eq!(m.reactions()[0].kinetic_law, "k*S1");
        let mut named = before.clone();
        named.find_species_mut("S1").unwrap().display_name = Some("glucose".into());
        assert_eq!(*m, named);
        v.undo();
        // As saved: undo clears the selection, which isn't.
        assert_eq!(
            crate::json::to_json(v.model()),
            crate::json::to_json(&before)
        );
    }

    #[test]
    fn a_display_name_goes_to_a_reaction_or_compartment_and_blank_removes_it() {
        let mut v = view();
        v.model
            .compartments
            .push(crate::model::Compartment::new("cell"));
        v.set_display_name("R5", "uptake");
        v.set_display_name("cell", "astrocyte");
        assert_eq!(
            v.model()
                .find_reaction("R5")
                .unwrap()
                .display_name
                .as_deref(),
            Some("uptake")
        );
        assert_eq!(
            v.model()
                .find_compartment("cell")
                .unwrap()
                .display_name
                .as_deref(),
            Some("astrocyte")
        );
        v.set_display_name("R5", "   ");
        assert_eq!(v.model().find_reaction("R5").unwrap().display_name, None);
        // The same name again is no step at all.
        let steps = v.undo_description();
        v.set_display_name("cell", "astrocyte");
        assert_eq!(v.undo_description(), steps);
    }

    /// D4 of docs/display-names-design.md: the labels a view shows.
    fn labels(v: &DiagramView) -> Vec<String> {
        let m = v.model();
        m.species()
            .iter()
            .map(|s| s.label().to_owned())
            .chain(m.reactions().iter().map(|r| r.label().to_owned()))
            .chain(m.compartments.iter().map(|c| c.label().to_owned()))
            .collect()
    }

    /// S1 (glucose) with an alias, S2 without a name, E, S4, R5 (uptake),
    /// and a compartment `cell` (astrocyte). Nothing selected.
    fn named_view() -> DiagramView {
        let mut v = view();
        v.model
            .compartments
            .push(crate::model::Compartment::new("cell"));
        v.model.add_alias("S1", pt(0.0, 100.0)).unwrap();
        v.set_display_name("S1", "glucose");
        v.set_display_name("R5", "uptake");
        v.set_display_name("cell", "astrocyte");
        v.model.clear_selection();
        v
    }

    #[test]
    fn labels_show_ids_until_the_diagram_shows_display_names() {
        let mut v = named_view();
        assert_eq!(labels(&v), ["S1", "S2", "E", "S4", "S1", "R5", "cell"]);
        v.set_show_display_names(true);
        assert_eq!(v.undo_description(), Some("Show Display Names"));
        // The alias shows its species' name; S2 has none, so shows its id.
        assert_eq!(
            labels(&v),
            ["glucose", "S2", "E", "S4", "glucose", "uptake", "astrocyte"]
        );
        v.undo();
        assert_eq!(labels(&v)[0], "S1");
        v.redo();
        assert_eq!(labels(&v)[0], "glucose");
        // A name typed while names are shown shows at once.
        v.set_display_name("S2", "fructose");
        assert_eq!(labels(&v)[1], "fructose");
    }

    #[test]
    fn an_items_label_choice_overrides_the_diagram() {
        let mut v = named_view();
        // The alias alone shows its name; the primary keeps its id.
        let alias = v.model().species()[4].id.clone();
        v.model.find_species_mut(&alias).unwrap().selected = true;
        v.set_label_choice(StyleKind::Species, LabelChoice::DisplayName);
        assert_eq!(v.undo_description(), Some("Label"));
        assert_eq!(labels(&v)[0], "S1");
        assert_eq!(labels(&v)[4], "glucose");
        // With the diagram showing names, a reaction set to its id keeps it.
        v.set_show_display_names(true);
        v.model.clear_selection();
        v.model.find_reaction_mut("R5").unwrap().selected = true;
        v.set_label_choice(StyleKind::Reaction, LabelChoice::Id);
        assert_eq!(labels(&v)[5], "R5");
        assert_eq!(
            v.selected_label_choice(StyleKind::Reaction),
            Some(LabelChoice::Id)
        );
        // Saved and read back: the setting and the choices come too.
        let text = crate::json::to_json(v.model());
        let back = DiagramView::new(crate::json::from_json(&text).unwrap().model);
        assert_eq!(labels(&back), labels(&v));
    }

    #[test]
    fn the_selection_s_label_choice_is_none_when_mixed() {
        let mut v = named_view();
        for s in v.model.species_mut() {
            s.selected = true;
        }
        assert_eq!(
            v.selected_label_choice(StyleKind::Species),
            Some(LabelChoice::Diagram)
        );
        v.model.find_species_mut("S1").unwrap().label_choice = LabelChoice::Id;
        assert_eq!(v.selected_label_choice(StyleKind::Species), None);
    }

    #[test]
    fn opening_a_model_labels_it() {
        let mut m = Model::new();
        let s = m.add_species(pt(0.0, 0.0), 80.0, 36.0);
        m.find_species_mut(&s).unwrap().display_name = Some("glucose".into());
        m.show_display_names = true;
        // Not refreshed yet: as an import hands it over.
        assert_eq!(m.species()[0].label(), s);
        let v = DiagramView::new(m.clone());
        assert_eq!(v.model().species()[0].label(), "glucose");
        let mut v = DiagramView::new(Model::new());
        v.set_model(m);
        assert_eq!(v.model().species()[0].label(), "glucose");
    }

    #[test]
    fn fit_to_name_measures_the_label_shown() {
        let mut v = named_view();
        let id_width = crate::render::draw::width_for_name(&v.model().species()[0]);
        v.set_show_display_names(true);
        let name_width = crate::render::draw::width_for_name(&v.model().species()[0]);
        assert!(name_width > id_width, "{name_width} > {id_width}");
    }

    #[test]
    fn typing_a_law_adds_and_removes_modifier_arcs() {
        // M7 acceptance.
        let mut v = view();
        v.set_kinetic_law("R5", "Vm*S1/(Km + S1)*E");
        assert_eq!(modifiers(&v), ["E"]);
        // Parameters, reactants and products are not modifiers.
        v.set_kinetic_law("R5", "Vm*S1/(Km + S1)");
        assert!(modifiers(&v).is_empty());
        v.set_kinetic_law("R5", "k*E*S4*E");
        assert_eq!(modifiers(&v), ["E", "S4"]);
        assert_eq!(v.undo_description(), Some("Edit Rate Law"));
        v.undo();
        assert!(modifiers(&v).is_empty());
    }

    #[test]
    fn a_law_that_doesnt_parse_is_kept_and_leaves_modifiers_alone() {
        let mut v = view();
        v.set_kinetic_law("R5", "k*E");
        v.set_kinetic_law("R5", "k*(E + ");
        assert_eq!(v.model().reactions()[0].kinetic_law, "k*(E + ");
        assert_eq!(modifiers(&v), ["E"]);
        // An empty law removes them all.
        v.set_kinetic_law("R5", "  ");
        assert!(modifiers(&v).is_empty());
    }

    #[test]
    fn modifiers_are_matched_by_primary() {
        let mut v = view();
        let mut m = v.model().clone();
        let alias = m.add_alias("E", pt(300.0, -200.0)).unwrap();
        m.find_reaction_mut("R5")
            .unwrap()
            .modifiers
            .push(Participant::new(alias.as_str(), 1.0));
        v.set_model(m);
        // The law names E; the alias modifier stands for E, so it stays and
        // no second E is added.
        v.set_kinetic_law("R5", "k*E");
        assert_eq!(modifiers(&v), [alias]);
    }

    #[test]
    fn unchanged_values_record_nothing() {
        let mut v = view();
        v.set_kinetic_law("R5", "");
        assert!(!v.can_undo());
        v.set_initial_value("S1", 0.0);
        assert!(!v.can_undo());
    }

    #[test]
    fn concentration_and_boundary() {
        let mut v = view();
        let alias = {
            let mut m = v.model().clone();
            let a = m.add_alias("S1", pt(0.0, 100.0)).unwrap();
            v.set_model(m);
            a
        };
        // An alias edits its primary.
        v.set_initial_value(&alias, 2.5);
        assert_eq!(v.model().find_species("S1").unwrap().initial_value, 2.5);
        assert_eq!(v.undo_description(), Some("Edit Concentration"));
        v.select_all();
        v.set_boundary(true);
        assert!(v.model().find_species("S1").unwrap().is_boundary);
        assert!(
            !v.model().find_species(&alias).unwrap().is_boundary,
            "the primary holds it"
        );
        v.undo();
        assert!(!v.model().find_species("S1").unwrap().is_boundary);
        assert!(!v.set_initial_value("S1", f64::NAN).model_changed);
    }

    #[test]
    fn colour_applies_to_every_selected_item_as_one_step() {
        let mut v = view();
        v.select_all();
        let c = Rgba::rgb(0xFF, 0xE0, 0xB2);
        v.apply_colour(ColourField::Fill, c);
        assert!(
            v.model()
                .species()
                .iter()
                .all(|s| s.style.fill_color == c && s.style.has_custom_style)
        );
        assert_eq!(v.undo_description(), Some("Style"));
        // Only the fill: the border stays the default (M7 acceptance).
        assert_eq!(v.swatch_colour(ColourField::Border), Some(SPECIES_BORDER));
        v.undo();
        assert!(
            v.model()
                .species()
                .iter()
                .all(|s| !s.style.has_custom_style)
        );
        assert!(!v.can_undo(), "one step for all four species");
    }

    #[test]
    fn line_colour_goes_to_reactions_only() {
        let mut v = view();
        v.select_all();
        v.apply_colour(ColourField::Line, Rgba::rgb(1, 2, 3));
        assert_eq!(
            v.model().reactions()[0].style.line_color,
            Rgba::rgb(1, 2, 3)
        );
        assert!(
            v.model()
                .species()
                .iter()
                .all(|s| !s.style.has_custom_style)
        );
        assert_eq!(v.swatch_colour(ColourField::Line), Some(Rgba::rgb(1, 2, 3)));
    }

    #[test]
    fn swatches_show_effective_colours() {
        let mut v = view();
        assert_eq!(
            v.swatch_colour(ColourField::Fill),
            None,
            "no species selected"
        );
        assert_eq!(v.swatch_colour(ColourField::Line), Some(REACTION_LINE));
        let mut m = v.model().clone();
        let a = m.add_alias("S1", pt(0.0, 100.0)).unwrap();
        m.find_species_mut(&a).unwrap().selected = true;
        v.set_model(m);
        assert_eq!(v.swatch_colour(ColourField::Fill), Some(ALIAS_FILL));
        assert_eq!(v.swatch_colour(ColourField::Border), Some(ALIAS_BORDER));
    }

    #[test]
    fn widths_and_junction_visibility() {
        let mut v = view();
        v.set_border_width("S1", 3.0);
        let s = &v.model().find_species("S1").unwrap().style;
        assert!(s.has_custom_style && s.border_width == 3.0);
        v.set_line_width("R5", 2.0);
        assert_eq!(v.model().reactions()[0].style.line_width, 2.0);
        assert!(!v.set_line_width("R5", -1.0).model_changed);
        v.set_junction_visible(false);
        let st = v.model().reactions()[0].style;
        assert!(!st.junction_visible);
        assert!(!v.all_junctions_visible());
        // Undo each, in turn.
        v.undo();
        assert!(v.all_junctions_visible());
        v.undo();
        assert_eq!(v.model().reactions()[0].style.line_width, 0.0);
        v.undo();
        assert!(!v.model().find_species("S1").unwrap().style.has_custom_style);
    }

    #[test]
    fn junction_visibility_doesnt_make_a_style_custom() {
        let mut v = view();
        v.set_junction_visible(false);
        assert!(!v.model().reactions()[0].style.has_custom_style);
        // The box is ticked only if every selected reaction shows its junction.
        v.handle(&ViewEvent::PointerDown {
            pos: pt(0.0, 0.0),
            button: Button::Primary,
            modifiers: Modifiers::NONE,
        });
        assert!(v.all_junctions_visible(), "no reactions selected");
    }
}
