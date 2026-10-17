---
name: pdfcraft-pdf-fidelity
description: Use when a PdfCraft parser, editor, writer, appearance, redaction, signature, encryption, or interchange change can alter PDF meaning or preserved data.
---

# PDF fidelity and interoperability

Use for operation-specific PDF preservation/transformation. Shell-only layout change: non-trigger. Read [task contract](references/contract.md), [fidelity matrix](references/pdf-fidelity.md), and [transaction phases](references/transactions.md). Obtain exact feature/version, legal synthetic fixture, expected facts, allowed differences, forbidden loss/leakage, current APIs, required oracles, finite resource budget. Public specifications/target behavior/tests guide implementation; another PDF implementation's code is not behavior source.

Before writer run, expectation manifest: reachable unknown dictionaries/streams, aliases, page geometry/content, fields/widgets, metadata, permissions, signatures, revision history, intentionally changed dimensions. Compare graph relationships/values after save/reopen; full rewrite may change object numbers. Eligible incremental saves also require original byte prefix/coherent revision/xref chain. No prefix equality for redaction, encryption change, repaired/full-save paths or new-path full-save defaults.

Bounded example: sentinel key in trailer plus reachable private stream; edit one page, save; check expected page semantics/sentinel relationships. Wrong control deletes sentinel, preserves page count/pixels; semantic verifier must reject. Target-generated round trips: useful integration evidence, no independent writer correctness proof. Hand-author differing xref/generation structures when fixture generator shares tested code.

Qualify logical values/rendered appearances separately: AcroForm `/V`, inherited resources, widget `/AP`/`/AS`, rotated/Unicode cases may disagree. Display-only generated appearances: not saved document content. Unsupported glyphs preserved in `/V` must not silently flatten to permanent question marks. Redaction: full-history confidentiality checks across nested/shared resources, encoded content; painted rectangle/zero text-search hits insufficient.

Signatures: distinguish covered-byte integrity, later changes, DocMDP/FieldMDP permissions, trust, revocation. Encryption: verify user/owner authentication, permission behavior, declared algorithm, independent decryption, encrypted recovery. Single parse/render/signature badge cannot cover all dimensions.

Use installed external qpdf/Poppler or named profile validator only under existing oracle/licence policy, bounded synthetic copies, no upload. Absent oracle: retain explicit native assertions, record skips; required-oracle gaps incomplete. Stop for unclear rights/unsafe output. Retain artifacts without overwriting original; report exact hashes, operation/save mode, controls, raw oracle outcomes, repairs/warnings, preservation evidence, unqualified dimensions.

Author: David Osipov; personal@david-osipov.vision; [website](https://david-osipov.vision). Original skill material: MIT; see [licence and notices](THIRD_PARTY_NOTICES.md).
