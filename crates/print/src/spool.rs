//! System print spooler supplied by craft-print. PDF composition and permission checks
//! stay in PdfCraft; these adapters retain the application's print-error surface.

pub use craft_print::{
    Duplex, HANDLED_OPTIONS, HAS_PRINTER_PREFERENCES, Job, Printer, PrinterOption, Scaling, lp_args, lpstat_command, parse_lpoptions, parse_lpstat,
    parse_lpstat_e, parse_ppd, parse_windows_printers, ppd_text, printers_parsed,
};

/// The portal is an operation, not an enumerated printer. Native packages retain CUPS.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Backend {
    Native,
    Portal,
}

pub fn backend() -> Backend {
    #[cfg(target_os = "linux")]
    {
        select_backend(true, std::path::Path::new("/.flatpak-info").is_file(), std::env::var_os("FLATPAK_ID").is_some_and(|id| !id.is_empty()))
    }
    #[cfg(not(target_os = "linux"))]
    {
        Backend::Native
    }
}

#[cfg(any(target_os = "linux", test))]
fn select_backend(linux: bool, flatpak_info: bool, flatpak_id: bool) -> Backend {
    if linux && (flatpak_info || flatpak_id) { Backend::Portal } else { Backend::Native }
}

/// Flatpak deliberately does not query CUPS. The UI offers the system dialog separately.
pub fn printers() -> Vec<Printer> {
    if backend() == Backend::Portal { Vec::new() } else { craft_print::printers() }
}

/// The driver's job options for a printer (CUPS: its PPD, with the queue's current defaults);
/// empty when it has none or on other platforms. Reads files and runs `lpoptions`, so call it
/// when the user asks, not every frame.
pub fn printer_options(printer: &str) -> Vec<PrinterOption> {
    if backend() == Backend::Portal {
        return Vec::new();
    }
    #[cfg(all(unix, not(target_arch = "wasm32")))]
    {
        printer_options_via(printer, std::path::Path::new("/etc/cups/ppd"), std::process::Command::new("lpoptions"))
    }
    #[cfg(not(all(unix, not(target_arch = "wasm32"))))]
    {
        craft_print::printer_options(printer)
    }
}

/// Read a destination's options with the PPD directory and command supplied, so tests need
/// neither a system queue nor a physical printer. CUPS instances share their queue's PPD but
/// have their own defaults, queried with the complete `queue/instance` destination.
#[cfg(all(unix, not(target_arch = "wasm32")))]
pub(crate) fn printer_options_via(printer: &str, ppd_dir: &std::path::Path, mut lpoptions: std::process::Command) -> Vec<PrinterOption> {
    let (queue, instance) = printer.split_once('/').map_or((printer, None), |(q, i)| (q, Some(i)));
    // Retain the conservative name checks, applied to each component. Neither component is
    // a path: reject dot components, backslashes and any additional separator before I/O.
    let valid = |name: &str| {
        !name.is_empty()
            && name.len() <= 64
            && !matches!(name, "." | "..")
            && name.bytes().all(|b| b.is_ascii_graphic() && !matches!(b, b'/' | b'\\' | b':' | b'=' | b'"'))
    };
    if !valid(queue) || instance.is_some_and(|name| !valid(name)) {
        return Vec::new();
    }
    const PPD_CAP: u64 = 16 << 20;
    let ppd = std::fs::File::open(ppd_dir.join(format!("{queue}.ppd"))).ok().and_then(|f| {
        use std::io::Read as _;
        let mut bytes = Vec::new();
        f.take(PPD_CAP).read_to_end(&mut bytes).ok().map(|_| bytes)
    });
    let mut options = ppd.map(|b| parse_ppd(&ppd_text(&b))).unwrap_or_default();
    let current = lpoptions.args(["-p", printer, "-l"]).output().map(|o| parse_lpoptions(&String::from_utf8_lossy(&o.stdout))).unwrap_or_default();
    for (key, value) in current {
        if let Some(o) = options.iter_mut().find(|o| o.key == key)
            && o.choices.iter().any(|(c, _)| *c == value)
        {
            o.default = value;
        }
    }
    options
}

pub fn submit(pdf: &[u8], job: &Job) -> Result<String, crate::PrintError> {
    submit_with_progress(pdf, job, &mut |_, _| {})
}

/// [`submit`], told `(sent, sheets)` as each sheet reaches the spooler (Windows; CUPS takes the
/// whole PDF at once and reports nothing).
pub fn submit_with_progress(pdf: &[u8], job: &Job, progress: &mut dyn FnMut(usize, usize)) -> Result<String, crate::PrintError> {
    #[cfg(target_os = "linux")]
    if backend() == Backend::Portal {
        if job.printer.as_deref().is_some_and(|p| p != "system-dialog")
            || job.copies != 1
            || job.duplex.is_some()
            || job.grayscale
            || job.scaling != Scaling::Exact
            || !job.options.is_empty()
        {
            return Err(crate::PrintError::Spool(
                "Flatpak printing accepts a finished PDF; choose printer, copies, duplex, colour and paper in the system dialog".into(),
            ));
        }
        return match crate::portal::submit(pdf, &job.title, "", &Default::default())? {
            crate::portal::Outcome::Accepted => Ok("Accepted by the system print service; physical completion is not reported".into()),
            crate::portal::Outcome::Cancelled => Err(crate::PrintError::Spool("Printing cancelled".into())),
        };
    }
    craft_print::submit_with_progress(pdf, job, progress).map_err(|e| crate::PrintError::Spool(e.to_string()))
}

pub fn open_printer_preferences(printer: &str) -> Result<(), crate::PrintError> {
    if backend() == Backend::Portal {
        return Err(crate::PrintError::Spool("Choose printer settings in the system print dialog".into()));
    }
    craft_print::open_printer_preferences(printer).map_err(|e| crate::PrintError::Spool(e.to_string()))
}

#[cfg(test)]
mod backend_tests {
    use super::*;
    #[test]
    fn sandbox_selection_keeps_native_packages_and_other_platforms() {
        assert_eq!(select_backend(true, false, false), Backend::Native);
        assert_eq!(select_backend(true, true, false), Backend::Portal);
        assert_eq!(select_backend(true, false, true), Backend::Portal);
        assert_eq!(select_backend(false, true, true), Backend::Native);
    }
}
