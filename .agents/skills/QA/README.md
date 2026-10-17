# PdfCraft skill guide

41 skills grouped here for navigation, including verification and Rust/PDF implementation guidance. Pick the smallest set matching the requested work; preserve current repository rules and required gates.

## Quick choices

| Task | Start with |
| --- | --- |
| One known, bounded check | Matching evidence-layer or behavior profile below |
| Missing verification plan | Test planning |
| Several dependent verification phases | Test orchestrator |
| Failing or intermittent check | Failure triage |
| Stage intended changes and audit them before committing | Commit preparation |
| Reconcile final evidence and unresolved findings | Final qualification |

Read each selected `SKILL.md` and the references it calls for. The linked name in its frontmatter is the invocation name; for example, `pdfcraft-qa-prepare-commit` identifies Commit preparation. The directory groups do not impose an execution sequence. Implementation/repair, delegation and tool installation retain the authority boundaries in the selected skill and current task.

## Discovery and use

This README is a navigation reference. Codex's documented discovery starts with skill names/descriptions and paths; the full skill loads when selected. A neighboring README is not a documented automatic instruction entrypoint. Use explicit skill invocation when selection must be precise; implicit selection matches descriptions. See [official skill documentation](https://learn.chatgpt.com/docs/build-skills).

README: navigation. Repository routing: [AGENTS.md §5.1](../../../AGENTS.md#51-repository-skills-and-verification); consult guide when choosing QA workflow. Sync guide with skill metadata; linked skill governs scope/procedure under repository rules.

## Maintaining the portable package

Each skill carries references/notices. Update all same-named reference copies together; keep portable notices identical. After edits run `cargo xtask skills`; Linux CI and `cargo xtask ci` run same package check. Checks: supported plain scalar metadata, skill names, local Markdown links, guide coverage, duplicate references/notices. Unsupported metadata/link formats require explicit validator update. Structural pass proves neither automatic skill selection nor workflow behavior.

## Choose and coordinate work

| Skill | When to use |
| --- | --- |
| [Environment preflight](pdfcraft-qa-environment-preflight/SKILL.md) | A PdfCraft test command, build configuration, tool availability or resource ownership is uncertain before execution. |
| [Contributor tooling](pdfcraft-contributor-tooling-preflight/SKILL.md) | A PdfCraft contributor needs host capability discovery, source-index validation, or a proposal for optional compiler, agent, MCP, or diagnostic tooling. |
| [Test planning](pdfcraft-qa-test-planning/SKILL.md) | PdfCraft testing needs a scoped plan with independent expectations, reachable controls, evidence layers and finite execution budgets. |
| [Test orchestrator](pdfcraft-qa-test-orchestrator/SKILL.md) | A PdfCraft verification assignment spans dependent phases or several authorized workers with shared evidence and resource ownership. |
| [Regression selection](pdfcraft-qa-regression-selection/SKILL.md) | A PdfCraft change needs an economical affected regression slice and explicit accounting for transitive inputs and required wider gates. |
| [Failure triage](pdfcraft-qa-failure-triage/SKILL.md) | A PdfCraft test or gate fails, times out, intermittently passes or contradicts its reports and the first causal failure needs classification. |
| [Independent review](pdfcraft-qa-independent-review/SKILL.md) | PdfCraft tests or verification claims need independent assessment of oracle strength, reachable boundaries, execution and remaining gaps. |
| [Subagent dispatch](pdfcraft-qa-subagent-dispatch/SKILL.md) | Authorized PdfCraft verification work has independent subtasks that require explicit worker, file and aggregate resource ownership. |
| [Context checkpoint](pdfcraft-qa-context-checkpoint/SKILL.md) | PdfCraft verification must survive interruption, handoff or context reduction while preserving evidence identity, owned resources and consumed budgets. |
| [Final qualification](pdfcraft-qa-final-qualification/SKILL.md) | A PdfCraft verification scope is ready for final reconciliation of fresh evidence, required gates, unresolved findings and exact qualification limits. |
| [Commit preparation](pdfcraft-qa-prepare-commit/SKILL.md) | Preparing PdfCraft changes for a commit, selecting staged hunks, or assessing staged readiness while unrelated or partially staged work may exist. |

## Choose the evidence layer

| Skill | When to use |
| --- | --- |
| [Static checks](pdfcraft-qa-static-checks/SKILL.md) | PdfCraft changes need compiler, clippy, formatting, dependency-layer or static gate evidence with explicit target and feature scope. |
| [Unit tests](pdfcraft-qa-unit-tests/SKILL.md) | A small PdfCraft Rust behavior needs focused assertions through a real function or engine seam, including error and preservation cases. |
| [Component tests](pdfcraft-qa-component-tests/SKILL.md) | An egui widget, dialog or PdfCraft shell interaction needs local state and AccessKit semantics checked with the existing Rust harness. |
| [Integration tests](pdfcraft-qa-integration-tests/SKILL.md) | A PdfCraft claim depends on real collaboration among engine, COS writer, renderer, automation or filesystem persistence seams. |
| [Contract tests](pdfcraft-qa-contract-tests/SKILL.md) | PdfCraft Rust APIs, automation schemas, command registries, CLI reports or MCP messages need producer-consumer compatibility evidence. |
| [End-to-end tests](pdfcraft-qa-e2e-tests/SKILL.md) | A PdfCraft claim requires a complete journey through the built CLI, opt-in native UI control or actual browser-WASM application. |
| [Golden fixtures and parity](pdfcraft-qa-golden-fixture-parity/SKILL.md) | PdfCraft output needs comparison with a reviewed semantic, byte or pixel fixture whose provenance and permitted variation can be established. |

## Explore cases and test sensitivity

| Skill | When to use |
| --- | --- |
| [Positive paths](pdfcraft-qa-positive-paths/SKILL.md) | A PdfCraft change needs valid accepted scenarios or useful benign controls across edit, save, reopen and automation boundaries. |
| [Negative paths](pdfcraft-qa-negative-paths/SKILL.md) | PdfCraft must reject an invalid input or state, preserve work, or avoid a forbidden effect at a specific validation boundary. |
| [Boundary and edge cases](pdfcraft-qa-boundary-edge-cases/SKILL.md) | PdfCraft behavior changes at a numeric, byte, Unicode, page-index, depth, allocation or queue threshold. |
| [Adversarial inputs](pdfcraft-qa-adversarial-inputs/SKILL.md) | Hostile or ambiguous PDF, script, path or automation input could confuse a specific PdfCraft trust boundary. |
| [Property-based testing](pdfcraft-qa-property-based-testing/SKILL.md) | A PdfCraft contract supports an independent invariant over generated values and shrinking can reveal cases beyond selected examples. |
| [Model-based testing](pdfcraft-qa-model-based-testing/SKILL.md) | PdfCraft edit, save, undo, redo or lifecycle sequences need comparison with a small independent state model. |
| [Structured fuzzing](pdfcraft-qa-structured-fuzzing/SKILL.md) | A PdfCraft PDF, filter, script or automation grammar needs bounded valid and invalid generation with raw-input replay and minimization. |
| [Mutation testing](pdfcraft-qa-mutation-testing/SKILL.md) | A bounded PdfCraft test-sensitivity question requires selected source faults, causal killing assertions and survivor analysis. |

## Verify a behavior area

| Skill | When to use |
| --- | --- |
| [State, concurrency and lifecycle](pdfcraft-qa-state-concurrency-lifecycle/SKILL.md) | PdfCraft state transitions, render generations, replacement, cancellation, close or worker retirement can interact with stale completion. |
| [Fault recovery](pdfcraft-qa-fault-recovery/SKILL.md) | A valid PdfCraft operation can lose storage, refresh, worker or collaborator support and its partial state and recovery need verification. |
| [Security boundaries](pdfcraft-qa-security-boundaries/SKILL.md) | PdfCraft verification concerns filesystem read/write authority, permissions, nested automation dispatch, script effects or opt-in control boundaries. |
| [Performance and resources](pdfcraft-qa-performance-resources/SKILL.md) | A PdfCraft edit, render, save, cache, worker, or dependency change may affect latency, memory, CPU, disk use, or cleanup. |
| [Visual and responsive behavior](pdfcraft-qa-visual-responsive/SKILL.md) | PdfCraft layout, typography, dialogs, page presentation, scaling, or hit geometry changes across window sizes or rendering environments. |
| [Accessibility](pdfcraft-qa-accessibility/SKILL.md) | A PdfCraft control, keyboard or focus path, assistive-technology interaction, or PDF accessibility behavior changes or needs qualification. |
| [Internationalization and localization](pdfcraft-qa-i18n-localization/SKILL.md) | PdfCraft locale selection, translations, plurals, Unicode text, script rendering, or language-dependent UI state changes. |
| [Build, content and metadata](pdfcraft-qa-build-content-metadata/SKILL.md) | PdfCraft changes PDF Info or XMP, generated reports or catalogues, attributed assets, or the identity of built output. |

## Implement or trace a change

| Skill | When to use |
| --- | --- |
| [Architecture and data flows](pdfcraft-architecture-data-flows/SKILL.md) | Locating PdfCraft callers, tracing document or cache state, assessing change impact, or resolving current versus planned architecture. |
| [Rust implementation](pdfcraft-rust-implementation/SKILL.md) | Implementing or repairing PdfCraft Rust behavior that requires ownership, error, API, type, or module-boundary decisions. |
| [Bounded parser design](pdfcraft-bounded-parser-design/SKILL.md) | Designing or repairing PdfCraft parsing, decoding, graph traversal, rendering, or cancellation limits for untrusted document input. |
| [PDF fidelity](pdfcraft-pdf-fidelity/SKILL.md) | A PdfCraft parser, editor, writer, appearance, redaction, signature, encryption, or interchange change can alter PDF meaning or preserved data. |
| [Safe filesystem persistence](pdfcraft-safe-filesystem-persistence/SKILL.md) | PdfCraft changes save, export, recovery, staging, overwrite conflict, or automation path-authority behavior on a filesystem. |
| [Transactional editing](pdfcraft-transactional-editing/SKILL.md) | Designing PdfCraft edit, undo, redo, dirty-state, generation, password, or save transitions that must preserve a coherent session. |
| [Vendored dependency maintenance](pdfcraft-vendored-dependency-maintenance/SKILL.md) | Updating a PdfCraft vendored crate, local dependency patch, feature graph, lockfile, advisory exception, or upstream patch provenance. |

Author: David Osipov; personal@david-osipov.vision; [website](https://david-osipov.vision). Original guide material: [MIT licence and notices](pdfcraft-qa-test-orchestrator/THIRD_PARTY_NOTICES.md). Prepared with AI assistance.
