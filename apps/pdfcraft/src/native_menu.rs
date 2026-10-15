//! Main-thread macOS menu adapter. All document actions stay in the frontend's command path.

use std::sync::mpsc::{self, Receiver};

use egui::{Context, RawInput};
use muda::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu, accelerator::KeyAccelerator};
use pdfcraft_engine::commands;
use pdfcraft_ui_egui::{PdfCraftApp, i18n, native_menu};

type Error = Box<dyn std::error::Error + Send + Sync>;

enum Item {
    Normal(MenuItem),
    Check(CheckMenuItem),
}
struct Entry {
    id: &'static str,
    item: Item,
    state: native_menu::State,
}

pub struct DesktopApp {
    app: PdfCraftApp,
    menu: NativeMenu,
}

impl DesktopApp {
    pub fn new(app: PdfCraftApp, ctx: &Context) -> Result<Self, Error> {
        i18n::set_current(i18n::Lang::from_pref(&app.language));
        let menu = NativeMenu::install(ctx, &app)?;
        Ok(Self { app, menu })
    }
}

impl eframe::App for DesktopApp {
    fn raw_input_hook(&mut self, ctx: &Context, raw: &mut RawInput) {
        self.app.raw_input_hook(ctx, raw);
        self.menu.input(ctx, raw, &self.app);
    }
    fn logic(&mut self, ctx: &Context, frame: &mut eframe::Frame) {
        self.app.logic(ctx, frame);
        // Runs while hidden/minimized too, like the frontend's existing quit guard.
        for action in self.menu.pending.drain(..) {
            match action {
                Action::Command(id) => self.app.native_menu_command(ctx, &id),
                Action::Recent(path) if self.app.native_menu_state(ctx, "file.open").enabled => self.app.open_recent(&path),
                Action::Recent(_) => {}
            }
        }
        self.menu.sync(ctx, &self.app);
    }
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        self.app.ui(ui, frame);
        self.menu.sync(ui.ctx(), &self.app);
    }
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        self.app.save(storage);
    }
}

