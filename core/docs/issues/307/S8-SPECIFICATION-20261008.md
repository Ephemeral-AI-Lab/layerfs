# S8 specification: native filesystem, ordinary runtime access and cache lifecycle

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

Review correction after `77cf51686`, 2026-10-08: the
[six-finding correction ledger](checks/s8-spec-review-fixes-20261008/02-correction-ledger.md)
revises Exec completion, lookup custody, terminal drain, unmount probing and
receiver admission. Original review reports/dispositions remain historical;
these current rules supersede their conflicting decisions and count hypotheses.
The product source pin is unchanged and no runtime proof is added.

Owner supersession 2026-10-08, R0 at `1a6bb53ef`: SDK organization is
ProjectApi, WorkspaceApi and SandboxApi. Ordinary Sandbox/runtime or the external
executor owns commands, streams, exit status and explicit cancellation. The
filesystem daemon has no Exec supervisor, launcher mode, per-Exec cgroups,
command registration or custom Exec wire. Filesystem admission/capture and
complete drain require their own exact owners. The
[R0 reconciliation and withdrawal ledger](checks/r0-owner-reconciliation-20261008/03-owner-and-proof-ledger.md)
owns the prospective disposition; historical source pins, receipts and verdicts
stay unchanged. The [R0–R9 rollout](ROLLOUT-LEDGER-20261008.md) is the current
implementation assignment, superseding narrower old checkpoint dispatches.

Reviewed source-ownership update2026-10-08 at `5be93f6d7`: follow the
[ownership review](R2-R5-SOURCE-OWNERSHIP-REVIEW-20261008.md) and
[reviewed file layout](FINAL-CLUSTER-TWO-FILE-LAYOUT-20261008.md). FUSE owns the
connection and kernel request service; daemon assembles it with the existing
shared SQL/Store owners and composes overall Ready/unmount/Commit. Planned daemon
`native/` and kernel `request/steps/` homes are superseded. This is proposed source
organization, not implementation or relaxed proof requirements. R1 is complete;
only optional admin cancellation/client provenance is deferred. R2–R5 execution
awaits owner dispatch; forced teardown/concurrency/frozen acceptance remain later.

## 1. Scope, authority and status of claims

S8 delivers, on the existing direct-Store daemon: a native FUSE mount per
Workspace with exact readiness and detach, accessible to ordinary Bash and any
permitted process; Sandbox/runtime access setup protecting the databases and
credentials; an event-driven request service
that parks admitted work off the receive loops, with the explicit pre-admission
capacity exception of §6.2; the kernel cache
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
| Open and lookup references are independent indexed lease rows; native FORGET/RELEASE mapping "remains S8" | [S6 lifetime contract](S6-LIFETIME-CONTRACT.md) | S8 maps kernel lookup counts to indexed per-mount/per-inode custody, independent of OPEN/RELEASE and processing leases. The original D-6 counter-only proposal is withdrawn | S6 and review correction R2 |
| Mmap flush "emits a ctime-bearing SETATTR" | [fuse-investigation 01 §6, 03 §6](../303/fuse-investigation/01-kernel-and-request-path.md) | Contradicted for writeback off: inodes are `S_NOCMTIME`, the daemon owns times and never receives `FATTR_CTIME` | Kernel review finding 26 |
| The kernel sends DESTROY at unmount | [fuse-investigation 03](../303/fuse-investigation/03-per-call-lifecycle.md) | Contradicted for the `fuse` type: connection abort makes loops observe `ENODEV`. Loop exit/join establishes receiver disposal, not mount detach or the complete daemon-work drain; record all three separately | Kernel review finding 22 and corrections R3/R5 |
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
 mount / Commit / status /   ------->    |        |             |
 unmount over authenticated channels         Store (1 writer,     Overlay Owner       Fuse request service
                                             N readers, 1 cache)  (1 SQL thread)      (K workers, fair)
                                                 ^                    ^                   ^
                                                 +---------+----------+-------------------+
                                                           |
                                Workspace A (ns a)   Workspace B (ns b)   ...   independent mounts
                                FUSE session A       FUSE session B             fresh kernel caches each
                                ordinary processes   ordinary processes         runtime-owned identity
