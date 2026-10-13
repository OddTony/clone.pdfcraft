//! Type 3 fonts drawn from craft-fonts outlines, for text the standard fonts can't encode.
//!
//! Each character gets one glyph procedure built from a craft-fonts face, and each font carries a
//! ToUnicode map, so the text extracts, copies and searches as typed; no font program is embedded.
//! [`type3_glyph`] and [`type3_font`] are the building blocks (the editor shards and names its
//! fonts itself). [`type3_plan`] picks one face for a whole text and decides once which characters
//! it draws, so what a caller measures, what it reports as undrawable and what it draws agree.

use std::collections::HashMap;
use std::sync::Arc;

use pdfcraft_cos::{Dict, Object, PdfString, Stream};

use crate::{CraftFont, GlyphError, GlyphOutline, japanese_glyph_from};

/// Codes 1..=240 of one Type 3 font.
pub const MAX_TYPE3_GLYPHS: usize = 240;
/// The `/Name` of generated fonts whose glyphs come from the Japanese faces. The editor recognises
/// its fonts by it, so a replacement split across several fonts reads back as one line.
pub const JAPANESE_TYPE3_NAME: &str = "PCJapanese";
/// Entries per `beginbfchar` block: a CMap allows at most 100 (Adobe Technical Note #5411).
const BFCHAR_BLOCK: usize = 100;
/// Side bearing (em) given to a respaced glyph on each side.
const SIDE_BEARING: f64 = 0.05;

/// One glyph of a generated font: its character, advance (em, to 1/1000) and glyph procedure.
#[derive(Clone, Debug, PartialEq)]
pub struct Type3Glyph {
    pub ch: char,
    pub width: f64,
    /// The glyph procedure (`d1`, then the filled outline), in the font's 1/1000 em units.
    pub program: Vec<u8>,
}

/// The glyph of `ch` in `face`, as a Type 3 glyph procedure.
///
/// Japanese faces draw Greek and Cyrillic one em wide (as in JIS X 0208) and have no proportional
/// metrics for them, so those letters get their ink width plus a side bearing instead.
pub fn type3_glyph(face: &CraftFont, ch: char) -> Result<Arc<Type3Glyph>, GlyphError> {
    japanese_glyph_from(face, ch).map(|outline| Arc::new(glyph(ch, &outline)))
}

fn glyph(ch: char, outline: &GlyphOutline) -> Type3Glyph {
    let (dx, width) = proportional(ch, outline);
    // Rounded once, so the procedure's `d1` and the font's `/Widths` state the same advance.
    let width = (width * 1000.0).round() / 1000.0;
    let scale = 1000.0;
    // d1 is `wx wy llx lly urx ury` (ISO 32000-2 §9.6.4) with a box enclosing the glyph;
    // Acrobat draws a bullet in place of a glyph whose d1 is malformed.
    let b = if outline.contours.is_empty() { [0.0; 4] } else { outline.bbox };
    let mut program = format!(
        "{} 0 {} {} {} {} d1\n",
        num(width * scale),
        num(((b[0] + dx) * scale).floor()),
        num((b[1] * scale).floor()),
        num(((b[2] + dx) * scale).ceil()),
        num((b[3] * scale).ceil())
    )
    .into_bytes();
    for contour in &outline.contours {
        let Some(first) = contour.first() else { continue };
        program.extend_from_slice(format!("{} {} m\n", num((first[0] + dx) * scale), num(first[1] * scale)).as_bytes());
        for p in contour.iter().skip(1) {
            program.extend_from_slice(format!("{} {} l\n", num((p[0] + dx) * scale), num(p[1] * scale)).as_bytes());
        }
        program.extend_from_slice(b"h\n");
    }
    program.extend_from_slice(b"f\n");
    Type3Glyph { ch, width, program }
}

