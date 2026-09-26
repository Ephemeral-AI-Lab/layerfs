# #245 resource admission and remaining ceiling plan

> **Status:** Current planning checklist; no release candidate exists.
>
> Source pin: `f74dbe77da12fa533587be8a578375bce3f19373` (2026-09-26).
> This describes the target after #248, #256 and #249, not implemented behavior
> or a measured capacity. Earlier #245 handoffs retain their historical source
> and result labels.

## Contract and concurrency boundary

The only **logical serialization rule** is one active Commit/Stage submission
per Workspace. One sandbox daemon admits multiple mounted Workspaces. Independent
`WorkspaceApi::exec` commands may overlap within a Workspace and across
Workspaces. No fixed per-daemon, per-Workspace or lifetime Exec count is part of
admission; only the resources each active command actually owns can bind.
A Commit freezes that Workspace's `G1` root while later accepted
filesystem calls update `G2`; a command is not an atomic transaction and its
syscalls can straddle the capture boundary. Different Workspaces may Commit
concurrently if the Store's charged writer budget admits them. Two Workspaces
targeting one Branch still use `expected_head`; a moved head is a conflict.

```text
sandbox: charged RAM, PID, FD, private-backing and Store resources
  |
  +-- Workspace A: active root A2 -- Exec A1, Exec A2, mounted callbacks
  |     Commit A1: pin A1 -> build/publish A's head -> reconcile A2
  |     Commit A2: cannot start while A1 owns A's submission slot
  |
  +-- Workspace B: active root B1 -- Exec B1, mounted callbacks
        Commit B1: may overlap Commit A1; owns B's submission slot

No daemon-wide Commit lock. No whole-Exec runtime timer in the target.
Quota/custody refusal and per-syscall/Commit deadlines remain separate.
```

"Resource based" means a supported workload is admitted while its measured,
charged memory, disk, PID, FD and Store budgets have room. A fixed transport
frame, page fanout or readdir *page* size is valid when it has continuation and
does not cap the total workload. A fixed count that rejects before a budget
binds is not. The current 4 GiB logical-file format, path/name grammar and
finite identifier widths are separate representability contracts; they are
listed below instead of being called resource limits.

## Ceiling inventory and lift path

| Current source ceiling | What it actually limits | Target and work owner |
| --- | --- | --- |
| 4,096 file changed runs | The explicit refusal was removed by #252; public 4,097-run speed and exact-root proof remain open. | #248 proves bounded Commit traversal and no downstream count refusal. |
| 128 dirty identities/names, 128 local directory names, 32 KiB prepared metadata, narrow record counts | One changed generation or directory, even with free backing. | #256 uses path-local keyed updates and streamed, replayable prepared rows. |
| 4,096 pending C1 serials and fixed namespace builders | One wide namespace Commit. | #256 uses charged ordering backing and progressive multilevel construction. |
| 65,536 private page slots and fixed ledger/temporary-page reserves | Private metadata may stop near 260 MiB of page/ledger blocks under a larger quota. | Shared #248/#256 page store charges actual pages, ledgers and construction progress to the configured budgets. |
| 8 GiB `SaveFile` body | Descriptor bytes plus final Local/Zero bytes for one save; it can bind before available spool space. | #248 makes stream admission depend on charged actual bytes and a declared storage budget, with checked totals and no hidden run cap. |
| 256 observed nodes, 128 simultaneous handles, 1,024 directory cookies | Cached/pinned runtime observations and readdir positions, not total changed files. | #256 indexes and evicts only unpinned observations; live handles/cursors grow with charged RAM/FD/backing and release on close. |
| 32 private roots and 11 arenas | Simultaneously owned COW/capture states and backing contexts across Workspaces. | Shared backing work and #249 use charged registries, prompt safe reclamation and a concurrency proof. A pinned root cannot be evicted to make room. |
| One daemon-selected mount and one live control session | Multiple SDK calls and Workspaces cannot overlap even when resources exist. | #249 keys daemon ownership by Workspace ID and incarnation, gives each admitted command a process lease, and removes the global dispatch hold. No numeric Exec count replaces this singleton. |
| 30-second whole-Exec deadline | The daemon kills a long shell process group. | #249 removes the total command timer across SDK, Bridge, native transport and daemon; explicit cancellation, disconnect and teardown retain process custody. |
| 4,096-byte Exec command text | The UTF-8 string passed to `/bin/sh -c`, not bytes written by the command. | For a command-text-unbounded public API, #249 must stream/charge the command and define execution from a script FD or equivalent when it exceeds OS argument limits. Preserve or version the small-command `/bin/sh -c` behavior. |
| 8 MiB per direct `Workspace::write_file` payload | One internal write call, not a file or shell command. The mounted FUSE route negotiates at most 128 KiB per WRITE callback. | A huge mounted file already uses many callbacks. If direct single-call writes must scale too, #248 must stream/split the payload and construct one atomic mutation with bounded memory. A 128 KiB FUSE callback remains a batch size. |

