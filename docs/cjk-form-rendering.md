# CJK form rendering and editing

An existing form can store correct Unicode in `/V` while its appearance stream
uses a non-embedded CJK font. The editing overlay shows the value using UI fonts;
leaving an unchanged field reveals the original `/AP` again. Regenerating that
appearance only after an edit does not fix initial viewing.

The renderer now resolves AdobeGB1 to an approved Hans face and AdobeCNS1 to an
approved Hant face from the optional craft-fonts input. Japan1 keeps its existing
Japanese style selection. CMap decoding supplies Unicode; source widths and
placement remain authoritative. Viewing does not rewrite the PDF, create undo
entries, or set the dirty flag.

When editing CJK text, form appearances and page text use embedded approved
faces and their real glyph advances. A source font is reused only when both
its encoding and embedded glyph coverage support the value: `/ToUnicode` alone
does not prove that a subset contains the glyphs. Mixed runs can fall back across
Japanese, Simplified Chinese and Traditional Chinese faces. Appearance and
AcroForm resources are kept together for save/reopen and subsequent edits.

## Build input and scope

The Chinese faces are IBM Plex Sans SC/TC Regular, authored by IBM and Sandoll,
licensed under OFL-1.1 and pinned in craft-fonts with checksums and source URLs.
See [craft-fonts PR #26](https://github.com/storytold/craft-fonts/pull/26).
No font binary belongs in this repository. Windows MSI and portable packages
carry the embedded fonts' license notices.

Builds without the optional font input retain their existing missing-font
behavior. The web build retains its smaller font selection; this does not add
Chinese fonts to WASM. Vertical layout and full language-specific typography
are not added.

## Regression coverage

Contributor-original GBK and Big5 fixtures exercise prefilled appearances before
any `form_fill` call, unchanged focus/blur, clean state, and full/tiled rendering
at 65%, 100%, 200% and 400%. Separate tests cover embedding, mixed runs, source
subset coverage, multiline/comb/password/choice fields, undo, save and reopen.
No private PDF or screenshot is included.

With `CRAFT_FONTS_DIR` pointing to the pinned input and
`CRAFT_FONTS_REQUIRED=1`, run:

```sh
cargo test -p pdfcraft-fonts -p pdfcraft-forms -p pdfcraft-edit \
  -p pdfcraft-render -p pdfcraft-automation -p pdfcraft-ui-egui \
  portable_cjk -- --include-ignored
```

Linux CI explicitly runs this acceptance suite so missing Hans/Hant inputs fail
rather than silently skipping the important tests. Windows CI continues to
exercise the configuration without optional fonts.
