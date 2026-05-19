//! Generates a minimal AcroForm PDF at `tests/fixtures/form.pdf` for Phase 3
//! smoke-testing. `pdfium-render` 0.9 doesn't expose widget creation, so we
//! emit the PDF bytes directly. The structure is deliberately small but
//! complete enough that PDFium recognises the fields and Adobe Reader is happy.
//!
//! Layout (US Letter, 612 × 792 pt):
//!   • a "Name:" label and a text field
//!   • an "Email:" label and a text field
//!
//! Run with: `cargo run --example make_form_pdf`

use std::path::PathBuf;

use anyhow::Result;

fn main() -> Result<()> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let out_dir = manifest_dir.join("tests").join("fixtures");
    std::fs::create_dir_all(&out_dir)?;
    let out_path = out_dir.join("form.pdf");

    let bytes = build_form_pdf();
    std::fs::write(&out_path, &bytes)?;
    println!("wrote {} ({} bytes)", out_path.display(), bytes.len());
    Ok(())
}

/// Constructs a minimal AcroForm PDF as raw bytes.
///
/// Object map:
///   1: Catalog (refers to 2: Pages and 7: AcroForm)
///   2: Pages   (single page in /Kids: 3)
///   3: Page    (MediaBox + Contents=4, Annots=[5,6])
///   4: Page content stream (labels "Name:" / "Email:")
///   5: Widget annotation #1 — text field "name"
///   6: Widget annotation #2 — text field "email"
///   7: AcroForm dictionary (Fields=[5,6])
///   8: Helvetica font used by /DR + /DA
fn build_form_pdf() -> Vec<u8> {
    use std::fmt::Write;

    let mut body = String::new();
    let mut offsets: Vec<usize> = vec![0]; // offsets[i] = byte offset of object i; index 0 is "free".

    // Header.
    let mut pdf = String::from("%PDF-1.7\n%\u{00E2}\u{00E3}\u{00CF}\u{00D3}\n");

    // 1: Catalog.
    offsets.push(pdf.len() + body.len());
    write!(
        body,
        "1 0 obj\n<< /Type /Catalog /Pages 2 0 R /AcroForm 7 0 R >>\nendobj\n"
    )
    .unwrap();

    // 2: Pages.
    offsets.push(pdf.len() + body.len());
    write!(
        body,
        "2 0 obj\n<< /Type /Pages /Kids [3 0 R] /Count 1 >>\nendobj\n"
    )
    .unwrap();

    // 3: Page (US Letter).
    offsets.push(pdf.len() + body.len());
    write!(
        body,
        "3 0 obj\n<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] \
         /Resources << /Font << /Helv 8 0 R >> >> \
         /Contents 4 0 R /Annots [5 0 R 6 0 R] >>\nendobj\n"
    )
    .unwrap();

    // 4: Page content stream — draws two labels.
    let content = "BT /Helv 12 Tf 72 720 Td (Name:) Tj 0 -40 Td (Email:) Tj ET\n";
    let content_stream = format!(
        "4 0 obj\n<< /Length {} >>\nstream\n{}endstream\nendobj\n",
        content.len(),
        content
    );
    offsets.push(pdf.len() + body.len());
    body.push_str(&content_stream);

    // 5: Widget annotation — "name" text field.
    // Field rect: x=120..400, y=712..732 (a 280×20 box just right of the label).
    // /DA is the default-appearance string that PDFium uses when rendering text
    // typed into the field.
    offsets.push(pdf.len() + body.len());
    write!(
        body,
        "5 0 obj\n<< /Type /Annot /Subtype /Widget /Rect [120 712 400 732] \
         /FT /Tx /T (name) /V () /DA (/Helv 12 Tf 0 g) /MK << >> /F 4 \
         /BS << /W 1 /S /S >> /Border [0 0 1] /Ff 0 /P 3 0 R >>\nendobj\n"
    )
    .unwrap();

    // 6: Widget annotation — "email" text field.
    offsets.push(pdf.len() + body.len());
    write!(
        body,
        "6 0 obj\n<< /Type /Annot /Subtype /Widget /Rect [120 672 400 692] \
         /FT /Tx /T (email) /V () /DA (/Helv 12 Tf 0 g) /MK << >> /F 4 \
         /BS << /W 1 /S /S >> /Border [0 0 1] /Ff 0 /P 3 0 R >>\nendobj\n"
    )
    .unwrap();

    // 7: AcroForm.
    offsets.push(pdf.len() + body.len());
    write!(
        body,
        "7 0 obj\n<< /Fields [5 0 R 6 0 R] /NeedAppearances true \
         /DA (/Helv 12 Tf 0 g) /DR << /Font << /Helv 8 0 R >> >> >>\nendobj\n"
    )
    .unwrap();

    // 8: Helvetica font dictionary (standard 14, no embedding needed).
    offsets.push(pdf.len() + body.len());
    write!(
        body,
        "8 0 obj\n<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>\nendobj\n"
    )
    .unwrap();

    pdf.push_str(&body);

    // xref table.
    let xref_offset = pdf.len();
    let n_objects = offsets.len(); // includes the free object at index 0
    let mut xref = String::new();
    write!(xref, "xref\n0 {}\n", n_objects).unwrap();
    // Free object 0.
    xref.push_str("0000000000 65535 f \n");
    for &off in &offsets[1..] {
        write!(xref, "{:010} 00000 n \n", off).unwrap();
    }
    pdf.push_str(&xref);

    // Trailer.
    write!(
        pdf,
        "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{}\n%%EOF\n",
        n_objects, xref_offset
    )
    .unwrap();

    pdf.into_bytes()
}
