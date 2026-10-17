---
name: pdfcraft-safe-filesystem-persistence
description: Use when PdfCraft changes save, export, recovery, staging, overwrite conflict, or automation path-authority behavior on a filesystem.
---

# Filesystem persistence design

Use for filesystem write/authority boundary. Read-only metadata query: non-trigger. Read [task contract](references/contract.md), [save transactions](references/transactions.md), and [security/resource guidance](references/security-resources.md). Obtain operation, destination authority, preexisting bytes, platform/filesystem, conflict policy, current save mode, allowed changes, owner, bounded fault cases. No transfer of native atomic-save guarantees to browser downloads/host-message dispatch.

Trace path validation/each phase separately: optional compound edit, prepare, `Session::check_save`, create-new same-directory staging, write, checked sync, close, rename/publication, `mark_saved`, refresh/report, recovery cleanup. New-path automation saves normally request full rewrite; flatten-fill-sign may change session history before preparation. Record phase-appropriate snapshot; do not assert every error restores pre-call state.

Use existing `pdfcraft_platform::staging::write_atomically_with` seam: deterministic name-collision/refused-sync controls. Automation save writer callback: reached publication tests. Bounded example: tiny valid editable document + existing sentinel destination. Refuse sync after staging write; require unchanged destination, observed staging cleanup, retryable work retained against post-edit snapshot, later successful save. Editable fixture: invalid prepared bytes must fail `check_save` before writer call. No-editor/original-bytes branch: assess separately; `check_save` skips reparse/page checks. Capture fault-hit evidence; invalid document ID cannot test write boundary.

Test rename refusal, destination directory, new/existing target, Unicode/long names, relevant OS sharing rules separately. If replacement succeeds but `mark_saved`/refresh fails: new bytes published; resolve disk/session divergence before retry/overwrite. Do not promise every error leaves dirty true: `mark_saved` changes path/dirty/generation before final refresh. Atomic replacement differs from power-loss durability; file sync cannot prove durable parent-directory rename. Preserve checked macOS fallback; do not describe every unsupported full flush as unsynced success.

Trace every nested/import/export path through automation `resolve` and `file_access::AutomationRoots` with valid document. Distinguish read/write grants, whole-component containment, missing parents, links, reserved Windows names, documented check/open race. Native UI overwrite-change handling: separate consumer contract.

Use installed native tests, owned directories, after preflight. Missing filesystem/OS/fault capability: dimension not run; source analysis/reviewable phase table useful. Never install/weaken confinement to finish. Report hashes/listings, actual reached phases, error/cleanup outcomes, subsequent recovery, platform limits, outstanding durability/authority decisions.

Author: David Osipov; personal@david-osipov.vision; [website](https://david-osipov.vision). Original skill material: MIT; see [licence and notices](THIRD_PARTY_NOTICES.md).
