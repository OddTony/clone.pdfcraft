# pdfcraft-optimize

Layer L4: the PDF Optimizer (execution plan M11.1; Acrobat's Reduce File Size and Optimize PDF ▸
Advanced optimization).

```rust
let report = optimize(&mut doc, &Settings::default())?;   // Screen / email choices
let report = optimize(&mut doc, &Preset::Mobile.settings())?; // smaller, for screens
// Or with progress: called before each image and the clean-up; `false` cancels.
let report = optimize_with_progress(&mut doc, &settings, &mut |stage| keep_going)?;
```

- **Presets:** Standard (Reduce File Size's choices: images above 225 ppi to 150 ppi, JPEG
  medium) and Mobile (above 144 ppi to 96 ppi, JPEG high), as in Acrobat's Settings menu.
- **Merge image fragments** (`fragments`, on by default): some PDF printers draw a page picture
  as thousands of images one pixel row high. Runs of upright 8-bit grey or RGB images in one
  colour space, sharing their left and right edges and stacked edge to edge (at least four,
  drawn with nothing else between them), become one lossless image at their finest resolution,
  which the image settings then resample; the strips are dropped by Clean Up. A real page
  printed this way went from 4.3 MB to 231 KB (Standard) and 161 KB (Mobile). Not merged yet:
  side-by-side tiles, rotated strips, masks.
- **Images:** the effective resolution of each image is measured where pages draw it (the CTM
  through form XObjects, up to 12 deep; the smallest over all uses). If a use can't be
  measured (forms nested deeper, or drawn too many times), no image is resampled. Colour and grayscale images above a
  threshold are resampled with a bicubic (Catmull-Rom) filter to a target resolution and
  stored as JPEG (Acrobat's quality steps) or ZIP; soft masks are resampled with their image.
  An image is replaced only if the result is smaller. Left alone: CMYK and other colour spaces,
  image masks, decode arrays, more than 8 bits, JPEG 2000, JBIG2 and CCITT.
- **Discard objects:** thumbnails, alternate images, document tags, print settings.
- **Clean up:** Flate for streams with no filter; for streams with an outer ASCII85 or ASCIIHex
  layer, that layer is removed when the result is smaller (the inner filters keep their encoded
  bytes). Streams with `/DecodeParms`, images and object streams are left alone.

The engine runs Remove Hidden Information (`pdfcraft-redact`) for the user-data categories,
then this, then merges identical objects and writes a full, compressed save
(`pdfcraft_engine::optimizer::OptimizeJob`, which the UI runs on a worker thread with a
progress bar). JPEG decoding and
encoding and resampling come from the `image` crate.

## Reduce File Size quality

`ReduceQuality` is separate from the advanced optimizer's Standard/Mobile presets.
`Screen` returns `Settings::default()` unchanged (150 ppi above 225, JPEG 60).
`Print` uses 300 ppi above 450 and `Compression::JpegOnly(85)`: only supported,
unmasked JPEG photographs are recompressed, and only when smaller. Lossless image
streams retain their original encoding, resolution and exact samples, including
Flate line art. Fragment merging is disabled because merging mixed fragments can
resample lossless inputs. Masked JPEGs and unsupported image formats stay unchanged.
This can produce larger copies than Screen, or no beneficial reduction at all.

The UI asks before starting and remembers only the last confirmed quality. The
existing worker, progress, cancellation and reduced-copy save rules are reused.
`doc_reduce quality=screen|print` exposes the same choices; omitting it preserves
the CLI default. Standard, Mobile and the advanced dialog are unchanged.

There is no output-size estimate. “Keep image quality” is deferred to the independent
lossless-profile work in PR #1282; Print itself is **not** a lossless document mode.
Generated fixture coverage is in `tests/optimize.rs` (`reduce_quality_baseline`,
`print_preserves_line_art_pixels_and_document_structure`, `print_threshold_and_masked_jpegs`).
