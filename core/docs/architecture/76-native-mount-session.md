# Native mount session, Ready and reversible unmount

> **Status:** Current general guide.

This record describes the implemented R2 composition: one Linux kernel
connection per attached Workspace, its Ready evidence, the control records that
carry it, and normal terminal unmount. R3 added ordinary mutation on the same
connection; see [native mutation and kernel coherence](77-native-mutation-coherence.md).
Mounted Commit is described in [product Commit](79-product-commit.md) and
forced unmount in [forced teardown](80-forced-teardown.md). No daemon-wide
graceful drain exists and none is planned.
Scope and receipts are in the [R2](../issues/307/R2-COMPLETION-20261008.md) and
[R3](../issues/307/R3-COMPLETION-20261008.md) completion records.

## Ownership

Dependency is daemon → fuse → workspace. [Fuse](../../crates/layerfs-fuse/src/session/mod.rs)
owns the connection: direct mount syscalls, the patched fuser session, receive
loops, request accounting and connection drain. The daemon owns the registry,
overall Ready, unmount composition and the engine/Store owners. Fuse contains no
daemon command, registry or application setting, and neither side registers,
supervises, signals or times out a command.

[Assembly](../../crates/layerfs-daemon/src/application/filesystem.rs) runs once
before control readiness. It starts the one shared
[Dispatch](75-native-request-service.md) for the daemon's existing Overlay Owner
and direct Store; no device is opened and nothing is mounted until an explicit
Attach. Two startup conditions are checked rather than repaired:

- `/dev/fuse` must be a character device owned by the daemon identity. One
  attempt narrows it to mode 0600 and rechecks, so the command identity cannot
  open a second connection. A node that is absent, foreign-owned or still
  reachable refuses daemon startup.
- The mount home from `DaemonSetup` must be absolute, daemon-owned, not writable
  by group or others and traversable by the command identity. A missing home is
  created once at 0755; an existing one is never altered.

Outside Linux the same application assembles control only and Attach is a
definite `Invalid` refusal.

## Attach

[`NativeSession::attach`](../../crates/layerfs-fuse/src/session/startup.rs) is one
attempt with no retry. In order it builds the callback adapter, creates the mount
directory (an existing path is refused), opens the device, calls `mount(2)`,
completes the handshake through `Session::from_fd`, spawns the session owner and
a first-exit watcher, waits for all-loop serving, then records the mount-table
entry and optional abort control.

The mount is type `fuse`, source `layerfs`, flags `nosuid,nodev,noatime`, with
`rootmode=40000`, `default_permissions`, `allow_other` and `max_read=131072`.
The mount owner is the daemon identity; projected inode ownership is the command
identity. Kernel permission checks, not a library uid filter, decide access.
[Negotiation](../../crates/layerfs-fuse/src/mount/profile.rs) requests 128 KiB
write and readahead windows, background and congestion limits of 1, and adds no
optional capability; writeback caching stays absent.

Serving means the kernel mount exists, the handshake completed and the receive
loop entered and has not exited. (R7 update, 2026-10-09: a connection has one
receive loop, `RECEIVE_SLOTS`. The kernel hands each request to the loop that
has waited longest, so two loops made a serial caller alternate between two
threads; owner jobs are serialized on the one connection either way, a first
step that cannot have its turn parks, and provider reads leave the loop for a
worker, so the second loop bought no service. Measured on C01, receipt 516
against 508: command 455.1 to 416.4 ms, owner queue wait 31.0 to 1.8 ms.) A successful spawn or a probe read does not
substitute. `ready_wait` (five seconds in the application) is an observation
deadline; its expiry cancels nothing.

`AttachFailure` keeps the original phase and cause and states what remains.
Before the mount call succeeded, the directory and dispatcher lane are disposed
or their exact removal errors are reported. Afterwards one detach attempt and the
ordinary complete drain run; anything not established stays `Retained` with the
whole session. A later Attach is a new operation, never a replay.

The first-exit watcher closes new handoff admission when any receive loop stops
or unwinds, waking borrowed admission waiters. It depends on no polling interval
and no request capacity.

## Callback dispositions and accounting

Every kernel operation the pinned adapter can receive has one declared
[disposition](../../crates/layerfs-fuse/src/request/callbacks.rs), and every
received unit is counted once by
[opcode and disposal](../../crates/layerfs-fuse/src/request/accounting.rs):

