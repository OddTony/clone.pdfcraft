//! Office import tests. Every package is generated here from synthetic text and pixels; no
//! Office file, font or image is committed.

use std::sync::Arc;

use pdfcraft_cos::{Dict, Document, Object, SaveOptions, write_full};

use super::zip::tests::build;
use super::*;
use crate::{SourceKind, source_kind};

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const A: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
const REL: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
const T: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

fn rels(items: &[(&str, &str, &str)]) -> String {
    let mut s = format!(r#"<?xml version="1.0" encoding="UTF-8"?><Relationships xmlns="{REL}">"#);
    for (id, ty, target) in items {
        s.push_str(&format!(r#"<Relationship Id="{id}" Type="{T}/{ty}" Target="{target}"/>"#));
    }
    s.push_str("</Relationships>");
    s
}

fn png(w: u32, h: u32) -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, w, h);
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(png::BitDepth::Eight);
        let mut wr = enc.write_header().unwrap();
        wr.write_image_data(&vec![90u8; (w * h * 3) as usize]).unwrap();
    }
    out
}

fn reopen(doc: &Document) -> Document {
    let bytes = write_full(doc, &SaveOptions::default()).unwrap();
    hayro_syntax::Pdf::new(bytes.clone()).expect("parses");
    Document::open(Arc::new(bytes)).unwrap()
}

fn pages(doc: &Document) -> Vec<Dict> {
    let pages = doc.get(doc.root().unwrap()).as_dict().unwrap().reference(b"Pages").unwrap();
    let kids = doc.get(pages).as_dict().unwrap().get(b"Kids").unwrap().as_array().unwrap().clone();
    kids.iter().map(|k| doc.resolve(k).as_dict().cloned().unwrap()).collect()
}

fn content(doc: &Document, page: &Dict) -> String {
    let Object::Stream(s) = &*doc.resolve(page.get(b"Contents").unwrap()) else { panic!("no content") };
    // WinAnsi bytes as characters (Latin-1 agrees with WinAnsi from 0xA0 up).
    s.decoded().unwrap().iter().map(|b| char::from(*b)).collect()
}

fn all_text(doc: &Document) -> String {
    pages(doc).iter().map(|p| content(doc, p)).collect::<Vec<_>>().join("\n")
}

fn size(p: &Dict) -> (f64, f64) {
    let m: Vec<f64> = p.get(b"MediaBox").unwrap().as_array().unwrap().iter().map(|o| o.as_f64().unwrap()).collect();
    (m[2], m[3])
}

// ── Word ────────────────────────────────────────────────────────────────────────────────────

