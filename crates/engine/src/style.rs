//! Reference formatting profiles over open documents (Format Studio).
//!
//! Sampling is bounded for large PDFs: at most `max_pages` pages (clamped to
//! 1–20) and 2,000 blocks. `create_from_style` builds a new styled PDF from
//! plain raw text and a profile.

use std::sync::Arc;

use crate::{DocId, Document, EditError, Session};

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
            samples.push(BlockSample {
                text: l.text.clone(),
                size: l.size,
                bold: l.bold,
                rect: l.rect,
                base_font: l.base_font.clone(),
                color: l.color,
            });
        }
    }
    pdfcraft_style::extract(page, &samples).map_err(|e| EditError::Invalid(e.to_string()))
}

impl Session {
    /// A new styled PDF from plain raw text and a profile (Format Studio apply).
    pub fn create_from_style(&self, title: &str, raw_text: &str, profile: &StyleProfile) -> Result<Arc<Vec<u8>>, EditError> {
        let items = pdfcraft_style::parse_plain(raw_text).map_err(|e| EditError::Invalid(e.to_string()))?;
        self.create_from_style_items(title, &items, profile)
    }

    /// A new styled PDF from already-structured items and a profile.
    pub fn create_from_style_items(&self, title: &str, items: &[StyledItem], profile: &StyleProfile) -> Result<Arc<Vec<u8>>, EditError> {
        let doc = pdfcraft_style::render(title, items, profile).map_err(|e| EditError::Invalid(e.to_string()))?;
        self.write_new(&doc)
    }

    /// Restyle an open document's own paragraphs from a profile, in place: each
    /// paragraph keeps its text and box and takes the reference body or heading
    /// size, family and colour. One undo step; atomic (a refusal changes
    /// nothing). Paragraphs the editor can't rewrite (scanned OCR layers,
    /// rotated text, undrawable characters) are skipped and counted.
    pub fn restyle_text(&mut self, id: DocId, profile: &StyleProfile) -> Result<RestyleReport, EditError> {
        if profile.version != 1 {
            return Err(EditError::Invalid("profile.version must be 1".into()));
        }
        let doc = self.get(id).ok_or(EditError::NoDocument)?;
        let pages = doc.info.pages.len();
        if pages == 0 {
            return Err(EditError::Invalid("the document has no pages".into()));
        }
        // The target's own body size and heading sizes classify its paragraphs.
        let first = doc.info.pages.first().ok_or(EditError::NoDocument)?;
        let page_size = [f64::from(first.width), f64::from(first.height)];
        let mut samples = Vec::new();
        for p in 0..pages.min(20) {
            for l in doc.text_lines(p) {
                if samples.len() >= 2000 {
                    break;
                }
                if l.text.trim().is_empty() {
                    continue;
                }
                samples.push(BlockSample {
                    text: l.text.clone(),
                    size: l.size,
                    bold: l.bold,
                    rect: l.rect,
                    base_font: l.base_font.clone(),
                    color: l.color,
                });
            }
        }
        let target = pdfcraft_style::extract(page_size, &samples).map_err(|e| EditError::Invalid(e.to_string()))?;
        let mut target_heads: Vec<f64> = target.headings.iter().map(|h| h.size).collect();
        target_heads.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));
        let family = match profile.base_font {
            BaseFont::Helvetica => crate::FontFamily::Helvetica,
            BaseFont::Times => crate::FontFamily::Times,
            BaseFont::Courier => crate::FontFamily::Courier,
        };
        let mut edits = Vec::new();
        let mut report = RestyleReport::default();
        for page in 0..pages {
            let blocks = doc.text_blocks(page);
            // Descending indices: an applied rewrite must not shift the
            // paragraphs still to come on the same page.
            for (index, block) in blocks.iter().enumerate().rev() {
                if block.text.trim().is_empty() || block.invisible || block.rotation.abs() > 0.1 {
                    report.skipped += 1;
                    continue;
                }
                let rank = target_heads.iter().position(|s| (block.size - s).abs() < 0.51);
                let rank = match rank {
                    Some(r) => r,
                    None if block.size >= target.body_size * 1.08 => target_heads.len(),
                    None => {
                        // Body paragraph.
                        let style = pdfcraft_edit::BlockStyle {
                            family: Some((family, block.bold, block.italic)),
                            bold: Some(block.bold),
                            size: Some(profile.body_size),
                            color: profile.body_color,
                            ..Default::default()
                        };
                        if doc.text_edit_layout(page, index, &block.text, &style).is_err() {
                            report.skipped += 1;
                            continue;
                        }
                        report.paragraphs += 1;
                        edits.push(crate::Edit::EditTextBlock { page, block: index, text: block.text.clone(), style });
                        continue;
                    }
                };
                let Some(heading) = profile.headings.get(rank.min(profile.headings.len().saturating_sub(1))) else {
                    report.skipped += 1;
                    continue;
                };
                let style = pdfcraft_edit::BlockStyle {
                    family: Some((family, heading.bold, block.italic)),
                    bold: Some(heading.bold),
                    size: Some(heading.size),
                    color: profile.heading_color,
                    ..Default::default()
                };
                if doc.text_edit_layout(page, index, &block.text, &style).is_err() {
                    report.skipped += 1;
                    continue;
                }
                report.headings += 1;
                edits.push(crate::Edit::EditTextBlock { page, block: index, text: block.text.clone(), style });
            }
        }
        if edits.is_empty() {
            return Err(EditError::Invalid("no paragraph could be restyled (scanned, rotated or undrawable text is left alone)".into()));
        }
        self.apply(id, crate::Edit::Batch { label: "Restyle text".into(), edits })?;
        Ok(report)
    }
}

/// What `Session::restyle_text` changed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RestyleReport {
    /// Body paragraphs restyled.
    pub paragraphs: usize,
    /// Heading paragraphs restyled.
    pub headings: usize,
    /// Paragraphs left alone (scanned, rotated, or refused by the editor).
    pub skipped: usize,
}