| Disposition | Operations |
| --- | --- |
| Handoff to the shared dispatcher | LOOKUP, GETATTR, OPEN, READ, READLINK, OPENDIR, READDIR, RELEASE, RELEASEDIR, FORGET units; and the mutations SETATTR, MKNOD (regular), MKDIR, UNLINK, RMDIR, SYMLINK, RENAME, LINK, WRITE, CREATE |
| Inline success, no engine job | STATFS |
| `ENOSYS` | FLUSH, FSYNC, FSYNCDIR (R7, 2026-10-09: the kernel then returns success to every caller and stops asking); extended attributes, ACCESS, READDIRPLUS, locks, BMAP, IOCTL, POLL, LSEEK, FALLOCATE, COPY_FILE_RANGE |
| Refused at processing, no engine custody | MKNOD of a FIFO, socket or device, set-id bits and foreign ownership (`EPERM`); exchange and whiteout renames (`EINVAL`) |

`fsync`, `fdatasync` and `close` still succeed for the caller: the kernel
answers them itself once FSYNC, FSYNCDIR or FLUSH has been answered `ENOSYS` on
the connection. No durability is claimed; Disposable backing is never
synchronized. STATFS reports fixed declared values, not physical capacity,
and is never an admission signal. No operation answers `EROFS` any more.
`Terminal` counts a reply-bearing
unit that met stopped admission and received one error attempt. `Unadmitted`
counts a FORGET unit that met stopped admission: its kernel reference is not
decremented by guess and stays in indexed custody for revocation.

The pinned library delivers a batched FORGET as repeated single callbacks that
share one request identity and does not expose the opcode. The adapter counts
ownership frames and separately those observed with a second unit; each unit is
admitted, charged and disposed independently.

## Control records

[Bridge](../../crates/layerfs-bridge/src/control_native.rs) adds records without
changing any existing byte encoding:

