//! WinAnsiEncoding plus the [`EXTRA_LATIN`] letters, for text written in the standard-14 text
//! fonts (#1513, #1608).
//!
//! WinAnsi has no code for Turkish Ğ ğ İ ı Ş ş, but every standard text font has their glyphs. A
//! font that needs them gets its own encoding: WinAnsi as the base, and a `/Differences` entry that
//! gives each such letter a code the text doesn't otherwise use. The codes are chosen per text, so
//! WinAnsi text is never re-read as something else (`ý` stays `ý` next to a Turkish `ı`), and the
//! font carries a ToUnicode map so the text copies and searches as typed. Text that WinAnsi holds
//! on its own needs none of this and is written byte for byte as before.

use pdfcraft_cos::Object;

use crate::encodings::WIN_ANSI;
use crate::std14::{EXTRA_LATIN, Std14};
use crate::win_ansi_byte;

/// An encoding for one text in a standard-14 text font: WinAnsi, plus a code for each
/// [`EXTRA_LATIN`] letter the text has.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct LatinEncoding {
    /// Each extra letter the text uses, its code and glyph name, in first-use order.
    extra: Vec<(char, u8, &'static str)>,
}

impl LatinEncoding {
    /// The encoding for text made of `parts` (its lines). `Err` holds the first character that
    /// neither WinAnsi nor [`EXTRA_LATIN`] has, which these fonts can't draw. A tab counts as a
    /// space, as [`crate::win_ansi`] writes it.
    pub fn new<'a>(parts: impl IntoIterator<Item = &'a str> + Clone) -> Result<Self, char> {
        let mut used = [false; 256];
        let mut wanted: Vec<(char, &'static str)> = Vec::new();
        for c in parts.clone().into_iter().flat_map(str::chars) {
            if c == '\t' {
                continue;
            }
            if let Some(b) = win_ansi_byte(c) {
                if let Some(u) = used.get_mut(usize::from(b)) {
                    *u = true;
                }
                continue;
            }
            match EXTRA_LATIN.iter().find(|(e, _, _)| *e == c) {
                Some((e, name, _)) => {
                    if !wanted.iter().any(|(w, _)| w == e) {
                        wanted.push((*e, *name));
                    }
                }
                None => return Err(c),
            }
        }
        // The upper-half codes WinAnsi leaves undefined first, then its defined upper-half codes
        // from the top down that the text doesn't use, then DEL. Printable ASCII is never
        // reassigned.
        let undefined = (0x80u8..=0xFF).filter(|&b| WIN_ANSI.get(usize::from(b)) == Some(&0));
        let defined = (0xA1u8..=0xFF).rev().filter(|&b| WIN_ANSI.get(usize::from(b)) != Some(&0));
        let mut free = undefined.chain(defined).chain([0x7F]).filter(|&b| !used.get(usize::from(b)).copied().unwrap_or(true));
        // The five undefined upper codes and DEL are never used by WinAnsi text, and there are six
        // extra letters: `free` can't run out.
        let mut extra = Vec::with_capacity(wanted.len());
        for (c, name) in wanted {
            match free.next() {
                Some(code) => extra.push((c, code, name)),
                None => return Err(c),
            }
        }
        Ok(Self { extra })
    }

    /// Whether this is plain WinAnsiEncoding: the text has none of the extra letters.
    pub fn is_win_ansi(&self) -> bool {
        self.extra.is_empty()
    }

    /// The code for `c`: its WinAnsi code, or the one chosen for an extra letter.
    pub fn byte(&self, c: char) -> Option<u8> {
        win_ansi_byte(c).or_else(|| self.extra.iter().find(|(e, _, _)| *e == c).map(|(_, b, _)| *b))
    }

    /// Encode `s`; tabs become spaces and characters the encoding lacks become `?`, as in
    /// [`crate::win_ansi`].
    pub fn encode(&self, s: &str) -> Vec<u8> {
        s.chars().map(|c| if c == '\t' { b' ' } else { self.byte(c).unwrap_or(b'?') }).collect()
    }

    /// The advance of `s` in `font` at `size` points, measured as [`Self::encode`] writes it.
    pub fn text_width(&self, font: Std14, s: &str, size: f64) -> f64 {
        let units: f64 = s
            .chars()
            .map(|c| match font.extra_latin_width(c).filter(|_| self.extra.iter().any(|(e, _, _)| *e == c)) {
                Some(w) => w,
                None => font.width(if c == '\t' { b' ' } else { win_ansi_byte(c).unwrap_or(b'?') }),
            })
            .sum();
        units * size / 1000.0
    }

    /// The font's `/Encoding`: `None` for plain WinAnsi (name it `/WinAnsiEncoding`), else an
    /// encoding dictionary with WinAnsi as its base and the extra letters as `/Differences`.
    pub fn encoding(&self) -> Option<pdfcraft_cos::Dict> {
        if self.is_win_ansi() {
            return None;
        }
        let mut extra = self.extra.clone();
        extra.sort_by_key(|(_, b, _)| *b);
        // One `code /name` pair per letter; the codes are distinct, so each starts its own run.
        let differences = extra.iter().flat_map(|(_, b, name)| [Object::Int(i64::from(*b)), Object::name(name)]).collect();
        let mut d = pdfcraft_cos::Dict::new();
        d.set(b"Type".to_vec(), Object::name("Encoding"));
        d.set(b"BaseEncoding".to_vec(), Object::name("WinAnsiEncoding"));
        d.set(b"Differences".to_vec(), Object::Array(differences));
        Some(d)
    }

    /// A ToUnicode CMap for every code the encoding defines, so text extraction never has to
    /// guess a glyph name's meaning.
    pub fn to_unicode(&self) -> Vec<u8> {
        let mut entries: Vec<(u8, u32)> = (0x20u8..=0xFF)
            .filter_map(|b| {
                let extra = self.extra.iter().find(|(_, code, _)| *code == b).map(|(c, _, _)| u32::from(*c));
                extra.or_else(|| WIN_ANSI.get(usize::from(b)).copied().filter(|u| *u != 0)).map(|u| (b, u))
            })
            .collect();
        entries.sort_by_key(|(b, _)| *b);
        let mut cmap = String::from(
            "/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n/CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n/CMapName /PdfCraft-Latin-UCS def\n/CMapType 2 def\n1 begincodespacerange\n<00> <FF>\nendcodespacerange\n",
        );
        // A CMap block holds at most 100 entries.
        for block in entries.chunks(100) {
            cmap.push_str(&format!("{} beginbfchar\n", block.len()));
            for (b, u) in block {
                // Every WinAnsi and extra letter is in the Basic Multilingual Plane.
                cmap.push_str(&format!("<{b:02X}> <{u:04X}>\n"));
            }
            cmap.push_str("endbfchar\n");
        }
        cmap.push_str("endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n");
        cmap.into_bytes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TURKISH: &str = "İSTANBUL ışık Ğğ Şş Öö Üü Çç";

    #[test]
    fn win_ansi_text_needs_no_new_encoding() {
        let e = LatinEncoding::new(["Café — 5€", "Ýð þ"]).unwrap();
        assert!(e.is_win_ansi());
        assert_eq!(e.encoding(), None);
        assert_eq!(e.encode("Café\t5€"), crate::win_ansi("Café\t5€"));
    }

    #[test]
    fn turkish_letters_get_codes_the_text_does_not_use() {
        let e = LatinEncoding::new([TURKISH]).unwrap();
        assert!(!e.is_win_ansi());
        let bytes = e.encode(TURKISH);
        assert!(!bytes.contains(&b'?'), "{bytes:?}");
        // Ö ö Ü ü Ç ç are WinAnsi already and keep their codes.
        for c in ['Ö', 'ö', 'Ü', 'ü', 'Ç', 'ç', 'S', 'I'] {
            assert_eq!(e.byte(c), crate::win_ansi_byte(c), "{c}");
        }
        // The six extra letters have six distinct codes, none of them a WinAnsi code the text uses.
        let codes: Vec<u8> = ['Ğ', 'ğ', 'İ', 'ı', 'Ş', 'ş'].iter().map(|c| e.byte(*c).unwrap()).collect();
        let mut distinct = codes.clone();
        distinct.sort_unstable();
        distinct.dedup();
        assert_eq!(distinct.len(), 6);
        let win: Vec<u8> = TURKISH.chars().filter_map(crate::win_ansi_byte).collect();
        assert!(codes.iter().all(|c| !win.contains(c) && *c >= 0x80), "{codes:?} {win:?}");
    }

    #[test]
    fn turkish_next_to_icelandic_keeps_both() {
        // #1513 put ı on 0xFD, which is ý: here both are written, each as itself.
        let text = "ı ý İ Ý ş þ ğ ð";
        let e = LatinEncoding::new([text]).unwrap();
        assert_eq!(e.byte('ý'), Some(0xFD));
        assert_eq!(e.byte('Ý'), Some(0xDD));
        assert_eq!(e.byte('þ'), Some(0xFE));
        assert_eq!(e.byte('ð'), Some(0xF0));
        for c in ['ı', 'İ', 'ş', 'ğ'] {
            let b = e.byte(c).unwrap();
            assert!(![0xFD, 0xDD, 0xFE, 0xF0].contains(&b), "{c} took {b:#X}");
        }
        // The CMap reads each code back as the character it was written for.
        let cmap = String::from_utf8(e.to_unicode()).unwrap();
        for c in text.chars().filter(|c| *c != ' ') {
            let entry = format!("<{:02X}> <{:04X}>", e.byte(c).unwrap(), u32::from(c));
            assert!(cmap.contains(&entry), "{c}: {entry} missing");
        }
    }

    #[test]
    fn differences_name_the_standard_glyphs() {
        let e = LatinEncoding::new(["Iğdır", "Şile"]).unwrap();
        let d = e.encoding().unwrap();
        assert_eq!(d.get(b"BaseEncoding").and_then(Object::as_name), Some(&b"WinAnsiEncoding"[..]));
        let Some(Object::Array(diff)) = d.get(b"Differences") else { panic!("no /Differences") };
        let names: Vec<&[u8]> = diff.iter().filter_map(Object::as_name).collect();
        assert_eq!(names.len(), 3, "ğ ı Ş: {diff:?}");
        for name in [&b"gbreve"[..], b"dotlessi", b"Scedilla"] {
            assert!(names.contains(&name), "{}", String::from_utf8_lossy(name));
        }
        // Every code is followed by its own name.
        for pair in diff.chunks(2) {
            let [Object::Int(code), name] = pair else { panic!("{pair:?}") };
            assert_eq!(e.byte(['ğ', 'ı', 'Ş'].into_iter().find(|c| name.as_name() == Some(glyph(*c))).unwrap()), u8::try_from(*code).ok());
        }
    }

    fn glyph(c: char) -> &'static [u8] {
        EXTRA_LATIN.iter().find(|(e, _, _)| *e == c).map(|(_, n, _)| n.as_bytes()).unwrap()
    }

    #[test]
    fn widths_are_the_published_ones() {
        let e = LatinEncoding::new([TURKISH]).unwrap();
        // Helvetica: dotlessi is 278 while i is 222; Gbreve is G's 778.
        assert!((e.text_width(Std14::Helvetica, "ı", 1000.0) - 278.0).abs() < 1e-9);
        assert!((e.text_width(Std14::Helvetica, "i", 1000.0) - 222.0).abs() < 1e-9);
        assert!((e.text_width(Std14::Helvetica, "Ğ", 1000.0) - 778.0).abs() < 1e-9);
        assert!((e.text_width(Std14::TimesBold, "İ", 1000.0) - 389.0).abs() < 1e-9);
        // Courier: every letter is 600.
        assert!((e.text_width(Std14::Courier, "ĞğİıŞş", 10.0) - 36.0).abs() < 1e-9);
        // WinAnsi text measures as Std14 measures it.
        assert!((e.text_width(Std14::TimesRoman, "Öykü", 12.0) - Std14::TimesRoman.text_width("Öykü", 12.0)).abs() < 1e-9);
        // An extra letter the encoding has no code for measures as the `?` it would be written as.
        let plain = LatinEncoding::default();
        assert!((plain.text_width(Std14::Helvetica, "ı", 1000.0) - 556.0).abs() < 1e-9);
    }

    #[test]
    fn every_extra_letter_has_a_width_in_every_text_font() {
        for font in
            [Std14::Helvetica, Std14::HelveticaBoldOblique, Std14::TimesRoman, Std14::TimesBoldItalic, Std14::Courier, Std14::CourierBoldOblique]
        {
            for (c, _, _) in EXTRA_LATIN {
                assert!(font.extra_latin_width(c).is_some_and(|w| w > 0.0), "{font:?} {c}");
            }
        }
        assert_eq!(Std14::Symbol.extra_latin_width('ı'), None);
        assert_eq!(Std14::Helvetica.extra_latin_width('a'), None);
    }

    #[test]
    fn other_scripts_are_refused_with_the_character() {
        assert_eq!(LatinEncoding::new(["Iğdır", "Račun"]), Err('č'));
        assert_eq!(LatinEncoding::new(["签名"]), Err('签'));
        assert_eq!(LatinEncoding::new(["a\nb"]), Err('\n'));
    }

    #[test]
    fn hostile_text_does_not_panic() {
        let all: String =
            (0x80u8..=0xFF).filter_map(|b| WIN_ANSI.get(usize::from(b)).and_then(|u| char::from_u32(*u)).filter(|c| *c != '\0')).collect();
        // Every WinAnsi upper-half code in use still leaves the undefined ones for the extras.
        let e = LatinEncoding::new([all.as_str(), "ĞğİıŞş"]).unwrap();
        assert!(!e.encode("ĞğİıŞş").contains(&b'?'));
        let mut codes: Vec<u8> = "ĞğİıŞş".chars().filter_map(|c| e.byte(c)).collect();
        codes.sort_unstable();
        codes.dedup();
        assert_eq!(codes.len(), 6, "{codes:?}");
        for s in ["", "\0", "\u{FFFD}", &"ı".repeat(10_000)] {
            let _ = LatinEncoding::new([s]);
        }
    }
}
