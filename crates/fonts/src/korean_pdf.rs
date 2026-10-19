//! Embeds subsets of the Korean document font (NanumGothic, the craft-fonts `Kore` face) as PDF fonts.
//!
//! Lets appearance streams that could only use standard fonts (form field values, FreeText comments, visible
//! signatures) draw Hangul as real characters instead of `?`. The font is Type0 / CIDFontType2 / `Identity-H`
//! with `/ToUnicode`, so other viewers draw it as it is and can extract the text.
//!
//! - Subset: only the glyphs of the characters used (and the components of composite glyphs) are kept and
//!   renumbered (.notdef = 0, then 1… in the order of the used characters' original glyph ids, component-only glyphs after).
//!   CID = new glyph id (`/CIDToGIDMap /Identity`). The same character set always gives the same numbers.
//! - A font already in the document is never changed (safe for post-signature change checks and incremental saves).
//!   When more characters are needed a new font is made from the union. It is read back from the character list in the font dictionary's [`KOREAN_MARK`].
//! - Characters NanumGothic lacks are not embedded and draw as the `?` glyph (callers report them with
//!   [`KoreanFont::first_missing`]). `?` is always in the subset.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

use pdfcraft_cos::{Dict, Document, ObjRef, Object, PdfString, Stream};
use skrifa::{
    FontRef, MetadataProvider,
    instance::{LocationRef, Size},
    raw::types::Tag,
};

/// Private key in the (Type0) font dictionary marking a NanumGothic subset PdfCraft embedded, holding the characters it contains.
pub const KOREAN_MARK: &[u8] = b"PCKoreanText";

/// The most distinct characters one font holds (comfortably more than the 11,172 Hangul syllables).
const MAX_CHARS: usize = 16_384;

/// What the subset font draws character `c` as: tab and newline are drawn as a space.
fn drawn_as(c: char) -> char {
    if matches!(c, '\t' | '\n' | '\r') { ' ' } else { c }
}

/// Whether `s` must be drawn in NanumGothic: it has Hangul and this build has a Korean font.
pub fn korean_text(s: &str) -> bool {
    s.chars().any(crate::is_hangul) && crate::document_korean_font(false).is_some()
}

/// The Korean version of [`crate::first_undrawable`]: when `s` has Hangul and NanumGothic is there, the first character
/// NanumGothic lacks, otherwise the first character the standard fonts can't draw. Newlines and tabs don't count.
pub fn first_undrawable_korean(s: &str) -> Option<char> {
    if korean_text(s) { s.chars().find(|c| !matches!(c, '\n' | '\r' | '\t') && !crate::korean_has(*c)) } else { crate::first_undrawable(s) }
}

/// The width of `s` drawn in NanumGothic (Regular) at `size` points. Characters the font lacks count as `?`, tabs and newlines as spaces.
/// Used to break lines and size boxes before a font is put in the document (the same value as [`KoreanFont::width`]).
pub fn korean_text_width(s: &str, size: f64) -> f64 {
    let Some(font) = crate::document_korean_font(false).and_then(|f| FontRef::new(f.bytes).ok()) else { return crate::helvetica_width(s, size) };
    let loc = LocationRef::default();
    let upem = f64::from(font.metrics(Size::unscaled(), loc).units_per_em.max(1));
    let (charmap, advances) = (font.charmap(), font.glyph_metrics(Size::unscaled(), loc));
    let advance = |c: char| charmap.map(c).and_then(|g| advances.advance_width(g)).map(f64::from);
    let em: f64 = s.chars().map(|c| advance(drawn_as(c)).or_else(|| advance('?')).unwrap_or(0.0)).sum::<f64>() / upem;
    if em.is_finite() { em * size } else { 0.0 }
}

/// One NanumGothic subset font in the document: the reference to its (Type0) font dictionary and each character's CID and width.
#[derive(Clone, Debug, PartialEq)]
pub struct KoreanFont {
    /// The Type0 font dictionary. Resources point at it as `Object::Ref(obj)`.
    pub obj: ObjRef,
    /// Character → (CID, width per 1 em).
    cids: BTreeMap<char, (u16, f64)>,
}

