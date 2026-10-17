//! Tool accelerators and keyboard movement through the real document workspace.
use egui::{Key, Modifiers};
use egui_kittest::{Harness, kittest::Queryable};
use pdfcraft_ui_egui::{PdfCraftApp, QuickTool, comments::CommentTool};

fn harness() -> Harness<'static, PdfCraftApp> {
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(|_| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.set_option("animation_effects", "off").unwrap();
        app.open_bytes("form.pdf", None, include_bytes!("data/form.pdf").to_vec()).unwrap();
        app
    });
    h.run_steps(12);
    h
}

#[test]
fn tool_accelerators_select_and_cycle_without_extra_modifiers() {
    let mut h = harness();
    for (key, tool) in [
        (Key::Z, QuickTool::MarqueeZoom),
        (Key::H, QuickTool::Hand),
        (Key::V, QuickTool::Select),
        (Key::U, QuickTool::Comment(CommentTool::Highlight)),
        (Key::U, QuickTool::Comment(CommentTool::Underline)),
        (Key::U, QuickTool::Comment(CommentTool::StrikeOut)),
        (Key::U, QuickTool::Comment(CommentTool::Highlight)),
        (Key::D, QuickTool::Comment(CommentTool::Line)),
        (Key::D, QuickTool::Comment(CommentTool::Arrow)),
        (Key::D, QuickTool::Comment(CommentTool::Rectangle)),
        (Key::D, QuickTool::Comment(CommentTool::Line)),
        (Key::S, QuickTool::Comment(CommentTool::Note)),
        (Key::T, QuickTool::Comment(CommentTool::TextBox)),
    ] {
        h.key_press(key);
        h.run_steps(3);
        assert_eq!(h.state().quick_tool, tool, "{key:?}");
    }
    h.key_press_modifiers(Modifiers::SHIFT, Key::U);
    h.run_steps(2);
    assert_eq!(h.state().quick_tool, QuickTool::Comment(CommentTool::TextBox));
}

#[test]
fn tool_accelerators_work_after_clicking_the_document() {
    let mut h = harness();
    let page = h.state().views[0].page_screen_rect(0).unwrap();
    let at = egui::pos2(page.right() - 4.0, page.top() + 30.0);
    h.hover_at(at);
    h.drag_at(at);
    h.drop_at(at);
    h.run_steps(3);
    assert!(h.state().views[0].forms.focus.is_none());
    h.key_press(Key::Z);
    h.run_steps(3);
    assert_eq!(h.state().quick_tool, QuickTool::MarqueeZoom);
}

#[test]
fn space_still_temporarily_pans_the_focused_document_without_a_selected_item() {
    let mut h = harness();
    focus_document(&mut h);
    h.input_mut().events.push(egui::Event::Key { key: Key::Space, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE });
    h.step();
    assert_eq!(h.state().quick_tool, QuickTool::Hand);
    h.input_mut().events.push(egui::Event::Key { key: Key::Space, physical_key: None, pressed: false, repeat: false, modifiers: Modifiers::NONE });
    h.step();
    assert_eq!(h.state().quick_tool, QuickTool::Select);
}

#[test]
fn f6_cycles_visible_regions_and_tab_enters_toolbar_controls() {
    let mut h = harness();
    h.state_mut().set_option("panel", "comments").unwrap();
    h.run_steps(4);
    for name in ["Toolbar", "Tools", "Side panel", "Document"] {
        h.key_press(Key::F6);
        h.run_steps(3);
        assert!(h.query_all_by_label(name).any(|n| n.is_focused()), "F6 focuses {name}");
    }
    h.key_press_modifiers(Modifiers::SHIFT, Key::F6);
    h.run_steps(3);
    assert!(h.query_all_by_label("Side panel").any(|n| n.is_focused()));
    h.key_press(Key::F6);
    h.run_steps(3);
    h.key_press(Key::F6);
    h.run_steps(3);
    h.key_press(Key::Tab);
    h.run_steps(3);
    assert!(h.get_by_label("Menu").is_focused(), "Tab enters the toolbar's first control");
    h.key_press(Key::Enter);
    h.run_steps(3);
    assert!(egui::Popup::is_any_open(&h.ctx), "Enter activates the focused Menu button");
}

fn focus_document(h: &mut Harness<'static, PdfCraftApp>) {
    for _ in 0..6 {
        if h.query_all_by_role(egui::accesskit::Role::Document).any(|n| n.is_focused()) {
            return;
        }
        h.key_press(Key::F6);
        h.run_steps(3);
    }
    panic!(
        "F6 must reach the document; document nodes: {:?}",
        h.query_all_by_role(egui::accesskit::Role::Document).map(|n| format!("{n:?}")).collect::<Vec<_>>()
    );
}

