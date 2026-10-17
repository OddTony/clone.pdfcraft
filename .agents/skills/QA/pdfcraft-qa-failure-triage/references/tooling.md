# Contributor capability and integration decisions

Inspect actual task host; do not dump environment values, credentials, account selectors or broad personal configuration. Record platform/architecture, current source state, available shell/Git, Rust/Cargo toolchain/target, existing runner/oracle/browser/GUI capabilities and actual agent tools. Wrapper skill, exposed tool, executable on PATH, healthy daemon and qualified project workflow: separate states.

Standard workflow: current source, Git/ripgrep, approved compiler/Cargo, existing xtask checks. Read callers, consumers, feature gates, generated/vendor paths. Search miss cannot prove absence; stale index cannot prove current source. Compiler absent: finish independent source work, mark execution blocked. No automatic toolchain installation or dependency fetching.

## Semantic navigation

rust-analyzer can add typed navigation after reviewing build scripts, proc macros and Cargo execution settings. Its [security guidance](https://rust-analyzer.github.io/book/security.html) explains execution boundary; disabling build/proc-macro execution reduces semantic coverage. Record selected features/target; verify important relationships in current source.

[SocratiCode](https://github.com/giancarloerra/SocratiCode/tree/eb94e073cb8aa80d08fd56c8aecfe7f2fa065be8): optional. Observed engine/npm version1.16.1 declares AGPL-3.0-only; external operation and copying/distribution of its code or instruction assets need separate policy/rights decisions. Do not copy implementation or bundled skills into permissive target. Documented local stack may pull Docker images/models and persist indexes; remote embedding/Qdrant adds egress. Assistant receiving search results: another data boundary.

Startup can auto-resume indexing; status/search/graph interactions can start watchers for existing index. Future explicitly approved snapshot needs SOCRATICODE_WATCHER=off and SOCRATICODE_AUTO_RESUME=off on every sharing process, exact project scope/providers/model/container pins and owned storage. Do not call status as inert preflight in unknown setup. Rust macros, generated includes, cfg variants and trait dispatch limit graph accuracy; compare synthetic known-edge set against current source/compiler evidence.

If plugin-prefixed and standalone SocratiCode tool namespaces both actually appear, report duplicate; propose removing standalone registration. Plugin skill entry alone cannot prove duplicate server.

## Documentation and repository tools

Context7 can retrieve public dependency documentation; confirm returned primary API at target version. Queries must exclude private source/PDFs without separate authorization. Existing GitHub reads can pin commits/inspect public source. Exposed write tools do not authorize publication; unknown connector runtime/scope stays unknown.

Defer extra integrations unless measured capability gap justifies them. Serena's inspected v2 application: GPL-3.0-or-later; Atlas Scout Preview29: proprietary, Windows artifact unsigned. Wrappers cannot prove runtime availability or permission. MCP Inspector can test protocol/schema behavior after approved installation; cannot replace document, confinement or recovery tests.

## A reviewable installation proposal

State concrete task gap, exact package/toolchain version/source, verified artifact integrity (or unresolved trust gap), licence, prerequisites, hooks/build execution, filesystem privileges, network/data flow, CPU/RAM/disk limits, bounded synthetic smoke test, rollback and no-tool fallback. Missing integrity details keep proposal conditional. Leave unexecuted until authorized. Rust1.95.0: MSRV baseline candidate from pinned target, no newest-stable claim. Check current official [Rustup guidance](https://rust-lang.github.io/rustup/) and actual Cargo/CI context at installation time.

Refusal: continue source/Git/manual work. Missing optional tool never justifies global settings changes, cloud source indexing, new product network path or passing qualification claim.
