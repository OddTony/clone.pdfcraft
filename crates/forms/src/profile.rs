//! Opt-in native form timing. No field names, values or script source are logged.
//! Set `PDFCRAFT_PROFILE_FORMS=1`; nested durations are inclusive and must not be added.

/// A timing span, disabled unless explicitly requested (and always disabled on wasm).
#[doc(hidden)]
pub struct Span {
    #[cfg(not(target_arch = "wasm32"))]
    started: Option<std::time::Instant>,
    label: &'static str,
}

impl Span {
    pub fn new(label: &'static str) -> Self {
        Self {
            #[cfg(not(target_arch = "wasm32"))]
            started: (std::env::var_os("PDFCRAFT_PROFILE_FORMS").as_deref() == Some(std::ffi::OsStr::new("1"))).then(std::time::Instant::now),
            label,
        }
    }
}

impl Drop for Span {
    fn drop(&mut self) {
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(started) = self.started {
            use std::io::Write;
            let _ = writeln!(std::io::stderr().lock(), "form-profile {} {:.3} ms", self.label, started.elapsed().as_secs_f64() * 1000.0);
        }
        #[cfg(target_arch = "wasm32")]
        let _ = self.label;
    }
}
