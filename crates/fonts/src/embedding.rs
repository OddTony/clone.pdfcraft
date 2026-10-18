//! Unicode embedding of an explicitly supplied, embeddable TrueType face. Font files are
//! document inputs or the optional craft-fonts build input, never system-interface fallbacks.

use std::fmt::Write as _;

use pdfcraft_cos::{Dict, Document, Object, PdfString, Stream};
use skrifa::{
    FontRef, MetadataProvider,
    instance::{LocationRef, Size},
    raw::TableProvider,
};

/// Compare PDF subset/PostScript names with the full face's family name.
pub fn face_key(name: &str) -> String {
    name.rsplit('+').next().unwrap_or(name).chars().filter(char::is_ascii_alphanumeric).flat_map(char::to_lowercase).collect()
}

/// Full, approved faces of the source family. This never silently selects another family.
pub fn matching_faces(name: &str) -> Vec<&'static crate::CraftFont> {
    let key = face_key(name);
    crate::CRAFT_FONTS.iter().filter(|f| face_key(f.family) == key || face_key(&f.name()) == key).collect()
}

/// Whether a complete font program can show every non-newline character.
pub fn covers(bytes: &[u8], text: &str) -> bool {
    FontRef::new(bytes).is_ok_and(|f| text.chars().all(|c| matches!(c, '\n' | '\r') || f.charmap().map(c).is_some_and(|g| g.to_u32() != 0)))
}

/// Names come from the program, never a caller's guessed family.
pub fn names(bytes: &[u8]) -> Result<(String, String), String> {
    use skrifa::string::StringId;
    validate(bytes)?;
    let f = FontRef::new(bytes).map_err(|e| e.to_string())?;
    let name = |id| {
        f.localized_strings(id)
            .find(|s| s.language() == Some("en"))
            .or_else(|| f.localized_strings(id).next())
            .map(|s| s.to_string())
            .filter(|s| !s.is_empty() && s.len() <= 128)
    };
    let family = name(StringId::FAMILY_NAME).ok_or("the font has no family name")?;
    let postscript = name(StringId::POSTSCRIPT_NAME).unwrap_or_else(|| family.replace(' ', ""));
    Ok((family, postscript))
}

/// A selected external font is document input. Require its own open-licence declaration;
/// never turn a proprietary system-interface fallback into an exported PDF font.
pub fn validate_document_input(bytes: &[u8]) -> Result<(), String> {
    use skrifa::string::StringId;
    let (family, _) = names(bytes)?;
    let key = face_key(&family);
    if ["sourcehan", "sourceserif", "sourcesans", "notosanscjk", "notoserifcjk"].iter().any(|n| key.starts_with(n)) {
        return Err("this font family is excluded by the project's asset policy".into());
    }
    let f = FontRef::new(bytes).map_err(|e| e.to_string())?;
    let attribution: String =
        [StringId::COPYRIGHT_NOTICE, StringId::MANUFACTURER].into_iter().flat_map(|id| f.localized_strings(id)).map(|s| s.to_string()).collect();
    if attribution.to_ascii_lowercase().contains("adobe") {
        return Err("Adobe-authored font assets are excluded by the project's asset policy".into());
    }
    let license: String = f.localized_strings(StringId::LICENSE_DESCRIPTION).map(|s| s.to_string()).collect();
    if !["SIL OPEN FONT", "SIL Open Font", "Apache License", "Ubuntu Font Licence", "Bitstream Vera", "MIT License"]
        .iter()
        .any(|l| license.contains(l))
    {
        return Err("select an openly licensed font whose name table declares its licence (OFL, Apache, Ubuntu, Bitstream Vera or MIT)".into());
    }
    Ok(())
}