/// The glyph layout for a character set: per character (original glyph, new glyph), and the base list in original glyph order.
struct Layout {
    chars: BTreeMap<char, u16>,
    base: Vec<u16>,
}

impl KoreanFont {
    /// Put a subset font holding the characters of `text` (those NanumGothic has) plus space and `?` in `doc`.
    /// Errors for a build without a Korean font, a font that doesn't allow embedding, or too many characters (the document is unchanged).
    pub fn embed(doc: &mut Document, text: &str) -> Result<Self, String> {
        let face = crate::document_korean_font(false).ok_or_else(|| {
            "Korean text needs PdfCraft's Korean font (NanumGothic), which this build doesn't include \
             (to build it in, set CRAFT_FONTS_DIR to a craft-fonts checkout that has a Kore face)"
                .to_string()
        })?;
        let bytes = face.bytes;
        // Check the embedding permission (OS/2 fsType), outlines and metrics before making any object.
        crate::embedding::validate(bytes)?;
        let font = FontRef::new(bytes).map_err(|e| e.to_string())?;
        let layout = layout(&font, text)?;
        let order = with_components(bytes, &layout.base)?;
        let mapped: Vec<(char, u16)> = layout.chars.iter().filter_map(|(&c, &old)| Some((c, new_gid(&layout.base, old)?))).collect();
        let program = subset_program(bytes, &order, &mapped)?;
        let loc = LocationRef::default();
        let m = font.metrics(Size::unscaled(), loc);
        let upem = f64::from(m.units_per_em.max(1));
        let advances = font.glyph_metrics(Size::unscaled(), loc);
        let width = |gid: u16| {
            let w = f64::from(advances.advance_width(skrifa::GlyphId::new(u32::from(gid))).unwrap_or(0.0)) / upem;
            if w.is_finite() { w.clamp(0.0, 16.0) } else { 0.0 }
        };
        let cids: BTreeMap<char, (u16, f64)> =
            layout.chars.iter().filter_map(|(&c, &old)| Some((c, (new_gid(&layout.base, old)?, width(old))))).collect();

        // The font program and descriptor.
        let family: String = face.family.chars().filter(char::is_ascii_alphanumeric).collect();
        let chars: String = layout.chars.keys().collect();
        let name = format!("{}+{family}", subset_tag(&chars));
        let mut file = Dict::new();
        file.set(b"Length1".to_vec(), Object::Int(i64::try_from(program.len()).map_err(|_| "font program too large")?));
        let file = doc.add(Object::Stream(Stream::flate(file, &program)));
        let scale = 1000.0 / upem;
        let mut descriptor = Dict::new();
        descriptor.set(b"Type".to_vec(), Object::name("FontDescriptor"));
        descriptor.set(b"FontName".to_vec(), Object::name(&name));
        descriptor.set(b"FontFamily".to_vec(), Object::String(PdfString::literal(face.family.as_bytes().to_vec())));
        descriptor.set(b"Flags".to_vec(), Object::Int(4));
        descriptor.set(b"ItalicAngle".to_vec(), Object::Real(0.0));
        descriptor.set(b"Ascent".to_vec(), Object::Real(f64::from(m.ascent) * scale));
        descriptor.set(b"Descent".to_vec(), Object::Real(f64::from(m.descent) * scale));
        descriptor.set(b"CapHeight".to_vec(), Object::Real(f64::from(m.cap_height.unwrap_or(m.ascent)) * scale));
        descriptor.set(b"StemV".to_vec(), Object::Int(80));
        let bounds = m.bounds.ok_or("the Korean font has no outline bounds")?;
        descriptor.set(
            b"FontBBox".to_vec(),
            Object::Array([bounds.x_min, bounds.y_min, bounds.x_max, bounds.y_max].map(|v| Object::Real(f64::from(v) * scale)).to_vec()),
        );
        descriptor.set(b"FontFile2".to_vec(), Object::Ref(file));
        let descriptor = doc.add(Object::Dict(descriptor));

        // The CID font: CID = new glyph id, widths in new-id order 1….
        let mut system = Dict::new();
        system.set(b"Registry".to_vec(), Object::String(PdfString::literal(b"Adobe".to_vec())));
        system.set(b"Ordering".to_vec(), Object::String(PdfString::literal(b"Identity".to_vec())));
        system.set(b"Supplement".to_vec(), Object::Int(0));
        let widths: Vec<Object> = order.iter().skip(1).map(|&g| Object::Real((width(g) * 1000.0 * 1000.0).round() / 1000.0)).collect();
        let mut cid = Dict::new();
        cid.set(b"Type".to_vec(), Object::name("Font"));
        cid.set(b"Subtype".to_vec(), Object::name("CIDFontType2"));
        cid.set(b"BaseFont".to_vec(), Object::name(&name));
        cid.set(b"CIDSystemInfo".to_vec(), Object::Dict(system));
        cid.set(b"FontDescriptor".to_vec(), Object::Ref(descriptor));
        cid.set(b"CIDToGIDMap".to_vec(), Object::name("Identity"));
        cid.set(b"DW".to_vec(), Object::Int(1000));
        cid.set(b"W".to_vec(), Object::Array(vec![Object::Int(1), Object::Array(widths)]));
        let cid = doc.add(Object::Dict(cid));

        // ToUnicode: for each CID, the first character drawn with that glyph (other characters sharing the glyph read as that one).
        let mut unicode: BTreeMap<u16, char> = BTreeMap::new();
        for (&c, &(code, _)) in &cids {
            unicode.entry(code).or_insert(c);
        }
        let mut cmap = String::from(
            "/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n/CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n\
             /CMapName /PCKoreanUnicode def\n/CMapType 2 def\n1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n",
        );
        let pairs: Vec<(u16, char)> = unicode.into_iter().collect();
        for chunk in pairs.chunks(100) {
            let _ = writeln!(cmap, "{} beginbfchar", chunk.len());
            for (code, c) in chunk {
                let hex: String = c.encode_utf16(&mut [0; 2]).iter().map(|u| format!("{u:04X}")).collect();
                let _ = writeln!(cmap, "<{code:04X}> <{hex}>");
            }
            cmap.push_str("endbfchar\n");
        }
        cmap.push_str("endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n");
        let to_unicode = doc.add(Object::Stream(Stream::flate(Dict::new(), cmap.as_bytes())));

        let mut top = Dict::new();
        top.set(b"Type".to_vec(), Object::name("Font"));
        top.set(b"Subtype".to_vec(), Object::name("Type0"));
        top.set(b"BaseFont".to_vec(), Object::name(&name));
        top.set(b"Encoding".to_vec(), Object::name("Identity-H"));
        top.set(b"DescendantFonts".to_vec(), Object::Array(vec![Object::Ref(cid)]));
        top.set(b"ToUnicode".to_vec(), Object::Ref(to_unicode));
        top.set(KOREAN_MARK.to_vec(), PdfString::text(&chars));
        let obj = doc.add(Object::Dict(top));
        Ok(Self { obj, cids })
    }

