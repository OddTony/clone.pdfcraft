//! The macOS application menu (the first menu in the menu bar: About, Services, Hide, Hide
//! Others, Show All, Quit) in the interface language.
//!
//! AppKit builds that menu in English for apps without a nib (winit's default menu) and
//! localises it only through a nib's strings, so it stayed English while the rest of PdfCraft
//! changed language. The vendored winit's `set_app_menu_titles` patch retitles the items behind
//! a safe function (AppKit calls need `unsafe`, which this workspace forbids); this module words
//! the titles from the catalog and is told each language by [`PdfCraftApp::on_language`].
//!
//! [`PdfCraftApp::on_language`]: pdfcraft_ui_egui::PdfCraftApp::on_language

use pdfcraft_ui_egui::i18n::{self, Lang};
use winit::platform::macos::{AppMenuTitles, set_app_menu_titles};

/// The six titles for `lang`, with `app` (the process name, as AppKit shows it) in place of
/// `{app}`. A catalog without the entries gives the English titles AppKit itself would show.
pub fn titles(lang: Lang, app: &str) -> AppMenuTitles {
    let with_app = |key: &str| i18n::fmt(i18n::tr(lang, key), &[("app", app)]);
    AppMenuTitles {
        about: with_app("About {app}"),
        services: i18n::tr(lang, "Services").to_string(),
        hide: with_app("Hide {app}"),
        hide_others: i18n::tr(lang, "Hide Others").to_string(),
        show_all: i18n::tr(lang, "Show All").to_string(),
        quit: with_app("Quit {app}"),
    }
}

/// The name AppKit put in the menu: the executable's ("PdfCraft" in the bundle, "pdfcraft" under
/// cargo), as `NSProcessInfo.processName` reads it.
fn process_name() -> String {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.file_stem().map(|stem| stem.to_string_lossy().into_owned()))
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "PdfCraft".to_string())
}

/// Relabel the menu for `lang`; the app calls it from its first frame and after every change of
/// the preference. Returns whether the menu was there to relabel: winit builds it just before the
/// first event, so a `false` (nothing retitled yet) makes the app ask again next frame.
pub fn apply(lang: Lang) -> bool {
    let relabeled = set_app_menu_titles(&titles(lang, &process_name()));
    if relabeled == 0 {
        log::debug!("application menu not relabeled yet for {}", lang.code());
    }
    relabeled > 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn italian_titles_come_from_the_catalog() {
        let expected = AppMenuTitles {
            about: "Informazioni su PdfCraft".to_string(),
            services: "Servizi".to_string(),
            hide: "Nascondi PdfCraft".to_string(),
            hide_others: "Nascondi altre".to_string(),
            show_all: "Mostra tutte".to_string(),
            quit: "Esci da PdfCraft".to_string(),
        };
        assert_eq!(Lang::from_code("it").map(|lang| titles(lang, "PdfCraft")), Some(expected));
    }

    #[test]
    fn english_titles_are_the_ones_appkit_shows() {
        let expected = AppMenuTitles {
            about: "About pdfcraft".to_string(),
            services: "Services".to_string(),
            hide: "Hide pdfcraft".to_string(),
            hide_others: "Hide Others".to_string(),
            show_all: "Show All".to_string(),
            quit: "Quit pdfcraft".to_string(),
        };
        assert_eq!(titles(Lang::EN, "pdfcraft"), expected);
    }

    #[test]
    fn a_language_without_the_entries_keeps_english() {
        // Every catalog may add the six entries; until then the menu reads as AppKit built it.
        for lang in Lang::all() {
            let t = titles(lang, "PdfCraft");
            assert!(!t.about.is_empty() && !t.quit.is_empty(), "{}", lang.code());
            assert!(t.about.contains("PdfCraft") && t.hide.contains("PdfCraft") && t.quit.contains("PdfCraft"), "{}", lang.code());
        }
    }
}
