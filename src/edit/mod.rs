//! User-authored edits layered over a PDF.
//!
//! An [`Edit`] is a per-page record that lives in the [`EditSession`] in memory
//! and gets committed to a real PDFium annotation on save. Each phase adds a
//! variant: `FormFill` (Phase 3), `FreeText` (Phase 4), `Highlight` (Phase 5),
//! `Signature` (Phase 6). Phase 2 is scaffolding — the enum is intentionally
//! empty so the surrounding plumbing (storage, undo/redo, tool dispatch) can
//! compile and be wired before any feature lands.
//!
//! Scaffolding warnings (unused methods/fields here) are silenced for the
//! whole module — they all become live once Phase 3 lands.
#![allow(dead_code)]

pub mod command;

use std::sync::atomic::{AtomicU64, Ordering};

pub use command::UndoStack;

/// Process-wide unique identifier for an [`Edit`]. Stable across mutations of
/// the per-page edit list (unlike `Vec` indices).
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct EditId(u64);

impl EditId {
    pub fn next() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(1);
        EditId(COUNTER.fetch_add(1, Ordering::Relaxed))
    }
}

/// A pending edit to be committed to the PDF on save. Variants are added one
/// per phase; until Phase 3 lands this enum is empty by design — code that
/// matches on it can be exhaustive *now* and stay correct as variants appear.
#[derive(Clone, Debug)]
pub enum Edit {
    // Phase 3: FormFill { widget_id: u32, value: String },
    // Phase 4: FreeText { rect_pt: PdfRect, contents: String, font_size: f32, color: [u8; 4] },
    // Phase 5: Highlight { quads_pt: Vec<PdfQuad> }, HighlightRect { rect_pt: PdfRect, color: [u8; 4] },
    // Phase 6: Signature { rect_pt: PdfRect, image_id: SignatureId },
}

impl Edit {
    /// Stable identity used for selection and undo-tracking. Lives on the
    /// variant payload — each variant will carry its own `EditId`.
    pub fn id(&self) -> EditId {
        match *self {
            // Each future variant returns its embedded id. Until then this is
            // unreachable because `Edit` is uninhabited.
        }
    }
}

/// All edits the user has authored, partitioned by page index. Variants are
/// added in later phases; for now this is the storage shape every feature
/// will plug into.
#[derive(Default)]
pub struct EditSession {
    by_page: Vec<Vec<Edit>>,
    pub undo: UndoStack,
    pub dirty: bool,
}

impl EditSession {
    pub fn new(page_count: usize) -> Self {
        Self {
            by_page: vec![Vec::new(); page_count],
            undo: UndoStack::default(),
            dirty: false,
        }
    }

    pub fn page(&self, page_index: usize) -> &[Edit] {
        self.by_page
            .get(page_index)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    /// Total edit count across all pages — used for status bar / dirty checks.
    pub fn total(&self) -> usize {
        self.by_page.iter().map(Vec::len).sum()
    }
}
