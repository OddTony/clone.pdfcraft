---
name: pdfcraft-qa-independent-review
description: Use when PdfCraft tests or verification claims need independent assessment of oracle strength, reachable boundaries, execution and remaining gaps.
---

# Review evidence independently

Use for requested review of defined artifact/claim. Ordinary bounded work needs no second agent; skill itself authorizes neither delegation nor product edits.

Read current instructions and [scope contract](references/contract.md). Receive review scope, requirements, frozen diff/source/configuration/fixtures, native reports, allowed reads/writes, budget. Derive expectation before accepting author's explanation. Relevant input changes invalidate affected conclusions.

1. Trace requirement → preconditions → public action → assertion → observed result. Separate specification/characterization. Name plausible wrong implementation that would pass and contract-permitted change overspecified assertion would reject; reasoning may suffice where sensitivity clear.
2. Check accepted controls, exact identity/order/multiplicity, forbidden effects, intended failure stage. Automation confinement requires valid opened document/path-specific refusal. Harmless pre-resolution failure is no security evidence.
3. Inspect [native execution accounting](references/evidence.md): planned versus executed IDs, toolchain/target/features, ignored/filtered tests, doctests, retries, first failures, wrapper exits, artifact hashes. Successful record validator checks supported consistency, not assertion adequacy/automatic product qualification.
4. Load [Rust workflow](references/rust-workflow.md) for Cargo admission, compile controls, test-layer questions; [architecture](references/architecture.md) only for dependency/caller claims. Current layering permits some automation dependencies below engine; documentation mismatch is no automatic forbidden-edge defect.
5. Generated/property/model/mutation claims: consult [generative methods](references/generative.md). Confirm independent expectations, replay, finite scope; equal killed mutants do not justify deleting different contracts. No full mutation campaign follows from review.
6. Review [tool constraints](references/tooling.md) for claims depending on optional integrations. Search/index absence, unavailable Rust/native UI/browser execution remain explicit gaps. Native source/Git review is fallback with narrower verdict.

Separate introduced defects, pre-existing blockers, evidence gaps, policy questions. Existing early failed-edit regression does not settle late refresh/history recovery; source ordering alone cannot prove runtime corruption. Stop only conclusion missing authority/identity/environment.

Output findings: location, mechanism, requirement, raw evidence, uncertainty, next owner. State exactly which review objective succeeded, which product gates incomplete. Reviewer agreement adds no execution; tests-only RED may be useful completed testing deliverable.

Author: David Osipov; personal@david-osipov.vision; [website](https://david-osipov.vision). Original skill material: MIT; see [licence and notices](THIRD_PARTY_NOTICES.md).
