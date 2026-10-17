---
name: pdfcraft-qa-static-checks
description: Use when PdfCraft changes need compiler, clippy, formatting, dependency-layer or static gate evidence with explicit target and feature scope.
---

# Verify static contracts

Use for source structure, types, manifests, policy checks, build admission. Clean check cannot qualify edit/save behavior, native UI or browser execution. No scanner added merely for report.

Read current repository rules and [scope contract](references/contract.md). Obtain affected paths, requirement IDs, relevant dirty/source/lock/configuration identities, allowed writes, selected target/features, finite budget. Inspect actual Cargo aliases, manifests, build scripts, CI/xtask invocation before execution. Use [Rust workflow](references/rust-workflow.md) for commands/admission.

1. Map claim → enforcing formatter, compiler, clippy, asset/parity checker or dependency-layer rule. Inventory included production/test/doctest/generated/vendor inputs. Root MSRV and member inheritance distinct facts.
2. Preserve target's scoped local-build policy. After preflight, proposed engine recipe: `cargo clippy -p pdfcraft-engine -j 2 --all-targets --locked --offline -- -D warnings`, under outer deadline. Unrun recipe, not result. Build jobs/test threads: separate budgets; no habitual workspace sweep.
3. Dependency claims: read [architecture guidance](references/architecture.md) and actual `xtask/src/layers.rs` model. Challenge forbidden lower-layer UI edge plus legitimate allowed edge in authorized disposable specimen. Metadata/parser failure cannot prove intended violation detected. Current automation-to-COS/render edges checker-permitted.
4. Compiler guarantee: pair legitimate compiling caller with precise misuse; capture intended diagnostic, prove both specimens admitted. Missing import/unrelated error cannot qualify rejection.
5. Capture actual command, toolchain, target/features, included inputs, fresh diagnostics, final wrapper exit. Gate enforcement: establish declaration → invocation → failure propagation. `cargo xtask ci` stops at first failure; report later stages unrun.

Missing tools/cache or unreviewed build effects: stop affected execution. Fallback native source/manifests/Git review must stay labelled source-only. Missing optional dependency scanner: skipped; do not weaken required CI behavior. Preserve failed diagnostics; no out-of-scope auto-fix, warning suppression or baseline changes. Output checked contracts, observed diagnostics/controls, exact scope, unrun gates, next owner.

Feature/MSRV work: declared supported matrix; all-features one selection. Review cargo-hack manifest-writing options before claiming read-only command. Read relevant tool-specific section in [rust workflow](references/rust-workflow.md).

Author: David Osipov; personal@david-osipov.vision; [website](https://david-osipov.vision). Original skill material: MIT; see [licence and notices](THIRD_PARTY_NOTICES.md).
