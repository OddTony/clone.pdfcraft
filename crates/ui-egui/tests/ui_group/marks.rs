//! Header & footer, watermark and background dialogs in the real shell (egui_kittest).

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use pdfcraft_engine::MarkKind;
use pdfcraft_ui_egui::{Dialog, PdfCraftApp};

const FIXTURE: &[u8] = b"%PDF-1.7
1 0 obj << /Type /Catalog /Pages 2 0 R >> endobj
2 0 obj << /Type /Pages /Kids [3 0 R 4 0 R 5 0 R] /Count 3 /MediaBox [0 0 300 400] >> endobj
3 0 obj << /Type /Page /Parent 2 0 R >> endobj
4 0 obj << /Type /Page /Parent 2 0 R >> endobj
5 0 obj << /Type /Page /Parent 2 0 R >> endobj
trailer << /Root 1 0 R >>
%%EOF";

fn harness() -> Harness<'static, PdfCraftApp> {
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(|_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.open_bytes("doc.pdf", None, FIXTURE.to_vec()).unwrap();
        app
    });
    h.run_steps(4);
    h
}

fn marks(h: &Harness<'static, PdfCraftApp>) -> Vec<MarkKind> {
    let s = h.state();
    s.session.get(s.views[0].id).unwrap().marks().to_vec()
}

#[test]
fn header_and_footer_dialog_inserts_tokens_and_applies_to_a_subset() {
    let mut h = harness();
    assert!(!h.state_mut().execute("edit.header_footer.remove"), "nothing to remove yet");
    assert!(h.state_mut().execute("edit.header_footer"));
    h.run_steps(3);
    assert_eq!(h.state().dialog, Some(Dialog::Marks(MarkKind::HeaderFooter)));
    // The center footer box takes the page-number token.
    h.state_mut().marks_draft.focused_box = 4;
    h.get_by_label("Insert Page Number").click();
    h.run_steps(2);
    assert_eq!(h.state().marks_draft.hf.text[4], "<<1>>");
    h.state_mut().marks_draft.range.subset = pdfcraft_ui_egui::marks::Subset::Odd;
    h.run_steps(2);
    h.get_by_label("OK").click();
    h.run_steps(4);
    assert_eq!(h.state().dialog, None);
    assert_eq!(marks(&h), [MarkKind::HeaderFooter]);
    // Update and Remove are now available.
    assert!(h.state_mut().execute("edit.header_footer.update"));
    h.run_steps(2);
    h.get_by_label("Update Header and Footer");
    h.get_by_label("Cancel").click();
    h.run_steps(2);
    assert!(h.state_mut().execute("edit.header_footer.remove"));
    assert!(marks(&h).is_empty());
}

#[test]
fn watermark_and_background_dialogs() {
    let mut h = harness();
    h.state_mut().execute("edit.watermark");
    h.run_steps(2);
    assert!(h.query_by(|n| n.label().as_deref() == Some("OK") && n.is_disabled()).is_some(), "OK waits for text");
    h.get_by_label("Offset from centre");
    h.state_mut().marks_draft.wm.text = "DRAFT".into();
    h.run_steps(2);
    h.get_by_label("OK").click();
    h.run_steps(3);
    h.state_mut().execute("edit.background");
    h.run_steps(2);
    h.get_by_label("OK").click();
    h.run_steps(3);
    assert_eq!(marks(&h).len(), 2);
}

#[test]
fn backgrounds_and_watermarks_from_a_file() {
    let mut h = harness();
    h.state_mut().execute("edit.background");
    h.run_steps(2);
    h.get_by_label("File").click();
    h.run_steps(2);
    h.get_by_label("No file chosen");
    assert!(h.query_by(|n| n.label().as_deref() == Some("OK") && n.is_disabled()).is_some(), "OK waits for a file");
    // (Browse… opens the system picker; the test supplies the file directly.)
    h.state_mut().marks_draft.file = Some(("letterhead.pdf".into(), std::sync::Arc::new(FIXTURE.to_vec())));
    h.run_steps(2);
    h.get_by_label("Page number");
    h.get_by_label("Scale relative to target page");
    h.get_by_label("OK").click();
    h.run_steps(3);
    assert_eq!(marks(&h), vec![MarkKind::Background]);
    let s = h.state();
    assert_eq!(s.session.get(s.views[0].id).unwrap().can_undo(), Some("Add background"));
}

/// A leading white page fill does not require blending: the normal background stays below
/// original content and the dialog leaves Multiply as an explicit choice.
#[test]
fn background_dialog_places_below_content_on_pages_that_paint_themselves_white() {
    let content = "q 1 g 0 0 300 400 re f Q 0 g 40 140 20 20 re f";
    let pdf = format!(
        "%PDF-1.7
1 0 obj << /Type /Catalog /Pages 2 0 R >> endobj
2 0 obj << /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 300 400] >> endobj
3 0 obj << /Type /Page /Parent 2 0 R /Contents 4 0 R >> endobj
4 0 obj << /Length {} >> stream
{content}
endstream endobj
trailer << /Root 1 0 R >>
%%EOF",
        content.len()
    );
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(move |_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.open_bytes("white.pdf", None, pdf.clone().into_bytes()).unwrap();
        app
    });
    h.run_steps(4);
    h.state_mut().execute("edit.background");
    h.run_steps(2);
    assert!(!h.state().marks_draft.bg.multiply, "normal placement preserves foreground colours");
    h.get_by_label("Behind text and images");
    assert!(h.query_by_label_contains("opaque background of its own").is_none());
    h.get_by_label("Blend with page content").click();
    h.run_steps(2);
    assert!(h.state().marks_draft.bg.multiply);
    h.get_by_label("Behind text and images").click();
    h.run_steps(2);
    assert!(!h.state().marks_draft.bg.multiply);
    h.get_by_label("OK").click();
    h.run_steps(3);
    assert_eq!(marks(&h), vec![MarkKind::Background]);
    let s = h.state();
    let bytes = s.session.get(s.views[0].id).unwrap().bytes.clone();
    // Rendered: the default cream background on the page's white, the black square still black.
    let mut r = pdfcraft_render::PageRenderer::new(bytes, pdfcraft_render::RenderConfig::default());
    let p = r.render(pdfcraft_render::RenderRequest { page: 0, scale: 1.0, ..Default::default() });
    assert!(p.error.is_none(), "{:?}", p.error);
    let px = |x: u32, y: u32| p.rgba[((y * p.width + x) * 4) as usize..][..4].to_vec();
    assert_eq!(px(5, 5), vec![255, 255, 217, 255], "the background shows on the white");
    assert_eq!(px(50, 400 - 150), vec![0, 0, 0, 255], "the content stays black");
}

/// On an ordinary page the dialog says nothing and keeps a background behind the content.
#[test]
fn background_dialog_stays_behind_on_ordinary_pages() {
    let mut h = harness();
    h.state_mut().execute("edit.background");
    h.run_steps(2);
    assert!(!h.state().marks_draft.bg.multiply && !h.state().marks_draft.hides_background);
    h.get_by_label("Blend with page content");
    assert!(h.query_by_label_contains("opaque background of its own").is_none());
}
