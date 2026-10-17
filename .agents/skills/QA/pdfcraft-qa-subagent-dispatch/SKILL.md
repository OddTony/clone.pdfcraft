---
name: pdfcraft-qa-subagent-dispatch
description: Use when authorized PdfCraft verification work has independent subtasks that require explicit worker, file and aggregate resource ownership.
---

# Dispatch bounded work

Use only with user/applicable project delegation authority and useful independence. No workers merely because skill available. Small dependent sequence often better direct.

Read current instructions and [scope contract](references/contract.md). Inputs: accepted objective, frozen source/configuration/fixture identities, selected scenarios, allowed effects, available worker tools, remaining aggregate budget. Consult [tooling](references/tooling.md) only for considered capabilities.

1. Partition by causal boundary. Example: one read-only reviewer inspects engine rotation/history assertions, another automation page conversion on same frozen source. Two writers of same fixture/registry require serialization.
2. Inventory shared mutable resources beyond filenames: Cargo target output, caches, temporary directories, generated files, fonts, CLI output, UI control files, ports, process trees. PdfCraft calls for separate worktrees and `CARGO_TARGET_DIR` per agent; account for shared storage/machine capacity too.
3. Single writer or demonstrated isolation per resource. Prose allocation not lock. Bound build jobs/test threads separately; simultaneous worker demand must fit coordinator's total budget.
4. Brief: one objective, relevant contract/source paths, owned files, excluded actions, input barrier, independent expected result, accepted/wrong controls, cases/deadline/retry limits, output location. Tests-only worker preserves confirmed regression; no product behavior change. Independent reviewer gets raw question, not desired verdict.
5. Register ownership before dispatch. Use exposed host-native tools; no orchestrator installation or invented compaction API. Begin dependent work only after inspecting/accepting input artifact.
6. Review worker files and [execution evidence](references/evidence.md); distinguish reported/directly observed runs; integrate under one owner. New shared dependencies, collisions or relevant source changes: stop affected task for coordination.

Unavailable delegation tools/enforceable isolation: same scoped tasks sequentially; record limit. Join/stop only identified owned work. Close with every worker/task disposition, accepted artifacts, first failures, remaining budgets, cleanup, next owner. Coordinated work requires durable checkpoint; worker agreement adds no test execution.

Author: David Osipov; personal@david-osipov.vision; [website](https://david-osipov.vision). Original skill material: MIT; see [licence and notices](THIRD_PARTY_NOTICES.md).
