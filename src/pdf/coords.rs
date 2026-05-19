#![allow(dead_code)] // pdf_to_screen / pdf_rect_to_screen become live in Phases 3+.
//! Coordinate transforms between PDF page space and on-screen logical pixels.
//!
//! ## Two coordinate systems
//!
//! - **PDF space**: units are points (1/72 inch). Origin is **bottom-left**;
//!   +y goes up. Every value the PDFium API takes or returns is in this space.
//! - **Screen space**: units are egui logical pixels. Origin is **top-left**;
//!   +y goes down. A page is laid out as a rectangle (`screen_rect`) on screen
//!   at the current zoom level.
//!
//! `PageTransform` carries the page's logical size in points plus the screen
//! rectangle the page is laid out in, and exposes mutually-inverse
//! `pdf_to_screen` / `screen_to_pdf` helpers. Every edit tool uses it.

use egui::{Pos2, Rect, Vec2};

/// Transforms between a single page's PDF coordinates (points, bottom-left
/// origin) and screen coordinates (logical pixels, top-left origin).
///
/// The screen rectangle is the on-screen layout of the page at the current
/// zoom — the same rect [`PageView`] paints the bitmap into. From it we derive
/// the zoom (`screen_rect.width() / page_size_pt.x`) instead of carrying it
/// separately, so the two cannot drift out of sync.
///
/// [`PageView`]: crate::ui::PageView
#[derive(Copy, Clone, Debug)]
pub struct PageTransform {
    /// Page size in PDF points (width, height).
    pub page_size_pt: Vec2,
    /// The page's on-screen rectangle in logical pixels.
    pub screen_rect: Rect,
}

impl PageTransform {
    pub fn new(page_size_pt: Vec2, screen_rect: Rect) -> Self {
        Self {
            page_size_pt,
            screen_rect,
        }
    }

    /// Pixels per PDF point in the horizontal direction (== vertical, since we
    /// preserve aspect ratio).
    #[inline]
    pub fn zoom(self) -> f32 {
        self.screen_rect.width() / self.page_size_pt.x.max(f32::EPSILON)
    }

    /// PDF point → screen position. The PDF origin is bottom-left, so we flip y.
    #[inline]
    pub fn pdf_to_screen(self, pdf: Pos2) -> Pos2 {
        let zoom = self.zoom();
        Pos2::new(
            self.screen_rect.min.x + pdf.x * zoom,
            self.screen_rect.min.y + (self.page_size_pt.y - pdf.y) * zoom,
        )
    }

    /// Screen position → PDF point.
    #[inline]
    pub fn screen_to_pdf(self, screen: Pos2) -> Pos2 {
        let zoom = self.zoom().max(f32::EPSILON);
        Pos2::new(
            (screen.x - self.screen_rect.min.x) / zoom,
            self.page_size_pt.y - (screen.y - self.screen_rect.min.y) / zoom,
        )
    }

    /// Transforms a PDF-space axis-aligned rect (bottom-left origin) into a
    /// screen-space rect (top-left origin). Note the y-flip: the PDF rect's
    /// `top` (numerically larger y) becomes the screen rect's `min.y`.
    pub fn pdf_rect_to_screen(self, pdf_left: f32, pdf_bottom: f32, pdf_right: f32, pdf_top: f32) -> Rect {
        let tl = self.pdf_to_screen(Pos2::new(pdf_left, pdf_top));
        let br = self.pdf_to_screen(Pos2::new(pdf_right, pdf_bottom));
        Rect::from_min_max(tl, br)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::pos2;

    fn letter_at(zoom: f32) -> PageTransform {
        // US Letter is 612 × 792 PDF points.
        let size = Vec2::new(612.0, 792.0);
        let w = 612.0 * zoom;
        let h = 792.0 * zoom;
        PageTransform::new(size, Rect::from_min_size(pos2(100.0, 50.0), Vec2::new(w, h)))
    }

    #[test]
    fn zoom_round_trip_at_1x() {
        let t = letter_at(1.0);
        assert!((t.zoom() - 1.0).abs() < 1e-5);
    }

    #[test]
    fn zoom_round_trip_at_2x() {
        let t = letter_at(2.0);
        assert!((t.zoom() - 2.0).abs() < 1e-5);
    }

    #[test]
    fn pdf_origin_maps_to_bottom_left_of_screen_rect() {
        let t = letter_at(1.0);
        let p = t.pdf_to_screen(pos2(0.0, 0.0));
        // PDF (0,0) is the bottom-left → screen-space bottom-left of the rect.
        assert!((p.x - t.screen_rect.min.x).abs() < 1e-3);
        assert!((p.y - t.screen_rect.max.y).abs() < 1e-3);
    }

    #[test]
    fn pdf_top_right_maps_to_top_right_of_screen_rect() {
        let t = letter_at(1.5);
        let p = t.pdf_to_screen(pos2(t.page_size_pt.x, t.page_size_pt.y));
        assert!((p.x - t.screen_rect.max.x).abs() < 1e-3);
        assert!((p.y - t.screen_rect.min.y).abs() < 1e-3);
    }

    #[test]
    fn round_trip_random_points() {
        let t = letter_at(1.25);
        // A spread of points across the page interior.
        let cases = [
            pos2(0.0, 0.0),
            pos2(72.0, 720.0),
            pos2(306.0, 396.0),
            pos2(611.9, 791.9),
            pos2(150.5, 250.25),
        ];
        for &p in &cases {
            let s = t.pdf_to_screen(p);
            let back = t.screen_to_pdf(s);
            assert!(
                (back.x - p.x).abs() < 1e-2 && (back.y - p.y).abs() < 1e-2,
                "round-trip failed for {:?} → {:?} → {:?}",
                p,
                s,
                back
            );
        }
    }

    #[test]
    fn pdf_rect_to_screen_handles_y_flip() {
        // A rect near the top of the page in PDF space should be near the top of
        // the screen rect.
        let t = letter_at(1.0);
        let r = t.pdf_rect_to_screen(72.0, 700.0, 200.0, 750.0);
        // y_top_pdf=750 → y_min_screen = 50 + (792-750) = 92
        // y_bottom_pdf=700 → y_max_screen = 50 + (792-700) = 142
        assert!((r.min.y - 92.0).abs() < 1e-3, "min.y was {}", r.min.y);
        assert!((r.max.y - 142.0).abs() < 1e-3, "max.y was {}", r.max.y);
        assert!(r.min.y < r.max.y, "screen rect must be top-down");
    }
}
