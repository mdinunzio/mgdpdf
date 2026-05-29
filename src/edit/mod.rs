//! User-authored edits layered over a PDF.
//!
//! An [`Edit`] is a per-page record that lives in the [`EditSession`] in memory
//! and gets committed to a real PDFium annotation on save. Each phase adds a
//! variant: `FormFill` (Phase 3), `FreeText` (Phase 4), `Highlight` (Phase 5),
//! `Signature` (Phase 6). Phase 2 left the enum empty so the surrounding
//! plumbing could land first; Phase 3 introduces `FormFill`.
//!
//! Scaffolding warnings for not-yet-used helpers are silenced module-wide;
//! they become live as more variants are added.
#![allow(dead_code)]

pub mod command;
pub mod commands;

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use image::RgbaImage;

use crate::pdf::document::WidgetId;

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

/// A free-text box authored by the user. Position is the top-left corner in
/// PDF points (the box grows downward in screen space); `font_size` is in
/// points; `color` is RGBA (alpha currently ignored on commit — PDFium uses
/// opaque text fill).
#[derive(Clone, Debug)]
pub struct FreeTextBox {
    pub id: EditId,
    pub page_index: usize,
    /// Top-left corner in PDF points.
    pub origin_pt: [f32; 2],
    /// Box size in PDF points (width, height).
    pub size_pt: [f32; 2],
    pub text: String,
    pub font_size: f32,
    pub color: [u8; 4],
}

/// A highlight authored by the user: one or more rectangles (PDF points,
/// bottom-left origin) sharing a colour. Text selection produces one rect per
/// line; the scanned-page fallback produces a single dragged rect.
#[derive(Clone, Debug)]
pub struct Highlight {
    pub id: EditId,
    pub page_index: usize,
    pub rects_pt: Vec<[f32; 4]>, // each [left, bottom, right, top]
    pub color: [u8; 4],
}

/// A placed signature: a transparent RGBA image stamped at a page rectangle.
/// The image is shared via `Arc` so dragging/undo don't clone the bitmap.
#[derive(Clone)]
pub struct SignaturePlacement {
    pub id: EditId,
    pub page_index: usize,
    /// Top-left corner in PDF points.
    pub origin_pt: [f32; 2],
    /// Rendered size in PDF points (width, height); preserves image aspect.
    pub size_pt: [f32; 2],
    pub image: Arc<RgbaImage>,
}

impl std::fmt::Debug for SignaturePlacement {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SignaturePlacement")
            .field("id", &self.id)
            .field("page_index", &self.page_index)
            .field("origin_pt", &self.origin_pt)
            .field("size_pt", &self.size_pt)
            .field("image", &format_args!("{}x{}", self.image.width(), self.image.height()))
            .finish()
    }
}

/// A pending edit to be committed to the PDF on save.
#[derive(Clone, Debug)]
pub enum Edit {
    /// In-memory override for a text-field widget's value. The value PDFium
    /// reports is treated as the "saved" baseline; while a `FormFill` exists
    /// in the session, the UI shows `value` instead.
    FormFill {
        id: EditId,
        widget: WidgetId,
        value: String,
    },
    /// A new free-text box stamped onto the page.
    FreeText(FreeTextBox),
    /// A highlight (set of translucent rects) over the page.
    Highlight(Highlight),
    /// A placed signature image.
    Signature(SignaturePlacement),
}

impl Edit {
    pub fn id(&self) -> EditId {
        match self {
            Edit::FormFill { id, .. } => *id,
            Edit::FreeText(b) => b.id,
            Edit::Highlight(h) => h.id,
            Edit::Signature(s) => s.id,
        }
    }
}

/// All edits the user has authored, partitioned by page index.
#[derive(Default)]
pub struct EditSession {
    by_page: Vec<Vec<Edit>>,
    pub dirty: bool,
}

