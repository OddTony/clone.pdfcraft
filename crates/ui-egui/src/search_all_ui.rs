//! Application-level Advanced Search scope, results and cross-tab navigation.
use crate::{
    PdfCraftApp, RightPanel,
    canvas::Find,
    theme::{self, Tokens},
};
use pdfcraft_engine::search::{Hit, Options, Scan, Snapshot};

#[derive(Default)]
pub(crate) struct State {
    pub all: bool,
    pub options: Options,
    pub scan: Scan,
    selected: Option<Hit>,
    focus: bool,
}

impl State {
    pub(crate) fn request_focus(&mut self) {
        self.focus = true;
    }
}

pub(crate) fn tick(app: &mut PdfCraftApp, ctx: &egui::Context) {
    let state = &mut app.global_search;
    if !state.all || app.right != Some(RightPanel::Search) {
        if !state.scan.documents.is_empty() {
            state.scan.reset(Vec::new(), Options::default());
        }
        state.selected = None;
        return;
    }
    let documents: Vec<_> = app.views.iter().filter_map(|v| app.session.get(v.id)).map(Snapshot::new).collect();
    if !state.scan.unchanged(&documents, &state.options) {
        state.scan.reset(documents, state.options.clone());
        state.selected = None;
    }
    state.scan.poll(None);
    if !state.scan.finished {
        ctx.request_repaint_after(std::time::Duration::from_millis(30));
    }
    // Wait for the normal canvas worker to extract the destination's text. Then reuse the
    // exact existing find navigation, including its scroll position and selected highlight.
    if let Some(hit) = &state.selected {
        let current = state.scan.documents.iter().find(|d| d.id == hit.doc).zip(app.session.get(hit.doc)).is_some_and(|(s, d)| s.current(d));
        let Some(view) = app.views.iter_mut().find(|v| v.id == hit.doc) else {
            state.selected = None;
            return;
        };
        if !current
            || !view.find.as_ref().is_some_and(|f| {
                f.query == state.options.query && f.case_sensitive == state.options.case_sensitive && f.whole_words == state.options.whole_words
            })
        {
            state.selected = None;
        } else if view.texts.contains_key(&hit.page) {
            view.rerun_find();
            if let Some(i) = view.find.as_ref().and_then(|f| {
                f.matches
                    .iter()
                    .enumerate()
                    .filter(|(_, (p, _))| *p == hit.page)
                    .nth(hit.page_match)
                    .filter(|(_, (_, range))| *range == hit.range)
                    .map(|(i, _)| i)
            }) {
                view.go_to_match(i);
            }
            state.selected = None;
        } else if view.text_failed.contains(&hit.page) {
            state.selected = None;
        }
    }
}

fn navigate(app: &mut PdfCraftApp, hit: Hit) {
    let state = &mut app.global_search;
    if state.options != state.scan.options
        || !state.scan.results.iter().any(|current| {
            current.doc == hit.doc
                && current.generation == hit.generation
                && current.page == hit.page
                && current.range == hit.range
                && current.page_match == hit.page_match
        })
    {
        return;
    }
    let Some(snapshot) = state.scan.documents.iter().find(|d| d.id == hit.doc && d.generation == hit.generation) else {
        return;
    };
    if !app.session.get(hit.doc).is_some_and(|d| snapshot.current(d)) {
        return;
    }
    let Some(index) = app.views.iter().position(|v| v.id == hit.doc) else {
        return;
    };
    let Some(view) = app.views.get_mut(index) else {
        return;
    };
    // Selecting a result explicitly applies the global query to the destination's Find;
    // typing a global query alone never changes any tab's Find state.
    view.find = Some(Find {
        query: state.options.query.clone(),
        case_query: state.options.query.clone(),
        case_sensitive: state.options.case_sensitive,
        whole_words: state.options.whole_words,
        matches: vec![(hit.page, hit.range.clone())],
        current: Some(0),
        in_panel: true,
        ..Default::default()
    });
    // A page cached by a prior revision must not supply geometry for this result.
    view.retry_text(hit.page);
    view.go_to_match(0);
    app.active = Some(index);
    state.selected = Some(hit);
}

