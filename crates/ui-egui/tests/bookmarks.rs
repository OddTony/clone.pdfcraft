//! Editing bookmarks in the Bookmarks panel (M4.6), driven like a user would.

use egui::{Key, Modifiers};
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use pdfcraft_ui_egui::PdfCraftApp;

const PAGES: &[u8] = b"%PDF-1.7
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
        app.open_bytes("pages.pdf", None, PAGES.to_vec()).expect("opens");
        app.set_option("left", "closed").unwrap();
        app.set_option("panel", "bookmarks").unwrap();
        app
    });
    h.run_steps(6);
    h
}

fn outline(h: &Harness<'static, PdfCraftApp>) -> Vec<(String, Option<usize>, usize)> {
    let id = h.state().views[0].id;
    fn flat(items: &[pdfcraft_render::OutlineItem], depth: usize, out: &mut Vec<(String, Option<usize>, usize)>) {
        for i in items {
            out.push((i.title.clone(), i.page, depth));
            flat(&i.children, depth + 1, out);
        }
    }
    let mut out = Vec::new();
    flat(&h.state().session.get(id).unwrap().info.outline, 0, &mut out);
    out
}

/// The inline rename editor (the text field, not its inner text run).
fn field<'a>(h: &'a Harness<'static, PdfCraftApp>, value: &'a str) -> egui_kittest::Node<'a> {
    h.query_all_by_value(value).next().unwrap_or_else(|| panic!("no editor showing {value:?}"))
}

/// Add a bookmark with ⌘B at the current page and name it by typing.
fn add(h: &mut Harness<'static, PdfCraftApp>, title: &str) {
    h.key_press_modifiers(Modifiers::COMMAND, Key::B);
    h.run_steps(3);
    field(h, "Untitled"); // the inline editor is open and focused
    h.key_press_modifiers(Modifiers::COMMAND, Key::A);
    h.run_steps(1);
    field(h, "Untitled").type_text(title);
    h.run_steps(1);
    h.key_press(Key::Enter);
    h.run_steps(3);
}

fn menu(h: &mut Harness<'static, PdfCraftApp>, bookmark: &str, item: &str) {
    h.get_by_label(bookmark).click_secondary();
    h.run_steps(2);
    h.get_by_label(item).click();
    h.run_steps(3);
}

#[test]
fn new_rename_reorder_indent_delete_and_undo() {
    let mut h = harness();
    h.get_by_label("This document has no bookmarks.");
    add(&mut h, "Intro");
    h.state_mut().set_option("page", "3").unwrap();
    h.run_steps(3);
    add(&mut h, "Results");
    assert_eq!(outline(&h), [("Intro".into(), Some(0), 0), ("Results".into(), Some(2), 0)]);

    menu(&mut h, "Results", "Indent");
    assert_eq!(outline(&h), [("Intro".into(), Some(0), 0), ("Results".into(), Some(2), 1)]);
    menu(&mut h, "Results", "Outdent");
    menu(&mut h, "Results", "Move up");
    assert_eq!(outline(&h)[0].0, "Results");

    menu(&mut h, "Intro", "Rename");
    field(&h, "Intro");
    h.key_press_modifiers(Modifiers::COMMAND, Key::A);
    h.run_steps(1);
    field(&h, "Intro").type_text("Introduction");
    h.key_press(Key::Enter);
    h.run_steps(3);
    assert_eq!(outline(&h)[1].0, "Introduction");

    menu(&mut h, "Results", "Delete");
    assert_eq!(outline(&h), [("Introduction".into(), Some(0), 0)]);
    h.key_press_modifiers(Modifiers::COMMAND, Key::Z);
    h.run_steps(3);
    assert_eq!(outline(&h).len(), 2, "delete is undoable");
    assert!(h.state().session.get(h.state().views[0].id).unwrap().dirty);
}

/// The side panel's width as egui keeps it from one frame to the next.
fn panel_width(h: &Harness<'static, PdfCraftApp>) -> f32 {
    h.ctx.data(|data| data.get_temp::<egui::Rect>(egui::Id::new("pdfcraft-inspector-body"))).expect("the inspector body is shown").width()
}

