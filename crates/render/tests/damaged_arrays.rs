//! Synthetic regressions for unreadable slots in page-tree and page-content arrays (#2660).

use std::io::Write;
use std::sync::Arc;

use hayro_syntax::Pdf;
use hayro_syntax::object::{Array, FromBytes};
use pdfcraft_render::{PageRenderer, RenderConfig, RenderRequest, inspect};

const LEFT: &str = "0 0 1 rg 0 0 20 40 re f";
const RIGHT: &str = "1 0 0 rg 20 0 20 40 re f";

const BAD_ENTRIES: &[(&str, usize)] = &[
    ("99 0 R", 1),
    ("null", 1),
    ("42", 1),
    ("[null]", 1),
    ("+", 1),
    ("-", 1),
    (".", 1),
    ("+.", 1),
    ("-.", 1),
    ("+ - .", 3),
    ("99 0 R null false", 3),
    ("2147483648 0 R", 1),
];

#[test]
fn page_tree_retains_readable_siblings_in_order() {
    for &(bad, count) in BAD_ENTRIES {
        for position in [0, 3, 6] {
            let bytes = fixture(6, Some((bad, position)), None);
            let pdf = Pdf::new(bytes.clone()).unwrap();
            assert_eq!(pdf.pages().len(), 6, "bad slot {bad} at {position}");
            for (i, page) in pdf.pages().iter().enumerate() {
                let content = std::str::from_utf8(page.page_stream().unwrap()).unwrap();
                assert!(content.starts_with(&format!("% page {i}\n")), "page order changed: {content}");
            }
            let info = inspect(bytes, None).unwrap();
            assert_eq!(info.pages.len(), 6);
            assert!(info.warnings.iter().any(|w| w.contains(&format!("Skipped {count} unreadable page-tree"))), "{:?}", info.warnings);
        }
    }
}

#[test]
fn page_tree_counts_consecutive_unreadable_slots() {
    for bad in ["99 0 R null false", "+ - ."] {
        let info = inspect(fixture(6, Some((bad, 3)), None), None).unwrap();
        assert_eq!(info.pages.len(), 6);
        assert!(info.warnings.iter().any(|w| w.contains("Skipped 3 unreadable page-tree")), "{:?}", info.warnings);
    }
}

#[test]
fn joined_content_retains_streams_after_unreadable_slots() {
    for &(bad, _) in BAD_ENTRIES {
        for position in [0, 1, 2] {
            let pdf = Pdf::new(fixture(1, None, Some((bad, position)))).unwrap();
            let content = std::str::from_utf8(pdf.pages()[0].page_stream().unwrap()).unwrap();
            assert!(content.contains(LEFT), "left stream lost with {bad} at {position}");
            assert!(content.contains(RIGHT), "right stream lost with {bad} at {position}");
            assert!(content.find(LEFT) < content.find(RIGHT), "stream order changed");
        }
    }
}

#[test]
fn streaming_render_retains_content_after_unreadable_slots() {
    for &(bad, count) in BAD_ENTRIES {
        for position in [0, 1, 2] {
            let bytes = fixture(1, None, Some((bad, position)));
            let mut renderer = PageRenderer::new(bytes.clone(), RenderConfig::default());
            let rendered = renderer.render(RenderRequest { page: 0, scale: 1.0, ..Default::default() });
            assert!(rendered.error.is_none(), "{:?}", rendered.error);
            assert_eq!((rendered.width, rendered.height), (40, 40));
            for (x, color) in [(5, [0, 0, 255, 255]), (30, [255, 0, 0, 255])] {
                let offset = (20 * 40 + x) * 4;
                assert_eq!(&rendered.rgba[offset..offset + 4], &color, "bad slot {bad} at {position}, pixel {x}");
            }
            let info = inspect(bytes, None).unwrap();
            assert!(info.warnings.iter().any(|w| w.contains(&format!("Skipped {count} unreadable page-content"))), "{:?}", info.warnings);
        }
    }
}

#[test]
fn clean_arrays_need_no_recovery_warning() {
    let info = inspect(fixture(6, None, None), None).unwrap();
    assert_eq!(info.pages.len(), 6);
    assert!(info.warnings.is_empty(), "{:?}", info.warnings);
}

