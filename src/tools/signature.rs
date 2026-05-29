//! Signature tool: place a captured signature on a page (click), then drag the
//! grip to move it. The signature image itself is captured by the modal in
//! `ui::signature_modal`; this tool consumes the pending image and places it.
//!
//! Placed signatures are rendered (as textures) by `page_view`, and committed
//! to the saved PDF as page content image objects (see `pdf::document`).

use eframe::egui;
use egui::{Color32, Pos2, Rect, Sense, StrokeKind, Ui, Vec2};

use crate::edit::commands::{AddSignatureCommand, MoveSignatureCommand};
use crate::edit::{EditId, SignaturePlacement};
use crate::pdf::PageTransform;

use super::{Tool, ToolCtx, ToolEvent};

/// Default placement width in PDF points (height follows image aspect).
const DEFAULT_WIDTH_PT: f32 = 150.0;
/// Side length of the square move-grip, in screen pixels.
const GRIP_PX: f32 = 12.0;

#[derive(Default)]
pub struct SignatureTool {
    /// Box being dragged + its in-progress origin (PDF pts).
    dragging: Option<(EditId, [f32; 2])>,
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
            let origin = match self.dragging {
                Some((d, live)) if d == id => live,
                _ => s.origin_pt,
            };
            let screen_rect = transform.pdf_rect_to_screen(
                origin[0],
                origin[1] - s.size_pt[1],
                origin[0] + s.size_pt[0],
                origin[1],
            );

            // Selection border (the image itself is drawn by page_view).
            ui.painter_at(screen_rect).rect_stroke(
                screen_rect,
                2.0,
                egui::Stroke::new(1.0, Color32::from_rgb(90, 140, 220)),
                StrokeKind::Inside,
            );

            // Move grip at the top-left corner.
            let grip = Rect::from_min_size(
                Pos2::new(screen_rect.min.x - GRIP_PX, screen_rect.min.y - GRIP_PX),
                Vec2::splat(GRIP_PX),
            );
            let edit_id = egui::Id::new(("mgdpdf::signature", id));
            let grip_resp = ui.interact(grip, edit_id.with("grip"), Sense::click_and_drag());
            ui.painter().rect_filled(
                grip,
                2.0,
                if grip_resp.hovered() {
                    Color32::from_rgb(60, 110, 200)
                } else {
                    Color32::from_rgb(90, 140, 220)
                },
            );

            if grip_resp.drag_started() {
                self.dragging = Some((id, origin));
            }
            if let Some((d, cur)) = self.dragging {
                if d == id && grip_resp.dragged() {
                    let delta = grip_resp.drag_delta();
                    let zoom = transform.zoom().max(f32::EPSILON);
                    self.dragging = Some((id, [cur[0] + delta.x / zoom, cur[1] - delta.y / zoom]));
                }
                if d == id && grip_resp.drag_stopped() {
                    if let Some((_, final_origin)) = self.dragging.take() {
                        ctx.run(MoveSignatureCommand::new(page_index, id, final_origin));
                    }
                }
            }
        }
    }
}