#[test]
fn the_panel_keeps_its_width_while_the_pointer_moves() {
    // The search row was 2 px wider than the panel, and a resizable panel keeps the width its
    // content used, so every repaint (any pointer move) widened it until it reached its maximum.
    let mut h = harness();
    add(&mut h, "Intro");
    h.get_by_label("Search"); // the search row shows once there are bookmarks
    let before = panel_width(&h);
    for i in 0..30 {
        h.hover_at(egui::pos2(300.0 + 10.0 * i as f32, 400.0));
        h.run_steps(1);
    }
    assert_eq!(panel_width(&h), before, "the Bookmarks panel drifted wider");
}

#[test]
fn escape_cancels_a_rename() {
    let mut h = harness();
    add(&mut h, "Keep");
    menu(&mut h, "Keep", "Rename");
    field(&h, "Keep").type_text("zzz");
    h.key_press(Key::Escape);
    h.run_steps(3);
    assert_eq!(outline(&h)[0].0, "Keep");
}

#[test]
fn number_pages_dialog_labels_the_selected_pages() {
    let mut h = harness();
    h.state_mut().views[0].select_pages(&[0, 1]);
    assert!(h.state_mut().execute("page.number"));
    h.run_steps(3);
    h.get_by_label("Number pages");
    h.get_by_value("1, 2, 3").click(); // the style menu
    h.run_steps(2);
    h.get_by_label("i, ii, iii").click();
    h.run_steps(2);
    h.get_by_label_contains("Labels: i, ii");
    h.get_by_label("OK").click();
    h.run_steps(3);
    let id = h.state().views[0].id;
    let labels: Vec<String> = h.state().session.get(id).unwrap().info.pages.iter().map(|p| p.label.clone()).collect();
    assert_eq!(labels, ["i", "ii", "3"]);
    assert_eq!(h.state().session.get(id).unwrap().can_undo(), Some("Number pages"));
}

#[test]
fn search_finds_collapsed_bookmarks_and_keeps_original_paths() {
    use pdfcraft_engine::Edit;
    let mut h = harness();
    for (parent, index, title, page) in
        [(vec![], 0, "Unrelated", 0), (vec![], 1, "Reports", 0), (vec![1], 0, "Other chapter", 1), (vec![1], 1, "Résumé results", 2)]
    {
        h.state_mut().apply_edit(Edit::AddBookmark { parent, index, title: title.into(), page });
    }
    h.run_steps(3);
    h.get_by_label("Bookmark options").click();
    h.run_steps(2);
    h.get_by_label("Collapse all bookmarks").click();
    h.run_steps(3);
    assert!(h.query_by_label("Résumé results").is_none());
    let id = h.state().views[0].id;
    // A full save stamps /ModDate from the clock: pin it, or two saves a second apart differ.
    let session = std::mem::take(&mut h.state_mut().session);
    h.state_mut().session = session.with_clock(|| 1_700_000_000);
    let before = h.state().session.save_full_bytes(id).unwrap();
    h.get_by_label("Search").click();
    h.run_steps(1);
    h.get_by_label("Search").type_text("RÉSUMÉ");
    h.run_steps(3);
    h.get_by_label("Reports");
    h.get_by_label("Résumé results").click();
    h.run_steps(3);
    assert_eq!(h.state().views[0].current, 2);
    assert!(h.query_by_label("Unrelated").is_none());
    assert!(h.query_by_label("Other chapter").is_none());
    assert_eq!(h.state().session.save_full_bytes(id).unwrap(), before, "search doesn't edit the PDF");
    if let Ok(dir) = std::env::var("PDFCRAFT_BOOKMARK_SHOTS") {
        h.render().unwrap().save(std::path::Path::new(&dir).join("filtered-bookmarks.png")).unwrap();
    }
    menu(&mut h, "Résumé results", "Delete");
    assert_eq!(outline(&h).len(), 3);
    assert!(outline(&h).iter().any(|b| b.0 == "Other chapter"));
    h.get_by_label("No matches.");
    add(&mut h, "New section");
    assert!(outline(&h).iter().any(|b| b.0 == "New section"), "new bookmarks remain nameable while searching");
    h.get_by_label("No matches.");
    h.get_by_label("Clear").click();
    h.run_steps(3);
    h.get_by_label("Unrelated");
    assert!(h.query_by_label("Other chapter").is_none(), "search did not change saved expansion state");
}

