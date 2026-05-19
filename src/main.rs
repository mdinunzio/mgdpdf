use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::Parser;
use pdfium_render::prelude::*;
use tracing::info;

#[derive(Parser, Debug)]
#[command(name = "mgdpdf", about = "Fast Rust PDF viewer + lightweight editor")]
struct Cli {
    /// Path to a PDF file to open
    file: Option<PathBuf>,
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info,wgpu=warn")),
        )
        .init();

    let cli = Cli::parse();

    let pdfium = bind_pdfium().context("failed to bind PDFium library")?;
    info!("PDFium bound successfully");

    if let Some(path) = cli.file.as_deref() {
        let doc = pdfium
            .load_pdf_from_file(path, None)
            .with_context(|| format!("failed to open PDF: {}", path.display()))?;
        println!(
            "{} — {} page(s)",
            path.display(),
            doc.pages().len()
        );
    } else {
        println!("PDFium bound. Pass a PDF path as the first argument to inspect it.");
    }

    Ok(())
}

/// Binds PDFium from an explicit list of candidate directories, in priority order:
///   1. Directory of the running executable (production layout).
///   2. Current working directory (fallback).
///   3. `vendor/pdfium/` relative to CWD (developer workflow without `cargo run`).
///
/// We never call `bind_to_system_library()` — on Windows that's unreliable and would
/// pick up arbitrary DLLs from PATH.
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

    anyhow::bail!(
        "could not load pdfium.dll from any candidate location:\n  {}",
        errors.join("\n  ")
    );
}
