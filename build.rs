use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=vendor/pdfium/pdfium.dll");
    println!("cargo:rerun-if-changed=build.rs");

    let src = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("vendor")
        .join("pdfium")
        .join("pdfium.dll");

    if !src.exists() {
        panic!(
            "vendored pdfium.dll not found at {} — see vendor/pdfium/ in repo",
            src.display()
        );
    }

    let target_dir = locate_target_dir();
    let dest = target_dir.join("pdfium.dll");

    if needs_copy(&src, &dest) {
        std::fs::copy(&src, &dest).unwrap_or_else(|e| {
            panic!("failed to copy {} → {}: {e}", src.display(), dest.display())
        });
    }
}

fn locate_target_dir() -> PathBuf {
    let out_dir = PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR not set"));
    let mut dir = out_dir.as_path();
    while let Some(parent) = dir.parent() {
        if dir.file_name().and_then(|n| n.to_str()) == Some("build") {
            return parent.to_path_buf();
        }
        dir = parent;
    }
    out_dir
}

fn needs_copy(src: &std::path::Path, dest: &std::path::Path) -> bool {
    let Ok(src_meta) = std::fs::metadata(src) else {
        return true;
    };
    let Ok(dest_meta) = std::fs::metadata(dest) else {
        return true;
    };
    if src_meta.len() != dest_meta.len() {
        return true;
    }
    match (src_meta.modified(), dest_meta.modified()) {
        (Ok(s), Ok(d)) => s > d,
        _ => true,
    }
}
