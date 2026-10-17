---
name: pdfcraft-qa-contract-tests
description: Use when PdfCraft Rust APIs, automation schemas, command registries, CLI reports or MCP messages need producer-consumer compatibility evidence.
---

# Test an interface contract

Use for defined boundary/compatibility direction. Separate schema shape, compile admission, runtime semantics claims. Contract test alone cannot establish complete saved-document journey.

Read current rules and [scope contract](references/contract.md). Inputs: producer/consumer, authoritative requirements, versions, relevant source/config/fixtures, permitted writes, budget. Inspect `crates/automation/src/tools.rs`, `Automation::call`, consumers, existing tests; current producer snapshot is no independent oracle. Consult [Rust workflow](references/rust-workflow.md) for compiler/runner controls.

1. List required/optional/default/unknown arguments, representations, page/index bases, command links, error categories, result semantics. Tools use 1-based pages; engine `Edit::RotatePages` takes zero-based indices. Specify conversion independently.
2. With valid opened two-page synthetic PDF, call `page_rotate` for page 1 and 90 degrees. Assert only first page rotates, returned summary agrees. Pair page 0, unknown argument, non-multiple-of-90 controls with intended `ToolError::InvalidArgs` cause. Invalid document must not hide tested boundary.
3. Review `tool_table_is_well_formed` for unique names, schema membership, command links, incompatible flags. Checks table structure, not every tool's advertised behavior. Pair selected schema checks with actual admitted calls.
4. Real compile-time API claim: use owned rustdoc/compile specimen with legitimate compiling companion. Passing `usize` where `Session::apply` requires `DocId` must fail for type mismatch, not missing import. `DocId` has public tuple field; no claim of enforced document existence/permissions. Runtime validation still necessary.
5. CLI/MCP contracts: keep raw messages where malformed JSON/encoding matters; distinguish JSON-RPC errors, `isError` tool results, content serialization, action success. MCP handshake supplies protocol evidence only; never enable listener/register server by default.

Execute finite existing test selection; retain [native evidence](references/evidence.md): intended diagnostic/error, accepted control, exact bytes/hash, target/features, first failure, exit, selected/executed identities. Missing optional Inspector/trybuild blocks neither ordinary native tests nor source review; no install follows. Stop claims lacking real consumer/required runtime; label unproven direction explicitly.

Use trybuild only for meaningful diagnostic contract with admitted companion. Scope optional semver checks to explicit library baseline, version, target, supported features; confirm important changes with real consumers. Read relevant tool-specific section in [rust workflow](references/rust-workflow.md).

Author: David Osipov; personal@david-osipov.vision; [website](https://david-osipov.vision). Original skill material: MIT; see [licence and notices](THIRD_PARTY_NOTICES.md).
