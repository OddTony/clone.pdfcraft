//! The redaction tools: full reports, partial vs complete results, and saving redacted output.

use std::path::{Path, PathBuf};

use pdfcraft_automation::{Automation, Content, ToolError};
use serde_json::{Value, json};

fn workdir(test: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("pdfcraft-automation-redact-{test}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn auto(dir: &Path) -> Automation {
    Automation::new().with_root(dir).unwrap().with_clock(|| 1_700_000_000)
}

fn ok(a: &mut Automation, tool: &str, args: Value) -> Value {
    match a.call(tool, &args) {
        Ok(mut c) => match c.remove(0) {
            Content::Json(v) => v,
            other => panic!("{tool}: expected JSON, got {other:?}"),
        },
        Err(e) => panic!("{tool} {args}: {e}"),
    }
}

fn texts(a: &mut Automation, doc: u64) -> Vec<String> {
    ok(a, "text_extract", json!({ "doc": doc }))["pages"].as_array().unwrap().iter().map(|p| p["text"].as_str().unwrap().to_string()).collect()
}

const SECRET: &str = "Zebra-4711";

/// A document of three pages (about 60 lines fit on one) with the secret on pages 1 and 3.
fn open_memo(a: &mut Automation, dir: &Path) -> u64 {
    let mut memo: Vec<String> = (0..150).map(|i| format!("Line {i}")).collect();
    memo[1] = format!("The code is {SECRET}.");
    memo[140] = format!("Again {SECRET}.");
    std::fs::write(dir.join("memo.txt"), memo.join("\n")).unwrap();
    let doc = ok(a, "doc_create", json!({ "from": "text", "path": "memo.txt" }))["doc"].as_u64().unwrap();
    ok(a, "doc_save", json!({ "doc": doc, "path": "memo.pdf" }));
    doc
}

#[test]
fn the_result_reports_what_was_removed_without_its_content() {
    let dir = workdir("report");
    let mut a = auto(&dir);
    let doc = open_memo(&mut a, &dir);
    ok(&mut a, "redact_mark", json!({ "doc": doc, "find": SECRET }));
    let r = ok(&mut a, "redact_apply", json!({ "doc": doc }));
    assert_eq!((r["status"].as_str(), r["applied"].as_u64(), r["marks_pending"].as_u64()), (Some("complete"), Some(2), Some(0)));
    assert_eq!((r["report"]["marks"].as_u64(), r["report"]["pages"].as_u64()), (Some(2), Some(2)));
    assert!(r["report"]["text_glyphs"].as_u64() >= Some(20), "{r}");
    assert_eq!((r["checked_at_apply"].as_bool(), r["undo"].clone()), (Some(true), Value::Null), "history is purged");
    assert!(r.get("verified").is_none(), "no claim of a guarantee: {r}");
    assert!(r["caveats"].as_array().is_some_and(|c| c.len() == 2 && c[0].as_str().is_some_and(|t| t.contains("pixels"))), "{r}");
    // The independent proof, shareable: statuses and counts, per region, never the content.
    let proof = &r["proof"];
    assert_eq!(
        (proof["passed"].as_bool(), proof["survivors"].as_u64(), proof["unverifiable_regions"].as_u64()),
        (Some(true), Some(0), Some(0)),
        "{r}"
    );
    let regions = proof["regions"].as_array().unwrap();
    assert_eq!(regions.len(), 2, "{r}");
    assert!(regions.iter().all(|g| g["status"] == "Verified" && g["page"].as_u64().is_some() && g["area"].is_array()), "{r}");
    assert!(proof["surfaces"].as_array().is_some_and(|s| s.iter().any(|x| x["surface"] == "Info")), "{r}");
    // apply sanitizes too: the counts say what went.
    assert!(r["sanitized"]["total"].as_u64().is_some() && r["sanitized"]["counts"].is_object(), "{r}");
    assert!(r["sanitized"]["layers"].is_u64(), "layers are counted, never named: {r}");
    assert!(!r.to_string().contains(SECRET) && !r.to_string().contains("4711"), "{r}");
    assert!(texts(&mut a, doc).iter().all(|t| !t.contains("4711")), "the text cache followed the edit");
}

#[test]
fn applying_some_pages_is_reported_as_partial() {
    let dir = workdir("partial");
    let mut a = auto(&dir);
    let doc = open_memo(&mut a, &dir);
    ok(&mut a, "redact_mark", json!({ "doc": doc, "find": SECRET }));
    let r = ok(&mut a, "redact_apply", json!({ "doc": doc, "pages": [1] }));
    assert_eq!((r["status"].as_str(), r["applied"].as_u64(), r["marks_pending"].as_u64()), (Some("partial"), Some(1), Some(1)));
    let t = texts(&mut a, doc);
    assert!(!t[0].contains(SECRET) && t[2].contains(SECRET), "page 3 still has its content until its mark is applied");
}

#[test]
fn a_failed_apply_changes_nothing_and_names_no_content() {
    let dir = workdir("failed");
    let mut a = auto(&dir);
    let doc = open_memo(&mut a, &dir);
    ok(&mut a, "redact_mark", json!({ "doc": doc, "find": SECRET }));
    ok(&mut a, "redact_apply", json!({ "doc": doc }));
    // Nothing left to apply: an error, and the message is about marks, not content.
    let Err(ToolError::Failed(m)) = a.call("redact_apply", &json!({ "doc": doc })) else { panic!("expected a failure") };
    assert!(m.contains("no redaction marks") && !m.contains(SECRET), "{m}");
}

#[test]
fn redacted_output_does_not_replace_the_source_unless_asked() {
    let dir = workdir("save");
    let mut a = auto(&dir);
    let doc = open_memo(&mut a, &dir);
    let original = std::fs::read(dir.join("memo.pdf")).unwrap();
    ok(&mut a, "redact_mark", json!({ "doc": doc, "find": SECRET }));
    ok(&mut a, "redact_apply", json!({ "doc": doc }));
    // Saving in place (no path, or the same path) is refused and the source stays as it was.
    for args in [json!({ "doc": doc }), json!({ "doc": doc, "path": "memo.pdf" })] {
        let Err(ToolError::Failed(m)) = a.call("doc_save", &args) else { panic!("expected a refusal") };
        assert!(m.contains("overwrite_source"), "{m}");
    }
    assert_eq!(std::fs::read(dir.join("memo.pdf")).unwrap(), original);
    let r = ok(&mut a, "doc_save", json!({ "doc": doc, "path": "memo-redacted.pdf" }));
    assert_eq!(r["incremental"], false);
    let saved = std::fs::read(dir.join("memo-redacted.pdf")).unwrap();
    assert_eq!(saved.windows(5).filter(|w| *w == b"%%EOF").count(), 1, "a full rewrite, one revision");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(std::fs::metadata(dir.join("memo-redacted.pdf")).unwrap().permissions().mode() & 0o777, 0o600);
    }
    assert_eq!(std::fs::read(dir.join("memo.pdf")).unwrap(), original, "the source is untouched");
    // Saved: the document now lives at the new file, and saving in place is ordinary again.
    ok(&mut a, "doc_save", json!({ "doc": doc }));
}

