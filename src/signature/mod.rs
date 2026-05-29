//! Signature capture and persistence.
//!
//! A signature is just an RGBA image with a transparent background — ink where
//! the user drew/typed, transparent elsewhere. It can come from three sources:
//!   * **draw**: a set of mouse-stroke polylines rasterised with `tiny-skia`;
//!   * **type**: a name rendered in a bundled script font via `ab_glyph`;
//!   * **upload**: a PNG/JPG loaded from disk (white background auto-keyed to
//!     transparent so it composites cleanly over the page).
//!
//! Saved signatures live as PNGs under the user's data dir so they can be
//! reused across sessions. On save to a PDF the image is stamped as a page
//! content image object (renders in every viewer; see `pdf::document`).

pub mod library;
pub mod render;

pub use library::SignatureLibrary;
pub use render::{rasterize_strokes, render_typed_name, prepare_uploaded};

use image::RgbaImage;

/// A captured signature ready to stamp: an owned RGBA bitmap (transparent bg).
#[derive(Clone)]
pub struct Signature {
    pub image: RgbaImage,
}

impl Signature {
    pub fn new(image: RgbaImage) -> Self {
        Self { image }
    }

    pub fn width(&self) -> u32 {
        self.image.width()
    }

    pub fn height(&self) -> u32 {
        self.image.height()
    }

    /// Aspect ratio (width / height); 1.0 if degenerate.
    pub fn aspect(&self) -> f32 {
        let h = self.image.height().max(1);
        self.image.width() as f32 / h as f32
    }
}
