# Ordinary local Engine runtime foundation

> **Status:** Implemented R1c foundation after `cd541ae927`; scoped checks are
> retained under [runtime receipts](../issues/307/checks/r1-sandbox-runtime-20261008/).
> Full Sandbox lifecycle/SDK readiness/access/cancellation integration remains
> incomplete. This source does not establish R1 acceptance or a native mount.

The active replacement Sandbox provides a concrete Docker Engine API1.54 Unix
adapter for ordinary command creation, non-TTY launch, independent stdin/output
owners and coherent runtime inspection. It uses exact full acknowledged container
and Exec IDs, not names, prefixes, host/VM PID guesses or filesystem registration.
A command supplies its explicit nonroot uid/gid, arguments, environment, cwd and
stdin policy. No classifier, hidden restoration/install, automatic runtime timer,
implicit Commit/unmount or filesystem-daemon command supervisor is added.

ExecCreate keeps the caller's original request on failure and retains any decoded
partial ID separately from a completed acknowledgment. Starting consumes the
created owner, marks the original attempt, and refuses a second Start without
sending. Request selectors and decoded response IDs remain distinct. Inspect
requires matching IDs, actual Running and nullable ExitCode from their root JSON
fields; nested Arguments/lookalikes cannot change them. Known root exit requires
Running=false and a present code; null is never zero. PID is diagnostic only.
A later observation does not resolve an earlier original lost-operation receipt.

`backend/docker/` owns standard HTTP/JSON/multiplex framing. Requests count escaped
metadata locally then stream through an8KiB buffer; this is not a second runtime
operation. Responses retain the same buffered reader through101 and validate the
multiplexed media type. Content-Length/chunk sizes/frame lengths are scalar
remaining counters, never payload allocations. Unknown JSON strings/Arguments
stream without collection. Selected-key duplicates, escapes/UTF-8/surrogates,
integer types/width, malformed/truncated framing and conflicting headers refuse
with original transfer and failed-fence knowledge. Error HTTP statuses retain a
separate bounded prefix/count/completion/error diagnostic with redacted Debug.

The selected metadata profile has an explicit32-level JSON support bound,128B
selected-key/prefix capture,8KiB individual header lines and32KiB header/trailer
metadata windows. Chunk extensions are explicitly unsupported; trailer fields
share the strict initial-header grammar. These are declared metadata/profile
limits, not command-output, duration, file or cumulative-flow limits. No complete
JSON document, command output or lifetime event stream is collected by product.

Existing locked nix0.31.3 supplies safe readiness polling over nonblocking Unix
sockets. Metadata calls have the explicitly configured blocking I/O wait; the
attached command stream changes its local readiness policy to no timeout. It
does not alter socket deadlines after a peer has closed. Poll/read/write errors,
including Interrupted/WouldBlock, end their original attempt without retry.
Positive partial progress continues within that same operation and is counted.
No reconnect, resend, alternate decoder or failed-operation replay occurs.

Attached owners split for independent stdin, output and process observations.
Explicit stdin close half-closes Write and returns its original terminal counters;
a failed close retains the input owner and prevents another close attempt.
Dropping a writer alone does not establish EOF while another socket owner remains.
Output uses an8KiB pending window, exposes exact acknowledged counts and original
undelivered bytes/partial headers on failure, and fences once. A returned terminal
output owner cannot repeat its copy. Slow caller sinks provide ordinary
backpressure; count saturation is reported and never becomes a total-flow cap.
Output EOF establishes transport completion, not process/descendant exit,
filesystem-request drain or complete production of every potential future byte.

The adapter's process-specific cancel currently returns an explicit unsupported
capability before effects. Engine has no per-Exec kill API; local attach/CLI closure
is not remote cancellation. Current runtime metadata reports containerd2.2.4 at
`/run/containerd/containerd.sock`, namespace `moby`. An isolated, pinned standard
client using ContainerID+ExecID is a concrete next route, with actual socket/client
access and CNI-extension absence to qualify. Its signal acknowledgment, root exit,
streams and descendants remain separate; no whole-Sandbox scope is inferred.
The [capability investigation](../issues/307/checks/r1-sandbox-runtime-20261008/78-cancellation-prerequisites.md)
preserves exact primary-source limits. The filesystem daemon gains no Exec wire.

Excluded old Sandbox source/test bytes were moved intact to
`layerfs-sandbox-legacy`, with only its package manifest name adjusted. It is
unbuilt reference integration, not a replacement dependency or fallback. The
replacement has no API-core/Server/host-data-runtime edge. Its current first-party
production dependency set is empty; only existing nix polling is activated.
Future actual daemon/bootstrap composition can add the existing Bridge edge.
SDK SandboxApi will wrap that real lifecycle; no placeholder SDK facade is exposed.

Functional evidence exercises public Unix endpoints with strict malformed/partial
metadata and delivery cases, plus the actual macOS-controller/Linux-Engine route:
nonroot UID/GID, zero CapEff/CapAmb, NoNewPrivs,2MiB stdin/stdout, separate stderr,
pre-start null exit and actual nonzero exit37. It is not a memory/performance gate,
protected Store/Overlay/config access proof, mounted Workspace proof, complete
Sandbox lifecycle, per-command cancellation proof or graceful daemon drain.
