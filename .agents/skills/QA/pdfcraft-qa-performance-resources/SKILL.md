---
name: pdfcraft-qa-performance-resources
description: Use when a PdfCraft edit, render, save, cache, worker, or dependency change may affect latency, memory, CPU, disk use, or cleanup.
---

# Performance and resource evidence

Use for credible cost/lifecycle question. Wording-only change without resource mechanism is non-trigger. Read [task contract](references/contract.md), [measurement guidance](references/performance-dependencies.md), relevant [transaction phases](references/transactions.md). Obtain workload, affected crates, existing budget authority, source/configuration/lock identities, allowed effects, resource owner, finite run count, deadline, output paths. Without product budget, compare measured baseline; leave budget qualification undecided.

Trace complete operation before microbenchmark choice. Engine `Session::apply` can serialize, inspect, refresh display appearances, replace `RenderPool`; save and close add separate costs. Copy-on-write graph snapshots do not measure these phases. Inspect `release_working`, retained undo states, `RenderPool::threads`/`retire`, completed-result storage. Queue byte cap excludes active pixels, permits one oversized completed item alone; not a process memory ceiling.

Predeclare metric start/stop events, sample order/statistic, fixture hashes, compiler/profile/features/target, renderer, font inputs, machine conditions, cold/warm cache definition. Run functional control first. Example: compare one synthetic document through open, first render, metadata edit, content edit, undo/redo, save/reopen, close; separately bound repeated edits/cancellation. Assert expected saved content and unknown sentinel object survive. Retain every sample, timeout, first failure, spread, missing value. Never improve memory by discarding required history/unknown data.

Measure aggregate owned process/worker lifetime, peak/settled memory, CPU, elapsed time, output storage. Distinguish cancellation requested, stale-result invalidation, worker exit, bytes released. High working set after frees alone cannot prove leak. Serialize competing performance runs; account for build/test workers too.

Use existing `Instant`, `RenderStats`, diagnostic `search_bench`, installed process counters where sufficient. Build timings measure compilation. Absent profiler needs no installation: retain source/work-count analysis; mark runtime measurements not run. Stop on breached experiment bounds/unreviewed effect; preserve partial evidence/observed cleanup. Report exact commands, planned/executed cases, raw samples, comparable baseline identity, functional results, budget source, performance-claim limits.

Author: David Osipov; personal@david-osipov.vision; [website](https://david-osipov.vision). Original skill material: MIT; see [licence and notices](THIRD_PARTY_NOTICES.md).
