---
name: pdfcraft-bounded-parser-design
description: Use when designing or repairing PdfCraft parsing, decoding, graph traversal, rendering, or cancellation limits for untrusted document input.
---

# Bounded parser and renderer design

Use for input-sized allocation, recursive/cyclic walk, decompression, work-budget or cancellation mechanism. Fixed-size ordinary UI code: non-trigger. Read [task contract](references/contract.md), [security/resource boundaries](references/security-resources.md), and [generative guidance](references/generative.md). Obtain trusted/untrusted inputs, current limits/authority, expected tolerant/error behavior, budget owner, affected callers, worker model, finite hostile/healthy controls. Preserve current no-unsafe/nonpanicking rules; invent no product thresholds.

Before work, trace every input-derived length, offset, dimension, count, nesting edge, conversion. Check signed-to-unsigned conversion, overflow, slice/UTF-8 boundaries, zero division, finite geometry, independent allocation/work maximum. Checked multiplication then unlimited allocation still unbounded; no overflow does not make saturated maximum safe. Explicit error/partial/repair outcomes; no empty success hiding truncation.

Inspect actual layer API. `pdfcraft_filters::decode(chain, data, max_output)` caps every intermediate buffer; `decode_tolerant` may return partial data for corruption but retains limit/unsupported errors. Caller must propagate distinctions. Example: tiny two-filter synthetic stream should decode under declared limit; bounded expansion exceeding intermediate cap must reach intended limit error after valid setup. Add relevant zero/negative predictor dimensions, huge products, malformed UTF-8 positions, cyclic references; verify healthy retry works.

Name per-object/per-page/operation/aggregate process budgets separately. Nested forms/shared references charge intended owner; no allowance reset per recursive call. Seen-sets/depth limits need clear repeated-reference versus cycle semantics. Completed-result queue caps exclude active render pixels; worker replacements, multiple documents, test jobs, retained snapshots can multiply cost.

Cancellation requested, result invalidated, worker exited, memory released: separate events. Current `RenderPool` may retain worker in long operator after retirement; use handles/counters, hard observation deadline. `catch_unwind`: last-resort safety net, no process isolation or abort/OOM protection.

Start small deterministic boundary/property tests, source-independent malformed envelopes. Fuzz only when separately scoped with cases/time/jobs/output/cleanup bounds. Missing Rust/native workers: limit/ownership map, unrun controls; no installation/resilience claim. Stop at exhausted experiment bounds. Report reached branches, measured aggregate effects, partial/repair statuses, first failures, cleanup, residual hard-isolation limits.

Author: David Osipov; personal@david-osipov.vision; [website](https://david-osipov.vision). Original skill material: MIT; see [licence and notices](THIRD_PARTY_NOTICES.md).
