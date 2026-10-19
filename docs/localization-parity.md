# Localization parity

> **Last reviewed:** 2026-10-19 · **Last updated:** 2026-10-19 · **Change:** patch (Korean PR refreshed on main 43550c44; 2,674 current catalog keys; measured scoped verification; estimates unchanged) · **Target:** Adobe Acrobat Pro (Acrobat DC, continuous track 26.002.21931, macOS)
> **Last reviewed:** 2026-10-16 · **Last updated:** 2026-10-16 · **Change:** patch (Korean PR refreshed on main f0c9901c; 2,672 catalog keys and measured scoped verification; upstream progress and historical evidence retained; estimates not re-measured) · **Target:** Adobe Acrobat Pro (Acrobat DC, continuous track 26.002.21931, macOS)
> **Last reviewed:** 2026-10-15 · **Last updated:** 2026-10-15 · **Change:** minor (Korean PR conflict refresh on upstream main dc41044b; common catalog and regression repairs retained from main; scoped verification repeated; readiness and effort estimates not re-measured) · **Target:** Adobe Acrobat Pro (Acrobat DC, continuous track 26.002.21931, macOS)
> **Last reviewed:** 2026-10-14 · **Last updated:** 2026-10-14 · **Change:** minor (Korean desktop catalog, date names and installed Hangul fallback; readiness and hours not re-measured) · **Target:** Adobe Acrobat Pro (Acrobat DC, continuous track 26.002.21931, macOS)
> **Last reviewed:** 2026-10-10 · **Last updated:** 2026-10-13 · **Change:** minor (Vietnamese 100% of current key union; native-speaker community review; twelve-language mean ≈68%) · **Target:** Adobe Acrobat Pro (Acrobat DC, continuous track 26.002.21931, macOS)
> **Last reviewed:** 2026-10-10 · **Last updated:** 2026-10-14 · **Change:** minor (Italian count after completing its catalog) · **Target:** Adobe Acrobat Pro (Acrobat DC, continuous track 26.002.21931, macOS)

Per-language status of PdfCraft's interface against Acrobat Pro's. How translations work and how to
add a language: [localization.md](localization.md). Summary in [ROADMAP.md](../ROADMAP.md).

## How this was measured

