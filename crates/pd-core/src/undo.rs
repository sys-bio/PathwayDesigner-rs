//! Undo and redo (S §9, SPEC-Rust §7.1).
//!
//! A linear history of at most 100 commands. Lightweight commands find model
//! objects by id when they are applied, so they still work after a snapshot
//! has been restored (S §9.2).

use std::collections::VecDeque;

use crate::geometry::Pt;
use crate::model::{Id, Model, Role, VisualStyle};

/// Maximum number of undo steps (S §9.1).
pub const UNDO_DEPTH: usize = 100;

/// A participant's handles: `(ctrl1, ctrl2, ctrl_pts_set)`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CtrlState {
    pub ctrl1: Pt,
    pub ctrl2: Pt,
    pub ctrl_pts_set: bool,
}

/// One undoable change (S §9.2).
#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    /// The whole model before and after: for anything structural, or a change
    /// touching several kinds of data. The id counter is part of the model, so
    /// it is restored too.
    Snapshot {
        desc: &'static str,
        before: Box<Model>,
        after: Box<Model>,
    },
    /// A plain node drag: species and junction positions, before and after.
    MoveNodes {
        species: Vec<(Id, Pt, Pt)>,
        junctions: Vec<(Id, Pt, Pt)>,
    },
    /// A plain junction drag.
    MoveJunction { reaction: Id, from: Pt, to: Pt },
    /// A Bézier handle drag on a reaction without a smooth junction.
    DragCtrlPt {
        reaction: Id,
        role: Role,
        index: usize,
        from: CtrlState,
        to: CtrlState,
    },
    /// Colour and width edits.
    StyleEdit {
        species: Vec<(Id, VisualStyle, VisualStyle)>,
        reactions: Vec<(Id, VisualStyle, VisualStyle)>,
    },
}

impl Command {
    /// The short name shown in the Edit menu ("Undo Move").
    pub fn description(&self) -> &'static str {
        match self {
            Command::Snapshot { desc, .. } => desc,
            Command::MoveNodes { .. } => "Move",
            Command::MoveJunction { .. } => "Move Junction",
            Command::DragCtrlPt { .. } => "Move Handle",
            Command::StyleEdit { .. } => "Style",
        }
    }

    /// Puts the model in the state before (`undo`) or after this command.
    fn apply(&self, model: &mut Model, undo: bool) {
        let pick = |from: Pt, to: Pt| if undo { from } else { to };
        match self {
            Command::Snapshot { before, after, .. } => {
                *model = if undo {
                    (**before).clone()
                } else {
                    (**after).clone()
                };
            }
            Command::MoveNodes { species, junctions } => {
                for (id, from, to) in species {
                    if let Some(s) = model.find_species_mut(id) {
                        s.center = pick(*from, *to);
                    }
                }
                for (id, from, to) in junctions {
                    if let Some(r) = model.find_reaction_mut(id) {
                        r.junction = pick(*from, *to);
                    }
                }
            }
            Command::MoveJunction { reaction, from, to } => {
                if let Some(r) = model.find_reaction_mut(reaction) {
                    r.junction = pick(*from, *to);
                }
            }
            Command::DragCtrlPt {
                reaction,
                role,
                index,
                from,
                to,
            } => {
                let state = if undo { from } else { to };
                if let Some(p) = model
                    .find_reaction_mut(reaction)
                    .and_then(|r| r.participants_mut(*role).get_mut(*index))
                {
                    p.ctrl1 = state.ctrl1;
                    p.ctrl2 = state.ctrl2;
                    p.ctrl_pts_set = state.ctrl_pts_set;
                }
            }
            Command::StyleEdit { species, reactions } => {
                for (id, before, after) in species {
                    if let Some(s) = model.find_species_mut(id) {
                        s.style = if undo { *before } else { *after };
                    }
                }
                for (id, before, after) in reactions {
                    if let Some(r) = model.find_reaction_mut(id) {
                        r.style = if undo { *before } else { *after };
                    }
                }
            }
        }
    }
}

