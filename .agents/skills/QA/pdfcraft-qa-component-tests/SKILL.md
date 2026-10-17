---
name: pdfcraft-qa-component-tests
description: Use when an egui widget, dialog or PdfCraft shell interaction needs local state and AccessKit semantics checked with the existing Rust harness.
---

# Test shell components

Use for local UI contract: control availability, label/role/state, dialog response, focus-driven action. Whole native journey, real picker, OS IME, screen-reader announcement, browser download requires actual environment.

Read current policy and [scope contract](references/contract.md). Obtain component/interaction, independent expectation, synthetic fixture, relevant source/callers, allowed files, locale/size/scale, finite budget. Inspect `crates/ui-egui/tests/control.rs`, existing dialog tests, locked harness features. Read [Rust workflow](references/rust-workflow.md) for execution; [UI/platform guidance](references/ui-platforms.md) for native/accessibility claim.

1. Choose smallest real shell path. Current harness builds `PdfCraftApp` through `egui_kittest::Harness::builder().build_eframe`; use actual locked API/existing helpers, not old eframe recipe.
2. Fix fixture identity, interface language, logical size, pixels-per-point. Async preparation: wait for state predicate with hard deadline/last-state diagnostics; sleep then assertion cannot prove readiness.
3. Bounded example: Read/Edit mode control. Inspect uniquely named button; send Tab/Shift+Tab and Enter through existing `ControlClient`; assert intended focused node/mode, unchanged document/history. Direct `ui.command` dispatch alone cannot prove keyboard reachability. Keep unavailable/disabled operation as distinct rejection control where requirement applies.
4. Assert semantic effect and forbidden mutation, not merely click/method call. Check selected dialog cancel/validation, focus restoration. Observe owned application work before generic teardown.
5. Execute only relevant test target after preflight. Proposed and unrun: `cargo test -p pdfcraft-ui-egui -j 2 --locked --offline --test control`, under owned outer deadline. Separately record filtered/ignored tests, font prerequisites, actual GPU/snapshot execution.

egui `with_os` simulates reported OS data; no OS backend exercise. AccessKit queries/screenshots cannot prove delivered native assistive-technology speech. Missing GPU, fonts, Rust block only dependent claims; source review/existing harness design are native fallback.

Close with [evidence](references/evidence.md): runner/toolchain, target/features, fixture, scale/locale, actual events/observations, first failures, cleanup, unrun native/browser obligations. Do not weaken assertion or accept new snapshot to conceal product mismatch.

Author: David Osipov; personal@david-osipov.vision; [website](https://david-osipov.vision). Original skill material: MIT; see [licence and notices](THIRD_PARTY_NOTICES.md).
