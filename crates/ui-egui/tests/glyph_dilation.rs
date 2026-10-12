//! The glyph dilation (epaint patch; Preferences ▸ Appearance ▸ Text rendering ▸ Like macOS) is a
//! process-wide setting, so it is tested in its own process.

use egui::epaint::text::{Fonts, TextOptions};
use egui::{Color32, FontFamily, FontId};
use pdfcraft_ui_egui::theme;

/// The alpha the glyphs of `text` put in the font atlas.
fn ink(dilation: f32, text: &str) -> u64 {
    egui::epaint::text::set_glyph_dilation(dilation);
    let mut fonts = Fonts::new(TextOptions::default(), theme::font_definitions());
    {
        let mut view = fonts.with_pixels_per_point(2.0);
        let _ = view.layout_no_wrap(text.to_owned(), FontId::new(13.0, FontFamily::Proportional), Color32::BLACK);
    }
    fonts.image().pixels.iter().map(|p| u64::from(p.a())).sum()
}

#[test]
fn glyph_dilation_adds_ink() {
    let text = "Recommended tools";
    let plain = ink(0.0, text);
    let dilated = ink(0.15, text);
    egui::epaint::text::set_glyph_dilation(0.0);
    assert!(plain > 0);
    assert!(dilated > plain, "dilated {dilated} vs plain {plain}");
    // A fraction of a pixel adds some body, not a bold face.
    assert!((dilated as f64) < plain as f64 * 1.5, "dilated {dilated} vs plain {plain}");
}
