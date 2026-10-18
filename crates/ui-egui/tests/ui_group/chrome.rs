//! The window chrome for full-screen reading: one top strip (Home, the Menu icon, the tabs), the
//! workspaces on the floating bar at the left, and search, document actions, shortcuts and the
//! theme on the floating bar at the right.

use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use pdfcraft_ui_egui::{Mode, PdfCraftApp};

const WIDTH: f32 = 1400.0;

/// A one-page PDF with a correct cross-reference table, so it opens without the repaired-file notice
/// (which takes a row above the page of its own).
fn fixture() -> Vec<u8> {
    let objects =
        ["<< /Type /Catalog /Pages 2 0 R >>", "<< /Type /Pages /Kids [3 0 R] /Count 1 >>", "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 300 400] >>"];
    let mut pdf = String::from("%PDF-1.7\n");
    let mut offsets = Vec::new();
    for (i, body) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        pdf.push_str(&format!("{} 0 obj {body} endobj\n", i + 1));
    }
    let xref = pdf.len();
    pdf.push_str(&format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1));
    for offset in offsets {
        pdf.push_str(&format!("{offset:010} 00000 n \n"));
    }
    pdf.push_str(&format!("trailer << /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objects.len() + 1));
    pdf.into_bytes()
}

fn harness(setup: impl FnOnce(&mut PdfCraftApp) + 'static) -> Harness<'static, PdfCraftApp> {
    let mut h = Harness::builder().with_size(egui::vec2(WIDTH, 900.0)).build_eframe(move |cc| {
        // Shortcuts read Ctrl+ whichever platform runs the tests.
        cc.egui_ctx.set_os(egui::os::OperatingSystem::Nix);
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        setup(&mut app);
        app
    });
    h.run_steps(6);
    h
}

fn with_document() -> Harness<'static, PdfCraftApp> {
    harness(|app| {
        app.open_bytes("one.pdf", None, fixture()).unwrap();
    })
}

/// Whether the node labelled `label` (exactly) is enabled.
fn enabled(h: &Harness<'static, PdfCraftApp>, label: &str) -> bool {
    h.get_by_label(label);
    h.query_by(|n| n.label().as_deref() == Some(label) && n.is_disabled()).is_none()
}

/// The right bar's buttons, with or without a document: search, the document's actions,
/// shortcuts and the theme, all at the window's right edge and none in the tab strip.
#[test]
fn the_right_bar_holds_search_document_actions_shortcuts_and_theme() {
    for document in [false, true] {
        let h = if document { with_document() } else { harness(|_| {}) };
        let strip = h.get_by_label("Home").rect().bottom();
        for label in ["Find tools and commands (Ctrl+K)", "Save (Ctrl+S)", "Print (Ctrl+P)", "Keyboard shortcuts"] {
            let r = h.get_by_label(label).rect();
            assert!(r.left() > WIDTH - 80.0, "{label} at {r:?} is on the right bar (document: {document})");
            assert!(r.top() > strip, "{label} at {r:?} is below the tab strip (document: {document})");
        }
        let theme = h.get_by_label_contains("Display theme:").rect();
        assert!(theme.left() > WIDTH - 80.0 && theme.top() > strip, "theme at {theme:?}");
        // The document's actions wait for a document.
        assert_eq!(enabled(&h, "Save (Ctrl+S)"), document);
        assert_eq!(enabled(&h, "Print (Ctrl+P)"), document);
        assert!(enabled(&h, "Keyboard shortcuts"));
    }
}

/// With a document, the side panels and page navigation are on the right too.
#[test]
fn the_right_bars_hold_the_side_panels_and_page_navigation() {
    let h = with_document();
    for label in ["Bookmarks", "Page thumbnails", "Next page", "Previous page", "Toggle fit page / fit width"] {
        let r = h.get_by_label(label).rect();
        assert!(r.right() > WIDTH - 500.0, "{label} at {r:?} is at the right");
    }
    // Page navigation floats at the bottom, below the bar of panels.
    assert!(h.get_by_label("Next page").rect().top() > h.get_by_label("Page thumbnails").rect().bottom());
}

/// The right bars float over the page, as the left bar does, instead of in a column of their own:
/// the page area runs under them to the window's edge, and they keep the left bar's inset.
#[test]
fn the_right_bars_float_over_the_page() {
    let mut h = with_document();
    h.get_by_label("Read").click();
    h.run_steps(4);
    let page_area = h.state().views[0].viewport_rect();
    for label in ["Save (Ctrl+S)", "Next page"] {
        let r = h.get_by_label(label).rect();
        assert!(page_area.right() > r.right(), "the page area {page_area:?} runs under {label} at {r:?}");
        // The card's margin (6) plus the 14 the bars keep from the edge.
        let inset = WIDTH - r.right();
        assert!((14.0..=24.0).contains(&inset), "{label} at {r:?} floats {inset} in from the edge");
    }
}

/// `n` pages of 600 × 800, for a document that scrolls.
fn pages(n: usize) -> Vec<u8> {
    let kids: Vec<String> = (0..n).map(|i| format!("{} 0 R", i + 3)).collect();
    let mut objects = vec!["<< /Type /Catalog /Pages 2 0 R >>".to_string(), format!("<< /Type /Pages /Kids [{}] /Count {n} >>", kids.join(" "))];
    objects.extend((0..n).map(|_| "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 600 800] >>".to_string()));
    let mut pdf = String::from("%PDF-1.7\n");
    let mut offsets = Vec::new();
    for (i, body) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        pdf.push_str(&format!("{} 0 obj {body} endobj\n", i + 1));
    }
    let xref = pdf.len();
    pdf.push_str(&format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1));
    for offset in offsets {
        pdf.push_str(&format!("{offset:010} 00000 n \n"));
    }
    pdf.push_str(&format!("trailer << /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objects.len() + 1));
    pdf.into_bytes()
}

/// Reading, the tab strip floats over the page: the pages start below it, scroll on up under it,
/// and the page being read is the one below it. Elsewhere it keeps a band of its own.
#[test]
fn reading_the_pages_scroll_on_under_the_tab_strip() {
    let mut h = harness(|app| {
        app.open_bytes("long.pdf", None, pages(4)).unwrap();
    });
    h.get_by_label("Read").click();
    h.run_steps(4);
    let strip = h.get_by_label("Home").rect().bottom();
    let first = |h: &Harness<'static, PdfCraftApp>| h.state().views[0].page_screen_rect(0).expect("page 1 is drawn");
    assert!(first(&h).top() > strip, "page 1 {:?} starts below the strip ({strip})", first(&h));
    let read_from = h.state().views[0].viewport_rect().top();
    assert!(read_from >= strip && read_from - strip < 8.0, "the page is read from below the strip: {read_from}");
    // Scroll until page 1 has gone up under the strip but for its last strip of paper: below
    // the strip, a page that far up is past the view and not drawn at all.
    let centre = h.state().views[0].viewport_rect().center();
    h.hover_at(centre);
    for _ in 0..400 {
        if first(&h).bottom() < strip - 6.0 {
            break;
        }
        h.event(egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: egui::vec2(0.0, -20.0),
            phase: egui::TouchPhase::Move,
            modifiers: egui::Modifiers::NONE,
        });
        h.run_steps(1);
    }
    h.run_steps(2);
    let under = first(&h);
    assert!(under.bottom() < strip && under.bottom() > 0.0, "page 1 {under:?} is drawn under the strip ({strip})");
    assert_eq!(h.state().views[0].current, 1, "the page read is the one below the strip");
    // In the tools workspaces the strip keeps its band: nothing is drawn under it.
    h.get_by_label("All tools").click();
    h.run_steps(6);
    let page_area = h.state().views[0].viewport_rect();
    assert!(page_area.top() >= strip, "{page_area:?}");
}