- Requests `Attach(WorkspaceToken)` and `Locate(WorkspaceId)`.
- Replies `Ready`, `Located` and `Retained`, and `Activity::Attaching`.
- R6, in [`control_forced.rs`](../../crates/layerfs-bridge/src/control_forced.rs):
  request `ForceUnmount`, reply `ForceUnmounted`, `NativePhase::Stopping`,
  `TeardownStage::Abort` and `TeardownCustody.forced`; listed in
  [native control](68-native-workspace-control.md#forced-unmount-records).
- `WorkspaceStatus.native`. A status that carries the block uses its own reply
  tag, so the original status encoding is unchanged and no record is a valid
  prefix of another.

`ReadyMount` carries the token, the mount directory (at most 1024 bytes) and a
`NativeReceipt`: engine mount incarnation, root serial, mount id and device,
ABI version, offered and selected capability bits, the negotiated limits, page
size, loop count and whether an abort control was bound. `NativeStatus` reports
phase, the Ready record, whether detach is known and `NativeWork` counters copied
from maintained state. Status performs no syscall and one existing engine
observation; it never settles an unknown.

The [SDK](../../crates/layerfs-api/sdk/src/workspace/attach.rs) exposes `attach`,
`locate` and a two-acknowledgement `mount`. If the second acknowledgement fails,
`MountFailure` returns the already bound Workspace so the caller can observe or
attach explicitly; nothing is rolled back or resent on its behalf.

## Registry state

Each registry entry carries one native state alongside its control activity:

| Phase | Meaning |
| --- | --- |
| Unattached | Bound only; no directory, lane or connection |
| Attaching | One Attach owns the entry; other terminal operations are refused |
| Ready | Serving session and its Ready record |
| Probing / Draining | One unmount owns the session; an observer remains for status |
| Stopping | One forced unmount owns the session, from admission until its drain and detach are known; then Draining |
| Retained | A terminal step was not established; exact owners and stage are kept |

Attach is admitted only from Unattached with idle control activity. A second
Attach on a Ready Workspace is `Invalid` at `attach:admission` and creates no
second connection. `Locate` answers from the registry by Workspace identity, so
a caller that lost a Mount or Attach acknowledgement observes the existing
binding and Ready record instead of repeating the operation.

The mount directory is `<mount home>/<namespace>`. A namespace is never reused
within a daemon, so neither is a directory or an engine mount incarnation.

## Normal unmount

[Unmount](../../crates/layerfs-daemon/src/control/detach.rs) keeps its existing
Commit-custody admission and then, for an attached Workspace:

1. **Probe.** One plain `umount2(path, 0)` while ordinary request service
   continues. `EBUSY` is the kernel's reversible answer for any retained
   descriptor, working directory, `O_PATH` reference, mapping or blocked
   call. The Workspace returns to Ready and the caller receives `Busy` at
   `unmount:kernel`. No request was failed, no terminal error injected and no
   process signalled. There is no daemon-side request precheck: the kernel
   answer is the fence.
2. **Detach known.** Admission stops and waiters wake. Already admitted work
   keeps its original disposition.
3. **Drain.** [`drain`](../../crates/layerfs-fuse/src/session/drain.rs)
   establishes that every created loop is joined, the owner and watcher
   returned, no received, admitted or retained request remains, and releases the
   lane. `drain_wait` is an observation deadline.
4. **Revocation.** `NativeJob::Revoke` logically revokes the engine mount.
5. **Close and registry removal** through the existing terminal Close.

Any step that is not established leaves the entry `Retained` with its stage
(`Detach`, `Join`, `Owner`, `Requests`, `Lane`, `Revoke`, `Close` or `Registry`),
the observer and the original evidence. The reply is `Retained` with
`TeardownCustody`; it is not reported as an uncertain mutation and nothing is
replayed. Later Attach, Unmount or ForceUnmount on that entry returns the same
custody. The normal path never uses the abort control and never sets the
terminal fence.

## Forced unmount

[`ForceUnmount`](80-forced-teardown.md) takes the session out of the entry the
same way, into phase `Stopping`, after its own before-effect guard. It then
uses the abort control bound at Attach for exactly one write, lets
[`force_drain`](../../crates/layerfs-fuse/src/session/force.rs) establish the
same drain predicate as step 3 and make the connection's one plain detach
afterwards, and finishes with the same revocation and Close as steps 4 and 5.
Its stops use the same stages plus `Abort`, and carry the forced facts. Once
the abort write was made, a normal `detach()` of that session is refused
before any syscall: the forced path owns its single detach.

### Revocation after detach

A successful plain detach aborts the connection, so the kernel may never deliver
the asynchronous RELEASE and FORGET units for references it held. Revocation is
therefore qualified by connection drain rather than by an empty handle set.
[`revoke_native_mount`](../../crates/layerfs-overlay/src/lifetime/native.rs) is
refused only while a native request source or read association remains; file
handles, directory handles and lookup counts are retired by
[automatic maintenance](../../crates/layerfs-overlay/src/maintenance/native.rs)
in indexed windows of at most 64 rows per turn: file handles, open directory
headers, lookup groups by serial cursor, then the mount row. This supersedes the
earlier rule that an outstanding handle refused revocation. The mount row fences
whole-namespace cleanup until retirement ends, so a terminal unmount reply does
not claim that every physical row is already gone.

R7 update, 2026-10-09: a READ or READLINK records no source or read
association ([native read custody](73-native-read-custody.md)), so this
refusal cannot see one. Such a request is accounted by the connection drain
that precedes revocation on both paths: it stays admitted in its lane until
its future ends, and `Revoke` is submitted only after the drain observed
nothing received and nothing admitted.
[`forced_unmount.rs`](../../crates/layerfs-daemon/tests/forced_unmount.rs)
stages a READ parked for a Store reader on a real mount: the engine's row
counts equal those of the idle descriptor, normal Unmount is `Busy` at
`unmount:kernel` with no effect, and Force ends the request through the
fence before its one detach. A READ that is inside a Store read at that
moment is not staged on a real mount; that case rests on the drain predicate
and on the request staying admitted, by source.

A re-Attach issued immediately after a failed Attach can be refused until the
revoked mount row has retired. The caller observes and issues a new Attach; the
daemon does not wait or retry on its behalf.

## Sandbox access

The Docker [container configuration](../../crates/layerfs-sandbox/src/backend/docker/container.rs)
adds exactly `CapAdd: ["CAP_SYS_ADMIN"]`, one `/dev/fuse` device mapping and
`apparmor=unconfined`, keeping `Privileged: false` and `no-new-privileges=true`.
Since 2026-10-10 (owner decision C-1) it also sends one inline system-call
filter that denies creating a user namespace (`CLONE_NEWUSER` to `unshare` or
`clone`; `clone3` answers `ENOSYS`) and allows everything else. An unprivileged
command can make a mount namespace only inside a user namespace of its own, so
no copy of a Workspace mount can exist outside the daemon's namespace and a
normal Unmount stays truthful against any holder. A tool that needs an
unprivileged user namespace fails in a Workspace command with `EPERM`. The
filter replaces the Engine's default one for this container.
[Topology inspection](../../crates/layerfs-sandbox/src/backend/docker/topology.rs)
requires exactly that set: a missing, widened or additional capability, device
or security option is not reported as the daemon endpoint. Commands still run
as the nonroot command identity with an empty effective capability set.

## Limits of this record

- The native status block does not include engine open-handle or lookup counts,
  retirement debt or cleanup state.
- Abort control binding depends on the fusectl filesystem being mounted in the
  container; where it is not, `abort_bound` is false and forced unmount is
  refused at `force:capability`. The Sandbox container does not mount it.
- `Busy` at `unmount:admission` exists for a second concurrent terminal
  operation; it has no staged in-flight-request case.
- Nothing here is a latency, storage or resident-memory measurement.
