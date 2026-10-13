# Document tab reordering

Status: implemented on the local `feature/tab-reordering` branch; macOS control-driven smoke tests passed with wgpu and Glow; other platforms and physical input still need validation. Last reviewed: 2026-10-13.

Drag a document tab horizontally to show its insertion position. The tab order changes only when the primary pointer is released inside the visible document-tab region. Escape, loss of focus, a modal interaction, or release outside that region cancels the move. Moving outside the strip does not create a window. Close buttons keep their independent click behavior.

Overflow tabs retain their scroll arrows. Holding a dragged tab near either visible edge scrolls the strip at a bounded speed. Home and Combine Files stay in their existing positions and cannot be reordered as document tabs. Feedback is an immediate accent-colored insertion line; no animation is needed, including with Animation effects turned off.

Right-click a document tab for **Move Tab Left** or **Move Tab Right**. The target is the clicked tab even when another document is active. An unavailable edge action is disabled. Keyboard users can open the command palette (Cmd/Ctrl+K), search for either command, and press Enter to move the active document. Existing tab focus and page-view history shortcuts are retained; no conflicting default reorder binding is introduced.

## Placement API and state ownership

`PdfCraftApp::move_tab(from, to)` uses zero-based source and **final destination** indices. Invalid indices and no-op moves return `false` without mutation. `move_tab_by_id(id, delta)` accepts `-1` or `1` and resolves the source by its stable document ID.

Both operations move the original `DocView` value within `views`. They preserve active selection by DocId (or Home/Combine's `None`). They do not touch engine Session ordering, PDF content, undo/redo, recovery, closed-tab history, view scroll identity, or rendering caches. Document IDs remain runtime session identities, not persistent cross-process identifiers.

Drag intent is transient egui state: source DocId and candidate final index. Geometry comes from the actual rendered tab rectangles after scrolling/compression. Rendering computes feedback; placement is applied after the strip has finished rendering, so its iteration is never invalidated by an action. The source remains in its original slot throughout the preview.

## Headless/control access

The shared command registry contains `tab.move_left` and `tab.move_right`. Existing opt-in UI control exposes them through `ui.commands` and `ui.command`:

```text
pdfcraft --control /private/tmp/pdfcraft-control.json first.pdf second.pdf
pdfcraft-cli ui --control /private/tmp/pdfcraft-control.json command id=tab.move_left
pdfcraft-cli ui --control /private/tmp/pdfcraft-control.json state
```

The control listing reports whether a command is enabled, and disabled commands return errors. The public placement API can be tested without rendering. UI tab order is not an engine document operation: the standalone `pdfcraft-cli run`/MCP Session has no window or tabs, so no engine tab reorder tool is added. The appropriate live-window automation boundary is the existing authenticated, opt-in UI control channel. No listener/security/launch behavior changes are introduced.

Optional last-session reopening already serializes `views` in their current order; this change needs no persistence schema or new always-on path storage. It does not add detachable tabs, a window registry, multi-window transfer, or durable undo restoration.

## Validation and remaining manual checks

Focused placement, command/control and real-widget pointer tests cover active/inactive moves, boundaries, state preservation, cancel/release behavior, overflow, close-button independence, closing or switching tabs during a drag, first/last placement, empty/single-tab behavior, native titlebar gestures, special workspaces, keyboard command invocation and light/dark feedback with animation disabled. See `crates/ui-egui/src/tab_order.rs`, `tab_drag.rs`, and `crates/ui-egui/tests/tab_reordering.rs`.

Native macOS smoke tests through the actual authenticated control channel passed with wgpu and Glow: drag/commands, outside cancellation, active identity, unsaved edits, undo/redo, reduced motion, screenshots, save and quit. After a normal merge of upstream `b5467d93` (native-open/password-prompt fixes), the full scoped UI/engine suite passes: 1,363 passed, zero failed, eight ignored; all 26 added tests pass. Formatting, strict Clippy, native desktop/CLI builds, layering, assets and parity checks pass. WASM compilation remains unavailable because the target standard library is missing.

Before release, manually check physical input on macOS and Windows/X11/Wayland: real native titlebar move/double-click/resize, compressed and overflow tabs, mixed scaling, keyboard/screen-reader focus, and both wgpu/Glow. Headless pointer tests establish widget behavior; they do not establish native compositor or screen-reader behavior.

## Revision history

- 2026-10-13: initial single-window placement API and interactions; multi-window work remains separate.
