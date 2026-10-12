//! #389 end to end: apply, full save, reopen the saved bytes, and look for the original scan the
//! way an image extractor would (`pdfimages`, PyMuPDF's `extract_image`, another viewer's "export
//! images"): every image the redacted page's resources can reach, followed into forms and masks,
//! and every image stream anywhere in the file. A superseded scan must be in neither, whether it
//! was a JPEG (removed whole: PdfCraft can't clear DCT pixels) or a Flate scan (cleared into a
//! copy), whether the page owns its /Resources, inherits them, or draws the scan through a form
//! without resources of its own. A scan another page still draws stays, for that page only.

use std::sync::Arc;

use pdfcraft_cos::{Dict, Document, ObjRef, Object, SaveOptions, Stream, write_full};

use super::tests::*;
use super::*;

/// A stand-in for a scanned JPEG: DCT data PdfCraft can't decode, so it can't be cleared and
/// the whole image is removed. The canary is what an extractor would recover.
const JPEG: &[u8] = b"\xff\xd8\xff\xe0\x00\x10JFIF\x00\x01 CANARY-SCAN-3H6W \xff\xd9";
const JPEG_DICT: &str = "/Type /XObject /Subtype /Image /Width 8 /Height 8 /ColorSpace /DeviceGray /BitsPerComponent 8 /Filter /DCTDecode";

/// An 8×2 gray scan whose samples are all distinct and non-zero, so a cleared pixel shows.
fn samples() -> Vec<u8> {
    (0..16u8).map(|i| 0x40 + i * 9).collect()
}

/// The scan as a Flate image object, and its raw (compressed) bytes.
fn flate_image(scan: &[u8]) -> (Vec<u8>, Vec<u8>) {
    let s = Stream::flate(Dict::new(), scan);
    let mut v = format!(
        "<< /Type /XObject /Subtype /Image /Width 8 /Height 2 /ColorSpace /DeviceGray /BitsPerComponent 8 /Filter /FlateDecode /Length {} >>\nstream\n",
        s.raw.len()
    )
    .into_bytes();
    v.extend_from_slice(&s.raw);
    v.extend_from_slice(b"\nendstream");
    (v, s.raw.as_ref().to_vec())
}

/// The scans the tests use: the object bytes, and how to recognise the original in a file.
enum Scan {
    Jpeg,
    Flate,
}

impl Scan {
    fn object(&self) -> Vec<u8> {
        match self {
            Scan::Jpeg => stream(JPEG_DICT, JPEG),
            Scan::Flate => flate_image(&samples()).0,
        }
    }

    /// Whether `s` is the original scan: the same stored bytes (what `pdfimages -j` writes out),
    /// or, decoded, the same samples (what survives a re-compression).
    fn is_original(&self, s: &Stream) -> bool {
        match self {
            Scan::Jpeg => s.raw.windows(JPEG.len()).any(|w| w == JPEG),
            Scan::Flate => s.raw.as_ref() == flate_image(&samples()).1.as_slice() || s.decoded().is_ok_and(|d| d == samples()),
        }
    }
}

/// Apply every mark, save in full and reopen the saved bytes.
fn apply_save_reopen(mut doc: Document) -> (Report, Document, Vec<u8>) {
    let r = apply(&mut doc, None).unwrap();
    let saved = write_full(&doc, &SaveOptions::default()).unwrap();
    (r, Document::open(Arc::new(saved.clone())).unwrap(), saved)
}

