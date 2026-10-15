//! Format Studio: restyle a raw document like a reference one, with an AI follow-up chat.
//!
//! The dialog holds two picks — a reference PDF (its [`StyleProfile`] is
//! extracted on pick) and raw content (a PDF, a `.txt` file, or pasted text)
//! — and generates a styled PDF that opens as a new tab. The optional chat
//! sends the profile plus a bounded slice of the raw text to an
//! OpenRouter-compatible endpoint with a key the user pastes per send (never
//! stored); an answer that carries fenced JSON becomes an Apply/Discard
//! proposal through the validated [`apply_patch`](pdfcraft_style::apply_patch).
//!
//! Network runs only on Send, on its own thread, never on wasm (the web build
//! says so instead of hanging).

use std::sync::Arc;

use pdfcraft_engine::style::StyleProfile;

use crate::PdfCraftApp;

/// Raw text cap for pasted and picked content (generation refuses more downstream anyway).
const MAX_RAW_CHARS: usize = 500_000;
/// Context slice folded into one chat turn.
const MAX_CHAT_CHARS: usize = 12_000;

/// Raw content being read on a worker thread.
#[derive(Default)]
pub(crate) enum Extraction {
    #[default]
    Idle,
    #[cfg(not(target_arch = "wasm32"))]
    Working(std::sync::mpsc::Receiver<(String, bool)>),
    Done,
}

/// One AI answer waiting on its worker thread.
#[derive(Default)]
pub(crate) enum ChatReply {
    #[default]
    Idle,
    #[cfg(not(target_arch = "wasm32"))]
    Waiting(std::sync::mpsc::Receiver<Result<String, String>>),
}

/// A chat turn.
#[derive(Clone, Debug)]
pub(crate) struct ChatTurn {
    pub(crate) who: &'static str,
    pub(crate) text: String,
}

/// The Format Studio dialog state.
#[derive(Default)]
pub(crate) struct FormatStudio {
    pub(crate) reference_name: Option<String>,
    pub(crate) profile: Option<StyleProfile>,
    pub(crate) profile_summary: String,
    pub(crate) raw_name: Option<String>,
    pub(crate) raw_text: String,
    pub(crate) raw_truncated: bool,
    pub(crate) extraction: Extraction,
    pub(crate) key: String,
    pub(crate) model: String,
    pub(crate) prompt: String,
    pub(crate) chat: Vec<ChatTurn>,
    pub(crate) reply: ChatReply,
    pub(crate) pending_patch: Option<serde_json::Value>,
}

impl FormatStudio {
    pub(crate) fn fresh() -> Self {
        Self { model: "openrouter/auto".to_string(), ..Self::default() }
    }

    pub(crate) fn can_generate(&self) -> bool {
        self.profile.is_some() && !self.raw_text.trim().is_empty() && matches!(self.extraction, Extraction::Idle | Extraction::Done)
    }
}

fn summarize(profile: &StyleProfile) -> String {
    format!(
        "{}×{} pt · margins {}/{}/{}/{} · body {} pt · {} heading level{} · bullets {} pt · page numbers {}",
        profile.page[0].round(),
        profile.page[1].round(),
        profile.margins[0].round(),
        profile.margins[1].round(),
        profile.margins[2].round(),
        profile.margins[3].round(),
        profile.body_size,
        profile.headings.len(),
        if profile.headings.len() == 1 { "" } else { "s" },
        profile.bullet_indent.round(),
        if profile.page_numbers { "on" } else { "off" },
    )
}

/// The fenced ```json block of an answer, if it parses.
fn fenced_patch(answer: &str) -> Option<serde_json::Value> {
    let fence = answer.find("```json").or_else(|| answer.find("```"))?;
    let body = answer.get(fence..)?.find('\n').map(|n| &answer[fence + n + 1..])?;
    let end = body.find("```")?;
    serde_json::from_str(body[..end].trim()).ok()
}

impl PdfCraftApp {
    /// File ▸ Format Studio…: start over with a fresh dialog.
    pub(crate) fn format_studio_dialog(&mut self) {
        self.format = FormatStudio::fresh();
        self.dialog = Some(crate::Dialog::FormatStudio);
    }

