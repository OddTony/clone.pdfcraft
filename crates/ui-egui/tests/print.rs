//! The Print dialog in the real shell (egui_kittest): settings, preview sheets, Save as PDF.

use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use pdfcraft_ui_egui::{Dialog, PdfCraftApp};

fn harness() -> Harness<'static, PdfCraftApp> {
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(|_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.open_bytes("form.pdf", None, include_bytes!("data/form.pdf").to_vec()).unwrap();
        app
    });
    h.run_steps(4);
    h
}

#[test]
fn current_print_page_saves_the_viewed_page_when_labels_restart() {
    // Five distinct painted pages, labelled i, ii, iii, 1, 2.
    let mut objects = vec![
        "<< /Type /Catalog /Pages 2 0 R /PageLabels << /Nums [0 << /S /r >> 3 << /S /D /St 1 >>] >> >>".to_owned(),
        "<< /Type /Pages /Kids [3 0 R 5 0 R 7 0 R 9 0 R 11 0 R] /Count 5 /MediaBox [0 0 200 300] >>".to_owned(),
    ];
    for (i, color) in ["1 0 0", "0 1 0", "0 0 1", "1 1 0", "1 0 1"].iter().enumerate() {
        objects.push(format!("<< /Type /Page /Parent 2 0 R /Resources << >> /Contents {} 0 R >>", 4 + i * 2));
        let content = format!("{color} rg 0 0 200 300 re f");
        objects.push(format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()));
    }
    let mut bytes = b"%PDF-1.7\n".to_vec();
    let mut offsets = Vec::new();
    for (i, object) in objects.iter().enumerate() {
        offsets.push(bytes.len());
        bytes.extend_from_slice(format!("{} 0 obj\n{object}\nendobj\n", i + 1).as_bytes());
    }
    let xref = bytes.len();
    bytes.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes());
    for offset in offsets {
        bytes.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    bytes.extend_from_slice(format!("trailer << /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objects.len() + 1).as_bytes());
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(move |_| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.open_bytes("labelled.pdf", None, bytes).unwrap();
        app
    });
    h.run_steps(4);
    let id = h.state().views[0].id;
    let labels: Vec<_> = h.state().session.get(id).unwrap().info.pages.iter().map(|p| p.label.as_str()).collect();
    assert_eq!(labels, ["i", "ii", "iii", "1", "2"]);
    for (current, expected) in [[255, 0, 0], [0, 255, 0]].into_iter().enumerate() {
        h.state_mut().set_option("page", &(current + 1).to_string()).unwrap();
        h.run_steps(3);
        assert!(h.state_mut().execute("print.dialog"));
        h.run_steps(3);
        h.get_by_label("Current page").click();
        h.run_steps(3);
        assert_eq!(h.state().print_draft.current_page, current);
        h.get_by_label("Sheet 1 of 1");
        let out = std::env::temp_dir().join(format!("pdfcraft-print-current-{}-{current}.pdf", std::process::id()));
        h.state_mut().save_override = Some(out.to_string_lossy().into_owned());
        h.state_mut().print_draft.printer = None;
        h.run_steps(1);
        h.get_by_label("Save as PDF").click();
        h.run_steps(3);
        assert_eq!(h.state().dialog, None);
        let saved = std::sync::Arc::new(std::fs::read(&out).unwrap());
        std::fs::remove_file(out).unwrap();
        let doc = pdfcraft_cos::Document::open(saved.clone()).unwrap();
        assert_eq!(pdfcraft_model::pages(&doc).len(), 1);
        let mut renderer = pdfcraft_render::PageRenderer::new(saved, Default::default());
        let shot = renderer.render(pdfcraft_render::RenderRequest { page: 0, scale: 1.0, ..Default::default() });
        assert!(shot.error.is_none(), "{:?}", shot.error);
        let center = ((shot.height / 2 * shot.width + shot.width / 2) * 4) as usize;
        assert_eq!(&shot.rgba[center..center + 3], &expected, "saved the wrong physical page for {current}");
    }
}

#[test]
fn print_dialog_lays_out_sheets_and_saves_a_pdf() {
    let mut h = harness();
    assert!(h.state_mut().execute("print.dialog"));
    h.run_steps(3);
    assert_eq!(h.state().dialog, Some(Dialog::Print));
    h.get_by_label("Pages to Print".to_uppercase().as_str());
    h.get_by_label("Sheet 1 of 1");
    // Two copies of the one page, as a 2-up poster-free layout: Multiple.
    h.get_by_label("Multiple").click();
    h.run_steps(2);
    h.get_by_label("Sheet 1 of 1");
    // No printer on a test machine: Save as PDF.
    let out = std::env::temp_dir().join(format!("pdfcraft-print-test-{}.pdf", std::process::id()));
    h.state_mut().save_override = Some(out.to_string_lossy().into_owned());
    h.state_mut().print_draft.printer = None;
    h.run_steps(1);
    h.get_by_label("Save as PDF").click();
    h.run_steps(3);
    assert_eq!(h.state().dialog, None);
    let bytes = std::fs::read(&out).expect("saved");
    let doc = pdfcraft_cos::Document::open(std::sync::Arc::new(bytes)).unwrap();
    assert_eq!(pdfcraft_model::pages(&doc).len(), 1);
    let _ = std::fs::remove_file(out);
}

/// Pages picked beforehand are what the dialog offers to print (Acrobat's "Selected pages").
#[test]
fn print_dialog_prints_the_selected_pages() {
    use pdfcraft_ui_egui::PrintWhich;
    let mut h = harness();
    let bytes = h.state().session.create_blank(200.0, 300.0, 5).unwrap();
    h.state_mut().open_bytes("five.pdf", None, bytes.as_ref().clone()).unwrap();
    h.run_steps(3);
    let tab = h.state().views.len() - 1;
    // Nothing picked: the whole document, and no "Selected pages" choice.
    assert!(h.state_mut().execute("print.dialog"));
    h.run_steps(3);
    assert_eq!(h.state().print_draft.which, PrintWhich::All);
    assert!(h.query_by_label_contains("Selected pages").is_none());
    h.get_by_label("Sheet 1 of 5");
    h.state_mut().dialog = None;
    h.run_steps(2);
    // Pages 2 and 4 picked.
    h.state_mut().views[tab].select_pages(&[1, 3]);
    assert!(h.state_mut().execute("print.dialog"));
    h.run_steps(3);
    assert_eq!(h.state().print_draft.which, PrintWhich::Selected);
    h.get_by_label("Selected pages (2)");
    h.get_by_label("Sheet 1 of 2");
    let out = std::env::temp_dir().join(format!("pdfcraft-print-selected-{}.pdf", std::process::id()));
    h.state_mut().save_override = Some(out.to_string_lossy().into_owned());
    h.state_mut().print_draft.printer = None;
    h.run_steps(1);
    h.get_by_label("Save as PDF").click();
    h.run_steps(3);
    assert_eq!(h.state().dialog, None);
    let saved = std::fs::read(&out).expect("saved");
    let doc = pdfcraft_cos::Document::open(std::sync::Arc::new(saved)).unwrap();
    assert_eq!(pdfcraft_model::pages(&doc).len(), 2);
    let _ = std::fs::remove_file(out);
    // The selection gone, the remembered choice falls back to the whole document.
    h.state_mut().views[tab].select_pages(&[]);
    assert!(h.state_mut().execute("print.dialog"));
    h.run_steps(3);
    assert_eq!(h.state().print_draft.which, PrintWhich::All);
    assert!(h.state().print_draft.selected.is_empty());
}

#[test]
fn invalid_ranges_are_explained_in_the_preview() {
    let mut h = harness();
    h.state_mut().execute("print.dialog");
    h.run_steps(2);
    h.state_mut().print_draft.which = pdfcraft_ui_egui::PrintWhich::Range;
    h.state_mut().print_draft.range = "7".into();
    h.run_steps(2);
    h.get_by_label_contains("out of range");
}

#[test]
fn cut_stack_dialog_previews_saves_and_refuses_duplex() {
    use pdfcraft_engine::print::{PageOrder, spool::Duplex};
    let mut h = harness();
    // Ten source pages, generated using the headless engine.
    let bytes = h.state().session.create_blank(200.0, 300.0, 10).unwrap();
    h.state_mut().open_bytes("numbered.pdf", None, bytes.as_ref().clone()).unwrap();
    h.state_mut().execute("print.dialog");
    h.run_steps(3);
    h.get_by_label("Multiple").click();
    h.run_steps(2);
    // Pick the new order through the actual widgets.
    h.get_by_value("Horizontal").click();
    h.run_steps(2);
    h.get_by_label("Cut and stack").click();
    h.run_steps(2);
    assert_eq!(h.state().print_draft.order, PageOrder::CutStack);
    h.state_mut().print_draft.per_sheet = 4;
    h.run_steps(2);
    h.get_by_label("Sheet 1 of 3");
    h.get_by_label_contains("Keep the sheets in order");
    h.get_by_label("›").click();
    h.run_steps(2);
    h.get_by_label("Sheet 2 of 3");
    h.state_mut().print_draft.duplex = Duplex::LongEdge;
    h.run_steps(2);
    h.get_by_label_contains("Cut and stack needs Two-sided: Off");
    assert!(!h.state_mut().print_now());
    h.state_mut().print_draft.duplex = Duplex::Off;
    h.state_mut().print_draft.printer = None;
    let out = std::env::temp_dir().join(format!("pdfcraft-cut-stack-ui-{}.pdf", std::process::id()));
    h.state_mut().save_override = Some(out.to_string_lossy().into_owned());
    h.run_steps(2);
    h.get_by_label("Save as PDF").click();
    h.run_steps(3);
    assert_eq!(h.state().dialog, None);
    let printed = pdfcraft_cos::Document::open(std::sync::Arc::new(std::fs::read(&out).unwrap())).unwrap();
    assert_eq!(pdfcraft_model::pages(&printed).len(), 3);
    let _ = std::fs::remove_file(out);
}

#[test]
fn a_failed_save_as_pdf_keeps_the_dialog_open() {
    let mut h = harness();
    assert!(h.state_mut().execute("print.dialog"));
    h.run_steps(3);
    h.state_mut().print_draft.printer = None;
    // A folder that doesn't exist: the write fails.
    let out = std::env::temp_dir().join(format!("pdfcraft-missing-{}", std::process::id())).join("x.pdf");
    h.state_mut().save_override = Some(out.to_string_lossy().into_owned());
    h.run_steps(2);
    h.get_by_label("Save as PDF").click();
    h.run_steps(3);
    assert_eq!(h.state().dialog, Some(Dialog::Print), "the dialog stays open, with its settings");
    let toast = h.state().toast.as_ref().map(|(m, _)| m.clone()).unwrap_or_default();
    assert!(toast.contains("Could not save"), "the user is told why: {toast:?}");
}

/// Properties… shows the printer driver's own options (CUPS) and sends only those changed.
#[test]
fn print_dialog_shows_the_printer_driver_options() {
    use pdfcraft_engine::print::spool;
    let mut h = harness();
    assert!(h.state_mut().execute("print.dialog"));
    h.run_steps(3);
    h.state_mut().print_draft.printer = None;
    h.run_steps(1);
    assert!(h.query_by(|n| n.label().as_deref() == Some("Properties…") && n.is_disabled()).is_some(), "Save as PDF has no driver");
    let tray = spool::PrinterOption {
        key: "InputSlot".into(),
        label: "Paper tray".into(),
        group: "Media".into(),
        choices: vec![("AutoSelect".into(), "Auto tray select".into()), ("Tray2".into(), "Tray 2".into())],
        default: "AutoSelect".into(),
    };
    {
        let d = &mut h.state_mut().print_draft;
        d.printer = Some("Test_Queue".into());
        d.driver_options = Some(("Test_Queue".into(), vec![tray]));
        d.driver_choices.insert("InputSlot".into(), "Tray2".into());
        d.show_driver_options = true;
    }
    h.run_steps(2);
    h.get_by_label("Printer properties");
    h.get_by_label("Media");
    h.get_by_label("Paper tray");
    // The selected sheet's media is always sent, independently of changed driver properties.
    let media = vec![("media".to_string(), "Letter".to_string())];
    assert_eq!(h.state().print_draft.job("form.pdf").options, [("InputSlot".to_string(), "Tray2".to_string()), media[0].clone()]);
    h.get_by_label("Reset to printer defaults").click();
    h.run_steps(2);
    assert_eq!(h.state().print_draft.job("form.pdf").options, media, "driver defaults are not sent; selected paper is retained");
    // Options read for another printer never go with this one's job.
    h.state_mut().print_draft.driver_choices.insert("InputSlot".into(), "Tray2".into());
    h.state_mut().print_draft.printer = Some("Other_Queue".into());
    assert_eq!(h.state().print_draft.job("form.pdf").options, media, "another queue must not inherit driver options");
}

/// The desktop app lists printers in the background (PowerShell on Windows takes a second or
/// more): the dialog opens at once, then preselects the default when the list arrives — unless
/// the user has already picked a printer.
#[test]
fn printers_listed_in_the_background_preselect_the_default_unless_one_was_picked() {
    use pdfcraft_engine::print::spool::Printer;
    use std::sync::{Arc, Mutex};
    let mut h = harness();
    // The source waits until the test lets it answer, so the dialog is seen while it looks.
    let gate = Arc::new(Mutex::new(()));
    let hold = gate.lock().unwrap();
    let wait = gate.clone();
    h.state_mut().printer_source = Some(Arc::new(move || {
        let _open = wait.lock();
        vec![
            Printer { name: "Office".into(), default: false, two_sided: None },
            Printer { name: "Front Desk".into(), default: true, two_sided: None },
        ]
    }));
    assert!(h.state_mut().execute("print.dialog"));
    h.run_steps(2);
    assert_eq!(h.state().dialog, Some(Dialog::Print), "the dialog doesn't wait for the printers");
    assert!(h.state().print_draft.printers_loading);
    assert_eq!(h.state().print_draft.printer, None);
    // Nothing is known yet (#160): the dialog says it is looking, and Print waits instead of
    // saving a PDF.
    h.get_by_value("Looking for printers…");
    assert!(h.get_by_role_and_label(Role::Button, "Print").accesskit_node().is_disabled(), "Print waits for the list");
    assert!(h.query_by_label("Save as PDF").is_none(), "not offered as the choice while looking");
    drop(hold);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while h.state().print_draft.printers_loading && std::time::Instant::now() < deadline {
        h.run_steps(1);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(!h.state().print_draft.printers_loading);
    assert_eq!(h.state().print_draft.printers.len(), 2);
    assert_eq!(h.state().print_draft.printer.as_deref(), Some("Front Desk"));

    // Opened again: last time's list shows at once; a choice made before the fresh list
    // arrives is kept.
    h.state_mut().dialog = None;
    let hold = gate.lock().unwrap();
    assert!(h.state_mut().execute("print.dialog"));
    h.run_steps(2);
    assert_eq!(h.state().print_draft.printers.len(), 2, "the last list shows while looking");
    {
        let d = &mut h.state_mut().print_draft;
        d.printer = None;
        d.printer_picked = true;
    }
    drop(hold);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while h.state().print_draft.printers_loading && std::time::Instant::now() < deadline {
        h.run_steps(1);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert!(!h.state().print_draft.printers_loading);
    assert_eq!(h.state().print_draft.printer, None, "Save as PDF, as picked");
}

#[test]
fn architectural_page_can_be_matched_at_100_percent() {
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 1000.0)).build_eframe(|_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        let bytes = app.session.create_blank(2592.0, 1728.0, 1).unwrap();
        app.open_bytes("architectural.pdf", None, bytes.as_ref().clone()).unwrap();
        app
    });
    h.run_steps(4);
    assert!(h.state_mut().execute("print.dialog"));
    h.run_steps(3);
    h.get_by_label("PDF page: 36.00 × 24.00 in");
    h.get_by_label("Match page at 100%").click();
    h.run_steps(3);
    h.get_by_value("ARCH D (24 x 36 in)");
    h.get_by_label("PDF scale: 100.00%");
    h.get_by_label("36.00 × 24.00 in");
    h.get_by_label("Printer paper and scaling may differ. Check driver settings.");
    let settings = h.state().print_draft.settings(1, &[]).unwrap();
    assert_eq!(settings.paper, (1728.0, 2592.0));
    assert_eq!(settings.layout, pdfcraft_engine::print::Layout::Size(pdfcraft_engine::print::SizeMode::Actual));
}

