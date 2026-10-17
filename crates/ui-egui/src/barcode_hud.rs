//! Interactive Barcode & QR Code Scanner HUD for PDF Craft (L7 UI).
//!
//! Provides automated detection, anchored HUD popups, multi-code pagination (< / >),
//! clipboard copy, and pixel-perfect 100% scale replacement directly onto canvas coordinates.

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

use std::sync::Arc;
use egui::{Color32, CornerRadius, Pos2, Rect, Stroke};
use pdfcraft_engine::barcode_scan::ScannedCode;
use pdfcraft_engine::{Document, Edit, ImageEdit};
use pdfcraft_render::DocInfo;

use crate::canvas::DocView;
use crate::theme::Tokens;

/// Persistent HUD state for barcode / QR detection and interactive editing.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BarcodeHudState {
    /// Cached detected codes for the active page.
    pub scanned_codes: Vec<ScannedCode>,
    /// Index in `scanned_codes` currently focused by the HUD.
    pub active_index: usize,
    /// Whether HUD popup is currently open and visible.
    pub open: bool,
    /// In-progress text string when editing a code.
    pub edit_buffer: Option<String>,
    /// Feedback banner timestamp: `(message, expiry_time)`.
    pub toast: Option<(String, f64)>,
    /// Whether the QR generator & scanner panel is open on the viewing window.
    pub panel_open: bool,
    /// Text or URL payload for generating a new QR code.
    pub qr_input: String,
    /// The page that was last scanned for codes.
    pub last_scanned_page: Option<usize>,
}

impl BarcodeHudState {
    /// Populate or update scanned codes for a page.
    pub fn update_codes(&mut self, codes: Vec<ScannedCode>) {
        self.scanned_codes = codes;
        if self.active_index >= self.scanned_codes.len() {
            self.active_index = 0;
        }
        if !self.scanned_codes.is_empty() {
            self.open = true;
        }
    }

    /// Step to previous code on the page.
    pub fn prev_code(&mut self) {
        if !self.scanned_codes.is_empty() {
            if self.active_index == 0 {
                self.active_index = self.scanned_codes.len().saturating_sub(1);
            } else {
                self.active_index = self.active_index.saturating_sub(1);
            }
            self.edit_buffer = None;
        }
    }

    /// Step to next code on the page.
    pub fn next_code(&mut self) {
        if !self.scanned_codes.is_empty() {
            self.active_index = (self.active_index.saturating_add(1)) % self.scanned_codes.len();
            self.edit_buffer = None;
        }
    }

    /// Current active code.
    pub fn active_code(&self) -> Option<&ScannedCode> {
        self.scanned_codes.get(self.active_index)
    }

    /// Show feedback toast in HUD.
    pub fn show_toast(&mut self, msg: String, now: f64) {
        self.toast = Some((msg, now + 2.0));
    }
}

/// Floating HUD widget.
pub struct BarcodeScannerHud;

