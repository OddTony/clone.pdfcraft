//! Interactive Barcode & QR Code Scanner HUD for Sandeep PDF Studio v3 (L7 UI).
//!
//! Provides automated page detection, unified compact HUD (data of one code at a time),
//! multi-code navigation (◀ / ▶ and arrow keys), 0.5px blue highlight box, clipboard copy,
//! and pixel-perfect 100% scale re-embedding.

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

use egui::{Color32, CornerRadius, Margin, Pos2, Rect, Stroke, vec2};
use pdfcraft_engine::barcode_scan::ScannedCode;
use pdfcraft_engine::{Document, Edit, ImageEdit};
use pdfcraft_render::DocInfo;
use std::sync::Arc;

use crate::canvas::DocView;
use crate::theme;

/// Persistent HUD state for barcode / QR detection and interactive editing.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BarcodeHudState {
    /// Cached detected codes for the active page.
    pub scanned_codes: Vec<ScannedCode>,
    /// Index in `scanned_codes` currently focused by the HUD.
    pub active_index: usize,
    /// Whether HUD is currently open and visible.
    pub open: bool,
    /// Compatibility alias for `open`.
    pub panel_open: bool,
    /// In-progress text string when editing a code.
    pub edit_buffer: Option<String>,
    /// Feedback banner timestamp: `(message, expiry_time)`.
    pub toast: Option<(String, f64)>,
    /// Text or URL payload for generating a new QR code.
    pub qr_input: String,
    /// The page that was last scanned for codes.
    pub last_scanned_page: Option<usize>,
}

impl BarcodeHudState {
    /// Populate or update scanned codes for a page with explicit open control.
    pub fn update_codes_conditional(&mut self, codes: Vec<ScannedCode>, keep_open: bool) {
        self.scanned_codes = codes;
        if self.active_index >= self.scanned_codes.len() {
            self.active_index = 0;
        }
        self.open = keep_open;
        self.panel_open = keep_open;
    }

    /// Populate or update scanned codes for a page.
    pub fn update_codes(&mut self, codes: Vec<ScannedCode>) {
        let has_codes = !codes.is_empty();
        self.update_codes_conditional(codes, has_codes);
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
        self.toast = Some((msg, now + 3.0));
    }
}

/// Unified Floating HUD widget.
pub struct BarcodeScannerHud;

impl BarcodeScannerHud {
    /// Reactive Auto-Page Tracking hook:
    ///
    /// CONDITIONAL AUTO-SCAN STATE:
    /// Never force the QR/Barcode HUD open automatically on page scroll or arrow-key navigation.
    /// Only auto-scan the new page and refresh HUD data if the user already has the HUD explicitly open.
    pub fn auto_scan_on_page_change(view: &mut DocView, doc: &Document, now: f64) {
        let current_page = view.current;
        if view.barcode_hud.last_scanned_page != Some(current_page) {
            view.barcode_hud.last_scanned_page = Some(current_page);

            // Only auto-scan the new page if the HUD is already open
            if view.barcode_hud.open {
                let codes = doc.scan_page_codes(current_page);
                let count = codes.len();
                view.barcode_hud.update_codes_conditional(codes, true);
                if count > 0 {
                    view.barcode_hud.show_toast(format!("Detected {count} code(s) on page {}", current_page + 1), now);
                } else {
                    view.barcode_hud.show_toast(format!("No codes on page {}", current_page + 1), now);
                }
            }
        }
    }

