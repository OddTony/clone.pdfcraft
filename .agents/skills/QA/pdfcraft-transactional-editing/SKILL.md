---
name: pdfcraft-transactional-editing
description: Use when designing PdfCraft edit, undo, redo, dirty-state, generation, password, or save transitions that must preserve a coherent session.
---

# Transactional editing design

Use for multi-state edit/recovery contract. Pure read-only query without transition: non-trigger. Read [task contract](references/contract.md), [transaction reference](references/transactions.md), and [PDF invariants](references/pdf-fidelity.md). Obtain operation, applicable permissions/signed restrictions, complete logical state, intended success/error guarantees, current implementation, allowed patch, bounded fault plan. Name contract ambiguities before strengthening product rollback promise.

Inventory graph, saved base, working/display bytes, keys/passwords, undo/redo availability/labels, dirty flag, generation, derived lists/caches, renderer/workers, path, recovery state. Distinguish prepared data, committed graph/history, refreshed view, published file, acknowledged save. Choose candidate/validation/commit arrangement avoiding fallible work after irreversible transition where practical; preserve existing public behavior/unknown objects. Make every remaining late failure's partial outcome explicit.

Current `Session::apply`: inspect candidate graph preparation, history changes, refresh, rollback; inspect undo/redo order separately. Bounded example: valid edited document, nonempty redo stack, near-capacity undo history; specify reached refresh-failure control/healthy counterpart. Compare all observable state with relevant phase snapshot; require successful subsequent action. Source ordering: hypothesis to reproduce, not verified rollback defect. Missing document error cannot exercise refresh recovery.

Saving: phase-specific guarantees. Automation `doc_save` with `flatten_fill_sign=true` applies undoable flatten edit before serialization; later save failure need not restore pre-call graph. Prepared bytes pass `check_save` before native publication. Successful replacement then `mark_saved`/refresh failure already leaves new disk bytes; observe dirty/path/generation separately. Browser download/host-message dispatch has own acknowledgment contract.

Preserve permissions, encryption/reopen keys, signed-byte restrictions, full-save triggers, undo across save. Use deterministic existing callbacks/seams where available; new product injection point requires authorized implementation scope. Observe fault-hit evidence, destination state, native error, cleanup before teardown hides them.

Use focused native tests/explicit state snapshots. Model, `Result`, caught panic or parse success alone cannot prove atomicity. Missing execution/fault injection: precise phase table/test design, runtime claims not run; continue independent source work. Report chosen guarantee, current/proposed ordering, healthy/wrong cases, first failure, recovery, unresolved owner decisions.

Author: David Osipov; personal@david-osipov.vision; [website](https://david-osipov.vision). Original skill material: MIT; see [licence and notices](THIRD_PARTY_NOTICES.md).