    /// The font, when `obj` is one [`Self::embed`] put in (the CIDs are recomputed from its character list).
    pub fn from_existing(doc: &Document, obj: ObjRef) -> Option<Self> {
        let d = doc.get(obj);
        let d = d.as_dict()?;
        if d.name(b"Subtype") != Some(b"Type0") {
            return None;
        }
        let chars = doc.resolve(d.get(KOREAN_MARK)?).as_string()?.to_text();
        let face = crate::document_korean_font(false)?;
        let font = FontRef::new(face.bytes).ok()?;
        let layout = layout(&font, &chars).ok()?;
        let loc = LocationRef::default();
        let upem = f64::from(font.metrics(Size::unscaled(), loc).units_per_em.max(1));
        let advances = font.glyph_metrics(Size::unscaled(), loc);
        let cids = layout
            .chars
            .iter()
            .filter_map(|(&c, &old)| {
                let w = f64::from(advances.advance_width(skrifa::GlyphId::new(u32::from(old))).unwrap_or(0.0)) / upem;
                Some((c, (new_gid(&layout.base, old)?, if w.is_finite() { w.clamp(0.0, 16.0) } else { 0.0 })))
            })
            .collect::<BTreeMap<char, (u16, f64)>>();
        // The program embedded in the document must agree with the CIDs recomputed from this build's face: a
        // craft-fonts revision with another glyph order, or another producer's dictionary carrying the key, would
        // otherwise get strings that pick the wrong glyphs (the subset's own cmap maps BMP characters to its glyph ids).
        let program = crate::pdf::embedded_sfnt(doc, d)?;
        let sub = FontRef::new(&program).ok()?;
        let map = sub.charmap();
        let agrees = |c: char, cid: u16| u32::from(c) > 0xFFFF || map.map(c).map(|g| g.to_u32()) == Some(u32::from(cid));
        if !cids.iter().all(|(&c, &(cid, _))| agrees(c, cid)) {
            return None;
        }
        Some(Self { obj, cids })
    }

