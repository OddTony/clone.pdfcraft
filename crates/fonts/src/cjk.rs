//! Approved bundled CJK fonts for portable page content and form appearances.
use std::collections::BTreeMap;

use pdfcraft_cos::{Dict, Document};
use skrifa::{
    FontRef, MetadataProvider,
    instance::{LocationRef, Size},
};

use crate::{CraftFont, embedding, pdf::Metrics};

/// Japanese candidates retain their established style preference, followed by Chinese faces.
pub fn document_cjk_fonts_for_style(serif: bool, bold: bool) -> Vec<&'static CraftFont> {
    let mut faces = crate::document_japanese_fonts_for_style(serif, bold);
    faces.extend(crate::CRAFT_FONTS.iter().filter(|f| !f.covers("Jpan") && (f.covers("Hans") || f.covers("Hant"))));
    faces
}

pub fn contains_cjk(text: &str) -> bool {
    text.chars().any(|c| matches!(c, '\u{2e80}'..='\u{9fff}' | '\u{f900}'..='\u{faff}' | '\u{ff01}'..='\u{ffef}' | '\u{20000}'..='\u{3fffd}'))
}

fn normalized(c: char) -> char {
    if matches!(c, '\r' | '\n' | '\t') { ' ' } else { c }
}

/// Layout validated before allocating any PDF objects. Only manifest fonts are candidates.
pub struct PreparedCjkText {
    faces: Vec<&'static CraftFont>,
    selection: BTreeMap<char, usize>,
    widths: BTreeMap<char, f64>,
}

impl PreparedCjkText {
    pub fn new(text: &str, serif: bool, bold: bool) -> Result<Self, String> {
        if text.len() > 1 << 20 {
            return Err("CJK text exceeds the 1 MiB layout limit; use smaller text boxes".into());
        }
        let mut candidates = document_cjk_fonts_for_style(serif, bold);
        let normalized_text: String = text.chars().map(normalized).chain(std::iter::once(' ')).collect();
        if let Some(index) = candidates.iter().position(|f| embedding::covers(f.bytes, &normalized_text)) {
            let face = candidates.remove(index);
            candidates.insert(0, face);
        }
        let parsed: Vec<_> = candidates.iter().filter_map(|f| FontRef::new(f.bytes).ok().map(|font| (*f, font))).collect();
        let mut out = Self { faces: Vec::new(), selection: BTreeMap::new(), widths: BTreeMap::new() };
        for c in normalized_text.chars() {
            if out.selection.contains_key(&c) {
                continue;
            }
            let (face, font, glyph) = parsed
                .iter()
                .find_map(|(face, font)| font.charmap().map(c).filter(|g| g.to_u32() != 0).map(|g| (*face, font, g)))
                .ok_or_else(|| {
                    format!("no bundled Japanese/Chinese font can draw '{c}' (U+{:04X}); include an approved face in CRAFT_FONTS_DIR", u32::from(c))
                })?;
            let index = match out.faces.iter().position(|f| std::ptr::eq(*f, face)) {
                Some(index) => index,
                None => {
                    embedding::validate(face.bytes)?;
                    out.faces.push(face);
                    out.faces.len().saturating_sub(1)
                }
            };
            let metrics = font.metrics(Size::unscaled(), LocationRef::default());
            let advance = font.glyph_metrics(Size::unscaled(), LocationRef::default()).advance_width(glyph).ok_or("font has no glyph advance")?;
            let width = f64::from(advance) / f64::from(metrics.units_per_em);
            if !width.is_finite() || !(0.0..=16.0).contains(&width) {
                return Err("CJK font has an invalid glyph advance".into());
            }
            out.selection.insert(c, index);
            out.widths.insert(c, width);
        }
        Ok(out)
    }

    pub fn width(&self, text: &str, size: f64) -> f64 {
        measure(&self.widths, text, size)
    }

