//! Cursor leak regressions: overlays drawn over the canvas (the mode bar, tab strip, side
//! panels, toolbars, palettes, dialogs, popups) must set their own cursor when the pointer is
//! over a clickable. The canvas also runs hover detection every frame and would otherwise
//! leak `Text` / `Resize*` / `Move` cursors through them — last `set_cursor_icon` wins per frame.
//!
//! The fix lives in `crate::canvas_cursor::set`, which guards the canvas's cursor writes with
//! `canvas_owns_pointer`: the pointer must be inside the canvas's clip rect (so the mode bar
//! band is excluded) and no popup / dialog / tooltip layer must sit on top.

use egui::Pos2;
use egui_kittest::Harness;
use pdfcraft_ui_egui::{Mode, PdfCraftApp};

/// Bring the app up with a fresh 1-page document and the Edit mode engaged.
fn edit_harness() -> Harness<'static, PdfCraftApp> {
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(|_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        // A single page, 200×300, with the text "Page 1" — same fixture editing.rs uses so the
        // hovering on its text shows the I-beam in Edit mode.
        app.open_bytes(
            "doc.pdf",
            None,
            b"%PDF-1.7\n\
              1 0 obj\n<< /Type /Catalog /Pages 2 0 R >>\nendobj\n\
              2 0 obj\n<< /Type /Pages /Kids [4 0 R] /Count 1 /MediaBox [0 0 200 300] >>\nendobj\n\
              3 0 obj\n<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>\nendobj\n\
              4 0 obj\n<< /Type /Page /Parent 2 0 R /Contents 5 0 R /Resources << /Font << /F1 3 0 R >> >> >>\nendobj\n\
              5 0 obj\n<< /Length 35 >>\nstream\nBT /F1 24 Tf 20 150 Td (Page 1) Tj ET\nendstream\nendobj\n\
              xref\n0 6\n0000000000 65535 f \n\
              0000000010 00000 n \n0000000060 00000 n \n0000000120 00000 n \n0000000230 00000 n \n0000000340 00000 n \n\
              trailer\n<< /Size 6 /Root 1 0 R >>\nstartxref\n430\n%%EOF\n"
                .to_vec(),
        )
        .expect("fixture opens");
        app
    });
    h.state_mut().mode = Mode::Edit;
    h.run_steps(4);
    h
}

/// Where the pointer should sit so the canvas reports `CursorIcon::Text` on the underlying
/// text — the leak that the guard was added to prevent.
fn text_under_page(h: &Harness<'static, PdfCraftApp>) -> Pos2 {
    let r = h.state().views[0].page_screen_rect(0).expect("page on screen");
    // The fixture draws "Page 1" around user-space (20, 150) on a 200×300 page, so the same
    // fraction in screen space lands on the text.
    egui::pos2(r.left() + 40.0 / 200.0 * r.width(), r.top() + 150.0 / 300.0 * r.height())
}

/// Background layer at the top of the window: the mode bar (`Panel::top("mode_bar")` is 48px
/// tall). Hovering here with a text-bearing page underneath used to leak the I-beam cursor.
#[test]
fn mode_bar_does_not_leak_text_cursor_through_its_band() {
    let mut h = edit_harness();
    let on_text = text_under_page(&h);
    // Sanity check the regression we are guarding against: on the text, the canvas correctly
    // offers the I-beam. If this ever stops being `Text`, the rest of the test is meaningless.
    h.hover_at(on_text);
    h.run_steps(2);
    assert_eq!(h.output().platform_output.cursor_icon, egui::CursorIcon::Text);
    // Now the bar band: same x as the text, but above the page (the mode bar is 48 px tall and
    // covers the window's top, so any y at most 24 is in the bar and outside the canvas clip).
    let above_text = egui::pos2(on_text.x, 24.0);
    assert!(above_text.y < h.state().views[0].page_screen_rect(0).unwrap().top(), "target is above the page (inside the mode bar)");
    h.hover_at(above_text);
    h.run_steps(2);
    let icon = h.output().platform_output.cursor_icon;
    assert_ne!(icon, egui::CursorIcon::Text, "the mode bar must not leak the I-beam through");
    // The mode bar's clickable options call `on_hover_cursor(PointingHand)` (via
    // `widgets::mode_tab` / `widgets::ghost_button` / etc.), so an option hovered on would
    // show the pointing hand. Hovering a non-clickable strip of the bar (between options)
    // shows nothing — `Default`. Both are acceptable — only the canvas cursor leak is not.
    assert!(matches!(icon, egui::CursorIcon::Default | egui::CursorIcon::PointingHand), "the bar band stays neutral ({icon:?})");
}
