//! Saved styles (S §8.6): looks kept under a name, in the model and in My
//! Styles (the app's own list), given to items, which then follow them;
//! changing a style changes every item that follows it.
//!
//! Changes to the model are undo steps; changes to My Styles are not, as for
//! preferences.

use super::DiagramView;
use super::properties::style_target;
use crate::model::{Look, SavedStyle, StyleKind};
use crate::styles::{self, StyleEntry};
use crate::view::{Target, ViewResponse};

/// Why a style couldn't be saved or renamed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StyleError {
    /// The name is empty, or only spaces.
    EmptyName,
    /// Another style of the same kind has this name, in the model or in My
    /// Styles.
    NameTaken(String),
    /// There is no item there with a look (a null node, empty canvas).
    NoItem,
    /// Neither the model nor My Styles has a style of that kind with this
    /// name.
    NoStyle(String),
}

impl std::fmt::Display for StyleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StyleError::EmptyName => f.write_str("a style needs a name"),
            StyleError::NameTaken(n) => write!(f, "there is already a style named “{n}”"),
            StyleError::NoItem => f.write_str("there is nothing there to take a style from"),
            StyleError::NoStyle(n) => write!(f, "there is no style named “{n}”"),
        }
    }
}

impl std::error::Error for StyleError {}

impl DiagramView {
    // ----- My Styles ----------------------------------------------------------

    /// My Styles, in the order they were saved.
    pub fn my_styles(&self) -> &[SavedStyle] {
        &self.my_styles
    }

    /// Replaces My Styles, as the app does when it starts.
    pub fn set_my_styles(&mut self, styles: Vec<SavedStyle>) {
        self.my_styles = styles;
    }

    /// Keep in My Styles: the model's style `name` is copied to My Styles,
    /// replacing one there of the same name. Not an undo step.
    pub fn keep_in_my_styles(&mut self, kind: StyleKind, name: &str) -> bool {
        let Some(st) = styles::find(&self.model, kind, name).cloned() else {
            return false;
        };
        styles::put(&mut self.my_styles, st);
        true
    }

    /// Removes the style `name` from My Styles; the model keeps its own. Not
    /// an undo step.
    pub fn remove_from_my_styles(&mut self, kind: StyleKind, name: &str) {
        self.my_styles
            .retain(|s| !(s.kind() == kind && s.name == name));
    }

    // ----- Reading ------------------------------------------------------------

    /// The styles of `kind` to choose from: the model's, then those of My
    /// Styles the model doesn't have.
    pub fn style_entries(&self, kind: StyleKind) -> Vec<StyleEntry> {
        styles::entries(&self.model, &self.my_styles, kind)
    }

    /// The names of the model's own styles of `kind`, in the order they were
    /// saved.
    pub fn style_names(&self, kind: StyleKind) -> Vec<String> {
        self.model
            .styles
            .iter()
            .filter(|s| s.kind() == kind)
            .map(|s| s.name.clone())
            .collect()
    }

    /// The name of the style the item at `item` follows.
    pub fn style_of(&self, item: &Target) -> Option<String> {
        let (kind, id) = style_target(item)?;
        styles::link(&self.model, kind, &id)
    }

    /// The style every selected item of `kind` follows, if there are some and
    /// they all follow the same one.
    pub fn selected_style_name(&self, kind: StyleKind) -> Option<String> {
        let ids = styles::selected_ids(&self.model, kind);
        let mut names = ids.iter().map(|id| styles::link(&self.model, kind, id));
        let first = names.next()??;
        names.all(|n| n.as_deref() == Some(&first)).then_some(first)
    }

    /// True if some item of `kind` is selected (null nodes aside), so a
    /// style of that kind can be applied.
    pub fn can_apply_style(&self, kind: StyleKind) -> bool {
        !styles::selected_ids(&self.model, kind).is_empty()
    }

    /// The look the style `name` of `kind` has here: the model's, or My
    /// Styles'.
    fn look_named(&self, kind: StyleKind, name: &str) -> Option<Look> {
        styles::find(&self.model, kind, name)
            .or_else(|| styles::find_in(&self.my_styles, kind, name))
            .map(|s| s.look)
    }

    // ----- Commands -----------------------------------------------------------

