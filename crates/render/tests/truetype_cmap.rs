//! Regression tests for embedded TrueType glyph mapping (issues #2159 and #2426).

use std::sync::Arc;

use lopdf::{Document, Object, Stream, dictionary};
use pdfcraft_render::{PageRenderer, RenderConfig, RenderRequest, RequestKind};

const INTER_TTF: &[u8] = include_bytes!("../../../assets/fonts/Inter-Regular.ttf");

/// Find the byte offset of a table in a TrueType `sfnt` font.
fn find_table(ttf: &[u8], tag: &[u8; 4]) -> usize {
    let num_tables = u16::from_be_bytes([ttf[4], ttf[5]]) as usize;
    for i in 0..num_tables {
        let rec = 12 + i * 16;
        if &ttf[rec..rec + 4] == tag {
            return u32::from_be_bytes([ttf[rec + 8], ttf[rec + 9], ttf[rec + 10], ttf[rec + 11]]) as usize;
        }
    }
    panic!("missing table {:?}", std::str::from_utf8(tag));
}

/// Rewrite the `cmap` table of a TrueType font so its Windows `(3, 1)` / `(3, 10)` subtable
/// headers become Unicode `(0, 3)` subtable headers and its `post` table is version 3.0 (no
/// glyph names), matching embedded TrueType font subsets produced on macOS (issue #2159).
fn make_macos_unicode_cmap_subset(ttf: &[u8]) -> Vec<u8> {
    let mut out = ttf.to_vec();
    let cmap = find_table(&out, b"cmap");
    let num_records = u16::from_be_bytes([out[cmap + 2], out[cmap + 3]]) as usize;
    let mut changed = false;
    for i in 0..num_records {
        let rec = cmap + 4 + i * 8;
        let platform_id = u16::from_be_bytes([out[rec], out[rec + 1]]);
        let encoding_id = u16::from_be_bytes([out[rec + 2], out[rec + 3]]);
        if platform_id == 3 && (encoding_id == 1 || encoding_id == 10) {
            out[rec..rec + 2].copy_from_slice(&0u16.to_be_bytes());
            out[rec + 2..rec + 4].copy_from_slice(&3u16.to_be_bytes());
            changed = true;
        }
    }
    assert!(changed, "expected at least one Windows Unicode cmap record to convert");

    // Set `post` version to 3.0 (0x00030000), which carries no PostScript glyph names.
    let post = find_table(&out, b"post");
    out[post..post + 4].copy_from_slice(&0x0003_0000u32.to_be_bytes());
    out
}

/// Render a single-byte string `codes` on a 120x80 page and return the RGBA buffer.
fn render_codes(font_bytes: Vec<u8>, flags: i64, encoding: Option<Object>, to_unicode: Option<&[u8]>, codes: &[u8]) -> Vec<u8> {
    let mut doc = Document::with_version("1.7");
    let program = doc.add_object(Stream::new(dictionary! {}, font_bytes));
    let descriptor = doc.add_object(dictionary! {
        "Type" => "FontDescriptor",
        "FontName" => "ABCDEF+Inter-Regular",
        "Flags" => flags,
        "FontFile2" => program,
    });
    let mut font = dictionary! {
        "Type" => "Font",
        "Subtype" => "TrueType",
        "BaseFont" => "ABCDEF+Inter-Regular",
        "FontDescriptor" => descriptor,
    };
    if let Some(enc) = encoding {
        font.set("Encoding", enc);
    }
    if let Some(cmap) = to_unicode {
        font.set("ToUnicode", doc.add_object(Stream::new(dictionary! {}, cmap.to_vec())));
    }
    let font_id = doc.add_object(font);

    let mut ops = b"BT /F1 36 Tf 1 0 0 1 15 25 Tm <".to_vec();
    for &b in codes {
        ops.extend_from_slice(format!("{b:02X}").as_bytes());
    }
    ops.extend_from_slice(b"> Tj ET");
    let content = doc.add_object(Stream::new(dictionary! {}, ops));
    let pages = doc.new_object_id();
    let page = doc.add_object(dictionary! {
        "Type" => "Page",
        "Parent" => pages,
        "MediaBox" => vec![Object::Integer(0), Object::Integer(0), Object::Integer(120), Object::Integer(80)],
        "Resources" => dictionary! { "Font" => dictionary! { "F1" => font_id } },
        "Contents" => content,
    });
    doc.objects.insert(pages, dictionary! { "Type" => "Pages", "Kids" => vec![Object::Reference(page)], "Count" => 1 }.into());
    let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages });
    doc.trailer.set("Root", catalog);

    let mut pdf_bytes = Vec::new();
    doc.save_to(&mut pdf_bytes).expect("save synthetic PDF");

    let rendered = PageRenderer::new(Arc::new(pdf_bytes), RenderConfig::default()).render(RenderRequest {
        page: 0,
        kind: RequestKind::Pixels,
        scale: 1.0,
        ..Default::default()
    });
    assert!(rendered.error.is_none(), "render failed: {:?}", rendered.error);
    rendered.rgba.to_vec()
}