#[test]
fn bookmark_search_is_per_document() {
    let mut h = harness();
    add(&mut h, "First document");
    h.get_by_label("Search").click();
    h.run_steps(1);
    h.get_by_label("Search").type_text("not found");
    h.run_steps(3);
    h.get_by_label("No matches.");
    h.state_mut().open_bytes("second.pdf", None, PAGES.to_vec()).unwrap();
    h.run_steps(3);
    add(&mut h, "Second document");
    h.get_by_label("Second document");
    h.state_mut().active = Some(0);
    h.run_steps(3);
    h.get_by_label("No matches.");
    h.get_by_label("Clear").click();
    h.run_steps(3);
    h.get_by_label("First document");
}

#[test]
fn new_bookmarks_from_structure_nest_the_tagged_headings() {
    const TAGGED: &[u8] = b"%PDF-1.7
1 0 obj << /Type /Catalog /Pages 2 0 R /StructTreeRoot 5 0 R >> endobj
2 0 obj << /Type /Pages /Kids [3 0 R 4 0 R] /Count 2 /MediaBox [0 0 300 400] >> endobj
3 0 obj << /Type /Page /Parent 2 0 R >> endobj
4 0 obj << /Type /Page /Parent 2 0 R >> endobj
5 0 obj << /Type /StructTreeRoot /K [6 0 R 7 0 R] >> endobj
6 0 obj << /S /H1 /Pg 3 0 R /ActualText (Overview) >> endobj
7 0 obj << /S /H2 /Pg 4 0 R /ActualText (Method) >> endobj
trailer << /Root 1 0 R >>
%%EOF";
    let mut h = harness();
    h.state_mut().open_bytes("tagged.pdf", None, TAGGED.to_vec()).unwrap();
    h.run_steps(3);
    h.get_by_label("Bookmark options").click();
    h.run_steps(2);
    h.get_by_label("New bookmarks from structure").click();
    h.run_steps(3);
    let id = h.state().active.and_then(|i| h.state().views.get(i)).unwrap().id;
    let titles: Vec<String> = h.state().session.get(id).unwrap().info.outline.iter().map(|o| o.title.clone()).collect();
    assert_eq!(titles, ["Untitled"]);
    h.get_by_label("Overview");
    h.get_by_label("Method");
    assert_eq!(h.state().session.get(id).unwrap().can_undo(), Some("New bookmarks from structure"));
    if let Ok(dir) = std::env::var("PDFCRAFT_BOOKMARK_SHOTS") {
        h.render().unwrap().save(std::path::Path::new(&dir).join("bookmarks-from-structure.png")).unwrap();
    }
}

#[test]
fn expanding_and_collapsing_all_bookmarks() {
    use pdfcraft_engine::Edit;
    let mut h = harness();
    for (parent, title) in [(vec![], "Part"), (vec![0], "Chapter"), (vec![0, 0], "Section")] {
        h.state_mut().apply_edit(Edit::AddBookmark { parent, index: 0, title: title.into(), page: 0 });
    }
    h.run_steps(3);
    let shown = |h: &Harness<'static, PdfCraftApp>, t: &str| h.query_by_label(t).is_some();
    h.get_by_label("Bookmark options").click();
    h.run_steps(2);
    h.get_by_label("Expand all bookmarks").click();
    h.run_steps(3);
    assert!(shown(&h, "Chapter") && shown(&h, "Section"));
    h.get_by_label("Bookmark options").click();
    h.run_steps(2);
    h.get_by_label("Collapse all bookmarks").click();
    h.run_steps(3);
    assert!(shown(&h, "Part") && !shown(&h, "Chapter"));
    h.get_by_label("Bookmark options").click();
    h.run_steps(2);
    h.get_by_label("Expand top-level bookmarks").click();
    h.run_steps(3);
    assert!(shown(&h, "Chapter") && !shown(&h, "Section"));
}