#[test]
fn positional_arrays_still_reject_unreadable_coordinates() {
    let array = Array::from_bytes(b"[0 null 40 40 80]").unwrap();
    assert!(<[f32; 4]>::try_from(array).is_err());
}

#[test]
fn streaming_content_reader_retains_streams_after_unparseable_slots() {
    for &(bad, _) in BAD_ENTRIES {
        for position in [0, 1, 2] {
            let pdf = Pdf::new(fixture(1, None, Some((bad, position)))).unwrap();
            let mut reader = pdf.pages()[0].content_reader();
            let mut bytes = Vec::new();
            // Read in small chunks; bound the loop so a failure cannot hang the test.
            for _ in 0..32 {
                if reader.read_into(&mut bytes, 7) == 0 {
                    break;
                }
            }
            assert_eq!(reader.read_into(&mut bytes, 7), 0, "reader did not finish");
            assert!(!reader.truncated());
            let content = std::str::from_utf8(&bytes).unwrap();
            assert!(content.contains(LEFT), "left stream lost with {bad} at {position}");
            assert!(content.contains(RIGHT), "right stream lost with {bad} at {position}");
            assert!(content.find(LEFT) < content.find(RIGHT), "stream order changed");
        }
    }
}

#[test]
fn resolved_entries_preserve_failed_parse_slots_and_valid_signed_numbers() {
    let array = Array::from_bytes(b"[+12 + % unreadable slot\n -3 -. .5 . 7 + ]").unwrap();
    let mut entries = array.resolved_entries::<f32>();
    let expected = vec![Some(12.0), None, Some(-3.0), Some(0.0), Some(0.5), Some(0.0), Some(7.0), None];
    assert_eq!(entries.by_ref().collect::<Vec<_>>(), expected);
    assert!(entries.next().is_none());
    assert!(entries.next().is_none());
}

#[test]
fn resolved_entries_do_not_split_overflowing_references_into_numbers() {
    let array = Array::from_bytes(b"[1 2147483648 0 R 2]").unwrap();
    assert_eq!(array.resolved_entries::<f32>().collect::<Vec<_>>(), vec![Some(1.0), None, Some(2.0)]);
}

// A complete xref table, with deliberately absent object 99. Build the bytes directly so
// a writer cannot repair the malformed references before the reader under test sees them.
fn fixture(pages: usize, bad_kid: Option<(&str, usize)>, bad_content: Option<(&str, usize)>) -> Arc<Vec<u8>> {
    let mut kids: Vec<_> = (0..pages).map(|i| format!("{} 0 R", 3 + 3 * i)).collect();
    if let Some((entry, position)) = bad_kid {
        kids.insert(position, entry.into());
    }
    let mut objects =
        vec![b"<< /Type /Catalog /Pages 2 0 R >>".to_vec(), format!("<< /Type /Pages /Count {pages} /Kids [{} ] >>", kids.join(" ")).into_bytes()];
    for i in 0..pages {
        let first = 4 + 3 * i;
        let mut contents = vec![format!("{first} 0 R"), format!("{} 0 R", first + 1)];
        if let Some((entry, position)) = bad_content {
            contents.insert(position, entry.into());
        }
        objects.push(format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 40 40] /Contents [{} ] >>", contents.join(" ")).into_bytes());
        for content in [format!("% page {i}\n{LEFT}"), RIGHT.into()] {
            objects.push(format!("<< /Length {} >>\nstream\n{content}\nendstream", content.len()).into_bytes());
        }
    }
    let mut bytes = b"%PDF-1.7\n".to_vec();
    let mut offsets = vec![0];
    for (i, object) in objects.iter().enumerate() {
        offsets.push(bytes.len());
        writeln!(bytes, "{} 0 obj", i + 1).unwrap();
        bytes.extend_from_slice(object);
        bytes.extend_from_slice(b"\nendobj\n");
    }
    let xref = bytes.len();
    writeln!(bytes, "xref\n0 {}\n0000000000 65535 f ", offsets.len()).unwrap();
    for offset in &offsets[1..] {
        writeln!(bytes, "{offset:010} 00000 n ").unwrap();
    }
    write!(bytes, "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", offsets.len()).unwrap();
    Arc::new(bytes)
}
