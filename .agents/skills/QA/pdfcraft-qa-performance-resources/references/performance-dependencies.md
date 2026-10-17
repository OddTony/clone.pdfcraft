# Performance, builds, and dependency maintenance

Use existing gates and bounded experiments. No mandatory percentage improvement, memory ceiling, benchmark framework, scanner or CI lane follows. Baseline f0c9901c398372782a4eb001c95638289a5edab0 has no established/enforced engine performance budget. Without owner-approved threshold report measured comparable baselines; missing measurements stay unknown.

## Whole-operation measurement

Trace open, first visible result, graph edit, serialization, inspection/derived lists, display appearances, renderer replacement, save/mark-saved/reopen and close. COW clone cost covers only part. Engine `release_working` releases old working/display/renderer ownership before rebuilding: inspect this counterevidence before alleging repeated full retention. Undo count cap is no byte budget.

Before sampling define workload/fixture expectation: independently scaled object/page/content/image sizes, content/unknown sentinels, build/source/lock hashes, compiler/profile/features/target, font/backend/scale, cold/warm state, background load, sample count/order/statistic, time/CPU/RAM/output caps and owner. Alternate comparable baseline/proposal order where useful. Retain every raw sample, outlier, first error, timeout and missing value; report size/dispersion. Missing is not zero. Microbenchmark is not first-frame latency; lab run is not field population evidence.

Healthy functional control before cost comparison. Small sequence: open/render, metadata/content edits, undo/redo, save/reopen, close. Separately bound repeated edits/open-close cycles and one cancellation/slow-decoder case. Verify saved meaning, required history, unknown objects and stale-result rejection; deleting data/history for memory savings violates contract. Deterministic visited-work counters expose duplicate work despite unchanged output; real elapsed/CPU/memory establish impact.

Measure owned process-tree cost/retention, not nominal buffer cap. RenderPool workers/replacements may outlive retirement during long operators. Distinguish cancellation requested, result invalidated, thread exited, bytes released using handles/counters. Completed-result queue: 32 MiB/32-item source limits, one oversized result alone, active pixels excluded. Documents, render pools, compiler/test/fuzz workers, snapshots, caches and instrumentation multiply demand. Serialize competing measurements; observe cleanup before generic teardown hides retention. High RSS/working set alone proves neither allocator-live retention nor leak; sampling may miss peaks.

Start with existing `Instant`, `RenderStats`, owned process counters and `search_bench`. Search example measures search, not edit lifecycle; compilation `--timings` measures builds. Extra harness/profiler/dev dependency needs concrete diagnostic gap and normal review. No telemetry/provider upload implied.

## Conditional browser resource measurements

