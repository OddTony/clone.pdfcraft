//! Home tab: recommended tools, open card, pinned folders and recent files (local only, never
//! another app's list).

use egui::{Align2, CornerRadius, Rect, Sense, Stroke, vec2};
use pdfcraft_engine::catalog;

use crate::theme::{self, Tokens};
use crate::{PdfCraftApp, icons, panels::human_size, widgets};

const RECOMMENDED: [&str; 5] = ["organize", "comment", "form", "edit", "protect"];

/// A soft shadow that rises on hover and settles when the card is pressed.
fn card_lift(ui: &egui::Ui, t: &Tokens, rect: Rect, press: theme::Press) {
    let lift = press.hover * (1.0 - press.down);
    if lift > 0.04 {
        let shadow = egui::Shadow { offset: [0, 3], blur: (14.0 * lift) as u8, spread: 0, color: t.page_shadow.gamma_multiply(0.85 * lift) };
        ui.painter().add(shadow.as_shape(rect, CornerRadius::same(10)));
    }
}

/// One recommended tool: its coloured icon and name in a compact button. What it does shows on
/// hover, so the screen carries names only.
fn tool_chip(ui: &mut egui::Ui, t: &Tokens, g: &catalog::ToolGroup) -> egui::Response {
    let label = tl!(g.label);
    let font = theme::medium(13.0);
    let w = ui.fonts_mut(|f| f.layout_no_wrap(label.to_owned(), font.clone(), t.text).size().x);
    let (rect, resp) = ui.allocate_exact_size(vec2(w + 46.0, 34.0), Sense::click());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label));
    let press = theme::Press::track(ui, &resp);
    ui.painter().rect(rect, CornerRadius::same(8), press.fill(t.card, t.hover, t.pressed), Stroke::new(1.0, t.divider), egui::StrokeKind::Inside);
    let body = rect.translate(press.offset());
    let color = egui::Color32::from_rgb(g.hue[0], g.hue[1], g.hue[2]);
    icons::paint(ui, Rect::from_min_size(body.min + vec2(12.0, 8.0), vec2(18.0, 18.0)), g.icon, 17.0, color);
    ui.painter().text(body.left_center() + vec2(36.0, 0.0), Align2::LEFT_CENTER, label, font, t.text);
    let blurb = g.sections.first().map(|s| s.items.iter().take(3).map(|i| tl!(i.label)).collect::<Vec<_>>().join(" · ")).unwrap_or_default();
    theme::hand(resp).on_hover_text(blurb)
}

/// A section heading with its actions on the right.
fn heading(ui: &mut egui::Ui, text: &str, actions: impl FnOnce(&mut egui::Ui)) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(text).font(theme::semibold(15.0)));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), actions);
    });
}

