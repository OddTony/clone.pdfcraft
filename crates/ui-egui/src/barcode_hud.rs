//! Interactive Barcode & QR Code Scanner HUD for Sandeep PDF Studio v3 (L7 UI).
//!
//! Provides automated page detection, anchored HUD popups, multi-code pagination (◀ / ▶),
//! clipboard copy, and pixel-perfect 100% scale replacement directly onto canvas coordinates.

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

use std::sync::Arc;
use egui::{vec2, Color32, CornerRadius, Margin, Pos2, Rect, Stroke};
use pdfcraft_engine::barcode_scan::ScannedCode;
use pdfcraft_engine::{Document, Edit, ImageEdit};
use pdfcraft_render::DocInfo;

use crate::canvas::DocView;
use crate::theme;

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
        self.toast = Some((msg, now + 3.0));
    }
}

/// Floating HUD widget and Inspector panel.
pub struct BarcodeScannerHud;

impl BarcodeScannerHud {
    /// Reactive Auto-Page Tracking hook:
    /// Whenever the current page index cleanly snaps/changes (via arrow keys, scrolling,
    /// or page navigation), this hook reacts instantly to the new page context and extracts/displays
    /// its QR codes and barcodes seamlessly.
    pub fn auto_scan_on_page_change(
        view: &mut DocView,
        doc: &Document,
        now: f64,
    ) {
        let current_page = view.current;
        if view.barcode_hud.last_scanned_page != Some(current_page) {
            view.barcode_hud.last_scanned_page = Some(current_page);

            // Reactively scan if either the panel is open, the HUD is active, or we're tracking codes
            if view.barcode_hud.panel_open || view.barcode_hud.open || !view.barcode_hud.scanned_codes.is_empty() {
                let codes = doc.scan_page_codes(current_page);
                if codes.is_empty() {
                    view.barcode_hud.scanned_codes.clear();
                    view.barcode_hud.open = false;
                } else {
                    let count = codes.len();
                    view.barcode_hud.update_codes(codes);
                    view.barcode_hud.open = true;
                    view.barcode_hud.show_toast(format!("Auto-detected {count} code(s) on page {}", current_page + 1), now);
                }
            }
        }
    }

    /// Render the interactive HUD on the canvas foreground anchored directly to the active barcode.
    ///
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
        let hud_w = 340.0;
        let hud_h = if state.edit_buffer.is_some() { 165.0 } else { 128.0 };
        let x = (screen_rect.center().x - hud_w * 0.5)
            .clamp(viewport.left() + 10.0, (viewport.right() - hud_w - 10.0).max(viewport.left() + 10.0));
        let y = if screen_rect.bottom() + hud_h + 10.0 <= viewport.bottom() {
            screen_rect.bottom() + 8.0
        } else {
            (screen_rect.top() - hud_h - 8.0).max(viewport.top() + 10.0)
        };
        let anchor = Pos2::new(x, y);

        // Highlight code on canvas with clean electric blue accent border (Sandeep PDF Studio v3 look)
        ui.painter().rect_stroke(
            screen_rect.expand(2.5),
            CornerRadius::same(3),
            Stroke::new(2.0, Color32::from_rgb(0x3B, 0x82, 0xF6)),
            egui::StrokeKind::Outside,
        );
        ui.painter().rect_filled(
            screen_rect.expand(2.5),
            CornerRadius::same(3),
            Color32::from_rgba_unmultiplied(0x3B, 0x82, 0xF6, 18),
        );

        let now = ui.input(|i| i.time);
        let mut pending_edit = None;
        let mut step_prev = false;
        let mut step_next = false;
        let mut close_hud = false;
        let mut copy_text = None;
        let mut start_edit = false;
        let mut apply_code = None;
        let mut cancel_edit = false;

        // Sandeep PDF Studio v3 Modern Dark Aesthetics
        let card_bg = Color32::from_rgb(0x13, 0x14, 0x1A);
        let border_stroke = Stroke::new(1.0, Color32::from_rgb(0x2E, 0x32, 0x42));
        let text_primary = Color32::from_rgb(0xF8, 0xFA, 0xFC);
        let text_muted = Color32::from_rgb(0x94, 0xA3, 0xB8);
        let chip_bg = Color32::from_rgb(0x1E, 0x21, 0x2D);
        let accent_blue = Color32::from_rgb(0x3B, 0x82, 0xF6);

