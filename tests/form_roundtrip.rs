//! End-to-end form-fill round trip:
//!   1. open `tests/fixtures/form.pdf`,
//!   2. assert PDFium finds the two text widgets,
//!   3. fill them via `set_text_field_value`,
//!   4. save to a temp PDF,
//!   5. reopen and assert the saved values come back.
//!
//! This is the single most important Phase 3 verification — it proves the
//! whole edit → commit → PDFium → save → reload pipeline works.

use std::path::PathBuf;
use std::sync::OnceLock;

use mgdpdf::pdf::document::Document;
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
fn form_fill_round_trip() {
    let pdfium = pdfium();
    let fixture = manifest_dir().join("tests").join("fixtures").join("form.pdf");
    assert!(
        fixture.exists(),
        "missing fixture {} — run `cargo run --example make_form_pdf` to generate it",
        fixture.display()
    );

    let doc = Document::open(pdfium, &fixture).expect("open form.pdf");
    let widgets = doc.collect_text_widgets();
    assert_eq!(widgets.len(), 2, "expected 2 text fields, got {:?}", widgets);

    let name_id = widgets
        .iter()
        .find(|w| w.name.as_deref() == Some("name"))
        .expect("`name` field")
        .id;
    let email_id = widgets
        .iter()
        .find(|w| w.name.as_deref() == Some("email"))
        .expect("`email` field")
        .id;

    let mut doc = doc;
    doc.set_text_field_value(name_id, "Alice Liddell").expect("set name");
    doc.set_text_field_value(email_id, "alice@example.com").expect("set email");

    let out = std::env::temp_dir().join("mgdpdf-test-form-roundtrip.pdf");
    doc.save_as(&out).expect("save");
    drop(doc);

    let doc2 = Document::open(pdfium, &out).expect("reopen");
    let widgets2 = doc2.collect_text_widgets();
    let by_name: std::collections::HashMap<String, String> = widgets2
        .iter()
        .filter_map(|w| w.name.clone().map(|n| (n, w.value.clone())))
        .collect();

    assert_eq!(by_name.get("name").map(String::as_str), Some("Alice Liddell"));
    assert_eq!(
        by_name.get("email").map(String::as_str),
        Some("alice@example.com")
    );

    let _ = std::fs::remove_file(&out);
}
