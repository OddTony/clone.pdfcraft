# pdfcraft-render

Page rasterization and document inspection. **Bootstrap (ADR-0004):** pages render through the
vendored `hayro` (hayro-interpret + vello_cpu, patches in `vendor/README.md`) and documents are
inspected through the lazy `cos` reader. Both move to `model` and the DisplayList devices in
M1–M2; the public API stays, so the engine and UI don't change with them.

- `RenderPool` / `PageRenderer`: background and single-threaded page rasters (premultiplied
  RGBA8), tiles, previews and text extraction. Every render runs under `catch_unwind`.
- `inspect`: document information, pages, outline, annotations, fields, layers, attachments.
- `text`: page text and search.
- `separations`: Output Preview (below).

## Output Preview: separations, coverage, overprinting

`PageRenderer::page_inks(page)` lists the inks a page prints with and
`PageRenderer::separate(page, scale, overprint)` returns its `Separations`: one plate per ink,
each ink's tint per pixel. On top of that: coverage of every ink and the total at a pixel
(`coverage_at`), the page's highest total area coverage (`max_total`), the pixels above a
limit (`above`), per-ink highest tint and printed area (`stats`), one plate as film (`film`),
and the page as the chosen inks print it (`composite`, optionally with the pixels above a
coverage limit painted in a warning colour).

**How a plate is made.** The page is rendered once per ink with hayro-interpret's plate mode
(`InterpreterSettings::plate`, vendored patch 34). Every paint draws that ink's tint as a gray
film instead of its colour; a paint that overprints without specifying the ink leaves the film
alone. Everything else — clipping, anti-aliasing, soft masks, transparency groups, blending,
patterns, Type 3 glyphs, annotations — is the renderer's own, so a plate is the page as drawn,
one ink at a time. We chose this over interpreting content into coverage ourselves (which
would duplicate the renderer) and over rewriting colour spaces in the PDF (which cannot express
overprinting, a property of the graphics state rather than of the colour).

What a colour puts on a plate follows its colour space (ISO 32000-2 §8.6):

| Colour space | Inks it specifies | Tint on a plate |
|---|---|---|
| DeviceCMYK, ICCBased with 4 components | the four process inks | the component |
| DeviceGray, CalGray, ICCBased with 1 component | the four process inks | black `1 − gray`, the others 0 (§10.4.2.2) |
| DeviceRGB, CalRGB, Lab, ICCBased with 3 components | the four process inks | through sRGB and §10.4.2.4 |
| Separation | its colorant; `All` every ink; `None` nothing | the tint |
| DeviceN | its named colorants (process names are the process inks) | that component |
| Indexed | its base space's | the palette entry's |

Overprinting follows §8.6.7: with `OP` (strokes) or `op` (everything else; `OP` alone sets
both) a paint leaves the plates of inks its colour space doesn't specify as they are, and with
`OPM 1` a DeviceCMYK component of 0 leaves its plate too. Without overprinting every paint
knocks out (tint 0) the inks it doesn't specify. Soft masks keep their own colours: a mask's
luminosity never comes from a plate. The inks are the four process inks (always listed, as a
press runs them) and then the spot inks the page's paints use, in the order first painted,
found through `InterpreterSettings::ink_sink`; an ink only a soft mask uses is not listed.

**What is approximated** (each a known difference from Acrobat's Output Preview):

- RGB, gray and CIE-based colours become CMYK by the ISO device formulas with black generation
  and undercolour removal as the identity, not through an ICC simulation profile or the
  document's output intent, so RGB artwork separates into heavier black and lighter CMY than a
  colour-managed conversion would.
- ICCBased CMYK colours are taken as they are (their profile is assumed to be the output
  condition); there is no CMYK-to-CMYK conversion.
- The composite shows the process plates through the CMYK profile pages are drawn with (a
  CGATS-compatible one) and multiplies each spot ink's own appearance (its alternate colour at
  that tint) over them. Real inks don't mix exactly multiplicatively. Paper colour and black
  ink simulation are not offered.
- `OPM 1` applies to colours set in DeviceCMYK only, not to ICCBased CMYK, images or shadings.
- Transfer functions, `BG`/`UCR` in the graphics state and halftones are not applied to plates.
- Plates are blended as gray films, which is ISO's complemented blending for separable blend
  modes in a subtractive space; the non-separable modes (Hue, Saturation, Color, Luminosity) and
  transparency groups that blend in RGB are approximated.
- A spot ink's appearance comes from the first colour space that names it.
- At most `MAX_SPOT_INKS` (32) spot inks are separated per page; more are listed, not separated.
  Plates hold at most `MAX_PLATE_PIXELS` (4 MP) each, so very large pages separate at a lower
  resolution. Coverage is per pixel at that resolution, anti-aliased edges included.
- No Ink Manager: spot inks can't be aliased or converted to process.
