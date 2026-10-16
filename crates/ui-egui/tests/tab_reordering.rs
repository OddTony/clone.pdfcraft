//! Regressions for the tab-order commands and cancellation on upstream's drag UI.
use egui::{Event, Key, Modifiers, PointerButton, Pos2, Rect};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use pdfcraft_ui_egui::PdfCraftApp;

fn fixture() -> Vec<u8> {
    fixture_pages(1)
}

fn fixture_pages(count: usize) -> Vec<u8> {
    let kids = (0..count).map(|i| format!("{} 0 R", i + 3)).collect::<Vec<_>>().join(" ");
    let mut objects = vec!["<< /Type /Catalog /Pages 2 0 R >>".to_owned(), format!("<< /Type /Pages /Kids [{kids}] /Count {count} >>")];
    objects.extend((0..count).map(|_| "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 100] >>".to_owned()));
    let mut out = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    for (i, object) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n{object}\nendobj\n", i + 1).as_bytes());
    }
    let xref = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes());
    for offset in offsets {
        out.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objects.len() + 1).as_bytes());
    out
}

fn harness(count: usize) -> Harness<'static, PdfCraftApp> {
    let mut h = Harness::builder().with_step_dt(1.0 / 60.0).with_size(egui::vec2(1400.0, 900.0)).build_eframe(move |_| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        for i in 0..count {
            app.open_bytes(&format!("document-{i}.pdf"), None, fixture()).unwrap();
        }
        app
    });
    h.run_steps(5);
    h
}

fn tab(h: &Harness<'_, PdfCraftApp>, name: &str) -> Rect {
    h.get_by_label(name).rect()
}

fn press(h: &mut Harness<'_, PdfCraftApp>, at: Pos2) {
    h.hover_at(at);
    h.step();
    h.drag_at(at);
    h.step();
}

