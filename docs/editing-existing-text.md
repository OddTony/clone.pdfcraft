# Editing existing text and source subsets

Click a text region in **Edit a PDF → Edit text and images**. One editor contains its
source rows and mixed styles. Embedded TrueType and OpenType/CFF faces are used in memory;
source font programs, colour, size and baseline anchors stay intact. A textless preview
prevents a second copy underneath. The frame shows the live extent as text grows or shrinks.

A subset font contains only some of the original face's glyphs. A new character cannot be
reconstructed from a glyph the PDF does not contain. **Load font…** imports an openly licensed,
embeddable standalone `.ttf` or `.otf` face. **Installed fonts…** discovers eligible user-installed
faces on request, off the UI thread; the document-font menu selects resources already on the
page. Select **Keep source fonts** to preserve each run's original face. A matching full face
completes its subsets automatically, including new Chinese characters. Choosing another face
is explicit. Fonts with restricted embedding, unsupported collections, excluded families or
no open-licence declaration are refused. Interface fallback fonts are never exported.

The full selected program, glyph mappings, widths and Unicode text are embedded in the PDF.
Saving and reopening therefore needs no locally installed font. Original subset bytes are
preserved. Optional matching faces from `CRAFT_FONTS_DIR` can also complete source subsets;
this works only when that build input actually contains the full face. This change does not
add a font binary or change the release font pin. Fonts shared by releases belong in
`storytold/craft-fonts` under the repository's asset rules.

Select text within the editor before changing size, colour, font, underline or spacing to
format that selection. Automation supports up to 1024 disjoint UTF-8 byte ranges with the
same properties; invalid or overlapping ranges fail atomically. Untouched source formatting
survives. OpenType shaping uses the selected/source face's substitution and positioning
rules, preserves marks and ligatures, and applies Unicode bidi ordering for a single face.
Logical text remains available after save/reopen through ToUnicode and ActualText.
Added Arabic text records ActualText per shaped line for copying and searching in typing order.
ActualText already supplies typing order, so the editor preserves it without applying
a second bidi reversal; adjacent visual glyph runs still recover their own reading order.
Only right-to-left ActualText is kept apart this way: left-to-right spans, such as the Latin
ligatures (`fi`, `fl`) Word and Chrome wrap in ActualText mid-word, join their line as before.

Old Hebrew PDFs often use a font laid out for a Hebrew code page, with no `/ToUnicode`: code
`0xE0` draws alef, `0xE1` bet, and so on, while the PDF still declares a Latin encoding. Reading
the declared encoding gave Latin letters or replacement boxes in the editor. **Preferences ▸
Legacy text encoding** decides how codes `0x80`–`0xFF` of such fonts are read:

- **Auto-detect** (the default) applies ISO 8859-8, or Windows-1255 when the font also draws
  Hebrew points, only where the embedded TrueType/OpenType program itself shows Hebrew at those
  codes: its symbol or Macintosh cmap leads to the glyphs its Unicode cmap names alef…tav, or its
  OS/2 table declares the Hebrew code page and not Latin 1. Ordinary Latin fonts are untouched.
- **As declared in the PDF** always reads the PDF's encoding.
- **Hebrew (ISO 8859-8)** and **Hebrew (Windows-1255)** apply that code page to every simple font
  without `/ToUnicode`, for files Auto-detect can't recognise (Type 1 or non-embedded fonts).

A `/ToUnicode` CMap always wins, Symbol and ZapfDingbats are never reinterpreted, and a code the
code page leaves undefined keeps its declared meaning. The editor's in-memory copy of a
symbol-cmap font gets a Unicode cmap for the decoded letters, so they draw in the document's own
face; typing Hebrew writes the same codes back. When no embedded face can draw a Hebrew letter,
the interface falls back to an installed Hebrew face (never exported). The choice is app-wide,
saved with the other preferences, and applies the next time a paragraph is opened; `text_encoding`
reads or sets it headlessly. Search and copy (`text_extract`, `text_find`) use the renderer's own
text extraction and do not follow it yet.