/// Shift and advance (em) for a glyph: Greek and Cyrillic are respaced to their ink, see
/// [`type3_glyph`]; everything else keeps the face's own advance.
fn proportional(ch: char, glyph: &GlyphOutline) -> (f64, f64) {
    let respace = matches!(ch, '\u{0370}'..='\u{03FF}' | '\u{0400}'..='\u{052F}' | '\u{1F00}'..='\u{1FFF}');
    let ink = glyph.bbox[2] - glyph.bbox[0];
    if !respace || glyph.contours.is_empty() || !(ink > 0.0 && ink < glyph.width) {
        return (0.0, glyph.width);
    }
    (SIDE_BEARING - glyph.bbox[0], ink + 2.0 * SIDE_BEARING)
}

fn num(v: f64) -> String {
    if v.fract() == 0.0 { format!("{v:.0}") } else { format!("{v:.4}").trim_end_matches('0').trim_end_matches('.').to_string() }
}

/// A Type 3 font whose codes 1, 2, … show `glyphs` (the first [`MAX_TYPE3_GLYPHS`] of them),
/// named `name` (`/Name`) and described as `face`.
///
/// The glyph streams, the ToUnicode stream and the font descriptor must be indirect objects
/// (ISO 32000-2 §7.3.8, §9.6.5); each is handed to `add`, which returns what to store in its
/// place: `doc.add` for a font that goes straight into a document, or the object itself when the
/// caller makes it indirect later.
pub fn type3_font(face: &CraftFont, name: &str, glyphs: &[Arc<Type3Glyph>], mut add: impl FnMut(Object) -> Object) -> Dict {
    let glyphs = glyphs.get(..glyphs.len().min(MAX_TYPE3_GLYPHS)).unwrap_or_default();
    let mut charprocs = Dict::new();
    let mut differences = vec![Object::Int(1)];
    let mut widths = Vec::with_capacity(glyphs.len());
    for (i, glyph) in glyphs.iter().enumerate() {
        let glyph_name = format!("g{:02X}", i + 1);
        charprocs.set(glyph_name.as_bytes().to_vec(), add(Object::Stream(Stream::flate(Dict::new(), &glyph.program))));
        differences.push(Object::name(&glyph_name));
        widths.push(Object::Real((glyph.width * 1000.0).round()));
    }
    let chars: Vec<char> = glyphs.iter().map(|g| g.ch).collect();
    let mut encoding = Dict::new();
    encoding.set(b"Type".to_vec(), Object::name("Encoding"));
    encoding.set(b"Differences".to_vec(), Object::Array(differences));
    let mut descriptor = Dict::new();
    descriptor.set(b"Type".to_vec(), Object::name("FontDescriptor"));
    descriptor.set(b"FontName".to_vec(), Object::name(&format!("{}-{}", face.family, face.style).replace(' ', "")));
    descriptor.set(b"FontFamily".to_vec(), Object::String(PdfString::literal(face.family.as_bytes().to_vec())));
    descriptor.set(b"Flags".to_vec(), Object::Int(if face.family.contains("Mincho") { 6 } else { 4 }));
    descriptor.set(b"ItalicAngle".to_vec(), Object::Int(0));
    let mut font = Dict::new();
    font.set(b"Type".to_vec(), Object::name("Font"));
    font.set(b"Subtype".to_vec(), Object::name("Type3"));
    font.set(b"Name".to_vec(), Object::name(name));
    // PDF 1.7 tables 5.9 and 5.19: a Type 3 descriptor is indirect; Ascent/Descent may be omitted.
    font.set(b"FontDescriptor".to_vec(), add(Object::Dict(descriptor)));
    font.set(b"FontBBox".to_vec(), Object::Array(vec![Object::Int(0), Object::Int(-300), Object::Int(1000), Object::Int(1000)]));
    font.set(
        b"FontMatrix".to_vec(),
        Object::Array(vec![Object::Real(0.001), Object::Int(0), Object::Int(0), Object::Real(0.001), Object::Int(0), Object::Int(0)]),
    );
    font.set(b"FirstChar".to_vec(), Object::Int(1));
    // At most MAX_TYPE3_GLYPHS (240), so the conversion can't fail.
    font.set(b"LastChar".to_vec(), Object::Int(i64::try_from(glyphs.len()).unwrap_or(0)));
    font.set(b"Widths".to_vec(), Object::Array(widths));
    font.set(b"Encoding".to_vec(), Object::Dict(encoding));
    font.set(b"CharProcs".to_vec(), Object::Dict(charprocs));
    font.set(b"ToUnicode".to_vec(), add(Object::Stream(Stream::flate(Dict::new(), to_unicode(&chars).as_bytes()))));
    font
}

