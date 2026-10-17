//! Native macOS system menu bar (File, Edit, Pages, View, Window, Help) using muda.
//!
//! Provides the standard top-level macOS application menus, wiring menu events to the running
//! [`pdfcraft_ui_egui::PdfCraftApp`] through [`pdfcraft_ui_egui::OsEvent::Command`]. The Quit item
//! uses [`craft_quit::QUIT_MENU_ID`] and routes to [`craft_quit::on_menu_event`] so unsaved-changes
//! prompts remain fully guarded.

use std::cell::RefCell;

use egui::Context;
use muda::accelerator::{Accelerator, Code, Modifiers};
use muda::{Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};

use crate::apple_events::AppleEvents;

thread_local! {
    /// The installed menu. muda's items must outlive the native menu that displays them.
    static MENU: RefCell<Option<Menu>> = const { RefCell::new(None) };
}

fn to_accelerator(shortcut: pdfcraft_engine::commands::Shortcut) -> Option<Accelerator> {
    let mut mods = Modifiers::empty();
    if shortcut.command {
        mods |= Modifiers::META;
    }
    if shortcut.shift {
        mods |= Modifiers::SHIFT;
    }
    if shortcut.mac_ctrl {
        mods |= Modifiers::CONTROL;
    }
    let code = match shortcut.key {
        "A" => Code::KeyA,
        "B" => Code::KeyB,
        "C" => Code::KeyC,
        "D" => Code::KeyD,
        "E" => Code::KeyE,
        "F" => Code::KeyF,
        "G" => Code::KeyG,
        "H" => Code::KeyH,
        "I" => Code::KeyI,
        "J" => Code::KeyJ,
        "K" => Code::KeyK,
        "L" => Code::KeyL,
        "M" => Code::KeyM,
        "N" => Code::KeyN,
        "O" => Code::KeyO,
        "P" => Code::KeyP,
        "Q" => Code::KeyQ,
        "R" => Code::KeyR,
        "S" => Code::KeyS,
        "T" => Code::KeyT,
        "U" => Code::KeyU,
        "V" => Code::KeyV,
        "W" => Code::KeyW,
        "X" => Code::KeyX,
        "Y" => Code::KeyY,
        "Z" => Code::KeyZ,
        "0" => Code::Digit0,
        "1" => Code::Digit1,
        "2" => Code::Digit2,
        "3" => Code::Digit3,
        "4" => Code::Digit4,
        "5" => Code::Digit5,
        "6" => Code::Digit6,
        "7" => Code::Digit7,
        "8" => Code::Digit8,
        "9" => Code::Digit9,
        "," => Code::Comma,
        "." => Code::Period,
        "[" => Code::BracketLeft,
        "]" => Code::BracketRight,
        "Tab" => Code::Tab,
        "Delete" => Code::Delete,
        _ => return None,
    };
    Some(Accelerator::new(mods, code))
}

fn cmd_item(id: &'static str, fallback: &'static str) -> MenuItem {
    let label = pdfcraft_ui_egui::i18n::menu_label(id, fallback);
    let shortcut = pdfcraft_engine::commands::command(id).and_then(|c| c.shortcut).and_then(to_accelerator);
    MenuItem::with_id(id, label, true, shortcut)
}