fn docx(body: &str, extra: &[(&str, &[u8])], doc_rels: &[(&str, &str, &str)]) -> Vec<u8> {
    let document = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><w:document xmlns:w="{W}" xmlns:r="{R}" xmlns:a="{A}" xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" xmlns:pic="http://schemas.openxmlformats.org/drawingml/2006/picture"><w:body>{body}</w:body></w:document>"#
    );
    let styles = format!(
        r#"<w:styles xmlns:w="{W}"><w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii="Calibri"/><w:sz w:val="22"/></w:rPr></w:rPrDefault><w:pPrDefault><w:pPr><w:spacing w:after="160" w:line="259" w:lineRule="auto"/></w:pPr></w:pPrDefault></w:docDefaults>
        <w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/></w:style>
        <w:style w:type="paragraph" w:styleId="Heading1"><w:basedOn w:val="Normal"/><w:pPr><w:spacing w:before="240"/></w:pPr><w:rPr><w:rFonts w:ascii="Cambria"/><w:b/><w:sz w:val="32"/><w:color w:val="2F5496"/></w:rPr></w:style>
        <w:style w:type="table" w:styleId="TableGrid"><w:tblPr><w:tblBorders><w:top w:val="single" w:sz="4"/><w:left w:val="single" w:sz="4"/><w:bottom w:val="single" w:sz="4"/><w:right w:val="single" w:sz="4"/><w:insideH w:val="single" w:sz="4"/><w:insideV w:val="single" w:sz="4"/></w:tblBorders></w:tblPr></w:style>
        <w:style w:type="paragraph" w:styleId="Loop"><w:basedOn w:val="Loop2"/></w:style><w:style w:type="paragraph" w:styleId="Loop2"><w:basedOn w:val="Loop"/></w:style></w:styles>"#
    );
    let numbering = format!(
        r#"<w:numbering xmlns:w="{W}"><w:abstractNum w:abstractNumId="0"><w:lvl w:ilvl="0"><w:start w:val="1"/><w:numFmt w:val="decimal"/><w:lvlText w:val="%1."/><w:pPr><w:ind w:left="720" w:hanging="360"/></w:pPr></w:lvl><w:lvl w:ilvl="1"><w:start w:val="1"/><w:numFmt w:val="lowerLetter"/><w:lvlText w:val="%2)"/></w:lvl></w:abstractNum>
        <w:abstractNum w:abstractNumId="1"><w:lvl w:ilvl="0"><w:numFmt w:val="bullet"/><w:lvlText w:val="&#xF0B7;"/><w:pPr><w:ind w:left="720" w:hanging="360"/></w:pPr></w:lvl></w:abstractNum>
        <w:num w:numId="1"><w:abstractNumId w:val="0"/></w:num><w:num w:numId="2"><w:abstractNumId w:val="1"/></w:num></w:numbering>"#
    );
    let mut r = vec![("rIdS", "styles", "styles.xml"), ("rIdN", "numbering", "numbering.xml")];
    r.extend_from_slice(doc_rels);
    let doc_rels = rels(&r);
    let root_rels = rels(&[("rId1", "officeDocument", "word/document.xml")]);
    let mut parts: Vec<(&str, &[u8])> = vec![
        ("[Content_Types].xml", b"<Types/>"),
        ("_rels/.rels", root_rels.as_bytes()),
        ("word/document.xml", document.as_bytes()),
        ("word/_rels/document.xml.rels", doc_rels.as_bytes()),
        ("word/styles.xml", styles.as_bytes()),
        ("word/numbering.xml", numbering.as_bytes()),
    ];
    parts.extend_from_slice(extra);
    build(&parts)
}

fn p(text: &str) -> String {
    format!("<w:p><w:r><w:t xml:space=\"preserve\">{text}</w:t></w:r></w:p>")
}

