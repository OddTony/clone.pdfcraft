//! Bounded session search over working revisions, using the existing renderer and matcher.
use crate::{DocId, Document};
#[cfg(target_arch = "wasm32")]
use pdfcraft_render::Retiring;
use pdfcraft_render::{PageText, RenderConfig, RenderPool, RenderRequest, RequestKind};
use std::{ops::Range, sync::Arc};

pub const MAX_RESULTS: usize = 2_000;
pub const MAX_QUERY_BYTES: usize = 4_096;
pub const MAX_WARNINGS: usize = 100;

/// Extraction safety limits must not turn an incomplete page into an empty success.
pub fn extracted_text(result: pdfcraft_render::RenderedPage) -> Result<Arc<PageText>, String> {
    if let Some(error) = result.error {
        return Err(error);
    }
    if result.warnings.contains(&pdfcraft_render::RenderWarning::ContentTruncated) {
        return Err("text extraction exceeded the page content safety limit; results are incomplete".into());
    }
    result.text.ok_or_else(|| "text extraction returned no result".into())
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Options {
    pub query: String,
    pub case_sensitive: bool,
    pub whole_words: bool,
}

#[derive(Clone)]
pub struct Snapshot {
    pub id: DocId,
    pub generation: u64,
    pub name: String,
    pub pages: usize,
    pub bytes: Arc<Vec<u8>>,
    pub password: Option<Arc<str>>,
    pub layers: Arc<Vec<(i32, i32, bool)>>,
    pub hide_comments: bool,
}
impl Snapshot {
    pub fn new(doc: &Document) -> Self {
        let end = doc.name.floor_char_boundary(doc.name.len().min(1024));
        let mut name = doc.name.get(..end).unwrap_or_default().to_owned();
        if end < doc.name.len() {
            name.push('…');
        }
        Self {
            id: doc.id,
            generation: doc.edit_generation(),
            name,
            pages: doc.info.pages.len(),
            bytes: doc.bytes.clone(),
            password: doc.password.as_deref().map(Arc::from),
            layers: doc.config.layers.clone(),
            hide_comments: doc.config.hide_comments,
        }
    }
    pub fn current(&self, doc: &Document) -> bool {
        self.id == doc.id
            && self.generation == doc.edit_generation()
            && Arc::ptr_eq(&self.bytes, &doc.bytes)
            && self.layers == doc.config.layers
            && self.hide_comments == doc.config.hide_comments
    }
}

#[derive(Clone, Debug)]
pub struct Hit {
    pub doc: DocId,
    pub generation: u64,
    /// Internal page index; public tools convert it to 1-based numbering.
    pub page: usize,
    pub range: Range<usize>,
    pub page_match: usize,
    pub before: String,
    pub text: String,
    pub after: String,
    pub text_truncated: bool,
    pub rects: Vec<[f32; 4]>,
    pub geometry_truncated: bool,
}

/// Copy bounded Unicode context with the renderer's word gaps and normalised line breaks.
fn excerpt(text: &PageText, range: Range<usize>) -> (String, bool) {
    let mut out = String::new();
    let mut previous = None;
    for i in range.clone() {
        let Some(glyph) = text.glyphs.get(i) else { break };
        let line = text.line_of.get(i).copied();
        if i != range.start && (previous != line || text.space_before.get(i).copied().unwrap_or(false)) {
            if out.len() >= 512 {
                return (out, true);
            }
            out.push(' ');
        }
        for c in glyph.text.chars() {
            if out.len().saturating_add(c.len_utf8()) > 512 {
                return (out, true);
            }
            out.push(c);
        }
        previous = line;
    }
    (out, false)
}

/// The matching algorithm lives in PageText. Both frontends use this presentation adapter.
pub fn hits(text: &PageText, snapshot: &Snapshot, page: usize, options: &Options, limit: usize) -> Vec<Hit> {
    hits_skipping(text, snapshot, page, options, limit, 0)
}

pub fn hits_skipping(text: &PageText, snapshot: &Snapshot, page: usize, options: &Options, limit: usize, skip: usize) -> Vec<Hit> {
    text.find_opts_skipping(&options.query, options.case_sensitive, options.whole_words, limit.min(MAX_RESULTS.saturating_add(1)), skip)
        .into_iter()
        .enumerate()
        .map(|(ordinal, range)| {
            let (before, _) = excerpt(text, range.start.saturating_sub(36)..range.start);
            let (hit, text_truncated) = excerpt(text, range.clone());
            let (after, _) = excerpt(text, range.end..range.end.saturating_add(36));
            // Geometry output is bounded separately from the complete glyph range.
            let all_rects = text.line_rects(range.start..range.end.min(range.start.saturating_add(4096)));
            let geometry_truncated = range.end.saturating_sub(range.start) > 4096 || all_rects.len() > 256;
            let rects = all_rects.into_iter().take(256).collect();
            Hit {
                doc: snapshot.id,
                generation: snapshot.generation,
                page,
                range,
                page_match: skip.saturating_add(ordinal),
                before,
                text: hit,
                after,
                text_truncated,
                rects,
                geometry_truncated,
            }
        })
        .collect()
}

#[derive(Clone, Debug)]
pub struct Warning {
    pub doc: DocId,
    pub page: usize,
    pub error: String,
    pub error_truncated: bool,
}
pub fn warning(doc: DocId, page: usize, mut error: String) -> Warning {
    let end = error.floor_char_boundary(error.len().min(1024));
    let error_truncated = end < error.len();
    error.truncate(end);
    Warning { doc, page, error, error_truncated }
}

/// At most one coordinator, one extraction worker and one page request, including workers that are
/// still exiting. Text is released after each page; only capped snippets/geometry are retained.
/// Native extraction runs off the frame thread. Web uses RenderPool's established inline path.
#[derive(Default)]
pub struct Scan {
    pub documents: Vec<Snapshot>,
    pub options: Options,
    pub results: Vec<Hit>,
    pub searched: usize,
    pub warnings: Vec<Warning>,
    pub warning_count: usize,
    pub truncated: bool,
    pub finished: bool,
    pub cancelled: bool,
    #[cfg(target_arch = "wasm32")]
    document: usize,
    #[cfg(target_arch = "wasm32")]
    page: usize,
    #[cfg(target_arch = "wasm32")]
    pool: Option<RenderPool>,
    #[cfg(target_arch = "wasm32")]
    retiring: Option<Retiring>,
    #[cfg(target_arch = "wasm32")]
    pending: bool,
    #[cfg(not(target_arch = "wasm32"))]
    worker: Option<std::thread::JoinHandle<()>>,
    #[cfg(not(target_arch = "wasm32"))]
    receiver: Option<std::sync::mpsc::Receiver<Event>>,
    #[cfg(not(target_arch = "wasm32"))]
    cancel: Option<Arc<std::sync::atomic::AtomicBool>>,
}
impl Scan {
    pub fn reset(&mut self, documents: Vec<Snapshot>, options: Options) {
        #[cfg(target_arch = "wasm32")]
        self.retire();
        #[cfg(not(target_arch = "wasm32"))]
        self.cancel_worker();
        self.documents = documents;
        self.options = options;
        self.results.clear();
        self.searched = 0;
        self.warnings.clear();
        self.warning_count = 0;
        self.truncated = false;
        self.cancelled = false;
        self.finished = self.options.query.trim().is_empty() || self.options.query.len() > MAX_QUERY_BYTES;
        #[cfg(target_arch = "wasm32")]
        {
            self.document = 0;
            self.page = 0;
            self.pending = false;
        }
    }
    /// Cancel without discarding already displayed partial results. A later reset starts fresh.
    pub fn cancel(&mut self) {
        #[cfg(not(target_arch = "wasm32"))]
        self.cancel_worker();
        #[cfg(target_arch = "wasm32")]
        self.retire();
        self.finished = true;
        self.cancelled = true;
    }
    pub fn unchanged(&self, documents: &[Snapshot], options: &Options) -> bool {
        self.options == *options
            && self.documents.len() == documents.len()
            && self.documents.iter().zip(documents).all(|(a, b)| {
                a.id == b.id
                    && a.generation == b.generation
                    && Arc::ptr_eq(&a.bytes, &b.bytes)
                    && a.name == b.name
                    && a.layers == b.layers
                    && a.hide_comments == b.hide_comments
            })
    }
    #[cfg(target_arch = "wasm32")]
    fn retire(&mut self) {
        if let Some(pool) = self.pool.take() {
            self.retiring = Some(pool.retire());
        }
    }
    pub fn total_pages(&self) -> usize {
        self.documents.iter().fold(0usize, |n, d| n.saturating_add(d.pages))
    }
    #[cfg(target_arch = "wasm32")]
    pub fn next_page(&self) -> Option<(DocId, usize)> {
        self.documents.get(self.document).map(|d| (d.id, self.page))
    }
    /// Process at most one page. A cached page may be supplied before a request is started.
    #[cfg(target_arch = "wasm32")]
    pub fn poll(&mut self, cached: Option<Arc<PageText>>) {
        if let Some(retiring) = &mut self.retiring {
            if retiring.running() != 0 {
                return;
            }
            self.retiring = None;
        }
        if self.finished {
            return;
        }
        while let Some(doc) = self.documents.get(self.document) {
            if self.page < doc.pages {
                break;
            }
            self.retire();
            self.document = self.document.saturating_add(1);
            self.page = 0;
            return;
        }
        let Some(doc) = self.documents.get(self.document) else {
            self.finished = true;
            self.retire();
            return;
        };
        let text = if !self.pending && cached.is_some() {
            cached
        } else {
            if self.pool.is_none() {
                let pool = RenderPool::new(
                    doc.bytes.clone(),
                    1,
                    RenderConfig {
                        password: doc.password.clone(),
                        layers: doc.layers.clone(),
                        hide_comments: doc.hide_comments,
                        ..Default::default()
                    },
                )
                .without_replacements();
                self.pool = Some(pool);
            }
            let Some(pool) = self.pool.as_ref() else { return };
            if !self.pending {
                pool.set_queue(vec![RenderRequest { page: self.page, kind: RequestKind::Text, tile: None, scale: 1.0, tag: 0, preview: false }]);
                self.pending = true;
            }
            let Some(result) = pool.try_recv() else { return };
            let text = extracted_text(result);
            if let Err(error) = &text {
                self.warning_count = self.warning_count.saturating_add(1);
                if self.warnings.len() < MAX_WARNINGS {
                    self.warnings.push(warning(doc.id, self.page, error.clone()));
                }
            }
            text.ok()
        };
        if let Some(text) = text {
            let remaining = MAX_RESULTS.saturating_sub(self.results.len());
            let found = hits(&text, doc, self.page, &self.options, remaining.saturating_add(1));
            self.truncated = found.len() > remaining;
            self.results.extend(found.into_iter().take(remaining));
        }
        self.searched = self.searched.saturating_add(1);
        self.page = self.page.saturating_add(1);
        self.pending = false;
        if self.truncated {
            self.finished = true;
            self.retire();
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
enum Event {
    Page { hits: Vec<Hit>, warning: Option<Warning>, truncated: bool },
    Done,
}
#[cfg(not(target_arch = "wasm32"))]
impl Scan {
    fn cancel_worker(&mut self) {
        if let Some(cancel) = self.cancel.take() {
            cancel.store(true, std::sync::atomic::Ordering::Release);
        }
        // Dropping the receiver releases a worker waiting to deliver an obsolete page.
        self.receiver = None;
    }
    /// Extract and match off-thread. Only one coordinator and one renderer worker can exist,
    /// including a cancelled job. The bounded channel holds at most one page's capped results.
    pub fn poll(&mut self, _cached: Option<Arc<PageText>>) {
        if self.finished {
            return;
        }
        if self.receiver.is_none() {
            if self.worker.as_ref().is_some_and(|h| !h.is_finished()) {
                return;
            }
            self.worker = None;
            let documents = self.documents.clone();
            let options = self.options.clone();
            let (tx, rx) = std::sync::mpsc::sync_channel(1);
            let cancel = Arc::new(std::sync::atomic::AtomicBool::new(false));
            let stopped = cancel.clone();
            match std::thread::Builder::new().name("pdfcraft-session-search".into()).spawn(move || run(documents, options, &stopped, &tx)) {
                Ok(worker) => {
                    self.worker = Some(worker);
                    self.receiver = Some(rx);
                    self.cancel = Some(cancel);
                }
                Err(error) => {
                    self.finished = true;
                    self.warning_count = self.total_pages().max(1);
                    if let Some(doc) = self.documents.first() {
                        self.warnings.push(warning(doc.id, 0, format!("could not start search worker: {error}")));
                    }
                    return;
                }
            }
        }
        for _ in 0..8 {
            let event = self.receiver.as_ref().map(|r| r.try_recv());
            match event {
                Some(Ok(Event::Page { hits, warning, truncated })) => {
                    self.searched = self.searched.saturating_add(1);
                    self.results.extend(hits.into_iter().take(MAX_RESULTS.saturating_sub(self.results.len())));
                    if let Some(warning) = warning {
                        self.warning_count = self.warning_count.saturating_add(1);
                        if self.warnings.len() < MAX_WARNINGS {
                            self.warnings.push(warning);
                        }
                    }
                    self.truncated |= truncated;
                }
                Some(Ok(Event::Done)) => {
                    self.finished = true;
                    break;
                }
                Some(Err(std::sync::mpsc::TryRecvError::Disconnected)) => {
                    self.finished = true;
                    self.warning_count = self.warning_count.saturating_add(self.total_pages().saturating_sub(self.searched).max(1));
                    if let Some(doc) = self.documents.first() {
                        self.warnings.push(warning(doc.id, 0, "search worker stopped before completion".into()));
                    }
                    break;
                }
                _ => break,
            }
        }
    }
}
#[cfg(not(target_arch = "wasm32"))]
impl Drop for Scan {
    fn drop(&mut self) {
        self.cancel_worker();
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn run(documents: Vec<Snapshot>, options: Options, cancel: &std::sync::atomic::AtomicBool, tx: &std::sync::mpsc::SyncSender<Event>) {
    use std::sync::atomic::Ordering;
    let mut retained = 0usize;
    for doc in documents {
        if cancel.load(Ordering::Acquire) {
            return;
        }
        let pool = RenderPool::new(
            doc.bytes.clone(),
            1,
            RenderConfig { password: doc.password.clone(), layers: doc.layers.clone(), hide_comments: doc.hide_comments, ..Default::default() },
        )
        .without_replacements();
        let mut stop = false;
        for page in 0..doc.pages {
            if cancel.load(Ordering::Acquire) {
                stop = true;
                break;
            }
            pool.set_queue(vec![RenderRequest { page, kind: RequestKind::Text, tile: None, scale: 1.0, tag: 0, preview: false }]);
            let result = loop {
                if cancel.load(Ordering::Acquire) {
                    break None;
                }
                if let Some(result) = pool.try_recv() {
                    break Some(result);
                }
                std::thread::sleep(std::time::Duration::from_millis(1));
            };
            let Some(result) = result else {
                stop = true;
                break;
            };
            let remaining = MAX_RESULTS.saturating_sub(retained);
            // Matching is untrusted-document work too; a failure preserves earlier results.
            let matched = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                extracted_text(result).map(|text| hits(&text, &doc, page, &options, remaining.saturating_add(1)))
            }))
            .unwrap_or_else(|_| Err("text matching failed".into()));
            let (found, warning) = match matched {
                Ok(hits) => (hits, None),
                Err(error) => (Vec::new(), Some(warning(doc.id, page, error))),
            };
            let truncated = found.len() > remaining;
            let hits: Vec<_> = found.into_iter().take(remaining).collect();
            retained = retained.saturating_add(hits.len());
            if cancel.load(Ordering::Acquire) || tx.send(Event::Page { hits, warning, truncated }).is_err() {
                stop = true;
                break;
            }
            if truncated {
                stop = true;
                break;
            }
        }
        // Keep the retiring parser counted against the concurrency cap even after its watchdog
        // reports a timeout. A pathological operator may delay cancellation, never multiply it.
        let mut retiring = pool.retire();
        while retiring.running() != 0 {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        if cancel.load(Ordering::Acquire) {
            return;
        }
        if stop {
            break;
        }
    }
    let _ = tx.send(Event::Done);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AddedText, Edit, Session};
    fn add(session: &mut Session, id: DocId, page: usize, value: &str) {
        session
            .apply(id, Edit::AddText { page, text: AddedText { rect: [20., 40., 500., 100.], text: value.into(), ..Default::default() } })
            .unwrap();
    }
    fn finish(scan: &mut Scan) {
        for _ in 0..100_000 {
            scan.poll(None);
            if scan.finished {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        panic!("search did not complete");
    }
    #[test]
    fn scans_all_working_pages_in_order_without_rendering_or_changing_documents() {
        let mut session = Session::new();
        let mut ids = Vec::new();
        for (name, n) in [("same.pdf", 5), ("same.pdf", 3), ("empty.pdf", 4)] {
            let bytes = session.create_blank(600., 800., n).unwrap();
            ids.push(session.open(name, None, bytes, None).unwrap());
        }
        add(&mut session, ids[0], 1, "the invoice invoice");
        add(&mut session, ids[0], 4, "review the invoice details");
        add(&mut session, ids[1], 2, "già ricevuta: invoice caffè");
        let docs: Vec<_> = session.docs().iter().map(Snapshot::new).collect();
        let mut scan = Scan::default();
        scan.reset(docs.clone(), Options { query: "invoice".into(), ..Default::default() });
        finish(&mut scan);
        assert_eq!(scan.searched, 12);
        assert_eq!(scan.warning_count, 0);
        assert!(!scan.truncated);
        assert_eq!(scan.results.iter().map(|h| (h.doc, h.page)).collect::<Vec<_>>(), vec![(ids[0], 1), (ids[0], 1), (ids[0], 4), (ids[1], 2)]);
        assert!(scan.results[2].before.contains("review"));
        assert!(!scan.results[0].rects.is_empty());
        for snapshot in &docs {
            assert!(snapshot.current(session.get(snapshot.id).unwrap()));
        }
        scan.reset(docs, Options { query: "CAFFÈ".into(), ..Default::default() });
        finish(&mut scan);
        assert_eq!(scan.results.len(), 1);
        assert_eq!(scan.results[0].text, "caffè");
    }
    #[test]
    fn new_queries_and_working_revisions_discard_old_results_and_pending_work() {
        let mut session = Session::new();
        let bytes = session.create_from_text("synthetic", "invoice contract").unwrap();
        let id = session.open("same.pdf", None, bytes, None).unwrap();
        let documents = || session.docs().iter().map(Snapshot::new).collect::<Vec<_>>();
        let mut scan = Scan::default();
        scan.reset(documents(), Options { query: "invoice".into(), ..Default::default() });
        scan.poll(None); // A real request may be in flight when the query changes.
        scan.cancel();
        assert!(scan.cancelled && scan.finished);
        scan.reset(documents(), Options { query: "contract".into(), ..Default::default() });
        finish(&mut scan);
        assert_eq!(scan.results.len(), 1);
        assert_eq!(scan.results[0].text, "contract");
        let old = scan.documents[0].clone();
        add(&mut session, id, 0, "edited");
        assert!(!old.current(session.get(id).unwrap()));
        let new = session.docs().iter().map(Snapshot::new).collect::<Vec<_>>();
        assert!(!scan.unchanged(&new, &scan.options));
        scan.reset(new, Options { query: "edited".into(), ..Default::default() });
        finish(&mut scan);
        assert_eq!(scan.results.len(), 1);
        let edited = scan.documents[0].clone();
        session.undo(id).unwrap();
        assert!(!edited.current(session.get(id).unwrap()));
        scan.reset(session.docs().iter().map(Snapshot::new).collect(), Options { query: "edited".into(), ..Default::default() });
        finish(&mut scan);
        assert!(scan.results.is_empty());
        let undone = scan.documents[0].clone();
        session.redo(id).unwrap();
        assert!(!undone.current(session.get(id).unwrap()));
        scan.reset(session.docs().iter().map(Snapshot::new).collect(), Options { query: "edited".into(), ..Default::default() });
        finish(&mut scan);
        assert_eq!(scan.results.len(), 1);
        let redone = scan.documents[0].clone();
        let replacement = session.create_from_text("synthetic", "replacement").unwrap();
        session.reload(id, replacement).unwrap();
        assert!(!redone.current(session.get(id).unwrap()));
        scan.reset(session.docs().iter().map(Snapshot::new).collect(), Options { query: "replacement".into(), ..Default::default() });
        finish(&mut scan);
        assert_eq!(scan.results.len(), 1);
        session.close(id);
        scan.reset(Vec::new(), Options { query: "edited".into(), ..Default::default() });
        finish(&mut scan);
        assert!(scan.results.is_empty());
    }
    #[test]
    fn dense_results_are_capped_and_textless_pages_are_not_failures() {
        let mut session = Session::new();
        for _ in 0..3 {
            let bytes = session.create_from_text("dense", &"invoice ".repeat(750)).unwrap();
            session.open("dense.pdf", None, bytes, None).unwrap();
        }
        let mut scan = Scan::default();
        scan.reset(session.docs().iter().map(Snapshot::new).collect(), Options { query: "invoice".into(), ..Default::default() });
        finish(&mut scan);
        assert!(scan.truncated);
        assert_eq!(scan.results.len(), MAX_RESULTS);
        assert!(scan.results.iter().all(|h| h.before.len() <= 512 && h.text.len() <= 512 && h.after.len() <= 512 && h.rects.len() <= 256));
        scan.reset(session.docs().iter().map(Snapshot::new).collect(), Options::default());
        assert!(scan.finished);
        assert!(scan.results.is_empty());
    }
    #[test]
    fn unreadable_pages_preserve_readable_documents() {
        let mut session = Session::new();
        let bytes = session.create_from_text("readable", "invoice").unwrap();
        let id = session.open("valid.pdf", None, bytes, None).unwrap();
        let valid = Snapshot::new(session.get(id).unwrap());
        let mut bad = valid.clone();
        bad.id = DocId(id.0 + 1);
        bad.bytes = Arc::new(b"not a PDF".to_vec());
        bad.pages = 2;
        let mut scan = Scan::default();
        scan.reset(vec![bad, valid], Options { query: "invoice".into(), ..Default::default() });
        finish(&mut scan);
        assert_eq!(scan.results.len(), 1);
        assert_eq!(scan.results[0].doc, id);
        assert_eq!(scan.warning_count, 2);
        assert_eq!(scan.searched, 3);
        assert_eq!(scan.warnings[0].page, 0);
    }
    #[test]
    fn image_only_documents_stay_empty_and_existing_ocr_layers_are_searched() {
        let mut session = Session::new();
        let png = crate::export::encode_png(8, 8, &[255u8; 8 * 8 * 4]).unwrap();
        let bytes = session.create_from_images(&[("synthetic.png".into(), png)]).unwrap();
        let image = session.open("image.pdf", None, bytes.clone(), None).unwrap();
        let ocr = session.open("ocr.pdf", None, bytes, None).unwrap();
        session
            .apply(
                ocr,
                Edit::AddOcrText {
                    page: 0,
                    words: vec![crate::ocr::PlacedWord { text: "invoice".into(), origin: [0., 4.], across: [7., 0.], up: [0., 4.] }],
                },
            )
            .unwrap();
        let before = session.get(ocr).unwrap().bytes.clone();
        let mut scan = Scan::default();
        scan.reset(session.docs().iter().map(Snapshot::new).collect(), Options { query: "invoice".into(), ..Default::default() });
        finish(&mut scan);
        assert_eq!(scan.warning_count, 0);
        assert_eq!(scan.searched, 2);
        assert_eq!(scan.results.len(), 1);
        assert_eq!(scan.results[0].doc, ocr);
        assert!(scan.results.iter().all(|h| h.doc != image));
        assert!(Arc::ptr_eq(&before, &session.get(ocr).unwrap().bytes));
        assert!(session.get(ocr).unwrap().dirty);
    }
    #[test]
    #[ignore = "synthetic workload observation; run explicitly with --ignored --nocapture"]
    fn synthetic_multi_document_search_workloads() {
        for count in [2, 10, 20] {
            let mut session = Session::new();
            for _ in 0..count {
                let bytes = session.create_blank(600., 800., 200).unwrap();
                let id = session.open("synthetic.pdf", None, bytes, None).unwrap();
                add(&mut session, id, 199, "invoice");
            }
            let mut scan = Scan::default();
            scan.reset(session.docs().iter().map(Snapshot::new).collect(), Options { query: "invoice".into(), ..Default::default() });
            let started = std::time::Instant::now();
            finish(&mut scan);
            assert_eq!(scan.results.len(), count);
            assert_eq!(scan.searched, count * 200);
            let retained: usize = scan
                .results
                .iter()
                .map(|h| h.before.capacity() + h.text.capacity() + h.after.capacity() + h.rects.capacity() * std::mem::size_of::<[f32; 4]>())
                .sum();
            eprintln!(
                "{count} documents / {} pages: {:?}, {retained} retained snippet/rectangle capacity bytes (not RSS)",
                count * 200,
                started.elapsed()
            );
        }
    }
}
