# Shared PDF reader

`pdfcraft-render` uses `craft-pdf` for parser construction, backend types and bounded
raster execution. The workspace enables only its `raster` feature, which includes
reading and interpretation. PDF writing, the COS editor and the locally patched
`lopdf` inspector remain app-owned.

The shared package and the four root Hayro patches must resolve to one immutable
`storytold/craft-libs` Git revision. Cargo does not inherit dependency workspace
patches, so the consuming workspace carries all four entries. `cargo xtask pdf-deps`
checks their exact versions and source, the shared feature set, and the application's
production dependency boundary. Existing backend regression tests retain development
access to the same Hayro sources.

| Package | Version | Licence |
| --- | --- | --- |
| craft-pdf | 0.1.0 | MIT OR Apache-2.0 |
| hayro | 0.7.1 | MIT OR Apache-2.0 |
| hayro-interpret | 0.7.0 | MIT OR Apache-2.0 |
| hayro-syntax | 0.7.2 | MIT OR Apache-2.0 |
| hayro-jbig2 | 0.3.0 | MIT OR Apache-2.0 |

The exact revision is recorded in `Cargo.toml` and `Cargo.lock`. Its source and patch
ledger retain the application's existing Hayro fixes. Licence texts and dependency
asset notices remain in `licenses/pdf-backend/`, `NOTICE` and `ATTRIBUTION.toml` and
are included in package documentation. The shared revision must be published before
a fresh checkout can resolve it. Rust 1.95 remains the workspace minimum.

## Application policy

The general viewer accepts already-loaded inputs without adding the shared helper's
512 MiB default input cap: its explicit `OpenOptions` set `max_input_bytes` to
`usize::MAX`. File and remote-link entrypoints retain their own policies. Immutable
backend decode limits bound ordinary stream decoding, including parser repair paths;
they do not bound total document or process memory.

PdfCraft retains password compatibility, COS and `lopdf` inspection, its text extraction
device, font substitution, optional-content selection, annotation policy, reading
colours, print bypass, preview choice, worker scheduling, cancellation and watchdogs.
A native backend panic retires the affected parser/cache generation as before. Browser
rendering does not gain the native worker watchdog, and WASM aborts cannot be caught.

Raster entrypoints validate the explicit viewport before writing into the app's existing
word-aligned pixel buffer. Full pages keep rounded-up edge coverage within 8192 pixels
per side and 64 million pixels; tiles keep their own 4096-pixel bound. The GUI takes
ownership without another pixel copy. Known content omissions still reach the app's
`ContentTruncated` warning, but diagnostics describe only backend warnings that were
actually emitted; a successful render is not a fidelity guarantee.

## Updating the dependency

1. Review the shared source changes, patch ledger and regression evidence.
2. Advance `craft-pdf` and all four Hayro patches together, update the expected revision
   in `xtask/src/pdf_deps.rs`, and refresh the lockfile without unrelated upgrades.
3. Run `cargo xtask pdf-deps` and `cargo xtask ci`; inspect the resolved feature graph.
4. Exercise native and browser rendering separately, including malformed input, passwords,
   tiles, rotation, crop boxes, `/UserUnit`, text, layers and reading colours.

The `lopdf`, `winit` and `egui-wgpu` sources remain local and can advance independently.

The application regressions also retain missing-Encoding TrueType WinAnsi extraction and packed TIFF predictor row boundaries. Arabic ActualText editing order and Type 3 extraction resource ownership remain app-level behavior, with save/reopen coverage.