/// Compact Home (Preferences ▸ Appearance): the title with Open file, the recommended tools as
/// compact buttons (what each does shows on hover), pinned folders when there are some, and the
/// recent files one line each.
fn show_compact(app: &mut PdfCraftApp, ui: &mut egui::Ui) {
    let t = Tokens::get(ui.ctx());
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        egui::Frame::NONE.inner_margin(egui::Margin { left: 40, right: 40, top: 32, bottom: 32 }).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(tl!("Welcome to PdfCraft")).font(theme::semibold(22.0)));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if widgets::icon_pill(ui, "folder-open", tl!("Open file"), true).clicked() {
                        app.open_dialog();
                    }
                });
            });
            ui.add_space(18.0);
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = vec2(8.0, 8.0);
                for id in RECOMMENDED {
                    if let Some(g) = catalog::group(id)
                        && tool_chip(ui, &t, g).clicked()
                    {
                        app.select_tool(g.id);
                    }
                }
            });

            #[cfg(not(target_arch = "wasm32"))]
            if !app.pinned.folders.is_empty() {
                ui.add_space(32.0);
                crate::folders_ui::section(app, ui);
            }

            ui.add_space(32.0);
            let mut clear = false;
            #[cfg_attr(target_arch = "wasm32", expect(unused_mut))]
            let mut pin = false;
            heading(ui, tl!("Recent"), |ui| {
                if !app.recent.is_empty() {
                    let resp = widgets::ghost_button(ui, "trash-2", tl!("Clear"));
                    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, tl!("Clear Recent Files")));
                    clear = resp.on_hover_text(tl!("Clear Recent Files")).clicked();
                }
                // With no folder pinned yet, pinning one starts here instead of in an empty section.
                #[cfg(not(target_arch = "wasm32"))]
                if app.pinned.folders.is_empty() {
                    pin = widgets::ghost_button(ui, "folder-plus", tl!("Pin a folder…")).clicked();
                }
            });
            #[cfg(not(target_arch = "wasm32"))]
            if pin {
                app.pin_folder_dialog();
            }
            ui.add_space(6.0);
            if app.recent.is_empty() {
                ui.label(egui::RichText::new(tl!("Files you open in PdfCraft appear here. Drop a PDF anywhere to open it.")).color(t.text_muted));
            }
            let mut open = None;
            let mut remove = None;
            for r in &app.recent {
                let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 36.0), Sense::click());
                resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &r.name));
                let press = theme::Press::track(ui, &resp);
                // The row, not its response: the pointer stays "in" the row over its remove button.
                let hovered = ui.rect_contains_pointer(rect);
                if hovered && press.hover < 0.04 && press.down < 0.04 {
                    // The remove button sits in the row, so the row response may not be hovered.
                    ui.painter().rect_filled(rect, CornerRadius::same(8), t.hover);
                } else {
                    press.wash(ui, rect, 8, false);
                }
                if hovered {
                    // Remove just this file from the list (#430); the file itself is untouched.
                    let x_rect = Rect::from_center_size(rect.right_center() - vec2(22.0, 0.0), vec2(26.0, 26.0));
                    let x = ui.interact(x_rect, ui.id().with(("remove-recent", &r.path)), Sense::click());
                    let label = crate::i18n::fmt(tl!("Remove {name} from Recent"), &[("name", &r.name)]);
                    x.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &label));
                    if x.hovered() {
                        ui.painter().rect_filled(x_rect, CornerRadius::same(6), t.pressed);
                    }
                    icons::paint(ui, x_rect.shrink(6.0), "x", 14.0, t.icon);
                    if x.on_hover_text(tl!("Remove from Recent")).clicked() {
                        remove = Some(r.path.clone());
                    }
                }
                let body = rect.translate(press.offset());
                icons::paint(
                    ui,
                    Rect::from_min_size(body.min + vec2(10.0, 9.0), vec2(18.0, 18.0)),
                    "file-text",
                    17.0,
                    egui::Color32::from_rgb(0xE0, 0x3E, 0x3E),
                );
                // Room on the right for the remove button; the full path shows on hover.
                let detail = ui.painter().text(
                    body.right_center() - vec2(44.0, 0.0),
                    Align2::RIGHT_CENTER,
                    format!("{}  ·  {}", crate::i18n::trn(crate::i18n::current(), r.pages as u64, "{n} page", "{n} pages"), human_size(r.size)),
                    theme::regular(12.0),
                    t.text_faint,
                );
                let width = detail.left() - 16.0 - (body.left() + 38.0);
                widgets::row_text(ui, body.min + vec2(38.0, 18.0), crate::bidi::visual(&r.name), theme::regular(13.0), t.text, width);
                if theme::hand(resp).on_hover_text(&r.path).clicked() {
                    open = Some(r.path.clone());
                }
            }
            if let Some(p) = open {
                app.open_recent(&p);
            }
            if let Some(p) = remove {
                app.recent.retain(|r| r.path != p);
            }
            if clear {
                app.execute("file.clear_recent");
            }
        });
    });
}

