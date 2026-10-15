//! Who draws the window frame on Linux.
//!
//! By default winit draws an Adwaita client-side frame when the Wayland compositor does not
//! offer server-side decorations (GNOME), and uses the window manager's frame otherwise.
//! The desktop build enables `wayland-csd-adwaita`; buttons are drawn as vectors, not glyphs.
//!
//! `PDFCRAFT_TITLEBAR=app` explicitly opts into PdfCraft's integrated header buttons.
//! `PDFCRAFT_TITLEBAR=system` keeps the default toolkit frame.

use pdfcraft_ui_egui::window_controls::Layout;

/// The window buttons to draw in the app, or `None` when the desktop draws the frame.
pub(crate) fn app_drawn() -> Option<Layout> {
    let value = std::env::var("PDFCRAFT_TITLEBAR").ok();
    app_frame_requested(value.as_deref()).then(button_layout)
}

/// GNOME's button order, or minimize, maximize and close on the right when it names none.
fn button_layout() -> Layout {
    #[cfg(any(target_os = "linux", target_os = "freebsd", target_os = "openbsd", target_os = "netbsd"))]
    if let Some(value) = crate::text_scale::gsettings(&["get", "org.gnome.desktop.wm.preferences", "button-layout"]) {
        let layout = Layout::parse(&value);
        // A layout without a single button would leave no way to close the window by mouse.
        if !layout.left.is_empty() || !layout.right.is_empty() {
            return layout;
        }
    }
    Layout::default()
}

fn app_frame_requested(value: Option<&str>) -> bool {
    value.map(str::trim) == Some("app")
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_toolkit_frame_is_default_and_the_app_frame_is_opt_in() {
        for value in [None, Some("system"), Some(""), Some("unknown")] {
            assert!(!super::app_frame_requested(value));
        }
        assert!(super::app_frame_requested(Some("app")));
        assert!(super::app_frame_requested(Some(" app ")));
    }
}