/// Synthetic three-level outline, duplicate titles and distinct page destinations. No PDF
/// binaries or fonts are needed; fixtures are built through the existing engine operations.
fn tree() -> Harness<'static, PdfCraftApp> {
    use pdfcraft_engine::Edit;
    let mut h = harness();
    for (parent, index, title, page) in [
        (vec![], 0, "A", 0),
        (vec![], 1, "B", 1),
        (vec![], 2, "C", 2),
        (vec![], 3, "D", 0),
        (vec![1], 0, "B0", 2),
        (vec![1], 1, "B1", 0),
        (vec![1], 2, "Duplicate", 1),
        (vec![1, 0], 0, "Grandchild", 1),
        (vec![2], 0, "Duplicate", 2),
    ] {
        h.state_mut().apply_edit(Edit::AddBookmark { parent, index, title: title.into(), page });
    }
    h.run_steps(3);
    h
}

fn drag(h: &mut Harness<'static, PdfCraftApp>, from: egui::Pos2, to: egui::Pos2) {
    start_drag(h, from, to);
    h.drop_at(to);
    h.run_steps(4);
}

fn start_drag(h: &mut Harness<'static, PdfCraftApp>, from: egui::Pos2, to: egui::Pos2) {
    h.hover_at(from);
    h.run_steps(1);
    h.drag_at(from);
    h.run_steps(1);
    for k in 1..=6 {
        h.hover_at(from.lerp(to, k as f32 / 6.0));
        h.run_steps(1);
    }
}

fn row(h: &Harness<'static, PdfCraftApp>, name: &str) -> egui::Rect {
    h.get_by_label(name).rect()
}

fn roots(h: &Harness<'static, PdfCraftApp>) -> Vec<String> {
    outline(h).into_iter().filter(|(_, _, d)| *d == 0).map(|(t, _, _)| t).collect()
}

#[test]
fn real_drag_reorders_siblings_once_and_undo_redo_restore_the_tree() {
    for (source, target, below, expected) in [
        ("D", "A", false, vec!["D", "A", "B", "C"]),
        ("A", "D", true, vec!["B", "C", "D", "A"]),
        ("A", "B", true, vec!["B", "A", "C", "D"]),
        ("D", "B", false, vec!["A", "D", "B", "C"]),
    ] {
        let mut h = tree();
        let before = outline(&h);
        let id = h.state().views[0].id;
        let generation = h.state().session.get(id).unwrap().edit_generation();
        let a = row(&h, source).center();
        let r = row(&h, target);
        let b = egui::pos2(r.center().x, if below { r.bottom() - 2.0 } else { r.top() + 2.0 });
        start_drag(&mut h, a, b);
        assert_eq!(outline(&h), before, "pointer movement must never edit");
        h.drop_at(b);
        h.run_steps(4);
        assert_eq!(roots(&h), expected);
        assert_eq!(h.state().session.get(id).unwrap().edit_generation(), generation + 1, "one drop = one edit");
        h.state_mut().undo();
        h.run_steps(2);
        assert_eq!(outline(&h), before);
        h.state_mut().redo();
        h.run_steps(2);
        assert_eq!(roots(&h), expected);
    }
}

#[test]
fn real_drag_moves_a_subtree_into_a_later_parent_and_back_to_root() {
    let mut h = tree();
    let from = row(&h, "B").center();
    let to = row(&h, "C").center();
    drag(&mut h, from, to);
    assert_eq!(roots(&h), ["A", "C", "D"]);
    assert_eq!(h.state().views[0].bookmarks.selected, Some(vec![1, 1]));
    assert!(outline(&h).contains(&("B0".into(), Some(2), 2)));
    // B0 is now three levels down; expand its ancestors with the existing menu.
    h.get_by_label("Bookmark options").click();
    h.run_steps(2);
    h.get_by_label("Expand all bookmarks").click();
    h.run_steps(2);
    assert!(outline(&h).contains(&("Grandchild".into(), Some(1), 3)));
    let from = row(&h, "B").center();
    let r = row(&h, "A");
    drag(&mut h, from, egui::pos2(r.center().x, r.top() + 2.0));
    assert_eq!(roots(&h), ["B", "A", "C", "D"]);
    assert_eq!(h.state().views[0].bookmarks.selected, Some(vec![0]));
    assert!(outline(&h).contains(&("Grandchild".into(), Some(1), 2)));
}