impl BarcodeScannerHud {
    /// Render the interactive HUD on the canvas foreground.
    /// Render the interactive HUD on the canvas foreground.
    /// Returns a pending replacement `Edit` if the user modified and applied a code.
    pub fn show(
        ui: &mut egui::Ui,
        view: &mut DocView,
        info: &DocInfo,
        can_modify: bool,
    ) -> Option<Edit> {
        if !view.barcode_hud.open || view.barcode_hud.scanned_codes.is_empty() {
            return None;
        }

        let total_codes = view.barcode_hud.scanned_codes.len();
        let active = view.barcode_hud.active_code()?.clone();
        let page = active.page;
        let xf = view.page_xform(page)?;
        let screen_rect = xf.user_rect(info, page, active.rect.map(|v| v as f32));
        let viewport = view.viewport_rect();
        let view_id = view.id.0;

        let state = &mut view.barcode_hud;

        // Calculate anchor position immediately below or above the code's bounding box
        let hud_w = 330.0;
        let hud_h = if state.edit_buffer.is_some() { 160.0 } else { 120.0 };
        let x = (screen_rect.center().x - hud_w * 0.5).clamp(viewport.left() + 10.0, (viewport.right() - hud_w - 10.0).max(viewport.left() + 10.0));
        let y = if screen_rect.bottom() + hud_h + 10.0 <= viewport.bottom() {
            screen_rect.bottom() + 8.0
        } else {
            (screen_rect.top() - hud_h - 8.0).max(viewport.top() + 10.0)
        };
        let anchor = Pos2::new(x, y);

        // Highlight code on canvas with clean accent border
        ui.painter().rect_stroke(
            screen_rect.expand(2.0),
            CornerRadius::same(2),
            Stroke::new(2.0, Color32::from_rgb(0x14, 0x73, 0xE6)),
            egui::StrokeKind::Outside,
        );

        let t = Tokens::get(ui.ctx());
        let now = ui.input(|i| i.time);
        let mut pending_edit = None;
        let mut step_prev = false;
        let mut step_next = false;
        let mut close_hud = false;
        let mut copy_text = None;
        let mut start_edit = false;
        let mut apply_code = None;
        let mut cancel_edit = false;

        egui::Area::new(egui::Id::new(("barcode-scanner-hud", view_id, page, active.id)))
            .order(egui::Order::Foreground)
            .fixed_pos(anchor)
            .show(ui.ctx(), |ui| {
                egui::Frame::NONE
                    .fill(t.card)
                    .stroke(Stroke::new(1.0, t.border))
                    .corner_radius(CornerRadius::same(10))
                    .shadow(egui::Shadow {
                        offset: [0, 6],
                        blur: 20,
                        spread: 0,
                        color: Color32::from_black_alpha(if t.dark() { 110 } else { 40 }),
                    })
                    .inner_margin(egui::Margin::symmetric(12, 10))
                    .show(ui, |ui| {
                        ui.set_width(hud_w - 24.0);

                        // Header: Icon + Symbology Title + Multi-Code Pager
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new(active.kind.label()).font(crate::theme::semibold(13.0)));

                            if total_codes > 1 {
                                ui.add_space(4.0);
                                ui.label(
                                    egui::RichText::new(format!("{}/{}", state.active_index + 1, total_codes))
                                        .font(crate::theme::medium(11.0))
                                        .color(t.text_muted),
                                );

                                // Multi-code navigation arrows: < and >
                                if ui.small_button("◀").clicked() {
                                    step_prev = true;
                                }
                                if ui.small_button("▶").clicked() {
                                    step_next = true;
                                }
                            }

                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui.small_button("✕").clicked() {
                                    close_hud = true;
                                }
                            });
                        });

                        ui.separator();

