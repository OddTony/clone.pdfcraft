# DOCX to PDF import

> **Status:** M10 candidate, partial native DOCX import for #2871. **Revision history:** 2026-10-16 first version.

This change adds automatic DOCX conversion at File → Open (including dropped files),
Organize Pages → Insert from File, and Combine Files (including folders and drops).
The native Rust importer runs locally, without Microsoft Office, LibreOffice, macros or network access.
Direct Open runs conversion in the desktop open worker. Browser and in-memory imports use the
same converter. Insert and Combine retain the existing synchronous conversion behavior.

A directly opened DOCX becomes a **new unsaved PDF** named after the source. It has no PDF
save path until Save As chooses one. The DOCX is read only. Insertion keeps its existing
selection, ordering and undo/redo behavior; Combine retains its page ranges, thumbnails,
per-file bookmarks and password handling for PDFs. A malformed DOCX produces an error;
in Combine it remains an unreadable row requiring the existing omission confirmation.

## Conversion limits

The importer preserves paragraph/run styles, lists, tables, embedded raster pictures, sections,
page breaks, and headers/footers with page-number fields. Footnotes/endnotes move to the end.
It substitutes Helvetica, Times or Courier for document fonts; pagination can differ from Word.
Turkish letters missing from WinAnsi (ĞğİıŞş) use embedded OFL Inter Regular/SemiBold,
with synthetic slant for italic runs and matching advances for wrapping. Canonically equivalent
accents are normalized to NFC. Other unsupported characters become explicit `[U+XXXX]` code points.
Charts, SmartArt, embedded objects and unsupported/linked pictures use placeholder boxes.
Text boxes flow after their paragraphs; multi-column sections become one column; even-page
header/footer variants and full Word layout fidelity remain unsupported. Review converted pages
before sharing. This is partial DOCX import, not a promise of Word-identical rendering.

Open and Insert show conversion caveats and content notes. Combine retains them in each row's
warnings. Standalone PDFs also carry the notice in their Subject metadata. Automation returns
conversion reports for `doc_open`, `doc_create`, `page_insert_file` and `doc_combine`.
The older batch/Create Multiple APIs accept DOCX through the shared converter; their individual
PDF outputs retain Subject metadata, but their combined output does not aggregate reports.
Legacy `.doc`, `.docm`, `.dotx`, other Office formats, OpenDocument and encrypted DOCX are
outside this change. The reader validates the Word main part and never executes macros.

## Automation

All input paths pass through the existing `--root` confinement before reading:

```json
[
  {"tool":"doc_open","args":{"path":"report.docx"}},
  {"tool":"doc_save","args":{"doc":1,"path":"report.pdf"}},
  {"tool":"page_insert_file","args":{"doc":1,"path":"annex.docx","pages":[1],"at":2}},
  {"tool":"doc_combine","args":{"paths":["report.pdf","annex.docx"],"out":"combined.pdf"}}
]
```

`doc_create` also accepts `from: "docx"`, `path` and an optional PDF `name`.
Page numbers remain 1-based in automation. Save without a destination refuses a new DOCX import.

## Bounds and provenance

Packages are never extracted to disk. Only stored/deflated ZIP entries are read, with caps of
20,000 entries, 32 MiB per part and 128 MiB total decompressed reads. Part names are bounded
to 4 KiB each and 8 MiB total; malformed UTF-8/UTF-16 XML is refused without text substitution. ZIP64 and encrypted packages
are refused. XML forbids DTDs and caps nodes; style chains and content nesting are bounded;
output is capped at 5,000 pages. Engine conversion has a panic guard and validates written PDF
pages before returning. External relationships never read files or fetch URLs.

The DOCX layout, canvas, XML and ZIP code and its original DOCX regression cases are adapted
from ArturFaleiros's [PdfCraft PR #2326](https://github.com/storytold/pdfcraft/pull/2326),
revision `13caa2402467c8bb8e175412b976849b7cfad8dd`, under this repository's MIT OR Apache-2.0
license. That work follows ECMA-376. This proposal excludes its PowerPoint, Excel and redaction
changes and adds the three-workflow integration, conversion reports, stricter package bounds,
namespace validation and workflow regressions. #2326 and #2634 remain overlapping unmerged
proposals; maintainers can select or consolidate their preferred approach.

Tests synthesize documents in memory or export synthetic PdfCraft pages. No binary document,
font or image asset is committed. See `crates/create/src/docx/tests.rs`, the engine DOCX test,
`crates/automation/tests/automation.rs::docx_open_insert_combine_and_root_confinement`, and
`crates/ui-egui/tests/create.rs::docx_opens_inserts_and_combines_as_pdf_pages`.
