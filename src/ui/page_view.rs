//! Scrollable multi-page viewport. Lays pages vertically, requests textures
//! from the cache, paints each page, runs the active tool's overlay, and
//! dispatches pointer events to the tool for the page under the cursor.

use eframe::egui;
use egui::{Color32, Pos2, Rect, ScrollArea, Stroke, StrokeKind, Vec2};

use crate::edit::EditSession;
use crate::pdf::coords::PageTransform;
use crate::pdf::document::{Document, TextFieldWidget};
use crate::pdf::render::{TextureCache, ZoomBucket};
use crate::tools::{ToolBox, ToolCtx, ToolEvent, ToolSettings};

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
    pub widgets: &'a [TextFieldWidget],
    pub settings: ToolSettings,
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
            widgets,
            settings,
        } = state;

        let pixels_per_point = ui.ctx().pixels_per_point();
        let bucket = ZoomBucket::nearest(zoom);

        let mut current_page = 0usize;

        // Pre-compute each page's logical (point*zoom) height and the running
        // y-offset of its top edge within the scroll content. This lets us tell
        // the ScrollArea the exact total height up front (so the scrollbar is
        // correct) and position pages by absolute offset rather than relying on
        // sequential `allocate` calls — which is what `show_viewport` wants.
        let page_count = doc.page_count();
        let mut tops = Vec::with_capacity(page_count);
        let mut sizes = Vec::with_capacity(page_count);
        let mut running = 0.0f32;
        for i in 0..page_count {
            let size = doc.page_size_pt(i).unwrap_or(crate::pdf::document::PageSizePt {
                width: 612.0,
                height: 792.0,
            });
            let logical = Vec2::new(size.width * zoom, size.height * zoom);
            tops.push(running);
            sizes.push((size, logical));
            running += logical.y + PAGE_GAP;
        }
        let total_height = running.max(0.0);

        ScrollArea::vertical()
            .auto_shrink([false, false])
            .show_viewport(ui, |ui, viewport| {
                // Reserve the full scrollable height so the scrollbar tracks the
                // whole document, not just the visible page.
                ui.set_height(total_height);
                let content_top = ui.min_rect().top();
                let viewport_width = ui.available_width();

                for page_index in 0..page_count {
                    let (size_pt, logical) = sizes[page_index];
                    let top_y = content_top + tops[page_index];
                    let left_pad = ((viewport_width - logical.x) * 0.5).max(0.0);
                    let min = Pos2::new(ui.min_rect().left() + left_pad, top_y);
                    let rect = Rect::from_min_size(min, logical);

                    // Cull pages outside the visible viewport (+ a margin). The
                    // viewport rect is content-relative, so compare against the
                    // page's content-relative band.
                    let page_band_top = tops[page_index];
                    let page_band_bottom = page_band_top + logical.y;
                    let margin = viewport.height();
                    let visible = page_band_bottom >= viewport.min.y - margin
                        && page_band_top <= viewport.max.y + margin;
                    if !visible {
                        continue;
                    }

                    let transform = PageTransform::new(
                        Vec2::new(size_pt.width, size_pt.height),
                        rect,
                    );

                    // Page nearest the viewport centre → status bar.
                    let viewport_centre = (viewport.min.y + viewport.max.y) * 0.5;
                    if page_band_top <= viewport_centre && page_band_bottom >= viewport_centre {
                        current_page = page_index;
                    }

                    // Interaction surface for this page (sense set by the tool).
                    let sense = tools.active().page_sense();
                    let response = ui.interact(
                        rect,
                        egui::Id::new(("mgdpdf::page", page_index)),
                        sense,
                    );

                    match cache.get_or_render(ui.ctx(), doc, page_index, bucket, pixels_per_point) {
                        Ok(page_tex) => {
                            let painter = ui.painter_at(rect);
                            painter.image(
                                page_tex.texture.id(),
                                rect,
                                Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                                Color32::WHITE,
                            );
                            painter.rect_stroke(
                                rect,
                                0.0,
                                Stroke::new(1.0, Color32::from_gray(180)),
                                StrokeKind::Outside,
                            );
                            tools.active().draw_overlay(page_index, &painter, &transform, session);
                        }
                        Err(_) => paint_placeholder(ui, rect),
                    }

                    // Interactive overlay (text inputs, drag handles, etc).
                    ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
                        let mut ctx = ToolCtx {
                            session,
                            undo,
                            widgets,
                            settings,
                        };
                        tools
                            .active_mut()
                            .draw_interactive(page_index, ui, &transform, &mut ctx);
                    });

                    dispatch_pointer_events(
                        page_index, &response, &transform, tools, session, undo, widgets, settings,
                    );
                }
            });

        current_page
    }
}

#[allow(clippy::too_many_arguments)]
fn dispatch_pointer_events(
    page_index: usize,
    response: &egui::Response,
    transform: &PageTransform,
    tools: &mut ToolBox,
    session: &mut EditSession,
    undo: &mut crate::edit::UndoStack,
    widgets: &[TextFieldWidget],
    settings: ToolSettings,
) {
    let mut ctx = ToolCtx {
        session,
        undo,
        widgets,
        settings,
    };
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
