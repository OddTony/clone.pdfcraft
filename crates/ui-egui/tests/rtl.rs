//! The window in a right-to-left interface language (Arabic): the chrome is mirrored, the page
//! is not, and the left-to-right languages are laid out as before.

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use pdfcraft_ui_egui::{PdfCraftApp, i18n};

/// A tiny PDF with two pages, two bookmarks, one sticky note and a text field.
const FIXTURE: &[u8] = b"%PDF-1.7
1 0 obj << /Type /Catalog /Pages 2 0 R /Outlines 6 0 R /AcroForm << /Fields [10 0 R] >> >> endobj
2 0 obj << /Type /Pages /Kids [3 0 R 4 0 R] /Count 2 >> endobj
3 0 obj << /Type /Page /Parent 2 0 R /MediaBox [0 0 300 400] /Annots [9 0 R 10 0 R] >> endobj
4 0 obj << /Type /Page /Parent 2 0 R /MediaBox [0 0 300 400] >> endobj
6 0 obj << /Type /Outlines /First 7 0 R /Last 8 0 R /Count 2 >> endobj
7 0 obj << /Title (Alpha section) /Parent 6 0 R /Next 8 0 R /Dest [3 0 R /Fit] /First 11 0 R /Last 11 0 R /Count 1 >> endobj
8 0 obj << /Title (Beta section) /Parent 6 0 R /Prev 7 0 R /Dest [4 0 R /Fit] >> endobj
9 0 obj << /Type /Annot /Subtype /Text /Rect [10 370 30 390] /T (Tester) /Contents (Check the numbers) >> endobj
10 0 obj << /Type /Annot /Subtype /Widget /FT /Tx /T (name) /Rect [40 300 200 320] /P 3 0 R >> endobj
11 0 obj << /Title (Alpha detail) /Parent 7 0 R /Dest [3 0 R /Fit] >> endobj
trailer << /Root 1 0 R >>
%%EOF";

const WIDTH: f32 = 1400.0;

fn harness(language: &'static str, setup: impl FnOnce(&mut PdfCraftApp) + 'static) -> Harness<'static, PdfCraftApp> {
    let mut h = Harness::builder().with_size(egui::vec2(WIDTH, 900.0)).build_eframe(move |_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", language).unwrap();
        setup(&mut app);
        app
    });
    // Fonts install on frame 1 and apply on frame 2; a mirrored island settles a frame later.
    h.run_steps(6);
    h
}

