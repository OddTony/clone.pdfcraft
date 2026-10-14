//! Legacy single-byte Hebrew encodings for simple fonts that don't say what they draw.
//!
//! Old Hebrew PDFs (DOS and Windows-era word processors, typesetting systems, scanned-and-retyped
//! archives) often use a font laid out for a Hebrew code page: code `0xE0` draws alef, `0xE1` bet,
//! and so on. When such a font has no `/ToUnicode`, the PDF's declared encoding (WinAnsi,
//! Standard or none) reads those codes as Latin letters (`à`, `á`, …) or as nothing at all, so
//! Edit text showed the wrong characters, or replacement boxes, where the page shows Hebrew.
//!
//! [`LegacyEncoding`] is the user's choice of how to read the upper half (codes `0x80`–`0xFF`) of
//! such fonts. `Auto` applies a Hebrew code page only where the embedded font program itself shows
//! that it draws Hebrew letters at those codes ([`detect_hebrew`]); `Declared` always reads the
//! encoding the PDF names; the two Hebrew choices apply their code page to every simple font
//! without `/ToUnicode`. A `/ToUnicode` CMap is the producer's explicit statement and always wins.
//!
//! The tables are the published ISO/IEC 8859-8 and Windows code page 1255 assignments. Only the
//! code points they define are applied: a code the table leaves undefined keeps its declared
//! meaning, so a forced choice never erases text it has no better reading for.

use std::sync::atomic::{AtomicU8, Ordering};

/// How Edit text (and the engine's other content readers) interpret codes `0x80`–`0xFF` of simple
/// fonts that have no `/ToUnicode`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum LegacyEncoding {
    /// A Hebrew code page only where the embedded font program draws Hebrew at its codes.
    #[default]
    Auto,
    /// Always the encoding the PDF declares.
    Declared,
    /// ISO/IEC 8859-8 (Hebrew) for every simple font without `/ToUnicode`.
    Iso8859_8,
    /// Windows-1255 (Hebrew with points) for every simple font without `/ToUnicode`.
    Windows1255,
}

impl LegacyEncoding {
    pub const ALL: [Self; 4] = [Self::Auto, Self::Declared, Self::Iso8859_8, Self::Windows1255];

    /// The stable name used in settings files, the control channel and automation.
    pub fn name(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Declared => "declared",
            Self::Iso8859_8 => "iso-8859-8",
            Self::Windows1255 => "windows-1255",
        }
    }

    /// The English label shown in Preferences.
    pub fn label(self) -> &'static str {
        match self {
            Self::Auto => "Auto-detect",
            Self::Declared => "As declared in the PDF",
            Self::Iso8859_8 => "Hebrew (ISO 8859-8)",
            Self::Windows1255 => "Hebrew (Windows-1255)",
        }
    }

    /// The choice named `value` ([`Self::name`], case-insensitive; `off` means `declared`).
    pub fn parse(value: &str) -> Option<Self> {
        let value = value.trim();
        if value.eq_ignore_ascii_case("off") {
            return Some(Self::Declared);
        }
        Self::ALL.into_iter().find(|e| e.name().eq_ignore_ascii_case(value))
    }

    fn to_u8(self) -> u8 {
        match self {
            Self::Auto => 0,
            Self::Declared => 1,
            Self::Iso8859_8 => 2,
            Self::Windows1255 => 3,
        }
    }

    fn from_u8(v: u8) -> Self {
        match v {
            1 => Self::Declared,
            2 => Self::Iso8859_8,
            3 => Self::Windows1255,
            _ => Self::Auto,
        }
    }

    /// The table for codes `0x80`–`0xFF` (index = code − `0x80`; 0 = undefined).
    pub(crate) fn upper_half(self) -> Option<&'static [u32; 128]> {
        match self {
            Self::Iso8859_8 => Some(&ISO_8859_8),
            Self::Windows1255 => Some(&WINDOWS_1255),
            Self::Auto | Self::Declared => None,
        }
    }
}

/// The process-wide choice. It is an application preference: the desktop app sets it from
/// Preferences and the automation `text_encoding` tool sets it for a session.
static MODE: AtomicU8 = AtomicU8::new(0);

/// The current choice ([`LegacyEncoding::Auto`] until something sets another).
pub fn legacy_encoding() -> LegacyEncoding {
    LegacyEncoding::from_u8(MODE.load(Ordering::Relaxed))
}

/// Sets the process-wide choice. Text read from now on uses it; text already read (an open
/// paragraph editor) keeps the reading it was opened with.
pub fn set_legacy_encoding(mode: LegacyEncoding) {
    MODE.store(mode.to_u8(), Ordering::Relaxed);
}

