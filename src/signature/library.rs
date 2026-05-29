//! Persistent library of saved signatures, stored as PNGs under the user's
//! data directory (`%APPDATA%/mgdpdf/signatures/` on Windows). Lets the user
//! reuse a signature across sessions instead of re-drawing it each time.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use image::RgbaImage;

pub struct SignatureLibrary {
    dir: PathBuf,
}

impl SignatureLibrary {
    /// Opens (creating if needed) the signatures directory under the platform
    /// data dir. Falls back to `./signatures` if the data dir can't be found.
    pub fn open() -> Self {
        let dir = directories::ProjectDirs::from("", "", "mgdpdf")
            .map(|d| d.data_dir().join("signatures"))
            .unwrap_or_else(|| PathBuf::from("signatures"));
        let _ = std::fs::create_dir_all(&dir);
        Self { dir }
    }

    /// Library rooted at an explicit directory (used by tests).
    pub fn at(dir: impl Into<PathBuf>) -> Self {
        let dir = dir.into();
        let _ = std::fs::create_dir_all(&dir);
        Self { dir }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Saves `image` as a PNG named `<name>.png`, returning its path. The name
    /// is sanitised to a safe filename stem.
    pub fn save(&self, name: &str, image: &RgbaImage) -> Result<PathBuf> {
        let stem = sanitize(name);
        let path = self.dir.join(format!("{stem}.png"));
        image
            .save(&path)
            .with_context(|| format!("saving signature to {}", path.display()))?;
        Ok(path)
    }

    /// Lists saved signature PNG paths, sorted by name.
    pub fn list(&self) -> Vec<PathBuf> {
        let mut out: Vec<PathBuf> = std::fs::read_dir(&self.dir)
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("png"))
            .collect();
        out.sort();
        out
    }

    /// Loads a saved signature image from a path.
    pub fn load(path: &Path) -> Result<RgbaImage> {
        let img = image::open(path)
            .with_context(|| format!("loading signature {}", path.display()))?;
        Ok(img.to_rgba8())
    }

    /// Deletes a saved signature file.
    #[allow(dead_code)]
    pub fn delete(path: &Path) -> Result<()> {
        std::fs::remove_file(path)
            .with_context(|| format!("deleting signature {}", path.display()))?;
        Ok(())
    }
}

/// Reduces an arbitrary name to a safe filename stem.
fn sanitize(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
        .collect();
    let trimmed = cleaned.trim_matches('_');
    if trimmed.is_empty() {
        "signature".to_string()
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    #[test]
    fn save_list_load_round_trip() {
        let tmp = std::env::temp_dir().join("mgdpdf-test-sig-lib");
        let _ = std::fs::remove_dir_all(&tmp);
        let lib = SignatureLibrary::at(&tmp);

        let mut img = RgbaImage::new(3, 2);
        img.put_pixel(0, 0, Rgba([10, 20, 30, 255]));
        let path = lib.save("My Name!", &img).expect("save");
        assert!(path.exists());
        assert_eq!(path.file_name().unwrap().to_str().unwrap(), "My_Name.png");

        let listed = lib.list();
        assert_eq!(listed.len(), 1);

        let loaded = SignatureLibrary::load(&listed[0]).expect("load");
        assert_eq!(loaded.dimensions(), (3, 2));
        assert_eq!(loaded.get_pixel(0, 0).0, [10, 20, 30, 255]);

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn sanitize_handles_empty_and_symbols() {
        assert_eq!(sanitize(""), "signature");
        assert_eq!(sanitize("!!!"), "signature");
        assert_eq!(sanitize("Ada L."), "Ada_L");
    }
}