/// Every image stream page `page` can reach through its effective /Resources: its /XObject
/// images, the images of the forms it maps (followed with each form's own /Resources, else the
/// resources that drew it), and their /SMask and /Mask streams.
fn reachable_images(doc: &Document, page: usize) -> Vec<Stream> {
    fn walk(doc: &Document, res: &Dict, depth: usize, seen: &mut Vec<ObjRef>, out: &mut Vec<Stream>) {
        let Some(xo) = res.get(b"XObject").and_then(|x| doc.resolve(x).as_dict().cloned()) else { return };
        for (_, v) in xo.iter() {
            if let Object::Ref(r) = v {
                if seen.contains(r) {
                    continue;
                }
                seen.push(*r);
            }
            let Object::Stream(s) = &*doc.resolve(v) else { continue };
            match s.dict.name(b"Subtype") {
                Some(b"Image") => {
                    for key in [&b"SMask"[..], b"Mask"] {
                        if let Some(Object::Stream(m)) = s.dict.get(key).map(|m| doc.resolve(m)).as_deref() {
                            out.push(m.clone());
                        }
                    }
                    out.push(s.clone());
                }
                Some(b"Form") if depth < 16 => {
                    let own = s.dict.get(b"Resources").and_then(|r| doc.resolve(r).as_dict().cloned());
                    walk(doc, own.as_ref().unwrap_or(res), depth + 1, seen, out);
                }
                _ => {}
            }
        }
    }
    let p = &pdfcraft_model::pages(doc)[page];
    let res = p.dict.get(b"Resources").and_then(|r| doc.resolve(r).as_dict().cloned()).unwrap_or_default();
    let mut out = Vec::new();
    walk(doc, &res, 0, &mut Vec::new(), &mut out);
    out
}

/// Every image stream in the file, reachable or not.
fn file_images(doc: &Document) -> Vec<Stream> {
    doc.object_numbers()
        .into_iter()
        .filter_map(|n| match &*doc.get(ObjRef::new(n, doc.generation(n))) {
            Object::Stream(s) if s.dict.name(b"Subtype") == Some(b"Image") => Some(s.clone()),
            _ => None,
        })
        .collect()
}

/// The names page `page`'s content draws with `Do`.
fn drawn(doc: &Document, page: usize) -> Vec<Vec<u8>> {
    pdfcraft_content::parse(content(doc, page).as_bytes())
        .ops
        .iter()
        .filter(|op| op.op.as_slice() == b"Do")
        .filter_map(|op| op.operands.first().and_then(Object::as_name).map(<[u8]>::to_vec))
        .collect()
}

/// The redacted page reaches no copy of the original scan, and the saved file holds none.
fn assert_gone(scan: &Scan, doc: &Document, saved: &[u8], page: usize) {
    assert!(!reachable_images(doc, page).iter().any(|s| scan.is_original(s)), "page {} still reaches the original scan", page + 1);
    assert!(!file_images(doc).iter().any(|s| scan.is_original(s)), "the saved file still holds the original scan");
    if let Scan::Jpeg = scan {
        assert!(!saved.windows(JPEG.len()).any(|w| w == JPEG), "the JPEG's bytes are not anywhere in the saved file");
    }
}

/// For a Flate scan: page `page` reaches exactly one image, a copy with columns 0–2 of both rows
/// cleared (see [`DRAW`]) and every other pixel the scan's.
fn assert_cleared_copy(doc: &Document, page: usize) {
    let images = reachable_images(doc, page);
    assert_eq!(images.len(), 1, "page {} reaches one image, the cleared copy", page + 1);
    let scan = samples();
    let expect: Vec<u8> = scan.iter().enumerate().map(|(i, &v)| if i % 8 < 3 { 0 } else { v }).collect();
    assert_eq!(images[0].decoded().unwrap(), expect, "pixels 0–2 of both rows cleared, the rest kept");
}

/// The 8×2 scan drawn 16 × 4 pt at (10, 10): column `i` spans x 10 + 2i to 12 + 2i. [`MARK`]
/// covers both rows and x up to 15, so columns 0–2 (column 2 spans 14–16) overlap it and are
/// cleared, and column 3 (from 16) on keeps its samples.
const DRAW: &[u8] = b"q 16 0 0 4 10 10 cm /Im0 Do Q";
const MARK: [f64; 4] = [5.0, 5.0, 15.0, 20.0];

