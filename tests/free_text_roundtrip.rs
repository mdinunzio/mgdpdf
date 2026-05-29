//! Free-text round trip: open the plain fixture, stamp text via the
//! save-with-edits path, reopen, and assert the text is *visibly rendered* in
//! the saved file (not merely stored as annotation metadata). We draw the text
//! as a page content object so it renders in every PDF viewer, so the only
//! meaningful check is that the rendered page gained ink where the text is.
//! Also verifies saving twice doesn't double up the text (idempotent save).

use std::path::PathBuf;
use std::sync::OnceLock;

use mgdpdf::pdf::document::{Document, EditBundle, FreeTextSpec};
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

/// Counts non-white pixels in a page render — a proxy for "ink was drawn".
fn ink_pixels(doc: &Document, page_index: usize) -> usize {
    let img = doc
        .render_page_rgba(page_index, 612, 792)
        .expect("render page");
    img.pixels
        .chunks_exact(4)
        .filter(|p| (p[0] as u16 + p[1] as u16 + p[2] as u16) < 720 && p[3] > 0)
        .count()
}

#[test]
fn free_text_renders_in_saved_file_and_save_is_idempotent() {
    let pdfium = pdfium();
    let fixture = manifest_dir().join("tests").join("fixtures").join("hello.pdf");
    assert!(
        fixture.exists(),
        "missing fixture {} — run `cargo run --example make_test_pdf`",
        fixture.display()
    );

    let doc = Document::open(pdfium, &fixture).expect("open hello.pdf");
    let base_ink = ink_pixels(&doc, 0);

    let bundle = EditBundle {
        form_fills: Vec::new(),
        free_texts: vec![FreeTextSpec {
            page_index: 0,
            origin_pt: [72.0, 680.0],
            size_pt: [240.0, 30.0],
            text: "Reviewed by Bob".to_string(),
            font_size: 18.0,
            color: [200, 0, 0, 255],
        }],
    };

    let out = std::env::temp_dir().join("mgdpdf-test-freetext.pdf");

    // Save twice from the same working doc — must not double the text.
    doc.save_with_edits(&out, &bundle).expect("save 1");
    let after_once = {
        let r = Document::open(pdfium, &out).expect("reopen 1");
        ink_pixels(&r, 0)
    };
    doc.save_with_edits(&out, &bundle).expect("save 2");
    let after_twice = {
        let r = Document::open(pdfium, &out).expect("reopen 2");
        ink_pixels(&r, 0)
    };

    // The text must actually render (more ink than the original page).
    assert!(
        after_once > base_ink,
        "free text did not render: base={base_ink} after_once={after_once}"
    );
    // Saving twice must produce the same result (idempotent) — within a small
    // tolerance for anti-aliasing nondeterminism.
    let diff = after_twice.abs_diff(after_once);
    assert!(
        diff <= base_ink / 100 + 50,
        "save not idempotent: after_once={after_once} after_twice={after_twice} diff={diff}"
    );

    let _ = std::fs::remove_file(&out);
}