pub(crate) fn panel(app: &mut PdfCraftApp, ui: &mut egui::Ui, t: &Tokens) {
    let mut close = false;
    let mut selected = None;
    egui::Frame::NONE
        .fill(t.panel)
        .inner_margin(egui::Margin { left: 14, right: 12, top: 12, bottom: 8 })
        .stroke(egui::Stroke::new(1.0, t.divider))
        .show(ui, |ui| {
            close = crate::panels::panel_header(ui, t, "Search", false).1;
            ui.push_id("advanced-search-scope", |ui| {
                if ui.selectable_label(!app.global_search.all, tl!("Current document")).clicked() {
                    app.global_search.all = false;
                }
                if ui.selectable_label(app.global_search.all, tl!("All open documents")).clicked() {
                    app.global_search.all = true;
                    app.global_search.focus = true;
                }
            });
            ui.add_space(6.0);
            if !app.global_search.all {
                if let Some((index, id)) = app.active_ids() {
                    let pages = app.session.get(id).map_or(0, |d| d.info.pages.len());
                    if let Some(view) = app.views.get_mut(index) {
                        view.find.get_or_insert_with(Find::default);
                        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| crate::search_ui::panel(ui, t, view, pages));
                    }
                }
                return;
            }
            let state = &mut app.global_search;
            crate::widgets::begin_dialog(ui);
            let label = ui.label(egui::RichText::new(tl!("What word or phrase would you like to search for?")).color(t.text_muted));
            let input = ui
                .add(
                    crate::widgets::line(&mut state.options.query)
                        .id(egui::Id::new("search-all-input"))
                        .hint_text(tl!("Search"))
                        .char_limit(1024)
                        .desired_width(f32::INFINITY),
                )
                .labelled_by(label.id);
            if state.focus {
                input.request_focus();
                state.focus = false;
            }
            ui.add(crate::widgets::checkbox(&mut state.options.whole_words, tl!("Whole words only")));
            ui.add(crate::widgets::checkbox(&mut state.options.case_sensitive, tl!("Case-sensitive")));
            ui.label(egui::RichText::new(tl!("Open tabs and edits update the search automatically.")).small().color(t.text_faint));
            ui.label(egui::RichText::new(tl!("Selecting a result applies this query to the destination’s Find.")).small().color(t.text_faint));
            // Hide results immediately on query/option changes, even before next frame's reset.
            if state.options != state.scan.options {
                return;
            }
            ui.horizontal(|ui| {
                if !state.scan.finished && ui.button(tl!("Cancel")).clicked() {
                    state.scan.cancel();
                }
                if state.scan.cancelled && ui.button(tl!("Search")).clicked() {
                    state.scan.reset(state.scan.documents.clone(), state.options.clone());
                }
            });
            let scan = &state.scan;
            if state.options.query.trim().is_empty() {
                return;
            }
            let mut documents = std::collections::HashSet::new();
            for hit in &scan.results {
                documents.insert(hit.doc);
            }
            let count = crate::i18n::fmt(
                tl!("Results shown: {n} · Documents: {d}"),
                &[("n", &scan.results.len().to_string()), ("d", &documents.len().to_string())],
            );
            ui.label(egui::RichText::new(count).font(theme::semibold(12.5)));
            if !scan.finished {
                ui.label(crate::i18n::fmt(
                    tl!("Searching… {s} of {p} pages"),
                    &[("s", &scan.searched.to_string()), ("p", &scan.total_pages().to_string())],
                ));
            } else if scan.cancelled {
                ui.label(tl!("Cancelled"));
            } else if scan.truncated {
                ui.label(tl!("Showing the first 2,000 results — more matches may exist."));
            } else if scan.warning_count == 0 && scan.results.is_empty() {
                ui.label(tl!("No matches."));
            } else if scan.warning_count == 0 {
                ui.label(tl!("Search complete."));
            }
            if scan.warning_count != 0 {
                ui.label(crate::i18n::fmt(
                    tl!("{n} pages could not be searched; results are incomplete."),
                    &[("n", &scan.warning_count.to_string())],
                ));
            }
            // Fixed-height virtual rows bound egui widgets even at the retention limit. Include a
            // document heading at group boundaries and the first visible row, so identity stays visible while scrolling.
            egui::ScrollArea::vertical().auto_shrink([false, false]).show_rows(ui, 100.0, scan.results.len(), |ui, rows| {
                let first_visible = rows.start;
                for i in rows {
                    let Some(hit) = scan.results.get(i) else {
                        continue;
                    };
                    let Some(doc) = scan.documents.iter().find(|d| d.id == hit.doc) else {
                        continue;
                    };
                    ui.push_id(
                        (
                            "search-all-result",
                            hit.doc.0,
                            hit.generation,
                            hit.page,
                            hit.range.start,
                            i,
                            &scan.options.query,
                            scan.options.case_sensitive,
                            scan.options.whole_words,
                        ),
                        |ui| {
                            ui.spacing_mut().item_spacing.y = 4.0;
                            ui.set_min_height(100.0);
                            let heading =
                                if i == first_visible || scan.results.get(i.saturating_sub(1)).is_none_or(|previous| previous.doc != hit.doc) {
                                    doc.name.as_str()
                                } else {
                                    ""
                                };
                            ui.add_sized(
                                [ui.available_width(), 17.0],
                                egui::Label::new(egui::RichText::new(heading).font(theme::semibold(12.5))).halign(egui::Align::Min).truncate(),
                            );
                            ui.label(
                                egui::RichText::new(crate::i18n::fmt(tl!("Page {label}"), &[("label", &hit.page.saturating_add(1).to_string())]))
                                    .small()
                                    .color(t.text_faint),
                            );
                            let mut job = crate::search_ui::snippet(t, &hit.before, &hit.text, &hit.after, ui.available_width());
                            job.wrap.max_rows = 2;
                            let response = ui
                                .add_sized([ui.available_width(), 44.0], egui::Button::selectable(false, job).wrap())
                                .on_hover_text(tl!("Go to this result"));
                            if response.clicked() {
                                selected = Some(hit.clone());
                            }
                        },
                    );
                }
            });
        });
    if close {
        app.choose_right_panel(None);
    }
    if let Some(hit) = selected {
        navigate(app, hit);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn app() -> PdfCraftApp {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        for text in ["invoice contract", "invoice", "absent"] {
            let bytes = app.session.create_from_text("synthetic", text).unwrap();
            app.open_bytes("same.pdf", None, bytes.to_vec()).unwrap();
        }
        app.right = Some(RightPanel::Search);
        app.global_search.all = true;
        app.global_search.options.query = "invoice".into();
        app
    }
    fn finish(app: &mut PdfCraftApp) {
        let ctx = egui::Context::default();
        for _ in 0..10_000 {
            tick(app, &ctx);
            if app.global_search.scan.finished {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        panic!("search did not complete");
    }
    #[test]
    fn query_changes_and_new_tabs_restart_without_changing_per_document_find() {
        let mut app = app();
        app.views[0].open_find();
        app.views[0].find.as_mut().unwrap().query = "previous".into();
        tick(&mut app, &egui::Context::default());
        app.global_search.options.query = "contract".into();
        finish(&mut app);
        assert_eq!(app.global_search.scan.results.len(), 1);
        assert_eq!(app.global_search.scan.results[0].text, "contract");
        assert_eq!(app.views[0].find.as_ref().unwrap().query, "previous");
        let bytes = app.session.create_from_text("new", "contract").unwrap();
        app.open_bytes("same.pdf", None, bytes.to_vec()).unwrap();
        finish(&mut app);
        assert_eq!(app.global_search.scan.results.len(), 2);
        app.global_search.options.case_sensitive = true;
        app.global_search.options.query = "CONTRACT".into();
        finish(&mut app);
        assert!(app.global_search.scan.results.is_empty());
        app.global_search.options.case_sensitive = false;
        finish(&mut app);
        assert_eq!(app.global_search.scan.results.len(), 2);
        app.global_search.all = false;
        tick(&mut app, &egui::Context::default());
        assert_eq!(app.views[0].find.as_ref().unwrap().query, "previous");
    }
    #[test]
    fn a_result_from_an_old_query_cannot_navigate_after_new_results_arrive() {
        let mut app = app();
        finish(&mut app);
        let old = app.global_search.scan.results[0].clone();
        app.global_search.options.query = "contract".into();
        finish(&mut app);
        let active = app.active;
        navigate(&mut app, old);
        assert_eq!(app.active, active);
    }
    #[test]
    fn closed_or_edited_results_cannot_navigate_to_another_tab_or_revision() {
        let mut app = app();
        finish(&mut app);
        let hit = app.global_search.scan.results[0].clone();
        let old = hit.doc;
        app.close_tab(0);
        let bytes = app.session.create_from_text("new", "absent").unwrap();
        app.open_bytes("same.pdf", None, bytes.to_vec()).unwrap();
        let active = app.active;
        navigate(&mut app, hit);
        assert_eq!(app.active, active);
        finish(&mut app);
        assert!(app.global_search.scan.results.iter().all(|h| h.doc != old));
        let hit = app.global_search.scan.results[0].clone();
        app.active = app.views.iter().position(|v| v.id == hit.doc);
        assert!(app.apply_edit(pdfcraft_engine::Edit::AddText {
            page: 0,
            text: pdfcraft_engine::AddedText { rect: [20., 300., 400., 400.], text: "new invoice".into(), ..Default::default() }
        }));
        let edited = hit.doc;
        let before = app.session.get(edited).unwrap().bytes.clone();
        app.active = app.views.iter().position(|v| v.id != edited);
        let active = app.active;
        navigate(&mut app, hit);
        assert_eq!(app.active, active);
        assert!(std::sync::Arc::ptr_eq(&before, &app.session.get(edited).unwrap().bytes));
        assert!(app.session.get(edited).unwrap().dirty);
        finish(&mut app);
        assert_eq!(app.global_search.scan.results.len(), 2);
    }
}
