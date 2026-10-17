---
name: pdfcraft-rust-implementation
description: Use when implementing or repairing PdfCraft Rust behavior that requires ownership, error, API, type, or module-boundary decisions.
---

# Rust implementation decisions

Use for authorized Rust implementation. Source-only explanation/unrelated documentation correction: non-trigger. Read [task contract](references/contract.md), [Rust workflow](references/rust-workflow.md), and [current architecture guidance](references/architecture.md). Obtain required behavior, affected callers/consumers, allowed changes, current policy, manifests/features/target, test environment. Missing private plans: use public issue/task, committed architecture/crate documentation, parity data, actual source; never invent internal requirements.

State invariant/failure contract before types. Name borrowed inputs, owned mutable state, shared immutable bytes, history snapshots, workers that can outlive caller. `Arc`/copy-on-write can preserve snapshots; no cheap serialization/teardown guarantee. Preserve unknown PDF objects; no rebuilding into closed typed schema. Add newtype, enum, trait or checked constructor only where it prevents demonstrated confusion, e.g. document identity, page index base, coordinate space or permission state. No generic hierarchy without caller need.

Trace actual edit seam. Example: `Edit::RotatePages` change reaches permission checks, `scope_of`, graph editing, history, refresh, saving, automation arguments, command/parity evidence. Specify independent page rotation/untouched-object expectations; include empty/invalid page controls reaching intended branch. Preserve deliberate first-field first-match behavior. Select smallest causal regression before repair; witness RED by reverting repair in authorized isolated checkout, then GREEN with unchanged oracle.

Use crate error types/`Result` for input failures. Check conversion/arithmetic before allocation/slicing; saturation permits no saturated-size allocation. Keep fallible preparation/commits visible. Follow existing no-unsafe/nonpanicking-input rules; `catch_unwind` cannot restore state or catch abort/OOM. Proportionally review expression/function/module/cross-project effects, including headless reachability, native/WASM differences.

After build-script preflight, run scoped installed Cargo/clippy/tests under existing job/deadline policy. Preserve doctests separately where needed; verify exact filters execute intended tests. Without Rust, finish reviewable design/patch; mark compilation, RED/GREEN, runtime claims not run. Never auto-install or weaken gate. Report invariant/ownership decisions, changed seams, actual checks/artifacts, first failures, feature/target coverage, remaining risks.

Use toolchain-compatible Clippy policy/supported feature matrix. No incidental newer-Cargo flags, diagnostic snapshot updates or manifest-writing QA helpers. Read relevant tool-specific section in [rust workflow](references/rust-workflow.md).

Author: David Osipov; personal@david-osipov.vision; [website](https://david-osipov.vision). Original skill material: MIT; see [licence and notices](THIRD_PARTY_NOTICES.md).