#[test]
fn invalid_noop_and_cancelled_ui_drags_never_edit() {
    for mode in ["self", "descendant", "noop", "outside", "escape", "filter", "panel", "tab", "close", "mutation", "readonly"] {
        let mut h = tree();
        if mode == "readonly" {
            h.state_mut().apply_edit(pdfcraft_engine::Edit::Protect(pdfcraft_engine::Protection {
                permissions_password: Some("owner".into()),
                changes: pdfcraft_engine::Changes::None,
                ..Default::default()
            }));
            let id = h.state().views[0].id;
            let bytes = h.state().session.save_bytes(id).unwrap();
            h.state_mut().close_tab(0);
            h.state_mut().open_bytes("protected.pdf", None, bytes.as_ref().clone()).unwrap();
            h.run_steps(2);
        }
        let id = h.state().views[0].id;
        let before = outline(&h);
        let generation = h.state().session.get(id).unwrap().edit_generation();
        let source = if mode == "noop" { "A" } else { "B" };
        let a = row(&h, source).center();
        let b = match mode {
            "self" => a + egui::vec2(6.0, 0.0),
            "descendant" => row(&h, "B0").center(),
            "noop" => {
                let r = row(&h, "B");
                egui::pos2(r.center().x, r.top() + 2.0)
            }
            "outside" => egui::pos2(400.0, 200.0),
            _ => row(&h, "C").center(),
        };
        start_drag(&mut h, a, b);
        match mode {
            "escape" => {
                h.key_press(Key::Escape);
                h.run_steps(2);
            }
            "filter" => {
                // Programmatic query change models a filter activated during a held drag.
                h.ctx.data_mut(|d| d.insert_temp(egui::Id::new(("bookmark_search", id.0)), "C".to_string()));
                h.run_steps(2);
            }
            "panel" => {
                h.state_mut().set_option("panel", "pages").unwrap();
                h.run_steps(2);
            }
            "tab" | "close" => {
                h.state_mut().open_bytes("other.pdf", None, PAGES.to_vec()).unwrap();
                h.run_steps(2);
                if mode == "close" {
                    h.state_mut().close_tab(0);
                } else {
                    h.state_mut().active = Some(0);
                }
                h.run_steps(2);
            }
            "mutation" => {
                h.state_mut().session.apply(id, pdfcraft_engine::Edit::DeleteBookmark { path: vec![0] }).unwrap();
                h.run_steps(2);
            }
            _ => {}
        }
        h.drop_at(b);
        h.run_steps(3);
        if mode == "readonly" {
            h.key_press_modifiers(Modifiers::COMMAND, Key::B);
            h.run_steps(2);
        }
        if mode == "close" {
            assert!(h.state().session.get(id).is_none());
        } else if mode == "mutation" {
            assert_eq!(h.state().session.get(id).unwrap().edit_generation(), generation + 1);
            assert_eq!(roots(&h), ["B", "C", "D"]);
        } else {
            assert_eq!(h.state().session.get(id).unwrap().edit_generation(), generation, "{mode}");
            assert_eq!(outline(&h), before, "{mode}");
        }
    }
}

