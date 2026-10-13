//! Reordering document views inside one window. Session documents stay in the engine's own
//! ordering; this is presentation state, so moving a tab never closes or reopens a document.

use pdfcraft_engine::DocId;

impl crate::PdfCraftApp {
    /// Move the tab at `from` to its final index `to`.
    ///
    /// Returns false for an invalid index or when the tab is already there. The active document
    /// is tracked by id while the view vector moves, and the Home selection (`None`) is preserved.
    pub fn move_tab(&mut self, from: usize, to: usize) -> bool {
        if from >= self.views.len() || to >= self.views.len() || from == to {
            return false;
        }

        let active_id = self.active.and_then(|index| self.views.get(index)).map(|view| view.id);
        let view = self.views.remove(from);
        self.views.insert(to, view);

        self.active = active_id.and_then(|id| self.views.iter().position(|candidate| candidate.id == id));
        true
    }

    /// Move the view with `id` by one tab slot. `delta` is -1 for left and +1 for right.
    pub fn move_tab_by_id(&mut self, id: DocId, delta: isize) -> bool {
        if !matches!(delta, -1 | 1) {
            return false;
        }
        let Some(from) = self.views.iter().position(|view| view.id == id) else { return false };
        let Some(to) = from.checked_add_signed(delta) else { return false };
        self.move_tab(from, to)
    }
}

#[cfg(test)]
mod tests {

    fn fixture() -> Vec<u8> {
        fixture_pages(1)
    }

