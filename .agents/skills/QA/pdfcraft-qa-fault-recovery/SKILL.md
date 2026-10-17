---
name: pdfcraft-qa-fault-recovery
description: Use when a valid PdfCraft operation can lose storage, refresh, worker or collaborator support and its partial state and recovery need verification.
---

# Reached faults and safe recovery

Use for dependency failure after valid path begins, including partial completion/uncertain publication. Invalid arguments alone belong to rejection tests. Skill grants no authorization for production fault hooks, restarting unowned processes, retrying uncertain external effect.

Read [task contract](references/contract.md), [transaction phase table](references/transactions.md), [resource/authority limits](references/security-resources.md). Receive healthy operation, intended fault window, permitted effects, expected preservation/recovery contract, injection seam, resource owner, cumulative attempt/time budget. Reread actual source/existing tests; identify successful no-fault control and observed fault reachability.

Capture phase-appropriate logical state, saved/working/display bytes, history/keys/dirty/generation, destination, directory listing. Separate diagnostics. Inject deterministic failure through real path without bypassing tested behavior. Retain primary/cleanup failures. Callback injection tests callback seam; native denied-write/open-handle/disk conditions need separate OS evidence.

Example: extend existing automation sync-failure pattern on valid edited two-page synthetic document. Demonstrate complete new bytes in staging while destination still old; refuse sync; verify old destination, retryable session state, owned staging cleanup. Restore dependency; perform one successful save/reopen. Include new-path/existing-path variants only within declared budget. Unused injection/nonexistent document ID cannot qualify storage recovery.

When `flatten_fill_sign=true`, `doc_save` can apply undoable edit before serialization. Record pre-call/post-edit snapshots; later pre-publication error need not restore pre-call graph/history without explicit whole-call guarantee. After successful rename, later `mark_saved`/refresh failure leaves complete new disk file and potentially changed saved/path/dirty/generation state. Inspect authoritative disk/session divergence, documented recovery outcome; do not overwrite published data to force generic rollback. Web download/host-message dispatch needs separate acknowledgment contract.

Observe failure state before cleanup; restore only owned fault switches; verify next authorized operation. Green retry does not erase first failed assertion. Unknown publication/delivery: resolve authoritative state before repeating effect. Claimed cancellation also needs actual worker exit/release evidence.

No permitted late-fault seam: deliver precise source-backed test proposal; mark runtime recovery unverified. Missing Rust/optional fault tooling: source review/native design, no installation. Report reached phase, controls, state/artifact differences, error, retry outcome, first failures, spent budget, cleanup limits.

Author: David Osipov; personal@david-osipov.vision; [website](https://david-osipov.vision). Original skill material: MIT; see [licence and notices](THIRD_PARTY_NOTICES.md).