Affected browser-WASM startup/fetch: separate transferred, encoded-body, decoded-body bytes from load/paint/interaction metrics. Identify each observed frame/worker timeline, collector start, capacity and overflow handling. Parent-only/late snapshots cannot prove complete traffic; buffer-full event alone does not prove loss. Review cross-origin restrictions before interpreting zero sizes. Record HTTP, service-worker and application-cache states separately. Existing Playwright routing disables HTTP cache, so repeated navigation cannot establish ordinary warm-cache behavior. Preserve egress limits; report altered conditions. Lab samples cannot establish field percentiles; missing frames/workers/records leave dependent claims incomplete. No new network permission or required browser harness. Sources: [Resource Timing, September2026 Candidate Recommendation Draft](https://www.w3.org/TR/2026/CRD-resource-timing-20260901/) and [Playwright1.63 routing documentation](https://github.com/microsoft/playwright/blob/v1.63.0/docs/src/api/class-browsercontext.md).

## Reproducible build evidence

Before execution inspect Cargo config/aliases, relevant `build.rs`, procedural macros, transitive build dependencies/native tools, output ownership, optional craft-fonts inputs and network/cache needs. `--locked` preserves resolution but may fetch; `--offline` prevents fetching, not local build-script effects. Isolate writable target/cache/artifact resources; follow job/deadline policy. No habitual workspace sweep.

Record effective package/default features, target/cfg, native tools, lockfile, git dependency revision, optional inputs and artifact hashes. Root `workspace.package.rust-version="1.95"` is not every member's declaration: baseline engine does not inherit; ui-egui declares 1.95. Stable CI pass is no MSRV result. Compile actual promised member/feature/target matrix on claimed toolchain. `--all-features` covers neither every combination nor browser execution.

Prepared examples; authoring pack did not execute:

```text
cargo metadata --format-version 1 --no-deps --locked --offline
cargo tree -p pdfcraft-cli -e features --locked --offline
cargo tree --duplicates --locked --offline
cargo build -p pdfcraft-cli -j 2 --release --locked --offline --timings
cargo run -p pdfcraft-render --release --example search_bench --locked --offline
```

Use approved platform deadline; Windows timeout.exe is no general child-process deadline. Inspect nested Cargo/effects too. Missing cache/toolchain: not run, no installation/fetch authority.

## Vendor and scanner decisions

Baseline routes hayro, hayro-interpret, hayro-syntax, hayro-jbig2, lopdf, winit, egui-wgpu and vello_common patches to local vendor copies. Reinventory changes. Retain upstream source/version/hash, licence/notices, local patches/rationale, regressions/commands, consumers, upstream issue/PR/release status and removal condition. Preserve provenance and asset/font restrictions; permissive root code licence cannot settle transitive rights/build effects.

Vendor is excluded from workspace. Workspace tests may exercise patches without running all vendor tests. Map changed vello clip optimization to standalone visited-row regression plus renderer pixels; winit source guards differ from actual OS IME/DPI reproduction. Witness intended failure with patch/routing reverted in authorized isolated control. Never read/copy copyleft PDF implementation to reproduce behavior; permitted external oracles stay separate processes.

`deny.toml` covers all-feature licences, sources, wildcards and advisory exceptions. Check exceptions against actual paths/targets; record database revision/freshness. Lockfile scan cannot prove patched-source security or complete attribution. Missing local cargo-deny skips deliberately; CI absence fails. Record skip, never successful scan. No duplicate scanner or CI/exception changes merely for green badge.

`xtask ci` stops at first failure; later stages unrun. Hosted lanes differ, with uneven platform/resource coverage. Preserve stage results and actual resolved external workflow revisions. Local/hosted pass cannot establish unexecuted macOS, FreeBSD, GPU, reader or browser behavior.

Source anchors: [engine lifecycle](https://github.com/storytold/pdfcraft/blob/f0c9901c398372782a4eb001c95638289a5edab0/crates/engine/src/lib.rs), [render queue budget](https://github.com/storytold/pdfcraft/blob/f0c9901c398372782a4eb001c95638289a5edab0/crates/render/src/raster/work_queue.rs), [vendor provenance](https://github.com/storytold/pdfcraft/blob/f0c9901c398372782a4eb001c95638289a5edab0/vendor/README.md), [manifest/patch routes](https://github.com/storytold/pdfcraft/blob/f0c9901c398372782a4eb001c95638289a5edab0/Cargo.toml), and [dependency policy](https://github.com/storytold/pdfcraft/blob/f0c9901c398372782a4eb001c95638289a5edab0/deny.toml).

## Dependency assurance boundaries

Preserve actual scanned graph: features/targets, development/build dependencies, exclusions, patched/local sources. cargo-deny's [graph configuration](https://embarkstudios.github.io/cargo-deny/checks/cfg.html) can filter them. Record database commit/update time, scan time, tool/config/lock hashes and exceptions. Current [advisory documentation](https://embarkstudios.github.io/cargo-deny/checks/advisories/cfg.html): 90-day default staleness limit with fetching disabled. Month-old database need not fail tool, but cannot cover later advisories. Clean scan means no matching findings in checked scope, not security certification. No unauthorized exception changes/database fetch.

Optional [cargo-audit](https://github.com/rustsec/rustsec/blob/010f12aea2d7c663704ddd4c22d351ea5629e402/cargo-audit/README.md): focused lockfile diagnostic. Keep broader deny gate. Missing lockfile may trigger Cargo resolution; `fix` changes requirements. Review cwd/config/cache/network effects first. Verify exact package manifest/release artifact: pinned README claims Rust 1.74, contradicting [manifest's Rust 1.88](https://github.com/rustsec/rustsec/blob/010f12aea2d7c663704ddd4c22d351ea5629e402/cargo-audit/Cargo.toml). Latest monorepo release may be another package.

Defer cargo-vet until maintainer chooses trust/import criteria and review owner. [path-dependency policy](https://mozilla.github.io/cargo-vet/first-party-code.html) may require published-base audit without identifying local modifications; cannot qualify PdfCraft vendor patches. Keep independent hashes/diffs/regressions. [cargo-geiger](https://github.com/geiger-rs/cargo-geiger/blob/118a9a2812098ad757c9174ad00be21deaea7e0d/README.md) gives unsafe statistics for triage, not safety score. Checked [manifest](https://github.com/geiger-rs/cargo-geiger/blob/118a9a2812098ad757c9174ad00be21deaea7e0d/cargo-geiger/Cargo.toml) needs Rust 1.97; tool-host prerequisite changes no product MSRV. These observations authorize no install, import, certification or CI lane.
