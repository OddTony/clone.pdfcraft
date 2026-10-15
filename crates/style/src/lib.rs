//! pdfcraft-style — reference formatting profiles (L3).
//!
//! A [`StyleProfile`] is the deterministic part of "format like this document":
//! page size, margins, body size, up to three heading levels, bullet indent and
//! whether the reference numbers its pages. Extraction takes plain block samples
//! (so this crate never depends on the edit or render layers); generation writes
//! a new PDF from [`StyledItem`]s with the profile's geometry.
//!
//! The LLM (OpenRouter BYOK) only maps raw content onto items and interprets
//! follow-up prompts. It never invents geometry.

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

use pdfcraft_cos::{Dict, Document, ObjRef, Object, PdfString, Stream};
use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum StyleError {
    #[error("{0}")]
    Invalid(String),
}

/// One text line sampled from a reference page (mapped from `TextLine` by the caller).
#[derive(Clone, Debug)]
pub struct BlockSample {
    pub text: String,
    pub size: f64,
    pub bold: bool,
    pub rect: [f64; 4],
    pub base_font: String,
    /// The text fill colour as RGB.
    pub color: [f64; 3],
}

/// Which Type 1 family generated pages use (WinAnsi, like `pdfcraft-create`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BaseFont {
    Helvetica,
    Times,
    Courier,
}

impl BaseFont {
    fn base_name(self, bold: bool, italic: bool) -> &'static str {
        match (self, bold, italic) {
            (Self::Helvetica, false, false) => "Helvetica",
            (Self::Helvetica, true, false) => "Helvetica-Bold",
            (Self::Helvetica, false, true) => "Helvetica-Oblique",
            (Self::Helvetica, true, true) => "Helvetica-BoldOblique",
            (Self::Times, false, false) => "Times-Roman",
            (Self::Times, true, false) => "Times-Bold",
            (Self::Times, false, true) => "Times-Italic",
            (Self::Times, true, true) => "Times-BoldItalic",
            (Self::Courier, false, false) => "Courier",
            (Self::Courier, true, false) => "Courier-Bold",
            (Self::Courier, false, true) => "Courier-Oblique",
            (Self::Courier, true, true) => "Courier-BoldOblique",
        }
    }
}

/// One heading level extracted from the reference.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HeadingStyle {
    /// 1-based level (1 is the largest).
    pub level: u8,
    pub size: f64,
    pub bold: bool,
    pub space_before: f64,
    pub space_after: f64,
}

/// Serializable formatting profile, versioned for forward compatibility.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StyleProfile {
    pub version: u32,
    /// Page size in points [width, height].
    pub page: [f64; 2],
    /// Margins in points [left, top, right, bottom].
    pub margins: [f64; 4],
    pub base_font: BaseFont,
    pub body_size: f64,
    pub body_leading: f64,
    /// Body text colour (RGB 0–1), when the reference stated one.
    #[serde(default)]
    pub body_color: Option<[f64; 3]>,
    /// Heading text colour (RGB 0–1), when the reference stated one.
    #[serde(default)]
    pub heading_color: Option<[f64; 3]>,
    pub headings: Vec<HeadingStyle>,
    pub bullet_indent: f64,
    pub page_numbers: bool,
}

impl Default for StyleProfile {
    fn default() -> Self {
        Self {
            version: 1,
            page: [612.0, 792.0],
            margins: [72.0, 72.0, 72.0, 72.0],
            base_font: BaseFont::Helvetica,
            body_size: 11.0,
            body_leading: 13.75,
            body_color: None,
            heading_color: None,
            headings: vec![
                HeadingStyle { level: 1, size: 18.0, bold: true, space_before: 12.0, space_after: 6.0 },
                HeadingStyle { level: 2, size: 14.0, bold: true, space_before: 10.0, space_after: 4.0 },
            ],
            bullet_indent: 18.0,
            page_numbers: false,
        }
    }
}

/// One styled paragraph of the output document.
#[derive(Clone, Debug, PartialEq)]
pub enum StyledItem {
    Body(String),
    Heading { level: u8, text: String },
    Bullet(String),
}