    /// A picked reference PDF: open it as a tab and extract its profile.
    pub(crate) fn use_format_reference(&mut self, name: String, bytes: Vec<u8>) {
        self.open_created(&name, Arc::new(bytes), "Opened the reference");
        let Some(view) = self.views.last() else { return };
        let Some(doc) = self.session.get(view.id) else { return };
        match pdfcraft_engine::style::profile_of(doc, 12) {
            Ok(profile) => {
                self.format.profile_summary = summarize(&profile);
                self.format.profile = Some(profile);
                self.format.reference_name = Some(name);
            }
            Err(e) => self.notify_fmt("Couldn't read the reference formatting: {e}", &[("e", &e.to_string())]),
        }
    }

    /// A picked raw file: text goes straight into the editor, a PDF opens as
    /// a tab and its text is read on a worker thread (bounded, with notice).
    pub(crate) fn use_format_raw(&mut self, name: String, bytes: Vec<u8>) {
        let lower = name.to_ascii_lowercase();
        if lower.ends_with(".txt") || lower.ends_with(".text") {
            let mut text = pdfcraft_engine::decode_text(&bytes);
            if text.len() > MAX_RAW_CHARS {
                text.truncate(text.floor_char_boundary(MAX_RAW_CHARS.min(text.len())));
                self.format.raw_truncated = true;
            }
            self.format.raw_text = text;
            self.format.raw_name = Some(name);
            return;
        }
        self.open_created(&name, Arc::new(bytes.clone()), "Opened the raw document");
        let Some(view) = self.views.last() else { return };
        let Some(doc) = self.session.get(view.id) else { return };
        if !doc.allows_copying() {
            self.notify_tr("This document doesn't allow copying text out of it.");
            return;
        }
        let (bytes, password, pages) = (doc.bytes.clone(), doc.password.clone(), doc.info.pages.len());
        self.format.raw_name = Some(name);
        #[cfg(not(target_arch = "wasm32"))]
        {
            let (tx, rx) = std::sync::mpsc::channel();
            let ctx = self.ctx.clone();
            if std::thread::Builder::new()
                .name("format-studio-extract".into())
                .spawn(move || {
                    let (text, left_out) = pdfcraft_engine::style::source_text(&bytes, password.as_deref(), pages, 2000, MAX_RAW_CHARS);
                    let _ = tx.send((text, left_out));
                    if let Some(ctx) = ctx {
                        ctx.request_repaint();
                    }
                })
                .is_err()
            {
                self.notify_tr("Couldn't start reading the raw document.");
                return;
            }
            self.format.extraction = Extraction::Working(rx);
        }
        #[cfg(target_arch = "wasm32")]
        {
            let (text, left_out) = pdfcraft_engine::style::source_text(&bytes, password.as_deref(), pages, 2000, MAX_RAW_CHARS);
            self.format.raw_text = text;
            self.format.raw_truncated = left_out;
        }
    }

