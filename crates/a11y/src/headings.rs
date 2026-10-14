//! The document's headings (H, H1–H6 after the role map) with the text they mark: what New
//! Bookmarks from Structure turns into bookmarks.

use std::collections::{HashMap, HashSet};

use pdfcraft_cos::{Dict, Document, ObjRef, Object, PdfString};
use pdfcraft_fonts::pdf::Metrics;
use pdfcraft_model::Page;

use crate::alt::{page_content, standard, tree_root};

const MAX_HEADINGS: usize = 10_000;
const MAX_VISITS: usize = 1_000_000;
const MAX_DEPTH: usize = 256;
/// Characters kept of a heading's title.
const MAX_TITLE: usize = 256;

/// A heading element.
#[derive(Clone, Debug, PartialEq)]
pub struct Heading {
    /// The structure element (`None` for a direct element, which can't be referenced).
    pub obj: Option<ObjRef>,
    /// 1 for H1 … 6 for H6; a plain H takes the number of sections (Sect, Part, Art) around it.
    pub level: u8,
    pub title: String,
    /// 0-based page of its content.
    pub page: usize,
}

struct Walker<'a> {
    doc: &'a Document,
    role_map: Dict,
    pages: &'a [Page],
    page_index: HashMap<ObjRef, usize>,
    seen: HashSet<ObjRef>,
    visits: usize,
    /// Per page: the text each MCID shows (read once per page).
    texts: HashMap<usize, HashMap<i64, String>>,
    out: Vec<Heading>,
}

/// The headings, in document order. Headings without text or a page are left out.
pub fn headings(doc: &Document) -> Vec<Heading> {
    let Some(root) = tree_root(doc) else { return Vec::new() };
    let pages = pdfcraft_model::pages(doc);
    let mut w = Walker {
        doc,
        role_map: root.get(b"RoleMap").map(|o| doc.resolve(o)).and_then(|o| o.as_dict().cloned()).unwrap_or_default(),
        pages: &pages,
        page_index: pages.iter().enumerate().map(|(i, p)| (p.obj, i)).collect(),
        seen: HashSet::new(),
        visits: 0,
        texts: HashMap::new(),
        out: Vec::new(),
    };
    if let Some(k) = root.get(b"K") {
        w.walk(k, None, 0, 0);
    }
    w.out
}

