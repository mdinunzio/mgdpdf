//! Form-fill tool: paints translucent rectangles over each text-field widget
//! and lets the user type into them. On `TextEdit` change the new value is
//! committed via [`FillFormFieldCommand`] so the change is undoable.

use std::collections::HashMap;

use eframe::egui;
use egui::{Color32, FontId, Frame, Pos2, Rect, Stroke, StrokeKind, TextEdit, Ui};

use crate::edit::commands::FillFormFieldCommand;
use crate::pdf::document::{TextFieldWidget, WidgetId};
use crate::pdf::forms::compute_tab_order;
use crate::pdf::PageTransform;

use super::{Tool, ToolCtx};

#[derive(Default)]
pub struct FormFillTool {
    /// Live buffer per widget. We can't read straight from `session` each
    /// frame because `TextEdit` needs a `&mut String` it owns the lifetime of.
    buffers: HashMap<WidgetId, String>,
}

impl Tool for FormFillTool {
    fn id(&self) -> &'static str {
        "form_fill"
    }

    fn label(&self) -> &'static str {
        "Form Fill"
    }

    fn draw_interactive(
        &mut self,
        page_index: usize,
        ui: &mut Ui,
        transform: &PageTransform,
        ctx: &mut ToolCtx<'_>,
    ) {
        // Widgets on the current page, in reading order. We assign a deterministic
        // ui.tab_index based on this so Tab cycles fields the way the user expects.
        let order = compute_tab_order(ctx.widgets, page_index);
        let widgets: Vec<TextFieldWidget> =
            order.iter().map(|&i| ctx.widgets[i].clone()).collect();

        for widget in widgets {
            let screen_rect = transform.pdf_rect_to_screen(
                widget.rect_pt.left,
                widget.rect_pt.bottom,
                widget.rect_pt.right,
                widget.rect_pt.top,
            );

            // Subtle highlight so the user can see where the fillable fields are.
            ui.painter_at(screen_rect).rect_filled(
                screen_rect,
                0.0,
                Color32::from_rgba_unmultiplied(255, 235, 130, 64),
            );
            ui.painter_at(screen_rect).rect_stroke(
                screen_rect,
                0.0,
                Stroke::new(1.0, Color32::from_rgb(220, 170, 0)),
                StrokeKind::Inside,
            );

            // Synchronise the local buffer with the session / base value. The
            // session's pending value wins; otherwise we fall back to whatever
            // PDFium reported for the widget on open.
            let session_value = ctx
                .session
                .form_fill_value(widget.id)
                .map(ToOwned::to_owned);
            let baseline = session_value.unwrap_or_else(|| widget.value.clone());
            let buf = self.buffers.entry(widget.id).or_insert_with(|| baseline.clone());
            if buf != &baseline {
                // The session got mutated by undo/redo — pull in the new value
                // so the visible TextEdit matches what we'll actually save.
                let pending = ctx.session.form_fill_value(widget.id);
                if pending.map(|p| p != buf).unwrap_or(true) {
                    *buf = baseline.clone();
                }
            }

            // Match the field's pixel height to its on-screen rect; ab_glyph
            // doesn't snap perfectly but this keeps text from clipping.
            let font_size = (screen_rect.height() * 0.6).clamp(8.0, 28.0);
            let text_edit_id = egui::Id::new(("mgdpdf::form_field", widget.id));
            let response = ui.put(
                shrink(screen_rect, 2.0),
                TextEdit::singleline(buf)
                    .id(text_edit_id)
                    .frame(Frame::NONE)
                    .font(FontId::proportional(font_size))
                    .text_color(Color32::BLACK),
            );

            if response.changed() {
                let new_value = buf.clone();
                ctx.run(FillFormFieldCommand::new(widget.id, new_value));
            }
        }
    }
}

fn shrink(rect: Rect, inset: f32) -> Rect {
    Rect::from_min_max(
        Pos2::new(rect.min.x + inset, rect.min.y + inset),
        Pos2::new(rect.max.x - inset, rect.max.y - inset),
    )
}