```

Daemon-lifetime owners, never recreated per mount [implemented and
source-verified]: the `Store` (writer, fixed readers, `CanonicalCache`), the
overlay `Owner` thread with its queue and connection, and the control `Service`
registry. [Proposed design] S8 adds one daemon-assembled `layerfs-fuse` request service with a
fixed K-worker pool shared across all mounts. It and the engine owners stay alive
across zero-mounted-Workspace intervals; only connection state is per mount.

The crate dependency is daemon -> fuse -> workspace, never Fuse -> daemon.
Fuse owns mount/profile/session/readiness/drain, callbacks, receive/handoff
admission, queues/workers, parked continuations, operation handlers, replies and
coherence. Its handlers reuse Workspace semantics and backed Overlay ownership.
Daemon application/filesystem.rs assembles services; service/filesystem_port.rs
implements only the narrow missing engine service interfaces. Existing SQL owner
fairness across filesystem, Commit and cleanup remains separate and unchanged.

Fuse connection-serving facts are necessary for overall Ready; daemon combines
them with registry/Workspace/service admission. Fuse connection-drained facts are
necessary for terminal unmount; daemon also proves all namespace-bound engine,
Store and control work disposed, then revocation/Close/routing removal. Neither
receipt substitutes for the other owner's predicate or creates a second registry.
The full review specifies these interfaces without claiming they exist today.

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
| I-7 | Every admitted reply-bearing kernel request receives exactly one explicit reply attempt in daemon-owned time. No-reply ownership work has an explicit completion/disposal path. No path relies on dropping a reply object or on FUSE_INTERRUPT |
| I-8 | An admitted request never waits on a prerequisite on a dispatch loop or service worker; it parks with owned inputs/reply/credits and no Workspace/cache/registry/SQL lock. The sole dispatch exception is the pre-admission handoff-capacity wait in §6.2, charged to one fixed receive slot per loop and woken on shutdown |
| I-9 | No park depends on a future kernel request on the same connection. Every prerequisite a request can park on is resolved by the daemon alone |
| I-10 | READ replies exactly `min(length, EOF − offset)` bytes; WRITE replies the full count or an error; the WRITE offset is always the kernel's |
| I-11 | Ordinary Bash/runtime execution belongs to Sandbox or an external executor. Optional WorkspaceApi.exec delegates with the mounted directory. No implicit Commit, reset, unmount, install, restore, command classifier or automatic timeout |
| I-12 | Runtime exit status, stream EOF/disposal, descendants, descriptors, mappings and FUSE work are independent observations. Runtime owns stream/result correctness; shell exit or zero registered commands never proves filesystem drain (§5.4) |
| I-13 | Bash cannot open the Store, the overlay database, a daemon credential, `/dev/fuse` or a control socket through any path alias or inherited descriptor |
| I-14 | Terminal success requires native detach, all loops joined, drained daemon request/owner/completion/Store/control work, no owner capable of later accessing the namespace, native ownership revoked, logical Close acknowledged and routing removed. Indexed physical deletion may finish later as reported debt |
| I-15 | No Workspace `fsync`/`fdatasync`/`sync_data`/`sync_all`; no automatic retry, busy handler, polling sleep or failed-operation replay anywhere in these paths |
| I-16 | Resident daemon state is bounded by admitted requests, fixed receive slots, open handles and configured caches. Kernel lookup custody is indexed in Overlay, with bounded processing windows; no resident map grows with the visited tree, a file or a Commit |

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
| `ForceUnmount { token, relinquish_unknown }` (new) | `ForceUnmounted { token, outcome }` or `Retained(TeardownCustody)` | Explicit forced policy. The additive result preserves original publication knowledge, filesystem work disposition, detach/drain receipts and cleanup debt (§11) |
| `Fork`, `History` (existing) | unchanged | Unchanged |

The SDK exposes a typed `mount` helper that performs `Mount` then `Attach` as
two separately acknowledged attempts, each retaining its own request, phase and
original failure. It is two control exchanges per mount; each is one bounded
record pair on an existing connection.

ProjectApi reuses Init/seal/install and fork/history. WorkspaceApi owns
mount/location/Commit/status/unmount over this channel. SandboxApi owns actual
sandbox lifecycle and ordinary runtime execution. Optional WorkspaceApi.exec
selects the Ready mount directory and delegates to Sandbox; it adds no filesystem
command registration. Standard runtime streams and status use the runtime's
existing route. No Exec records or process identifiers are added to Bridge.

The former dedicated Exec stream proposal and its 32768-byte custom records are
withdrawn before implementation. Runtime streams still require bounded buffering,
ordinary backpressure, complete per-stream delivery or explicit failure/disposal,
actual exit status and no automatic execution replay or total output/runtime cap.

### 4.2 Typed refusals

Refusals reuse `ControlRefusal { code, phase, moved, published, detail }` and the
existing codes. S8 adds phases, not message parsing.

| Situation | Code | Effects |
| --- | --- | --- |
| `Attach` on a token that is not `Bound`/`Unattached` | `Invalid` or `Busy` | None |
| `Attach` fails before `mount(2)` returns 0 | `Failed`, phase `attach:mount` | None. Entry stays `Unattached`; a new explicit `Attach` is a new operation |
| `mount(2)` returned 0 but handshake or loop start failed | `Failed`, phase `attach:session`, with exact teardown custody | One owned detach attempt and complete daemon-work drain are required before returning to Unattached; any unestablished detach, join or consumer disposal remains Retained |
| `Attach` outcome unknown to the daemon (owner thread lost, panic) | `Unknown` | Entry `Retained`; never a second attachment for that incarnation |
| Normal `Unmount` while opens, request/receive work or active control/Commit custody remain | `Busy`, phase `unmount:admission`, or `Unknown` for retained Commit custody | No terminal effect. Workspace stays usable |
| Normal `umount2` returned `EBUSY` during the reversible probe | `Busy`, phase `unmount:kernel` | Control probe withdrawn; kernel requests were serviced normally throughout, so no terminal filesystem error was injected |
| Detach began and detach/join/daemon-work drain is not established | `Retained(TeardownCustody)` | Original phase and remaining owners retained; usability is not promised |
| New `Mount` while maintenance is stopped or declared debt headroom is exhausted | `Capacity`, phase `mount:debt` | None (D-13) |

### 4.3 Status additions

`WorkspaceStatus` gains a bounded `native` block: native phase; mount and
connection identity; admitted/parked/running requests and fixed receive slots in
use; queued ownership decrements and completions; open file/directory handles;
control producers and Store demands/subscriptions retaining this namespace;
lookup-reference totals and indexed-retirement debt; engine `CleanupState`,
maintenance failure and reader/writer quarantine. Lists use fixed windows.
Every field is a maintained counter or one indexed point observation. The total
kernel lookup count does not block normal unmount: the connection can retain
lookups until detach. A zero live-work barrier, not a cache/count observation,
authorizes retirement. Status observes and never resolves an original unknown.

## 5. State machines and ownership

### 5.1 Workspace and mount

Extend the existing registry `Binding`; there is no second routing registry.
The registry arbitrates control operations. Each binding retains one native
admission/drain owner with an atomic phase and counted operation guards;
callbacks use that owner, not the global registry mutex per request. Crossing
a terminal phase and acquiring a guard must have one linearization order, so
no producer can appear behind a completed drain. No lock spans a syscall, job,
Store demand, stream send or join. This is new S8 wiring, not an existing proof.

```text
 Absent -> Binding -> Unattached -> Attaching -> Ready
                                   failure       |  \
                                   retained      |   Force (only after control-admission guard)
                                                 |                  v
                                     normal ProbeUnmount         Stopping
                                     kernel service continues      | abort + drain
                                      | EBUSY       | detach=0     | detach=0
                                      v             v              v
                                    Ready        Draining <--------+
                                                     |
                                    loops + daemon work drained; native ownership revoked
                                                     v
                                               logical Close -> Absent (physical debt remains)
 any unknown/unestablished terminal phase -> Retained { original phase and custody }
