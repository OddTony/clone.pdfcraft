//! The process-wide legacy-encoding choice. It lives in its own test binary because it changes a
//! global that every other test of the crate reads.

use pdfcraft_cos::{Dict, Document, Object};
use pdfcraft_fonts::LegacyEncoding;
use pdfcraft_fonts::pdf::Metrics;

#[test]
fn the_process_wide_choice_drives_from_dict() {
    let doc = Document::new_empty();
    let mut font = Dict::new();
    font.set(b"Type".to_vec(), Object::name("Font"));
    font.set(b"Subtype".to_vec(), Object::name("Type1"));
    font.set(b"BaseFont".to_vec(), Object::name("Helvetica"));
    font.set(b"Encoding".to_vec(), Object::name("WinAnsiEncoding"));
    let read = || Metrics::from_dict(&doc, &font).decode(b"\xF9\xEC\xE5\xED");
    assert_eq!(pdfcraft_fonts::legacy_encoding(), LegacyEncoding::Auto, "the default");
    assert_eq!(read(), "ùìåí", "Helvetica has no program to detect from");
    for (mode, expected) in [
        (LegacyEncoding::Iso8859_8, "שלום"),
        (LegacyEncoding::Windows1255, "שלום"),
        (LegacyEncoding::Declared, "ùìåí"),
        (LegacyEncoding::Auto, "ùìåí"),
    ] {
        pdfcraft_fonts::set_legacy_encoding(mode);
        assert_eq!(pdfcraft_fonts::legacy_encoding(), mode);
        assert_eq!(read(), expected, "{mode:?}");
    }
}