#[test]
fn creation_uses_selected_sibling_page_and_rename_focus_for_toolbar_and_shortcut() {
    let mut h = tree();
    h.get_by_label("B0").click();
    h.run_steps(2);
    h.state_mut().set_option("page", "2").unwrap();
    h.run_steps(2);
    add(&mut h, "Inserted child");
    assert_eq!(h.state().views[0].bookmarks.selected, Some(vec![1, 1]));
    assert!(outline(&h).contains(&("Inserted child".into(), Some(1), 1)));
    add(&mut h, "Next child");
    assert_eq!(h.state().views[0].bookmarks.selected, Some(vec![1, 2]));
    let names: Vec<_> = outline(&h).into_iter().map(|(t, _, _)| t).collect();
    assert!(names.windows(4).any(|w| w == ["B0", "Grandchild", "Inserted child", "Next child"]));
    h.get_by_label("A").click();
    h.run_steps(2);
    h.get_by_label("New bookmark (Ctrl+B)").click();
    h.run_steps(3);
    field(&h, "Untitled");
    assert_eq!(h.state().views[0].bookmarks.selected, Some(vec![1]));
    h.key_press(Key::Escape);
    h.run_steps(2);
    assert_eq!(roots(&h), ["A", "Untitled", "B", "C", "D"]);
    h.state_mut().undo();
    h.run_steps(2);
    assert_eq!(roots(&h), ["A", "B", "C", "D"]);
    h.state_mut().redo();
    h.run_steps(2);
    assert_eq!(roots(&h), ["A", "Untitled", "B", "C", "D"]);
}

#[test]
fn selection_and_rename_are_per_document_and_follow_moves_and_deletions() {
    let mut h = tree();
    h.get_by_label("B0").click();
    h.run_steps(2);
    h.state_mut().open_bytes("other.pdf", None, PAGES.to_vec()).unwrap();
    h.run_steps(2);
    add(&mut h, "Other first");
    assert_eq!(h.state().views[1].bookmarks.selected, Some(vec![0]));
    h.state_mut().active = Some(0);
    h.run_steps(2);
    add(&mut h, "Child after switching");
    assert_eq!(h.state().views[0].bookmarks.selected, Some(vec![1, 1]));
    menu(&mut h, "Child after switching", "Outdent");
    assert_eq!(h.state().views[0].bookmarks.selected, Some(vec![2]));
    add(&mut h, "Root after moving");
    assert_eq!(roots(&h), ["A", "B", "Child after switching", "Root after moving", "C", "D"]);
    menu(&mut h, "Root after moving", "Delete");
    assert!(h.state().views[0].bookmarks.selected.is_none());
    add(&mut h, "Appended after deleting");
    assert_eq!(roots(&h).last().unwrap(), "Appended after deleting");
    // A rename on A must never appear over a bookmark at the same path in the other PDF.
    menu(&mut h, "A", "Rename");
    h.state_mut().active = Some(1);
    h.run_steps(2);
    assert!(h.query_all_by_value("A").next().is_none());
    h.state_mut().active = Some(0);
    h.run_steps(2);
    field(&h, "A");
}

#[test]
fn collapsed_destination_expands_on_hover_and_new_nested_bookmark_is_visible() {
    let mut h = tree();
    h.get_by_label("Bookmark options").click();
    h.run_steps(2);
    h.get_by_label("Collapse all bookmarks").click();
    h.run_steps(2);
    assert!(h.query_by_label("B0").is_none());
    let from = row(&h, "A").center();
    let to = row(&h, "B").center();
    start_drag(&mut h, from, to);
    h.run_steps(60);
    h.get_by_label("B0");
    h.drop_at(to);
    h.run_steps(3);
    assert_eq!(roots(&h), ["B", "C", "D"]);
    assert_eq!(h.state().views[0].bookmarks.selected, Some(vec![0, 3]));
    add(&mut h, "After dropped child");
    h.get_by_label("After dropped child");
    assert_eq!(h.state().views[0].bookmarks.selected, Some(vec![0, 4]));
}

