//! Command for adding a highlight (undo removes it; redo re-adds it).

use crate::edit::{command::Command, EditId, EditSession, Highlight};

pub struct AddHighlightCommand {
    highlight: Option<Highlight>,
    id: EditId,
    page_index: usize,
}

impl AddHighlightCommand {
    pub fn new(highlight: Highlight) -> Self {
        Self {
            id: highlight.id,
            page_index: highlight.page_index,
            highlight: Some(highlight),
        }
    }
}

impl Command for AddHighlightCommand {
    fn label(&self) -> &'static str {
        "Add highlight"
    }

    fn apply(&mut self, session: &mut EditSession) {
        if let Some(h) = self.highlight.take() {
            session.add_highlight(h);
        }
    }

    fn revert(&mut self, session: &mut EditSession) {
        self.highlight = session.remove_highlight(self.page_index, self.id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit::command::UndoStack;

    fn sample(page: usize) -> Highlight {
        Highlight {
            id: EditId::next(),
            page_index: page,
            rects_pt: vec![[72.0, 700.0, 300.0, 716.0]],
            color: [255, 235, 60, 110],
        }
    }

    #[test]
    fn add_undo_redo() {
        let mut s = EditSession::new(1);
        let mut stack = UndoStack::default();
        let h = sample(0);
        let id = h.id;
        stack.push_apply(Box::new(AddHighlightCommand::new(h)), &mut s);
        assert_eq!(s.highlights_on(0).count(), 1);
        assert!(stack.undo(&mut s));
        assert_eq!(s.highlights_on(0).count(), 0);
        assert!(stack.redo(&mut s));
        assert_eq!(s.highlights_on(0).count(), 1);
        assert!(s.highlights_on(0).any(|h| h.id == id));
    }
}
