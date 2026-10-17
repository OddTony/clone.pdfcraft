//! The six Turkish letters absent from WinAnsi, drawn with the already bundled OFL Inter faces.
//! Keep codes, measured advances and embedded Unicode mappings together. Other scripts continue
//! to use the importer's explicit, counted placeholders.

use std::sync::OnceLock;

use pdfcraft_cos::{Dict, Document, ObjRef};
use pdfcraft_fonts::{
    embedding,
    source_shaping::{self, ShapedRun},
};

const LETTERS: [char; 6] = ['Ğ', 'ğ', 'İ', 'ı', 'Ş', 'ş'];
const SEMIBOLD: &[u8] = include_bytes!("../../../../assets/fonts/Inter-SemiBold.ttf");

fn bytes(bold: bool) -> &'static [u8] {
    if bold { SEMIBOLD } else { pdfcraft_fonts::INTER_REGULAR }
}

fn runs(bold: bool) -> &'static [Option<ShapedRun>; 6] {
    static REGULAR: OnceLock<[Option<ShapedRun>; 6]> = OnceLock::new();
    static BOLD: OnceLock<[Option<ShapedRun>; 6]> = OnceLock::new();
    let cache = if bold { &BOLD } else { &REGULAR };
    cache.get_or_init(|| LETTERS.map(|c| source_shaping::shape(bytes(bold), &c.to_string()).ok()))
}

/// The two-byte CID and the actual embedded face's advance in em units.
pub(super) fn glyph(c: char, bold: bool) -> Option<(u16, f64)> {
    let index = LETTERS.iter().position(|letter| *letter == c)?;
    let run = runs(bold).get(index)?.as_ref()?;
    (run.glyphs.len() == 1 && run.advance.is_finite() && run.advance > 0.0).then(|| (u16::try_from(index + 1).unwrap_or(1), run.advance))
}

pub(super) fn embed(doc: &mut Document, bold: bool) -> Result<ObjRef, String> {
    let mappings = LETTERS
        .iter()
        .zip(runs(bold))
        .map(|(c, run)| {
            let glyph = run.as_ref().and_then(|r| r.glyphs.first()).ok_or("the bundled Inter face lacks a Turkish letter")?;
            Ok((c.to_string(), glyph.id))
        })
        .collect::<Result<Vec<_>, String>>()?;
    let mut fonts = Dict::new();
    let name = embedding::embed_mapping(doc, &mut fonts, bytes(bold), if bold { "Inter-SemiBold" } else { "Inter" }, &mappings)?;
    fonts.reference(name.as_bytes()).ok_or_else(|| "the Turkish fallback font resource is missing".into())
}
