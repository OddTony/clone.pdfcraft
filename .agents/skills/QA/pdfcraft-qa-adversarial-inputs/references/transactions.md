# Transaction, publication and lifecycle observations

Use current [task contract](contract.md) and actual operation guarantee. Returned error alone identifies no rollback boundary. Anchors describe reviewed revision; reread changed local implementation.

## State and phase snapshots

Before discriminating tests, state permitted differences; independently capture relevant reachable COS values/references/unknown sentinels, saved/working/display-only bytes, reopening/encryption keys, undo/redo availability/labels, dirty/path/name/generation, derived page/form/comment lists, cache identity, destination bytes and directory entries. Compare semantics where serialization/object numbering may differ legitimately. Keep diagnostics/notices/warnings/error logs separate: failure reporting may change diagnostics while preserving content. If public seam cannot observe a fact, leave it unknown. Identify test-only private-state witnesses explicitly.

[engine edit/history source](https://github.com/storytold/pdfcraft/blob/f0c9901c398372782a4eb001c95638289a5edab0/crates/engine/src/lib.rs#L3600) clones candidate graph, checks permissions/edit, then commits and refreshes. Early `failed_edit_changes_nothing` regression counters claims that failed edits are generally untested; does not exercise late refresh. Reviewed apply mutates history before fallible refresh; undo/redo move graph/history before refresh. Reachable late-fault redo loss/undo eviction remains source hypothesis, not reproduced defect. Establish intended observable contract, healthy control and actual late fault first. Documentation/testing-only assignment authorizes no production failure hook.

## Save is several operations

[automation save path](https://github.com/storytold/pdfcraft/blob/f0c9901c398372782a4eb001c95638289a5edab0/crates/automation/src/lib.rs#L906): resolve write authority, optionally apply `FlattenFillSign`, serialize, check prepared bytes, write, call `mark_saved`. Snapshot before/after compound edit. Later serialization/write failure can retain that undoable edit. Test post-edit boundary unless independently established whole-call rollback requires more.

| Reached phase | Required distinction |
| --- | --- |
| Preparation or `check_save` fails | Writer unreached; destination old/absent. Compare session against appropriate post-edit snapshot; inspect intended error. |
| Staging create/write/sync or rename fails | Existing destination intact; observe owned staging cleanup/failure. Preserve retryable work per phase contract. |
| Rename succeeds, `mark_saved` or refresh fails | Complete new destination exists. Record disk/session divergence, saved graph/keys/path/dirty/generation and recovery. Do not overwrite published output to force rollback assertion. |
| Web Blob/anchor or host `postMessage` dispatch completes | Dispatch observed; host acceptance/storage/durability require acknowledgment/evidence. Native rename guarantees do not transfer. |

`mark_saved` may set saved state/path, clear dirty and advance generation before fallible refresh. Every failed save stays dirty is too broad. New-path Save As defaults to full rewrite; require original-byte prefix only for named incremental-eligible operation. Signed-document, redaction, sanitization and encryption rules may change allowed operation/output.

[platform staging implementation](https://github.com/storytold/pdfcraft/blob/f0c9901c398372782a4eb001c95638289a5edab0/crates/platform/src/staging.rs): unpredictable same-directory names, `create_new`, write, checked sync, close, rename. Sixteen occupied names refuse save. macOS unsupported full-sync falls back to checked `fsync`; other errors remain errors. No parent-directory/power-loss durability promise. Native OS/open-handle/filesystem tests differ from callback injection.

## Reachability, recovery and stale work

Per fault, first complete valid no-fault operation. Prove intended writer/sync/refresh/worker reached; invalid document ID or malformed envelope may reject earlier. Preserve first failure and primary/cleanup causes. Observe state/staging artifacts, restore owned dependency, then perform one bounded permitted recovery. Successful retry erases no failure. Resolve authoritative published state before retrying uncertain consequential operation.

For render/edit/close schedules trace document/request identity, generation/tag and validity token. Coordinate `start A → edit/replace B → finish B → finish A → observe → close/reopen` with barriers/controlled completion. Stale work must satisfy actual publication contract. Observe application invalidation/cleanup before harness reset. [render retirement API](https://github.com/storytold/pdfcraft/blob/f0c9901c398372782a4eb001c95638289a5edab0/crates/render/src/raster.rs#L932) allows long drawing operator to outlive pool. Cancellation requested, result invalidated, worker exited and memory released are separate observations. Dropped handle is no join. Bound cumulative workers, waits and retained bytes; finite schedules are not exhaustive concurrency proof.
