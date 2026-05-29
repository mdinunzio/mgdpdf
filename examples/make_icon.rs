//! Generates `assets/icon.ico` — a simple app icon (a document page with a
//! red signature swoosh). Drawn with tiny-skia, encoded as PNG, wrapped in a
//! single-image ICO container. Run: `cargo run --example make_icon`.

use std::io::Cursor;
use std::path::PathBuf;

use image::{ImageFormat, RgbaImage};
use tiny_skia::{Color, FillRule, Paint, PathBuilder, Pixmap, Stroke, Transform};

fn main() -> anyhow::Result<()> {
    let size = 256u32;
    let mut pm = Pixmap::new(size, size).unwrap();

    // Background: rounded blue tile.
    let mut bg = Paint::default();
    bg.set_color(Color::from_rgba8(38, 70, 140, 255));
    bg.anti_alias = true;
    if let Some(rect) = round_rect(20.0, 20.0, 216.0, 216.0, 36.0) {
        pm.fill_path(&rect, &bg, FillRule::Winding, Transform::identity(), None);
    }

    // White document page with a folded corner.
    let mut white = Paint::default();
    white.set_color(Color::from_rgba8(248, 248, 250, 255));
    white.anti_alias = true;
    let mut page = PathBuilder::new();
    page.move_to(78.0, 60.0);
    page.line_to(150.0, 60.0);
    page.line_to(184.0, 94.0);
    page.line_to(184.0, 196.0);
    page.line_to(78.0, 196.0);
    page.close();
    if let Some(p) = page.finish() {
        pm.fill_path(&p, &white, FillRule::Winding, Transform::identity(), None);
    }

    // Text lines on the page.
    let mut gray = Paint::default();
    gray.set_color(Color::from_rgba8(150, 160, 180, 255));
    gray.anti_alias = true;
    let line_stroke = Stroke {
        width: 6.0,
        ..Default::default()
    };
    for y in [96.0, 116.0, 136.0] {
        let mut l = PathBuilder::new();
        l.move_to(96.0, y);
        l.line_to(166.0, y);
        if let Some(p) = l.finish() {
            pm.stroke_path(&p, &gray, &line_stroke, Transform::identity(), None);
        }
    }

    // Red signature swoosh across the lower page.
    let mut red = Paint::default();
    red.set_color(Color::from_rgba8(210, 40, 40, 255));
    red.anti_alias = true;
    let sig_stroke = Stroke {
        width: 8.0,
        line_cap: tiny_skia::LineCap::Round,
        line_join: tiny_skia::LineJoin::Round,
        ..Default::default()
    };
    let mut sig = PathBuilder::new();
    sig.move_to(92.0, 176.0);
    sig.cubic_to(108.0, 150.0, 120.0, 196.0, 138.0, 166.0);
    sig.cubic_to(150.0, 148.0, 162.0, 188.0, 178.0, 162.0);
    if let Some(p) = sig.finish() {
        pm.stroke_path(&p, &red, &sig_stroke, Transform::identity(), None);
    }

    // tiny-skia premultiplied RGBA -> image RgbaImage (unpremultiplied).
    let mut img = RgbaImage::new(size, size);
    for (i, px) in pm.data().chunks_exact(4).enumerate() {
        let (r, g, b, a) = (px[0], px[1], px[2], px[3]);
        let unpre = |c: u8| if a == 0 { 0 } else { ((c as u16 * 255 + a as u16 / 2) / a as u16).min(255) as u8 };
        img.put_pixel((i as u32) % size, (i as u32) / size, image::Rgba([unpre(r), unpre(g), unpre(b), a]));
    }

    // Encode the 256x256 as PNG, then wrap in a single-image ICO container.
    let mut png_bytes = Vec::new();
    image::DynamicImage::ImageRgba8(img)
        .write_to(&mut Cursor::new(&mut png_bytes), ImageFormat::Png)?;
    let ico = wrap_png_in_ico(&png_bytes, size, size);

    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets").join("icon.ico");
    std::fs::create_dir_all(out.parent().unwrap())?;
    std::fs::write(&out, &ico)?;
    println!("wrote {} ({} bytes)", out.display(), ico.len());
    Ok(())
}

fn round_rect(x: f32, y: f32, w: f32, h: f32, r: f32) -> Option<tiny_skia::Path> {
    let mut pb = PathBuilder::new();
    pb.move_to(x + r, y);
    pb.line_to(x + w - r, y);
    pb.quad_to(x + w, y, x + w, y + r);
    pb.line_to(x + w, y + h - r);
    pb.quad_to(x + w, y + h, x + w - r, y + h);
    pb.line_to(x + r, y + h);
    pb.quad_to(x, y + h, x, y + h - r);
    pb.line_to(x, y + r);
    pb.quad_to(x, y, x + r, y);
    pb.close();
    pb.finish()
}

/// Wraps a PNG byte buffer in a minimal single-image ICO container. ICO
/// supports embedded PNGs directly (Vista+), so this is valid for modern
/// Windows. Width/height of 256 are encoded as 0 per the ICO spec.
fn wrap_png_in_ico(png: &[u8], w: u32, h: u32) -> Vec<u8> {
    let mut out = Vec::new();
    // ICONDIR
    out.extend_from_slice(&0u16.to_le_bytes()); // reserved
    out.extend_from_slice(&1u16.to_le_bytes()); // type: 1 = icon
    out.extend_from_slice(&1u16.to_le_bytes()); // image count
    // ICONDIRENTRY
    out.push(if w >= 256 { 0 } else { w as u8 });
    out.push(if h >= 256 { 0 } else { h as u8 });
    out.push(0); // palette
    out.push(0); // reserved
    out.extend_from_slice(&1u16.to_le_bytes()); // color planes
    out.extend_from_slice(&32u16.to_le_bytes()); // bits per pixel
    out.extend_from_slice(&(png.len() as u32).to_le_bytes()); // image size
    out.extend_from_slice(&22u32.to_le_bytes()); // offset (6 + 16)
    out.extend_from_slice(png);
    out
}