**Current main refresh (`43550c44`, 2026-10-19).** Korean PR refreshed on main `43550c44`; Korean, Lithuanian and Vietnamese each cover 2,674 keys. 1,405 scoped native tests passed, 0 failed, 2 ignored; 3 behavioral assertion probes verified with exact source restoration. Earlier verification retains its stated bases; estimates unchanged. Full scope and limits: [localization.md](localization.md#korean).

**Historical main refresh (`f0c9901c`, 2026-10-16).** The 2026-10-16 refresh integrates main `f0c9901c`, preserves upstream renderer/font, signature-overlay and Nix workflow progress, and adds the two Korean snapshot notices (2,672 keys total). User document names and revision placeholders remain literal. Measured Windows checks: **1,325 scoped native tests passed, 0 failed, 2 ignored** across 5 library suites and 6 UI integration targets. 2 current targeted removed/inverted-condition probes failed their expected behavioral assertions; source bytes were restored exactly before positive reruns. Strict scoped all-target native Clippy, formatting, assets, layers, partial parity and wasm32 compilation passed; wasm retains 7 Rust warnings. Repeated font modes are separate from the native aggregate, and missing-font early returns are harness passes rather than glyph-coverage evidence. The older 929-test records describe their stated dc41044b/afa8e184 bases. These are local checks, without full-workspace, upstream CI, release rebuild, native OS IME or macOS/Linux/Nix runtime claims. Web Hangul font delivery and Korean PDF writing remain separate unsupported work. Readiness and effort estimates have not been re-measured.

**Historical main refresh (`dc41044b`, 2026-10-15).** Lithuanian, Vietnamese and Traditional Chinese now use the exact upstream catalogs from #2510/#2414; the earlier common repairs are already on main. Korean remains unchanged at 2,670 keys. The 2026-10-15 conflict refresh on upstream main `dc41044b` recorded **929 scoped native tests passed, 0 failed, 2 existing tests ignored** across the same three libraries and five UI integration targets. Repeated font checks with installed fonts disabled and without craft-fonts passed; optional missing-font checks may return early and are not added to the native total. Three mutation probes were compiled together, each failed its expected targeted assertion, and source bytes were restored exactly before positive reruns. Strict scoped native all-target Clippy, formatting and scoped wasm32 compilation passed; wasm retains existing warnings. Actual OS Korean IME, macOS/Linux/NixOS runtime, web Hangul font delivery and Korean PDF text writing remain unverified or unsupported as previously recorded. This is local scoped verification, without a full workspace sweep or upstream CI claim.

**Historical afa8e184 verification.** Validation on upstream main `afa8e184` plus this local integration: **929 scoped native tests passed, 0 failed, 2 existing tests ignored** (778 library tests in `pdfcraft-engine`, `pdfcraft-forms` and `pdfcraft-ui-egui`; 151 UI integration tests across commands, control, fonts, reload and viewer). The 12 font tests also passed without craft-fonts and with `PDFCRAFT_SYSTEM_FONTS=0`; these repeated mode checks are not added to 929. Ten removed/inverted-condition probes produced the expected assertion failures; source bytes were restored exactly and all positive checks rerun. Strict scoped all-target native Clippy, formatting, asset, layer, partial-parity and scoped wasm32 checks passed. Wasm retains seven pre-existing Rust warnings in five files unchanged from `afa8e184`; the warning about deliberately excluding Noto CJK is the existing asset policy. Validation uses Windows headless UI and synthetic IME events: actual OS Korean IME and macOS/Linux/NixOS runtime remain unverified. Web Hangul font delivery and Korean PDF text writing remain unsupported. The scope excludes a full workspace sweep and upstream CI results.

**2026-10-15 Korean integration:** Korean covers the 2,670-key bundled source/context union after merging upstream main `afa8e184`, adding 78 keys to the 2,592-key 2026-10-14 base. Scoped post-integration catalog, Windows glyph and production language-switch checks passed. The older per-language percentages and upstream weighted localization estimate below retain their recorded snapshots; they have not been re-measured for Korean. Web font delivery, localized help and Korean PDF text writing remain separate work.

The first integrated native run also exposed upstream catalog inconsistencies: two duplicate
Traditional Chinese rows, 63 missing Vietnamese keys and four missing Lithuanian keys. Removing
only the duplicate rows and filling the missing keys brought Vietnamese from 2,601 to 2,664 and
Lithuanian from 2,660 to 2,664. These 63 Vietnamese and four Lithuanian additions are AI-assisted
and have not had native-speaker review; the community-review records below concern the earlier
catalogs. A later refresh to upstream main `afa8e184` adds six further keys, bringing Korean,
Vietnamese and Lithuanian to 2,670 each. Final scoped verification of that refreshed state passed.

- **Strings translated** is measured: the catalogs in `crates/ui-egui/src/i18n/<code>.tsv`, counted
  as distinct `(context, English)` keys with a non-empty translation. The denominator is
  the union of keys across all catalogs. The Vietnamese measurement used **2,590** plain keys
  as of 2026-10-13, plus language-specific context rows such as `theme`/`Appearance`; this is
  a historical snapshot rather than a claim of coverage for every later key. The twelve-language percentage table below
  still quotes the **historical 2,143-key snapshot** from 2026-10-10 for every language except
  Korean and Vietnamese, which are listed against the current 2,674-key union. English is built in.
- **Not counted:** error details from the engine and the operating system, which are shown in
  English inside a translated frame; there is no translated user documentation or help yet (Acrobat
  ships localized help).
- **Script support** comes from [localization.md](localization.md), the vendored winit IME patches
  (`vendor/README.md`) and user reports.
- **Native review**: "community" means a native speaker contributed or corrected the catalog in a
  pull request or issue; "no" means no record of review. No catalog has had a formal review.
- **Acrobat** ships 24 interface localizations on macOS (`.lproj`: cs, da, de, en, es, fi, fr, hu,
  it, ja, ko, nl, no, pl, pt, ru, sv, tr, uk, zh_CN, zh_TW, plus en_AE, en_IL, fr_MA) and Middle
  East / North Africa editions (MEA Arabic, MEH Hebrew, NAF French; `Resources/Sequences/`).

## The twelve key languages

| Language | Code | UI strings translated | Dialogs / tooltips / help | Script support | Native review | Status | Acrobat ships it | To `full` (h) |
|---|---|---|---|---|---|---|---|---|
| English | `en` | built in (100%) | all; README and docs | Latin | yes | **full** | yes | 0 |
| Simplified Chinese | `zh-hans` | 2,088 (97.4%) | dialogs and tooltips through `tl!`; no help | CJK text and IME work, but **release builds have no Hans face**: many characters show as boxes (#826, #688, #728, #758; Flatpak #689) | no | partial | yes | 6–12 |
| Spanish | `es` | 2,046 (95.5%) | dialogs and tooltips; no help | Latin | no | partial | yes | 2–4 |
| Hindi | `hi` | 0 | none | Devanagari not tested (egui shapes Telugu conjuncts, so shaping is likely workable); no Devanagari face in craft-fonts | no | none | no | 10–18 |
| Arabic | `ar` | 2,143 (100%) | dialogs and tooltips; no help | Letters joined and lines reordered at load, but **layout not mirrored**, wrapped lines break at the wrong end, typed text runs left to right (`localization.md` §Arabic) | no | partial | yes (Middle East edition) | 25–45 |
| French | `fr` | 2,128 (99.3%) | dialogs and tooltips; no help | Latin | no | partial | yes | 1–2 |
| Portuguese (Brazil) | `pt-br` | 2,130 (99.4%) | dialogs and tooltips; no help | Latin | community (#288) | partial | yes | 1–2 |
| Indonesian | `id` | 0 | none | Latin | no | none | no | 3–5 |
| Japanese | `ja` | 2,123 (99.1%) | dialogs and tooltips; no help | BIZ UDPGothic from craft-fonts; IME patched on macOS, Windows, X11, Wayland; vertical text renders but can't be edited | no | partial | yes | 4–8 |
| German | `de` | 2,128 (99.3%) | dialogs and tooltips; no help | Latin | community, in progress (#828: terminology, clipping) | partial | yes | 2–4 |
| Korean | `ko` | 2,674 current keys | menus, dialogs, tooltips and translatable engine wordings; no localized help | Desktop reads installed Hangul faces in every UI family, with alternate Windows/macOS/Unix candidates and a 96 MiB collection limit; no embedded Korean face or web delivery | AI-authored, cross-reviewed; no external native-speaker review | partial | yes | not re-estimated |
| Vietnamese | `vi` | 2,674 current keys | dialogs and tooltips; no help | Latin with stacked diacritics; bundled Inter covers them (fonts test) | community for the earlier catalog; 63 AI-assisted additions await native review | partial | no | 2–4 |

**Localization dimension ≈ 68% (historical upstream snapshot)**: the mean over the twelve,
counting a catalog's percentage but only 60% for Chinese and Arabic until their script problems
are fixed. That snapshot counted Hindi, Indonesian and Korean as 0; Vietnamese moved from 0%
to 100% in that mean (59% → 68%). Korean now has desktop coverage, but the weighted figure has
not been re-measured for this addition. **≈ 55–110 h**
to bring all twelve to `full`, of which ≈ 35–60 h is engineering (Hans face, RTL layout, bidi text)
and the rest catalogs and fixes; native-speaker review needs humans.

No language is `full` except English: every catalog leaves engine error details in English, and
none has translated help. Vietnamese's **To `full`** hours are therefore residual help/docs polish
(catalog and script work are already done), not missing UI strings.

## Other shipped languages

| Language | Code | UI strings translated | Status | Acrobat ships it |
|---|---|---|---|---|
| Traditional Chinese | `zh-hant` | 2,016 (94.1%) | partial (same Hans/Hant face caveat) | yes |
| Russian | `ru` | 2,043 (95.3%) | partial | yes |
| Bulgarian | `bg` | 2,094 (97.7%) | partial | no |
| Czech | `cs` | 135 (6.3%) | menus only | yes |
| Telugu | `te` | 2,058 (96.0%) | partial (the release-pinned craft-fonts has no Telugu face yet) | no |
| Hungarian | `hu` | 2,126 (99.2%) | partial | yes |
| Ukrainian | `uk` | 2,142 (100%) | partial | yes |
| Italian | `it` | 2,528 entries (every key of the other catalogs that the code looks up) | partial | yes |
| Kazakh | `kk` | 2,327 entries | partial | no |
| Polish | `pl` | 2,287 entries | partial | yes |
| Slovenian | `sl` | 2,324 entries | partial | no |
| Greek | `el` | 2,341 entries | partial | no |
| Turkish | `tr` | 2,313 entries | partial | yes |
| Lithuanian | `lt` | 2,674 current keys | partial; community native-speaker review of the earlier catalog ([#1401](https://github.com/storytold/pdfcraft/pull/1401)); four AI-assisted additions await native review | no |

**14 other languages** ship (23 catalogs plus English = **24** interface languages in Preferences).
The entry counts without percentages were measured on 2026-10-12–13 (Italian on 2026-10-14; Korean, Vietnamese and Lithuanian on 2026-10-19); they are not percentages of the older 2,143-key snapshot above. Their presence does not imply a detailed linguistic or runtime review. Vietnamese is shipped here but not by Acrobat.
Acrobat languages PdfCraft lacks: Danish, Dutch, Finnish, Norwegian, Swedish, Hebrew.

## Revision history

| Date | Change | Summary |
|---|---|---|
| 2026-10-19 | patch | Korean PR refreshed on main `43550c44`; Korean, Lithuanian and Vietnamese each cover 2,674 keys. 1,405 scoped native tests passed, 0 failed, 2 ignored; 3 behavioral assertion probes verified with exact source restoration. Earlier verification retains its stated bases; estimates unchanged. Scope and limits: [localization.md](localization.md#korean). |
| 2026-10-16 | patch | Korean PR refreshed on `f0c9901c`; two snapshot notices bring the catalog to 2,672 keys. 1,325 scoped native tests passed, 2 ignored; 2 behavioral assertion probes verified with exact source restoration. Earlier verification retains its stated bases. Estimates unchanged. |
| 2026-10-15 | patch | Refreshed the Korean PR against `dc41044b`, retaining upstream common catalog and page-draft/Monospace regressions. 929 scoped native tests passed, 2 ignored; three batched assertion probes verified. Korean retains 2,670 keys; historical `afa8e184` verification and estimates are unchanged. |
| 2026-10-15 | patch | The first integrated native run exposed two duplicate Traditional Chinese rows and missing Vietnamese/Lithuanian union keys. Removed only the duplicate rows and added 63 Vietnamese/four Lithuanian translations (both 2,664 keys at that baseline repair); these additions are AI-assisted and await native review. The later main `afa8e184` refresh brings both to 2,670. Final scoped integrated verification passed on `afa8e184`. |
| 2026-10-15 | minor | Reconciled Korean and Vietnamese registrations (23 catalogs plus English), retained Italian completion and historical estimates, and recorded the 23-string Korean wording follow-up. Post-integration scoped verification passed on `afa8e184`. |
| 2026-10-14 | minor | Korean desktop catalog covers 2,592 current keys; date names and installed interface-only Hangul fallback added. Follow-up corrects 19 wordings and expands installed-font candidates with a 96 MiB Hangul collection cap, retaining Chinese font priority. Web font delivery, localized help and Korean PDF writing remain separate. Weighted readiness and hours not re-measured. |
| 2026-10-13 | minor | Vietnamese (`vi`) catalog: 2,591 entries (100% of current key union + `theme`/`Appearance`); twelve-language mean ≈ 68%; native-speaker community review; registered with `plural_none` |
| 2026-10-14 | minor | Italian: 2,528 entries after adding the 201 keys the other catalogs had and the code looks up |
| 2026-10-12 | minor | Added current counts for Kazakh, Polish, Slovenian, Greek, Turkish and Lithuanian; retained existing language assessments and the original percentage snapshot |
| 2026-10-10 | major | Created. Catalog counts measured from `crates/ui-egui/src/i18n/*.tsv` against the 2,143-key union; Acrobat's localizations from its bundle's `.lproj` and Sequences folder names |
