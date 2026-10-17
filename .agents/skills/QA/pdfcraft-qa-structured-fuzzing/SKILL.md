---
name: pdfcraft-qa-structured-fuzzing
description: Use when a PdfCraft PDF, filter, script or automation grammar needs bounded valid and invalid generation with raw-input replay and minimization.
---

# Structured input exploration

Use for named grammar/state boundary and defined invariants. Random bytes alone: not coverage-guided fuzzing; source mutation separate technique. No unbounded campaign, corpus download or external scan.

Read [task contract](references/contract.md), [fuzz/replay guidance](references/generative.md), [raw/resource boundaries](references/security-resources.md) and [PDF fixture rules](references/pdf-fidelity.md). Receive actual input representation, accepted scaffold, independent expectations, mutation mechanisms, corpus rights, output owner, aggregate cases/bytes/depth/time/process budget. Inspect existing target harness/child execution behavior before command selection. Missing tool permits no installation.

Prove tiny valid control accepted and reaches target. Generate structured valid family; change one property per causal case: bounded PDF xref/stream lengths, indirect cycles, filter parameters, truncated tokens, malformed bytes, script nesting or linked command identities. Add multi-fault combinations deliberately. Feed exact bytes/text through real decoder/parser; reserialization may remove duplicate members or repair malformed encoding. Assert accepted semantics or prescribed rejection/repair plus forbidden effects; survival alone insufficient.

Bounded example: existing filter decode seam, short accepted ASCIIHex vector, odd/truncated variants, output caps around decoded size. Retain exact filter/parameter/header/body representation. Whole-document lifecycle: existing `xtask fuzz` sends mutated inputs through `check-one --edit` children. Proposed isolated smoke: `cargo xtask fuzz --time 30 --iterations 25 --jobs 1 --timeout 2 --seed 1`, only after command/build review, available approved Rust/caches and outer total/process-tree budget. Builds release CLI, writes findings; neither read-only nor hard 30-second deadline.

Record ordinary rejection, crash, hang, spawn/wait failure and cleanup separately. Current fuzzer exit policy: crashes fail, hangs reported; new hang must stay visible even at exit zero. Source-observed accounting concerns: not reproduced runtime defects. Confirm attempted children actually execute. Linux/FreeBSD address-space limits cannot qualify Windows/macOS limits; detached cargo-fuzz harness has separate platform/nightly prerequisites.

Minimize with anomaly/failure-mechanism-preserving oracle; fresh state per probe. Keep first/minimized input/hash, operator, seed/options/tool versions, reached branch, response/resource evidence. Stop on uncontrolled effects, reset failure or exhausted budget. Replay concrete minimum alone. Missing execution capability: source-backed proposal, explicit unrun cases; no suppressed payload or automatic corpus approval for green.

Distinguish tmin failure-input minimization from cmin corpus reduction; preserve originals before either. Verify selected release/host toolchain: current cargo-fuzz README and Windows setup documentation disagree. Read relevant tool-specific section in [generative](references/generative.md).

Author: David Osipov; personal@david-osipov.vision; [website](https://david-osipov.vision). Original skill material: MIT; see [licence and notices](THIRD_PARTY_NOTICES.md).