    /// The characters in this font (space and `?` included).
    pub fn chars(&self) -> String {
        self.cids.keys().collect()
    }

    /// Whether every character of `s` is in this font (newlines and tabs count as spaces).
    pub fn covers(&self, s: &str) -> bool {
        s.chars().all(|c| self.cids.contains_key(&drawn_as(c)))
    }

    /// The first character of `s` this font lacks and draws as `?`.
    pub fn first_missing(&self, s: &str) -> Option<char> {
        s.chars().find(|c| !self.cids.contains_key(&drawn_as(*c)))
    }

    /// The bytes that draw `s` (two-byte CIDs, wrapped with [`crate::literal`] for `Tj`). Missing characters are `?`.
    pub fn encode(&self, s: &str) -> Vec<u8> {
        let fallback = self.cids.get(&'?').map_or(0, |c| c.0);
        s.chars().flat_map(|c| self.cids.get(&drawn_as(c)).map_or(fallback, |c| c.0).to_be_bytes()).collect()
    }

    /// The width of `s` at `size` points.
    pub fn width(&self, s: &str, size: f64) -> f64 {
        let fallback = self.cids.get(&'?').map_or(0.5, |c| c.1);
        s.chars().map(|c| self.cids.get(&drawn_as(c)).map_or(fallback, |c| c.1)).sum::<f64>() * size
    }
}

/// The glyph layout for the characters of `text` NanumGothic has, plus space and `?`.
fn layout(font: &FontRef, text: &str) -> Result<Layout, String> {
    let charmap = font.charmap();
    let mut chars = BTreeMap::new();
    for c in text.chars().map(drawn_as).chain([' ', '?']) {
        if chars.contains_key(&c) {
            continue;
        }
        if let Some(g) = charmap.map(c).and_then(|g| u16::try_from(g.to_u32()).ok()).filter(|g| *g != 0) {
            chars.insert(c, g);
            if chars.len() > MAX_CHARS {
                return Err(format!("the text has more than {MAX_CHARS} different characters for one Korean font"));
            }
        }
    }
    let base: BTreeSet<u16> = chars.values().copied().collect();
    Ok(Layout { chars, base: base.into_iter().collect() })
}

/// The new id (1…) of original glyph `old` in the base list.
fn new_gid(base: &[u16], old: u16) -> Option<u16> {
    base.binary_search(&old).ok().and_then(|i| u16::try_from(i.checked_add(1)?).ok())
}

/// The subset tag for a character set (six capital letters, FNV-1a).
fn subset_tag(chars: &str) -> String {
    let mut h: u32 = 0x811c_9dc5;
    for b in chars.bytes() {
        h = (h ^ u32::from(b)).wrapping_mul(0x0100_0193);
    }
    (0..6)
        .map(|i| {
            let v = (h >> (i * 5)) % 26;
            // 0–25, so always one capital letter.
            char::from(b'A' + v as u8)
        })
        .collect()
}

