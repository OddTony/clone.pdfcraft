# Tab movement commands and cancellation

Status: reduced PR #1997 integration candidate based on upstream `f0c9901c398372782a4eb001c95638289a5edab0`, 2026-10-16. Prepared for a normal update to the existing PR, preserving all published history.

Upstream already provides drag reordering, insertion markers, clipped first-tab feedback, overflow edge scrolling, document identity/state preservation, keyboard tab activation and Ctrl+Tab cycling. This candidate uses that implementation unchanged for placement, feedback and scrolling. It does not add another `move_tab` API or drag model.

## Remaining functionality

- Move Tab Left/Right commands in the existing registry and command palette. Use Cmd/Ctrl+K, search and Enter; no dedicated reorder shortcut is assigned.
- Context-menu actions move the clicked document by DocId, even when another tab is active. Edge actions are disabled; Home and Combine cannot be moved by active-document commands.
- The opt-in authenticated UI control channel can list and execute the same commands. Tab order is window UI state; no separate headless engine/MCP tab model is introduced.
- A transient DocId/cancellation latch supplements upstream's gesture handling. Escape, focus loss or a modal interaction cancels the gesture until release; releasing outside the document strip cannot reorder. The next gesture works normally.

The existing `PdfCraftApp::move_tab(from, to)` retains its unit-returning API and final-index semantics. A private by-DocId helper only resolves an adjacent destination before calling it. No documents are closed/reopened, no PDF bytes are changed and no persistence schema is added. Session reopening already stores the current order; the regression uses native temporary paths so Windows drive/separator normalization is respected.

## Validation plan and limits

Eleven focused regressions cover commands and boundaries, inactive context targets, real keyboard palette access, authenticated control, canceled gestures/recovery after Escape, focus loss and modal opening, source closure, session persistence and unsaved/view/undo state. Six initial regressions failed on unchanged main; source closure and session persistence already passed. Separate modal/focus baselines distinguish additional gaps from existing behavior.

Broader validation includes the complete UI crate (including main's original drag, marker-pixel, cycling, accessibility and Combine tests), engine tests, formatting, strict Clippy, native builds, assets, layers, parity and dependency checks. Final local results: 1,270 UI tests and 283 engine tests passed (eight documented ignores); formatting, strict all-target Clippy, native desktop/CLI builds, assets/layers/parity and cargo-deny passed. Native authenticated command checks and synthetic context-menu/palette previews passed. Raw cargo audit retains only main’s inherited RSA advisory; Cargo.lock is unchanged. The initial boundary prototype failed two existing upstream UI tests; the boundary was corrected and the complete UI suite passed without weakening those tests. Windows, Linux/BSD, WASM, physical input, native titlebar/compositor behavior and screen-reader testing still require platform validation before release. Local synthetic UI previews do not establish those manual behaviors. Hosted CI must validate this normal merge.

## Integration

This reduced implementation is integrated with main through a normal merge on the existing contribution history. Upstream owns drag geometry, placement, feedback and overflow scrolling; the obsolete contribution modules are removed. Escape and focus-loss cancellation are followed by a successful fresh gesture in the regressions. Modal cancellation/recovery and overflow scrolling have explicit coverage too. Physical input and hosted platform validation remain required.

## Revision history

- 2026-10-16: reduction of #1997 to explicit commands and cancellation guards, retaining upstream drag placement and existing published history.
