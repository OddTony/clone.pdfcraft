//! Wait for observable completion of background document opens, rather than counting frames.
use egui_kittest::Harness;
use pdfcraft_ui_egui::PdfCraftApp;
use std::time::{Duration, Instant};

pub fn wait_for(h: &mut Harness<'static, PdfCraftApp>, ready: impl Fn(&PdfCraftApp) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        h.run_steps(2);
        if ready(h.state()) {
            // Completion may be processed after widgets are drawn. Render the resulting
            // interface before callers locate and click its controls.
            h.run_steps(2);
            return;
        }
        assert!(Instant::now() < deadline, "background document open did not reach the expected state");
        std::thread::sleep(Duration::from_millis(1));
    }
}