                        // Payload display or edit mode
                        if let Some(buf) = &mut state.edit_buffer {
                            ui.label(egui::RichText::new("Edit Payload:").font(crate::theme::medium(11.0)).color(t.text_muted));
                            ui.add(egui::TextEdit::singleline(buf).desired_width(ui.available_width()));
                            ui.add_space(6.0);

                            ui.horizontal(|ui| {
                                if ui.add_enabled(can_modify, egui::Button::new("Apply (100% Match)")).clicked() {
                                    apply_code = Some(buf.clone());
                                }
                                if ui.button("Cancel").clicked() {
                                    cancel_edit = true;
                                }
                            });
                        } else {
                            // View mode: Text display + Copy / Edit actions
                            ui.horizontal(|ui| {
                                let display_text = if active.text.len() > 32 {
                                    format!("{}…", &active.text[..active.text.floor_char_boundary(32)])
                                } else {
                                    active.text.clone()
                                };
                                ui.label(egui::RichText::new(display_text).font(crate::theme::medium(12.5)).monospace());
                            });

                            ui.add_space(6.0);
                            ui.horizontal(|ui| {
                                if ui.button("Copy Scanned Text").clicked() {
                                    copy_text = Some(active.text.clone());
                                }
                                if can_modify && ui.button("Edit").clicked() {
                                    start_edit = true;
                                }

                                // Pixel dimensions badge
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    ui.label(
                                        egui::RichText::new(format!("{}×{}px", active.pixel_size[0], active.pixel_size[1]))
                                            .font(crate::theme::medium(10.5))
                                            .color(t.text_muted),
                                    );
                                });
                            });
                        }

                        // Feedback message / toast
                        if let Some((msg, expiry)) = &state.toast {
                            if *expiry > now {
                                ui.label(egui::RichText::new(msg).font(crate::theme::medium(11.0)).color(Color32::from_rgb(0x10, 0x7C, 0x10)));
                            }
                        }
                    });
            });

        if cancel_edit {
            state.edit_buffer = None;
        }
        if start_edit {
            state.edit_buffer = Some(active.text.clone());
        }
        if let Some(buf) = apply_code {
            // Generate replacement image with 100% pixel-perfect dimensions
            let [pw, ph] = active.pixel_size;
            match Document::generate_replacement_code(active.kind, &buf, pw, ph) {
                Ok(png_bytes) => {
                    if let Some(img_idx) = active.image_index {
                        pending_edit = Some(Edit::EditPageImage {
                            page,
                            index: img_idx,
                            change: ImageEdit::Replace {
                                name: "UpdatedCode.png".into(),
                                bytes: Arc::new(png_bytes),
                            },
                        });
                    } else {
                        pending_edit = Some(Edit::AddImage {
                            page,
                            rect: Some(active.rect),
                            name: "UpdatedCode.png".into(),
                            bytes: Arc::new(png_bytes),
                        });
                    }
                }
                Err(err) => {
                    state.show_toast(format!("Error: {err}"), now);
                }
            }
            state.edit_buffer = None;
        }

        if step_prev {
            state.prev_code();
        }
        if step_next {
            state.next_code();
        }
        if close_hud {
            state.open = false;
        }
        if let Some(text) = copy_text {
            ui.ctx().copy_text(text);
            state.show_toast("Copied to clipboard!".into(), now);
        }

        pending_edit
    }

    /// Render the QR Code generation and Barcode scanner panel in the active viewport.
    pub fn show_panel(
        ui: &mut egui::Ui,
        view: &mut DocView,
        doc: &Document,
        info: &DocInfo,
        can_modify: bool,
    ) -> Option<Edit> {
        if !view.barcode_hud.panel_open {
            return None;
        }

        let viewport = view.viewport_rect();
        let t = Tokens::get(ui.ctx());
        let mut pending_edit = None;
        let mut close_panel = false;
        let mut trigger_scan = false;
        let mut trigger_generate = false;
        let now = ui.input(|i| i.time);

        let panel_w = 280.0;
        let panel_pos = Pos2::new(
            (viewport.right() - panel_w - 16.0).max(viewport.left() + 16.0),
            viewport.top() + 16.0,
        );

        egui::Area::new(egui::Id::new(("barcode-qr-panel", view.id.0)))
            .order(egui::Order::Foreground)
            .fixed_pos(panel_pos)
            .show(ui.ctx(), |ui| {
                egui::Frame::NONE
                    .fill(t.card)
                    .stroke(Stroke::new(1.0, t.border))
                    .corner_radius(CornerRadius::same(10))
                    .shadow(egui::Shadow {
                        offset: [0, 4],
                        blur: 16,
                        spread: 0,
                        color: Color32::from_black_alpha(if t.dark() { 100 } else { 32 }),
                    })
                    .inner_margin(egui::Margin::symmetric(14, 12))
                    .show(ui, |ui| {
                        ui.set_width(panel_w - 28.0);

                        // Header: Title + Close button
                        ui.horizontal(|ui| {
                            crate::icons::paint(ui, Rect::from_min_size(ui.cursor().min, egui::vec2(16.0, 16.0)), "scan", 16.0, t.accent);
                            ui.add_space(20.0);
                            ui.label(egui::RichText::new("QR & Barcodes").font(crate::theme::semibold(14.0)));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui.small_button("✕").clicked() {
                                    close_panel = true;
                                }
                            });
                        });

                        ui.add_space(6.0);
                        ui.separator();
                        ui.add_space(6.0);

                        // Section 1: Page Scanner
                        ui.label(egui::RichText::new("SCANNER").font(crate::theme::semibold(10.5)).color(t.text_muted));
                        ui.add_space(2.0);

                        let current_page = view.current;
                        let scan_btn_label = format!("Scan Page {} for Codes", current_page + 1);
                        if ui.button(scan_btn_label).clicked() {
                            trigger_scan = true;
                        }

                        if view.barcode_hud.open && !view.barcode_hud.scanned_codes.is_empty() {
                            ui.add_space(2.0);
                            ui.label(
                                egui::RichText::new(format!(
                                    "✓ Found {} code(s) on page {}",
                                    view.barcode_hud.scanned_codes.len(),
                                    current_page + 1
                                ))
                                .font(crate::theme::medium(11.0))
                                .color(Color32::from_rgb(0x10, 0x7C, 0x10)),
                            );
                        } else if view.barcode_hud.last_scanned_page == Some(current_page) && view.barcode_hud.scanned_codes.is_empty() {
                            ui.add_space(2.0);
                            ui.label(
                                egui::RichText::new("No codes detected on this page")
                                    .font(crate::theme::regular(11.0))
                                    .color(t.text_faint),
                            );
                        }

                        ui.add_space(10.0);
                        ui.separator();
                        ui.add_space(6.0);

                        // Section 2: QR Code Generator
                        ui.label(egui::RichText::new("GENERATE QR CODE").font(crate::theme::semibold(10.5)).color(t.text_muted));
                        ui.add_space(2.0);
                        ui.label(egui::RichText::new("Text or URL payload:").font(crate::theme::regular(11.5)).color(t.text));
                        ui.add_space(2.0);

                        ui.add(
                            egui::TextEdit::singleline(&mut view.barcode_hud.qr_input)
                                .desired_width(ui.available_width())
                                .hint_text("https://example.com or text..."),
                        );

                        ui.add_space(6.0);
                        let can_embed = can_modify && !view.barcode_hud.qr_input.trim().is_empty();
                        if ui.add_enabled(can_embed, egui::Button::new("Embed QR on Page")).clicked() {
                            trigger_generate = true;
                        }

                        // Feedback Toast
                        if let Some((msg, expiry)) = &view.barcode_hud.toast {
                            if *expiry > now {
                                ui.add_space(4.0);
                                ui.label(egui::RichText::new(msg).font(crate::theme::medium(11.0)).color(Color32::from_rgb(0x10, 0x7C, 0x10)));
                            }
                        }
                    });
            });

        if close_panel {
            view.barcode_hud.panel_open = false;
        }

        if trigger_scan {
            let codes = doc.scan_page_codes(view.current);
            view.barcode_hud.last_scanned_page = Some(view.current);
            if codes.is_empty() {
                view.barcode_hud.scanned_codes.clear();
                view.barcode_hud.open = false;
                view.barcode_hud.show_toast("No QR or barcodes found on page".into(), now);
            } else {
                let count = codes.len();
                view.barcode_hud.update_codes(codes);
                view.barcode_hud.show_toast(format!("Found {count} code(s) on page!"), now);
            }
        }

        if trigger_generate {
            let text = view.barcode_hud.qr_input.trim();
            match pdfcraft_engine::qr::QrCode::encode_text(text) {
                Ok(matrix) => {
                    match matrix.to_png_bytes(8, 2) {
                        Ok(png_bytes) => {
                            let page = view.current;
                            let (page_w, page_h) = info.pages.get(page).map(|p| (p.width as f64, p.height as f64)).unwrap_or((612.0, 792.0));
                            let qr_size = 144.0; // 2 inches
                            let x = (page_w - qr_size) * 0.5;
                            let y = (page_h - qr_size) * 0.5;
                            let rect = [x, y, x + qr_size, y + qr_size];

                            pending_edit = Some(Edit::AddImage {
                                page,
                                rect: Some(rect),
                                name: "QRCode.png".into(),
                                bytes: Arc::new(png_bytes),
                            });
                            view.barcode_hud.show_toast("QR code placed on page!".into(), now);
                        }
                        Err(e) => {
                            view.barcode_hud.show_toast(format!("PNG error: {e}"), now);
                        }
                    }
                }
                Err(e) => {
                    view.barcode_hud.show_toast(format!("QR error: {e}"), now);
                }
            }
        }

        pending_edit
    }
}
