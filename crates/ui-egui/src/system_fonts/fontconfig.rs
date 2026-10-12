//! Discover font files in configured directories, including Nix store paths.
//!
//! Only the existing fallback filenames are selected by the caller. Font bytes are still
//! checked for size and glyph coverage there; no font is copied or embedded.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

// A configured directory may contain unrelated files or symlink cycles. Bound the walk.
const MAX_ENTRIES: usize = 32_768;
const MAX_DEPTH: usize = 16;

pub(super) fn config_path() -> PathBuf {
    let file = std::env::var_os("FONTCONFIG_FILE").map_or_else(|| PathBuf::from("fonts.conf"), PathBuf::from);
    if file.is_absolute() {
        return file;
    }
    let dir =
        std::env::var_os("FONTCONFIG_PATH").and_then(|paths| std::env::split_paths(&paths).next()).unwrap_or_else(|| PathBuf::from("/etc/fonts"));
    dir.join(file)
}

pub(super) fn font_files(config_path: &Path) -> Vec<PathBuf> {
    let mut config = fontconfig_parser::FontConfig::default();
    // Missing or malformed configuration must not prevent the ordinary fallback paths.
    if let Err(error) = config.merge_config(config_path) {
        log::debug!("interface font directories: {}: {error}", config_path.display());
    }
    let mut pending: Vec<_> = config.dirs.into_iter().take(MAX_ENTRIES).map(|dir| (dir.path, 0)).collect();
    let mut seen = HashSet::new();
    let mut files = Vec::new();
    let mut remaining = MAX_ENTRIES;
    while let Some((dir, depth)) = pending.pop() {
        let Ok(canonical) = dir.canonicalize() else { continue };
        if !seen.insert(canonical) {
            continue;
        }
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries {
            if remaining == 0 {
                files.sort();
                return files;
            }
            remaining -= 1;
            let Ok(entry) = entry else { continue };
            let path = entry.path();
            // Follow links to fonts and directories, but visit each directory only once.
            if path.is_dir() {
                if depth < MAX_DEPTH {
                    pending.push((path, depth + 1));
                }
            } else if path.extension().is_some_and(|ext| ext == "ttf" || ext == "ttc") && path.is_file() {
                files.push(path);
            }
        }
    }
    files.sort();
    files
}

#[cfg(test)]
mod tests {
    use super::*;

    // Fixtures contain only configuration and dummy files, never copied system fonts.
    struct Fixture(PathBuf);

    impl Fixture {
        fn new() -> Self {
            static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
            let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let dir = std::env::temp_dir().join(format!("pdfcraft-fontconfig-{}-{n}", std::process::id()));
            std::fs::create_dir(&dir).unwrap();
            Self(dir)
        }

        fn config(&self, name: &str, body: &str) -> PathBuf {
            let path = self.0.join(name);
            std::fs::write(&path, format!("<fontconfig>{body}</fontconfig>")).unwrap();
            path
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn included_configuration_finds_fonts_outside_standard_directories() {
        let fixture = Fixture::new();
        let fonts = fixture.0.join("nix-store-font/share/fonts/truetype");
        std::fs::create_dir_all(&fonts).unwrap();
        let font = fonts.join("DejaVuSans.ttf");
        std::fs::write(&font, b"not parsed during discovery").unwrap();
        std::fs::write(fonts.join("README.txt"), b"not a font").unwrap();
        let included = fixture.config("nixos.conf", &format!("<dir>{}</dir>", fixture.0.join("nix-store-font").display()));
        let config = fixture.config("fonts.conf", &format!("<include>{}</include>", included.display()));
        assert_eq!(font_files(&config), vec![font]);
    }

    #[test]
    fn missing_and_malformed_configuration_are_skipped() {
        let fixture = Fixture::new();
        assert!(font_files(&fixture.0.join("missing.conf")).is_empty());
        let path = fixture.0.join("broken.conf");
        std::fs::write(&path, "<fontconfig><dir>").unwrap();
        assert!(font_files(&path).is_empty());
    }

    #[test]
    fn symlinked_fonts_are_found_and_directory_cycles_terminate() {
        let fixture = Fixture::new();
        let fonts = fixture.0.join("fonts");
        std::fs::create_dir(&fonts).unwrap();
        let original = fixture.0.join("original.ttf");
        std::fs::write(&original, b"dummy").unwrap();
        let linked = fonts.join("DejaVuSans.ttf");
        std::os::unix::fs::symlink(&original, &linked).unwrap();
        std::os::unix::fs::symlink(&fonts, fonts.join("loop")).unwrap();
        std::os::unix::fs::symlink(fixture.0.join("missing.ttf"), fonts.join("broken.ttf")).unwrap();
        let config = fixture.config("fonts.conf", &format!("<dir>{}</dir><dir>{}</dir>", fonts.display(), fonts.display()));
        assert_eq!(font_files(&config), vec![linked]);
    }

    #[test]
    fn deeply_nested_directories_are_bounded() {
        let fixture = Fixture::new();
        let mut dir = fixture.0.join("fonts");
        std::fs::create_dir(&dir).unwrap();
        let shallow = dir.join("DejaVuSans.ttf");
        std::fs::write(&shallow, b"dummy").unwrap();
        for _ in 0..MAX_DEPTH + 1 {
            dir.push("nested");
            std::fs::create_dir(&dir).unwrap();
        }
        std::fs::write(dir.join("DejaVuSans.ttf"), b"dummy").unwrap();
        let config = fixture.config("fonts.conf", &format!("<dir>{}</dir>", fixture.0.join("fonts").display()));
        assert_eq!(font_files(&config), vec![shallow]);
    }
}
