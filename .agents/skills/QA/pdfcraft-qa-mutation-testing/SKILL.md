---
name: pdfcraft-qa-mutation-testing
description: Use when a bounded PdfCraft test-sensitivity question requires selected source faults, causal killing assertions and survivor analysis.
---

# Focused source fault sensitivity

Use to challenge tests against specified plausible source changes. Input mutation fuzzing, general regression execution, product correctness alone do not select technique. No implied full-workspace campaign/mutation-score threshold.

Read [task contract](references/contract.md), [source mutation guidance](references/generative.md), relevant [PDF invariants](references/pdf-fidelity.md). Receive independent requirement, source perimeter, selected tests, authorized isolated writes, tool/config/source/lock/fixture identity, total time/mutant/build budget. Inspect installed cargo-mutants version/help, actual test selection, baseline behavior, report/cache ownership. Missing tooling: one reviewed reversible bad edit in owned isolated copy or unexecuted proposal; no automatic install.

First execute nonempty healthy baseline on original source; retain actual assertions. List candidate mutants before executing finite approved subset. Use separate build/output ownership, low concurrency, outer total deadline. Existing target instructions govern scoped Cargo gates. Listed mutant, empty selection, failed baseline, uncompiled source is no sensitivity evidence.

Small probe: challenge filter output-limit assertion by removing one relevant cap/comparator in isolated copy. Use hand-computed accepted vector at limit, cap-breaching vector reaching altered decoder. Original must accept/reject as specified; mutant must fail intended assertion. Pair benign change outside promised semantics to detect overfitted comparator. Do not count unrelated compiler/import failure or timeout as behavioral kill. Restore isolated source; rerun original witness before closing.

Retain each mutant's stable source/location/operator, original/mutated hash, executed tests, first outcome, causal killing witness. Preserve actual cargo-mutants classifications, e.g.: caught, missed, timeout, unviable; do not import StrykerJS meanings. Investigate survivor with requirement-grounded distinguishing input/sequence. Equivalent/unobservable needs bounded argument and independent review, not score-driven exclusion. Distinct requirements remain distinct even if they kill same finite mutant set.

Record fresh versus reused results; invalidate stale cache lineage after relevant configuration/fixture changes. Preserve failures; no scope broadening to obtain green. Focused score cannot replace broader required gate; leave gate pending. Stop if baseline, test binding, isolation, budget unresolved.

Report candidate/executed counts/identities, baseline/healthy/benign controls, causal kills, survivors/unresolved classifications, exact commands/artifacts, cumulative resources, cleanup. Runtime absence leaves authored examples uncompiled/unqualified.

Author: David Osipov; personal@david-osipov.vision; [website](https://david-osipov.vision). Original skill material: MIT; see [licence and notices](THIRD_PARTY_NOTICES.md).