#[test]
fn custom_page_cannot_silently_match_a_standard_paper() {
    let mut h = harness();
    assert!(h.state_mut().execute("print.dialog"));
    h.run_steps(3);
    // The 300 x 400 pt fixture is not a standard sheet.
    assert!(pdfcraft_engine::print::matching_paper((300.0, 400.0)).is_none());
    let before = h.state().print_draft.clone();
    assert!(h.get_by_label("Match page at 100%").accesskit_node().is_disabled());
    h.get_by_label("Match page at 100%").click();
    h.run_steps(3);
    assert_eq!(h.state().print_draft.paper, before.paper);
    assert_eq!(h.state().print_draft.size, before.size);
}

/// A driver Windows reports without two-sided printing keeps its own setting whatever Two-sided
/// says (some print both sides with it Off), so the dialog says where to change it.
#[test]
fn a_printer_without_two_sided_printing_points_to_its_properties() {
    use pdfcraft_engine::print::spool::Printer;
    let mut h = harness();
    h.state_mut().printer_source = Some(std::sync::Arc::new(|| {
        vec![
            Printer { name: "Office PCL".into(), default: true, two_sided: Some(true) },
            Printer { name: "Office PS".into(), default: false, two_sided: Some(false) },
        ]
    }));
    assert!(h.state_mut().execute("print.dialog"));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while h.state().print_draft.printers.is_empty() && std::time::Instant::now() < deadline {
        h.run_steps(1);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    h.run_steps(2);
    let note = "doesn't take two-sided settings";
    assert!(h.query_by_label_contains(note).is_none(), "a driver that prints two-sided as asked");
    h.state_mut().print_draft.printer = Some("Office PS".into());
    h.run_steps(2);
    h.get_by_label_contains(note);
}

/// Print as image (Acrobat's Advanced Print Setup) is a choice in the Print dialog's Advanced
/// section: the saved print-ready PDF carries the page as a picture, no text.
#[test]
fn print_as_image_is_an_advanced_choice_that_saves_pictures() {
    let mut h = harness();
    assert!(h.state_mut().execute("print.dialog"));
    h.run_steps(3);
    h.get_by_label("Advanced".to_uppercase().as_str()).click();
    h.run_steps(2);
    h.get_by_label("Print as image").click();
    h.run_steps(2);
    assert!(h.state().print_draft.as_image);
    let out = std::env::temp_dir().join(format!("pdfcraft-print-image-test-{}.pdf", std::process::id()));
    h.state_mut().save_override = Some(out.to_string_lossy().into_owned());
    h.state_mut().print_draft.printer = None;
    h.run_steps(1);
    h.get_by_label("Save as PDF").click();
    h.run_steps(3);
    assert_eq!(h.state().dialog, None);
    let bytes = std::fs::read(&out).expect("saved");
    let doc = pdfcraft_cos::Document::open(std::sync::Arc::new(bytes)).unwrap();
    assert_eq!(pdfcraft_model::pages(&doc).len(), 1);
    let pictures =
        doc.scan_objects().filter(|(_, o)| matches!(&**o, pdfcraft_cos::Object::Stream(st) if st.dict.name(b"Subtype") == Some(b"Image"))).count();
    assert_eq!(pictures, 1, "the page is one picture");
    let _ = std::fs::remove_file(out);
}

/// The spooler answering with no printers at all says so (#160), and Save as PDF still works.
#[test]
fn an_empty_printer_list_says_none_were_found() {
    use std::sync::Arc;
    let mut h = harness();
    h.state_mut().printer_source = Some(Arc::new(Vec::new));
    assert!(h.state_mut().execute("print.dialog"));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while h.state().print_draft.printers_loading && std::time::Instant::now() < deadline {
        h.run_steps(1);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    h.run_steps(2);
    h.get_by_label_contains("No printers were found");
    assert!(!h.get_by_role_and_label(Role::Button, "Save as PDF").accesskit_node().is_disabled());
}

/// Two pages: the first draws a glyph, a space and a glyph with a Chrome-style Type 3 font
/// (#2684), the second Helvetica text.
fn type3_and_helvetica_pdf() -> Vec<u8> {
    pdfcraft_testkit::Pdf::new()
        .obj("<< /Type /Catalog /Pages 2 0 R >>")
        .obj("<< /Type /Pages /Kids [3 0 R 4 0 R] /Count 2 /MediaBox [0 0 200 300] >>")
        .obj("<< /Type /Page /Parent 2 0 R /Contents 5 0 R /Resources << /Font << /F9 7 0 R >> >> >>")
        .obj("<< /Type /Page /Parent 2 0 R /Contents 6 0 R /Resources << /Font << /F1 10 0 R >> >> >>")
        .stream("", "1 0 0 -1 0 300 cm BT /F9 40 Tf 1 0 0 -1 30 80 Tm <010F01> Tj ET")
        .stream("", "BT /F1 24 Tf 20 150 Td (Page 2) Tj ET")
        .obj(
            "<< /Type /Font /Subtype /Type3 /FontMatrix [0.001 0 0 -0.001 0 0] /FontBBox [0 -700 600 0] /FirstChar 0 /LastChar 15 \
             /Widths [0 600 0 0 0 0 0 0 0 0 0 0 0 0 0 250] /Encoding << /Differences [1 /g3B 15 /g3] >> /CharProcs << /g3B 8 0 R /g3 9 0 R >> >>",
        )
        .stream("", "600 0 0 -700 600 0 d1\n0 -700 600 700 re f")
        .stream("", "250 0 0 0 0 0 d1")
        .obj("<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>")
        .build()
}

/// On Windows, the pages that draw with Type 3 fonts go to a printer as pictures, at the Print as
/// image resolution, because the Windows print component can close the app on them (#2684), and
/// the dialog says so while such a page is being printed. Save as PDF, Print as image (every page
/// a picture already) and other systems leave the pages as they are, and say nothing.
#[test]
fn type3_pages_go_to_windows_printers_as_images_and_the_dialog_says_so() {
    use pdfcraft_engine::print::{Raster, spool::Printer};
    use pdfcraft_ui_egui::PrintWhich;
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(|_| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.open_bytes("chrome.pdf", None, type3_and_helvetica_pdf()).unwrap();
        app
    });
    h.run_steps(4);
    h.state_mut().printer_source = Some(std::sync::Arc::new(|| vec![Printer { name: "Office".into(), default: true, two_sided: Some(true) }]));
    assert!(h.state_mut().execute("print.dialog"));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while h.state().print_draft.printers_loading && std::time::Instant::now() < deadline {
        h.run_steps(1);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    h.run_steps(2);
    assert_eq!(h.state().print_draft.printer.as_deref(), Some("Office"));
    let windows = cfg!(windows);
    let note = "Pages that use Type 3 fonts print as images";
    let type3 = |h: &Harness<'_, PdfCraftApp>| h.state().print_draft.settings(2, &[]).unwrap().type3_raster;
    assert_eq!(h.state().print_draft.type3_pages, if windows { vec![0] } else { Vec::new() });
    assert_eq!(type3(&h), windows.then_some(Raster { dpi: 300.0, grayscale: false }));
    assert_eq!(h.query_by_label_contains(note).is_some(), windows);
    // The second page alone has no Type 3 font: nothing to say.
    h.state_mut().print_draft.which = PrintWhich::Range;
    h.state_mut().print_draft.range = "2".into();
    h.run_steps(2);
    assert!(h.query_by_label_contains(note).is_none());
    h.state_mut().print_draft.which = PrintWhich::All;
    h.run_steps(2);
    assert_eq!(h.query_by_label_contains(note).is_some(), windows);
    // Print as image already prints every page as a picture.
    h.state_mut().print_draft.as_image = true;
    h.run_steps(2);
    assert_eq!(type3(&h), None);
    assert!(h.query_by_label_contains(note).is_none());
    h.state_mut().print_draft.as_image = false;
    // Save as PDF keeps the pages as they are.
    h.state_mut().print_draft.printer = None;
    h.run_steps(2);
    assert_eq!(type3(&h), None);
    assert!(h.query_by_label_contains(note).is_none());
}
