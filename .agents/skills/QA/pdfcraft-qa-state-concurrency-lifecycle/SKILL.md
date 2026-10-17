---
name: pdfcraft-qa-state-concurrency-lifecycle
description: Use when PdfCraft state transitions, render generations, replacement, cancellation, close or worker retirement can interact with stale completion.
---

# State and competing work

Use for specific ordering/lifetime invariant. Pure stateless helper, visual comparison or throughput measurement alone needs no workflow. Sequential model helps histories; cannot establish native concurrent scheduling.

Read [task contract](references/contract.md), [transaction/lifecycle guidance](references/transactions.md) and [aggregate budgets](references/security-resources.md). Receive initial state, transition contract, observable identities, schedule hypothesis, owned resources and finite waits/workers. Inspect current Session, renderer queue/pool, consumers, generation/tag and cancellation tokens. Classify logical/derived/diagnostic state.

Specify event trace plus independent state/effect expectation after each transition. Order-dependent hypothesis: barriers/controlled completions, no arbitrary sleeps. Preserve normal-flow controls plus plausible wrong control, e.g. omitted invalidation. Do not make delayed production work immediate for easier tests. Observe application before fixture reset/harness drop.

Bounded render example: request A for synthetic document; edit or replace demand with generation B; complete B, release A. Check actual consumer accepts current identity; stale pixels/text cannot replace it. Close/reopen as new identity; settle retained completions, verify no publication into reopened document. First run one controlled trace; record native barriers/exact stale window. Manual queue callback qualifies local mechanism, not full UI/render journey.

Track separately: cancellation requested, validity token changed, result discarded/retried, worker exited, retained bytes released. `RenderPool::retire` retains handles: long drawing operator can outlive pool; Drop cannot prove exit. Completed-result storage excludes active pixels, allows one oversized result alone. Bound total active/replacement/retiring workers and retained state across repeated close/edit cycles; do not multiply per-pool allowances.

Apply/undo/redo late-refresh: establish promised failure contract, actual reached fault and complete history/state snapshots. Source ordering: lead, not reproduced defect. Allowed diagnostics may differ despite preserved logical content.

Stop schedule on cleanup/reset failure unless fresh isolation demonstrated. Missing Loom/Shuttle or native execution: explicit transition vectors/source tracing; no exhaustive-schedule or worker-exit claim. Report schedule, source/fixture identities, reached events, logical/diagnostic differences, current/stale results, first failures, cumulative budgets and cleanup outcomes.

Interpreter/sanitizer success covers only executed, instrumented perimeter. Preserve native schedule/cancellation/cleanup controls when auxiliary tool omits platform or FFI boundary. Read relevant tool-specific section in [security resources](references/security-resources.md).

Author: David Osipov; personal@david-osipov.vision; [website](https://david-osipov.vision). Original skill material: MIT; see [licence and notices](THIRD_PARTY_NOTICES.md).
