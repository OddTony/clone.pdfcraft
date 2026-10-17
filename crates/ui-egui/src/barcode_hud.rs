//! Interactive Barcode & QR Code Scanner HUD for PDF Craft (L7 UI).
//!
//! Provides automated detection, anchored HUD popups, multi-code pagination (< / >),
//! clipboard copy, and pixel-perfect 100% scale replacement directly onto canvas coordinates.

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

use std::sync::Arc;
use egui::{Color32, CornerRadius, Pos2, Stroke};
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
}