/// Hebrew letters alef (U+05D0) … tav (U+05EA): codes `0xE0`–`0xFA` in both code pages.
const LETTERS: u32 = 27;
const FIRST_LETTER_CODE: u32 = 0xE0;
const ALEF: u32 = 0x05D0;
/// Hebrew points sheva (U+05B0) … sof pasuq (U+05C3): codes `0xC0`–`0xD3` in Windows-1255 only.
const POINTS: u32 = 20;
const FIRST_POINT_CODE: u32 = 0xC0;
const SHEVA: u32 = 0x05B0;

/// ISO/IEC 8859-8, codes `0x80`–`0xFF`.
static ISO_8859_8: [u32; 128] = iso_8859_8();
/// Windows code page 1255, codes `0x80`–`0xFF`.
static WINDOWS_1255: [u32; 128] = windows_1255();

/// The Latin-1 symbols both code pages keep at `0xA0`–`0xBF`, with their two Hebrew-specific
/// changes (multiplication and division signs at `0xAA` and `0xBA`).
const fn shared_symbols(mut t: [u32; 128]) -> [u32; 128] {
    let mut c = 0xA0;
    while c <= 0xBF {
        t[c - 0x80] = c as u32;
        c += 1;
    }
    t[0xAA - 0x80] = 0x00D7;
    t[0xBA - 0x80] = 0x00F7;
    t
}

const fn letters_and_marks(mut t: [u32; 128]) -> [u32; 128] {
    let mut k = 0;
    while k < LETTERS as usize {
        t[FIRST_LETTER_CODE as usize - 0x80 + k] = ALEF + k as u32;
        k += 1;
    }
    // Left-to-right and right-to-left marks.
    t[0xFD - 0x80] = 0x200E;
    t[0xFE - 0x80] = 0x200F;
    t
}

const fn iso_8859_8() -> [u32; 128] {
    let mut t = letters_and_marks(shared_symbols([0; 128]));
    // 0x80–0x9F are C1 controls: no meaning in a PDF string.
    t[0xA1 - 0x80] = 0;
    t[0xBF - 0x80] = 0;
    t[0xDF - 0x80] = 0x2017; // double low line
    t
}

const fn windows_1255() -> [u32; 128] {
    let mut t = letters_and_marks(shared_symbols([0; 128]));
    // The Windows punctuation block, as in Windows-1252 where both define a character.
    let punctuation: [(usize, u32); 23] = [
        (0x80, 0x20AC),
        (0x82, 0x201A),
        (0x83, 0x0192),
        (0x84, 0x201E),
        (0x85, 0x2026),
        (0x86, 0x2020),
        (0x87, 0x2021),
        (0x88, 0x02C6),
        (0x89, 0x2030),
        (0x8B, 0x2039),
        (0x91, 0x2018),
        (0x92, 0x2019),
        (0x93, 0x201C),
        (0x94, 0x201D),
        (0x95, 0x2022),
        (0x96, 0x2013),
        (0x97, 0x2014),
        (0x98, 0x02DC),
        (0x99, 0x2122),
        (0x9B, 0x203A),
        (0xA4, 0x20AA), // new sheqel sign, where ISO 8859-8 has the currency sign
        // Yiddish digraphs and Hebrew punctuation.
        (0xD4, 0x05F0),
        (0xD5, 0x05F1),
    ];
    let mut i = 0;
    while i < punctuation.len() {
        let (code, u) = punctuation[i];
        t[code - 0x80] = u;
        i += 1;
    }
    t[0xD6 - 0x80] = 0x05F2;
    t[0xD7 - 0x80] = 0x05F3; // geresh
    t[0xD8 - 0x80] = 0x05F4; // gershayim
    let mut k = 0;
    while k < POINTS as usize {
        t[FIRST_POINT_CODE as usize - 0x80 + k] = SHEVA + k as u32;
        k += 1;
    }
    t
}

/// Most cmap encoding records looked at in a font program (real fonts have a handful).
const MAX_CMAP_RECORDS: usize = 32;