#[test]
fn word_paragraphs_styles_lists_tables_images_and_page_numbers() {
    let image = png(40, 20);
    let header = format!(
        r#"<w:hdr xmlns:w="{W}"><w:p><w:pPr><w:jc w:val="right"/></w:pPr><w:r><w:t xml:space="preserve">Page </w:t></w:r><w:r><w:fldChar w:fldCharType="begin"/></w:r><w:r><w:instrText> PAGE </w:instrText></w:r><w:r><w:fldChar w:fldCharType="separate"/></w:r><w:r><w:t>9</w:t></w:r><w:r><w:fldChar w:fldCharType="end"/></w:r><w:r><w:t xml:space="preserve"> of </w:t></w:r><w:fldSimple w:instr="NUMPAGES"><w:r><w:t>9</w:t></w:r></w:fldSimple></w:p></w:hdr>"#
    );
    let body = format!(
        r#"<w:p><w:pPr><w:pStyle w:val="Heading1"/></w:pPr><w:r><w:t>Relatório Anual</w:t></w:r></w:p>
        <w:p><w:r><w:t xml:space="preserve">Texto com </w:t></w:r><w:r><w:rPr><w:b/><w:i/></w:rPr><w:t>negrito</w:t></w:r><w:r><w:t xml:space="preserve"> e ação, coração, São Paulo.</w:t></w:r></w:p>
        <w:p><w:pPr><w:numPr><w:ilvl w:val="0"/><w:numId w:val="1"/></w:numPr></w:pPr><w:r><w:t>Primeiro</w:t></w:r></w:p>
        <w:p><w:pPr><w:numPr><w:ilvl w:val="1"/><w:numId w:val="1"/></w:numPr></w:pPr><w:r><w:t>Sub</w:t></w:r></w:p>
        <w:p><w:pPr><w:numPr><w:ilvl w:val="0"/><w:numId w:val="1"/></w:numPr></w:pPr><w:r><w:t>Segundo</w:t></w:r></w:p>
        <w:p><w:pPr><w:numPr><w:ilvl w:val="0"/><w:numId w:val="2"/></w:numPr></w:pPr><w:r><w:t>Marcador</w:t></w:r></w:p>
        <w:tbl><w:tblPr><w:tblStyle w:val="TableGrid"/></w:tblPr><w:tblGrid><w:gridCol w:w="3000"/><w:gridCol w:w="3000"/></w:tblGrid>
          <w:tr><w:tc><w:tcPr><w:shd w:fill="D9E2F3"/></w:tcPr>{}</w:tc><w:tc>{}</w:tc></w:tr>
          <w:tr><w:tc><w:tcPr><w:gridSpan w:val="2"/></w:tcPr>{}</w:tc></w:tr></w:tbl>
        <w:p><w:r><w:drawing><wp:inline><wp:extent cx="1270000" cy="635000"/><a:graphic><a:graphicData uri="http://schemas.openxmlformats.org/drawingml/2006/picture"><pic:pic><pic:blipFill><a:blip r:embed="rIdImg"/></pic:blipFill></pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing></w:r></w:p>
        <w:p><w:r><w:t>中文</w:t></w:r><w:r><w:br w:type="page"/></w:r><w:r><w:t>Depois da quebra</w:t></w:r></w:p>
        <w:p><w:pPr><w:pStyle w:val="Loop"/></w:pPr><w:r><w:t>Estilo em ciclo</w:t></w:r></w:p>
        <w:sectPr><w:headerReference w:type="default" r:id="rIdH"/><w:pgSz w:w="11906" w:h="16838"/><w:pgMar w:top="1440" w:right="1440" w:bottom="1440" w:left="1440" w:header="708" w:footer="708"/></w:sectPr>"#,
        p("Nome"),
        p("Valor"),
        p("Linha inteira")
    );
    let bytes = docx(
        &body,
        &[("word/media/image1.png", &image), ("word/header1.xml", header.as_bytes())],
        &[("rIdImg", "image", "media/image1.png"), ("rIdH", "header", "header1.xml")],
    );
    let (doc, report) = from_docx("Relatorio.docx", &bytes).unwrap();
    assert_eq!(report.pages, 2, "{report:?}");
    assert_eq!(report.escaped, 2, "the two Chinese characters are written as code points: {report:?}");
    let doc = reopen(&doc);
    let pg = pages(&doc);
    assert_eq!(pg.len(), 2);
    let (w, h) = size(&pg[0]);
    assert!((w - 595.3).abs() < 0.2 && (h - 841.9).abs() < 0.2, "A4 from the section: {w}x{h}");
    let first = content(&doc, &pg[0]);
    let second = content(&doc, &pg[1]);
    for needle in [
        "(Relatório)",
        "(negrito)",
        "(ação,)",
        "(coração,)",
        "(1.)",
        "(a\\))",
        "(2.)",
        "(Primeiro)",
        "(Nome)",
        "(Valor)",
        "(inteira)",
        "([U+4E2D][U+6587])",
        "/Im0 Do",
    ] {
        assert!(first.contains(needle), "missing {needle} on page 1");
    }
    assert!(first.contains("(\u{95})"), "the Symbol bullet is drawn as a bullet");
    assert!(second.contains("(quebra)"), "text after the page break starts page 2");
    assert!(first.contains("(1)") && first.contains("(2)"), "header page number and count on page 1: {first}");
    assert!(second.contains("(2)"), "header page number on page 2");
    // Heading in Times (Cambria is a serif face), body in Helvetica (Calibri).
    let fonts = pg[0].get(b"Resources").unwrap().as_dict().unwrap().get(b"Font").unwrap().as_dict().unwrap().clone();
    let names: Vec<String> =
        fonts.iter().map(|(_, r)| String::from_utf8_lossy(doc.resolve(r).as_dict().unwrap().name(b"BaseFont").unwrap()).into_owned()).collect();
    assert!(
        names.iter().any(|n| n == "Times-Bold") && names.iter().any(|n| n == "Helvetica") && names.iter().any(|n| n == "Helvetica-BoldOblique"),
        "{names:?}"
    );
}