/// Reference raster for a single glyph named `glyph_name` in `Inter-Regular.ttf`.
fn reference_glyph_rgba(glyph_name: &[u8]) -> Vec<u8> {
    let encoding = Object::Dictionary(dictionary! {
        "Type" => "Encoding",
        "BaseEncoding" => Object::Name(b"WinAnsiEncoding".to_vec()),
        "Differences" => vec![
            Object::Integer(65),
            Object::Name(glyph_name.to_vec()),
        ],
    });
    render_codes(INTER_TTF.to_vec(), 32, Some(encoding), None, &[65])
}

#[test]
fn symbolic_truetype_with_differences_renders_czech_and_slovak_carons() {
    // ISO 32000-2 §9.8.2 Table 121 specifies /Flags 4 (Symbolic) when a font contains glyphs
    // outside the Adobe Standard Latin set (such as Czech ř/ě in #2426 or Slovak č/ľ in #2159).
    // Even with /Flags 4, ISO 32000-2 §9.6.5.4 requires consulting /Encoding and /Differences.
    let encoding = Object::Dictionary(dictionary! {
        "Type" => "Encoding",
        "BaseEncoding" => Object::Name(b"WinAnsiEncoding".to_vec()),
        "Differences" => vec![
            Object::Integer(190),
            Object::Name(b"lcaron".to_vec()),
            Object::Integer(232),
            Object::Name(b"ccaron".to_vec()),
            Object::Integer(236),
            Object::Name(b"ecaron".to_vec()),
            Object::Integer(248),
            Object::Name(b"rcaron".to_vec()),
        ],
    });
    for (code, name) in [(190u8, &b"lcaron"[..]), (232u8, &b"ccaron"[..]), (236u8, &b"ecaron"[..]), (248u8, &b"rcaron"[..])] {
        let expected = reference_glyph_rgba(name);
        let actual = render_codes(INTER_TTF.to_vec(), 4, Some(encoding.clone()), None, &[code]);
        assert_eq!(actual, expected, "Symbolic TrueType font with /Differences drew wrong glyph for {}", String::from_utf8_lossy(name));
    }
}

#[test]
fn macos_unicode_platform_cmap_renders_slovak_carons() {
    // Issue #2159: on macOS, embedded TrueType fonts with a PlatformId::Unicode (0, 3) cmap,
    // post v3.0 (no glyph names), and no (3, 1) Windows subtable failed to map Slovak č and ľ.
    let macos_ttf = make_macos_unicode_cmap_subset(INTER_TTF);
    let encoding = Object::Dictionary(dictionary! {
        "Type" => "Encoding",
        "BaseEncoding" => Object::Name(b"WinAnsiEncoding".to_vec()),
        "Differences" => vec![
            Object::Integer(190),
            Object::Name(b"lcaron".to_vec()),
            Object::Integer(232),
            Object::Name(b"ccaron".to_vec()),
            Object::Integer(236),
            Object::Name(b"ecaron".to_vec()),
            Object::Integer(248),
            Object::Name(b"rcaron".to_vec()),
        ],
    });
    for (code, name) in [(190u8, &b"lcaron"[..]), (232u8, &b"ccaron"[..]), (236u8, &b"ecaron"[..]), (248u8, &b"rcaron"[..])] {
        let expected = reference_glyph_rgba(name);
        let actual = render_codes(macos_ttf.clone(), 32, Some(encoding.clone()), None, &[code]);
        assert_eq!(actual, expected, "macOS (0, 3) Unicode cmap drew wrong glyph for {}", String::from_utf8_lossy(name));
    }
}

