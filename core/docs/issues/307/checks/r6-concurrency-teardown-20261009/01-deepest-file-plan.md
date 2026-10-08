# R6 deepest-file plan: concurrency, sustained ownership and forced teardown

> **Status:** Dated planning checkpoint; not release evidence or a product contract.

Drafted by a planning subagent against product source `6e8cb6ede`, reviewed
and ruled on by the lead (section 10). While it was written `main` advanced to
`57244684b` (R5 tracks F and N, then fix R1).
Product source changed in five files since `6e8cb6ede`:
`layerfs-fuse/src/operations/directory.rs` (one line and comments),
`layerfs-daemon/src/control/failure.rs`, `control/operations.rs`,
`overlay/admission.rs` and `store/captured.rs`. Line references and LOC below
are at `57244684b`; fact 21 records the one change that touches this design.
This is a plan, not evidence: no
Cargo, Docker, test or measurement command was run to write it. Assignment:
[handoff](../../HANDOFF-R5-R9-20261009.md#r6-concurrency-sustained-ownership-and-forced-teardown),
input [audit](00-audit-findings.md). SPEC is `S8-SPECIFICATION-20261008.md`,
PLAN is `S8-PROOF-PLAN-20261008.md`, UM is `303/workspace-api/unmount.md`.

Global Store profile Disposable / WAL / synchronous=OFF only; Durable
`NOT_RUN — disabled by owner until explicit reauthorization`; daemon overlay
MEMORY/OFF/EXCLUSIVE. R6 is a functional stage: counted work only, no timing,
cold, storage or memory claim.

## 1. Source facts

Each fact was read in source at the lines given (paths under `core/crates/`).

**Owner admission and the H-A cycle**

1. Defaults: 8 MiB credited bytes, 64 KiB lifecycle reserve, 16 lanes, 16
   ordinary and 2 lifecycle jobs per lane
   (`layerfs-daemon/src/overlay/owner.rs:25-35`). Six classes: Read, Mutation,
   Capture, Lifecycle, OperationRecord, Source (`overlay/commands.rs:13-20`).
2. A lane keeps two counters, `ordinary` and `lifecycle`
   (`overlay/queue.rs:81-88`). `try_submit` refuses `AdmissionFull` when
   `lane.ordinary == jobs_per_namespace` for every class except Lifecycle
   (`owner.rs:264-281`) and counts Source-class jobs in `ordinary`
   (`owner.rs:301-305`). The slot returns only when the job's credit drops,
   that is when the caller drops the completion (`overlay/credits.rs:49-60`,
   `queue.rs:178-191`).
3. `Lane::take` holds back a Source-class job while an install with a smaller
   id is queued, and holds back that install while an earlier Source job is
   queued (`queue.rs:116-132`). The install then parks until
   `base_readers == 0` (`owner.rs:386-402`,
   `layerfs-overlay/src/lifetime/generation.rs:60-63`); a parked job stays in
   its queue and keeps its slot (`queue.rs:336-350`).
4. Source-class jobs are `AcquireBaseSource` (`commands.rs:323`), the native
   `Source`, `FileSource`, `HandleSource`, `OpenSource`
   (`overlay/native_job.rs:95-98`) and the directory `Read`
   (`overlay/native_directory_job.rs:64-66`). The release
   `ReleaseBaseSource` is Lifecycle (`commands.rs:304`). `InstallPrepared` is
   Capture class (`commands.rs:327`, `420-425`).
5. A kernel request takes exactly one source and drops that job's completion
   before its next job (`layerfs-fuse/src/operations/lookup.rs:170-176`,
   `operations/mutation.rs:256-272`), then needs an ordinary job: `observe`
   (Read) or `mutate` (Mutation) (`lookup.rs:189`, `mutation.rs:303`). Every
   port call waits for admission through `submit_when_available`
   (`layerfs-daemon/src/service/filesystem_port.rs:28-44`,
   `overlay/admission.rs:109-144`). A mount admits 16 requests
   (`layerfs-fuse/src/dispatch/types.rs:4`).
6. **H-A is real.** Request H holds a source and no credit. Commit queues
   `InstallPrepared` (ordinary 1). The mount's other 15 requests queue Source
   jobs (ordinary 16), all held back by fact 3. H's `observe` is
   `AdmissionFull` and waits on a credit that cannot free: install waits for
   H's release, the 15 Source jobs wait for install, and H waits for one of
   their slots. With two holders one slot stays free and the cycle does not
   close, so the trigger is exactly 16 in-flight requests of which one holds.
7. H-B is fixed at `6e8cb6ede`: `OwnerClient::submit_waiting`
   (`overlay/admission.rs:167-180`) is used by `BoundWorkspace::job` and the
   captured ports. The control `Service::job` still uses `try_submit`
   (`control/operations.rs:219-234`).
8. Capture has no such cycle: it holds back later Mutation jobs
   (`queue.rs:133-152`) and waits for reply tickets, whose `ReplyAttempted` is
   Lifecycle (`commands.rs:309`).

**Normal unmount as built**

9. Admission (`control/unmount.rs:35-63`): a `Retained` entry answers its
   stored custody first; `idle()` refuses `Committing`, `Closing` and
   `Attaching` as `Busy` and `Uncertain`/`LocalFailure` as `Unknown`
   (`control/registry.rs:126-144`); the session moves to
   `Native::Leaving { phase: Probing }` (`control/detach.rs:20-43`).
10. Sequence (`detach.rs:47-115`): one `umount2(path, 0)`
    (`layerfs-fuse/src/mount/syscalls.rs:63-65`); `EBUSY` restores Ready;
    otherwise `stop_admission`, `drain()`, `NativeJob::Revoke`, `Close`,
    registry removal (`unmount.rs:65-98`). A stop after effects stores `Kept`
    with the session as an opaque `Box<dyn Debug>`
    (`control/serving.rs:71-79`, `detach.rs:147-170`): nothing can use it
    again, so `Retained` has no way back. Activity stays `Closing`.
11. `drain()` refuses unless detached, then waits for loop join, joins owner
    and watcher, waits for `received == 0 && admitted == 0`, releases the lane
    and removes the directory (`session/drain.rs:38-88`,
    `dispatch/admission.rs:82-103`). It covers parts 1 and 2 of the SPEC
    predicate for work owned by request futures.

**Pieces forced teardown can reuse**

12. `abort_control` opens `/sys/fs/fuse/connections/<minor>/abort` write-only
    at Attach, the minor read from the daemon's own mount-table row for the
    directory it just mounted; `NotFound` gives `None`
    (`mount/syscalls.rs:68-90`, `session/startup.rs:125-133`). It is kept in
    `NativeSession.abort` (`session/state.rs:48`) and nothing writes it.
    `SessionFacts.abort_bound` and `NativeReceipt.abort_bound` report it
    (`state.rs:66-67`, `layerfs-bridge/src/control_native.rs:57-58`).
13. `MountQueue::stop_admission` sets the lane terminal and wakes receive
    waiters (`dispatch/admission.rs:70-77`); a waiter then answers `ENOTCONN`
    once (`dispatch/admission.rs:157-163`, `request/state.rs:68-75`). A parked
    task is requeued only by its own waker (`dispatch/task.rs:54-85`). There
    is no terminal completion of an admitted, parked request.
14. Any port error ends a request `Retained`, keeps its slot and fences the
    lane (`request/reply.rs:57-66`, `request/mutate.rs:89-95`,
    `dispatch/task.rs:150-166`). A definite refusal instead replies and
    releases custody (`reply.rs:67-73`, `coherence/reply_order.rs:18-23`).
15. Dropping an `Admission` or a `ReadTicket` cancels only that unattempted
    wait (`overlay/admission.rs:222-226`, `store/read_service.rs:265-266`,
    `335-341`). Dropping a `Pending` cancels nothing.
16. The first-exit watcher fences admission when any loop stops
    (`session/startup.rs:165-172`). The pinned fuser loop returns `Ok` on
    `ENODEV` and an error on anything else (`vendor/fuser-0.18.0/src/session.rs:568-569`).
    The negotiated bits recorded by R2 are async read, big writes and max
    pages; `FUSE_ABORT_ERROR` (bit 21) is not selected, so by Linux 6.12
    source an aborted device read is `ENODEV`. Not yet observed.
17. Engine `close` accepts a retained capture and then queues no physical
    cleanup (`layerfs-overlay/src/lifetime/close.rs:14-29`, `76-91`);
    `cleanup_state` is one point read giving `Live`, `Held`, `Queued` or
    `Gone` (`close.rs:48-66`). `revoke_native_mount` refuses only while a
    native source or read row remains (`lifetime/native.rs:363-374`).

**Control wire and SDK**

18. Request tags 1 to 10 are used; 11 is free
    (`layerfs-bridge/src/control_request.rs:22-73`, `84-134`). Reply tags 1 to
    13 are used; 14 and 15 are free (`control_reply.rs:22-83`, `93-135`). One
    record is at most 8192 bytes (`wire.rs:3`).
19. `TeardownCustody { token, stage, detached, work, detail }`
    (`control_native.rs:150-162`); `TeardownStage` 1 to 8 (`129-148`);
    `NativePhase` 1 to 6 with no `Stopping` (`9-24`); `Activity`
    (`control_types.rs:55-69`). A status with a native block already uses its
    own tag 13 so the tag 3 bytes stay unchanged (`control_reply.rs:36-43`).
20. SDK: `WorkspaceApi::unmount` (`layerfs-api/sdk/src/workspace/unmount.rs:12-18`),
    reply pairing in `matches_reply` (`sdk/src/control/connection.rs:177-207`),
    `OperationCause::Retained` (`sdk/src/operation.rs:22`).
21. A retained Commit's capture, reader and owner are not in the registry:
    the entry keeps only `activity` and `published`
    (`control/operations.rs:168-194`). Since `57244684b` the two custody
    states mean more than their names: `LocalFailure` is any known
    publication with something still held, including a Commit that succeeded
    but whose reader or operation owner was not released
    (`operations.rs:175-180`); `Uncertain` is every other retained custody,
    including a settled non-publication with an unreleased reader or owner
    (`operations.rs:184-190`). The registry cannot tell that last case from
    an unknown publication.

**Maintenance and observables**

22. The owner thread runs one maintenance turn after 8 foreground jobs or
    when idle, alternating live and closed work; the first failure stops
    maintenance for good (`overlay/owner.rs:403-428`, `483-503`).
    `maintenance_failure()` reads it (`owner.rs:339-345`).
23. `Command::Resources { global: true }` returns `StoredCounts`,
    `database_pages`, `free_pages` (`commands.rs:480-484`,
    `layerfs-overlay/src/database/accounting.rs:4-31`); `OwnerWork` has
    `closed_namespaces`, `maintenance_jobs`, `maintenance_rows`
    (`queue.rs:42-46`). `Harness::engine` already reads the global counts
    through a live route (`tests/support/mounted.rs:126-141`).
24. The Sandbox container has no fusectl mount and its inspection requires
    exactly the current capability, device and security set
    (`layerfs-sandbox/src/backend/docker/container.rs:19`). In that topology
    `abort_bound` is false.

## 2. Design

### 2.1 H-A fix (decision D1)

Count Source-class jobs in their own per-lane counter.

- `Lane` gains `source: usize` beside `ordinary` and `lifecycle`.
- `try_submit` checks `lane.source == jobs_per_namespace` for Source,
  `lane.ordinary == jobs_per_namespace` for the other four ordinary classes,
  and the lifecycle bound as today; it increments the matching counter.
- `State::release` decrements the matching counter; the lane leaves the
  rotation when all three are zero.

Nothing else changes. The Source queue already has capacity
`jobs_per_namespace` (`queue.rs:94-100`), so no queue grows and
`planned_bytes` keeps its formula; `Lane` grows by one word. `OwnerConfig`
gains no field: several tests build it as a full literal. The 288 notification
slots, the byte limit and the lifecycle reserve are untouched.

What changes in bounds: a lane may hold 16 Source jobs plus 16 other ordinary
jobs plus 2 lifecycle jobs, 34 instead of 18. Every bound stays fixed.
`peak_queued` and `outstanding` can therefore read higher.

Why it closes the cycle: a held-back Source job now occupies a Source slot,
which no holder needs. A holder's next job is Read or Mutation (fact 5) and
its releases are Lifecycle. A 17th Source job still waits before admission
while holding nothing. No request acquires a second source (fact 5).

Constraining tests, all expected unchanged: `owner.rs`
(`known_install_fences_later_sources_…`, `result_credits_…_ordinary_saturation`),
`admission_future.rs`, `completion_ownership.rs`, `completion_storage.rs`,
`finite_service.rs` with `examples/engine_finite`, and `product_commit.rs`
(`a_commit_waits_for_owner_admission_…`). None saturates the ordinary bound
with a Source-class job. Track A checks for a pinned `scheduler_bytes` or
`peak_queued` value in `core/benchmark` before editing and reports one if
found; it does not edit the harness.

### 2.2 Forced teardown (decision D3)

Sequence, from SPEC 862-976 and UM 122-168:

```text
 admission guard (one registry section, no effect on refusal)
   -> Stopping -> one abort write -> terminal fence (wake receivers, requeue
   parked requests) -> local drain (loops joined, no request left)
   -> one plain umount -> lane release -> Revoke -> Close -> registry removal
   -> ForceUnmounted
 any step not established -> Retained(TeardownCustody), nothing repeated
```

**Fuse owns the connection side.**

- `mount/syscalls.rs`: `abort(control: &File) -> AbortWrite`, exactly one
  `write(2)` of one byte. Return 1 is `Written`; another count is `Short(n)`;
  an error is `Failed(errno)`. No `write_all`, no `EINTR` loop.
  `abort_control` additionally binds only a row whose type is `fuse` and
  source is `layerfs`; anything else is unbound.
- `session/force.rs` (new): `NativeSession::abort_bound`, `abort` and
  `force_drain`.
  - `abort(&mut self)` refuses before any effect when no control is bound or
    the session is already detached or aborted. Otherwise it takes the
    `File` out of the session, writes once, records the result where the
    observer can read it, and on `Written` calls `MountQueue::stop_service`.
    A second write is structurally impossible. On `Short` or `Failed` it
    does nothing further.
  - `force_drain(self)` requires a recorded `Written`. It waits for loop
    join, joins owner and watcher, waits for `received == 0 && admitted == 0`,
    then makes one `syscalls::detach`, releases the lane and removes the
    directory. It stops as `Undrained` at `Join`, `Owner`, `Requests`,
    `Detach` or `Lane`. No detach is attempted unless the local drain held.
    `EBUSY` from the detach is `Undrained { stage: Detach }` with the errno:
    aborted and still mounted.
  - `session/drain.rs` is split into the shared join-and-quiesce half and the
    release half; the normal `drain()` keeps its behaviour.
- **Terminal fence.** `ports.rs` gains `Fence` (a shared stop flag plus a
  count of terminal replies) and the marker error `Fenced`. Each lane owns
  one; `MountQueue::fence()` returns it. `MountQueue::stop_service` sets the
  lane terminal, sets the fence, moves every `Parked` slot to `Queued` and
  marks every `Running` slot notified, then wakes workers and receive waiters.
  A spurious poll is already tolerated by every awaited future.
- **Port contract.** `MountServices::request` takes the fence:
  `fn request(&self, fence: &Fence)`. The daemon adapter
  (`service/filesystem_port.rs`) checks it at two points only, both before an
  attempt: before `submit_when_available` and on every poll of the admission
  wait; before `read_ticket` and on every poll of the ticket wait. A set
  fence returns `Fenced` and drops the unattempted `Admission` or
  `ReadTicket` (fact 15). A job already submitted is never abandoned: its
  `Pending` is awaited to its original result. Acquiring calls are gated:
  `source`, `open_source`, `observe`, `mutate`, `local_read`, `immutable`,
  `reserve_serial`, `directory`, `directory_read`, `directory_page`,
  `directory_cookies`, `publish_cookies`. Disposal calls are never gated:
  `release_read`, `release_source`, `reply_attempted`, `close_file`,
  `close_directory`, `forget`.
- **One terminal reply.** `request/terminal.rs` (new): when a driver's
  failure is `Fenced`, the request makes one `ENOTCONN` reply attempt,
  releases the read and source it holds through the ungated calls, counts one
  terminal reply and ends `Complete`. A failed release ends `Retained` as
  today. `ReadFailure`, `MutationFailure` and `DirectoryFailure` gain
  `fenced()` and a relinquishing consumer; `request::failure::failed` ends
  `Complete` for a fenced request that owns nothing. A publication can never
  be pending on this path: nothing gated follows `mutate`.

Drain predicate, mapped:

| SPEC part | Established by |
| --- | --- |
| 1 native admission terminal, callbacks returned, loops joined | lane `terminal`; `received == 0`; monitor `Joined`; owner and watcher joins |
| 2 every admitted request, step, decrement, completion and ticket disposed | `admitted == 0`: a request future ends only after it dropped its completions; `Revoke` refuses while a source or read row remains |
| 3 no namespace-bound Store consumer | every lease lives inside a request future (covered by 2) or a control operation (covered by 4) |
| 4 no control producer | the admission guard; original Commit knowledge is copied into the receipt, never settled |
| 5 no continuation can reach the namespace | fence set, lane released, engine mount revoked, namespace closed, registry entry removed |

**Bridge owns the records.** New file `control_forced.rs`; all additive.

| Record | Encoding |
| --- | --- |
| `Request::ForceUnmount { token, relinquish_unknown }` | request tag 11: token, then one strict boolean byte |
| `Reply::ForceUnmounted(Box<ForceUnmounted>)` | reply tag 14: token, facts, `NativeWork`, cleanup byte |
| `Reply::Retained` with forced facts | reply tag 15: the tag 12 body, then facts. `TeardownCustody.forced: Option<ForcedFacts>`; `None` still encodes as tag 12 with identical bytes |
| `ForcedFacts { abort, detach, commit, fenced }` | abort byte (`Written` 1, `Short` 2, `Failed` 3), detach byte (`NotAttempted` 1, `Detached` 2, `Busy` 3, `Failed` 4), commit knowledge (`Absent` 1; `Published` 2 then the existing outcome encoding; `Unknown` 3), `u64` |
| `ForcedCleanup` | `Held` 1, `Queued` 2, `Gone` 3, `Unobserved` 4 |
| `NativePhase::Stopping` | value 7 in the existing native status block |
| `TeardownStage::Abort` | value 9, accepted only under tag 15 |

Unknown tags, bytes and booleans are refused as today. Every record stays
under 8192 bytes; track W proves the worst case by a round trip at maximum
field sizes. `Absent` means no Commit custody was retained at admission; it is
not a not-published verdict.

**SDK.** `WorkspaceApi::force_unmount(token, relinquish_unknown)` in
`workspace/unmount.rs`. The flag is a required argument with no default.
`matches_reply` pairs `ForceUnmount` with `ForceUnmounted` and `Retained` by
token.

**Daemon control owns admission, order and outcome.** New file
`control/force.rs`.

Admission, in this order, under one registry lock:

| Condition | Answer | Effect |
| --- | --- | --- |
| entry `Retained` | the stored custody as `Retained` | none; no second abort or umount ever |
| activity `Committing`, `Attaching` or `Closing` | `Busy`, phase `force:admission` | none |
| native `Unattached` | `Invalid`, phase `force:admission` | none |
| native `Attaching` | `Unknown`, phase `force:admission` | none |
| activity `Uncertain` and `relinquish_unknown == false` | `Unknown`, phase `force:custody` | none |
| no abort control bound | `Failed`, phase `force:capability` | none; the Workspace stays Ready |
| otherwise | admitted | `Leaving { phase: Stopping }`, activity `Closing` |

Commit knowledge copied at admission: `Idle` gives `Absent`; `LocalFailure`
gives `Published(binding.published)` and needs no flag; `Uncertain` with the
flag gives `Unknown`. By fact 21 `Unknown` is then the conservative answer
for a settled non-publication with held custody, and both custody states
close with the engine capture, reader or owner still `Held`. Then, outside
the lock:

1. `session.abort()`. `Short` or `Failed`: `Retained` at stage `Abort`.
2. `session.force_drain()`. `Undrained`: `Retained` at that stage with
   `abort: Written` and `detach` `NotAttempted`, `Busy` or `Failed`.
3. Phase `Draining`; `Revoke`; `Close`; one `CleanupState` observation whose
   failure is `Unobserved`; registry removal; `ForceUnmounted`.

`Kept` gains the forced facts so that a later `Unmount`, `ForceUnmount` or
`Attach` returns the same custody, with them. After success the token is
`Missing`. `Unmount::close` takes the reply to send. Outside Linux the request
is `Invalid`.

With `relinquish_unknown == true` the namespace is closed with its capture,
reader and operation owner still in the engine: cleanup reports `Held`, the
receipt says `Unknown`, and no engine release of that custody is attempted
(fact 17 and the engine's own rule that unknown disposition keeps the
capture).

### 2.3 H-C (decision D2)

No mechanism is added. A request that ended `Retained` keeps its source; an
install queued behind it parks; the Commit thread waits on it, so activity is
`Committing`. Normal unmount and Force are both `Busy` before any effect. The
only exit is daemon stop: `Shared::stop` returns the install unattempted
(`queue.rs:370-389`), the Commit ends with a known publication and no local
install, and the published Commit is readable from a fresh mount. The same
limit covers a retained request without a Commit: Force is admitted, aborts,
and stops `Retained` at `Requests` with no detach.

### 2.4 Not built, and why

- No fusectl mount by the product, no connection number lookup at teardown,
  no `MNT_FORCE`, `MNT_DETACH`, retry, second abort or second umount, no
  signal, no wait on a process or a stream.
- No Sandbox topology change (D4). In the Sandbox topology Force is therefore
  always `force:capability`.
- No recovery out of `Retained`, no disposal of a retained request's custody,
  no interruptible Commit (D2).
- No per-binding drain gauges and no status field beyond the phase value:
  adding a field changes existing record bytes.
- No terminal fence on the normal path: the kernel's `EBUSY` already excludes
  blocked callers there and R2's verified behaviour stays.
- No `mount:debt` refusal and no low-water serial refill: neither has a value
  in source or a hook-free proof (D5, open points 3 and 4).
- No daemon-wide graceful stop.
- No test hook, fault injection or test-only API in product source.

No file R6 touches is near a ceiling. Largest touched: `request/callbacks.rs`
682 physical lines, `overlay/owner.rs` 503, `overlay/queue.rs` 390,
`service/filesystem_port.rs` 353, `control_native.rs` 350. The largest file
in these crates, `overlay/commands.rs` at 829, is not touched. Every
`mod.rs`/`lib.rs` involved is at most 22 lines and gains declarations only.

## 3. Files

Production LOC from the pinned `tools/production_loc.py --files` (SHA-256
`c0fe7f36…24adb`) run on the working tree at `57244684b`, whose product
source is committed. Only files R6 plans to add or change are listed, so each
bracket covers the listed files only.

```
layerfs-daemon/src/                 [2183]  (part)
  control/                          [1002]  (part)
    detach.rs                         154   (retain carries forced facts)
    failure.rs                        234   (only if a forced refusal needs a bounded detail)
    force.rs                            —   new (guard, order, outcome)
    mod.rs                             19   (declaration)
    native.rs                          82   (only if the Stopping block needs its own arm)
    operations.rs                     222   (ForceUnmount routed)
    serving.rs                        160   (Kept and custody carry forced facts)
    unmount.rs                        131   (close takes its reply; non-Linux refusal)
  overlay/                           [833]  (part)
    owner.rs                          470   (Source bound in try_submit)
    queue.rs                          363   (Lane.source; release)
  service/                           [348]  (part)
    filesystem_port.rs                348   (fence at the two waits; request takes the fence)
```

```
layerfs-fuse/src/                   [3642]  (part)
  ports.rs                            129   (Fence, Fenced, request signature)
  dispatch/                          [412]  (part)
    admission.rs                      241   (fence, stop_service)
    mod.rs                             14   (only if an export is needed)
    queue.rs                          157   (lane owns its fence)
  mount/                             [123]  (part)
    mod.rs                              5   (export AbortWrite)
    syscalls.rs                       118   (one abort write; fuse/layerfs row check)
  operations/                        [937]  (part)
    directory.rs                      323   (fenced, relinquish)
    lookup.rs                         217   (fenced, relinquish)
    mutation.rs                       318   (fenced, relinquish)
    read.rs                            79   (only if the data failure needs its own arm)
  request/                          [1583]  (part)
    callbacks.rs                      657   (request call site)
    directory.rs                      219   (terminal branch; request call sites)
    failure.rs                         70   (fenced request with no custody)
    inline.rs                          72   (request call site)
    mod.rs                             11   (declaration)
    mutate.rs                         227   (terminal branch; request call site)
    reply.rs                          156   (terminal branch)
    state.rs                          171   (request call site; lane fence)
    terminal.rs                         —   new (one terminal reply, relinquish, count)
  session/                           [458]  (part)
    drain.rs                           86   (shared join-and-quiesce and release halves)
    force.rs                            —   new (abort, force_drain)
    mod.rs                              8   (declaration, exports)
    startup.rs                        157   (literals gain the new fields)
    state.rs                          207   (abort facts, Forced, refusal type)
```

```
layerfs-bridge/src/                  [967]  (part)
  control.rs                          144   (exports)
  control_forced.rs                     —   new (forced records and codec)
  control_native.rs                   282   (Stopping, Abort, custody.forced)
  control_reply.rs                    280   (tags 14 and 15)
  control_request.rs                  139   (tag 11)
  control_types.rs                    105   (two variants)
  lib.rs                               17   (declaration)
```

```
layerfs-api/sdk/src/                 [199]  (part)
  control/                           [187]  (part)
    connection.rs                     187   (reply pairing)
  workspace/                          [12]  (part)
    unmount.rs                         12   (force_unmount)
```

`layerfs-sandbox` is not changed. No manifest, lockfile or schema changes.
Architecture: new `80-forced-teardown.md`; updates to 21 (slot accounting),
75 (fence and gated ports), 76 (forced sequence, abort use), 79 (H-A limit
removed, H-C limit kept), the index and `core/AGENTS.md`, each in the commit
that changes the source.

Tests (outside `src`, not counted; new unless marked):

```
layerfs-daemon/tests/
  install_slots.rs              R6-1 at owner scope (host and Linux)
  fenced_port.rs                the port's two gates, no mount (host and Linux)
  filesystem_port.rs            existing: request call sites
  mounted_commit_failures.rs    existing: request call sites
  mounted_concurrency.rs        Linux: R6-1 mounted, R6-2, R6-3, R6-6
  mounted_parking.rs            Linux: FP-8, FP-34 admission half
  mounted_cycles.rs             Linux: R6-7, R6-8, FP-24
  mounted_failure_scope.rs      Linux: FP-27, P-1
  mounted_fusectl.rs            Linux: FP-2 fusectl half, FP-9
  mounted_drain.rs              Linux: FP-21, FP-29, FP-31
  forced_unmount.rs             Linux: FP-33, FP-19-FS, FP-22-FS, FP-34 terminal half
  forced_producers.rs           Linux: FP-32
  forced_unavailable.rs         Linux: force:capability, fusectl never mounted
  support/mounted.rs            existing: try_force helper
  support/fusectl.rs            mount once, own-connection read-only accessors
  support/holds.rs              held owner credits, held read leases, bounded release
  support/history_gate.rs       blocking HistoryCatalog wrapper with bounded waits
layerfs-fuse/tests/
  fence.rs                      stop_service wakes receivers and requeues parked tasks
layerfs-bridge/tests/
  control_golden.rs             pre-R6 encodings of tags 1-10 and 1-13 pinned
  forced_records.rs             round trips, bounds, refusals of the new records
```

## 4. Tracks

| Track | Write set | Depends on |
| --- | --- | --- |
| A | `layerfs-daemon/src/overlay/{owner.rs,queue.rs}`, `tests/install_slots.rs`, architecture 21 and 79 | — |
| U | all listed `layerfs-fuse` files, `layerfs-fuse/tests/fence.rs`, `layerfs-daemon/src/service/filesystem_port.rs`, `tests/fenced_port.rs`, call sites in `tests/filesystem_port.rs` and `tests/mounted_commit_failures.rs`, architecture 75 | — |
| W | all listed `layerfs-bridge` and `layerfs-api/sdk` files, the two bridge tests; two compile shims in `control/operations.rs` and `control/serving.rs` | U committed before W builds the daemon |
| D | `layerfs-daemon/src/control/*` as listed, architecture 76 and 80, `core/AGENTS.md` | U, W |
| T | the three new support files, `support/mounted.rs` | D |
| P1 | `mounted_concurrency.rs`, `mounted_parking.rs` | A, T |
| P2 | `mounted_cycles.rs`, `mounted_failure_scope.rs` | A, T |
| P3 | `mounted_fusectl.rs`, `mounted_drain.rs` | T |
| P4 | `forced_unmount.rs`, `forced_producers.rs`, `forced_unavailable.rs` | D, T |

Order: A alone first. Then U and W edit in parallel but build only their own
packages (`-p layerfs-fuse`; `-p layerfs-bridge -p layerfs-sdk`); the lead
commits U, then W builds the daemon with its shims and is committed; then D;
then T; then P1 to P4 in any order. W's first step, before any edit, is
`control_golden.rs` passing on unchanged source. A's first run of
`install_slots.rs` is on unchanged source and is kept as the failing receipt.
Receipts: `checks/r6-concurrency-teardown-20261009/<track>-attempt<N>-<binary>.txt`
on the host and `…-linux-<binary>.txt` on Linux, with
`RX_STAGE=r6-concurrency-teardown-20261009`.

Frozen interfaces:

```rust
// layerfs-fuse::ports (U; used by D through the daemon adapter U also owns)
#[derive(Clone, Debug, Default)] pub struct Fence(/* shared */);
impl Fence { pub fn stopped(&self) -> bool; pub fn terminal_replies(&self) -> u64; }
#[derive(Clone, Copy, Debug)] pub struct Fenced;            // Display + Error
pub trait MountServices: Send + Sync {
    fn request(&self, fence: &Fence) -> Result<Arc<dyn RequestServices>, ServiceError>;
}
// layerfs-fuse::dispatch
impl MountQueue { pub fn fence(&self) -> Fence; pub fn stop_service(&self) -> Result<(), DispatchError>; }
// layerfs-fuse::mount
pub enum AbortWrite { Written, Short(usize), Failed(nix::errno::Errno) }
// layerfs-fuse::session (U -> D)
pub enum ForceRefusal { AbortUnavailable, NotServing }
pub struct Forced { pub abort: AbortWrite, pub detach: Option<nix::errno::Errno>, pub terminal_replies: u64 }
impl NativeSession {
    pub fn abort_bound(&self) -> bool;
    pub fn abort(&mut self) -> Result<AbortWrite, ForceRefusal>;
    pub fn force_drain(self) -> Result<Box<Drained>, Box<Undrained>>;
}
// Drained and Undrained each gain `pub forced: Option<Forced>`; DrainStage is unchanged.

// layerfs-bridge::control (W -> D, SDK, tests)
Request::ForceUnmount { token: WorkspaceToken, relinquish_unknown: bool }   // tag 11
Reply::ForceUnmounted(Box<ForceUnmounted>)                                  // tag 14
pub struct ForceUnmounted { pub token: WorkspaceToken, pub outcome: ForcedOutcome }
pub struct ForcedOutcome { pub facts: ForcedFacts, pub work: NativeWork, pub cleanup: ForcedCleanup }
pub struct ForcedFacts { pub abort: AbortDisposition, pub detach: DetachDisposition,
                         pub commit: CommitKnowledge, pub fenced: u64 }
pub enum CommitKnowledge { Absent, Published(CommitStagedOutcome), Unknown }
// TeardownCustody gains `pub forced: Option<ForcedFacts>` (tag 12 when None, 15 when Some)
NativePhase::Stopping = 7;  TeardownStage::Abort = 9;

// layerfs-sdk
impl WorkspaceApi<'_> {
    pub fn force_unmount(&mut self, token: WorkspaceToken, relinquish_unknown: bool)
        -> Result<ForcedOutcome, Box<OperationFailure>>;
}
// layerfs-daemon (D -> tests): Service::execute_control(&Request::ForceUnmount { .. });
// refusal phases "force:admission", "force:custody", "force:capability".
```

## 5. Proof rows

Every mounted test runs only in the pinned Linux container with Store, Overlay
and scratch on the container's native filesystem. Every wait is bounded; no
thread's exit depends on another thread finishing without panicking; holds
are released by a guard on every path. Each binary has at most six tests and
must finish well inside 100 s.

**fusectl safety rule (D4), binding on product and tests.** The fusectl mount
in the test container lists connections of other containers on the same
Docker VM, and protected containers run FUSE mounts there. Nothing may open
for writing, or write, any file under `/sys/fs/fuse/connections` except the
product's own `abort_control` on the minor of its own mount row. Tests never
write there at all, never glob or iterate it for anything but reading their
own entry, and read only `<minor>/` after checking that `minor` equals both
`ReadyMount.receipt.device_minor` and the test's own mount-table row for that
directory. No script contains a redirect into that tree. Reviewers grep for
this.

| Row | Requirement | Hook-free route | Observable asserted | Can close |
| --- | --- | --- | --- | --- |
| R6-1 | Install completes under slot pressure (H-A) | Owner scope: one held source, `Install` queued, 15 `AcquireBaseSource` queued, then a Read job on the held source. Mounted: 24 external processes reading and writing through three Commits | The Read job is admitted and completes; a 17th Source job is `AdmissionFull`; after release the install and all 15 sources complete on the new root; `outstanding` returns to 0. Mounted: every Commit returns, every process exits 0 | Whole row at owner scope; mounted part labelled "no stall observed", not the exact interleaving |
| R6-2 | Commit against 16 or more writers on one Workspace | 20 external writers appending numbered files during a Commit | Commit is not refused; the committed root holds an exact prefix of each writer's sequence; the mount shows all; the next Commit holds the rest; fresh-mount oracle | Whole row |
| R6-3 | Commit on A while sibling B writes | Two mounts; B's writer runs through A's Commit | A's root has none of B's names; B's `completed` count advances between Commit start and return; B's later Commit is exact | Whole row |
| FP-8 | With both loops holding parked requests, an unrelated request on the same mount and on another Workspace completes | All Store read leases held by the test; cold readers park on A; then a read of a file written earlier through the mount (no base demand) on A, and a write on B | The unrelated requests complete while `parked > 0` on A; after release all readers return exact bytes | Whole row if the lease-free request really needs no lease; otherwise a finding |
| FP-34 | Held units never exceed R+N; idle reservations are not requests; stop wakes every borrowed callback; another Workspace progresses | Same hold with 18 or more cold readers. Terminal half in `forced_unmount.rs` | `admitted <= 16` and `received <= 2` at every observation; B progresses. On Force: `terminal` rises by the blocked callbacks, every reader returns an error, none is signalled | Admission half in P1, terminal half in P4; whole row only with both |
| FP-9 | A second cold cached read queues in the kernel behind a parked readahead READ; foreground proceeds | fusectl mounted; leases held; two processes read two large cold files buffered | Own connection's `waiting >= 2` while the mount's READ count is 1; a local write completes meanwhile; both reads complete after release | Whole row if the first READ is a background request; else `PARTIAL` with the observed classification |
| FP-2 fusectl half | mountinfo and fusectl values agree with the receipt | fusectl mounted before Attach | `abort_bound == true`; own `max_background` and `congestion_threshold` equal the receipt | The fusectl part; observed maximum request size stays unrun |
| R6-6 | Finite fair arrivals on two Workspaces | A: 8 processes, 400 writes of 128 KiB; B: 2 processes, 200 small operations; concurrent | Both finish; per-mount `completed` at least the expected floor; every owner class advanced; no `retained` | Finite completion by counts; no share ratio (H-12 has no declared number) |
| R6-7 / FP-26 | Closed-namespace count and database pages plateau over repeated mount, write, Commit, unmount, live and idle | 12 cycles beside one anchor Workspace; idle variant waits for maintenance after each unmount; live variant keeps a sibling writer running | After cycle 2 every `StoredCounts` field equals its value after cycle 1; `closed_namespaces` equals unmounted Workspaces with no extra API call; `database_pages` does not grow over the last six cycles; no maintenance failure. Monotone values (ids, generations, cumulative counters, Store history) are recorded, not asserted | Plateau half. The refusal half (`mount:debt`, stopped maintenance refusing Mount) is `NOT_RUN`: not implemented |
| R6-8 | An output-backpressured caller neither blocks Commit nor is touched by unmount | A process holding a descriptor, cwd and mapping in the mount blocks on a full stdout pipe | Commit includes its earlier writes; Unmount is `Busy` at `unmount:kernel`; the process is alive and unsignalled; after the pipe is read it exits 0 and Unmount succeeds | Filesystem side only; stream delivery belongs to the runtime rows |
| FP-24 | Simultaneous mounts over different and related roots; isolated state; a stale token never redirects | One daemon: a Branch, a fork of it and a Workspace on a committed descendant; same name written differently in each | Each mount and each fresh-bind oracle shows only its own bytes; a token of an unmounted Workspace is `Missing`; a wrong namespace is `Invalid` and changes no epoch | Whole row |
| FP-27 | A quarantined reader gets no further demand; one request's cold failure does not fail a later one | Store composed from public pieces as in `mounted_commit_failures.rs`; one reader quarantined as `store_read_service.rs` does; cold reads through the mount | `quarantined == 1`; at most the observing request fails with `EIO`; later reads are exact; the quarantined reader's demand count stays | Whole row at mount scope |
| P-1 | Exhausted serial range plus one `Busy` reservation is `EAGAIN` with no effect and no second attempt | `HeldWriter` holds the Store writer; `touch` on a Workspace with no local range | `EAGAIN`; the name is absent; no reservation succeeded; after release a later `touch` succeeds | The exhaustion half. The low-water early refill is `NOT_RUN`: not implemented |
| FP-21 | Detach and joined loops alone do not retire; held completions keep Close waiting or `Retained` | Both lifecycle credits of the lane held by the test; a descriptor closed so its RELEASE parks before admission; normal Unmount | Released inside the window: `Unmounted`. Not released: `Retained` at `Requests`, engine mount `Live`, namespace not closed. After a complete drain: counts return to baseline and `CleanupState` reaches `Gone` | Owner-completion half. A Store consumer held by the test is not a namespace-bound consumer and is recorded as not staged |
| FP-29 | A processing read spanning the release of its handle | Leases held; a cold READ parks on one descriptor while a sibling handle closes and the name is unlinked from warmed facts | The read returns exact bytes after release; `fstat` reports link count 0 | The remaining part; with R3's part the whole row |
| FP-31 | Exact partial and batched FORGET; bounded retirement of K outstanding lookups | 2,000 lookups; cgroup `memory.reclaim`; then a detach with K outstanding | `forget_units` matches the engine decrement; after detach `maintenance_rows` covers K in turns of at most 64 with no scan | Mounted halves. Foreign incarnation and underflow stay at component scope |
| FP-17 | Whole-row verdict | No new test: `native_mount` and `native_mutation` both run in the final suite at one identity | Both pass at the R6 final identity | Whole row, from the final-suite receipts |
| FP-33 | One abort write, one plain umount; a held cwd gives aborted-but-mounted `Retained`; no fallback, no replay | Clean case; then an external process with its cwd in the mount | Clean: `ForceUnmounted` with `Written`, `Detached`; mount row and own connection directory gone; token `Missing`. Held: `Retained` at `Detach` with `Written`, `Busy`, `detached == false`; the row remains; the holder is alive and later sees `ENOTCONN`; a second Force and an Unmount return the equal custody; nothing changes after the holder exits | Whole row for the outcomes staged. The syscall counts are the product's receipt and its single call sites; no independent syscall trace exists |
| FP-32 | Force is before-effect `Busy` in every active Commit phase; attempted work past loop exit blocks Close; outcomes survive | Commit held before Save finish by a blocking closure through `Service::execute`; before publication and before install by `history_gate.rs`; unknown and known-publication custody as R5 tracks F3 and F4 stage them; both lifecycle credits held so a published write's ticket release cannot be admitted | Each hold: `Busy` at `force:admission`, the mount still serves, the own connection still exists, the Commit then ends with its own outcome. Unknown without the flag: `Unknown` at `force:custody`, no effect. With the flag: `ForceUnmounted`, `commit: Unknown`, cleanup `Held`. Known publication: `commit: Published` equal to the registry's. Held ticket: released in the window gives `ForceUnmounted`; otherwise `Retained` at `Requests`, no detach, engine mount `Live`, revision unchanged | Whole row except a cold-read consumer held inside a provider read, which has no hook-free hold |
| FP-19-FS | Force releases a blocked syscall without signalling; attempted work keeps its result and guard | Leases held; a cold reader parks before its demand; Force | The reader exits non-zero by its own status with `ECONNABORTED` or `ENOTCONN` (recorded); `fenced >= 1`; `ForceUnmounted` returns while the test still holds the leases. Attempted half is FP-32's held ticket | Whole row with FP-32's case |
| FP-22-FS | External references give truthful Busy; A's reference never holds B; Force need not kill | Holder in A; normal Unmount of B; Force on A; then a write on B | B unmounts or serves normally; A is aborted-but-mounted `Retained`; the holder is alive | `PARTIAL`: proven in the test container's single mount namespace, not the Sandbox topology, where Force is refused |
| FP-23-FS | Refusal before effect, else exactly one abort, disposition, drain and detach; no process kill | Union of FP-32 and FP-33 | Both pass | Whole row when both pass |
| capability | Missing abort control is a typed refusal that leaves the Workspace Ready | A mount attached with fusectl never mounted | `Failed` at `force:capability`; phase Ready; a read works; normal Unmount succeeds | Whole |
| FP-15 header identity | Writeback request header identity | none | — | `PARTIAL` as in R3; no accounting of flagged WRITE headers exists and none is added (D5) |
| FP-16 fixture half | A GETATTR held across a WRITE | none | — | `NOT_RUN`; no real-resource hold exists after the attribute is sampled (D5) |

A partial-scope receipt never closes a whole row.

## 6. Rules for every track

1. One checkout, no worktrees. Only the lead runs `git add`, `commit`,
   `stash`, `checkout` or `reset`, counts LOC, edits the ledger or a
   completion record, or removes anything. No push, pull request or publish.
2. Each implementing subagent edits only its listed files. A needed change
   elsewhere is reported, not made. An ambiguity, a conflict with source or an
   unexplained result stops the subagent; it does not ask the owner.
3. One Cargo, Docker or test command at a time under the lock
   (`core/target/r4-locked.pl` through `r4-build.sh`, `rx-run.sh`,
   `rx-linux.sh`). A busy lock is not an attempt.
4. Host cargo is `cargo +1.85.1 --locked`. Linux uses image
   `sha256:378b799ef43343fc64008b6a5ef456dd6bf0cb8f6ea4cec72e7b42dfc17d2cd6`.
5. Every test command has a wall limit of 100 s. Build with `--no-run` first.
   Never loop, background or output-filter a test. A test at its limit FAILED
   as a hang. Receipts are append-only; failed attempts are kept and never
   relabelled.
6. Disposable/WAL/synchronous=OFF selected explicitly; Durable is
   `NOT_RUN — disabled by owner until explicit reauthorization`; overlay
   MEMORY/OFF/EXCLUSIVE; no `fsync`-family call on Disposable backing.
7. `LAYERFS_CONSTRUCTION_WORKERS=1`; one construction producer; never seal
   per file or per Commit.
8. One attempted operation: no retry, replay, refresh or busy handler; the
   first original failure is kept; unknown outcomes are retained.
9. Product source only under `crates/<package>/src`, with no test hook. No new
   dependency; no third-party edit beyond the authorized fuser patches.
10. The fusectl safety rule of section 5.
11. Do not touch containers `9cf2fe345496…`, `ce75ac504df9…`,
    `d2433851ea59…`, `d2550144998b…`, the exited `layerfs-e04-*` and
    `cf-build-probe` containers, or
    `/var/folders/s4/xpkmz7wn6yq97w1ls_4f_dfc0000gn/T/layerfs-installed-56560-full-native-handoff`.
    Remove only containers and volumes you create.
12. Do not stage or modify `HANDOFF-PRE-S8-SERVERLESS-20261007.md`,
    `HANDOFF-S7-S9-RESUME-20261006.md`, `S7-S9-SPEED-TEST-PLAN.md` or
    `checks/multi-workspace-model-confirmation-20261008/04-commit-confirmation.json`.
13. No timing, cold, storage or memory claim. Only the lead measures.

## 7. Decisions taken without the owner

| # | Decision | Alternative | If the owner rules otherwise |
| --- | --- | --- | --- |
| D1 | Fix H-A in product with a per-lane Source counter bounded by `jobs_per_namespace` | Refuse Source admission while an install is queued; or a new `OwnerConfig` field | Either is a local change in `owner.rs` and `queue.rs`; the proof stays |
| D2 | No recovery for an install parked behind a retained request, nor out of `Retained` | An explicit relinquish of a retained request's custody | New product work under a contract that does not exist yet |
| D3 | Forced teardown per SPEC with request tag 11 and reply tag 14 | — | — |
| D4 | Sandbox topology unchanged; tests mount fusectl themselves | Mount fusectl in the Sandbox container | A Sandbox and topology-inspection change; the forced rows are then rerun in that topology |
| D5 | FP-16 fixture half and FP-15 header identity stay unrun | A holding fixture in the daemon | Forbidden by the product-source rule unless the owner changes it |
| D6 | No dependency, format, profile, negotiated-profile or schema change | — | — |
| D7 | `Retained` with forced facts uses a second new reply tag, 15, as R2 did for status | Put the facts in `detail`; or change tag 12 | Tag 12 must stay byte-identical; text is never parsed |
| D8 | New values `NativePhase::Stopping = 7` and `TeardownStage::Abort = 9` in existing records | A separate forced status record | Old values keep their bytes; a peer older than R6 refuses the new value |
| D9 | Missing abort control is `Failed` at `force:capability` | `Invalid`, as Attach without native serving | One constant |
| D10 | Force is admitted only from Ready | Force on an Unattached Workspace as a local disposal | See open point 6 |
| D11 | `relinquish_unknown` closes the namespace and leaves the engine capture `Held` | Release reader, owner and capture locally | A bounded job sequence after Close; the receipt still says `Unknown` |
| D12 | The normal unmount path gets no terminal fence | Fence at known detach, as SPEC 360-366 reads | One call in `detach()`; R2's proofs are rerun |
| D13 | Local drain before the one detach; no detach if the drain did not hold | Detach first | Reorders `force_drain` |
| D14 | A short or failed abort write attempts nothing further | Still fence and detach | Changes stage `Abort` handling only |
| D15 | Forced `Revoke` and `Close` use `try_submit`, as the normal path | `submit_waiting` | See the risk on the credit race |
| D16 | `MountServices::request` takes the fence | A daemon-side wrapper type | The wrapper cannot be built by a public-API test |
| D17 | The terminal reply errno is `ENOTCONN`; the kernel's own abort releases the caller | — | One constant |
| D18 | `mount:debt` and low-water refill are not built in R6 | Build them unproven | They have no value in source and no hook-free proof |

## 8. Risks

- **Riskiest product change: the terminal fence across three request
  drivers.** A wrong branch would end a failed request `Complete` and drop
  custody. Guard: `Fenced` is produced only before submission, at two places;
  `fenced_port.rs` proves that and that disposal calls are never gated; P4
  proves the mounted path. If the lead wants a smaller first step, gate at
  call entry only and do not cancel an in-progress wait: parked requests then
  drain when their resource frees or the teardown ends `Retained`, and
  FP-19-FS weakens to that.
- `stop_service` re-polls every parked future once. `Admission`, `ReadTicket`
  and `Pending` re-register a waker on each poll in source; track U confirms
  it for every awaited future before relying on it.
- Kernel behaviour after abort (device read `ENODEV`, callers
  `ECONNABORTED`, plain umount of an aborted mount) is read from kernel
  source, not yet observed. P4's first receipt settles it; a different errno
  is a plan amendment, not a retry.
- Forced `Revoke` or `Close` can meet `AdmissionFull` if a credit is still
  held by the engine thread just after a completion, ending `Retained` at
  `Revoke`. The same race exists on the normal path. If a receipt shows it,
  amend both paths together.
- H-A raises the per-lane admitted maximum from 18 to 34; `peak_queued` and
  `scheduler_bytes` receipts change value. The known host credit-race tests
  may fail again; report, do not rerun for a pass.
- FP-9 depends on the kernel sending the first READ as a background request;
  FP-8 and FP-29 depend on a request that needs no lease. Either can end
  `PARTIAL` or as a finding.
- The fusectl hazard of section 5.
- Rig construction per test costs seconds; six tests per binary is the cap.
- R5 is still open; its remaining fixes may move the lines cited here, as
  fix R1 already did once during planning.

## 9. Open points for the lead

1. **Forced teardown is unreachable in the product topology.** With D4 the
   Sandbox never has `abort_bound`, so every product Force is
   `force:capability`. R6 delivers it wired and proven in the test container
   only. This needs an owner decision.
2. **R3 named two recoveries as R6 work** (a mount fenced by a retained
   request; a Workspace `Retained` at unmount). Under D2 neither is built, and
   Force on a mount with a retained request ends aborted-but-mounted
   `Retained`. Confirm this is the intended closed state for R6.
3. FP-26's refusal half (D-13, `mount:debt`) is not implemented and cannot be
   staged hook-free. Confirm `NOT_RUN`.
4. The P-1 ruling's low-water early refill is not in source (R3 refills at
   exhaustion). Confirm `NOT_RUN` for that half, or schedule it.
