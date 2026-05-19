//! Page-bitmap rendering cache keyed by `(page_index, zoom_bucket)`.
//!
//! Renders are produced on-demand by [`TextureCache::get_or_render`] and stored
//! as `egui::TextureHandle`s. The cache is bounded in size with simple LRU
//! eviction. Rendering happens synchronously on the UI thread — adequate for
//! the page sizes we deal with; we'll move it to a worker once we see jank.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::Result;
use egui::{ColorImage, Context, TextureHandle, TextureOptions};

use crate::pdf::document::Document;

/// Maximum number of `(page, bucket)` textures kept resident.
const CACHE_CAPACITY: usize = 32;

/// Quantized zoom levels. Render-time zoom is rounded to the nearest bucket so
/// micro-scrolls don't trigger re-renders.
#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
pub struct ZoomBucket(u32);

impl ZoomBucket {
    /// Discrete render scales (multiplied by `pixels_per_point` at render time).
    const LEVELS: &'static [f32] = &[0.5, 0.75, 1.0, 1.25, 1.5, 2.0, 3.0, 4.0];

    pub fn nearest(zoom: f32) -> Self {
        let mut best = 0usize;
        let mut best_d = f32::INFINITY;
        for (i, &lvl) in Self::LEVELS.iter().enumerate() {
            let d = (lvl - zoom).abs();
            if d < best_d {
                best_d = d;
                best = i;
            }
        }
        ZoomBucket(best as u32)
    }

    pub fn scale(self) -> f32 {
        Self::LEVELS[self.0 as usize]
    }
}

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
struct CacheKey {
    page: usize,
    bucket: ZoomBucket,
}

struct CacheEntry {
    texture: TextureHandle,
    /// Logical (point) size of the page; used by callers to lay out the viewport.
    /// We keep it here so the page-view layout doesn't need to round-trip into
    /// `Document` after a render.
    size_pt: [f32; 2],
    last_used: u64,
}

/// Resulting texture + its source page size in PDF points.
#[derive(Clone)]
pub struct PageTexture {
    pub texture: TextureHandle,
    #[allow(dead_code)] // Consumed by edit tools in Phase 2+.
    pub size_pt: [f32; 2],
}

pub struct TextureCache {
    entries: HashMap<CacheKey, CacheEntry>,
    clock: AtomicU64,
}

impl Default for TextureCache {
    fn default() -> Self {
        Self::new()
    }
}

impl TextureCache {
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
            clock: AtomicU64::new(0),
        }
    }

    /// Drops every cached texture. Call when the document changes.
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Returns a texture for `(page, bucket)`, rendering it through PDFium on
    /// cache miss. The render size is `bucket.scale() * pixels_per_point` so
    /// pages stay sharp on HiDPI displays.
    pub fn get_or_render(
        &mut self,
        egui_ctx: &Context,
        doc: &Document,
        page: usize,
        bucket: ZoomBucket,
        pixels_per_point: f32,
    ) -> Result<PageTexture> {
        let key = CacheKey { page, bucket };
        let now = self.clock.fetch_add(1, Ordering::Relaxed);

        if let Some(entry) = self.entries.get_mut(&key) {
            entry.last_used = now;
            return Ok(PageTexture {
                texture: entry.texture.clone(),
                size_pt: entry.size_pt,
            });
        }

        let Some(page_size) = doc.page_size_pt(page) else {
            anyhow::bail!("page index out of range: {page}");
        };

        let render_scale = bucket.scale() * pixels_per_point;
        // PDF "point" is 1/72". egui pixel size of a page is `pt * pixels_per_point`.
        // We want PDFium to produce a bitmap matching the on-screen pixel count
        // scaled by the bucket's logical zoom.
        let width_px = (page_size.width * render_scale).max(1.0).round() as u32;
        let height_px = (page_size.height * render_scale).max(1.0).round() as u32;

        let rgba = doc.render_page_rgba(page, width_px, height_px)?;
        let color_image = ColorImage::from_rgba_unmultiplied(
            [rgba.width as usize, rgba.height as usize],
            &rgba.pixels,
        );
        let texture = egui_ctx.load_texture(
            format!("pdf_page_{page}_b{}", bucket.0),
            color_image,
            TextureOptions::LINEAR,
        );

        let entry = CacheEntry {
            texture: texture.clone(),
            size_pt: [page_size.width, page_size.height],
            last_used: now,
        };
        self.entries.insert(key, entry);
        self.evict_if_needed();

        Ok(PageTexture {
            texture,
            size_pt: [page_size.width, page_size.height],
        })
    }

    fn evict_if_needed(&mut self) {
        while self.entries.len() > CACHE_CAPACITY {
            // Linear scan is fine at this capacity. If we ever raise it past a
            // few hundred, swap in an LRU crate.
            let Some((&oldest_key, _)) = self
                .entries
                .iter()
                .min_by_key(|(_, e)| e.last_used)
            else {
                break;
            };
            self.entries.remove(&oldest_key);
        }
    }
}
