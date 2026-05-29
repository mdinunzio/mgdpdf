//! Signature tool: place a captured signature on a page (click), then drag the
//! grip to move it. The signature image itself is captured by the modal in
//! `ui::signature_modal`; this tool consumes the pending image and places it.
//!
//! Placed signatures are rendered (as textures) by `page_view`, and committed
//! to the saved PDF as page content image objects (see `pdf::document`).

use eframe::egui;
use egui::{Color32, Pos2, Rect, Sense, StrokeKind, Ui, Vec2};

use crate::edit::commands::{AddSignatureCommand, MoveSignatureCommand, ResizeSignatureCommand};
use crate::edit::{EditId, SignaturePlacement};
use crate::pdf::PageTransform;

use super::{Tool, ToolCtx, ToolEvent};

/// Default placement width in PDF points (height follows image aspect).
const DEFAULT_WIDTH_PT: f32 = 150.0;
/// Side length of the square grips, in screen pixels.
const GRIP_PX: f32 = 12.0;
/// Minimum signature width in PDF points (keeps it grabbable).
const MIN_WIDTH_PT: f32 = 24.0;

#[derive(Default)]
pub struct SignatureTool {
    /// Box being moved + its in-progress origin (PDF pts).
    dragging: Option<(EditId, [f32; 2])>,
    /// Box being resized: (id, pre-drag size, in-progress size) in PDF pts.
    resizing: Option<(EditId, [f32; 2], [f32; 2])>,
}

impl Tool for SignatureTool {
    fn id(&self) -> &'static str {
        "signature"
    }

    fn label(&self) -> &'static str {
        "Signature"
    }

    fn page_sense(&self) -> Sense {
        Sense::click_and_drag()
    }

    fn on_event(&mut self, page_index: usize, event: ToolEvent, ctx: &mut ToolCtx<'_>) {
        // Place the pending signature (if any) where the user clicks.
        if let ToolEvent::PointerDown { pdf } = event {
            if let Some(image) = ctx.pending_signature.take() {
                let aspect = image.height().max(1) as f32 / image.width().max(1) as f32;
                let w = DEFAULT_WIDTH_PT;
                let h = w * aspect;
                let placement = SignaturePlacement {
                    id: EditId::next(),
                    page_index,
                    // Centre the signature on the click point.
                    origin_pt: [pdf.x - w / 2.0, pdf.y + h / 2.0],
                    size_pt: [w, h],
                    image,
                };
                ctx.run(AddSignatureCommand::new(placement));
            }
        }
    }

    fn draw_interactive(
        &mut self,
        page_index: usize,
        ui: &mut Ui,
        transform: &PageTransform,
        ctx: &mut ToolCtx<'_>,
    ) {
        let ids: Vec<EditId> = ctx
            .session
            .signatures_on(page_index)
            .map(|s| s.id)
            .collect();

        for id in ids {
            let Some(s) = ctx.session.signature_mut(page_index, id).map(|s| s.clone()) else {
                continue;
            };
            let aspect = s.image.height().max(1) as f32 / s.image.width().max(1) as f32;

            // Live origin (while moving) and live size (while resizing).
            let origin = match self.dragging {
                Some((d, live)) if d == id => live,
                _ => s.origin_pt,
            };
            let size = match self.resizing {
                Some((d, _start, live)) if d == id => live,
                _ => s.size_pt,
            };
            let screen_rect = transform.pdf_rect_to_screen(
                origin[0],
                origin[1] - size[1],
                origin[0] + size[0],
                origin[1],
            );

            // Selection border (the image itself is drawn by page_view).
            ui.painter_at(screen_rect).rect_stroke(
                screen_rect,
                2.0,
                egui::Stroke::new(1.0, Color32::from_rgb(90, 140, 220)),
                StrokeKind::Inside,
            );

            let edit_id = egui::Id::new(("mgdpdf::signature", id));
            let zoom = transform.zoom().max(f32::EPSILON);

            // --- Move grip (top-left) ---
            let move_grip = Rect::from_min_size(
                Pos2::new(screen_rect.min.x - GRIP_PX, screen_rect.min.y - GRIP_PX),
                Vec2::splat(GRIP_PX),
            );
            let move_resp = ui.interact(move_grip, edit_id.with("move"), Sense::click_and_drag());
            ui.painter().rect_filled(
                move_grip,
                2.0,
                grip_color(move_resp.hovered()),
            );
            if move_resp.drag_started() {
                self.dragging = Some((id, origin));
            }
            if let Some((d, cur)) = self.dragging {
                if d == id && move_resp.dragged() {
                    let delta = move_resp.drag_delta();
                    self.dragging = Some((id, [cur[0] + delta.x / zoom, cur[1] - delta.y / zoom]));
                }
                if d == id && move_resp.drag_stopped() {
                    if let Some((_, final_origin)) = self.dragging.take() {
                        ctx.run(MoveSignatureCommand::new(page_index, id, final_origin));
                    }
                }
            }

            // --- Resize grip (bottom-right) ---
            let resize_grip = Rect::from_min_size(
                Pos2::new(screen_rect.max.x, screen_rect.max.y),
                Vec2::splat(GRIP_PX),
            );
            let resize_resp =
                ui.interact(resize_grip, edit_id.with("resize"), Sense::click_and_drag());
            ui.painter().rect_filled(
                resize_grip,
                2.0,
                grip_color(resize_resp.hovered()),
            );
            if resize_resp.drag_started() {
                self.resizing = Some((id, size, size));
            }
            if let Some((d, start, cur)) = self.resizing {
                if d == id && resize_resp.dragged() {
                    let delta = resize_resp.drag_delta();
                    // Dragging right/down grows width; height follows aspect.
                    let new_w = (cur[0] + delta.x / zoom).max(MIN_WIDTH_PT);
                    let new_size = [new_w, new_w * aspect];
                    self.resizing = Some((id, start, new_size));
                    // Keep the rendered image in sync with the border preview.
                    if let Some(sig) = ctx.session.signature_mut(page_index, id) {
                        sig.size_pt = new_size;
                    }
                }
                if d == id && resize_resp.drag_stopped() {
                    if let Some((_, start_size, final_size)) = self.resizing.take() {
                        // Reset to the pre-drag size, then run the command so it
                        // captures the correct prior size for undo.
                        if let Some(sig) = ctx.session.signature_mut(page_index, id) {
                            sig.size_pt = start_size;
                        }
                        ctx.run(ResizeSignatureCommand::new(page_index, id, final_size));
                    }
                }
            }
        }
    }
}

fn grip_color(hovered: bool) -> Color32 {
    if hovered {
        Color32::from_rgb(60, 110, 200)
    } else {
        Color32::from_rgb(90, 140, 220)
    }
}