    /// Pick up finished extraction (each frame).
    pub(crate) fn poll_format_studio(&mut self) {
        #[cfg(not(target_arch = "wasm32"))]
        if let Extraction::Working(rx) = &self.format.extraction {
            match rx.try_recv() {
                Err(std::sync::mpsc::TryRecvError::Empty) => (),
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.format.extraction = Extraction::Idle;
                    self.notify_tr("Reading the raw document stopped unexpectedly.");
                }
                Ok((text, left_out)) => {
                    self.format.raw_text = text;
                    self.format.raw_truncated = left_out;
                    self.format.extraction = Extraction::Done;
                    if left_out {
                        self.notify_tr("The raw document was truncated to what fits; the rest stays in its tab.");
                    }
                }
            }
        }
        #[cfg(not(target_arch = "wasm32"))]
        if let ChatReply::Waiting(rx) = &self.format.reply {
            match rx.try_recv() {
                Err(std::sync::mpsc::TryRecvError::Empty) => (),
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.format.reply = ChatReply::Idle;
                    self.format.chat.push(ChatTurn { who: "PdfCraft", text: tl!("The answer stopped unexpectedly.").to_string() });
                }
                Ok(answer) => {
                    self.format.reply = ChatReply::Idle;
                    match answer {
                        Err(e) => self.format.chat.push(ChatTurn { who: "PdfCraft", text: e }),
                        Ok(text) => {
                            if let Some(patch) = fenced_patch(&text)
                                && let Some(profile) = &self.format.profile
                                && pdfcraft_style::apply_patch(profile, &patch).is_ok()
                            {
                                self.format.pending_patch = Some(patch);
                            }
                            self.format.chat.push(ChatTurn { who: "AI", text });
                        }
                    }
                }
            }
        }
    }

    /// Generate the styled document and open it as a new tab.
    pub(crate) fn format_generate(&mut self) {
        let (Some(profile), text) = (self.format.profile.clone(), self.format.raw_text.clone()) else { return };
        if text.trim().is_empty() {
            self.notify_tr("There is no raw text yet.");
            return;
        }
        let title = format!("Styled {}", self.format.raw_name.as_deref().unwrap_or("document"));
        match self.session.create_from_style(&title, &text, &profile) {
            Ok(bytes) => self.open_created(&format!("{title}.pdf"), bytes, "Created the styled document"),
            Err(e) => self.notify_fmt("Couldn't style the document: {e}", &[("e", &e.to_string())]),
        }
    }

    /// Send the prompt to the provider (its own thread; the key is never stored).
    pub(crate) fn format_send(&mut self) {
        let prompt = self.format.prompt.trim().to_string();
        if prompt.is_empty() || prompt.len() > 8_000 {
            self.notify_tr("Write a prompt first (at most 8000 characters).");
            return;
        }
        if self.format.key.trim().is_empty() {
            self.notify_tr("Paste an API key first; it is sent only with this request.");
            return;
        }
        let profile_json = self.format.profile.as_ref().map(|p| serde_json::to_string(p).unwrap_or_default()).unwrap_or_default();
        let mut context = String::new();
        if !profile_json.is_empty() {
            context.push_str(&format!("Formatting profile:\n{profile_json}\n\n"));
        }
        let head = self.format.raw_text.chars().take(MAX_CHAT_CHARS).collect::<String>();
        if !head.trim().is_empty() {
            context.push_str(&format!("Raw text (head):\n{head}"));
        }
        let user = if context.is_empty() { prompt.clone() } else { format!("{prompt}\n\nDocument context:\n{context}") };
        self.format.chat.push(ChatTurn { who: "You", text: prompt });
        self.format.prompt.clear();
        #[cfg(target_arch = "wasm32")]
        {
            let _ = user;
            self.format
                .chat
                .push(ChatTurn { who: "PdfCraft", text: tl!("AI chat isn't available in the web build; use the desktop app or CLI.").to_string() });
            return;
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            let config = pdfcraft_ai::ProviderConfig {
                base_url: "https://openrouter.ai/api/v1".to_string(),
                api_key: self.format.key.clone(),
                model: if self.format.model.trim().is_empty() { "openrouter/auto".to_string() } else { self.format.model.trim().to_string() },
            };
            let messages = vec![pdfcraft_ai::ChatMessage::system(pdfcraft_ai::STYLE_PATCH_SYSTEM), pdfcraft_ai::ChatMessage::user(user)];
            let (tx, rx) = std::sync::mpsc::channel();
            let ctx = self.ctx.clone();
            if std::thread::Builder::new()
                .name("format-studio-chat".into())
                .spawn(move || {
                    let answer = pdfcraft_ai::chat(&config, &messages).map_err(|e| e.to_string());
                    let _ = tx.send(answer);
                    if let Some(ctx) = ctx {
                        ctx.request_repaint();
                    }
                })
                .is_err()
            {
                self.notify_tr("Couldn't start the request.");
                return;
            }
            self.format.reply = ChatReply::Waiting(rx);
        }
    }

    /// Apply the proposed patch the last answer carried.
    pub(crate) fn format_apply_patch(&mut self) {
        let (Some(profile), Some(patch)) = (self.format.profile.clone(), self.format.pending_patch.clone()) else { return };
        match pdfcraft_style::apply_patch(&profile, &patch) {
            Ok(patched) => {
                self.format.profile_summary = summarize(&patched);
                self.format.profile = Some(patched);
                self.format.pending_patch = None;
                self.notify_tr("Applied the proposed formatting; Generate builds it.");
            }
            Err(e) => self.notify_fmt("The proposal doesn't apply: {e}", &[("e", &e.to_string())]),
        }
    }
}

