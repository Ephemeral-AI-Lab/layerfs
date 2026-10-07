# Owned Sandbox lifecycle and authenticated SDK startup

> **Status:** Implemented R1d source over `1b3e90e1fdb53d0af79fb3b5cc88215a96a00cb1`.
> [Final source identity](../issues/307/checks/r1-sandbox-lifecycle-20261008/88-final-source-identity.json)
> and [actual macOS-controller/Linux-daemon proof](../issues/307/checks/r1-sandbox-lifecycle-20261008/94-final-native-lifecycle-proof.txt)
> establish only their recorded scope. Full R1, native FUSE Ready, mounted Commit,
> cancellation qualification, application drain and R8/R9 acceptance remain open.

Sandbox owns actual Engine resources. `SandboxApi` owns the concrete Engine endpoint
and composes `SandboxCreate` into an exact acknowledged runtime container,
expected-peer authenticated `Control`, original `DaemonStatus`, handshake work and
admitted nonroot command identity. No daemon command launcher, filesystem Exec
registration, host data runtime, legacy adapter or retry is added.

The create path validates a full immutable image digest and one existing plain
local named VM volume (exact Name/local Driver/local Scope/empty Options), then
creates a stopped container. The shared Store volume is borrowed; the container's
writable layer owns its config, Overlay and mount directory. Fixed deployment
entries contain config0600/private parents0700 and executable0755. The tar contains
no shared Store directory/file entry. Installer-mode daemon configuration creates
its absent Store parent once; existing-Store mode checks that parent without
repair. No credentials appear in container env, argv or labels.

The HTTP implementation uses one exchange: immutable JSON encodes once for local
length preparation and once for transfer; the known-length archive producer is
`FnOnce`, streaming executable bytes in8KiB windows. Ustar size/field bounds are
format support for this deployment artifact, not runtime file/Workspace/flow caps.
The upload is incremental extraction. A failed/lost response never proves an empty
destination and never authorizes another upload, adoption, automatic deletion or
Start. Original declared executable/tar sizes, source bytes read, buffer-accepted
archive bytes, positive socket body/wire bytes and two disjoint pending windows
remain separate. Pending bytes and HTTP diagnostic prefixes have redacted Debug.
The caller retains the original file/config; SDK failure retains original inputs.

Each original upload, Start, listener wait, stop and delete has a one-attempt flag.
Known acknowledgments are distinct from attempted/unknown outcomes. Explicit
stop is whole-Sandbox crash teardown, not per-command cancellation or graceful
filesystem drain. Delete requires known stop after any Start attempt and deletes
only the exact acknowledged container/local writable layer with volume deletion
disabled. There is no Drop cleanup. Partial Create IDs remain observed response
facts, never automatically adopted resource ownership.

After Start, the caller-selected absolute startup deadline covers nonblocking
Unix connection admission, request/header transfer, buffered body input and log
frames. Unix EAGAIN is a failed original admission; only EINPROGRESS gets one
completion observation. The existing nix0.31.3 socket/fs/poll capabilities supply
this path; the lock adds normal socket-feature transitives memoffset0.9.1 and
autocfg1.5.1, with no new direct third-party crate or third-party modification.

The listener waits for the actual complete stdout line from this fresh exact
container with RestartPolicy=no and non-TTY json-file logging. Stdout line state
spans frames/windows, stderr cannot satisfy the marker, and giant unrelated lines
are discarded without collection. A marker inside an unfinished frame remains
an observed marker fact, while truncated-frame/timeout failure prevents readiness
success. Received-byte counts include paid read-ahead. The original log fence is
attempted once; its failure keeps marker and exact HTTP transfer attribution.

One container inspection checks exact ID/image, Running/not Paused/Restarting/Dead,
daemon User/command/non-TTY, NNP/not Privileged/no restart/logging, one exact shared
volume mount and the selected port's single loopback mapping. Other Ports members
are skipped; no whole-map exclusivity claim follows. The SDK connects once and
authenticates the explicit expected daemon peer. Original handshake work and
retained Control survive any later policy/Hello failure. Handshake I/O waits are
cleared on the actual shared socket before product operations; this adds no
ordinary command duration limit.

Installer mode sends Hello without waiting for Store and selects InstallPending.
`Control::install_project` borrows that same Connection and keeps its correlation
counter; an attempted failure makes the owner terminal while returning the
original InstallFailure. A same-incarnation Hello wait then observes ControlReady.
Additional Sandboxes explicitly open the retained Installed manifest on the same
volume. Listener/authentication/ControlReady are distinct from native Workspace
Ready. `ManagedSandbox::exec` selects its admitted UID/GID and delegates ordinary
Exec creation; returned runtime owners launch, stream and inspect independently
of filesystem lifetime. No shell exit or stream EOF establishes FS drain.

EndSession now preserves terminal socket ownership: the daemon knows its final
reply send completed, then waits once for caller EOF and fences once before slot
release. The SDK independently knows it received/validated SessionEnded, then
fences its original owner once. Trailing input is refused before record decoding.
Terminal failures retain original Served plus ending byte/work/phase/fence-attempt
facts. While waiting, a caller-held socket retains a finite Running slot; after a
failure, the slot retains receipts and the local Connection drops. This is not a
recoverable live socket owner or application force-close/join implementation.
Reserved startup Capacity refusal retains its distinct immediate-closing scope.
Mac responder ENOTCONN is retained as a real fence failure, never suppressed.

The actual9s functional proof imports/seals on macOS SQLite3.51.0, installs into
Linux SQLite3.53.2 on a shared named VM volume, runs two simultaneous daemons with
separate private Overlays, and verifies UID501/GID20/groups20, CapEff0/CapAmb0/NNP1,
private backing/proc aliases/device-control denial. Both ordinary command roots
exit0. Logical bind/unmount/session-end and explicit stop/delete complete. A third
fresh daemon opens and binds the original root after both prior containers are
deleted, proving the borrowed Store survives product cleanup. Complete wall time
5793396917ns is a functional stop receipt, not a speed/cold/residency/numeric-gate
claim. The failed predecessor proof and original test/build/review failures remain
append-only. Global Store execution is Disposable/WAL/OFF only; Durable NOT_RUN,
disabled by owner. Overlay remains separate MEMORY/OFF/EXCLUSIVE.

Official unchanged static ctr2.2.4 archive preparation is checksum verified,
but endpoint/CNI/exact process-signal qualification is still open. Engine's
per-Exec cancellation remains typed Unsupported before effects. Native FUSE,
mounted mutation/Commit, sustained drain/reclamation and conditional reference
retirement are not established by this checkpoint.
