//! The view's own input events (SPEC-Rust §2.1).
//!
//! The app turns egui input into these, using only `pd-core` types, so the
//! interaction state machine can be tested without a window.

use crate::bezier::Ctrl;
use crate::geometry::Pt;
use crate::model::{Id, Role};

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
    /// A double click (left button), after its two clicks were handled.
    DoubleClick {
        pos: Pt,
    },
    /// The pointer left the canvas.
    PointerLeft,
    /// Esc: cancel the current gesture or mode and clear the selection.
    Cancel,
    /// Enter: with the open reaction tool, go on from the reactants to the
    /// products, or make the reaction (S §6.5).
    Confirm,
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
    /// Something the app must do for the view, such as asking for a name.
    pub request: Option<Request>,
}

/// Requests that need the app: dialogs and menus (SPEC-Rust §6.2).
#[derive(Clone, Debug, PartialEq)]
pub enum Request {
    /// Ask for a new id for this species, then call `DiagramView::rename`.
    Rename(Id),
    /// Show the context menu for what was right-clicked (S §13.4).
    ContextMenu(Target),
    /// Put the keyboard in the editor for this text's words (S §7.8).
    EditText(Id),
}

impl ViewResponse {
    /// Folds `other` into this response.
    pub fn merge(&mut self, other: ViewResponse) {
        self.repaint |= other.repaint;
        self.selection_changed |= other.selection_changed;
        self.model_changed |= other.model_changed;
        if other.request.is_some() {
            self.request = other.request;
        }
    }
}

/// The current tool (S §6.1, §13.2).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tool {
    #[default]
    Select,
    /// Each click adds a species.
    AddSpecies,
    /// Pick this many reactants, then this many products.
    AddReaction { reactants: usize, products: usize },
    /// A click places new text, then goes back to Select (S §7.8).
    AddText,
    /// A drag draws a compartment's box, then goes back to Select (S §7.9).
    AddCompartment,
}

impl Tool {
    pub const UNI_UNI: Tool = Tool::AddReaction {
        reactants: 1,
        products: 1,
    };
    pub const BI_UNI: Tool = Tool::AddReaction {
        reactants: 2,
        products: 1,
    };
    pub const UNI_BI: Tool = Tool::AddReaction {
        reactants: 1,
        products: 2,
    };
    pub const BI_BI: Tool = Tool::AddReaction {
        reactants: 2,
        products: 2,
    };
    /// Any number of reactants and products (S §6.5): pick the reactants,
    /// Enter, pick the products, Enter. A count of 0 means "as many as are
    /// picked".
    pub const ANY_REACTION: Tool = Tool::AddReaction {
        reactants: 0,
        products: 0,
    };

    /// True for [`Tool::ANY_REACTION`].
    pub fn is_open_reaction(self) -> bool {
        self == Tool::ANY_REACTION
    }
}

/// One Bézier handle: which reaction, which leg, which of its two handles.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HandleId {
    pub reaction: Id,
    pub role: Role,
    pub index: usize,
    pub ctrl: Ctrl,
}

/// What is under a point (S §6.2).
#[derive(Clone, Debug, PartialEq)]
pub enum Target {
    /// A handle of a selected Bézier reaction.
    Handle(HandleId),
    Junction(Id),
    Species(Id),
    Leg(Id),
    /// Free text (S §7.8).
    Text(Id),
    /// The wrap handle on a selected text's right edge.
    TextWrap(Id),
    /// A compartment, by its border or its name (S §7.9).
    Compartment(Id),
    /// A corner handle of a selected compartment, clockwise from the top left.
    CompartmentCorner(Id, usize),
    /// A corner handle of a selected species, clockwise from the top left
    /// (S §7.10).
    SpeciesCorner(Id, usize),
    /// A species' name drawn outside its node (S §5.11).
    SpeciesLabel(Id),
    /// A reaction's name (S §5.12).
    ReactionLabel(Id),
    /// An end of a selected reaction's stem (S §5.13): the reactants' end or
    /// the products'.
    StemEnd(Id, crate::model::Role),
    Empty,
}