```

| From | Event | Guard | To | Transition owner |
| --- | --- | --- | --- | --- |
| Absent | `Mount` | capacity and debt headroom | Binding | control Service |
| Binding | bind success | known Open | Unattached | control Service |
| Binding | definite bind failure | original proves no local ownership | Absent | control Service |
| Binding | uncertain Open | original custody retained | Retained { bind } | control Service |
| Unattached | Attach | Idle; prior native ownership disposed | Attaching | control Service |
| Attaching | mount, handshake, loops ready | all native readiness evidence | Ready | mount owner |
| Attaching | definite pre-mount failure | no mount effect | Unattached | mount owner |
| Attaching | post-mount failure | detach and complete drain established | Unattached | mount/drain owner |
| Attaching | effect or drain not established | original custody retained | Retained { attach } | mount/drain owner |
| Ready | normal Unmount | no live control producer, open or received/admitted work | ProbeUnmount | control Service |
| ProbeUnmount | kernel EBUSY | kernel service remained normal | Ready | control Service |
| ProbeUnmount | plain umount returned 0 | mount identity unchanged | Draining | mount/drain owner |
| Ready | ForceUnmount | no active Commit/Attach/control producer; §11.1 guards | Stopping | control Service |
| Stopping | abort and local work disposition, then plain detach=0 | §11.2 | Draining | mount/drain owner |
| Draining | complete drain predicate | native ownership logically revoked | Closing | drain/Overlay owners |
| Closing | logical Close acknowledged | original outcomes retained in result | Absent | control Service |
| terminal phase | outcome/drain not established | no guessed cleanup | Retained { phase } | original owner |

No command/session gauge participates in filesystem admission or drain. `requests` includes admitted requests and no-reply ownership work until
completion disposal. Received-but-unadmitted callbacks have separate fixed-slot
gauges. Track queued/running completions, control producers and namespace-bound
Store consumers separately. Idle receive-buffer reservations and cached kernel
lookups are not active-request gauges. Shared Store/cache owners survive unmount.

### 5.2 Kernel request and reply

```text
 /dev/fuse -> fixed receive slot: decode, bind incarnation, check native phase
                 |            pre-admission wait for mount handoff capacity (§6.2)
                 |            take request credit + byte credit
                 |            copy bounded inputs (name <= 255, WRITE data <= 128 KiB, once)
                 |            move the fuser reply object into NativeRequest
                 |
                 +-- refused/inline opcode --> reply, release
                 +-- FORGET unit --> bounded owned decrement job; no reply
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
        ReplyAttempt (exactly one)  -->  release after original work/result disposal
                 |
        [mutation only] publication ticket released to the engine (ReplyAttempted)
