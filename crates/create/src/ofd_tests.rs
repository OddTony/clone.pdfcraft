//! OFD → PDF on synthetic packages built here: no binary fixtures, no real documents.

use std::sync::Arc;

use pdfcraft_cos::{SaveOptions, write_full};

use super::*;

fn crc32(bytes: &[u8]) -> u32 {
    let mut c = 0xFFFF_FFFFu32;
    for &b in bytes {
        c ^= u32::from(b);
        for _ in 0..8 {
            c = if c & 1 == 1 { (c >> 1) ^ 0xEDB8_8320 } else { c >> 1 };
        }
    }
    !c
}

/// A stored (uncompressed) ZIP of `files`.
fn zip(files: &[(&str, Vec<u8>)]) -> Vec<u8> {
    let (mut out, mut central) = (Vec::new(), Vec::new());
    for (name, data) in files {
        let offset = out.len() as u32;
        let header = |central: bool| {
            let mut h = Vec::new();
            h.extend(if central { 0x0201_4b50u32 } else { 0x0403_4b50 }.to_le_bytes());
            if central {
                h.extend(20u16.to_le_bytes());
            }
            h.extend(20u16.to_le_bytes());
            h.extend(0x0800u16.to_le_bytes());
            h.extend(0u16.to_le_bytes()); // stored
            h.extend([0u8; 4]);
            h.extend(crc32(data).to_le_bytes());
            h.extend((data.len() as u32).to_le_bytes());
            h.extend((data.len() as u32).to_le_bytes());
            h.extend((name.len() as u16).to_le_bytes());
            h.extend(0u16.to_le_bytes());
            if central {
                h.extend([0u8; 10]);
                h.extend(offset.to_le_bytes());
            }
            h.extend(name.as_bytes());
            h
        };
        out.extend(header(false));
        out.extend(data);
        central.extend(header(true));
    }
    let cd = out.len() as u32;
    out.extend(&central);
    out.extend(0x0605_4b50u32.to_le_bytes());
    out.extend([0u8; 4]);
    out.extend((files.len() as u16).to_le_bytes());
    out.extend((files.len() as u16).to_le_bytes());
    out.extend((central.len() as u32).to_le_bytes());
    out.extend(cd.to_le_bytes());
    out.extend(0u16.to_le_bytes());
    out
}

fn png_bytes() -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, 2, 2);
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(png::BitDepth::Eight);
        let mut w = enc.write_header().unwrap();
        w.write_image_data(&[90u8; 12]).unwrap();
    }
    out
}

const NS: &str = r#"xmlns:ofd="http://www.ofdspec.org/2016""#;

fn package(pages: &[&str], signed: bool) -> Vec<u8> {
    package_with_font(pages, signed, b"not a font")
}

/// `font` is the file behind font 5 (an embedded OFD font).
fn package_with_font(pages: &[&str], signed: bool, font: &[u8]) -> Vec<u8> {
    package_full(pages, signed, font, &[])
}