/// Validate before allocating any PDF objects, so an invalid font cannot partly mutate a doc.
pub fn validate(bytes: &[u8]) -> Result<(), String> {
    if bytes.len() > 16 << 20 {
        return Err("the selected font exceeds 16 MiB".into());
    }
    let font = FontRef::new(bytes).map_err(|_| "select a standalone TrueType/OpenType font (not a font collection)")?;
    if font.fvar().is_ok() {
        return Err("select a static instance of this variable font before embedding".into());
    }
    if (font.glyf().is_err() && font.cff().is_err()) || font.head().is_err() || font.hhea().is_err() || font.hmtx().is_err() {
        return Err("the selected font has no usable TrueType/CFF outlines and metrics".into());
    }
    if font.os2().is_ok_and(|t| t.fs_type() & (0x0002 | 0x0004 | 0x0200) != 0) {
        return Err("this font's embedding permissions prohibit editable outline embedding".into());
    }
    let m = font.metrics(Size::unscaled(), LocationRef::default());
    if m.units_per_em == 0 || font.charmap().mappings().next().is_none() {
        return Err("the selected font has no Unicode mapping or valid units per em".into());
    }
    if font.charmap().mappings().take(65_535).count() >= 65_535 {
        return Err("the selected font has too many Unicode mappings for one PDF font".into());
    }
    if font.charmap().mappings().any(|(_, g)| g.to_u32() >= u32::from(m.glyph_count)) {
        return Err("the selected font maps Unicode to a glyph outside its program".into());
    }
    Ok(())
}

fn stream(doc: &mut Document, bytes: Vec<u8>) -> Object {
    Object::Ref(doc.add(Object::Stream(Stream::from_raw(Dict::new(), bytes))))
}

/// Embed the complete font once and provide separate CIDs for Unicode aliases. /ToUnicode,
/// /CIDToGIDMap and widths agree, including supplementary characters, so reopening needs no
/// installed font. Keeping the full program also lets subsequent edits add other glyphs.
pub fn embed(doc: &mut Document, fonts: &mut Dict, bytes: &[u8], family: &str) -> Result<String, String> {
    validate(bytes)?;
    let font = FontRef::new(bytes).map_err(|e| e.to_string())?;
    let mappings: Vec<_> = font
        .charmap()
        .mappings()
        .filter(|(_, gid)| gid.to_u32() > 0)
        .filter_map(|(cp, gid)| char::from_u32(cp).map(|c| (c.to_string(), gid)))
        .collect();
    embed_mapping(doc, fonts, bytes, family, &mappings)
}

