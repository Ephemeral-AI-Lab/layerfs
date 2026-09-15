# Overlay and snapshot rules

> **Scoped successor, 2026-09-15:** use the
> [sandbox-local snapshot spec and plan](sandbox-local-snapshot-spec-and-plan.md)
> for the new experiment from v0.1.5. Its explicit overrides permit sandbox-owned
> temporary backing, compact resident metadata, one Commit compute worker and
> one sample per case/arm. Non-pausing behavior, stable live identity, snapshot
> correctness and canonical encoding remain required. The text below records
> the earlier host-backed design and is retained for historical context.

Status: owner-discussed requirements, 2026-09-14. Foundation for the next
implementation specification; not an implemented architecture or a qualification
claim. Related product work: [#123](https://github.com/Ephemeral-AI-Lab/layerfs/issues/123).
Reuse applicable infrastructure from [#122](https://github.com/Ephemeral-AI-Lab/layerfs/issues/122).

MUST and MUST NOT identify requirements. The next specification must resolve the
open decisions in this document without weakening these requirements. It must
distinguish existing mechanisms, proposed changes, and measured results.

## 1. Model and terminology

| Term | Meaning |
| --- | --- |
| Workspace | One mutable current filesystem state with its own lifetime. |
| Overlay | Current inode, namespace, attribute, and file-range state over immutable committed backing. |
| Mutation installation | Make one filesystem operation's resulting state visible atomically under that operation's contract. |
| Snapshot acquisition | Acquire an owned, consistent filesystem view for a Commit. |
| Commit construction | Build canonical committed objects from that snapshot. |
| Publication | Conditionally publish the result into branch history. |

Committed history belongs to the branch. The Workspace MUST NOT acquire a
checkpoint/history entry per file update or per successful Commit. Internal
copy-on-write records, ownership references, and change-order tokens are storage
mechanisms, not user-visible Workspace versions or an operation history.

Ordinary operations MUST update current state and change tracking as they happen.
Only snapshot acquisition and committed construction are deferred to Commit.
Repeated edits replace superseded state; disjoint ranges still contributing to
the result remain represented. Changes overwritten or reverted before a snapshot
are represented by their final effect, not by a promise to preserve every action.

```text
Commands / SDK --> current mutable Workspace
                            |
                     acquire snapshot S
                            |
             +--------------+----------------+
             |                               |
      Workspace continues             build from S only
      changing normally                      |
             |                        publish branch Commit
             |                               |
             |                        release snapshot ownership
             v
      same mount, inodes, and open handles

No snapshot is installed back into the live Workspace.
```

## 2. Non-pausing lifecycle

- Commit MUST NOT stop/restart commands, require executions to finish or writable
  handles to close, unmount/remount the Workspace, or replace its live view.
- Commit MUST NOT use a Workspace-wide filesystem-operation freeze, including
  freeze/build/publish/checkpoint/resume as a supposedly simpler implementation.
- Filesystem reads, writes, namespace mutations, and SDK operations MUST be able
  to complete while Commit construction is in progress. Keeping a process alive
  while its filesystem operations wait for Commit is insufficient.
- Normal operation ordering and short atomic metadata synchronization remain
  necessary. Commit MUST NOT retain live-state, lifecycle, or backing-service
  locks across construction I/O in a way that excludes foreground operations.
- Mount identity, live inode identity, file offsets, append behavior, working
  directories, and supported open-handle semantics MUST survive Commit. Ordinary
  open descriptors follow live inodes; retained snapshot readers follow their
  captured view. These are different ownership contracts.

## 3. Snapshot boundary and sequential Commits

Each Commit MUST acquire one consistent snapshot and read exclusively through it.
Snapshot registration MUST be ordered with mutation installation and reclamation.
Related records of an atomic rename/link/unlink operation MUST NOT be captured
partially. Application sequences spanning multiple operations are not implicitly
transactions, and application-private buffers are not filesystem state.

A mutation included before the acquisition boundary belongs to that snapshot.
A later mutation MUST NOT leak into the snapshot, even if construction reads the
affected file after that mutation completes. Missing snapshot data MUST NOT be
substituted with current live data.

Initially, there MUST be at most one in-flight Commit per Workspace. Queued
requests acquire their snapshots when execution starts, after the preceding
Commit reaches a resolved outcome. Queue length and retained work must be bounded.
This ordering MUST NOT impose global construction serialization across Workspaces,
branches, or a LayerStack. Preserve existing branch admission/lease rules and
conditional publication; this document does not independently authorize multiple
same-branch leases. Necessary Store publication transactions are not a reason to
hold a global lock for the whole build.

```text
Time   Live Workspace             Commit 1               Commit 2
----   ------------------------   ---------------------  ------------------
t0     state A
t1                                acquire S1 = A
t2     apply x; state A + x        build S1
t3     continue operations        publish C1 = A
t4                                                       acquire S2 = A + x
t5     continue operations                               publish C2 = A + x

Branch: C0 --> C1 --> C2
```

If x remains effective at S2, C2 MUST contain it and C1 MUST NOT. If x is
superseded before S2, C2 contains the resulting current state instead.

## 4. Host backing and container FUSE placement

The host MUST own the persistent Store, temporary shared backing,
snapshot-retained storage, and Commit construction. The container serves the
live FUSE mount with bounded caching and its assigned operation-ordering duties.
The specification MUST choose exactly one authority for installed metadata and
define mutation acknowledgment, cache visibility, and snapshot acquisition across
the host/container boundary. Cached copies and exported facts MUST NOT become
competing authorities. Snapshotting stale host facts is insufficient.

```text
CONTAINER                            HOST
---------                            ----
Commands / SDK route                 Workspace backing service
       |                                    |
FUSE mount /workspace/a               current overlay root
       |                             retained snapshot root S
Live operation adapter                      |
Bounded cache -------- protocol ---- shared metadata/index pages
                                     replacement-byte segments
                                     committed base references
                                            |
                                     snapshot reader --> Commit builder
```

`/dev/fuse` is the kernel/userspace device interface, not a backing directory.
The mount root is placement-configured; there is no `/dev/fuse/<workspace-id>`.
Snapshots MUST NOT require a second mount, a copied directory tree, a visible
snapshot directory inside the Workspace, or live-path reads through FUSE.

The current SDK uses:

```text
<host temp directory>/layerfs-runtime/<pid>-<counter>/
    workspaces/<workspace-id>/spool/
```

Lower-level callers may supply another runtime root. Current spool segments are
opened and immediately unlinked; their bytes remain on the backing filesystem
through retained descriptors. The new private metadata layout is not yet chosen.
It should share the Workspace's host backing rather than duplicate it per snapshot.
Snapshot descriptors may remain in host memory; referenced pages/ranges use bounded
buffers/cache and temporary disk. Temporary backing MUST NOT imply crash durability,
restartable Workspaces, or process checkpointing.

## 5. Storage efficiency and ownership

- Preserve one authoritative current state per live inode and consistent namespace
  bindings. Hardlink aliases share the inode; open-unlinked content stays available
  to its handles without being resurrected into the committed namespace.
- Small edits MUST NOT require full-file copy-up or rewriting unchanged payload.
  Retain committed/base ranges and store replacement ranges. Snapshot acquisition
  MUST share existing content and metadata instead of copying the complete state.
- Metadata updates MUST be incremental: a small operation must not serialize the
  complete Workspace, wide directory, or entire growing fragmented-file record.
  Updating affected tree/index paths and required bounded nodes is permitted.
- Indexes, dirty tracking, backing registries, and reclamation bookkeeping MUST
  themselves be bounded or disk-backed. An unbounded inode-to-offset map is not
  a bounded-memory design merely because record bodies are on disk.
- Reuse shared payload segments where suitable. Bound descriptor counts and avoid
  one open backing file per logical file or range.
- Use bounded buffering, batching, and locality. Do not require a separate durable
  flush per ordinary mutation merely to provide snapshots. Define the actual
  supported fsync/durability semantics separately and preserve them.
- Current state, snapshots, retained readers, and in-flight operations own backing
  independently. No page or byte range may be reused until all relevant owners
  release it. Ownership of disk-resident records must not depend solely on live
  in-memory `Arc` references.
- Reclaim superseded intermediate states once unowned. Declare measurable bounds
  or triggers for deferred reclamation, including partially live segment slack.
  A long-lived owner may retain necessary storage; it may not justify retaining
  unrelated historical updates indefinitely.
- Repeated Commits MUST NOT inherently accumulate redundant temporary histories.
  Safely reuse equivalent committed backing or otherwise bound duplication when
  live state still references bytes already published. Such substitutions must
  preserve live contents and identity without global checkpoint installation.

Logical live bytes, physical allocated bytes, index overhead, snapshot-only
retention, shared backing, and cleanup backlog MUST be reported separately.
Shared bytes must not be double-counted as independent physical allocations.

## 6. Fast snapshot acquisition

Acquisition MUST have bounded work independent of total files, changed files,
pieces, and payload bytes. A constant-sized root/descriptor acquisition with
ownership registration is the preferred design target, not a measured claim.

Acquisition MUST NOT walk the namespace, clone the changed-node map, serialize
all dirty records, export all changed facts, drain the entire cache, or copy
payloads. All snapshot-visible state must already have a valid retained read path,
including data still buffered rather than physically written to disk.

Pinning a snapshot MUST pin storage ownership, not its whole metadata set in RAM.
The retained pages remain evictable. Snapshot acquisition must not be made
superficially constant-time by shifting a whole-overlay copy onto the first
post-acquisition mutation or allocating an unbounded ownership registry later.

Measure the entire acquisition path, including synchronization, host/container
handoff, and retention registration. Ordinary resource contention is possible;
it must not conceal a Commit-wide operation barrier or unbounded acquisition work.

## 7. Incremental Commit and committed storage format

Preserve CAS identity/authentication, deduplication, existing small-file FULL/DELTA
eligibility and selection, large-file CDC/extent processing, compression, and
conditional branch publication. Not every small file must become a DELTA, and
CDC does not require rechunking every changed large file in full. Snapshot support
is not a rewrite of the committed storage engine.

Init and Workspace Commit MUST continue to reuse the existing shared content
construction, bounded output/admission machinery, canonical formats, and common
namespace/inode primitives where applicable. Snapshot support MUST NOT introduce
a second CAS/CDC/FULL-DELTA construction pipeline. Their input planning and
publication lifecycles remain distinct: Init discovers its source and assembles
initial state; Workspace Commit consumes captured changes and applies incremental
updates. Sharing machinery MUST NOT make Commit traverse the initial source or
reconstruct unaffected state. Do not force distinct planners into a new generic
framework merely to claim that every stage is identical.

Temporary replacements need not become canonical objects on every write. Safe,
bounded existing precomputation may be reused when tied to the exact captured
content; it must not force foreground operations to wait for Commit completion.

The ordinary live/FUSE path MUST construct from captured changes without a full
mounted-filesystem scan to discover them. Preserve the dirty-inode frontier,
per-directory binding deltas, inode indirection, link-reference accounting, sorted
tree updates, unchanged-object reuse, and bounded task/result/spill journals.
FUSE enables mutation observation; these implementation techniques provide locality.
Affected tree traversal and work required by genuinely large operations remain
necessary. Materialized/reconciliation paths must not be misrepresented as the
ordinary live/FUSE path.

Successive Commits MUST remain incremental. After a large Commit, a small changed
subset must not cause all changes since Workspace creation to be reconstructed.
The specification must define snapshot-consistent change enumeration, removals,
new inode identity, and correspondence to previously produced canonical objects.
Live content may retain older base references; predecessor mismatch must not
silently turn every later Commit into full-file comparison/reconstruction.

Stream required data with bounded buffers. Process content by distinct inode,
not repeatedly for every hardlink name. Bound Commit CPU/I/O concurrency and
avoid long shared backing locks so foreground latency remains acceptable.

Remove mandatory live-state coupling from ordinary Commit, not useful shared
algorithms indiscriminately. Replace frozen borrowed maps with owned snapshot
access and replace checkpoint/reset completion with exact attempt/coverage
bookkeeping. Useful bounded checkpoint-result journals may be reused for canonical
correspondence without installing them into live files. Trace remaining consumers
before removing gates, fact transfer, path tracking, or checkpoint helpers used
by fsync, SDK cache coordination, reconciliation, or explicit lifecycle operations.

## 8. Publication, failure, and cleanup

The pipeline is snapshot -> canonical object construction/admission ->
`workspace_stages` candidate root -> conditional branch publication. Staging is
not another history Commit or a second copy of the candidate payload. Preserve
the existing expected-head/base validation and atomic publication/stage-retirement
transaction. Removing the stage record does not delete objects referenced by the
new Commit. One retained stage per Workspace is sufficient initially; a later
attempt must not overwrite an unresolved earlier candidate.

Stage/publication state belongs to the Commit attempt, not filesystem activity.
Retry MUST use the same candidate and expected context instead of rebuilding from
newer live state. Distinguish the published comparison root/head/base from live
content provenance; advancing only expected head is insufficient when live ranges
still reference an older base. Snapshot ownership and canonical stage ownership
have different release conditions. A retained stage may delay the next Commit but
MUST NOT by itself make ordinary Workspace operations inactive.

Successful publication updates branch history, expected branch context, and the
bookkeeping identifying the captured changes covered. It MUST NOT install snapshot
contents into live inodes, reset current overlay state, or globally clear newer
dirty changes. Snapshot acquisition is not publication; retries must distinguish
these stages.

Capture/build/publication failure MUST preserve current valid Workspace state,
including mutations made during construction. A head conflict or retained candidate
must not by itself freeze the Workspace or discard pending changes. Preserve
conditional publication and specify reconciliation/retry behavior separately.

If publication succeeded but acknowledgment or cleanup failed, retry must resolve
that same publication without creating a duplicate Commit or rolling live state
backward. Release snapshot ownership only when construction and required retry
handling no longer need it. Cleanup failure and publication success are distinct
outcomes; retain accurate resource charges until cleanup succeeds.

## 9. Memory safety, budgets, and FUSE visibility

Memory safety and bounded allocation are separate requirements. Specify independent
host/container budgets and an aggregate policy for concurrent Workspaces. Account
for metadata/index residency, dirty buffers, inline payloads, read pins, queues,
snapshot bookkeeping, worker scratch, and transient old/new copy-on-write state.
Container resource limits do not cap host RSS.

Reserve peak transient resources before installation. Quota exhaustion or backing
I/O failure must preserve the previous readable state. Bound queues, file descriptors,
concurrent builders, and I/O buffers; stream large files. Preserve public input
limits and trust-boundary validation. No unlimited-capacity claim follows from
disk backing. Distinguish action count, live pieces, changed inodes, namespace
size, RAM allowance, disk quota, and input limits; remove universal edit-count claims.

The specification MUST define snapshot visibility for completed writes, buffered
writes, kernel writeback, writable mappings, and concurrent SDK/FUSE operations.
Capturing consistent daemon metadata while omitting bytes promised visible by the
filesystem contract is incorrect. This must be solved without the forbidden global
freeze or silently disabling existing supported behavior. It is a prerequisite
design/qualification gate, not deferred implementation cleanup.

## 10. Required correctness evidence and performance evaluation

Owner decision, 2026-09-14: implement correctly, then evaluate using applicable
existing benchmarks and inspect the measured results. Do not add new numerical
performance gates, capture targets, or a mandatory replacement benchmark campaign
at this stage. Existing benchmark contracts and established criteria remain in
force. Record their required fixtures, budgets, host/source identities, timing and
verification; follow [benchmark rules](../../../general/benchmark_rules.md).
Correctness, non-pausing semantics and bounded resource ownership remain required.
Add focused coverage only where existing tests/benchmarks leave an identified gap.

Required public-path evidence includes:

- Exact snapshot contents and metadata before publication and after fresh Store
  reopen, under deliberately small metadata caches with proven spill/reload.
- At least 1,000,000 distinct changed regular files in one Workspace before one
  final Commit, with declared fixed work, deadline, RAM allowance, disk quota,
  and verification coverage. No incremental-Commit or multi-Workspace substitute.
- A large unchanged namespace with a small changed subset; report namespace costs
  and prove changes are discovered through captured deltas rather than a full scan.
- Fixed-work same-range/whole-file overwrites and disjoint edits; prove correct
  results and reclamation after retained owners release their references.
- Multiple sequential Commits without restarting the Workspace, including a small
  Commit after a large one. Hold construction at a test barrier after acquisition:
  complete reads, writes, namespace changes, and SDK operations before releasing
  the builder. Verify x is absent from C1 and present in C2 when still effective.
- Equal/unequal-length replacements, insert/delete, append/truncate, sparse ranges,
  boundary and large files, spool-backed writes, hardlinks, rename/unlink,
  open-unlinked lifetime, held readers, and supported writeback/mapping behavior.
- Concurrent Workspace isolation, expected-head conflicts where supported, quota
  and backing-I/O failures at transitions, publication acknowledgment uncertainty,
  idempotent retry, snapshot release, and Commit/Discard/end cleanup.
- Affected Init regressions when changing shared construction/admission or
  namespace primitives; preserve its existing fast paths and supported outputs
  under their original identity/format context. Do not assume independently
  initialized and committed trees must have identical roots with different inode
  identities or predecessor contexts.

Measure acquisition latency and work, foreground p50/p95/p99 latency during Commit,
edit metadata write amplification, records/tree nodes visited, bytes reconstructed,
construction/publication time, host/container RSS, cache/index/transient/pinned
bytes, spill I/O, descriptors, physical temporary growth, and reclamation. Compare
matched small-workload before/after results and immediate post-snapshot writes.
Passing RSS alone does not prove correct accounting; accounted cache bytes alone
do not prove a bounded process.

Retain failures and timing misses. Do not weaken work, verification, or resource
limits, introduce intermediate Commits, or pause commands to obtain a pass.
The million-file proof is explicitly selected; it is not silently added to #122's
regular 15-second matrix.

The existing v0.1.6 [roadmap](README.md) and [review decisions](review-decisions.md)
require benchmark helpers/writers to finish before certain Commits. Those existing
cases do not prove this non-pausing model. Preserve their declared contracts and
evidence; explicitly add/version overlap qualification. Their fences are not
permission to impose the forbidden freeze on the new product design.

## 11. Decisions required in the next specification

| Decision | Required constraint |
| --- | --- |
| Metadata/index structure and snapshot representation | Bounded caches/indexes, incremental updates, bounded acquisition without copying the changed set. |
| Metadata authority and host/container protocol | One installed-state authority; correct acknowledgment and snapshot visibility; no stale/mixed facts. |
| Buffering and batching | Bounded resources and efficient foreground operations without hidden snapshot drainage. |
| Successive-Commit tracking and canonical correspondence | Preserve post-snapshot changes and avoid cumulative rebuilds without live checkpoint installation. |
| Ownership and reclamation | Disk-resident references are safe; snapshot pages are evictable; physical slack and cleanup are bounded/measurable. |
| Kernel writeback and writable mappings | Explicit supported semantics and non-pausing snapshot proof. |
| Publication/retry state machine | Branch history remains conditional and idempotent; failures do not unnecessarily freeze live state. |
| Runtime paths, quotas, and evaluation | Explicit host/container placement and aggregate resource budgets; use existing benchmark contracts after correctness, without new numerical gates now. |

Choose the smallest implementation satisfying these rules. Do not prescribe a
new database/dependency prematurely, remove resource checks, reintroduce explicit
Store compaction, or claim completion from descriptor arithmetic alone.

## 12. Existing implementation starting points

Source inspected during discussion: `0814cc37f`. These are reuse/repair references,
not claims that the proposed lifecycle already exists.

- [Piece trees, compact forms, and mutation admission](../../../../crates/layerfs-workspace-core/src/file_edit.rs)
- [Current live maps and frozen input view](../../../../crates/layerfs-workspace-core/src/lib.rs)
- [Retained backing ownership](../../../../crates/layerfs-workspace-core/src/backing.rs)
- [Spool segments and retained range readers](../../../../crates/layerfs-workspace/src/file_io.rs)
- [Detached candidate inputs, file fast paths, task journals, and frontier spill](../../../../crates/layerfs-workspace/src/changes.rs)
- [Sorted namespace/inode updates](../../../../crates/layerfs-content/src/tree/batch.rs)
- [Host backing and exported facts](../../../../crates/layerfs-workspace/src/live_backing.rs)
- [Existing freeze/checkpoint Commit lifecycle to replace](../../../../crates/layerfs-workspace/src/lifecycle.rs)
- [Current checkpoint installation and global reset assumptions](../../../../crates/layerfs-workspace-core/src/checkpoint.rs)
- [FUSE operation gates](../../../../crates/layerfs-fuse/src/live_runtime.rs)
- [Live-owner freeze, cache coordination, and checkpoint protocol](../../../../crates/layerfs-fuse/src/live_owner.rs)
- [Existing sequential-content precomputation](../../../../crates/layerfs-workspace/src/capture.rs)
- [Existing temporary object spill/index facilities](../../../../crates/layerfs-layerstack-store/src/objects/spill.rs)
- [SDK runtime-root placement](../../../../crates/layerfs-sdk/src/client.rs)
