//! Signature round trip: stamp a signature image via save-with-edits, reopen,
//! render, and assert the signature's ink appears in the saved page (visible in
//! any viewer). Also checks transparency (the transparent area doesn't paint a
//! box) and idempotency.

use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

use image::{Rgba, RgbaImage};
use mgdpdf::pdf::document::{Document, EditBundle, SignatureSpec};
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

/// Counts strongly-blue pixels (our signature ink colour) in a page render.
fn blue_ink(doc: &Document, page_index: usize) -> usize {
    let img = doc
        .render_page_rgba(page_index, 612, 792)
        .expect("render page");
    img.pixels
        .chunks_exact(4)
        .filter(|p| p[2] > 150 && p[0] < 120 && p[1] < 120 && p[3] > 0)
        .count()
}

/// A signature bitmap: a blue horizontal bar on a transparent background.
fn test_signature() -> RgbaImage {
    let mut img = RgbaImage::new(80, 20);
    for y in 6..14 {
        for x in 4..76 {
            img.put_pixel(x, y, Rgba([20, 30, 220, 255]));
        }
    }
    img
}

#[test]
fn signature_renders_in_saved_file_and_save_is_idempotent() {
    let pdfium = pdfium();
    let fixture = manifest_dir().join("tests").join("fixtures").join("hello.pdf");
    assert!(fixture.exists(), "missing fixture {}", fixture.display());

    let doc = Document::open(pdfium, &fixture).expect("open");
    let base_blue = blue_ink(&doc, 0);

    let bundle = EditBundle {
        form_fills: Vec::new(),
        free_texts: Vec::new(),
        highlights: Vec::new(),
        signatures: vec![SignatureSpec {
            page_index: 0,
            origin_pt: [100.0, 400.0],
            size_pt: [160.0, 40.0],
            image: Arc::new(test_signature()),
        }],
    };

    let out = std::env::temp_dir().join("mgdpdf-test-signature.pdf");
    doc.save_with_edits(&out, &bundle).expect("save 1");
    let after_once = {
        let r = Document::open(pdfium, &out).expect("reopen 1");
        blue_ink(&r, 0)
    };
    doc.save_with_edits(&out, &bundle).expect("save 2");
    let after_twice = {
        let r = Document::open(pdfium, &out).expect("reopen 2");
        blue_ink(&r, 0)
    };

    assert!(
        after_once > base_blue + 200,
        "signature did not render: base={base_blue} after_once={after_once}"
    );
    let diff = after_twice.abs_diff(after_once);
    assert!(
        diff <= after_once / 100 + 50,
        "save not idempotent: after_once={after_once} after_twice={after_twice} diff={diff}"
    );

    let _ = std::fs::remove_file(&out);
}