#[test]
fn tab_visits_fields_and_annotations_without_changing_values_and_wraps_backwards() {
    use pdfcraft_engine::{Edit, NewAnnotation, Shape, Style};
    let mut h = harness();
    let shape = Shape::Rectangle { rect: [10.0, 10.0, 60.0, 40.0] };
    h.state_mut().apply_edit(Edit::AddAnnotation(NewAnnotation {
        page: 0,
        style: Style::default_for(&shape),
        shape,
        contents: "Keyboard comment".into(),
        author: "Test".into(),
    }));
    h.run_steps(4);
    let (targets, before) = {
        let app = h.state();
        let doc = app.session.get(app.views[0].id).unwrap();
        (doc.page_focus_targets(0), doc.form.clone())
    };
    assert!(targets.len() > 4);
    focus_document(&mut h);
    for target in &targets {
        h.key_press(Key::Tab);
        h.run_steps(5);
        assert_eq!(h.state().views[0].keyboard_item.as_ref().map(|(_, item)| item), Some(&target.item), "{}", target.label);
    }
    h.key_press(Key::Tab);
    h.run_steps(4);
    assert_eq!(h.state().views[0].keyboard_item.as_ref().map(|(_, item)| item), Some(&targets[0].item));
    h.key_press_modifiers(Modifiers::SHIFT, Key::Tab);
    h.run_steps(4);
    assert_eq!(h.state().views[0].keyboard_item.as_ref().map(|(_, item)| item), targets.last().map(|t| &t.item));
    let app = h.state();
    assert_eq!(&app.session.get(app.views[0].id).unwrap().form, &before, "focus alone never toggles a field");
    assert!(app.views[0].comments.selected.is_some());
    assert!(h.query_all_by_label("Document").any(|n| n.is_focused()), "document retains focus after reverse Tab");
    h.key_press(Key::Enter);
    h.run_steps(4);
    assert!(
        h.state().views[0].comments.composer.is_some(),
        "Enter opens the focused comment; item={:?}, focused={:?}, text={}, toast={:?}",
        h.state().views[0].keyboard_item,
        h.ctx.memory(|m| m.focused()),
        h.ctx.text_edit_focused(),
        h.state().toast
    );
}

#[test]
fn focused_checkbox_activates_with_space_and_undo_restores_it() {
    use pdfcraft_engine::keyboard::PageItem;
    let mut h = harness();
    focus_document(&mut h);
    for _ in 0..20 {
        h.key_press(Key::Tab);
        h.run_steps(4);
        if matches!(&h.state().views[0].keyboard_item, Some((_, PageItem::Field { name, .. })) if name == "agree") {
            break;
        }
    }
    assert!(matches!(&h.state().views[0].keyboard_item, Some((_, PageItem::Field { name, .. })) if name == "agree"));
    let checked = |h: &Harness<'static, PdfCraftApp>| {
        let a = h.state();
        !a.session.get(a.views[0].id).unwrap().form.iter().find(|f| f.name == "agree").unwrap().value.is_empty()
    };
    assert!(!checked(&h));
    h.key_press(Key::Space);
    h.run_steps(4);
    assert!(checked(&h));
    h.state_mut().undo();
    h.run_steps(3);
    assert!(!checked(&h));
}

#[test]
fn hidden_regions_are_skipped_and_dialogs_keep_their_focus() {
    let mut h = harness();
    h.state_mut().set_option("left", "closed").unwrap();
    h.state_mut().set_option("panel", "none").unwrap();
    h.run_steps(4);
    focus_document(&mut h);
    h.key_press(Key::F6);
    h.run_steps(3);
    assert!(h.query_all_by_label("Toolbar").any(|n| n.is_focused()));
    h.key_press(Key::F6);
    h.run_steps(3);
    assert!(h.query_all_by_label("Document").any(|n| n.is_focused()));
    h.state_mut().execute("help.about");
    h.run_steps(3);
    let focused = h.ctx.memory(|m| m.focused());
    h.key_press(Key::F6);
    h.run_steps(3);
    assert_eq!(h.ctx.memory(|m| m.focused()), focused);
    assert!(h.state().dialog.is_some());
}

#[test]
fn f6_preserves_the_last_form_keystrokes_before_moving_to_the_toolbar() {
    let mut h = harness();
    focus_document(&mut h);
    h.key_press(Key::Tab);
    h.run_steps(4);
    let field = h.state().views[0]
        .forms
        .focus
        .as_ref()
        .unwrap_or_else(|| panic!("first text field; item={:?}", h.state().views[0].keyboard_item))
        .name
        .clone();
    for key in [Key::R, Key::I, Key::X, Key::M] {
        h.key_press(key);
        h.run_steps(2);
        assert_eq!(h.state().quick_tool, QuickTool::Select, "the form editor owns {key:?}");
    }
    h.input_mut().events.extend([
        egui::Event::Text("Keyboard draft rixm".into()),
        egui::Event::Key { key: Key::F6, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE },
    ]);
    h.run_steps(5);
    let a = h.state();
    assert_eq!(a.session.get(a.views[0].id).unwrap().form.iter().find(|f| f.name == field).unwrap().value, ["Keyboard draft rixm"]);
    assert!(h.query_all_by_label("Toolbar").any(|n| n.is_focused()));
}

#[test]
fn deleting_a_keyboard_selected_comment_clears_its_stale_index() {
    use pdfcraft_engine::{Edit, NewAnnotation, Shape, Style};
    let mut h = harness();
    let shape = Shape::Rectangle { rect: [10.0, 10.0, 60.0, 40.0] };
    h.state_mut().apply_edit(Edit::AddAnnotation(NewAnnotation {
        page: 0,
        style: Style::default_for(&shape),
        shape,
        contents: "Delete me".into(),
        author: "Test".into(),
    }));
    h.run_steps(4);
    h.state_mut().views[0].comments.selected = None;
    focus_document(&mut h);
    h.key_press_modifiers(Modifiers::SHIFT, Key::Tab);
    h.run_steps(4);
    assert!(h.state().views[0].comments.selected.is_some());
    h.key_press(Key::Delete);
    h.run_steps(4);
    assert!(h.state().views[0].keyboard_item.is_none());
    assert!(h.state().session.get(h.state().views[0].id).unwrap().info.annotations.is_empty());
    h.key_press(Key::Enter);
    h.run_steps(3);
    assert!(h.state().views[0].comments.composer.is_none());
}