The source anchors for the non-namespace rows are [request/stream limits](../../../crates/layerfs-bridge/src/contract/request.rs), [Exec validation](../../../crates/layerfs-bridge/src/contract/workspace_request.rs), [SDK Exec](../../../crates/layerfs-api/sdk/src/workspace.rs), [daemon execution](../../../crates/layerfs-daemon/src/execution.rs), [daemon creation lifecycle](../../../crates/layerfs-daemon/src/lifecycle.rs), [Workspace host admission](../../../crates/layerfs-workspace/src/runtime/host.rs), [Workspace writes](../../../crates/layerfs-workspace/src/filesystem/write.rs), [FUSE write negotiation](../../../crates/layerfs-fuse/src/adapter.rs), [page references](../../../crates/layerfs-workspace/src/backing/metadata_pages.rs), [root/arena ownership](../../../crates/layerfs-workspace/src/backing/metadata.rs), [live state](../../../crates/layerfs-workspace/src/runtime/state.rs), and [directory cookies](../../../crates/layerfs-workspace/src/filesystem/directory.rs). The [joint tree study](JOINT_248_256_TREE_RESEARCH.md) gives page, payload and namespace capacity estimates rather than admission measurements.

## Large files and one WRITE

For a mounted command such as `cp source bigfile`, the shell command text can
be short while the process makes many FUSE WRITEs. The mount currently requests
at most 128 KiB for each callback; the 8 MiB internal guard therefore does not
cap the total mounted file. For example, 1 GiB requires at least 8,192
128 KiB callbacks, possibly more if the kernel splits them further. Each
accepted callback stores its Local payload and publishes a coherent private
root. The declared 4 GiB logical-file bound is still checked independently.

Keep request chunking and file capacity distinct in the acceptance report.
Changing a per-call guard is useful only if its public route can submit a
larger single call, and a direct large call must not materialize a replacement
vector proportional to its byte length. Commit must see the final extents and
bytes, regardless of how many callbacks built them.

## Deadline-free Exec is a protocol change

The current `Request.deadline_ms` is mandatory and bounded; the SDK owner,
native client/server, response writer and daemon all create absolute deadlines.
The daemon polls the child and sends `SIGKILL` to its process group at the
30-second whole-Exec deadline. Replacing `30_000` with a larger integer merely
moves that ceiling.

For Exec, encode an explicit *no total execution deadline* state while leaving
other operation deadlines finite. Keep a command lease from admission through
spawn, output draining, exit and response delivery. End that lease on normal
exit; explicit cancellation, lost client, unmount or shutdown must terminate
and reap the exact process group. Authenticated, bounded-rate heartbeats must
keep a read-only or silent command's transport live without counting toward a
small lifetime response-frame allowance (the current `frame_budget(0)` would
otherwise become a new time-dependent ceiling). Bound captured stdout/stderr
and per-command resident state independently of runtime. A synchronous SDK
caller may wait until the command exits; elapsed wall time alone does not kill
it. Benchmark harnesses still measure wall and apply their separately declared
performance and functional gates; those gates are not product Exec policy.

The 4,096-byte command-text limit is different. A short script path works for
arbitrarily large file operations today, but an arbitrarily long inline command
does not. Lifting that interface limit needs bounded streaming or spooling of
the command and a defined shell invocation for text larger than the OS
single-argument limit. Executing from a script FD may change `$0` and stdin
semantics relative to `/bin/sh -c`; specify and test that contract rather than
silently switching behavior.

## Lightweight, count-free Exec supervision

Let `Q` be active Exec commands, `N` completed Exec commands and `W` live
Workspaces. The current daemon has one control `Session` and a thread for that
session; its Exec path polls one child and its pipes every five milliseconds.
Replacing that one session with one worker thread and stack per command would
make concurrency expensive even though it removes the refusal. The #249 target
uses an event-driven supervisor shared by admitted commands:

```text
SDK calls -> authenticated control connections -> small keyed Exec leases
                                                 |   (Workspace ID, request ID)
                      shared readiness/exit loop -+-- socket, stdout/stderr pipes
                                                 +-- child exit and cancellation
                                                 +-- bounded-rate heartbeat
               one lease = process group + required FDs + bounded output
               completed lease -> response + reap + release, no history scan
```