impl EditSession {
    pub fn new(page_count: usize) -> Self {
        Self {
            by_page: vec![Vec::new(); page_count],
            dirty: false,
        }
    }

    pub fn page(&self, page_index: usize) -> &[Edit] {
        self.by_page
            .get(page_index)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    pub fn page_mut(&mut self, page_index: usize) -> Option<&mut Vec<Edit>> {
        self.by_page.get_mut(page_index)
    }

    /// Returns the pending value for `widget`, if any.
    pub fn form_fill_value(&self, widget: WidgetId) -> Option<&str> {
        // Latest wins.
        for edit in self.by_page.get(widget.page_index)?.iter().rev() {
            if let Edit::FormFill { widget: w, value, .. } = edit {
                if *w == widget {
                    return Some(value.as_str());
                }
            }
        }
        None
    }

    /// Replaces (or inserts) a `FormFill` for `widget` and returns the
    /// previous pending value (if any). Used by the form-fill command's
    /// `apply` / `revert` to make the operation reversible.
    pub fn upsert_form_fill(&mut self, widget: WidgetId, new_value: String) -> Option<String> {
        let page = self.by_page.get_mut(widget.page_index)?;
        for edit in page.iter_mut().rev() {
            if let Edit::FormFill { widget: w, value, .. } = edit {
                if *w == widget {
                    let prev = std::mem::replace(value, new_value);
                    return Some(prev);
                }
            }
        }
        page.push(Edit::FormFill {
            id: EditId::next(),
            widget,
            value: new_value,
        });
        None
    }

    /// Removes any `FormFill` for `widget`. Returns the removed value.
    pub fn remove_form_fill(&mut self, widget: WidgetId) -> Option<String> {
        let page = self.by_page.get_mut(widget.page_index)?;
        let pos = page.iter().position(|edit| {
            matches!(edit, Edit::FormFill { widget: w, .. } if *w == widget)
        })?;
        match page.remove(pos) {
            Edit::FormFill { value, .. } => Some(value),
            other => {
                // Shouldn't happen — we matched FormFill above. Restore and bail.
                self.by_page[widget.page_index].insert(pos, other);
                None
            }
        }
    }

    /// Total edit count across all pages.
    pub fn total(&self) -> usize {
        self.by_page.iter().map(Vec::len).sum()
    }

    /// Iterates every `FormFill` edit in the session. Stable order per call.
    pub fn iter_form_fills(&self) -> impl Iterator<Item = (WidgetId, &str)> {
        self.by_page.iter().flatten().filter_map(|edit| match edit {
            Edit::FormFill { widget, value, .. } => Some((*widget, value.as_str())),
            _ => None,
        })
    }

    // --- Free-text helpers -------------------------------------------------

    /// Adds a free-text box and returns its id.
    pub fn add_free_text(&mut self, b: FreeTextBox) -> EditId {
        let id = b.id;
        if let Some(page) = self.by_page.get_mut(b.page_index) {
            page.push(Edit::FreeText(b));
        }
        id
    }

    /// Removes the free-text box with `id` from `page_index`, returning it.
    pub fn remove_free_text(&mut self, page_index: usize, id: EditId) -> Option<FreeTextBox> {
        let page = self.by_page.get_mut(page_index)?;
        let pos = page
            .iter()
            .position(|e| matches!(e, Edit::FreeText(b) if b.id == id))?;
        match page.remove(pos) {
            Edit::FreeText(b) => Some(b),
            other => {
                self.by_page[page_index].insert(pos, other);
                None
            }
        }
    }

    /// Mutable access to a free-text box by id.
    pub fn free_text_mut(&mut self, page_index: usize, id: EditId) -> Option<&mut FreeTextBox> {
        self.by_page.get_mut(page_index)?.iter_mut().find_map(|e| match e {
            Edit::FreeText(b) if b.id == id => Some(b),
            _ => None,
        })
    }

    /// Iterates the free-text boxes on a page in insertion order.
    pub fn free_texts_on(&self, page_index: usize) -> impl Iterator<Item = &FreeTextBox> {
        self.by_page
            .get(page_index)
            .into_iter()
            .flatten()
            .filter_map(|e| match e {
                Edit::FreeText(b) => Some(b),
                _ => None,
            })
    }

    /// Iterates every free-text box across all pages.
    pub fn iter_free_texts(&self) -> impl Iterator<Item = &FreeTextBox> {
        self.by_page.iter().flatten().filter_map(|e| match e {
            Edit::FreeText(b) => Some(b),
            _ => None,
        })
    }

    // --- Highlight helpers -------------------------------------------------

    /// Adds a highlight and returns its id.
    pub fn add_highlight(&mut self, h: Highlight) -> EditId {
        let id = h.id;
        if let Some(page) = self.by_page.get_mut(h.page_index) {
            page.push(Edit::Highlight(h));
        }
        id
    }

    /// Removes the highlight with `id` from `page_index`, returning it.
    pub fn remove_highlight(&mut self, page_index: usize, id: EditId) -> Option<Highlight> {
        let page = self.by_page.get_mut(page_index)?;
        let pos = page
            .iter()
            .position(|e| matches!(e, Edit::Highlight(h) if h.id == id))?;
        match page.remove(pos) {
            Edit::Highlight(h) => Some(h),
            other => {
                self.by_page[page_index].insert(pos, other);
                None
            }
        }
    }

    /// Iterates the highlights on a page in insertion order.
    pub fn highlights_on(&self, page_index: usize) -> impl Iterator<Item = &Highlight> {
        self.by_page
            .get(page_index)
            .into_iter()
            .flatten()
            .filter_map(|e| match e {
                Edit::Highlight(h) => Some(h),
                _ => None,
            })
    }

    /// Iterates every highlight across all pages.
    pub fn iter_highlights(&self) -> impl Iterator<Item = &Highlight> {
        self.by_page.iter().flatten().filter_map(|e| match e {
            Edit::Highlight(h) => Some(h),
            _ => None,
        })
    }

    // --- Signature helpers -------------------------------------------------

    /// Adds a signature placement and returns its id.
    pub fn add_signature(&mut self, s: SignaturePlacement) -> EditId {
        let id = s.id;
        if let Some(page) = self.by_page.get_mut(s.page_index) {
            page.push(Edit::Signature(s));
        }
        id
    }

    /// Removes the signature with `id` from `page_index`, returning it.
    pub fn remove_signature(&mut self, page_index: usize, id: EditId) -> Option<SignaturePlacement> {
        let page = self.by_page.get_mut(page_index)?;
        let pos = page
            .iter()
            .position(|e| matches!(e, Edit::Signature(s) if s.id == id))?;
        match page.remove(pos) {
            Edit::Signature(s) => Some(s),
            other => {
                self.by_page[page_index].insert(pos, other);
                None
            }
        }
    }

    /// Mutable access to a signature placement by id.
    pub fn signature_mut(
        &mut self,
        page_index: usize,
        id: EditId,
    ) -> Option<&mut SignaturePlacement> {
        self.by_page.get_mut(page_index)?.iter_mut().find_map(|e| match e {
            Edit::Signature(s) if s.id == id => Some(s),
            _ => None,
        })
    }

    /// Iterates the signatures on a page in insertion order.
    pub fn signatures_on(&self, page_index: usize) -> impl Iterator<Item = &SignaturePlacement> {
        self.by_page
            .get(page_index)
            .into_iter()
            .flatten()
            .filter_map(|e| match e {
                Edit::Signature(s) => Some(s),
                _ => None,
            })
    }

    /// Iterates every signature across all pages.
    pub fn iter_signatures(&self) -> impl Iterator<Item = &SignaturePlacement> {
        self.by_page.iter().flatten().filter_map(|e| match e {
            Edit::Signature(s) => Some(s),
            _ => None,
        })
    }
}
