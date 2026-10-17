---
name: pdfcraft-qa-integration-tests
description: Use when a PdfCraft claim depends on real collaboration among engine, COS writer, renderer, automation or filesystem persistence seams.
---

# Exercise the real seam

Use when collaboration can fail despite passing local functions. Mocked writer cannot qualify saved bytes; in-process automation call cannot prove built CLI process/native UI.

Read current rules and [scope contract](references/contract.md). Obtain entry action, collaborators, independent expected facts, permitted files, source/dirty/config/fixture identities, resource owner, finite budget. Inspect actual caller-to-output flow; consult [Rust workflow](references/rust-workflow.md) for execution, [transactions](references/transactions.md) for edits/saves/rollback.

1. Draw minimum risk-exposing path; name substitutions. Engine output: `Session::open` → `Session::apply` → `save_bytes` → actual COS reopen/inspection. Persistence: add `check_save`, real owned publication and acknowledgement; in-memory buffer is no disk save.
2. Build small contributor-original synthetic PDF with distinct page labels/unknown dictionary sentinel. Rotate specified page; change literal title. Reopen emitted bytes through real consumer; assert expected rotation/title, untouched content, sentinel preservation. Check original incremental prefix only where operation promises incremental output; required full rewrites need own expectation.
3. Compare state at relevant failure phase. `doc_save` may run `FlattenFillSign` before byte preparation. Serialization, pre-publication validation, replacement, `mark_saved` are separate boundaries; successful replacement then acknowledgement failure does not leave unchanged destination. No broad rollback-guarantee extrapolation.
4. Exercise accepted flow and reachable collaborator failure; preserve primary and cleanup causes. Existing automation `edit_undo_redo_and_save_round_trip` exercises real edits, disk output, new session; inspect assertions before claiming added coverage.
5. Allocate owned temporary/output paths/build resources. Proposed and unrun: `cargo test -p pdfcraft-automation -j 2 --locked --offline --test automation edit_undo_redo_and_save_round_trip -- --exact --test-threads=2`, with reviewed effects/outer deadline. Await collaborators and cleanup.

Native Cargo tests and explicit expected values default; absent optional oracle narrows independent compatibility evidence. Missing Rust/cache leaves recipe unrun. Stop dependent execution on unreachable fault seam/unavailable isolation; do not substitute earlier error.

Report [evidence](references/evidence.md): real/substituted path, fixture/output hashes, actual executed IDs, first failures, target/features, exit, cleanup, remaining CLI/UI/platform obligations. Never count source-defined tests as executed.

Author: David Osipov; personal@david-osipov.vision; [website](https://david-osipov.vision). Original skill material: MIT; see [licence and notices](THIRD_PARTY_NOTICES.md).