pub fn install(ctx: &Context, apple_events: &AppleEvents, app_name: &str) -> muda::Result<()> {
    if MENU.with_borrow(Option::is_some) {
        return Ok(());
    }

    let menu = Menu::new();

    // 1. App menu (PdfCraft)
    let app_menu = Submenu::new(app_name, true);
    let quit = MenuItem::with_id(craft_quit::QUIT_MENU_ID, format!("Quit {app_name}"), true, Some(Accelerator::new(Modifiers::META, Code::KeyQ)));
    app_menu.append_items(&[
        &PredefinedMenuItem::about(Some(&format!("About {app_name}")), None),
        &PredefinedMenuItem::separator(),
        &cmd_item("app.preferences", "Preferences…"),
        &PredefinedMenuItem::separator(),
        &PredefinedMenuItem::services(None),
        &PredefinedMenuItem::separator(),
        &PredefinedMenuItem::hide(Some(&format!("Hide {app_name}"))),
        &PredefinedMenuItem::hide_others(None),
        &PredefinedMenuItem::show_all(None),
        &PredefinedMenuItem::separator(),
        &quit,
    ])?;

    // 2. File menu
    let file_menu = Submenu::new("File", true);
    file_menu.append_items(&[
        &cmd_item("file.open", "Open…"),
        &cmd_item("create.blank", "New Blank PDF"),
        &cmd_item("create.file", "Create PDF from File…"),
        &cmd_item("page.combine", "Combine Files…"),
        &PredefinedMenuItem::separator(),
        &cmd_item("file.close", "Close File"),
        &cmd_item("file.close_all", "Close All"),
        &cmd_item("file.save", "Save"),
        &cmd_item("file.save_as", "Save As…"),
        &cmd_item("file.revert", "Revert"),
        &PredefinedMenuItem::separator(),
        &cmd_item("optimize.reduce", "Reduce File Size…"),
        &cmd_item("file.properties", "Document Properties…"),
        &PredefinedMenuItem::separator(),
        &cmd_item("print.dialog", "Print…"),
    ])?;

    // 3. Edit menu
    let edit_menu = Submenu::new("Edit", true);
    edit_menu.append_items(&[
        &cmd_item("edit.undo", "Undo"),
        &cmd_item("edit.redo", "Redo"),
        &PredefinedMenuItem::separator(),
        &PredefinedMenuItem::cut(None),
        &PredefinedMenuItem::copy(None),
        &PredefinedMenuItem::paste(None),
        &PredefinedMenuItem::select_all(None),
        &PredefinedMenuItem::separator(),
        &cmd_item("edit.find", "Find…"),
        &cmd_item("edit.advanced_search", "Advanced Search…"),
    ])?;

    // 4. Pages menu
    let pages_menu = Submenu::new("Pages", true);
    pages_menu.append_items(&[
        &cmd_item("page.organize", "Organize Pages"),
        &cmd_item("bookmark.add", "New Bookmark"),
        &cmd_item("bookmark.from_structure", "New Bookmarks from Structure"),
        &PredefinedMenuItem::separator(),
        &cmd_item("page.rotate", "Rotate Pages Clockwise"),
        &cmd_item("page.rotate_ccw", "Rotate Pages Counterclockwise"),
        &cmd_item("page.rotate_dialog", "Rotate Pages…"),
        &cmd_item("page.delete", "Delete Pages"),
        &cmd_item("page.insert_blank", "Insert Blank Page"),
        &cmd_item("page.duplicate", "Duplicate Pages"),
        &cmd_item("page.crop", "Crop Pages"),
        &cmd_item("page.boxes", "Set Page Boxes…"),
        &cmd_item("page.insert", "Insert Pages from File…"),
        &cmd_item("page.replace", "Replace Pages…"),
        &cmd_item("page.extract", "Extract Pages…"),
        &cmd_item("page.split", "Split Document…"),
        &cmd_item("page.number", "Number Pages…"),
    ])?;

    // 5. View menu
    let view_menu = Submenu::new("View", true);
    view_menu.append_items(&[
        &cmd_item("view.palette", "Find Tools and Commands…"),
        &PredefinedMenuItem::separator(),
        &cmd_item("view.fit_visible", "Fit Visible"),
        &cmd_item("view.marquee_zoom", "Marquee Zoom"),
        &PredefinedMenuItem::separator(),
        &cmd_item("view.full_screen", "Full Screen Mode"),
        &cmd_item("view.read_mode", "Read Mode"),
        &PredefinedMenuItem::separator(),
        &cmd_item("view.next_tab", "Next Tab"),
        &cmd_item("view.previous_tab", "Previous Tab"),
        &cmd_item("view.focus_page_input", "Go to Page…"),
        &PredefinedMenuItem::separator(),
        &cmd_item("comment.list", "Comments Panel"),
        &cmd_item("form.fields", "Form Fields Panel"),
        &cmd_item("sign.panel", "Signatures Panel"),
        &PredefinedMenuItem::separator(),
        &cmd_item("ui.dock.reset", "Reset Panel Layout"),
    ])?;

    // 6. Window menu
    let window_menu = Submenu::new("Window", true);
    window_menu.append_items(&[
        &PredefinedMenuItem::minimize(None),
        &PredefinedMenuItem::zoom(None),
        &PredefinedMenuItem::separator(),
        &PredefinedMenuItem::bring_all_to_front(None),
        &PredefinedMenuItem::separator(),
        &PredefinedMenuItem::fullscreen(None),
    ])?;

    // 7. Help menu
    let help_menu = Submenu::new("Help", true);
    help_menu.append_items(&[
        &cmd_item("help.shortcuts", "Keyboard Shortcuts"),
        &PredefinedMenuItem::separator(),
        &cmd_item("help.discord", "Join the ArtCraft Discord"),
        &cmd_item("help.app_page", "PdfCraft Web Page"),
        &cmd_item("help.github", "PdfCraft on GitHub"),
        &cmd_item("help.website", "ArtCraft Website"),
        &PredefinedMenuItem::separator(),
        &cmd_item("help.check_updates", "Check for Updates…"),
        &cmd_item("help.about", "About PdfCraft"),
    ])?;

    menu.append_items(&[&app_menu, &file_menu, &edit_menu, &pages_menu, &view_menu, &window_menu, &help_menu])?;

    menu.init_for_nsapp();

    let ctx = ctx.clone();
    let menu_inbox = apple_events.menu_inbox();
    MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
        let id = event.id.as_ref();
        if id == craft_quit::QUIT_MENU_ID {
            craft_quit::on_menu_event(&ctx, id);
        } else {
            if let Ok(mut lock) = menu_inbox.lock() {
                lock.push(id.to_string());
            }
            ctx.request_repaint();
        }
    }));

    MENU.set(Some(menu));
    Ok(())
}
