---
name: pdfcraft-qa-failure-triage
description: Use when a PdfCraft test or gate fails, times out, intermittently passes or contradicts its reports and the first causal failure needs classification.
---

# Classify the first failure

Use for observed contradiction/missing required result. Unexecuted source concern is hypothesis, not reproduced defect. Skill grants no production-fix/unlimited-retry authorization.

Read current policy and [scope contract](references/contract.md). Freeze first attempt: exact command/cwd, source and relevant dirty/config/lock/fixture identities, compiler/runner, target/features, selected tests, raw outputs, exit/signal, deadline, process state. Keep later attempts separate under [evidence guidance](references/evidence.md).

1. Locate first causal break: dependency resolution/build, discovery, fixture preparation, action reachability, assertion, asynchronous completion, cleanup, reporting. Missing compiler/offline cache is environment limitation. Filtered zero-test success is missing execution.
2. Trace relevant preconditions before accepting alleged mechanism. For `doc_save` confinement, first open valid synthetic document; prove path resolver reached. `ToolError::Failed` from absent document cannot establish path protection; schema rejection may precede desired engine error.
3. Falsify competing explanations with smallest same-input rerun/accepted control within remaining budget. Preserve intermittent-failure seeds, order, contention. Retry pass records pattern; cannot explain/erase first failure.
4. Compare independent expectation. Existing engine `failed_edit_changes_nothing` checks early all-pages-deletion rejection; does not reproduce late refresh failure after history mutation. Label unexecuted concern accordingly.
5. Classify product, oracle/test, harness/configuration, environment/resource, policy/gate, unresolved. Proposed fixture/selector/expectation fix: show why it restores intended measurement. Keep valid tests-only RED; route product implication without weakening assertion.
6. Inspect wrapper exits/optional-tool behavior. Local `cargo xtask deny` skip is no completed dependency scan. Use [native fallback](references/tooling.md) for unavailable tools; retain narrower evidence limit.

Stop affected conclusion when report identity missing, intended boundary unreachable, budget exhausted. Output smallest preserving witness, observed versus inferred mechanism, contrary evidence, first-failure artifacts, cleanup state, next owner/action. Required failed/unexecuted gates remain incomplete.

Author: David Osipov; personal@david-osipov.vision; [website](https://david-osipov.vision). Original skill material: MIT; see [licence and notices](THIRD_PARTY_NOTICES.md).
