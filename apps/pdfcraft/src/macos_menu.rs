//! Native macOS system menu bar (File, Edit, Pages, View, Tools, Window, Help) using muda.
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
        "+" | "=" => Code::Equal,
        "-" | "−" => Code::Minus,
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

fn custom_item(id: &'static str, label: &'static str, shortcut: Option<Accelerator>) -> MenuItem {
    let text = pdfcraft_ui_egui::i18n::t(label);
    MenuItem::with_id(id, text, true, shortcut)
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

    let open_recent_menu = Submenu::new("Open Recent", true);
    open_recent_menu.append_items(&[
        &cmd_item("file.open_recent", "Open Most Recent"),
        &PredefinedMenuItem::separator(),
        &cmd_item("file.clear_recent", "Clear Recent Files"),
    ])?;

    let create_menu = Submenu::new("Create PDF", true);
    create_menu.append_items(&[
        &cmd_item("create.blank", "New Blank PDF"),
        &PredefinedMenuItem::separator(),
        &cmd_item("create.file", "Create PDF from File…"),
        &cmd_item("create.multiple", "Create PDF from Multiple Files…"),
        &cmd_item("create.multiple_separate", "Create Separate PDFs…"),
        &cmd_item("create.images", "Create PDF from Images…"),
        &cmd_item("create.clipboard", "Create PDF from Clipboard"),
    ])?;

    let export_menu = Submenu::new("Export & Convert", true);
    export_menu.append_items(&[
        &cmd_item("export.image", "Export to Image…"),
        &cmd_item("export.all_images", "Export All Images…"),
        &PredefinedMenuItem::separator(),
        &cmd_item("export.docx", "Export to Word…"),
        &cmd_item("export.pptx", "Export to PowerPoint…"),
        &cmd_item("export.text", "Export to Text…"),
        &cmd_item("export.rtf", "Export to RTF…"),
        &cmd_item("export.html", "Export to HTML…"),
    ])?;

    let optimize_menu = Submenu::new("Reduce & Optimize", true);
    optimize_menu.append_items(&[&cmd_item("optimize.reduce", "Reduce File Size…"), &cmd_item("optimize.advanced", "Optimize PDF…")])?;

    let security_menu = Submenu::new("Password & Security", true);
    security_menu.append_items(&[
        &cmd_item("protect.password", "Protect Using Password…"),
        &cmd_item("protect.remove", "Remove Security"),
        &cmd_item("protect.properties", "Security Properties…"),
    ])?;

    file_menu.append_items(&[
        &cmd_item("file.open", "Open…"),
        &open_recent_menu,
        &cmd_item("file.pin_folder", "Pin Folder to Home…"),
        &PredefinedMenuItem::separator(),
        &cmd_item("create.blank", "New Blank PDF"),
        &create_menu,
        &cmd_item("page.combine", "Combine Files…"),
        &PredefinedMenuItem::separator(),
        &cmd_item("file.close", "Close File"),
        &cmd_item("file.close_all", "Close All"),
        &cmd_item("file.save", "Save"),
        &cmd_item("file.save_as", "Save As…"),
        &cmd_item("file.revert", "Revert"),
        &PredefinedMenuItem::separator(),
        &export_menu,
        &optimize_menu,
        &security_menu,
        &PredefinedMenuItem::separator(),
        &cmd_item("file.properties", "Document Properties…"),
        &cmd_item("file.word_count", "Word Count…"),
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
        &cmd_item("edit.column_select", "Column Select"),
        &cmd_item("edit.snapshot", "Take a Snapshot"),
        &PredefinedMenuItem::separator(),
        &cmd_item("edit.find", "Find…"),
        &cmd_item("edit.advanced_search", "Advanced Search…"),
        &PredefinedMenuItem::separator(),
        &cmd_item("form.clear", "Clear Form"),
    ])?;

    // 4. Pages menu
    let pages_menu = Submenu::new("Pages", true);
    pages_menu.append_items(&[
        &cmd_item("page.organize", "Organize Pages"),
        &PredefinedMenuItem::separator(),
        &cmd_item("bookmark.add", "New Bookmark"),
        &cmd_item("bookmark.from_structure", "New Bookmarks from Structure"),
        &PredefinedMenuItem::separator(),
        &cmd_item("page.rotate", "Rotate Pages Clockwise"),
        &cmd_item("page.rotate_ccw", "Rotate Pages Counterclockwise"),
        &cmd_item("page.rotate_dialog", "Rotate Pages…"),
        &PredefinedMenuItem::separator(),
        &cmd_item("page.insert_blank", "Insert Blank Page"),
        &cmd_item("page.insert", "Insert Pages from File…"),
        &cmd_item("page.duplicate", "Duplicate Pages"),
        &cmd_item("page.delete", "Delete Pages"),
        &cmd_item("page.replace", "Replace Pages…"),
        &cmd_item("page.extract", "Extract Pages…"),
        &cmd_item("page.split", "Split Document…"),
        &PredefinedMenuItem::separator(),
        &cmd_item("page.crop", "Crop Pages"),
        &cmd_item("page.boxes", "Set Page Boxes…"),
        &cmd_item("page.number", "Number Pages…"),
    ])?;

    // 5. View menu
    let view_menu = Submenu::new("View", true);

    let zoom_menu = Submenu::new("Zoom", true);
    zoom_menu.append_items(&[
        &custom_item("view.zoom.in", "Zoom In", Some(Accelerator::new(Modifiers::META, Code::Equal))),
        &custom_item("view.zoom.out", "Zoom Out", Some(Accelerator::new(Modifiers::META, Code::Minus))),
        &PredefinedMenuItem::separator(),
        &custom_item("view.zoom.actual_size", "Actual Size", Some(Accelerator::new(Modifiers::META, Code::Digit1))),
        &custom_item("view.zoom.page_level", "Zoom to Page Level", Some(Accelerator::new(Modifiers::META, Code::Digit0))),
        &custom_item("view.zoom.fit_width", "Fit to Width", Some(Accelerator::new(Modifiers::META, Code::Digit2))),
        &custom_item("view.zoom.fit_height", "Fit to Height", None),
        &cmd_item("view.fit_visible", "Fit Visible"),
        &PredefinedMenuItem::separator(),
        &custom_item("view.rotate_view.cw", "Rotate View Clockwise", Some(Accelerator::new(Modifiers::META | Modifiers::SHIFT, Code::Equal))),
        &custom_item("view.rotate_view.ccw", "Rotate View Counterclockwise", Some(Accelerator::new(Modifiers::META | Modifiers::SHIFT, Code::Minus))),
    ])?;

    let page_nav_menu = Submenu::new("Page Navigation", true);
    page_nav_menu.append_items(&[
        &cmd_item("view.focus_page_input", "Go to Page…"),
        &cmd_item("view.previous_view", "Previous View"),
        &cmd_item("view.next_view", "Next View"),
    ])?;

    let page_display_menu = Submenu::new("Page Display", true);
    page_display_menu.append_items(&[
        &cmd_item("view.layout.single", "Single Page"),
        &cmd_item("view.layout.two_up", "Two-Page View"),
        &PredefinedMenuItem::separator(),
        &cmd_item("view.continuous", "Continuous Scrolling"),
        &cmd_item("view.layout.cover", "Show Cover Page in Two-Page View"),
        &PredefinedMenuItem::separator(),
        &cmd_item("view.fit_width_scrolling", "Fit to Width Scrolling"),
        &cmd_item("view.fit_one_page", "Fit One Full Page"),
    ])?;

    let side_panels_menu = Submenu::new("Side Panels", true);
    side_panels_menu.append_items(&[
        &custom_item("panel.comments", "Comments", None),
        &custom_item("panel.bookmarks", "Bookmarks", None),
        &custom_item("panel.pages", "Pages", None),
        &custom_item("panel.fields", "Fields", None),
        &custom_item("panel.layers", "Layers", None),
        &custom_item("panel.attachments", "Attachments", None),
        &custom_item("panel.signatures", "Signatures", None),
        &custom_item("panel.accessibility", "Accessibility Checker", None),
        &custom_item("panel.tags", "Tags", None),
        &custom_item("panel.search", "Search", None),
        &custom_item("panel.compare", "Compare", None),
    ])?;

    let theme_menu = Submenu::new("Display Theme", true);
    theme_menu.append_items(&[
        &cmd_item("view.theme.system", "Use System Setting"),
        &cmd_item("view.theme.light", "Light Gray"),
        &cmd_item("view.theme.dark", "Dark Gray"),
    ])?;

    view_menu.append_items(&[
        &zoom_menu,
        &page_nav_menu,
        &page_display_menu,
        &PredefinedMenuItem::separator(),
        &cmd_item("view.palette", "Find Tools and Commands…"),
        &cmd_item("view.marquee_zoom", "Marquee Zoom"),
        &PredefinedMenuItem::separator(),
        &cmd_item("view.full_screen", "Full Screen Mode"),
        &cmd_item("view.read_mode", "Read Mode"),
        &PredefinedMenuItem::separator(),
        &cmd_item("view.next_tab", "Next Tab"),
        &cmd_item("view.previous_tab", "Previous Tab"),
        &PredefinedMenuItem::separator(),
        &side_panels_menu,
        &theme_menu,
        &PredefinedMenuItem::separator(),
        &cmd_item("ui.dock.reset", "Reset Panel Layout"),
    ])?;

    // 6. Tools menu
    let tools_menu = Submenu::new("Tools", true);

    let modes_menu = Submenu::new("Workspace Modes", true);
    modes_menu.append_items(&[
        &custom_item("mode.all_tools", "All Tools", None),
        &custom_item("mode.read", "Read", None),
        &custom_item("mode.edit", "Edit", None),
        &custom_item("mode.convert", "Convert", None),
        &custom_item("mode.sign", "E-Sign", None),
    ])?;

    let comment_tools = Submenu::new("Comments & Markup", true);
    comment_tools.append_items(&[
        &cmd_item("comment.note", "Add a Sticky Note"),
        &cmd_item("comment.freetext", "Add a Text Box"),
        &cmd_item("comment.highlight", "Highlight Text"),
        &cmd_item("comment.underline", "Underline Text"),
        &cmd_item("comment.strikeout", "Strikethrough Text"),
        &cmd_item("comment.ink", "Draw Freehand"),
        &cmd_item("comment.stamp", "Add a Stamp"),
        &PredefinedMenuItem::separator(),
        &cmd_item("comment.flatten", "Flatten Comments"),
        &cmd_item("comment.summarize", "Summarize Comments"),
    ])?;

    let edit_tools = Submenu::new("Edit PDF Content", true);
    edit_tools.append_items(&[
        &cmd_item("edit.text", "Add Text"),
        &cmd_item("edit.edit_text", "Edit Text & Images"),
        &cmd_item("edit.link", "Add or Edit Links"),
        &cmd_item("edit.image", "Add Image…"),
        &PredefinedMenuItem::separator(),
        &cmd_item("edit.header_footer", "Add Header & Footer…"),
        &cmd_item("edit.watermark", "Add Watermark…"),
        &cmd_item("edit.background", "Add Background…"),
        &cmd_item("edit.bates", "Add Bates Numbering…"),
    ])?;

    let form_tools = Submenu::new("Forms", true);
    form_tools.append_items(&[
        &cmd_item("form.prepare", "Prepare a Form"),
        &cmd_item("form.detect", "Detect Form Fields"),
        &cmd_item("form.flatten", "Flatten Form Fields"),
    ])?;

    let sign_tools = Submenu::new("Sign & Certify", true);
    sign_tools.append_items(&[
        &cmd_item("sign.fill.signature", "Fill & Sign"),
        &cmd_item("sign.digital", "Digitally Sign"),
        &cmd_item("sign.validate", "Validate All Signatures"),
    ])?;

    let redact_tools = Submenu::new("Redact", true);
    redact_tools.append_items(&[
        &cmd_item("redact.mark", "Redact Text and Images"),
        &cmd_item("redact.pages", "Redact Pages…"),
        &cmd_item("redact.sanitize", "Sanitize Document…"),
    ])?;

    let ocr_tools = Submenu::new("Scan & OCR", true);
    ocr_tools.append_items(&[&cmd_item("ocr.recognize", "Recognize Text…"), &cmd_item("ocr.enhance_camera", "Enhance Camera Image")])?;

    let a11y_tools = Submenu::new("Accessibility", true);
    a11y_tools.append_items(&[&cmd_item("a11y.check", "Check for Accessibility…"), &cmd_item("a11y.autotag", "Automatically Tag PDF")])?;

    tools_menu.append_items(&[
        &modes_menu,
        &PredefinedMenuItem::separator(),
        &comment_tools,
        &edit_tools,
        &form_tools,
        &sign_tools,
        &redact_tools,
        &ocr_tools,
        &a11y_tools,
        &PredefinedMenuItem::separator(),
        &cmd_item("doc.compare", "Compare Files…"),
    ])?;

    // 7. Window menu
    let window_menu = Submenu::new("Window", true);
    window_menu.append_items(&[
        &PredefinedMenuItem::minimize(None),
        &PredefinedMenuItem::zoom(None),
        &PredefinedMenuItem::separator(),
        &PredefinedMenuItem::bring_all_to_front(None),
        &PredefinedMenuItem::separator(),
        &PredefinedMenuItem::fullscreen(None),
    ])?;

    // 8. Help menu
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

    menu.append_items(&[&app_menu, &file_menu, &edit_menu, &pages_menu, &view_menu, &tools_menu, &window_menu, &help_menu])?;

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