    fn fixture_pages(page_count: usize) -> Vec<u8> {
        // Minimal synthetic PDF with a complete xref table; distinct names are held in
        // the same Session so successful moves must preserve each original DocId.
        let kids = (0..page_count).map(|index| format!("{} 0 R", index + 3)).collect::<Vec<_>>().join(" ");
        let mut objects = vec!["<< /Type /Catalog /Pages 2 0 R >>".to_string(), format!("<< /Type /Pages /Kids [{kids}] /Count {page_count} >>")];
        for _ in 0..page_count {
            objects.push("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 100 100] >>".to_string());
        }
        let mut out = b"%PDF-1.4\n".to_vec();
        let mut offsets = Vec::new();
        for (index, object) in objects.iter().enumerate() {
            offsets.push(out.len());
            out.extend_from_slice(format!("{} 0 obj\n{object}\nendobj\n", index + 1).as_bytes());
        }
        let xref = out.len();
        out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes());
        for offset in offsets {
            out.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
        }
        out.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objects.len() + 1).as_bytes());
        out
    }

    #[test]
    fn reordering_preserves_active_document_and_session_edit_state() {
        let mut app = crate::PdfCraftApp::new();
        app.open_bytes("first.pdf", None, fixture()).unwrap();
        let first = app.views[0].id;
        app.open_bytes("second.pdf", None, fixture()).unwrap();
        let second = app.views[1].id;
        app.open_bytes("third.pdf", None, fixture()).unwrap();
        let third = app.views[2].id;

        app.session.apply(first, pdfcraft_engine::Edit::InsertBlankPage { at: 0, width: 100.0, height: 100.0 }).unwrap();
        app.active = Some(1);
        let can_undo = app.session.get(first).and_then(|doc| doc.can_undo()).map(str::to_owned);
        assert!(app.session.get(first).is_some_and(|doc| doc.dirty));

        assert!(app.move_tab(0, 2));
        assert_eq!(app.views.iter().map(|view| view.id).collect::<Vec<_>>(), [second, third, first]);
        assert_eq!(app.active, Some(0), "the active document remains second.pdf");
        let moved = app.session.get(first).expect("the original session document remains open");
        assert!(moved.dirty);
        assert_eq!(moved.can_undo(), can_undo.as_deref());
        assert_eq!(moved.info.pages.len(), 2);
    }

    #[test]
    fn invalid_moves_do_not_change_order_or_home_selection() {
        let mut app = crate::PdfCraftApp::new();
        app.open_bytes("first.pdf", None, fixture()).unwrap();
        let first = app.views[0].id;
        app.open_bytes("second.pdf", None, fixture()).unwrap();
        let second = app.views[1].id;
        app.active = None;

        assert!(!app.move_tab(2, 0));
        assert!(!app.move_tab(0, 2));
        assert!(!app.move_tab(0, 0));
        assert!(!app.move_tab_by_id(first, -1));
        assert!(!app.move_tab_by_id(second, 0));
        assert_eq!(app.views.iter().map(|view| view.id).collect::<Vec<_>>(), [first, second]);
        assert_eq!(app.active, None);

        assert!(app.move_tab_by_id(first, 1));
        assert_eq!(app.views.iter().map(|view| view.id).collect::<Vec<_>>(), [second, first]);
        assert_eq!(app.active, None);
    }

    #[test]
    fn every_move_pair_matches_final_index_semantics_across_many_tabs() {
        let mut app = crate::PdfCraftApp::new();
        for index in 0..8 {
            app.open_bytes(&format!("document-{index}.pdf"), None, fixture()).unwrap();
        }
        let original = app.views.iter().map(|view| view.id).collect::<Vec<_>>();

        for active_index in [0, original.len() - 1, original.len() / 2] {
            let active_id = original[active_index];
            app.active = Some(active_index);
            for from in 0..original.len() {
                for to in 0..original.len() {
                    let mut expected = original.clone();
                    let moved = expected.remove(from);
                    expected.insert(to, moved);

                    assert_eq!(app.move_tab(from, to), from != to);
                    assert_eq!(app.views.iter().map(|view| view.id).collect::<Vec<_>>(), expected, "move {from} -> {to}");
                    assert_eq!(app.active_ids().map(|(_, id)| id), Some(active_id));
                    assert_eq!(app.active, expected.iter().position(|id| *id == active_id));

                    if from != to {
                        assert!(app.move_tab(to, from), "reverse move restores original order");
                    }
                    assert_eq!(app.views.iter().map(|view| view.id).collect::<Vec<_>>(), original);
                }
            }
        }
    }

    #[test]
    fn move_preserves_view_state_pdf_bytes_and_session_history() {
        let mut app = crate::PdfCraftApp::new();
        for name in ["first.pdf", "second.pdf", "third.pdf"] {
            app.open_bytes(name, Some(format!("/tmp/{name}")), fixture_pages(4)).unwrap();
        }
        let ids = app.views.iter().map(|view| view.id).collect::<Vec<_>>();
        app.closed_tabs = vec!["/tmp/closed.pdf".into()];

        let moving_id = ids[0];
        let view = app.views.get_mut(0).unwrap();
        view.current = 2;
        view.zoom = 1.75;
        view.fit = crate::canvas::Fit::None;
        view.layout = crate::canvas::PageLayout::TwoUp;
        view.continuous = false;
        view.rotation = 90;
        view.selected = [1, 3].into_iter().collect();
        view.back = vec![0, 1];
        view.forward = vec![3];
        view.organize = true;
        let find = view.find.get_or_insert_with(crate::canvas::Find::default);
        find.query = "search query".into();
        find.case_sensitive = true;
        find.whole_words = true;
        app.props_draft = Some((moving_id, ["Title draft".into(), "Author".into(), "Subject".into(), "Keywords".into()]));
        app.view_draft = Some((moving_id, pdfcraft_engine::InitialView::default()));

        app.session.apply(moving_id, pdfcraft_engine::Edit::InsertBlankPage { at: 0, width: 100.0, height: 100.0 }).unwrap();
        assert!(app.session.get(moving_id).is_some_and(|doc| doc.dirty));
        let before_closed = app.closed_tabs.clone();
        let before_doc_count = ids.iter().filter(|id| app.session.get(**id).is_some()).count();
        let bytes = ids.iter().map(|id| app.session.get(*id).map(|doc| std::sync::Arc::clone(&doc.bytes)).unwrap()).collect::<Vec<_>>();
        let edited_bytes = std::sync::Arc::clone(&app.session.get(moving_id).unwrap().bytes);
        app.active = Some(2);
        assert!(app.move_tab(0, 2));
        assert_eq!(app.active_ids().map(|(_, id)| id), Some(ids[2]));
        let moved_index = app.views.iter().position(|view| view.id == moving_id).unwrap();
        let moved_view = &app.views[moved_index];
        assert_eq!(moved_view.current, 2);
        assert_eq!(moved_view.zoom, 1.75);
        assert_eq!(moved_view.fit, crate::canvas::Fit::None);
        assert_eq!(moved_view.layout, crate::canvas::PageLayout::TwoUp);
        assert!(!moved_view.continuous);
        assert_eq!(moved_view.rotation, 90);
        assert_eq!(moved_view.selected, [1, 3].into_iter().collect());
        assert_eq!(moved_view.back, [0, 1]);
        assert_eq!(moved_view.forward, [3]);
        assert!(moved_view.organize);
        let find = moved_view.find.as_ref().unwrap();
        assert_eq!(find.query, "search query");
        assert!(find.case_sensitive && find.whole_words);
        assert_eq!(app.props_draft.as_ref().map(|(id, draft)| (*id, draft[0].as_str())), Some((moving_id, "Title draft")));
        assert_eq!(app.view_draft.as_ref().map(|(id, _)| *id), Some(moving_id));
        assert_eq!(app.closed_tabs, before_closed);
        assert_eq!(ids.iter().filter(|id| app.session.get(**id).is_some()).count(), before_doc_count);
        for (id, original_bytes) in ids.iter().zip(&bytes) {
            let current = &app.session.get(*id).unwrap().bytes;
            assert!(std::sync::Arc::ptr_eq(original_bytes, current), "working PDF bytes remain the same Arc for {id:?}");
        }
        assert!(std::sync::Arc::ptr_eq(&edited_bytes, &app.session.get(moving_id).unwrap().bytes));

        assert_eq!(app.session.get(moving_id).unwrap().info.pages.len(), 5);
        app.session.undo(moving_id).unwrap();
        assert_eq!(app.session.get(moving_id).unwrap().info.pages.len(), 4);
        app.session.redo(moving_id).unwrap();
        assert_eq!(app.session.get(moving_id).unwrap().info.pages.len(), 5);
        assert!(app.session.get(moving_id).is_some_and(|doc| doc.dirty));
    }

    #[test]
    fn opt_in_last_session_serialization_uses_reordered_tabs_and_active_id() {
        let mut app = crate::PdfCraftApp::new();
        for name in ["first.pdf", "second.pdf", "third.pdf"] {
            app.open_bytes(name, Some(format!("/tmp/{name}")), fixture_pages(3)).unwrap();
        }
        app.views[0].current = 1;
        app.views[1].current = 2;
        app.views[2].current = 0;
        app.active = Some(0);

        let disabled = serde_json::from_str::<serde_json::Value>(&app.persist()).unwrap();
        assert!(disabled["last_session"].is_null());

        assert!(app.move_tab(0, 2));
        app.reopen_last_session = true;
        let persisted = serde_json::from_str::<serde_json::Value>(&app.persist()).unwrap();
        let files = persisted["last_session"]["files"].as_array().unwrap();
        let paths = files.iter().map(|entry| entry["path"].as_str().unwrap()).collect::<Vec<_>>();
        assert_eq!(paths, ["/tmp/second.pdf", "/tmp/third.pdf", "/tmp/first.pdf"]);
        assert_eq!(files.iter().map(|entry| entry["page"].as_u64().unwrap()).collect::<Vec<_>>(), [2, 0, 1]);
        assert_eq!(persisted["last_session"]["active"], "/tmp/first.pdf");
    }

    #[test]
    fn directional_commands_are_noops_at_both_strip_edges() {
        let mut app = crate::PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.open_bytes("first.pdf", None, fixture()).unwrap();
        app.open_bytes("second.pdf", None, fixture()).unwrap();
        let order = app.views.iter().map(|view| view.id).collect::<Vec<_>>();

        app.active = Some(0);
        assert!(!app.execute("tab.move_left"));
        assert_eq!(app.toast.as_ref().map(|toast| toast.0.as_str()), Some("This tab is already first"));
        app.active = Some(1);
        assert!(!app.execute("tab.move_right"));
        assert_eq!(app.toast.as_ref().map(|toast| toast.0.as_str()), Some("This tab is already last"));
        assert_eq!(app.views.iter().map(|view| view.id).collect::<Vec<_>>(), order);
    }
}
