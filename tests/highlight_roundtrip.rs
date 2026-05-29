//! Highlight round trip: stamp a translucent highlight rect via save-with-edits,
//! reopen, render, and assert highlight-coloured pixels appear in the saved file
//! (i.e. it renders in any viewer, not just ours). Also checks idempotency.

use std::path::PathBuf;
use std::sync::OnceLock;

use mgdpdf::pdf::document::{Document, EditBundle, HighlightSpec, PdfRectPt};
use pdfium_render::prelude::Pdfium;

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn pdfium() -> &'static Pdfium {
    static CELL: OnceLock<Pdfium> = OnceLock::new();
    CELL.get_or_init(|| {
        let lib = Pdfium::pdfium_platform_library_name_at_path(
            &manifest_dir().join("vendor").join("pdfium"),
        );
        let bindings = Pdfium::bind_to_library(&lib)
            .unwrap_or_else(|e| panic!("bind pdfium: {} ({e})", lib.display()));
        Pdfium::new(bindings)
    })
}

/// Counts pixels that look like our yellow highlight (high R+G, low B).
fn yellow_pixels(doc: &Document, page_index: usize) -> usize {
    let img = doc
        .render_page_rgba(page_index, 612, 792)
        .expect("render page");
    img.pixels
        .chunks_exact(4)
        .filter(|p| p[0] > 180 && p[1] > 180 && p[2] < 160 && p[3] > 0)
        .count()
}

#[test]
fn highlight_renders_in_saved_file_and_save_is_idempotent() {
    let pdfium = pdfium();
    let fixture = manifest_dir().join("tests").join("fixtures").join("hello.pdf");
    assert!(fixture.exists(), "missing fixture {}", fixture.display());

    let doc = Document::open(pdfium, &fixture).expect("open hello.pdf");
    let base_yellow = yellow_pixels(&doc, 0);

    // A big yellow band near the top of the page.
    let bundle = EditBundle {
        form_fills: Vec::new(),
        free_texts: Vec::new(),
        highlights: vec![HighlightSpec {
            page_index: 0,
            rects_pt: vec![PdfRectPt {
                left: 60.0,
                bottom: 700.0,
                right: 400.0,
                top: 730.0,
            }],
            color: [255, 235, 60, 160],
        }],
    };

    let out = std::env::temp_dir().join("mgdpdf-test-highlight.pdf");
    doc.save_with_edits(&out, &bundle).expect("save 1");
    let after_once = {
        let r = Document::open(pdfium, &out).expect("reopen 1");
        yellow_pixels(&r, 0)
    };
    doc.save_with_edits(&out, &bundle).expect("save 2");
    let after_twice = {
        let r = Document::open(pdfium, &out).expect("reopen 2");
        yellow_pixels(&r, 0)
    };

    assert!(
        after_once > base_yellow + 1000,
        "highlight did not render: base={base_yellow} after_once={after_once}"
    );
    let diff = after_twice.abs_diff(after_once);
    assert!(
        diff <= after_once / 100 + 50,
        "save not idempotent: after_once={after_once} after_twice={after_twice} diff={diff}"
    );

    let _ = std::fs::remove_file(&out);
}