5. Reply tag 15 and the two new enum values (D7, D8) go beyond the two tags
   named in the lead's D3. Confirm, or choose the text-only alternative.
6. A Workspace that is Unattached with unknown Commit custody has no terminal
   exit: Unmount is `Unknown` and Force is `Invalid` (D10).
7. With `relinquish_unknown` the closed namespace stays `Held` and its rows
   are never reclaimed in that daemon (D11). Confirm, or plan the local
   release.
8. Whether a later Force or Unmount may re-observe the drain predicate of an
   entry `Retained` at `Join` or `Requests` (an observation, no syscall). Not
   planned: the session is stored as opaque evidence today.
9. Architecture 76 and `core/AGENTS.md` call "daemon-wide graceful drain" R6
   work. The handoff scope and the proof plan define no such operation, and
   the application has no stop path. Not planned; say so in the completion
   record or assign it.
10. The whole-row FP-17 verdict is taken from the final suite; confirm that
    two existing binaries passing at one identity is accepted as the row.
11. `MountServices::request` changes signature (D16), touching two existing
    daemon test files in track U's write set.
12. Track T is a new support track between D and the proof tracks; the lead
    may prefer to write it.
13. Since fix R1 (`57244684b`) `LocalFailure` also covers a known Commit with
    an unreleased reader or owner, and `Uncertain` also covers a settled
    non-publication with one (fact 21). The plan forces `LocalFailure`
    without the flag and reports the second case as `Unknown`. Confirm, or
    require the flag whenever any custody is held.
