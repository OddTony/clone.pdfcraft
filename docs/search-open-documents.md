# Search open documents

Edit → Advanced Search offers **Current document** (the default) and **All open documents**.
Current document keeps the existing find bar options and results. All open documents searches
active and inactive tabs, existing OCR text and unsaved edits, in tab/page/text order. It does
not OCR images or open any other files. Matching uses the same Unicode, whole-word, case and
whitespace rules as Find.

The global query is independent of per-document Find until a result is selected. Selection
applies the global query/options to Find in the destination tab, activates that exact document,
and uses normal find navigation/highlighting. Switching scope alone preserves each tab's Find.
Document identities are stable session IDs, even when names match.

Open/close, tab reorder and document revision changes automatically restart the search. Old
results and pending navigation are discarded. The coordinator checks both engine edit generation
and working-byte identity, so reload/save replacements invalidate old references too. Query and
option changes invalidate results before they are displayed or used.

Native extraction and matching run off the frame thread, with one coordinator, one existing
RenderPool worker and one page request at a time. Cancelled workers must exit before another
search is started. A bounded channel holds at most one page of capped results.
The renderer's cancellation/watchdog behavior is
preserved; parser initialization or an expensive individual operator may delay cancellation
and the next search while retired workers exit. On WebAssembly, RenderPool
uses its existing inline, one-page-per-poll fallback; web extraction can pause a frame, as existing
per-document extraction does. A web worker implementation is not introduced here.

The search retains at most 2,000 results and stops after finding one additional match. The UI
clearly marks that cap and never presents the count as a complete total. Context/matched-text
segments are capped at 512 UTF-8 bytes each, geometry at 256 rectangles/4,096 matched glyphs;
only snippets and geometry survive the page. Current layer/comment display choices are captured
with each revision; headless searches bypass the shared default-text cache for display overrides,
so that incompatible page text cannot pollute existing text_find requests.
Fixed-height virtual result rows limit widgets to
what is visible. Search shows progress, completion and failed-page counts separately; unreadable
pages do not discard other results. These are retention/concurrency limits, not a whole-process
RSS or per-page extraction allocation guarantee. Renderer extraction limits remain renderer-owned.
Pages whose extraction reaches the renderer's content safety limit are reported as incomplete;
their partial text is excluded from results and from the headless success cache.

The headless equivalent is `text_find_all`; see `crates/automation/README.md` for limits,
continuation and structured warning semantics. Folder/index/Boolean searches remain separate,
unimplemented features.
