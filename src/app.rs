//! Top-level eframe App.

use std::path::{Path, PathBuf};

use eframe::egui;
use egui::{Key, Modifiers};
use pdfium_render::prelude::Pdfium;
use tracing::warn;

use crate::edit::{EditSession, UndoStack};
use crate::pdf::{Document, TextureCache};
use crate::tools::ToolBox;
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
    pending_open_dialog: bool,

    tools: ToolBox,
    session: EditSession,
    undo: UndoStack,
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
            pending_open_dialog: false,
            tools: ToolBox::default(),
            session: EditSession::new(0),
            undo: UndoStack::default(),
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
                self.session = EditSession::new(doc.page_count());
                self.undo.clear();
                self.doc = Some(doc);
                self.current_page = 0;
                self.zoom = DEFAULT_ZOOM;
                self.error = None;
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
        let (open, zoom_in, zoom_out, zoom_reset, ctrl_scroll, undo, redo) = ctx.input_mut(|i| {
            let open = i.consume_key(Modifiers::CTRL, Key::O);
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

            (open, zoom_in, zoom_out, zoom_reset, ctrl_scroll, undo, redo)
        });

        if open {
            self.pending_open_dialog = true;
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
    }
}