/// A ToUnicode CMap for codes 1, 2, … showing `chars`, in blocks of at most [`BFCHAR_BLOCK`].
fn to_unicode(chars: &[char]) -> String {
    let mut cmap = String::from(
        "/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n/CMapType 2 def\n1 begincodespacerange\n<01> <FF>\nendcodespacerange\n",
    );
    for (block, chunk) in chars.chunks(BFCHAR_BLOCK).enumerate() {
        cmap.push_str(&format!("{} beginbfchar\n", chunk.len()));
        for (i, ch) in chunk.iter().enumerate() {
            let code = block * BFCHAR_BLOCK + i + 1;
            cmap.push_str(&format!("<{code:02X}> <{}>\n", unicode_hex(*ch)));
        }
        cmap.push_str("endbfchar\n");
    }
    cmap.push_str("endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n");
    cmap
}

fn unicode_hex(ch: char) -> String {
    let mut units = [0u16; 2];
    ch.encode_utf16(&mut units).iter().map(|u| format!("{u:04X}")).collect()
}

/// Characters a one-glyph-per-character font can't set right: combining marks, joiners and
/// variation selectors, direction controls, and the scripts whose letters change shape or move
/// with their neighbours (Hebrew, Arabic and the rest of the right-to-left blocks, Indic, Thai,
/// Lao, Tibetan, Myanmar, Khmer). A [`type3_plan`] never draws them, so they are reported rather
/// than set apart or out of order.
pub fn needs_shaping(c: char) -> bool {
    matches!(c as u32,
        0x0300..=0x036F // combining diacritical marks
        | 0x0590..=0x08FF // Hebrew, Arabic, Syriac, Thaana, NKo, Samaritan, Mandaic
        | 0x0900..=0x0DFF // Indic
        | 0x0E00..=0x0FFF // Thai, Lao, Tibetan
        | 0x1000..=0x109F // Myanmar
        | 0x1780..=0x17FF // Khmer
        | 0x1AB0..=0x1AFF | 0x1DC0..=0x1DFF | 0x20D0..=0x20FF | 0xFE20..=0xFE2F // more combining marks
        | 0x200C..=0x200F | 0x202A..=0x202E | 0x2066..=0x2069 // joiners and direction controls
        | 0x3099..=0x309A // combining kana voicing marks
        | 0xFB1D..=0xFDFF | 0xFE70..=0xFEFF // Hebrew and Arabic presentation forms
        | 0xFE00..=0xFE0F | 0xE0100..=0xE01EF // variation selectors
        | 0x10800..=0x10FFF | 0x1E800..=0x1EFFF // historic right-to-left scripts
    )
}

/// The face chosen for a text, and the glyphs it draws. Cloning shares the glyphs.
#[derive(Clone)]
pub struct Type3Plan {
    face: &'static CraftFont,
    /// Drawable characters in order of first use; glyph `i` is code `i % 240 + 1` of font `i / 240`.
    glyphs: Vec<Arc<Type3Glyph>>,
    index: HashMap<char, usize>,
}