const MAX_BLOCKS: usize = 20_000;
const MAX_ITEMS: usize = 50_000;
const MAX_PAGES: usize = 10_000;
const MAX_TEXT_LEN: usize = 4_000_000;

fn clamp_size(size: f64) -> f64 {
    if size.is_finite() { size.clamp(6.0, 48.0) } else { 11.0 }
}

fn median(mut values: Vec<f64>) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    values.get(values.len() / 2).copied()
}

/// The median colour of `blocks`: black when none qualifies.
fn median_color(blocks: &[&BlockSample]) -> [f64; 3] {
    let channel = |i: usize| {
        let values: Vec<f64> = blocks.iter().map(|b| b.color[i]).filter(|v| v.is_finite()).map(|v| v.clamp(0.0, 1.0)).collect();
        median(values).unwrap_or(0.0)
    };
    [channel(0), channel(1), channel(2)]
}

/// Extract a profile from sampled reference lines and the reference page size.
///
/// `page` is the reference page size in points; `blocks` are text lines with
/// user-space rects on that page size. Only the first [`MAX_BLOCKS`] lines are read.
pub fn extract(page: [f64; 2], blocks: &[BlockSample]) -> Result<StyleProfile, StyleError> {
    if !(page[0].is_finite() && page[1].is_finite()) {
        return Err(StyleError::Invalid("reference page size must be finite".into()));
    }
    if !(3.0..=14_400.0).contains(&page[0]) || !(3.0..=14_400.0).contains(&page[1]) {
        return Err(StyleError::Invalid("reference page size is out of range".into()));
    }
    let mut profile = StyleProfile { page, ..StyleProfile::default() };
    let usable: Vec<&BlockSample> =
        blocks.iter().take(MAX_BLOCKS).filter(|b| !b.text.trim().is_empty() && b.size.is_finite() && b.size > 0.0).collect();
    if usable.is_empty() {
        return Ok(profile);
    }
    // Margins from the union of block rects (user space, y up).
    let mut left = f64::INFINITY;
    let mut right = f64::NEG_INFINITY;
    let mut top = f64::NEG_INFINITY;
    let mut bottom = f64::INFINITY;
    for b in &usable {
        let [x0, y0, x1, y1] = b.rect;
        if ![x0, y0, x1, y1].iter().all(|v| v.is_finite()) {
            continue;
        }
        left = left.min(x0.min(x1));
        right = right.max(x0.max(x1));
        top = top.max(y0.max(y1));
        bottom = bottom.min(y0.min(y1));
    }
    if left.is_finite() && right.is_finite() && left < right {
        profile.margins[0] = left.clamp(0.0, page[0] / 2.0 - 18.0).max(18.0);
        profile.margins[2] = (page[0] - right).clamp(0.0, page[0] / 2.0 - 18.0).max(18.0);
    }
    if top.is_finite() && bottom.is_finite() && bottom < top {
        profile.margins[1] = (page[1] - top).clamp(0.0, page[1] / 2.0 - 18.0).max(18.0);
        profile.margins[3] = bottom.clamp(0.0, page[1] / 2.0 - 18.0).max(18.0);
    }
    // Body size: median of long blocks (real paragraphs), else median of all.
    let long: Vec<&BlockSample> = usable.iter().filter(|b| b.text.chars().count() >= 40).copied().collect();
    let body_lines: Vec<&BlockSample> = if long.is_empty() { usable.clone() } else { long };
    let sizes: Vec<f64> = body_lines.iter().map(|b| b.size.clamp(4.0, 72.0)).collect();
    let body = median(sizes).unwrap_or(11.0);
    profile.body_size = clamp_size(body);
    profile.body_leading = (profile.body_size * 1.25 * 100.0).round() / 100.0;
    if !body_lines.is_empty() {
        profile.body_color = Some(median_color(&body_lines));
    }
    // Heading levels: distinct sizes clearly above body, largest first, at most three.
    // Each cluster keeps its members' colours for the heading colour.
    let mut clusters: Vec<(f64, Vec<[f64; 3]>, usize)> = Vec::new();
    for b in &usable {
        let size = (b.size.clamp(4.0, 72.0) * 2.0).round() / 2.0;
        if size < profile.body_size * 1.08 {
            continue;
        }
        let color = [b.color[0].clamp(0.0, 1.0), b.color[1].clamp(0.0, 1.0), b.color[2].clamp(0.0, 1.0)];
        if let Some(slot) = clusters.iter_mut().find(|(s, _, _)| (*s - size).abs() < 0.26) {
            if slot.1.len() < 512 {
                slot.1.push(color);
            }
            slot.2 += usize::from(b.bold);
        } else if clusters.len() < 8 {
            clusters.push((size, vec![color], usize::from(b.bold)));
        }
    }
    clusters.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    let mut heading_colors: Vec<[f64; 3]> = Vec::new();
    profile.headings = clusters
        .into_iter()
        .take(3)
        .enumerate()
        .map(|(i, (size, colors, bold_count))| {
            let level = (i as u8).saturating_add(1);
            let factor = match level {
                1 => 1.0,
                2 => 0.8,
                _ => 0.6,
            };
            heading_colors.extend(colors);
            HeadingStyle {
                level,
                size: clamp_size(size),
                bold: bold_count > 0,
                space_before: (profile.body_size * factor).round(),
                space_after: (profile.body_size * factor / 2.0).round(),
            }
        })
        .collect();
    if !heading_colors.is_empty() {
        let channel = |i: usize| {
            let values: Vec<f64> = heading_colors.iter().map(|c| c[i]).collect();
            median(values).unwrap_or(0.0)
        };
        profile.heading_color = Some([channel(0), channel(1), channel(2)]);
    }
    // Base font from the most common reference family name.
    let (mut helvetica, mut times, mut courier) = (0usize, 0usize, 0usize);
    for b in &usable {
        let name = b.base_font.to_ascii_lowercase();
        if name.contains("times") {
            times += 1;
        } else if name.contains("courier") {
            courier += 1;
        } else {
            helvetica += 1;
        }
    }
    profile.base_font = if times > helvetica && times >= courier {
        BaseFont::Times
    } else if courier > helvetica && courier > times {
        BaseFont::Courier
    } else {
        BaseFont::Helvetica
    };
    // Page numbers: a block near the bottom whose digits dominate.
    profile.page_numbers = usable.iter().any(|b| {
        let t = b.text.trim();
        if t.is_empty() || t.chars().count() > 12 {
            return false;
        }
        let digits = t.chars().filter(|c| c.is_ascii_digit()).count();
        let near_bottom = b.rect[1].is_finite() && b.rect[1] <= profile.margins[3] + 36.0;
        near_bottom && digits * 2 >= t.chars().filter(|c| !c.is_whitespace()).count().max(1)
    });
    Ok(profile)
}

