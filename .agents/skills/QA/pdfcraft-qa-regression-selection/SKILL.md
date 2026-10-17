---
name: pdfcraft-qa-regression-selection
description: Use when a PdfCraft change needs an economical affected regression slice and explicit accounting for transitive inputs and required wider gates.
---

# Select affected regressions

Choose/reconcile meaningful slice. Do not make every edit workspace build or all-skills campaign. Selection cannot waive mandatory commit/CI gates or qualify unexecuted platform.

Read current rules and [scope contract](references/contract.md). Inputs: base/current revisions, relevant dirty paths, behavior requirements, callers/consumers, changed features/vendor/generated inputs, existing tests, prior failures and budget. Consult [architecture](references/architecture.md) for transitive impact and [Rust workflow](references/rust-workflow.md) for Cargo selection.

1. Build change → affected contract → plausible break → selected test/environment rows. Follow consumers beyond filenames: engine edits can affect automation, renderer refresh, save/reopen and UI state. Shared manifest, font input, Cargo feature or vendored patch can expand scope beyond edited crate.
2. Choose fast direct checks plus real seams observing material consequences. For `save_bytes` change, inspect engine incremental/rebase and undo-after-save assertions, automation save/reopen, CLI output where serialization crosses process. Preserve signed/encrypted/full-rewrite cases when changed condition reaches them.
3. Include relevant feature/target rows. CLI/automation default MCP separate from no-default CLI compile; `--all-features` is not every combination. `cargo xtask wasm` compiles its classified packages; supplies no browser journey. Vendor workspace exclusion requires explicit regression/standalone-test reasoning.
4. Write exact package/test binary/full test names, target/features, fixtures, owner and deadline before execution. Proposed unrun engine slice: `cargo test -p pdfcraft-engine -j 2 --locked --offline tests::save_is_incremental_stamps_mod_date_and_rebases -- --exact --test-threads=2`. Verify selection; run under owned outer deadline. Discovery alone is not execution.
5. Reconcile planned/executed identities by package, test binary, full name, environment and attempt. Preserve first failures, ignored/filtered/skipped/cancelled/timeout states and distinct doctest runs. Do not sum overlapping runs. Explain every selected-but-unexecuted test.

Record each material exclusion: reason, claim limit and next disposition. Expensive required gate remains pending. Missing Cargo/cache or optional semantic tooling: source/Git impact review plus explicitly unrun command plan. Search/index misses cannot prove absence.

Output slice's actual outcome, evidence freshness, affected-contract coverage, exclusions, wider required gates, cleanup and next owner. Rerun only checks invalidated by new relevant inputs; retain earlier evidence as historical attempts.

Coverage can guide missing-case selection only after reconciling source/profile lineage and actual test execution. Preserve runner isolation differences and affected supported feature rows when reducing selection. Read relevant tool-specific section in [rust workflow](references/rust-workflow.md).

Author: David Osipov; personal@david-osipov.vision; [website](https://david-osipov.vision). Original skill material: MIT; see [licence and notices](THIRD_PARTY_NOTICES.md).