impl std::fmt::Debug for Type3Plan {
    /// The face and the glyph count, not the glyphs.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Type3Plan").field("face", &self.face.name()).field("glyphs", &self.glyphs.len()).finish()
    }
}

impl Type3Plan {
    /// The face the glyphs come from.
    pub fn face(&self) -> &'static CraftFont {
        self.face
    }

    /// Whether the plan's fonts draw `ch`.
    pub fn can_draw(&self, ch: char) -> bool {
        self.index.contains_key(&ch)
    }

    /// Advance of `ch` in em, as the font's `/Widths` states it. `None` if it isn't drawn.
    pub fn width(&self, ch: char) -> Option<f64> {
        self.glyphs.get(*self.index.get(&ch)?).map(|g| g.width)
    }

    /// Which font (0-based) and which code in it draw `ch`.
    pub fn code(&self, ch: char) -> Option<(usize, u8)> {
        let i = *self.index.get(&ch)?;
        Some((i / MAX_TYPE3_GLYPHS, u8::try_from(i % MAX_TYPE3_GLYPHS + 1).ok()?))
    }

    /// How many fonts [`Type3Plan::fonts`] makes.
    pub fn font_count(&self) -> usize {
        self.glyphs.len().div_ceil(MAX_TYPE3_GLYPHS)
    }

    /// The plan's fonts, in order (see [`type3_font`] for `add`).
    pub fn fonts(&self, name: &str, mut add: impl FnMut(Object) -> Object) -> Vec<Dict> {
        self.glyphs.chunks(MAX_TYPE3_GLYPHS).map(|chunk| type3_font(self.face, name, chunk, &mut add)).collect()
    }
}