/// Explicit glyph mapping for shaped text. A CID can represent a contextual glyph or a
/// ligature, while its ToUnicode retains the logical cluster. The full source program stays exact.
pub fn embed_mapping(
    doc: &mut Document,
    fonts: &mut Dict,
    bytes: &[u8],
    family: &str,
    mappings: &[(String, skrifa::GlyphId)],
) -> Result<String, String> {
    validate(bytes)?;
    if mappings.len() > 65_534 || mappings.iter().any(|(s, _)| s.len() > 4096) {
        return Err("shaped font mapping exceeds its budget".into());
    }
    let font = FontRef::new(bytes).map_err(|e| e.to_string())?;
    let cff = font.glyf().is_err();
    // For a CID-keyed CFF the CIDs of the CIDFontType0 are the CIDs of the CFF charset. Writing GIDs as
    // CIDs draws every glyph whose charset entry is not the identity as a different glyph. A name-keyed
    // CFF has CID = GID (ISO 32000-2 §9.7.4.2).
    let charset = if cff { cid_charset(&font)? } else { None };
    let loc = LocationRef::default();
    let m = font.metrics(Size::unscaled(), loc);
    let scale = 1000.0 / f64::from(m.units_per_em);
    let advances = font.glyph_metrics(Size::unscaled(), loc);
    // The CMap program carries the same CIDSystemInfo and CMapName as its stream dictionary (ISO 32000-2
    // §9.7.5.3).
    let (registry, ordering, supplement) = IDENTITY;
    let mut cid_encoding = format!(
        "/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n/CIDSystemInfo << /Registry ({registry}) /Ordering ({ordering}) /Supplement {supplement} >> def\n/CMapName /{GLYPH_CMAP} def\n/CMapType 1 def\n1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n",
    );
    let mut cff_widths = std::collections::BTreeMap::new();
    let mut gids = vec![0, 0];
    let mut widths = Vec::with_capacity(mappings.len());
    let mut cmap = String::from(
        "/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n/CMapName /PCUnicode def\n/CMapType 2 def\n1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n",
    );
    for (chunk_index, chunk) in mappings.chunks(100).enumerate() {
        let _ = writeln!(cmap, "{} beginbfchar", chunk.len());
        let _ = writeln!(cid_encoding, "{} begincidchar", chunk.len());
        for (offset, (unicode, gid)) in chunk.iter().enumerate() {
            let cid = u16::try_from(chunk_index.saturating_mul(100).saturating_add(offset).saturating_add(1)).map_err(|_| "font mapping limit")?;
            let glyph = u16::try_from(gid.to_u32()).map_err(|_| "font glyph limit")?;
            if glyph == 0 || u32::from(glyph) >= u32::from(m.glyph_count) {
                return Err("font contains an unavailable shaped glyph".into());
            }
            let width = advances.advance_width(*gid).unwrap_or(0.0) as f64 * scale;
            if !width.is_finite() || !(0.0..=16_000.0).contains(&width) {
                return Err("font contains an invalid advance".into());
            }
            gids.extend_from_slice(&glyph.to_be_bytes());
            widths.push(Object::Real(width));
            let font_cid = match &charset {
                Some(cs) => cs.string_id(*gid).map(|sid| sid.to_u16()).map_err(|_| "font glyph has no CID in its charset")?,
                None => glyph,
            };
            cff_widths.insert(font_cid, width);
            let _ = writeln!(cid_encoding, "<{cid:04X}> {font_cid}");
            let hex: String = unicode.encode_utf16().map(|u| format!("{u:04X}")).collect();
            let _ = writeln!(cmap, "<{cid:04X}> <{hex}>");
        }
        cmap.push_str("endbfchar\n");
        cid_encoding.push_str("endcidchar\n");
    }
    cmap.push_str("endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n");
    cid_encoding.push_str("endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n");
    let name = family.chars().filter(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_')).collect::<String>();
    if name.is_empty() || name.len() > 128 {
        return Err("the font family name is invalid".into());
    }
    let glyph_map = if cff { cid_encoding.as_bytes() } else { gids.as_slice() };
    if let Some(resource) = same_resource(doc, fonts, &name, bytes, glyph_map, cmap.as_bytes()) {
        return Ok(resource);
    }
    let mut descriptor = Dict::new();
    descriptor.set(b"Type".to_vec(), Object::name("FontDescriptor"));
    descriptor.set(b"FontName".to_vec(), Object::name(&name));
    descriptor.set(b"FontFamily".to_vec(), Object::String(PdfString::literal(family.as_bytes().to_vec())));
    descriptor.set(b"Flags".to_vec(), Object::Int(4));
    descriptor.set(b"ItalicAngle".to_vec(), Object::Real(f64::from(m.italic_angle)));
    descriptor.set(b"Ascent".to_vec(), Object::Real(f64::from(m.ascent) * scale));
    descriptor.set(b"Descent".to_vec(), Object::Real(f64::from(m.descent) * scale));
    descriptor.set(b"CapHeight".to_vec(), Object::Real(f64::from(m.cap_height.unwrap_or(m.ascent)) * scale));
    descriptor.set(b"StemV".to_vec(), Object::Int(80));
    let bounds = m.bounds.ok_or("font has no outline bounds")?;
    descriptor.set(
        b"FontBBox".to_vec(),
        Object::Array([bounds.x_min, bounds.y_min, bounds.x_max, bounds.y_max].map(|v| Object::Real(f64::from(v) * scale)).to_vec()),
    );
    // Reuse the program another resource of this page already has, else one registered on another
    // page; source subset programs stay untouched.
    let existing = written(doc, fonts).find(|w| w.has_program(doc, bytes)).map(|w| w.program).or_else(|| {
        doc.scan_objects().take(8192).find_map(|(r, o)| match o.as_ref() {
            Object::Stream(s) if s.dict.get(b"PCFullFont").is_some() && s.raw.as_ref() == bytes => Some(Object::Ref(r)),
            _ => None,
        })
    });
    let program = existing.unwrap_or_else(|| {
        let mut d = Dict::new();
        d.set(b"Length1".to_vec(), Object::Int(bytes.len() as i64));
        if cff {
            d.set(b"Subtype".to_vec(), Object::name("OpenType"));
        }
        d.set(b"PCFullFont".to_vec(), Object::Bool(true));
        Object::Ref(doc.add(Object::Stream(Stream::from_raw(d, bytes.to_vec()))))
    });
    descriptor.set(if cff { b"FontFile3".to_vec() } else { b"FontFile2".to_vec() }, program);
    let desc = Object::Ref(doc.add(Object::Dict(descriptor)));
    let system = identity_system();
    let mut cid = Dict::new();
    cid.set(b"Type".to_vec(), Object::name("Font"));
    cid.set(b"Subtype".to_vec(), Object::name(if cff { "CIDFontType0" } else { "CIDFontType2" }));
    cid.set(b"BaseFont".to_vec(), Object::name(&name));
    cid.set(b"CIDSystemInfo".to_vec(), Object::Dict(system));
    cid.set(b"FontDescriptor".to_vec(), desc);
    if cff {
        cid.set(
            b"W".to_vec(),
            Object::Array(cff_widths.into_iter().flat_map(|(gid, w)| [Object::Int(i64::from(gid)), Object::Array(vec![Object::Real(w)])]).collect()),
        );
    } else {
        cid.set(b"CIDToGIDMap".to_vec(), stream(doc, gids));
        cid.set(b"W".to_vec(), Object::Array(vec![Object::Int(1), Object::Array(widths)]));
    }
    let descendant = Object::Ref(doc.add(Object::Dict(cid)));
    let mut top = Dict::new();
    top.set(b"Type".to_vec(), Object::name("Font"));
    top.set(b"Subtype".to_vec(), Object::name("Type0"));
    top.set(b"BaseFont".to_vec(), Object::name(&name));
    let encoding = if cff {
        // An embedded CMap stream requires /Type /CMap, /CMapName and /CIDSystemInfo (ISO 32000-2
        // Table 118). Without them Acrobat rejects the whole page ("error parsing a form, Type 3 font
        // or pattern").
        let mut d = Dict::new();
        d.set(b"Type".to_vec(), Object::name("CMap"));
        d.set(b"CMapName".to_vec(), Object::name(GLYPH_CMAP));
        d.set(b"CIDSystemInfo".to_vec(), Object::Dict(identity_system()));
        Object::Ref(doc.add(Object::Stream(Stream::from_raw(d, cid_encoding.into_bytes()))))
    } else {
        Object::name("Identity-H")
    };
    top.set(b"Encoding".to_vec(), encoding);
    top.set(b"DescendantFonts".to_vec(), Object::Array(vec![descendant]));
    top.set(b"ToUnicode".to_vec(), stream(doc, cmap.into_bytes()));
    let mut index = 0usize;
    while fonts.contains(format!("PCFull{index}").as_bytes()) {
        index = index.saturating_add(1);
    }
    let resource = format!("PCFull{index}");
    fonts.set(resource.as_bytes().to_vec(), Object::Ref(doc.add(Object::Dict(top))));
    Ok(resource)
}

/// Name of the embedded glyph CMap of a CFF embedding.
const GLYPH_CMAP: &str = "PCGlyphs";
/// Registry, ordering and supplement of `Adobe-Identity-0`.
const IDENTITY: (&str, &str, i64) = ("Adobe", "Identity", 0);

/// `Adobe-Identity-0` CIDSystemInfo.
fn identity_system() -> Dict {
    let (registry, ordering, supplement) = IDENTITY;
    let mut system = Dict::new();
    system.set(b"Registry".to_vec(), Object::String(PdfString::literal(registry.as_bytes().to_vec())));
    system.set(b"Ordering".to_vec(), Object::String(PdfString::literal(ordering.as_bytes().to_vec())));
    system.set(b"Supplement".to_vec(), Object::Int(supplement));
    system
}

/// The charset (GID → CID) of a CID-keyed CFF, or `None` for a name-keyed CFF (CID = GID).
fn cid_charset<'a>(font: &FontRef<'a>) -> Result<Option<skrifa::raw::ps::cff::charset::Charset<'a>>, String> {
    let data = font.table_data(skrifa::Tag::new(b"CFF ")).ok_or("the font has no CFF table")?;
    let cff = skrifa::raw::ps::cff::CffFontRef::new_cff(data.as_bytes(), 0, None).map_err(|e| format!("invalid CFF table: {e}"))?;
    if !cff.is_cid() {
        return Ok(None);
    }
    cff.charset().map(Some).ok_or_else(|| "the CID-keyed CFF table has no valid charset".into())
}

