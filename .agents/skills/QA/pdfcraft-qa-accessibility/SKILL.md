---
name: pdfcraft-qa-accessibility
description: Use when a PdfCraft control, keyboard or focus path, assistive-technology interaction, or PDF accessibility behavior changes or needs qualification.
---

# Accessibility evidence at the right boundary

Use for affected interaction/document accessibility requirement; pixel-only comparison without accessibility claim is non-trigger. Read [task contract](references/contract.md), [UI/platform evidence map](references/ui-platforms.md), [PDF fidelity reference](references/pdf-fidelity.md). Obtain controls/document features, required states/users, exact requirement, supported platform, input method, fixture rights, allowed work, bounded cases. Separate application/PDF accessibility objectives.

Specify independent expectations where relevant: role, name, value/state, keyboard reachability, focus order, activation, modal entry/containment/cancel/return, visible focus, error announcements. Start keyboard traversal from reachable state; direct `ui.focus` useful setup, no proof of Tab reachability. Enabled uniquely named control must cause one intended action. Missing name, disabled target, duplicate match, ignored activation must fail corresponding check without unrelated document mutation.

Use current egui 0.36.2/kittest idiom in `crates/ui-egui/tests/control.rs` for shell semantics. Follow `Harness`/AccessKit queries with actual state assertions and bounded completion; exhausted frame/step budgets mean failure, not settled UI. Example: open synthetic dirty document, traverse dialog with Tab/Enter/Escape, verify focus restoration and preserved history on cancel. Record semantic snapshots separately from screenshots.

Native reader claims require actual OS accessibility adapter and identified reader/operator: Narrator on Windows, VoiceOver on macOS, available Orca/AT-SPI session on relevant Unix desktop. Observe spoken role/name/state, timely error delivery. Complete AccessKit tree, screenshot, `with_os` simulation, injected key cannot prove native speech, physical IME, another OS backend. Browser-WASM needs actual built app/browser/AT run; native accesskit and web reader features differ.

For documents, use independent expected facts to inspect tags, `/Lang`, reading order, descriptions, forms, save/reopen. `tests/accessibility.rs` primarily exercises PDF checker UI; preserve Passed/Failed/Skipped/Manual distinctions. Identical pixels cannot prove usable reading order; machine PDF/UA checks leave human judgments.

If native reader, GUI, validator, matching build absent, continue source/kittest/document planning; mark dependent claim unavailable. No default install/upload. Retain exact platform/features/versions, reached states, actions, outputs, wrong controls, cleanup, unresolved manual checks; never infer full conformance from focused pass.

Author: David Osipov; personal@david-osipov.vision; [website](https://david-osipov.vision). Original skill material: MIT; see [licence and notices](THIRD_PARTY_NOTICES.md).
