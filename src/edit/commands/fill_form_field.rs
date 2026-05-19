//! Set (or clear) the pending value of a single text-field widget.

use crate::edit::{command::Command, EditSession};
use crate::pdf::document::WidgetId;

pub struct FillFormFieldCommand {
    widget: WidgetId,
    /// Value to apply.
    new_value: String,
    /// Captured on first `apply` so `revert` can restore the prior pending
    /// value (or remove the edit if there was none).
    prior: Option<Option<String>>,
}

impl FillFormFieldCommand {
    pub fn new(widget: WidgetId, value: impl Into<String>) -> Self {
        Self {
            widget,
            new_value: value.into(),
            prior: None,
        }
    }
}

impl Command for FillFormFieldCommand {
    fn label(&self) -> &'static str {
        "Fill field"
    }

    fn apply(&mut self, session: &mut EditSession) {
        let prior_value = session
            .form_fill_value(self.widget)
            .map(ToOwned::to_owned);
        // `upsert` returns the value it overwrote, but we want the *initial* one
        // before any apply happened, so cache only on the first call.
        if self.prior.is_none() {
            self.prior = Some(prior_value);
        }
        session.upsert_form_fill(self.widget, self.new_value.clone());
    }

    fn revert(&mut self, session: &mut EditSession) {
        match self.prior.take() {
            Some(Some(v)) => {
                session.upsert_form_fill(self.widget, v);
            }
            Some(None) => {
                session.remove_form_fill(self.widget);
            }
            None => {
                // revert without prior apply — leave state untouched.
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit::command::UndoStack;
    use crate::pdf::document::WidgetId;

    fn wid(p: usize, a: u32) -> WidgetId {
        WidgetId {
            page_index: p,
            annotation_index: a,
        }
    }

    #[test]
    fn fill_then_undo_clears_pending() {
        let mut s = EditSession::new(1);
        let mut stack = UndoStack::default();
        stack.push_apply(
            Box::new(FillFormFieldCommand::new(wid(0, 0), "hello")),
            &mut s,
        );
        assert_eq!(s.form_fill_value(wid(0, 0)), Some("hello"));
        assert!(stack.undo(&mut s));
        assert_eq!(s.form_fill_value(wid(0, 0)), None);
    }

    #[test]
    fn two_fills_undo_restores_first_value() {
        let mut s = EditSession::new(1);
        let mut stack = UndoStack::default();
        stack.push_apply(
            Box::new(FillFormFieldCommand::new(wid(0, 0), "first")),
            &mut s,
        );
        stack.push_apply(
            Box::new(FillFormFieldCommand::new(wid(0, 0), "second")),
            &mut s,
        );
        assert_eq!(s.form_fill_value(wid(0, 0)), Some("second"));
        assert!(stack.undo(&mut s));
        assert_eq!(s.form_fill_value(wid(0, 0)), Some("first"));
        assert!(stack.undo(&mut s));
        assert_eq!(s.form_fill_value(wid(0, 0)), None);
    }
}
