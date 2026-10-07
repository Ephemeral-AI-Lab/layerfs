# S8 specification: native mount, Bash Exec, request service and cache lifecycle

> **Status:** Proposal; target LayerFS 0.1.7; not a released contract.
> Written 2026-10-08 on local `main` at `32d969776151588aec5aee1e9296658b8d00908d`
> (product source pin `f0797c646d82835ec922c4bb62fa2374ec8843cf`, tree
> `461a53713b35e44dd62793375acad6ebe7359582`; the later commit is documentation only).
> This document specifies S8. It contains no product code and claims no
> implementation, native mount, measurement, qualification or release admission.
> No build, test, mount or benchmark was run to produce it.

This is the single decision owner for S8. The
[implementation plan](S8-IMPLEMENTATION-PLAN-20261008.md),
[mechanism and evidence ledger](S8-MECHANISM-EVIDENCE-20261008.md),
[proof plan](S8-PROOF-PLAN-20261008.md) and
[implementation handoff](HANDOFF-S8-IMPLEMENTATION-20261008.md) reference it and
decide nothing themselves. Where they disagree with this document, this
document governs and the other is a defect.

## 1. Scope, authority and status of claims

S8 delivers, on the existing direct-Store daemon: a native FUSE mount per
Workspace with exact readiness and detach; ordinary `/bin/bash -c` Exec with
bounded streaming I/O and exact process custody; an event-driven request service
that never pins a receive loop on an unavailable prerequisite; the kernel cache
profile with its coherence rules; stable identity; confinement of Bash from both
databases; and terminal unmount with bounded automatic reclamation.

