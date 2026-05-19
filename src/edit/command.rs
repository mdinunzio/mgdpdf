//! Reversible commands and the undo/redo stack.
//!
//! A [`Command`] is the unit of user intent — adding a free-text box, editing
//! a form field, deleting a highlight. Each command knows how to `apply` itself
//! to the [`EditSession`] and how to `revert` itself back to the prior state.
//! The [`UndoStack`] tracks applied commands and offers `undo` / `redo`.

use super::EditSession;

pub trait Command: Send + 'static {
    /// Short label for the command palette / accessibility, e.g. `"Fill field"`.
    fn label(&self) -> &'static str;

    /// Mutates the session forward.
    fn apply(&mut self, session: &mut EditSession);

    /// Reverts the mutation. After `revert`, the session must be equivalent to
    /// its state immediately before the matching `apply`.
    fn revert(&mut self, session: &mut EditSession);
}

pub struct UndoStack {
    /// Most recently applied at the back.
    done: Vec<Box<dyn Command>>,
    /// Most recently undone at the back.
    redo: Vec<Box<dyn Command>>,
    /// Cap to bound memory; older entries beyond this are dropped from the
    /// "done" side as the user keeps editing.
    capacity: usize,
}

impl Default for UndoStack {
    fn default() -> Self {
        Self::with_capacity(256)
    }
}

impl UndoStack {
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            done: Vec::new(),
            redo: Vec::new(),
            capacity,
        }
    }

    /// Applies `cmd` and records it. Clears the redo lane — once the user takes
    /// a new action, any previously-undone branch is abandoned.
    pub fn push_apply(&mut self, mut cmd: Box<dyn Command>, session: &mut EditSession) {
        cmd.apply(session);
        session.dirty = true;
        self.redo.clear();
        if self.done.len() == self.capacity {
            // Drop the oldest to keep memory bounded.
            self.done.remove(0);
        }
        self.done.push(cmd);
    }

    pub fn can_undo(&self) -> bool {
        !self.done.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn undo(&mut self, session: &mut EditSession) -> bool {
        let Some(mut cmd) = self.done.pop() else {
            return false;
        };
        cmd.revert(session);
        session.dirty = true;
        self.redo.push(cmd);
        true
    }

    pub fn redo(&mut self, session: &mut EditSession) -> bool {
        let Some(mut cmd) = self.redo.pop() else {
            return false;
        };
        cmd.apply(session);
        session.dirty = true;
        self.done.push(cmd);
        true
    }

    pub fn clear(&mut self) {
        self.done.clear();
        self.redo.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A trivial command that toggles `session.dirty` itself via a side counter.
    /// We can't yet exercise real edits (the `Edit` enum is empty in Phase 2),
    /// so we use the bookkeeping fields to prove the stack mechanics.
    struct Noop {
        applied: i32,
    }

    impl Command for Noop {
        fn label(&self) -> &'static str {
            "noop"
        }
        fn apply(&mut self, _session: &mut EditSession) {
            self.applied += 1;
        }
        fn revert(&mut self, _session: &mut EditSession) {
            self.applied -= 1;
        }
    }

    #[test]
    fn push_apply_clears_redo_and_marks_dirty() {
        let mut s = EditSession::new(1);
        let mut stack = UndoStack::default();
        // Seed the redo lane by undoing one command.
        stack.push_apply(Box::new(Noop { applied: 0 }), &mut s);
        assert!(stack.undo(&mut s));
        assert!(stack.can_redo());

        // A fresh apply must abandon the redo lane.
        stack.push_apply(Box::new(Noop { applied: 0 }), &mut s);
        assert!(!stack.can_redo());
        assert!(s.dirty);
    }

    #[test]
    fn undo_redo_round_trip() {
        let mut s = EditSession::new(1);
        let mut stack = UndoStack::default();
        stack.push_apply(Box::new(Noop { applied: 0 }), &mut s);
        stack.push_apply(Box::new(Noop { applied: 0 }), &mut s);
        assert!(stack.undo(&mut s));
        assert!(stack.undo(&mut s));
        assert!(!stack.can_undo());
        assert!(stack.redo(&mut s));
        assert!(stack.redo(&mut s));
        assert!(!stack.can_redo());
    }

    #[test]
    fn capacity_drops_oldest() {
        let mut s = EditSession::new(1);
        let mut stack = UndoStack::with_capacity(2);
        stack.push_apply(Box::new(Noop { applied: 0 }), &mut s);
        stack.push_apply(Box::new(Noop { applied: 0 }), &mut s);
        stack.push_apply(Box::new(Noop { applied: 0 }), &mut s);
        // Three pushes against a cap of 2 → only the latest two remain undoable.
        assert!(stack.undo(&mut s));
        assert!(stack.undo(&mut s));
        assert!(!stack.can_undo());
    }
}
