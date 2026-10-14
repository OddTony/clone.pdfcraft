//! Issue #295: quick clicks on text widen the selection, as in Acrobat: two clicks select a
//! word, three the line, four all the text on the page.

use egui::{Event, Modifiers, MouseWheelUnit, PointerButton, Pos2, TouchPhase};
use egui_kittest::Harness;
use pdfcraft_ui_egui::PdfCraftApp;

/// One 300×200 page with two lines of Helvetica text.
fn two_lines() -> Vec<u8> {
    let content = "BT /F1 14 Tf 20 150 Td (The quick brown fox) Tj 0 -24 Td (jumps over the dog) Tj ET";
    format!(
        "%PDF-1.7
1 0 obj << /Type /Catalog /Pages 2 0 R >> endobj
2 0 obj << /Type /Pages /Kids [3 0 R] /Count 1 >> endobj
3 0 obj << /Type /Page /Parent 2 0 R /MediaBox [0 0 300 200] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >> endobj
4 0 obj << /Length {} >> stream
{content}
endstream endobj
5 0 obj << /Type /Font /Subtype /Type1 /BaseFont /Helvetica >> endobj
trailer << /Root 1 0 R >>
%%EOF",
        content.len()
    )
    .into_bytes()
}

/// The page on screen and the pointer over the "u" of "quick". Frames are 1/60 s: a click takes
/// two frames (press, release), and egui counts a double-click only within 0.3 s.
fn harness(setup: impl FnOnce(&mut PdfCraftApp) + 'static) -> (Harness<'static, PdfCraftApp>, Pos2) {
    harness_with(two_lines(), setup)
}

/// `harness` with another document; the pointer ends over glyph 5 of its first page.
fn harness_with(pdf: Vec<u8>, setup: impl FnOnce(&mut PdfCraftApp) + 'static) -> (Harness<'static, PdfCraftApp>, Pos2) {
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).with_step_dt(1.0 / 60.0).build_eframe(move |_cc| {
        let mut app = PdfCraftApp::new();
        app.set_option("language", "en").unwrap();
        app.open_bytes("lines.pdf", None, pdf).expect("opens");
        app.set_option("left", "closed").unwrap();
        app.set_option("author", "Tester").unwrap();
        setup(&mut app);
        app
    });
    for _ in 0..200 {
        h.run_steps(2);
        if !h.state().render_pending() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    let at = h.state().views[0].glyph_screen_pos(0, 5).expect("text layer loaded");
    h.hover_at(at);
    h.run_steps(1);
    (h, at)
}

/// Press and release without moving or leaving the page.
fn click(h: &mut Harness<'static, PdfCraftApp>, pos: Pos2) {
    for pressed in [true, false] {
        h.event(Event::PointerButton { pos, button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE });
    }
    h.run_steps(1);
}

#[test]
fn quick_clicks_select_a_word_then_the_line_then_the_page() {
    let (mut h, at) = harness(|_| {});
    let page = "The quick brown fox\njumps over the dog";
    for (n, want) in [None, Some("quick"), Some("The quick brown fox"), Some(page), Some(page)].into_iter().enumerate() {
        click(&mut h, at);
        assert_eq!(h.state().views[0].selected_text().as_deref(), want, "after {} clicks", n + 1);
    }
    // A pause ends the run: the next double-click selects a word again.
    h.run_steps(60);
    click(&mut h, at);
    click(&mut h, at);
    assert_eq!(h.state().views[0].selected_text().as_deref(), Some("quick"));
}

#[test]
fn quick_clicks_with_the_highlighter_mark_only_the_word() {
    let (mut h, at) = harness(|app| assert!(app.execute("comment.highlight")));
    for _ in 0..4 {
        click(&mut h, at);
        // The double-click's highlight re-renders the page; give its text layer up to 8 frames
        // to come back (still well inside egui's 0.3 s multi-click window at 1/60 s a frame), so
        // a loaded machine doesn't make the later clicks land on a page with no text yet.
        for _ in 0..8 {
            if !h.state().render_pending() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
            h.run_steps(1);
        }
    }
    h.run_steps(3);
    // Under load the mark can land a few frames later; wait for it (egui's clock is fixed per
    // frame, so waiting can't turn the clicks into a different run).
    for _ in 0..200 {
        let app = h.state();
        if app.session.get(app.views[0].id).is_some_and(|d| !d.info.annotations.is_empty()) && !app.render_pending() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
        h.run_steps(1);
    }
    let app = h.state();
    let doc = app.session.get(app.views[0].id).unwrap();
    let marks: Vec<(&str, usize)> = doc.info.annotations.iter().map(|a| (a.subtype.as_str(), a.quads.len())).collect();
    assert_eq!(marks, [("Highlight", 1)], "one highlight, on the double-clicked word");
    assert!(app.views[0].selected_text().is_none());
}

