//! `PdfCraftApp::on_language`: the desktop app hears the interface language once at start and
//! once per change (the macOS application menu follows it), and again next frame while it says
//! it is not done.

use std::cell::RefCell;
use std::rc::Rc;

use pdfcraft_ui_egui::PdfCraftApp;
use pdfcraft_ui_egui::i18n::Lang;

fn app_with_listener(done: bool) -> (PdfCraftApp, Rc<RefCell<Vec<&'static str>>>) {
    let heard = Rc::new(RefCell::new(Vec::new()));
    let mut app = PdfCraftApp::new();
    let sink = Rc::clone(&heard);
    app.on_language = Some(Box::new(move |lang| {
        sink.borrow_mut().push(lang.code());
        done
    }));
    (app, heard)
}

#[test]
fn the_listener_hears_each_language_once() {
    let (mut app, heard) = app_with_listener(true);
    app.set_option("language", "en").unwrap();
    let en = Lang::from_pref(&app.language);
    app.notify_language(en);
    app.notify_language(en);
    assert_eq!(*heard.borrow(), vec!["en"]);
    app.set_option("language", "it").unwrap();
    let it = Lang::from_pref(&app.language);
    app.notify_language(it);
    app.notify_language(it);
    app.notify_language(en);
    assert_eq!(*heard.borrow(), vec!["en", "it", "en"]);
}

#[test]
fn a_listener_that_is_not_done_is_asked_again() {
    let (mut app, heard) = app_with_listener(false);
    app.set_option("language", "it").unwrap();
    let it = Lang::from_pref(&app.language);
    app.notify_language(it);
    app.notify_language(it);
    assert_eq!(*heard.borrow(), vec!["it", "it"]);
}

#[test]
fn without_a_listener_nothing_happens() {
    let mut app = PdfCraftApp::new();
    app.set_option("language", "it").unwrap();
    app.notify_language(Lang::from_pref(&app.language));
}
