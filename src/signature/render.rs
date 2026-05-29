//! Rasterising signatures from strokes / typed text / uploaded images into
//! transparent-background RGBA bitmaps.

use ab_glyph::{Font, FontRef, PxScale, ScaleFont};
use image::{DynamicImage, GenericImageView, Rgba, RgbaImage};
use tiny_skia::{Color, LineCap, LineJoin, Paint, PathBuilder, Pixmap, Stroke, Transform};

/// Bundled script font for typed signatures (OFL-licensed).
const SCRIPT_FONT: &[u8] = include_bytes!("../../assets/fonts/Caveat-Regular.ttf");

/// One drawn stroke: a polyline in canvas pixel coordinates.
pub type StrokePath = Vec<(f32, f32)>;

/// Rasterises drawn strokes onto a transparent canvas of `width`×`height`.
/// `ink` is the stroke RGB; strokes are drawn at `stroke_width` px.
pub fn rasterize_strokes(
    strokes: &[StrokePath],
    width: u32,
    height: u32,
    stroke_width: f32,
    ink: [u8; 3],
) -> RgbaImage {
    let mut pixmap = Pixmap::new(width.max(1), height.max(1)).expect("pixmap alloc");
    // Transparent background (Pixmap starts zeroed = transparent).

    let mut paint = Paint::default();
    paint.set_color(Color::from_rgba8(ink[0], ink[1], ink[2], 255));
    paint.anti_alias = true;

    let stroke = Stroke {
        width: stroke_width,
        line_cap: LineCap::Round,
        line_join: LineJoin::Round,
        ..Default::default()
    };

    for poly in strokes {
        if poly.len() < 2 {
            // A dot — draw a tiny segment so single taps register.
            if let Some(&(x, y)) = poly.first() {
                let mut pb = PathBuilder::new();
                pb.move_to(x, y);
                pb.line_to(x + 0.1, y + 0.1);
                if let Some(path) = pb.finish() {
                    pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
                }
            }
            continue;
        }
        let mut pb = PathBuilder::new();
        pb.move_to(poly[0].0, poly[0].1);
        for &(x, y) in &poly[1..] {
            pb.line_to(x, y);
        }
        if let Some(path) = pb.finish() {
            pixmap.stroke_path(&path, &paint, &stroke, Transform::identity(), None);
        }
    }

    premultiplied_pixmap_to_rgba(&pixmap)
}

/// Renders `name` in the bundled script font, returning a tightly-cropped
/// transparent RGBA bitmap. `px` is the font size in pixels; `ink` the colour.
pub fn render_typed_name(name: &str, px: f32, ink: [u8; 3]) -> RgbaImage {
    let font = FontRef::try_from_slice(SCRIPT_FONT).expect("bundled font parses");
    let scale = PxScale::from(px.max(8.0));
    let scaled = font.as_scaled(scale);

    // First pass: measure the laid-out glyphs to size the canvas.
    let mut pen_x = 0.0f32;
    let ascent = scaled.ascent();
    let descent = scaled.descent();
    let mut glyphs = Vec::new();
    let mut prev = None;
    for ch in name.chars() {
        let gid = font.glyph_id(ch);
        if let Some(p) = prev {
            pen_x += scaled.kern(p, gid);
        }
        let glyph = gid.with_scale_and_position(scale, ab_glyph::point(pen_x, ascent));
        pen_x += scaled.h_advance(gid);
        glyphs.push(glyph);
        prev = Some(gid);
    }

    let pad = (px * 0.25).ceil() as u32;
    let width = (pen_x.ceil() as u32).max(1) + pad * 2;
    let height = ((ascent - descent).ceil() as u32).max(1) + pad * 2;
    let mut img = RgbaImage::new(width, height);

    for glyph in glyphs {
        if let Some(outline) = font.outline_glyph(glyph) {
            let bounds = outline.px_bounds();
            outline.draw(|gx, gy, coverage| {
                let px_x = gx as i32 + bounds.min.x as i32 + pad as i32;
                let px_y = gy as i32 + bounds.min.y as i32 + pad as i32;
                if px_x >= 0 && px_y >= 0 && (px_x as u32) < width && (px_y as u32) < height {
                    let a = (coverage * 255.0).round().clamp(0.0, 255.0) as u8;
                    if a > 0 {
                        img.put_pixel(px_x as u32, px_y as u32, Rgba([ink[0], ink[1], ink[2], a]));
                    }
                }
            });
        }
    }

    crop_transparent(&DynamicImage::ImageRgba8(img))
}