/// Apply a follow-up patch to a profile (what `ai_chat` proposes as fenced
/// JSON, or what a user picks in the UI): `body_size`, `headings`
/// (`[{level, size, bold}]`), `page_numbers`, `bullet_indent`, `base_font`
/// (`helvetica`/`times`/`courier`), `body_color` and `heading_color`
/// (`[r, g, b]` in 0–1, or null to keep the source colour) and `margins`
/// (`[left, top, right, bottom]`).
/// Unknown keys are refused so typos fail loudly; everything is range-checked
/// and the result keeps `version`. Geometry the patch doesn't mention
/// (page size) is never invented.
pub fn apply_patch(profile: &StyleProfile, patch: &serde_json::Value) -> Result<StyleProfile, StyleError> {
    if profile.version != 1 {
        return Err(StyleError::Invalid("profile.version must be 1".into()));
    }
    let map = patch.as_object().ok_or_else(|| StyleError::Invalid("the patch must be a JSON object".into()))?;
    for key in map.keys() {
        if !["body_size", "headings", "page_numbers", "bullet_indent", "base_font", "body_color", "heading_color", "margins", "summary"]
            .contains(&key.as_str())
        {
            return Err(StyleError::Invalid(format!("unknown patch key {key:?}")));
        }
    }
    let mut out = profile.clone();
    if let Some(size) = map.get("body_size") {
        let size = size.as_f64().filter(|s| s.is_finite()).ok_or_else(|| StyleError::Invalid("body_size must be a number".into()))?;
        if !(6.0..=24.0).contains(&size) {
            return Err(StyleError::Invalid("body_size must be between 6 and 24 pt".into()));
        }
        out.body_size = (size * 100.0).round() / 100.0;
        out.body_leading = ((size * 1.25) * 100.0).round() / 100.0;
    }
    if let Some(headings) = map.get("headings") {
        let list = headings.as_array().ok_or_else(|| StyleError::Invalid("headings must be an array".into()))?;
        if list.len() > 3 {
            return Err(StyleError::Invalid("at most 3 heading levels".into()));
        }
        let mut parsed = Vec::with_capacity(list.len());
        for h in list {
            let o = h.as_object().ok_or_else(|| StyleError::Invalid("each heading must be an object".into()))?;
            let level = o
                .get("level")
                .and_then(serde_json::Value::as_u64)
                .filter(|l| (1..=3).contains(l))
                .ok_or_else(|| StyleError::Invalid("each heading needs level 1, 2 or 3".into()))?;
            let size = o
                .get("size")
                .and_then(serde_json::Value::as_f64)
                .filter(|s| s.is_finite())
                .ok_or_else(|| StyleError::Invalid("each heading needs a size".into()))?;
            if !(6.0..=48.0).contains(&size) {
                return Err(StyleError::Invalid("heading sizes must be between 6 and 48 pt".into()));
            }
            let bold = o.get("bold").and_then(serde_json::Value::as_bool).unwrap_or(true);
            let factor = match level {
                1 => 1.0,
                2 => 0.8,
                _ => 0.6,
            };
            parsed.push(HeadingStyle {
                level: level as u8,
                size: (size * 100.0).round() / 100.0,
                bold,
                space_before: (out.body_size * factor).round(),
                space_after: (out.body_size * factor / 2.0).round(),
            });
        }
        parsed.sort_by_key(|h| h.level);
        out.headings = parsed;
    }
    if let Some(numbers) = map.get("page_numbers") {
        out.page_numbers = numbers.as_bool().ok_or_else(|| StyleError::Invalid("page_numbers must be true or false".into()))?;
    }
    if let Some(indent) = map.get("bullet_indent") {
        let indent = indent.as_f64().filter(|v| v.is_finite()).ok_or_else(|| StyleError::Invalid("bullet_indent must be a number".into()))?;
        if !(0.0..=144.0).contains(&indent) {
            return Err(StyleError::Invalid("bullet_indent must be between 0 and 144 pt".into()));
        }
        out.bullet_indent = indent;
    }
    if let Some(font) = map.get("base_font") {
        out.base_font = match font.as_str() {
            Some("helvetica") => BaseFont::Helvetica,
            Some("times") => BaseFont::Times,
            Some("courier") => BaseFont::Courier,
            _ => return Err(StyleError::Invalid("base_font must be helvetica, times or courier".into())),
        };
    }
    for key in ["body_color", "heading_color"] {
        let Some(value) = map.get(key) else { continue };
        if value.is_null() {
            if key == "body_color" {
                out.body_color = None;
            } else {
                out.heading_color = None;
            }
            continue;
        }
        let list = value.as_array().filter(|v| v.len() == 3).ok_or_else(|| StyleError::Invalid(format!("{key} must be [r, g, b] or null")))?;
        let mut color = [0.0; 3];
        for (i, v) in list.iter().enumerate() {
            color[i] = v.as_f64().filter(|n| n.is_finite()).ok_or_else(|| StyleError::Invalid(format!("{key} must be [r, g, b] or null")))?;
            if !(0.0..=1.0).contains(&color[i]) {
                return Err(StyleError::Invalid(format!("{key} components must be between 0 and 1")));
            }
        }
        if key == "body_color" {
            out.body_color = Some(color);
        } else {
            out.heading_color = Some(color);
        }
    }
    if let Some(margins) = map.get("margins") {
        let list =
            margins.as_array().filter(|m| m.len() == 4).ok_or_else(|| StyleError::Invalid("margins must be [left, top, right, bottom]".into()))?;
        let mut parsed = [0.0; 4];
        for (i, m) in list.iter().enumerate() {
            parsed[i] = m.as_f64().filter(|v| v.is_finite()).ok_or_else(|| StyleError::Invalid("each margin must be a number".into()))?;
            if !(18.0..=360.0).contains(&parsed[i]) {
                return Err(StyleError::Invalid("margins must be between 18 and 360 pt".into()));
            }
        }
        if parsed[0] + parsed[2] >= out.page[0] || parsed[1] + parsed[3] >= out.page[1] {
            return Err(StyleError::Invalid("margins leave no room for text".into()));
        }
        out.margins = parsed;
    }
    Ok(out)
}

