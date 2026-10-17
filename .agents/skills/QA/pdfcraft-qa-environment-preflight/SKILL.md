---
name: pdfcraft-qa-environment-preflight
description: Use when a PdfCraft test command, build configuration, tool availability or resource ownership is uncertain before execution.
---

# Establish execution prerequisites

Use before unfamiliar command/changed environment. Reuse relevant fresh observations for routine checks; understood source read needs no broad machine audit. Establishes prerequisites, not behavioral success.

Read current root/applicable nested instructions and [scope contract](references/contract.md). Receive claim, checkout, proposed commands, allowed writes/effects, resource budget. Preserve private environment values; record only names/non-secret facts needed for reproduction.

1. Confirm checkout, revision, relevant dirty changes, concurrent owners. Read Cargo manifests, lockfile, `.cargo/config.toml`, build scripts, relevant CI/xtask expansion. `xtask` alias runs Rust package; nested Cargo commands have own effects/options.
2. Inspect selected target/features, actual compiler/runner/dependency availability. Root `workspace.package.rust-version` does not automatically become every member's MSRV. `--locked` can still fetch; `--offline` blocks Cargo network access but does not sandbox build scripts/procedural macros.
3. Review relevant transitive executable setup before build. Concrete PdfCraft inputs: local `CRAFT_FONTS_DIR`, font requirements, generated contributor credits, Windows resource compilation. Missing optional fonts/external oracles limit corresponding claims even if test returns successfully.
4. Allocate owned writable checkout/output directory, `CARGO_TARGET_DIR`, build-job/test-thread/outer deadline limits. Inspect process-tree termination on actual host; Windows `timeout.exe` is no child-command deadline wrapper.
5. Map environment to claim. Native engine test cannot qualify WASM runtime; egui harness cannot qualify native IME; `cargo xtask wasm` is compile check. Confirm each selected row's prerequisites separately.
6. Record verified/assumed/unavailable/blocked prerequisites with [evidence identities](references/evidence.md). Use [native tool fallbacks](references/tooling.md) when optional tools absent; local source/Git/ripgrep remain useful. Do not start indexer, watcher, server, installer as availability probe.

Stop only commands missing required isolation/capability/authority. No global configuration change or missing-component fetch to green table. Output exact finite command proposal, reviewed effects, resource owner, prerequisite evidence, next action. Successful preflight does not mean tests compiled/executed/passed.

Author: David Osipov; personal@david-osipov.vision; [website](https://david-osipov.vision). Original skill material: MIT; see [licence and notices](THIRD_PARTY_NOTICES.md).