#[test]
fn headings_keep_with_their_next_paragraph_and_lists_drop_contextual_spacing() {
    // Enough body text to leave a little room at the bottom of page 1, then a heading.
    let filler: String = (0..41).map(|i| p(&format!("Linha de enchimento {i}"))).collect();
    let keep = r#"<w:p><w:pPr><w:keepNext/><w:spacing w:before="0" w:after="0"/></w:pPr><w:r><w:t>Titulo preso</w:t></w:r></w:p>"#;
    let tall = r#"<w:p><w:pPr><w:spacing w:before="0" w:after="0"/></w:pPr><w:r><w:t>Corpo</w:t></w:r><w:r><w:br/><w:t>a</w:t><w:br/><w:t>b</w:t><w:br/><w:t>c</w:t><w:br/><w:t>d</w:t></w:r></w:p>"#;
    let (doc, _) = from_docx("k.docx", &docx(&format!("{filler}{keep}{tall}"), &[], &[])).unwrap();
    let doc = reopen(&doc);
    let pg = pages(&doc);
    assert_eq!(pg.len(), 2);
    assert!(content(&doc, &pg[1]).contains("(preso)"), "the heading moves to the next page with its paragraph");
    assert!(!content(&doc, &pg[0]).contains("(preso)"));

    // Two list items with contextual spacing sit closer than two plain paragraphs.
    let item = |t: &str, ctx: bool| {
        let c = if ctx { "<w:contextualSpacing/>" } else { "" };
        format!(r#"<w:p><w:pPr><w:spacing w:after="400"/>{c}</w:pPr><w:r><w:t>{t}</w:t></w:r></w:p>"#)
    };
    let gap = |ctx: bool| {
        let (doc, _) = from_docx("c.docx", &docx(&format!("{}{}", item("Um", ctx), item("Dois", ctx)), &[], &[])).unwrap();
        let text = content(&reopen(&doc), &pages(&reopen(&doc))[0]);
        let y = |w: &str| -> f64 {
            let at = text.find(&format!("({w})")).unwrap();
            let tm = text[..at].rfind(" Tm").unwrap();
            text[..tm].split_whitespace().last().unwrap().parse().unwrap()
        };
        y("Um") - y("Dois")
    };
    assert!(gap(false) - gap(true) > 19.0, "contextual spacing removes the 20 pt after: {} vs {}", gap(false), gap(true));
}

#[test]
fn long_documents_paginate_and_tables_split_across_pages() {
    let many: String = (0..120).map(|i| p(&format!("Paragrafo numero {i} com algum texto para ocupar a linha inteira da pagina"))).collect();
    let rows: String = (0..80).map(|i| format!("<w:tr><w:tc>{}</w:tc></w:tr>", p(&format!("Celula {i}")))).collect();
    let tall = format!(
        "<w:tbl><w:tblGrid><w:gridCol w:w=\"5000\"/></w:tblGrid><w:tr><w:tc>{}</w:tc></w:tr></w:tbl>",
        (0..90).map(|i| p(&format!("Linha alta {i}"))).collect::<String>()
    );
    let body = format!("{many}<w:tbl><w:tblGrid><w:gridCol w:w=\"5000\"/></w:tblGrid>{rows}</w:tbl>{tall}");
    let (doc, report) = from_docx("long.docx", &docx(&body, &[], &[])).unwrap();
    assert!(report.pages >= 6, "{report:?}");
    let text = all_text(&reopen(&doc));
    for needle in ["(119)", "(Celula) Tj", "(79)", "(89)"] {
        assert!(text.contains(needle), "missing {needle}");
    }
}

#[test]
fn malformed_docx_is_refused_without_panicking() {
    for data in [b"not a zip".as_slice(), b"PK\x03\x04", &[0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1]] {
        let error = from_docx("broken.docx", data).unwrap_err().to_string();
        assert!(error.starts_with("broken.docx:"), "{error}");
    }
    let no_main = build(&[("_rels/.rels", rels(&[]).as_bytes())]);
    assert!(from_docx("missing.docx", &no_main).unwrap_err().to_string().contains("no main document"));
    let dtd = build(&[("word/document.xml", b"<!DOCTYPE x [<!ENTITY a 'expanded'>]><document>&a;</document>")]);
    assert!(from_docx("dtd.docx", &dtd).unwrap_err().to_string().contains("malformed XML"));
    let other = build(&[("word/document.xml", b"<workbook/>")]);
    assert!(from_docx("sheet.docx", &other).is_err());
    let deep = format!("{}{}{}", "<w:sdt><w:sdtContent>".repeat(200), p("deep text"), "</w:sdtContent></w:sdt>".repeat(200));
    let (_, report) = from_docx("deep.docx", &docx(&deep, &[], &[])).unwrap();
    assert!(report.notes.iter().any(|n| n.contains("nest")), "{report:?}");
}

#[test]
fn docx_detection_and_empty_documents() {
    assert_eq!(source_kind("a.DOCX", b"PK\x03\x04"), Some(SourceKind::Docx));
    assert_eq!(source_kind("a.docx", b"%PDF-1.7"), Some(SourceKind::Pdf));
    assert_eq!(source_kind("a.zip", &docx("", &[], &[])), None);
    let (_, report) = from_docx("empty.docx", &docx("", &[], &[])).unwrap();
    assert_eq!(report.pages, 1);
    assert!(report.notice().contains("layout may differ"));
}

/// Turkish letters outside WinAnsi must keep both their ink and Unicode when saved.
#[test]
fn turkish_letters_survive_docx_import_in_each_style() {
    let text = "Çç Ğğ İı Öö Şş Üü İstanbul, Iğdır, Şişli";
    let body: String = ["", "<w:b/>", "<w:i/>", "<w:b/><w:i/>"]
        .into_iter()
        .map(|style| format!("<w:p><w:r><w:rPr>{style}</w:rPr><w:t>{text}</w:t></w:r></w:p>"))
        .collect();
    let (doc, report) = from_docx("Türkçe.docx", &docx(&body, &[], &[])).unwrap();
    assert_eq!(report.escaped, 0, "Turkish is drawable, not a code-point placeholder: {report:?}");
    let doc = reopen(&doc);
    assert!(!all_text(&doc).contains("[U+"));
    let page = pages(&doc).remove(0);
    let fonts = page.get(b"Resources").unwrap().as_dict().unwrap().get(b"Font").unwrap().as_dict().unwrap();
    for bold in [false, true] {
        let name = if bold { b"UT1" } else { b"UT0" };
        let font = doc.resolve(fonts.get(name).expect("embedded fallback resource"));
        let font = font.as_dict().unwrap();
        assert!(pdfcraft_fonts::pdf::embedded_sfnt(&doc, font).is_some());
        let metrics = pdfcraft_fonts::pdf::Metrics::from_dict(&doc, font);
        let encoded = metrics.encode("ĞğİıŞş").expect("all six letters remain encodable");
        assert_eq!(metrics.decode(&encoded), "ĞğİıŞş");
        let style = RunStyle { bold, ..Default::default() };
        for (pair, c) in encoded.chunks_exact(2).zip("ĞğİıŞş".chars()) {
            let code = u16::from_be_bytes([pair[0], pair[1]]);
            assert!(
                (style.width(&c.to_string()) - metrics.width(u32::from(code)) * style.draw_size()).abs() < 0.001,
                "wrapping must use the embedded advance for {c}"
            );
        }
    }
}

#[test]
fn decomposed_turkish_letters_are_normalized_before_escaping() {
    let text = "C\u{0327}c\u{0327} G\u{0306}g\u{0306} I\u{0307}ı O\u{0308}o\u{0308} S\u{0327}s\u{0327} U\u{0308}u\u{0308}";
    let mut notes = Notes::default();
    assert_eq!(notes.drawable(text), "Çç Ğğ İı Öö Şş Üü");
    assert_eq!(notes.escaped, 0);
    let (_, report) = from_docx("decomposed.docx", &docx(&p(text), &[], &[])).unwrap();
    assert_eq!(report.escaped, 0);
}