/// The Format Studio dialog: two picks, a text editor, Generate, and the AI follow-up chat.
pub(crate) fn dialog(ui: &mut egui::Ui, app: &mut PdfCraftApp) {
    use crate::{theme, widgets};
    use egui::{Align, Layout};

    ui.set_width(560.0);
    ui.label(egui::RichText::new(tl!("Format Studio")).font(theme::semibold(18.0)));
    ui.add_space(8.0);

    ui.horizontal(|ui| {
        let name = app.format.reference_name.as_deref().unwrap_or(tl!("No reference yet"));
        ui.label(tl!("Reference:"));
        ui.label(name);
        if ui.button(tl!("Choose reference…")).clicked() {
            app.format_reference_dialog();
        }
    });
    if !app.format.profile_summary.is_empty() {
        ui.label(egui::RichText::new(&app.format.profile_summary).weak());
    }
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        let name = app.format.raw_name.as_deref().unwrap_or(tl!("No raw file yet"));
        ui.label(tl!("Raw:"));
        ui.label(name);
        if ui.button(tl!("Choose raw file…")).clicked() {
            app.format_raw_dialog();
        }
    });
    #[cfg(not(target_arch = "wasm32"))]
    if matches!(app.format.extraction, Extraction::Working(_)) {
        ui.label(tl!("Reading the raw document…"));
    }
    if app.format.raw_truncated {
        ui.label(tl!("The raw text was truncated to what fits."));
    }
    ui.add(egui::TextEdit::multiline(&mut app.format.raw_text).desired_rows(6).hint_text(tl!("…or paste the raw text here; # headings, - bullets")));
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        let ready = app.format.can_generate();
        if widgets::pill_button(ui, tl!("Generate styled PDF"), ready).clicked() && ready {
            app.format_generate();
        }
    });
    ui.add_space(8.0);
    ui.separator();
    ui.add_space(4.0);
    ui.label(egui::RichText::new(tl!("Ask AI to change the formatting")).font(theme::semibold(14.0)));
    ui.horizontal(|ui| {
        ui.label(tl!("API key:"));
        ui.add(egui::TextEdit::singleline(&mut app.format.key).password(true).hint_text(tl!("Paste an OpenRouter key")));
    });
    ui.horizontal(|ui| {
        ui.label(tl!("Model:"));
        ui.text_edit_singleline(&mut app.format.model);
    });
    ui.horizontal(|ui| {
        #[cfg(not(target_arch = "wasm32"))]
        let busy = matches!(app.format.reply, ChatReply::Waiting(_));
        #[cfg(target_arch = "wasm32")]
        let busy = false;
        let prompt = ui.add(
            egui::TextEdit::singleline(&mut app.format.prompt).hint_text(tl!("e.g. bigger headings, numbered pages")).desired_width(f32::INFINITY),
        );
        let send = widgets::pill_button(ui, if busy { tl!("Sending…") } else { tl!("Send") }, !busy);
        if (send.clicked() || (!busy && prompt.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)))) && !busy {
            app.format_send();
        }
    });
    #[cfg(target_arch = "wasm32")]
    ui.label(tl!("AI chat isn't available in the web build; use the desktop app or CLI."));
    if !app.format.chat.is_empty() {
        ui.add_space(4.0);
        egui::ScrollArea::vertical().max_height(160.0).show(ui, |ui| {
            for turn in &app.format.chat {
                ui.label(egui::RichText::new(format!("{}: {}", turn.who, turn.text)).small());
            }
        });
    }
    if app.format.pending_patch.is_some() {
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label(tl!("The answer proposes formatting changes."));
            if widgets::pill_button(ui, tl!("Apply"), true).clicked() {
                app.format_apply_patch();
            }
            if widgets::pill_button(ui, tl!("Discard"), false).clicked() {
                app.format.pending_patch = None;
            }
        });
    }
    ui.add_space(12.0);
    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
        if widgets::pill_button(ui, tl!("Close"), false).clicked() {
            app.dialog = None;
        }
    });
}
