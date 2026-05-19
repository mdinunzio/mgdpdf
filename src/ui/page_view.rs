//! Scrollable multi-page viewport. Lays pages vertically, requests textures
//! from the cache, paints each page, runs the active tool's overlay, and
//! dispatches pointer events to the tool for the page under the cursor.

use eframe::egui;
use egui::{Color32, Pos2, Rect, ScrollArea, Sense, Stroke, StrokeKind, Vec2};

use crate::edit::EditSession;
use crate::pdf::coords::PageTransform;
use crate::pdf::document::Document;
use crate::pdf::render::{TextureCache, ZoomBucket};
use crate::tools::{ToolCtx, ToolEvent, ToolBox};

/// Space between consecutive pages, in logical pixels.
const PAGE_GAP: f32 = 12.0;

pub struct PageView;

pub struct PageViewState<'a> {
    pub doc: &'a Document,
    pub cache: &'a mut TextureCache,
    pub zoom: f32,
    pub tools: &'a mut ToolBox,
    pub session: &'a mut EditSession,
    pub undo: &'a mut crate::edit::UndoStack,
}

impl PageView {
    /// Renders the multi-page scroll view and returns the page index closest
    /// to the centre of the viewport.
    pub fn show(ui: &mut egui::Ui, state: PageViewState<'_>) -> usize {
        let PageViewState {
            doc,
            cache,
            zoom,
            tools,
            session,
            undo,
        } = state;

        let pixels_per_point = ui.ctx().pixels_per_point();
        let bucket = ZoomBucket::nearest(zoom);

        let mut current_page = 0usize;

        ScrollArea::both()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.vertical_centered(|ui| {
                    for page_index in 0..doc.page_count() {
                        let Some(size_pt) = doc.page_size_pt(page_index) else {
                            continue;
                        };
                        let logical_w = size_pt.width * zoom;
                        let logical_h = size_pt.height * zoom;
                        let desired = Vec2::new(logical_w, logical_h);

                        // Sense::click_and_drag lets us capture pointer events
                        // on the page even while the scroll view is active.
                        let (rect, response) = ui.allocate_exact_size(
                            desired,
                            Sense::click_and_drag(),
                        );

                        let transform = PageTransform::new(
                            Vec2::new(size_pt.width, size_pt.height),
                            rect,
                        );

                        // Track viewport-centre page for the status bar.
                        let visible = ui.clip_rect();
                        if rect.center().y >= visible.top()
                            && rect.center().y <= visible.bottom()
                        {
                            current_page = page_index;
                        }

                        // Cheap visibility check: skip painting (and texture
                        // upload) for pages outside the viewport + one screen
                        // of margin in either direction.
                        if !is_in_viewport(visible, rect) {
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
                                    Rect::from_min_max(
                                        Pos2::ZERO,
                                        Pos2::new(1.0, 1.0),
                                    ),
                                    Color32::WHITE,
                                );
                                painter.rect_stroke(
                                    rect,
                                    0.0,
                                    Stroke::new(1.0, Color32::from_gray(180)),
                                    StrokeKind::Outside,
                                );

                                // Tool overlay (drawn on top of the bitmap).
                                tools.active().draw_overlay(
                                    page_index,
                                    &painter,
                                    &transform,
                                    session,
                                );
                            }
                            Err(_) => paint_placeholder(ui, rect),
                        }

                        // Dispatch pointer events to the active tool.
                        dispatch_pointer_events(
                            page_index,
                            &response,
                            &transform,
                            tools,
                            session,
                            undo,
                        );

                        ui.add_space(PAGE_GAP);
                    }
                });
            });

        current_page
    }
}

fn dispatch_pointer_events(
    page_index: usize,
    response: &egui::Response,
    transform: &PageTransform,
    tools: &mut ToolBox,
    session: &mut EditSession,
    undo: &mut crate::edit::UndoStack,
) {
    let mut ctx = ToolCtx { session, undo };
    if response.hovered() {
        if let Some(screen) = response.hover_pos() {
            let pdf = transform.screen_to_pdf(screen);
            tools
                .active_mut()
                .on_event(page_index, ToolEvent::PointerMove { pdf }, &mut ctx);
        }
    } else {
        tools
            .active_mut()
            .on_event(page_index, ToolEvent::PointerLeave, &mut ctx);
    }
    if response.drag_started() || response.clicked() {
        if let Some(screen) = response.interact_pointer_pos() {
            let pdf = transform.screen_to_pdf(screen);
            tools
                .active_mut()
                .on_event(page_index, ToolEvent::PointerDown { pdf }, &mut ctx);
        }
    }
    if response.drag_stopped() {
        if let Some(screen) = response.interact_pointer_pos() {
            let pdf = transform.screen_to_pdf(screen);
            tools
                .active_mut()
                .on_event(page_index, ToolEvent::PointerUp { pdf }, &mut ctx);
        }
    }
}

fn is_in_viewport(clip: Rect, rect: Rect) -> bool {
    // Add a margin so we render the next page just below the fold.
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
