//! Top-level eframe App.

use std::path::{Path, PathBuf};

use eframe::egui;
use egui::{Key, Modifiers};
use pdfium_render::prelude::Pdfium;
use tracing::warn;

use crate::edit::{EditSession, UndoStack};
use crate::pdf::document::{EditBundle, FreeTextSpec, TextFieldWidget};
use crate::pdf::{Document, TextureCache};
use crate::tools::{ToolBox, ToolSettings};
use crate::ui::page_view::{PageView, PageViewState};

const MIN_ZOOM: f32 = 0.25;
const MAX_ZOOM: f32 = 6.0;
const DEFAULT_ZOOM: f32 = 1.0;
const ZOOM_STEP: f32 = 1.1;

pub struct App {
    pdfium: &'static Pdfium,
    doc: Option<Document>,
    cache: TextureCache,
    zoom: f32,
    /// 0-based index of the page nearest the viewport centre; updated each frame.
    current_page: usize,
    error: Option<String>,
    status: Option<String>,
    pending_open_dialog: bool,
    pending_save_as_dialog: bool,

    tools: ToolBox,
    session: EditSession,
    undo: UndoStack,
    /// Cached text-field widgets for the open document. Rebuilt on each open.
    widgets: Vec<TextFieldWidget>,
    /// Styling applied to newly-created edits (free-text font size + colour).
    tool_settings: ToolSettings,
}

impl App {
    pub fn new(pdfium: &'static Pdfium, initial_file: Option<PathBuf>) -> Self {
        let mut app = Self {
            pdfium,
            doc: None,
            cache: TextureCache::new(),
            zoom: DEFAULT_ZOOM,
            current_page: 0,
            error: None,
            status: None,
            pending_open_dialog: false,
            pending_save_as_dialog: false,
            tools: ToolBox::default(),
            session: EditSession::new(0),
            undo: UndoStack::default(),
            widgets: Vec::new(),
            tool_settings: ToolSettings::default(),
        };
        if let Some(path) = initial_file {
            app.open_path(&path);
        }
        app
    }

    fn open_path(&mut self, path: &Path) {
        self.cache.clear();
        match Document::open(self.pdfium, path) {
            Ok(doc) => {
                self.widgets = doc.collect_text_widgets();
                self.session = EditSession::new(doc.page_count());
                self.undo.clear();
                self.doc = Some(doc);
                self.current_page = 0;
                self.zoom = DEFAULT_ZOOM;
                self.error = None;
                self.status = None;
            }
            Err(e) => {
                let msg = format!("Failed to open {}: {e:#}", path.display());
                warn!("{msg}");
                self.error = Some(msg);
            }
        }
    }

    fn open_via_dialog(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("PDF", &["pdf"])
            .pick_file()
        {
            self.open_path(&path);
        }
    }

    /// Builds an edit bundle from the session and writes it to `path` against a
    /// fresh copy of the source PDF (so saving is idempotent and never mutates
    /// the working document or the original file).
    fn save_to(&mut self, path: &Path) {
        let Some(doc) = self.doc.as_ref() else {
            return;
        };

        let mut bundle = EditBundle::default();
        bundle.form_fills = self
            .session
            .iter_form_fills()
            .map(|(id, v)| (id, v.to_string()))
            .collect();
        bundle.free_texts = self
            .session
            .iter_free_texts()
            .map(|b| FreeTextSpec {
                page_index: b.page_index,
                origin_pt: b.origin_pt,
                size_pt: b.size_pt,
                text: b.text.clone(),
                color: b.color,
            })
            .collect();

        if let Err(e) = doc.save_with_edits(path, &bundle) {
            let msg = format!("Failed to save {}: {e:#}", path.display());
            warn!("{msg}");
            self.error = Some(msg);
            return;
        }
        self.session.dirty = false;
        self.status = Some(format!("Saved to {}", path.display()));
    }

    fn save_as_via_dialog(&mut self) {
        let suggested = self
            .doc
            .as_ref()
            .and_then(|d| d.path().file_stem().map(|s| s.to_string_lossy().to_string()))
            .map(|stem| format!("{stem}-edited.pdf"))
            .unwrap_or_else(|| "edited.pdf".to_string());
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("PDF", &["pdf"])
            .set_file_name(&suggested)
            .save_file()
        {
            self.save_to(&path);
        }
    }

    fn set_zoom(&mut self, new_zoom: f32) {
        self.zoom = new_zoom.clamp(MIN_ZOOM, MAX_ZOOM);
    }