#[test]
fn a_partly_covered_jpeg_scan_cannot_be_extracted_after_save() {
    // The #389 report itself: a JPEG scan partly under a mark. PdfCraft can't clear DCT pixels,
    // so the image goes whole — and it must go from the resources and the file, not just from
    // the content stream.
    let mut doc = one_page(DRAW, "/XObject << /Im0 7 0 R >>", vec![Scan::Jpeg.object()]);
    mark(&mut doc, 0, &[MARK], "");
    let (r, doc, saved) = apply_save_reopen(doc);
    assert_eq!((r.images_removed, r.images_cleared), (1, 0), "{r:?}");
    assert!(drawn(&doc, 0).is_empty(), "{}", content(&doc, 0));
    assert_gone(&Scan::Jpeg, &doc, &saved, 0);
}

#[test]
fn a_partly_covered_flate_scan_leaves_only_its_cleared_copy_after_save() {
    // The lossless half of #389: the page draws a cleared copy, and no image in the reopened file
    // — compared by stored bytes and by decoded samples — is the original.
    let mut doc = one_page(DRAW, "/XObject << /Im0 7 0 R >>", vec![Scan::Flate.object()]);
    mark(&mut doc, 0, &[MARK], "");
    let (r, doc, saved) = apply_save_reopen(doc);
    assert_eq!((r.images_removed, r.images_cleared), (0, 1), "{r:?}");
    assert_gone(&Scan::Flate, &doc, &saved, 0);
    assert_cleared_copy(&doc, 0);
}

/// Two pages inheriting /Resources from the page-tree root, whose /XObject maps `root_xo`;
/// `extra` objects are numbered from 7.
fn inheriting(root_xo: &str, page1: &[u8], page2: &[u8], extra: Vec<Vec<u8>>) -> Document {
    let mut objs = vec![
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        format!("<< /Type /Pages /Kids [3 0 R 4 0 R] /Count 2 /MediaBox [0 0 300 300] /Resources << /XObject << {root_xo} >> >> >>").into_bytes(),
        b"<< /Type /Page /Parent 2 0 R /Contents 5 0 R >>".to_vec(),
        b"<< /Type /Page /Parent 2 0 R /Contents 6 0 R >>".to_vec(),
        stream("", page1),
        stream("", page2),
    ];
    objs.extend(extra);
    pdf(objs)
}

#[test]
fn an_inherited_jpeg_scan_cannot_be_extracted_after_save() {
    // The scan lives in the Pages node's /Resources and only page 2 draws it.
    let mut doc = inheriting("/Im0 7 0 R", b"0 g", DRAW, vec![Scan::Jpeg.object()]);
    mark(&mut doc, 1, &[MARK], "");
    let (r, doc, saved) = apply_save_reopen(doc);
    assert_eq!((r.images_removed, r.images_cleared), (1, 0), "{r:?}");
    assert_gone(&Scan::Jpeg, &doc, &saved, 1);
    assert_gone(&Scan::Jpeg, &doc, &saved, 0);
}

#[test]
fn an_inherited_scan_both_pages_draw_stays_for_the_unredacted_page_only() {
    // Both pages inherit and draw the scan; only page 2 is redacted. Page 1 must keep drawing the
    // original, and page 2 must not reach it any more, through its own resources or the tree.
    for scan in [Scan::Jpeg, Scan::Flate] {
        let mut doc = inheriting("/Im0 7 0 R", DRAW, DRAW, vec![scan.object()]);
        mark(&mut doc, 1, &[MARK], "");
        let (_, doc, _) = apply_save_reopen(doc);
        assert_eq!(drawn(&doc, 0), [b"Im0".to_vec()], "page 1 still draws the scan");
        assert!(reachable_images(&doc, 0).iter().any(|s| scan.is_original(s)), "page 1 still shows the original");
        assert!(!reachable_images(&doc, 1).iter().any(|s| scan.is_original(s)), "page 2 no longer reaches it");
        if let Scan::Flate = scan {
            assert_cleared_copy(&doc, 1);
        }
    }
}

