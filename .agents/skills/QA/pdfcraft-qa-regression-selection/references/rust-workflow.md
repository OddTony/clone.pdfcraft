# Rust verification workflow

Maps evidence layers to current PdfCraft source. Reread current `AGENTS.md`, `CLAUDE.md`, relevant crate READMEs, manifests, command implementations before recipe use. Public contributors can use issue/task, `docs/architecture.md`, `docs/gaps.md`, `docs/target-app-parity.md`, parity entries/source when private `plan/` or internal background links unavailable. Missing background authorizes neither invented task nor weakened rule.

Authoring baseline: [PdfCraft f0c9901c398372782a4eb001c95638289a5edab0](https://github.com/storytold/pdfcraft/tree/f0c9901c398372782a4eb001c95638289a5edab0). Workspace declares edition 2024, Rust 1.95. Explicit member inheritance: root `workspace.package.rust-version` is not every package's MSRV. Lockfile pins execution dependencies; manifest range/available modern docs cannot prove compatibility. Source observations: no completed build/runtime result.

## Before executing

Review `.cargo/config.toml`, selected manifests, dependency/build/procedural-macro effects, CI/xtask expansion. Cargo builds/checks may execute local code. `--locked`: preserves resolution, may fetch. `--offline`: prevents Cargo network retrieval, not arbitrary build-script I/O. Use existing approved toolchains/caches; absence/offline cache miss stays blocked. No component installation, global configuration change, code indexing or server launch to repair evidence gap.

Local build inputs include `crates/fonts/build.rs` reading optional `CRAFT_FONTS_DIR`, `crates/ui-egui/build.rs` generating contributor credits, Windows executable resource compilation. Missing craft-fonts may leave relevant tests without intended assertions; record actual font requirement/input state. External oracle absence may be optional too: inspect `PDFCRAFT_ORACLES_REQUIRED` semantics/actual test path before claiming oracle execution.

Scope local compilation/tests to affected crates under current policy. Separate writers: separate worktrees/`CARGO_TARGET_DIR`; isolate temporary PDFs, output files, writable caches, processes too. Bound total machine demand. `-j 2` bounds build jobs, not libtest threads/renderer workers. Add reviewed outer process deadline; verify owned descendant cleanup. Windows console `timeout.exe` cannot wrap/terminate arbitrary child command. Command proposal without working supervisor stays unrun.

## Native recipes and their limits

Source-checked recipes, **not executed results**. Adjust package/test selection from current source; independently justify each claim. Review process-local offline policy for nested Cargo too; xtask alias does not automatically forward Cargo flags to subprocesses.

```text
cargo fmt --all -- --check
cargo clippy -p pdfcraft-engine -j 2 --all-targets --locked --offline -- -D warnings
cargo test -p pdfcraft-engine -j 2 --locked --offline tests::failed_edit_changes_nothing -- --list --exact
cargo test -p pdfcraft-engine -j 2 --locked --offline tests::failed_edit_changes_nothing -- --exact --test-threads=2
cargo test -p pdfcraft-engine -j 2 --locked --offline --doc
cargo test -p pdfcraft-automation -j 2 --locked --offline --test automation edit_undo_redo_and_save_round_trip -- --exact --test-threads=2
cargo test -p pdfcraft-ui-egui -j 2 --locked --offline --test control
cargo check -p pdfcraft-cli -j 2 --locked --offline --no-default-features
```

Per process preserve command/cwd, compiler/runner, target/features/profile, relevant source/dirty/config/lock/fixture identity, planned test list, actual named execution, ignored/filtered outcomes, first failure/retries, exit/signal, timing, cleanup. Discovery compiles/lists as applicable, executes no assertions. Zero-test filter may exit successfully. `cargo test --all-targets` excludes doctests; nextest needs separate doctest path too. Rustdoc `compile_fail` proves rejection for actual diagnostic only; `no_run` compiles example without running. See [Cargo test](https://doc.rust-lang.org/cargo/commands/cargo-test.html) and [rustdoc documentation tests](https://doc.rust-lang.org/rustdoc/write-documentation/documentation-tests.html).

Arbitrary stdout words/ordinal total cannot establish sufficient test identity. Preserve native artifacts directly. Unobservable required dimension: keep unknown, withhold dependent qualification. Manual records support review; never impersonate automatic capture/attestation.

## Pick the boundary that observes the claim

| Layer | Concrete seam | Boundary still unproved |
| --- | --- | --- |
| Unit/domain | `Session::apply`, filter function or geometry transformation with explicit expected facts | CLI process, filesystem replacement, UI interaction |
| Component | Existing `egui_kittest` shell, `ControlClient`, AccessKit state/local actions | Native picker/backend, physical IME, platform AT delivery |
| Contract | `ToolDef`, `Automation::call`, typed API, CLI report or MCP message | Whole application journey, durable output |
| Integration | Engine/COS writer/reopen, automation file output/real consumer | Built executable or actual OS/browser path unless included |
| End-to-end | Built CLI script, opt-in native UI, or actual browser-WASM artifact | Other platforms, AT/durability boundaries unobserved |
| Static | Compiler/clippy, metadata layers, assets/parity checks | Runtime behavior, full feature completeness |

Engine indices differ from automation pages. `Session::apply(DocId, Edit::RotatePages { pages, degrees })`: zero-based indices; automation: 1-based pages. `DocId(pub u64)` distinguishes Rust type; public constructor cannot establish existence/permission. Compile misuse with `usize` needs intended type diagnostic/compiling companion; runtime tests still need document opened from valid fixture.

Prefer current [`engine` source](https://github.com/storytold/pdfcraft/blob/f0c9901c398372782a4eb001c95638289a5edab0/crates/engine/src/lib.rs) over stale summaries. `save_bytes`: incremental result unless rewrite required; `check_save`: validate prepared output before publication; `mark_saved`: acknowledge/reopen after successful writing. Unknown graph data, saved bytes, working bytes, display-only appearances: distinct preservation obligations. Automation `doc_save` may apply `FlattenFillSign` before serialization. Compare failure against contract's phase snapshot. Disk replacement followed by `mark_saved` failure differs from rejection before replacement.

Existing `tests::failed_edit_changes_nothing` rejects deleting all pages before edit commits. Evidence for that path, no late-refresh fault reproducer. Late failure: prove seam reached; inspect appropriate graph, bytes, history, keys, dirty/generation, derived state. Source ordering may motivate scenario; cannot prove reproduced defect.

UI tests: locked egui/kittest API/target's existing helpers. `build_eframe` currently returns app state directly. Asynchronous preparation: bounded state predicates, timeout diagnostics, observed cleanup. Simulated `with_os` cannot load OS backend. `cargo xtask wasm` checks classified library/standalone crates; no complete browser app, download, real IME or AT execution.

## Controls, gates and optional instruments

Keep accepted useful control beside rejection cases. State plausible wrong behavior detected and permitted variation that must remain valid. Wrong error before intended guard, missing import, parser failure, timeout or cleanup error cannot establish selected behavioral RED. Regression claim: observe failure with actual fix reverted in isolated permitted copy; rerun after authorized fix. Existing correct behavior may pass immediately; never manufacture RED. Tests-only may finish with preserved valid regression while product qualification stays incomplete.

Current `cargo xtask layers` reads Cargo metadata/enforces own rule model, including standalone/permitted sideways edges. Cannot prove all facade documentation/runtime architecture claims. When changing mechanism, challenge actual bad/legitimate edges. Vendor packages excluded from workspace; map changed patches to relevant named regressions/explicit standalone commands. Do not count every vendor test as covered by workspace execution.

Existing required gates remain authoritative. `cargo xtask ci`: fmt, clippy, workspace tests, layers, wasm, assets, deny, parity; stops at first failure. Hosted CI differs; successful hosted job cannot establish every local xtask stage. Missing local cargo-deny may yield successful skip; record skipped, preserve required CI behavior. Keep local builds scoped unless current policy and concrete reason justify widening. No automatic workspace mutation/fuzz campaign follows from skill.

Default: existing Cargo/libtest, synthetic testkit fixtures, current tests. Use already-declared proptest with explicit boundary examples, independent expectations, per-case reset, finite case/shrink bounds, replay seeds. Engine has no proptest dev-dependency at baseline; short deterministic transition table: valid fallback. Optional nextest/coverage/compile-test/mutation tools need approved availability and meaningful controls; absence cannot fail contributor. Naming instrument creates no mandatory gate.

## Feature and compiler compatibility

Before runner selection, write small supported package/target/compiler/profile/defaults/explicit-features matrix. Check default, no-default, affected supported optional combinations; document invalid combinations. Workspace all-features build unifies selections, may hide member's missing feature declaration. Neither powerset nor actual MSRV run. Cargo resolver 3's Rust-version fallback assists resolution; cannot prove dependencies compile on promised compiler. See [Cargo resolver](https://doc.rust-lang.org/cargo/reference/resolver.html) and [feature unification](https://doc.rust-lang.org/cargo/reference/features.html).

[cargo-hack](https://github.com/taiki-e/cargo-hack/blob/aad44e470198c08385a7b0c6d81284942a21a14b/README.md): optional for bounded declared matrix. Each-feature, depth-limited powersets, exclusions have different coverage. Never default to exponential workspace powerset. `--no-dev-deps` temporarily rewrites actual manifests; `--no-private` may modify them; `--remove-dev-deps` leaves changes. Never call read-only or run beside another writer. If scope permits writes: disposable owned copy, original/final manifest hashes, restoration inspection after interruption. Omitting options retains possible dev-dependency feature leakage; explicit Cargo selections: fallback, no claim of pure consumer-feature isolation.

## Runner isolation and coverage diagnostics

Nextest: individual tests in separate processes; ordinary libtest: tests within test binary's process. Nextest pass may hide process-global interference seen under libtest. Preserve relevant original-runner regression. Files/ports/external services may still collide across processes. Nextest `-j`: test concurrency; explicit `--build-jobs`/`--test-threads` for separate budgets, unlike Cargo's `-j`. Fail zero selected tests with supported `--no-tests=fail`; record actual runner version; keep doctests separate. Sources: [execution model](https://nexte.st/docs/design/why-process-per-test/), [running configuration](https://github.com/nextest-rs/nextest/blob/6c2ab7e669761d9ef063bf00b2440c0fb5cca1ac/site/src/docs/running.md).

[cargo-llvm-cov](https://github.com/taiki-e/cargo-llvm-cov/blob/0dc905679ae6b4740b186f6e2c2ef1ab78fb76d7/README.md): diagnostic option when available. May offer missing LLVM component installation; preflight separately. Checked [setup path](https://github.com/taiki-e/cargo-llvm-cov/blob/0dc905679ae6b4740b186f6e2c2ef1ab78fb76d7/src/context.rs) supports `CARGO_LLVM_COV_SETUP=no` to fail instead of setup. Use owned fresh instrumentation/profile area; `--no-clean`/`--no-report` retain data for intentional merges. Identify every merged run; reconcile selected/executed tests, source, features, exclusions before reporting coverage. State actual line/region/branch collection; unsupported doctest/branch instrumentation stays unrun. Percentage cannot prove assertion quality, PDF correctness or new gate threshold.

## Compile diagnostics and public API compatibility

Optional [trybuild](https://github.com/dtolnay/trybuild/blob/3f6a32437cf608d28fb2eda2d8c3e9c0dfc06ff3/README.md): useful important diagnostic/macro/type-state contracts with compiling companion/intended rejection cause. Pass specimens execute after compilation; preflight effects. Maintainer discourages wrong-type call for every public API; existing small DocId/usize specimen cannot justify new framework. Diagnostic snapshots depend on compiler/version/components; review against contract, never blindly overwrite stderr.

Optional [cargo-semver-checks](https://github.com/obi1kenobi/cargo-semver-checks/blob/260fad71517e8b320c5c1ad1f367900ab37362a1/README.md): library API compatibility versus explicit baseline. Implicit registry baseline/heuristic feature selection may differ from intended comparison. Choose actual prior revision/release, manifest version, target, supported feature selections. Default-feature check may miss optional APIs; declared version changes affect allowed compatibility level. Confirm important trait/re-export/dependency-type changes with minimal real consumer compiling against baseline. Generated builds may affect Cargo/config/cache/network. Tool cannot prove behavioral/full compatibility; source/API review and consumer compile specimens: no-new-tool fallback.

Keep repository's current Clippy policy. Rolling docs describe Cargo 1.97 warning configuration; no Rust 1.95 recipe. Select useful restriction/nursery lints individually rather than enabling both entire groups or suppressing findings to become green. See [Clippy usage](https://doc.rust-lang.org/clippy/usage.html) and [lint groups](https://doc.rust-lang.org/clippy/lints.html). All additional tool recipes: unexecuted, installation-gated.
