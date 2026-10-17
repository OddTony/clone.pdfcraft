# Evidence identity and honest qualification

Use for selection, capture, triage, review, resume and final qualification. Native/manual workflow; automatic native-report binding remains separately unqualified.

## Before execution

Freeze independent requirement/selection plan. Record target commit and relevant dirty-diff digest; actual input/config/lockfile/fixture/generated/built hashes; runner/compiler version; Cargo package/features/target; exact argv and working directory; allowed effects; owner and cumulative attempt/time/resource limits. HEAD alone cannot identify dirty or rebuilt artifacts. Enumerate intended semantic test identities before viewing completed output. Disambiguate duplicate titles/attempts by file, harness/project, full name and attempt; unknown dimensions stay unknown.

Tests-only: do not repair product code. Inspect commands/build scripts before launch. Prefer existing scoped Cargo tests under current repository job/deadline rules. With nextest, preserve separate doctests. Discovered list or zero-test filtered run is not successful execution.

## During and after execution

Keep native stdout/stderr and structured output unchanged, hashed. Capture process exit, signal, timeout/cancellation, start/end, actual test IDs and all outcomes. Preserve original failed attempt before retries, including reporter/collection/cleanup failures. Wrapper must prove child-failure propagation; package supplies no such capture wrapper. Outer deadlines/descendant cleanup need identified platform owner.

Reconcile selected/executed identities and native totals; matching counts cannot establish matching tests. Unexecuted required tests, ignored tests, missing doctests, unsupported target, incomplete reporter output or unknown native dimensions leave claim incomplete. Behavioral RED needs intended assertion, reached operation and independent causal review; compiler/setup failures cannot substitute. Truthful RED is acceptable evidence with incomplete product disposition.

Observe cleanup before generic harness teardown. Record owned processes, workers, temporary files, caches and outstanding effects. Resolve uncertain publication from authoritative state before retrying effect. Checkpoint retains consumed budgets, first failures, current owners and exact next action; reload policy and invalidate changed inputs on resume.

## Optional record checker

Proposed future Rust command: `cargo xtask skills evidence <absolute-owned-root> <relative-plan.json> <relative-runs.json>`. Evidence subcommand, implementation/source candidate and record templates absent from package. Format: design guidance, no runnable checker. Proposed scope: smaller Rust-specific record format + declared artifact bytes; no parsing/authenticating native libtest output, child supervision, reporter certification, attesting agent file reading or product qualification.

Plan: schema_version=pdfcraft-evidence-1; source={commit,dirty_sha256,inputs:[{path,sha256}]}; nonempty checks={id,kind,required,command,cwd,runner,runner_version,selected_ids}; resources; budget={milliseconds,attempts}. Kinds: tests/static/manual; tests require nonempty independently selected ID list. cwd: actual absolute working-directory identity, must match result.

Result: identical source, checks={id,status,command,cwd,runner,runner_version,executed_ids,exit_code,signal,timed_out,cleanup,first_failures,retries,raw_artifacts:[{path,sha256}],unsupported_dimensions}, resources=[{id,state}], spent budget. Use pass/fail/blocked/not-run/skipped/cancelled/timeout/not-applicable-with-rationale honestly. Required checks non-pass, required checks/resources missing, unresolved failures or unexplained retries: qualification incomplete. Retain resolved historical failures with source revision/attempt + documented disposition, including any causal repair; require fresh required evidence for final candidate. Unknown mandatory typed fields: manual record + reason; future strict validator must reject unknown values. Proposed templates: optional authoring aids.

Even fully consistent record returns product_qualified=false. Independent input anchors, before/after execution-input verification, native semantic identity/attempt/status reconciliation, reporter completion/error ledgers and process supervision remain manual or deferred automation. Files/JSON do not prove implementation. Before future automatic gate, validate empty execution, wrong selection, stale inputs, altered/missing logs, hidden first failure, timeout/signal and incomplete cleanup with separate controls.
