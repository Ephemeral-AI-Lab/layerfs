# workspace_api.unmount — terminal detach, ownership close and cleanup

> **Status:** Proposal; target LayerFS 0.1.7; not a released contract.
> Owner revisions: 2026-10-05. Product baseline:
> `f96d97651be5299f153ccde2bc8d921dd58807ad`; supersedes the detach/close
> direction of design `334fc743751b9a181e670d0601a24fb3169208f9`.
> No implementation, teardown measurement or cleanup qualification is claimed.

This document owns the terminal Workspace operation. Read [mount](mount.md),
[Exec](exec.md), [Commit](commit.md), [status](status.md),
[daemon/SQLite](../daemon-sqlite.md) and [FUSE](../fuse.md) for their respective
contracts. There is no required separate `workspace_api.close`.

Owner supersession 2026-10-08 at R0 input `1a6bb53ef`: the SDK exposes
ProjectApi, WorkspaceApi and SandboxApi. Ordinary Sandbox/runtime or an external
executor owns command launch, standard streams, exit status and explicit
cancellation; the filesystem daemon owns no command supervisor, launcher,
per-Exec cgroup, command registration or custom Exec wire. FUSE serves every
permitted visible process. Shell exit/zero registered commands proves no
filesystem drain, and forced filesystem teardown never implicitly kills caller
processes. Current [S8 specification](../../307/S8-SPECIFICATION-20261008.md) and
[R0–R9 rollout](../../307/ROLLOUT-LEDGER-20261008.md) govern prospective work;
historical baseline pins, receipts and verdicts retain their original scope.

Reviewed R2 ownership [proposed design]: Fuse owns normal native detach and
connection/request drain; daemon combines those exact receipts with all namespace-
bound SQL/completion/Store/control consumers before native revocation, Close and
route removal. Busy keeps service usable. Connection-drained alone is insufficient.
Forced teardown remains R6; this ownership change does not complete its proofs.
See [source review](../../307/R2-R5-SOURCE-OWNERSHIP-REVIEW-20261008.md).

## 1. Observable contract

[owner decision]

Successful `workspace_api.unmount` detaches the mount, closes logical Workspace
ownership, invalidates its live incarnation/capabilities and automatically owns
cleanup of its metadata, physical payload, operation records, orphan and reclamation state.
It discards local uncommitted changes. It never implicitly publishes a Commit.

This is explicit final teardown in both per-tool-call and per-task modes.
A Workspace may serve multiple calls and incremental Commits before it. Neither
Exec completion nor Commit success invokes unmount automatically. Short and
long-lived Execs use the same custody rules; granularity does not establish that
processes, descriptors or mappings have finished.

Automatic batched SQL row deletion can finish after the terminal
reply. Rows remain in the same shared database until those jobs remove them;
there are no separate per-file payload files to delete. This is unmount-owned automatic work requiring no later API call. Success
does not claim that every row was physically deleted or the shared database file
shrunk. The daemon database and other mounted Workspaces remain live.

```text
 caller wants to preserve the tool-call delta
                  |
            explicit commit
                  |
        exact known result required
                  |
                  v
        terminal unmount
          [fence activity] -> [detach FUSE] -> [logical close]
                                                   |
                                   terminal result + accounted reclaim debt
                                                   |
                                   automatic bounded SQL row deletion

 caller deliberately discards local delta
                  |
        terminal unmount directly
                  |
        same detach/close/cleanup; no hidden history publication
```

### 1.1 Logical retirement versus SQL deletion

Successful unmount immediately closes logical ownership after its activity/native
fences. Inode/directory entry/payload rows need not all be deleted before return. The daemon
enqueues their cleanup and continues servicing it even when no further tool calls
arrive; it is not triggered only by another mutation or a manual API. No intentional
TTL retains unreachable rows, and no unmeasured cleanup-time promise is made.

```text
W active -> unmount fences/detaches -> W closed, rows queued -> SQL batches delete rows
                                            |
                                      terminal reply

shared SQLite schema/connection remain; freed space reused for later Workspaces
```

Retain only the terminal/reclaim bookkeeping and any explicitly required custody
until cleanup finishes. A Busy/Uncertain refusal before effects does not retire
Workspace rows. Partial teardown failure reports actual custody rather than
claiming all rows are safe to delete. Global committed objects/history are not
removed by local cleanup, and SQL deletion does not imply database-file shrinking.

