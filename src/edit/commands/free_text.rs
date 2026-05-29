//! Commands for the free-text tool: add, edit contents, move/resize.

use crate::edit::{command::Command, EditId, EditSession, FreeTextBox};

/// Adds a new free-text box. Undo removes it; redo re-adds the same box
/// (preserving its id so later edit/move commands still resolve).
pub struct AddFreeTextCommand {
    box_: Option<FreeTextBox>,
    id: EditId,
    page_index: usize,
}

impl AddFreeTextCommand {
    pub fn new(box_: FreeTextBox) -> Self {
        Self {
            id: box_.id,
            page_index: box_.page_index,
            box_: Some(box_),
        }
    }
}

impl Command for AddFreeTextCommand {
    fn label(&self) -> &'static str {
        "Add text"
    }

    fn apply(&mut self, session: &mut EditSession) {
        if let Some(b) = self.box_.take() {
            session.add_free_text(b);
        }
    }

    fn revert(&mut self, session: &mut EditSession) {
        self.box_ = session.remove_free_text(self.page_index, self.id);
    }
}

/// Replaces a free-text box's contents. Stores the prior text for undo.
pub struct EditFreeTextCommand {
    page_index: usize,
    id: EditId,
    new_text: String,
    prior: Option<String>,
}

impl EditFreeTextCommand {
    pub fn new(page_index: usize, id: EditId, new_text: impl Into<String>) -> Self {
        Self {
            page_index,
            id,
            new_text: new_text.into(),
            prior: None,
        }
    }
}

impl Command for EditFreeTextCommand {
    fn label(&self) -> &'static str {
        "Edit text"
    }

    fn apply(&mut self, session: &mut EditSession) {
        if let Some(b) = session.free_text_mut(self.page_index, self.id) {
            if self.prior.is_none() {
                self.prior = Some(b.text.clone());
            }
            b.text = self.new_text.clone();
        }
    }

    fn revert(&mut self, session: &mut EditSession) {
        if let (Some(b), Some(prev)) =
            (session.free_text_mut(self.page_index, self.id), self.prior.take())
        {
            b.text = prev;
        }
    }
}

/// Moves a free-text box to a new origin. Stores the prior origin for undo.
pub struct MoveFreeTextCommand {
    page_index: usize,
    id: EditId,
    new_origin: [f32; 2],
    prior: Option<[f32; 2]>,
}

impl MoveFreeTextCommand {
    pub fn new(page_index: usize, id: EditId, new_origin: [f32; 2]) -> Self {
        Self {
            page_index,
            id,
            new_origin,
            prior: None,
        }
    }
}

impl Command for MoveFreeTextCommand {
    fn label(&self) -> &'static str {
        "Move text"
    }

    fn apply(&mut self, session: &mut EditSession) {
        if let Some(b) = session.free_text_mut(self.page_index, self.id) {
            if self.prior.is_none() {
                self.prior = Some(b.origin_pt);
            }
            b.origin_pt = self.new_origin;
        }
    }

    fn revert(&mut self, session: &mut EditSession) {
        if let (Some(b), Some(prev)) =
            (session.free_text_mut(self.page_index, self.id), self.prior.take())
        {
            b.origin_pt = prev;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit::command::UndoStack;
    use crate::edit::EditId;

    fn sample_box(page: usize) -> FreeTextBox {
        FreeTextBox {
            id: EditId::next(),
            page_index: page,
            origin_pt: [100.0, 700.0],
            size_pt: [180.0, 24.0],
            text: "hello".into(),
            font_size: 12.0,
            color: [0, 0, 0, 255],
        }
    }

    #[test]
    fn add_undo_redo() {
        let mut s = EditSession::new(1);
        let mut stack = UndoStack::default();
        let b = sample_box(0);
        let id = b.id;
        stack.push_apply(Box::new(AddFreeTextCommand::new(b)), &mut s);
        assert_eq!(s.free_texts_on(0).count(), 1);
        assert!(stack.undo(&mut s));
        assert_eq!(s.free_texts_on(0).count(), 0);
        assert!(stack.redo(&mut s));
        assert_eq!(s.free_texts_on(0).count(), 1);
        // id preserved across undo/redo
        assert!(s.free_text_mut(0, id).is_some());
    }

    #[test]
    fn edit_text_undo_restores_prior() {
        let mut s = EditSession::new(1);
        let mut stack = UndoStack::default();
        let b = sample_box(0);
        let id = b.id;
        stack.push_apply(Box::new(AddFreeTextCommand::new(b)), &mut s);
        stack.push_apply(
            Box::new(EditFreeTextCommand::new(0, id, "world")),
            &mut s,
        );
        assert_eq!(s.free_text_mut(0, id).unwrap().text, "world");
        assert!(stack.undo(&mut s));
        assert_eq!(s.free_text_mut(0, id).unwrap().text, "hello");
    }

    #[test]
    fn move_undo_restores_origin() {
        let mut s = EditSession::new(1);
        let mut stack = UndoStack::default();
        let b = sample_box(0);
        let id = b.id;
        stack.push_apply(Box::new(AddFreeTextCommand::new(b)), &mut s);
        stack.push_apply(
            Box::new(MoveFreeTextCommand::new(0, id, [250.0, 400.0])),
            &mut s,
        );
        assert_eq!(s.free_text_mut(0, id).unwrap().origin_pt, [250.0, 400.0]);
        assert!(stack.undo(&mut s));
        assert_eq!(s.free_text_mut(0, id).unwrap().origin_pt, [100.0, 700.0]);
    }
}
