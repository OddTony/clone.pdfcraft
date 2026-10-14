//! Preferences ▸ Legacy text encoding: how Edit text reads old 8-bit Hebrew fonts. Its own test
//! binary, because the choice is process-wide (the engine reads it) and other tests decode text.

use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use pdfcraft_fonts::LegacyEncoding;
use pdfcraft_ui_egui::PdfCraftApp;

#[test]
fn preferences_choose_the_legacy_text_encoding_and_the_choice_is_saved() {
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(|_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.set_option("dialog", "preferences").unwrap();
        app
    });
    h.run_steps(4);
    assert_eq!(h.state().legacy_encoding, LegacyEncoding::Auto);
    assert_eq!(pdfcraft_fonts::legacy_encoding(), LegacyEncoding::Auto);
    // The dialog is taller than the window: bring the combo into view before clicking it.
    h.get_by(|n| n.role() == Role::ComboBox && n.value().is_some_and(|v| v == "Auto-detect")).scroll_to_me();
    h.run_steps(4);
    h.get_by(|n| n.role() == Role::ComboBox && n.value().is_some_and(|v| v == "Auto-detect")).click();
    h.run_steps(4);
    h.get_by_label("Hebrew (ISO 8859-8)").click();
    h.run_steps(2);
    assert_eq!(h.state().legacy_encoding, LegacyEncoding::Iso8859_8);
    assert_eq!(pdfcraft_fonts::legacy_encoding(), LegacyEncoding::Iso8859_8, "the engine decodes with the choice");

    let saved = h.state().persist();
    assert!(saved.contains(r#""legacy_encoding":"iso-8859-8""#), "{saved}");
    let mut restored = PdfCraftApp::new();
    restored.restore(&saved);
    assert_eq!(restored.legacy_encoding, LegacyEncoding::Iso8859_8);
    // An unknown name (a newer or damaged settings file) keeps the default.
    let mut fresh = PdfCraftApp::new();
    fresh.restore(&saved.replace(r#""legacy_encoding":"iso-8859-8""#, r#""legacy_encoding":"klingon""#));
    assert_eq!(fresh.legacy_encoding, LegacyEncoding::Auto);
    // The control channel and the command line take the same names.
    fresh.set_option("legacy_encoding", "Windows-1255").unwrap();
    assert_eq!(fresh.legacy_encoding, LegacyEncoding::Windows1255);
    assert!(fresh.set_option("legacy_encoding", "klingon").unwrap_err().contains("iso-8859-8"));
    assert_eq!(fresh.legacy_encoding, LegacyEncoding::Windows1255);
}