/// The Hebrew code page a simple font's embedded TrueType/OpenType `program` is laid out for, if
/// the program itself shows it:
///
/// - its `OS/2` code page ranges declare Hebrew (1255) and not Latin 1 (1252), or
/// - its code-keyed cmap (Windows symbol `3,0` or Macintosh `1,x`) sends some code in
///   `0xE0`–`0xFA` to the glyph that its Unicode cmap (or its `uniXXXX` glyph name) gives the
///   Hebrew letter at that position, and none of them to the Latin letter there.
///
/// Windows-1255 is reported when the font also draws Hebrew points at `0xC0`–`0xD3` (or only the
/// `OS/2` evidence exists, which names that code page); otherwise ISO 8859-8. Malformed programs
/// give `None`.
pub fn detect_hebrew(program: &[u8]) -> Option<LegacyEncoding> {
    use skrifa::raw::TableProvider as _;
    use skrifa::raw::tables::cmap::PlatformId;
    let face = skrifa::FontRef::from_index(program, 0).ok()?;
    let cmap = face.cmap().ok();
    let mut by_code = Vec::new();
    let mut by_unicode = Vec::new();
    if let Some(cmap) = &cmap {
        let data = cmap.offset_data();
        for record in cmap.encoding_records().iter().take(MAX_CMAP_RECORDS) {
            let Ok(subtable) = record.subtable(data) else { continue };
            match (record.platform_id(), record.encoding_id()) {
                (PlatformId::Windows, 0) | (PlatformId::Macintosh, _) => by_code.push(subtable),
                (PlatformId::Unicode, _) | (PlatformId::Windows, 1 | 10) => by_unicode.push(subtable),
                _ => {}
            }
        }
    }
    let post = face.post().ok();
    // The glyph code `code` draws through a code-keyed subtable (symbol fonts use 0xF000 + code).
    let glyph_of =
        |code: u32| by_code.iter().find_map(|s| s.map_codepoint(code).or_else(|| s.map_codepoint(0xF000 + code))).filter(|g| g.to_u32() != 0);
    // Whether the font's own Unicode data says `glyph` is `u`.
    let is = |glyph: skrifa::GlyphId, u: u32| {
        by_unicode.iter().any(|s| s.map_codepoint(u) == Some(glyph))
            || post.as_ref().is_some_and(|p| {
                u16::try_from(glyph.to_u32())
                    .ok()
                    .and_then(|g| p.glyph_name(skrifa::raw::types::GlyphId16::new(g)))
                    .is_some_and(|name| name.strip_prefix("uni").and_then(|hex| u32::from_str_radix(hex, 16).ok()) == Some(u))
            })
    };
    let (mut hebrew, mut latin) = (0u32, 0u32);
    for k in 0..LETTERS {
        let Some(glyph) = glyph_of(FIRST_LETTER_CODE + k) else { continue };
        if is(glyph, ALEF + k) {
            hebrew += 1;
        } else if is(glyph, FIRST_LETTER_CODE + k) {
            // Latin-1 / WinAnsi put à…ú at these codes.
            latin += 1;
        }
    }
    if hebrew > 0 && latin == 0 {
        let points = (0..POINTS).any(|k| glyph_of(FIRST_POINT_CODE + k).is_some_and(|g| is(g, SHEVA + k)));
        return Some(if points { LegacyEncoding::Windows1255 } else { LegacyEncoding::Iso8859_8 });
    }
    // Hebrew (bit 5) without Latin 1 (bit 0): the face is laid out for code page 1255.
    let pages = face.os2().ok().and_then(|os2| os2.ul_code_page_range_1())?;
    (pages & (1 << 5) != 0 && pages & 1 == 0 && latin == 0).then_some(LegacyEncoding::Windows1255)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// One cmap subtable: platform, encoding and its `(code point, glyph)` pairs.
    pub(crate) type Subtable = (u16, u16, Vec<(u32, u16)>);

    /// A `cmap` table of format-4 subtables.
    pub(crate) fn cmap_table(subtables: &[Subtable]) -> Vec<u8> {
        fn format4(map: &[(u32, u16)]) -> Vec<u8> {
            let mut map: Vec<(u16, u16)> = map.iter().filter_map(|&(c, g)| Some((u16::try_from(c).ok()?, g))).collect();
            map.sort_unstable();
            let segs = map.len() + 1;
            let mut ends = Vec::new();
            let mut starts = Vec::new();
            let mut deltas = Vec::new();
            for &(c, g) in &map {
                ends.push(c);
                starts.push(c);
                deltas.push(g.wrapping_sub(c));
            }
            ends.push(0xFFFF);
            starts.push(0xFFFF);
            deltas.push(1);
            let mut t = Vec::new();
            let len = 16 + segs * 8;
            for v in [4u16, len as u16, 0, (segs * 2) as u16, 0, 0, 0] {
                t.extend_from_slice(&v.to_be_bytes());
            }
            for v in &ends {
                t.extend_from_slice(&v.to_be_bytes());
            }
            t.extend_from_slice(&0u16.to_be_bytes());
            for v in starts.iter().chain(&deltas) {
                t.extend_from_slice(&v.to_be_bytes());
            }
            for _ in 0..segs {
                t.extend_from_slice(&0u16.to_be_bytes());
            }
            t
        }
        let mut cmap = Vec::new();
        cmap.extend_from_slice(&0u16.to_be_bytes());
        cmap.extend_from_slice(&(subtables.len() as u16).to_be_bytes());
        let mut bodies = Vec::new();
        let mut offset = 4 + subtables.len() * 8;
        for (platform, encoding, map) in subtables {
            cmap.extend_from_slice(&platform.to_be_bytes());
            cmap.extend_from_slice(&encoding.to_be_bytes());
            cmap.extend_from_slice(&(offset as u32).to_be_bytes());
            let body = format4(map);
            offset += body.len();
            bodies.extend(body);
        }
        cmap.extend(bodies);
        cmap
    }

    /// A minimal sfnt with a `cmap` of format-4 subtables and an optional `OS/2` (version 1)
    /// carrying `pages` as its first code page range.
    pub(crate) fn sfnt(subtables: &[Subtable], pages: Option<u32>) -> Vec<u8> {
        let cmap = cmap_table(subtables);
        let mut tables: Vec<(&[u8; 4], Vec<u8>)> = vec![(b"cmap", cmap)];
        if let Some(pages) = pages {
            let mut os2 = vec![0u8; 86];
            os2[1] = 1; // version 1
            os2[78..82].copy_from_slice(&pages.to_be_bytes());
            tables.push((b"OS/2", os2));
        }
        tables.sort_by_key(|(tag, _)| **tag);
        let mut out = Vec::new();
        out.extend_from_slice(&0x0001_0000u32.to_be_bytes());
        out.extend_from_slice(&(tables.len() as u16).to_be_bytes());
        out.extend_from_slice(&[0; 6]);
        let mut at = 12 + tables.len() * 16;
        let mut data = Vec::new();
        for (tag, body) in &tables {
            out.extend_from_slice(*tag);
            out.extend_from_slice(&0u32.to_be_bytes());
            out.extend_from_slice(&(at as u32).to_be_bytes());
            out.extend_from_slice(&(body.len() as u32).to_be_bytes());
            let mut body = body.clone();
            while !body.len().is_multiple_of(4) {
                body.push(0);
            }
            at += body.len();
            data.extend(body);
        }
        out.extend(data);
        out
    }

    /// A legacy Hebrew symbol font: codes 0xE0.. draw glyphs 10.. through `3,0`, and a Unicode
    /// cmap names those glyphs alef...
    pub(crate) fn legacy_hebrew_font(letters: u32, points: bool) -> Vec<u8> {
        let by_code: Vec<_> = (0..letters).map(|k| (0xF0E0 + k, 10 + k as u16)).chain(points.then_some((0xF0C0, 60))).collect();
        let unicode: Vec<_> = (0..letters).map(|k| (ALEF + k, 10 + k as u16)).chain(points.then_some((SHEVA, 60))).collect();
        sfnt(&[(3, 0, by_code), (3, 1, unicode)], None)
    }

    #[test]
    fn tables_follow_the_published_code_pages() {
        let iso = |c: usize| ISO_8859_8[c - 0x80];
        let win = |c: usize| WINDOWS_1255[c - 0x80];
        assert_eq!((iso(0xE0), iso(0xEA), iso(0xFA)), (0x05D0, 0x05DA, 0x05EA));
        assert_eq!((win(0xE0), win(0xFA)), (0x05D0, 0x05EA));
        assert_eq!((iso(0xFB), iso(0xFC), iso(0xFD), iso(0xFE), iso(0xFF)), (0, 0, 0x200E, 0x200F, 0));
        assert_eq!((iso(0xAA), iso(0xBA), iso(0xA4), win(0xA4)), (0xD7, 0xF7, 0xA4, 0x20AA));
        assert_eq!((iso(0xA1), win(0xA1), iso(0xBF), win(0xBF)), (0, 0xA1, 0, 0xBF));
        assert_eq!((iso(0xDF), win(0xDF)), (0x2017, 0));
        assert_eq!((iso(0x80), iso(0x93), win(0x80), win(0x93), win(0x96)), (0, 0, 0x20AC, 0x201C, 0x2013));
        // Points sheva … sof pasuq, then the Yiddish digraphs and geresh/gershayim.
        assert_eq!((win(0xC0), win(0xC9), win(0xCA), win(0xD3)), (0x05B0, 0x05B9, 0x05BA, 0x05C3));
        assert_eq!((win(0xD4), win(0xD8), win(0xD9)), (0x05F0, 0x05F4, 0));
        assert!((0xC0..=0xDE).all(|c| iso(c) == 0), "ISO 8859-8 leaves 0xC0–0xDE undefined");
        // Every defined value is a real character.
        assert!(ISO_8859_8.iter().chain(&WINDOWS_1255).all(|&u| u == 0 || char::from_u32(u).is_some()));
    }

    #[test]
    fn names_round_trip_and_unknown_names_are_rejected() {
        for e in LegacyEncoding::ALL {
            assert_eq!(LegacyEncoding::parse(e.name()), Some(e));
            assert_eq!(LegacyEncoding::parse(&e.name().to_uppercase()), Some(e));
            assert_eq!(LegacyEncoding::from_u8(e.to_u8()), e);
        }
        assert_eq!(LegacyEncoding::parse("off"), Some(LegacyEncoding::Declared));
        assert_eq!(LegacyEncoding::parse("latin-9"), None);
        assert_eq!(LegacyEncoding::parse(""), None);
        assert_eq!(LegacyEncoding::from_u8(200), LegacyEncoding::Auto);
    }

    #[test]
    fn hebrew_fonts_are_detected_from_their_own_cmaps() {
        assert_eq!(detect_hebrew(&legacy_hebrew_font(27, false)), Some(LegacyEncoding::Iso8859_8));
        // A subset with only two letters still says so.
        assert_eq!(detect_hebrew(&legacy_hebrew_font(2, false)), Some(LegacyEncoding::Iso8859_8));
        // Points at 0xC0 make it Windows-1255.
        assert_eq!(detect_hebrew(&legacy_hebrew_font(27, true)), Some(LegacyEncoding::Windows1255));
        // A Mac code-keyed table counts too (no 0xF000 offset).
        let mac = sfnt(&[(1, 0, vec![(0xE0, 5), (0xE1, 6)]), (3, 1, vec![(ALEF, 5), (ALEF + 1, 6)])], None);
        assert_eq!(detect_hebrew(&mac), Some(LegacyEncoding::Iso8859_8));
    }

    #[test]
    fn latin_fonts_are_never_read_as_hebrew() {
        // A Latin-1 font: code 0xE0 draws the glyph its Unicode cmap calls à, even though the face
        // also has Hebrew letters elsewhere (Arial, Tahoma, David).
        let latin = sfnt(&[(1, 0, vec![(0xE0, 5), (0xE1, 6)]), (3, 1, vec![(0xE0, 5), (0xE1, 6), (ALEF, 40)])], None);
        assert_eq!(detect_hebrew(&latin), None);
        // A Unicode-only font has no code-keyed table to read codes through.
        assert_eq!(detect_hebrew(&sfnt(&[(3, 1, vec![(ALEF, 5)])], None)), None);
        // One Latin letter among Hebrew ones makes the evidence contradictory: no override.
        let mixed = sfnt(&[(3, 0, vec![(0xF0E0, 5), (0xF0E1, 6)]), (3, 1, vec![(ALEF, 5), (0xE1, 6)])], None);
        assert_eq!(detect_hebrew(&mixed), None);
        // Malformed programs are not fonts.
        for junk in [&b""[..], b"\0\x01\0\0", &[0xFF; 64]] {
            assert_eq!(detect_hebrew(junk), None);
        }
    }

    #[test]
    fn the_os2_code_page_declares_a_hebrew_only_face() {
        let symbol = vec![(0xF0E0, 5)];
        assert_eq!(detect_hebrew(&sfnt(&[(3, 0, symbol.clone())], Some(1 << 5))), Some(LegacyEncoding::Windows1255));
        // Latin 1 and Hebrew together is an ordinary multilingual face.
        assert_eq!(detect_hebrew(&sfnt(&[(3, 0, symbol.clone())], Some((1 << 5) | 1))), None);
        assert_eq!(detect_hebrew(&sfnt(&[(3, 0, symbol)], Some(1))), None);
    }

    #[test]
    fn hostile_cmaps_never_panic() {
        let mut font = legacy_hebrew_font(27, true);
        for i in 0..font.len() {
            let saved = font[i];
            for v in [0x00, 0xFF, 0x7F] {
                font[i] = v;
                let _ = detect_hebrew(&font);
            }
            font[i] = saved;
            let _ = detect_hebrew(font.get(..i).unwrap_or_default());
        }
    }
}
