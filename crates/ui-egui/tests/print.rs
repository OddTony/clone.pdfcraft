//! The Print dialog in the real shell (egui_kittest): settings, preview sheets, Save as PDF.

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
    assert_eq!(h.state().print_draft.job("form.pdf").options, [("InputSlot".to_string(), "Tray2".to_string())]);
    h.get_by_label("Reset to printer defaults").click();
    h.run_steps(2);
    assert!(h.state().print_draft.job("form.pdf").options.is_empty(), "defaults are not sent");
    // Options read for another printer never go with this one's job.
    h.state_mut().print_draft.driver_choices.insert("InputSlot".into(), "Tray2".into());
    h.state_mut().print_draft.printer = Some("Other_Queue".into());
    assert!(h.state().print_draft.job("form.pdf").options.is_empty());
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
    // Paper by page size is on, so the dialog already opens matched at 100%.
    assert!(h.get_by_label("Match page at 100%").accesskit_node().toggled() == Some(egui::accesskit::Toggled::True));
    h.get_by_value("ARCH D (24 x 36 in)");
    h.get_by_label("PDF scale: 100.00%");
    h.get_by_label("36.00 × 24.00 in");
    h.get_by_label("Printer paper and scaling may differ. Check driver settings.");
    let settings = h.state().print_draft.settings(1, &[]).unwrap();
    assert_eq!(settings.paper, (1728.0, 2592.0));
    assert_eq!(settings.layout, pdfcraft_engine::print::Layout::Size(pdfcraft_engine::print::SizeMode::Actual));
    // Off: Fit, which shrinks by the margin; on again: 100%.
    h.get_by_label("Match page at 100%").click();
    h.run_steps(3);
    assert_eq!(h.state().print_draft.size, pdfcraft_engine::print::SizeMode::Fit);
    assert!(h.query_by_label("PDF scale: 100.00%").is_none());
    h.get_by_label("Match page at 100%").click();
    h.run_steps(3);
    h.get_by_label("PDF scale: 100.00%");
    // Without paper by page size, the box picks the matching sheet: another paper unticks it.
    h.state_mut().print_draft.paper_from_page = false;
    h.state_mut().print_draft.paper = 0;
    h.run_steps(2);
    assert!(h.get_by_label("Match page at 100%").accesskit_node().toggled() != Some(egui::accesskit::Toggled::True));
    h.get_by_label("Match page at 100%").click();
    h.run_steps(3);
    h.get_by_value("ARCH D (24 x 36 in)");
    assert!(h.get_by_label("Match page at 100%").accesskit_node().toggled() == Some(egui::accesskit::Toggled::True));
}

