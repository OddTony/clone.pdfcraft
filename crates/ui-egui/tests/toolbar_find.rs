//! The find field docked in the toolbar (Preferences ▸ Find text in the toolbar, on by default)
//! and the Save as button beside Save.

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use pdfcraft_ui_egui::PdfCraftApp;

/// `n` pages of 200×300; page i says "Page i+1" plus "page" and "Pages" for find tests.
fn fixture(n: usize) -> Vec<u8> {
    let mut objs: Vec<String> = vec!["<< /Type /Catalog /Pages 2 0 R >>".into()];
    let kids: Vec<String> = (0..n).map(|i| format!("{} 0 R", 4 + 2 * i)).collect();
    objs.push(format!("<< /Type /Pages /Kids [{}] /Count {n} /MediaBox [0 0 200 300] >>", kids.join(" ")));
    objs.push("<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".into());
    for i in 0..n {
        objs.push(format!("<< /Type /Page /Parent 2 0 R /Contents {} 0 R /Resources << /Font << /F1 3 0 R >> >> >>", 5 + 2 * i));
        let body = format!("BT /F1 18 Tf 20 150 Td (Page {} pages PAGE) Tj ET", i + 1);
        objs.push(format!("<< /Length {} >>\nstream\n{body}\nendstream", body.len()));
    }
    let mut out = b"%PDF-1.7\n".to_vec();
    let mut offs = Vec::new();
    for (i, o) in objs.iter().enumerate() {
        offs.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n{o}\nendobj\n", i + 1).as_bytes());
    }
    let x = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).as_bytes());
    for o in offs {
        out.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{x}\n%%EOF\n", objs.len() + 1).as_bytes());
    out
}

fn harness(find_in_toolbar: bool) -> Harness<'static, PdfCraftApp> {
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(move |_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.find_in_toolbar = find_in_toolbar;
        app.open_bytes("a.pdf", None, fixture(3)).unwrap();
        app
    });
    for _ in 0..3000 {
        h.run_steps(2);
        if !h.state().render_pending() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    h
}

fn floating_bar_shown(h: &Harness<'static, PdfCraftApp>) -> bool {
    h.ctx.memory(|m| m.areas().visible_last_frame(&egui::LayerId::new(egui::Order::Middle, egui::Id::new("find-bar"))))
}

fn wait_for_matches(h: &mut Harness<'static, PdfCraftApp>, at_least: usize) -> usize {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while std::time::Instant::now() < deadline {
        h.run_steps(2);
        std::thread::sleep(std::time::Duration::from_millis(5));
        if h.state().views[0].find.as_ref().is_some_and(|f| f.matches.len() >= at_least) {
            break;
        }
    }
    h.state().views[0].find.as_ref().map_or(0, |f| f.matches.len())
}

#[test]
fn the_toolbar_has_save_as_beside_save() {
    let h = harness(true);
    h.get_by_label_contains("Save as (");
    h.get_by_label_contains("Save (");
}

#[test]
fn typing_into_the_toolbar_field_starts_a_search_and_nothing_floats() {
    let mut h = harness(true);
    assert!(h.state().views[0].find.is_none());
    // The empty field is there before any search: focus it and type.
    h.ctx.memory_mut(|m| m.request_focus(egui::Id::new("find-input")));
    h.run_steps(1);
    assert!(h.ctx.memory(|m| m.has_focus(egui::Id::new("find-input"))), "the draft field is in the toolbar");
    h.input_mut().events.push(egui::Event::Text("page".into()));
    h.run_steps(2);
    assert_eq!(h.state().views[0].find.as_ref().map(|f| f.query.as_str()), Some("page"));
    assert_eq!(wait_for_matches(&mut h, 9), 9, "three matches a page, three pages");
    assert!(!floating_bar_shown(&h), "the docked field replaces the floating bar");
    // ⌘F focuses the same field rather than opening a floating bar.
    h.state_mut().views[0].open_find();
    h.run_steps(2);
    assert!(!floating_bar_shown(&h));
    assert!(h.ctx.memory(|m| m.has_focus(egui::Id::new("find-input"))));
    // Esc ends the search; the field stays, empty.
    h.key_press(egui::Key::Escape);
    h.run_steps(2);
    assert!(h.state().views[0].find.is_none());
    h.ctx.memory_mut(|m| m.request_focus(egui::Id::new("find-input")));
    h.run_steps(1);
    assert!(h.ctx.memory(|m| m.has_focus(egui::Id::new("find-input"))), "the empty field is still there");
}

#[test]
fn with_the_preference_off_the_bar_floats_as_before() {
    let mut h = harness(false);
    h.state_mut().views[0].open_find();
    h.state_mut().views[0].find.as_mut().unwrap().query = "page".into();
    h.state_mut().views[0].rerun_find();
    assert_eq!(wait_for_matches(&mut h, 9), 9);
    assert!(floating_bar_shown(&h));
}

#[test]
fn the_preference_is_kept_across_launches() {
    let mut app = PdfCraftApp::new();
    assert!(app.find_in_toolbar, "on by default");
    app.find_in_toolbar = false;
    let json = app.persist();
    let mut again = PdfCraftApp::new();
    again.restore(&json);
    assert!(!again.find_in_toolbar);
}
