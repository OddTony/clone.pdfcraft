---
name: pdfcraft-qa-unit-tests
description: Use when a small PdfCraft Rust behavior needs focused assertions through a real function or engine seam, including error and preservation cases.
---

# Test one behavior

Use for bounded function, transformation or session invariant. Fully mocked call proves wiring only; filesystem publication, full CLI journeys and native UI need other evidence.

Read current rules and [scope contract](references/contract.md). Inputs: requirement, reachable public seam, relevant callers/tests, source/dirty/fixture identity, permitted files, execution budget. Use [Rust workflow](references/rust-workflow.md) for Cargo discovery/doctests; [evidence guidance](references/evidence.md) for run records.

1. Write preconditions, action, independent expectation, forbidden effects. Prefer explicit distinct values/exact error variants over no-panic/output-count assertions. Accepted control must fail reject-everything behavior.
2. Keep fast real collaborators exposing contract. Substitute only actual effect/nondeterministic input; disclose substitution/evidence limit. Synthetic PDF expectations: declared facts/independent model, not tested writer.
3. Example: valid two-page synthetic document; `Session::apply` deletion of both zero-based page indices; require `EditError::Organize(OrganizeError::WouldRemoveAllPages)` plus unchanged bytes, dirty state, history. Existing `tests::failed_edit_changes_nothing` covers early rejection; cannot establish late refresh rollback or every session-state field.
4. Select finite named test; verify discovery before execution. Source-checked unrun example: `cargo test -p pdfcraft-engine -j 2 --locked --offline tests::failed_edit_changes_nothing -- --exact --test-threads=2`; reviewed outer deadline required. Zero-test exit incomplete.
5. Regression claim: intended assertion fails with actual fix reverted in owned execution copy, passes after authorized change. Already-correct behavior may pass immediately; report honestly. Tests-only scope: retain valid RED, route defect.
6. Join owned asynchronous work; observe cancellation/cleanup before teardown hides it. Generated cases: fresh state per case/shrink, finite bounds, replay data. Deterministic boundary tables: no-new-dependency fallback.

Unavailable Rust/cache or unreachable preconditions: stop dependent execution; continue source review explicitly unrun. Output actual selected/executed tests, first failures/retries, native logs, compiler/feature/fixture identities, cleanup, remaining gates. `--all-targets` alone cannot cover doctests.

Retain libtest regression for process-global interference even if nextest passes. Separately limit build jobs, concurrent tests, shared external resources. Read relevant tool-specific section in [rust workflow](references/rust-workflow.md).

Author: David Osipov; personal@david-osipov.vision; [website](https://david-osipov.vision). Original skill material: MIT; see [licence and notices](THIRD_PARTY_NOTICES.md).
