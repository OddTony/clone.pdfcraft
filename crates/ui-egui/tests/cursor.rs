//! Page cursors and hover tips follow egui's view of what is under the pointer: a page under the
//! palette, the quick bar, a modal prompt or the chrome (scrolled up under it) shows neither.

use egui_kittest::Harness;
use pdfcraft_engine::StampKind;
use pdfcraft_ui_egui::{PdfCraftApp, QuickTool, UnlockPrompt};

/// One 300×400 page with a web link at x 50..250, y 300..350.
fn fixture() -> Vec<u8> {
    let objs = [
        "<< /Type /Catalog /Pages 2 0 R >>",
        "<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 300 400] >>",
        "<< /Type /Page /Parent 2 0 R /Annots [4 0 R] >>",
        "<< /Type /Annot /Subtype /Link /Rect [50 300 250 350] /Border [0 0 0] /A << /S /URI /URI (https://example.org/) >> >>",
    ];
    let mut out = b"%PDF-1.7\n".to_vec();
    let mut offsets = Vec::new();
    for (i, o) in objs.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n{o}\nendobj\n", i + 1).as_bytes());
    }
    let xref = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).as_bytes());
    for o in offsets {
        out.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objs.len() + 1).as_bytes());
    out
}

fn harness(zoom: &'static str) -> Harness<'static, PdfCraftApp> {
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(move |_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.open_bytes("link.pdf", None, fixture()).unwrap();
        app.set_option("zoom", zoom).unwrap();
        app
    });
    for _ in 0..30 {
        h.run_steps(2);
    }
    h
}

fn stamp(h: &mut Harness<'_, PdfCraftApp>) {
    h.state_mut().quick_tool = QuickTool::Stamp(StampKind::DynApproved);
    h.run_steps(4);
}

fn page(h: &Harness<'_, PdfCraftApp>) -> egui::Rect {
    h.state().views[0].page_screen_rect(0).expect("page on screen")
}

fn cursor_at(h: &mut Harness<'_, PdfCraftApp>, p: egui::Pos2) -> egui::CursorIcon {
    h.hover_at(p);
    h.run_steps(2);
    h.output().platform_output.cursor_icon
}

fn wheel(h: &mut Harness<'_, PdfCraftApp>, delta: egui::Vec2) {
    h.hover_at(egui::pos2(800.0, 500.0));
    h.event(egui::Event::MouseWheel { unit: egui::MouseWheelUnit::Point, delta, modifiers: egui::Modifiers::NONE, phase: egui::TouchPhase::Move });
    for _ in 0..10 {
        h.run_steps(2);
    }
}

fn tip_shown(h: &Harness<'_, PdfCraftApp>) -> bool {
    let tip = egui::LayerId::new(egui::Order::Tooltip, egui::Id::new("canvas-hover"));
    h.ctx.memory(|m| m.areas().is_visible(&tip))
}

#[test]
fn stamp_cursor_stays_off_the_command_palette() {
    let mut h = harness("300");
    stamp(&mut h);
    h.state_mut().palette_open = true;
    h.run_steps(4);
    let palette = h.ctx.memory(|m| m.area_rect(egui::Id::new("palette"))).expect("palette shown");
    // On the palette's frame margin, where none of its widgets sets a cursor, over the page.
    let p = egui::pos2(palette.left() + 4.0, palette.bottom() - 4.0);
    assert!(page(&h).contains(p), "the palette must lie over the page here");
    assert_eq!(cursor_at(&mut h, p), egui::CursorIcon::Default);
    // The page beside the palette still shows the tool.
    let beside = egui::pos2(palette.left() + 4.0, palette.bottom() + 40.0);
    assert!(page(&h).contains(beside));
    assert_eq!(cursor_at(&mut h, beside), egui::CursorIcon::Crosshair);
}

#[test]
fn stamp_cursor_stays_off_the_quick_bar() {
    let mut h = harness("400");
    stamp(&mut h);
    // Bring the page under the quick bar's left end.
    wheel(&mut h, egui::vec2(-200.0, 0.0));
    let bar = h.ctx.memory(|m| m.area_rect(egui::Id::new("quick-bar"))).expect("quick bar shown");
    // Inside the bar's frame margin: no button there.
    let p = egui::pos2(bar.left() + 3.0, bar.bottom() - 3.0);
    assert!(page(&h).contains(p), "the quick bar must lie over the page here");
    assert_eq!(cursor_at(&mut h, p), egui::CursorIcon::Default);
    let below = egui::pos2(bar.left() + 3.0, bar.bottom() + 20.0);
    assert!(page(&h).contains(below));
    assert_eq!(cursor_at(&mut h, below), egui::CursorIcon::Crosshair);
}

#[test]
fn stamp_cursor_stays_off_the_chrome_above_a_page_scrolled_under_it() {
    let mut h = harness("400");
    stamp(&mut h);
    let top = page(&h).top();
    wheel(&mut h, egui::vec2(0.0, -300.0));
    let page = page(&h);
    // Above where the page started (the viewport's top edge, give or take its margin): the
    // toolbar, with the page now scrolled up underneath it.
    // Left of the link, which would show the hand instead of the tool's cursor.
    let x = page.left() + 20.0;
    let p = egui::pos2(x, top - 45.0);
    assert!(page.contains(p), "the page must extend under the chrome here");
    // The toolbar keeps its own cursor (a button's hand here); the page's is gone.
    assert_ne!(cursor_at(&mut h, p), egui::CursorIcon::Crosshair);
    let inside = egui::pos2(x, top + 40.0);
    assert_eq!(cursor_at(&mut h, inside), egui::CursorIcon::Crosshair);
}

#[test]
fn link_tip_stays_under_the_unlock_prompt() {
    let mut h = harness("300");
    let page = page(&h);
    let k = page.width() / 300.0;
    // Near the link's top left corner, clear of the prompt's card in the middle of the window.
    let on_link = page.min + egui::vec2(55.0 * k, (400.0 - 345.0) * k);
    assert_eq!(cursor_at(&mut h, on_link), egui::CursorIcon::PointingHand);
    h.run_steps(2);
    assert!(tip_shown(&h), "the link's tip shows over an uncovered page");

    let doc = h.state().views[0].id;
    h.state_mut().unlock_prompt = Some(UnlockPrompt { doc, name: "link.pdf".into(), input: String::new(), error: None, then: None });
    h.hover_at(on_link + egui::vec2(1.0, 0.0));
    for _ in 0..4 {
        h.run_steps(2);
    }
    let card = h.ctx.memory(|m| m.area_rect(egui::Id::new("unlock")));
    assert!(card.is_none_or(|c| !c.contains(on_link)), "the point must be on the backdrop, not the card: {card:?}");
    assert!(!tip_shown(&h), "the link's tip must not float over the Unlock prompt");
    assert_ne!(h.output().platform_output.cursor_icon, egui::CursorIcon::PointingHand);
}