// ── TrueType subset ─────────────────────────────────────────────────────────────────────────────

fn table<'a>(bytes: &'a [u8], tag: &[u8; 4]) -> Option<&'a [u8]> {
    FontRef::new(bytes).ok()?.table_data(Tag::new(tag)).map(|d| d.as_bytes())
}

fn u16_at(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_be_bytes([*b.get(at)?, *b.get(at.checked_add(1)?)?]))
}

fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes([*b.get(at)?, *b.get(at + 1)?, *b.get(at + 2)?, *b.get(at.checked_add(3)?)?]))
}

/// The `glyf` data of original glyph `gid`.
fn glyph_data<'a>(glyf: &'a [u8], loca: &[u8], long: bool, gid: u16) -> Option<&'a [u8]> {
    let g = usize::from(gid);
    let (start, end) = if long {
        (u32_at(loca, g * 4)? as usize, u32_at(loca, g * 4 + 4)? as usize)
    } else {
        (usize::from(u16_at(loca, g * 2)?) * 2, usize::from(u16_at(loca, g * 2 + 2)?) * 2)
    };
    if end < start {
        return None;
    }
    glyf.get(start..end)
}

/// The components of a composite glyph: (where the glyph id is written, that id).
fn components(data: &[u8]) -> Vec<(usize, u16)> {
    let mut out = Vec::new();
    if u16_at(data, 0).is_none_or(|n| (n as i16) >= 0) {
        return out;
    }
    let mut p = 10usize;
    // Components are read only within the glyph's data, and their number is bounded.
    for _ in 0..1024 {
        let (Some(flags), Some(gid)) = (u16_at(data, p), u16_at(data, p + 2)) else { break };
        out.push((p + 2, gid));
        p += 4 + if flags & 0x0001 != 0 { 4 } else { 2 };
        p += if flags & 0x0008 != 0 {
            2
        } else if flags & 0x0040 != 0 {
            4
        } else if flags & 0x0080 != 0 {
            8
        } else {
            0
        };
        if flags & 0x0020 == 0 {
            break;
        }
    }
    out
}

struct Tables<'a> {
    glyf: &'a [u8],
    loca: &'a [u8],
    long: bool,
    glyphs: u16,
}

fn tables(bytes: &[u8]) -> Result<Tables<'_>, String> {
    let bad = || "the Korean font's TrueType tables can't be read".to_string();
    let head = table(bytes, b"head").ok_or_else(bad)?;
    let maxp = table(bytes, b"maxp").ok_or_else(bad)?;
    Ok(Tables {
        glyf: table(bytes, b"glyf").ok_or_else(bad)?,
        loca: table(bytes, b"loca").ok_or_else(bad)?,
        long: u16_at(head, 50).ok_or_else(bad)? != 0,
        glyphs: u16_at(maxp, 4).ok_or_else(bad)?,
    })
}

/// The new glyph order (original ids): .notdef, the base list, then glyphs used only as components (in the order found).
fn with_components(bytes: &[u8], base: &[u16]) -> Result<Vec<u16>, String> {
    let t = tables(bytes)?;
    let mut order: Vec<u16> = std::iter::once(0).chain(base.iter().copied()).collect();
    let mut seen: BTreeSet<u16> = order.iter().copied().collect();
    let mut i = 0;
    while let Some(&g) = order.get(i) {
        for (_, c) in glyph_data(t.glyf, t.loca, t.long, g).map(components).unwrap_or_default() {
            if c < t.glyphs && seen.insert(c) {
                order.push(c);
            }
        }
        i += 1;
        if order.len() > MAX_CHARS * 2 {
            return Err("the Korean font's composite glyphs are too deep".into());
        }
    }
    Ok(order)
}

fn checksum(data: &[u8]) -> u32 {
    data.chunks(4).fold(0u32, |sum, c| {
        let mut w = [0u8; 4];
        for (d, s) in w.iter_mut().zip(c) {
            *d = *s;
        }
        sum.wrapping_add(u32::from_be_bytes(w))
    })
}

