//! Real tab widgets and pointer events: cancellation and titlebar separation.
use egui::{Event, Key, Modifiers, PointerButton, Pos2, Rect, ViewportCommand, pos2};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use pdfcraft_ui_egui::PdfCraftApp;

fn fixture() -> Vec<u8> {
    let objects =
        ["<< /Type /Catalog /Pages 2 0 R >>", "<< /Type /Pages /Kids [3 0 R] /Count 1 >>", "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 100] >>"];
    let mut out = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    for (i, object) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n{object}\nendobj\n", i + 1).as_bytes());
    }
    let xref = out.len();
    out.extend_from_slice(b"xref\n0 4\n0000000000 65535 f \n");
    for offset in offsets {
        out.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(format!("trailer\n<< /Size 4 /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n").as_bytes());
    out
}

fn harness(count: usize, width: f32) -> Harness<'static, PdfCraftApp> {
    let mut h = Harness::builder().with_step_dt(1.0 / 60.0).with_size(egui::vec2(width, 900.0)).build_eframe(move |_| {
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

fn body(h: &Harness<'_, PdfCraftApp>, name: &str) -> Rect {
    h.get_by_label(name).rect()
}

fn press(h: &mut Harness<'_, PdfCraftApp>, from: Pos2) {
    h.hover_at(from);
    h.step();
    h.drag_at(from);
    h.step();
}

fn release(h: &mut Harness<'_, PdfCraftApp>, to: Pos2) {
    h.input_mut().events.push(Event::PointerButton { pos: to, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
    h.step();
}

fn order(h: &Harness<'_, PdfCraftApp>) -> Vec<pdfcraft_engine::DocId> {
    h.state().views.iter().map(|v| v.id).collect()
}
fn native_drag(h: &Harness<'_, PdfCraftApp>) -> bool {
    h.output().viewport_output.values().flat_map(|v| &v.commands).any(|c| matches!(c, ViewportCommand::StartDrag))
}

#[test]
fn dragging_tab_commits_on_release_and_preserves_active_document() {
    let mut h = harness(3, 1400.0);
    let initial = order(&h);
    let active = h.state().active_ids().unwrap().1;
    let from = body(&h, "document-0.pdf").center();
    let to = body(&h, "document-2.pdf").right_center() - egui::vec2(1.0, 0.0);
    press(&mut h, from);
    h.hover_at(to);
    h.step();
    assert_eq!(order(&h), initial, "preview must not mutate order");
    assert!(!native_drag(&h), "document-tab drag must not move native window");
    release(&mut h, to);
    assert_eq!(order(&h), [initial[1], initial[2], initial[0]]);
    assert_eq!(h.state().active_ids().unwrap().1, active);
}

#[test]
fn escape_and_outside_release_cancel_without_closing_tabs() {
    for escape in [true, false] {
        let mut h = harness(3, 1400.0);
        let initial = order(&h);
        let from = body(&h, "document-0.pdf").center();
        let mut to = body(&h, "document-2.pdf").center();
        press(&mut h, from);
        h.hover_at(to);
        h.step();
        if escape {
            h.key_press(Key::Escape);
            h.step();
        } else {
            to.y += 100.0;
            h.hover_at(to);
            h.step();
        }
        release(&mut h, to);
        assert_eq!(order(&h), initial);
        assert_eq!(h.state().session.docs().len(), 3);
    }
}

#[test]
fn small_motion_selects_tab_and_close_button_cannot_reorder() {
    let mut h = harness(3, 1400.0);
    let initial = order(&h);
    let rect = body(&h, "document-0.pdf");
    press(&mut h, rect.center());
    release(&mut h, rect.center() + egui::vec2(1.0, 0.0));
    assert_eq!(order(&h), initial);
    assert_eq!(h.state().active_ids().unwrap().1, initial[0]);
    // The close button retains its position within the accessible full tab.
    let close = rect.right_center() - egui::vec2(16.0, 0.0);
    press(&mut h, close);
    let to = body(&h, "document-2.pdf").center();
    h.hover_at(to);
    h.step();
    release(&mut h, to);
    assert_eq!(order(&h), initial);
    press(&mut h, close);
    release(&mut h, close);
    assert_eq!(order(&h), [initial[1], initial[2]]);
}

#[test]
fn overflow_edge_scroll_exposes_hidden_tabs_before_commit() {
    let mut h = harness(20, 820.0);
    let initial = order(&h);
    h.state_mut().active = Some(0);
    h.run_steps(4);
    let first = body(&h, "document-0.pdf");
    let right_arrow = body(&h, "Scroll tabs right");
    press(&mut h, first.center());
    let edge = pos2(right_arrow.left() - 6.0, first.center().y);
    h.hover_at(edge);
    h.step();
    for _ in 0..80 {
        h.step();
    }
    assert_eq!(order(&h), initial);
    release(&mut h, edge);
    let reordered = order(&h);
    assert!(reordered.iter().position(|id| *id == initial[0]).unwrap() > 2, "edge scrolling must allow insertion beyond initially visible tabs");
    assert_eq!(reordered.len(), 20);
}

#[test]
fn empty_titlebar_still_starts_native_window_movement() {
    let mut h = harness(2, 1400.0);
    // Between Open and the right-aligned theme/help/community controls.
    let from = pos2(850.0, 19.0);
    press(&mut h, from);
    h.hover_at(from + egui::vec2(20.0, 0.0));
    h.step();
    assert!(native_drag(&h));
}

#[test]
fn context_menu_moves_inactive_target_and_disables_strip_boundaries() {
    let mut h = harness(3, 1400.0);
    let initial = order(&h);
    let active = h.state().active_ids().unwrap().1;
    let at = body(&h, "document-0.pdf").center();
    for pressed in [true, false] {
        h.input_mut().events.push(Event::PointerButton { pos: at, button: PointerButton::Secondary, pressed, modifiers: Modifiers::NONE });
        h.step();
    }
    assert!(h.get_by_label("Move Tab Left").accesskit_node().is_disabled());
    h.get_by_label("Move Tab Right").click();
    h.step();
    assert_eq!(order(&h), [initial[1], initial[0], initial[2]]);
    assert_eq!(h.state().active_ids().unwrap().1, active);
}

#[test]
fn keyboard_palette_can_move_tab_without_mouse_drag() {
    let mut h = harness(3, 1400.0);
    let initial = order(&h);
    h.state_mut().execute("view.palette");
    h.state_mut().palette_query = "Move tab left".into();
    h.run_steps(3);
    h.key_press(Key::Enter);
    h.step();
    assert_eq!(order(&h), [initial[0], initial[2], initial[1]]);
    assert_eq!(h.state().active_ids().unwrap().1, initial[2]);
}

#[test]
fn combine_remains_after_documents_and_home_selection_survives_move() {
    let mut h = harness(3, 1400.0);
    let initial = order(&h);
    h.state_mut().open_combine_tab();
    h.run_steps(3);
    assert!(h.state_mut().move_tab(0, 2));
    h.step();
    assert_eq!(order(&h), [initial[1], initial[2], initial[0]]);
    assert!(h.state().combine_showing());
    let combine = h.get_by_role_and_label(egui::accesskit::Role::Tab, "Combine files").rect().center();
    let to = body(&h, "document-1.pdf").center();
    press(&mut h, combine);
    h.hover_at(to);
    h.step();
    release(&mut h, to);
    assert_eq!(order(&h), [initial[1], initial[2], initial[0]]);
    assert!(h.state().combine_showing());
}

#[test]
fn drag_feedback_works_in_both_themes_without_animation() {
    for theme in ["light", "dark"] {
        let mut h = harness(3, 1400.0);
        h.state_mut().execute(&format!("view.theme.{theme}"));
        h.state_mut().set_option("animation_effects", "off").unwrap();
        h.run_steps(3);
        let from = body(&h, "document-0.pdf").center();
        let to = body(&h, "document-2.pdf").center();
        press(&mut h, from);
        h.hover_at(to);
        h.step();
        assert!(
            h.output().shapes.iter().any(|s| matches!(&s.shape, egui::Shape::LineSegment { stroke, .. } if stroke.width == 3.0)),
            "insertion indicator must be painted without animation"
        );
        // Optional local visual QA; CI needs no GPU or snapshot golden files.
        if let Some(dir) = std::env::var_os("PDFCRAFT_TAB_REORDER_SHOTS") {
            let path = std::path::PathBuf::from(dir).join(format!("tab-reorder-{theme}.png"));
            h.render().unwrap().save(path).unwrap();
        }
    }
}

#[test]
fn double_click_maximizes_empty_titlebar_but_not_document_tabs() {
    for empty in [true, false] {
        let mut h = harness(3, 1400.0);
        let initial = order(&h);
        let at = if empty { pos2(850.0, 19.0) } else { body(&h, "document-0.pdf").center() };
        for _ in 0..2 {
            press(&mut h, at);
            release(&mut h, at);
        }
        let maximized = h.output().viewport_output.values().flat_map(|v| &v.commands).any(|c| matches!(c, ViewportCommand::Maximized(_)));
        assert_eq!(maximized, empty);
        assert_eq!(order(&h), initial);
    }
}

#[test]
fn active_tab_move_preserves_actual_scroll_position_and_cached_view() {
    let mut h = harness(3, 1400.0);
    h.state_mut().active = Some(0);
    h.state_mut().set_option("zoom", "1600").unwrap();
    h.state_mut().set_option("animation_effects", "off").unwrap();
    h.run_steps(5);
    let id = h.state().active_ids().unwrap().1;
    let page_before = h.state().views[0].visible_page_rects()[0].1;
    let point = h.state().views[0].viewport_rect().center();
    h.hover_at(point);
    h.step();
    h.input_mut().events.push(Event::MouseWheel {
        unit: egui::MouseWheelUnit::Point,
        delta: egui::vec2(0.0, -250.0),
        phase: egui::TouchPhase::Move,
        modifiers: Modifiers::NONE,
    });
    h.run_steps(40);
    for _ in 0..200 {
        let m = h.state().views[0].raster_memory();
        if m.page_bytes + m.tile_bytes > 0 {
            break;
        }
        h.step();
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let scrolled = h.state().views[0].visible_page_rects()[0].1;
    assert!(scrolled.top() < page_before.top() - 100.0, "the test must actually scroll before moving");
    let cached = |m: pdfcraft_ui_egui::RasterMemory| (m.page_count, m.page_bytes, m.tile_count, m.tile_bytes, m.thumbnail_count, m.thumbnail_bytes);
    let memory = h.state().views[0].raster_memory();
    assert!(memory.page_bytes + memory.tile_bytes > 0, "the test must populate a render cache before moving");
    assert!(h.state_mut().move_tab(0, 2));
    assert_eq!(cached(h.state().views[2].raster_memory()), cached(memory), "placement itself must not clear view caches");
    h.run_steps(3);
    let moved = &h.state().views[2];
    assert_eq!(h.state().active_ids().unwrap().1, id);
    assert_eq!(moved.zoom, 16.0);
    let after = moved.visible_page_rects()[0].1;
    assert!((after.top() - scrolled.top()).abs() < 1.0, "moving the active tab must not reset egui scroll memory: {scrolled:?} -> {after:?}");
}

#[test]
fn closing_the_dragged_tab_cancels_placement() {
    let mut h = harness(3, 1400.0);
    let initial = order(&h);
    let from = body(&h, "document-0.pdf").center();
    let to = body(&h, "document-2.pdf").right_center() - egui::vec2(1.0, 0.0);
    press(&mut h, from);
    h.hover_at(to);
    h.step();
    h.state_mut().close_tab(0);
    h.step();
    release(&mut h, to);
    assert_eq!(order(&h), [initial[1], initial[2]]);
    assert_eq!(h.state().session.docs().len(), 2);
}

#[test]
fn closing_another_tab_during_drag_recomputes_placement() {
    let mut h = harness(4, 1400.0);
    let initial = order(&h);
    let from = body(&h, "document-0.pdf").center();
    let to = body(&h, "document-3.pdf").center();
    press(&mut h, from);
    h.hover_at(to);
    h.step();
    h.state_mut().close_tab(1);
    h.step();
    let to = body(&h, "document-3.pdf").right_center() - egui::vec2(1.0, 0.0);
    h.hover_at(to);
    h.step();
    release(&mut h, to);
    assert_eq!(order(&h), [initial[2], initial[3], initial[0]]);
    h.state_mut().close_tab(2);
    assert_eq!(order(&h), [initial[2], initial[3]]);
}

#[test]
fn switching_active_document_during_drag_preserves_latest_selection() {
    let mut h = harness(3, 1400.0);
    let initial = order(&h);
    let from = body(&h, "document-0.pdf").center();
    let to = body(&h, "document-2.pdf").right_center() - egui::vec2(1.0, 0.0);
    press(&mut h, from);
    h.hover_at(to);
    h.step();
    h.state_mut().active = Some(1);
    h.step();
    release(&mut h, to);
    assert_eq!(order(&h), [initial[1], initial[2], initial[0]]);
    assert_eq!(h.state().active_ids().unwrap().1, initial[1]);
}

#[test]
fn empty_and_single_document_workspaces_reject_moves() {
    for count in [0, 1] {
        let mut h = harness(count, 1400.0);
        let initial = order(&h);
        assert!(!h.state_mut().move_tab(0, 0));
        assert!(!h.state_mut().move_tab(0, 1));
        h.state_mut().execute("tab.move_left");
        h.state_mut().execute("tab.move_right");
        h.step();
        assert_eq!(order(&h), initial);
        assert_eq!(h.state().session.docs().len(), count);
    }
}

#[test]
fn dragging_last_tab_to_first_preserves_active_document() {
    let mut h = harness(3, 1400.0);
    let initial = order(&h);
    let active = h.state().active_ids().unwrap().1;
    let from = body(&h, "document-2.pdf").center();
    let to = body(&h, "document-0.pdf").left_center() + egui::vec2(1.0, 0.0);
    press(&mut h, from);
    h.hover_at(to);
    h.step();
    release(&mut h, to);
    assert_eq!(order(&h), [initial[2], initial[0], initial[1]]);
    assert_eq!(h.state().active_ids().unwrap().1, active);
}
