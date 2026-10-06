# Workspace queues: fair scheduling for concurrent workloads

> **Status:** Current general guide.
> Source review: 2026-10-05, committed local `main`
> `6e84b91818bff510f4902537e413eda5094cf754`.
> Documentation issue: [#310](https://github.com/Ephemeral-AI-Lab/layerfs/issues/310).

Navigation links were updated on 2026-10-06 for the responsibility-folder moves.
The source-review claims retain their original pin above; current continuation
scope is in the [S7–S9 handoff](../../core/docs/issues/307/HANDOFF-S7-S9.md).

This guide explains how the daemon's bounded Workspace queues support concurrent
workloads over one overlay database. It documents the committed scheduler and
its intended native integration without treating concurrency, fairness, throughput
and total memory qualification as equivalent. Concurrent staged S3 source changes
are outside this review.

The [daemon contract](../../core/docs/issues/303/daemon-sqlite.md),
[FUSE scheduling contract](../../core/docs/issues/303/fuse.md#3-callback-ownership-and-scheduling),
[runtime integration contract](../../core/docs/issues/303/06-cluster-one-integration.md)
and [implementation tracker #307](https://github.com/Ephemeral-AI-Lab/layerfs/issues/307)
own the requirements. The [S2 exit audit](../../core/docs/issues/307/S2-EXIT-AUDIT.md)
records completed generation/service evidence. This document selects no new
connection topology, algorithm, resource allowance or performance campaign.

## 1. Concurrency is at Workspace and operation scope

Many Workspaces may run commands at the same time. A single Workspace may also
serve multiple sequential or concurrent calls. Both per-tool-call and per-task
orchestration use the same filesystem and service contracts; command duration
and explicit Commit/unmount cadence are independent.

Each producer submits short jobs when it needs mutable overlay state. The shared
owner executes those jobs one at a time. A whole Bash command, Commit, host object
fetch or content-construction operation is not one database job.

```text
Workspace A: commands / reads / writes / captures -----+
Workspace B: commands / reads / writes / captures -----+
Workspace C: observations / close / owner releases ----+
                                                      |
                                                      v
                                        Byte and slot admission
                                                      |
                                                      v
                           +------------------------------------------+
                           | Bounded per-Workspace service lanes      |
                           |                                          |
                           | NS A: read / mutation / capture / ...    |
                           | NS B: read / mutation / lifecycle / ...  |
                           | NS C: lifecycle / scratch / ...          |
                           +---------------------+--------------------+
                                                 |
                                 Namespace and service-class rotation
                                 Skip work whose prerequisites wait
                                                 |
                                                 v
                           +------------------------------------------+
                           | One initialized overlay owner thread     |
                           | One short typed SQL job per service turn |
                           | Bounded maintenance turns interleaved    |
                           +---------------------+--------------------+
                                                 |
                                                 v
                                          overlay.sqlite
                             Metadata / payload / scratch / custody
```

The queues contain admitted outstanding requests, not an inventory of every
file or every live Workspace. Authoritative mutable namespace/generation state
remains backed by SQLite.

## 2. Why direct Workspace connections do not solve write contention

SQLite permits only one writer at a time per database file. Separate connections
to the same file still contend for that writer. WAL can allow readers and a
writer to proceed concurrently, but still has one writer. See the official
[SQLite concurrency guidance](https://www.sqlite.org/whentouse.html) and
[WAL documentation](https://www.sqlite.org/wal.html).

The selected overlay uses one owner connection with MEMORY journal,
synchronous OFF, EXCLUSIVE locking and mmap disabled. In this topology, overlay
reads also take turns with other jobs. One database does not inherently require
one connection: the daemon contract records a bounded WAL reader pool as an
unselected alternative with separate snapshot, pager, checkpoint/WAL-growth and
restart obligations. Adding connections is not a drop-in change to the current
exclusive profile.

SQLite locking supplies database transaction exclusion. LayerFS additionally
needs Workspace-aware fairness, a finite capture frontier, resource admission,
lifecycle capacity and exact lost-result custody. Giving callers raw connections
does not establish those contracts.

| Requirement | Scheduler/owner responsibility |
| --- | --- |
| Unrelated Workspace progress | Rotate admitted namespace lanes and runnable service classes |
| Stable capture ordering | Drain the finite earlier mutation/reply frontier and defer later same-Workspace mutations |
| Bounded resident requests/results | Charge owned input, reply windows and retained-result lifetimes |
| Release/close progress under saturation | Reserve lifecycle bytes and per-lane slots |
| Automatic local cleanup | Give bounded maintenance turns and continue eligible work while idle |
| Original failure and uncertainty | One attempted operation with exact retained result/custody, not automatic SQL replay |

## 3. Concurrent work with short database turns

The following is illustrative ordering, not a measured trace or fixed priority:

```text
Time -------------------------------------------------------------->

Workspace A: [submit A1]  ... CPU / content / network ... [submit A2]
Workspace B:    [submit B1] ... command work ...             [submit B2]
Workspace C:       [submit release C1]

SQL owner:   [ A1 ][ B1 ][ C1 ][ A2 ][ maintenance ][ B2 ] ...
             Only each short database job owns the connection
```

The queue can postpone a request until another job finishes. An expensive SQL
job or slow storage still delays other work; queue fairness does not eliminate
that bottleneck or provide a latency guarantee. Bounds on processing windows
must be accompanied by actual statement, page, copy, wait and service evidence.

Network acquisition, chunking/hashing, canonical construction and Bash execution
occur outside the overlay owner. Queue locks are released before SQL and result
delivery. A caller must not keep a Workspace lock while waiting for SQL or base
acquisition. Complete native dispatch integration remains owning S8 work.

## 4. Namespace rotation, service classes and source responsibilities

At the reviewed committed source, each admitted namespace has five classes:
`Read`, `Mutation`, `Capture`, `Lifecycle` and `Scratch`. The lane rotates among
classes and the shared scheduler rotates among namespaces. FIFO heads are checked
for runnable state and the finite capture-order constraints before selection.
Later classes added by staged/future slices require their own source/evidence
update; directory presence or this explanation does not qualify them.

| Source file | Responsibility |
| --- | --- |
| [`commands.rs`](../../core/crates/layerfs-daemon/src/overlay/commands.rs) | Typed bounded commands/responses, class selection, input/reply charge and overlay dispatch |
| [`queue.rs`](../../core/crates/layerfs-daemon/src/overlay/queue.rs) | Lanes, rotation, runnable/parked jobs, event-driven waiting and fixed scheduling observations |
| [`owner.rs`](../../core/crates/layerfs-daemon/src/overlay/owner.rs) | Connection startup/readiness, admission, one-attempt execution, completion and maintenance |
| [`credits.rs`](../../core/crates/layerfs-daemon/src/overlay/credits.rs) | Resource ownership through queued, executing and caller-retained results |
| [`overlay`](../../core/crates/layerfs-overlay/src/database/connection.rs) | Indexed SQL, atomic mutation/capture/install state and exact database errors |

The dispatcher examines configured outstanding lanes/classes; finite-frontier
checks inspect bounded admitted jobs. These roster sizes are admission dimensions,
not total file/edit/Workspace/Commit-count caps. Fair service turns do not imply
equal elapsed time when job costs differ.

## 5. Capture waits without stopping unrelated Workspaces

A locally published mutation retains its publication/reply-attempt ticket. Capture
must follow its earlier finite domain. Readiness is checked before the capture's
single attempt; a waiting capture is parked rather than holding the SQL owner.

```text
Workspace A:
  earlier mutation M1 --> published state + ticket T1
                                 |
  capture C ---------------------+ waits for its earlier obligations
  later mutation M2 -------------> deferred behind C

Meanwhile the shared owner can serve:
  Workspace B's runnable jobs
  A's lifecycle job settling the original reply-send attempt
  Other eligible classes and bounded maintenance

Once the earlier frontier settles:
  C seals generation G --> new active generation G+1 --> M2 can publish
```

The frontier concerns locally published state and ordered reply-send attempts;
it is not proof that the kernel received/applied a reply. Lost replies cannot
erase published mutations. Observing retained tickets/captures does not settle,
resend or replay them. Unpublished dirty mmap stores remain outside this frontier.

Native inode/upstream/resource waiters must retain their bounded reply/cancellation
state and return dispatch workers to runnable work. The committed SQL scheduler
supports parked readiness; full kernel dispatch/reference integration remains
separate from its engine proofs.

## 6. Admission and last-owner resource accounting

At the reviewed source, `OwnerConfig` defaults are:

| Setting | Default | Actual scope |
| --- | ---: | --- |
| Credited bytes | 8 MiB | Aggregate admitted inputs, declared replies and retained results |
| Lifecycle reserve | 64 KiB | Capacity unavailable to ordinary admission |
| Namespace lanes | 16 | Concurrent outstanding credited lanes, not all Workspaces |
| Ordinary slots per lane | 16 | Queued/executing/caller-retained ordinary work |
| Lifecycle slots per lane | 2 | Separate per-lane lifecycle admission |

`try_submit` either admits the bounded job or returns `(OwnerError, Command)`
before the SQL attempt. An `AdmissionFull` result preserves the original owned
command. Native callers can defer admission without occupying workers; this is
distinct from retrying a failed/uncertain database operation.

Credits follow the pending job and completion envelope. Retained result data is
borrowed from its charged owner. After the last resident owner drops, byte/slot
charges are released; an empty lane leaves the resident roster. A lost internal
completion can release resident credits while publication/capture custody remains
backed in the database. Those are separate lifetimes.

These logical charges do not bound native producers' allocations before admission,
pager/journal memory, base caches, decoding/transport buffers, kernel references
or host/guest file-cache residency. Aggregate accounting and physical headroom
remain required. No Bash runtime timeout, total-output cap or silent data dropping
is introduced by queue admission.

## 7. Idle work, stopping and failure

The owner gives eligible terminal cleanup a short turn after at most eight
dispatched foreground jobs, and continues ready cleanup while idle. Namespace
rotation prevents one large ready namespace from taking all cleanup turns.
Event-revision checks plus a condition variable avoid idle polling and lost
admission/progress wakes.

The first automatic maintenance error is retained and further automatic attempts
stop. It does not turn a previously accepted ordinary mutation into failure,
retry cleanup or infer successful deletion. Original engine quarantine/uncertainty
continues to apply where relevant.

Stopping fences new admission and cancels unattempted queued work. An already
attempted job retains its actual result. Stopping is not rollback of published
state and is not the same as one Workspace's terminal unmount.

## 8. Proven scope and remaining workload acceptance

The existing S2 evidence covers unrelated progress while capture waits, lifecycle
admission under ordinary saturation, caller-retained result credits, lost internal
completion custody, stop fences, fixed capture/install behavior and exact route
affinity. The completed S2 identity is
`8e2976e4e08dfda70decc99225eb4bb14c3e204a`; its covering receipt reports 565 host
tests and 21 Linux ARM64 overlay/daemon tests. This guide reuses that record at
its original scope and runs no tests or measurements.

The public [daemon tests](../../core/crates/layerfs-daemon/tests/owner.rs),
[S2 audit](../../core/docs/issues/307/S2-EXIT-AUDIT.md) and
[terminal cleanup guide](../../core/docs/architecture/25-terminal-reclaim.md)
provide the source/evidence trail. Those scoped proofs do not establish full
mounted concurrency, sustained throughput or whole-system resource acceptance.

Owning S7/S8/S9/S12 work must correlate per-Workspace/class queue wait, SQL service,
resource/inode/upstream wait, actual request/statement/page/byte/copy work,
aggregate credited/retained memory, kernel/runtime buffers and cleanup debt.
Continuously arriving work must not starve capture, release or cleanup. Fair
rotation cannot compensate for a service rate below sustained demand.

The single-connection read path is a real tradeoff to qualify. If evidence shows
unacceptable read contention, a topology change needs explicit contracts and
proof; it is not selected by this document. Measurements follow the
[measurement workflow](agent-measurement-policy.md),
[benchmark rules](benchmark_rules.md) and
[core harness instructions](../../core/benchmark/fs-bench-pro/AGENTS.md), with
frozen cases/cache states, pinned builds and retained actual outcomes.

Related explanations: [sandbox cache design](sandbox-cache-design.md) /
[#308](https://github.com/Ephemeral-AI-Lab/layerfs/issues/308), and
[Workspace filesystem view](workspace-filesystem-view.md) /
[#309](https://github.com/Ephemeral-AI-Lab/layerfs/issues/309).