/// Pick the face for `chars` from `faces` (best first) and decide which of them it draws.
///
/// The face is the first that draws every character, else the first, which then draws what it can
/// (as the editor chooses). Duplicates are ignored, and so are characters that [`needs_shaping`].
/// At most `max_fonts` fonts of [`MAX_TYPE3_GLYPHS`] are planned, so hostile text can't make the
/// work unbounded: characters past that are not drawn. `None` without a face (no craft-fonts).
pub fn type3_plan(faces: &[&'static CraftFont], chars: impl IntoIterator<Item = char>, max_fonts: usize) -> Option<Type3Plan> {
    let first = *faces.first()?;
    let limit = MAX_TYPE3_GLYPHS.saturating_mul(max_fonts.max(1));
    let mut unique = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for ch in chars {
        if unique.len() >= limit {
            break;
        }
        if !needs_shaping(ch) && seen.insert(ch) {
            unique.push(ch);
        }
    }
    // Each face's glyphs are read once; the first face's are kept in case no face has them all.
    let mut chosen = None;
    for face in faces.iter().copied() {
        let glyphs: Vec<_> = unique.iter().map(|ch| type3_glyph(face, *ch)).collect();
        if glyphs.iter().all(Result::is_ok) {
            chosen = Some((face, glyphs));
            break;
        }
        chosen.get_or_insert((face, glyphs));
    }
    let (face, glyphs) = chosen.unwrap_or((first, Vec::new()));
    let mut plan = Type3Plan { face, glyphs: Vec::new(), index: HashMap::new() };
    for (ch, glyph) in unique.into_iter().zip(glyphs) {
        if let Ok(glyph) = glyph {
            plan.index.insert(ch, plan.glyphs.len());
            plan.glyphs.push(glyph);
        }
    }
    Some(plan)
}

#[cfg(test)]
mod tests {
    use super::*;

    static JPAN: &[&str] = &["Jpan"];

    fn japanese_faces() -> Vec<&'static CraftFont> {
        crate::document_japanese_fonts_for_style(false, false)
    }

    /// The faces come from craft-fonts, an optional build input.
    fn without_craft_fonts(test: &str) -> bool {
        if !japanese_faces().is_empty() {
            return false;
        }
        eprintln!("skipping {test}: built without craft-fonts (set CRAFT_FONTS_DIR to run it)");
        true
    }

    fn glyph(ch: char) -> Arc<Type3Glyph> {
        Arc::new(Type3Glyph { ch, width: 0.5, program: b"500 0 0 0 0 0 d1\nf\n".to_vec() })
    }

    fn tounicode(font: &Dict) -> String {
        let Some(Object::Stream(tu)) = font.get(b"ToUnicode") else { panic!("ToUnicode is a stream") };
        String::from_utf8(tu.decoded().unwrap_or_default()).unwrap_or_default()
    }

    #[test]
    fn no_plan_without_a_face() {
        assert!(type3_plan(&[], "日本語".chars(), 4).is_none());
    }

    #[test]
    fn a_font_has_indirect_parts_codes_from_one_and_a_tounicode() {
        let face = CraftFont { family: "Fake Mincho", style: "Regular", scripts: JPAN, bytes: b"" };
        let mut added = 0;
        let font = type3_font(&face, JAPANESE_TYPE3_NAME, &[glyph('日'), glyph('本'), glyph('😀')], |o| {
            added += 1;
            o
        });
        // Three glyph streams, the descriptor and the ToUnicode stream.
        assert_eq!(added, 5);
        assert_eq!(font.name(b"Subtype"), Some(&b"Type3"[..]));
        assert_eq!(font.name(b"Name"), Some(JAPANESE_TYPE3_NAME.as_bytes()));
        assert_eq!(font.int(b"LastChar"), Some(3));
        let procs = font.get(b"CharProcs").and_then(Object::as_dict).cloned().unwrap_or_default();
        let Some(Object::Stream(g)) = procs.get(b"g01") else { panic!("g01 is a stream") };
        assert_eq!(g.dict.name(b"Filter"), Some(&b"FlateDecode"[..]));
        assert_eq!(g.decoded().ok().as_deref(), Some(&b"500 0 0 0 0 0 d1\nf\n"[..]));
        let cmap = tounicode(&font);
        assert!(cmap.contains("<01> <65E5>") && cmap.contains("<02> <672C>") && cmap.contains("<03> <D83DDE00>"), "{cmap}");
        let Some(Object::Dict(fd)) = font.get(b"FontDescriptor") else { panic!("a descriptor") };
        assert_eq!(fd.int(b"Flags"), Some(6), "a Mincho is serif");
    }

    #[test]
    fn a_full_font_splits_its_tounicode_into_blocks_of_at_most_100() {
        let face = CraftFont { family: "Fake", style: "Regular", scripts: JPAN, bytes: b"" };
        let glyphs: Vec<_> = (0x4E00u32..0x4E00 + 300).filter_map(char::from_u32).map(glyph).collect();
        let font = type3_font(&face, JAPANESE_TYPE3_NAME, &glyphs, |o| o);
        assert_eq!(font.int(b"LastChar"), Some(240), "a font takes at most 240 glyphs");
        let cmap = tounicode(&font);
        let blocks: Vec<usize> = cmap.lines().filter_map(|l| l.strip_suffix(" beginbfchar")).filter_map(|n| n.parse().ok()).collect();
        assert_eq!(blocks, [100, 100, 40], "{cmap}");
        assert!(cmap.contains("<F0> <4EEF>"), "codes run on across blocks");
    }

    #[test]
    fn shaped_scripts_and_marks_need_shaping_and_cjk_does_not() {
        for c in ['\u{0301}', 'ש', 'م', 'क', 'ก', '\u{200D}', '\u{FE0F}', '\u{3099}'] {
            assert!(needs_shaping(c), "U+{:04X}", c as u32);
        }
        for c in ['日', 'あ', 'ア', 'Ж', 'λ', 'ř', '한', '、', '\u{3000}'] {
            assert!(!needs_shaping(c), "U+{:04X}", c as u32);
        }
    }

    #[test]
    fn codes_follow_first_use_and_ignore_duplicates() {
        if without_craft_fonts("codes_follow_first_use_and_ignore_duplicates") {
            return;
        }
        let plan = type3_plan(&japanese_faces(), "日本日語".chars(), 4).expect("plan");
        assert_eq!(plan.code('日'), Some((0, 1)));
        assert_eq!(plan.code('本'), Some((0, 2)));
        assert_eq!(plan.code('語'), Some((0, 3)));
        assert_eq!(plan.code('x'), None);
        assert_eq!(plan.font_count(), 1);
        let w = plan.width('日').expect("width");
        assert!((w - 1.0).abs() < 0.05, "an ideograph is about one em wide: {w}");
    }

    #[test]
    fn characters_no_face_has_or_that_need_shaping_are_not_drawn() {
        if without_craft_fonts("characters_no_face_has_or_that_need_shaping_are_not_drawn") {
            return;
        }
        // U+10FFFE is a noncharacter no face has; U+0301 is a combining accent.
        let plan = type3_plan(&japanese_faces(), "日\u{10fffe}e\u{301}".chars(), 4).expect("plan");
        assert!(plan.can_draw('日'));
        for c in ['\u{10fffe}', '\u{301}'] {
            assert!(!plan.can_draw(c) && plan.width(c).is_none() && plan.code(c).is_none());
        }
    }

    #[test]
    fn a_face_with_every_glyph_is_preferred() {
        if without_craft_fonts("a_face_with_every_glyph_is_preferred") {
            return;
        }
        let text = "ΕλληνικάПриветDvořák日本";
        let faces = crate::document_japanese_fonts_for_style(true, false);
        let plan = type3_plan(&faces, text.chars(), 4).expect("plan");
        if let Some(covering) = faces.iter().find(|f| text.chars().all(|c| type3_glyph(f, c).is_ok())) {
            assert!(std::ptr::eq(plan.face(), *covering), "the first face that has everything is used");
            assert!(text.chars().all(|c| plan.can_draw(c)));
        }
    }

    #[test]
    fn greek_and_cyrillic_are_set_proportionally() {
        if without_craft_fonts("greek_and_cyrillic_are_set_proportionally") {
            return;
        }
        let plan = type3_plan(&japanese_faces(), "ιи日".chars(), 4).expect("plan");
        for ch in ['ι', 'и'] {
            if let Some(w) = plan.width(ch) {
                assert!(w < 1.0, "{ch} is respaced to its ink width, not a full em: {w}");
            }
        }
    }

    #[test]
    fn a_plan_splits_into_fonts_of_240_and_stops_at_its_limit() {
        if without_craft_fonts("a_plan_splits_into_fonts_of_240_and_stops_at_its_limit") {
            return;
        }
        // 0x4E00.. is a run of common ideographs.
        let text: Vec<char> = (0x4E00u32..0x4E00 + 600).filter_map(char::from_u32).collect();
        let plan = type3_plan(&japanese_faces(), text.iter().copied(), 2).expect("plan");
        assert!(plan.font_count() <= 2);
        let drawn = text.iter().filter(|c| plan.can_draw(**c)).count();
        assert!(drawn <= 2 * MAX_TYPE3_GLYPHS && drawn > MAX_TYPE3_GLYPHS, "{drawn}");
        assert!(!plan.can_draw(text[599]), "past the limit nothing is drawn");
        if let Some(c) = text.iter().copied().filter(|c| plan.can_draw(*c)).nth(MAX_TYPE3_GLYPHS) {
            assert_eq!(plan.code(c), Some((1, 1)), "the 241st glyph is code 1 of the second font");
        }
        let fonts = plan.fonts(JAPANESE_TYPE3_NAME, |o| o);
        assert_eq!(fonts.len(), plan.font_count());
        assert!(fonts.iter().all(|f| f.int(b"LastChar").is_some_and(|n| n <= 240)));
    }
}
