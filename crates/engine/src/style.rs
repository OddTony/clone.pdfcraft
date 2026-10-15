//! Reference formatting profiles over open documents (Format Studio).
//!
//! Sampling is bounded for large PDFs: at most `max_pages` pages (clamped to
//! 1–20) and 2,000 blocks. `create_from_style` builds a new styled PDF from
//! plain raw text and a profile.

use std::sync::Arc;

use crate::{Document, EditError, Session};

pub use pdfcraft_style::{BaseFont, BlockSample, HeadingStyle, StyleError, StyleProfile, StyledItem};

/// Bounded raw text of a PDF for Format Studio generation: pages joined with
/// `Page N:` headers (like the automation `raw_doc` path), at most
/// `max_pages` pages (clamped to 1–2000) and `max_chars` characters (clamped
/// to 1 KiB–4 MiB, cut at a char boundary). Unreadable pages are skipped.
/// Returns the text and whether anything was left out. Never fails: the
/// caller checks copying permission first. Thread-safe: everything is owned.
pub fn source_text(bytes: &Arc<Vec<u8>>, password: Option<&str>, page_count: usize, max_pages: usize, max_chars: usize) -> (String, bool) {
    let pages = page_count.min(max_pages.clamp(1, 2000));
    let cap = max_chars.clamp(1024, 4_000_000);
    let config = pdfcraft_render::RenderConfig { password: password.map(std::sync::Arc::from), ..Default::default() };
    let mut renderer = pdfcraft_render::PageRenderer::new(bytes.clone(), config);
    let mut out = String::new();
    let mut left_out = page_count > pages;
    for page in 0..pages {
        let rendered =
            renderer.render(pdfcraft_render::RenderRequest { page, kind: pdfcraft_render::RequestKind::Text, scale: 1.0, ..Default::default() });
        let Some(text) = rendered.text else {
            left_out = true;
            continue;
        };
        let chunk = format!("Page {}:\n{}\n\n", page + 1, text.plain_text());
        if out.len() + chunk.len() > cap {
            let rest = cap.saturating_sub(out.len()).min(chunk.len());
            out.push_str(&chunk[..chunk.floor_char_boundary(rest)]);
            left_out = true;
            break;
        }
        out.push_str(&chunk);
    }
    (out, left_out)
}

/// Extract the [`StyleProfile`] of an open document.
///
/// Samples per-run text lines (not grouped paragraphs: grouping merges mixed
/// sizes into one region), at most `max_pages` pages (clamped to 1–20) and
/// 2,000 lines, so large PDFs stay fast.
pub fn profile_of(doc: &Document, max_pages: usize) -> Result<StyleProfile, EditError> {
    let pages = doc.info.pages.len();
    if pages == 0 {
        return Err(EditError::Invalid("the document has no pages".into()));
    }
    let first = doc.info.pages.first().ok_or_else(|| EditError::Invalid("the document has no pages".into()))?;
    let page = [f64::from(first.width), f64::from(first.height)];
    let limit = max_pages.clamp(1, 20).min(pages);
    let mut samples = Vec::new();
    for p in 0..limit {
        for l in doc.text_lines(p) {
            if samples.len() >= 2000 {
                break;
            }
            if l.text.trim().is_empty() {
                continue;
            }
            samples.push(BlockSample { text: l.text.clone(), size: l.size, bold: l.bold, rect: l.rect, base_font: l.base_font.clone() });
        }
    }
    pdfcraft_style::extract(page, &samples).map_err(|e| EditError::Invalid(e.to_string()))
}

impl Session {
    /// A new styled PDF from plain raw text and a profile (Format Studio apply).
    pub fn create_from_style(&self, title: &str, raw_text: &str, profile: &StyleProfile) -> Result<Arc<Vec<u8>>, EditError> {
        let items = pdfcraft_style::parse_plain(raw_text).map_err(|e| EditError::Invalid(e.to_string()))?;
        let doc = pdfcraft_style::render(title, &items, profile).map_err(|e| EditError::Invalid(e.to_string()))?;
        self.write_new(&doc)
    }
}
