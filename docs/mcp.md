# MCP conventions

Start explicitly with `pdfcraft-cli mcp [--root DIR] [--compact]`. The server uses JSON lines on
stdin/stdout, opens no port, and stops on EOF. `--root` confines all reads and writes, including
commands invoked through the common tools. Logs stay on stderr.

Supply `--root` at most once, followed by a nonempty directory path. Missing operands,
repeated roots and another long option in place of the directory are refused before any
script is read or server is started. Prefix a directory name beginning with `--` with `./`.

| Tool | Arguments |
|---|---|
| `command_list` | `doc?`, `filter?`, `enabled_only?`; returns `{commands: [...]}` with mapped tool schemas |
| `command_run` | `id`, `params?` (the mapped task tool's arguments, including `doc` when required) |
| `command_batch` | `steps: [{id, params?}]`, `stop_on_error?` (default true) |
| `doc_inspect` | `doc?`; one document's info or `{documents: [...]}` for the session |
| `render_preview` | `doc?`, `page?` (1-based, default 1), `max_side?` (1–4096, default 1024) |

All existing task tools remain available. Compact mode lists these five tools alongside its
existing core set and `tool_search` / `tool_call`. There are no legacy-name aliases. The full
[automation guide](../crates/automation/README.md) describes the tools, resources, coordinate
system, root confinement, and errors.

```text
command_run   {"id":"file.open","params":{"path":"input.pdf"}}
              → {"doc":1,...}
command_list  {"doc":1,"filter":"rotate"}
command_run   {"id":"page.rotate","params":{"doc":1,"degrees":90}}
render_preview {"doc":1,"page":1,"max_side":512}
```

Every tool has a title and the four MCP annotation hints. Unknown top-level argument keys return
JSON-RPC `-32602`; command parameter keys that do not belong to the mapped task tool are ignored
with warnings. Task failures and escaped tool panics return `isError: true`, and the server keeps
serving. A failed batch includes per-step errors and is marked `isError: true`.

Exports complete synchronously. A call's `progressToken` and cancellation notifications are
harmlessly ignored; the server does not provide background export progress or cancellation.

## Resources and protocol versions

`pdfcraft://document` returns the session's document information (as `doc_inspect` without
arguments), including an empty `documents` array before any PDF is opened. `pdfcraft://commands`
returns the command catalog (as `command_list`). Existing per-document info, text and page-image
resources and their templates are preserved.

Unknown top-level argument keys are refused with JSON-RPC `-32602` (invalid params) instead of
being ignored, so a misspelled argument is reported rather than silently dropped. Clients that
sent extra keys before get an error now.

## Connected mode: the open window

`pdfcraft-cli mcp --connect FILE [--root DIR] [--compact]` works on the desktop app started with
`pdfcraft --control FILE` instead of a session of its own:

- **Every tool and resource acts on the window's documents.** Each call goes to the window's
  control method `tool.call {name, arguments, root?}`, which runs the same tool table on the
  window's session between frames, like a menu command, and draws what changed. `doc` ids are the
  window's (`doc_list`, `ui_state`); a document a tool opens or makes gets a tab behind the
  person's. `--root` confines paths as it does headless.
- **Read-only window inspection**: `ui_state`, `ui_inspect` and `ui_screenshot` (an image) are
  listed in normal and compact mode. MCP inspection uses the protected `tool.inspect` boundary
  rather than the existing trusted UI driver's `ui.inspect`. MCP does not expose UI input or menu commands: those could
  bypass path confinement or make a signature without the person's click. Use document tools
  for edits. Inspection and captures are refused while a signing or password dialog is open.
- **Left to the person**: all `sign_*` calls, `fill_sign_add` with a signature or initials,
  embedded signature-image export (`comment_image_preview`), `redact_apply` and `doc_close` are
  refused in the window, including through `command_run` and `command_batch`. An agent may add
  a signature field, text and dates, or redaction marks; the person reviews and signs/applies in
  the app. No signing credentials are accepted by these connected tools. Headless mode keeps
  its existing signing behavior.
- **Security**: the control file is checked as `pdfcraft-cli ui` checks it (a regular file of
  yours, owner-only permissions on Unix, no link) and read again at each reconnection. The
  token goes only to `127.0.0.1`. No port is opened by the CLI. A lost, malformed or timed-out
  reply is never automatically replayed: it may already have changed the document. Inspect
  `ui_state`/`doc_info` before deciding whether to retry; a later call reconnects.
- **Typing is preserved**: if a window editor has uncommitted text (comments included), a queued edit or an open dialog, document
  tools refuse with guidance to finish typing and close the dialog first. A tool never saves an older field value
  behind the editor or overwrites the person's draft. Changed documents invalidate cached
  rendering and stale object/field selections; the active tab and reader's place are kept.
- **When the window isn't there**, the server still starts (a missing control file is reported
  on stderr) and each tool answers how to start the app; a control file others can read is
  refused at start.
- **Limits**: a tool runs on the window's thread, so a long one (OCR, optimizing) holds the window
  up, and the control channel gives up after 30 s (the tool may still finish). A hidden,
  minimized or locked window may draw no frames: screenshots then fail after 5 s, and
  requests can wait until it is drawn again.

Ask Agent (`wait_for_request`, `reply_to_request`) is not part of this bridge.