#[test]
fn truetype_without_differences_falls_back_to_tounicode_for_central_european_codes() {
    // Issue #2426: when a single-byte TrueType font encodes Czech/Slovak text using Windows-1250
    // codes (0xF8 = ř, 0xEC = ě, 0xE8 = č, 0xBE = ľ) with /ToUnicode and no /Differences, the
    // glyphs must resolve through the Unicode cmap rather than drawing .notdef or wrong glyphs.
    let to_unicode = b"/CIDInit /ProcSet findresource begin 12 dict begin begincmap \
        /CMapName /CP1250 def /CMapType 2 def \
        1 begincodespacerange <00> <FF> endcodespacerange \
        4 beginbfchar \
        <BE> <013E> \
        <E8> <010D> \
        <EC> <011B> \
        <F8> <0159> \
        endbfchar \
        endcmap CMapName currentdict /CMap defineresource pop end end";
    for (code, name) in [(0xBEu8, &b"lcaron"[..]), (0xE8u8, &b"ccaron"[..]), (0xECu8, &b"ecaron"[..]), (0xF8u8, &b"rcaron"[..])] {
        let expected = reference_glyph_rgba(name);
        let actual = render_codes(INTER_TTF.to_vec(), 32, None, Some(to_unicode), &[code]);
        assert_eq!(actual, expected, "TrueType without /Differences and with /ToUnicode drew wrong glyph for {}", String::from_utf8_lossy(name));
    }
}

#[test]
fn post_v2_uni_and_agl_glyph_names_resolve_without_unicode_cmap() {
    // Strip all Unicode cmap subtables by rewriting platform_id to 99 so lookup must go through
    // the TrueType `post` v2.0 glyph name table (including uniXXXX <-> AGL aliasing).
    let mut post_only_ttf = INTER_TTF.to_vec();
    let cmap = find_table(&post_only_ttf, b"cmap");
    let num_records = u16::from_be_bytes([post_only_ttf[cmap + 2], post_only_ttf[cmap + 3]]) as usize;
    for i in 0..num_records {
        let rec = cmap + 4 + i * 8;
        post_only_ttf[rec..rec + 2].copy_from_slice(&99u16.to_be_bytes());
    }

    let encoding = Object::Dictionary(dictionary! {
        "Type" => "Encoding",
        "BaseEncoding" => Object::Name(b"WinAnsiEncoding".to_vec()),
        "Differences" => vec![
            Object::Integer(190),
            Object::Name(b"uni013E".to_vec()),
            Object::Integer(232),
            Object::Name(b"uni010D".to_vec()),
            Object::Integer(236),
            Object::Name(b"ecaron.salt".to_vec()),
            Object::Integer(248),
            Object::Name(b"u0159".to_vec()),
        ],
    });
    for (code, name) in [(190u8, &b"lcaron"[..]), (232u8, &b"ccaron"[..]), (236u8, &b"ecaron"[..]), (248u8, &b"rcaron"[..])] {
        let expected = reference_glyph_rgba(name);
        let actual = render_codes(post_only_ttf.clone(), 32, Some(encoding.clone()), None, &[code]);
        assert_eq!(actual, expected, "post v2.0 glyph name resolution drew wrong glyph for {}", String::from_utf8_lossy(name));
    }
}

#[test]
fn font_descriptor_flags_with_extra_bits_preserve_non_symbolic() {
    let encoding = Object::Dictionary(dictionary! {
        "Type" => "Encoding",
        "BaseEncoding" => Object::Name(b"WinAnsiEncoding".to_vec()),
        "Differences" => vec![
            Object::Integer(232),
            Object::Name(b"ccaron".to_vec()),
        ],
    });
    let expected = reference_glyph_rgba(b"ccaron");
    // Bit 6 (32 = NonSymbolic) plus a high producer-specific bit (1 << 20) that previously caused
    // FontFlags::from_bits to return None and discard all flags.
    let actual = render_codes(INTER_TTF.to_vec(), 32 | (1 << 20), Some(encoding), None, &[232]);
    assert_eq!(actual, expected);
}
