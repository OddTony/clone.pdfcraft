//! Keep compatibility-reader diagnostics useful when one damaged file has many bad objects.
//! This integration binary owns its logger; the lock isolates captures between tests, while
//! lopdf's default Rayon workers still execute concurrently inside each read.

use std::sync::{Arc, Mutex, Once};

use log::{Level, LevelFilter, Log, Metadata, Record};
use lopdf::{Document, LoadOptions};

static TEST_LOCK: Mutex<()> = Mutex::new(());
static RECORDS: Mutex<Vec<(Level, String)>> = Mutex::new(Vec::new());
static INIT: Once = Once::new();
struct Capture;
impl Log for Capture {
    fn enabled(&self, metadata: &Metadata<'_>) -> bool {
        metadata.target() == "lopdf::reader"
    }
    fn log(&self, record: &Record<'_>) {
        if self.enabled(record.metadata()) {
            RECORDS.lock().unwrap_or_else(|e| e.into_inner()).push((record.level(), record.args().to_string()));
        }
    }
    fn flush(&self) {}
}

fn start_capture() {
    INIT.call_once(|| {
        log::set_logger(&Capture).unwrap();
        log::set_max_level(LevelFilter::Trace);
    });
    RECORDS.lock().unwrap_or_else(|e| e.into_inner()).clear();
}

fn records() -> Vec<(Level, String)> {
    RECORDS.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

fn fixture(bad_objects: usize, bad_streams: usize) -> Vec<u8> {
    // The page is readable even though unrelated indirect objects are not.
    let names = if bad_objects > 0 { " /Names 6 0 R" } else { "" };
    let mut objects = vec![
        format!("<< /Type /Catalog /Pages 2 0 R{names} >>").into_bytes(),
        b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_vec(),
        b"<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 300] /Resources << /Font << /F1 5 0 R >> >> /Contents 4 0 R >>".to_vec(),
        b"<< /Length 38 >>\nstream\nBT /F1 12 Tf 20 150 Td (Visible) Tj ET\nendstream".to_vec(),
        b"<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_vec(),
    ];
    objects.extend((0..bad_objects).map(|_| b"not-a-pdf-object".to_vec()));
    // The member-header offset lies past the one-byte decoded content.
    objects.extend((0..bad_streams).map(|_| b"<< /Type /ObjStm /N 1 /First 99 /Length 1 >>\nstream\nx\nendstream".to_vec()));
    let mut pdf = b"%PDF-1.7\n".to_vec();
    let mut offsets = Vec::new();
    for (i, object) in objects.iter().enumerate() {
        offsets.push(pdf.len());
        pdf.extend_from_slice(format!("{} 0 obj\n", i + 1).as_bytes());
        pdf.extend_from_slice(object);
        pdf.extend_from_slice(b"\nendobj\n");
    }
    let xref = pdf.len();
    pdf.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes());
    for offset in offsets {
        pdf.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    pdf.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objects.len() + 1).as_bytes());
    pdf
}

fn compatibility_fixture() -> Vec<u8> {
    // A known producer mistake: startxref points just after "xref", so COS records a repair
    // and inspect uses the eager compatibility reader. The ordinary parser repairs it too.
    let pdf = String::from_utf8(fixture(1250, 0)).unwrap();
    let xref = pdf.find("xref\n").unwrap();
    let start = pdf.rfind("startxref\n").unwrap();
    format!("{}startxref\n{}\n%%EOF\n", &pdf[..start], xref + 4).into_bytes()
}

fn assert_bounded(messages: &[(Level, String)], total: usize, streams: usize) {
    let details = messages.iter().filter(|(_, s)| s.starts_with("Object load error") || s.starts_with("Object stream load error")).count();
    assert_eq!(details, 8, "expected bounded details, captured {} records", messages.len());
    let summaries: Vec<_> = messages.iter().filter(|(_, s)| s.starts_with("Object loading:")).collect();
    assert_eq!(summaries.len(), 1);
    assert!(summaries[0].1.contains(&format!("{total} failed objects ({streams} object streams)")), "{summaries:?}");
    assert!(summaries[0].1.contains(&format!("{} additional messages suppressed", total - 8)), "{summaries:?}");
    assert_eq!(messages.len(), 9, "no extra per-object log sites should bypass the budget");
    assert!(messages.iter().all(|(level, _)| *level == Level::Error));
}

#[test]
fn malformed_objects_log_bounded_details_and_an_exact_summary() {
    let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    start_capture();
    let doc = Document::load_mem(&fixture(1250, 0)).unwrap();
    assert_eq!(doc.get_pages().len(), 1);
    assert!(doc.get_object((5, 0)).is_ok(), "readable objects are retained");
    assert_bounded(&records(), 1250, 0);
}

#[test]
fn ordinary_and_object_stream_failures_share_one_log_budget() {
    let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    start_capture();
    let doc = Document::load_mem(&fixture(20, 30)).unwrap();
    assert_eq!(doc.get_pages().len(), 1);
    assert_bounded(&records(), 50, 30);
}

#[test]
fn compatibility_inspection_gets_a_fresh_budget_on_every_open() {
    let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let pdf = Arc::new(compatibility_fixture());
    for _ in 0..2 {
        start_capture();
        let info = pdfcraft_render::inspect(pdf.clone(), None).unwrap();
        assert_eq!(info.pages.len(), 1);
        let messages = records();
        assert!(messages.iter().any(|(level, text)| *level == Level::Warn && text.starts_with("Cross-reference offset ")));
        let object_messages: Vec<_> = messages.into_iter().filter(|(_, text)| !text.starts_with("Cross-reference offset ")).collect();
        assert_bounded(&object_messages, 1250, 0);
    }
}

#[test]
fn healthy_and_small_damaged_files_keep_their_diagnostics() {
    let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    start_capture();
    Document::load_mem(&fixture(0, 0)).unwrap();
    assert!(records().is_empty());
    for count in [1, 3, 8] {
        start_capture();
        Document::load_mem(&fixture(count, 0)).unwrap();
        let messages = records();
        assert_eq!(messages.len(), count);
        assert!(messages.iter().all(|(level, s)| *level == Level::Error && s.starts_with("Object load error at offset ")));
    }
}

#[test]
fn strict_loading_still_refuses_malformed_objects() {
    let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    for (objects, streams) in [(1250, 0), (0, 1250)] {
        start_capture();
        let result = Document::load_mem_with_options(&fixture(objects, streams), LoadOptions { strict: true, ..Default::default() });
        assert!(result.is_err());
        let messages = records();
        assert!(!messages.is_empty());
        assert!(messages.len() <= 9, "strict Rayon reads also share a bounded budget");
        assert!(messages.iter().all(|(level, _)| *level == Level::Error));
    }
}

#[test]
fn concurrent_document_reads_have_independent_budgets() {
    let _guard = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    start_capture();
    let a = std::thread::spawn(|| Document::load_mem(&fixture(50, 0)).unwrap().get_pages().len());
    let b = std::thread::spawn(|| Document::load_mem(&fixture(50, 0)).unwrap().get_pages().len());
    assert_eq!(a.join().unwrap(), 1);
    assert_eq!(b.join().unwrap(), 1);
    let messages = records();
    assert_eq!(messages.len(), 18, "two reads each get eight details and one summary");
    assert_eq!(messages.iter().filter(|(_, s)| s.starts_with("Object loading:") && s.contains("42 additional messages suppressed")).count(), 2);
}
