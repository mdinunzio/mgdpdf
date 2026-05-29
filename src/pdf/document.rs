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

/// PDF-space axis-aligned rect with origin at the **bottom-left** of the page.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct PdfRectPt {
    pub left: f32,
    pub bottom: f32,
    pub right: f32,
    pub top: f32,
}

/// Stable identity for a widget annotation within an open document. The
/// annotation index is local to a page; combined with `page_index` it uniquely
/// identifies a widget for the lifetime of this `Document`.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct WidgetId {
    pub page_index: usize,
    pub annotation_index: u32,
}

/// Read-only snapshot of a single text-field widget. Authored once at document
/// open by [`Document::collect_text_widgets`]; later updates flow back through
/// [`Document::set_text_field_value`].
#[derive(Clone, Debug)]
pub struct TextFieldWidget {
    pub id: WidgetId,
    /// Optional `/T` field name from the PDF. Unnamed fields have `None`.
    /// Kept for diagnostics and future field-by-name lookups.
    #[allow(dead_code)]
    pub name: Option<String>,
    /// Bounding rect in PDF points.
    pub rect_pt: PdfRectPt,
    /// Current value as PDFium sees it (may be the default placeholder).
    pub value: String,
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

    /// `true` if the document carries an interactive form (AcroForm or XFA).
    /// Surfaced in the UI by Phase 3's polish pass (status-bar hint) — until
    /// then it's read by external callers / tests only.
    #[allow(dead_code)]
    pub fn has_form(&self) -> bool {
        self.inner.form().is_some()
    }

    /// Walks every page's annotations and returns the text widgets it finds.
    /// Pure-read; safe to call any time after open.
    pub fn collect_text_widgets(&self) -> Vec<TextFieldWidget> {
        let mut out = Vec::new();
        let pages = self.inner.pages();
        for page_index in 0..pages.len() {
            let Ok(page) = pages.get(page_index as i32) else {
                continue;
            };
            for (annotation_index, annotation) in page.annotations().iter().enumerate() {
                if let PdfPageAnnotation::Widget(ref w) = annotation {
                    let Some(PdfFormField::Text(t)) = w.form_field() else {
                        continue;
                    };
                    let Ok(bounds) = w.bounds() else {
                        continue;
                    };
                    out.push(TextFieldWidget {
                        id: WidgetId {
                            page_index: page_index as usize,
                            annotation_index: annotation_index as u32,
                        },
                        name: t.name(),
                        rect_pt: PdfRectPt {
                            left: bounds.left().value,
                            bottom: bounds.bottom().value,
                            right: bounds.right().value,
                            top: bounds.top().value,
                        },
                        value: t.value().unwrap_or_default(),
                    });
                }
            }
        }
        out
    }

    /// Writes `value` into the text field identified by `id`. Persisted to disk
    /// only on [`Document::save_as`]. Returns an error if the widget does not
    /// resolve to a text field.
    pub fn set_text_field_value(&mut self, id: WidgetId, value: &str) -> Result<()> {
        set_text_field_value_on(&mut self.inner, id, value)
    }

    /// Number of annotations on a page — used by tests to verify a stamp landed.
    #[allow(dead_code)]
    pub fn annotation_count(&self, page_index: usize) -> usize {
        self.inner
            .pages()
            .get(page_index as i32)
            .map(|p| p.annotations().len())
            .unwrap_or(0)
    }

    /// Contents of every free-text annotation on a page — used by tests to
    /// verify that typed text actually round-trips to the saved file.
    #[allow(dead_code)]
    pub fn collect_free_text_contents(&self, page_index: usize) -> Vec<String> {
        let Ok(page) = self.inner.pages().get(page_index as i32) else {
            return Vec::new();
        };
        page.annotations()
            .iter()
            .filter_map(|a| match a {
                PdfPageAnnotation::FreeText(ref ft) => Some(ft.contents().unwrap_or_default()),
                _ => None,
            })
            .collect()
    }

    /// Writes the in-memory document to a new file. Does not modify the source
    /// file; the caller is responsible for tracking whether `path` matches
    /// `self.path()` (i.e. "Save" vs "Save As").
    pub fn save_as(&self, path: impl AsRef<Path>) -> Result<()> {
        let path = path.as_ref();
        self.inner
            .save_to_file(path)
            .with_context(|| format!("failed to save PDF: {}", path.display()))?;
        Ok(())
    }

