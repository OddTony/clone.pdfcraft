#[test]
fn winit_carries_the_app_menu_titles_patch() {
    // The relabeling itself needs AppKit on the main thread; `mac_menu`'s tests cover the titles.
    // This guards PdfCraft's vendored copy, which workspace tests do not test itself.
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let menu = std::fs::read_to_string(root.join("vendor/winit/src/platform_impl/macos/menu.rs")).unwrap();
    assert!(menu.contains("pub(crate) fn relabel(titles: &AppMenuTitles) -> usize"));
    for action in ["orderFrontStandardAboutPanel:", "hide:", "hideOtherApplications:", "unhideAllApplications:", "terminate:"] {
        assert!(menu.contains(&format!("sel!({action})")), "{action}");
    }
    assert!(menu.contains("Retained::as_ptr(submenu) == Retained::as_ptr(services)"));
    let platform = std::fs::read_to_string(root.join("vendor/winit/src/platform/macos.rs")).unwrap();
    assert!(platform.contains("pub struct AppMenuTitles"));
    assert!(platform.contains("pub fn set_app_menu_titles(titles: &AppMenuTitles) -> usize"));
    let italian = std::fs::read_to_string(root.join("crates/ui-egui/src/i18n/it.tsv")).unwrap();
    assert!(italian.contains("\tQuit {app}\tEsci da {app}"));
}