/// The decoded data of the stream `object` refers to.
fn stream_data(doc: &Document, object: Option<&Object>) -> Option<Vec<u8>> {
    match doc.resolve(object?).as_ref() {
        Object::Stream(s) => s.decoded_within(17 << 20).ok(),
        _ => None,
    }
}

/// A `PCFull` resource that [`embed_mapping`] wrote.
struct Written {
    resource: String,
    top: Dict,
    cid: Dict,
    /// The reference to its full program.
    program: Object,
}

impl Written {
    /// Whether its program is `bytes`. The length it was written with is compared before decoding.
    fn has_program(&self, doc: &Document, bytes: &[u8]) -> bool {
        let Object::Stream(s) = doc.resolve(&self.program).as_ref().clone() else { return false };
        s.dict.get(b"PCFullFont").is_some()
            && s.dict.get(b"Length1").and_then(|l| doc.resolve(l).as_int()) == i64::try_from(bytes.len()).ok()
            && s.decoded_within(17 << 20).is_ok_and(|data| data == bytes)
    }
}

/// The `PCFull` resources in `fonts`, in order.
fn written<'a>(doc: &'a Document, fonts: &'a Dict) -> impl Iterator<Item = Written> + 'a {
    fonts.iter().filter(|(key, _)| key.starts_with(b"PCFull")).take(256).filter_map(move |(key, value)| {
        let top = doc.dict(value)?;
        let cid = doc.dict(doc.resolve(top.get(b"DescendantFonts")?).as_array()?.first()?)?;
        let descriptor = doc.dict(cid.get(b"FontDescriptor")?)?;
        let program = descriptor.get(b"FontFile2").or_else(|| descriptor.get(b"FontFile3")).filter(|p| matches!(p, Object::Ref(_)))?.clone();
        Some(Written { resource: String::from_utf8_lossy(key).into_owned(), top, cid, program })
    })
}