        egui::Area::new(egui::Id::new(("barcode-scanner-hud", view_id, page, active.id)))
            .order(egui::Order::Foreground)
            .fixed_pos(anchor)
            .show(ui.ctx(), |ui| {
                egui::Frame::NONE
                    .fill(card_bg)
                    .stroke(border_stroke)
                    .corner_radius(CornerRadius::same(12))
                    .shadow(egui::Shadow {
                        offset: [0, 8],
                        blur: 24,
                        spread: 0,
                        color: Color32::from_black_alpha(160),
                    })
                    .inner_margin(Margin::symmetric(14, 12))
                    .show(ui, |ui| {
                        ui.set_width(hud_w - 28.0);

                        // Header: Icon + Symbology Title + Multi-Code Pager
                        ui.horizontal(|ui| {
                            // Status icon dot
                            ui.label(egui::RichText::new("●").color(accent_blue).font(theme::semibold(12.0)));
                            ui.label(egui::RichText::new(active.kind.label()).font(theme::semibold(13.0)).color(text_primary));

                            if total_codes > 1 {
                                ui.add_space(4.0);
                                ui.painter().rect_filled(
                                    Rect::from_min_size(ui.cursor().min, vec2(50.0, 18.0)),
                                    CornerRadius::same(4),
                                    chip_bg,
                                );
                                ui.label(
                                    egui::RichText::new(format!("{}/{}", state.active_index + 1, total_codes))
                                        .font(theme::medium(11.0))
                                        .color(text_muted),
                                );

                                // Multi-code navigation arrows: ◀ and ▶
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

                        ui.add_space(6.0);

                        // Payload display or edit mode
                        if let Some(buf) = &mut state.edit_buffer {
                            ui.label(egui::RichText::new("Edit Payload:").font(theme::medium(11.0)).color(text_muted));
                            ui.add_space(2.0);
                            ui.add(egui::TextEdit::singleline(buf).desired_width(ui.available_width()));
                            ui.add_space(8.0);

                            ui.horizontal(|ui| {
                                if ui.add_enabled(can_modify, egui::Button::new("Apply (100% Lock)")).clicked() {
                                    apply_code = Some(buf.clone());
                                }
                                if ui.button("Cancel").clicked() {
                                    cancel_edit = true;
                                }
                            });
                        } else {
                            // View mode: Monospace container + Copy / Edit actions
                            egui::Frame::NONE
                                .fill(Color32::from_rgb(0x0C, 0x0D, 0x12))
                                .stroke(Stroke::new(1.0, Color32::from_rgb(0x22, 0x25, 0x30)))
                                .corner_radius(CornerRadius::same(6))
                                .inner_margin(Margin::symmetric(10, 6))
                                .show(ui, |ui| {
                                    let display_text = if active.text.len() > 34 {
                                        format!("{}…", &active.text[..active.text.floor_char_boundary(34)])
                                    } else {
                                        active.text.clone()
                                    };
                                    ui.label(
                                        egui::RichText::new(display_text)
                                            .font(theme::medium(12.5))
                                            .monospace()
                                            .color(text_primary),
                                    );
                                });

                            ui.add_space(8.0);
                            ui.horizontal(|ui| {
                                if ui.button("📋 Copy Text").clicked() {
                                    copy_text = Some(active.text.clone());
                                }
                                if can_modify && ui.button("✏ Edit").clicked() {
                                    start_edit = true;
                                }

                                // Pixel dimensions & 100% lock badge
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    ui.label(
                                        egui::RichText::new(format!("{}×{}px • 100%", active.pixel_size[0], active.pixel_size[1]))
                                            .font(theme::medium(10.5))
                                            .color(Color32::from_rgb(0x38, 0xBD, 0xF8)),
                                    );
                                });
                            });
                        }

                        // Feedback message / toast
                        if let Some((msg, expiry)) = &state.toast {
                            if *expiry > now {
                                ui.add_space(4.0);
                                ui.label(
                                    egui::RichText::new(msg)
                                        .font(theme::medium(11.0))
                                        .color(Color32::from_rgb(0x34, 0xD3, 0x99)),
                                );
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
                        // Vector-rendered code replacement: place replacement raster at exact coordinates
                        pending_edit = Some(Edit::AddImage {
                            page,
                            rect: Some(active.rect),
                            name: "UpdatedCode.png".into(),
                            bytes: Arc::new(png_bytes),
                        });
                    }
                    state.edit_buffer = None;
                    state.show_toast("Code replaced with 100% pixel precision!".into(), now);
                }
                Err(e) => {
                    state.show_toast(format!("Replacement failed: {e}"), now);
                }
            }
        }
        if let Some(text) = copy_text {
            ui.ctx().copy_text(text);
            state.show_toast("Copied code payload to clipboard!".into(), now);
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

        pending_edit
    }

    /// Render the QR Code generation and Barcode scanner panel in the active viewport
    /// matching the exact modern dark styling of Sandeep PDF Studio v3.
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

        let current_page = view.current;
        let total_pages = info.pages.len();
        let viewport = view.viewport_rect();
        let mut pending_edit = None;
        let mut close_panel = false;
        let mut trigger_scan = false;
        let mut trigger_generate = false;
        let now = ui.input(|i| i.time);

        // Reactive Auto-Page Tracking:
        // Automatically triggers detection whenever the active page index changes via scrolling or arrow keys.
        if view.barcode_hud.last_scanned_page != Some(current_page) {
            trigger_scan = true;
        }

        let panel_w = 310.0;
        let panel_pos = Pos2::new(
            (viewport.right() - panel_w - 18.0).max(viewport.left() + 18.0),
            viewport.top() + 18.0,
        );

        // Sandeep PDF Studio v3 Palette
        let panel_bg = Color32::from_rgb(0x13, 0x14, 0x1A);
        let border_stroke = Stroke::new(1.0, Color32::from_rgb(0x2A, 0x2D, 0x3A));
        let card_bg = Color32::from_rgb(0x1B, 0x1D, 0x26);
        let card_stroke = Stroke::new(1.0, Color32::from_rgb(0x28, 0x2C, 0x3B));
        let text_primary = Color32::from_rgb(0xF8, 0xFA, 0xFC);
        let text_muted = Color32::from_rgb(0x94, 0xA3, 0xB8);
        let accent_blue = Color32::from_rgb(0x3B, 0x82, 0xF6);
        let emerald_green = Color32::from_rgb(0x10, 0xB9, 0x81);

        egui::Area::new(egui::Id::new(("barcode-qr-panel-v3", view.id.0)))
            .order(egui::Order::Foreground)
            .fixed_pos(panel_pos)
            .show(ui.ctx(), |ui| {
                egui::Frame::NONE
                    .fill(panel_bg)
                    .stroke(border_stroke)
                    .corner_radius(CornerRadius::same(14))
                    .shadow(egui::Shadow {
                        offset: [0, 8],
                        blur: 24,
                        spread: 0,
                        color: Color32::from_black_alpha(170),
                    })
                    .inner_margin(Margin::symmetric(14, 14))
                    .show(ui, |ui| {
                        ui.set_width(panel_w - 28.0);

                        // Header: Icon + Title + Sandeep PDF Studio v3 Tag + Close button
                        ui.horizontal(|ui| {
                            crate::icons::paint(
                                ui,
                                Rect::from_min_size(ui.cursor().min, vec2(18.0, 18.0)),
                                "scan",
                                18.0,
                                accent_blue,
                            );
                            ui.add_space(24.0);
                            ui.label(
                                egui::RichText::new("QR & Barcodes")
                                    .font(theme::semibold(14.0))
                                    .color(text_primary),
                            );

                            // v3.0 Version Chip
                            ui.painter().rect_filled(
                                Rect::from_min_size(ui.cursor().min + vec2(4.0, 2.0), vec2(28.0, 16.0)),
                                CornerRadius::same(4),
                                Color32::from_rgb(0x1D, 0x4E, 0xD8),
                            );
                            ui.add_space(8.0);
                            ui.label(
                                egui::RichText::new("v3")
                                    .font(theme::semibold(10.0))
                                    .color(Color32::WHITE),
                            );

                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui.small_button("✕").clicked() {
                                    close_panel = true;
                                }
                            });
                        });

                        ui.add_space(8.0);

                        // Section 1: Reactive Page Scanner Card
                        egui::Frame::NONE
                            .fill(card_bg)
                            .stroke(card_stroke)
                            .corner_radius(CornerRadius::same(10))
                            .inner_margin(Margin::symmetric(12, 10))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.label(
                                        egui::RichText::new("LIVE PAGE TRACKER")
                                            .font(theme::semibold(10.0))
                                            .color(text_muted),
                                    );
                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        ui.label(
                                            egui::RichText::new("● AUTO")
                                                .font(theme::semibold(10.0))
                                                .color(emerald_green),
                                        );
                                    });
                                });

                                ui.add_space(4.0);
                                ui.horizontal(|ui| {
                                    ui.label(
                                        egui::RichText::new(format!("Page {} of {}", current_page + 1, total_pages))
                                            .font(theme::semibold(12.5))
                                            .color(text_primary),
                                    );
                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        if ui.small_button("⟳ Rescan").clicked() {
                                            trigger_scan = true;
                                        }
                                    });
                                });

                                ui.add_space(6.0);

                                if !view.barcode_hud.scanned_codes.is_empty() {
                                    let count = view.barcode_hud.scanned_codes.len();
                                    ui.label(
                                        egui::RichText::new(format!("✓ {} Code(s) Detected", count))
                                            .font(theme::semibold(11.5))
                                            .color(emerald_green),
                                    );

                                    // Display list of detected codes on this page
                                    for (i, c) in view.barcode_hud.scanned_codes.iter().enumerate() {
                                        ui.add_space(4.0);
                                        let is_active = i == view.barcode_hud.active_index;
                                        let bg = if is_active { Color32::from_rgb(0x23, 0x28, 0x38) } else { Color32::from_rgb(0x13, 0x14, 0x1A) };
                                        let border = if is_active { Stroke::new(1.0, accent_blue) } else { Stroke::NONE };

                                        egui::Frame::NONE
                                            .fill(bg)
                                            .stroke(border)
                                            .corner_radius(CornerRadius::same(6))
                                            .inner_margin(Margin::symmetric(8, 6))
                                            .show(ui, |ui| {
                                                ui.horizontal(|ui| {
                                                    ui.label(
                                                        egui::RichText::new(format!("#{}: {}", i + 1, c.kind.label()))
                                                            .font(theme::semibold(11.0))
                                                            .color(text_primary),
                                                    );
                                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                                        if ui.small_button("Select").clicked() {
                                                            view.barcode_hud.active_index = i;
                                                            view.barcode_hud.open = true;
                                                        }
                                                    });
                                                });
                                                ui.label(
                                                    egui::RichText::new(&c.text)
                                                        .font(theme::regular(11.0))
                                                        .monospace()
                                                        .color(text_muted),
                                                );
                                            });
                                    }
                                } else {
                                    ui.label(
                                        egui::RichText::new("No barcodes or QR codes on this page")
                                            .font(theme::regular(11.0))
                                            .color(text_muted),
                                    );
                                }
                            });

                        ui.add_space(10.0);

                        // Section 2: QR Code Generator Card
                        egui::Frame::NONE
                            .fill(card_bg)
                            .stroke(card_stroke)
                            .corner_radius(CornerRadius::same(10))
                            .inner_margin(Margin::symmetric(12, 10))
                            .show(ui, |ui| {
                                ui.label(
                                    egui::RichText::new("GENERATE & EMBED QR")
                                        .font(theme::semibold(10.0))
                                        .color(text_muted),
                                );
                                ui.add_space(4.0);

                                ui.add(
                                    egui::TextEdit::singleline(&mut view.barcode_hud.qr_input)
                                        .desired_width(ui.available_width())
                                        .hint_text("https://example.com or payload..."),
                                );

                                ui.add_space(8.0);
                                let can_embed = can_modify && !view.barcode_hud.qr_input.trim().is_empty();
                                if ui.add_enabled(can_embed, egui::Button::new("⚡ Embed QR on Page")).clicked() {
                                    trigger_generate = true;
                                }
                            });

                        // Feedback Toast
                        if let Some((msg, expiry)) = &view.barcode_hud.toast {
                            if *expiry > now {
                                ui.add_space(6.0);
                                ui.label(
                                    egui::RichText::new(msg)
                                        .font(theme::medium(11.0))
                                        .color(emerald_green),
                                );
                            }
                        }
                    });
            });

        if close_panel {
            view.barcode_hud.panel_open = false;
        }

        // Automatic or manual scan trigger
        if trigger_scan {
            let codes = doc.scan_page_codes(view.current);
            view.barcode_hud.last_scanned_page = Some(view.current);
            if codes.is_empty() {
                view.barcode_hud.scanned_codes.clear();
                view.barcode_hud.open = false;
            } else {
                let count = codes.len();
                view.barcode_hud.update_codes(codes);
                view.barcode_hud.show_toast(format!("Auto-detected {count} code(s) on page {}", view.current + 1), now);
            }
        }

        if trigger_generate {
            let text = view.barcode_hud.qr_input.trim();
            match pdfcraft_engine::qr::QrCode::encode_text(text) {
                Ok(matrix) => {
                    match matrix.to_png_bytes(8, 2) {
                        Ok(png_bytes) => {
                            let page = view.current;
                            let (page_w, page_h) = info.pages.get(page)
                                .map(|p| (p.width as f64, p.height as f64))
                                .unwrap_or((612.0, 792.0));
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

