//! Generates a tiny multi-page PDF at `tests/fixtures/hello.pdf` for smoke-testing.
//!
//! Run with: `cargo run --example make_test_pdf`

use std::path::PathBuf;

use anyhow::{Context, Result};
use pdfium_render::prelude::*;

fn main() -> Result<()> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let dll_dir = manifest_dir.join("vendor").join("pdfium");
    let lib = Pdfium::pdfium_platform_library_name_at_path(&dll_dir);
    let pdfium = Pdfium::new(
        Pdfium::bind_to_library(&lib)
            .with_context(|| format!("loading pdfium from {}", lib.display()))?,
    );

    let mut doc = pdfium.create_new_pdf()?;
    let font = doc.fonts_mut().helvetica();
    let letter = PdfPagePaperSize::from_inches(8.5, 11.0);

    for n in 1..=3 {
        let mut page = doc.pages_mut().create_page_at_end(letter)?;
        page.objects_mut().create_text_object(
            PdfPoints::new(72.0),
            PdfPoints::new(720.0),
            format!("Page {n}"),
            font,
            PdfPoints::new(24.0),
        )?;
    }

    let out_dir = manifest_dir.join("tests").join("fixtures");
    std::fs::create_dir_all(&out_dir)?;
    let out = out_dir.join("hello.pdf");
    doc.save_to_file(&out)?;
    println!(
        "wrote {} ({} bytes)",
        out.display(),
        std::fs::metadata(&out)?.len()
    );
    Ok(())
}
