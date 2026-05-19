//! Form-field helpers that pdfium-render doesn't surface directly.
//!
//! Right now: tab-order computation. PDFium exposes a `/Tabs` page entry
//! ("R" row-order / "C" column-order / "S" structure-order) but the Rust
//! wrapper doesn't currently expose it, so we compute the order ourselves.

use crate::pdf::document::TextFieldWidget;

/// Returns the [`TextFieldWidget`] ids on `page_index` in reading order:
/// top-to-bottom by y (in PDF space, *larger* y is higher up), then
/// left-to-right within a horizontal band. Two widgets are considered to be
/// in the same band if their y-centres are within `band_tolerance_pt` of each
/// other (default ~half a typical field height).
pub fn compute_tab_order(widgets: &[TextFieldWidget], page_index: usize) -> Vec<usize> {
    compute_tab_order_with(widgets, page_index, 8.0)
}

fn compute_tab_order_with(
    widgets: &[TextFieldWidget],
    page_index: usize,
    band_tolerance_pt: f32,
) -> Vec<usize> {
    let mut indexed: Vec<(usize, [f32; 2])> = widgets
        .iter()
        .enumerate()
        .filter(|(_, w)| w.id.page_index == page_index)
        .map(|(i, w)| {
            let cy = (w.rect_pt.bottom + w.rect_pt.top) * 0.5;
            let cx = (w.rect_pt.left + w.rect_pt.right) * 0.5;
            (i, [cx, cy])
        })
        .collect();

    // Two widgets are in the same row if their y-centres are within tolerance.
    // PDF y increases upward, so "higher on the page" means larger y → sort y
    // descending. Within a row, sort by x ascending.
    indexed.sort_by(|a, b| {
        let dy = a.1[1] - b.1[1];
        if dy.abs() <= band_tolerance_pt {
            a.1[0]
                .partial_cmp(&b.1[0])
                .unwrap_or(std::cmp::Ordering::Equal)
        } else {
            // Larger y first.
            b.1[1]
                .partial_cmp(&a.1[1])
                .unwrap_or(std::cmp::Ordering::Equal)
        }
    });

    indexed.into_iter().map(|(i, _)| i).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf::document::{PdfRectPt, TextFieldWidget, WidgetId};

    fn w(idx_on_page: u32, page: usize, left: f32, bottom: f32, w: f32, h: f32) -> TextFieldWidget {
        TextFieldWidget {
            id: WidgetId {
                page_index: page,
                annotation_index: idx_on_page,
            },
            name: None,
            rect_pt: PdfRectPt {
                left,
                bottom,
                right: left + w,
                top: bottom + h,
            },
            value: String::new(),
        }
    }

    #[test]
    fn single_row_orders_left_to_right() {
        let widgets = vec![
            w(0, 0, 300.0, 700.0, 100.0, 20.0),
            w(1, 0, 100.0, 700.0, 100.0, 20.0),
            w(2, 0, 200.0, 700.0, 100.0, 20.0),
        ];
        let order = compute_tab_order(&widgets, 0);
        assert_eq!(order, vec![1, 2, 0]);
    }

    #[test]
    fn stacked_rows_order_top_to_bottom() {
        // Two rows; the high-y row should come first (PDF y is bottom-up).
        let widgets = vec![
            w(0, 0, 100.0, 100.0, 100.0, 20.0), // lower row, left
            w(1, 0, 100.0, 700.0, 100.0, 20.0), // upper row, left
            w(2, 0, 220.0, 700.0, 100.0, 20.0), // upper row, right
            w(3, 0, 220.0, 100.0, 100.0, 20.0), // lower row, right
        ];
        let order = compute_tab_order(&widgets, 0);
        assert_eq!(order, vec![1, 2, 0, 3]);
    }

    #[test]
    fn widgets_on_other_pages_are_ignored() {
        let widgets = vec![
            w(0, 0, 100.0, 700.0, 100.0, 20.0),
            w(0, 1, 100.0, 700.0, 100.0, 20.0),
            w(1, 0, 220.0, 700.0, 100.0, 20.0),
        ];
        let order = compute_tab_order(&widgets, 0);
        // Only entries from page 0; their original `widgets` indices are 0 and 2.
        assert_eq!(order, vec![0, 2]);
    }

    #[test]
    fn near_equal_y_still_groups_into_one_row() {
        // Field A and B differ in y by less than band tolerance — should still
        // be one row sorted left-to-right.
        let widgets = vec![
            w(0, 0, 200.0, 700.0, 100.0, 20.0),
            w(1, 0, 100.0, 705.0, 100.0, 20.0),
        ];
        let order = compute_tab_order(&widgets, 0);
        assert_eq!(order, vec![1, 0]);
    }
}