/// Prepares an uploaded image for use as a signature: converts to RGBA and keys
/// near-white pixels to transparent so the signature composites cleanly over a
/// coloured page. Then crops to the non-transparent bounds.
pub fn prepare_uploaded(img: &DynamicImage) -> RgbaImage {
    let mut rgba = img.to_rgba8();
    for px in rgba.pixels_mut() {
        let [r, g, b, a] = px.0;
        // Treat very light pixels as background.
        if a > 0 && r > 240 && g > 240 && b > 240 {
            px.0 = [r, g, b, 0];
        }
    }
    crop_transparent(&DynamicImage::ImageRgba8(rgba))
}

/// Public wrapper used by the modal to tightly crop a drawn-stroke canvas.
pub fn crop_for_modal(img: RgbaImage) -> RgbaImage {
    crop_transparent(&DynamicImage::ImageRgba8(img))
}

/// Crops an RGBA image to the bounding box of its non-transparent pixels.
/// Returns a 1×1 transparent image if fully transparent.
fn crop_transparent(img: &DynamicImage) -> RgbaImage {
    let (w, h) = img.dimensions();
    let rgba = img.to_rgba8();
    let (mut min_x, mut min_y, mut max_x, mut max_y) = (w, h, 0u32, 0u32);
    let mut any = false;
    for y in 0..h {
        for x in 0..w {
            if rgba.get_pixel(x, y).0[3] > 0 {
                any = true;
                min_x = min_x.min(x);
                min_y = min_y.min(y);
                max_x = max_x.max(x);
                max_y = max_y.max(y);
            }
        }
    }
    if !any {
        return RgbaImage::new(1, 1);
    }
    let cw = max_x - min_x + 1;
    let ch = max_y - min_y + 1;
    let mut out = RgbaImage::new(cw, ch);
    for y in 0..ch {
        for x in 0..cw {
            out.put_pixel(x, y, *rgba.get_pixel(min_x + x, min_y + y));
        }
    }
    out
}

/// Converts a tiny-skia premultiplied-RGBA pixmap into an unpremultiplied
/// `image::RgbaImage` (the format the `image` crate and PDFium expect).
fn premultiplied_pixmap_to_rgba(pixmap: &Pixmap) -> RgbaImage {
    let w = pixmap.width();
    let h = pixmap.height();
    let data = pixmap.data(); // premultiplied RGBA, row-major
    let mut out = RgbaImage::new(w, h);
    for (i, px) in data.chunks_exact(4).enumerate() {
        let (r, g, b, a) = (px[0], px[1], px[2], px[3]);
        let (ur, ug, ub) = if a == 0 {
            (0, 0, 0)
        } else {
            let unpremul = |c: u8| ((c as u16 * 255 + a as u16 / 2) / a as u16).min(255) as u8;
            (unpremul(r), unpremul(g), unpremul(b))
        };
        let x = (i as u32) % w;
        let y = (i as u32) / w;
        out.put_pixel(x, y, Rgba([ur, ug, ub, a]));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strokes_produce_some_ink() {
        let strokes = vec![vec![(10.0, 10.0), (90.0, 50.0), (10.0, 90.0)]];
        let img = rasterize_strokes(&strokes, 100, 100, 3.0, [0, 0, 0]);
        let inked = img.pixels().filter(|p| p.0[3] > 0).count();
        assert!(inked > 0, "expected ink from strokes");
    }

    #[test]
    fn typed_name_renders_and_crops() {
        let img = render_typed_name("Ada", 48.0, [10, 10, 40]);
        assert!(img.width() > 4 && img.height() > 4, "typed sig too small: {}x{}", img.width(), img.height());
        let inked = img.pixels().filter(|p| p.0[3] > 0).count();
        assert!(inked > 0, "expected ink from typed name");
    }

    #[test]
    fn upload_keys_white_to_transparent() {
        let mut src = RgbaImage::new(4, 4);
        for px in src.pixels_mut() {
            *px = Rgba([255, 255, 255, 255]); // all white
        }
        src.put_pixel(1, 1, Rgba([0, 0, 0, 255])); // one black dot
        let out = prepare_uploaded(&DynamicImage::ImageRgba8(src));
        // After keying white→transparent and cropping, only the dot remains.
        assert_eq!(out.dimensions(), (1, 1));
        assert_eq!(out.get_pixel(0, 0).0[3], 255);
    }
}
