//! Reference formatting profiles over open documents (Format Studio).
//!
//! Sampling is bounded for large PDFs: at most `max_pages` pages (clamped to
//! 1–20) and 2,000 blocks. `create_from_style` builds a new styled PDF from
//! plain raw text and a profile.

use std::sync::Arc;

use crate::{Document, EditError, Session};

pub use pdfcraft_style::{BaseFont, BlockSample, HeadingStyle, StyleError, StyleProfile, StyledItem};

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