#[test]
fn ui_bookmark_changes_save_reopen_with_order_hierarchy_and_destinations() {
    let mut h = tree();
    let a = row(&h, "A").center();
    let b = row(&h, "B0").center();
    drag(&mut h, a, b);
    h.state_mut().set_option("page", "3").unwrap();
    h.run_steps(2);
    add(&mut h, "Inserted at depth three");
    let from = row(&h, "D").center();
    let r = row(&h, "B");
    drag(&mut h, from, egui::pos2(r.center().x, r.top() + 2.0));
    let expected = outline(&h);
    let id = h.state().views[0].id;
    let bytes = h.state().session.save_bytes(id).unwrap();
    let parsed = pdfcraft_cos::Document::open(bytes.clone()).unwrap();
    assert!(parsed.root().is_some());
    h.state_mut().close_tab(0);
    h.state_mut().open_bytes("reopened.pdf", None, bytes.as_ref().clone()).unwrap();
    h.run_steps(3);
    assert_eq!(outline(&h), expected);
    assert!(h.state().views[0].bookmarks.selected.is_none());
}

#[test]
fn dragging_near_the_edge_scrolls_and_reorders_a_large_synthetic_tree() {
    let mut h = harness();
    for root in 0..40 {
        h.state_mut().apply_edit(pdfcraft_engine::Edit::AddBookmark {
            parent: vec![],
            index: root,
            title: format!("Root {root:03}"),
            page: root % 3,
        });
        for child in 0..2 {
            h.state_mut().apply_edit(pdfcraft_engine::Edit::AddBookmark {
                parent: vec![root],
                index: child,
                title: format!("Child {root:03}/{child}"),
                page: child,
            });
        }
    }
    h.run_steps(3);
    let before = outline(&h);
    assert_eq!(before.len(), 120);
    let from = row(&h, "Root 000").center();
    // Find the final fully visible row and hold near the bottom of the scroll viewport.
    let to = egui::pos2(from.x, 875.0);
    start_drag(&mut h, from, to);
    let frames_started = std::time::Instant::now();
    h.run_steps(80);
    eprintln!("120-bookmark held drag: 80 frames in {:?}", frames_started.elapsed());
    assert_eq!(outline(&h), before);
    assert!(row(&h, "Root 000").bottom() < 100.0, "the first row scrolls out of view");
    // Settle egui's animated scroll away from the edge before choosing a precise gap.
    h.hover_at(egui::pos2(from.x, 400.0));
    h.run_steps(40);
    let target = (1..40)
        .rev()
        .find_map(|i| {
            let rect = row(&h, &format!("Root {i:03}"));
            (rect.top() > 150.0 && rect.bottom() < 830.0).then_some((i, rect))
        })
        .expect("a distant root is visible after scrolling");
    assert!(target.0 > 8);
    let drop = egui::pos2(target.1.center().x, target.1.bottom() - 2.0);
    h.drop_at(drop);
    h.run_steps(3);
    let expected: Vec<_> = (1..=target.0).chain(std::iter::once(0)).chain(target.0 + 1..40).map(|i| format!("Root {i:03}")).collect();
    assert_eq!(roots(&h), expected);
    h.state_mut().undo();
    h.run_steps(2);
    assert_eq!(outline(&h), before);
    h.state_mut().redo();
    h.run_steps(2);
    assert_eq!(roots(&h), expected);
}

#[test]
fn long_multiline_bookmark_rows_accept_before_after_and_inside_drops() {
    for (fraction, selected) in [(0.05, vec![1]), (0.5, vec![1, 1]), (0.95, vec![2])] {
        let mut h = tree();
        let title = "Long bookmark title with repeated sections — ".repeat(8).trim().to_string();
        h.state_mut().apply_edit(pdfcraft_engine::Edit::RenameBookmark { path: vec![2], title: title.clone() });
        h.run_steps(3);
        let source = row(&h, "A").center();
        let destination = row(&h, &title);
        assert!(destination.height() > 56.0);
        drag(&mut h, source, egui::pos2(destination.center().x, destination.top() + destination.height() * fraction));
        assert_eq!(h.state().views[0].bookmarks.selected, Some(selected));
        assert!(outline(&h).contains(&(title, Some(2), 0)));
    }
}

