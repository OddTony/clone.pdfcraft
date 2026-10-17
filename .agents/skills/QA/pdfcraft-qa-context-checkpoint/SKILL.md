---
name: pdfcraft-qa-context-checkpoint
description: Use when PdfCraft verification must survive interruption, handoff or context reduction while preserving evidence identity, owned resources and consumed budgets.
---

# Save and resume verification

Use for coordinated work, interruption, actual continuation boundary. Small completed source question needs no checkpoint ritual. Checkpoint writing does not compact conversation.

Read current repository instructions and [scope contract](references/contract.md). Inputs: objective/authorization, accepted plan, worker results, relevant source/configuration/fixture/artifact identities, open findings, owned resources, cumulative budgets. Follow [evidence rules](references/evidence.md) for status/provenance; [tooling constraints](references/tooling.md) only where host capabilities matter.

1. Reconcile deliverables with existing files/raw reports. Preserve first failures, retries, incomplete required gates, known external effects. Worker summaries alone insufficient. Separate source-only proposals/executed checks.
2. Record every live owned worker/process and writable resource, including checkout, `CARGO_TARGET_DIR`, temporary PDFs, CLI output, UI control file, browser profile when used. Record enforced/assumed isolation. Never include control token/secret value.
3. Write objective, stable task/check IDs, completed artifacts/applicability, unfinished work, findings, resource disposition, consumed/remaining time/cases/retries, next exact owner/action. Coordinator serializes shared checkpoint updates; workers write separate evidence.
4. Publish durable checkpoint via owned temporary file and same-filesystem replacement where available; otherwise record weaker serialized-write method. Preserve earlier records when correcting report.
5. Use compaction operation only if actually exposed and needed; record performed only with confirmation. If unavailable, preserve checkpoint, continue safe independent work; no compaction request after every phase.

On resume, reread current authorization/applicable instructions; compare relevant hashes/actual resources before restarting. Example: checkpoint says engine regression passed, but `crates/engine/src/lib.rs` changed afterward. Preserve historical run, invalidate affected verdict, rerun only necessary evidence on new state. Unchanged HEAD does not settle dirty files/fixtures.

Resume first incomplete task without replenishing budgets or redispatching completed work. Investigate unknown save/publication outcome from existing artifacts before repeating. If ownership, state, effect outcome unreconstructable, stop action; name missing fact. Output checkpoint location, reusable evidence, pending gates, resources, next action.

Author: David Osipov; personal@david-osipov.vision; [website](https://david-osipov.vision). Original skill material: MIT; see [licence and notices](THIRD_PARTY_NOTICES.md).
