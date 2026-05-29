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

/// A single glyph's bounding box in PDF points (bottom-left origin). Used by
/// the highlight tool to hit-test a drag selection against the text layer.
#[derive(Copy, Clone, Debug)]
pub struct GlyphRect {
    pub rect_pt: PdfRectPt,
    /// `true` for whitespace/control chars — usually excluded from highlights.
    pub is_whitespace: bool,
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

    /// Returns each glyph's bounding box on `page_index`, in PDF points. Empty
    /// for pages with no text layer (e.g. scanned images) — the highlight tool
    /// uses that emptiness to fall back to a free-drawn rectangle.
    pub fn collect_glyph_rects(&self, page_index: usize) -> Vec<GlyphRect> {
        let Ok(page) = self.inner.pages().get(page_index as i32) else {
            return Vec::new();
        };
        let Ok(text) = page.text() else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for ch in text.chars().iter() {
            let Ok(bounds) = ch.loose_bounds() else {
                continue;
            };
            let is_whitespace = ch
                .unicode_char()
                .map(|c| c.is_whitespace())
                .unwrap_or(true);
            out.push(GlyphRect {
                rect_pt: PdfRectPt {
                    left: bounds.left().value,
                    bottom: bounds.bottom().value,
                    right: bounds.right().value,
                    top: bounds.top().value,
                },
                is_whitespace,
            });
        }
        out
    }

    /// `true` if `page_index` has any extractable text (a text layer).
    #[allow(dead_code)]
    pub fn page_has_text(&self, page_index: usize) -> bool {
        let Ok(page) = self.inner.pages().get(page_index as i32) else {
            return false;
        };
        let has_text = match page.text() {
            Ok(t) => t.len() > 0,
            Err(_) => false,
        };
        has_text
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
        // Highlights first so they sit *under* any free text we add, and under
        // the page's existing content where possible.
        for hl in &edits.highlights {
            add_highlight_on(&mut scratch, hl)?;
        }
        for ft in &edits.free_texts {
            add_free_text_on(&mut scratch, ft)?;
        }
        for sig in &edits.signatures {
            add_signature_on(&mut scratch, sig)?;
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
    pub font_size: f32,
    pub color: [u8; 4],
}

/// A highlight to stamp onto a page: one or more translucent rectangles
/// (PDF-space, bottom-left origin) sharing a colour. Text selection yields one
/// rect per line; the scanned-page fallback yields a single dragged rect.
#[derive(Clone, Debug)]
pub struct HighlightSpec {
    pub page_index: usize,
    pub rects_pt: Vec<PdfRectPt>,
    /// RGBA; alpha controls translucency of the highlight fill.
    pub color: [u8; 4],
}

/// A signature to stamp onto a page: a transparent RGBA image placed at a
/// rectangle (PDF points, top-left origin).
#[derive(Clone)]
pub struct SignatureSpec {
    pub page_index: usize,
    /// Top-left corner in PDF points.
    pub origin_pt: [f32; 2],
    /// Rendered size in PDF points (width, height).
    pub size_pt: [f32; 2],
    pub image: std::sync::Arc<image::RgbaImage>,
}

/// All edits to apply to a fresh copy of the source PDF on save.
#[derive(Default)]
pub struct EditBundle {
    pub form_fills: Vec<(WidgetId, String)>,
    pub free_texts: Vec<FreeTextSpec>,
    pub highlights: Vec<HighlightSpec>,
    pub signatures: Vec<SignatureSpec>,
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
    // We draw the text as a real page *content* text object rather than a
    // free-text annotation. PDFium's C API can't generate an appearance stream
    // for free-text annotations, so annotation-based text renders in PDFium-
    // based viewers (including ours) but is invisible in Adobe and others.
    // A page text object is part of the page's drawing instructions, so it
    // renders identically everywhere.
    let font = doc.fonts_mut().helvetica();

    let mut page = doc
        .pages_mut()
        .get(spec.page_index as i32)
        .with_context(|| format!("page index out of range: {}", spec.page_index))?;

    // Regenerate page content on change so the new object is baked into the
    // saved content stream.
    page.set_content_regeneration_strategy(
        PdfPageContentRegenerationStrategy::AutomaticOnEveryChange,
    );

    // `origin_pt` is the box's top-left in PDF points; PDF text is positioned
    // by its baseline, which sits ~`font_size` below the top.
    let baseline_x = spec.origin_pt[0];
    let baseline_y = spec.origin_pt[1] - spec.font_size;

    let object = page.objects_mut().create_text_object(
        PdfPoints::new(baseline_x),
        PdfPoints::new(baseline_y),
        &spec.text,
        font,
        PdfPoints::new(spec.font_size),
    )?;
    // `create_text_object` returns the object already attached to the page.
    let mut object = object;
    object.set_fill_color(PdfColor::new(
        spec.color[0],
        spec.color[1],
        spec.color[2],
        spec.color[3],
    ))?;

    page.regenerate_content()?;
    Ok(())
}

fn add_signature_on(doc: &mut PdfDocument<'_>, spec: &SignatureSpec) -> Result<()> {
    // Stamp the signature as a page content image object (renders in every
    // viewer, with transparency preserved via the image's alpha channel).
    let dynamic = image::DynamicImage::ImageRgba8((*spec.image).clone());

    let left = spec.origin_pt[0];
    let top = spec.origin_pt[1];
    let bottom = top - spec.size_pt[1];

    let mut page = doc
        .pages_mut()
        .get(spec.page_index as i32)
        .with_context(|| format!("page index out of range: {}", spec.page_index))?;
    page.set_content_regeneration_strategy(
        PdfPageContentRegenerationStrategy::AutomaticOnEveryChange,
    );
    page.objects_mut().create_image_object(
        PdfPoints::new(left),
        PdfPoints::new(bottom),
        &dynamic,
        Some(PdfPoints::new(spec.size_pt[0])),
        Some(PdfPoints::new(spec.size_pt[1])),
    )?;
    page.regenerate_content()?;
    Ok(())
}

fn add_highlight_on(doc: &mut PdfDocument<'_>, spec: &HighlightSpec) -> Result<()> {
    // Highlights are drawn as translucent filled rectangles in the page's
    // content stream — same cross-viewer-safe approach as free text. (PDFium
    // can't generate appearance streams for highlight *annotations*, so those
    // would be invisible in Adobe.)
    let fill = PdfColor::new(spec.color[0], spec.color[1], spec.color[2], spec.color[3]);

    // Build detached path objects first (each borrows `doc` only briefly), then
    // attach them to the page — avoids holding a page borrow across `new_rect`.
    let mut objects = Vec::with_capacity(spec.rects_pt.len());
    for r in &spec.rects_pt {
        let rect = PdfRect::new(
            PdfPoints::new(r.bottom),
            PdfPoints::new(r.left),
            PdfPoints::new(r.top),
            PdfPoints::new(r.right),
        );
        let object = PdfPagePathObject::new_rect(doc, rect, None, None, Some(fill))?;
        objects.push(object);
    }

    let mut page = doc
        .pages_mut()
        .get(spec.page_index as i32)
        .with_context(|| format!("page index out of range: {}", spec.page_index))?;
    page.set_content_regeneration_strategy(
        PdfPageContentRegenerationStrategy::AutomaticOnEveryChange,
    );
    for object in objects {
        page.objects_mut().add_path_object(object)?;
    }
    page.regenerate_content()?;
    Ok(())
}