The no-change `UpToDate` Commit outcome retains the existing history head.
Unmount does not synthesize a tool-call audit record.

## 2. Inputs and result knowledge

[proposed design; final Rust/wire types remain integration work]

| Item | Required semantics |
| --- | --- |
| Selector | Exact Workspace ID/incarnation and authorized daemon instance |
| Normal policy | Quiescent terminal teardown; no silent process termination |
| Force policy | Explicit forced filesystem abort/detach/drain with exact uncertain-history custody; no implicit process cancellation |
| Success | Native detach and activity/routing fences established; logical ownership closed; automatic cleanup scheduled/owned |
| Busy | Admission refused before terminal effects because filesystem/control owners are active or the kernel reports retained references; Workspace stays usable |
| Uncertain | Exact Commit/native/lifecycle outcome cannot be established; retain ownership/context, do not claim ordinary success |
| Retained partial teardown | Teardown began but detach/drain failed; return phase, actual remaining owner and known state |
| Cleanup debt | Accounted rows/bytes/work still awaiting physical reclamation; not falsely reported as zero |

A normal Busy/Uncertain refusal before teardown is not a successful partial
unmount. Once native effects begin, a failure may retain a stopping/partially
detached owner; do not promise that a failed detach restores usability. The result
and [status](status.md) must distinguish those cases.

## 3. Fair lifecycle admission

Normal/forced unmount atomically arbitrates with Attach/Commit and other exact
namespace control producers. Force refuses active producers before any terminal
effect. Normal admission also refuses actual active filesystem/daemon work and
original uncertain Commit custody. There is no command/session registration gate.

Normal ProbeUnmount freezes new control producers while ordinary FUSE admission
and service continue. One plain detach attempt returning EBUSY withdraws the
control probe with no injected ENOTCONN/EIO. Kernel cached lookups are not active
requests; descriptors/cwd/mappings can keep the kernel busy independently of
daemon open or command counts. Locks never span syscalls, queue waits or joins.

## 4. Terminal workflow and ownership transfer

Normal: control probe → one plain detach; EBUSY → usable Ready. Known detach →
terminal native phase → all loops and daemon work drained → fixed logical native
owner revocation → logical Close/routing removal → indexed physical cleanup debt.

Force: before-effect control producer/capability guard → Stopping → one validated
connection-specific fusectl abort write → wake/dispose native waiters and preserve
attempted-work results → local work drain → one plain detach. Abort and detach
are independent effects. No MNT_FORCE fallback, retry, lazy detach or guessed
connection number. Short/failed/unknown abort or failed detach after abort retains
original phase/effects/custody; it is never the reversible normal Busy result.

Loop exit alone is not full drain. Every received callback/reservation, admitted
request, queued/running service step, FORGET decrement, owner Pending/completion,
publication ticket, Store consumer/subscription and namespace control/capture/
Commit producer must be disposed with original outcome and no continuation able
to access the namespace. A run() error is not join evidence. Only after this
barrier may native indexed lookup/open/processing owners be logically revoked.
Physical rows retire in bounded O(K) indexed turns; no whole-base scan or early
reclamation. Unestablished drain preserves exact Retained custody.

## 5. Commit-aware forced teardown

Both normal and force refuse a running namespace Commit/constructor before
effects. This design adds no interruptible Commit and cannot race publication
or local install. Stopped known publication retains its root/head even if install
failed. Stopped unknown custody requires explicit local relinquishment policy
and remains Unknown in terminal receipt; it is never settled by an observer.
Previously saved immutable waves remain in the Store. No guessed discard, resend,
rollback or history deletion is introduced. [Direct integration](../06-cluster-one-integration.md)
and existing Store Commit own the exact one-attempt sequence and failure phases.

Commands, stdout/stderr and exit/cancellation belong to Sandbox/runtime or an
external executor. Forced filesystem teardown does not signal/kill caller-owned
processes or wait for output drain. Caller cancellation is a separate explicit
runtime action. Accepted filesystem changes survive a lost reply/process exit;
normal kernel busy/detach and full daemon-work drain prove filesystem lifetime.

## 6. Cleanup and large-state cost

[proposed resource contract]

