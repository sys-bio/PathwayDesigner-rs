//! Saved styles (S §8.6): looks kept in the model under a name, given to
//! items, which then follow them; changing a style changes every item that
//! follows it.

use super::DiagramView;
use super::properties::style_target;
use crate::model::{SavedStyle, StyleKind};
use crate::styles;
use crate::view::{Target, ViewResponse};

/// Why a style couldn't be saved, renamed or updated.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StyleError {
    /// The name is empty, or only spaces.
    EmptyName,
    /// Another style of the same kind has this name.
    NameTaken(String),
    /// There is no item there with a look (a null node, empty canvas).
    NoItem,
    /// The model has no style of that kind with this name.
    NoStyle(String),
    /// The item is of another kind than the style.
    WrongKind,
}

impl std::fmt::Display for StyleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StyleError::EmptyName => f.write_str("a style needs a name"),
            StyleError::NameTaken(n) => write!(f, "there is already a style named “{n}”"),
            StyleError::NoItem => f.write_str("there is nothing there to take a style from"),
            StyleError::NoStyle(n) => write!(f, "there is no style named “{n}”"),
            StyleError::WrongKind => f.write_str("that style is for another kind of item"),
        }
    }
}

impl std::error::Error for StyleError {}

impl DiagramView {
    /// The names of the model's styles of `kind`, in the order they were
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

    /// Save Style: the look of the item at `item` is saved as `name`, and the
    /// item follows it. One undo step.
    pub fn save_style(&mut self, item: &Target, name: &str) -> Result<ViewResponse, StyleError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(StyleError::EmptyName);
        }
        let (kind, id) = style_target(item).ok_or(StyleError::NoItem)?;
        let look = styles::look_of(&self.model, kind, &id).ok_or(StyleError::NoItem)?;
        if styles::find(&self.model, kind, name).is_some() {
            return Err(StyleError::NameTaken(name.to_owned()));
        }
        let before = self.model.clone();
        self.model.styles.push(SavedStyle {
            name: name.to_owned(),
            look,
        });
        styles::set_link(&mut self.model, kind, &id, Some(name.to_owned()));
        Ok(self.record("Save Style", before))
    }

    /// Apply Style: every selected item of `kind` takes the style `name` and
    /// follows it; selected items of other kinds are left alone. One undo
    /// step.
    pub fn apply_style(&mut self, kind: StyleKind, name: &str) -> ViewResponse {
        let Some(look) = styles::find(&self.model, kind, name).map(|s| s.look) else {
            return ViewResponse::default();
        };
        let before = self.model.clone();
        for id in styles::selected_ids(&self.model, kind) {
            styles::give_look(&mut self.model, &id, &look);
            styles::set_link(&mut self.model, kind, &id, Some(name.to_owned()));
        }
        self.record_if_changed("Apply Style", before)
    }

    /// Update Style: the style `name` takes the look of the item at `item`,
    /// which then follows it, and every item following it changes with it.
    /// One undo step.
    pub fn update_style(&mut self, name: &str, item: &Target) -> Result<ViewResponse, StyleError> {
        let (kind, id) = style_target(item).ok_or(StyleError::NoItem)?;
        let look = styles::look_of(&self.model, kind, &id).ok_or(StyleError::NoItem)?;
        if styles::find(&self.model, kind, name).is_none() {
            let other_kind = self.model.styles.iter().any(|s| s.name == name);
            return Err(if other_kind {
                StyleError::WrongKind
            } else {
                StyleError::NoStyle(name.to_owned())
            });
        }
        let before = self.model.clone();
        let users = styles::users(&self.model, kind, name);
        if let Some(st) = self
            .model
            .styles
            .iter_mut()
            .find(|s| s.kind() == kind && s.name == name)
        {
            st.look = look;
        }
        for user in users {
            styles::give_look(&mut self.model, &user, &look);
        }
        styles::set_link(&mut self.model, kind, &id, Some(name.to_owned()));
        Ok(self.record_if_changed("Change Style", before))
    }

    /// Renames the style `old` of `kind` to `new`; the items following it
    /// follow it under its new name. One undo step.
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
        if styles::find(&self.model, kind, old).is_none() {
            return Err(StyleError::NoStyle(old.to_owned()));
        }
        if new == old {
            return Ok(ViewResponse::default());
        }
        if styles::find(&self.model, kind, new).is_some() {
            return Err(StyleError::NameTaken(new.to_owned()));
        }
        let before = self.model.clone();
        for id in styles::users(&self.model, kind, old) {
            styles::set_link(&mut self.model, kind, &id, Some(new.to_owned()));
        }
        if let Some(st) = self
            .model
            .styles
            .iter_mut()
            .find(|s| s.kind() == kind && s.name == old)
        {
            st.name = new.to_owned();
        }
        Ok(self.record("Rename Style", before))
    }

    /// Deletes the style `name` of `kind` from the model. The items that
    /// followed it keep their look. One undo step.
    pub fn delete_style(&mut self, kind: StyleKind, name: &str) -> ViewResponse {
        let before = self.model.clone();
        self.model
            .styles
            .retain(|s| !(s.kind() == kind && s.name == name));
        // Their links now point nowhere, so recording drops them.
        self.record_if_changed("Delete Style", before)
    }
}