#[test]
fn overwriting_the_source_needs_explicit_consent() {
    let dir = workdir("overwrite");
    let mut a = auto(&dir);
    let doc = open_memo(&mut a, &dir);
    ok(&mut a, "redact_mark", json!({ "doc": doc, "find": SECRET }));
    ok(&mut a, "redact_apply", json!({ "doc": doc }));
    let r = ok(&mut a, "doc_save", json!({ "doc": doc, "overwrite_source": true }));
    assert_eq!(r["incremental"], false);
    let saved = std::fs::read(dir.join("memo.pdf")).unwrap();
    assert_eq!(saved.windows(5).filter(|w| *w == b"%%EOF").count(), 1);
    let again = ok(&mut a, "doc_open", json!({ "path": "memo.pdf" }))["doc"].as_u64().unwrap();
    assert!(texts(&mut a, again).iter().all(|t| !t.contains("4711")));
}

#[test]
fn another_spelling_of_the_source_path_is_still_the_source() {
    // No root: paths are taken as written, so the source must be recognised however it is spelled.
    let dir = workdir("spelling");
    std::fs::create_dir_all(dir.join("sub")).unwrap();
    let mut a = Automation::new().with_clock(|| 1_700_000_000);
    let mut memo: Vec<String> = (0..20).map(|i| format!("Line {i}")).collect();
    memo[1] = format!("The code is {SECRET}.");
    std::fs::write(
        dir.join("memo.txt"),
        memo.join(
            "
",
        ),
    )
    .unwrap();
    let source = dir.join("memo.pdf");
    let doc = ok(&mut a, "doc_create", json!({ "from": "text", "path": dir.join("memo.txt") }))["doc"].as_u64().unwrap();
    ok(&mut a, "doc_save", json!({ "doc": doc, "path": source }));
    let original = std::fs::read(&source).unwrap();
    ok(&mut a, "redact_mark", json!({ "doc": doc, "find": SECRET }));
    ok(&mut a, "redact_apply", json!({ "doc": doc }));
    for spelling in [dir.join("sub").join("..").join("memo.pdf"), dir.join(".").join("memo.pdf")] {
        let Err(ToolError::Failed(m)) = a.call("doc_save", &json!({ "doc": doc, "path": spelling })) else {
            panic!("{} was saved over the source", spelling.display())
        };
        assert!(m.contains("overwrite_source"), "{m}");
    }
    assert_eq!(std::fs::read(&source).unwrap(), original, "the source is untouched");
}

