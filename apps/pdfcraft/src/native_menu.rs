//! macOS: the native menu bar (PdfCraft, File, Edit, View, Window, Help) matching Adobe
//! Acrobat's top-level menu hierarchy on the Mac. Items run the corresponding commands.

use std::str::FromStr;
use std::sync::mpsc::{Receiver, channel};

use muda::accelerator::Accelerator;
use muda::{CheckMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};
use pdfcraft_ui_egui::{PdfCraftApp, ThemePreference};

pub struct NativeMenu {
    _menu: Menu,
    events: Receiver<MenuEvent>,
    theme_items: Vec<(&'static str, CheckMenuItem)>,
}

fn select_theme(app: &mut PdfCraftApp, id: &str) -> bool {
    let pref = match id {
        "theme:system" => ThemePreference::System,
        "theme:light" => ThemePreference::Light,
        "theme:dark" => ThemePreference::Dark,
        _ => return false,
    };
    app.set_theme_preference(pref);
    true
}

fn item(id: &'static str, label: &'static str, acc: Option<&'static str>) -> MenuItem {
    let accel = acc.and_then(|a| Accelerator::from_str(a).ok());
    MenuItem::with_id(id, label, true, accel)
}

fn build_file_menu(app: &PdfCraftApp) -> Submenu {
    let file = Submenu::new("File", true);

    // Open… and Reopen
    let _ = file.append(&item("file.open", "Open…", Some("CMD+O")));
    let _ = file.append(&item("file.reopen_last", "Reopen PDFs from Last Session", None));

    // Open Recent submenu populated from recent files
    let recent = Submenu::new("Open Recent File", true);
    for r in app.recent.iter().take(15) {
        let id = format!("recent:{}", r.path);
        let _ = recent.append(&MenuItem::with_id(id, &r.name, true, None));
    }
    if !app.recent.is_empty() {
        let _ = recent.append(&PredefinedMenuItem::separator());
    }
    let _ = recent.append(&item("file.clear_recent", "Clear Recent Files", None));
    let _ = file.append(&recent);

    // Create submenu
    let create = Submenu::new("Create", true);
    let _ = create.append_items(&[
        &item("create.blank", "New Blank PDF", None),
        &item("create.file", "Create PDF from File…", None),
        &item("create.multiple", "Create PDF from Multiple Files…", None),
        &item("create.images", "Create PDF from Images…", None),
        &item("create.clipboard", "Create PDF from Clipboard", None),
        &PredefinedMenuItem::separator(),
        &item("page.combine", "Combine Files…", None),
    ]);
    let _ = file.append(&create);

    let _ = file.append(&PredefinedMenuItem::separator());
    let _ = file.append(&item("file.save", "Save", Some("CMD+S")));
    let _ = file.append(&item("file.save_as", "Save As…", Some("CMD+SHIFT+S")));

    // Save as Other submenu
    let save_other = Submenu::new("Save as Other", true);
    let _ = save_other.append_items(&[
        &item("optimize.reduce", "Reduce File Size…", None),
        &item("optimize.advanced", "Optimize PDF…", None),
        &item("standards.pdfa", "PDF/A…", None),
    ]);
    let _ = file.append(&save_other);

    // Export To submenu
    let export = Submenu::new("Export To", true);
    let _ = export.append_items(&[
        &item("export.docx", "Microsoft Word Document", None),
        &item("export.pptx", "Microsoft PowerPoint Presentation", None),
        &item("export.html", "HTML Web Page", None),
        &item("export.rtf", "Rich Text Format", None),
        &item("export.text", "Text (Plain)", None),
        &item("export.image", "Image", None),
        &item("export.all_images", "All Images", None),
    ]);
    let _ = file.append(&export);

    let _ = file.append(&item("file.share", "Share File", None));
    let _ = file.append(&PredefinedMenuItem::separator());
    let _ = file.append(&item("file.revert", "Revert", None));
    let _ = file.append(&item("file.close", "Close File", Some("CMD+W")));
    let _ = file.append(&item("file.close_all", "Close All", Some("CMD+SHIFT+W")));
    let _ = file.append(&PredefinedMenuItem::separator());
    let _ = file.append(&item("file.properties", "Properties…", Some("CMD+D")));
    let _ = file.append(&PredefinedMenuItem::separator());
    let _ = file.append(&item("print.dialog", "Print…", Some("CMD+P")));

    file
}

fn build_edit_menu() -> Submenu {
    let edit = Submenu::new("Edit", true);

    let _ = edit.append_items(&[
        &item("edit.undo", "Undo", Some("CMD+Z")),
        &item("edit.redo", "Redo", Some("CMD+SHIFT+Z")),
        &PredefinedMenuItem::separator(),
        &item("edit.cut", "Cut", Some("CMD+X")),
        &item("edit.copy", "Copy", Some("CMD+C")),
        &item("edit.paste", "Paste", Some("CMD+V")),
        &item("page.delete", "Delete", None),
        &PredefinedMenuItem::separator(),
        &item("edit.select_all", "Select All", Some("CMD+A")),
        &item("edit.deselect_all", "Deselect All", Some("CMD+SHIFT+A")),
        &PredefinedMenuItem::separator(),
        &item("edit.edit_text", "Edit Text & Images", None),
        &item("edit.snapshot", "Take a Snapshot", None),
        &PredefinedMenuItem::separator(),
    ]);

    // Check Spelling submenu
    let spelling = Submenu::new("Check Spelling", true);
    let _ = spelling.append(&item("spell.check", "In Comments and Form Fields…", None));
    let _ = edit.append(&spelling);

    let _ = edit.append(&item("edit.lookup", "Look Up Selected Word…", None));

    let _ = edit.append_items(&[
        &item("edit.find", "Find", Some("CMD+F")),
        &item("edit.advanced_search", "Advanced Search", Some("CMD+SHIFT+F")),
        &PredefinedMenuItem::separator(),
        &item("standards.pdfa", "Preflight…", Some("CMD+SHIFT+X")),
    ]);

    // Protection submenu
    let protection = Submenu::new("Protection", true);
    let _ = protection.append_items(&[
        &item("protect.password", "Protect Using Password…", None),
        &item("protect.remove", "Remove Security", None),
        &item("protect.unlock", "Enter Permissions Password…", None),
        &item("protect.properties", "Security Properties…", None),
    ]);
    let _ = edit.append(&protection);

    let _ = edit.append_items(&[
        &item("a11y.check", "Accessibility Setup Assistant…", None),
        &item("view.palette", "Manage Tools", Some("CMD+K")),
        &item("form.clear", "Clear Form", None),
        &item("edit.special_characters", "Special Characters…", None),
    ]);

    edit
}

fn build_view_menu(theme_items: &mut Vec<(&'static str, CheckMenuItem)>) -> Submenu {
    let view = Submenu::new("View", true);

    // Rotate View submenu
    let rotate = Submenu::new("Rotate View", true);
    let _ = rotate.append_items(&[
        &item("view.rotate_cw", "Rotate Clockwise", Some("CMD+SHIFT+PLUS")),
        &item("view.rotate_ccw", "Rotate Counterclockwise", Some("CMD+SHIFT+MINUS")),
    ]);
    let _ = view.append(&rotate);

    // Page Navigation submenu
    let nav = Submenu::new("Page Navigation", true);
    let _ = nav.append_items(&[
        &item("view.nav.first", "First Page", Some("HOME")),
        &item("view.nav.prev", "Previous Page", Some("CMD+LEFT")),
        &item("view.nav.next", "Next Page", Some("CMD+RIGHT")),
        &item("view.nav.last", "Last Page", Some("END")),
        &PredefinedMenuItem::separator(),
        &item("view.focus_page_input", "Go to Page…", Some("CMD+SHIFT+N")),
    ]);
    let _ = view.append(&nav);

    // Page Display submenu
    let display = Submenu::new("Page Display", true);
    let _ = display.append_items(&[
        &item("view.layout.single", "Single Page", None),
        &item("view.layout.two_up", "Two-Page View", None),
        &item("view.continuous", "Continuous Scrolling", None),
        &item("view.layout.cover", "Show Cover Page in Two-Page View", None),
    ]);
    let _ = view.append(&display);

    // Zoom submenu
    let zoom = Submenu::new("Zoom", true);
    let _ = zoom.append_items(&[
        &item("view.zoom.actual", "Actual Size", Some("CMD+1")),
        &item("view.zoom.fit_page", "Fit Page", Some("CMD+0")),
        &item("view.zoom.fit_width", "Fit Width", Some("CMD+2")),
        &item("view.fit_visible", "Fit Visible", Some("CMD+3")),
        &item("view.zoom.in", "Zoom In", Some("CMD+=")),
        &item("view.zoom.out", "Zoom Out", Some("CMD+-")),
        &item("view.marquee_zoom", "Marquee Zoom", None),
    ]);
    let _ = view.append(&zoom);

    let _ = view.append(&PredefinedMenuItem::separator());

    // Tools submenu
    let tools = Submenu::new("Tools", true);
    let _ = tools.append_items(&[
        &item("comment.list", "Comments Panel", None),
        &item("form.fields", "Form Fields Panel", None),
        &item("sign.panel", "Signatures Panel", None),
        &item("a11y.tags", "Tags Panel", None),
        &item("page.organize", "Organize Pages", None),
        &item("tools.js_console", "JavaScript Console…", Some("CMD+J")),
    ]);
    let _ = view.append(&tools);

    // Show/Hide submenu
    let show_hide = Submenu::new("Show/Hide", true);
    let _ = show_hide.append(&item("ui.dock.reset", "Reset Panel Layout", None));
    let _ = view.append(&show_hide);

    // Display Theme submenu
    let themes = Submenu::new("Display Theme", true);
    for (mode, label) in [("system", "Use System Setting"), ("light", "Light Gray"), ("dark", "Dark Gray")] {
        let theme_item = CheckMenuItem::with_id(format!("theme:{mode}"), label, true, false, None);
        let _ = themes.append(&theme_item);
        theme_items.push((mode, theme_item));
    }
    let _ = view.append(&themes);

    let _ = view.append_items(&[
        &PredefinedMenuItem::separator(),
        &item("view.read_mode", "Read Mode", Some("CTRL+CMD+H")),
        &item("view.full_screen", "Full Screen Mode", Some("CMD+L")),
        &PredefinedMenuItem::separator(),
        &item("view.tracker", "Tracker…", None),
    ]);

    // Read Out Loud submenu
    let read_out_loud = Submenu::new("Read Out Loud", true);
    let _ = read_out_loud.append_items(&[
        &item("view.read_out_loud.activate", "Activate Read Out Loud", None),
        &item("view.read_out_loud.page", "Read This Page Only", None),
        &item("view.read_out_loud.document", "Read to End of Document", None),
    ]);
    let _ = view.append(&read_out_loud);

    let _ = view.append(&item("doc.compare", "Compare Files", None));

    view
}

fn build_window_menu() -> Submenu {
    let window = Submenu::new("Window", true);
    let _ = window.append_items(&[
        &PredefinedMenuItem::zoom(None),
        &PredefinedMenuItem::minimize(None),
        &item("window.new_window", "New Window", None),
        &PredefinedMenuItem::separator(),
        &item("window.cascade", "Cascade", Some("CMD+SHIFT+J")),
    ]);

    let tile = Submenu::new("Tile", true);
    let _ = tile.append_items(&[&item("window.tile_horiz", "Horizontally", None), &item("window.tile_vert", "Vertically", None)]);
    let _ = window.append(&tile);

    let _ = window.append_items(&[
        &item("window.split", "Split", None),
        &item("window.spreadsheet_split", "Spreadsheet Split", None),
        &PredefinedMenuItem::separator(),
        &item("window.next_tab", "Next Tab", Some("CTRL+TAB")),
        &item("window.prev_tab", "Previous Tab", Some("CTRL+SHIFT+TAB")),
    ]);
    window
}

fn build_help_menu() -> Submenu {
    let help = Submenu::new("Help", true);
    let _ = help.append_items(&[
        &item("help.app_page", "PdfCraft Help", None),
        &item("help.shortcuts", "Keyboard Shortcuts", None),
        &item("help.check_updates", "Check for Updates…", None),
        &PredefinedMenuItem::separator(),
        &item("help.discord", "Join the ArtCraft Discord…", None),
        &item("help.github", "PdfCraft on GitHub", None),
        &item("help.website", "ArtCraft Website", None),
        &PredefinedMenuItem::separator(),
        &item("help.about", "About PdfCraft", None),
    ]);
    help
}

impl NativeMenu {
    pub fn install(app: &PdfCraftApp, ctx: &egui::Context) -> NativeMenu {
        let menu = Menu::new();

        // Application menu (PdfCraft)
        let app_menu = Submenu::new("PdfCraft", true);
        let about = MenuItem::with_id("help.about", "About PdfCraft", true, None);
        let prefs = MenuItem::with_id("app.preferences", "Preferences…", true, Accelerator::from_str("CMD+,").ok());
        let _ = app_menu.append_items(&[
            &about,
            &PredefinedMenuItem::separator(),
            &prefs,
            &PredefinedMenuItem::separator(),
            &PredefinedMenuItem::services(None),
            &PredefinedMenuItem::separator(),
            &PredefinedMenuItem::hide(None),
            &PredefinedMenuItem::hide_others(None),
            &PredefinedMenuItem::show_all(None),
            &PredefinedMenuItem::separator(),
            &PredefinedMenuItem::quit(None),
        ]);
        let _ = menu.append(&app_menu);

        // File, Edit, View, Window, Help menus
        let _ = menu.append(&build_file_menu(app));
        let _ = menu.append(&build_edit_menu());

        let mut theme_items = Vec::new();
        let _ = menu.append(&build_view_menu(&mut theme_items));

        let window_menu = build_window_menu();
        let _ = menu.append(&window_menu);
        let _ = menu.append(&build_help_menu());

        menu.init_for_nsapp();

        // Register window_menu with NSApp for window tiling and window list
        window_menu.set_as_windows_menu_for_nsapp();

        let (tx, rx) = channel();
        let ctx = ctx.clone();
        MenuEvent::set_event_handler(Some(move |e: MenuEvent| {
            let _ = tx.send(e);
            ctx.request_repaint();
        }));

        NativeMenu { _menu: menu, events: rx, theme_items }
    }

    pub fn poll(&self, app: &mut PdfCraftApp, ctx: &egui::Context) {
        while let Ok(ev) = self.events.try_recv() {
            let id = ev.id.as_ref().to_string();

            // Text input / clipboard commands when typing in an editor
            if ctx.egui_wants_keyboard_input() {
                let clipboard_cmd = match id.as_str() {
                    "edit.copy" => Some(egui::ViewportCommand::RequestCopy),
                    "edit.cut" => Some(egui::ViewportCommand::RequestCut),
                    "edit.paste" => Some(egui::ViewportCommand::RequestPaste),
                    _ => None,
                };
                if let Some(cmd) = clipboard_cmd {
                    ctx.send_viewport_cmd(cmd);
                    continue;
                }
                if id == "edit.undo" || id == "edit.redo" {
                    ctx.input_mut(|i| {
                        i.events.push(egui::Event::Key {
                            key: egui::Key::Z,
                            physical_key: None,
                            pressed: true,
                            repeat: false,
                            modifiers: egui::Modifiers { command: true, mac_cmd: true, shift: id == "edit.redo", ..Default::default() },
                        });
                    });
                    continue;
                }
                if id == "edit.select_all" {
                    ctx.input_mut(|i| {
                        i.events.push(egui::Event::Key {
                            key: egui::Key::A,
                            physical_key: None,
                            pressed: true,
                            repeat: false,
                            modifiers: egui::Modifiers { command: true, mac_cmd: true, shift: false, ..Default::default() },
                        });
                    });
                    continue;
                }
            }

            if select_theme(app, &id) {
                continue;
            }

            if let Some(path) = id.strip_prefix("recent:") {
                app.open_path(path);
                continue;
            }

            match id.as_str() {
                // Clipboard actions outside text fields
                "edit.copy" => {
                    app.copy_selection(ctx);
                }
                "edit.cut" => {
                    app.copy_pages(true);
                }
                "edit.paste" => {
                    app.paste_pages();
                }
                "edit.select_all" => {
                    app.select_all();
                }
                "edit.deselect_all" => {
                    app.deselect_all();
                }

                // File menu specials
                "file.reopen_last" => {
                    app.reopen_previous_session();
                }
                "file.share" => {
                    if let Some(i) = app.active {
                        if let Some(path) = app.session.get(app.views[i].id).and_then(|d| d.path.as_deref()) {
                            ctx.copy_text(path.to_string());
                            app.notify_tr("File path copied to clipboard");
                        } else {
                            app.notify_tr("Save the file first to share it");
                        }
                    } else {
                        app.notify_tr("Open a document first");
                    }
                }

                // Edit specials
                "spell.check" => {
                    app.notify_tr("Spell check is active for all comments and form fields");
                }
                "edit.lookup" => {
                    if let Some(i) = app.active
                        && let Some(text) = app.views[i].selected_text()
                    {
                        let word = text.trim();
                        if !word.is_empty() {
                            app.open_url(&format!("https://en.wiktionary.org/wiki/{word}"));
                        } else {
                            app.notify_tr("Select a word in the document to look up");
                        }
                    } else {
                        app.notify_tr("Select a word in the document to look up");
                    }
                }
                "edit.special_characters" => {
                    app.notify_tr("Use ⌃⌘Space to open the Character Viewer for emoji and symbols");
                }

                // View specials
                "view.rotate_cw" => {
                    if let Some(i) = app.active {
                        app.views[i].rotate_view(true);
                    }
                }
                "view.rotate_ccw" => {
                    if let Some(i) = app.active {
                        app.views[i].rotate_view(false);
                    }
                }
                "view.nav.first" => {
                    if let Some(i) = app.active {
                        app.views[i].go_to_page(0);
                    }
                }
                "view.nav.prev" => {
                    if let Some(i) = app.active {
                        app.views[i].step_page(false);
                    }
                }
                "view.nav.next" => {
                    if let Some(i) = app.active {
                        app.views[i].step_page(true);
                    }
                }
                "view.nav.last" => {
                    if let Some(i) = app.active {
                        app.views[i].go_to_page(usize::MAX);
                    }
                }
                "view.zoom.actual" => {
                    if let Some(i) = app.active {
                        app.views[i].set_zoom(1.0);
                    }
                }
                "view.zoom.fit_page" => {
                    if let Some(i) = app.active {
                        app.views[i].set_fit(pdfcraft_ui_egui::canvas::Fit::Page);
                        app.views[i].goto = Some((app.views[i].current, 0.0));
                    }
                }
                "view.zoom.fit_width" => {
                    if let Some(i) = app.active {
                        app.views[i].set_fit(pdfcraft_ui_egui::canvas::Fit::Width);
                        app.views[i].goto = Some((app.views[i].current, 0.0));
                    }
                }
                "view.zoom.in" => {
                    if let Some(i) = app.active {
                        app.views[i].zoom_step(true);
                    }
                }
                "view.zoom.out" => {
                    if let Some(i) = app.active {
                        app.views[i].zoom_step(false);
                    }
                }
                "view.tracker" => {
                    app.notify_tr("No active tracked reviews or distributions");
                }
                "view.read_out_loud.activate" | "view.read_out_loud.page" | "view.read_out_loud.document" => {
                    app.notify_tr("Read Out Loud is supported through VoiceOver (press ⌘F5 to turn on VoiceOver)");
                }

                // Window specials
                "window.new_window" => {
                    app.open_dialog();
                }
                "window.cascade" => {
                    ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(egui::pos2(80.0, 80.0)));
                }
                "window.tile_horiz" | "window.tile_vert" => {
                    app.execute("view.layout.two_up");
                }
                "window.split" => {
                    app.execute("page.split");
                }
                "window.spreadsheet_split" => {
                    app.execute("form.merge_data");
                }
                "window.next_tab" => {
                    if !app.views.is_empty() {
                        let cur = app.active.unwrap_or(0);
                        app.active = Some((cur + 1) % app.views.len());
                    }
                }
                "window.prev_tab" => {
                    if !app.views.is_empty() {
                        let cur = app.active.unwrap_or(0);
                        app.active = Some((cur + app.views.len() - 1) % app.views.len());
                    }
                }

                // Default: execute registered command
                cmd => {
                    app.execute(cmd);
                }
            }
        }

        // Sync theme checkboxes
        for (mode, item) in &self.theme_items {
            let checked = match *mode {
                "system" => app.theme_preference == ThemePreference::System,
                "light" => app.theme_preference == ThemePreference::Light,
                "dark" => app.theme_preference == ThemePreference::Dark,
                _ => false,
            };
            if item.is_checked() != checked {
                item.set_checked(checked);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_menu_accelerators_and_commands() {
        for acc in [
            "CMD+O",
            "CMD+S",
            "CMD+SHIFT+S",
            "CMD+W",
            "CMD+D",
            "CMD+P",
            "CMD+Z",
            "CMD+SHIFT+Z",
            "CMD+X",
            "CMD+C",
            "CMD+V",
            "CMD+A",
            "CMD+F",
            "CMD+K",
            "CMD+L",
            "CTRL+CMD+H",
            "CTRL+TAB",
            "CTRL+SHIFT+TAB",
        ] {
            assert!(Accelerator::from_str(acc).is_ok(), "accelerator {acc} should parse");
        }
        for cmd in [
            "file.open",
            "file.save",
            "file.save_as",
            "file.close",
            "file.properties",
            "print.dialog",
            "edit.undo",
            "edit.redo",
            "edit.find",
            "view.full_screen",
            "view.read_mode",
            "help.discord",
            "help.about",
            "help.shortcuts",
        ] {
            assert!(pdfcraft_engine::commands::command(cmd).is_some(), "command {cmd} should exist");
        }
    }

    #[test]
    fn test_theme_selection() {
        let mut app = PdfCraftApp::new();
        assert!(select_theme(&mut app, "theme:dark"));
        assert_eq!(app.theme_preference, ThemePreference::Dark);
        assert!(select_theme(&mut app, "theme:light"));
        assert_eq!(app.theme_preference, ThemePreference::Light);
        assert!(select_theme(&mut app, "theme:system"));
        assert_eq!(app.theme_preference, ThemePreference::System);
        assert!(!select_theme(&mut app, "theme:invalid"));
    }
}
