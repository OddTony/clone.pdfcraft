---
name: pdfcraft-qa-test-planning
description: Use when PdfCraft testing needs a scoped plan with independent expectations, reachable controls, evidence layers and finite execution budgets.
---

# Plan evidence

Use before material verification or for incomplete plan. Already specified bounded check needs no new ceremony. Product design/production repairs: separate scopes.

Read current repository policy and [scope contract](references/contract.md). Obtain requested behavior, changed paths, source/dirty identity, relevant callers, existing assertions, permitted writes, required gates, available environments. Requirements/public specifications justify correctness; current implementation can supply characterization, labelled accordingly.

1. Per selected claim: stable ID, preconditions, action, expected result, forbidden effects. Identify actual boundary: Rust function, engine session, automation adapter, CLI process, egui shell, filesystem or browser artifact.
2. Name plausible wrong behavior plus useful accepted control. Confirm fixture reaches intended seam. Nonexistent document ID cannot test save-path confinement; document lookup may fail first.
3. Choose smallest sufficient layer. Rotation change: explicit engine page-state assertions, automation conversion page 1 → index 0, save/reopen semantics when output affected. UI keyboard/browser download claims require own executed environments.
4. Map direct/transitive inputs, including Cargo features, vendored patches, generated credits, optional fonts, oracle availability. Record selected checks/reasoned exclusions; narrow local slice leaves mandatory later gates pending.
5. Allocate owner, dependency barrier, writable resources, cases, deadline, retries, build jobs, test threads. Review [tooling constraints](references/tooling.md) before commands; missing tools require fallback or explicit blocked claim, never implied installation.
6. Predeclare evidence/acceptance. Follow [evidence guidance](references/evidence.md) for selected/executed identities, first failures, skips, doctests, output hashes, cleanup. Keep source-only recipes marked unrun.

Save rejection: specify relevant phase snapshot. `doc_save` with `flatten_fill_sign` may complete undoable edit before serialization. Pre-publication error need not promise unchanged pre-call graph.

Stop dependent planning for unresolved contractual expectation/required threshold. Preserve contrary evidence; continue independent analysis. Output compact claim-to-check map: controls, owners, budgets, exclusions, required gates, next action. Completed plan alone cannot qualify PdfCraft.

Author: David Osipov; personal@david-osipov.vision; [website](https://david-osipov.vision). Original skill material: MIT; see [licence and notices](THIRD_PARTY_NOTICES.md).
