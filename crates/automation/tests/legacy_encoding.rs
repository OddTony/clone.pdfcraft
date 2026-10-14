//! `text_encoding` end to end: an old Hebrew PDF whose embedded font draws Hebrew at code-page
//! codes, with no ToUnicode. Its own test binary, because the choice is process-wide.

use std::path::PathBuf;

use pdfcraft_automation::{Automation, Content};
use serde_json::{Value, json};

fn ok(a: &mut Automation, tool: &str, args: Value) -> Value {
    match a.call(tool, &args) {
        Ok(mut c) => match c.remove(0) {
            Content::Json(v) => v,
            other => panic!("{tool}: expected JSON, got {other:?}"),
        },
        Err(e) => panic!("{tool} {args}: {e}"),
    }
}

/// Inter's outlines behind a Windows symbol cmap only (codes 0xE0 + k and 0x20 draw some of its
/// glyphs), declaring the Hebrew code page and not Latin 1 in OS/2: a legacy Hebrew TrueType
/// program in miniature.
fn legacy_hebrew_program() -> Vec<u8> {
    let inter = include_bytes!("../../../assets/fonts/Inter-Regular.ttf");
    // Which of Inter's glyphs stand in for the letters doesn't matter here.
    let mut map: Vec<(u16, u16)> = (0..27u16).map(|k| (0xF0E0 + k, 40 + k)).collect();
    map.push((0xF020, 3));
    map.sort_unstable();
    // One format-4 segment per code, then the closing 0xFFFF segment.
    let segs = map.len() + 1;
    let mut sub = Vec::new();
    for v in [4u16, (16 + segs * 8) as u16, 0, (segs * 2) as u16, 0, 0, 0] {
        sub.extend_from_slice(&v.to_be_bytes());
    }
    let ends: Vec<u16> = map.iter().map(|m| m.0).chain([0xFFFF]).collect();
    let deltas: Vec<u16> = map.iter().map(|&(c, g)| g.wrapping_sub(c)).chain([1]).collect();
    for v in ends.iter().chain(&[0]).chain(&ends).chain(&deltas).chain(&vec![0; segs]) {
        sub.extend_from_slice(&v.to_be_bytes());
    }
    let mut cmap = vec![0, 0, 0, 1, 0, 3, 0, 0, 0, 0, 0, 12];
    cmap.extend(sub);
    let mut program = inter.to_vec();
    let count = u16::from_be_bytes([program[4], program[5]]) as usize;
    let entry = |p: &[u8], tag: &[u8]| (0..count).map(|i| 12 + i * 16).find(|i| &p[*i..*i + 4] == tag).unwrap();
    let os2 = entry(&program, b"OS/2");
    let at = u32::from_be_bytes(program[os2 + 8..os2 + 12].try_into().unwrap()) as usize;
    program[at + 78..at + 82].copy_from_slice(&(1u32 << 5).to_be_bytes());
    let cmap_entry = entry(&program, b"cmap");
    while !program.len().is_multiple_of(4) {
        program.push(0);
    }
    let offset = program.len() as u32;
    program[cmap_entry + 8..cmap_entry + 12].copy_from_slice(&offset.to_be_bytes());
    program[cmap_entry + 12..cmap_entry + 16].copy_from_slice(&(cmap.len() as u32).to_be_bytes());
    program.extend(cmap);
    program
}