```

| State | Owns | Leaves by |
| --- | --- | --- |
| Admitted | one request credit; owned input bytes charged to the mount's byte credit; the reply object | first step |
| Parked | the above plus one `Pending` or one flight subscription or one admission wait | the prerequisite's completion event |
| Runnable | the above | a service worker taking it in per-Workspace round-robin order |
| Published | the above plus an engine `Publication` ticket | reply attempt |
| ReplyAttempt | the reply object until `ok`/`error` returns | return of the pinned void-return reply method; one attempt counted, send result/delivery unavailable |
| Released | nothing | — |

This table describes reply-bearing requests. A FORGET unit has no reply object;
its completion is the known checked decrement or explicit retained teardown
disposition, with its credit and drain guard held until then.

Linearization: a mutation takes effect at the commit of its single owner job
(K4). Metadata/data reads observe one consistent owner turn. A positive LOOKUP
also atomically acquires lookup custody before the entry reply; it is an owning
write job, not a read-only exception (D-4/D-6). Reply order to the kernel is not
controlled by the daemon and fuser gives
no exact send-result or delivery receipt; correctness therefore rests on I-5, I-6 and the kernel's
own `attr_version` discard of attribute replies sampled before a newer inode
version (section 8.3). Capture includes exactly the published frontier, ordered
with earlier reply attempts through the engine's existing
`Publication`/`ReplyAttempted`/`PendingPublications` mechanism [implemented and
source-verified]; it never includes unflushed userspace or mapped stores.

A reversible normal-unmount probe changes no kernel request handling. Only
Stopping/known detach is a terminal fence. Then received waiters and unattempted
parked requests get one terminal reply attempt (no-reply FORGET work is retained
until disposal). For an already-attempted prerequisite, the drain owner retains
its original result, completion and credits even if a terminal reply is attempted
earlier to release a blocked task. The reply attempt does not dispose that job
or settle its outcome. A published mutation stays published. A caller thread blocked in FUSE must be released/aborted by its filesystem
disposition. Any separately requested process cancellation/wait belongs to its
runtime owner; the daemon-work barrier precedes lease retirement and Close.

### 5.3 Lookup references and open handles

D-6 preserves the S6 distinction between names, kernel lookup references, open
handles and processing/captured owners. Unique serials prevent aliasing; they
do not make an unlinked inode disposable while the kernel can still request it.

| Kernel object | Daemon owner | State kept | Released by |
| --- | --- | --- | --- |
| Node identity | canonical serial and native mount incarnation | scalar identity | never reused within the identity domain |
| Kernel lookup reference | indexed aggregate keyed by `(namespace, mount incarnation, serial)`, backed by the existing lookup-lease semantics | one backing row/count per live kernel inode; bounded command windows, no resident visited-tree map | checked FORGET decrement or logical revocation after complete detach/drain |
| Implicit root reference | mount-owned root custody | one indexed/scalar owner | complete detach/drain |
| Open regular file | existing `OpenFile` custody | indexed lease; encoded handle | RELEASE, or drain-qualified native-owner revocation |
| Open directory | directory processing/open custody and bounded cursor | last name, cookie and reply window per handle | RELEASEDIR, or drain-qualified revocation |
| In-flight operation | separate processing custody whenever it outlives its lookup/open protection | indexed lease plus bounded request plan | original result/reply/completion disposal |
| Removed inode | exact retained metadata and, where needed, orphan content domain | indexed/backed state | last independent lookup/open/processing/captured owner |

A positive LOOKUP acquires one reference in the same short owner transaction
that validates and composes its answer. CREATE/MKDIR/SYMLINK/LINK entry replies
include that acquisition in their existing mutation transaction. The increment
is known before any entry reply attempt. A send attempt is not delivery evidence;
never guess a compensating decrement after an unobservable send failure.
Future READDIRPLUS, if selected, must charge every returned reference too.

FORGET subtracts the kernel's exact `nlookup`, checks the mount/incarnation and
underflow, and releases the lookup lease only at zero. It has no reply. The
adapter hands each callback an owned bounded decrement record; the default
fuser BATCH_FORGET callback can hand off several such records, each charged.
The fair owner may combine already-queued records within a declared window,
without a batching timer or whole-table scan. Processing that outlives a lookup
or handle retains its independent lease before the protecting owner is released.
A decrement failure preserves the record and custody and stops that cleanup;
it is never converted to a successful counter update or automatically retried.

A removed cwd and an O_PATH reference retain exact metadata without requiring
an OpenFile handle. Reclamation timing must not turn their valid GETATTR/fstat
into ESTALE. Ordinary unlink, replacement rename and FORGET affect different
owners; physical deletion is eligible only after the last independent owner.
At native teardown, missing FORGET/RELEASE records are handled by the bounded
logical-revocation/physical-cleanup sequence in §11, not by forgetting live work.

This changes the original zero-SQL FORGET and read-only positive-LOOKUP targets.
Backing grows with live kernel ownership; resident windows stay bounded. Count
indexed writes and total retirement work honestly in H-4/H-8 and prove removal,
O_PATH, partial/batched FORGET and detach in FP-29/FP-31.

<a id="54-exec-process-group-and-streams"></a>
### 5.4 Runtime process and stream ownership

Sandbox/runtime or the external executor launches ordinary commands and owns
standard I/O, actual exit/signal status and explicit caller cancellation. The
old daemon ResourceTerminal/cgroup/stream-record state machine is withdrawn.
No process or stream owner is created in the filesystem daemon. Runtime EOF,
output delivery/disposal and exit status remain independently correct: buffered
bytes can remain after process exit, and descendants can retain a pipe or file.
A lost result never authorizes automatic command re-execution.

Ordinary filesystem admission has no command identity or parent requirement.
The daemon cannot infer released cwd, O_PATH, open descriptors, mappings or
kernel lookup ownership from shell exit or a runtime observation. Runtime setup
establishes identity and mount visibility/protection; the native mount owner
qualifies actual kernel busy/detach behavior at that topology. The filesystem
barrier is §11.2, and forced teardown never implicitly signals caller processes.

FP-6-Runtime, FP-7-Runtime and FP-30-Runtime replace the old prospective daemon
stream/session selections. FP-20/21/22-FS/28/29/31–34 retain filesystem-lifetime
proofs independent of how commands were launched.

### 5.5 Terminal drain

The terminal owner first freezes control admission and establishes the correct
probe/stopping phase, then owns kernel detach and all remaining daemon work.
FUSE receiver-loop exit is one barrier, not the whole barrier. Requests, pending
owner completions, namespace-bound Store consumers, control producers retain their original ownership until result disposal. Only the complete
§11.2 predicate permits revocation of native leases and logical Close. Physical
retirement/deletion is then paged through the existing fair owner, with debt
reported until Gone. No receiver shutdown silently drops another owner's job.

## 6. Native request service and scheduling

### 6.1 Threads and what each may do

| Thread | Count | May do | Never does |
| --- | --- | --- | --- |
| Dispatch loop (fuser `run`) | 2 per mount initially | Decode into its fixed receive slot; wait only for that mount's native handoff credit before any copy/job; own bounded inputs and hand off request/FORGET work | Wait for an owner result/credit, Store demand or stream; retain Workspace/cache/registry/SQL locks during the admission wait |
| Fuse request-service worker | `K` per daemon-assembled service, fixed at readiness; initial `K = read_handles + 2` | Run one step of one request; perform a cold Store demand when it holds a reader; compose and send replies | Wait on a `Pending`, a flight or an admission credit |
| Overlay Owner | 1 per daemon [implemented] | Short typed SQL jobs and maintenance turns | Content decode, Store I/O, reply sends |
| Mount session owner | 1 per mount, around `fuser::Session::run` | Retain session/result custody; distinguish joined successful completion from errors that can leave unjoined loops | Infer complete drain from run() returning an error |

The existing daemon engine remains the only SQL scheduler. The Fuse request service
schedules requests, not SQL: it decides which parked-then-runnable request a
worker advances next. It adds no whole-Exec or whole-Commit gate, no polling,
no batching sleep and no second queue in front of SQL beyond the Owner's own
lanes.

The current OwnerClient read/job adapters synchronously wait. They cannot simply
be hidden behind `ports.rs` and called by all K workers. The missing service port
must hand out an owned original pending operation with race-safe completion/loss/
credit notifications. Fuse parks the continuation and later consumes that exact
outcome once. Engine completion/credit owners stay in daemon; interfaces must not
expose daemon types to Fuse or implement polling/thread-per-waiter fallbacks.
Workspace semantic fact steps remain reusable, rather than duplicated in handlers.

### 6.2 Admission and backpressure

The pinned fuser loop reads before calling Filesystem methods; it exposes no
pre-read credit hook. S8 therefore gates at callback entry, not by claiming to
control the library's next read. This is the narrow admission-only exception to
I-8. While waiting, the callback holds only its borrowed native buffer/reply and
one fixed receive-slot reservation. It has submitted no owner job, acquired no
mutable lease and copied no WRITE payload. It holds no shared product lock.

There are `N = 2` fixed receive slots per mount, one per loop, allocated/charged
before the loops start. Each may hold one received-but-unadmitted callback.
They are separate from `R = 16` admitted native handoff credits. Thus at most
`R + N` native units are held in userspace, including the two callbacks waiting
at admission. Count active receive slots separately; an empty reserved slot is
not an active request and does not make an idle mount Busy. fuser's own buffer
allocation and transient borrowed WRITE bytes remain a separate resource domain.

On admission the callback moves its reply and one bounded input copy into an
owned request and returns immediately. An admitted unit keeps its credit through
original result/reply/completion disposal. A no-reply FORGET unit releases its
credit after its checked owner decrement/disposition. The default BATCH_FORGET
may deliver several units from one frame; each uses this same bounded handoff,
without copying the entire batch or a timer. Opcode and handoff-unit counts stay
distinct. Valid inline/no-reply operations have an explicit counted disposal path.

A condition/event wakes admission waiters when handoff credit is released or the
mount enters a terminal phase. The wait loop only rechecks admission/phase; it
never repeats an attempted filesystem or Store operation. A normal ProbeUnmount
continues kernel admission/service. Stopping or known detach wakes every waiter:
a borrowed reply gets one terminal attempt, no-reply ownership work is retained
for teardown disposal, and no waiter remains blocked on an empty credit pool.
No callback owns a request needed to release the capacity it is awaiting. By I-9,
progress of admitted units cannot require reading a later kernel request.

Owner admission is a different wait. AdmissionFull returns the original command
before any attempt; the admitted native request parks on OwnerAdmission and
relinquishes its receiver/worker. The existing notifier makes it runnable on
credit release. No dispatch callback waits for an Owner credit or completion.
Daemon-wide byte credits and ordinary/lifecycle lane settings stay explicit;
count additional lookup ownership jobs instead of assuming every request uses
only a read lane. A control Mount with a full lane still receives a before-effect
Capacity refusal. FP-34/H-10 cover full handoff capacity and shutdown wakeup.

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
| LOOKUP | Base child/inode facts under the bound root, from cache or a cold demand | One consistent terminal job: positive answer plus indexed lookup acquisition is a short write transaction; negative answer is read-only. Any zero-effect Need/root-fact rounds are counted separately | Entry reply only after known acquisition; no implicit lease rollback after send |
| GETATTR | Base inode facts for the serial | One read-only compound job | Reply |
| READLINK | Base target | One read-only job for the local layer | Reply |
| READ | — | One `SourceRead`/`FileRead` job returning the local window, mask and base root [implemented] | One base range demand for inherited bytes, then reply (I-10) |
| READDIR | Base page for the cursor | One read-only job for the local page | Merge in name order, reply at most one window |
| OPEN / RELEASE | — | One lifecycle job each [implemented] | Reply |
| WRITE, SETATTR, CREATE, MKDIR, SYMLINK, LINK, UNLINK, RMDIR, RENAME | Required immutable facts | One mutation transaction; entry-bearing successes also acquire lookup custody in it. Need rounds precede the effect | Reply, then release publication ticket; all ownership effects counted |
| FSYNC, FSYNCDIR | — | none | Success; no work and no durability claim |
| FLUSH | — | none | Success |
| xattr family | — | none | `ENOSYS` (sticky), section 8.4 |
| FORGET / BATCH_FORGET callbacks | Own bounded `(mount, serial, nlookup)` decrement units | Indexed checked ownership decrements on the existing fair owner; bounded already-queued batching is permitted | No reply; retain failed decrement custody, otherwise release handoff credit |
| STATFS | — | none | Inline reply from fixed declared values: block size 4096, name length 255 and a constant nonzero free-space figure. It reports no physical capacity and is not an admission signal; the library default of all zeros is not used |

Pure metadata/data observations use unframed reads in one owner turn when all
facts are available, retaining a complete bounded answer/plan and immutable root
for later base reads. Positive entry replies additionally acquire lookup custody;
they are owning writes. A parked/multi-job plan that needs mutable state after
its protecting reference can be released must retain separate processing/source
custody. Record these lease jobs and zero-effect fact rounds explicitly; the
former blanket zero-write/zero-lease read-class hypothesis is withdrawn.

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
| Per-mount native | R admitted units plus N fixed receive slots (initially 16 + 2); two 16 MiB + 4 KiB library buffers and transient handshake buffer | active/reserved receive slots, handoff credits, owned-copy bytes and native buffer observations separately |
| Indexed lookup ownership | backing rows/counts for live kernel references; bounded resident command windows | rows, counts, physical bytes, decrement jobs and revocation debt |
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
profile candidate in the mechanism ledger. The owner has agreed to one
prospectively registered single-mechanism arm that varies it (P-6 ruling).

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
| UNLINK, RMDIR | Kernel drops link count locally and invalidates ctime and parent attributes | Publish and reply; preserve independent lookup/open/processing custody | Exact removed-inode metadata until last owner, independent of reclamation timing |
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
| `st_uid`, `st_gid` | The runtime's configured command identity for every inode; not stored | proposed design |
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

## 10. Ordinary runtime execution, access and confinement

<a id="101-launch"></a>
### 10.1 Runtime execution

SandboxApi uses the actual ordinary Sandbox runtime backend for lifecycle,
standard streams, exit status and explicit caller-requested cancellation.
WorkspaceApi.exec, when exposed, only chooses the mounted directory and delegates.
External executors can launch Bash directly. There is no filesystem daemon
launcher mode, process supervisor, per-Exec cgroup, Exec registration, custom
stream/status/cancellation protocol or change-at-exit hook. Resource limits
selected by the runtime remain explicit; LayerFS adds no runtime/output cap.

### 10.2 What confinement is and is not

| Property | Owning requirement | Proof |
| --- | --- | --- |
| Commands cannot open Store, Overlay or daemon credentials | Sandbox setup/external executor establishes unprivileged identity, protected path visibility, /proc protections and no inherited protected descriptors | FP-5-Runtime at actual topology; a directory layout is not isolation evidence |
| Commands cannot abort/unmount the connection | Fuse owns native mount/fusectl controls inside the privileged daemon process; daemon control authorizes transitions and runtime withholds relevant capabilities and helper escalation. P-2's explicit no-new-privileges setting belongs to runtime setup | FP-5-Runtime |
| Permitted filesystem access | Kernel default_permissions and reported configured uid/gid/mode; same semantics for all processes seeing the mount | FP-17; no Exec admission |
| Sibling mount visibility | Runtime access setup declares and proves its actual namespace topology; shared uid alone is no adversarial isolation boundary | FP-5-Runtime and FP-22-FS |
| Working directory | Caller/runtime chooses it; optional SDK convenience chooses a mounted directory | It is not confinement |

Protected descriptor custody is tested for the actual runtime. Requiring exactly
three descriptors from a daemon launcher is withdrawn. The setup must prevent
Store/Overlay/credential access through path, /proc and descriptor aliases, with
an exact source/environment proof. P-2's original launcher wording remains a
historical owner ruling in §15.3; its helper barrier is now enforced by runtime.

### 10.3 Mount propagation contract

Sandbox setup/external executor owns mount visibility. The daemon must qualify
normal busy/detach and connection/drain behavior for every supported topology.
If namespaces copy a mount, prove that a retained cwd/descriptor/mapping keeps
normal unmount truthful, successful detach disposes all required copies, and a
process using Workspace A does not keep B's connection alive. Do not choose
private or slave propagation by assumption; FP-22-FS proves the actual runtime
setup. A caller-owned process surviving forced connection teardown is permitted;
the daemon does not own its later exit, output or cancellation.

<a id="104-process-custody-and-cancellation"></a>
### 10.4 Explicit caller cancellation and forced filesystem teardown

Runtime owns command identity and requested cancellation. Daemon ForceUnmount
owns only the exact connection-specific abort, native request disposition,
attempted daemon-work drain, plain detach and namespace retirement of §11.
It never implicitly kills commands, waits for command output EOF or treats
process exit as a filesystem fence. Accepted filesystem effects stay published.
If a caller needs both process cancellation and unmount, it explicitly requests
both from their owners; neither establishes the other's completion.

## 11. Terminal unmount, drain and reclamation

### 11.1 Admission

Control admission and the native drain owner establish one ordering with request and
control-operation guards. No guard may be minted after the terminal
barrier closes; an already-owned guard keeps its original result/lease alive.

| Policy | Before any terminal effect | Admitted phase |
| --- | --- | --- |
| Normal | Refuse Busy for an active control producer, open, received/admitted request, ownership decrement or pending completion. Retained Commit uncertainty returns Unknown. Cached nlookup references and empty receive reservations are not activity | ProbeUnmount |
| Forced | Refuse Busy while Commit, Attach or another namespace control producer is still running. No abort/unmount occurs on that refusal. A stopped known-publication failure keeps its original outcome; stopped unknown custody requires explicit relinquish_unknown and stays Unknown in the terminal receipt | Stopping, even if kernel request owners remain |

Relinquishing stopped unknown custody is not cancellation or resolution of an
in-flight operation. It permits explicit local disposal only after all consumers
have stopped and their original outcomes are retained. S8 does not add an
interruptible Commit or let a terminal action race its Store publication.

### 11.2 Probe, forced stop and complete drain

**Normal path.** ProbeUnmount temporarily refuses new Attach/Commit control
admission but continues ordinary kernel request admission and service. It neither
returns terminal errors nor parks filesystem requests waiting for the unmount
syscall; that syscall can itself need filesystem service. Make one plain
`umount2(path, 0)` attempt on the owned mount. EBUSY withdraws the control probe
and returns Busy with no terminal filesystem effect. Mutations legitimately
completed during the probe remain visible. A known detach transitions to
Draining and only then closes native admission. Any unestablished outcome retains
the exact mount/phase; it does not guess Ready or Detached.

**Forced path.** Use the connection-specific fusectl `abort` control already
owned/validated for that mount; do not use `umount2(MNT_FORCE)` as an abort-only
primitive. First establish Stopping, then make one one-byte abort write on the
retained control descriptor, wake receive-capacity waiters, complete unattempted
parked replies. Preserve the results of jobs already attempted even if their
terminal reply was sent early. The daemon neither signals caller processes nor
waits for their exit or stream EOF; it drains exact filesystem/daemon consumers.

Abort and detach are independent effects. Linux 6.12's
[fusectl abort handler](https://github.com/torvalds/linux/blob/v6.12/fs/fuse/control.c#L31-L42)
invokes connection abort without unmounting. In contrast,
[MNT_FORCE continues through unmount](https://github.com/torvalds/linux/blob/v6.12/fs/namespace.c#L1773-L1814).
The selected path therefore has one abort write and, after local consumers drain,
exactly one plain detach attempt, with no second unmount after success and no
retry after EBUSY/error. Bind the abort descriptor to the connection/mount
incarnation at Attach; do not reopen a guessed connection number at teardown.
Its write return alone is not loop-exit or detach evidence. Missing capability
refuses forced admission before effects; short/failed/unknown abort or
detach retains the original phase and custody. A failed detach after abort is
Retained/aborted-but-still-mounted, never the reversible normal Busy result.
A caller-held cwd/descriptor/mapping can cause that post-abort EBUSY while its
process survives; caller cleanup remains external and no second detach is attempted.

**Complete drain predicate, required by both paths before revocation/Close:**

1. Native admission is terminal; every received callback has returned/disposed
   its reservation, and every FUSE loop is known exited and joined. A fuser
   `run()` error is not sufficient evidence: partial thread creation or a panic
   can return without joining remaining loops. Keep Retained when join cannot
   be established; never infer success from the outer thread or mountinfo alone.
2. Each admitted request, queued/running service step, ownership decrement,
   owner Pending/completion/publication ticket and already-attempted operation
   has its original outcome and disposal acknowledged. Sending ENOTCONN is not
   completion of the job that preceded it. No queued continuation can later
   acquire or use a local namespace owner.
3. Every namespace-bound Store read/decode consumer or flight subscription has
   completed or been explicitly detached with no access to the namespace.
   A shared immutable acquisition may continue for other subscribers under its
   Store owner; it cannot retain a departing Workspace continuation/custody.
4. No control constructor/Commit/capture consumer remains active. Normal and
   forced admission refuse a running Commit; any other unestablished disposition
   keeps the terminal operation Retained. Original known/unknown publication
   records are transferred into the terminal receipt, never settled by a read.
5. No native/control/request continuation can acquire or use this namespace
   after the barrier. Caller processes and runtime streams are independent;
   their survival after forced filesystem abort does not reauthorize filesystem
   access or permit retirement before the daemon-work barrier.

A service/cold job may outlive FUSE loop exit. Its drain guard, original inputs,
result and credits stay alive until the above predicate holds; no timeout drops
it to fabricate completion. Any unestablished item returns Retained with the
remaining owner/phase and leaves leases and routing held.

```text
 normal: ProbeUnmount (kernel service continues) -> plain detach once
          EBUSY -> Ready; no filesystem request failed because of the probe
 forced: Stopping -> abort once -> replies/dispositions -> daemon drain
          -> plain detach once; failure remains Retained, not Ready
 both: detach + all loop joins + complete daemon-work drain
          -> fixed logical native-owner revocation -> logical Close -> terminal reply
          -> fair indexed retirement/deletion until Gone