Use nonblocking pipe/socket readiness and child-exit notification on the
daemon's Linux platform, with one supervisor or a small fixed set shared by
all commands. There is no dedicated OS thread, 2 MiB stack, fixed semaphore
count or timer poll loop per Exec. Each admitted command owns `O(1)` small
daemon metadata plus its actual process, FDs and bounded output; total daemon
owner state is `O(W + Q)`, not `O(N)` or a fixed-size array. Admission and
retirement must not scan previous commands; work follows ready I/O and exited
children. A shell may create its own child processes, charged by the sandbox's
PID/memory limits. When process or FD creation fails, report the resource
failure precisely, with no hidden Exec-count refusal or unbounded wait queue.
The Workspace-count policy does not count Exec leases.

## Tree references and live ownership

Private [`PageRef`](../../../crates/layerfs-workspace/src/backing/metadata_pages.rs)
encodes a `u32` slot plus a `u32` reuse epoch. Its current decoder also rejects
slots above 65,536. First remove that smaller admission and scale allocator,
ledger identity storage, ownership and quota checks together. At 4 KiB/page,
the 32-bit slot field represents roughly 16 TiB of raw page slots before
ledger and other overhead. An epoch must never wrap to make a stale reference
valid: retire the slot and allocate a charged fresh one. If the supported quota
or lifetime can exhaust the field, a versioned wider reference and migration
path are required across page headers, child cells, ledgers, root records and
cursor codecs.

The current seven-level tree declaration is another finite format check.
Prove from minimum occupancy and the configured maximum page quota that it
cannot bind first. If the proof fails, allow more levels and charge the cursor
path dynamically; widen the encoded level only if its existing byte cannot
represent the supported height. Do not change a page format simply because a
larger theoretical integer exists.

For live state, charge open handles, kernel lookup pins, directory cookies,
root owners, arenas, control sessions and command process leases to their
actual RAM, FD, PID and backing budgets. An unpinned node can be evicted; an
open handle, frozen root, active command or uncertain Commit outcome cannot.
Directory pagination needs a resumable cursor or charged cookie backing so
one large directory does not require all prior names resident. Root and arena
registries reclaim entries only after the last owner releases them. The
current sandbox launcher sets `LAYERFS_WORKSPACE_MAX_COUNT=2`; [#219](https://github.com/Ephemeral-AI-Lab/layerfs/issues/219)
is now the sibling of #249 under #245. It owns the operator
`max_workspaces_per_sandbox` admission policy for Workspaces created
*internally by the daemon* through `WorkspaceHost::attach`. Its one selected
positive value is chosen when a sandbox is created, with the current internal
`2` proposed as the default; it is immutable for that sandbox's lifetime.
The daemon reserves before a Workspace becomes visible and releases only after
exact cleanup. Attaching and retained failures count, but completed past
Workspaces do not. The selected setting may admit multiple Workspaces without
a lower singleton daemon cap. It never counts Exec commands or generations
and is separate from the one-Commit-per-Workspace ordering rule.

## Proof before a resource-only claim

1. Through public SDK/FUSE, verify one file with 4,097 separated final runs,
   one generation with 1,025 changed files/names, and a mounted file larger
   than 8 MiB. Check old/new heads and full bytes; record actual FUSE writes,
   quota charges and peak resident/file-backed memory. #248 and #256 own the
   registered gates and their benchmark rules.
2. With enough declared resources, cross 65,536 private slots, 128 live
   handles, 1,024 cookies on one directory handle, 32 owned roots and 11
   arenas in focused tests. Show a precise resource refusal only after the
   configured budget is consumed; preserve pinned-owner and failure custody.
3. For #249, mount at least two Workspaces in one sandbox, overlap two Exec
   calls on one Workspace, and overlap Commits on distinct Workspaces with
   Store writer capacity for two. The second Commit on one Workspace must not
   publish. Test a moved shared Branch head separately. With enough PID/FD/RAM
   headroom, increase simultaneous Exec count on one and several Workspaces;
   then complete many sequential calls and verify that command leases return
   to baseline, no completed-command scan appears and no per-Exec supervisor
   thread is created. #219's configured Workspace count remains unchanged by
   Exec admission.
4. Run an Exec beyond the old 30-second boundary with no file or output
   progress; it must remain live. Cancel/disconnect/teardown must reap its
   process group. Keep transport heartbeat and output state bounded. Test a
   command longer than 4,096 bytes only after its execution semantics are set.
5. Validate slot/epoch and tree-height boundaries with external tests and a
   supported-quota calculation. The existing 4 GiB file format, path grammar,
   per-callback deadlines and finite identifier envelope remain explicit until
   separately changed. No unmeasured capacity or latency claim follows from
   these design targets.