impl Walker<'_> {
    /// Whether `k` may be visited (bounded depth and size, each object once).
    fn enter(&mut self, k: &Object, depth: usize) -> bool {
        self.visits += 1;
        depth <= MAX_DEPTH && self.visits <= MAX_VISITS && self.out.len() < MAX_HEADINGS && !matches!(k, Object::Ref(r) if !self.seen.insert(*r))
    }

    fn element_of(&self, k: &Object) -> Option<(Option<ObjRef>, Dict)> {
        let d = match k {
            Object::Ref(r) => self.doc.get(*r).as_dict().cloned()?,
            Object::Dict(d) => d.clone(),
            _ => return None,
        };
        d.contains(b"S").then(|| (k.as_ref(), d))
    }

    fn walk(&mut self, k: &Object, pg: Option<ObjRef>, sections: u8, depth: usize) {
        if !self.enter(k, depth) {
            return;
        }
        if let Object::Array(a) = k {
            a.iter().for_each(|x| self.walk(x, pg, sections, depth + 1));
            return;
        }
        let Some((obj, d)) = self.element_of(k) else { return };
        let pg = d.get(b"Pg").and_then(Object::as_ref).or(pg);
        let ty = d.name(b"S").map(|s| standard(self.doc, &self.role_map, s)).unwrap_or_default();
        let level = match ty.as_slice() {
            [b'H', n @ b'1'..=b'6'] => Some(n - b'0'),
            b"H" => Some(sections.max(1)),
            _ => None,
        };
        if let Some(level) = level {
            self.heading(obj, &d, pg, level, depth);
            return;
        }
        let sections = if matches!(ty.as_slice(), b"Sect" | b"Part" | b"Art") { sections.saturating_add(1) } else { sections };
        if let Some(kids) = d.get(b"K") {
            self.walk(kids, pg, sections, depth + 1);
        }
    }

    /// The marked content under `k`: (page, MCID) pairs in order.
    fn marks(&mut self, k: &Object, pg: Option<ObjRef>, depth: usize, out: &mut Vec<(Option<ObjRef>, i64)>) {
        if !self.enter(k, depth) {
            return;
        }
        match k {
            Object::Int(n) => out.push((pg, *n)),
            Object::Array(a) => a.iter().for_each(|x| self.marks(x, pg, depth + 1, out)),
            Object::Dict(m) if m.name(b"Type") == Some(b"MCR") => {
                if let Some(n) = m.get(b"MCID").and_then(Object::as_int) {
                    out.push((m.get(b"Pg").and_then(Object::as_ref).or(pg), n));
                }
            }
            _ => {
                if let Some((_, d)) = self.element_of(k)
                    && let Some(kids) = d.get(b"K")
                {
                    self.marks(kids, d.get(b"Pg").and_then(Object::as_ref).or(pg), depth + 1, out);
                }
            }
        }
    }

    fn heading(&mut self, obj: Option<ObjRef>, d: &Dict, pg: Option<ObjRef>, level: u8, depth: usize) {
        let mut marks = Vec::new();
        if let Some(kids) = d.get(b"K") {
            self.marks(kids, pg, depth + 1, &mut marks);
        }
        let Some(page) = pg.or_else(|| marks.iter().find_map(|m| m.0)).and_then(|p| self.page_index.get(&p).copied()) else { return };
        let doc = self.doc;
        let attr = |key: &[u8]| d.get(key).map(|o| doc.resolve(o)).and_then(|o| o.as_string().map(|s| s.to_text())).filter(|t| !t.trim().is_empty());
        let title = match attr(b"ActualText") {
            Some(t) => Some(t),
            None => self.marked_text(&marks, page).or_else(|| attr(b"Alt")).or_else(|| attr(b"T")),
        };
        let Some(title) = title else { return };
        let title: String = title.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(MAX_TITLE).collect();
        self.out.push(Heading { obj, level, title, page });
    }

    /// The text the marked content shows (`page`: where marks without a page of their own are).
    fn marked_text(&mut self, marks: &[(Option<ObjRef>, i64)], page: usize) -> Option<String> {
        let mut parts = Vec::new();
        // The title is cut to MAX_TITLE characters later: stop gathering well before a heading
        // that repeats one mark many times can pile up gigabytes.
        let mut gathered = 0usize;
        for (p, id) in marks {
            if gathered >= MAX_TITLE * 4 {
                break;
            }
            let page = p.and_then(|p| self.page_index.get(&p).copied()).unwrap_or(page);
            if let Some(text) = self.texts_of(page).get(id) {
                gathered = gathered.saturating_add(text.len()).saturating_add(1);
                parts.push(text.clone());
            }
        }
        Some(parts.join(" ")).filter(|t| !t.trim().is_empty())
    }

    fn texts_of(&mut self, page: usize) -> &HashMap<i64, String> {
        let (doc, pages) = (self.doc, self.pages);
        self.texts.entry(page).or_insert_with(|| pages.get(page).map(|p| mcid_texts(doc, &p.dict)).unwrap_or_default())
    }
}

struct MarkedContent {
    mcid: Option<i64>,
    replacement: Option<String>,
    emitted: bool,
}

fn replacement_text(doc: &Document, value: &Object) -> Option<String> {
    let indirect = value.as_ref().map(|r| doc.get(r));
    let string = indirect.as_deref().unwrap_or(value).as_string()?;
    let mut prefix = string.bytes.get(..string.bytes.len().min(MAX_TITLE * 16 + 4))?;
    if prefix.len() < string.bytes.len()
        && !string.bytes.starts_with(&[0xFE, 0xFF])
        && !string.bytes.starts_with(&[0xEF, 0xBB, 0xBF])
        && let Err(error) = std::str::from_utf8(prefix)
        && error.error_len().is_none()
    {
        // A split UTF-8 character must not switch the prefix to PDFDocEncoding.
        prefix = prefix.get(..error.valid_up_to())?;
    }
    Some(PdfString::literal(prefix).to_text().chars().take(MAX_TITLE * 4).collect())
}