#[test]
fn expansion_state_follows_a_moved_parent_instead_of_its_old_path() {
    let mut h = tree();
    // The collapse-all operation is UI state only; move the closed B subtree before A.
    h.get_by_label("Bookmark options").click();
    h.run_steps(2);
    h.get_by_label("Collapse all bookmarks").click();
    h.run_steps(2);
    let from = row(&h, "B").center();
    let r = row(&h, "A");
    drag(&mut h, from, egui::pos2(r.center().x, r.top() + 2.0));
    assert_eq!(roots(&h), ["B", "A", "C", "D"]);
    assert!(h.query_by_label("B0").is_none());
}

#[test]
fn baseline_selected_root_inserts_after_selection() {
    let mut h = tree();
    h.get_by_label("A").click();
    h.run_steps(2);
    add(&mut h, "Inserted");
    assert_eq!(roots(&h), ["A", "Inserted", "B", "C", "D"]);
}

#[test]
fn real_drag_handles_nested_sibling_gaps_and_different_parents() {
    for (source, target, fraction, parent, expected) in [
        ("B1", "B0", 0.05, vec![1], vec!["B1", "B0", "Duplicate"]),
        ("B0", "B1", 0.95, vec![1], vec!["B1", "B0", "Duplicate"]),
        ("B1", "C", 0.5, vec![2], vec!["Duplicate", "B1"]),
        ("B0", "A", 0.5, vec![0], vec!["B0"]),
    ] {
        let mut h = tree();
        let from = row(&h, source).center();
        let r = row(&h, target);
        drag(&mut h, from, egui::pos2(r.center().x, r.top() + r.height() * fraction));
        let info = &h.state().session.get(h.state().views[0].id).unwrap().info;
        let mut items = &info.outline;
        for i in parent {
            items = &items[i].children;
        }
        assert_eq!(items.iter().map(|i| i.title.as_str()).collect::<Vec<_>>(), expected);
        assert!(outline(&h).iter().any(|(title, page, _)| title == "Grandchild" && *page == Some(1)));
    }
}

#[test]
fn filtering_disables_reordering_before_a_drag_begins() {
    let mut h = tree();
    h.get_by_label("Search").click();
    h.run_steps(1);
    h.get_by_label("Search").type_text("B");
    h.run_steps(2);
    let before = outline(&h);
    let from = row(&h, "B1").center();
    let to = row(&h, "B0").center();
    drag(&mut h, from, to);
    assert_eq!(outline(&h), before);
}

#[test]
fn a_truncated_outline_disables_reordering_without_using_incomplete_child_counts() {
    let mut pdf = String::from_utf8(PAGES.to_vec()).unwrap().replace("/Pages 2 0 R >>", "/Pages 2 0 R /Outlines 6 0 R >>");
    pdf.push_str("\n6 0 obj << /Type /Outlines /First 7 0 R /Last 41 0 R /Count 35 >> endobj\n");
    for level in 0..34 {
        let object = level + 7;
        let parent = if level == 0 { 6 } else { object - 1 };
        let child = if level < 33 { format!("/First {} 0 R /Last {} 0 R /Count {}", object + 1, object + 1, 33 - level) } else { String::new() };
        let next = if level == 0 { "/Next 41 0 R" } else { "" };
        pdf.push_str(&format!("{object} 0 obj << /Title (Level {level}) /Parent {parent} 0 R /Dest [3 0 R /Fit] {child} {next} >> endobj\n"));
    }
    pdf.push_str("41 0 obj << /Title (Other root) /Parent 6 0 R /Prev 7 0 R /Dest [5 0 R /Fit] >> endobj\n");
    let mut h = harness();
    h.state_mut().close_tab(0);
    h.state_mut().open_bytes("deep.pdf", None, pdf.into_bytes()).unwrap();
    h.run_steps(3);
    assert!(h.state().session.get(h.state().views[0].id).unwrap().info.outline_more);
    // Collapse makes both root rows visible. This would be a valid drop in a complete tree.
    h.get_by_label("Bookmark options").click();
    h.run_steps(2);
    h.get_by_label("Collapse all bookmarks").click();
    h.run_steps(2);
    let before = outline(&h);
    let from = row(&h, "Other root").center();
    let to = row(&h, "Level 0").center();
    drag(&mut h, from, to);
    assert_eq!(outline(&h), before);
}
