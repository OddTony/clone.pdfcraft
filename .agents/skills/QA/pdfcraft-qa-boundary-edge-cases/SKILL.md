---
name: pdfcraft-qa-boundary-edge-cases
description: Use when PdfCraft behavior changes at a numeric, byte, Unicode, page-index, depth, allocation or queue threshold.
---

# Meaningful boundary neighbors

Use when ordered domain has specified transition/representation edge; use representative partitions for unordered categories. Ordinary success coverage or resource benchmark alone needs no such skill.

Read [task contract](references/contract.md), [units and resource limits](references/security-resources.md), [state/effect boundaries](references/transactions.md). Receive rule/authority, unit, inclusive/exclusive semantics, actual representation, entry seam, allowed effects, finite budget. Inspect current types, conversions, schemas, tests. Implementation constant shows present behavior; needs policy/specification justification before independent correctness-oracle use.

Build compact table: case ID, precondition, input, measured unit, expected disposition, forbidden effect. Choose boundary, nearest meaningful values on both sides, interior valid control. Integer plus/minus one does not describe floating-point neighbors, UTF-8 character boundaries, categorical enum. Preserve raw numeric tokens/bytes when normal construction rounds/repairs them. Explicitly cover reachable negative-to-unsigned conversion, zero/division, checked multiplication/offsets, NaN/infinity, cyclic/deep references.

Small example: exercise automation `page_render` DPI at 1 and 600, just below/above inclusive domain, supported interior value against valid synthetic page. Internal engine page index zero-based; automation pages one-based. Separately test first/last/out-of-range pages. Prove DPI/page guard reached before interpreting result; schema-only/missing-document error insufficient.

Focused seam: staging-name truncation at 128 UTF-8 bytes. Original one-byte prefix then four-byte characters puts cutoff inside character; verify name ends on character boundary, retains required suffix. Do not equate Linux byte limits with Windows filename units or claim native behavior without corresponding host.

Select only justified interactions, e.g.: nested filter expansion with output cap, several active renders plus completed-result limit. Per-item cap does not establish aggregate process budget. Observe post-state and next valid operation. Random generation supplements exact neighbors; volume cannot prove boundary reachability.

Stop affected verdict if unit, threshold contract, host capability unknown. Missing tooling: use existing deterministic cases/source analysis. Report exact representations, table coverage, reached branches, measured outcomes, omissions, first failures, cleanup; never widen limit or normalize tested condition away to obtain green.

Author: David Osipov; personal@david-osipov.vision; [website](https://david-osipov.vision). Original skill material: MIT; see [licence and notices](THIRD_PARTY_NOTICES.md).