/// A resource in `fonts` that already is this embedding: the same font name, program, glyph map
/// and Unicode map. Editing the same text again reuses it instead of adding another copy.
fn same_resource(doc: &Document, fonts: &Dict, name: &str, bytes: &[u8], glyph_map: &[u8], unicode: &[u8]) -> Option<String> {
    written(doc, fonts)
        .find(|w| {
            let glyphs = w.cid.get(b"CIDToGIDMap").or_else(|| w.top.get(b"Encoding"));
            w.top.name(b"BaseFont") == Some(name.as_bytes())
                && stream_data(doc, w.top.get(b"ToUnicode")).is_some_and(|d| d == unicode)
                && stream_data(doc, glyphs).is_some_and(|d| d == glyph_map)
                && w.has_program(doc, bytes)
        })
        .map(|w| w.resource)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf::Metrics;

    #[test]
    fn full_unicode_embedding_keeps_outlines_advances_and_roundtrip_codes() {
        let bytes = include_bytes!("../../../assets/fonts/Inter-Regular.ttf");
        let mut doc = Document::new_empty();
        let mut fonts = Dict::new();
        let name = embed(&mut doc, &mut fonts, bytes, "Inter").unwrap();
        let font = doc.dict(fonts.get(name.as_bytes()).unwrap()).unwrap();
        let m = Metrics::from_dict(&doc, &font);
        let text = "Aé ΩЖ €";
        let codes = m.encode(text).unwrap();
        assert_eq!(m.decode(&codes), text);
        assert_eq!(crate::pdf::embedded_sfnt(&doc, &font).unwrap(), bytes);
        let sfnt = FontRef::new(bytes).unwrap();
        for (ch, (code, _)) in text.chars().zip(m.codes(&codes)) {
            let gid = sfnt.charmap().map(ch).unwrap();
            let w = sfnt.glyph_metrics(Size::unscaled(), LocationRef::default()).advance_width(gid).unwrap() as f64
                / f64::from(sfnt.metrics(Size::unscaled(), LocationRef::default()).units_per_em);
            assert!((m.width(code) - w).abs() < 1e-8);
        }
        let before = doc.object_numbers().len();
        assert!(embed(&mut doc, &mut fonts, b"broken font", "Broken").is_err());
        assert!(embed_mapping(&mut doc, &mut fonts, bytes, "Inter", &[("X".into(), skrifa::GlyphId::new(65_535))]).is_err());
        assert_eq!(doc.object_numbers().len(), before);
        let programs = || doc.scan_objects().filter(|(_, o)| matches!(o.as_ref(), Object::Stream(s) if s.dict.get(b"PCFullFont").is_some())).count();
        assert_eq!(programs(), 1);
        let mut second = Dict::new();
        embed(&mut doc, &mut second, bytes, "Inter").unwrap();
        assert_eq!(doc.scan_objects().filter(|(_, o)| matches!(o.as_ref(), Object::Stream(s) if s.dict.get(b"PCFullFont").is_some())).count(), 1);
        assert_eq!(face_key("ABCDEF+Droid-Sans-Fallback"), face_key("Droid Sans Fallback"));
    }

    /// A contributor-original synthetic OTF: a CID-keyed CFF with three glyphs (.notdef, A, B) whose
    /// charset is not the identity (GID 1 → CID 7, GID 2 → CID 9). Built in memory, so no font file is
    /// committed (AGENTS.md §1.4).
    fn cid_keyed_otf() -> Vec<u8> {
        fn be16(out: &mut Vec<u8>, n: u16) {
            out.extend_from_slice(&n.to_be_bytes());
        }
        fn be32(out: &mut Vec<u8>, n: u32) {
            out.extend_from_slice(&n.to_be_bytes());
        }
        // Fixed-width (3-byte) integers, so a DICT's length is known before its offsets are.
        fn num(n: u16) -> Vec<u8> {
            let mut v = vec![28];
            v.extend_from_slice(&n.to_be_bytes());
            v
        }
        fn index(items: &[Vec<u8>]) -> Vec<u8> {
            let mut out = Vec::new();
            be16(&mut out, items.len() as u16);
            if items.is_empty() {
                return out;
            }
            out.push(1);
            let mut at = 1u8;
            out.push(at);
            for i in items {
                at += i.len() as u8;
                out.push(at);
            }
            items.iter().for_each(|i| out.extend(i));
            out
        }
        let top = |charset: u16, charstrings: u16, fd_array: u16| {
            let mut d = Vec::new();
            [391, 392, 0].iter().for_each(|n| d.extend(num(*n)));
            d.extend([12, 30]);
            d.extend(num(charset));
            d.push(15);
            d.extend(num(charstrings));
            d.push(17);
            d.extend(num(fd_array));
            d.extend([12, 36]);
            d
        };
        let name = index(&[b"CidFont".to_vec()]);
        let strings = index(&[b"Adobe".to_vec(), b"Identity".to_vec()]);
        let charset_at = 4 + name.len() + index(&[top(0, 0, 0)]).len() + strings.len() + 2;
        let charset = [0u8, 0, 7, 0, 9];
        let charstrings = index(&[vec![14], vec![14], vec![14]]);
        let charstrings_at = charset_at + charset.len();
        let fd_array_at = charstrings_at + charstrings.len();
        let mut font_dict = num(0);
        font_dict.extend(num(0));
        font_dict.push(18);
        let mut cff = vec![1, 0, 4, 4];
        cff.extend(name);
        cff.extend(index(&[top(charset_at as u16, charstrings_at as u16, fd_array_at as u16)]));
        cff.extend(strings);
        cff.extend([0, 0]);
        cff.extend(charset);
        cff.extend(charstrings);
        cff.extend(index(&[font_dict]));
        let mut head = vec![0; 54];
        head[0..4].copy_from_slice(&[0, 1, 0, 0]);
        head[12..16].copy_from_slice(&0x5f0f3cf5u32.to_be_bytes());
        head[18..20].copy_from_slice(&1000u16.to_be_bytes());
        head[40..42].copy_from_slice(&700i16.to_be_bytes());
        head[42..44].copy_from_slice(&700i16.to_be_bytes());
        let mut hhea = vec![0; 36];
        hhea[0..4].copy_from_slice(&[0, 1, 0, 0]);
        hhea[4..6].copy_from_slice(&700i16.to_be_bytes());
        hhea[34..36].copy_from_slice(&3u16.to_be_bytes());
        let mut hmtx = Vec::new();
        for w in [500, 600, 700] {
            be16(&mut hmtx, w);
            be16(&mut hmtx, 0);
        }
        let mut maxp = vec![0, 0, 0x50, 0];
        be16(&mut maxp, 3);
        let mut cmap = vec![0, 0, 0, 1, 0, 3, 0, 10];
        be32(&mut cmap, 12);
        be16(&mut cmap, 12);
        be16(&mut cmap, 0);
        be32(&mut cmap, 40);
        be32(&mut cmap, 0);
        be32(&mut cmap, 2);
        for (cp, gid) in [(65, 1), (66, 2)] {
            [cp, cp, gid].iter().for_each(|n| be32(&mut cmap, *n));
        }
        let tables = [(*b"CFF ", cff), (*b"cmap", cmap), (*b"head", head), (*b"hhea", hhea), (*b"hmtx", hmtx), (*b"maxp", maxp)];
        let mut out = b"OTTO".to_vec();
        [tables.len() as u16, 64, 2, 32].iter().for_each(|n| be16(&mut out, *n));
        let mut payload = Vec::new();
        let mut offset = 12 + 16 * tables.len();
        for (tag, bytes) in tables {
            out.extend(tag);
            let mut padded = bytes.clone();
            padded.resize(bytes.len().div_ceil(4) * 4, 0);
            let sum = padded.chunks(4).fold(0u32, |s, b| s.wrapping_add(u32::from_be_bytes([b[0], b[1], b[2], b[3]])));
            [sum, offset as u32, bytes.len() as u32].iter().for_each(|n| be32(&mut out, *n));
            offset += padded.len();
            payload.extend(padded);
        }
        out.extend(payload);
        out
    }

    /// Acrobat rejected a whole page whose embedded CMap lacked /Type, /CMapName and /CIDSystemInfo, and
    /// drew characters whose GID had been written as their CID with other glyphs.
    #[test]
    fn cid_keyed_cff_embedding_maps_glyphs_through_the_charset_and_declares_its_cmap() {
        let bytes = cid_keyed_otf();
        let mut doc = Document::new_empty();
        let mut fonts = Dict::new();
        let name = embed(&mut doc, &mut fonts, &bytes, "CidFont").unwrap();
        let top = doc.dict(fonts.get(name.as_bytes()).unwrap()).unwrap();
        let Object::Stream(encoding) = doc.resolve(top.get(b"Encoding").unwrap()).as_ref().clone() else { panic!("embedded CMap") };
        assert_eq!(encoding.dict.name(b"Type"), Some(&b"CMap"[..]));
        assert_eq!(encoding.dict.name(b"CMapName"), Some(&b"PCGlyphs"[..]));
        let system = encoding.dict.get(b"CIDSystemInfo").and_then(Object::as_dict).expect("CIDSystemInfo");
        let entry = |key: &[u8]| String::from_utf8(system.get(key).and_then(Object::as_string).unwrap().bytes.clone()).unwrap();
        let cmap = String::from_utf8(encoding.decoded().unwrap()).unwrap();
        // The program's own definitions match its stream dictionary (ISO 32000-2 §9.7.5.3).
        let declared = format!(
            "/CIDSystemInfo << /Registry ({}) /Ordering ({}) /Supplement {} >> def\n",
            entry(b"Registry"),
            entry(b"Ordering"),
            system.get(b"Supplement").and_then(Object::as_int).unwrap()
        );
        assert_eq!(declared, "/CIDSystemInfo << /Registry (Adobe) /Ordering (Identity) /Supplement 0 >> def\n");
        assert!(cmap.contains(&declared) && cmap.contains("/CMapName /PCGlyphs def\n"), "{cmap}");
        assert!(cmap.contains("<0001> 7\n") && cmap.contains("<0002> 9\n"), "{cmap}");
        let cid = doc.dict(doc.resolve(top.get(b"DescendantFonts").unwrap()).as_array().unwrap().first().unwrap()).unwrap();
        assert_eq!(cid.name(b"Subtype"), Some(&b"CIDFontType0"[..]));
        let m = Metrics::from_dict(&doc, &top);
        let codes = m.encode("AB").unwrap();
        assert_eq!(m.decode(&codes), "AB");
        let widths: Vec<f64> = m.codes(&codes).iter().map(|(c, _)| m.width(*c)).collect();
        assert!(
            widths.len() == 2 && (widths[0] - 0.6).abs() < 1e-9 && (widths[1] - 0.7).abs() < 1e-9,
            "/W is keyed by the charset's CIDs: {widths:?}"
        );
    }

    #[test]
    fn embedding_the_same_text_again_reuses_its_resource_and_program() {
        let bytes = include_bytes!("../../../assets/fonts/Inter-Regular.ttf");
        let face = FontRef::new(bytes).unwrap();
        let glyph = |c| face.charmap().map(c).unwrap();
        let programs = |doc: &Document| {
            doc.scan_objects().filter(|(_, o)| matches!(o.as_ref(), Object::Stream(s) if s.dict.get(b"PCFullFont").is_some())).count()
        };
        // Objects before the program push it past the first 8192 that the document-wide search reads.
        let mut doc = Document::new_empty();
        for _ in 0..9000 {
            doc.add(Object::Null);
        }
        let mut fonts = Dict::new();
        let full = embed(&mut doc, &mut fonts, bytes, "Inter").unwrap();
        let objects = doc.scan_objects().count();
        assert_eq!(embed(&mut doc, &mut fonts, bytes, "Inter").unwrap(), full, "the same embedding is the same resource");
        let shaped = [("fi".to_string(), glyph('f')), ("é".to_string(), glyph('é'))];
        let first = embed_mapping(&mut doc, &mut fonts, bytes, "Inter", &shaped).unwrap();
        assert_ne!(first, full, "another mapping is another resource");
        assert_eq!(embed_mapping(&mut doc, &mut fonts, bytes, "Inter", &shaped).unwrap(), first);
        assert_eq!(embed_mapping(&mut doc, &mut fonts, bytes, "Inter", &shaped[..1]).map(|r| r != first), Ok(true), "a different mapping");
        assert_eq!(programs(&doc), 1, "one program for every mapping");
        assert_eq!(fonts.iter().count(), 3);
        assert!(doc.scan_objects().count() > objects);
        // Another writer may have compressed the streams since; the page still recognises them.
        for (r, o) in doc.scan_objects().collect::<Vec<_>>() {
            if let Object::Stream(st) = o.as_ref()
                && st.dict.get(b"Filter").is_none()
            {
                let data = st.decoded().unwrap();
                doc.set(r, Object::Stream(Stream::flate(st.dict.clone(), &data)));
            }
        }
        let objects = doc.scan_objects().count();
        assert_eq!(embed(&mut doc, &mut fonts, bytes, "Inter").unwrap(), full);
        assert_eq!(embed_mapping(&mut doc, &mut fonts, bytes, "Inter", &shaped).unwrap(), first);
        assert_eq!(doc.scan_objects().count(), objects, "nothing new is written");
        // A different program under the same name is not taken for this one.
        let other = include_bytes!("../../../assets/fonts/Inter-Medium.ttf");
        let mut more = fonts.clone();
        let medium = embed(&mut doc, &mut more, other, "Inter").unwrap();
        assert!(!fonts.contains(medium.as_bytes()), "{medium} is new");
        assert_eq!(programs(&doc), 2);
    }
}
