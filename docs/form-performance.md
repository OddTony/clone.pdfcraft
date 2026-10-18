# AcroForm interaction profiling

Status: optimized Release build and synthetic profiling complete; user confirms the initial
fixes on the original sheet. Regression and scoped quality checks passed. Last updated: 2026-10-18.

The benchmark creates contributor-original PDF objects in memory; no third-party sheet, font,
or screenshot is committed. The original input is never saved by the profiling example.

## Architecture

- `crates/ui-egui/src/forms_ui.rs` queues `Edit::SetFieldValue` for a text commit or checkbox
  toggle. `crates/automation/src/forms.rs::form_fill` uses the same edit API.
- `crates/engine/src/lib.rs::Session::apply` runs edits on a copy, preserving rollback and undo.
  `run_edit` calls `pdfcraft_forms::set_value_with` with an engine `JsRunner`.
- `crates/forms/src/lib.rs` validates and writes the edited value, then runs one calculation
  pass. `/CO` precedes calculated fields omitted from `/CO`. Each script sees prior writes.
  There is no dependency graph or dependency-based filtering: dynamic references and script
  side effects are evaluated on every pass.
- `crates/engine/src/js.rs::JsRunner::run` snapshots the fields, invokes `pdfcraft_js::run`,
  then converts changed JavaScript field states into form updates.
- `crates/js/src/lib.rs::run` starts a scoped worker per script. `run_here` builds a fresh Boa
  context, installs the API, evaluates document scripts and evaluates the action. No script or
  bytecode cache is present in this AcroForm path. XFA uses a separate session implementation.
- `crates/forms/src/scripting.rs::apply_changes` applies script side effects before the next
  calculation. It rereads the field snapshot when side effects occur. Calculation results
  equal to the existing value already skip value writes and appearance generation.
- Checkboxes write the field `/V` and widget `/AS`; they enter the same calculation pass as
  text edits. JavaScript exposes the export value or `Off`, and `isBoxChecked`.
- `Session::refresh_scoped` writes an incremental working revision, rereads fields and rebuilds
  the render pool. `crates/ui-egui/src/editing.rs::apply_edit` compares old/new fields and
  invalidates affected pages (including dependent fields on other pages). It does not blindly
  invalidate every page for an ordinary AcroForm fill. Rendering uses page/tile requests;
  widget appearance streams are separate from cached page rasters.
- `crates/forms/src/lib.rs::walk` reads inherited `/MaxLen`; the UI and value validation both
  consume `Field::max_len`.

## Findings and changes

The dominant measured cost was the bridge, not executing calculation logic:

1. `JsRunner` expanded a sparse `Outcome::changed` into a complete second snapshot. `changes`
   then searched the first snapshot linearly for **each** field, even when JavaScript had not
   changed any field directly. That repeated quadratic comparison ran after every calculation.
   It now compares the sparse outcome only, with an indexed original snapshot when needed.
   Empty outcomes return immediately; unchanged writes still produce no form updates. The
   console/button path uses the same conversion.
2. Once that was removed, field-state construction dominated. Reading every widget's display
   flags on every script traversed the same immutable pre-edit COS snapshot. This workload
   exceeds COS's 1,024-object editing-cache cap, repeatedly reparsing widget dictionaries.
   The runner now memoizes only those scalar display readings by widget reference. Each edit
   creates a new runner; current values, read-only/required flags and options still come from
   the current field snapshot. No global cache or object-cache limit changes were needed.