/// Parse plain raw text into styled items.
///
/// `# `, `## `, `### ` start headings; `- `, `* `, `• ` start bullets;
/// `1. ` starts a bullet; anything else is body. Blank lines are skipped.
/// Lines that are short and ALL-CAPS become level-2 headings.
pub fn parse_plain(text: &str) -> Result<Vec<StyledItem>, StyleError> {
    if text.len() > MAX_TEXT_LEN {
        return Err(StyleError::Invalid("raw text is too large".into()));
    }
    let mut items = Vec::new();
    for line in text.replace("\r\n", "\n").split('\n') {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if items.len() >= MAX_ITEMS {
            return Err(StyleError::Invalid("too many paragraphs".into()));
        }
        let item = if let Some(rest) = trimmed.strip_prefix("### ") {
            StyledItem::Heading { level: 3, text: rest.trim().to_string() }
        } else if let Some(rest) = trimmed.strip_prefix("## ") {
            StyledItem::Heading { level: 2, text: rest.trim().to_string() }
        } else if let Some(rest) = trimmed.strip_prefix("# ") {
            StyledItem::Heading { level: 1, text: rest.trim().to_string() }
        } else if let Some(rest) = trimmed.strip_prefix("- ").or_else(|| trimmed.strip_prefix("* ")).or_else(|| trimmed.strip_prefix("• ")) {
            StyledItem::Bullet(rest.trim().to_string())
        } else if is_numbered(trimmed) {
            StyledItem::Bullet(trimmed.to_string())
        } else if is_shouted_heading(trimmed) {
            StyledItem::Heading { level: 2, text: trimmed.to_string() }
        } else {
            StyledItem::Body(trimmed.to_string())
        };
        items.push(item);
    }
    Ok(items)
}