fn release(h: &mut Harness<'_, PdfCraftApp>, at: Pos2) {
    h.input_mut().events.push(Event::PointerButton { pos: at, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
    h.step();
}

fn order(h: &Harness<'_, PdfCraftApp>) -> Vec<pdfcraft_engine::DocId> {
    h.state().views.iter().map(|view| view.id).collect()
}

#[test]
fn directional_commands_move_the_active_tab_and_respect_boundaries_and_special_tabs() {
    let mut h = harness(3);
    let initial = order(&h);
    let active = h.state().active_ids().unwrap().1;

    assert!(h.state_mut().execute("tab.move_left"));
    assert_eq!(order(&h), [initial[0], initial[2], initial[1]]);
    assert_eq!(h.state().active_ids().unwrap().1, active);
    assert!(h.state_mut().execute("tab.move_right"));
    assert_eq!(order(&h), initial);
    h.state_mut().active = Some(0);
    assert!(!h.state_mut().execute("tab.move_left"));
    h.state_mut().active = Some(2);
    assert!(!h.state_mut().execute("tab.move_right"));
    assert_eq!(order(&h), initial);

    h.state_mut().active = None; // Home selection.
    assert!(!h.state_mut().execute("tab.move_right"));
    assert_eq!(order(&h), initial);
    h.state_mut().open_combine_tab();
    assert!(!h.state_mut().execute("tab.move_left"));
    assert_eq!(order(&h), initial);
}

#[test]
fn command_palette_moves_active_tab_using_keyboard() {
    let mut h = harness(3);
    let initial = order(&h);
    let active = h.state().active_ids().unwrap().1;
    h.key_press_modifiers(Modifiers::COMMAND | Modifiers::CTRL, Key::K);
    h.run_steps(2);
    assert!(h.state().palette_open);
    h.state_mut().palette_query = "Move tab left".into();
    h.run_steps(3);
    if let Ok(dir) = std::env::var("PDFCRAFT_TAB_REVIEW_SHOTS") {
        h.render().unwrap().save(format!("{dir}/command-palette.png")).unwrap();
    }
    h.key_press(Key::Enter);
    h.step();
    assert_eq!(order(&h), [initial[0], initial[2], initial[1]]);
    assert_eq!(h.state().active_ids().unwrap().1, active);
}

#[test]
fn context_menu_moves_the_clicked_inactive_tab_and_disables_edges() {
    let mut h = harness(3);
    let initial = order(&h);
    let active = h.state().active_ids().unwrap().1;
    let at = tab(&h, "document-0.pdf").center();
    for pressed in [true, false] {
        h.input_mut().events.push(Event::PointerButton { pos: at, button: PointerButton::Secondary, pressed, modifiers: Modifiers::NONE });
        h.step();
    }
    assert!(h.get_by_label("Move Tab Left").accesskit_node().is_disabled());
    if let Ok(dir) = std::env::var("PDFCRAFT_TAB_REVIEW_SHOTS") {
        h.run_steps(5);
        h.render().unwrap().save(format!("{dir}/context-menu.png")).unwrap();
    }
    h.get_by_label("Move Tab Right").click();
    h.step();
    assert_eq!(order(&h), [initial[1], initial[0], initial[2]]);
    assert_eq!(h.state().active_ids().unwrap().1, active);
}

#[test]
fn canceled_drag_recovers_for_the_next_gesture() {
    let mut h = harness(3);
    let initial = order(&h);
    let first = tab(&h, "document-0.pdf").center();
    let last = tab(&h, "document-2.pdf").right_center() - egui::vec2(1.0, 0.0);

    press(&mut h, first);
    h.hover_at(last);
    h.step();
    h.key_press(Key::Escape);
    h.step();
    release(&mut h, last);
    assert_eq!(order(&h), initial, "Escape must cancel without changing tab order");

    let first = tab(&h, "document-0.pdf").center();
    let last = tab(&h, "document-2.pdf").right_center() - egui::vec2(1.0, 0.0);
    press(&mut h, first);
    h.hover_at(last);
    h.step();
    release(&mut h, last);
    assert_eq!(order(&h), [initial[1], initial[2], initial[0]], "a canceled drag must not poison the next gesture");
}

#[test]
fn release_outside_tab_strip_cancels_instead_of_dropping_on_nearest_tab() {
    let mut h = harness(3);
    let initial = order(&h);
    let first = tab(&h, "document-0.pdf").center();
    let last = tab(&h, "document-2.pdf").center();
    press(&mut h, first);
    h.hover_at(last);
    h.step();
    let outside = last + egui::vec2(0.0, 100.0);
    h.hover_at(outside);
    h.step();
    release(&mut h, outside);
    assert_eq!(order(&h), initial);
    assert_eq!(h.state().session.docs().len(), 3);
}

fn interrupted_drag(modal: bool) {
    {
        let mut h = harness(3);
        let initial = order(&h);
        let first = tab(&h, "document-0.pdf").center();
        let last = tab(&h, "document-2.pdf").center();
        press(&mut h, first);
        h.hover_at(last);
        h.step();
        if modal {
            h.state_mut().dialog = Some(pdfcraft_ui_egui::Dialog::About);
        } else {
            h.input_mut().focused = false;
        }
        h.step();
        if modal {
            h.state_mut().dialog = None;
        } else {
            h.input_mut().focused = true;
        }
        h.step();
        release(&mut h, last);
        assert_eq!(order(&h), initial, "modal={modal}");
        // Focus/modal cancellation must not poison the next real pointer gesture.
        let first = tab(&h, "document-0.pdf").center();
        let last = tab(&h, "document-2.pdf").right_center() - egui::vec2(1.0, 0.0);
        press(&mut h, first);
        h.hover_at(last);
        h.step();
        release(&mut h, last);
        assert_eq!(order(&h), [initial[1], initial[2], initial[0]], "next gesture after modal={modal}");
    }
}

#[test]
fn focus_loss_cancels_an_in_progress_drag() {
    interrupted_drag(false);
}

#[test]
fn modal_open_cancels_an_in_progress_drag() {
    interrupted_drag(true);
}

#[test]
fn closing_the_drag_source_invalidates_its_pending_drop() {
    let mut h = harness(3);
    let initial = order(&h);
    let first = tab(&h, "document-0.pdf").center();
    let last = tab(&h, "document-2.pdf").right_center() - egui::vec2(1.0, 0.0);
    press(&mut h, first);
    h.hover_at(last);
    h.step();
    h.state_mut().close_tab(0);
    h.step();
    release(&mut h, last);
    assert_eq!(order(&h), [initial[1], initial[2]]);
    assert_eq!(h.state().session.docs().len(), 2);
}

#[test]
fn opt_in_session_persistence_uses_reordered_tabs_and_active_document() {
    let mut app = PdfCraftApp::new();
    let paths = ["first.pdf", "second.pdf", "third.pdf"].map(|name| std::env::temp_dir().join(name));
    for path in &paths {
        app.open_bytes(path.file_name().unwrap().to_string_lossy().as_ref(), Some(path.to_string_lossy().into_owned()), fixture_pages(3)).unwrap();
    }
    app.views[0].current = 1;
    app.views[1].current = 2;
    app.views[2].current = 0;
    let disabled = serde_json::from_str::<serde_json::Value>(&app.persist()).unwrap();
    assert!(disabled["last_session"].is_null());
    app.active = Some(0);
    app.move_tab(0, 2);
    app.reopen_last_session = true;
    let persisted = serde_json::from_str::<serde_json::Value>(&app.persist()).unwrap();
    let files = persisted["last_session"]["files"].as_array().unwrap();
    let actual_paths = files.iter().map(|entry| entry["path"].as_str().unwrap()).collect::<Vec<_>>();
    let expected_paths = paths[1..].iter().chain(&paths[..1]).map(|path| path.to_string_lossy().into_owned()).collect::<Vec<_>>();
    assert_eq!(actual_paths, expected_paths.iter().map(String::as_str).collect::<Vec<_>>());
    assert_eq!(files.iter().map(|entry| entry["page"].as_u64().unwrap()).collect::<Vec<_>>(), [2, 0, 1]);
    assert_eq!(persisted["last_session"]["active"], paths[0].to_string_lossy().as_ref());
}

#[test]
fn moving_by_command_preserves_unsaved_bytes_view_state_and_undo() {
    let mut app = PdfCraftApp::new();
    for name in ["first.pdf", "second.pdf", "third.pdf"] {
        app.open_bytes(name, None, fixture()).unwrap();
    }
    let id = app.active_ids().unwrap().1;
    app.session.apply(id, pdfcraft_engine::Edit::InsertBlankPage { at: 0, width: 100.0, height: 100.0 }).unwrap();
    let bytes = std::sync::Arc::clone(&app.session.get(id).unwrap().bytes);
    app.views[2].current = 1;
    app.views[2].zoom = 1.75;
    app.views[2].find.get_or_insert_with(Default::default).query = "retained query".into();
    assert!(app.execute("tab.move_left"));
    assert_eq!(app.active_ids(), Some((1, id)));
    assert_eq!(app.views[1].current, 1);
    assert_eq!(app.views[1].zoom, 1.75);
    assert_eq!(app.views[1].find.as_ref().unwrap().query, "retained query");
    let doc = app.session.get(id).unwrap();
    assert!(doc.dirty && doc.can_undo().is_some());
    assert!(std::sync::Arc::ptr_eq(&bytes, &doc.bytes));
    app.session.undo(id).unwrap();
    assert_eq!(app.session.get(id).unwrap().info.pages.len(), 1);
    app.session.redo(id).unwrap();
    assert_eq!(app.session.get(id).unwrap().info.pages.len(), 2);
}

#[test]
fn upstream_overflow_scrolling_still_exposes_hidden_drop_slots() {
    let mut h = harness(20);
    h.state_mut().active = Some(0);
    h.run_steps(4);
    let initial = order(&h);
    let first = tab(&h, "document-0.pdf");
    let arrow = h.get_by_label("Scroll tabs right").rect();
    let edge = egui::pos2(arrow.left() - 6.0, first.center().y);
    press(&mut h, first.center());
    h.hover_at(edge);
    for _ in 0..80 {
        h.step();
    }
    assert_eq!(order(&h), initial, "edge scrolling previews without moving a document");
    release(&mut h, edge);
    assert!(order(&h).iter().position(|id| *id == initial[0]).unwrap() > 2, "upstream auto-scroll exposes slots beyond the initially visible tabs");
    assert_eq!(h.state().views.len(), 20);
}
