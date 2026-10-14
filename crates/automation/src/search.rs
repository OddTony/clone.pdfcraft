//! Session-wide search; shares extraction/cache and renderer semantics with text_find.
use super::*;
use pdfcraft_engine::search::{self, Options, Snapshot};
use std::sync::Weak;

struct Revision {
    id: DocId,
    generation: u64,
    bytes: Weak<Vec<u8>>,
    layers: Arc<Vec<(i32, i32, bool)>>,
    hide_comments: bool,
}
pub(super) struct Continuation {
    revisions: Vec<Revision>,
    options: Options,
    cursor: Value,
    failed_pages: usize,
}
impl Automation {
    pub(super) fn text_find_all(&mut self, a: &Args) -> Result<Value> {
        // Keep one lazy parser for the current document, without retaining parsers for every
        // open file or constructing one for a wholly cached request.
        let mut renderer: Option<(DocId, Weak<Vec<u8>>, PageRenderer)> = None;
        self.text_find_all_with(a, &mut |doc, pages| {
            let current = renderer.as_ref().is_some_and(|(id, old, _)| *id == doc.id && old.upgrade().is_some_and(|b| Arc::ptr_eq(&b, &doc.bytes)));
            if !current {
                renderer = Some((
                    doc.id,
                    Arc::downgrade(&doc.bytes),
                    PageRenderer::new(
                        doc.bytes.clone(),
                        RenderConfig {
                            password: doc.password.clone(),
                            layers: doc.layers.clone(),
                            hide_comments: doc.hide_comments,
                            ..Default::default()
                        },
                    ),
                ));
            }
            let Some((_, _, parser)) = renderer.as_mut() else {
                return Vec::new();
            };
            pages.iter().map(|p| (*p, extract_text_page(parser, *p))).collect()
        })
    }