fn with_document(language: &'static str) -> Harness<'static, PdfCraftApp> {
    harness(language, |app| app.open_bytes("sample.pdf", None, FIXTURE.to_vec()).unwrap())
}

/// `text` as the interface shows it in `language`.
fn shown(language: &str, text: &'static str) -> &'static str {
    i18n::tr(i18n::Lang::from_code(language).unwrap(), text)
}

/// The horizontal centre of the first widget labelled `text` (in `language`).
fn x_of(h: &Harness<'static, PdfCraftApp>, language: &str, text: &'static str) -> f32 {
    let label = shown(language, text);
    h.query_all_by_label(label).next().unwrap_or_else(|| panic!("no widget labelled {text:?} in {language}")).rect().center().x
}

/// The page raster on screen: the largest textured mesh, as (its bounds, the texture u at its
/// left edge). Pages are rendered off the UI thread, so this waits for one.
fn page_on_screen(h: &mut Harness<'static, PdfCraftApp>) -> (egui::Rect, f32) {
    for _ in 0..400 {
        let page = h
            .output()
            .shapes
            .iter()
            .filter_map(|clipped| match &clipped.shape {
                egui::Shape::Mesh(mesh) if mesh.texture_id != egui::TextureId::default() => {
                    let bounds = mesh.calc_bounds();
                    let leftmost = mesh.vertices.iter().min_by(|a, b| a.pos.x.total_cmp(&b.pos.x))?;
                    Some((bounds, leftmost.uv.x))
                }
                _ => None,
            })
            .max_by(|a, b| a.0.area().total_cmp(&b.0.area()));
        if let Some(page) = page.filter(|(bounds, _)| bounds.width() > 100.0) {
            return page;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
        h.run_steps(1);
    }
    panic!("no page was drawn");
}

/// Where `label` is painted, among the text inside `within`.
fn painted_label(h: &Harness<'static, PdfCraftApp>, label: &str, within: egui::Rect) -> egui::Rect {
    h.output()
        .shapes
        .iter()
        .find_map(|clipped| match &clipped.shape {
            egui::Shape::Text(text) if text.galley.text() == label && within.expand(1.0).contains(text.visual_bounding_rect().center()) => {
                Some(text.visual_bounding_rect())
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("{label:?} is not painted in {within:?}"))
}

#[test]
fn arabic_is_the_only_mirrored_language() {
    for lang in i18n::Lang::all() {
        assert_eq!(lang.rtl(), lang.code() == "ar", "{}", lang.code());
    }
}

#[test]
fn the_window_reads_from_the_right_in_arabic_and_from_the_left_otherwise() {
    let middle = WIDTH / 2.0;
    for language in ["en", "fr", "ja"] {
        let h = with_document(language);
        assert!(x_of(&h, language, "Menu") < middle, "{language}: the menu starts the bar on the left");
        assert!(x_of(&h, language, "Find tools and commands") > middle, "{language}: the search box ends the bar on the right");
        assert!(x_of(&h, language, "Export a PDF") < middle, "{language}: the tool panel is on the left");
        assert!(x_of(&h, language, "Page thumbnails") > middle, "{language}: the rail is on the right");
        assert!(x_of(&h, language, "Home") < x_of(&h, language, "Open"), "{language}: Home comes before the tabs");
    }
    let h = with_document("ar");
    assert!(x_of(&h, "ar", "Menu") > middle, "the menu starts the bar on the right");
    assert!(x_of(&h, "ar", "Find tools and commands") < middle, "the search box ends the bar on the left");
    assert!(x_of(&h, "ar", "Export a PDF") > middle, "the tool panel is on the right");
    assert!(x_of(&h, "ar", "Page thumbnails") < middle, "the rail is on the left");
    assert!(x_of(&h, "ar", "Home") > x_of(&h, "ar", "Open"), "Home comes before the tabs, from the right");
    // The window is as wide as before: nothing was pushed outside it.
    for text in ["Menu", "Find tools and commands", "Export a PDF", "Page thumbnails", "Home", "Open"] {
        let x = x_of(&h, "ar", text);
        assert!((0.0..=WIDTH).contains(&x), "{text} is drawn at {x}");
    }
}

#[test]
fn the_mode_tabs_run_in_reading_order() {
    let tabs = ["All tools", "Read", "Edit", "Convert", "E-Sign"];
    let en = harness("en", |_| {});
    let ar = harness("ar", |_| {});
    for pair in tabs.windows(2) {
        assert!(x_of(&en, "en", pair[0]) < x_of(&en, "en", pair[1]), "{pair:?} in English");
        assert!(x_of(&ar, "ar", pair[0]) > x_of(&ar, "ar", pair[1]), "{pair:?} in Arabic");
    }
}

#[test]
fn the_page_is_never_mirrored() {
    let mut en = with_document("en");
    let mut ar = with_document("ar");
    let (en_page, en_u) = page_on_screen(&mut en);
    let (ar_page, ar_u) = page_on_screen(&mut ar);
    assert!(en_u < 0.5 && ar_u < 0.5, "the left edge of the page shows the left edge of its raster ({en_u}, {ar_u})");
    assert!((en_page.width() - ar_page.width()).abs() < 1.0 && (en_page.height() - ar_page.height()).abs() < 1.0, "{en_page:?} / {ar_page:?}");
    // Between the side panels, which swapped: the page moved by the difference of their widths.
    assert!(ar_page.left() > 0.0 && ar_page.right() < WIDTH, "{ar_page:?}");
}

#[test]
fn side_panels_swap_sides() {
    let mut en = harness("en", |app| {
        app.open_bytes("sample.pdf", None, FIXTURE.to_vec()).unwrap();
        app.set_option("panel", "bookmarks").unwrap();
    });
    let mut ar = harness("ar", |app| {
        app.open_bytes("sample.pdf", None, FIXTURE.to_vec()).unwrap();
        app.set_option("panel", "bookmarks").unwrap();
    });
    let middle = WIDTH / 2.0;
    // A bookmark's row is in the panel, which is on the right in English and on the left in Arabic.
    assert!(en.get_by_label("Alpha section").rect().center().x > middle);
    assert!(ar.get_by_label("Alpha section").rect().center().x < middle);
    // A row starts with its title: at the left in English, at the right in Arabic. A row with
    // children is mirrored once, not once more for each level under it.
    for (h, from_the_right) in [(&en, false), (&ar, true)] {
        for title in ["Alpha section", "Beta section"] {
            let row = h.get_by_label(title).rect();
            let text = painted_label(h, title, row);
            assert_eq!(row.right() - text.right() < text.left() - row.left(), from_the_right, "{title}: {text:?} in {row:?}");
        }
    }
    // The document sits between the panels in both.
    let (en_page, _) = page_on_screen(&mut en);
    let (ar_page, _) = page_on_screen(&mut ar);
    assert!(en_page.right() < en.get_by_label("Alpha section").rect().left());
    assert!(ar_page.left() > ar.get_by_label("Alpha section").rect().right());
}

#[test]
fn dialog_buttons_end_on_the_left_in_arabic() {
    let open = |language: &'static str| {
        harness(language, |app| {
            app.open_bytes("sample.pdf", None, FIXTURE.to_vec()).unwrap();
            app.set_option("dialog", "protect").unwrap();
        })
    };
    let (en, ar) = (open("en"), open("ar"));
    // The title starts the dialog and the buttons end it: opposite sides, swapped in Arabic.
    assert!(x_of(&en, "en", "Protect Using Password") < x_of(&en, "en", "Cancel"));
    assert!(x_of(&ar, "ar", "Protect Using Password") > x_of(&ar, "ar", "Cancel"));
    // The default button is the outermost one.
    assert!(x_of(&en, "en", "Apply") > x_of(&en, "en", "Cancel"));
    assert!(x_of(&ar, "ar", "Apply") < x_of(&ar, "ar", "Cancel"));
    // The dialog is no wider for it.
    let width = |h: &Harness<'static, PdfCraftApp>, language: &str| (x_of(h, language, "Protect Using Password") - x_of(h, language, "Apply")).abs();
    assert!(width(&ar, "ar") < 560.0, "{}", width(&ar, "ar"));
}

#[test]
fn the_main_menu_opens_under_its_button_and_reads_from_the_right() {
    let mut h = with_document("ar");
    h.get_by_label(shown("ar", "Menu")).click();
    h.run_steps(4);
    let menu = h.get_by_label(shown("ar", "Menu")).rect();
    let file = h.get_by_label(shown("ar", "File")).rect();
    let view = h.get_by_label(shown("ar", "View")).rect();
    // Items share a right edge near the button's, not a left one at it.
    assert!((file.right() - view.right()).abs() < 1.0, "{file:?} / {view:?}");
    assert!(file.right() > menu.center().x && file.left() < menu.left(), "{file:?} under {menu:?}");
    assert!(file.right() <= WIDTH, "{file:?}");
    // A submenu opens and draws.
    h.get_by_label(shown("ar", "File")).click();
    h.run_steps(4);
    let open = h.get_by_label_contains(shown("ar", "Open…")).rect();
    assert!(open.width() > 0.0 && open.left() >= 0.0 && open.right() <= WIDTH, "{open:?}");
    // On the side reading goes to: left of the menu it came from, not over it.
    let file = h.get_by_label(shown("ar", "File")).rect();
    assert!(open.right() <= file.left() + 1.0, "{open:?} beside {file:?}");
}

/// Everything `set_option` can show, drawn for a few frames: a layout egui cannot do from the
/// right (a grid, a sideways scroll area) asserts in debug builds, so this walks all of them.
#[test]
fn every_panel_dialog_and_tool_draws_in_arabic() {
    let mut h = with_document("ar");
    let step = |h: &mut Harness<'static, PdfCraftApp>, key: &str, value: &str| {
        h.state_mut().set_option(key, value).unwrap_or_else(|e| panic!("{key}={value}: {e}"));
        h.run_steps(4);
    };
    for panel in ["comments", "bookmarks", "pages", "fields", "layers", "attachments", "signatures", "accessibility", "search", "compare", "none"] {
        step(&mut h, "panel", panel);
    }
    for dialog in [
        "properties",
        "security",
        "fonts",
        "advanced",
        "shortcuts",
        "split",
        "protect",
        "page-boxes",
        "header-footer",
        "watermark",
        "background",
        "export-image",
        "export-text",
        "export-all-images",
        "accessibility-options",
        "recognize-text",
        "js-console",
        "document-js",
        "preferences",
        "compare-files",
        "action-wizard",
        "pdfa",
        "signature",
        "optimize",
        "number-pages",
        "none",
    ] {
        step(&mut h, "dialog", dialog);
    }
    for mode in ["read", "edit", "convert", "sign", "all"] {
        step(&mut h, "mode", mode);
    }
    for quick in ["hand", "crop", "redact", "add-text", "edit-text", "link", "sign", "marquee-zoom", "snapshot", "select"] {
        step(&mut h, "quick", quick);
    }
    step(&mut h, "find", "Alpha");
    step(&mut h, "organize", "on");
    step(&mut h, "organize", "off");
    step(&mut h, "palette", "open");
    step(&mut h, "home", "true");
    // Back to a left-to-right language and to Arabic again, with everything still open.
    step(&mut h, "language", "en");
    step(&mut h, "language", "ar");
}

#[test]
fn every_tool_panel_draws_in_arabic() {
    let mut h = with_document("ar");
    for group in pdfcraft_engine::catalog::TOOL_GROUPS.iter() {
        h.state_mut().set_option("tool", group.id).unwrap_or_else(|e| panic!("tool={}: {e}", group.id));
        h.run_steps(4);
    }
}

#[test]
fn tool_rows_lead_with_their_icon() {
    let row_and_label = |language: &'static str| {
        let h = with_document(language);
        let label = shown(language, "Export a PDF");
        let row = h.query_all_by_label(label).next().unwrap().rect();
        (row, painted_label(&h, label, row))
    };
    let (row, text) = row_and_label("en");
    assert!(text.center().x - row.left() < row.right() - text.center().x, "English: the label follows the icon from the left ({text:?} in {row:?})");
    let (row, text) = row_and_label("ar");
    assert!(row.right() - text.center().x < text.center().x - row.left(), "Arabic: the label follows the icon from the right ({text:?} in {row:?})");
    assert!(row.contains_rect(text), "{text:?} stays in {row:?}");
}

#[test]
fn a_check_box_has_its_box_on_the_right_and_is_still_one_widget() {
    const LABEL: &str = "Delete pages after extracting";
    let open = |language: &'static str| {
        harness(language, |app| {
            app.open_bytes("sample.pdf", None, FIXTURE.to_vec()).unwrap();
            app.execute("page.extract");
        })
    };
    for (language, box_on_the_right) in [("en", false), ("ar", true)] {
        let mut h = open(language);
        let label = shown(language, LABEL);
        // One widget carries the name (a box with a separate label would be two).
        let boxes: Vec<_> = h.query_all_by_label(label).map(|node| node.rect()).collect();
        assert_eq!(boxes.len(), 1, "{language}: {boxes:?}");
        let text = painted_label(&h, label, boxes[0]);
        assert_eq!(text.center().x < boxes[0].center().x, box_on_the_right, "{language}: {text:?} in {:?}", boxes[0]);
        assert!(boxes[0].expand(1.0).contains_rect(text), "{language}: {text:?} in {:?}", boxes[0]);
        // Clicking it anywhere toggles it.
        assert!(!h.state().extract_draft.delete);
        h.get_by_label(label).click();
        h.run_steps(3);
        assert!(h.state().extract_draft.delete, "{language}");
    }
}

/// GNOME on Wayland, where the tab strip carries the window buttons: they stay on the side the
/// desktop names and in its order, whichever way the strip reads.
#[test]
fn window_buttons_keep_the_desktops_side_and_order() {
    use pdfcraft_ui_egui::window_controls::Layout;
    for language in ["en", "ar"] {
        let h = harness(language, |app| app.window_controls = Some(Layout::default()));
        let (minimize, maximize) = (x_of(&h, language, "Minimize"), x_of(&h, language, "Maximize"));
        assert!(minimize > WIDTH - 120.0, "{language}: the window buttons are at the right end, minimize at {minimize}");
        assert!(minimize < maximize, "{language}: minimize ({minimize}) comes before maximize ({maximize})");
        let home = x_of(&h, language, "Home");
        assert_eq!(home < WIDTH / 2.0, language == "en", "{language}: Home at {home}");
    }
}
