//! BYOK chat over an open document (Format Studio follow-ups).
//!
//! Opt-in: every call carries its own key; nothing is stored. Context is
//! bounded (profile JSON plus at most a few pages of text) so a large PDF is
//! never shipped whole to a third party.

use serde_json::{Value, json};

use crate::{Args, Automation, Result, failed};

/// Pages of document text folded into one chat turn, bounded.
const MAX_CONTEXT_CHARS: usize = 12_000;
/// Pages sampled for context (the first ones, where the formatting lives).
const MAX_CONTEXT_PAGES: usize = 3;

impl Automation {
    pub(crate) fn ai_chat(&mut self, a: &Args) -> Result<Value> {
        let prompt = a.str("prompt")?;
        if prompt.trim().is_empty() || prompt.len() > 8_000 {
            return Err(crate::ToolError::InvalidArgs("prompt must be 1 to 8000 characters".into()));
        }
        let base_url = a.opt_str("base_url")?.unwrap_or("https://openrouter.ai/api/v1");
        let config = pdfcraft_ai::ProviderConfig {
            base_url: base_url.to_string(),
            api_key: a.str("api_key")?.to_string(),
            model: a.opt_str("model")?.unwrap_or("anthropic/claude-sonnet-4").to_string(),
        };
        let mut context = String::new();
        if let Some(raw) = a.opt_int("doc")? {
            let id = u64::try_from(raw).map(pdfcraft_engine::DocId).map_err(|_| crate::ToolError::InvalidArgs("doc must be positive".into()))?;
            let (profile, pages) = {
                let doc = self.session.get(id).ok_or_else(|| failed(format!("no open document with id {raw} (see doc_list)")))?;
                doc.check_copying().map_err(failed)?;
                let profile = pdfcraft_engine::style::profile_of(doc, 6).map_err(failed)?;
                (profile, (0..doc.info.pages.len().min(MAX_CONTEXT_PAGES)).collect::<Vec<_>>())
            };
            let profile_json = serde_json::to_string(&profile).unwrap_or_default();
            context.push_str(&format!("Formatting profile:\n{profile_json}\n\n"));
            let texts = self.page_texts(id, &pages)?;
            for (p, t) in pages.iter().zip(texts) {
                let chunk = format!("--- page {} ---\n{}\n", p + 1, t.plain_text());
                if context.len() + chunk.len() > MAX_CONTEXT_CHARS {
                    let rest = MAX_CONTEXT_CHARS.saturating_sub(context.len());
                    let end = chunk.floor_char_boundary(rest.min(chunk.len()));
                    context.push_str(&chunk[..end]);
                    break;
                }
                context.push_str(&chunk);
            }
        }
        let user = if context.is_empty() { prompt.to_string() } else { format!("{prompt}\n\nDocument context:\n{context}") };
        let messages = vec![pdfcraft_ai::ChatMessage::system(pdfcraft_ai::STYLE_PATCH_SYSTEM), pdfcraft_ai::ChatMessage::user(user)];
        let answer = pdfcraft_ai::chat(&config, &messages).map_err(|e| match e {
            pdfcraft_ai::AiError::Invalid(m) => crate::ToolError::InvalidArgs(m),
            other => failed(other.to_string()),
        })?;
        Ok(json!({ "answer": answer }))
    }
}