14. Re-pin every line reference and the LOC listing at the commit where R6
    starts.

## 10. Lead rulings on the open points

Taken without the owner, each recorded for review in the R6 completion record.

1. **Forced teardown in the product topology (point 1, D4).** Kept as planned.
   Mounting fusectl in the Sandbox container would expose the abort file of
   every FUSE connection on the host kernel to that container, and changes a
   topology the Sandbox inspects exactly. That is the owner's decision. R6
   delivers forced teardown wired and proven where the abort control is bound,
   and a typed refusal where it is not.
2. **Recoveries R3 named (point 2, D2).** Not built. No contract says what
   may be done with a retained request's custody. The closed state is: a
   retained request leaves the mount fenced; Force ends aborted-but-mounted
   `Retained`; daemon stop is the exit. Recorded as an R6 limit.
3. **FP-26 refusal half and P-1 low-water refill (points 3, 4).** `NOT_RUN`,
   not implemented, not built here: neither has a value in source or in a
   contract, and inventing one is a contract change.
4. **Reply tag 15, `Stopping = 7`, `Abort = 9` (point 5, D7, D8).** Accepted.
   All are additive; tags 1 to 13 and every existing value keep their bytes,
   pinned by `control_golden.rs` before the first edit. SDK and daemon ship
   from one tree, so no older peer exists to refuse the new values.
