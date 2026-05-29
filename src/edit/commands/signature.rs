//! Commands for placing and moving signatures.

use crate::edit::{command::Command, EditId, EditSession, SignaturePlacement};

pub struct AddSignatureCommand {
    placement: Option<SignaturePlacement>,
    id: EditId,
    page_index: usize,
}

impl AddSignatureCommand {
    pub fn new(placement: SignaturePlacement) -> Self {
        Self {
            id: placement.id,
            page_index: placement.page_index,
            placement: Some(placement),
        }
    }
}

impl Command for AddSignatureCommand {
    fn label(&self) -> &'static str {
        "Add signature"
    }

    fn apply(&mut self, session: &mut EditSession) {
        if let Some(s) = self.placement.take() {
            session.add_signature(s);
        }
    }

    fn revert(&mut self, session: &mut EditSession) {
        self.placement = session.remove_signature(self.page_index, self.id);
    }
}

pub struct MoveSignatureCommand {
    page_index: usize,
    id: EditId,
    new_origin: [f32; 2],
    prior: Option<[f32; 2]>,
}

impl MoveSignatureCommand {
    pub fn new(page_index: usize, id: EditId, new_origin: [f32; 2]) -> Self {
        Self {
            page_index,
            id,
            new_origin,
            prior: None,
        }
    }
}

impl Command for MoveSignatureCommand {
    fn label(&self) -> &'static str {
        "Move signature"
    }

    fn apply(&mut self, session: &mut EditSession) {
        if let Some(s) = session.signature_mut(self.page_index, self.id) {
            if self.prior.is_none() {
                self.prior = Some(s.origin_pt);
            }
            s.origin_pt = self.new_origin;
        }
    }

    fn revert(&mut self, session: &mut EditSession) {
        if let (Some(s), Some(prev)) =
            (session.signature_mut(self.page_index, self.id), self.prior.take())
        {
            s.origin_pt = prev;
        }
    }
}

/// Resizes a signature (sets a new size in PDF points), preserving its origin.
pub struct ResizeSignatureCommand {
    page_index: usize,
    id: EditId,
    new_size: [f32; 2],
    prior: Option<[f32; 2]>,
}

impl ResizeSignatureCommand {
    pub fn new(page_index: usize, id: EditId, new_size: [f32; 2]) -> Self {
        Self {
            page_index,
            id,
            new_size,
            prior: None,
        }
    }
}

impl Command for ResizeSignatureCommand {
    fn label(&self) -> &'static str {
        "Resize signature"
    }

    fn apply(&mut self, session: &mut EditSession) {
        if let Some(s) = session.signature_mut(self.page_index, self.id) {
            if self.prior.is_none() {
                self.prior = Some(s.size_pt);
            }
            s.size_pt = self.new_size;
        }
    }

    fn revert(&mut self, session: &mut EditSession) {
        if let (Some(s), Some(prev)) =
            (session.signature_mut(self.page_index, self.id), self.prior.take())
        {
            s.size_pt = prev;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit::command::UndoStack;
    use std::sync::Arc;

    fn sample(page: usize) -> SignaturePlacement {
        SignaturePlacement {
            id: EditId::next(),
            page_index: page,
            origin_pt: [100.0, 200.0],
            size_pt: [120.0, 40.0],
            image: Arc::new(image::RgbaImage::new(4, 2)),
        }
    }

    #[test]
    fn add_undo_redo() {
        let mut s = EditSession::new(1);
        let mut stack = UndoStack::default();
        let p = sample(0);
        let id = p.id;
        stack.push_apply(Box::new(AddSignatureCommand::new(p)), &mut s);
        assert_eq!(s.signatures_on(0).count(), 1);
        assert!(stack.undo(&mut s));
        assert_eq!(s.signatures_on(0).count(), 0);
        assert!(stack.redo(&mut s));
        assert!(s.signatures_on(0).any(|x| x.id == id));
    }

    #[test]
    fn move_undo_restores_origin() {
        let mut s = EditSession::new(1);
        let mut stack = UndoStack::default();
        let p = sample(0);
        let id = p.id;
        stack.push_apply(Box::new(AddSignatureCommand::new(p)), &mut s);
        stack.push_apply(
            Box::new(MoveSignatureCommand::new(0, id, [300.0, 500.0])),
            &mut s,
        );
        assert_eq!(s.signature_mut(0, id).unwrap().origin_pt, [300.0, 500.0]);
        assert!(stack.undo(&mut s));
        assert_eq!(s.signature_mut(0, id).unwrap().origin_pt, [100.0, 200.0]);
    }

    #[test]
    fn resize_undo_restores_size() {
        let mut s = EditSession::new(1);
        let mut stack = UndoStack::default();
        let p = sample(0);
        let id = p.id;
        stack.push_apply(Box::new(AddSignatureCommand::new(p)), &mut s);
        stack.push_apply(
            Box::new(ResizeSignatureCommand::new(0, id, [240.0, 80.0])),
            &mut s,
        );
        assert_eq!(s.signature_mut(0, id).unwrap().size_pt, [240.0, 80.0]);
        assert!(stack.undo(&mut s));
        assert_eq!(s.signature_mut(0, id).unwrap().size_pt, [120.0, 40.0]);
    }
}
