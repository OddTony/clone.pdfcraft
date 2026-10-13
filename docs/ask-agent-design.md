# Ask Agent: selection requests to the user's existing agent

> **Status:** design only, not implemented. **Reviewed:** 2026-10-13.
> Prerequisite: the connected MCP bridge in [mcp.md](mcp.md#connected-mode-the-open-window).
> This document specifies stage (a); it does not claim a shipped chip or request tool.

## User interaction

Select text, a form field, or a page, then choose **Ask Agent** beside that selection. The chip
uses PdfCraft's existing theme tokens, fonts and button sizing; no new design system or assets.
A text label and keyboard focus ring are required. A context-menu entry gives the same action.
If no agent is listening, the chip explains how to connect; it must not pretend work has started.
Do not assign a shortcut before checking the registry and native platform shortcuts.

The anchored bubble contains a short instruction editor, Send, request status and the response.
It stays within the visible viewport and never covers the selected field or keyboard focus.
A keyboard-accessible document status row exposes pending requests when their selection is off
screen or the tab is inactive. Document/request names use actual titles, not unrelated issue ids.

Sending snapshots the target and returns focus to the document. The bubble closes when the person
resumes typing or clicks back into the document; the request remains available in the status row.
Escape dismisses only the focused bubble. It does not reject a proposal, cancel a request or
consume Escape meant for another dialog/keytip. Explicit Cancel request and Discard proposal
are separate actions. Closing a bubble never throws away a response or changes the PDF.

Compute selected text, UTF-8 lengths, geometry and request titles once at selection/send changes.
Render a small cached status summary; do not clone the whole queue or count text each frame.
Snapshots come from engine text/field data, never from screenshots or image calculations.
Text in a document is untrusted data, not instructions for the agent.

## Protocol contract

Add the following authenticated control methods, compatible in purpose and naming with WordCraft:

| Method | Input | Result |
|---|---|---|
| `agent.listen` | `{agent?, timeoutMs?}` | `{request: {...}}`, or `{none: true}` on timeout |
| `agent.reply` | `{id, text, done?}` | `{ok: true, cancelled: false}`, or explicit stale/cancelled error |

`timeoutMs` defaults to 30,000 and is capped at 60,000. `text`/instruction are capped at 4,000
characters; selection text at 100,000 UTF-8 bytes, cut only at a character boundary with an
explicit `truncated` flag. Keep at most 20 retained requests; never evict active requests to
make room silently. Reject a new request if the bounded active queue is full.

The request includes `id`, `instruction`, `document: {doc, name, generation}`, `scope`, `selection`
(text and page/ranges), optional `field` (name, page, type), and optional `action`/`apply`.
No passwords, private key paths, PKCS #12 bytes or reusable signature images enter this envelope.
Stage (a) responses are explanatory text only; mutating proposals are introduced by stage (b).

Connected MCP adds `wait_for_request {timeoutMs?}` and `reply_to_request {id, text, done?}`.
Headless MCP returns an actionable error explaining `--connect FILE`; these tools are not silently
mapped to a second headless document session. Compact `tool_call` must discover and invoke them
with the same validation. Selected text is sent only in an explicit request, not `ui_inspect`.

Implement the queue/hub in ui-egui behind `Arc<Mutex<_>>` plus `Condvar`. Handle `agent.listen`
on the authenticated connection thread in `control::serve`, never in `Control::tick`. The UI
must keep drawing and typing while listen waits; locked-window listen may return a timeout.
Check peer liveness periodically while waiting, and before delivering a request. Lease ownership
is per authenticated connection, not a self-declared agent name. A failed write/disconnect requeues
the request, subject to the generation check; late replies from a former owner are refused.
The MCP bridge uses no request replay after a lost reply (stage 0 transport policy).

## Presence, requests and edits

| State | Entered by | Allowed transition |
|---|---|---|
| waiting | person clicks Send | leased, cancelled |
| leased | live listener takes request | answered, waiting after lease loss, cancelled |
| answered | owner sends `done:true` | retained for display, dismissed explicitly |
| cancelled | person cancels / closes target / target becomes stale | terminal; agent cannot edit it |

Presence lasts at most 60 seconds since a live listen/progress heartbeat, using the same value
in the code, docs, tooltip and tests. A working request does not extend presence to ten minutes.
`done:false` updates progress/lease heartbeat; `done:true` closes ownership. Check the connection
on a bounded cadence (at most one second), rather than only when a new request arrives.
Treat reconnects and multiple agents independently; one request has one owner and one completion.

A request pins the document id and edit generation. Replies refer to that id, never whichever
selection/tab is now active. Before any stage (b) application, validate document existence,
generation, text/field target and original proposal author/request id again.
An answered proposal needs no active agent lease; the person's native Apply click authorizes it. A changed target is stale and needs a new proposal;
do not silently apply against moved page/object indexes. Never switch the person's active tab.

## Implementation map and acceptance tests

1. `ui-egui/src/ask_agent.rs`: bounded hub, cached selection, chip/bubble/status row and focus rules.
2. `ui-egui/src/control.rs`: authenticate before hub access; connection-thread wait/reply with
   disconnect/lease handling. No model client, credential store or outbound network.
3. `automation/src/mcp.rs`/`window.rs`: request-tool schemas and relay; CLI connection uses the
   existing file/token transport. No provider-specific channel is required for stage (a).
4. Tests: tools missing on main; waits don't block egui frames; wrong token rejected before a
   request is delivered; disconnect before/after delivery; timeout and heartbeat; two listeners;
   stale document/selection; UTF-8 truncation and full queue; bubble closes on resumed typing;
   Escape in another dialog does not cancel; status visible across tabs; no per-frame selection
   extraction; no signing data in serialized request; no response or PDF lost on dismissal.
5. Stage (a) changes no PDF, hence adds no undo step. Stage (b) mutations must be staged and
   committed as one request transaction; independent human typing has its own undo step.

Reference reviewed locally: WordCraft `origin/feat/ask-agent-review`, `ask_agent.rs`,
`agent_actions.rs`, control server and MCP tools; prior independent review notes highlighted
presence mismatch, per-frame copying, Escape consumption and undo joining with human typing.
These are requirements above, not claims that PdfCraft already implements them.
