//! Free-text tool: click empty page space to place a text box, type into it,
//! drag the grip handle to move it. Boxes commit to PDFium free-text
//! annotations on save.
//!
//! The boxes themselves (text + border) are rendered by `page_view` for every
//! page regardless of the active tool — see [`draw_free_text_content`] — so
//! they don't vanish when the user switches to Hand or Form Fill. This tool
//! adds only the *interactive* affordances: the editable field, the move grip,
//! and click-to-place.

use eframe::egui;
use egui::{Color32, FontId, Frame, Pos2, Rect, Sense, Stroke, StrokeKind, TextEdit, Ui, Vec2};

use crate::edit::commands::{AddFreeTextCommand, MoveFreeTextCommand};
use crate::edit::{EditId, EditSession, FreeTextBox};
use crate::pdf::PageTransform;

use super::{Tool, ToolCtx, ToolEvent};

/// Default new-box size in PDF points.
const DEFAULT_BOX_PT: [f32; 2] = [200.0, 28.0];
/// Side length of the square move-grip, in screen pixels.
const GRIP_PX: f32 = 12.0;

#[derive(Default)]
pub struct FreeTextTool {
    /// Box currently being dragged + its in-progress origin (PDF pts).
    dragging: Option<(EditId, [f32; 2])>,
    /// Box whose editor we should focus on the next frame (just placed).
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
        // Place a new box only when the click lands on empty page space — not
        // inside an existing box (where the user means to edit, not create).
        if let ToolEvent::PointerDown { pdf } = event {
            let click = [pdf.x, pdf.y];
            let on_existing = ctx
                .session
                .free_texts_on(page_index)
                .any(|b| point_in_box(click, b));
            if on_existing {
                return;
            }

            let id = EditId::next();
            let b = FreeTextBox {
                id,
                page_index,
                origin_pt: click,
                size_pt: DEFAULT_BOX_PT,
                text: String::new(),
                font_size: ctx.settings.font_size,
                color: ctx.settings.text_color,
            };
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
        let ids: Vec<EditId> = ctx
            .session
            .free_texts_on(page_index)
            .map(|b| b.id)
            .collect();

        for id in ids {
            // Read the current geometry/colour for layout.
            let Some(b) = ctx.session.free_text_mut(page_index, id).map(|b| b.clone()) else {
                continue;
            };

            let origin = match self.dragging {
                Some((d, live)) if d == id => live,
                _ => b.origin_pt,
            };
            let screen_rect = box_screen_rect(transform, origin, b.size_pt);

            // Edit border (only shown by this tool — the content border is drawn
            // by `draw_free_text_content`).
            ui.painter_at(screen_rect).rect_stroke(
                screen_rect,
                2.0,
                Stroke::new(1.0, Color32::from_rgb(90, 140, 220)),
                StrokeKind::Inside,
            );

            // Editable field. We write directly into the session box's `text`
            // so the session is the single source of truth — no separate buffer
            // that can desync from what eventually gets saved.
            let font_size = (b.font_size * transform.zoom()).clamp(6.0, 48.0);
            let text_rect = Rect::from_min_max(
                Pos2::new(screen_rect.min.x + 4.0, screen_rect.min.y + 2.0),
                screen_rect.max,
            );
            let edit_id = egui::Id::new(("mgdpdf::free_text", id));

            // Borrow the session box mutably just for the TextEdit call.
            let response = {
                let Some(box_mut) = ctx.session.free_text_mut(page_index, id) else {
                    continue;
                };
                let resp = ui.put(
                    text_rect,
                    TextEdit::multiline(&mut box_mut.text)
                        .id(edit_id)
                        .frame(Frame::NONE)
                        .desired_rows(1)
                        .font(FontId::proportional(font_size))
                        .text_color(Color32::from_rgb(b.color[0], b.color[1], b.color[2])),
                );
                resp
            };
            if response.changed() {
                ctx.session.dirty = true;
            }
            if self.focus_next == Some(id) {
                response.request_focus();
                self.focus_next = None;
            }

            // Move grip at the top-left corner.
            let grip = Rect::from_min_size(
                Pos2::new(screen_rect.min.x - GRIP_PX, screen_rect.min.y - GRIP_PX),
                Vec2::splat(GRIP_PX),
            );
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
                        ctx.run(MoveFreeTextCommand::new(page_index, id, final_origin));
                    }
                }
            }
        }
    }
}

/// Renders the committed free-text boxes for a page as *content* — text plus a
/// faint border — independent of the active tool. Called by `page_view` for
/// every visible page so the boxes stay visible under Hand / Form Fill too.
pub fn draw_free_text_content(
    page_index: usize,
    painter: &egui::Painter,
    transform: &PageTransform,
    session: &EditSession,
) {
    for b in session.free_texts_on(page_index) {
        let screen_rect = box_screen_rect(transform, b.origin_pt, b.size_pt);

        // Faint guide border so empty/!focused boxes are discoverable.
        painter.rect_stroke(
            screen_rect,
            2.0,
            Stroke::new(1.0, Color32::from_rgba_unmultiplied(90, 140, 220, 90)),
            StrokeKind::Inside,
        );

        if !b.text.is_empty() {
            let font_size = (b.font_size * transform.zoom()).clamp(6.0, 48.0);
            painter.text(
                Pos2::new(screen_rect.min.x + 4.0, screen_rect.min.y + 2.0),
                egui::Align2::LEFT_TOP,
                &b.text,
                FontId::proportional(font_size),
                Color32::from_rgb(b.color[0], b.color[1], b.color[2]),
            );
        }
    }
}

fn box_screen_rect(transform: &PageTransform, origin_pt: [f32; 2], size_pt: [f32; 2]) -> Rect {
    transform.pdf_rect_to_screen(
        origin_pt[0],
        origin_pt[1] - size_pt[1],
        origin_pt[0] + size_pt[0],
        origin_pt[1],
    )
}

fn point_in_box(p: [f32; 2], b: &FreeTextBox) -> bool {
    let left = b.origin_pt[0];
    let right = left + b.size_pt[0];
    let top = b.origin_pt[1];
    let bottom = top - b.size_pt[1];
    p[0] >= left && p[0] <= right && p[1] >= bottom && p[1] <= top
}
