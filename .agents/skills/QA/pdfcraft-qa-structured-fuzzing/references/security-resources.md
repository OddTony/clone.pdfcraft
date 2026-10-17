# Trust boundaries, raw inputs and aggregate limits

Read current [task contract](contract.md). Derive bounded local probe from concrete input-to-effect path; Naming attack category does not authorize scanning, egress, MCP/control endpoint startup or product policy change. Use synthetic owned inputs and protected-effect witnesses.

## Authority at the actual effect

[automation file-access implementation](https://github.com/storytold/pdfcraft/blob/f0c9901c398372782a4eb001c95638289a5edab0/crates/automation/src/file_access.rs) separates read/write roots. Either automation root configured: omitting other grants no authority of that kind. Neither configured: confinement feature inactive. Legacy `--root` and automation roots resolve differently; test actual selected configuration. Inspect effective working directory/root arguments without logging tokens/private paths.

Use valid open document and allowed same-operation control before disallowed path. Observe rejected resolution/authorization branch and protected side: outside file absent/unchanged, no leaked read data, no mutation, no extra publication. Nonexistent document may error before output-path check. Repeat at direct tools, `command_run`/`command_batch` and compact MCP `tool_call` where changed route reaches them. Read-only annotation/schema is not enforcement.

Select OS-relevant cases: component-prefix sibling, absolute/relative and parent traversal, nearest existing parent, symlink/junction or dangling link, read-root save-in-place, ungranted access kind, Windows device/share/drive spellings. Do not create links/permissions outside owned fixtures, require elevation, or infer native Windows behavior from Unix run. Source documents check/open race with another process replacing directories; confinement pass cannot establish hardened OS sandbox or exploit.

## Preserve the representation

Map bytes/text → decoder/parser → validation/repair → permission → output/effect. When material, keep exact input hashes, encoding, options, transport. Duplicate JSON members need raw JSON at parser seam; map construction/reserialization collapses them. Malformed UTF-8 needs bytes through real decoder, never replacement-decoded text. Escaped names, numeric tokens, PDF offsets/xref structure and indirect cycles must survive to challenged boundary. Establish application policy before expecting universal rejection: lenient PDF parsing may repair with explicit notice. Acceptance, rejection, repair and unsupported input: distinct outcomes.

Pair every hostile/invalid family with useful accepted neighbors. Check full intended semantics and forbidden effects; reject-everything must fail. Local collaborator double proves only substituted behavior. Never transfer website DOM/sanitizer/CSP requirements into native PdfCraft because donor used them; browser-host message/origin behavior needs actual WASM/browser producer and permitted local test surface.

## Budget ownership

Input-derived signed values, lengths, offsets, page numbers and geometry need meaningful domain units, finite checks, checked conversions/arithmetic, allocation caps and cycle/depth handling. Saturation alone cannot make enormous allocation safe. Cover boundary neighbors/high-risk combinations explicitly; random samples rarely establish exact threshold.

[script entry](https://github.com/storytold/pdfcraft/blob/f0c9901c398372782a4eb001c95638289a5edab0/crates/js/src/lib.rs#L1000): source-byte, syntactic nesting/prefix/conditional, loop/recursion and host-output limits. Read current values; distinguish bytes/characters and syntactic guards/runtime work. Native XFA timeouts may abandon still-running worker; WASM has different threading constraints. Per-script limits cannot prove bound across all document scripts/events.

[render result queue](https://github.com/storytold/pdfcraft/blob/f0c9901c398372782a4eb001c95638289a5edab0/crates/render/src/raster/work_queue.rs) bounds completed storage at reviewed revision, excludes active raster pixels, admits one oversized result alone. Filters cap intermediate output; neither is whole-process memory guarantee. Name budget owner, shared/nested counters, active jobs, retained snapshots/caches, replacements, shrink/replay work and release points. Observe process/worker totals where claim needs them. Unwind containment cannot catch abort/OOM or undo filesystem effects. Potentially fatal/resource-heavy cases need owned child/supported host limits; in-process “no panic” assertion insufficient.

Record selected/reached cases, benign/hostile outcomes, exact branch witnesses, state/effect differences, peak/cumulative measurements, timeout/cancel/exit and cleanup. Stop affected run when effect isolation, raw delivery or process ownership unavailable. Continue source review/small deterministic cases; leave unsupported safety dimension unqualified.

## Conditional interpreter and sanitizer evidence

[Miri](https://github.com/rust-lang/miri) can detect undefined behavior in executed interpreted paths; one passing case cannot prove crate sound. Record pinned toolchain, flags, target, seeds/inputs, supported operations, reached boundaries. Interpreter/FFI/platform limits leave native behavior outside result. Borrow/provenance checking options and evolving Rust memory model need version-specific interpretation; do not label every report fully validated production exploit.

[Rust sanitizer support](https://doc.rust-lang.org/unstable-book/compiler-flags/sanitizer.html) depends on exact sanitizer, nightly, target, linking and instrumentation perimeter. Native dependencies may need separate instrumentation; never silently count uninstrumented boundary. Sanitizers observe executions, not all possible inputs, generally alter resource cost. Keep ordinary native regressions and independent semantic assertions. Neither Miri nor sanitizer replaces whole-process hostile-input budgets, saved-document fidelity, actual OS behavior or existing no-unsafe policy. Missing capabilities stay optional proposals with deterministic/source-review fallbacks; no component/native tool installation follows.
