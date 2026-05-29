//! The signature-capture modal: a window with Draw / Type / Upload tabs that
//! produces a transparent RGBA signature image. On confirm, the image is handed
//! back to the app, which arms the signature tool to place it.

use std::sync::Arc;

use eframe::egui;
use egui::{Color32, Pos2, Sense, Stroke, StrokeKind, Vec2};
use image::RgbaImage;

use crate::signature::render::{prepare_uploaded, rasterize_strokes, render_typed_name, StrokePath};
use crate::signature::SignatureLibrary;

#[derive(Copy, Clone, PartialEq, Eq)]
enum Tab {
    Draw,
    Type,
    Upload,
}

pub struct SignatureModal {
    pub open: bool,
    tab: Tab,
    // Draw state — strokes in canvas-local pixel coords.
    strokes: Vec<StrokePath>,
    current: StrokePath,
    // Type state.
    typed_name: String,
    // Upload state — loaded + keyed RGBA.
    uploaded: Option<RgbaImage>,
    upload_error: Option<String>,
    // Saving to the library.
    save_name: String,
    save_to_library: bool,
    // Saved-signatures list (paths), refreshed when the modal opens.
    saved: Vec<std::path::PathBuf>,
    // Result of a successful confirm: the image to place. Drained by the app.
    pub result: Option<Arc<RgbaImage>>,
}

impl Default for SignatureModal {
    fn default() -> Self {
        Self {
            open: false,
            tab: Tab::Draw,
            strokes: Vec::new(),
            current: Vec::new(),
            typed_name: String::new(),
            uploaded: None,
            upload_error: None,
            save_name: String::new(),
            save_to_library: false,
            saved: Vec::new(),
            result: None,
        }
    }
}

const CANVAS_W: f32 = 420.0;
const CANVAS_H: f32 = 160.0;
const INK: [u8; 3] = [20, 30, 90];

impl SignatureModal {
    pub fn open(&mut self, library: &SignatureLibrary) {
        self.open = true;
        self.strokes.clear();
        self.current.clear();
        self.typed_name.clear();
        self.uploaded = None;
        self.upload_error = None;
        self.save_name.clear();
        self.save_to_library = false;
        self.saved = library.list();
        self.result = None;
    }