fn is_numbered(line: &str) -> bool {
    let mut digits = 0usize;
    for c in line.chars() {
        if c.is_ascii_digit() {
            digits += 1;
            if digits > 4 {
                return false;
            }
        } else {
            return digits > 0 && (c == '.' || c == ')') && line.chars().count() > digits + 2;
        }
    }
    false
}

fn is_shouted_heading(line: &str) -> bool {
    let letters: Vec<char> = line.chars().filter(|c| c.is_alphabetic()).collect();
    !letters.is_empty() && letters.len() <= 60 && letters.iter().all(|c| !c.is_lowercase()) && line.chars().count() <= 80
}

fn heading_style(profile: &StyleProfile, level: u8) -> (f64, bool) {
    profile
        .headings
        .iter()
        .find(|h| h.level == level)
        .map(|h| (h.size, h.bold))
        .or_else(|| profile.headings.first().map(|h| (h.size, h.bold)))
        .unwrap_or((profile.body_size + 4.0, true))
}

fn add_page(doc: &mut Document, w: f64, h: f64, resources: Dict, content: Option<Vec<u8>>) -> Result<ObjRef, StyleError> {
    let pages = doc
        .root()
        .and_then(|r| doc.get(r).as_dict().and_then(|d| d.reference(b"Pages")))
        .ok_or_else(|| StyleError::Invalid("the new document has no page tree".into()))?;
    let mut page = Dict::new();
    page.set(b"Type".to_vec(), Object::name("Page"));
    page.set(b"Parent".to_vec(), Object::Ref(pages));
    page.set(b"MediaBox".to_vec(), Object::Array(vec![0.into(), 0.into(), Object::Real(w), Object::Real(h)]));
    page.set(b"Resources".to_vec(), Object::Dict(resources));
    if let Some(c) = content {
        let s = doc.add(Object::Stream(Stream::flate(Dict::new(), &c)));
        page.set(b"Contents".to_vec(), Object::Ref(s));
    }
    let r = doc.add(page);
    let _ = doc.update_dict(pages, |d| {
        let mut kids = d.get(b"Kids").and_then(|k| k.as_array().cloned()).unwrap_or_default();
        kids.push(Object::Ref(r));
        d.set(b"Count".to_vec(), Object::Int(kids.len() as i64));
        d.set(b"Kids".to_vec(), Object::Array(kids));
    });
    Ok(r)
}