/// Press at `from`, move to `to` and release there.
fn drag(h: &mut Harness<'static, PdfCraftApp>, from: Pos2, to: Pos2) {
    h.event(Event::PointerButton { pos: from, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
    h.run_steps(1);
    for k in 1..=4 {
        h.hover_at(from + (to - from) * (k as f32 / 4.0));
        h.run_steps(1);
    }
    h.event(Event::PointerButton { pos: to, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
    h.run_steps(1);
}

/// Click at `pos` with Shift held.
fn shift_click(h: &mut Harness<'static, PdfCraftApp>, pos: Pos2) {
    h.event(Event::ModifiersChanged(Modifiers::SHIFT));
    h.hover_at(pos);
    h.run_steps(1);
    for pressed in [true, false] {
        h.event(Event::PointerButton { pos, button: PointerButton::Primary, pressed, modifiers: Modifiers::SHIFT });
    }
    h.run_steps(1);
    h.event(Event::ModifiersChanged(Modifiers::NONE));
    h.run_steps(1);
}

/// Issue #527: ⇧-click extends the text selection to the click, keeping its anchor.
#[test]
fn shift_click_extends_the_selection_from_its_anchor() {
    let (mut h, _) = harness(|_| {});
    let glyph = |h: &Harness<'static, PdfCraftApp>, g: usize| h.state().views[0].glyph_screen_pos(0, g).expect("glyph on screen");
    // Drag over "quick" (glyphs 4..=8).
    let (q, k) = (glyph(&h, 4), glyph(&h, 8));
    drag(&mut h, q, k);
    assert_eq!(h.state().views[0].selected_text().as_deref(), Some("quick"));
    h.run_steps(60);
    // ⇧-click on the "n" of "brown": the selection runs from the anchor to it.
    let n = glyph(&h, 14);
    shift_click(&mut h, n);
    assert_eq!(h.state().views[0].selected_text().as_deref(), Some("quick brown"));
    h.run_steps(60);
    // ⇧-click before the anchor: the anchor stays, the selection now runs back to the click.
    let t = glyph(&h, 0);
    shift_click(&mut h, t);
    assert_eq!(h.state().views[0].selected_text().as_deref(), Some("The q"));
    h.run_steps(60);
    // A plain click still clears it.
    click(&mut h, n);
    assert_eq!(h.state().views[0].selected_text(), None);
    h.run_steps(60);
    // ⇧-click with nothing selected selects nothing.
    shift_click(&mut h, n);
    assert_eq!(h.state().views[0].selected_text(), None);
}

/// Issue #1024: multi-click selection triggers immediately on pointer press, before release.
#[test]
fn multiclick_selects_on_pointer_press_before_release() {
    let (mut h, at) = harness(|_| {});
    // First click: press and release.
    click(&mut h, at);
    // Second click: press and HOLD (do not release).
    h.event(Event::PointerButton { pos: at, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
    h.run_steps(1);
    assert_eq!(h.state().views[0].selected_text().as_deref(), Some("quick"), "word highlights on press");
    // Release the second click: still selected.
    h.event(Event::PointerButton { pos: at, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
    h.run_steps(1);
    assert_eq!(h.state().views[0].selected_text().as_deref(), Some("quick"), "word stays selected on release");

    // Third click: press and HOLD.
    h.event(Event::PointerButton { pos: at, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
    h.run_steps(1);
    assert_eq!(h.state().views[0].selected_text().as_deref(), Some("The quick brown fox"), "line highlights on press");
    // Release third click.
    h.event(Event::PointerButton { pos: at, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
    h.run_steps(1);
    assert_eq!(h.state().views[0].selected_text().as_deref(), Some("The quick brown fox"), "line stays selected on release");
}

/// `n` pages of 300×200, each ending with one line of Helvetica text near its bottom edge.
fn bottom_lines(n: usize) -> Vec<u8> {
    let mut objects = String::new();
    let mut kids = Vec::new();
    for k in 0..n {
        let (page, contents) = (4 + 2 * k, 5 + 2 * k);
        kids.push(format!("{page} 0 R"));
        let content = format!("BT /F1 14 Tf 20 20 Td (Page {} ends with this line) Tj ET", k + 1);
        objects.push_str(&format!(
            "{page} 0 obj << /Type /Page /Parent 2 0 R /MediaBox [0 0 300 200] /Contents {contents} 0 R /Resources << /Font << /F1 3 0 R >> >> >> endobj\n\
             {contents} 0 obj << /Length {} >> stream\n{content}\nendstream endobj\n",
            content.len()
        ));
    }
    format!(
        "%PDF-1.7\n1 0 obj << /Type /Catalog /Pages 2 0 R >> endobj\n2 0 obj << /Type /Pages /Kids [{}] /Count {n} >> endobj\n\
         3 0 obj << /Type /Font /Subtype /Type1 /BaseFont /Helvetica >> endobj\n{objects}trailer << /Root 1 0 R >>\n%%EOF",
        kids.join(" ")
    )
    .into_bytes()
}

/// Press at `from` and move to `to`, keeping the button down.
fn press_and_move(h: &mut Harness<'static, PdfCraftApp>, from: Pos2, to: Pos2) {
    h.event(Event::PointerButton { pos: from, button: PointerButton::Primary, pressed: true, modifiers: Modifiers::NONE });
    h.run_steps(1);
    for k in 1..=4 {
        h.hover_at(from + (to - from) * (k as f32 / 4.0));
        h.run_steps(1);
    }
}

/// One wheel event, with time to scroll (egui spreads a wheel line over a few frames).
fn wheel(h: &mut Harness<'static, PdfCraftApp>, unit: MouseWheelUnit, dy: f32) {
    h.event(Event::MouseWheel { unit, delta: egui::vec2(0.0, dy), phase: TouchPhase::Move, modifiers: Modifiers::NONE });
    h.run_steps(10);
}

/// The top edge of `page` on screen.
fn page_top(h: &Harness<'static, PdfCraftApp>, page: usize) -> f32 {
    h.state().views[0].page_screen_rect(page).expect("page on screen").min.y
}

/// Issue #721: selecting near the bottom of a page with the button held, the wheel keeps
/// scrolling while the pointer crosses the gap onto the next page — and the selection follows
/// it there (#527, #2110).
#[test]
fn a_selection_drag_keeps_scrolling_over_the_gap_and_the_next_page() {
    let (mut h, _) = harness_with(bottom_lines(3), |app| {
        app.set_option("layout", "continuous").unwrap();
        app.set_option("zoom", "200").unwrap();
    });
    let glyph = |h: &Harness<'static, PdfCraftApp>, g: usize| h.state().views[0].glyph_screen_pos(0, g).expect("glyph on screen");
    // "ends" (glyphs 7..=10) in "Page 1 ends with this line".
    let (from, to) = (glyph(&h, 7), glyph(&h, 10));
    press_and_move(&mut h, from, to);
    assert_eq!(h.state().views[0].selected_text().as_deref(), Some("ends"));
    let (mut over_gap, mut over_next) = (false, false);
    for _ in 0..100 {
        let before = page_top(&h, 0);
        wheel(&mut h, MouseWheelUnit::Line, -0.1);
        assert!(page_top(&h, 0) < before - 0.5, "the wheel scrolls during the drag (over the gap yet: {over_gap})");
        let view = &h.state().views[0];
        let on = |page| view.page_screen_rect(page).is_some_and(|r| r.contains(to));
        over_gap |= !on(0) && !on(1);
        over_next = on(1);
        if over_next {
            break;
        }
    }
    assert!(over_gap && over_next, "the pointer crossed the gap ({over_gap}) onto the next page ({over_next})");
    // The head follows the pointer onto the next page, so the run spans both while the drag is
    // still held; it lands once that page's line is under the pointer.
    let mut held = h.state().views[0].selected_text().expect("still selected");
    for _ in 0..200 {
        if let Some((first, rest)) = held.split_once('\n')
            && first.starts_with("ends with this line")
            && rest.contains("Page 2")
        {
            break;
        }
        wheel(&mut h, MouseWheelUnit::Line, -0.1);
        held = h.state().views[0].selected_text().expect("still selected");
    }
    let (first, rest) = held.split_once('\n').expect("the selection spans both pages");
    assert!(first.starts_with("ends with this line"), "page 1 runs from the anchor to its end: {first:?}");
    assert!(rest.contains("Page 2"), "page 2 starts at the head: {rest:?}");
    h.event(Event::PointerButton { pos: to, button: PointerButton::Primary, pressed: false, modifiers: Modifiers::NONE });
    h.run_steps(2);
    assert_eq!(h.state().views[0].selected_text(), Some(held), "releasing keeps the selection");
}

/// Issue #527: a drag that walks off the bottom of a page and onto the next one selects both,
/// and Copy carries the pages in document order.
#[test]
fn drag_across_a_gap_selects_text_on_both_pages() {
    let (mut h, _) = harness_with(bottom_lines(2), |app| {
        app.set_option("layout", "continuous").unwrap();
        app.set_option("zoom", "100").unwrap();
    });
    let line = |k: usize| format!("Page {} ends with this line", k + 1);
    let glyph = |h: &Harness<'static, PdfCraftApp>, page: usize, g: usize| h.state().views[0].glyph_screen_pos(page, g).expect("glyph on screen");
    // Drag from the start of page 1's line to the "h" of "with" on page 2.
    let (from, to) = (glyph(&h, 0, 0), glyph(&h, 1, 12));
    drag(&mut h, from, to);
    let want1 = line(0);
    let want2: String = line(1).chars().take(13).collect();
    assert_eq!(h.state().views[0].selected_text().as_deref(), Some(format!("{want1}\n{want2}").as_str()), "both pages, in order");
    // A plain click releases it.
    h.run_steps(60);
    click(&mut h, to);
    assert_eq!(h.state().views[0].selected_text(), None);
}

/// Issue #527: ⇧-click lands the head on another page, keeping the anchor.
#[test]
fn shift_click_extends_the_selection_onto_another_page() {
    let (mut h, _) = harness_with(bottom_lines(2), |app| {
        app.set_option("layout", "continuous").unwrap();
        app.set_option("zoom", "100").unwrap();
    });
    let line = |k: usize| format!("Page {} ends with this line", k + 1);
    let glyph = |h: &Harness<'static, PdfCraftApp>, page: usize, g: usize| h.state().views[0].glyph_screen_pos(page, g).expect("glyph on screen");
    // Select page 1's line from its start.
    let (from, to) = (glyph(&h, 0, 0), glyph(&h, 0, 4));
    drag(&mut h, from, to);
    assert_eq!(h.state().views[0].selected_text().as_deref(), Some("Page "), "the line's start is selected");
    h.run_steps(60);
    // ⇧-click the "2" of page 2's line: the anchor stays put and the head jumps pages.
    let two = glyph(&h, 1, 5);
    shift_click(&mut h, two);
    let want2: String = line(1).chars().take(6).collect();
    assert_eq!(
        h.state().views[0].selected_text().as_deref(),
        Some(format!("{}\n{want2}", line(0)).as_str()),
        "from page 1's anchor to page 2's head"
    );
}

/// Issue #527: a cross-page selection is marked on every page it covers, and one undo takes
/// the whole marking back.
#[test]
fn a_cross_page_selection_marks_both_pages_with_one_undo() {
    let (mut h, _) = harness_with(bottom_lines(2), |app| {
        app.set_option("layout", "continuous").unwrap();
        app.set_option("zoom", "100").unwrap();
    });
    h.state_mut().views[0].select_text_range((0, 0), (1, 5));
    h.run_steps(2);
    let highlights = |h: &Harness<'static, PdfCraftApp>| {
        let app = h.state();
        let doc = app.session.get(app.views[0].id).expect("doc open");
        doc.info.annotations.iter().filter(|a| a.subtype == "Highlight").count()
    };
    assert!(h.state_mut().execute("comment.highlight"), "the highlight command runs");
    for _ in 0..200 {
        if highlights(&h) == 2 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
        h.run_steps(1);
    }
    assert_eq!(highlights(&h), 2, "one highlight per page the selection covers");
    assert!(h.state().views[0].selected_text().is_none(), "the selection is spent");
    assert!(h.state_mut().execute("edit.undo"), "undo runs");
    for _ in 0..200 {
        if highlights(&h) == 0 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(5));
        h.run_steps(1);
    }
    assert_eq!(highlights(&h), 0, "one undo takes back both pages");
}