Claim labels follow the [#303 index](../303/README.md#claim-labels):
[implemented and source-verified], [measured diagnostic evidence], [owner
requirement], [proposed design], [unresolved question]. Everything in sections
4–13 is [proposed design] unless it carries another label. Kernel statements are
pinned to `torvalds/linux` tag `v6.12` and fuser statements to the checked-in
`core/vendor/fuser-0.18.0`; exact file/line citations are in the retained
[kernel review](checks/s8-specification-20261008/02-review-kernel-fuse.md). The
retained environment runs `6.12.76-linuxkit` aarch64; the stable tree was not
diffed against `v6.12`, and its page size is not recorded. S8 is specified
against that kernel line only; another kernel is unqualified, not assumed equal.

Authority order: current [root](../../../../AGENTS.md) and
[core](../../../AGENTS.md) guides and dated owner decisions; the seven #303
primary contracts and [integration contract](../303/06-cluster-one-integration.md);
then this specification, which refines them for S8 and records each place it
supersedes their text in section 2. Research and experiments establish no
capability. Durable execution is
`NOT_RUN — disabled by owner until explicit reauthorization`; nothing here
selects, executes or designs around it.

Out of scope: live namespace normalization and its Commit (S10, section 14);
host data services; a replacement SQL engine or second scheduler; any new
dependency other than the packages the already pinned fuser 0.18.0 itself
requires, which enter the lockfile when it is first built (implementation plan
section 2); any third-party change beyond the authorized fuser 0.18.0
signed-timestamp patch; numerical latency or memory acceptance thresholds, which
remain owner-deferred under the
[acceptance decision](PRE-S8-ACCEPTANCE-DECISION-20261007.md).

## 2. Reconciliation with superseded and contradicted text

Historical documents and diagrams stay unchanged. This table states which
reading is current.

| Earlier statement | Where | Current reading | Basis |
| --- | --- | --- | --- |
| Base objects come "from the host global Store" through a host runtime; the October 5 architecture diagram | [#309](https://github.com/Ephemeral-AI-Lab/layerfs/issues/309) body, [snapshot](checks/s8-spec-prompt-20261008/sources/issue-309.json) | Superseded for the data path. Every daemon opens the shared Store in-process; the host is control-only after install. The diagram is preserved as the owner-requested historical picture; the daemon box now contains Storage/History, and the host box shrinks to Init/seal/install/control | K28–K33, owner direction 2026-10-07 |
| SDK `client/`/`runtime/`, Bridge data codec, daemon `upstream/` layouts | #307 S9 plans | Retired, not extended. No host data service is designed here | [F12](PRE-S8-F12-20261007.md) |
| One command identity per Workspace (O-8 recommendation) | [08 §7](../303/08-decisions-provenance.md#7-questions-only-the-owner-can-answer) | Not adopted. One unprivileged Bash user per daemon, different from the daemon user | O-24 |
| Reply `Bound` as mount success | [control records](../../../crates/layerfs-bridge/src/control_types.rs) | `Bound` is the Store/engine half only. Native readiness is the new `Ready` reply (section 4) | [native control](../../architecture/68-native-workspace-control.md) |
| "Each handle, including the write handle, sits behind its own mutex" | [06 §4](../303/06-cluster-one-integration.md#4-the-daemon-adapter) | Contradicted by source: the Persistence session uses `try_lock` and returns `Busy`, in-process as well as cross-process. K30 forbids a writer gate. Consequences and the one open choice are D-9 and P-1 | [cache review C1](checks/s8-specification-20261008/03-review-cache-lifecycle.md) |
| Open and lookup references are independent indexed lease rows; native FORGET/RELEASE mapping "remains S8" | [S6 lifetime contract](S6-LIFETIME-CONTRACT.md) | S8 maps OPEN/RELEASE to the existing `OpenFile` custody and maps kernel `nlookup` to no per-inode state (D-6). `LookupOwner` remains an engine capability for request-processing custody | This document |
| Mmap flush "emits a ctime-bearing SETATTR" | [fuse-investigation 01 §6, 03 §6](../303/fuse-investigation/01-kernel-and-request-path.md) | Contradicted for writeback off: inodes are `S_NOCMTIME`, the daemon owns times and never receives `FATTR_CTIME` | Kernel review finding 26 |
| The kernel sends DESTROY at unmount | [fuse-investigation 03](../303/fuse-investigation/03-per-call-lifecycle.md) | Contradicted for the `fuse` type: the connection is aborted first; loops observe `ENODEV`. Loop exit and join, not DESTROY, is the detach signal | Kernel review finding 22 |
| Removing the per-WRITE `inval_inode` is an optimization | [05 §7](../303/05-fuse-assessment.md#7-optimization-disposition), [fuse.md §4](../303/fuse.md#4-cache-coherence-and-lifetime-transitions) | Understated. Under cached I/O that call takes folio locks held by an in-flight partial-page WRITE and self-deadlocks. Its absence is a liveness requirement (I-9) | Kernel review finding 20 |
| Parked requests never block unrelated files | [fuse.md §3](../303/fuse.md#3-callback-ownership-and-scheduling) | True for foreground requests only. With `max_background` 1, one parked background request (readahead READ, mapped WRITE, RELEASE) queues every other background request of that mount in the kernel (section 8.2) | Kernel review counterexample 1 |
| Predecessor `layerfs-fuse` as the adapter to re-enable | [fuse.md §2](../303/fuse.md#2-profile-and-inherited-behavior) | It imports symbols that exist only in `layerfs-workspace-legacy` and cannot compile against the active Workspace crate. It is reference reading for semantics, not a base to port | Kernel review (b) |

## 3. Product model and invariants

[owner requirement] Many sandboxes share one global Store. One long-lived daemon
belongs to each sandbox and opens that Store in-process. One daemon serves
several Workspaces. One Workspace is one mount serving any number of sequential
or concurrent Bash commands. Per-tool-call `mount → Exec → explicit terminal
unmount → next fresh mount` is the expected common mode and must be cheap;
persistent per-task Workspaces are equally supported. Command duration,
Workspace lifetime and Commit cadence are independent.

```text
 host (control only after install)             Linux sandbox: one daemon
 --------------------------------             ---------------------------------------------
 Init -> seal -> install (once)               control Service + registry (one per daemon)
 mount / Exec / Commit / status /   ------->    |        |             |
 unmount over authenticated channels         Store (1 writer,     Overlay Owner       request service
                                             N readers, 1 cache)  (1 SQL thread)      (K workers, fair)
                                                 ^                    ^                   ^
                                                 +---------+----------+-------------------+
                                                           |
                                Workspace A (ns a)   Workspace B (ns b)   ...   independent mounts
                                FUSE session A       FUSE session B             fresh kernel caches each
                                Exec sessions        Exec sessions              shared Bash identity
```

Daemon-lifetime owners, never recreated per mount [implemented and
source-verified]: the `Store` (writer, fixed readers, `CanonicalCache`), the
overlay `Owner` thread with its queue and connection, and the control `Service`
registry. S8 adds one more daemon-lifetime owner, the request service. They stay
alive across zero-mounted-Workspace intervals.

Invariants. Each is a requirement with a proof row in the
[proof plan](S8-PROOF-PLAN-20261008.md#3-functional-proofs).

| ID | Invariant |
| --- | --- |
| I-1 | A mount binds a complete root with bounded work. It never walks, copies, materializes or prefetches the base, installs dependencies, or creates a database |
| I-2 | An admitted Workspace identity is never implicitly reset or reused. A stale token never redirects to a later incarnation or namespace |
| I-3 | `Ready` is returned only after the kernel mount exists, the FUSE handshake completed and every dispatch loop is running. `Bound` is never presented as `Ready` |
| I-4 | The view of a mounted Workspace changes only through a kernel request on that mount. S8 issues no kernel notification and accepts no non-FUSE mutation of a mounted Workspace |
| I-5 | A successful mutation reply is preceded by local publication of all of its effects in one atomic owner job. A failed or lost reply never undoes a published mutation |
| I-6 | Every attribute-bearing reply is computed from published state at or after the request was received. No reply is produced from a view older than a mutation whose reply was already attempted |
| I-7 | Every admitted kernel request receives exactly one explicit reply attempt in daemon-owned time. No path relies on dropping a reply object or on FUSE_INTERRUPT |
| I-8 | No dispatch loop, service worker or SQL owner turn waits on a prerequisite. A parked request holds an owned reply and its credits and holds no Workspace, cache, registry or SQL lock |
| I-9 | No park depends on a future kernel request on the same connection. Every prerequisite a request can park on is resolved by the daemon alone |
| I-10 | READ replies exactly `min(length, EOF − offset)` bytes; WRITE replies the full count or an error; the WRITE offset is always the kernel's |
| I-11 | Exec launches ordinary `/bin/bash -c`. There is no implicit Commit, reset, unmount, install, restore, command classifier, shell-specific filesystem route or automatic timeout |
| I-12 | Shell exit, each stream's EOF, descendant quiescence, open descriptors, dirty mappings and in-flight requests are distinct observations with distinct owners. None is inferred from another |
| I-13 | Bash cannot open the Store, the overlay database, a daemon credential, `/dev/fuse` or a control socket through any path alias or inherited descriptor |
| I-14 | Terminal unmount success means: admission fenced, kernel mount detached, every dispatch loop exited and joined, session buffers released, routing removed, logical close acknowledged. Physical row deletion may finish later and is reported as debt |
| I-15 | No Workspace `fsync`/`fdatasync`/`sync_data`/`sync_all`; no automatic retry, busy handler, polling sleep or failed-operation replay anywhere in these paths |
| I-16 | Resident daemon state is bounded by admitted requests, open handles, live Execs and configured caches. Nothing resident is proportional to the namespace, a file, the visited tree or a Commit |

## 4. Control operations and acknowledgement points

All records travel on the existing authenticated Noise KK channel
([Bridge native](../../../crates/layerfs-bridge/src/native/channel.rs)); plaintext
per record stays at most `MAX_PLAINTEXT_BYTES` (65519) and single control
records stay within the existing 8192-byte limit. Additions are new tags; no
existing record changes encoding or meaning, so retained F13 receipts keep their
sense. One connection carries one sequential conversation; concurrent callers
use separate connections, as today.

### 4.1 Requests and replies

| Request | Reply on success | Meaning and acknowledgement point |
| --- | --- | --- |
| `Mount { workspace, branch }` (existing) | `Bound { token, binding }` | Store/engine half. Acknowledged after the overlay `Open` job is known and the registry entry is installed. Not kernel readiness |
| `Attach(token)` (new) | `Ready(NativeMount)` | Native half. Acknowledged after I-3 holds and the registry entry records `Ready`. `NativeMount` carries the token, the mount directory, and the negotiation receipt of section 8.1 |
| `Locate(workspace)` (new) | `Located(WorkspaceStatus)` or `Refused(Missing)` | Explicit later observation by incarnation, for a caller whose `Mount` reply was lost and who therefore holds no namespace. It reads the registry only and settles no original unknown |
| `Status(token)` (existing, extended) | `Status(WorkspaceStatus)` | Adds the native fields of section 4.3. Still zero Store SQL and one indexed engine observation |
| `Commit(token)` (existing) | `Committed(outcome)` | Unchanged Store half; allowed in `Bound` and `Ready`. Live normalization is S10 (section 14) |
| `Unmount(token)` (existing, extended) | `Unmounted(token)` | Normal terminal unmount, section 11. Acknowledged after I-14 |
| `ForceUnmount { token, relinquish_unknown }` (new) | `Unmounted(token)` or `Retained(TeardownCustody)` | Explicit forced teardown, section 11.3 |
| `ExecStatus { token, exec }` (new) | `Exec(ExecObservation)` | Bounded observation of one Exec session by identity |
| `ExecCancel { token, exec, signal }` (new) | `Exec(ExecObservation)` | Explicit cancellation: signals the whole owned group, section 10.4 |
| `Fork`, `History` (existing) | unchanged | Unchanged |

The SDK exposes a typed `mount` helper that performs `Mount` then `Attach` as
two separately acknowledged attempts, each retaining its own request, phase and
original failure. It is two control exchanges per mount; each is one bounded
record pair on an existing connection.

Exec runs on a dedicated authenticated connection per Exec, so that stream
backpressure never blocks another control conversation:

```text
 host -> daemon                         daemon -> host
 ------------------------------         ----------------------------------------------
 ExecStart { token, exec, cwd,          ExecStarted { exec }            launch acknowledged
             stdin: Closed|Piped,       ExecRefused(ControlRefusal)     before-spawn refusal
             environment }
 ExecCommand(bytes)*  ExecCommandEnd
 ExecStdin(bytes)*    ExecStdinEof      ExecStdout(bytes)*  ExecStderr(bytes)*
                                        ExecStreamEof(Stdout|Stderr)
                                        ExecExited { code | signal }    root shell reaped
                                        ExecQuiescent                   owned group empty
```

`exec` is a host-selected 16-byte identity, unique within the Workspace
incarnation. A second `ExecStart` with an admitted identity is refused `Busy`;
nothing is ever executed twice on the daemon's initiative. The command text
arrives in bounded chunks and is refused only at the operating system's
single-argument limit, reported as that limit. Stream chunks are at most
32768 bytes. The daemon holds at most one unsent chunk per stream; a slow host
applies ordinary pipe backpressure to the command through the channel's own flow
control. Total output and runtime are unbounded.

### 4.2 Typed refusals

Refusals reuse `ControlRefusal { code, phase, moved, published, detail }` and the
existing codes. S8 adds phases, not message parsing.

| Situation | Code | Effects |
| --- | --- | --- |
| `Attach` on a token that is not `Bound`/`Unattached` | `Invalid` or `Busy` | None |
| `Attach` fails before `mount(2)` returns 0 | `Failed`, phase `attach:mount` | None. Entry stays `Unattached`; a new explicit `Attach` is a new operation |
| `mount(2)` returned 0 but handshake or loop start failed | `Failed`, phase `attach:session`, with `Retained` custody | The daemon detaches what it owns; if detach is not established the entry becomes `Retained` and says so |
| `Attach` outcome unknown to the daemon (owner thread lost, panic) | `Unknown` | Entry `Retained`; never a second attachment for that incarnation |
| `ExecStart` on a Workspace that is not `Ready`, or is fenced | `Invalid` / `Busy`, phase `exec:admission` | None |
| Exec capacity exhausted | `Capacity` | None |
| Launch failed before the child existed | `Failed`, phase `exec:launch`, OS code in detail | No command ran |
| Normal `Unmount` while Execs, opens, requests or Commit custody remain | `Busy`, phase `unmount:admission`, or `Unknown` for retained Commit custody | None. Workspace stays usable |
| Kernel `umount2` returned `EBUSY` after daemon gauges passed | `Busy`, phase `unmount:kernel` | Fence withdrawn. Workspace stays usable |
| Detach began and loop exit/join is not established | `Retained(TeardownCustody)` | Stopping owner retained; usability is not promised |
| New `Mount` while maintenance is stopped or declared debt headroom is exhausted | `Capacity`, phase `mount:debt` | None (D-13) |

### 4.3 Status additions

`WorkspaceStatus` gains a bounded `native` block: native state (section 5.1);
mount directory; requests in flight and parked, by prerequisite kind; open file
and directory handles; live Exec sessions (count, and at most a fixed window of
identities with their observation); owned WRITE bytes; engine `CleanupState`;
whether automatic maintenance is stopped; quarantined readers; and whether the
Store writer session is quarantined. Every field is a maintained counter or one
point observation. Status never scans, reads the Store, drives cleanup or
resolves an unknown.

## 5. State machines and ownership

### 5.1 Workspace and mount

The registry `Binding`
([registry.rs](../../../crates/layerfs-daemon/src/control/registry.rs)) is
extended; no second registry exists. The existing `Activity` keeps arbitrating
Commit and terminal transitions. A new `native` field and three gauges are
updated only under the same short registry mutex, which never spans a kernel
call, a queue wait, a Store read or a join.

```text
 Absent --Mount--> Binding --bind ok--> Bound/Unattached --Attach--> Attaching
                      |                       |    ^                    |
                bind definite fail            |    | attach definite    | I-3 holds
                      v                       |    | failure            v
                   Absent                     |    +------------------ Ready <----+
                                              |                          |        | kernel EBUSY /
                           Unmount (no native)|            Unmount/Force |        | gauges busy:
                                              v                          v        | fence withdrawn
                                           Closing --------------------> Fenced --+
                                              |                          |
                                              |                  umount2 ok
                                              |                          v
                                              |                      Detaching --loops joined--> Detached
                                              |                          |                          |
                                              +<----- logical Close job --------------------------+
                                              v                          | join/abort not established
                                           Absent (routing removed;      v
                                           reclaim debt remains)      Retained { phase }
```

| From | Event | Guard | To | Owner of the transition |
| --- | --- | --- | --- | --- |
| Absent | `Mount` | capacity, debt headroom | Binding | control Service |
| Binding | bind success | — | Bound/Unattached | control Service |
| Binding | definite bind failure | original proves no local ownership | Absent | control Service |
| Binding | uncertain `Open` | — | Retained { bind } (observable through `Locate`; today's silent placeholder is removed) | control Service |
| Unattached | `Attach` | activity `Idle` | Attaching | control Service |
| Attaching | `mount(2)`=0, handshake ok, loops running | — | Ready | mount session owner |
| Attaching | definite failure before `mount(2)`=0 | — | Unattached | mount session owner |
| Attaching | failure after `mount(2)`=0, detach established | — | Unattached | mount session owner |
| Attaching | failure after `mount(2)`=0, detach not established | — | Retained { attach } | mount session owner |
| Ready | `Unmount`/`ForceUnmount` admitted | lifecycle gate (section 11.1) | Fenced | control Service |
| Fenced | busy by gauge or kernel `EBUSY` (normal policy) | — | Ready | control Service |
| Fenced | `umount2` returned 0 | — | Detaching | mount session owner |
| Detaching | every loop exited and joined | — | Detached | mount session owner |
| Detaching | join or abort not established | — | Retained { detach } | mount session owner |
| Detached, or Unattached under `Closing` | engine `Close` acknowledged | — | Absent | control Service |
| any | engine `Close` outcome uncertain | — | activity `Uncertain`, entry kept | control Service |

Gauges on the binding: `execs` (admitted Exec sessions not yet quiescent),
`handles` (open file plus directory handles), `requests` (admitted and not
released). Exec, kernel requests and Commit may overlap freely; only terminal
admission reads all three.

### 5.2 Kernel request and reply

```text
 /dev/fuse -> dispatch loop:  decode, bind incarnation, check fence
                 |            take request credit + byte credit
                 |            copy bounded inputs (name <= 255, WRITE data <= 128 KiB, once)
                 |            move the fuser reply object into NativeRequest
                 |
                 +-- inline class (FORGET, BATCH_FORGET, refused opcodes) --> reply/no reply, release
                 |
                 +-- first step inline when it cannot wait: cached base facts, try_submit owner job
                 |
                 v
        Parked { OwnerJob | OwnerAdmission | ColdDemand }      holds: reply, credits, owned inputs
                 |  completion notifier (no polling)            holds no lock
                 v
        Runnable -> per-Workspace queue -> service worker runs ONE step
                 |        (decode, compose, plan; may park again)
                 v
        [mutation only] Published (owner job committed)  <-- linearization point
                 |
                 v
        ReplyAttempt (exactly one)  -->  release owned inputs and credits
                 |
        [mutation only] publication ticket released to the engine (ReplyAttempted)
```

| State | Owns | Leaves by |
| --- | --- | --- |
| Admitted | one request credit; owned input bytes charged to the mount's byte credit; the reply object | first step |
| Parked | the above plus one `Pending` or one flight subscription or one admission wait | the prerequisite's completion event |
| Runnable | the above | a service worker taking it in per-Workspace round-robin order |
| Published | the above plus an engine `Publication` ticket | reply attempt |
| ReplyAttempt | the reply object until `ok`/`error` returns | return of the send call; a send error is recorded, never retried |
| Released | nothing | — |

Linearization: a mutation takes effect at the commit of its single owner job
(K4). A read-class request is linearized at its single read-only owner job
(D-4). Reply order to the kernel is not controlled by the daemon and fuser gives
no delivery receipt; correctness therefore rests on I-5, I-6 and the kernel's
own `attr_version` discard of attribute replies sampled before a newer inode
version (section 8.3). Capture includes exactly the published frontier, ordered
with earlier reply attempts through the engine's existing
`Publication`/`ReplyAttempted`/`PendingPublications` mechanism [implemented and
source-verified]; it never includes unflushed userspace or mapped stores.

After a fence or connection abort, a parked request is completed with one reply
attempt (errno in section 12) before any wait for a process to exit, because a
task blocked in a request the daemon already read cannot be killed until that
reply arrives (kernel review finding 7). A prerequisite already attempted runs
to its original outcome; a published mutation stays published.

### 5.3 Lookup references and open handles

| Kernel object | Daemon owner | State kept | Released by |
| --- | --- | --- | --- |
| Node identity | The canonical inode serial is the FUSE node id; generation 0; serials are never reused [implemented and source-verified] | none | — |
| `nlookup` reference | none (D-6). FORGET and BATCH_FORGET update two per-mount gauges and do nothing else: no SQL, no table lookup, no collection | two counters per mount | not applicable |
| Open regular file (`OPEN`, `CREATE`) | engine `OpenFile` custody, one lifecycle-class owner job to acquire and one to release [implemented and source-verified] | the file-handle value encodes the engine owner; no per-open adapter table | `RELEASE`, or group retirement at detach (section 11.2) |
| Open directory (`OPENDIR`) | one cursor per handle: the last returned name (at most 255 bytes), the offset told to the kernel, and the fixed reply window | bounded by open directory handles | `RELEASEDIR` or detach |
| In-flight read after a park | engine `FileRead` processing custody where the request must outlive its handle [implemented and source-verified] | per request | reply attempt |
| Unlinked-while-open file | the engine's existing orphan domain | SQL rows | last `RELEASE` |

Consequence of D-6, stated as a boundary of the contract rather than hidden: an
inode with no name and no open handle has no custody. A later request that names
it by node id (the practical case is a process whose current directory was
removed) is answered from retained rows while they exist and with `ESTALE`
afterwards. It can never reach another inode's data, because serials are not
reused. Regular-file data after unlink is always reached through a handle and is
exact.

### 5.4 Exec, process group and streams

```text
 ExecStart admitted --> Launching --child exists--> Running
        |                  |                           |  \
   refused (no effect)   launch failed               shell reaped -> Exited{status}   (pidfd/wait event)
                         (no command)                  |             stdout EOF -> StreamEof  (pipe event)
                                                       |             stderr EOF -> StreamEof
                                                       v
                                               owned group empty -> Quiescent  (cgroup event)
                                                       |
                                              session released; `execs` gauge decremented
```

`Exited`, each `StreamEof` and `Quiescent` are independent events in any order.
A descendant that keeps a pipe open delays that stream's EOF without delaying
`Exited`; a descendant that outlives the shell delays `Quiescent` and keeps the
Workspace busy for normal unmount. Each Exec session owns: one process-custody
group, the write end of stdin if piped, the read ends of stdout and stderr, at
most one unsent chunk per stream, and its dedicated connection.

| Event | Session effect | What it does not imply |
| --- | --- | --- |
| Connection lost | Session becomes `OutputDetached`: the daemon stops reading the pipes; the command blocks on a full pipe exactly as with any stalled consumer | No kill, no timeout, no replay. `ExecStatus`/`ExecCancel` on another connection still work |
| `ExecCancel(signal)` | Signal delivered to every member of the owned group | Not completion: `Exited` and `Quiescent` still arrive as events |
| `Exited` | Status recorded and sent once | Not stream EOF, not quiescence, not a flush of mappings |
| `Quiescent` | Session released | Not a Commit and not an unmount |

### 5.5 Terminal drain

Covered in section 11; the ownership handover is: registry (fence) → mount
session owner (kernel detach, reply completion, loop join, buffer release) →
overlay Owner (logical `Close`, then bounded automatic reclamation). No step is
performed by a detached, unowned background task: reclamation is the existing
Owner thread's maintenance turn, and every other step is joined before the
operation that started it replies.

## 6. Native request service and scheduling

### 6.1 Threads and what each may do

| Thread | Count | May do | Never does |
| --- | --- | --- | --- |
| Dispatch loop (fuser `run`) | 2 per mount initially | Decode, fence check, credit admission, one bounded copy of inputs, cache-hit-only base facts, `try_submit`, inline FORGET | Wait for an owner job, a Store read, a stream consumer or another request; take the registry mutex per request |
| Service worker | `K` per daemon, fixed at readiness; initial `K = read_handles + 2` | Run one step of one request; perform a cold Store demand when it holds a reader; compose and send replies | Wait on a `Pending`, a flight or an admission credit |
| Overlay Owner | 1 per daemon [implemented] | Short typed SQL jobs and maintenance turns | Content decode, Store I/O, reply sends |
| Exec supervisor | 1 per daemon | Wait on process, pipe and group events; move at most one chunk per stream | Filesystem work |
| Mount session owner | 1 per mount, only inside `fuser::Session::run` | Own the session for its whole life and return when every loop has exited | — |

The existing engine remains the only SQL scheduler. The request service
schedules requests, not SQL: it decides which parked-then-runnable request a
worker advances next. It adds no whole-Exec or whole-Commit gate, no polling,
no batching sleep and no second queue in front of SQL beyond the Owner's own
lanes.

### 6.2 Admission and backpressure

Each mount has a request credit count `R` and a byte credit for owned inputs.
Initial settings, not optima: `R = 16`, equal to the Owner's
`jobs_per_namespace`, so one outstanding ordinary owner job per in-flight
request never exceeds the lane; owned WRITE bytes at most `R × 128 KiB`. A
dispatch loop reads the next kernel request only while a request credit is
free. When none is free it stops reading; the kernel queues and the calling
tasks block. That is the only backpressure, and by I-9 it cannot deadlock.

Owner admission can still refuse a job (`AdmissionFull`: the daemon-wide 8 MiB
credit, or the two lifecycle slots per namespace used by OPEN, RELEASE and
reply-attempt releases). That refusal is before any attempt and returns the
original command; the request parks on `OwnerAdmission` and is made runnable by
a credit-release notification. It is a readiness wait before an attempt, not a
retry, and never becomes `EBUSY` (K13). The lifecycle slot count is an explicit
`OwnerConfig` value chosen at daemon readiness for native serving; the plan
sizes it and the proof plan measures the parks. The lane shared by `Open` and
by undropped completions is sized by the same explicit value. A control `Mount`
that still meets a full lane is refused `Capacity` before any effect, as today:
that is an exact typed control result, not a kernel request, and it is not
queued.

### 6.3 Events instead of blocking ports

Today every Workspace port blocks its caller in `Pending::wait`
([completion.rs](../../../crates/layerfs-daemon/src/service/completion.rs)); the
only wake is a thread unpark. S8 generalizes that one wake: a `Pending` may be
given a notifier that, on publication or loss, places the owning request on its
Workspace's runnable queue. `try_complete` is already non-blocking. Credit
release ([credits.rs](../../../crates/layerfs-daemon/src/overlay/credits.rs))
gains the same notification for admission waiters. The synchronous `wait`
remains for control operations and tests; it is not used on the native path.

### 6.4 Request shapes and their owner jobs

Requests are resumable: a step either finishes the request or returns one
prerequisite. The Workspace crate already has this shape for mutations
(`NamespaceJob` with `Need`/`BaseFacts` rounds,
[driver.rs](../../../crates/layerfs-workspace/src/mutation/driver.rs)); S8 gives
reads the same shape and stops composing one reply from several independent
owner jobs.

| Request | Round 0 (no SQL) | Owner job | After the job |
| --- | --- | --- | --- |
| LOOKUP | Base child and inode facts for `(parent, name)` under the currently bound root, from cache or a cold demand | One read-only compound job: validates the bound root, reads the parent row, the local entry and the target row, composes the effective answer | Reply. If the job reports the root changed, one further round under the new root |
| GETATTR | Base inode facts for the serial | One read-only compound job | Reply |
| READLINK | Base target | One read-only job for the local layer | Reply |
| READ | — | One `SourceRead`/`FileRead` job returning the local window, mask and base root [implemented] | One base range demand for inherited bytes, then reply (I-10) |
| READDIR | Base page for the cursor | One read-only job for the local page | Merge in name order, reply at most one window |
| OPEN / RELEASE | — | One lifecycle job each [implemented] | Reply |
| WRITE, SETATTR, CREATE, MKDIR, SYMLINK, LINK, UNLINK, RMDIR, RENAME | Facts the evaluation needs | One mutation job (one transaction) [implemented]; `Need` adds a fact round before it | Reply, then release the publication ticket |
| FSYNC, FSYNCDIR | — | none | Success; no work and no durability claim |
| FLUSH | — | none | Success |
| xattr family | — | none | `ENOSYS` (sticky), section 8.4 |
| FORGET | — | none | no reply |
| STATFS | — | none | Inline reply from fixed declared values: block size 4096, name length 255 and a constant nonzero free-space figure. It reports no physical capacity and is not an admission signal; the library default of all zeros is not used |

A read-class job is a set of unframed read statements in one owner turn: no
write transaction and no base-source lease, because after the job the request
performs only immutable base reads keyed by an explicit root. The engine's
base-source windows remain for the multi-job sequences that still need them
(captured reads, construction).

Kernel VFS locking makes a name-level recheck unnecessary: LOOKUP holds the
parent shared while every mutation of that parent holds it exclusively, and by
I-4 nothing else changes the view. This is a design dependency on `v6.12`
directory locking with a native oracle (FP-12), not an assumption left untested.

### 6.5 Fairness

Runnable requests wait in one FIFO per Workspace. Workers serve Workspaces in
round-robin order, one step per turn. Service is non-preemptive, so the delay a
runnable request can see is bounded by the steps ahead of it in the rotation,
each of which is one bounded window. The Owner's lane rotation is fair by job
count for finite arrivals [measured diagnostic evidence: F10]; it is not proved
for sustained unequal job weights, and this specification does not claim it.
That hypothesis is H-12 in the proof plan and its possible remedy is a ranked
candidate, not a selected change.

## 7. Store read service: cache, cold admission and health

### 7.1 What exists

[implemented and source-verified] One `CanonicalCache` per daemon: a
single-mutex exact LRU keyed by `ObjectId`, default 8 MiB logical charge
(`len + 256` per entry), objects larger than the allowance bypass retention and
are still returned. A hit clones the bytes under the mutex; an insert allocates,
copies and frees evicted entries under the mutex; no lock spans provider I/O.
Readers are selected by a blind rotating counter and then a blocking mutex.
There is no coalescing of identical misses. File lengths are not cached above
Storage: every length demand reaches a reader. A warm repeat bind performs zero
object Store demands while still paying one history snapshot and the local Open
[measured diagnostic evidence: F13, F15].

These settings (8 MiB, four readers) are starting values. They are not an
optimum and not a bound on the sandbox: acquisition transients, decode arenas,
output copies, SQLite pagers, per-mount buffers and kernel caches are separate
domains (section 7.4).

### 7.2 Mandatory changes

| ID | Change | Why it is contract, not tuning |
| --- | --- | --- |
| R-1 | Cold demands go through a daemon read service: one FIFO of bounded demand batches per Workspace, served round-robin, with concurrency equal to the fixed read set. A demand runs on a service worker that holds an idle reader; no thread waits on a reader mutex | Today a scanning Workspace with as many threads as readers holds all of them and a hot Workspace's demand queues behind every cold decode. I-8 forbids the blocking wait |
| R-2 | Reader selection picks an idle, healthy reader. A reader whose session quarantined itself is removed from the rotation, never retried, and reported in status | Today a quarantined reader stays in rotation and fails a fixed share of demands until restart |
| R-3 | Each kernel request has its own `StorePorts` failure scope built from the session's `Arc<BoundWorkspace>` | Today one operation's first failure is returned to every later demand on the same scope, and the scope's mutex is held across provider I/O |
| R-4 | Mount's Branch snapshot is read through a read-only session's history provider | It removes in-process `Busy` between a mount and the same daemon's Commit publication without adding a writer gate (D-9) |

Head-of-line bound for R-1: a newly arrived demand of another Workspace waits
for at most the remaining service of the batches in progress, each bounded by
the existing demand window (4096 identities, 32 MiB). The plan records batch
size and wait per Workspace so that this bound is observed rather than assumed.
Normal batching is preserved: the read service never splits a caller's batch
into one Store call per identity and never delays a demand to form a larger one.

### 7.3 Candidates, not selected here

Internal shared ownership of cached bytes, coalescing of identical concurrent
misses, a file-length fact cache, role-segmented or scan-resistant admission and
lock sharding are ranked with their hypotheses, costs and gates in the
[mechanism ledger](S8-MECHANISM-EVIDENCE-20261008.md#4-ranked-candidates).
[#314](https://github.com/Ephemeral-AI-Lab/layerfs/issues/314) keeps miss
coalescing and alternate eviction as optional investigations, and this
specification does not override that: none becomes an S8 gate without the
demonstrated requirement its row names. Two rules bind any of them that is
later selected:

- A flight belongs to the Store, not to the request that started it. A
  subscriber that is cancelled, or whose Workspace unmounts, drops only its own
  subscription. A failed flight delivers the same original `PortError` to every
  current subscriber's own failure scope and is never re-attempted on their
  behalf. Because Storage batches are all-or-nothing, a subscriber shares the
  fate of the leader's batch; that is accepted with exact provenance, since all
  Workspaces of one daemon read under one validated whole-Store authority.
- Bytes lent to a caller from an evicted entry stay charged to a separate
  `lent` gauge until the last borrower releases them. Eviction is never reported
  as freed memory while a borrower exists.

### 7.4 Memory domains

Each is reported separately and none is summed into a single "cache bound".

| Domain | Bound today | Counter |
| --- | --- | --- |
| Retained immutable cache | 8 MiB logical charge | `ClientWork.charged_cache_bytes` |
| Demand result and decode transients | per concurrent demand, up to the 32 MiB window; at most `read_handles` concurrent after R-1 | new read-service gauge |
| Output copies | at most one 128 KiB window per in-flight READ, two to three copies | request-service gauge |
| Overlay pager and MEMORY journal | SQLite suggestion; per-transaction journal | existing engine observations |
| Store sessions | one writer plus `read_handles` pager suggestions | existing session counters |
| Owner job credits | 8 MiB daemon-wide | `OwnerWork.credited_bytes` |
| Per-mount native | `R` owned requests; two 16 MiB + 4 KiB receive buffers (virtual; residency unverified); one transient handshake buffer per attach | request-service gauges; proof plan R-domain |
| Kernel dentries, inodes, pages | kernel-owned; grows with the visited tree and file bytes | cgroup and `/proc` observations only |

A cache allowance never becomes an object, file or Commit size limit: oversized
valid objects keep bypassing retention.

## 8. Kernel profile and coherence

### 8.1 Profile

The owner-promoted candidate is the starting profile. It is a candidate until
its mounted coherence and resource proofs pass.

| Element | Setting | How it is expressed with the pinned library | Receipt field |
| --- | --- | --- | --- |
| Mount | First-party `mount(2)`/`umount2(2)` through the existing `nix` dependency; `fuser::Session::from_fd` + `run()` on the mount session owner thread | `Session::new`, `spawn`, `SessionUnmounter`, the `fusermount` helper fallback and lazy detach are not used: the helper path is silent, and fuser's unmount handle is consumed on the first `EBUSY` and reports success afterwards | mount options as seen in mountinfo |
| Options | `allow_other`, `default_permissions`, `nosuid`, `nodev`, `noatime`, `max_read=131072`, `user_id`/`group_id` of the daemon | mount data string | same |
| Access filter | `SessionACL::All` | the default `Owner` would reject every request from the Bash uid | — |
| Entry and attribute lifetime | 60 s on every entry/attribute reply | `ReplyEntry`/`ReplyAttr` TTL. CREATE and READDIRPLUS cannot carry separate entry and attribute lifetimes | — |
| Opens | `FOPEN_KEEP_CACHE`, no `FOPEN_DIRECT_IO` | open reply flags | — |
| Request window | `max_write` and `max_readahead` 131072 | `KernelConfig` setters inside `init` | negotiated values |
| Background | `max_background` 1, congestion threshold 1 | `KernelConfig` | negotiated values and fusectl read-back |
| Loops | 2, sharing the descriptor | `Config.n_threads` | thread count |
| Writeback | `FUSE_WRITEBACK_CACHE` not requested | absence asserted | selected flag set |
| Not requested, absence asserted | `AUTO_INVAL_DATA`, `ATOMIC_O_TRUNC`, open-less flags, both `KILLPRIV` flags, READDIRPLUS, PARALLEL_DIROPS, CACHE_SYMLINKS, EXPLICIT_INVAL_DATA | only `add_capabilities` exists; ASYNC_READ, BIG_WRITES and MAX_PAGES are fuser defaults that cannot be removed | selected flag set |
| Granularity | 1 ns | default | — |

The negotiation facts (ABI minor, offered and selected flags, limits, page size)
exist only inside `init`; they are recorded there into the mount receipt and
returned in `Ready`. A required capability that the kernel does not offer
refuses the attach explicitly; there is no reduced-profile fallback.

Negotiating 128 KiB does not size fuser's receive buffer: each loop allocates a
zero-filled 16 MiB + 4 KiB vector and the handshake one more transient buffer.
Two loops therefore request more than 32 MiB of address space per mount. Whether
untouched pages become resident, and what the allocation costs per attach, is
unverified and is a resource row of the proof plan, not a claim.

### 8.2 Background head-of-line limit

With `max_background` 1 exactly one background request is in userspace per
mount. Readahead READ (the normal path for cached buffered reads, because
ASYNC_READ cannot be removed), mapped-page WRITE and RELEASE are background.
While the daemon parks one of them on a cold demand, every other background
request of that mount waits in the kernel, whatever the number of loops or
workers. By I-9 this is a throughput limit, not a deadlock: several Bash
commands in one Workspace read cold data one request at a time.

The promoted value stays as the initial setting because it is the owner's
candidate. The limit is recorded as a known property with a native oracle
(FP-9) and a count hypothesis (H-9); raising `max_background` is the first
profile candidate in the mechanism ledger and needs the owner's prospective
agreement to vary a promoted element (P-6).

### 8.3 Coherence matrix

"Kernel" cites behaviour read at `v6.12`; each row has a through-mount oracle in
the proof plan (FP-10 to FP-18). The daemon sends no notification in any row.

| Event | Kernel behaviour | Daemon rule | Residual hazard |
| --- | --- | --- | --- |
| `write(2)` | Synchronous WRITE before return; full pages are marked up to date before the request is sent and cleared on error; size/mtime/ctime attributes invalidated after each write | Publish data, size and mtime atomically, then reply the full count (I-5, I-10) | A reader can briefly see a page of a write the daemon then rejects |
| Append with `O_APPEND` | Offset resolved by the kernel under the inode lock from its cached size | Always write at the kernel's offset; `Position::End` is never used on the native path | — |
| GETATTR racing a write or truncate | A reply sampled before a newer inode version, or arriving during a write/truncate, is discarded; the stat caller may still see the old values | I-6 | A reply from a view older than an already-acknowledged write would truncate the page cache: I-6 is what prevents it |
| Short READ | A short READ shrinks the kernel's size, under the same version check | I-10 exactly | — |
| CREATE, MKDIR, SYMLINK | Reply attributes applied; parent attributes invalidated | Reply the published attributes with the 60 s lifetime | — |
| UNLINK, RMDIR | Kernel drops link count locally and invalidates ctime and parent attributes | Publish, reply | D-6 boundary for an unopened removed inode |
| Hard link (`link`) and aliases | Aliases share one kernel inode per node id: one page cache, one attribute set | One serial per inode; LINK replies the published attributes | — |
| Replacement rename | Target always revalidated; replaced inode's link count dropped | One atomic job for both names | — |
| Negative lookup | An `ENOENT` error installs an uncached negative dentry | Reply `ENOENT`. Cached negative entries are a candidate only | Repeated misses cost a request each |
| `chmod`, `utimens` | SETATTR; permission checks run on cached attributes under `default_permissions` | Publish, reply published attributes; `ctime` reported equal to `mtime` | Revocation is immediate for kernel-origin changes because the reply updates the cache |
| Truncate, `O_TRUNC` | A size-changing SETATTR truncates and drops cached pages; `O_TRUNC` arrives as OPEN then SETATTR size 0 without a handle | Logical cutoff in one job | Tail pages after shrink then regrow: FP-13 |
| Symlink | READLINK buffer is one page | Targets longer than one page minus one are refused at creation; a longer target in a base root is an explicit `ENAMETOOLONG` | Page size is recorded in the receipt, not assumed 4 KiB |
| Shared mapping store | Reaches the daemon later as a background WRITE carrying the per-request `FUSE_WRITE_CACHE` flag, clipped to the current size, with no credentials and the first writable handle of that inode from any process | Accept the flag; require only that the handle is live for that inode; ignore the handle's mode and append flag; never change size; assign mtime at processing | Stores past EOF inside the last page never reach the daemon; a later extension can leave the cached page differing from daemon state (FP-15) |
| Capture during activity | No visible change | Captures the published frontier only | Mapped stores still in the kernel queue are outside it |
| Known base install | No visible change is allowed | S10. Permitted without notification only when names, bytes, links, attributes, serials and link counts are identical before and after and later active mutations are preserved; otherwise refused | — |
| A different Workspace | Separate connection, superblock, `st_dev` and caches | Nothing | — |

`FUSE_WRITE_CACHE` is a flag on one WRITE request. `FUSE_WRITEBACK_CACHE` is a
mount-wide negotiated mode in which `write(2)` returns before the daemon sees
the data. The first is accepted; the second is never requested.

The entry and attribute lifetime bounds how long the kernel may answer without
asking. It is never the mechanism that makes an answer correct, and no rule
above waits for it to expire.

### 8.4 Requests refused on purpose

| Request | Reply | Reason |
| --- | --- | --- |
| GETXATTR, SETXATTR, LISTXATTR, REMOVEXATTR | `ENOSYS` | Sticky per connection. `EOPNOTSUPP` would plausibly cost one GETXATTR per `write(2)` through the privilege-removal check |
| MKNOD of a FIFO, socket or device | `EPERM` | The canonical format has files, directories and symlinks only |
| set-id bits on files, set-gid on directories, ownership change | `EPERM` | Existing portable-metadata boundary [implemented and source-verified] |
| Hard link to a directory or symlink | `EPERM` | Existing boundary |
| Byte-range and BSD locks | kernel-local | No lock capability is requested |
| INTERRUPT | `ENOSYS` from fuser | Not supported by the pinned library; I-7 is the substitute |
| SETATTR carrying `ctime` | unreachable under this profile | Times are daemon-owned with writeback off |

## 9. Stable identity and attributes

| Field | Rule | Label |
| --- | --- | --- |
| `st_ino`, node id | Canonical inode serial; new inodes take serials from ranges reserved through History, so the number in the overlay is the number after Commit | implemented and source-verified |
| Generation | 0 | proposed design |
| `st_dev` | Assigned by the kernel per mount; differs across mounts and is outside daemon control | kernel fact |
| `st_mode`, `st_size`, `st_mtime` | Stored, 1 ns granularity | implemented and source-verified |
| `st_ctime`, `st_atime` | Reported equal to `st_mtime`; the mount is `noatime` | owner decision O-9 direction, K12 |
| `st_uid`, `st_gid` | The daemon's configured Bash identity for every inode; not stored | proposed design |
| `st_nlink` | Stored name count for files; 2 for directories | proposed design |
| `st_blocks`, `st_blksize` | Derived from size; fixed block size | proposed design |

Invariant: an unchanged file reports identical inode number, size, mtime, ctime,
mode, uid, gid and link count in every mount of every Workspace that contains
it. Fresh mounts therefore agree with each other, but the kernel's dentry,
attribute and page caches belong to one connection: a fresh mount starts with
none, and stable serials transplant nothing. Reuse across mounts comes from the
daemon's immutable cache and from data that is actually in the selected root.

Consequences that must not be overstated:

- The numeric Bash uid and gid must be the same in every daemon that shares a
  Store (D-11). They are reported in every stat and reach committed tool state
  such as `.git/index`.
- A `.git/index` imported from another filesystem records that filesystem's
  inode, device and change-time values. The first `git status` on any LayerFS
  mount of such a root therefore mismatches every tracked file and re-reads it.
  The refreshed index is an ordinary file in the overlay and is lost at terminal
  unmount unless it is committed. The fast second-mount path needs stable
  identity (S8) and a committed refreshed index (S10). How git compares index
  entries was not verified from a primary source; this is a stated dependency
  with its own count hypothesis (H-14), not a claim.
- `ctime = mtime` hides metadata-only changes from tools that rely on change
  time. A `chmod` does not advance it.
- The mounted fractional signed-minimum timestamp endpoint remains a retained
  platform FAIL: the VFS zeroes nanoseconds at the superblock time limits before
  any filesystem sees them. S8 neither repairs it nor reduces the timestamp
  contract around it.

## 10. Exec, streams and confinement

### 10.1 Launch

The daemon never runs code between `fork` and `exec` in its own image. It spawns
its own executable in a launcher mode; that process, using only safe `nix` and
standard-library calls:

1. joins the Exec session's process-custody group;
2. enters a new mount namespace and applies the propagation contract of 10.3;
3. detaches the Store volume, the overlay directory and every sibling Workspace
   mount from its namespace;
4. sets no-new-privileges if policy P-2 says so;
5. drops supplementary groups, then gid, then uid, to the daemon's Bash identity;
6. changes directory to the mount, then to the authorized relative directory,
   resolved beneath the mount without leaving it;
7. replaces itself with `/bin/bash -c <command>` and the caller's environment.

Any failure before step 7 is a launch failure with the failing step and OS code,
reported as `exec:launch`; no command ran. The launcher inherits exactly three
descriptors (the pipes); every other daemon descriptor is close-on-exec, and the
launcher asserts its own descriptor table before step 7.

### 10.2 What confinement is and is not

| Property | Mechanism | Strength |
| --- | --- | --- |
| Bash cannot open the Store or overlay database by path | Volume and overlay directory traversable only by the daemon user; detached from the launcher's namespace | Both are required: the user change defeats `/proc/<daemon>/root` and `/proc/<daemon>/fd`; the detach removes the path |
| Bash cannot reach them by descriptor | Close-on-exec everywhere, asserted | Verified per launch |
| Bash cannot abort or unmount the connection | The mount owner is the daemon user; the fusectl files belong to that user; no setuid unmount helper is reachable when P-2 is set | Depends on P-2 for helpers outside the mount |
| File access inside the mount | Kernel `default_permissions` on the reported mode with the Bash uid as owner | The only correct option: requests carry one uid and gid, and cached reads never reach the daemon |
| Separation between Workspaces of one daemon | A sibling's mount is absent from a command's namespace | **Path visibility only.** All commands of one daemon share one uid (O-24); a same-uid process is reachable through `/proc/<pid>/root`. This is not an adversarial isolation boundary and is not claimed as one |
| The working directory | Chosen under the mount | Not confinement. A command can `cd` anywhere its identity permits |

### 10.3 Mount propagation contract

Each Workspace mount point lives in its own shared peer group in the daemon's
namespace. A launcher's namespace receives its own Workspace's mount as a slave
and holds no copy of any other Workspace's mount. Required consequences:

- the daemon's plain `umount2(path, 0)` returns `EBUSY` while any process of
  that Workspace uses the mount in its own namespace, so the kernel busy check
  stays truthful;
- when it returns 0, every copy is gone, the connection ends and the loops
  observe `ENODEV`;
- a long-running command in Workspace A holds no reference that could keep
  Workspace B's connection alive.

These follow from general VFS propagation semantics that the kernel review did
not line-verify. They are therefore requirements with a dedicated native oracle
(FP-22) in the first lifecycle checkpoint, and the design is not considered
established until that oracle passes.

### 10.4 Process custody and cancellation

Custody of descendants is by a kernel-maintained group that a process cannot
leave by `setsid` or double-fork: one cgroup v2 leaf per Exec session, observed
through its population event and killed as a unit. The root shell's exit status
comes from its own exit event. If the sandbox does not delegate a writable
cgroup v2 subtree, daemon readiness fails explicitly; there is no silent
fallback to process-group signalling, which cannot see an escaped descendant.
This requirement, together with `/dev/fuse` and the capabilities for `mount`,
namespace entry and identity change, is verified against the actual environment
in the first implementation checkpoint and is not inferred from the retained
image identity.

Cancellation is explicit and has three independent parts: signal the group;
complete parked filesystem replies of that Workspace where the caller requested
forced teardown (I-7 makes this precede any wait for exit); observe `Exited`,
EOFs and `Quiescent` as events. Accepted filesystem mutations are never rolled
back.

## 11. Terminal unmount, drain and reclamation

### 11.1 Admission

Unmount admission arbitrates under the registry mutex with Exec admission,
Commit admission and the gauges, and has exactly one winner.

| Policy | Refuses when | Otherwise |
| --- | --- | --- |
| Normal | activity is not `Idle` (Commit running: `Busy`; `Uncertain`/`LocalFailure`: `Unknown` with retained custody); or `execs`, `handles` or `requests` is nonzero | Fenced |
| Forced | never for activity or gauges; `relinquish_unknown` must be explicitly true to proceed past retained Commit custody, and the result then says the publication outcome is unknown, never `NotPublished` | Fenced |

### 11.2 Sequence

```text
 Unmount(token)
   | registry: fence (no new Exec, Attach, Commit; new kernel requests refused at the loop)
   |
   +-- normal: umount2(path, 0) --EBUSY--> withdraw fence, reply Busy (no effect)
   |                           \--0------> Detaching
   +-- forced: signal Exec groups; abort the connection (umount2 MNT_FORCE);
   |           complete every parked reply; umount2(path, 0)
   v
 loops observe ENODEV and return -> mount session owner joins all loops
   | session dropped: receive buffers, cursors, owned requests released
   v
 engine: one bounded job retires this namespace's remaining open/read/request owners as a group
 engine: logical Close (existing)            -> acknowledged
 registry: remove routing                    -> reply Unmounted
   :
 Owner maintenance turns (existing): bounded row deletion while other Workspaces run and when idle
```

Rules:

- Loop exit and join is the detach evidence. Disappearance of a path from
  mountinfo is corroboration only.
- The kernel queues no FORGET once the superblock is inactive, and an abort ends
  queued RELEASE and mapped-WRITE requests. Outstanding open and request owners
  of the namespace are therefore retired as one group by a bounded engine job
  after detach, keyed by namespace and paged by the existing row windows. This
  is new engine work: today an unreleased lease, request or base-source row
  keeps a closed namespace `Held` forever, because no reclaim phase deletes
  those tables. There is no per-FORGET collection, no sweep of the base, no
  shared-cache flush, no database rebuild, no history deletion and no sync.
- Mapped stores that were still queued in the kernel when the connection ended
  are lost with the discarded local state. Normal unmount cannot reach that
  state: it requires no live user of the mount.
- Unmount never publishes, and never deletes shared objects or history.

### 11.3 Outcomes

| Outcome | Meaning | Workspace afterwards |
| --- | --- | --- |
| Busy | Refused before any terminal effect | Fully usable |
| Unknown | Retained Commit or lifecycle custody blocks a normal unmount | Usable for status; custody unchanged |
| Cancelled | Forced policy signalled Execs and completed parked replies | Proceeding to detach |
| Detached | Kernel mount gone, loops joined, session released | No longer mounted; logical close follows |
| Cleanup-pending | `Unmounted` was replied; engine reports `Queued` rows | Gone from routing; debt visible in daemon status until `Gone` |
| Complete | Engine reports `Gone` for the namespace | — |
| Retained { phase } | A teardown step was not established | The stopping owner and its exact phase are kept and reported; no success is claimed and nothing is guessed |

Logical close is reported separately from physical debt in every result.

### 11.4 Reclamation and debt

Existing behaviour is reused [implemented and source-verified]: closed-namespace
reclaim runs in bounded steps (at most 14 payload cells or 64 metadata rows per
step) on the Owner thread, one step per eight foreground jobs or when idle, and
freed pages are reused without shrinking the file. S8 adds two things. Status
surfaces closed-namespace debt and whether maintenance has stopped. Mount
admission refuses with a typed `Capacity` result while maintenance is stopped or
a declared debt headroom is exhausted (D-13), so that repeated per-call
Workspaces cannot grow the overlay without bound while every operation reports
success. One failed maintenance attempt still stops automatic maintenance with
no replay, as the S6 contract already states.

## 12. Failure, unknown and custody rules

| Situation | Reply to the kernel or caller | Retained | Never done |
| --- | --- | --- | --- |
| Definite pre-effect refusal of an operation (`Refusal`) | The mapped errno (`EEXIST`, `ENOENT`, `ENOTDIR`, `EISDIR`, `ENOTEMPTY`, `EPERM`, `EOPNOTSUPP`, `EINVAL`, `EMLINK`, `EFBIG`) | nothing | — |
| Owner admission full | none yet: the request parks | request and credits | `EBUSY` |
| Cold demand fails | `EIO` for that request | the original `PortError` in that request's failure scope and in a bounded daemon diagnostic | a second demand on the request's behalf |
| A reader session quarantined | `EIO` for the demand that observed it | reader excluded and reported | reuse of that reader |
| Mutation job outcome uncertain (owner lost mid-attempt) | `EIO` | the engine's original unknown; the Workspace's activity becomes `Uncertain` | a resend, a rollback, a guessed success |
| Serial range exhausted because the Store write was `Busy` | per P-1 (recommended `EAGAIN`) | the create has no effect | a wait, a gate or a replay of the failed reservation |
| Reply send fails | none possible | a per-mount counter and the publication, if any | a second send |
| Request on a fenced or detaching mount | `ENOTCONN` | — | new owner work |
| Parked request when the connection is aborted | one reply attempt with `ENOTCONN`; if its owner job was already attempted, that job's original outcome stands | published state | cancellation of an attempted job |
| Lost `Mount` reply | — | the entry, observable through `Locate` | a second binding for that incarnation |
| Lost `Attach` reply | — | the `Ready` or `Retained` entry | a second attachment |
| Lost `Exec` connection | — | the `OutputDetached` session and its process | a kill, a timeout or a re-execution |
| Known publication, failed local install | existing `LocalFailure` custody | the original capture and known publication | resolution by a later read |
| Later explicit observation (`Status`, `Locate`, `ExecStatus`) | the current observation | — | settling an earlier unknown |

Uncertain outcomes stay terminal until the owner rules on O-4; no observer
resolves one.

## 13. Capabilities: supported, rejected, deferred

| Capability | Disposition |
| --- | --- |
| Complete root at mount, including `.git`, ignored files, dependencies, symlinks, caches and output | Supported. Initial faithful acquisition of the historical full fixture into a sealed Store has not been performed and is a proof prerequisite |
| Several Workspaces per daemon; several concurrent Bash commands per Workspace | Supported, with the background limit of 8.2 |
| Fresh mount per tool call, and persistent multi-call Workspaces | Supported; the same contracts |
| Regular files, directories, symlinks, hard links to regular files | Supported |
| Special files, xattrs, ACLs, ownership, set-id bits, file locks across mounts | Rejected explicitly (8.4) |
| Files above 4 GiB | No size cap is introduced. Actual native files above 4 GiB remain `NOT_RUN` under the original owner's pre-S8 waiver; no new waiver is implied |
| Kernel writeback, kernel passthrough bypassing capture, permission removal, fixed CPU affinity | Rejected |
| FUSE_INTERRUPT cancellation, `batch_forget` override, notify-retrieve, capability removal, receive buffers sized to negotiation | Not available in the pinned library; not worked around by patching |
| Kernel notifications and invalidation | Not used in S8 |
| Exact attributes of an unopened inode after its last name is removed | Best effort, bounded by D-6 |
| Negative entries, READDIRPLUS, FLUSH elision, CACHE_SYMLINKS, PARALLEL_DIROPS, larger requests, more background or loops, CopyFileRange reuse | Deferred candidates, each behind its own proof |
| Interactive PTY and direct program modes | Deferred; not part of S8 |
| Live Commit of a mounted Workspace's namespace, and its survival in a fresh mount | S10 |

## 14. S10 seams

| Seam | What S8 provides | What S10 must add |
| --- | --- | --- |
| Commit from a mounted Workspace | `Commit(token)` admitted in `Ready`; capture of the published frontier while requests continue; the Store half with exact custody [implemented] | The live namespace normalizer feeding Content: names, links, metadata and changed file roots. A root constructed directly through Content in a test is not that normalizer and proves nothing about it |
| Install under a live mount | The identity rule of section 9 and the install row of 8.3 | Proof that install preserves names, bytes, links, attributes and serials, including directory link counts, and later active mutations; otherwise refusal |
| Carrying edits into a fresh Workspace | Explicit: publish them, or keep the Workspace | Cache reuse transfers no uncommitted edit |
| Second-mount tool fast paths (`git status`) | Stable identity | A committed refreshed index |
| Repeated incremental Commit on one mount | Ownership and reclamation that do not grow with Commit count [implemented in the engine] | Mounted proof over the normalizer |

No S8 checkpoint depends on S10, and no S8 proof may be relabelled as S10
coverage.

## 15. Decisions and pending owner choices

### 15.1 Decisions of this specification

| ID | Decision | Alternative rejected and why |
| --- | --- | --- |
| D-1 | Extend the existing registry `Binding` with a native state and three gauges; add `Attach`, `Locate`, `ForceUnmount`, `ExecStatus`, `ExecCancel` and the Exec stream records as additive tags | A second routing registry; a single combined mount record that would blur the two acknowledgement points |
| D-2 | First-party `mount(2)`/`umount2(2)` and `Session::from_fd`; two loops through `Config.n_threads` | fuser's mount and unmount ownership: silent helper fallback, lazy detach, a handle that reports success after `EBUSY` |
| D-3 | A daemon request service with per-Workspace runnable queues, fixed workers and completion notifiers; dispatch loops never wait | Blocking the loop in `Pending::wait`; more loops per mount as a substitute for parking |
| D-4 | One read-only compound owner job per read-class reply, with immutable base facts fetched outside it | Today's composition of up to five independent jobs with rechecks, which the engine contract already forbids |
| D-5 | Per-mount request and byte credits; a loop stops reading when none is free; owner admission refusals park | Unbounded owned queues; `EBUSY` |
| D-6 | No per-inode state for kernel `nlookup`; open handles and request reads carry custody; FORGET is a counter update | A table proportional to the visited tree (I-16); an SQL write transaction per LOOKUP reply and per FORGET |
| D-7 | Fair bounded cold-demand admission with idle, healthy reader selection and per-request failure scopes | The blind rotating counter and blocking reader mutex |
| D-8 | Group retirement of a detached namespace's remaining owners by a bounded engine job | Waiting for FORGET or RELEASE that the kernel does not send at teardown |
| D-9 | Mount reads its Branch snapshot through a read-only session; no daemon-local gate on the writer session | A writer mutex or wait, which K30 forbids |
| D-10 | Exec through a launcher mode of the daemon executable, cgroup custody, a dedicated connection per Exec with transport backpressure | `pre_exec` code in the daemon image; process-group-only custody; application-level stream credits |
| D-11 | The Bash identity is one explicit daemon configuration value, required equal in every daemon sharing a Store; mount access is `allow_other` plus `default_permissions` with the daemon as mount owner | Making the Bash user the mount owner, which would let it abort the connection |
| D-12 | Per-Workspace shared peer group, slave propagation into each launcher namespace, sibling mounts detached | Private propagation, under which the daemon's unmount succeeds while commands still use the mount and the join never completes |
| D-13 | Mount admission refuses on stopped maintenance or exhausted declared debt headroom | Nominal success with unbounded overlay growth |
| D-14 | No kernel notifications in S8; xattr family answered `ENOSYS` | Per-WRITE invalidation, which deadlocks under cached I/O |
| D-15 | Subscribers of a shared failed acquisition receive the same original failure and are never re-attempted; lent bytes stay charged after eviction | Hidden re-acquisition; reporting eviction as freed memory |

### 15.2 Choices that are genuinely the owner's

Planning and the first implementation checkpoints do not wait on these; each
row says where it starts to matter.

| ID | Question | Recommendation | First needed |
| --- | --- | --- | --- |
| P-1 | A `create` needs an inode serial; the local range is exhausted; the one reservation attempt meets a Store `Busy` (K30). K13 says nothing returns `EBUSY` for contention. Which errno does Bash see, and may the daemon attempt a refill early, at a low-water mark, so that exhaustion needs several consecutive `Busy` results? | Early single attempts at a low-water mark (each still one attempt, none replayed) and `EAGAIN` on exhaustion; no gate | Mutation checkpoint |
| P-2 | Do Exec children run with no-new-privileges? It prevents setuid helpers outside the mount from crossing the user boundary that hides the Store, and it changes what "ordinary Bash" can do (`sudo`, `su`) | Yes | The first checkpoint builds it as an explicit configuration value with no hidden default; the ruling is needed before Exec is qualified |
| P-3 | Proof budgets for full-fixture cases: the historical byte copy of the full tree alone took 10.36–12.65 s, and scoped oracles took 7.3–9.6 s natively, against the 15 s and 10 s defaults. For each of setup scope, long workloads and full-byte oracle: retain `NOT_RUN` with the conflict stated, or grant a prospective scoped exception. May a read-only sample bind one closed sealed Store without a per-sample copy, under a before/after identity proof? | Stated per case in the [proof plan](S8-PROOF-PLAN-20261008.md#8-budgets-and-pending-owner-decisions); no threshold proposed | Performance checkpoint |
| P-4 | Are new matched controls authorized at prospective identities: native ext4, and a passthrough adapter under the promoted profile? What does a "materially better" claim compare against, given that no completed LayerFS-route control exists? | Authorize native ext4 as context; decide the passthrough arm and the comparison target before registration | Performance checkpoint |
| P-5 | Which residency-proof mechanism is accepted inside the shared VM, given that a VM-wide cache drop touches the protected containers and is not itself residency proof? | A per-file residency measurement before the attempt; ineligible with zero attempts when nonzero | Performance checkpoint |
| P-6 | May `max_background`/congestion be varied from the promoted 1/1 in a prospectively registered single-mechanism arm if the head-of-line oracle confirms per-mount read serialization? | Yes, as a candidate arm; the promoted value stays the default until its proof | Concurrency checkpoint |
| P-7 | Historical cases with no admissible oracle or with an S10 dependency: E09 (nondeterministic output) stays `NOT_RUN` or gets a normalized oracle; E18/E19 run before S10 as explicitly "unrefreshed index" cases or wait | Keep E09 `NOT_RUN`; run E18 only as a labelled unrefreshed case; defer E19 to S10 | Performance checkpoint |

No S8 product implementation and no measurement was performed in producing this
specification.
