//! File ▸ Format Studio: pick a reference and raw content, then generate a styled document.

mod support;

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use pdfcraft_ui_egui::{Dialog, PdfCraftApp};

fn reference_pdf() -> Vec<u8> {
    let session = pdfcraft_engine::Session::new();
    let bytes = session
        .create_from_style(
            "Reference",
            "# Annual Report\n\nFirst body paragraph with enough words to fill a line.",
            &pdfcraft_style::StyleProfile::default(),
        )
        .expect("reference");
    bytes.as_ref().clone()
}

#[test]
fn format_studio_picks_reference_and_raw_then_generates() {
    let dir = std::env::temp_dir().join(format!("pdfcraft-format-studio-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let reference = dir.join("reference.pdf");
    std::fs::write(&reference, reference_pdf()).unwrap();
    let raw = dir.join("raw.txt");
    std::fs::write(&raw, "# Quarterly summary\n\nRevenue grew across every region.").unwrap();

    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(|_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app
    });
    h.run_steps(2);
    h.state_mut().run_command("format.studio");
    h.run_steps(2);
    assert!(matches!(h.state().dialog, Some(Dialog::FormatStudio)), "the command opens the dialog");

    h.state_mut().pick_override = Some(vec![reference.to_string_lossy().into_owned()]);
    h.get_by_label_contains("Choose reference").click();
    support::wait_until(&mut h, "reference profile", |app| app.views.len() == 1 && matches!(app.dialog, Some(Dialog::FormatStudio)));
    h.run_steps(2);
    h.get_by_label_contains("body 11 pt");

    h.state_mut().pick_override = Some(vec![raw.to_string_lossy().into_owned()]);
    h.get_by_label_contains("Choose raw file").click();
    h.run_steps(3);
    h.get_by_value("# Quarterly summary\n\nRevenue grew across every region.");

    h.get_by_label("Generate styled PDF").click();
    support::wait_until(&mut h, "styled document", |app| app.views.len() == 2);
    let s = h.state();
    let doc = s.session.get(s.views[1].id).unwrap();
    assert!(doc.name.contains("Styled"), "{}", doc.name);
    let _ = std::fs::remove_dir_all(&dir);
}