    /// Renders the modal. Returns the captured signature image once, on confirm.
    pub fn show(&mut self, ctx: &egui::Context, library: &SignatureLibrary) -> Option<Arc<RgbaImage>> {
        if !self.open {
            return None;
        }

        let mut confirmed: Option<Arc<RgbaImage>> = None;
        let mut keep_open = true;

        egui::Window::new("Add signature")
            .collapsible(false)
            .resizable(false)
            .open(&mut keep_open)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut self.tab, Tab::Draw, "Draw");
                    ui.selectable_value(&mut self.tab, Tab::Type, "Type");
                    ui.selectable_value(&mut self.tab, Tab::Upload, "Upload");
                });
                ui.separator();

                match self.tab {
                    Tab::Draw => self.draw_tab(ui),
                    Tab::Type => self.type_tab(ui),
                    Tab::Upload => self.upload_tab(ui),
                }

                ui.separator();

                // Saved-signatures shortcuts.
                if !self.saved.is_empty() {
                    ui.label("Saved signatures:");
                    ui.horizontal_wrapped(|ui| {
                        for path in self.saved.clone() {
                            let label = path
                                .file_stem()
                                .and_then(|s| s.to_str())
                                .unwrap_or("signature")
                                .to_string();
                            if ui.button(label).clicked() {
                                if let Ok(img) = SignatureLibrary::load(&path) {
                                    confirmed = Some(Arc::new(img));
                                }
                            }
                        }
                    });
                    ui.separator();
                }

                ui.horizontal(|ui| {
                    ui.checkbox(&mut self.save_to_library, "Save to library as");
                    ui.add_enabled(
                        self.save_to_library,
                        egui::TextEdit::singleline(&mut self.save_name).hint_text("name"),
                    );
                });

                ui.horizontal(|ui| {
                    if ui.button("Place signature").clicked() {
                        if let Some(img) = self.compose() {
                            if self.save_to_library && !self.save_name.trim().is_empty() {
                                let _ = library.save(self.save_name.trim(), &img);
                            }
                            confirmed = Some(Arc::new(img));
                        }
                    }
                    if ui.button("Clear").clicked() {
                        self.strokes.clear();
                        self.current.clear();
                        self.typed_name.clear();
                        self.uploaded = None;
                    }
                    if ui.button("Cancel").clicked() {
                        confirmed = None;
                        self.open = false;
                    }
                });
            });

        if !keep_open {
            self.open = false;
        }
        if confirmed.is_some() {
            self.open = false;
        }
        confirmed
    }

    fn draw_tab(&mut self, ui: &mut egui::Ui) {
        ui.label("Draw your signature:");
        let (rect, response) =
            ui.allocate_exact_size(Vec2::new(CANVAS_W, CANVAS_H), Sense::click_and_drag());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 4.0, Color32::from_gray(250));
        painter.rect_stroke(
            rect,
            4.0,
            Stroke::new(1.0, Color32::from_gray(180)),
            StrokeKind::Inside,
        );

        // Capture drawing.
        if response.dragged() {
            if let Some(p) = response.interact_pointer_pos() {
                let local = (p - rect.min).to_pos2();
                self.current.push((local.x, local.y));
            }
        }
        if response.drag_stopped() && !self.current.is_empty() {
            self.strokes.push(std::mem::take(&mut self.current));
        }

        // Render existing strokes + the in-progress one.
        let draw_poly = |poly: &StrokePath| {
            if poly.len() < 2 {
                return;
            }
            let pts: Vec<Pos2> = poly
                .iter()
                .map(|&(x, y)| Pos2::new(rect.min.x + x, rect.min.y + y))
                .collect();
            painter.add(egui::Shape::line(
                pts,
                Stroke::new(2.5, Color32::from_rgb(INK[0], INK[1], INK[2])),
            ));
        };
        for poly in &self.strokes {
            draw_poly(poly);
        }
        draw_poly(&self.current);
    }

    fn type_tab(&mut self, ui: &mut egui::Ui) {
        ui.label("Type your name (rendered in a script font):");
        ui.text_edit_singleline(&mut self.typed_name);
        if !self.typed_name.trim().is_empty() {
            // Preview in the same script font used for the output, so the
            // preview matches what gets placed.
            ui.label(
                egui::RichText::new(&self.typed_name)
                    .font(egui::FontId::new(
                        44.0,
                        egui::FontFamily::Name("signature".into()),
                    ))
                    .color(Color32::from_rgb(INK[0], INK[1], INK[2])),
            );
        }
    }

    fn upload_tab(&mut self, ui: &mut egui::Ui) {
        ui.label("Upload a signature image (PNG/JPG). White is keyed transparent.");
        if ui.button("Choose file…").clicked() {
            if let Some(path) = rfd::FileDialog::new()
                .add_filter("Image", &["png", "jpg", "jpeg"])
                .pick_file()
            {
                match image::open(&path) {
                    Ok(img) => {
                        self.uploaded = Some(prepare_uploaded(&img));
                        self.upload_error = None;
                    }
                    Err(e) => {
                        self.upload_error = Some(format!("Couldn't load image: {e}"));
                        self.uploaded = None;
                    }
                }
            }
        }
        if let Some(img) = &self.uploaded {
            ui.label(format!("Loaded {}×{} px", img.width(), img.height()));
        }
        if let Some(err) = &self.upload_error {
            ui.colored_label(Color32::RED, err);
        }
    }

    /// Builds the final signature image for the active tab, or `None` if empty.
    fn compose(&self) -> Option<RgbaImage> {
        match self.tab {
            Tab::Draw => {
                if self.strokes.is_empty() && self.current.is_empty() {
                    return None;
                }
                let mut all = self.strokes.clone();
                if !self.current.is_empty() {
                    all.push(self.current.clone());
                }
                let img = rasterize_strokes(&all, CANVAS_W as u32, CANVAS_H as u32, 2.5, INK);
                Some(crate::signature::render::crop_for_modal(img))
            }
            Tab::Type => {
                let name = self.typed_name.trim();
                if name.is_empty() {
                    return None;
                }
                Some(render_typed_name(name, 96.0, INK))
            }
            Tab::Upload => self.uploaded.clone(),
        }
    }
}