3. The parser exposed `/MaxLen 0` as `Some(0)`, so both editor and API blocked input. It now
   exposes only positive limits; zero clears an inherited limit. The original object entry is
   preserved through save/reopen. This is an interoperability interpretation for zero, **not
   a claim that ISO explicitly defines zero as unlimited**. ISO 32000-1 Table 229 defines an
   optional inheritable maximum length without defining a special zero meaning:
   [public specification](https://opensource.adobe.com/dc-acrobat-sdk-docs/pdfstandards/PDF32000_2008.pdf).
   Positive limits remain enforced; no field names or character-sheet rules enter product code.

All calculation actions still execute in the same order. No dependency filtering, script
removal, context reuse, bytecode caching or recursive scheduling changes were introduced.
Equal calculated values already skipped appearance regeneration; this behavior remains.
The Release investigation also found a full field-state copy inside the JavaScript host on
**every** script invocation. `pdfcraft_js::run_shared` now accepts an immutable shared snapshot;
`Arc::make_mut` detaches it on the first field write. The engine retains the initial snapshot
for comparing effects. Read-only calculations no longer copy every field again. Each script
still gets a fresh context and thread; exceptions, globals and uncommitted writes remain
isolated. Scripts that write fields retain the previous copy isolation. The existing borrowed
`run` API remains available. No new dependencies were added.

## Measurements

Measured on this checkout under Rust 1.99.1, Linux x86_64, the default optimized development
profile, two-job builds. Serial benchmark runs had no concurrent compilation. These are
**synthetic engine measurements, not measurements of the original Pathfinder PDF or GUI input
latency**. Five edits: ranks 1, checkbox on, ranks 2, checkbox off, ranks 0. Every edit runs all
190 custom calculations among 3,129 fields on a six-page document. Widgets are colocated
on the first page; the fixture is an engine calculation stress test, not a layout replica.

| Synchronous edit | Before (ms) | After (ms) |
|---|---:|---:|
| ranks 1 | 4,425.274 | 424.465 |
| checkbox on | 4,378.642 | 458.716 |
| ranks 2 | 4,291.131 | 521.721 |
| checkbox off | 4,391.942 | 492.851 |
| ranks 0 | 4,322.570 | 453.392 |
| Median | **4,378.642** | **458.716** |

Median speedup: **9.55×**, approximately **89.5% less synchronous editing time**. The synthetic
scripts are deliberately small; this does not predict the exact result for other documents.
Raw logs and the baseline test executable are retained locally in `target/form-performance/`.

Separate instrumented serial runs (mean per edit; inclusive nested spans):

| Phase | Before (ms) | After (ms) |
|---|---:|---:|
| JavaScript, including thread/context setup | 225.934 | 201.584 |
| Changed-field comparison | 2,814.805 | 0.024 |
| Entire script dispatch, all 190 scripts | 4,320.643 | 352.733 |
| Calculation pass, including changed appearances | 4,399.084 | 390.958 |
| Direct field write | 0.201 | 0.196 |
| Appearance generation, including Format | 61.225 | 22.981 |
| Form event, including direct write and calculations | 4,411.767 | 402.981 |
| Engine refresh/serialization | 23.499 | 21.826 |
| Whole engine edit | 4,442.144 | 431.539 |

An intermediate run after sparse comparison alone measured 1,193.419 ms/edit in field-state
construction. After display memoization it measured 101.821 ms/edit. The intermediate run's
whole engine edit averaged 1,592.101 ms. Appearance time also benefits from the changed cache
working set; there was no change to which calculated appearances are generated.

Rendering and compositor time are not included in these edit numbers. The original sheet and
GUI acceptance run remain required to quantify them. The example below measures page rendering
separately; the UI spans report dispatch and cache invalidation separately.

## Release comparison and remaining cost

The application and CLI were built with the repository's Release profile (thin LTO,
one code-generation unit). The profiling example was built in the same dependency graph.
A fresh in-memory fixture was exported with create-new semantics to
`target/form-performance/synthetic-large.pdf` (SHA-256
`300431eeae117458d96e52877e12a9e63ea8dcbb8ad4500da717ba00ebb2f1e1`). It remained unchanged.
The example alternated `ranks` between 1 and 2 for six samples per process, always executing
all 190 scripts. Page rendering was timed separately.

Initial matched comparison, before the additional shared-snapshot change:

| Build | Samples | Median engine edit (ms) |
|---|---:|---:|
| Development | 6 | 460.697 |
| Release | 6 | 408.358 |