#[test]
fn custom_page_cannot_silently_match_a_standard_paper() {
    let mut h = harness();
    assert!(h.state_mut().execute("print.dialog"));
    h.run_steps(3);
    // The 300 x 400 pt fixture is not a standard sheet.
    assert!(pdfcraft_engine::print::matching_paper((300.0, 400.0)).is_none());
    let before = h.state().print_draft.clone();
    // With paper by page size, the box only sets the scale; the paper stays.
    assert!(h.get_by_label("Match page at 100%").accesskit_node().toggled() == Some(egui::accesskit::Toggled::True));
    h.get_by_label("Match page at 100%").click();
    h.run_steps(3);
    assert_eq!(h.state().print_draft.paper, before.paper);
    assert_eq!(h.state().print_draft.size, pdfcraft_engine::print::SizeMode::Fit);
    // Without it, a page no sheet matches can't be matched at all.
    h.state_mut().print_draft.paper_from_page = false;
    h.run_steps(2);
    assert!(h.get_by_label("Match page at 100%").accesskit_node().is_disabled());
    h.get_by_label("Match page at 100%").click();
    h.run_steps(3);
    assert_eq!(h.state().print_draft.paper, before.paper);
    assert_eq!(h.state().print_draft.size, pdfcraft_engine::print::SizeMode::Fit);
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

/// A PDF whose pages have the given MediaBox sizes (points), each painted so it renders.
fn pdf_with_pages(sizes: &[(f64, f64)]) -> Vec<u8> {
    let kids: Vec<String> = (0..sizes.len()).map(|i| format!("{} 0 R", 3 + i * 2)).collect();
    let mut objects =
        vec!["<< /Type /Catalog /Pages 2 0 R >>".to_owned(), format!("<< /Type /Pages /Kids [{}] /Count {} >>", kids.join(" "), sizes.len())];
    for (i, (w, h)) in sizes.iter().enumerate() {
        objects.push(format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {w} {h}] /Resources << >> /Contents {} 0 R >>", 4 + i * 2));
        let content = format!("0.5 g 10 10 {} {} re f", w - 20.0, h - 20.0);
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
    bytes
}

fn paper_index(name: &str) -> usize {
    pdfcraft_engine::print::PAPERS.iter().position(|(n, _)| *n == name).unwrap()
}

/// #1103: the Print dialog opens on the standard sheet the document's pages match, and with
/// "Choose paper by PDF page size" each page prints on its own sheet.
#[test]
fn the_print_dialog_chooses_paper_by_pdf_page_size() {
    const A4: (f64, f64) = (595.28, 841.89);
    const A4_LANDSCAPE: (f64, f64) = (841.89, 595.28);
    const LETTER: (f64, f64) = (612.0, 792.0);
    const ODD: (f64, f64) = (200.0, 300.0);
    let docs = [("a4.pdf", vec![A4, A4_LANDSCAPE]), ("mixed.pdf", vec![LETTER, A4, A4, ODD]), ("odd.pdf", vec![ODD])];
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(move |_| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        for (name, sizes) in &docs {
            app.open_bytes(name, None, pdf_with_pages(sizes)).unwrap();
        }
        app
    });
    h.run_steps(4);
    assert!(h.state().print_draft.paper_from_page, "on by default, as the issue asks");
    let open_print = |h: &mut Harness<'static, PdfCraftApp>, tab: usize| {
        h.state_mut().active = Some(tab);
        h.run_steps(2);
        assert!(h.state_mut().execute("print.dialog"));
        h.run_steps(3);
        assert_eq!(h.state().dialog, Some(Dialog::Print));
    };
    let close_print = |h: &mut Harness<'static, PdfCraftApp>| {
        h.get_by_label("Cancel").click();
        h.run_steps(3);
        assert_eq!(h.state().dialog, None);
    };
    // A4 pages, one of them landscape: the dialog opens on A4.
    open_print(&mut h, 0);
    assert_eq!(h.state().print_draft.paper, paper_index("A4"));
    h.get_by_label("Choose paper by PDF page size");
    close_print(&mut h);
    // Mostly A4: A4. Only the current page, which is Letter: Letter.
    open_print(&mut h, 1);
    assert_eq!(h.state().print_draft.paper, paper_index("A4"));
    h.get_by_label("Current page").click();
    h.run_steps(2);
    close_print(&mut h);
    open_print(&mut h, 1);
    assert_eq!(h.state().print_draft.current_page, 0);
    assert_eq!(h.state().print_draft.paper, paper_index("US Letter"), "the current page alone decides");
    h.get_by_label("All").click();
    h.run_steps(2);
    close_print(&mut h);
    // Back to all pages: A4 again. Saved as PDF: Letter, A4, A4 sheets, and the odd page on the
    // paper shown, A4.
    open_print(&mut h, 1);
    assert_eq!(h.state().print_draft.paper, paper_index("A4"));
    h.get_by_label("Sheet 1 of 4");
    let out = std::env::temp_dir().join(format!("pdfcraft-print-paper-{}.pdf", std::process::id()));
    h.state_mut().save_override = Some(out.to_string_lossy().into_owned());
    h.state_mut().print_draft.printer = None;
    h.run_steps(1);
    h.get_by_label("Save as PDF").click();
    h.run_steps(3);
    assert_eq!(h.state().dialog, None);
    let saved = std::fs::read(&out).unwrap();
    std::fs::remove_file(&out).unwrap();
    h.state_mut().open_bytes("printed.pdf", None, saved).unwrap();
    h.run_steps(2);
    let id = h.state().views.last().unwrap().id;
    let round = |v: f32| (f64::from(v) * 100.0).round() / 100.0;
    let sheets: Vec<(f64, f64)> = h.state().session.get(id).unwrap().info.pages.iter().map(|p| (round(p.width), round(p.height))).collect();
    assert_eq!(sheets, [LETTER, A4, A4, A4], "each page on its own sheet, the odd one on the dialog's paper");
    h.state_mut().execute("file.close");
    h.run_steps(2);
    // No standard sheet matches: the paper chosen last stays.
    h.state_mut().print_draft.paper = paper_index("A5");
    open_print(&mut h, 2);
    assert_eq!(h.state().print_draft.paper, paper_index("A5"));
    close_print(&mut h);
    // Off: the paper chosen last stays, whatever the pages measure, and every sheet is that paper.
    h.state_mut().print_draft.paper_from_page = false;
    open_print(&mut h, 1);
    assert_eq!(h.state().print_draft.paper, paper_index("A5"));
    let labels: Vec<String> = (1..=4).map(|n| n.to_string()).collect();
    let settings = h.state().print_draft.settings(4, &labels).unwrap();
    let a5 = pdfcraft_engine::print::PAPERS[paper_index("A5")].1;
    let laid = pdfcraft_engine::print::layout(&[LETTER, A4, A4, ODD], &settings).unwrap();
    assert!(laid.iter().all(|s| s.size == a5 || s.size == (a5.1, a5.0)), "{:?}", laid.iter().map(|s| s.size).collect::<Vec<_>>());
    close_print(&mut h);
    // The choice persists across restarts.
    let json = h.state().persist();
    let mut fresh = PdfCraftApp::new();
    fresh.restore(&json);
    assert!(!fresh.print_draft.paper_from_page);
}
