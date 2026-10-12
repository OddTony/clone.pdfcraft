# Windows single-instance security

Status: implemented; platform validation in progress, based on upstream `87b448b6` (2026-10-12).

## Scope and threat model

This follows maintainer echelon's [invitation in #616](https://github.com/storytold/pdfcraft/pull/616)
to harden the socket merged in [#715](https://github.com/storytold/pdfcraft/pull/715),
based on [#602](https://github.com/storytold/pdfcraft/pull/602). The transport remains
Windows AF_UNIX through the existing `uds_windows` dependency. Ordinary file-only
launches forward paths; options including `--control`, `--new-window`, image imports,
and empty launches retain their existing independent-window behavior. A failed
handoff opens an independent window and does not produce a fatal error dialog.

Protect document-path confidentiality and integrity, primary availability and UI
state against another unprivileged Windows account and malformed local clients.
Treat all socket bytes, endpoint files and environment values as untrusted.
Privileged administrators, SYSTEM, and malicious processes running as the same
user are outside the authentication boundary. They may access that user's files
and Windows data-protection credentials. A same-user process in another session
is not treated as a different security principal.

## Existing safeguards and missing protections

- AF_UNIX is a local transport. Windows enforces socket-path/parent-directory
  access permissions, but `uds_windows` exposes no peer-token or credential API.
  `%LOCALAPPDATA%` placement normally inherits profile permissions; the old code
  does not verify ownership or ACLs. Predictable names alone are not authentication.
- The startup file lock serializes connect, stale-socket removal and bind. Windows
  releases it when its holder exits. It is never deleted. The old code continues
  election when locking fails, weakening startup synchronization.
- `SESSIONNAME` is a routing hint supplied through the environment, not an OS
  identity. Sanitization/truncation can collide and missing names share an endpoint.
- A one-line JSON request is read through a 1 MiB cap, with five-second socket
  timeouts. Missing delimiters/truncation are not explicitly rejected; there is no
  protocol version, strict key set, path count/length bound or outbound limit.
- One background thread receives serially. A stalled client blocks the next launch;
  trickled bytes can renew per-read timeouts. The pending UI path vector is unbounded.
- Accepted requests use the existing `OsEvent::Open` route. The response acknowledges
  memory admission, not durable delivery. A lost response can cause duplicate opens
  when the launching process falls back. That preferred fallback is retained.

## Mitigations (documented before implementation; now implemented)

1. Keep the same socket and election architecture. Derive LocalAppData from the
   Windows known-folder API with the process token, ignoring spoofed environment
   values. Use the Windows process token's
   session ID for collision-free routing; retain installed/portable endpoint separation.
   Reject reparse-point parent directories and lock files, and fall back if the
   election lock cannot be obtained. Only remove a stale endpoint on a specific
   missing/refused connection result, never on an arbitrary connection error.
   Leave unrelated regular files at the endpoint untouched.
2. Frame every message with an explicit bounded length. Require a protocol version,
   an exact path-opening schema, bounded absolute paths and complete frames.
3. Use the existing `windows` package's safe Windows data-protection APIs with
   `LOCAL=user`. The primary issues an encrypted fresh challenge. A sender must
   decrypt it, return it inside an encrypted path request, and verify an encrypted
   acknowledgement tied to its own fresh nonce. No reusable secrets are sent or
   stored in endpoint files. A wrong-user or replaced listener cannot read paths
   or forge a confirmed handoff. A relay can forward sealed bytes to the genuine
   primary, but cannot read/change the paths. The protocol must not accept an arbitrary decryptable
   blob as authentication: Windows can decrypt machine-scoped blobs too, so proof
   of the primary's secret user-scoped challenge is essential.
4. Use a fixed, bounded receiver pool, a bounded connection queue, atomic bounded
   inbox admission and total exchange deadlines, including slow/trickling clients.
   Reject the whole batch before acknowledgment when capacity is exhausted.
5. Keep listener ownership at launch scope through renderer fallback. Shut down
   workers when the launch-owned server is dropped. No new document loader,
   command/control channel, listener transport or cross-platform forwarding.

## Guarantees and limitations to verify

Inherited ACLs remain the OS connection boundary; the wrapper does not provide
an independently verified owner-only socket ACL or an OS-authenticated peer token.
Windows profile administrators can alter inherited ACLs; parent checks are best-effort
reparse checks, not race-free directory handles. Use of the OS profile API prevents
environment-only redirection, but does not prove its DACL is private.
The challenge protocol protects document operations even when an outsider
can reach or replace the endpoint, but cannot prevent filesystem denial of service or another reachable client
occupying the bounded connection slots.
Session-specific names route legitimate launches; they do not authenticate the
peer's logon session. AF_UNIX provides no remotely reachable TCP listener; Windows
path access via network shares must not be claimed equivalent to named-pipe remote
client rejection.

A completed ACK means queued in the surviving primary's memory. Primary termination
can lose pending paths. ACK loss retains the upstream fallback and its possible
duplicate opening. A protocol mismatch also falls back rather than discarding
files or startup options. Manual testing remains necessary for separate accounts,
console/RDP sessions, mixed elevation, modified ACLs/reparse points, Explorer/Open
With, minimized focus, and portable/network-drive launches. The wrapper lacks cancellable connect: a single connect thread per launch may
outlive the caller's five-second timeout until Winsock returns; late streams are
dropped and no socket is deleted on that timeout. Worker IO and asynchronous
protection waits observe stop/deadline, but synchronous OS provider activation
itself cannot be forcibly interrupted. Native tests exercise normal shutdown;
no guarantee is made for a stuck OS API. macOS/Linux/FreeBSD
launch behavior and web behavior are outside this Windows-only change.

## Sources

- [Microsoft: Windows AF_UNIX and pathname permissions](https://devblogs.microsoft.com/commandline/af_unix-comes-to-windows/)
- [Microsoft: file security and inherited descriptors](https://learn.microsoft.com/en-us/windows/win32/fileio/file-security-and-access-rights)
- [Microsoft: DataProtectionProvider](https://learn.microsoft.com/en-us/uwp/api/windows.security.cryptography.dataprotection.dataprotectionprovider)
- [Microsoft: socket peer impersonation supports IP sockets](https://learn.microsoft.com/en-us/windows/win32/api/ws2tcpip/nf-ws2tcpip-wsaimpersonatesocketpeer)

## Validation and review

Portable regression tests cover framing/truncation, schema/version/path limits,
nonce binding, queue admission/draining, concurrent reservations, total deadlines
and all option bypasses. Windows tests additionally exercise real protection and
sockets, hostile messages, simultaneous process launches, crashed election holders
and primaries, missing ACKs/fallback, idle-client expiry and shutdown. Review
identified environment-derived profile routing; it was replaced with the OS
known-folder/token query, and reparse checks now precede directory creation.
Full results and native Windows limitations are recorded in the follow-up PR draft.
