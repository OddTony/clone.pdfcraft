//! UniFFI bridge between [`pdfcraft_automation`] and the iOS companion app.
//!
//! The mobile app never calls `pdfcraft-engine`/`pdfcraft-render` directly: every operation
//! goes through [`MobileSession::call`], which forwards a tool name and a JSON arguments object
//! to [`pdfcraft_automation::Automation::call`] and returns the same tool result, translated
//! into UniFFI-friendly types. See `crates/automation/README.md` for the tool catalogue
//! (`doc_open`, `page_render`, `doc_save`, ...) and their argument/return shapes.

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

use std::sync::Mutex;

use pdfcraft_automation::{Automation, Content, ToolError};

uniffi::setup_scaffolding!();

/// A single piece of a tool result, mirroring [`pdfcraft_automation::Content`].
///
/// `Json` carries the result as a serialized JSON string (Swift decodes it with `JSONSerialization`
/// or `Codable`); `Png` carries raw, already-decoded image bytes plus their pixel dimensions.
#[derive(uniffi::Enum)]
pub enum MobileContent {
    Json { json: String },
    Png { data: Vec<u8>, width: u32, height: u32 },
}

impl From<Content> for MobileContent {
    fn from(content: Content) -> Self {
        match content {
            Content::Json(value) => MobileContent::Json { json: value.to_string() },
            Content::Png { data, width, height } => MobileContent::Png { data, width, height },
        }
    }
}

/// Errors surfaced to Swift. Each variant carries a human-readable `message` so the app can show
/// it directly or log it, without needing to pattern-match on the automation's internal error enum.
#[derive(Debug, Clone, thiserror::Error, uniffi::Error)]
pub enum MobileError {
    #[error("unknown tool: {name}")]
    UnknownTool { name: String },
    #[error("invalid arguments: {message}")]
    InvalidArgs { message: String },
    #[error("tool call failed: {message}")]
    Failed { message: String },
    #[error("could not open root directory: {message}")]
    Io { message: String },
}

impl From<ToolError> for MobileError {
    fn from(error: ToolError) -> Self {
        match error {
            ToolError::UnknownTool(name) => MobileError::UnknownTool { name },
            ToolError::InvalidArgs(message) => MobileError::InvalidArgs { message },
            ToolError::Failed(message) => MobileError::Failed { message },
        }
    }
}

/// A single, stateful automation session, backing one open document (or set of documents) in
/// the mobile app's lifetime. Safe to call from any thread; calls are serialized internally.
#[derive(uniffi::Object)]
pub struct MobileSession {
    inner: Mutex<Automation>,
}

#[uniffi::export]
impl MobileSession {
    /// Creates a session with no root confinement: tool calls may reference any readable path.
    #[uniffi::constructor]
    pub fn new() -> Self {
        Self { inner: Mutex::new(Automation::new()) }
    }

    /// Creates a session confined to `root`: relative paths passed to tools resolve under it, and
    /// absolute paths outside it are rejected. Use this with the sandboxed directory the iOS
    /// document picker copies files into.
    #[uniffi::constructor]
    pub fn with_root(root: String) -> Result<Self, MobileError> {
        let automation = Automation::new().with_root(root).map_err(|error| MobileError::Io { message: error.to_string() })?;
        Ok(Self { inner: Mutex::new(automation) })
    }

    /// Runs one automation tool (e.g. `doc_open`, `page_render`, `doc_save`) with its arguments
    /// encoded as a JSON object string, returning its result content.
    pub fn call(&self, name: String, args_json: String) -> Result<Vec<MobileContent>, MobileError> {
        let args: serde_json::Value = serde_json::from_str(&args_json).map_err(|error| MobileError::InvalidArgs { message: error.to_string() })?;
        let mut automation = self.inner.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let content = automation.call(&name, &args)?;
        Ok(content.into_iter().map(MobileContent::from).collect())
    }
}

impl Default for MobileSession {
    fn default() -> Self {
        Self::new()
    }
}