enum Action {
    Command(String),
    Recent(String),
}
struct NativeMenu {
    _menu: Menu,
    entries: Vec<Entry>,
    submenus: Vec<(&'static str, Submenu)>,
    recent: Submenu,
    recent_paths: Vec<(String, String)>,
    recent_language: i18n::Lang,
    events: Receiver<MenuEvent>,
    pending: Vec<Action>,
}

impl NativeMenu {
    fn install(ctx: &Context, app: &PdfCraftApp) -> Result<Self, Error> {
        let menu = Menu::new();
        let mut entries = Vec::new();
        let app_menu = Submenu::new("PdfCraft", true);
        add(&app_menu, &mut entries, ctx, app, "help.about")?;
        app_menu.append(&PredefinedMenuItem::separator())?;
        add(&app_menu, &mut entries, ctx, app, "app.preferences")?;
        app_menu.append(&PredefinedMenuItem::separator())?;
        app_menu.append(&PredefinedMenuItem::services(None))?;
        app_menu.append(&PredefinedMenuItem::separator())?;
        app_menu.append(&PredefinedMenuItem::hide(Some("Hide PdfCraft")))?;
        app_menu.append(&PredefinedMenuItem::hide_others(None))?;
        app_menu.append(&PredefinedMenuItem::show_all(None))?;
        app_menu.append(&PredefinedMenuItem::separator())?;
        let quit = MenuItem::with_id(craft_quit::QUIT_MENU_ID, "Quit PdfCraft", true, None);
        quit.set_key_accelerator(Some("Cmd+Q".parse::<KeyAccelerator>()?))?;
        app_menu.append(&quit)?;
        menu.append(&app_menu)?;
        let recent = Submenu::new(i18n::t("Open Recent"), false);
        let mut submenus = Vec::new();
        for name in ["File", "Edit", "View", "Pages", "Window", "Help"] {
            let submenu = Submenu::new(i18n::t(name), true);
            for spec in commands::menu(name) {
                if matches!(spec.id, "help.about" | "app.preferences" | "edit.copy") {
                    continue;
                }
                if spec.id == "file.open_recent" {
                    submenu.append(&recent)?;
                } else {
                    add(&submenu, &mut entries, ctx, app, spec.id)?;
                }
                if spec.id == "edit.redo" {
                    submenu.append(&PredefinedMenuItem::separator())?;
                    // Clipboard actions are semantic input events, including focused text editing.
                    for id in ["edit.cut", "edit.copy", "edit.paste", "edit.select_all"] {
                        add(&submenu, &mut entries, ctx, app, id)?;
                    }
                    submenu.append(&PredefinedMenuItem::separator())?;
                }
            }
            if name == "View" {
                submenu.append(&PredefinedMenuItem::separator())?;
                for id in [
                    "view.layout.single",
                    "view.layout.two_up",
                    "view.continuous",
                    "view.layout.cover",
                    "view.fit_visible",
                    "view.previous_view",
                    "view.next_view",
                ] {
                    add(&submenu, &mut entries, ctx, app, id)?;
                }
                let theme = Submenu::new(i18n::t("Display theme"), true);
                for id in ["view.theme.system", "view.theme.light", "view.theme.dark"] {
                    add(&theme, &mut entries, ctx, app, id)?;
                }
                submenu.append(&theme)?;
                submenus.push(("Display theme", theme));
            }
            if name == "Window" {
                submenu.append_items(&[
                    &PredefinedMenuItem::minimize(None),
                    &PredefinedMenuItem::maximize(Some("Zoom")),
                    &PredefinedMenuItem::fullscreen(None),
                    &PredefinedMenuItem::separator(),
                    &PredefinedMenuItem::bring_all_to_front(None),
                ])?;
            }
            menu.append(&submenu)?;
            submenus.push((name, submenu));
        }
        let (tx, events) = mpsc::channel();
        let wake = ctx.clone();
        MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
            // Preserve craft-quit's safe close request and platform termination handling.
            if !craft_quit::on_menu_event(&wake, event.id.as_ref()) {
                let _ = tx.send(event);
            }
            wake.request_repaint();
        }));
        menu.init_for_nsapp();
        for (name, submenu) in &submenus {
            match *name {
                "Window" => submenu.set_as_windows_menu_for_nsapp(),
                "Help" => submenu.set_as_help_menu_for_nsapp(),
                _ => {}
            }
        }
        let mut this =
            Self { _menu: menu, entries, submenus, recent, recent_paths: Vec::new(), recent_language: i18n::current(), events, pending: Vec::new() };
        this.sync(ctx, app);
        Ok(this)
    }

    fn input(&mut self, ctx: &Context, raw: &mut RawInput, app: &PdfCraftApp) {
        while let Ok(event) = self.events.try_recv() {
            let id = event.id.0;
            if let Some(index) = id.strip_prefix("recent:").and_then(|s| s.parse::<usize>().ok()) {
                if let Some((_, path)) = self.recent_paths.get(index) {
                    self.pending.push(Action::Recent(path.clone()));
                }
                continue;
            }
            // Some backends also forward the accelerator: let one activation run once.
            if let Some(s) = native_menu::shortcut(&id) {
                raw.events.retain(|event| {
                    !matches!(event, egui::Event::Key { key, modifiers, .. }
                    if Some(*key) == egui::Key::from_name(s.key)
                        && modifiers.command == s.command && modifiers.shift == s.shift
                        && modifiers.ctrl == s.mac_ctrl && !modifiers.alt)
                });
            }
            raw.events.retain(|event| {
                !matches!(
                    (id.as_str(), event),
                    ("edit.copy", egui::Event::Copy) | ("edit.cut", egui::Event::Cut) | ("edit.paste", egui::Event::Paste(_))
                )
            });
            if !app.native_menu_input(ctx, raw, &id, || arboard::Clipboard::new().and_then(|mut c| c.get_text()).unwrap_or_default()) {
                self.pending.push(Action::Command(id));
            }
        }
    }

    fn sync(&mut self, ctx: &Context, app: &PdfCraftApp) {
        for (name, submenu) in &self.submenus {
            let label = i18n::t(name);
            if submenu.text() != label {
                submenu.set_text(label);
            }
        }
        for entry in &mut self.entries {
            let state = app.native_menu_state(ctx, entry.id);
            // AppKit toggles check items before dispatch, even when a selected radio-like
            // choice or a rejected command leaves our state unchanged.
            if let Item::Check(item) = &entry.item
                && item.is_checked() != state.checked.unwrap_or(false)
            {
                item.set_checked(state.checked.unwrap_or(false));
            }
            if state != entry.state {
                match &entry.item {
                    Item::Normal(item) => {
                        item.set_text(&state.label);
                        item.set_enabled(state.enabled);
                    }
                    Item::Check(item) => {
                        item.set_text(&state.label);
                        item.set_enabled(state.enabled);
                    }
                }
                entry.state = state;
            }
        }
        let paths: Vec<_> = app.recent.iter().map(|r| (r.name.clone(), r.path.clone())).collect();
        if self.recent_paths != paths || self.recent_language != i18n::current() {
            while self.recent.remove_at(0).is_some() {}
            for (i, (name, _)) in paths.iter().enumerate() {
                if let Err(e) = self.recent.append(&MenuItem::with_id(format!("recent:{i}"), name, true, None)) {
                    log::warn!("couldn't add recent file: {e}");
                }
            }
            let clear = MenuItem::with_id("file.clear_recent", i18n::t("Clear Recent Files"), true, None);
            if let Err(e) = self.recent.append_items(&[&PredefinedMenuItem::separator(), &clear]) {
                log::warn!("couldn't update recent menu: {e}");
            }
            self.recent_paths = paths;
            self.recent_language = i18n::current();
        }
        let state = app.native_menu_state(ctx, "file.open_recent");
        self.recent.set_text(state.label);
        self.recent.set_enabled(state.enabled);
    }
}