#[test]
fn a_scan_drawn_by_a_form_without_resources_cannot_be_extracted_after_save() {
    // The page maps both the form and the scan; the form has no /Resources, so it draws the scan
    // with the page's. Rewriting the form must not leave the original reachable from the page's
    // /XObject, nor frozen into the rewritten form's resources.
    for scan in [Scan::Jpeg, Scan::Flate] {
        let form = stream("/Type /XObject /Subtype /Form /BBox [0 0 300 300]", DRAW);
        let mut doc = one_page(b"/Fm Do", "/XObject << /Fm 7 0 R /Im0 8 0 R >>", vec![form, scan.object()]);
        mark(&mut doc, 0, &[MARK], "");
        let (r, doc, saved) = apply_save_reopen(doc);
        assert_eq!(r.forms_rewritten, 1, "{r:?}");
        assert_eq!(drawn(&doc, 0).len(), 1, "the page draws the rewritten form: {}", content(&doc, 0));
        assert_gone(&scan, &doc, &saved, 0);
        match scan {
            Scan::Jpeg => assert!(reachable_images(&doc, 0).is_empty(), "nothing of the JPEG is left to draw"),
            Scan::Flate => assert_cleared_copy(&doc, 0),
        }
    }
}

#[test]
fn a_scan_drawn_by_a_form_without_resources_inherits_nothing_back_after_save() {
    // The same form, now with the form and the scan both in the Pages node's /Resources.
    for scan in [Scan::Jpeg, Scan::Flate] {
        let form = stream("/Type /XObject /Subtype /Form /BBox [0 0 300 300]", DRAW);
        let mut doc = inheriting("/Fm 7 0 R /Im0 8 0 R", b"/Fm Do", b"0 g", vec![form, scan.object()]);
        mark(&mut doc, 0, &[MARK], "");
        let (r, doc, saved) = apply_save_reopen(doc);
        assert_eq!(r.forms_rewritten, 1, "{r:?}");
        assert_gone(&scan, &doc, &saved, 0);
        assert_gone(&scan, &doc, &saved, 1);
    }
}

/// Two pages that each draw /Im0, the same image object 8, through `res1` and `res2` (their
/// /Resources entries, which may be the same indirect dictionary); extra objects from 8.
fn two_pages(res1: &str, res2: &str, extra: Vec<Vec<u8>>) -> Document {
    let mut objs = vec![
        b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(),
        b"<< /Type /Pages /Kids [3 0 R 4 0 R] /Count 2 /MediaBox [0 0 300 300] >>".to_vec(),
        format!("<< /Type /Page /Parent 2 0 R /Contents 5 0 R /Resources {res1} >>").into_bytes(),
        format!("<< /Type /Page /Parent 2 0 R /Contents 6 0 R /Resources {res2} >>").into_bytes(),
        stream("", DRAW),
        stream("", DRAW),
        b"<< /XObject << /Im0 8 0 R >> >>".to_vec(),
    ];
    objs.extend(extra);
    pdf(objs)
}

#[test]
fn an_image_shared_by_two_pages_stays_on_the_unredacted_page_only() {
    // One image object drawn by two pages; page 1 is redacted. Page 2 keeps showing the original
    // (it stays in the file, legitimately), and page 1 neither draws nor reaches it. Both
    // ways of sharing: each page with its own /Resources, and one indirect /Resources dictionary
    // both pages point at (what many producers write).
    let own = "<< /XObject << /Im0 8 0 R >> >>";
    for (res1, res2) in [(own, own), ("7 0 R", "7 0 R")] {
        for scan in [Scan::Jpeg, Scan::Flate] {
            let mut doc = two_pages(res1, res2, vec![scan.object()]);
            mark(&mut doc, 0, &[MARK], "");
            let (_, doc, _) = apply_save_reopen(doc);
            assert!(!drawn(&doc, 0).contains(&b"Im0".to_vec()), "page 1 no longer draws /Im0 ({res1})");
            assert!(!reachable_images(&doc, 0).iter().any(|s| scan.is_original(s)), "page 1 no longer reaches the original ({res1})");
            assert_eq!(drawn(&doc, 1), [b"Im0".to_vec()], "page 2 still draws the scan ({res2})");
            assert!(reachable_images(&doc, 1).iter().any(|s| scan.is_original(s)), "page 2 still shows the original ({res2})");
            if let Scan::Flate = scan {
                assert_cleared_copy(&doc, 0);
            }
        }
    }
}