/// Generate a styled PDF from items and a profile.
pub fn render(title: &str, items: &[StyledItem], profile: &StyleProfile) -> Result<Document, StyleError> {
    if items.len() > MAX_ITEMS {
        return Err(StyleError::Invalid("too many paragraphs".into()));
    }
    if !(profile.page[0].is_finite() && profile.page[1].is_finite()) {
        return Err(StyleError::Invalid("profile page size must be finite".into()));
    }
    let (w, h) = (profile.page[0].clamp(3.0, 14_400.0), profile.page[1].clamp(3.0, 14_400.0));
    let [ml, mt, mr, mb] = profile.margins;
    let width = (w - ml - mr).max(36.0);
    let body = clamp_size(profile.body_size);
    let leading = if profile.body_leading.is_finite() { profile.body_leading.clamp(6.0, 96.0) } else { body * 1.25 };

    let mut doc = Document::new_empty();
    // One Type 1 font object per used (family, bold, italic) triple, shared by all pages.
    let mut fonts: Vec<((BaseFont, bool, bool), ObjRef)> = Vec::new();
    let mut font_ref = |doc: &mut Document, family: BaseFont, bold: bool, italic: bool| -> ObjRef {
        if let Some((_, r)) = fonts.iter().find(|((f, b, i), _)| *f == family && *b == bold && *i == italic) {
            return *r;
        }
        let mut d = Dict::new();
        d.set(b"Type".to_vec(), Object::name("Font"));
        d.set(b"Subtype".to_vec(), Object::name("Type1"));
        d.set(b"BaseFont".to_vec(), Object::name(family.base_name(bold, italic)));
        d.set(b"Encoding".to_vec(), Object::name("WinAnsiEncoding"));
        let r = doc.add(d);
        fonts.push(((family, bold, italic), r));
        r
    };

    // Flatten items into drawable rows: (font key, size, indent, colour, text).
    let mut rows: Vec<StyledRow> = Vec::new();
    for item in items {
        match item {
            StyledItem::Body(text) => {
                for row in pdfcraft_fonts::wrap(text, body, width) {
                    if rows.len() >= MAX_ITEMS * 4 {
                        return Err(StyleError::Invalid("the styled document is too long".into()));
                    }
                    rows.push(((profile.base_font, false, false), body, 0.0, profile.body_color, row));
                }
            }
            StyledItem::Heading { level, text } => {
                let (size, bold) = heading_style(profile, *level);
                for row in pdfcraft_fonts::wrap(text, size, width) {
                    if rows.len() >= MAX_ITEMS * 4 {
                        return Err(StyleError::Invalid("the styled document is too long".into()));
                    }
                    rows.push(((profile.base_font, bold, false), size, 0.0, profile.heading_color, row));
                }
            }
            StyledItem::Bullet(text) => {
                let bullet = format!("•  {text}");
                for (i, row) in pdfcraft_fonts::wrap(&bullet, body, width - profile.bullet_indent).iter().enumerate() {
                    if rows.len() >= MAX_ITEMS * 4 {
                        return Err(StyleError::Invalid("the styled document is too long".into()));
                    }
                    let indent = if i == 0 { 0.0 } else { profile.bullet_indent };
                    rows.push(((profile.base_font, false, false), body, indent, profile.body_color, row.clone()));
                }
            }
        }
    }
    if rows.is_empty() {
        return Err(StyleError::Invalid("nothing to write".into()));
    }
    // Reject text the WinAnsi Type 1 fonts can't draw, naming the first offender.
    for (_, _, _, _, text) in &rows {
        if let Some(c) = pdfcraft_fonts::first_undrawable(text) {
            return Err(StyleError::Invalid(pdfcraft_fonts::undrawable_message("the styled text", c)));
        }
    }

    let per_page = (((h - mt - mb) / leading).floor() as usize).clamp(1, 500);
    let mut pages: Vec<Vec<StyledRow>> = vec![Vec::new()];
    for row in rows {
        if pages.len() >= MAX_PAGES {
            return Err(StyleError::Invalid("the styled document needs too many pages".into()));
        }
        let full = pages.last().is_some_and(|p| p.len() >= per_page);
        if full {
            pages.push(Vec::new());
        }
        if let Some(p) = pages.last_mut() {
            p.push(row);
        }
    }
    let total = pages.len();
    for (n, lines) in pages.iter().enumerate() {
        let mut keys: Vec<((BaseFont, bool, bool), String)> = Vec::new();
        for (key, _, _, _, _) in lines {
            if !keys.iter().any(|(k, _)| k == key) && keys.len() < 12 {
                let name = format!("F{}", keys.len() + 1);
                keys.push((*key, name));
            }
        }
        let mut refs = Dict::new();
        let mut dict = Dict::new();
        for ((family, bold, italic), name) in &keys {
            let r = font_ref(&mut doc, *family, *bold, *italic);
            dict.set(name.as_bytes().to_vec(), Object::Ref(r));
        }
        refs.set(b"Font".to_vec(), Object::Dict(dict));
        let key_of = |key: &(BaseFont, bool, bool)| -> String {
            keys.iter().find(|(k, _)| k == key).map(|(_, n)| n.clone()).unwrap_or_else(|| "F1".to_string())
        };
        let mut content = Vec::new();
        let mut y = h - mt - body;
        for (key, size, indent, color, text) in lines {
            let name = key_of(key);
            let x = ml + indent;
            let mut line = format!("BT /{name} {size:.2} Tf 1 0 0 1 {x:.2} {y:.2} Tm ");
            if let Some([r, g, b]) = color.filter(|c| c.iter().all(|v| v.is_finite())) {
                line.push_str(&format!("{} {} {} rg ", num(r), num(g), num(b)));
            }
            content.extend_from_slice(line.as_bytes());
            content.extend_from_slice(&pdfcraft_fonts::literal(&pdfcraft_fonts::win_ansi(text)));
            content.extend_from_slice(b" Tj ET\n");
            y -= leading;
        }
        if profile.page_numbers {
            let label = format!("{} / {}", n + 1, total);
            let x = (w / 2.0 - 20.0).max(ml);
            let y = (mb / 2.0).max(18.0);
            let line = format!("BT /F1 {:.2} Tf 1 0 0 1 {x:.2} {y:.2} Tm ", (body - 1.0).max(7.0));
            content.extend_from_slice(line.as_bytes());
            content.extend_from_slice(&pdfcraft_fonts::literal(&pdfcraft_fonts::win_ansi(&label)));
            content.extend_from_slice(b" Tj ET\n");
        }
        add_page(&mut doc, w, h, refs, Some(content))?;
    }
    let mut info = Dict::new();
    info.set(b"Title".to_vec(), PdfString::text(title));
    info.set(b"Producer".to_vec(), PdfString::text("PdfCraft"));
    let r = doc.add(info);
    doc.trailer_mut().set(b"Info".to_vec(), Object::Ref(r));
    Ok(doc)
}

/// One drawable row: (font key, size, extra indent, colour, text).
type StyledRow = ((BaseFont, bool, bool), f64, f64, Option<[f64; 3]>, String);

/// A colour component for a content stream: clamped, three decimals.
fn num(v: f64) -> String {
    format!("{:.3}", v.clamp(0.0, 1.0))
}

#[cfg(test)]
mod tests;
