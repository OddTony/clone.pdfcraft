//! PdfCraft patch: the file-name rules of the iOS backend (`ios.rs`), kept apart from UIKit so
//! they are tested on every platform.

use std::path::{Path, PathBuf};

/// `dir/<name>`, or `dir/<stem> 2.<ext>`, `dir/<stem> 3.<ext>`, ... for the first that is free.
pub(super) fn free_name(dir: &Path, file: &Path) -> Option<PathBuf> {
    let name = file.file_name()?;
    let first = dir.join(name);
    if !first.exists() {
        return Some(first);
    }
    let stem = file.file_stem()?.to_string_lossy();
    let ext = file
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();
    (2..10_000u32)
        .map(|n| dir.join(format!("{stem} {n}{ext}")))
        .find(|p| !p.exists())
}

/// The name a save prompt answer becomes: trimmed, without path separators, with the first
/// filter's extension when it has none. `None` when nothing usable is left.
pub(super) fn save_name(typed: &str, extensions: &[String]) -> Option<String> {
    let name: String = typed
        .trim()
        .chars()
        .map(|c| if c == '/' || c == ':' { '-' } else { c })
        .collect();
    let name = name.trim_start_matches('.').to_string();
    if name.is_empty() {
        return None;
    }
    let ext = extensions.iter().find(|e| !e.is_empty() && *e != "*");
    match ext {
        Some(ext) if Path::new(&name).extension().is_none() => Some(format!("{name}.{ext}")),
        _ => Some(name),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_name_cleans_and_adds_the_filter_extension() {
        let pdf = vec!["pdf".to_owned()];
        assert_eq!(save_name("  Report ", &pdf).as_deref(), Some("Report.pdf"));
        assert_eq!(save_name("a/b:c.pdf", &pdf).as_deref(), Some("a-b-c.pdf"));
        assert_eq!(save_name("notes.txt", &pdf).as_deref(), Some("notes.txt"));
        assert_eq!(save_name("..hidden", &[]).as_deref(), Some("hidden"));
        assert_eq!(save_name("   ", &pdf), None);
        assert_eq!(save_name("...", &pdf), None);
    }

    #[test]
    fn free_name_counts_past_taken_names() {
        let dir = std::env::temp_dir().join(format!("rfd-ios-free-name-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let src = Path::new("/inbox/Doc.pdf");
        assert_eq!(free_name(&dir, src), Some(dir.join("Doc.pdf")));
        std::fs::write(dir.join("Doc.pdf"), b"").unwrap();
        std::fs::write(dir.join("Doc 2.pdf"), b"").unwrap();
        assert_eq!(free_name(&dir, src), Some(dir.join("Doc 3.pdf")));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