5. **Unattached Workspace with unknown Commit custody (point 6, D10).** Left
   without a terminal exit; recorded as a limit.
6. **`relinquish_unknown` leaves the engine custody `Held` (point 7, D11).**
   Kept: it preserves the evidence and a later ruling can add the release.
7. **Re-observing a `Retained` entry's drain (point 8)** and **daemon-wide
   graceful drain (point 9)**: not planned. Architecture 76 and
   `core/AGENTS.md` are corrected in track D to say so.
8. **FP-17 (point 10).** The whole-row verdict is taken from both binaries
   passing at the R6 final identity, and the record says that this is its
   basis.
9. **Track T (point 12).** Written by the track D subagent after D is
   committed, as its own commit.
10. **Custody and the flag (point 13).** As planned: `LocalFailure` is a known
    publication and is forced without the flag; `Uncertain` needs it.
11. **Fallback for the terminal fence.** If track U cannot make the full
    fence correct, it stops and reports; the lead then amends this plan to
    the smaller gate-at-entry form before any corrected work.

Added from the R5 reviews, into existing tracks:

- **Track D:** a Commit failure that the registry records `Uncertain` while
  its error maps to another code (Busy at Begin, then a resolution that was
  not done) still answers that other code. The reply must say `Unknown`
  (`control/failure.rs`), with its proof at control scope.
- **Track P1 (R6-2):** admission waits are unordered and unbounded. R6-2
  records, by counts, how many filesystem jobs were admitted while the Commit
  waited at each stage. A Commit that does not finish under sustained writers
  is a finding for the lead, not a test to loosen.
- **Recorded limits, not work:** the two-slot guard of `commit_captured` does
  not cover a credited-byte budget too small for the thread's own held
  completions; an owner started with one Lifecycle slot serves a mount and
  refuses every Commit.