Release alone reduced the median by about 11%. A detailed Release run measured 106.947 ms/edit
constructing field states, 174.696 ms in JavaScript setup/execution, and 19.808 ms refreshing
engine view data. Calculation-order setup was only 0.319 ms. That supported investigating the
host's extra copy, rather than changing calculation scheduling or disabling calculations.

The pre-change Release profiler was retained, then compared with the shared-snapshot Release
profiler in alternating runs (old, new, old, new). No compilation ran during measurement.

| Release version | Samples | Median edit (ms) | Range (ms) |
|---|---:|---:|---:|
| Before shared snapshots | 12 | 381.380 | 353.468–447.833 |
| With shared snapshots | 12 | 267.063 | 258.545–313.777 |

The additional optimization reduced median engine time by **30.0%** (1.43× speedup).
This is a separate comparison from the earlier five-edit benchmark; do not combine their
speedup factors. The page-render/setup cost in the initial matched comparison was about
6–9 ms for this deliberately overlapping synthetic layout, not for the original sheet.

After shared snapshots, a separate instrumented Release run averaged:

| Phase | Mean per edit (ms) |
|---|---:|
| Field-state construction | 75.754 |
| JavaScript setup/execution | 83.198 |
| Complete script dispatch | 200.480 |
| Calculation pass | 237.780 |
| Calculated-value writes, including appearances | 25.089 |
| Appearances | 23.712 |
| All complete field enumerations (three per edit) | 27.387 |
| Calculation-order setup | 0.305 |
| Engine refresh | 19.128 |
| Whole engine edit | 272.807 |

These inclusive timings show that scheduling is now negligible on the fixture. The main
remaining costs are constructing/disposing field snapshots and creating/executing isolated
JavaScript contexts. Further snapshot caching or context reuse would need additional semantic
work and real-document evidence; no dependency-based skipping or global/context reuse was
introduced. The current shared-snapshot change was retained because its paired measurement
showed a substantial improvement and isolation regressions passed.

**Remaining limit:** these numbers do not quantify the user's original PDF or compositor
latency. The original file path and a representative delayed field are still needed for a
reproducible real-document engine/render/UI comparison. The user's report confirms that the
initial fixes improve the original sheet and retain calculations and class-name editing.

## Reproduce

Use the same compiler, build profile and machine for before/after runs. The default development
profile optimizes workspace crates at level 1 and dependencies at level 2. Build with two jobs.

```sh
cargo test -p pdfcraft-engine -j 2 --lib acroform_interaction_benchmark -- --ignored --nocapture
PDFCRAFT_PROFILE_FORMS=1 cargo test -p pdfcraft-engine -j 2 --lib acroform_interaction_benchmark -- --ignored --nocapture
cargo run -p pdfcraft-engine -j 2 --example profile_form -- '/path/to/sheet.pdf' 'field name' '1' '2'
cargo build --release -p pdfcraft -p pdfcraft-cli -p pdfcraft-engine --bins --example profile_form -j 2
./target/release/examples/profile_form '/path/to/sheet.pdf' 'field name' '1' '2'
```

To export a fresh synthetic fixture for standalone build comparisons, set
`PDFCRAFT_BENCH_EXPORT` to a new filename when running the ignored benchmark. The destination's
parent must exist; export refuses to overwrite any existing file. The ordinary benchmark
writes no PDF.

For a checkbox, pass `true false` instead of `1 2`. The example alternates the values for six
edits, reports script errors, and measures synchronous edit time separately from fresh renderer
setup and a page raster. It does not measure compositor latency or time until a GUI frame is
presented. Use the real app's opt-in control channel for that acceptance check.

`PDFCRAFT_PROFILE_FORMS=1` writes timings to stderr, without field names, values or script
source. Spans are inclusive and nested: **do not add them together**. `script_dispatch` includes
`field_states` conversion, `javascript` (including worker/context setup), and `script_diff`.
`calculation_pass` includes scripts and changed appearances; `appearance` includes Format.
`field_enumeration` times complete field snapshots, `calculation_order` times scheduling,
`native_calculation` times AF rules, and `script_side_effects` times applying JavaScript writes.
`calculation_write` includes the changed result and its appearance. `field_write` covers the direct edit, and `engine_refresh` covers serialization and rebuilding
view data. In the desktop app, `ui_edit` includes dispatch and engine work, while `ui_refresh`
covers the post-edit UI cache updates and invalidation (not asynchronous raster/compositor work).
`engine_edit` includes the whole synchronous engine operation. Profiling is disabled
on wasm to avoid unsupported native clocks. Page raster milliseconds are already returned by
the renderer and reported by the example.