/// The workspace button on the left bar labelled `label`.
fn on_left_bar(h: &Harness<'static, PdfCraftApp>, label: &str) -> egui::Rect {
    // All tools also names its panel of tools; the bar's button is the narrow one.
    h.query_all_by_label(label).map(|n| n.rect()).find(|r| r.width() < 50.0).expect("on the left bar")
}

/// Idle, the left bar slides out of the window to the left after eight seconds (kittest steps a
/// quarter second a frame); the pointer at the window's edge beside it brings it back.
#[test]
fn the_left_bar_slides_away_when_idle_and_comes_back_at_the_edge() {
    let mut h = with_document();
    h.get_by_label("Read").click();
    h.run_steps(2);
    let home = on_left_bar(&h, "Read");
    assert!(home.left() > 0.0, "in place at first: {home:?}");
    let page = h.state().views[0].viewport_rect().center();
    h.hover_at(page);
    h.run_steps(40);
    let away = on_left_bar(&h, "Read");
    assert!(away.right() < 0.0, "slid out of the window: {away:?}");
    h.hover_at(egui::pos2(2.0, home.center().y));
    h.run_steps(4);
    let back = on_left_bar(&h, "Read");
    assert!((back.left() - home.left()).abs() < 0.5, "back in place: {back:?}, was {home:?}");
}

