//! Concrete [`Command`] implementations. Each editing feature adds its own.
//!
//! Pattern: every command stores the *prior* state it overwrote so that
//! `revert` restores it exactly. Commands that only mutate the in-memory
//! [`EditSession`] don't need to touch PDFium until save — that keeps undo/redo
//! O(1) and prevents the PDFium handle from being mutated while typing.

pub mod fill_form_field;
pub mod free_text;
pub mod highlight;

pub use fill_form_field::FillFormFieldCommand;
pub use free_text::{AddFreeTextCommand, EditFreeTextCommand, MoveFreeTextCommand};
pub use highlight::AddHighlightCommand;
