---
name: pdfcraft-qa-property-based-testing
description: Use when a PdfCraft contract supports an independent invariant over generated values and shrinking can reveal cases beyond selected examples.
---

# Generated invariants

Use when domain, property and meaningful failure independently stateable. Few exact boundaries may need only table; no defensible invariant means not applicable. Technique chooses no integration layer and cannot prove all PDF behavior.

Read [task contract](references/contract.md), [generation and replay reference](references/generative.md) and [PDF oracle limits](references/pdf-fidelity.md). Receive boundary, invariant/preconditions, plausible violating implementation, authorized test files, source/lock/fixture identities and finite budget. Inspect existing crate dev-dependencies, proptest configuration/tests. Filters/COS/crypt already use proptest; recipe mention does not automatically add dependency to engine.

State quantified claim before strategies. Generate relational values together; bound byte size, dimensions/depth, cases, rejects, shrink work and total runtime. Avoid filtering almost all samples. Add explicit boundary/malformed/extreme examples randomness unlikely to reach. Create/dispose fresh mutable session/model/files inside every predicate execution, including shrinking/replay; await all owned work. Test-level fixture reset insufficient.

Example: output-limit contract in `pdfcraft-filters`; short hand-computed ASCIIHex vectors, useful accepted value and caps below/at/above decoded length. Complement existing encode/decode properties with independently defined expected byte vector. Shared encoder/decoder defect could pass round trip; pair with explicit semantics or approved external oracle where relevant. Do not import another PDF implementation's source for expectations.

Use small effective proptest configuration after checking current support; 32 cases and 256 shrink iterations can be smoke experiment, retaining exact named extremes and outer deadline. Smoke settings not permanent gate reductions. Confirm intended nonempty tests actually execute; generated samples/discarded candidates are not assertion coverage.

Preserve original/minimized bytes or values, seed, effective options/version, first failure and relevant schedule. Replay concrete minimum alone in fresh state; retain fixed regression for real defect. Classify wrong property, generator/reset leak, product contradiction or unresolved cause before changes. Stop shrinking if cleanup fails unless demonstrably new isolated instance available.

Missing Rust or optional generation tooling: deterministic native case design/source review; runtime unrun. Do not add dependencies or weaken property for green. Report selected/executed cases, rejects/shrink attempts, invariants/controls, exact replay artifacts, resource/cleanup outcomes and remaining oracle limits.

Persisted seeds depend on current strategy; cannot replace concrete deterministic regression. Count replay, rejected cases and shrinking in total budget; cases alone insufficient. Read relevant tool-specific section in [generative](references/generative.md).

Author: David Osipov; personal@david-osipov.vision; [website](https://david-osipov.vision). Original skill material: MIT; see [licence and notices](THIRD_PARTY_NOTICES.md).
