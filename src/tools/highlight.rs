//! Highlight tool. Drag across the page:
//!   * if the page has a text layer, the glyphs under the drag are selected and
//!     merged into one rect per text line;
//!   * if the page has no text (scanned image), the dragged region itself
//!     becomes a single highlight rect.
//!
//! Committed highlights render as translucent rectangles in the page content
//! stream (see `pdf::document`), so they show in every viewer. They are drawn
//! as content by `page_view` regardless of the active tool.

use eframe::egui;
use egui::{Color32, Painter, Pos2, Rect, Sense, StrokeKind};

use crate::edit::commands::AddHighlightCommand;
use crate::edit::{EditId, EditSession, Highlight};
use crate::pdf::document::GlyphRect;
use crate::pdf::PageTransform;

use super::{Tool, ToolCtx, ToolEvent};

/// Vertical tolerance (PDF points) for grouping glyphs into the same text line.
const LINE_BAND_PT: f32 = 4.0;

#[derive(Default)]
pub struct HighlightTool {
    /// (page, drag-start PDF pt, current PDF pt) while a drag is in progress.
    drag: Option<(usize, [f32; 2], [f32; 2])>,
}

impl Tool for HighlightTool {
    fn id(&self) -> &'static str {
        "highlight"
    }

    fn label(&self) -> &'static str {
        "Highlight"
    }

    fn page_sense(&self) -> Sense {
        Sense::click_and_drag()
    }

    fn on_event(&mut self, page_index: usize, event: ToolEvent, ctx: &mut ToolCtx<'_>) {
        match event {
            ToolEvent::PointerDown { pdf } => {
                self.drag = Some((page_index, [pdf.x, pdf.y], [pdf.x, pdf.y]));
            }
            ToolEvent::PointerMove { pdf } => {
                if let Some((p, start, _)) = self.drag {
                    if p == page_index {
                        self.drag = Some((p, start, [pdf.x, pdf.y]));
                    }
                }
            }
            ToolEvent::PointerUp { pdf } => {
                if let Some((p, start, _)) = self.drag.take() {
                    if p != page_index {
                        return;
                    }
                    let end = [pdf.x, pdf.y];
                    let rects = self.resolve_rects(page_index, start, end, ctx);
                    if !rects.is_empty() {
                        let h = Highlight {
                            id: EditId::next(),
                            page_index,
                            rects_pt: rects,
                            color: ctx.settings.highlight_color,
                        };
                        ctx.run(AddHighlightCommand::new(h));
                    }
                }
            }
            ToolEvent::PointerLeave => {}
        }
    }

    fn draw_overlay(
        &self,
        page_index: usize,
        painter: &Painter,
        transform: &PageTransform,
        _session: &EditSession,
    ) {
        // Live drag preview.
        if let Some((p, start, current)) = self.drag {
            if p == page_index {
                let a = transform.pdf_to_screen(Pos2::new(start[0], start[1]));
                let b = transform.pdf_to_screen(Pos2::new(current[0], current[1]));
                let rect = Rect::from_two_pos(a, b);
                painter.rect_filled(
                    rect,
                    0.0,
                    Color32::from_rgba_unmultiplied(255, 235, 60, 70),
                );
                painter.rect_stroke(
                    rect,
                    0.0,
                    egui::Stroke::new(1.0, Color32::from_rgb(220, 190, 0)),
                    StrokeKind::Inside,
                );
            }
        }
    }
}

impl HighlightTool {
    /// Turns a drag (start→end, PDF pts) into highlight rects. Uses the text
    /// layer when present, else falls back to the dragged region.
    fn resolve_rects(
        &self,
        page_index: usize,
        start: [f32; 2],
        end: [f32; 2],
        ctx: &ToolCtx<'_>,
    ) -> Vec<[f32; 4]> {
        let drag = bbox(start, end);
        let glyphs = ctx
            .glyphs
            .get(&page_index)
            .map(Vec::as_slice)
            .unwrap_or(&[]);

        if glyphs.is_empty() {
            // Scanned-page fallback: a single rect = the dragged region. Ignore
            // degenerate (click, no drag) gestures.
            if drag[2] - drag[0] < 2.0 || drag[3] - drag[1] < 2.0 {
                return Vec::new();
            }
            return vec![drag];
        }

        // Select glyphs whose centre falls within the drag bbox, then merge by
        // line band into one rect per line.
        let mut selected: Vec<&GlyphRect> = glyphs
            .iter()
            .filter(|g| !g.is_whitespace)
            .filter(|g| {
                let cx = (g.rect_pt.left + g.rect_pt.right) * 0.5;
                let cy = (g.rect_pt.bottom + g.rect_pt.top) * 0.5;
                cx >= drag[0] && cx <= drag[2] && cy >= drag[1] && cy <= drag[3]
            })
            .collect();

        if selected.is_empty() {
            return Vec::new();
        }

        // Sort by line (y descending) then x; merge per line band.
        selected.sort_by(|a, b| {
            let ay = (a.rect_pt.bottom + a.rect_pt.top) * 0.5;
            let by = (b.rect_pt.bottom + b.rect_pt.top) * 0.5;
            by.partial_cmp(&ay)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a.rect_pt.left.partial_cmp(&b.rect_pt.left).unwrap_or(std::cmp::Ordering::Equal))
        });

        let mut lines: Vec<[f32; 4]> = Vec::new();
        for g in selected {
            let r = [g.rect_pt.left, g.rect_pt.bottom, g.rect_pt.right, g.rect_pt.top];
            if let Some(last) = lines.last_mut() {
                let last_cy = (last[1] + last[3]) * 0.5;
                let g_cy = (r[1] + r[3]) * 0.5;
                if (last_cy - g_cy).abs() <= LINE_BAND_PT {
                    // Same line — extend the band.
                    last[0] = last[0].min(r[0]);
                    last[1] = last[1].min(r[1]);
                    last[2] = last[2].max(r[2]);
                    last[3] = last[3].max(r[3]);
                    continue;
                }
            }
            lines.push(r);
        }
        lines
    }
}

/// Renders committed highlights for a page as translucent rectangles — content,
/// drawn under everything else, on every page regardless of active tool.
pub fn draw_highlight_content(
    page_index: usize,
    painter: &Painter,
    transform: &PageTransform,
    session: &EditSession,
) {
    for h in session.highlights_on(page_index) {
        let fill = Color32::from_rgba_unmultiplied(h.color[0], h.color[1], h.color[2], h.color[3]);
        for r in &h.rects_pt {
            let screen = transform.pdf_rect_to_screen(r[0], r[1], r[2], r[3]);
            painter.rect_filled(screen, 0.0, fill);
        }
    }
}

fn bbox(a: [f32; 2], b: [f32; 2]) -> [f32; 4] {
    [
        a[0].min(b[0]),
        a[1].min(b[1]),
        a[0].max(b[0]),
        a[1].max(b[1]),
    ]
}
