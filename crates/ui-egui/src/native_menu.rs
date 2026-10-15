//! Desktop menu state and dispatch, without native toolkit dependencies. The desktop host
//! owns the menu objects; commands keep the same permissions, prompts and form-edit timing
//! as keyboard shortcuts. Public so a host (and headless tests) can use the same bridge.

use egui::{Context, Event, Key, Modifiers, RawInput};
use pdfcraft_engine::commands::{self, Shortcut};

use crate::{PdfCraftApp, i18n};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct State {
    pub label: String,
    pub enabled: bool,
    pub checked: Option<bool>,
}

/// Registry shortcuts plus the clipboard events egui normally receives from its backend.
pub fn shortcut(id: &str) -> Option<Shortcut> {
    match id {
        "edit.cut" => Some(Shortcut::cmd("X")),
        "edit.copy" => Some(Shortcut::cmd("C")),
        "edit.paste" => Some(Shortcut::cmd("V")),
        "edit.select_all" => Some(Shortcut::cmd("A")),
        _ => commands::command(id).and_then(|s| s.shortcut),
    }
}

impl PdfCraftApp {
    pub fn native_menu_state(&self, ctx: &Context, id: &str) -> State {
        let spec = commands::command(id);
        let view = self.active.and_then(|i| self.views.get(i));
        let organize = view.is_some_and(|v| v.organize);
        let text = ctx.text_edit_focused();
        let label = match id {
            "edit.cut" => "Cut".into(),
            "edit.paste" => "Paste".into(),
            "edit.select_all" => "Select all".into(),
            "file.close" if self.active_ids().is_none() => "Close window".into(),
            "edit.undo" | "edit.redo" if text => spec.map(|s| s.label.to_owned()).unwrap_or_default(),
            _ => spec.map(|s| commands::current_label(s, &self.session, self.active_ids().map(|(_, id)| id))).unwrap_or_default(),
        };
        let editing_text = text && matches!(id, "edit.cut" | "edit.copy" | "edit.paste" | "edit.select_all" | "edit.undo" | "edit.redo");
        let blocked = self.modal_open() || self.palette_open;
        let enabled = editing_text
            || (!blocked
                && match id {
                    "file.close" => true,
                    "file.open_recent" => !self.recent.is_empty(),
                    "edit.select_all" => self.active_ids().is_some(),
                    "edit.cut" => organize && commands::command("page.cut").is_some_and(|s| self.command_enabled(s)),
                    "edit.paste" => {
                        organize && self.page_clipboard.is_some() && commands::command("page.paste").is_some_and(|s| self.command_enabled(s))
                    }
                    "edit.copy" if organize => commands::command("page.copy").is_some_and(|s| self.command_enabled(s)),
                    "edit.copy" if view.is_some_and(|v| v.picked_image.is_some()) => {
                        self.active_ids().and_then(|(_, id)| self.session.get(id)).is_some_and(|d| d.allows_copying())
                    }
                    _ => spec.is_some_and(|s| self.command_enabled(s)),
                });
        let checked = match id {
            "view.layout.single" => Some(view.is_some_and(|v| v.layout == crate::canvas::PageLayout::Single)),
            "view.layout.two_up" => Some(view.is_some_and(|v| v.layout == crate::canvas::PageLayout::TwoUp)),
            "view.continuous" => Some(view.is_some_and(|v| v.continuous())),
            "view.layout.cover" => Some(view.is_some_and(|v| v.cover)),
            "view.full_screen" => Some(self.full_screen),
            "view.read_mode" => Some(self.mode == crate::Mode::Read),
            "view.theme.system" => Some(self.theme_preference == crate::theme::ThemePreference::System),
            "view.theme.light" => Some(self.theme_preference == crate::theme::ThemePreference::Light),
            "view.theme.dark" => Some(self.theme_preference == crate::theme::ThemePreference::Dark),
            _ => None,
        };
        let label = if id == "file.close" && self.active_ids().is_none() { i18n::t(&label).to_owned() } else { i18n::menu_label(id, &label) };
        State { label, enabled, checked }
    }

    /// Deliver editing events before egui builds the frame, so a focused editor retains its
    /// selection and local undo history. Returns whether the event is handled (or disabled).
    pub fn native_menu_input(&self, ctx: &Context, raw: &mut RawInput, id: &str, clipboard: impl FnOnce() -> String) -> bool {
        if !self.native_menu_state(ctx, id).enabled {
            return true;
        }
        if !ctx.text_edit_focused() {
            return false;
        }
        let event = match id {
            "edit.copy" => Some(Event::Copy),
            "edit.cut" => Some(Event::Cut),
            "edit.paste" => Some(Event::Paste(clipboard())),
            _ => None,
        };
        if let Some(event) = event {
            raw.events.push(event);
            return true;
        }
        if id == "edit.select_all" || (ctx.text_edit_focused() && matches!(id, "edit.undo" | "edit.redo")) {
            let (key, modifiers) = match id {
                "edit.select_all" => (Key::A, Modifiers::COMMAND),
                "edit.redo" => (Key::Z, Modifiers::COMMAND | Modifiers::SHIFT),
                _ => (Key::Z, Modifiers::COMMAND),
            };
            for pressed in [true, false] {
                raw.events.push(Event::Key { key, physical_key: None, pressed, repeat: false, modifiers });
            }
            return true;
        }
        false
    }

    /// Revalidate after logic (a dialog may have opened since the menu was displayed).
    pub fn native_menu_command(&mut self, ctx: &Context, id: &str) {
        if !self.native_menu_state(ctx, id).enabled {
            return;
        }
        if let Some((index, doc)) = self.active_ids() {
            let organize = self.views.get(index).is_some_and(|v| v.organize);
            match id {
                "edit.select_all" => {
                    if self.is_preparing() {
                        if let (Some(view), Some(document)) = (self.views.get_mut(index), self.session.get(doc)) {
                            crate::prepare::select_all(view, &document.form);
                        }
                    } else if let Some(view) = self.views.get_mut(index) {
                        view.select_all();
                    }
                    return;
                }
                "edit.copy" | "edit.cut" | "edit.paste" if organize => {
                    self.execute(match id {
                        "edit.cut" => "page.cut",
                        "edit.paste" => "page.paste",
                        _ => "page.copy",
                    });
                    return;
                }
                "edit.copy" => {
                    if let Some(picked) = self.views.get(index).filter(|v| v.selected_text().is_none()).and_then(|v| v.picked_image) {
                        self.copy_page_image_file(picked.page, picked.index);
                    } else {
                        self.execute(id);
                    }
                    return;
                }
                _ => {}
            }
        }
        self.dispatch_menu_command(ctx, id);
    }

    /// Shared by native menus and keyboard shortcuts. A form draft must get this frame's
    /// typing before a save, print or tab change reads it; keep the existing next-frame queue.
    pub(crate) fn dispatch_menu_command(&mut self, ctx: &Context, id: &str) {
        let active = self.active_ids();
        let form_typing = active.and_then(|(i, _)| self.views.get(i)).is_some_and(|v| v.forms.focus.is_some());
        if form_typing {
            if let Some(spec) = commands::command(id) {
                self.deferred_commands.push((spec.id, active.map(|(_, id)| id)));
                ctx.request_repaint();
            }
        } else if id == "file.close" && active.is_none() && crate::commands::mac_shortcuts(ctx) && !cfg!(target_arch = "wasm32") {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        } else {
            self.execute(id);
        }
    }
}
