---
name: pdfcraft-qa-golden-fixture-parity
description: Use when PdfCraft output needs comparison with a reviewed semantic, byte or pixel fixture whose provenance and permitted variation can be established.
---

# Reviewed output comparisons

Use when structured/large expected output benefits from fixture. Small scalar expectation/unknown historical file does not automatically warrant golden testing. Fixture needs provenance; age/matching output does not prove correctness.

Read [task contract](references/contract.md), [golden/control guidance](references/generative.md), [PDF fidelity rules](references/pdf-fidelity.md). Receive output boundary, independent invariants, fixture rights/hash/creation-review history, producer/source/configuration, allowed normalization, owned output, finite budget. Inspect real producer/effects before generating. Use contributor-original tiny PDFs; no proprietary corpus, fonts, Adobe visual material.

Create comparison matrix: required semantic facts, exact representation dimensions, allowed differences, unresolved observations. PDF object renumbering/full rewriting may preserve meaning despite byte changes; eligible incremental saves may require exact old prefix. Compare operation-relevant unknown key values/graph edges, page identity/order, effective form values/appearances, metadata. Parse/render agreement can miss preservation defects. Same-parser round trip supplies compatibility evidence with shared-defect limit.

Example: freeze three-page synthetic fixture with distinct labels/unknown sentinels. Produce unrelated rotation/save result into new owned output; preserve both artifacts; compare against independently written semantic manifest. Challenge comparator: remove one sentinel, retain page count/pixels; must fail. Also test permitted benign object-number/incidental timestamp variation under specifically reviewed rule. Do not strip every date, ID, warning, order, Unicode distinction by default.

Pixel goldens: freeze renderer/backend, scale, colour configuration, approved font inputs; retain exact raw images/separately justified tolerance. Pixel comparison cannot qualify logical fields, historical secrets, object preservation. External qpdf/Poppler processes provide different evidence; retain version, exit/warnings, declared absence. `PDFCRAFT_ORACLES_REQUIRED` set to any value makes absence required; skipped oracle is no independent agreement.

Inspect every material diff; classify deliberate change, stale fixture, unstable environment, product defect, unresolved cause. Baseline regeneration is separate review decision; preserve old/new fixture, raw output/diff, rationale; compare freshly. Missing fixture authority/required native capability stops only affected verdict. Use native literal assertions/source design if optional snapshot tools absent.

Report required/ignored dimensions, hashes/identities, actual producer/comparison execution, comparator controls, first differences, normalization decisions, cleanup. Do not update baseline/suppress semantic difference to pass.

Optional snapshot update modes change expectations; use comparison mode for qualification; explicitly review pending updates. Challenge any filter/redaction that could erase contractual sentinel/page order. Read relevant tool-specific section in [generative](references/generative.md).

Author: David Osipov; personal@david-osipov.vision; [website](https://david-osipov.vision). Original skill material: MIT; see [licence and notices](THIRD_PARTY_NOTICES.md).
