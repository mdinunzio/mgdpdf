//! Thin wrapper around `pdfium_render::PdfDocument` exposing only what the rest
//! of the app needs.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use pdfium_render::prelude::*;

/// Size of a page in PDF points (1/72 inch). Origin is bottom-left in PDF
/// space; the renderer flips for screen output.
#[derive(Copy, Clone, Debug)]
pub struct PageSizePt {
    pub width: f32,
    pub height: f32,
}

pub struct Document {
    pdfium: &'static Pdfium,
    inner: PdfDocument<'static>,
    path: PathBuf,
    page_sizes: Vec<PageSizePt>,
}

impl Document {
    pub fn open(pdfium: &'static Pdfium, path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let inner = pdfium
            .load_pdf_from_file(&path, None)
            .with_context(|| format!("failed to open PDF: {}", path.display()))?;

        let mut page_sizes = Vec::with_capacity(inner.pages().len() as usize);
        for page in inner.pages().iter() {
            page_sizes.push(PageSizePt {
                width: page.width().value,
                height: page.height().value,
            });
        }

        Ok(Self {
            pdfium,
            inner,
            path,
            page_sizes,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn page_count(&self) -> usize {
        self.page_sizes.len()
    }

    pub fn page_size_pt(&self, index: usize) -> Option<PageSizePt> {
        self.page_sizes.get(index).copied()
    }

    /// Renders a single page into an RGBA buffer (premultiplied, top-left origin)
    /// at the requested pixel dimensions.
    pub fn render_page_rgba(
        &self,
        page_index: usize,
        width_px: u32,
        height_px: u32,
    ) -> Result<RgbaImage> {
        let page = self
            .inner
            .pages()
            .get(page_index as i32)
            .with_context(|| format!("page index out of range: {page_index}"))?;

        let config = PdfRenderConfig::new().set_target_size(width_px as Pixels, height_px as Pixels);

        let bitmap = page.render_with_config(&config)?;
        let bytes = bitmap.as_rgba_bytes();
        let actual_w = bitmap.width() as u32;
        let actual_h = bitmap.height() as u32;
        Ok(RgbaImage {
            width: actual_w,
            height: actual_h,
            pixels: bytes,
        })
    }

    /// Suppress the unused-field warning while keeping the binding alive for the
    /// document's whole lifetime.
    #[allow(dead_code)]
    pub(crate) fn pdfium(&self) -> &'static Pdfium {
        self.pdfium
    }
}

/// Owned RGBA bitmap ready for upload to a GPU texture.
pub struct RgbaImage {
    pub width: u32,
    pub height: u32,
    /// Tightly packed `width * height * 4` RGBA bytes (no row padding).
    pub pixels: Vec<u8>,
}