/// One page drawing `shown` (hex) in the legacy font, and `latin` in Helvetica.
fn hebrew_pdf(shown: &str, latin: &str) -> Vec<u8> {
    let program = legacy_hebrew_program();
    let content = format!("BT /F1 24 Tf 20 200 Td <{shown}> Tj ET BT /F2 24 Tf 20 100 Td <{latin}> Tj ET");
    let widths = vec!["600"; 219].join(" ");
    let mut objs: Vec<Vec<u8>> = vec![
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 /MediaBox [0 0 300 300] >>".to_vec(),
        b"<< /Type /Page /Parent 2 0 R /Contents 4 0 R /Resources << /Font << /F1 5 0 R /F2 8 0 R >> >> >>".to_vec(),
        format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()).into_bytes(),
        format!(
            "<< /Type /Font /Subtype /TrueType /BaseFont /OldHebrew /FirstChar 32 /LastChar 250 /Widths [{widths}] /Encoding /WinAnsiEncoding /FontDescriptor 6 0 R >>"
        )
        .into_bytes(),
        b"<< /Type /FontDescriptor /FontName /OldHebrew /Flags 4 /FontBBox [0 -250 1000 900] /ItalicAngle 0 /Ascent 900 /Descent -250 /CapHeight 700 /StemV 80 /FontFile2 7 0 R >>".to_vec(),
    ];
    let mut stream = format!("<< /Length {} >>\nstream\n", program.len()).into_bytes();
    stream.extend(&program);
    stream.extend_from_slice(b"\nendstream");
    objs.push(stream);
    objs.push(b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>".to_vec());
    let mut out = b"%PDF-1.7\n".to_vec();
    let mut offsets = Vec::new();
    for (i, o) in objs.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
        out.extend_from_slice(o);
        out.extend_from_slice(b"\nendobj\n");
    }
    let xref = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).as_bytes());
    for o in offsets {
        out.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objs.len() + 1).as_bytes());
    out
}

fn lines(a: &mut Automation, doc: u64) -> Vec<String> {
    ok(a, "text_lines", json!({ "doc": doc, "page": 1 }))["lines"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l["text"].as_str().unwrap().trim().to_owned())
        .collect()
}

#[test]
fn text_encoding_reads_legacy_hebrew_fonts_and_can_be_chosen() {
    let dir: PathBuf = std::env::temp_dir().join(format!("pdfcraft-automation-legacy-encoding-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    // "שלום" drawn left to right in visual order (ם ו ל ש), as content streams draw Hebrew;
    // "café" in Helvetica.
    std::fs::write(dir.join("hebrew.pdf"), hebrew_pdf("EDE5ECF9", "636166E9")).unwrap();
    let mut a = Automation::new().with_root(&dir).unwrap();
    let doc = ok(&mut a, "doc_open", json!({ "path": "hebrew.pdf" }))["doc"].as_u64().unwrap();

    assert_eq!(ok(&mut a, "text_encoding", json!({})), json!({ "encoding": "auto", "label": "Auto-detect" }));
    let auto = lines(&mut a, doc);
    assert!(auto.contains(&"שלום".to_owned()), "the font program draws Hebrew: {auto:?}");
    assert!(auto.contains(&"café".to_owned()), "Helvetica stays Latin: {auto:?}");

    assert_eq!(ok(&mut a, "text_encoding", json!({ "encoding": "declared" }))["encoding"], "declared");
    let declared = lines(&mut a, doc);
    assert!(declared.contains(&"íåìù".to_owned()), "WinAnsi as declared: {declared:?}");

    // A chosen code page applies to every font without ToUnicode, Helvetica included.
    assert_eq!(ok(&mut a, "text_encoding", json!({ "encoding": "iso-8859-8" }))["encoding"], "iso-8859-8");
    let forced = lines(&mut a, doc);
    assert!(forced.contains(&"שלום".to_owned()), "{forced:?}");
    assert!(forced.iter().any(|l| l.contains('י') && l.contains("caf")), "0xE9 is yod: {forced:?}");

    // A bad name is refused and changes nothing.
    assert!(a.call("text_encoding", &json!({ "encoding": "latin-9" })).is_err());
    assert_eq!(ok(&mut a, "text_encoding", json!({}))["encoding"], "iso-8859-8");

    ok(&mut a, "text_encoding", json!({ "encoding": "auto" }));
    assert_eq!(lines(&mut a, doc), auto, "back to auto reads as before");
    let _ = std::fs::remove_dir_all(&dir);
}
