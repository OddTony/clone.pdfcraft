---
name: pdfcraft-vendored-dependency-maintenance
description: Use when updating a PdfCraft vendored crate, local dependency patch, feature graph, lockfile, advisory exception, or upstream patch provenance.
---

# Vendored dependency maintenance

Use for actual dependency/fork-maintenance decision. Application-only edit without dependency impact: non-trigger. Read [task contract](references/contract.md), [performance/dependency reference](references/performance-dependencies.md), and [Rust workflow](references/rust-workflow.md). Obtain affected package, upstream release/commit, current vendor/lock hashes, patch rationale, feature/target consumers, licences/assets, existing regression mapping, allowed changes, bounded validation environment. Upgrade request permits no broad dependency churn/product redesign.

Reconcile `Cargo.toml` patch routing, `Cargo.lock`, each vendored manifest/licence, `vendor/README.md`, local patch markers, upstream issue/PR/release evidence. Pinned baseline patches eight crates: hayro, hayro-interpret, hayro-syntax, hayro-jbig2, lopdf, winit, egui-wgpu, vello_common. Reinventory current source; list not assumed fixed. Existing patch documentation/tests counter unmanaged-vendoring claim; identify specific drift/missing coverage.

Map every retained/changed/dropped patch to behavior/test. Example: vello_common clip-row optimization has standalone visited-row regression plus render-pixel consumers. Upgrade must preserve expected geometry/pixels/bounded-work assertion; deliberately removing routed patch in isolated control must fail intended regression. `vendor` excluded from workspace: workspace tests do not automatically execute every vendored crate's tests. Static winit patch-presence guards cannot establish physical IME/DPI behavior on another OS.

Inspect effective feature/default-feature/target, build/proc-macro changes, transitive sources, native tools, notices, optional font/assets. Preserve no-unsafe, source/licence policy, opt-in MCP, headless behavior. Root workspace `rust-version` not inherited by every member; identify actual declarations, compile relevant MSRV promise separately. Stable CI success/`--all-features` cannot prove all supported configurations. `--locked` may fetch; offline mode permits local build-script effects.

Use existing deny policy/documented advisory exception reasons; record database freshness/code-path relevance. Missing optional local scanner: skipped, not passed. External copyleft oracles remain separate processes; never copy implementation, binaries or assets into dependency tree.

Unresolved source provenance, rights, build effects or required native test: conditional patch/proposal, named owner. Without Rust, source review/prepared commands reported unrun. Deliver patch ledger, upstream status, exact hashes/features/targets/MSRV coverage, reached regressions, first failures, unresolved exceptions, removal conditions once upstream has fix.

Published-base cargo-vet audit does not identify exact local path patches. Keep local diff/hash/regression evidence, scanner graph/database identities, trust-policy decisions separate. Read relevant tool-specific section in [performance dependencies](references/performance-dependencies.md).

Author: David Osipov; personal@david-osipov.vision; [website](https://david-osipov.vision). Original skill material: MIT; see [licence and notices](THIRD_PARTY_NOTICES.md).
