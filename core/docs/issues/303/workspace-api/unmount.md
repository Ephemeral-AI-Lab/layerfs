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
| Force policy | Explicit cancellation/termination semantics, including descendant and uncertain-history custody |
| Success | Native detach and activity/routing fences established; logical ownership closed; automatic cleanup scheduled/owned |
| Busy | Admission refused before terminal effects because Execs, handles or Commit are active; Workspace stays usable |
| Uncertain | Exact Commit/native/lifecycle outcome cannot be established; retain ownership/context, do not claim ordinary success |
| Retained partial teardown | Teardown began but detach/drain failed; return phase, actual remaining owner and known state |
| Cleanup debt | Accounted rows/bytes/work still awaiting physical reclamation; not falsely reported as zero |

A normal Busy/Uncertain refusal before teardown is not a successful partial
unmount. Once native effects begin, a failure may retain a stopping/partially
detached owner; do not promise that a failed detach restores usability. The result
and [status](status.md) must distinguish those cases.

## 3. Fair lifecycle admission

[proposed design]

Unmount admission atomically arbitrates with new Execs, new handles and Commit
admission for the exact Workspace. A race has one defined winner:

```text
    Exec / Commit admission                 Unmount admission
               |                                   |
               +------------ lifecycle gate -------+
                                  |
                         one ordered decision
                     +------------+------------+
                     |                         |
            new activity wins          terminal fence wins
            unmount returns Busy       future activity refused
            Workspace remains live     previously accepted work drained/fenced
```

Registry and Workspace locks are not held across process cancellation, FUSE
detach/join, network waits or shared SQL-owner queue waits. Resource/inode waiters
are deferred and cancellable rather than occupying every dispatch worker. A
large Workspace's teardown must not stall mount/Exec/read/write/Commit service
for another Workspace through a synchronous row deletion or maintenance lock.

## 4. Terminal workflow and ownership transfer

[proposed design]

```text
 Caller         Lifecycle owner         FUSE / processes        Shared overlay engine
 ------         ---------------         ----------------        ---------------------
 unmount(W) --> check identity/policy
                    |
                    +-- Busy/Uncertain ------> preserve exact live custody; report
                    |
                    v
               fence new activity
                    |
                    +------------------> drain accepted callbacks/replies/handles
                    |                    cancel parked waiters when appropriate
                    |                    event-driven native detach + worker join
                    | <----------------- exact native disposition
                    |
                    +----------------------------------------> bounded close-state job
                    |                                         namespace unreachable
                    |                                         transfer cleanup ownership
                    | <--------------------------------------- logical close result
                    |
                    v
               invalidate incarnation/capabilities
 result <------ exact terminal success
                                                              |
                                                     fair background cleanup
                                                     metadata / payload / operation records
                                                     orphan leases / retired state
                                                              |
                                                     debt converges to reclaimed

 overlay.sqlite stays open: other Workspaces and later mounts reuse this engine
 immutable shared base/attribute caches remain only within their declared budgets
```

Release/flush and cancellation callbacks needed to finish accepted activity must
remain serviceable after the entry fence. A stopped entry path cannot be allowed
to deadlock the release path. Construction, base-read and transport jobs retain
their operation leases until fenced completion; closed namespace visibility
alone does not make their payload reclaimable.

All queued jobs carry Workspace namespace/incarnation ownership. A stale job
cannot target a new mount. Namespace keys are not reused while retained rows,
leases, deferred jobs or cleanup contexts can still refer to them.

## 5. Commit-aware forced teardown

[proposed design; exact outcome preservation]

Normal unmount refuses an in-flight/uncertain Commit. Force must not guess that
cancelling a local task cancelled the corresponding host operation.

```text
 Commit phase                   Required fence / retained knowledge
 ------------                   ----------------------------------

 capture / construct / Save      stop producer + queued/in-flight calls
 before staging                 history not requested; keep possible saved waves
                                local cancellation may resolve after the fence

 staging completed              exact owned stage token + known discard disposition
                                before ordinary terminal cleanup can succeed

 staging reply lost             stage may exist; retain operation/context
                                no guessed absence / resend / discard

 transition in flight           publication may happen after cancellation request
 or reply lost                  fence original operation and settle exactly,
                                otherwise preserve/report uncertainty

 transition known successful    preserve published head/root even if local install fails
                                unmount cannot undo acknowledged history

 required discard reply lost     preserve exact stage custody; not retryable Idle
```

Exact resolution requires the runtime adapter's authorized operation identity and
completion fencing. Unfenced `GetStage`/`GetCommit` absence is insufficient while
the original request may still execute. One-attempt/no-guessed-retry rules in
[cluster-one integration](../06-cluster-one-integration.md) remain in force.

An explicit forced policy may relinquish local custody with an acknowledged-unknown
result if that product policy is supplied. It must never return ordinary success
implying NotPublished. Dropping the local capture does not delete a shared object,
cancel a host stage or undo a Branch transition. Immutability does not settle
history outcomes or remove authority/reference/durability requirements.

For Exec, forced policy signals/fences the owned processes/descendants, drains or
explicitly terminates their streams, and waits for mount references to release.
Ordinary unmount does not add a timeout to long-running Bash. The old fixed
lifecycle envelope cannot silently kill commands.

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
unmount refuses those handles; forced teardown must establish their release or
retain precise custody. Shared immutable objects are not local garbage and have
no guessed deletion path in these APIs.

## 7. Per-tool-call and concurrent workloads

[proposed validation; IDs are documentation cases, not frozen benchmark selections]

| Case | Workload | Required proof/cost observation |
| --- | --- | --- |
| U1 | Full tree after one command and exact Commit | Native detach + logical close; no separate close; next mount sees history |
| U2 | Many tiny dirty files/wide directories | Foreground unmount does not scan/delete all rows; actual cleanup debt tracked |
| U3 | Big files, many edits, large operation records and old captured state | Payload/operation records ownership fenced; physical reclamation windowed |
| U4 | Continuous logger/open-unlinked descriptor | Normal Busy leaves content live; force releases/fences before reclaim |
| U5 | Unmount races Exec/create/Commit | Exact admission winner and no late stale mutation into another namespace |
| U6 | Simultaneous Workspaces; one unmounts large state while others Commit/write | Fair SQL/runtime/dispatch progress; no global teardown lock |
| U7 | Long-running Bash/background descendant | No implicit timeout/kill in normal path; explicit forced process policy |
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

[source-verified baseline; required migration]

The dormant [SDK](../../../../crates/layerfs-api/sdk/src/workspace.rs) sends
`WorkspaceUnmount` using a 5,000 ms control call. The
[daemon control path](../../../../crates/layerfs-daemon/src/control.rs) detaches
its native mount handle for that operation. Separate
`WorkspaceCloseClean` and [lifecycle.close](../../../../crates/layerfs-daemon/src/lifecycle.rs)
close clean semantic state; the old lifecycle owns only one selected slot.

[MountHandle::unmount](../../../../crates/layerfs-fuse/src/mount.rs) stops
admission, polls Workspace activity/handles/replies, detaches, polls worker
completion and retains its owner on failure. Its comment explicitly permits
local semantic handles to remain after detach. This is detach-only behavior,
not the owner's terminal-unmount contract.

Migrate the SDK/wire/daemon routes to this combined terminal operation, event-driven
fences and bounded namespace cleanup. Preserve exact native-failure custody, but
do not preserve the separate public-close requirement, whole-Workspace scans,
fixed total cleanup envelope or old serialized control slot. Product code has
not yet implemented this contract.