    /// Applies a set of edits to a **fresh copy** of the source PDF and writes
    /// it to `out_path`. The working document (`self`) is left untouched, so
    /// saving is idempotent and repeatable — calling it twice does not stamp
    /// annotations twice. Form-fill values are idempotent; free-text boxes are
    /// additive, which is why we must start from a clean copy each time.
    pub fn save_with_edits(&self, out_path: impl AsRef<Path>, edits: &EditBundle) -> Result<()> {
        let out_path = out_path.as_ref();

        // Re-open the original from disk into a scratch document.
        let mut scratch = self
            .pdfium
            .load_pdf_from_file(&self.path, None)
            .with_context(|| format!("failed to reopen source PDF: {}", self.path.display()))?;

        for (widget, value) in &edits.form_fills {
            set_text_field_value_on(&mut scratch, *widget, value)?;
        }
        for ft in &edits.free_texts {
            add_free_text_on(&mut scratch, ft)?;
        }

        scratch
            .save_to_file(out_path)
            .with_context(|| format!("failed to save PDF: {}", out_path.display()))?;
        Ok(())
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

/// A free-text box to stamp onto a page, in PDF-space terms. Decouples the
/// `pdf` layer from the `edit` layer — `App` translates `edit::FreeTextBox`
/// into this on save.
#[derive(Clone, Debug)]
pub struct FreeTextSpec {
    pub page_index: usize,
    /// Top-left corner in PDF points.
    pub origin_pt: [f32; 2],
    /// Box size in PDF points (width, height).
    pub size_pt: [f32; 2],
    pub text: String,
    pub color: [u8; 4],
}

/// All edits to apply to a fresh copy of the source PDF on save.
#[derive(Default)]
pub struct EditBundle {
    pub form_fills: Vec<(WidgetId, String)>,
    pub free_texts: Vec<FreeTextSpec>,
}

fn set_text_field_value_on(
    doc: &mut PdfDocument<'_>,
    id: WidgetId,
    value: &str,
) -> Result<()> {
    let mut page = doc
        .pages_mut()
        .get(id.page_index as i32)
        .with_context(|| format!("page index out of range: {}", id.page_index))?;
    let annotations = page.annotations_mut();
    let mut annotation = annotations
        .get(id.annotation_index as usize)
        .with_context(|| {
            format!(
                "annotation index out of range: page {} annotation {}",
                id.page_index, id.annotation_index
            )
        })?;
    match &mut annotation {
        PdfPageAnnotation::Widget(w) => {
            let Some(field) = w.form_field_mut() else {
                anyhow::bail!("widget at {:?} has no form field", id);
            };
            match field {
                PdfFormField::Text(t) => {
                    t.set_value(value)?;
                    Ok(())
                }
                _ => anyhow::bail!("widget at {:?} is not a text field", id),
            }
        }
        _ => anyhow::bail!("annotation at {:?} is not a widget", id),
    }
}

fn add_free_text_on(doc: &mut PdfDocument<'_>, spec: &FreeTextSpec) -> Result<()> {
    let mut page = doc
        .pages_mut()
        .get(spec.page_index as i32)
        .with_context(|| format!("page index out of range: {}", spec.page_index))?;

    // Pages opened from disk start in `Manual` content-regeneration mode, so a
    // newly-created annotation is stored but PDFium never bakes its appearance
    // stream — the text saves invisibly. Switching to AutomaticOnEveryChange
    // makes pdfium-render regenerate the page content after each annotation
    // mutation, so the free text actually renders in the saved file.
    page.set_content_regeneration_strategy(
        PdfPageContentRegenerationStrategy::AutomaticOnEveryChange,
    );

    let left = spec.origin_pt[0];
    let top = spec.origin_pt[1];
    let right = left + spec.size_pt[0];
    let bottom = top - spec.size_pt[1];

    let annotations = page.annotations_mut();
    let mut annotation = annotations.create_free_text_annotation(&spec.text)?;
    annotation.set_bounds(PdfRect::new(
        PdfPoints::new(bottom),
        PdfPoints::new(left),
        PdfPoints::new(top),
        PdfPoints::new(right),
    ))?;
    annotation.set_fill_color(PdfColor::new(
        spec.color[0],
        spec.color[1],
        spec.color[2],
        spec.color[3],
    ))?;
    annotation.set_contents(&spec.text)?;

    // Force a final regeneration in case the strategy was applied after the
    // create call's internal check.
    page.regenerate_content()?;
    Ok(())
}
