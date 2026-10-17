---
name: pdfcraft-qa-model-based-testing
description: Use when PdfCraft edit, save, undo, redo or lifecycle sequences need comparison with a small independent state model.
---

# Independent command histories

Use for temporal obligations missed by isolated calls. Stateless single-value property/rendering snapshot alone needs no command model. Sequential command agreement cannot establish concurrent schedule coverage.

Read [task contract](references/contract.md), [model/replay guidance](references/generative.md), [transaction snapshots](references/transactions.md), [PDF semantics](references/pdf-fidelity.md). Receive public commands/observables, initial fixture, transition requirements, rejection/effect rules, allowed writes, finite sequence/case budget. Inspect current engine/automation APIs/existing undo/save tests. Start with explicit vectors if crate lacks suitable existing generator dependency.

Build smaller requirements-derived model: page identities/rotations, title, saved checkpoint, undo/redo transitions, dirty state. Specify per-command eligibility, expected model transition, public SUT action, postcondition. Do not query SUT output or reuse `scope_of`, writer output, production history helpers for expectations. Count generated, eligible, executed, rejected/skipped commands; sequence filtered to no work inadequate. Legitimately rejected operations should have explicit expected transitions, not disappearance through preconditions.

Bounded example: pages A/B/C, unknown sentinel. Run `move C first → rotate C → undo → save/mark-saved → redo → new edit → rejected delete-all`. Predeclare page order, rotations, redo availability, saved checkpoint/dirty semantics, sentinel preservation after each meaningful step. Reopen prepared/saved bytes at selected checkpoints. Challenge oracle with reviewed missing undo push/failure to clear redo after successful new edit in isolated copy. Ordinary sequence must pass; intended assertion must detect bad behavior.

Separate phase-sensitive failures. Late-refresh apply/undo/redo concern needs healthy control, reached fault, complete allowed state contract before defect claim. Compound flatten-save may have post-edit snapshot. Diagnostics can change without logical document-content change.

Every sequence/shrink/replay creates fresh model, real session, owned resources. Bound length, cases, invalid-command proportion, shrinking, workers, total time. Preserve exact original/minimized commands, expected/reached transitions, seed/options/version, any schedule. Cleanup must preserve both assertion and teardown causes; stop replay on failed reset unless fresh isolation demonstrated.

Missing optional state-machine/model-checking tools: native deterministic traces. Do not add dependency or claim exhaustive interleavings. Report executed transitions/exclusions, semantic mismatches, control sensitivity, replay/cleanup evidence, all unrun runtime dimensions.

Loom/Shuttle proposals: inventory instrumented versus ordinary synchronization, timers, I/O, exploration bounds. Passing bounded model cannot qualify unmodeled native schedules/worker cleanup. Read relevant tool-specific section in [generative](references/generative.md).

Author: David Osipov; personal@david-osipov.vision; [website](https://david-osipov.vision). Original skill material: MIT; see [licence and notices](THIRD_PARTY_NOTICES.md).