/// `templates` are (ID, z-order, body) template pages.
fn package_full(pages: &[&str], signed: bool, font: &[u8], templates: &[(u32, &str, &str)]) -> Vec<u8> {
    let signatures = if signed { "<ofd:Signatures>Doc_0/Signs/Signatures.xml</ofd:Signatures>" } else { "" };
    let ofd = format!(
        r#"<ofd:OFD {NS} Version="1.0" DocType="OFD"><ofd:DocBody><ofd:DocInfo><ofd:Title>Test</ofd:Title></ofd:DocInfo><ofd:DocRoot>Doc_0/Document.xml</ofd:DocRoot>{signatures}</ofd:DocBody></ofd:OFD>"#
    );
    let list: String = (0..pages.len()).map(|i| format!(r#"<ofd:Page ID="{}" BaseLoc="Pages/Page_{i}/Content.xml"/>"#, 100 + i)).collect();
    let tpls: String =
        templates.iter().map(|(id, z, _)| format!(r#"<ofd:TemplatePage ID="{id}" ZOrder="{z}" BaseLoc="Tpls/Tpl_{id}/Content.xml"/>"#)).collect();
    let document = format!(
        r#"<ofd:Document {NS}><ofd:CommonData><ofd:MaxUnitID>50</ofd:MaxUnitID><ofd:PageArea><ofd:PhysicalBox>0 0 210 297</ofd:PhysicalBox></ofd:PageArea>
<ofd:PublicRes>PublicRes.xml</ofd:PublicRes>{tpls}</ofd:CommonData><ofd:Pages>{list}</ofd:Pages></ofd:Document>"#
    );
    let res = format!(
        r#"<ofd:Res {NS} BaseLoc="Res"><ofd:Fonts><ofd:Font ID="4" FontName="宋体" FamilyName="宋体"/><ofd:Font ID="5" FontName="Fake"><ofd:FontFile>fake.ttf</ofd:FontFile></ofd:Font></ofd:Fonts>
<ofd:MultiMedias><ofd:MultiMedia ID="6" Type="Image" Format="PNG"><ofd:MediaFile>image_6.png</ofd:MediaFile></ofd:MultiMedia>
<ofd:MultiMedia ID="7" Type="Image" Format="JBIG2"><ofd:MediaFile>image_7.jb2</ofd:MediaFile></ofd:MultiMedia></ofd:MultiMedias></ofd:Res>"#
    );
    let mut files: Vec<(String, Vec<u8>)> = vec![
        ("OFD.xml".into(), ofd.into_bytes()),
        ("Doc_0/Document.xml".into(), document.into_bytes()),
        ("Doc_0/PublicRes.xml".into(), res.into_bytes()),
        ("Doc_0/Res/image_6.png".into(), png_bytes()),
        ("Doc_0/Res/image_7.jb2".into(), b"\x97JB2\r\n\x1a\n".to_vec()),
        ("Doc_0/Res/fake.ttf".into(), font.to_vec()),
    ];
    for (i, body) in pages.iter().enumerate() {
        files.push((format!("Doc_0/Pages/Page_{i}/Content.xml"), body.as_bytes().to_vec()));
    }
    for (id, _, body) in templates {
        files.push((format!("Doc_0/Tpls/Tpl_{id}/Content.xml"), page(body).into_bytes()));
    }
    let refs: Vec<(&str, Vec<u8>)> = files.iter().map(|(n, d)| (n.as_str(), d.clone())).collect();
    zip(&refs)
}

fn page(body: &str) -> String {
    format!(r#"<ofd:Page {NS}><ofd:Content><ofd:Layer ID="10">{body}</ofd:Layer></ofd:Content></ofd:Page>"#)
}

/// Write, reparse (with hayro-syntax too) and return each page's dictionary and content.
fn convert(bytes: &[u8]) -> (Vec<(Dict, String)>, OfdReport, Document) {
    let (doc, report) = from_ofd(bytes).unwrap();
    let out = write_full(&doc, &SaveOptions::default()).unwrap();
    hayro_syntax::Pdf::new(out.clone()).expect("parses");
    let doc = Document::open(Arc::new(out)).unwrap();
    let pages = doc.get(doc.root().unwrap()).as_dict().unwrap().reference(b"Pages").unwrap();
    let kids = doc.get(pages).as_dict().unwrap().get(b"Kids").unwrap().as_array().unwrap().clone();
    let pages = kids
        .iter()
        .map(|k| {
            let d = doc.resolve(k).as_dict().cloned().unwrap();
            let c = doc.resolve(d.get(b"Contents").unwrap());
            let Object::Stream(s) = &*c else { panic!("contents are not a stream") };
            let content = s.decoded().unwrap();
            (d, String::from_utf8_lossy(&content).into_owned())
        })
        .collect();
    (pages, report, doc)
}

fn media(d: &Dict) -> Vec<f64> {
    d.get(b"MediaBox").unwrap().as_array().unwrap().iter().map(|o| o.as_f64().unwrap()).collect()
}

fn font_names(doc: &Document, page: &Dict) -> Vec<String> {
    let res = doc.resolve(page.get(b"Resources").unwrap()).as_dict().cloned().unwrap();
    let fonts = doc.resolve(res.get(b"Font").unwrap()).as_dict().cloned().unwrap();
    let mut names: Vec<String> = fonts
        .iter()
        .map(|(_, f)| {
            let f = doc.resolve(f).as_dict().cloned().unwrap();
            String::from_utf8_lossy(f.get(b"BaseFont").unwrap().as_name().unwrap()).into_owned()
        })
        .collect();
    names.sort();
    names
}

const PATH: &str = r#"<ofd:PathObject ID="11" Boundary="10 20 50 30" LineWidth="0.5" Fill="true">
<ofd:FillColor Value="255 0 0"/><ofd:StrokeColor Value="0 0 255"/>
<ofd:AbbreviatedData>M 0 0 L 50 0 Q 50 30 0 30 A 5 5 0 0 1 0 10 C</ofd:AbbreviatedData></ofd:PathObject>"#;

const TEXT: &str = r#"<ofd:TextObject ID="12" Boundary="20 40 100 10" Font="4" Size="5">
<ofd:TextCode X="0" Y="5" DeltaX="3 g 2 5">Fé发票</ofd:TextCode></ofd:TextObject>"#;

#[test]
fn pages_get_the_ofd_page_size() {
    let custom = r#"<ofd:Page xmlns:ofd="http://www.ofdspec.org/2016"><ofd:Area><ofd:PhysicalBox>0 0 100 50</ofd:PhysicalBox></ofd:Area><ofd:Content/></ofd:Page>"#;
    let (pages, report, _) = convert(&package(&[&page(""), custom], false));
    assert_eq!((pages.len(), report.pages), (2, 2));
    let a4 = media(&pages[0].0);
    assert!((a4[2] - 595.2756).abs() < 1e-3 && (a4[3] - 841.8898).abs() < 1e-3, "{a4:?}");
    let small = media(&pages[1].0);
    assert!((small[2] - 283.4646).abs() < 1e-3 && (small[3] - 141.7323).abs() < 1e-3, "{small:?}");
    // Millimetres, y down → points, y up.
    assert!(pages[0].1.starts_with("2.8346 0 0 -2.8346 0 841.8898 cm\n"), "{}", pages[0].1);
}

#[test]
fn paths_keep_colours_width_and_curves() {
    let (pages, report, _) = convert(&package(&[&page(PATH)], false));
    let ops = &pages[0].1;
    assert!(ops.contains("1 0 0 1 10 20 cm"), "placed at the boundary: {ops}");
    assert!(ops.contains("0.5 w") && ops.contains("1 0 0 rg") && ops.contains("0 0 1 RG"), "{ops}");
    assert!(ops.contains("0 0 m") && ops.contains("50 0 l") && ops.contains(" c\n") && ops.contains("\nh\n"), "{ops}");
    assert!(ops.contains("\nB\nQ"), "filled and stroked: {ops}");
    assert_eq!(report.skipped, 0, "{:?}", report.notes);
}

#[test]
fn text_is_real_text_with_standard_and_chinese_fallback_fonts() {
    let (pages, report, doc) = convert(&package(&[&page(TEXT)], false));
    let ops = &pages[0].1;
    // "F" and "é" through WinAnsi Helvetica (é is 0xE9), the Chinese through STSong-Light.
    assert!(ops.contains("BT\n") && ops.contains("<46> Tj") && ops.contains("<E9> Tj"), "{ops}");
    assert!(ops.contains("<53D1> Tj") && ops.contains("<7968> Tj"), "{ops}");
    assert!(
        ops.contains("5 0 0 -5 0 5 Tm") && ops.contains("5 0 0 -5 3 5 Tm") && ops.contains("5 0 0 -5 8 5 Tm") && ops.contains("5 0 0 -5 13 5 Tm"),
        "DeltaX positions: {ops}"
    );
    assert_eq!(font_names(&doc, &pages[0].0), ["Helvetica", "STSong-Light"]);
    assert_eq!(report.skipped, 0);
}

#[test]
fn a_font_file_that_is_not_a_font_falls_back() {
    let body = r#"<ofd:TextObject ID="12" Boundary="0 0 50 10" Font="5" Size="4"><ofd:TextCode X="1" Y="4">Hi</ofd:TextCode></ofd:TextObject>"#;
    let (pages, _, doc) = convert(&package(&[&page(body)], false));
    assert!(pages[0].1.contains("<48> Tj"), "{}", pages[0].1);
    assert_eq!(font_names(&doc, &pages[0].0), ["Helvetica"]);
}

#[test]
fn embedded_fonts_are_embedded_with_widths_and_to_unicode() {
    // The project's own Inter stands in for a font an OFD file carries.
    let body = r#"<ofd:TextObject ID="12" Boundary="0 0 50 10" Font="5" Size="4"><ofd:TextCode X="1" Y="4">Hi发</ofd:TextCode></ofd:TextObject>"#;
    let (pages, _, doc) = convert(&package_with_font(&[&page(body)], false, pdfcraft_fonts::INTER_REGULAR));
    let ops = &pages[0].1;
    assert!(ops.contains("/F5 1 Tf"), "{ops}");
    // Inter has no Chinese: that glyph falls back to STSong-Light.
    assert!(ops.contains("/FCJK 1 Tf") && ops.contains("<53D1> Tj"), "{ops}");
    assert_eq!(font_names(&doc, &pages[0].0), ["Fake", "STSong-Light"]);
    let res = doc.resolve(pages[0].0.get(b"Resources").unwrap()).as_dict().cloned().unwrap();
    let fonts = doc.resolve(res.get(b"Font").unwrap()).as_dict().cloned().unwrap();
    let t0 = doc.resolve(fonts.get(b"F5").unwrap()).as_dict().cloned().unwrap();
    assert!(t0.get(b"ToUnicode").is_some());
    let kids = t0.get(b"DescendantFonts").unwrap().as_array().unwrap().clone();
    let cid = doc.resolve(&kids[0]).as_dict().cloned().unwrap();
    assert_eq!(cid.get(b"W").unwrap().as_array().unwrap().len(), 4, "two glyphs with widths");
    let fd = doc.resolve(cid.get(b"FontDescriptor").unwrap()).as_dict().cloned().unwrap();
    assert!(fd.get(b"FontFile2").is_some());
}

#[test]
fn images_are_drawn_and_undecodable_ones_are_reported() {
    let body = r#"<ofd:ImageObject ID="13" Boundary="150 10 40 40" CTM="40 0 0 40 0 0" ResourceID="6"/>
<ofd:ImageObject ID="14" Boundary="10 10 40 40" CTM="40 0 0 40 0 0" ResourceID="7"/>
<ofd:CompositeObject ID="15" Boundary="0 0 10 10" ResourceID="16"/>"#;
    let (pages, report, _) = convert(&package(&[&page(body)], false));
    let ops = &pages[0].1;
    assert!(ops.contains("40 0 0 40 150 10 cm") && ops.contains("1 0 0 -1 0 1 cm\n/Im6 Do"), "{ops}");
    assert!(!ops.contains("/Im7"), "{ops}");
    assert_eq!(report.skipped, 2, "{:?}", report.notes);
    assert!(report.notes.iter().any(|n| n.contains("JBIG2")) && report.notes.iter().any(|n| n.contains("composite")), "{:?}", report.notes);
    assert!(!report.signed);
}

#[test]
fn signed_files_say_seals_are_not_carried_over() {
    let (_, report, _) = convert(&package(&[&page(PATH)], true));
    assert!(report.signed);
    assert!(report.notes.iter().any(|n| n.contains("seals")), "{:?}", report.notes);
}

#[test]
fn files_that_are_not_ofd_are_errors_not_panics() {
    assert!(from_ofd(b"%PDF-1.7").is_err());
    assert!(from_ofd(b"PK\x03\x04garbage").is_err());
    let mut bytes = package(&[&page(PATH)], false);
    bytes.truncate(bytes.len() / 2);
    assert!(from_ofd(&bytes).is_err());
}

#[test]
fn source_kind_recognises_ofd() {
    assert_eq!(crate::source_kind("a.ofd", &package(&[&page("")], false)), Some(crate::SourceKind::Ofd));
}

#[test]
fn a_template_is_written_once_for_all_its_pages() {
    // A template used by every page is one form XObject, not a copy per page: a small file with
    // a large template and many pages must not multiply the template by its page count.
    let background = r#"<ofd:PathObject ID="61" Boundary="0 0 10 10"><ofd:AbbreviatedData>M 0 0 L 77 0</ofd:AbbreviatedData></ofd:PathObject>"#;
    let foreground = r#"<ofd:PathObject ID="63" Boundary="0 0 10 10"><ofd:AbbreviatedData>M 0 0 L 88 0</ofd:AbbreviatedData></ofd:PathObject>"#;
    let body = format!(
        r#"<ofd:Page {NS}><ofd:Template TemplateID="60"/><ofd:Template TemplateID="62"/><ofd:Content><ofd:Layer ID="10">{PATH}</ofd:Layer></ofd:Content></ofd:Page>"#
    );
    let pages: Vec<&str> = vec![body.as_str(); 30];
    let bytes = package_full(&pages, false, b"not a font", &[(60, "Background", background), (62, "Foreground", foreground)]);
    let (pages, report, doc) = convert(&bytes);
    assert_eq!(report.pages, 30);
    let mut forms = Vec::new();
    for (dict, ops) in &pages {
        assert!(!ops.contains("77 0 l") && !ops.contains("88 0 l"), "the template is not copied into the page: {ops}");
        let (bg, own, fg) = (ops.find("/Tpl60 Do").unwrap(), ops.find("50 0 l").unwrap(), ops.find("/Tpl62 Do").unwrap());
        assert!(bg < own && own < fg, "background, page, foreground: {ops}");
        let res = doc.resolve(dict.get(b"Resources").unwrap()).as_dict().cloned().unwrap();
        let xobjects = doc.resolve(res.get(b"XObject").unwrap()).as_dict().cloned().unwrap();
        forms.push((xobjects.get(b"Tpl60").unwrap().clone(), xobjects.get(b"Tpl62").unwrap().clone()));
    }
    assert!(forms.windows(2).all(|w| w[0] == w[1]), "every page draws the same two forms");
    let form = doc.resolve(&forms[0].0);
    let Object::Stream(form) = &*form else { panic!("the template is not a stream") };
    assert_eq!(form.dict.get(b"Subtype").and_then(Object::as_name), Some(&b"Form"[..]));
    assert!(String::from_utf8_lossy(&form.decoded().unwrap()).contains("77 0 l"));
}
