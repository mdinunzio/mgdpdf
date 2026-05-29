//! Free-text tool: click empty page space to place a text box, type into it,
//! drag the grip handle to move it. Boxes commit to PDFium free-text
//! annotations on save.

use std::collections::HashMap;

use eframe::egui;
use egui::{Color32, FontId, Frame, Pos2, Rect, Sense, Stroke, StrokeKind, TextEdit, Ui, Vec2};

use crate::edit::commands::{AddFreeTextCommand, EditFreeTextCommand, MoveFreeTextCommand};
use crate::edit::{EditId, FreeTextBox};
use crate::pdf::PageTransform;

use super::{Tool, ToolCtx, ToolEvent};

/// Default new-box size in PDF points.
const DEFAULT_BOX_PT: [f32; 2] = [200.0, 28.0];
/// Side length of the square move-grip, in screen pixels.
const GRIP_PX: f32 = 12.0;

#[derive(Default)]
pub struct FreeTextTool {
    /// Live text buffers per box id, so `TextEdit` has a stable `&mut String`.
    buffers: HashMap<EditId, String>,
    /// Box currently being dragged + its in-progress origin (PDF pts).
    dragging: Option<(EditId, [f32; 2])>,
    /// Set when a click placed a new box this frame; we focus its editor.
    focus_next: Option<EditId>,
}

impl Tool for FreeTextTool {
    fn id(&self) -> &'static str {
        "free_text"
    }

    fn label(&self) -> &'static str {
        "Text"
    }

    fn page_sense(&self) -> Sense {
        // We need to detect clicks on the bare page to place new boxes.
        Sense::click_and_drag()
    }

    fn on_event(&mut self, page_index: usize, event: ToolEvent, ctx: &mut ToolCtx<'_>) {
        // Place a new box on a plain click over empty page space. If the click
        // landed on an existing box, that box's TextEdit consumes the click
        // first (it's painted on top), so this only fires on empty space.
        if let ToolEvent::PointerDown { pdf } = event {
            let id = EditId::next();
            let b = FreeTextBox {
                id,
                page_index,
                origin_pt: [pdf.x, pdf.y],
                size_pt: DEFAULT_BOX_PT,
                text: String::new(),
                font_size: ctx.settings.font_size,
                color: ctx.settings.text_color,
            };
            self.buffers.insert(id, String::new());
            self.focus_next = Some(id);
            ctx.run(AddFreeTextCommand::new(b));
        }
    }

    fn draw_interactive(
        &mut self,
        page_index: usize,
        ui: &mut Ui,
        transform: &PageTransform,
        ctx: &mut ToolCtx<'_>,
    ) {
        // Snapshot the boxes on this page so we don't hold a borrow on `session`
        // while we also mutate it through commands.
        let boxes: Vec<FreeTextBox> = ctx.session.free_texts_on(page_index).cloned().collect();

        for b in boxes {
            // While dragging, preview at the live origin.
            let origin = if let Some((id, live)) = self.dragging {
                if id == b.id {
                    live
                } else {
                    b.origin_pt
                }
            } else {
                b.origin_pt
            };

            let screen_rect = transform.pdf_rect_to_screen(
                origin[0],
                origin[1] - b.size_pt[1],
                origin[0] + b.size_pt[0],
                origin[1],
            );

            // Box background + border.
            ui.painter_at(screen_rect).rect_filled(
                screen_rect,
                2.0,
                Color32::from_rgba_unmultiplied(255, 255, 255, 12),
            );
            ui.painter_at(screen_rect).rect_stroke(
                screen_rect,
                2.0,
                Stroke::new(1.0, Color32::from_rgb(90, 140, 220)),
                StrokeKind::Inside,
            );

            // Text editor.
            let font_size = b.font_size * transform.zoom();
            let text_rect = Rect::from_min_max(
                Pos2::new(screen_rect.min.x + 4.0, screen_rect.min.y + 2.0),
                screen_rect.max,
            );
            let buf = self.buffers.entry(b.id).or_insert_with(|| b.text.clone());
            let edit_id = egui::Id::new(("mgdpdf::free_text", b.id));
            let response = ui.put(
                text_rect,
                TextEdit::multiline(buf)
                    .id(edit_id)
                    .frame(Frame::NONE)
                    .desired_rows(1)
                    .font(FontId::proportional(font_size.clamp(6.0, 48.0)))
                    .text_color(Color32::from_rgb(
                        b.color[0],
                        b.color[1],
                        b.color[2],
                    )),
            );

            if self.focus_next == Some(b.id) {
                response.request_focus();
                self.focus_next = None;
            }

            if response.changed() {
                ctx.run(EditFreeTextCommand::new(page_index, b.id, buf.clone()));
            }

            // Move grip at the top-left corner.
            let grip = Rect::from_min_size(
                Pos2::new(screen_rect.min.x - GRIP_PX, screen_rect.min.y - GRIP_PX),
                Vec2::splat(GRIP_PX),
            );
            let grip_resp = ui.interact(
                grip,
                edit_id.with("grip"),
                Sense::click_and_drag(),
            );
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
                self.dragging = Some((b.id, b.origin_pt));
            }
            if let Some((id, _)) = self.dragging {
                if id == b.id && grip_resp.dragged() {
                    let delta = grip_resp.drag_delta();
                    // Screen delta → PDF delta (y inverted, divide by zoom).
                    let zoom = transform.zoom().max(f32::EPSILON);
                    let new_origin = [
                        origin[0] + delta.x / zoom,
                        origin[1] - delta.y / zoom,
                    ];
                    self.dragging = Some((id, new_origin));
                }
                if id == b.id && grip_resp.drag_stopped() {
                    if let Some((_, final_origin)) = self.dragging.take() {
                        ctx.run(MoveFreeTextCommand::new(page_index, b.id, final_origin));
                    }
                }
            }
        }
    }
}
