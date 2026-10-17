---
name: pdfcraft-qa-security-boundaries
description: Use when PdfCraft verification concerns filesystem read/write authority, permissions, nested automation dispatch, script effects or opt-in control boundaries.
---

# Protected effects and authority

Use for named authorization/trust invariant and actual enforcement point. Initiates no general security audit, server, exploit campaign or tool installation. Native document permission checks, filesystem roots and browser-host policy remain separate boundaries.

Read [task contract](references/contract.md), [authority/resource reference](references/security-resources.md) and [transaction observations](references/transactions.md). Receive rule, subject, protected object/effect, actual configuration, entry paths, permitted local writes, fixture rights and finite cases/time. Reread current AGENTS.md, relevant automation/root/permission code and consumers. Tool annotations, copied policy strings and schemas show intent; cannot prove enforcement.

Draw caller → direct or nested dispatch → document lookup → permission/path resolution → effect. Put independent witness on protected side. Ensure document, input and prerequisites valid before challenged guard. Pair denial with useful legitimate operation so unconditional rejection fails test. Inspect returned data plus absence of unauthorized read/write, mutation, publication or network effect at exercised layer.

Example: disjoint owned read/write/outside directories; open valid synthetic document through granted read root, save into write root. Then attempt save-in-place into read-only root and outside export. Verify correct refusal, unchanged bytes, no staging residue; perform subsequent allowed save. Repeat relevant routes through `command_run`/`command_batch` and compact `tool_call` if change reaches them. Current automation-root configuration: omitted access kind grants none of that kind; using neither root disables confinement feature. Do not conflate with legacy `--root` path.

Include OS-specific sibling-prefix, traversal, symlink/junction and Windows device/share cases only in owned fixtures on appropriate host. Current check/open resolution not hardened against another process swapping directories. Report explicit limit; do not promote to reproduced vulnerability. Encryption authentication/permission and signed rewrite controls require real document state; wrong password error before intended operation supplies no permission evidence.

Bound hostile payloads/resource use. Stop dependent execution if protected effects cannot be isolated or observed. Missing optional security tools: source tracing/native tests. Absent native/browser capability leaves enforcement claims unrun. Do not weaken guard or start opt-in endpoints to complete checklist.

Return rule/configuration and source identities, legitimate/denied controls, actual guard reachability, protected-side observations, recovery, selected/executed checks, substitutions and cleanup. Keep security conclusions proportional to evidence.

Miri and sanitizer passes: execution-limited evidence; state toolchain, instrumentation and FFI/platform gaps. Keep native semantic controls and hostile-input budgets independent. Read relevant tool-specific section in [security resources](references/security-resources.md).

Author: David Osipov; personal@david-osipov.vision; [website](https://david-osipov.vision). Original skill material: MIT; see [licence and notices](THIRD_PARTY_NOTICES.md).
