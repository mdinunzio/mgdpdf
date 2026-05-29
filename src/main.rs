#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Parser;
use eframe::egui;
use mgdpdf::app;
use pdfium_render::prelude::*;
use tracing::info;

#[derive(Parser, Debug)]
#[command(name = "mgdpdf", about = "Fast Rust PDF viewer + lightweight editor")]
struct Cli {
    /// Path to a PDF file to open on startup
    file: Option<PathBuf>,
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info,wgpu_core=warn,wgpu_hal=warn,naga=warn")),
        )
        .init();

    let cli = Cli::parse();

    let pdfium = Box::leak(Box::new(bind_pdfium()?));
    info!("PDFium bound successfully");

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1100.0, 800.0])
            .with_min_inner_size([600.0, 400.0])
            .with_title("mgdpdf"),
        ..Default::default()
    };

    eframe::run_native(
        "mgdpdf",
        native_options,
        Box::new(move |_cc| Ok(Box::new(app::App::new(pdfium, cli.file)))),
    )
    .map_err(|e| anyhow::anyhow!("eframe: {e}"))?;

    Ok(())
}

/// Binds PDFium from candidate directories, in priority order:
///   1. Directory of the running executable (production layout).
///   2. Current working directory (fallback).
///   3. `vendor/pdfium/` relative to CWD (developer workflow).
///
/// We never call `bind_to_system_library()` — on Windows that picks up arbitrary
/// DLLs from PATH.
fn bind_pdfium() -> Result<Pdfium> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.to_path_buf());
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.clone());
        candidates.push(cwd.join("vendor").join("pdfium"));
    }

    let mut errors: Vec<String> = Vec::new();
    for dir in &candidates {
        let lib = Pdfium::pdfium_platform_library_name_at_path(dir);
        match Pdfium::bind_to_library(&lib) {
            Ok(bindings) => {
                info!("loaded pdfium from {}", lib.display());
                return Ok(Pdfium::new(bindings));
            }
            Err(e) => errors.push(format!("{}: {e}", lib.display())),
        }
    }

    Err(anyhow::anyhow!(
        "could not load pdfium.dll from any candidate location:\n  {}",
        errors.join("\n  ")
    ))
    .context("failed to bind PDFium library")
}
