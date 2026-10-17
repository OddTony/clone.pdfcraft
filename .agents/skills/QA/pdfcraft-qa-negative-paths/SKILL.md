---
name: pdfcraft-qa-negative-paths
description: Use when PdfCraft must reject an invalid input or state, preserve work, or avoid a forbidden effect at a specific validation boundary.
---

# Prescribed rejection

Use for specified disposition of invalid arguments, unsupported operations, invalid document states. Dependency loss after valid action begins belongs to fault recovery. Do not infer every malformed PDF, missing optional capability, unsupported value must throw; lenient parsing can record repairs.

Read [task contract](references/contract.md), [raw inputs and authority](references/security-resources.md), [state snapshots](references/transactions.md). Receive rejection rule, accepted counterpart, entry point, preconditions, state/effect observations, bounded execution allowance. Reread current tool schemas, validators, permission checks, existing regressions before error assertion choice. Separate stable error category/actionable context from incidental full wording.

Arrange valid session/fixture reaching intended guard. Record pre-state/permitted diagnostic changes; call actual public seam; prove guard reachability; assert required error/fallback and forbidden effects. Compare operation-phase-appropriate logical document content, relevant history/keys/dirty/generation, destination artifacts. Log/error notice may change without authorizing content changes. Invalid document ID, collection failure, timeout cannot replace intended rejection.

Example: valid two-page synthetic document, attempted deletion of both pages. Existing engine `failed_edit_changes_nothing` covers early `WouldRemoveAllPages` path. Pair deletion of one page; assert surviving page identity; perform subsequent valid edit. Extend only observations needed for changed contract. Early guard does not establish rollback after later refresh failure.

Path rejection: first open valid document; permit same export/save inside write root; try owned outside target. Observe path guard/unchanged outside bytes. Preserve raw malformed bytes/JSON where preprocessing erases anomaly. Root rejection through nonexistent document is inadequate evidence.

Execute selected finite cases; retain first failures. If policy lacks rejection definition, report characterization; seek responsible contract decision for claim. Do not silently widen acceptance to green. Missing optional tools: native deterministic tests/source tracing. Missing required runtime capability leaves execution incomplete, no installation.

Return rule, valid/invalid pair, exact reached boundary, expected/actual error, logical-state/effect differences, follow-up recovery, execution identities, cleanup. Tests-only task may preserve justified failing regression; authorizes no product fix.

Author: David Osipov; personal@david-osipov.vision; [website](https://david-osipov.vision). Original skill material: MIT; see [licence and notices](THIRD_PARTY_NOTICES.md).