```

After this barrier, one short owner job marks the exact native mount group
revoked; its lookup/open/processing records cease to represent live consumers.
This is logical revocation, not bulk SQL deletion. Indexed maintenance releases
native ownership rows and dependent references in bounded windows, with total
work proportional to affected records. Unrelated/captured/control owners are
not swept into that group; any undisposed independent owner prevents the barrier.
Logical Close may then acknowledge with physical debt still Queued. Revocation,
Close and physical retirement are distinct counted phases; failures retain exact
custody and stop automatic replay. No missing FORGET/RELEASE at detach causes
an unbounded foreground walk or deletion of an owner that is still executing.

### 11.3 Outcomes

| Outcome | Meaning | Workspace afterwards |
| --- | --- | --- |
| Busy | Normal pre-effect refusal/kernel EBUSY, or forced refusal of a running control producer before effects | Fully usable; no terminal request error injected |
| Unknown | Retained Commit/lifecycle knowledge prevents normal unmount | Original custody retained |
| Stopping | Forced cancellation/abort/disposal has effects but detach/drain is incomplete | No Ready claim |
| Detached | Exact detach effect known | May still have daemon work; this alone permits no revocation or Close |
| Drained | All five drain conditions hold | Native-owner revocation and Close may run |
| Cleanup-pending | Terminal reply after logical Close; physical retirement/deletion is Queued | Gone from routing; debt visible until Gone |
| Complete | Engine reports Gone after terminal completion | No remaining local debt |
| Retained { phase } | Abort, detach, join, consumer disposition, revocation or Close was not established | Exact stopping owner/work and original publication knowledge remain; no success or guessed cleanup |

Normal success keeps its existing Unmounted record. ForceUnmounted is a new tag
whose bounded outcome carries original known/unknown Commit knowledge,
filesystem-work disposal, abort/detach/drain dispositions and cleanup state.
Unknown stays Unknown after explicit local relinquishment; an absent Commit
is never invented as a NotPublished verdict. Runtime execution/results remain
under their separate caller/runtime owner and do not enter this terminal record.

### 11.4 Reclamation and debt

Reuse the fair Owner's maintenance windows (currently at most14 payload cells or
64 small metadata rows per step), service while active/idle, page reuse and
first-failure stopping. Add indexed native-owner revocation/retirement phases;
prove their bounds separately rather than crediting old S6 receipts with new
behaviour. If there are K native ownership rows, the mark is fixed foreground
work and physical retirement is O(K) indexed work in bounded turns. No base scan,
per-FORGET global collection, shared-cache flush, database rebuild, history
deletion or Workspace sync is introduced. Surface retired-ownership and payload
debt separately. Mount admission still refuses on stopped maintenance or exhausted
declared headroom; actual growth/progress is proved, not assumed from a counter.

## 12. Failure, unknown and custody rules

| Situation | Reply to the kernel or caller | Retained | Never done |
| --- | --- | --- | --- |
| Definite pre-effect refusal of an operation (`Refusal`) | The mapped errno (`EEXIST`, `ENOENT`, `ENOTDIR`, `EISDIR`, `ENOTEMPTY`, `EPERM`, `EOPNOTSUPP`, `EINVAL`, `EMLINK`, `EFBIG`) | nothing | — |
| Owner admission full | none yet: the request parks | request and credits | `EBUSY` |
| Cold demand fails | `EIO` for that request | the original `PortError` in that request's failure scope and in a bounded daemon diagnostic | a second demand on the request's behalf |
| A reader session quarantined | `EIO` for the demand that observed it | reader excluded and reported | reuse of that reader |
| Mutation job outcome uncertain (owner lost mid-attempt) | `EIO` | the engine's original unknown; the Workspace's activity becomes `Uncertain` | a resend, a rollback, a guessed success |
| Serial range exhausted because the Store write was `Busy` | `EAGAIN` (P-1 ruling, section 15.3) | the create has no effect | a wait, a gate or a replay of the failed reservation |
| Reply method returns () | One explicit attempt; exact send result/delivery is unavailable through pinned fuser | Attempt count and publication, if any; internal library error logs are uncorrelated diagnostics | Retrying or claiming send/delivery success/error from method return |
| Request during ProbeUnmount / after terminal stop | ordinary service during probe; ENOTCONN only in Stopping/known detach | receive-slot/request disposal and original attempted work | terminal errors from a reversible probe |
| Parked request when the connection is aborted | one terminal reply attempt; no-reply work retains its explicit disposal path | every attempted job, original result/completion, credit and drain guard until disposal | treating a sent error or loop exit as daemon-work completion |
| Lost `Mount` reply | — | the entry, observable through `Locate` | a second binding for that incarnation |
| Lost `Attach` reply | — | the `Ready` or `Retained` entry | a second attachment |
| Known publication, failed local install | existing `LocalFailure` custody | the original capture and known publication | resolution by a later read |
| Later explicit observation (`Status`, `Locate`) | the current observation | — | settling an earlier unknown |

Uncertain outcomes remain unknown; no observer resolves one. Explicit forced
local relinquishment follows §11.1 only after consumers stop and preserves that
unknown in ForceUnmounted or Retained. It is not a publication-outcome resolver.

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
| Exact attributes after the last name is removed while a kernel lookup/cwd/O_PATH or processing owner remains | Supported through indexed D-6 custody; no cleanup-timing-dependent ESTALE |
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
| D-1 | Extend the existing registry `Binding` with a native state and three gauges; add `Attach`, `Locate`, `ForceUnmount` as additive tags | A second routing registry; a single combined mount record that would blur the two acknowledgement points |
| D-2 | First-party `mount(2)`/`umount2(2)` and `Session::from_fd`; two loops through `Config.n_threads` | fuser's mount and unmount ownership: silent helper fallback, lazy detach, a handle that reports success after `EBUSY` |
| D-3 | One Fuse-owned request service shared across daemon mounts, with per-Workspace runnable queues, fixed workers and engine completion notifications; admitted work never waits on receivers/workers | Blocking the loop in `Pending::wait`; more loops per mount as a substitute for parking |
| D-4 | One consistent answer job with required owning effects: positive LOOKUP acquires indexed custody, pure observations remain read-only; explicit processing leases/fact rounds are counted | A torn multi-job answer or a zero-SQL claim that drops necessary ownership |
| D-5 | R admitted handoffs plus N fixed receive slots; only callback-entry native-capacity admission may wait on a receiver. Owner/Store prerequisites park off-loop; terminal wakeup disposes all waiters | Pretending fuser exposes a pre-read hook, unbounded pending input, or waiting on SQL/Store while holding a receiver |
| D-6 | Indexed per-mount/per-inode lookup counts and leases; bounded checked FORGET jobs; independent open/processing custody | Counter-only FORGET and best-effort removed-inode attributes; a resident visited-tree map |
| D-7 | Fair bounded cold-demand admission with idle, healthy reader selection and per-request failure scopes | The blind rotating counter and blocking reader mutex |
| D-8 | Complete native/daemon-work drain, then fixed logical native-owner revocation and indexed bounded physical retirement | Treating receiver-loop exit as full drain, retiring active producers, or assuming all FORGET/RELEASE records arrive |
| D-9 | Mount reads its Branch snapshot through a read-only session; no daemon-local gate on the writer session | A writer mutex or wait, which K30 forbids |
| D-10 | Withdrawn 2026-10-08: no daemon command/session owner. Sandbox/runtime owns ordinary standard streams/status and explicit cancellation; optional WorkspaceApi.exec delegates | Historical launcher/cgroup/ResourceTerminal/custom-wire proposal is retained at its source pin only |
| D-11 | The command identity is an explicit runtime/access configuration value, required equal in every daemon sharing a Store; mount access is `allow_other` plus `default_permissions` with the daemon as mount owner | Making the Bash user the mount owner, which would let it abort the connection |
| D-12 | Runtime setup owns mount visibility/propagation; qualify exact busy/detach/drain at actual supported topology, including external processes and separate Workspaces | Unproved copied mounts, shell-exit fences or daemon launcher namespaces |
| D-13 | Mount admission refuses on stopped maintenance or exhausted declared debt headroom | Nominal success with unbounded overlay growth |
| D-14 | No kernel notifications in S8; xattr family answered `ENOSYS` | Per-WRITE invalidation, which deadlocks under cached I/O |
| D-15 | Subscribers of a shared failed acquisition receive the same original failure and are never re-attempted; lent bytes stay charged after eviction | Hidden re-acquisition; reporting eviction as freed memory |

### 15.2 Choices that are genuinely the owner's

Planning and the first implementation checkpoints do not wait on these; each
row says where it starts to matter.

All seven were ruled on 2026-10-08. The rulings are in section 15.3 and govern
over the recommendation column below.

| ID | Question | Recommendation | First needed |
| --- | --- | --- | --- |
| P-1 | A `create` needs an inode serial; the local range is exhausted; the one reservation attempt meets a Store `Busy` (K30). K13 says nothing returns `EBUSY` for contention. Which errno does Bash see, and may the daemon attempt a refill early, at a low-water mark, so that exhaustion needs several consecutive `Busy` results? | Early single attempts at a low-water mark (each still one attempt, none replayed) and `EAGAIN` on exhaustion; no gate | Mutation checkpoint |
| P-2 | Do Exec children run with no-new-privileges? It prevents setuid helpers outside the mount from crossing the user boundary that hides the Store, and it changes what "ordinary Bash" can do (`sudo`, `su`) | Yes | The first checkpoint builds it as an explicit configuration value with no hidden default; the ruling is needed before Exec is qualified |
| P-3 | Proof budgets for full-fixture cases: the historical byte copy of the full tree alone took 10.36–12.65 s, and scoped oracles took 7.3–9.6 s natively, against the 15 s and 10 s defaults. For each of setup scope, long workloads and full-byte oracle: retain `NOT_RUN` with the conflict stated, or grant a prospective scoped exception. May a read-only sample bind one closed sealed Store without a per-sample copy, under a before/after identity proof? | Stated per case in the [proof plan](S8-PROOF-PLAN-20261008.md#8-budgets-and-pending-owner-decisions); no threshold proposed | Performance checkpoint |
| P-4 | Are new matched controls authorized at prospective identities: native ext4, and a passthrough adapter under the promoted profile? What does a "materially better" claim compare against, given that no completed LayerFS-route control exists? | Authorize native ext4 as context; decide the passthrough arm and the comparison target before registration | Performance checkpoint |
| P-5 | Which residency-proof mechanism is accepted inside the shared VM, given that a VM-wide cache drop touches the protected containers and is not itself residency proof? | A per-file residency measurement before the attempt; ineligible with zero attempts when nonzero | Performance checkpoint |
| P-6 | May `max_background`/congestion be varied from the promoted 1/1 in a prospectively registered single-mechanism arm if the head-of-line oracle confirms per-mount read serialization? | Yes, as a candidate arm; the promoted value stays the default until its proof | Concurrency checkpoint |
| P-7 | Historical cases with no admissible oracle or with an S10 dependency: E09 (nondeterministic output) stays `NOT_RUN` or gets a normalized oracle; E18/E19 run before S10 as explicitly "unrefreshed index" cases or wait | Keep E09 `NOT_RUN`; run E18 only as a labelled unrefreshed case; defer E19 to S10 | Performance checkpoint |

### 15.3 Owner rulings, 2026-10-08

The recommendations of section 15.2 were presented to the owner in the main
chat on 2026-10-08 in a refined form, and the owner replied "proceed". The rows
below transcribe what was presented. They govern over the recommendation column
of 15.2, and none sets a number left for later. P-2's original launcher mechanism
wording is historical and superseded by R0: actual Sandbox/executor setup applies
its explicit no-new-privileges/helper barrier and proof. It authorizes no daemon
launcher, per-Exec cgroup or command supervision.

| ID | Ruling |
| --- | --- |
| P-1 | Early refill, then `EAGAIN`. Once the local unconsumed serial range is below an explicit low-water configuration value, a create makes at most one reservation attempt, outside every Workspace lock as today. A `Busy` result is that attempt's exact before-effect refusal and is never replayed. A later create is a new operation and may make its own single attempt; that is a new attempt, not a retry of the failed one. A create that finds the range exhausted and whose one attempt is `Busy` returns `EAGAIN` with no effect. No gate, wait, timer or busy handler |
| P-2 | Yes. The launcher sets no-new-privileges. It stays an explicit configuration value with no hidden default, and qualification runs with it set. A deployment that clears it gives up the helper barrier knowingly and is outside the confinement row that depends on it |
| P-3, shared Store | A sample may bind one closed sealed Store without a per-sample copy only when its own receipt shows zero serial reservations and zero Store write transactions, under a before/after identity proof of the Store file with a declared account of its sidecars, and the residency proof of its cache class. Every other case uses a declared byte-copy clone. The clone budget is decided after the fixture Store's size and copy time are recorded; none is set here |
| P-3, long workloads | E08 stays `NOT_RUN`. E03, E12, E13 and E14 are reported as priced comparisons against native at the same identity and cache class, labelled diagnostic, with no pass/fail budget and no admission verdict. Each still runs under an explicit wall stop declared at prospective registration; no bound is set here |
| P-3, verifier | Timing rows use a scoped oracle, labelled scoped. The full-byte oracle runs once at final identity as a functional proof under its own exception, declared at registration |
| P-4 | Native ext4 and a passthrough adapter under the promoted profile are both authorized at new prospective identities. The passthrough arm is harness code: never product source and never taken from the experiment tree. A "materially better" claim compares the product against that passthrough in the same cache class over mount + Exec + unmount, with the storage delta and the gap to native reported. The numerical threshold is set by the owner after the first diagnostic data; none is set here |
| P-5 | A per-file eviction hint followed by a per-file residency measurement of the Store file and its sidecars before the attempt. Nonzero residency is `INELIGIBLE` with zero attempts. The hint is never the proof. No VM-wide cache drop |
| P-6 | Yes. One prospectively registered single-mechanism arm may vary `max_background` and its congestion threshold after FP-9 confirms per-mount serialization. The promoted 1/1 stays the default until that arm has its own proof |
| P-7 | E09 stays `NOT_RUN`. E18 runs only as a labelled "unrefreshed index" case. E19 is deferred to S10 |

The P-1 ruling adds one oracle to the mutation checkpoint, recorded in section
8.1 of the [proof plan](S8-PROOF-PLAN-20261008.md#81-owner-rulings-2026-10-08).

No S8 product implementation and no measurement was performed in producing this
specification.