    /// Save Style: the look of the item at `item` is saved as `name`, in the
    /// model and in My Styles, and the item follows it. A name its kind has
    /// in either is refused. One undo step.
    pub fn save_style(&mut self, item: &Target, name: &str) -> Result<ViewResponse, StyleError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(StyleError::EmptyName);
        }
        let (kind, id) = style_target(item).ok_or(StyleError::NoItem)?;
        let look = styles::look_of(&self.model, kind, &id).ok_or(StyleError::NoItem)?;
        if self.look_named(kind, name).is_some() {
            return Err(StyleError::NameTaken(name.to_owned()));
        }
        let style = SavedStyle {
            name: name.to_owned(),
            look,
        };
        let before = self.model.clone();
        self.model.styles.push(style.clone());
        styles::set_link(&mut self.model, kind, &id, Some(name.to_owned()));
        self.my_styles.push(style);
        Ok(self.record("Save Style", before))
    }

    /// Apply Style: every selected item of `kind` takes the style `name` and
    /// follows it; selected items of other kinds are left alone. A style only
    /// in My Styles is copied into the model first. One undo step.
    pub fn apply_style(&mut self, kind: StyleKind, name: &str) -> ViewResponse {
        let Some(look) = self.look_named(kind, name) else {
            return ViewResponse::default();
        };
        if !self.can_apply_style(kind) {
            return ViewResponse::default();
        }
        let before = self.model.clone();
        if styles::find(&self.model, kind, name).is_none() {
            self.model.styles.push(SavedStyle {
                name: name.to_owned(),
                look,
            });
        }
        for id in styles::selected_ids(&self.model, kind) {
            styles::give_look(&mut self.model, &id, &look);
            styles::set_link(&mut self.model, kind, &id, Some(name.to_owned()));
        }
        self.record_if_changed("Apply Style", before)
    }

    /// Change Style to Match This: the style `name` takes the look of the
    /// item at `item`, which then follows it, and every item following it
    /// changes with it. The model gets the style if it hadn't it; My Styles'
    /// copy changes too, if there is one. One undo step.
    pub fn update_style(&mut self, name: &str, item: &Target) -> Result<ViewResponse, StyleError> {
        let (kind, id) = style_target(item).ok_or(StyleError::NoItem)?;
        let look = styles::look_of(&self.model, kind, &id).ok_or(StyleError::NoItem)?;
        let style = SavedStyle {
            name: name.to_owned(),
            look,
        };
        let before = self.model.clone();
        let users = styles::users(&self.model, kind, name);
        styles::put(&mut self.model.styles, style.clone());
        for user in users {
            styles::give_look(&mut self.model, &user, &look);
        }
        styles::set_link(&mut self.model, kind, &id, Some(name.to_owned()));
        if styles::find_in(&self.my_styles, kind, name).is_some() {
            styles::put(&mut self.my_styles, style);
        }
        Ok(self.record_if_changed("Change Style", before))
    }

    /// Renames the style `old` of `kind` to `new`, in the model and in My
    /// Styles, wherever it is; items following it follow it under its new
    /// name. A name its kind has in either is refused. The model's part is one
    /// undo step.
    pub fn rename_style(
        &mut self,
        kind: StyleKind,
        old: &str,
        new: &str,
    ) -> Result<ViewResponse, StyleError> {
        let new = new.trim();
        if new.is_empty() {
            return Err(StyleError::EmptyName);
        }
        if self.look_named(kind, old).is_none() {
            return Err(StyleError::NoStyle(old.to_owned()));
        }
        if new == old {
            return Ok(ViewResponse::default());
        }
        if self.look_named(kind, new).is_some() {
            return Err(StyleError::NameTaken(new.to_owned()));
        }
        for st in self
            .my_styles
            .iter_mut()
            .filter(|s| s.kind() == kind && s.name == old)
        {
            st.name = new.to_owned();
        }
        let before = self.model.clone();
        for id in styles::users(&self.model, kind, old) {
            styles::set_link(&mut self.model, kind, &id, Some(new.to_owned()));
        }
        for st in self
            .model
            .styles
            .iter_mut()
            .filter(|s| s.kind() == kind && s.name == old)
        {
            st.name = new.to_owned();
        }
        Ok(self.record_if_changed("Rename Style", before))
    }

    /// Deletes the style `name` of `kind` from the model; My Styles keeps its
    /// own. The items that followed it keep their look. One undo step.
    pub fn delete_style(&mut self, kind: StyleKind, name: &str) -> ViewResponse {
        let before = self.model.clone();
        self.model
            .styles
            .retain(|s| !(s.kind() == kind && s.name == name));
        // Their links now point nowhere, so recording drops them.
        self.record_if_changed("Delete Style", before)
    }

    /// Selects exactly the items following the style `name` of `kind`.
    pub fn select_style_users(&mut self, kind: StyleKind, name: &str) -> ViewResponse {
        let users = styles::users(&self.model, kind, name);
        let mut first = true;
        let target = |id: String| match kind {
            StyleKind::Species => Target::Species(id),
            StyleKind::Reaction => Target::Leg(id),
            StyleKind::Compartment => Target::Compartment(id),
            StyleKind::Text => Target::Text(id),
        };
        let mut r = ViewResponse::default();
        if users.is_empty() {
            self.model.clear_selection();
        }
        for id in users {
            r = self.select_item(&target(id), !first);
            first = false;
        }
        ViewResponse {
            repaint: true,
            selection_changed: true,
            ..r
        }
    }
}
