//! PDF engine wrapper: document loading, page rendering, texture caching.
//!
//! Everything that talks to `pdfium-render` lives below this module. The rest of
//! the app sees PDFs as `Document` (page metadata + handles) plus rendered
//! `PageTexture`s from the [`render`] cache.

pub mod coords;
pub mod document;
pub mod forms;
pub mod render;

pub use coords::PageTransform;
pub use document::Document;
pub use render::TextureCache;