/// The text each marked-content id shows on a page (text drawn directly in the page content).
fn mcid_texts(doc: &Document, page: &Dict) -> HashMap<i64, String> {
    let resources = page.get(b"Resources");
    let indirect_resources = resources.and_then(Object::as_ref).map(|r| doc.get(r));
    let res = indirect_resources.as_deref().or(resources).and_then(Object::as_dict);
    let fonts = res.and_then(|r| r.get(b"Font"));
    let indirect_fonts = fonts.and_then(Object::as_ref).map(|r| doc.get(r));
    let fonts = indirect_fonts.as_deref().or(fonts).and_then(Object::as_dict);
    let props = res.and_then(|r| r.get(b"Properties"));
    let indirect_props = props.and_then(Object::as_ref).map(|r| doc.get(r));
    let props = indirect_props.as_deref().or(props).and_then(Object::as_dict);
    let mut metrics: HashMap<Vec<u8>, Metrics> = HashMap::new();
    let mut font: Option<Vec<u8>> = None;
    let mut stack: Vec<MarkedContent> = Vec::new();
    let mut out: HashMap<i64, String> = HashMap::new();
    for op in pdfcraft_content::parse(&page_content(doc, page)).ops {
        match op.op.as_slice() {
            b"BMC" => stack.push(MarkedContent { mcid: None, replacement: None, emitted: false }),
            b"BDC" => {
                let property = match op.operands.get(1) {
                    Some(Object::Name(n)) => props.and_then(|p| p.get(n)),
                    other => other,
                };
                let indirect = property.and_then(Object::as_ref).map(|r| doc.get(r));
                let dict = indirect.as_deref().or(property).and_then(Object::as_dict);
                let mcid = dict.and_then(|d| d.get(b"MCID")).and_then(Object::as_int);
                let replacement = if stack.iter().any(|m| m.replacement.is_some()) {
                    None
                } else {
                    dict.and_then(|d| d.get(b"ActualText")).and_then(|v| replacement_text(doc, v))
                };
                stack.push(MarkedContent { mcid, replacement, emitted: false });
            }
            b"EMC" => {
                stack.pop();
            }
            b"Tf" => {
                font = op.name(0).map(<[u8]>::to_vec);
                if let Some(name) = op.name(0)
                    && !metrics.contains_key(name)
                    && let Some(f) = fonts.and_then(|f| f.get(name)).map(|f| doc.resolve(f)).and_then(|f| f.as_dict().cloned())
                {
                    metrics.insert(name.to_vec(), Metrics::from_dict(doc, &f));
                }
            }
            _ => {}
        }
        let id = stack.iter().rev().find_map(|m| m.mcid);
        if let Some(span) = stack.iter_mut().find(|m| m.replacement.is_some()) {
            if matches!(op.op.as_slice(), b"Tj" | b"TJ" | b"'" | b"\"") && !span.emitted {
                // ISO 32000-2 §14.9.4 replaces the whole occurrence, including nested content.
                span.emitted = true;
                if let Some(id) = id {
                    let text = out.entry(id).or_default();
                    for c in span.replacement.iter().flat_map(|s| s.chars()) {
                        if text.len().saturating_add(c.len_utf8()) > MAX_TITLE * 4 {
                            break;
                        }
                        text.push(c);
                    }
                }
            }
            continue;
        }
        let (Some(id), Some(m)) = (id, font.as_ref().and_then(|f| metrics.get(f))) else { continue };
        let text = out.entry(id).or_default();
        if text.len() >= MAX_TITLE * 4 {
            continue;
        }
        match op.op.as_slice() {
            b"Tj" | b"'" | b"\"" => {
                if op.op != b"Tj" {
                    text.push(' ');
                }
                if let Some(s) = op.operands.iter().rev().find_map(Object::as_string) {
                    text.push_str(&m.decode(&s.bytes));
                }
            }
            b"TJ" => {
                for item in op.operands.first().and_then(Object::as_array).into_iter().flatten() {
                    match item {
                        Object::String(s) => text.push_str(&m.decode(&s.bytes)),
                        n if n.as_f64().is_some_and(|n| n < -200.0) => text.push(' '),
                        _ => {}
                    }
                }
            }
            b"Td" | b"TD" | b"T*" | b"Tm" => text.push(' '),
            _ => {}
        }
    }
    out
}
