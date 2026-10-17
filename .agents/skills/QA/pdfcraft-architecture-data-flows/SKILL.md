---
name: pdfcraft-architecture-data-flows
description: Use when locating PdfCraft callers, tracing document or cache state, assessing change impact, or resolving current versus planned architecture.
---

# Architecture and data-flow analysis

Use for cross-module responsibility/state-flow/dependency question. Bounded local assertion with known callers: non-trigger. Read [task contract](references/contract.md), [architecture reference](references/architecture.md), and [evidence rules](references/evidence.md). Record task, target revision/dirty state, relevant manifests/features/cfg, entrypoints, expected consumers, allowed effects, bounded search scope. Read public governance/current code; internal plans/historical documentation counts are not current implementation evidence.

Start user/CLI/MCP entrypoint; trace applicable argument validation, path authority, session lookup, permissions, graph mutation, history, serialization, derived state, persistence, observable response. Per edge: owning symbol/path, directly read/tool-reported/inferred. Per state: writer, retainer, generation/cache key, invalidation point, failure return, release boundary. Before claiming missing edge, check cfg, traits, macros, generated includes, relevant tests.

Example: trace `doc_set_info` → `Edit::SetInfo`, `scope_of`'s metadata scope, `organize::set_info`, XMP synchronization, working bytes, signature refresh, document-properties consumers. Compare content edit replacing renderer state. Derive affected regressions from differences; shared file alone cannot justify every suite. Challenge plausible wrong analysis equating display-only appearances with saved bytes or assuming metadata refresh rebuilds every renderer.

Compare documented layers with `xtask/src/layers.rs` and actual manifests. Checker enforces dependency direction/selected exceptions, UI exclusion below L7; cannot prove stronger prose claim every frontend uses only façade. Current automation lower-layer dependencies: documentation/encapsulation decision evidence, no automatic refactor permission or reproduced defect. Separate existing/reserved future modules.

Prefer current Git/ripgrep source reads. Approved semantic index/rust-analyzer may shortlist relationships; record source root, revision, dirty identity, features/target, freshness. Stale hit: confirm current source, use direct callers/tests; no automatic index refresh/watcher. Without semantic tooling, mark unresolved dispatch edges; continue native text/Git analysis.

Deliver concise edge/state map, exact sources, observed/inferred distinction, plausible counterexample, touched consumers, selected checks, unresolved decisions/owner. Source-order concerns stay test hypotheses until valid runtime witness reaches suspected seam.

Author: David Osipov; personal@david-osipov.vision; [website](https://david-osipov.vision). Original skill material: MIT; see [licence and notices](THIRD_PARTY_NOTICES.md).