/// A TrueType program with only the glyphs of `order` (new id → original id). The hinting tables (`cvt `, `fpgm`, `prep`)
/// and `OS/2` are kept as they are, `post` is rewritten as version 3.0 without glyph names and `cmap` as format 4 with
/// only `mapped` (character → new id), for editors and viewers that read only the font program; `name` and the rest are dropped.
fn subset_program(bytes: &[u8], order: &[u16], mapped: &[(char, u16)]) -> Result<Vec<u8>, String> {
    let bad = || "the Korean font's TrueType tables can't be read".to_string();
    let t = tables(bytes)?;
    let count = u16::try_from(order.len()).map_err(|_| "too many glyphs for one font")?;
    let index: BTreeMap<u16, u16> = order.iter().enumerate().filter_map(|(new, &old)| Some((old, u16::try_from(new).ok()?))).collect();

    // glyf and loca: renumber component ids and pad to 4 bytes.
    let mut glyf = Vec::new();
    let mut loca = Vec::with_capacity((order.len() + 1) * 4);
    for &old in order {
        loca.extend_from_slice(&u32::try_from(glyf.len()).map_err(|_| "font program too large")?.to_be_bytes());
        let mut data = glyph_data(t.glyf, t.loca, t.long, old).unwrap_or_default().to_vec();
        for (at, c) in components(&data) {
            let new = index.get(&c).copied().unwrap_or(0).to_be_bytes();
            if let Some(slot) = data.get_mut(at..at + 2) {
                slot.copy_from_slice(&new);
            }
        }
        glyf.extend_from_slice(&data);
        while glyf.len() % 4 != 0 {
            glyf.push(0);
        }
    }
    loca.extend_from_slice(&u32::try_from(glyf.len()).map_err(|_| "font program too large")?.to_be_bytes());

    // hmtx: (advance, left side bearing) for every glyph.
    let hhea = table(bytes, b"hhea").ok_or_else(bad)?;
    let hmtx_src = table(bytes, b"hmtx").ok_or_else(bad)?;
    let metrics_count = usize::from(u16_at(hhea, 34).ok_or_else(bad)?.max(1));
    let mut hmtx = Vec::with_capacity(order.len() * 4);
    for &old in order {
        let g = usize::from(old);
        let (adv, lsb) = if g < metrics_count {
            (u16_at(hmtx_src, g * 4), u16_at(hmtx_src, g * 4 + 2))
        } else {
            (u16_at(hmtx_src, (metrics_count - 1) * 4), u16_at(hmtx_src, metrics_count * 4 + (g - metrics_count) * 2))
        };
        hmtx.extend_from_slice(&adv.unwrap_or(0).to_be_bytes());
        hmtx.extend_from_slice(&lsb.unwrap_or(0).to_be_bytes());
    }
    let mut hhea = hhea.to_vec();
    hhea.get_mut(34..36).ok_or_else(bad)?.copy_from_slice(&count.to_be_bytes());
    let mut maxp = table(bytes, b"maxp").ok_or_else(bad)?.to_vec();
    maxp.get_mut(4..6).ok_or_else(bad)?.copy_from_slice(&count.to_be_bytes());
    let mut head = table(bytes, b"head").ok_or_else(bad)?.to_vec();
    head.get_mut(8..12).ok_or_else(bad)?.copy_from_slice(&[0; 4]);
    head.get_mut(50..52).ok_or_else(bad)?.copy_from_slice(&1u16.to_be_bytes());
    let post = table(bytes, b"post").and_then(|p| p.get(..32)).map(|p| {
        let mut p = p.to_vec();
        if let Some(version) = p.get_mut(..4) {
            version.copy_from_slice(&0x0003_0000u32.to_be_bytes());
        }
        p
    });

    let mut list: Vec<([u8; 4], Vec<u8>)> =
        vec![(*b"glyf", glyf), (*b"head", head), (*b"hhea", hhea), (*b"hmtx", hmtx), (*b"loca", loca), (*b"maxp", maxp)];
    for tag in [b"cvt ", b"fpgm", b"prep", b"OS/2"] {
        if let Some(d) = table(bytes, tag) {
            list.push((*tag, d.to_vec()));
        }
    }
    if let Some(p) = post {
        list.push((*b"post", p));
    }
    list.push((*b"cmap", cmap_table(mapped)));
    list.sort_by_key(|(tag, _)| *tag);

    // The table directory and tables (4-byte aligned), then head's checkSumAdjustment.
    let n = u16::try_from(list.len()).map_err(|_| "too many tables")?;
    let mut entry_selector = 0u16;
    while (2u16 << entry_selector) <= n {
        entry_selector += 1;
    }
    let search_range = (1u16 << entry_selector) * 16;
    let mut out = Vec::new();
    out.extend_from_slice(&0x0001_0000u32.to_be_bytes());
    out.extend_from_slice(&n.to_be_bytes());
    out.extend_from_slice(&search_range.to_be_bytes());
    out.extend_from_slice(&entry_selector.to_be_bytes());
    out.extend_from_slice(&(n * 16).saturating_sub(search_range).to_be_bytes());
    let mut offset = 12 + list.len() * 16;
    let mut head_at = None;
    for (tag, data) in &list {
        out.extend_from_slice(tag);
        out.extend_from_slice(&checksum(data).to_be_bytes());
        out.extend_from_slice(&u32::try_from(offset).map_err(|_| "font program too large")?.to_be_bytes());
        out.extend_from_slice(&u32::try_from(data.len()).map_err(|_| "font program too large")?.to_be_bytes());
        if tag == b"head" {
            head_at = Some(offset);
        }
        offset += data.len().div_ceil(4) * 4;
    }
    for (_, data) in &list {
        out.extend_from_slice(data);
        while out.len() % 4 != 0 {
            out.push(0);
        }
    }
    let adjust = 0xB1B0_AFBAu32.wrapping_sub(checksum(&out));
    if let Some(slot) = head_at.and_then(|at| out.get_mut(at + 8..at + 12)) {
        slot.copy_from_slice(&adjust.to_be_bytes());
    }
    Ok(out)
}