## Launch the separate development build

```sh
cd /home/mushin/pdfcraft-dev
cargo build --release -p pdfcraft -p pdfcraft-cli -j 2
touch target/release/PdfCraft.portable
./target/release/pdfcraft '/path/to/sheet.pdf'
```

The local portable marker keeps preferences, logs and recovery files under
`target/release/PdfCraftData`, separating this build from the installed app's settings. The
installed executable is untouched. For timings, prefix the last command with
`PDFCRAFT_PROFILE_FORMS=1`. For opt-in UI control, add
`--control /home/mushin/pdfcraft-dev/target/form-performance/control.json`. Do not print or
share that file: it contains the launch token.

## Validation

- Engine unit suite: passed, including existing custom validation/calculation/format and
  document-helper tests, checkbox/rank ordered chains, dynamic lookup and property/value
  side effects, undo/save/reopen, and preservation of unchanged calculated appearances.
- Forms unit suite: all 68 tests passed. The new zero-limit regression was first run against
  the old parser and failed (`Some(0)` instead of `None`), then passed with the compatibility
  interpretation. Its positive-limit control and preserved raw entry also passed.
- The ignored 3,129-field/190-script benchmark passed before and after, and verified its
  final calculated chain and independent field. It remains opt-in rather than enforcing a
  machine-dependent wall-clock threshold in CI.
- Scoped WASM check of forms, scripting, engine and UI passed. Native-only timing code is
  excluded on wasm. Existing dead-code warnings remain in renderer/UI code outside this fix.
- Dependency layering passed (35 crates). Asset validation passed (270 repository assets,
  six dependency-bundled assets and 22 build-time fetched assets). Parity validation passed.
- The 3,129-field/190-script regression passed again after shared snapshots; final chain and
  independent values remain correct. Profiling confirmed exactly 190 script executions for
  each of the six measured Release edits.
- Desktop and CLI development and optimized Release executables built successfully. The UI timing calls use the
  engine façade (`pdfcraft_engine::form_profile`); directly naming the transitive forms crate
  caused two E0433 errors, corrected without adding a dependency or disabling instrumentation.
- UI regression suites: all 22 form tests and all 5 JavaScript tests passed.
- CLI smoke test: `form_fill` produced bonuses 1 → 4 → 5 → 2 → 0 across rank/checkbox changes;
  `page_render` and separate synthetic PDF save both succeeded. The generated render was inspected.
- User acceptance: the original sheet is reported substantially faster; class-name input and
  automatic calculations work correctly. Some input delay remains. Independent Release
  measurements on that document await its local path and a representative field.

- Shared-snapshot regression: caller state survives successful writes and exceptions;
  subsequent scripts see the original fields and a fresh global environment. All 45 scripting
  tests, 235 engine tests and 68 forms tests passed after this change.
- Scoped Clippy on scripting, engine, forms and UI, including all targets: passed with
  `-D warnings`. Formatting and whitespace checks passed.
- Final `javascript_through_tools` automation regression passed (custom calculation,
  document helper/formatting, validation, field side effects and JavaScript enable/disable).
  The final Release CLI smoke run also passed the bonus/render/save checks.

## Revision history

- 2026-10-18: build optimized Release executables; retain shared JavaScript snapshots after a paired 30% synthetic improvement; verify snapshot isolation, UI/tool regressions and scoped native/WASM quality gates.
- 2026-10-17: trace the AcroForm path, add opt-in timing and a synthetic benchmark, remove quadratic outcome conversion and repeated display reads, and accept zero-length limits as unlimited.
