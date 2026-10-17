---
name: pdfcraft-qa-i18n-localization
description: Use when PdfCraft locale selection, translations, plurals, Unicode text, script rendering, or language-dependent UI state changes.
---

# Localization and Unicode evidence

Use for locale/script-sensitive behavior. Language-neutral parser change is non-trigger unless text handling affected. Read [task contract](references/contract.md), [UI/locale reference](references/ui-platforms.md), [evidence guidance](references/evidence.md). Obtain affected keys/contexts, canonical wording, output layer, actual language registry, fallback contract, font inputs, platform, finite case budget. Read `i18n/mod.rs`, affected TSV, current localization/asset policy; translation count proves neither meaning nor glyph coverage.

Pinned-baseline registry contains exactly `en`, `ja`, `zh-hans`, `zh-hant`, `cs`, `pt-br`, `de`, `es`, `fr`, `ru`, `kk`, `bg`, `te`, `hu`, `uk`, `it`, `ar`, `pl`, `sl`, `el`, `tr`, `lt`, and `vi`. Recheck on source changes. English is source/fallback. Validate context/command-id identity, duplicate/unknown keys, escapes, placeholders, ellipsis, empty-entry behavior, required plural form counts. Derive expected plural words independently, including relevant zero, teen, hundred, boundary counts; no production plural function as oracle.

Start with `language_switch_preserves_document_and_command_ids` seam. Bounded example: switch open dirty synthetic document through affected locales; query menu/palette/tooltip/dialog labels; compare command IDs, document/path/history state. Invalid explicit `ui.set` language must return actionable error without mutation. Preference-file fallback, Auto detection, explicit user-input validation are distinct paths; unit-test/WASM system-language behavior cannot prove native Auto detection.

Separate logical storage, grapheme editing, glyph coverage, shaping, bidi order, layout. Exercise combining accents, affected mixed Arabic/Hebrew/Latin/numeric strings, composition commit/cancel, selection, copy/paste, undo at necessary layer. Arabic translation coverage cannot establish mirrored layout/correct wrapped/editing text. Missing face/replacement box is observed limitation, never successful rendering.

Use existing Rust catalogue/font checks/egui tests first. Physical IME needs actual OS/input method; raw keys insufficient. Runtime system fonts UI-only; cannot enter PDFs/published screenshots. Add fonts through approved craft-fonts policy. If fonts/native/browser execution unavailable, retain logical/source results; mark visual/input claims not run. Report exact locale/font/OS/build identities, independent expected strings, executed cases, first failures, fallback observations, native-speaker/manual gaps.

Author: David Osipov; personal@david-osipov.vision; [website](https://david-osipov.vision). Original skill material: MIT; see [licence and notices](THIRD_PARTY_NOTICES.md).