/// A `cmap` (Windows Unicode BMP, format 4) mapping the BMP characters of `mapped` to new glyph ids: one segment per character.
fn cmap_table(mapped: &[(char, u16)]) -> Vec<u8> {
    let mut segs: Vec<(u16, u16)> =
        mapped.iter().filter_map(|&(c, g)| Some((u16::try_from(u32::from(c)).ok().filter(|c| *c != 0xFFFF)?, g))).collect();
    segs.sort_unstable();
    segs.dedup_by_key(|s| s.0);
    // Format 4 requires the final 0xFFFF segment. The segment count is the character cap (16,384) + 1, so it fits a u16.
    let count = u16::try_from(segs.len() + 1).unwrap_or(u16::MAX);
    let mut selector = 0u16;
    while (2u32 << selector) <= u32::from(count) {
        selector += 1;
    }
    let range = (1u16 << selector).saturating_mul(2);
    let mut sub = Vec::new();
    let length = 16usize + usize::from(count) * 8;
    for v in
        [4u16, u16::try_from(length).unwrap_or(u16::MAX), 0, count.saturating_mul(2), range, selector, count.saturating_mul(2).saturating_sub(range)]
    {
        sub.extend_from_slice(&v.to_be_bytes());
    }
    let ends = segs.iter().map(|s| s.0).chain([0xFFFF]);
    for e in ends {
        sub.extend_from_slice(&e.to_be_bytes());
    }
    sub.extend_from_slice(&0u16.to_be_bytes());
    for s in segs.iter().map(|s| s.0).chain([0xFFFF]) {
        sub.extend_from_slice(&s.to_be_bytes());
    }
    for d in segs.iter().map(|&(c, g)| g.wrapping_sub(c)).chain([1]) {
        sub.extend_from_slice(&d.to_be_bytes());
    }
    for _ in 0..count {
        sub.extend_from_slice(&0u16.to_be_bytes());
    }
    let mut out = Vec::with_capacity(12 + sub.len());
    for v in [0u16, 1, 3, 1] {
        out.extend_from_slice(&v.to_be_bytes());
    }
    out.extend_from_slice(&12u32.to_be_bytes());
    out.extend(sub);
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf::Metrics;
    use skrifa::raw::TableProvider;

    /// A NanumGothic subset holds only the characters used, reads back with the same CIDs, and has the right ToUnicode and widths.
    #[test]
    fn korean_subset_font_round_trips_text_and_widths() {
        let mut doc = Document::new_empty();
        if crate::document_korean_font(false).is_none() {
            eprintln!("skipping the Korean subset checks: built without a craft-fonts Kore face (set CRAFT_FONTS_DIR)");
            assert!(KoreanFont::embed(&mut doc, "한글").is_err());
            assert!(!korean_text("한글"));
            assert_eq!(first_undrawable_korean("한글"), Some('한'));
            return;
        }
        // Embedding permission: NanumGothic allows editable embedding.
        let face = crate::document_korean_font(false).unwrap();
        crate::embedding::validate(face.bytes).unwrap();
        let text = "검토자 박검토 (A-1)\n둘째 줄";
        let k = KoreanFont::embed(&mut doc, text).unwrap();
        assert!(k.covers(text) && k.covers("?"));
        assert!(!k.covers("힣"));
        assert_eq!(k.first_missing("검힣"), Some('힣'));
        assert_eq!(first_undrawable_korean("한글 😀"), Some('😀'));
        assert!(korean_text("한") && !korean_text("abc"));
        // Reading the document's font through the PDF side (Metrics) gives back the same text.
        let font = doc.dict(&Object::Ref(k.obj)).unwrap();
        let m = Metrics::from_dict(&doc, &font);
        let codes = k.encode("박검토 A-1");
        assert_eq!(m.decode(&codes), "박검토 A-1");
        let pdf_w: f64 = m.codes(&codes).iter().map(|(c, _)| m.width(*c)).sum();
        assert!((pdf_w - k.width("박검토 A-1", 1.0)).abs() < 1e-3, "{pdf_w}");
        // The subset program is readable TrueType with about as many glyphs as characters used.
        let program = crate::pdf::embedded_sfnt(&doc, &font).unwrap();
        let sub = FontRef::new(&program).unwrap();
        let glyphs = sub.maxp().unwrap().num_glyphs();
        assert!(usize::from(glyphs) <= text.chars().count() + 4, "{glyphs}");
        assert!(program.len() < 60_000, "{}", program.len());
        assert_eq!(sub.charmap().map('박').map(|g| g.to_u32()), Some(u32::from(k.cids[&'박'].0)), "the subset's own cmap");
        let outlines = sub.outline_glyphs();
        let (cid_bak, _) = k.cids[&'박'];
        assert!(outlines.get(skrifa::GlyphId::new(u32::from(cid_bak))).is_some());
        // Reading it again gives the same font.
        assert_eq!(KoreanFont::from_existing(&doc, k.obj), Some(k.clone()));
        // A character list that disagrees with the embedded program isn't trusted (its CIDs would pick the wrong glyphs).
        let mut other = doc.clone();
        other
            .update_dict(k.obj, |d| {
                d.set(KOREAN_MARK.to_vec(), PdfString::text(&text.replace('박', "글")));
            })
            .unwrap();
        assert_eq!(KoreanFont::from_existing(&other, k.obj), None);
        // Missing characters are drawn as `?`.
        assert_eq!(k.encode("힣"), k.encode("?"));
        assert!((korean_text_width("박검토 A-1", 10.0) - k.width("박검토 A-1", 10.0)).abs() < 1e-6);
    }

    #[test]
    fn subset_tags_are_six_capitals() {
        let t = subset_tag("가나다");
        assert_eq!(t.len(), 6);
        assert!(t.bytes().all(|b| b.is_ascii_uppercase()));
        assert_ne!(t, subset_tag("가나라"));
    }
}