/// The right bars slide out of the window to the right the same way, each brought back by the
/// pointer at the edge beside it.
#[test]
fn the_right_bars_slide_away_when_idle_and_come_back_at_the_edge() {
    let mut h = with_document();
    h.get_by_label("Read").click();
    h.run_steps(2);
    let (save, next) = (h.get_by_label("Save (Ctrl+S)").rect(), h.get_by_label("Next page").rect());
    let page = h.state().views[0].viewport_rect().center();
    h.hover_at(page);
    h.run_steps(40);
    for label in ["Save (Ctrl+S)", "Next page"] {
        let r = h.get_by_label(label).rect();
        assert!(r.left() > WIDTH, "{label} slid out of the window: {r:?}");
    }
    h.hover_at(egui::pos2(WIDTH - 2.0, next.center().y));
    h.run_steps(4);
    let back = h.get_by_label("Next page").rect();
    assert!((back.left() - next.left()).abs() < 0.5, "page navigation back in place: {back:?}, was {next:?}");
    assert!(h.get_by_label("Save (Ctrl+S)").rect().left() > WIDTH, "the other bar stays away");
    h.hover_at(egui::pos2(WIDTH - 2.0, save.center().y));
    h.run_steps(4);
    assert!((h.get_by_label("Save (Ctrl+S)").rect().left() - save.left()).abs() < 0.5);
}

/// View ▸ Auto hide side panels turns the sliding off: the bars only fade. It is kept with the
/// settings, and the automation option sets it.
#[test]
fn auto_hide_side_panels_can_be_turned_off_from_the_view_menu() {
    let mut h = with_document();
    h.get_by_label("Read").click();
    h.run_steps(2);
    assert!(h.state().auto_hide_bars, "on by default");
    h.get_by_label("Menu").click();
    h.run_steps(2);
    h.get_by_label("View ⏵").click();
    h.run_steps(2);
    h.get_by_role_and_label(Role::CheckBox, "Auto hide side panels").click();
    h.run_steps(2);
    assert!(!h.state().auto_hide_bars);
    h.key_press(egui::Key::Escape);
    let home = on_left_bar(&h, "Read");
    h.hover_at(h.state().views[0].viewport_rect().center());
    h.run_steps(40);
    let stays = on_left_bar(&h, "Read");
    assert!((stays.left() - home.left()).abs() < 0.5, "the left bar stays in place: {stays:?}");
    assert!(h.get_by_label("Save (Ctrl+S)").rect().right() < WIDTH, "and so do the right bars");
    let saved = h.state().persist();
    let mut restored = PdfCraftApp::new();
    restored.restore(&saved);
    assert!(!restored.auto_hide_bars, "kept with the settings");
    restored.set_option("auto-hide-bars", "true").unwrap();
    assert!(restored.auto_hide_bars);
    assert!(restored.set_option("auto-hide-bars", "sometimes").is_err());
}

/// Pick `choice` from the Preferences list showing `current`.
fn pick(h: &mut Harness<'static, PdfCraftApp>, current: &str, choice: &str) {
    h.get_by_value(current).scroll_to_me();
    h.run_steps(4);
    h.get_by_value(current).click();
    h.run_steps(4);
    // The list scrolls: the last choices are below its fold.
    h.get_by_label(choice).scroll_to_me();
    h.run_steps(4);
    h.get_by_label(choice).click();
    h.run_steps(2);
}

