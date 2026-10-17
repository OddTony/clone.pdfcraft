---
name: pdfcraft-qa-e2e-tests
description: Use when a PdfCraft claim requires a complete journey through the built CLI, opt-in native UI control or actual browser-WASM application.
---

# Verify a built journey

Use for entry-to-observable-output claim crossing application boundary. In-process engine/harness tests remain lower-layer evidence. Static WASM compilation cannot establish browser behavior.

Read current rules and [scope contract](references/contract.md). Identify journey, independent expectations, authorized executable/build artifact, source/dirty/config/feature identity, synthetic fixtures, allowed effects, finite resources. Use [Rust workflow](references/rust-workflow.md) for builds, [UI/platform guidance](references/ui-platforms.md) for native/browser claims, [PDF fidelity](references/pdf-fidelity.md) for saved-output expectations. Load only applicable branch.

1. Prefer existing approved executable; inspect build effects before creating one. Allocate private owned root, outputs, process-tree deadline. No default MCP/control listener, server, dependency download, external account action implied.
2. CLI journey: finite JSON step array through `pdfcraft-cli run --script steps.json --report --root <owned-root>`. Open valid synthetic PDF, edit intended page, render via `page_render`, save to owned path, reopen, compare independent page/title/preservation facts. Declare fresh session state; verify returned IDs match later script arguments. No implied result-substitution language.
3. Reconcile every script step/output/owning exit. Current report marks first failure `error`, later steps `skipped`; later steps did not execute. Pair success with reached rejection/failure control; verify no forbidden output/silent discard.
4. Native UI: explicitly launch with owned `--control` file; drive actual key/focus/click paths; inspect resulting document/pixels. Keep token private. Direct command dispatch cannot prove keyboard reachability; harness/screenshot cannot qualify native IME/screen-reader speech.
5. Browser-WASM: identify actual wasm/glue, browser/OS/renderer/scale/locale, local fixture. Observe open/edit/render, actual downloaded artifact, reopen. Same-origin host `postMessage` dispatch differs from host durability. Test cancel/blocked delivery only within scope; preserve incomplete outcome.

If required executable, GPU, browser, native environment unavailable, keep lower-layer evidence and exact unrun journey. Existing CLI tests/manual local observation substitute for optional automation integrations with narrower claims.

Close with [native evidence](references/evidence.md): artifact hashes, selected/executed steps, real observations, first failures/retries, exits, resources, cleanup, unrun platform obligations. Retain product failures; repair only proven harness faults within authorization, without skipped tests/automatic baseline acceptance.

Author: David Osipov; personal@david-osipov.vision; [website](https://david-osipov.vision). Original skill material: MIT; see [licence and notices](THIRD_PARTY_NOTICES.md).
