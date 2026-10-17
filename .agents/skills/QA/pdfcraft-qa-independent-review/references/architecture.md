# PdfCraft architecture and ownership

Current repository policy, manifests, callers and tests govern. Checked baseline f0c9901c398372782a4eb001c95638289a5edab0 identifies observation, not permanent implementation contract. If local-only plans/internal background repositories are absent, use issue/task, AGENTS.md, CLAUDE.md, committed architecture/gaps/parity documentation, crate READMEs and source. Invent no contents or task IDs.

## Current shape and what the checker proves

Rust workspace: edition 2024, workspace version 0.8.0, root Rust 1.95 declaration. Read each member's effective MSRV separately. Binaries: desktop PdfCraft, CLI, browser-WASM. xtask owns build/contributor tooling. Inventory manifests; historical crate/test/tool counts may drift.

Editable document: object graph in `pdfcraft-cos`. Typed views must retain unknown reachable data. Foundation/object crates precede model/services/features, engine session façade, then platform/UI/automation consumers. Rendering uses vendored hayro; inspection also uses lopdf. Future DisplayList/model architecture is planned, not completed replacement.

`xtask/src/layers.rs` models classified layers, standalone exceptions, admitted same-layer edges, dev-only testkits and exempt binaries/tooling. `cargo xtask layers` checks these rules, including no UI toolkit below L7; does not prove every semantic ownership boundary or frontend-only-engine dependency. Current automation directly uses lower crates. Record mismatch, consumers and owner decision; analysis alone authorizes no checker strengthening or infrastructure refactor.

## A trace that supports a decision

Per edge retain entrypoint, callee, source location, feature/target condition, argument/data representation, authority check, mutation/external effect, return/failure and consumer. Separate source-read edges, tool-reported candidates and unresolved inference. Text hits prove occurrences. Absence needs macros, generated includes, cfg variants, trait dispatch, ignored paths and tests considered. Before shortlisting current relationships, index must identify root, revision, dirty inputs, features/target and freshness.

Editing trace usually crosses:

1. UI command or automation/CLI validates arguments and selects existing document. Automation read/write authority covers every direct/nested path.
2. Engine permissions and signed-document restrictions precede authorized `Edit`. User-facing feature also needs headless API/tool and honest parity evidence under current policy.
3. Candidate COS graph changes; history, passwords/keys, dirty/generation and view refresh have distinct order/failure points.
4. Serialization produces working bytes. Renderer/display appearances, field/comment/link/index lists and UI caches consume distinct related state.
5. Save preparation, validation, filesystem publication and saved-session acknowledgment are separate boundaries. Browser dispatch has different observations.

Metadata example: `doc_set_info` creates `Edit::SetInfo`; `scope_of` selects metadata refresh; organize updates Info and conditionally existing XMP. This updates document information/working bytes and signature results, retaining renderer. Content edits use another refresh path. Confirm consumers before regression selection; same changed file is no impact model.

## Ownership and derived state

Session editor retains COS graph, saved base, undo/redo snapshots, keys and display-appearance cache. Original bytes, parse caches and unchanged graph data may share storage. New working/display serialization, active renderer work and saved/history references can still dominate memory. `release_working` releases old view ownership before rebuilding: source counterevidence to retaining every previous buffer, not measured performance guarantee.

Per changed state identify sole writer/synchronization owner, sharing/lifetime, cache key, invalidation trigger, generation, rollback source and release point. Trace engine `scope_of`/`content_refresh_lists` separately from UI invalidation. Cache omitting changed property needs discriminating stale-result test, not another clear call. API documentation must distinguish logical values, working bytes, display-only appearances and disk bytes.

Choose abstractions from actual callers. Borrow read-only inputs within lifetime; own data surviving caller; share immutable ownership only when needed. Invariant-bearing types can protect index bases, coordinates, identity or permissions. Wrapping every string or using `Arc<Mutex<_>>` everywhere is no rule. Keep errors and commits visible. Never replace unknown PDF dictionaries with lossy application schema.

## Verification and limits

Use narrow functional controls across changed seam, scoped compiler/tests, existing layers/parity checks and actual consumers. Layer pass cannot prove rollback, cache coherence, headless completeness or native/WASM parity. Reached late-refresh faults: snapshot graph/history/keys/bytes/generation and next-action recovery. Source ordering stays hypothesis until valid-precondition reproduction; early negative return is no causal witness.

Missing compiler/semantic tooling leaves execution/unresolved dispatch explicit. Continue current-source/Git analysis without installs/index starts. Deliver edge/state map, independent expected effects, contrary evidence, selected checks, relevant hashes, observed results and owner decisions.

Source anchors: [target architecture](https://github.com/storytold/pdfcraft/blob/f0c9901c398372782a4eb001c95638289a5edab0/docs/architecture.md), [layer checker](https://github.com/storytold/pdfcraft/blob/f0c9901c398372782a4eb001c95638289a5edab0/xtask/src/layers.rs), [engine source](https://github.com/storytold/pdfcraft/blob/f0c9901c398372782a4eb001c95638289a5edab0/crates/engine/src/lib.rs), and [automation source](https://github.com/storytold/pdfcraft/blob/f0c9901c398372782a4eb001c95638289a5edab0/crates/automation/src/lib.rs).