/// Preferences ▸ Fade side panels after and Hide side panels after set how long the chrome stays
/// opaque and in place, each on its own. They are kept with the settings, and the automation
/// options set them.
#[test]
fn the_fade_and_hide_timers_are_chosen_in_preferences() {
    let mut h = with_document();
    h.get_by_label("Read").click();
    h.run_steps(2);
    assert_eq!((h.state().chrome_fade_after, h.state().chrome_hide_after), (5, 8), "five and eight seconds by default");
    h.state_mut().set_option("dialog", "preferences").unwrap();
    h.run_steps(2);
    pick(&mut h, "5 s", "10 s");
    assert_eq!(h.state().chrome_fade_after, 10);
    pick(&mut h, "8 s", "15 s");
    assert_eq!(h.state().chrome_hide_after, 15);
    h.state_mut().dialog = None;
    h.run_steps(2);
    let home = on_left_bar(&h, "Read");
    h.hover_at(h.state().views[0].viewport_rect().center());
    // Twelve seconds: past the usual eight, short of fifteen.
    h.run_steps(48);
    let stays = on_left_bar(&h, "Read");
    assert!((stays.left() - home.left()).abs() < 0.5, "still in place before fifteen seconds: {stays:?}");
    h.run_steps(16);
    assert!(on_left_bar(&h, "Read").right() < 0.0, "gone after fifteen");
    let saved = h.state().persist();
    let mut restored = PdfCraftApp::new();
    restored.restore(&saved);
    assert_eq!((restored.chrome_fade_after, restored.chrome_hide_after), (10, 15), "kept with the settings");
    restored.set_option("chrome-fade-after", "2").unwrap();
    restored.set_option("chrome-hide-after", "30").unwrap();
    assert_eq!((restored.chrome_fade_after, restored.chrome_hide_after), (2, 30));
    for wrong in ["6", "0", "-1", "soon"] {
        assert!(restored.set_option("chrome-fade-after", wrong).is_err(), "{wrong}");
        assert!(restored.set_option("chrome-hide-after", wrong).is_err(), "{wrong}");
    }
    assert_eq!((restored.chrome_fade_after, restored.chrome_hide_after), (2, 30));
    restored.restore(r#"{"chrome_fade_after": 7, "chrome_hide_after": 9}"#);
    assert_eq!((restored.chrome_fade_after, restored.chrome_hide_after), (2, 30), "a timer not on offer is ignored");
}

/// View ▸ Fade side panels turns the fading off; it is kept with the settings, and the automation
/// option sets it.
#[test]
fn fading_can_be_turned_off_from_the_view_menu() {
    let mut h = with_document();
    assert!(h.state().chrome_fades, "on by default");
    h.get_by_label("Menu").click();
    h.run_steps(2);
    h.get_by_label("View ⏵").click();
    h.run_steps(2);
    h.get_by_role_and_label(Role::CheckBox, "Fade side panels").click();
    h.run_steps(2);
    assert!(!h.state().chrome_fades);
    let mut restored = PdfCraftApp::new();
    restored.restore(&h.state().persist());
    assert!(!restored.chrome_fades, "kept with the settings");
    restored.set_option("fade-bars", "true").unwrap();
    assert!(restored.chrome_fades);
    assert!(restored.set_option("fade-bars", "maybe").is_err());
}

/// With a panel of tools beside the page, or a dialog open, the bars count as in use: they stay
/// in place however long the pointer is elsewhere.
#[test]
fn an_open_tools_panel_or_dialog_keeps_the_bars_in_place() {
    let mut h = with_document();
    // A document opens in All tools, its panel of tools beside the page.
    assert_eq!(h.state().mode, Mode::AllTools);
    let home = on_left_bar(&h, "Read");
    h.hover_at(h.state().views[0].viewport_rect().center());
    h.run_steps(48);
    let stays = on_left_bar(&h, "Read");
    assert!((stays.left() - home.left()).abs() < 0.5, "the tools panel keeps the bar in place: {stays:?}");
    // Reading, with Preferences open.
    h.get_by_label("Read").click();
    h.run_steps(2);
    let home = on_left_bar(&h, "Read");
    h.state_mut().set_option("dialog", "preferences").unwrap();
    h.hover_at(egui::pos2(WIDTH / 2.0, 850.0));
    h.run_steps(48);
    let stays = on_left_bar(&h, "Read");
    assert!((stays.left() - home.left()).abs() < 0.5, "the dialog keeps the bar in place: {stays:?}");
    // Closed, the clock starts again.
    h.state_mut().dialog = None;
    h.run_steps(48);
    assert!(on_left_bar(&h, "Read").right() < 0.0, "slid away once nothing was open");
}

/// Edit a PDF with Format text showing: its colour swatches fit the tools panel, so the tool rows
/// below keep its width and their Ready badges aren't cut off at the panel's edge.
#[test]
fn the_edit_panel_fits_its_width_with_format_text_showing() {
    let mut h = with_document();
    h.state_mut().set_option("tool", "edit").unwrap();
    h.state_mut().set_option("quick", "add-text").unwrap();
    h.run_steps(6);
    let edge = h.get_by_label("Close panel").rect().right();
    for label in ["Bold", "Align left", "Rotate pages", "Insert pages"] {
        let r = h.get_by_label(label).rect();
        assert!(r.right() <= edge + 1.0, "{label} at {r:?} stays inside the panel (its edge at {edge})");
    }
}

/// The search button opens Find tools and commands and closes it again; so does the palette's
/// own close button.
#[test]
fn search_opens_a_floating_search_that_closes() {
    let mut h = with_document();
    h.get_by_label("Find tools and commands (Ctrl+K)").click();
    h.run_steps(3);
    assert!(h.state().palette_open, "the search button opens the palette");
    h.get_by_label("Find tools and commands (Ctrl+K)").click();
    h.run_steps(3);
    assert!(!h.state().palette_open, "a second click closes it");
    h.get_by_label("Find tools and commands (Ctrl+K)").click();
    h.run_steps(3);
    assert!(h.state().palette_open);
    h.get_by_role_and_label(Role::Button, "Close").click();
    h.run_steps(3);
    assert!(!h.state().palette_open, "the palette's close button closes it");
    assert!(h.query_by_label("Close").is_none(), "and the palette is gone");
}

/// The workspaces are on the floating bar at the left of the page, not in a bar across the top,
/// and the page starts right under the tab strip.
#[test]
fn the_workspaces_float_at_the_left_and_the_page_starts_under_the_tabs() {
    let mut h = with_document();
    let strip = h.get_by_label("Home").rect().bottom();
    // The bar floats at the page area's left edge (right of the tools panel, in All tools).
    let left = h.state().views[0].viewport_rect().left();
    for label in ["All tools", "Read", "Edit", "Convert", "E-Sign"] {
        // All tools also names its panel of tools; the workspace button is the one on the bar.
        let on_bar = h.query_all_by_label(label).map(|n| n.rect()).any(|r| r.left() >= left && r.right() < left + 80.0 && r.top() > strip);
        assert!(on_bar, "{label} is on the left bar at {left}");
    }
    h.get_by_label("Read").click();
    h.run_steps(4);
    assert_eq!(h.state().mode, Mode::Read);
    let page_area = h.state().views[0].viewport_rect();
    // Home sits centred in the strip, a few points above its edge; the old mode bar added 48.
    assert!(
        page_area.top() >= strip && page_area.top() - strip < 8.0,
        "the page area {page_area:?} starts under the tab strip, whose Home button ends at {strip}"
    );
    h.get_by_label("All tools").click();
    h.run_steps(4);
    assert_eq!(h.state().mode, Mode::AllTools);
}

/// The Menu is an icon in the tab strip, right after Home.
#[test]
fn the_menu_is_an_icon_after_home() {
    let mut h = harness(|_| {});
    let home = h.get_by_label("Home").rect();
    let menu = h.get_by_label("Menu").rect();
    assert!(menu.left() >= home.right() && menu.left() - home.right() < 12.0, "Menu {menu:?} follows Home {home:?}");
    assert!((menu.center().y - home.center().y).abs() < 1.0, "on the same line");
    assert!(menu.width() < 40.0, "an icon, not a labelled button: {menu:?}");
    h.get_by_label("Menu").click();
    h.run_steps(3);
    h.get_by_label("File ⏵");
}

/// Where the app draws the window frame (Windows, GNOME), the tab strip is the window's only bar
/// at the top: Home, Menu and the tabs, then the window buttons at its end.
#[test]
fn one_bar_carries_the_tabs_and_the_window_buttons() {
    use pdfcraft_ui_egui::window_controls::{Layout, Style};
    let mut h = harness(|app| {
        app.window_controls = Some(Layout::default());
        app.open_bytes("one.pdf", None, fixture()).unwrap();
    });
    h.get_by_label("Read").click();
    h.run_steps(4);
    let home = h.get_by_label("Home").rect();
    let tab = h.get_by_label("one.pdf").rect();
    let (min, max, close) = (h.get_by_label("Minimize").rect(), h.get_by_label("Maximize").rect(), h.get_by_label("Close").rect());
    for (name, r) in [("Home", home), ("tab", tab), ("Minimize", min), ("Maximize", max), ("Close", close)] {
        assert!(r.top() >= 0.0 && r.bottom() <= 40.0, "{name} at {r:?} is in the one bar at the top");
    }
    assert!(home.right() < tab.left() && tab.right() < min.left(), "Home, the tabs, then the window buttons");
    assert!(min.right() <= max.left() && max.right() <= close.left(), "minimize, maximize, close");
    if Style::native() == Style::Windows {
        // Windows' caption buttons fill the corner: full height, flush with the window's edge.
        assert!((close.right() - WIDTH).abs() < 0.5 && close.top().abs() < 0.5, "Close {close:?} is in the corner");
        assert!(close.height() >= 37.0, "Close {close:?} is as tall as the bar");
    } else {
        assert!(close.right() <= WIDTH);
    }
    let page_area = h.state().views[0].viewport_rect();
    assert!(page_area.top() <= 40.0, "the page starts under the one bar: {page_area:?}");
    // Use system title bar gives the frame back to the desktop, and the buttons go.
    h.state_mut().system_title_bar = true;
    h.run_steps(4);
    assert!(h.query_by_label("Minimize").is_none());
}