    fn text_find_all_with(&mut self, a: &Args, extract: &mut impl FnMut(&Snapshot, &[usize]) -> PageTexts) -> Result<Value> {
        let query = a.str("query")?;
        if query.len() > search::MAX_QUERY_BYTES {
            return Err(ToolError::InvalidArgs("query exceeds 4096 UTF-8 bytes".into()));
        }
        let options = Options {
            query: query.into(),
            case_sensitive: a.opt_bool("case_sensitive")?.unwrap_or(false),
            whole_words: a.opt_bool("whole_words")?.unwrap_or(false),
        };
        let limit = usize::try_from(a.opt_int("limit")?.unwrap_or(500))
            .ok()
            .filter(|n| (1..=search::MAX_RESULTS).contains(n))
            .ok_or_else(|| ToolError::InvalidArgs("limit must be between 1 and 2000".into()))?;
        let documents: Vec<_> = self.session.docs().iter().map(Snapshot::new).collect();
        let mut previous_failures = 0usize;
        let (mut document, mut page, mut skip) = (0usize, 0usize, 0usize);
        if let Some(cursor) = a.0.get("continuation") {
            let state = self.all_find.as_ref().ok_or_else(|| ToolError::InvalidArgs("continuation expired; start a new search".into()))?;
            let current = state.revisions.len() == documents.len()
                && state.revisions.iter().zip(&documents).all(|(revision, d)| {
                    revision.id == d.id
                        && revision.generation == d.generation
                        && revision.layers == d.layers
                        && revision.hide_comments == d.hide_comments
                        && revision.bytes.upgrade().is_some_and(|b| Arc::ptr_eq(&b, &d.bytes))
                });
            if !current || state.options != options || state.cursor != *cursor {
                return Err(ToolError::InvalidArgs("session, query or continuation changed; start a new search".into()));
            }
            let position = |key: &str| {
                cursor
                    .get(key)
                    .and_then(Value::as_u64)
                    .and_then(|v| usize::try_from(v).ok())
                    .ok_or_else(|| ToolError::InvalidArgs("invalid continuation".into()))
            };
            previous_failures = state.failed_pages;
            document = position("document")?;
            page = position("page")?;
            skip = position("skip")?;
        } else {
            self.all_find_serial = self.all_find_serial.checked_add(1).ok_or_else(|| failed("search generation exhausted"))?;
            self.all_find = None;
        }
        let mut matches = Vec::new();
        let mut warnings = Vec::new();
        let mut warning_count = 0usize;
        let mut next = None;
        let mut pages_searched = 0usize;
        if !options.query.trim().is_empty() {
            'documents: while let Some(doc) = documents.get(document) {
                while page < doc.pages {
                    pages_searched = pages_searched.saturating_add(1);
                    // One in-flight page: retained data uses the existing shared bounded cache.
                    // Default extraction reuses text_find's cache. Display overrides bypass it
                    // rather than inserting incompatible page text into the shared cache.
                    let texts = if doc.layers.is_empty() && !doc.hide_comments {
                        self.page_text_results(doc.id, &[page], &mut |_, _, pages| extract(doc, pages))?
                    } else {
                        let result = extract(doc, &[page])
                            .into_iter()
                            .find(|(p, _)| *p == page)
                            .map(|(_, r)| r)
                            .unwrap_or_else(|| Err("text extraction returned no result".into()));
                        vec![(page, result)]
                    };
                    for (_, result) in texts {
                        match result {
                            Err(error) => {
                                warning_count = warning_count.saturating_add(1);
                                if warnings.len() < search::MAX_WARNINGS {
                                    let w = search::warning(doc.id, page, error);
                                    warnings.push(json!({"doc": w.doc.0, "page": w.page.saturating_add(1), "error": w.error, "error_truncated": w.error_truncated}));
                                }
                            }
                            Ok(text) => {
                                let remaining = limit.saturating_sub(matches.len());
                                let found = search::hits_skipping(&text, doc, page, &options, remaining.saturating_add(1), skip);
                                let more = found.len() > remaining;
                                let count = found.len().min(remaining);
                                for hit in found.into_iter().take(remaining) {
                                    matches.push(json!({"doc": doc.id.0, "name": doc.name, "generation": doc.generation, "page": page.saturating_add(1), "text": hit.text, "text_truncated": hit.text_truncated, "glyph_range": [hit.range.start, hit.range.end], "match_index": hit.page_match, "rects": hit.rects, "geometry_truncated": hit.geometry_truncated, "context": {"before": hit.before, "after": hit.after}}));
                                }
                                if more {
                                    next = Some(
                                        json!({"token": self.all_find_serial, "document": document, "page": page, "skip": skip.saturating_add(count)}),
                                    );
                                    break 'documents;
                                }
                            }
                        }
                    }
                    page = page.saturating_add(1);
                    skip = 0;
                }
                document = document.saturating_add(1);
                page = 0;
            }
        }
        let total_warning_count = previous_failures.saturating_add(warning_count);
        let mut out = json!({"query": options.query, "count": matches.len(), "matches": matches, "complete": next.is_none() && total_warning_count == 0, "truncated": next.is_some(), "pages_searched": pages_searched, "warning_count": warning_count, "total_warning_count": total_warning_count, "warnings_truncated": warning_count > warnings.len(), "warnings": warnings});
        if let Some(cursor) = next {
            self.all_find = Some(Continuation {
                revisions: documents
                    .iter()
                    .map(|d| Revision {
                        id: d.id,
                        generation: d.generation,
                        bytes: Arc::downgrade(&d.bytes),
                        layers: d.layers.clone(),
                        hide_comments: d.hide_comments,
                    })
                    .collect(),
                options: options.clone(),
                cursor: cursor.clone(),
                failed_pages: total_warning_count,
            });
            if let Some(object) = out.as_object_mut() {
                object.insert("next".into(), json!({"query": options.query, "case_sensitive": options.case_sensitive, "whole_words": options.whole_words, "limit": limit, "continuation": cursor}));
            }
        } else {
            self.all_find = None;
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn truncated_extraction_is_incomplete_and_never_cached_as_success() {
        let mut a = Automation::new();
        let bytes = a.session.create_blank(200.0, 300.0, 1).unwrap();
        a.session.open("truncated.pdf", None, bytes, None).unwrap();
        let bytes = a.session.create_from_text("readable", "invoice").unwrap();
        let good = a.session.open("readable.pdf", None, bytes, None).unwrap();
        let mut failures = 0;
        let mut extract = |doc: &Snapshot, pages: &[usize]| {
            pages
                .iter()
                .map(|p| {
                    let mut r = PageRenderer::new(doc.bytes.clone(), Default::default());
                    let mut result = r.render(RenderRequest { page: *p, kind: RequestKind::Text, ..Default::default() });
                    if doc.id != good {
                        failures += 1;
                        result.warnings.push(pdfcraft_render::RenderWarning::ContentTruncated);
                    }
                    (*p, search::extracted_text(result))
                })
                .collect()
        };
        for _ in 0..2 {
            let out = a.text_find_all_with(&Args(&json!({"query": "invoice"})), &mut extract).unwrap();
            assert_eq!(out["count"], 1);
            assert_eq!(out["matches"][0]["doc"], good.0);
            assert_eq!(out["complete"], false);
            assert_eq!(out["warning_count"], 1);
        }
        assert_eq!(failures, 2, "incomplete page text must not enter the shared success cache");
    }
    #[test]
    fn failed_pages_preserve_other_matches_and_incomplete_status_through_continuation() {
        let mut a = Automation::new();
        let bytes = a.session.create_blank(200.0, 300.0, 3).unwrap();
        let id = a.session.open("synthetic.pdf", None, bytes, None).unwrap();
        let text = || {
            Arc::new(PageText {
                glyphs: vec![pdfcraft_render::text::TextGlyph {
                    text: "invoice invoice".into(),
                    rect: [0., 0., 100., 10.],
                    quad: [[0., 0.]; 4],
                    direction: [1., 0.],
                }],
                line_of: vec![0],
                space_before: vec![false],
            })
        };
        let mut extract =
            |_: &Snapshot, pages: &[usize]| pages.iter().map(|p| (*p, if *p == 0 { Err("unreadable page".into()) } else { Ok(text()) })).collect();
        let first = a.text_find_all_with(&Args(&json!({"query":"invoice", "limit":1})), &mut extract).unwrap();
        assert_eq!(first["count"], 1);
        assert_eq!(first["complete"], false);
        assert_eq!(first["warnings"][0]["doc"], id.0);
        assert_eq!(first["warnings"][0]["page"], 1);
        let mut part = first;
        let mut count = part["count"].as_u64().unwrap();
        while let Some(next) = part.get("next").cloned() {
            part = a.text_find_all_with(&Args(&next), &mut extract).unwrap();
            count += part["count"].as_u64().unwrap();
        }
        assert_eq!(count, 4);
        assert_eq!(part["complete"], false);
        assert_eq!(part["total_warning_count"], 1);
    }
    #[test]
    fn continuation_is_bounded_even_when_one_glyph_contains_thousands_of_matches() {
        let mut a = Automation::new();
        let bytes = a.session.create_blank(200.0, 300.0, 1).unwrap();
        a.session.open("dense.pdf", None, bytes, None).unwrap();
        let text = Arc::new(PageText {
            glyphs: vec![pdfcraft_render::text::TextGlyph {
                text: "x ".repeat(2_105),
                rect: [0., 0., 1., 1.],
                quad: [[0., 0.]; 4],
                direction: [1., 0.],
            }],
            line_of: vec![0],
            space_before: vec![false],
        });
        let mut extract = |_: &Snapshot, pages: &[usize]| pages.iter().map(|p| (*p, Ok(text.clone()))).collect();
        let first = a.text_find_all_with(&Args(&json!({"query":"x", "limit":2000})), &mut extract).unwrap();
        assert_eq!(first["count"], 2000);
        assert_eq!(first["truncated"], true);
        let last = a.text_find_all_with(&Args(&first["next"]), &mut extract).unwrap();
        assert_eq!(last["count"], 105);
        assert_eq!(last["complete"], true);
    }
}