    fn handle_input(&mut self, ctx: &egui::Context) {
        // File drop.
        let dropped = ctx.input(|i| i.raw.dropped_files.clone());
        if let Some(file) = dropped.into_iter().find_map(|f| f.path) {
            self.open_path(&file);
        }

        // Keyboard shortcuts.
        let (open, save, save_as, zoom_in, zoom_out, zoom_reset, ctrl_scroll, undo, redo) =
            ctx.input_mut(|i| {
                let open = i.consume_key(Modifiers::CTRL, Key::O);
                let save = i.consume_key(Modifiers::CTRL, Key::S);
                let save_as = i.consume_key(Modifiers::CTRL | Modifiers::SHIFT, Key::S);
                let zoom_in = i.consume_key(Modifiers::CTRL, Key::Plus)
                    || i.consume_key(Modifiers::CTRL, Key::Equals);
                let zoom_out = i.consume_key(Modifiers::CTRL, Key::Minus);
                let zoom_reset = i.consume_key(Modifiers::CTRL, Key::Num0);
                let undo = i.consume_key(Modifiers::CTRL, Key::Z);
                let redo = i.consume_key(Modifiers::CTRL, Key::Y)
                    || i.consume_key(Modifiers::CTRL | Modifiers::SHIFT, Key::Z);

                let ctrl_scroll = if i.modifiers.ctrl {
                    let dy = i.smooth_scroll_delta.y;
                    if dy.abs() > 0.5 {
                        i.smooth_scroll_delta.y = 0.0;
                        Some(dy)
                    } else {
                        None
                    }
                } else {
                    None
                };

                (open, save, save_as, zoom_in, zoom_out, zoom_reset, ctrl_scroll, undo, redo)
            });

        if open {
            self.pending_open_dialog = true;
        }
        // Ctrl+S → Save As by default. We intentionally don't overwrite the
        // original — that's safer for the user, matches Adobe Reader's free
        // tier behaviour, and follows the plan.
        if save || save_as {
            self.pending_save_as_dialog = true;
        }
        if zoom_in {
            self.set_zoom(self.zoom * ZOOM_STEP);
        }
        if zoom_out {
            self.set_zoom(self.zoom / ZOOM_STEP);
        }
        if zoom_reset {
            self.set_zoom(DEFAULT_ZOOM);
        }
        if let Some(dy) = ctrl_scroll {
            let factor = if dy > 0.0 { ZOOM_STEP } else { 1.0 / ZOOM_STEP };
            self.set_zoom(self.zoom * factor);
        }
        if undo {
            self.undo.undo(&mut self.session);
        }
        if redo {
            self.undo.redo(&mut self.session);
        }
    }
}

impl eframe::App for App {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.handle_input(ctx);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::Panel::top("toolbar").show_inside(ui, |ui| {
            ui.horizontal(|ui| {
                if ui.button("Open…").on_hover_text("Ctrl+O").clicked() {
                    self.pending_open_dialog = true;
                }
                let can_save = self.doc.is_some();
                if ui
                    .add_enabled(can_save, egui::Button::new("Save As…"))
                    .on_hover_text("Ctrl+S (saves to a new file)")
                    .clicked()
                {
                    self.pending_save_as_dialog = true;
                }
                ui.separator();

                // Tool picker.
                let active = self.tools.active_index();
                let tool_buttons: Vec<(usize, &'static str)> = self
                    .tools
                    .tools()
                    .map(|(i, t)| (i, t.label()))
                    .collect();
                for (i, label) in tool_buttons {
                    if ui.selectable_label(i == active, label).clicked() {
                        self.tools.set_active(i);
                    }
                }
                ui.separator();

                // Text-styling controls — only relevant for the free-text tool.
                if self.tools.active().id() == "free_text" {
                    ui.label("Size");
                    ui.add(
                        egui::DragValue::new(&mut self.tool_settings.font_size)
                            .range(6.0..=72.0)
                            .speed(0.5)
                            .suffix(" pt"),
                    );
                    let mut rgb = [
                        self.tool_settings.text_color[0],
                        self.tool_settings.text_color[1],
                        self.tool_settings.text_color[2],
                    ];
                    if ui.color_edit_button_srgb(&mut rgb).changed() {
                        self.tool_settings.text_color = [rgb[0], rgb[1], rgb[2], 255];
                    }
                    ui.separator();
                }

                if ui
                    .add_enabled(self.undo.can_undo(), egui::Button::new("Undo"))
                    .on_hover_text("Ctrl+Z")
                    .clicked()
                {
                    self.undo.undo(&mut self.session);
                }
                if ui
                    .add_enabled(self.undo.can_redo(), egui::Button::new("Redo"))
                    .on_hover_text("Ctrl+Y / Ctrl+Shift+Z")
                    .clicked()
                {
                    self.undo.redo(&mut self.session);
                }
                ui.separator();

                if ui.button("−").on_hover_text("Zoom out (Ctrl+-)").clicked() {
                    self.set_zoom(self.zoom / ZOOM_STEP);
                }
                ui.label(format!("{:>3.0}%", self.zoom * 100.0));
                if ui.button("+").on_hover_text("Zoom in (Ctrl+=)").clicked() {
                    self.set_zoom(self.zoom * ZOOM_STEP);
                }
                if ui.button("100%").on_hover_text("Ctrl+0").clicked() {
                    self.set_zoom(DEFAULT_ZOOM);
                }
                ui.separator();

                if let Some(doc) = &self.doc {
                    ui.label(format!(
                        "Page {} / {}",
                        self.current_page + 1,
                        doc.page_count()
                    ));
                    ui.separator();
                    if self.session.dirty {
                        ui.colored_label(egui::Color32::from_rgb(200, 130, 0), "● unsaved");
                        ui.separator();
                    }
                    if let Some(name) = doc.path().file_name().and_then(|n| n.to_str()) {
                        ui.label(name);
                    }
                }
            });
        });

        egui::CentralPanel::default().show_inside(ui, |ui| {
            if let Some(msg) = &self.error {
                ui.colored_label(egui::Color32::RED, msg);
            } else if let Some(msg) = &self.status {
                ui.colored_label(egui::Color32::from_rgb(60, 140, 60), msg);
            }

            match &self.doc {
                Some(doc) => {
                    self.current_page = PageView::show(
                        ui,
                        PageViewState {
                            doc,
                            cache: &mut self.cache,
                            zoom: self.zoom,
                            tools: &mut self.tools,
                            session: &mut self.session,
                            undo: &mut self.undo,
                            widgets: &self.widgets,
                            settings: self.tool_settings,
                        },
                    );
                }
                None => {
                    ui.centered_and_justified(|ui| {
                        ui.label("Open a PDF (Ctrl+O) or drag one onto the window.");
                    });
                }
            }
        });

        if std::mem::take(&mut self.pending_open_dialog) {
            self.open_via_dialog();
        }
        if std::mem::take(&mut self.pending_save_as_dialog) {
            self.save_as_via_dialog();
        }
    }
}