    /// Reuse font dictionaries already registered in this resource scope, and full programs
    /// already embedded elsewhere. Imported/source resources are never overwritten.
    pub fn install(&self, doc: &mut Document, fonts: &mut Dict) -> Result<EmbeddedCjkText, String> {
        let mut faces = Vec::new();
        let mut resources = Dict::new();
        for face in &self.faces {
            let existing = fonts.iter().find_map(|(name, entry)| {
                let fd = doc.dict(entry)?;
                let same = fd.name(b"Subtype") == Some(b"Type0") && crate::pdf::embedded_sfnt(doc, &fd).is_some_and(|bytes| bytes == face.bytes);
                if !same {
                    return None;
                }
                let metrics = Metrics::from_dict(doc, &fd);
                let needed: String = self
                    .selection
                    .iter()
                    .filter(|(_, index)| self.faces.get(**index).is_some_and(|selected| std::ptr::eq(*selected, *face)))
                    .map(|(c, _)| *c)
                    .collect();
                metrics.encode(&needed)?;
                String::from_utf8(name.clone()).ok()
            });
            let name = match existing {
                Some(name) => name,
                None => embedding::embed(doc, fonts, face.bytes, &face.name())?,
            };
            let entry = fonts.get(name.as_bytes()).ok_or("embedded CJK font resource is missing")?;
            let fd = doc.dict(entry).ok_or("embedded CJK font dictionary is missing")?;
            resources.set(name.as_bytes().to_vec(), entry.clone());
            faces.push((name, Metrics::from_dict(doc, &fd)));
        }
        Ok(EmbeddedCjkText { resources, faces, selection: self.selection.clone(), widths: self.widths.clone() })
    }
}

pub struct EmbeddedCjkText {
    pub resources: Dict,
    faces: Vec<(String, Metrics)>,
    selection: BTreeMap<char, usize>,
    widths: BTreeMap<char, f64>,
}

impl EmbeddedCjkText {
    pub fn primary_name(&self) -> Option<&str> {
        self.faces.first().map(|(name, _)| name.as_str())
    }

    pub fn width(&self, text: &str, size: f64) -> f64 {
        measure(&self.widths, text, size)
    }

    /// Consecutive encoded runs; callers emit Tf between runs. No lossy '?' replacement.
    pub fn runs(&self, text: &str) -> Option<Vec<(&str, Vec<u8>)>> {
        let mut pieces: Vec<(usize, String)> = Vec::new();
        for c in text.chars().map(normalized) {
            let index = *self.selection.get(&c)?;
            match pieces.last_mut() {
                Some((previous, s)) if *previous == index => s.push(c),
                _ => pieces.push((index, c.to_string())),
            }
        }
        pieces
            .into_iter()
            .map(|(index, s)| {
                let (name, metrics) = self.faces.get(index)?;
                Some((name.as_str(), metrics.encode(&s)?))
            })
            .collect()
    }
}

fn measure(widths: &BTreeMap<char, f64>, text: &str, size: f64) -> f64 {
    text.chars().map(normalized).map(|c| widths.get(&c).copied().unwrap_or(0.0)).sum::<f64>() * size
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "portable release acceptance requires Japanese, Simplified and Traditional build inputs"]
    fn portable_cjk_unicode_runs_embed_once_and_keep_exact_text() {
        for script in ["Jpan", "Hans", "Hant"] {
            assert!(crate::CRAFT_FONTS.iter().any(|f| f.covers(script)), "required font group {script}");
        }
        let samples = [
            "〒100-0001東京都千代田区千代田1-1",
            "山田太郎",
            "简体中文：测试编辑汉龙马",
            "繁體中文：測試編輯漢龍馬",
            "ひらがな カタカナ ABC 123 丢龍馬",
        ];
        let mut doc = Document::new_empty();
        let mut resources = Dict::new();
        for text in samples {
            let prepared = PreparedCjkText::new(text, false, false).unwrap();
            let embedded = prepared.install(&mut doc, &mut resources).unwrap();
            assert!(embedded.width(text, 12.0).is_finite() && embedded.width(text, 12.0) > 0.0);
            let mut decoded = String::new();
            let mut measured = 0.0;
            for (name, bytes) in embedded.runs(text).unwrap() {
                let fd = doc.dict(resources.get(name.as_bytes()).unwrap()).unwrap();
                assert!(crate::pdf::embedded_sfnt(&doc, &fd).is_some());
                let metrics = Metrics::from_dict(&doc, &fd);
                decoded.push_str(&metrics.decode(&bytes));
                measured += metrics.codes(&bytes).iter().map(|(code, _)| metrics.width(*code) * 12.0).sum::<f64>();
            }
            assert_eq!(decoded, text);
            assert!((measured - prepared.width(text, 12.0)).abs() < 0.01);
            let objects = doc.object_numbers().len();
            prepared.install(&mut doc, &mut resources).unwrap();
            assert_eq!(doc.object_numbers().len(), objects, "repeat fields reuse their font resources");
        }
        let before = doc.object_numbers().len();
        assert!(PreparedCjkText::new("日本語\u{10ffff}", false, false).is_err());
        assert_eq!(doc.object_numbers().len(), before);
    }
}
