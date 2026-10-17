---
name: pdfcraft-qa-build-content-metadata
description: Use when PdfCraft changes PDF Info or XMP, generated reports or catalogues, attributed assets, or the identity of built output.
---

# Generated output and metadata

Use for emitted content/metadata claim; ordinary algorithm testing without producer/output relationship is non-trigger. Read [task contract](references/contract.md), [PDF semantic checks](references/pdf-fidelity.md), [build/dependency guidance](references/performance-dependencies.md). Identify canonical facts, producer, consumers, affected output, approved differences, fixture rights, current source/lock/configuration, finite execution scope. Read current repository policy/producer code before trusting old documentation.

For PDF properties, trace automation `doc_set_info` through `Edit::SetInfo` and `pdfcraft_organize::set_info` to serialized output. Inspect trailer `/Info` and catalog `/Metadata`. Current XMP synchronizer maps six common properties only when existing packet safely editable; creates none without XMP, preserves unsafe/oversized/unsupported packets. No unconditional synchronization promise or erasing unknown namespaces to satisfy comparison.

Define independent expectations: value encoding, trimming/removal semantics, XMP language alternatives/author sequence, unknown keys, indirect relationships, stream payloads. Bounded example: change Title containing `&` and `<` in synthetic document with matching Info/XMP and private sentinel property. Save/reopen actual bytes; check intended value, relevant escaped representation, unchanged sentinel semantics, unaffected page content. Challenge verifier with Info-only change leaving stale XMP: parse success, page count, identical pixels must not hide mismatch. Separately test absent/uneditable XMP under actual preservation contract.

For generated accessibility reports, translation/catalogue data, parity tables, credits, trace canonical inputs to actual generated artifact/consumer. Preserve entity identity, required fields, multiplicity, links, escaping, locale, explicit ordering. Source values cannot prove current generated file. Record source/input/output hashes; distinguish old build/newly emitted content.

Review build scripts, optional fonts, asset attribution, transitive effects before execution. Never hand-edit generated attribution/credits to green, import unlicensed asset, assume `--locked` forbids fetching. Existing semantic Rust tests and `cargo xtask assets` are native starting points after preflight. If Rust/output generation unavailable, finish source comparison; mark emitted-output claims not run. Report positive/wrong-control outcomes, actual artifacts, selected/executed checks, first failures, remaining inconsistencies, pending gates.

Author: David Osipov; personal@david-osipov.vision; [website](https://david-osipov.vision). Original skill material: MIT; see [licence and notices](THIRD_PARTY_NOTICES.md).