pub fn show(app: &mut PdfCraftApp, ui: &mut egui::Ui) {
    if app.compact_home {
        return show_compact(app, ui);
    }
    let t = Tokens::get(ui.ctx());
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
        egui::Frame::NONE.inner_margin(egui::Margin { left: 36, right: 36, top: 28, bottom: 28 }).show(ui, |ui| {
            ui.label(egui::RichText::new(tl!("Welcome to PdfCraft")).font(theme::semibold(24.0)));
            ui.label(
                egui::RichText::new(tl!("An open-source PDF workbench — local, private, and scriptable."))
                    .color(t.text_muted)
                    .font(theme::regular(14.0)),
            );
            ui.add_space(14.0);
            egui::Frame::NONE
                .fill(t.card)
                .stroke(Stroke::new(1.0, t.border))
                .corner_radius(CornerRadius::same(12))
                .inner_margin(egui::Margin::same(14))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        widgets::artcraft_mark(ui, 28.0);
                        ui.vertical(|ui| {
                            ui.label(egui::RichText::new(tl!("Join the ArtCraft community")).font(theme::semibold(15.0)));
                            ui.label(egui::RichText::new(tl!("Get help, share feedback and follow development on Discord.")).color(t.text_muted));
                        });
                    });
                    ui.add_space(8.0);
                    if let Some(cmd) = widgets::community_links(ui) {
                        app.execute(cmd);
                    }
                });
            ui.add_space(22.0);

            egui::Frame::NONE
                .fill(t.card)
                .stroke(Stroke::new(1.0, t.border))
                .corner_radius(CornerRadius::same(12))
                .inner_margin(egui::Margin::same(18))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(egui::RichText::new(tl!("Recommended tools")).font(theme::semibold(15.0)));
                    ui.add_space(10.0);
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = vec2(14.0, 14.0);
                        for id in RECOMMENDED {
                            let Some(g) = catalog::group(id) else { continue };
                            let (rect, resp) = ui.allocate_exact_size(vec2(190.0, 104.0), Sense::click());
                            resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, tl!(g.label)));
                            let press = theme::Press::track(ui, &resp);
                            card_lift(ui, &t, rect, press);
                            let fill = press.fill(t.card, t.hover, t.pressed);
                            ui.painter().rect(rect, CornerRadius::same(10), fill, Stroke::new(1.0, t.divider), egui::StrokeKind::Inside);
                            let body = rect.translate(press.offset());
                            let color = egui::Color32::from_rgb(g.hue[0], g.hue[1], g.hue[2]);
                            icons::paint(ui, Rect::from_min_size(body.min + vec2(14.0, 14.0), vec2(22.0, 22.0)), g.icon, 21.0, color);
                            let mut title_job = egui::text::LayoutJob::simple_singleline(tl!(g.label).to_owned(), theme::semibold(13.5), t.text);
                            title_job.wrap =
                                egui::text::TextWrapping { max_width: body.width() - 58.0, max_rows: 1, break_anywhere: true, ..Default::default() };
                            let title = ui.fonts_mut(|f| f.layout_job(title_job));
                            let title_elided = title.elided;
                            ui.painter().galley(body.min + vec2(44.0, 25.0 - title.size().y * 0.5), title, t.text);
                            let blurb = g
                                .sections
                                .first()
                                .map(|s| s.items.iter().take(3).map(|i| tl!(i.label)).collect::<Vec<_>>().join(" · "))
                                .unwrap_or_default();
                            // Fixed-height cards must leave room for their action in every language.
                            let mut job = egui::text::LayoutJob::simple(blurb.clone(), theme::regular(11.5), t.text_muted, body.width() - 28.0);
                            job.wrap.max_rows = 2;
                            let galley = ui.fonts_mut(|f| f.layout_job(job));
                            let blurb_elided = galley.elided;
                            ui.painter().galley(body.min + vec2(14.0, 46.0), galley, t.text_muted);
                            ui.painter().text(
                                body.left_bottom() + vec2(14.0, -14.0),
                                Align2::LEFT_CENTER,
                                tl!("Use now"),
                                theme::medium(12.0),
                                t.accent_text,
                            );
                            let resp = if title_elided || blurb_elided { resp.on_hover_text(format!("{}\n{blurb}", tl!(g.label))) } else { resp };
                            if theme::hand(resp).clicked() {
                                app.select_tool(g.id);
                            }
                        }
                        let (rect, resp) = ui.allocate_exact_size(vec2(170.0, 104.0), Sense::click());
                        resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, tl!("Open file")));
                        let press = theme::Press::track(ui, &resp);
                        card_lift(ui, &t, rect, press);
                        ui.painter().rect(
                            rect,
                            CornerRadius::same(10),
                            press.fill(t.pasteboard, t.hover, t.pressed),
                            Stroke::new(1.0, t.divider),
                            egui::StrokeKind::Inside,
                        );
                        let body = rect.translate(press.offset());
                        icons::paint(ui, Rect::from_center_size(body.center() - vec2(0.0, 16.0), vec2(28.0, 28.0)), "folder-open", 26.0, t.icon);
                        ui.painter().text(body.center() + vec2(0.0, 22.0), Align2::CENTER_CENTER, tl!("Open file"), theme::semibold(13.0), t.text);
                        if theme::hand(resp).clicked() {
                            app.open_dialog();
                        }
                    });
                });

            #[cfg(not(target_arch = "wasm32"))]
            {
                ui.add_space(26.0);
                crate::folders_ui::section(app, ui);
            }

            ui.add_space(26.0);
            let mut clear = false;
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(tl!("Recent")).font(theme::semibold(17.0)));
                if !app.recent.is_empty() {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let resp = widgets::ghost_button(ui, "trash-2", tl!("Clear"));
                        resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, tl!("Clear Recent Files")));
                        clear = resp.on_hover_text(tl!("Clear Recent Files")).clicked();
                    });
                }
            });
            ui.add_space(8.0);
            if app.recent.is_empty() {
                ui.label(egui::RichText::new(tl!("Files you open in PdfCraft appear here. Drop a PDF anywhere to open it.")).color(t.text_muted));
            }
            let mut open = None;
            let mut remove = None;
            for r in &app.recent {
                let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 46.0), Sense::click());
                resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &r.name));
                let press = theme::Press::track(ui, &resp);
                // The row, not its response: the pointer stays "in" the row over its remove button.
                let hovered = ui.rect_contains_pointer(rect);
                if hovered && press.hover < 0.04 && press.down < 0.04 {
                    // The remove button sits in the row, so the row response may not be hovered.
                    ui.painter().rect_filled(rect, CornerRadius::same(8), t.hover);
                } else {
                    press.wash(ui, rect, 8, false);
                }
                if hovered {
                    // Remove just this file from the list (#430); the file itself is untouched.
                    let x_rect = Rect::from_center_size(rect.right_center() - vec2(26.0, 0.0), vec2(28.0, 28.0));
                    let x = ui.interact(x_rect, ui.id().with(("remove-recent", &r.path)), Sense::click());
                    let label = crate::i18n::fmt(tl!("Remove {name} from Recent"), &[("name", &r.name)]);
                    x.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &label));
                    if x.hovered() {
                        ui.painter().rect_filled(x_rect, CornerRadius::same(6), t.pressed);
                    }
                    icons::paint(ui, x_rect.shrink(6.0), "x", 16.0, t.icon);
                    if x.on_hover_text(tl!("Remove from Recent")).clicked() {
                        remove = Some(r.path.clone());
                    }
                }
                let body = rect.translate(press.offset());
                icons::paint(
                    ui,
                    Rect::from_min_size(body.min + vec2(10.0, 11.0), vec2(24.0, 24.0)),
                    "file-text",
                    22.0,
                    egui::Color32::from_rgb(0xE0, 0x3E, 0x3E),
                );
                // Room on the right for the remove button.
                let detail = ui.painter().text(
                    body.right_center() - vec2(48.0, 0.0),
                    Align2::RIGHT_CENTER,
                    format!("{}  ·  {}", crate::i18n::trn(crate::i18n::current(), r.pages as u64, "{n} page", "{n} pages"), human_size(r.size)),
                    theme::regular(12.0),
                    t.text_muted,
                );
                // The name and path stop short of the page count; hovering shows the whole path.
                let width = detail.left() - 16.0 - (body.left() + 46.0);
                widgets::row_text(ui, body.min + vec2(46.0, 15.0), crate::bidi::visual(&r.name), theme::medium(13.5), t.text, width);
                widgets::row_text(ui, body.min + vec2(46.0, 32.0), crate::bidi::visual(&r.path), theme::regular(11.0), t.text_faint, width);
                if theme::hand(resp).on_hover_text(&r.path).clicked() {
                    open = Some(r.path.clone());
                }
            }
            if let Some(p) = open {
                app.open_recent(&p);
            }
            if let Some(p) = remove {
                app.recent.retain(|r| r.path != p);
            }
            if clear {
                app.execute("file.clear_recent");
            }
            ui.add_space(20.0);
            widgets::section_title(ui, tl!("Privacy"));
            ui.label(
                egui::RichText::new(tl!("PdfCraft works offline. No telemetry, no account, and no cloud processing unless you add a provider."))
                    .color(t.text_muted),
            );
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn translated_card_description_leaves_room_for_the_action() {
        let de = crate::i18n::Lang::from_code("de").unwrap();
        crate::i18n::set_current(de);
        let ctx = egui::Context::default();
        ctx.set_fonts(theme::font_definitions());
        let mut app = PdfCraftApp::new();
        let blurb = catalog::group("form")
            .unwrap()
            .sections
            .first()
            .unwrap()
            .items
            .iter()
            .take(3)
            .map(|item| crate::i18n::tr(de, item.label))
            .collect::<Vec<_>>()
            .join(" · ");
        let output = ctx
            .run_ui(egui::RawInput { screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(1440.0, 900.0))), ..Default::default() }, |ui| {
                show(&mut app, ui)
            });
        crate::i18n::set_current(crate::i18n::Lang::EN);
        let text_shapes: Vec<_> =
            output.shapes.iter().filter_map(|shape| if let egui::Shape::Text(text) = &shape.shape { Some(text) } else { None }).collect();
        let description = text_shapes.iter().find(|text| text.galley.job.text == blurb).unwrap();
        let action = text_shapes
            .iter()
            .find(|text| text.galley.job.text == crate::i18n::tr(de, "Use now") && (text.pos.x - description.pos.x).abs() < 0.1)
            .unwrap();
        assert!(description.galley.rows.len() <= 2);
        assert!(
            description.pos.y + description.galley.size().y + 4.0 <= action.pos.y,
            "the translated description must leave a visible gap before its action"
        );
        output.drop_without_applying_deltas();
    }

    #[test]
    fn compact_home_shows_names_and_actions_only() {
        let ctx = egui::Context::default();
        ctx.set_fonts(theme::font_definitions());
        let mut app = PdfCraftApp::new();
        app.compact_home = true;
        let output = ctx
            .run_ui(egui::RawInput { screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(1440.0, 900.0))), ..Default::default() }, |ui| {
                show(&mut app, ui)
            });
        let texts: Vec<String> = output
            .shapes
            .iter()
            .filter_map(|shape| if let egui::Shape::Text(text) = &shape.shape { Some(text.galley.job.text.clone()) } else { None })
            .collect();
        for id in RECOMMENDED {
            let label = catalog::group(id).unwrap().label;
            assert!(texts.iter().any(|t| t == label), "{label} on Home");
        }
        for gone in ["Use now", "Join the ArtCraft community", "Privacy", "An open-source PDF workbench — local, private, and scriptable."] {
            assert!(!texts.iter().any(|t| t.contains(gone)), "{gone} left Home");
        }
        output.drop_without_applying_deltas();
    }
}
