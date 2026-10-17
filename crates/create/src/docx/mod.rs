//! DOCX import: Word documents
//! become PDFs natively, without an office suite. Behaviour follows ECMA-376 (Office Open XML,
//! 5th edition) and the files' own markup. Adapted from ArturFaleiros's native DOCX
//! work in PdfCraft PR #2326 (13caa2402467c8bb8e175412b976849b7cfad8dd).
//!
//! Text is set in the standard-14 faces (Helvetica, Times, Courier and their bold and italic
//! styles) chosen from each run's font family. Turkish letters missing from WinAnsi use embedded
//! OFL Inter with Unicode mappings for search and copy. Other unsupported characters are written as `[U+XXXX]` and counted in
//! the [`DocxReport`], never set as `?`. Content the importer can't draw (charts, SmartArt,
//! vector images) is replaced by an outlined box of the same size and counted too.

mod canvas;
mod flow;
mod turkish;
mod word;
mod xml;
pub(crate) mod zip;

#[cfg(test)]
mod tests;

use pdfcraft_cos::Document;
use pdfcraft_fonts::Std14;
use unicode_normalization::UnicodeNormalization;

use crate::CreateError;

/// What the conversion could not carry over exactly. Nothing in the source is changed.
#[derive(Clone, Debug, PartialEq)]
pub struct DocxReport {
    pub pages: usize,
    /// Characters the standard fonts can't draw, written as `[U+XXXX]`.
    pub escaped: usize,
    /// One line per kind of content that was approximated or left out, with how many.
    pub notes: Vec<String>,
}

/// Tallies of approximations, turned into [`DocxReport::notes`].
#[derive(Default)]
pub(crate) struct Notes {
    escaped: usize,
    counts: Vec<(&'static str, usize)>,
}

impl Notes {
    pub(crate) fn add(&mut self, what: &'static str) {
        match self.counts.iter_mut().find(|(w, _)| *w == what) {
            Some((_, n)) => *n = n.saturating_add(1),
            None => self.counts.push((what, 1)),
        }
    }

    fn report(self, pages: usize) -> DocxReport {
        let mut notes: Vec<String> = self.counts.into_iter().map(|(what, n)| format!("{what}: {n}")).collect();
        if self.escaped > 0 {
            notes.insert(0, format!("characters the built-in fonts can't draw, written as [U+XXXX]: {}", self.escaped));
        }
        DocxReport { pages, escaped: self.escaped, notes }
    }

    /// `s` with every character the standard fonts can't draw written as `[U+XXXX]`, symbol-font
    /// private-use bullets mapped to their usual shapes, and invisible format characters dropped.
    pub(crate) fn drawable(&mut self, s: &str) -> String {
        let mut out = String::with_capacity(s.len());
        for c in s.nfc() {
            let c = match c {
                // Zero-width and soft-hyphen characters draw nothing.
                '\u{00AD}' | '\u{200B}'..='\u{200F}' | '\u{2060}' | '\u{FEFF}' => continue,
                // Symbol / Wingdings bullets stored in the private use area (U+F0xx).
                '\u{F0B7}' | '\u{F06C}' | '\u{F0A7}' | '\u{F0D8}' | '\u{F0FC}' | '\u{F076}' | '\u{F0A8}' | '\u{25CF}' | '\u{25AA}' | '\u{25A0}' => {
                    '•'
                }
                '\u{F0E8}' | '\u{F0E0}' | '\u{27A2}' | '\u{25BA}' | '\u{25B6}' => '>',
                '\u{2013}'..='\u{2014}'
                | '\u{2018}'..='\u{201E}'
                | '\u{2020}'..='\u{2022}'
                | '\u{2026}'
                | '\u{2030}'
                | '\u{2039}'
                | '\u{203A}'
                | '\u{20AC}' => c,
                '\u{2010}' | '\u{2011}' | '\u{2212}' => '-',
                '\u{00A0}' | '\u{2002}'..='\u{200A}' | '\u{202F}' => ' ',
                '\u{25E6}' | '\u{2219}' => '·',
                '\u{2713}' | '\u{2714}' => 'x',
                c => c,
            };
            if pdfcraft_fonts::win_ansi_byte(c).is_some() || turkish::glyph(c, false).is_some() || c == ' ' {
                out.push(c);
            } else if c.is_control() {
                // Other control characters carry no ink.
            } else {
                self.escaped = self.escaped.saturating_add(1);
                out.push_str(&format!("[U+{:04X}]", u32::from(c)));
            }
        }
        out
    }
}

/// A standard-14 family chosen from a document font name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Fam {
    Sans,
    Serif,
    Mono,
}

/// The standard family closest to a font name: serif faces become Times, monospaced faces Courier,
/// everything else (Calibri, Arial, Aptos, Segoe…) Helvetica.
pub(crate) fn family_of(name: &str) -> Fam {
    let n = name.to_ascii_lowercase();
    const MONO: [&str; 9] = ["courier", "consolas", "mono", "menlo", "lucida console", "code", "typewriter", "fixedsys", "terminal"];
    const SERIF: [&str; 16] = [
        "times",
        "georgia",
        "garamond",
        "cambria",
        "book",
        "palatino",
        "century",
        "constantia",
        "baskerville",
        "minion",
        "roman",
        "serif",
        "bodoni",
        "didot",
        "caslon",
        "liberation serif",
    ];
    if MONO.iter().any(|m| n.contains(m)) {
        Fam::Mono
    } else if SERIF.iter().any(|s| n.contains(s)) && !n.contains("sans") {
        Fam::Serif
    } else {
        Fam::Sans
    }
}