fn accelerator(id: &str) -> Result<Option<KeyAccelerator>, muda::accelerator::AcceleratorParseError> {
    native_menu::shortcut(id)
        .map(|s| {
            let mut key = String::new();
            if s.mac_ctrl {
                key.push_str("Ctrl+");
            }
            if s.command {
                key.push_str("Cmd+");
            }
            if s.shift {
                key.push_str("Shift+");
            }
            key.push_str(s.key);
            key.parse()
        })
        .transpose()
}

fn add(parent: &Submenu, entries: &mut Vec<Entry>, ctx: &Context, app: &PdfCraftApp, id: &'static str) -> Result<(), Error> {
    let state = app.native_menu_state(ctx, id);
    let item = if let Some(checked) = state.checked {
        let item = CheckMenuItem::with_id(id, &state.label, state.enabled, checked, None);
        item.set_key_accelerator(accelerator(id)?)?;
        parent.append(&item)?;
        Item::Check(item)
    } else {
        let item = MenuItem::with_id(id, &state.label, state.enabled, None);
        item.set_key_accelerator(accelerator(id)?)?;
        parent.append(&item)?;
        Item::Normal(item)
    };
    entries.push(Entry { id, item, state });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_accelerators_parse_without_taking_native_window_shortcuts() -> Result<(), Error> {
        let mut keys = std::collections::HashSet::new();
        for id in commands::COMMANDS.iter().map(|s| s.id).chain(["edit.cut", "edit.copy", "edit.paste", "edit.select_all"]) {
            // Copy also occurs in COMMANDS; the menu still inserts it only once.
            if id == "edit.copy" {
                continue;
            }
            if let Some(key) = accelerator(id)? {
                assert!(keys.insert(key), "duplicate accelerator for {id}");
            }
        }
        for native in ["Cmd+M", "Cmd+H", "Cmd+Q"] {
            assert!(!keys.contains(&native.parse::<KeyAccelerator>()?));
        }
        Ok(())
    }
}
