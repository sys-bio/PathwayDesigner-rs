//! The view's own input events (SPEC-Rust §2.1).
//!
//! The app turns egui input into these, using only `pd-core` types, so the
//! interaction state machine can be tested without a window.

use crate::geometry::Pt;
use crate::model::Id;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Button {
    Primary,
    Secondary,
    Middle,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Modifiers {
    pub shift: bool,
    /// Ctrl, or Cmd on macOS.
    pub command: bool,
    pub alt: bool,
}

impl Modifiers {
    pub const NONE: Modifiers = Modifiers {
        shift: false,
        command: false,
        alt: false,
    };
    pub const SHIFT: Modifiers = Modifiers {
        shift: true,
        command: false,
        alt: false,
    };
}

/// Pointer positions are in screen coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ViewEvent {
    PointerDown {
        pos: Pt,
        button: Button,
        modifiers: Modifiers,
    },
    PointerMove {
        pos: Pt,
    },
    PointerUp {
        pos: Pt,
        button: Button,
    },
    /// Esc: cancel the current gesture or mode and clear the selection.
    Cancel,
    /// The window lost focus: a drag in progress is cancelled.
    FocusLost,
}

/// What the app should do after an event.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ViewResponse {
    /// The view changed and must be drawn again.
    pub repaint: bool,
    /// The selection changed: panels showing it must update (S §6.3).
    pub selection_changed: bool,
    /// The model changed: an undo step was recorded.
    pub model_changed: bool,
}

impl ViewResponse {
    /// Folds `other` into this response.
    pub fn merge(&mut self, other: ViewResponse) {
        self.repaint |= other.repaint;
        self.selection_changed |= other.selection_changed;
        self.model_changed |= other.model_changed;
    }
}

/// What is under a point (S §6.2).
#[derive(Clone, Debug, PartialEq)]
pub enum Target {
    Junction(Id),
    Species(Id),
    Leg(Id),
    Empty,
}