The editing box lays out each line that holds right-to-left script in Unicode bidirectional
display order (UAX #9). egui shapes a run of one face in a single direction, so when one embedded
face drew a whole Hebrew line, its numbers and Latin words came out reversed ("1255" as "5521");
the box now hands egui one piece per face and direction instead.

Pieces of one row that are drawn as separate text (a Hebrew table row's cells, words set in
two fonts) join in reading order: a right-to-left row starts from its rightmost piece, so a
row number or heading on the right of a Hebrew row comes first, as on the page.

The caret follows the reading order too. egui placed the caret as if every line ran left to right,
and gave right-to-left runs a wrong glyph count, so Backspace after a Hebrew word could delete the
space on its other side. PdfCraft's vendored `epaint` fixes the glyph count and places and hits the
caret by screen position in a right-to-left row; the box hands it each Hebrew row in reading order.
A Hebrew line that wraps inside the box keeps the old drawing-order caret until it is unwrapped.

**Text direction.** As in Word, the Format text panel and the floating toolbar have
Right-to-left (←¶) and Left-to-right (¶→) buttons. Until one is chosen, a paragraph's direction
comes from its letters. A chosen direction decides each line's bidirectional order (where numbers
and punctuation go) and, without an alignment, the side the paragraph aligns to, in the box and in
the rewritten PDF; it goes with every later edit of the paragraph. In a right-to-left paragraph the
Left and Right arrow keys move the caret the way they point. Headless: `text_edit.direction`
(`rtl`, `ltr`, `auto`).

**Floating toolbar.** While a paragraph is open, a small toolbar floats above it, like Word's mini
toolbar: smaller and larger text, bold, italic, underline, text direction, alignment and four
colours. Drag it anywhere by its handle. Each change is the same undoable edit as in the Format text
panel, and the caret returns to the text. Preferences ▸ Show the formatting toolbar when editing
text turns it off (it is on by default).

For headless editing, use `text_font_files` → `text_font_import` → `text_fonts` and pass a
resource to `text_edit.font_resource`. `text_edit.ranges` holds selected range formatting.
`text_edit_layout` accepts resource/ranges too and measures a draft without applying it.
All file inputs respect CLI root confinement; importing a font and applying text are undoable.

On a scanned page, recognised text (Scan & OCR) is an invisible layer over the picture. Edit
text groups it into phrases: words joined by a word space, with a table cell, a tab stop and each
scanned line kept apart. Applying an edit draws the new text visibly in the scan's ink colour (or
the one chosen) and first covers the old words with the paper colour sampled around them. Each
word keeps its position and width, and a longer phrase grows rightwards rather than wrapping. The
cover is one flat colour, and the text uses the layer's Helvetica, not the scanned typeface.
This applies to any invisible text (rendering mode 3), not just a PdfCraft OCR layer: editing
invisible text, such as another OCR tool's layer, makes the rewritten paragraph visible over a
painted cover in the sampled paper colour. Edit text also groups invisible text differently: its lines never
merge into multi-line paragraphs, and words split at gaps of 0.6 em or more.

Explicit marked paragraph/heading scopes prevent unrelated tagged regions from merging.
Compatible geometry groups untagged content, including runs split across content streams
or equivalent graphics transforms. Geometry cannot infer author intent in every untagged
PDF. Semantic structure-tree preservation, legacy Type 1/Type 3 inline font preview,
font collections/variable-font instances, mixed-font/style contextual runs and complete rotated
layout fidelity still need broader coverage. Do not interpret the passing real-file checks
as a guarantee for every PDF.

Private acceptance tests are opt-in and keep inputs unmodified. They exercise actual Chinese
keyboard insertion/deletion on all four pages, source-face/baseline checks and identical
in-memory renders before and after saving/reopening. Synthetic tests cover full TrueType
mapping, OpenType/CFF, Arabic marks/ligatures/mixed bidi, repeated edits, disjoint styles,
invalid ranges, font selection and root confinement. CLI acceptance repeats insertion/deletion
and two saves/reopens on four pages; independent PDFium checks extraction and rendering.
No private document, font or screenshot
is included in the repository.
