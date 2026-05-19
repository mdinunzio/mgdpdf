//! Scrollable multi-page viewport. Lays pages vertically, requests textures
//! from the cache, and paints them. Stays read-only in Phase 1 — edit overlays
//! are added in later phases.

use eframe::egui;
use egui::{Color32, Pos2, Rect, ScrollArea, Sense, Stroke, StrokeKind, Vec2};

use crate::pdf::document::Document;
use crate::pdf::render::{TextureCache, ZoomBucket};

/// Space between consecutive pages, in logical pixels.
const PAGE_GAP: f32 = 12.0;

pub struct PageView;

impl PageView {
    /// Renders the multi-page scroll view and returns the page index closest
    /// to the centre of the viewport (useful for "go to page" status).
    pub fn show(
        ui: &mut egui::Ui,
        doc: &Document,
        cache: &mut TextureCache,
        zoom: f32,
    ) -> usize {
        let pixels_per_point = ui.ctx().pixels_per_point();
        let bucket = ZoomBucket::nearest(zoom);

        let mut current_page = 0usize;

        ScrollArea::both()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let available_w = ui.available_width();
                ui.vertical_centered(|ui| {
                    for page_index in 0..doc.page_count() {
                        let Some(size_pt) = doc.page_size_pt(page_index) else {
                            continue;
                        };
                        let logical_w = size_pt.width * zoom;
                        let logical_h = size_pt.height * zoom;

                        // Reserve space for the page-sized rectangle.
                        let desired = Vec2::new(logical_w, logical_h);
                        let (rect, _resp) = ui.allocate_exact_size(desired, Sense::hover());

                        // Track the page nearest to the vertical centre of the visible viewport.
                        if let Some(visible) = visible_rect(ui) {
                            if rect.center().y >= visible.top()
                                && rect.center().y <= visible.bottom()
                            {
                                current_page = page_index;
                            }
                        }

                        // Skip rendering off-screen pages — saves the texture upload and keeps
                        // the cache warm for what's actually visible.
                        if !is_in_viewport(ui, rect) {
                            paint_placeholder(ui, rect);
                            ui.add_space(PAGE_GAP);
                            continue;
                        }

                        match cache.get_or_render(
                            ui.ctx(),
                            doc,
                            page_index,
                            bucket,
                            pixels_per_point,
                        ) {
                            Ok(page_tex) => {
                                let painter = ui.painter_at(rect);
                                painter.image(
                                    page_tex.texture.id(),
                                    rect,
                                    Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                                    Color32::WHITE,
                                );
                                // Subtle page border so white pages don't blend into the bg.
                                painter.rect_stroke(
                                    rect,
                                    0.0,
                                    Stroke::new(1.0, Color32::from_gray(180)),
                                    StrokeKind::Outside,
                                );
                            }
                            Err(_) => paint_placeholder(ui, rect),
                        }

                        ui.add_space(PAGE_GAP);
                    }

                    // Mute the unused-width warning by referencing it.
                    let _ = available_w;
                });
            });

        current_page
    }
}

fn visible_rect(ui: &egui::Ui) -> Option<Rect> {
    Some(ui.clip_rect())
}

fn is_in_viewport(ui: &egui::Ui, rect: Rect) -> bool {
    let clip = ui.clip_rect();
    // Add a one-page margin so we pre-render the next page just below the fold.
    let expanded = clip.expand(rect.height().min(1500.0));
    expanded.intersects(rect)
}

fn paint_placeholder(ui: &egui::Ui, rect: Rect) {
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 0.0, Color32::from_gray(245));
    painter.rect_stroke(
        rect,
        0.0,
        Stroke::new(1.0, Color32::from_gray(200)),
        StrokeKind::Outside,
    );
}