/// The undo and redo stacks, and where the document was last saved.
///
/// Every command gets a number when it is recorded. The *position* of the
/// history is the number of the last command not undone, so each state of the
/// document has its own position. The document is unchanged when the position
/// is where it was at the last save or load (SPEC-Rust §10.1), so undoing back
/// to a save makes it unchanged again.
#[derive(Clone, Debug)]
pub struct UndoHistory {
    undo: VecDeque<(u64, Command)>,
    redo: Vec<(u64, Command)>,
    /// The number the next command gets.
    next: u64,
    /// The position with nothing left to undo: the number of the last command
    /// dropped off the front of a full history, or of the last clear.
    base: u64,
    /// The position at the last save or load, or `None` if that state can no
    /// longer be reached (it was undone, then replaced by a new command).
    saved: Option<u64>,
}

impl Default for UndoHistory {
    fn default() -> Self {
        Self {
            undo: VecDeque::new(),
            redo: Vec::new(),
            next: 1,
            base: 0,
            saved: Some(0),
        }
    }
}

impl UndoHistory {
    pub fn new() -> Self {
        Self::default()
    }

    fn position(&self) -> u64 {
        self.undo.back().map_or(self.base, |(n, _)| *n)
    }

    /// True if the document differs from when it was last saved or loaded.
    pub fn is_modified(&self) -> bool {
        self.saved != Some(self.position())
    }

    /// Notes that the document has just been saved.
    pub fn mark_saved(&mut self) {
        self.saved = Some(self.position());
    }

    /// Notes that the document has never been saved in its current state (a
    /// recovered autosave, say).
    pub fn mark_unsaved(&mut self) {
        self.saved = None;
    }

    /// Records a command that has already been applied to the model. Clears
    /// the redo stack; drops the oldest entry when the history is full.
    pub fn push(&mut self, cmd: Command) {
        // A saved state waiting to be redone can't be reached any more.
        if self.redo.iter().any(|(n, _)| Some(*n) == self.saved) {
            self.saved = None;
        }
        self.redo.clear();
        if self.undo.len() == UNDO_DEPTH
            && let Some((n, _)) = self.undo.pop_front()
        {
            self.base = n;
        }
        self.undo.push_back((self.next, cmd));
        self.next += 1;
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// What Undo would undo, for the Edit menu.
    pub fn undo_description(&self) -> Option<&'static str> {
        self.undo.back().map(|(_, c)| c.description())
    }

