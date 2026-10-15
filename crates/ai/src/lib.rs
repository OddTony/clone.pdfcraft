//! pdfcraft-ai — opt-in AI provider interface (M13 `misc.ai-provider`).
//!
//! Bring your own key: nothing runs without an explicit [`ProviderConfig`]
//! (pasted from OpenRouter or any OpenAI-compatible endpoint). No key is
//! stored here, no request fires without a call, and failures are plain
//! errors an agent can act on. Never log the key ([`ProviderConfig`] redacts it).

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

#[cfg(not(target_arch = "wasm32"))]
use std::time::Duration;

#[derive(Debug, thiserror::Error, PartialEq)]
pub enum AiError {
    #[error("{0}")]
    Invalid(String),
    #[error("the provider could not be reached: {0}")]
    Transport(String),
    #[error("the provider refused the request: {0}")]
    Refused(String),
}

/// OpenRouter-compatible endpoint configuration. The key travels only in the
/// `Authorization` header of the one request it is passed to.
pub struct ProviderConfig {
    /// e.g. `https://openrouter.ai/api/v1` (trailing slash tolerated).
    pub base_url: String,
    /// `sk-or-…` bearer key, pasted by the user.
    pub api_key: String,
    /// e.g. `anthropic/claude-sonnet-4`.
    pub model: String,
}

impl std::fmt::Debug for ProviderConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProviderConfig").field("base_url", &self.base_url).field("api_key", &"…").field("model", &self.model).finish()
    }
}

impl ProviderConfig {
    fn validate(&self) -> Result<String, AiError> {
        let base = self.base_url.trim().trim_end_matches('/');
        if !(base.starts_with("https://") && base.len() <= 256 && base.len() > "https://".len()) {
            return Err(AiError::Invalid("base_url must be an https:// URL".into()));
        }
        if self.api_key.trim().is_empty() || self.api_key.len() > 4096 {
            return Err(AiError::Invalid("api_key is missing".into()));
        }
        if self.model.trim().is_empty() || self.model.len() > 256 {
            return Err(AiError::Invalid("model is missing".into()));
        }
        Ok(base.to_string())
    }
}

/// One chat turn.
#[derive(Clone, Debug)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

impl ChatMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self { role: "system".into(), content: content.into() }
    }

    pub fn user(content: impl Into<String>) -> Self {
        Self { role: "user".into(), content: content.into() }
    }
}

/// System prompt for follow-up styling prompts: the model answers with either
/// plain guidance or a fenced `json` profile patch (`{body_size?, headings?,
/// page_numbers?}` plus `summary`). Geometry still comes from the deterministic
/// [`pdfcraft_style::StyleProfile`](https://docs.rs/pdfcraft-style); the model
/// only edits it on request.
pub const STYLE_PATCH_SYSTEM: &str = "You help restyle a PDF from a deterministic formatting profile (page size, margins, body size, heading levels, bullets, page numbers). Answer briefly. When the user asks for a formatting change, append a fenced json block with the profile patch keys you changed (body_size as a number 6-24, headings as [{level, size, bold}], page_numbers as a boolean) and a one-line summary field. Never invent page sizes or margins.";

const MAX_PROMPT_CHARS: usize = 100_000;
const MAX_BODY: usize = 1 << 20;

/// One OpenAI-compatible `/chat/completions` round trip. Capped and synchronous.
///
/// Validation runs on every target (tests included); the HTTPS transport is
/// native-only — on wasm32 there is no TCP, so a clear error is returned and
/// the crate still compiles for the web build.
pub fn chat(config: &ProviderConfig, messages: &[ChatMessage]) -> Result<String, AiError> {
    let (base, body) = request_parts(config, messages)?;
    #[cfg(target_arch = "wasm32")]
    {
        let _ = (base, body);
        return Err(AiError::Transport("AI chat isn't available in the web build; use the desktop app or CLI".into()));
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        transport(&base, &body, &config.api_key)
    }
}

fn request_parts(config: &ProviderConfig, messages: &[ChatMessage]) -> Result<(String, serde_json::Value), AiError> {
    let base = config.validate()?;
    if messages.is_empty() || messages.len() > 64 {
        return Err(AiError::Invalid("give 1 to 64 messages".into()));
    }
    let mut total = 0usize;
    for m in messages {
        if m.role.len() > 32 || m.content.len() > MAX_PROMPT_CHARS {
            return Err(AiError::Invalid("a message is too large".into()));
        }
        total = total.saturating_add(m.content.len());
        if total > MAX_PROMPT_CHARS {
            return Err(AiError::Invalid("the conversation is too large".into()));
        }
        if !["system", "user", "assistant"].contains(&m.role.as_str()) {
            return Err(AiError::Invalid("message role must be system, user or assistant".into()));
        }
    }
    let body = serde_json::json!({
        "model": config.model,
        "messages": messages.iter().map(|m| serde_json::json!({"role": m.role, "content": m.content})).collect::<Vec<_>>(),
    });
    Ok((base, body))
}

#[cfg(not(target_arch = "wasm32"))]
fn transport(base: &str, body: &serde_json::Value, api_key: &str) -> Result<String, AiError> {
    let url = format!("{base}/chat/completions");
    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(60)))
        .tls_config(ureq::tls::TlsConfig::builder().root_certs(os_roots()?).build())
        .build()
        .new_agent();
    let mut response = agent
        .post(&url)
        .header("Accept", "application/json")
        .header("Content-Type", "application/json")
        .header("Authorization", format!("Bearer {api_key}"))
        .header("HTTP-Referer", "https://getartcraft.com/apps/pdfcraft")
        .header("X-Title", "PdfCraft Format Studio")
        .config()
        .http_status_as_error(false)
        .build()
        .send_json(body)
        .map_err(|e| AiError::Transport(format!("couldn't reach the provider ({e})")))?;
    if !(200..300).contains(&response.status().as_u16()) {
        let detail = response.body_mut().with_config().limit(4096).read_to_string().unwrap_or_default();
        let short = detail.chars().take(300).collect::<String>();
        return Err(AiError::Refused(format!("http {}: {short}", response.status().as_u16())));
    }
    let text = response
        .body_mut()
        .with_config()
        .limit(MAX_BODY as u64)
        .read_to_string()
        .map_err(|e| AiError::Transport(format!("unreadable answer ({e})")))?;
    parse_answer(&text)
}

#[cfg(not(target_arch = "wasm32"))]
fn os_roots() -> Result<ureq::tls::RootCerts, AiError> {
    let found = rustls_native_certs::load_native_certs();
    let certs: Vec<ureq::tls::Certificate<'static>> = found.certs.iter().map(|c| ureq::tls::Certificate::from_der(c.as_ref()).to_owned()).collect();
    if certs.is_empty() {
        return Err(AiError::Transport("no trusted certificates found on this system".into()));
    }
    Ok(ureq::tls::RootCerts::new_with_certs(&certs))
}

/// The assistant text of an OpenAI-compatible chat answer. Fails closed on
/// anything unexpected, with a bounded message.
pub fn parse_answer(body: &str) -> Result<String, AiError> {
    let v: serde_json::Value = serde_json::from_str(body).map_err(|_| AiError::Transport("unreadable answer".into()))?;
    let text = v["choices"][0]["message"]["content"].as_str().ok_or_else(|| AiError::Transport("unreadable answer".into()))?;
    if text.len() > MAX_BODY {
        return Err(AiError::Transport("the answer is too large".into()));
    }
    Ok(text.to_string())
}

#[cfg(test)]
mod tests;