/// How a run of text looks.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct RunStyle {
    pub fam: Fam,
    pub bold: bool,
    pub italic: bool,
    /// Points.
    pub size: f64,
    pub color: [f64; 3],
    pub underline: bool,
    pub strike: bool,
    pub highlight: Option<[f64; 3]>,
    /// 1 superscript, -1 subscript, 0 baseline.
    pub vert: i8,
    pub caps: bool,
    pub hidden: bool,
}

impl Default for RunStyle {
    fn default() -> Self {
        RunStyle {
            fam: Fam::Sans,
            bold: false,
            italic: false,
            size: 11.0,
            color: [0.0; 3],
            underline: false,
            strike: false,
            highlight: None,
            vert: 0,
            caps: false,
            hidden: false,
        }
    }
}

impl RunStyle {
    pub(crate) fn face(&self) -> Std14 {
        match (self.fam, self.bold, self.italic) {
            (Fam::Sans, false, false) => Std14::Helvetica,
            (Fam::Sans, true, false) => Std14::HelveticaBold,
            (Fam::Sans, false, true) => Std14::HelveticaOblique,
            (Fam::Sans, true, true) => Std14::HelveticaBoldOblique,
            (Fam::Serif, false, false) => Std14::TimesRoman,
            (Fam::Serif, true, false) => Std14::TimesBold,
            (Fam::Serif, false, true) => Std14::TimesItalic,
            (Fam::Serif, true, true) => Std14::TimesBoldItalic,
            (Fam::Mono, false, false) => Std14::Courier,
            (Fam::Mono, true, false) => Std14::CourierBold,
            (Fam::Mono, false, true) => Std14::CourierOblique,
            (Fam::Mono, true, true) => Std14::CourierBoldOblique,
        }
    }

    /// The size glyphs are drawn at (super- and subscripts are smaller).
    pub(crate) fn draw_size(&self) -> f64 {
        let s = if self.vert != 0 { self.size * 0.65 } else { self.size };
        if s.is_finite() { s.clamp(1.0, 400.0) } else { 11.0 }
    }

    /// Width of already-drawable text in points.
    pub(crate) fn width(&self, s: &str) -> f64 {
        s.chars()
            .map(|c| {
                turkish::glyph(c, self.bold)
                    .map_or_else(|| self.face().text_width(c.encode_utf8(&mut [0; 4]), self.draw_size()), |(_, advance)| advance * self.draw_size())
            })
            .sum()
    }
}

/// A hex `RRGGBB` colour (an optional leading `#` or alpha byte `AARRGGBB` is ignored).
pub(crate) fn hex_color(v: &str) -> Option<[f64; 3]> {
    let v = v.trim().trim_start_matches('#');
    let v = if v.len() == 8 { v.get(2..)? } else { v };
    if v.len() != 6 || !v.is_ascii() {
        return None;
    }
    let ch = |i: usize| u8::from_str_radix(v.get(i..i + 2)?, 16).ok().map(|b| f64::from(b) / 255.0);
    Some([ch(0)?, ch(2)?, ch(4)?])
}

/// Convert a DOCX package into a PDF without a converter process or network access.
/// The source is read only; font substitutions and unsupported content are reported.
pub fn from_docx(name: &str, bytes: &[u8]) -> Result<(Document, DocxReport), CreateError> {
    let fail = |m: String| CreateError::Invalid(format!("{name}: {m}"));
    let zip = zip::Zip::open(bytes).map_err(fail)?;
    let rels = xml::Rels::load(&zip, "").map_err(fail)?;
    let main = rels
        .of_type("officeDocument")
        .map(str::to_owned)
        .or_else(|| zip.has("word/document.xml").then(|| "word/document.xml".to_owned()))
        .ok_or_else(|| fail("the package has no main document part".into()))?;
    let text = zip.text(&main).map_err(fail)?.ok_or_else(|| fail(format!("{main} is missing")))?;
    let xml = xml::parse(&text).map_err(fail)?;
    let tag = xml.root_element().tag_name();
    if tag.name() != "document"
        || !matches!(
            tag.namespace(),
            Some("http://schemas.openxmlformats.org/wordprocessingml/2006/main" | "http://purl.oclc.org/ooxml/wordprocessingml/main")
        )
    {
        return Err(fail("the package is not a Word document".into()));
    }
    let mut doc = Document::new_empty();
    let mut notes = Notes::default();
    let pages = word::convert(&zip, &main, &mut doc, &mut notes).map_err(fail)?;
    crate::set_title(&mut doc, name.rsplit_once('.').map_or(name, |(s, _)| s));
    let report = notes.report(pages);
    // Keep the conversion limitations with a standalone PDF, including headless callers.
    if let Some(info) = doc.trailer().reference(b"Info") {
        let notice = report.notice();
        let _ = doc.update_dict(info, |d| d.set(b"Subject".to_vec(), pdfcraft_cos::PdfString::text(&notice)));
    }
    Ok((doc, report))
}

impl DocxReport {
    /// Human-readable caveat, followed by the content-specific conversion notes.
    pub fn notice(&self) -> String {
        let mut notice = "DOCX import substitutes fonts; layout may differ from Word. Review the converted pages.".to_owned();
        if !self.notes.is_empty() {
            notice.push(' ');
            notice.push_str(&self.notes.join("; "));
        }
        notice
    }
}