#[test]
fn sanitizing_is_not_saved_over_the_source_without_consent_either() {
    let dir = workdir("sanitize-save");
    let mut a = auto(&dir);
    let doc = open_memo(&mut a, &dir);
    let original = std::fs::read(dir.join("memo.pdf")).unwrap();
    ok(&mut a, "doc_remove_hidden", json!({ "doc": doc }));
    let Err(ToolError::Failed(m)) = a.call("doc_save", &json!({ "doc": doc })) else { panic!("expected a refusal") };
    assert!(m.contains("overwrite_source"), "{m}");
    assert_eq!(std::fs::read(dir.join("memo.pdf")).unwrap(), original);
    ok(&mut a, "doc_save", json!({ "doc": doc, "path": "memo-clean.pdf" }));
}

/// A two-page PDF: page 1 a line of text, page 2 a full-page JPEG "scan" of `jpeg`.
fn scanned_pdf(jpeg: &[u8]) -> Vec<u8> {
    let content2 = b"q 300 0 0 300 0 0 cm /Im0 Do Q";
    let content1 = b"BT /F1 12 Tf 20 260 Td (Page one) Tj ET";
    let objs: Vec<Vec<u8>> = vec![
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R 4 0 R] /Count 2 /MediaBox [0 0 300 300] >>".to_vec(),
        b"<< /Type /Page /Parent 2 0 R /Contents 5 0 R /Resources << /Font << /F1 << /Type /Font /Subtype /Type1 /BaseFont /Helvetica >> >> >> >>".to_vec(),
        b"<< /Type /Page /Parent 2 0 R /Contents 6 0 R /Resources << /XObject << /Im0 7 0 R >> >> >>".to_vec(),
        [format!("<< /Length {} >>\nstream\n", content1.len()).as_bytes(), content1, b"\nendstream"].concat(),
        [format!("<< /Length {} >>\nstream\n", content2.len()).as_bytes(), content2, b"\nendstream"].concat(),
        [
            format!("<< /Type /XObject /Subtype /Image /Width 64 /Height 64 /ColorSpace /DeviceGray /BitsPerComponent 8 /Filter /DCTDecode /Length {} >>\nstream\n", jpeg.len()).as_bytes(),
            jpeg,
            b"\nendstream",
        ]
        .concat(),
    ];
    let mut out = b"%PDF-1.7\n".to_vec();
    let mut offs = Vec::new();
    for (i, o) in objs.iter().enumerate() {
        offs.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
        out.extend_from_slice(o);
        out.extend_from_slice(b"\nendobj\n");
    }
    let x = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).as_bytes());
    for o in offs {
        out.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{x}\n%%EOF\n", objs.len() + 1).as_bytes());
    out
}

#[test]
fn a_partly_redacted_jpeg_scan_is_not_in_the_saved_file() {
    // #389, the reporter's script on a synthetic scan: mark part of a JPEG page, apply, sanitize,
    // save in full, then look for the scan in the output the way `pdfimages -j` would. PdfCraft
    // can't clear JPEG pixels, so the image goes whole; it must go from the file, not only from
    // the page's content stream.
    let dir = workdir("jpeg-scan");
    let pixels: Vec<u8> = (0..64u32 * 64).map(|i| ((i * 37 + i / 64 * 11) % 251) as u8).collect();
    let mut jpeg = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 90).encode(&pixels, 64, 64, image::ExtendedColorType::L8).unwrap();
    std::fs::write(dir.join("in.pdf"), scanned_pdf(&jpeg)).unwrap();
    let mut a = auto(&dir);
    let doc = ok(&mut a, "doc_open", json!({ "path": "in.pdf" }))["doc"].as_u64().unwrap();
    assert_eq!(ok(&mut a, "page_images", json!({ "doc": doc, "page": 2 }))["count"], 1);
    ok(&mut a, "redact_mark", json!({ "doc": doc, "page": 2, "rect": [30, 30, 150, 60] }));
    let r = ok(&mut a, "redact_apply", json!({ "doc": doc }));
    assert_eq!((r["status"].as_str(), r["report"]["images_removed"].as_u64()), (Some("complete"), Some(1)), "{r}");
    ok(&mut a, "doc_remove_hidden", json!({ "doc": doc }));
    let s = ok(&mut a, "doc_save", json!({ "doc": doc, "path": "out.pdf", "full": true }));
    assert_eq!(s["incremental"], false, "{s}");
    let saved = std::fs::read(dir.join("out.pdf")).unwrap();
    // The JPEG's entropy-coded data (past its headers) is what an extractor writes out.
    let middle = jpeg.get(jpeg.len() / 2..jpeg.len() / 2 + 32).unwrap();
    assert!(!saved.windows(middle.len()).any(|w| w == middle), "the scan's JPEG data is not in the saved file");
    let again = ok(&mut a, "doc_open", json!({ "path": "out.pdf" }))["doc"].as_u64().unwrap();
    assert_eq!(ok(&mut a, "page_images", json!({ "doc": again, "page": 2 }))["count"], 0, "the reopened page has no image left");
    assert!(texts(&mut a, again)[0].contains("Page one"), "the unmarked page is untouched");
}
