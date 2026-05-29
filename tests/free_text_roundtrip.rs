//! Free-text round trip: open the plain fixture, stamp a free-text box via the
//! save-with-edits path, reopen, and assert a free-text annotation with the
//! right contents is present. Also verifies saving twice doesn't duplicate the
//! annotation (idempotent save).

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

#[test]
fn free_text_round_trip_and_idempotent_save() {
    let pdfium = pdfium();
    let fixture = manifest_dir().join("tests").join("fixtures").join("hello.pdf");
    assert!(
        fixture.exists(),
        "missing fixture {} — run `cargo run --example make_test_pdf`",
        fixture.display()
    );

    let doc = Document::open(pdfium, &fixture).expect("open hello.pdf");
    let base_annotations = doc.annotation_count(0);

    let bundle = EditBundle {
        form_fills: Vec::new(),
        free_texts: vec![FreeTextSpec {
            page_index: 0,
            origin_pt: [72.0, 680.0],
            size_pt: [240.0, 30.0],
            text: "Reviewed by Bob".to_string(),
            color: [200, 0, 0, 255],
        }],
    };

    let out = std::env::temp_dir().join("mgdpdf-test-freetext.pdf");

    // Save twice from the same working doc — must not duplicate annotations.
    doc.save_with_edits(&out, &bundle).expect("save 1");
    doc.save_with_edits(&out, &bundle).expect("save 2");

    let reopened = Document::open(pdfium, &out).expect("reopen");
    let new_annotations = reopened.annotation_count(0);
    assert_eq!(
        new_annotations,
        base_annotations + 1,
        "expected exactly one new annotation after (idempotent) save, base={base_annotations} new={new_annotations}"
    );

    // The typed text must actually round-trip to the saved file — this is the
    // bug a user hit where text rendered on screen but wasn't saved.
    let contents = reopened.collect_free_text_contents(0);
    assert!(
        contents.iter().any(|c| c == "Reviewed by Bob"),
        "saved free-text contents missing; got {contents:?}"
    );

    // ...and the annotation must actually RENDER (have a baked appearance
    // stream), not just carry contents. Render the page and confirm there are
    // non-white pixels near the box that weren't there in the original. This is
    // the gap between "contents stored" and "user sees text on save".
    let painted = render_has_dark_pixels(&reopened, 0);
    let original = Document::open(pdfium, &fixture).unwrap();
    let original_painted = render_has_dark_pixels(&original, 0);
    assert!(
        painted > original_painted,
        "free-text annotation did not render: dark pixels saved={painted} original={original_painted}"
    );

    let _ = std::fs::remove_file(&out);
}

/// Counts roughly-dark pixels in a page render — a proxy for "something was
/// drawn". Used to verify the free-text annotation has a visible appearance.
fn render_has_dark_pixels(doc: &Document, page_index: usize) -> usize {
    let img = doc
        .render_page_rgba(page_index, 612, 792)
        .expect("render page");
    img.pixels
        .chunks_exact(4)
        .filter(|p| p[0] < 128 && p[1] < 128 && p[2] < 128 && p[3] > 0)
        .count()
}