```text
 Closed namespace W
      |
      +--> lease-safe cleanup cursor, fixed namespace/domain
                 |
                 v
         bounded rows/pages/work job
                 |
         delete only unreachable state
                 |
         update actual allocation/debt
                 |
         yield to other runnable Workspaces
                 |
         next cursor window ... EOF

 no DELETE of the entire namespace in one foreground transaction
 no walk of every inode/extent while the terminal caller holds a global lock
 no row count / payload bytes hidden merely because cleanup is background
```

Let N be retained metadata rows and B local payload/operation records bytes. Physical
cleanup necessarily processes attributable retained state; total work can grow
with N/B. Required foreground terminal work is lifecycle/native fencing and a
bounded logical ownership transition, not a scan/deletion of all N rows and B
bytes. A real kernel detach
can still wait for outstanding activity, so no constant wall-time bound is claimed.

Reclamation consumes declared resources, actual page/index work and service share.
Freed pages may be reused without shrinking `overlay.sqlite`; physical file size
is not the live-byte count. Repeated per-tool-call Workspaces must not accumulate
unbounded cleanup debt or exhaust disk while nominally succeeding forever.
Admission, reserved headroom and fair cleanup service belong in the
[engine](../daemon-sqlite.md). Any genuine resource wait/refusal is explicit; no
inherited file-count, edit-count, payload-size or total-flow ceiling is introduced.

Orphan bytes held by live descriptors cannot be reclaimed prematurely. Normal
unmount refuses those handles; forced connection teardown must establish native/daemon-work disposal
and exact detach or retain precise custody. Shared immutable objects are not local garbage and have
no guessed deletion path in these APIs.

## 7. Per-tool-call and concurrent workloads

[proposed validation; IDs are documentation cases, not frozen benchmark selections]

| Case | Workload | Required proof/cost observation |
| --- | --- | --- |
| U1 | Full tree after one command and exact Commit | Native detach + logical close; no separate close; next mount sees history |
| U2 | Many tiny dirty files/wide directories | Foreground unmount does not scan/delete all rows; actual cleanup debt tracked |
| U3 | Big files, many edits, large operation records and old captured state | Payload/operation records ownership fenced; physical reclamation windowed |
| U4 | Continuous logger/open-unlinked descriptor | Normal Busy leaves content live; force releases/fences before reclaim |
| U5 | Unmount races create/Commit/control producers | Exact admission winner and no late stale mutation into another namespace |
| U6 | Simultaneous Workspaces; one unmounts large state while others Commit/write | Fair SQL/runtime/dispatch progress; no global teardown lock |
| U7 | Long-running Bash/background descendant | No implicit timeout/kill in either filesystem path; explicit caller runtime cancellation stays separate |
| U8 | Lost native detach or history stage/transition result | Retained phase/identity; no guessed absence, cleanup or replay |
| U9 | Repeated per-call mount/Exec/Commit/unmount faster than cleanup | Bounded aggregate debt/admission and eventual complete reclamation |

The full development fixture remains 130,045 entries / 3,475,776,149 regular-file
bytes, including `.git`, dependencies, symlinks, ignored caches and output.
Dependency replay is 95,021 entries / 2,126,509,110 bytes. Historical exact
identities and qualifications are in [mount](mount.md); no smaller subset replaces
final whole affected-state proof.

Prove ownership and cancellation deterministically before timing. Future reports
separate detach/drain, logical close, cleanup service/debt and eventual cleanup.
Complete-command budgets include required container/lifecycle/cleanup; moving
necessary measured work into preparation is forbidden. Use cold/equal-cache,
one-sample, separate-proof and append-only rules in
[validation](../07-implementation-validation.md). No workload was run here.

## 8. Current source versus target

The original 2026-10-05 dormant SDK/daemon/FUSE sources at
`f96d97651be5299f153ccde2bc8d921dd58807ad` separated detach from semantic close,
used a serialized selected slot and polling lifecycle. Those are historical
source observations, not current active paths or instructions to restore them.

Current [native control](../../../architecture/68-native-workspace-control.md)
implements authenticated logical unmount/Close and automatic engine cleanup,
refusing active/unresolved Commit custody. It is the pre-S8 Store/engine half;
no kernel attachment, busy probe or native drain is established by its receipts.
R2/R6 supplies the combined terminal native operation and exact FS lifetime
proofs through current S8 owners. Excluded old FUSE is reference until replacement
coverage, never a source include or fallback. No separate public close, daemon
process supervisor or command cancellation route returns.
