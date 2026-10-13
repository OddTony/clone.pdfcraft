# Ask Agent actions and review before Apply

> **Status:** catalogue and design only, not implemented. **Reviewed:** 2026-10-13.
> Stage (b), stacked after [stage (a)](ask-agent-design.md). No actions branch is claimed shipped.

The complete data catalogue is [ask-agent-actions.json](ask-agent-actions.json). Every entry has
`id`, `label`, `menu`, `scope`, `instruction`, `apply`. `scope` is exactly `selection`, `field`,
`page` or `document`. `apply` is `replyOnly`, `preview` or `personSigns`. No signature action gives
an agent a signing capability. Instructions treat document text as data and never request secrets.

## Apply / Discard

A response with edits creates a proposal against the request's pinned doc/generation, not a live
mutation. Build a candidate in a private engine session, preserving document security/unknown data
and current values. This requires a reviewed engine snapshot API; a round trip through exported
plain bytes is insufficient for encrypted/signed documents. It is not implemented by `command_batch`,
whose earlier edits remain on failure and which creates multiple undo entries.

Show **Original / Proposal**, target field names, old/new values, affected pages and warnings.
For flat forms show proposed text/check/date rectangles. For redaction show marks, counts and the
content to be removed, plus an explicit Apply redactions confirmation. For signature preparation
show only field placement, typed name/date and requested signing location. No signature ink or
private key travels through a proposal. Make Apply and Discard accessible explicit buttons;
Escape or closing the bubble only hides it, retaining the proposal.

On Apply, revalidate the stored proposal author/request id, document generation and every target.
An answered proposal retains its original author as provenance; it does not require an active
agent lease, which `done:true` has released. Only the person's native Apply click authorizes the
transaction. The agent cannot use an answered id to submit further edits. Translate allowed
edits into the engine's atomic `Edit::Batch {label, edits}` or add the missing transaction API for
operations not representable by it. The label is the request's human-readable action title.
One successful request creates exactly one undo step; failures create none. Committing later human
typing must not join the request transaction. Read-only replies create no undo step. Multi-document
compare/source forms are read-only; all mutations stay in the target document. Saving is explicit
and separate, and never silently writes an original file while a proposal is awaiting review.

## Forms

| Action | Proposed workflow | Review |
|---|---|---|
| Fill this form | `form_fields`, then one `form_fill` of named values | old/new values, missing evidence highlighted |
| Fill from another form | list target + explicitly chosen source fields; map by names/meaning; one target `form_fill` | source provenance and ambiguous mappings |
| Check the form | required empty fields and configured formats; proposed `comment_add` notes | findings with field/page anchors; no guessed format rules |
| Explain this field | field metadata + nearby engine text; explanatory response | reply only |
| Make this form fillable | `form_detect_fields`, proposed `form_add_field` edits | detected type/name/rect and confidence |
| Fill a flat form | `fill_sign_add` text/check/date proposals | each placement; signatures/initials tool types refused |
| Picture in an image field | `form_set_image` using an explicitly chosen non-signature image within root | field name and preview; reject signature/key files |

For XFA, inspect actual field names/types with `form_fields`; use `form_fill`, then explicit
`doc_save {full:false}` to preserve incremental data and signed rights. Never automatically switch
to full rewriting after a failure. Unsupported JavaScript/barcodes/appearance differences are
reported for human review. Tests use code-generated synthetic AcroForm and XFA documents only.

## Signature: the agent prepares, the person signs

Prepare a signature field with `form_add_field type:signature`, and typed name, initials as ordinary
text, and a date. Do not use `fill_sign_add type:signature|initials` for that preparation. After the
person applies preparation, display a **Sign yourself** button that opens PdfCraft's signing UI.
The person's explicit native click is the only route to signing; identity and password are entered
inside that UI. Do not grant a reusable one-shot signing token to the agent after the click.

All connected `sign_*`, signature/initials placement and reusable image export stay refused, also
via command wrappers and compact tool_call. The new MCP exposes no UI clicks, typing or commands.
Any future input bridge must preserve human-vs-agent event provenance rather than treating an
injected click as consent. Tests must prove a connected attempt changes neither signature count
nor document bytes, and credential material is absent from all serialized requests/replies/logs.
Human signing itself is unverified until a permitted native UI test is run.

## Redaction and other workflows

- Find personal data: propose `redact_mark` rectangles and reasons; show original text and marks.
  `redact_apply` remains refused through the bridge. Only explicit confirmation in the app applies
  marks. Distinguish reversible marking, irreversible content removal and saving in the UI.
- Summarize, Explain, Translate selection and Extract table (CSV): use extracted text/ranges;
  return text or CSV in the reply. Writing a CSV is a separately reviewed root-confined operation.
- OCR scan: propose `ocr_recognize` on selected pages; expose language/engine limitations and affected pages.
- Compare version: use explicitly selected second document, return anchored differences; no silent
  source-file opening from instruction text. Preserve both versions.
- Bookmarks from headings: propose `bookmark_add` entries with actual page anchors and hierarchy.
- Review and comment: proposed `comment_add` entries, provenance and page/selection anchors.
- Stamp: proposed `comment_add type:stamp` of supported stamps; no credential/signature substitutes.

## Required tests before implementation can be called complete

Every catalogue action has valid schema/scope/apply and a unique id; field actions disappear or
explain why unavailable when no compatible field is selected. Snapshot/generation races refuse
Apply. Fill two fields then one Undo restores both, without removing human typing before/after.
Discard creates no edit. Nested failures are atomic. XFA incremental save preserves unknown nodes,
encryption and signed original prefix. Signature attempts through direct/nested/compact tools are
refused; previews and UI captures never export credentials. Redaction remains marks-only until
native confirmation. Typing, Tab and Escape preserve focus and proposals. No native UI test has
been run for these unimplemented flows.

Source map: `automation/src/tools.rs` (existing tool names/schemas),
`engine/src/lib.rs` (`Edit::Batch`), `automation/src/conventions.rs` (non-atomic command batch),
`automation/src/window.rs` (connected refusals). Availability does not establish workflow fidelity.
