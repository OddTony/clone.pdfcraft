//! The floating formatting toolbar of Edit text (Preferences ▸ Show the formatting toolbar when
//! editing text) and Word's paragraph-direction buttons, in the toolbar and the Format text panel.

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use pdfcraft_ui_egui::PdfCraftApp;

/// One page showing "Page 1" in Helvetica 24 pt at (20, 150) on a 200 × 300 page.
fn fixture() -> Vec<u8> {
    let objs: Vec<String> = vec![
        "<< /Type /Catalog /Pages 2 0 R >>".into(),
        "<< /Type /Pages /Kids [4 0 R] /Count 1 /MediaBox [0 0 200 300] >>".into(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".into(),
        "<< /Type /Page /Parent 2 0 R /Contents 5 0 R /Resources << /Font << /F1 3 0 R >> >> >>".into(),
        {
            let body = "BT /F1 24 Tf 20 150 Td (Page 1) Tj ET";
            format!("<< /Length {} >>\nstream\n{body}\nendstream", body.len())
        },
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

/// The app with "Page 1" open for editing.
fn editing(setup: impl FnOnce(&mut PdfCraftApp) + 'static) -> Harness<'static, PdfCraftApp> {
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(move |_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.open_bytes("doc.pdf", None, fixture()).expect("fixture opens");
        setup(&mut app);
        app
    });
    h.run_steps(4);
    assert!(h.state_mut().execute("edit.edit_text"));
    h.run_steps(2);
    let r = h.state().views[0].page_screen_rect(0).expect("on screen");
    let at = egui::pos2(r.left() + 40.0 / 200.0 * r.width(), r.top() + (300.0 - 158.0) / 300.0 * r.height());
    h.hover_at(at);
    h.run_steps(1);
    h.drag_at(at);
    h.run_steps(1);
    h.drop_at(at);
    h.run_steps(4);
    assert!(h.state().views[0].line_editor.is_some(), "the paragraph opens for editing");
    h
}

#[test]
fn the_toolbar_floats_over_the_paragraph_and_formats_it() {
    let mut h = editing(|_| {});
    assert!(h.state().text_toolbar, "on by default");
    let size = h.state().views[0].line_editor.as_ref().unwrap().look.size;
    h.get_by_label("A+").click();
    h.run_steps(4);
    let ed = h.state().views[0].line_editor.clone().expect("the paragraph stays open after a toolbar click");
    assert!(ed.look.size > size, "{} → {}", size, ed.look.size);
    assert!(!ed.has_changes(), "the toolbar's change was applied to the document, as the panel's are");
}

#[test]
fn the_direction_buttons_choose_right_to_left_and_back() {
    let mut h = editing(|_| {});
    // "Page 1" reads left to right until a direction is chosen.
    assert_eq!(h.state().views[0].line_editor.as_ref().unwrap().extras.direction, None);
    // The toolbar and the Format text panel each show the pair; either sets the paragraph's direction.
    assert_eq!(h.get_all_by_label("←¶").count(), 2, "in the toolbar and in the Format text panel");
    h.get_all_by_label("←¶").next().unwrap().click();
    h.run_steps(4);
    assert_eq!(h.state().views[0].line_editor.as_ref().unwrap().extras.direction, Some(true));
    h.get_all_by_label("¶→").last().unwrap().click();
    h.run_steps(4);
    assert_eq!(h.state().views[0].line_editor.as_ref().unwrap().extras.direction, Some(false));
    assert!(h.query_by_label("Text direction").is_some(), "the Format text panel names it");
}

#[test]
fn the_toolbar_can_be_turned_off_and_the_choice_is_saved() {
    let h = editing(|app| app.set_option("text_toolbar", "off").unwrap());
    assert!(h.query_by_label("A+").is_none(), "no toolbar when it is turned off");
    assert_eq!(h.get_all_by_label("←¶").count(), 1, "the Format text panel keeps its direction buttons");
    let saved = h.state().persist();
    assert!(saved.contains(r#""text_toolbar":false"#), "{saved}");
    let mut restored = PdfCraftApp::new();
    restored.restore(&saved);
    assert!(!restored.text_toolbar);
    let mut fresh = PdfCraftApp::new();
    fresh.restore("{}");
    assert!(fresh.text_toolbar, "settings without the key keep it on");
    assert!(fresh.set_option("text_toolbar", "sideways").unwrap_err().contains("on or off"));
}
