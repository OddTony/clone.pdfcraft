//! Format Studio tools: reference profiles and styled generation.

use serde_json::{Value, json};

use crate::{Args, Automation, Result, failed, summary};

impl Automation {
    pub(crate) fn style_profile(&self, a: &Args) -> Result<Value> {
        let doc = self.doc(a)?;
        let max_pages = a.opt_int("max_pages")?.unwrap_or(12).clamp(1, 20) as usize;
        let profile = pdfcraft_engine::style::profile_of(doc, max_pages).map_err(failed)?;
        Ok(json!({ "profile": profile, "document": summary(doc) }))
    }

    pub(crate) fn style_patch(&self, a: &Args) -> Result<Value> {
        let profile: pdfcraft_style::StyleProfile = serde_json::from_value(a.get("profile").cloned().unwrap_or(Value::Null))
            .map_err(|e| crate::ToolError::InvalidArgs(format!("profile: {e}")))?;
        let patch = a.get("patch").cloned().unwrap_or(Value::Null);
        let patched = pdfcraft_style::apply_patch(&profile, &patch).map_err(|e| crate::ToolError::InvalidArgs(e.to_string()))?;
        Ok(json!({ "profile": patched }))
    }

    pub(crate) fn style_apply(&mut self, a: &Args) -> Result<Value> {
        let profile: pdfcraft_style::StyleProfile = serde_json::from_value(a.get("profile").cloned().unwrap_or(Value::Null))
            .map_err(|e| crate::ToolError::InvalidArgs(format!("profile: {e}")))?;
        if profile.version != 1 {
            return Err(crate::ToolError::InvalidArgs("profile.version must be 1".into()));
        }
        let title = a.opt_str("title")?.unwrap_or("Styled document");
        if title.len() > 512 {
            return Err(crate::ToolError::InvalidArgs("title is too long".into()));
        }
        // Raw content: literal text, a .txt/.docx file, or the extracted text
        // of an open raw document. Exactly one source.
        let sources =
            [a.opt_str("text")?.is_some(), a.opt_int("raw_doc")?.is_some(), a.opt_str("raw_path")?.is_some()].into_iter().filter(|s| *s).count();
        if sources != 1 {
            return Err(crate::ToolError::InvalidArgs("give exactly one of text, raw_doc or raw_path".into()));
        }
        let raw_text;
        let text = if let Some(t) = a.opt_str("text")? {
            t
        } else if a.opt_int("raw_doc")?.is_some() {
            let raw = a.int("raw_doc")?;
            let id = u64::try_from(raw).map(pdfcraft_engine::DocId).map_err(|_| crate::ToolError::InvalidArgs("raw_doc must be positive".into()))?;
            let pages: Vec<usize> = {
                let doc = self.session.get(id).ok_or_else(|| failed(format!("no open document with id {raw} (see doc_list)")))?;
                doc.check_copying().map_err(failed)?;
                (0..doc.info.pages.len()).collect()
            };
            let texts = self.page_texts(id, &pages)?;
            let mut out = String::new();
            for (p, t) in pages.iter().zip(texts) {
                if out.len() > 2_000_000 {
                    break;
                }
                if !out.is_empty() {
                    out.push_str("\n\n");
                }
                out.push_str(&format!("Page {}:\n{}", p + 1, t.plain_text()));
            }
            raw_text = out;
            &raw_text
        } else {
            let path = self.resolve(a.str("raw_path")?, false)?;
            let bytes = std::fs::read(&path).map_err(|e| failed(format!("{}: {e}", path.display())))?;
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            let lower = name.to_ascii_lowercase();
            if lower.ends_with(".txt") || lower.ends_with(".text") {
                raw_text = pdfcraft_engine::decode_text(&bytes);
            } else if lower.ends_with(".docx") {
                raw_text = pdfcraft_engine::docx_text(&name, &bytes).map_err(failed)?;
            } else {
                return Err(crate::ToolError::InvalidArgs("raw_path must be a .txt or .docx file (open PDFs with doc_open and pass raw_doc)".into()));
            }
            &raw_text
        };
        let bytes = self.session.create_from_style(title, text, &profile).map_err(failed)?;
        match a.opt_str("out")? {
            Some(path) => {
                let target = self.write_output(path, &bytes)?;
                Ok(json!({ "path": target.to_string_lossy(), "bytes": bytes.len() }))
            }
            None => {
                let name = format!("{title}.pdf");
                let id = self.session.open(name, None, bytes.clone(), None).map_err(failed)?;
                let doc = self.session.get(id).ok_or_else(|| failed("the document vanished"))?;
                Ok(summary(doc))
            }
        }
    }

    pub(crate) fn style_restyle(&mut self, a: &Args) -> Result<Value> {
        let profile: pdfcraft_style::StyleProfile = serde_json::from_value(a.get("profile").cloned().unwrap_or(Value::Null))
            .map_err(|e| crate::ToolError::InvalidArgs(format!("profile: {e}")))?;
        let id = self.doc(a)?.id;
        let report = self.session.restyle_text(id, &profile).map_err(failed)?;
        let mut out = summary(self.doc(a)?);
        out["paragraphs"] = json!(report.paragraphs);
        out["headings"] = json!(report.headings);
        out["skipped"] = json!(report.skipped);
        Ok(out)
    }
}
