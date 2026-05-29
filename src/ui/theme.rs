//! App-wide visual style. Installed once at startup from `main.rs` so every
//! panel, button, and page picks it up automatically.
//!
//! Goals: a clean light theme that reads as "PDF reader" — soft neutral gutter
//! around white pages, slightly-rounded controls, generous-but-not-loose
//! spacing, and a refined toolbar/status-bar palette.

use eframe::egui;
use egui::{Color32, CornerRadius, Stroke, Vec2};

/// Background of the viewport behind pages. Soft, neutral, not pure white.
pub const VIEWPORT_BG: Color32 = Color32::from_rgb(0xEC, 0xEE, 0xF1);

/// Toolbar / status-bar background.
pub const PANEL_BG: Color32 = Color32::from_rgb(0xF7, 0xF8, 0xFA);

/// Borders around panels and pages.
pub const BORDER: Color32 = Color32::from_rgb(0xD6, 0xDA, 0xE0);

/// Subtle accent for the active tool selection.
pub const ACCENT: Color32 = Color32::from_rgb(0x29, 0x5A, 0xC4);

/// Brand accent text (file name / page indicator).
pub const TEXT_MUTED: Color32 = Color32::from_rgb(0x5C, 0x65, 0x73);

/// Drop-shadow tint under each rendered PDF page.
pub const PAGE_SHADOW: Color32 = Color32::from_rgba_premultiplied(0, 0, 0, 32);

/// Installs the light theme on `ctx`. Idempotent.
pub fn install(ctx: &egui::Context) {
    let mut style = (*ctx.global_style()).clone();

    style.visuals = egui::Visuals::light();
    style.visuals.panel_fill = PANEL_BG;
    style.visuals.window_fill = PANEL_BG;
    style.visuals.extreme_bg_color = Color32::WHITE;
    style.visuals.faint_bg_color = Color32::from_rgb(0xF1, 0xF3, 0xF5);
    style.visuals.window_corner_radius = CornerRadius::same(8);
    style.visuals.menu_corner_radius = CornerRadius::same(6);
    style.visuals.window_stroke = Stroke::new(1.0, BORDER);
    style.visuals.widgets.noninteractive.bg_stroke = Stroke::new(1.0, BORDER);

    // Round buttons a bit, give them clearer states.
    let r = CornerRadius::same(5);
    for w in [
        &mut style.visuals.widgets.inactive,
        &mut style.visuals.widgets.hovered,
        &mut style.visuals.widgets.active,
        &mut style.visuals.widgets.open,
    ] {
        w.corner_radius = r;
    }
    style.visuals.widgets.inactive.weak_bg_fill = Color32::from_rgb(0xEE, 0xF0, 0xF4);
    style.visuals.widgets.inactive.bg_fill = Color32::from_rgb(0xEE, 0xF0, 0xF4);
    style.visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, Color32::from_rgb(0xDC, 0xE0, 0xE6));
    style.visuals.widgets.hovered.weak_bg_fill = Color32::from_rgb(0xE2, 0xE6, 0xEE);
    style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(0xE2, 0xE6, 0xEE);
    style.visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, Color32::from_rgb(0xB7, 0xC0, 0xCC));
    style.visuals.widgets.active.weak_bg_fill = Color32::from_rgb(0xD3, 0xDF, 0xF2);
    style.visuals.widgets.active.bg_fill = Color32::from_rgb(0xD3, 0xDF, 0xF2);
    style.visuals.widgets.active.bg_stroke = Stroke::new(1.0, ACCENT);

    // Selected toggle / SelectableLabel — used for the active tool.
    style.visuals.selection.bg_fill = ACCENT;
    style.visuals.selection.stroke = Stroke::new(1.0, ACCENT);

    // Generous toolbar spacing without feeling sparse.
    style.spacing.button_padding = Vec2::new(10.0, 5.0);
    style.spacing.item_spacing = Vec2::new(8.0, 6.0);
    style.spacing.menu_margin = egui::Margin::same(6);
    style.spacing.window_margin = egui::Margin::same(10);

    ctx.set_global_style(style);
}
