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
