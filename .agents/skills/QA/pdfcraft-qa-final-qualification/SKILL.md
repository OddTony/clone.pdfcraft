---
name: pdfcraft-qa-final-qualification
description: Use when a PdfCraft verification scope is ready for final reconciliation of fresh evidence, required gates, unresolved findings and exact qualification limits.
---

# Reconcile qualification

Use at declared handoff/readiness boundary. Passed planning phase, authored test, source review cannot become product qualification. Creates no waiver, commit, release, publication authority.

Read current repository rules, acceptance criteria, [scope contract](references/contract.md). Freeze latest relevant source, dirty diff, Cargo/configuration, fixture, built-artifact identities. Receive independent required-check plan, all native reports, first failures/retries, review findings, resources, remaining decisions.

1. Build row per applicable criterion: authority, independent expectation, selected test/check, required environment, actual execution, outcome, gap. Separate engine, automation, CLI, UI, browser claims.
2. Reconcile planned/executed IDs under [evidence requirements](references/evidence.md). Keep ignored/filtered/skipped/cancelled/missing/timeout states visible; no double-counting overlapping runs/retries. Zero exit without required tests is incomplete. Record doctests separately where runner omits them.
3. Trace each gate: policy, invocation, included inputs, failure propagation, final result. `cargo xtask ci` stops at first failure; later stages unrun. Hosted CI/local xtask have different command sets. Optional `cargo xtask deny` absence can produce successful skip, no scan pass.
4. Check claim limits. WASM compile cannot establish browser behavior; control-channel reply cannot establish physical keyboard/IME or assistive technology; serialized bytes cannot establish completed native replacement/browser download. Require evidence for actual promised boundary.
5. Invalidate evidence affected by later edits; retain unresolved first failures. Completed tests-only regression may be `verified-red` while product disposition incomplete. Do not accept baselines, lower thresholds, silently exclude expensive required work.
6. Use [tooling fallbacks](references/tooling.md) for missing optional tools; record lost claim. If required execution unavailable, preserve source review/exact next command/owner; record-consistency checker cannot supply missing behavior.

Issue `qualified-in-declared-scope` only when every applicable criterion and required gate has sufficient fresh evidence. Otherwise `incomplete`, or `not-assessed` if no qualification attempted. Include scope, contradictions, raw artifact identities, remaining gates, cleanup, next actions. Durable coordinated work also retains checkpoint/consumed budgets.

Author: David Osipov; personal@david-osipov.vision; [website](https://david-osipov.vision). Original skill material: MIT; see [licence and notices](THIRD_PARTY_NOTICES.md).