    /// Render the unified interactive HUD and canvas highlight overlays.
    ///
    /// Returns a pending replacement `Edit` if the user modified and applied a code.
    pub fn show(ui: &mut egui::Ui, view: &mut DocView, doc: &Document, info: &DocInfo, can_modify: bool) -> Option<Edit> {
        if !view.barcode_hud.open {
            return None;
        }

        let current_page = view.current;
        let total_pages = info.pages.len();
        let viewport = view.viewport_rect();
        let view_id = view.id.0;
        let now = ui.input(|i| i.time);

        // 1. Highlight barcodes on canvas:
        // Active code gets 0.5px blue bounding box with slight light blue fill.
        // Other codes get faint outline with no fill.
        if let Some(xf) = view.page_xform(current_page) {
            for (i, c) in view.barcode_hud.scanned_codes.iter().enumerate() {
                if c.page == current_page {
                    let code_rect = xf.user_rect(info, current_page, c.rect.map(|v| v as f32));
                    if i == view.barcode_hud.active_index {
                        // User requirement: "highlighted with a blue colour 0.5 px bounding box--- with very slight light blue fill"
                        ui.painter().rect(
                            code_rect,
                            CornerRadius::ZERO,
                            Color32::from_rgba_unmultiplied(0x3B, 0x82, 0xF6, 25), // slight light blue fill
                            Stroke::new(0.5, Color32::from_rgb(0x3B, 0x82, 0xF6)), // 0.5 px blue bounding box
                            egui::StrokeKind::Outside,
                        );
                    } else {
                        ui.painter().rect_stroke(
                            code_rect,
                            CornerRadius::ZERO,
                            Stroke::new(0.5, Color32::from_rgba_unmultiplied(0x3B, 0x82, 0xF6, 65)),
                            egui::StrokeKind::Inside,
                        );
                    }
                }
            }
        }

        let state = &mut view.barcode_hud;
        let total_codes = state.scanned_codes.len();

        // 2. Keyboard arrow navigation when viewing (not editing text)
        let mut step_prev = false;
        let mut step_next = false;
        if state.edit_buffer.is_none() {
            if ui.input(|i| i.key_pressed(egui::Key::ArrowLeft)) {
                step_prev = true;
            }
            if ui.input(|i| i.key_pressed(egui::Key::ArrowRight)) {
                step_next = true;
            }
        }

        let mut pending_edit = None;
        let mut close_hud = false;
        let mut trigger_rescan = false;
        let mut copy_text = None;
        let mut start_edit = false;
        let mut apply_code = None;
        let mut cancel_edit = false;
        let mut trigger_generate = false;

        // Sandeep PDF Studio v3 Modern Dark Aesthetics
        let card_bg = Color32::from_rgb(0x13, 0x14, 0x1A);
        let border_stroke = Stroke::new(1.0, Color32::from_rgb(0x28, 0x2C, 0x3B));
        let item_card_bg = Color32::from_rgb(0x1B, 0x1D, 0x26);
        let item_card_stroke = Stroke::new(1.0, Color32::from_rgb(0x28, 0x2C, 0x3B));
        let text_primary = Color32::from_rgb(0xF8, 0xFA, 0xFC);
        let text_muted = Color32::from_rgb(0x94, 0xA3, 0xB8);
        let accent_blue = Color32::from_rgb(0x3B, 0x82, 0xF6);
        let emerald_green = Color32::from_rgb(0x10, 0xB9, 0x81);

        // Fixed compact panel at top-right of canvas viewport
        let hud_w = 330.0;
        let hud_pos = Pos2::new((viewport.right() - hud_w - 18.0).max(viewport.left() + 18.0), viewport.top() + 18.0);

        egui::Area::new(egui::Id::new(("barcode-scanner-hud-v3", view_id))).order(egui::Order::Foreground).fixed_pos(hud_pos).show(ui.ctx(), |ui| {
            egui::Frame::NONE
                .fill(card_bg)
                .stroke(border_stroke)
                .corner_radius(CornerRadius::same(12))
                .shadow(egui::Shadow { offset: [0, 8], blur: 24, spread: 0, color: Color32::from_black_alpha(170) })
                .inner_margin(Margin::symmetric(14, 12))
                .show(ui, |ui| {
                    ui.set_width(hud_w - 28.0);

                    // Header: Icon + Title + Sandeep PDF Studio v3 Tag + Close button
                    ui.horizontal(|ui| {
                        crate::icons::paint(ui, Rect::from_min_size(ui.cursor().min, vec2(16.0, 16.0)), "scan", 16.0, accent_blue);
                        ui.add_space(22.0);
                        ui.label(egui::RichText::new("QR & Barcodes").font(theme::semibold(13.5)).color(text_primary));

                        // v3 Version Chip
                        ui.painter().rect_filled(
                            Rect::from_min_size(ui.cursor().min + vec2(4.0, 2.0), vec2(26.0, 15.0)),
                            CornerRadius::same(4),
                            Color32::from_rgb(0x1D, 0x4E, 0xD8),
                        );
                        ui.add_space(6.0);
                        ui.label(egui::RichText::new("v3").font(theme::semibold(9.5)).color(Color32::WHITE));

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.small_button("✕").on_hover_text("Close HUD").clicked() {
                                close_hud = true;
                            }
                        });
                    });

                    ui.add_space(8.0);

                    // Section 1: Page Tracker & Multi-Code Navigation Bar
                    egui::Frame::NONE
                        .fill(item_card_bg)
                        .stroke(item_card_stroke)
                        .corner_radius(CornerRadius::same(8))
                        .inner_margin(Margin::symmetric(10, 8))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(
                                    egui::RichText::new(format!("Page {} of {}", current_page + 1, total_pages))
                                        .font(theme::medium(11.5))
                                        .color(text_muted),
                                );
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    if ui.small_button("⟳").on_hover_text("Rescan current page").clicked() {
                                        trigger_rescan = true;
                                    }
                                    ui.label(egui::RichText::new("● AUTO").font(theme::semibold(9.5)).color(emerald_green));
                                });
                            });

                            ui.add_space(6.0);

                            if total_codes > 0 {
                                ui.horizontal(|ui| {
                                    let prev_btn =
                                        ui.add_sized([26.0, 22.0], egui::Button::new("◀")).on_hover_text("Previous barcode/QR code (Left Arrow)");
                                    if prev_btn.clicked() {
                                        step_prev = true;
                                    }

                                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                                        ui.painter().rect_filled(
                                            Rect::from_min_size(ui.cursor().min, vec2(136.0, 22.0)),
                                            CornerRadius::same(5),
                                            Color32::from_rgb(0x13, 0x14, 0x1A),
                                        );
                                        ui.add_space(8.0);
                                        ui.label(
                                            egui::RichText::new(format!("Code {} of {}", state.active_index + 1, total_codes))
                                                .font(theme::semibold(12.0))
                                                .color(accent_blue),
                                        );
                                    });

                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        let next_btn =
                                            ui.add_sized([26.0, 22.0], egui::Button::new("▶")).on_hover_text("Next barcode/QR code (Right Arrow)");
                                        if next_btn.clicked() {
                                            step_next = true;
                                        }
                                    });
                                });
                            } else {
                                ui.label(egui::RichText::new("No barcodes or QR codes on this page").font(theme::regular(11.0)).color(text_muted));
                            }
                        });

                    // Section 2: Active Code Details Card (Data of ONE code at a time)
                    if let Some(active) = state.active_code().cloned() {
                        ui.add_space(8.0);
                        egui::Frame::NONE
                            .fill(item_card_bg)
                            .stroke(item_card_stroke)
                            .corner_radius(CornerRadius::same(8))
                            .inner_margin(Margin::symmetric(10, 8))
                            .show(ui, |ui| {
                                // Symbology + Dimensions
                                ui.horizontal(|ui| {
                                    ui.label(egui::RichText::new("●").color(accent_blue).font(theme::semibold(11.0)));
                                    ui.label(egui::RichText::new(active.kind.label()).font(theme::semibold(12.0)).color(text_primary));
                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        ui.label(
                                            egui::RichText::new(format!("{}×{}px • 100%", active.pixel_size[0], active.pixel_size[1]))
                                                .font(theme::semibold(10.0))
                                                .color(Color32::from_rgb(0x38, 0xBD, 0xF8)),
                                        );
                                    });
                                });

                                ui.add_space(6.0);

                                // Payload View or Edit Mode
                                if let Some(buf) = &mut state.edit_buffer {
                                    ui.label(egui::RichText::new("EDIT PAYLOAD (100% RE-EMBED)").font(theme::semibold(9.5)).color(text_muted));
                                    ui.add_space(3.0);
                                    ui.add(egui::TextEdit::singleline(buf).desired_width(ui.available_width()));
                                    ui.add_space(8.0);
                                    ui.horizontal(|ui| {
                                        let can_apply = can_modify && !buf.trim().is_empty();
                                        if ui.add_enabled(can_apply, egui::Button::new("⚡ Re-embed")).clicked() {
                                            apply_code = Some((active.clone(), buf.clone()));
                                        }
                                        if ui.button("Cancel").clicked() {
                                            cancel_edit = true;
                                        }
                                    });
                                } else {
                                    // Monospace text container
                                    egui::Frame::NONE
                                        .fill(Color32::from_rgb(0x0C, 0x0D, 0x12))
                                        .stroke(Stroke::new(1.0, Color32::from_rgb(0x22, 0x25, 0x30)))
                                        .corner_radius(CornerRadius::same(6))
                                        .inner_margin(Margin::symmetric(8, 6))
                                        .show(ui, |ui| {
                                            ui.horizontal_wrapped(|ui| {
                                                ui.label(egui::RichText::new(&active.text).font(theme::medium(12.0)).monospace().color(text_primary));
                                            });
                                        });

                                    ui.add_space(6.0);
                                    ui.horizontal(|ui| {
                                        if ui.button("📋 Copy").on_hover_text("Copy payload to clipboard").clicked() {
                                            copy_text = Some(active.text.clone());
                                        }
                                        if can_modify && ui.button("✏ Edit Payload").on_hover_text("Edit and re-embed code").clicked() {
                                            start_edit = true;
                                        }
                                    });
                                }
                            });
                    }

                    // Section 3: Collapsible "Generate New QR" Section
                    ui.add_space(8.0);
                    ui.collapsing("Generate New QR Code", |ui| {
                        ui.add_space(4.0);
                        ui.add(
                            egui::TextEdit::singleline(&mut state.qr_input)
                                .desired_width(ui.available_width())
                                .hint_text("https://example.com or text..."),
                        );
                        ui.add_space(6.0);
                        let can_gen = can_modify && !state.qr_input.trim().is_empty();
                        if ui.add_enabled(can_gen, egui::Button::new("⚡ Embed QR on Page")).clicked() {
                            trigger_generate = true;
                        }
                    });

                    // Feedback Toast
                    if let Some((msg, expiry)) = &state.toast {
                        if *expiry > now {
                            ui.add_space(6.0);
                            ui.label(egui::RichText::new(msg).font(theme::medium(11.0)).color(emerald_green));
                        }
                    }
                });
        });

        if trigger_rescan {
            let codes = doc.scan_page_codes(current_page);
            let count = codes.len();
            state.update_codes(codes);
            state.show_toast(format!("Rescanned: found {count} code(s)"), now);
        }
        if cancel_edit {
            state.edit_buffer = None;
        }
        if start_edit {
            if let Some(active) = state.active_code() {
                state.edit_buffer = Some(active.text.clone());
            }
        }
        if let Some((active, buf)) = apply_code {
            let [pw, ph] = active.pixel_size;
            match Document::generate_replacement_code(active.kind, &buf, pw, ph) {
                Ok(png_bytes) => {
                    if let Some(img_idx) = active.image_index {
                        pending_edit = Some(Edit::EditPageImage {
                            page: current_page,
                            index: img_idx,
                            change: ImageEdit::Replace { name: "UpdatedCode.png".into(), bytes: Arc::new(png_bytes) },
                        });
                    } else {
                        // Vector-rendered code replacement: place replacement raster at exact coordinates
                        pending_edit = Some(Edit::AddImage {
                            page: current_page,
                            rect: Some(active.rect),
                            name: "UpdatedCode.png".into(),
                            bytes: Arc::new(png_bytes),
                        });
                    }
                    state.edit_buffer = None;
                    state.show_toast(format!("Replaced at 100% dimensions ({}×{}px)!", pw, ph), now);
                }
                Err(e) => {
                    state.show_toast(format!("Replacement failed: {e}"), now);
                }
            }
        }
        if let Some(text) = copy_text {
            ui.ctx().copy_text(text);
            state.show_toast("Copied payload to clipboard!".into(), now);
        }
        if step_prev {
            state.prev_code();
        }
        if step_next {
            state.next_code();
        }
        if close_hud {
            state.open = false;
            state.panel_open = false;
        }

        if trigger_generate {
            let text = state.qr_input.trim();
            match Document::generate_replacement_code(pdfcraft_engine::barcode_scan::CodeKind::QrCode, text, 240, 240) {
                Ok(png_bytes) => {
                    let (page_w, page_h) = info.pages.get(current_page).map(|p| (p.width as f64, p.height as f64)).unwrap_or((612.0, 792.0));
                    let qr_size = 144.0;
                    let x = (page_w - qr_size) * 0.5;
                    let y = (page_h - qr_size) * 0.5;
                    let rect = [x, y, x + qr_size, y + qr_size];

                    pending_edit =
                        Some(Edit::AddImage { page: current_page, rect: Some(rect), name: "QRCode.png".into(), bytes: Arc::new(png_bytes) });
                    state.show_toast("QR code placed on page!".into(), now);
                }
                Err(e) => {
                    state.show_toast(format!("QR generation error: {e}"), now);
                }
            }
        }

        pending_edit
    }

    /// Legacy compatibility stub for `show_panel` (functionality unified into `show`).
    pub fn show_panel(_ui: &mut egui::Ui, _view: &mut DocView, _doc: &Document, _info: &DocInfo, _can_modify: bool) -> Option<Edit> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pdfcraft_engine::barcode_scan::CodeKind;

    #[test]
    fn test_barcode_hud_state_navigation() {
        let mut state = BarcodeHudState::default();
        let code1 = ScannedCode {
            id: 0,
            page: 0,
            kind: CodeKind::QrCode,
            text: "https://example.com/1".into(),
            rect: [10.0, 10.0, 100.0, 100.0],
            pixel_size: [100, 100],
            image_index: Some(0),
        };
        let code2 = ScannedCode {
            id: 1,
            page: 0,
            kind: CodeKind::Code128,
            text: "TRACK123456".into(),
            rect: [10.0, 120.0, 200.0, 180.0],
            pixel_size: [200, 60],
            image_index: Some(1),
        };

        state.update_codes(vec![code1.clone(), code2.clone()]);
        assert_eq!(state.scanned_codes.len(), 2);
        assert!(state.open);
        assert_eq!(state.active_index, 0);
        assert_eq!(state.active_code().map(|c| c.text.as_str()), Some("https://example.com/1"));

        // Step next
        state.next_code();
        assert_eq!(state.active_index, 1);
        assert_eq!(state.active_code().map(|c| c.text.as_str()), Some("TRACK123456"));

        // Step next wraps to 0
        state.next_code();
        assert_eq!(state.active_index, 0);

        // Step prev wraps to 1
        state.prev_code();
        assert_eq!(state.active_index, 1);
    }

    #[test]
    fn test_toast_display() {
        let mut state = BarcodeHudState::default();
        state.show_toast("Code detected".into(), 10.0);
        assert!(state.toast.is_some());
        let (msg, expiry) = state.toast.unwrap();
        assert_eq!(msg, "Code detected");
        assert_eq!(expiry, 13.0);
    }
}
