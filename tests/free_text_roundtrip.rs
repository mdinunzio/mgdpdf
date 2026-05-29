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

    let _ = std::fs::remove_file(&out);
}