    pub fn redo_description(&self) -> Option<&'static str> {
        self.redo.last().map(|(_, c)| c.description())
    }

    /// Undoes the last command. The caller clears the selection and cancels
    /// any drag (S §9.1). Returns what was undone.
    pub fn undo(&mut self, model: &mut Model) -> Option<&'static str> {
        let (n, cmd) = self.undo.pop_back()?;
        cmd.apply(model, true);
        let desc = cmd.description();
        self.redo.push((n, cmd));
        Some(desc)
    }

    pub fn redo(&mut self, model: &mut Model) -> Option<&'static str> {
        let (n, cmd) = self.redo.pop()?;
        cmd.apply(model, false);
        let desc = cmd.description();
        self.undo.push_back((n, cmd));
        Some(desc)
    }

    /// Forgets everything, as Open and New do (S §9.1). The document is then
    /// unchanged.
    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
        self.base = self.next;
        self.next += 1;
        self.saved = Some(self.base);
    }

    pub fn len(&self) -> usize {
        self.undo.len()
    }

    pub fn is_empty(&self) -> bool {
        self.undo.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::pt;
    use crate::model::{Participant, Rgba};

    fn model() -> Model {
        let mut m = Model::new();
        let a = m.add_species(pt(0.0, 0.0), 80.0, 36.0);
        let r = m.add_reaction(pt(100.0, 0.0));
        m.find_reaction_mut(&r)
            .unwrap()
            .reactants
            .push(Participant::new(a, 1.0));
        m
    }

    #[test]
    fn move_nodes_undo_and_redo() {
        let mut m = model();
        let mut h = UndoHistory::new();
        m.find_species_mut("S1").unwrap().center = pt(5.0, 5.0);
        m.find_reaction_mut("R2").unwrap().junction = pt(105.0, 5.0);
        h.push(Command::MoveNodes {
            species: vec![("S1".into(), pt(0.0, 0.0), pt(5.0, 5.0))],
            junctions: vec![("R2".into(), pt(100.0, 0.0), pt(105.0, 5.0))],
        });
        assert_eq!(h.undo_description(), Some("Move"));
        assert_eq!(h.undo(&mut m), Some("Move"));
        assert_eq!(m.find_species("S1").unwrap().center, pt(0.0, 0.0));
        assert_eq!(m.find_reaction("R2").unwrap().junction, pt(100.0, 0.0));
        assert!(!h.can_undo() && h.can_redo());
        h.redo(&mut m);
        assert_eq!(m.find_species("S1").unwrap().center, pt(5.0, 5.0));
        assert_eq!(m.find_reaction("R2").unwrap().junction, pt(105.0, 5.0));
    }

    #[test]
    fn snapshot_restores_everything_including_the_id_counter() {
        let mut m = model();
        let before = m.clone();
        m.delete_species("S1");
        m.add_species(Pt::ZERO, 80.0, 36.0);
        let after = m.clone();
        let mut h = UndoHistory::new();
        h.push(Command::Snapshot {
            desc: "Delete",
            before: Box::new(before.clone()),
            after: Box::new(after.clone()),
        });
        h.undo(&mut m);
        assert_eq!(m, before);
        assert_eq!(m.next_id(), before.next_id());
        h.redo(&mut m);
        assert_eq!(m, after);
    }

    #[test]
    fn lightweight_commands_work_after_a_snapshot_restore() {
        // A MoveJunction recorded before a delete still finds its reaction
        // by id after the delete is undone and the model rebuilt.
        let mut m = model();
        let mut h = UndoHistory::new();
        m.find_reaction_mut("R2").unwrap().junction = pt(50.0, 50.0);
        h.push(Command::MoveJunction {
            reaction: "R2".into(),
            from: pt(100.0, 0.0),
            to: pt(50.0, 50.0),
        });
        let before = m.clone();
        m.delete_reaction("R2");
        h.push(Command::Snapshot {
            desc: "Delete",
            before: Box::new(before),
            after: Box::new(m.clone()),
        });
        h.undo(&mut m);
        h.undo(&mut m);
        assert_eq!(m.find_reaction("R2").unwrap().junction, pt(100.0, 0.0));
    }

    #[test]
    fn handle_and_style_commands() {
        let mut m = model();
        let mut h = UndoHistory::new();
        let from = CtrlState {
            ctrl1: Pt::ZERO,
            ctrl2: Pt::ZERO,
            ctrl_pts_set: false,
        };
        let to = CtrlState {
            ctrl1: pt(1.0, 2.0),
            ctrl2: pt(3.0, 4.0),
            ctrl_pts_set: true,
        };
        h.push(Command::DragCtrlPt {
            reaction: "R2".into(),
            role: Role::Reactant,
            index: 0,
            from,
            to,
        });
        h.undo(&mut m);
        h.redo(&mut m);
        let p = &m.find_reaction("R2").unwrap().reactants[0];
        assert_eq!(
            (p.ctrl1, p.ctrl2, p.ctrl_pts_set),
            (pt(1.0, 2.0), pt(3.0, 4.0), true)
        );

        let old = m.find_species("S1").unwrap().style;
        let mut new = old;
        new.has_custom_style = true;
        new.fill_color = Rgba::rgb(9, 9, 9);
        m.find_species_mut("S1").unwrap().style = new;
        h.push(Command::StyleEdit {
            species: vec![("S1".into(), old, new)],
            reactions: vec![],
        });
        h.undo(&mut m);
        assert_eq!(m.find_species("S1").unwrap().style, old);
    }

    #[test]
    fn a_new_command_clears_redo() {
        let mut m = model();
        let mut h = UndoHistory::new();
        let mv = |x: f64| Command::MoveJunction {
            reaction: "R2".into(),
            from: pt(100.0, 0.0),
            to: pt(x, 0.0),
        };
        h.push(mv(1.0));
        h.undo(&mut m);
        assert!(h.can_redo());
        h.push(mv(2.0));
        assert!(!h.can_redo());
    }

    #[test]
    fn history_is_capped_at_100() {
        let mut h = UndoHistory::new();
        for i in 0..150 {
            h.push(Command::MoveJunction {
                reaction: format!("R{i}"),
                from: Pt::ZERO,
                to: Pt::ZERO,
            });
        }
        assert_eq!(h.len(), UNDO_DEPTH);
        // The oldest were dropped: the first left is R50.
        let mut m = Model::new();
        for _ in 0..UNDO_DEPTH {
            h.undo(&mut m);
        }
        assert!(!h.can_undo());
        assert!(
            matches!(h.redo.last(), Some((_, Command::MoveJunction { reaction, .. })) if reaction == "R50")
        );
    }

    fn step(x: f64) -> Command {
        Command::MoveJunction {
            reaction: "R2".into(),
            from: Pt::ZERO,
            to: pt(x, 0.0),
        }
    }

    #[test]
    fn modified_follows_the_position_in_the_history() {
        let mut m = model();
        let mut h = UndoHistory::new();
        assert!(!h.is_modified(), "a new history is unchanged");
        h.push(step(1.0));
        assert!(h.is_modified());
        h.undo(&mut m);
        assert!(!h.is_modified(), "undoing back to the load");
        h.redo(&mut m);
        h.mark_saved();
        assert!(!h.is_modified());
        h.push(step(2.0));
        assert!(h.is_modified());
        h.undo(&mut m);
        assert!(!h.is_modified(), "undoing back to the save");
        h.undo(&mut m);
        assert!(h.is_modified());
        h.redo(&mut m);
        assert!(!h.is_modified(), "redoing back to the save");
    }

    #[test]
    fn a_saved_state_replaced_by_a_new_command_is_gone() {
        let mut m = model();
        let mut h = UndoHistory::new();
        h.push(step(1.0));
        h.mark_saved();
        h.undo(&mut m);
        // A new command replaces the saved state's branch.
        h.push(step(2.0));
        assert!(h.is_modified());
        h.undo(&mut m);
        assert!(h.is_modified(), "back at the load, but it was saved since");
    }

    #[test]
    fn a_full_history_doesnt_look_unchanged_when_undone_completely() {
        let mut m = model();
        let mut h = UndoHistory::new();
        for i in 0..UNDO_DEPTH + 5 {
            h.push(step(i as f64));
        }
        while h.can_undo() {
            h.undo(&mut m);
        }
        assert!(
            h.is_modified(),
            "five steps fell off: this isn't the loaded state"
        );
    }

    #[test]
    fn clear_makes_the_document_unchanged() {
        let mut h = UndoHistory::new();
        h.push(step(1.0));
        h.clear();
        assert!(!h.is_modified());
    }

    #[test]
    fn clear_forgets_both_stacks() {
        let mut m = model();
        let mut h = UndoHistory::new();
        h.push(Command::MoveJunction {
            reaction: "R2".into(),
            from: Pt::ZERO,
            to: Pt::ZERO,
        });
        h.push(Command::MoveJunction {
            reaction: "R2".into(),
            from: Pt::ZERO,
            to: Pt::ZERO,
        });
        h.undo(&mut m);
        h.clear();
        assert!(!h.can_undo() && !h.can_redo());
    }
}
