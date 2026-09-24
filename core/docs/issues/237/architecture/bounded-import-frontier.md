# #237: bounded frontier for native import

> **Status: Proposal; target LayerFS 0.1.7; not a released contract.**
> Source inspected at `00c677489f813d6157a72e245203e74123e732f6`.
> This note describes an option; no bounded-frontier C3 implementation or
> multi-writer memory result is claimed.

## Problem and boundary

The native `Client::init_project` route currently scans the whole source before
constructing files. [`scan_and_save`](../../../../crates/layerfs-service/src/save/import/scan.rs)
retains one `PreparedEntry` for every path and one `Job` with a full native path
for every regular file. After file Save, [`build_namespace`](../../../../crates/layerfs-service/src/save/import/namespace.rs)
retains the entries while constructing whole-tree serial, metadata-root,
content-root, inode and directory inputs for C1.

```text
Current C3 (N entries, F regular files)

source tree -> scan all -> Entries[N] + Jobs[F] -> four workers -> C2 file Save
                           |                                      |
                           +----------- Entries[N] --------------+
                                            |
                               whole-tree namespace vectors
                                            |
                              C1 build(base=None) -> C2 Save
                                            |
                                    C5 initialization
```

The retained [compact-Job diagnostic](../c3-job-metadata-memory-result-20260924.md)
had 100,000 jobs and 101,001 entries. At scan end, the job vector reserved
9,437,184 B, its path lengths totaled 16,000,000 B, and the entry vector
reserved 14,680,064 B. The candidate's process peak rose from 76,480,512 B
at scan end to 105,857,024 B after the file loop and 144,703,488 B by
namespace Save. Namespace input vectors alone reserved 18,838,440 B. These
are different accounting domains and observation points; they are not
additive parts of the final RSS peak. The old v0.1.6 t1 peak was 92,405,760 B
in a different public API and diagnostic identity, so the remaining
52,297,728-B difference is context, not a paired memory admission result.

The v0.1.6 [direct initialization path](../../../../../crates/layerfs-layerstack-store/src/layerstack.rs)
uses an 8-MiB *one-time task planner* for root and selected nested subtrees,
at most 1,000 task blocks, bounded file tasks and a bounded output handoff.
Workers recursively process the unexpanded subtrees; this is not a refillable
queue over every source path. The path can fall back to a serial importer.
Its 8-MiB number is not a whole-process RSS limit, and its canonical output
format differs from Core's.

## Proposed import frontier

The first queue candidate would bound **file jobs only**. Give each native
import a fixed byte/count budget for pending jobs and admit another job when
space opens. Keep the existing four file constructors and one Save owner;
the owner continues to drain finalized objects during backpressure. A full
discovery bound would additionally need a bounded breadth-first directory
queue and a bounded way to sort a high-fanout directory: today's scanner
collects all its children before sorting. The existing 4,097-directory
[fanout test](../../../../crates/layerfs-service/tests/history.rs)
expects import to succeed, so an arbitrary new input refusal is not a
drop-in memory fix.

```text
One native import

source directories -> scanner/feeder -> [bounded jobs] -> four constructors
                         |  ^               |                       |
                         |  | space opens   |                       v
                         |  +---------------+              [bounded results]
                         |                                      |
                         +-> Entries[N] <--- completions --- one C2 Save owner
```

Every in-flight job, completion and worker output needs an owner and a bound;
moving an unbounded list behind the queue does not bound memory. This first
step still retains `Entries[N]`, its completed roots, the directory queue and
each directory's sorted child list. The Save owner cannot block filling the input queue
while workers block on a full result queue: it must continue draining results.
Source device/inode and post-read length/mtime checks, deterministic parent
relationships, cancellation/deadline behavior, and definite-failure cleanup
must survive the change. Starting file construction before the full scan would
move file reads earlier relative to source mutations; that semantic timing
change needs an explicit decision and check. No Store transaction may remain open while waiting
for queue capacity. This is per-import backpressure, not a Store-wide mutex or
a new writer budget.

The explicit job/path storage now scales with all `F` files. A bounded job
queue would make that part scale with its budget `B` instead. For illustration
only, 512 jobs at the measured 72-B `Job` size and 160-B average path length
occupy about 118,784 B before capacity and allocator overhead, versus the
measured 25,437,184 B of job-vector reservation plus path lengths for 100,000
jobs. **This arithmetic is not a predicted RSS reduction.** The budget value
and effect on throughput remain unselected and unmeasured.

## Shared C1 path and end-to-end limit

Bounding file jobs alone leaves `Entries[N]` and the later namespace vectors
in place. C1's [`FilesystemInput`](../../../../crates/layerfs-content/src/filesystem/input.rs)
currently borrows complete directory, inode and new-serial slices. Import
calls `build_filesystem` with `base: None`; Workspace Commit calls
`update_filesystem_timed` with `base: Some(...)`. Both wrappers enter the same
[C1 `run` implementation](../../../../crates/layerfs-content/src/filesystem/update.rs).
Core already has a direct fresh-build count path inside that implementation;
the missing memory change is bounded input, not the earlier direct-build
speed treatment ([fixed-identity proof](../c1-fixed-identity.md)). The
[fresh-namespace proposal](bounded-compact-fresh-namespace.md) traces C1's
remaining whole-tree state and required proofs.

```text
native import ----> build(base=None) -----+
                                         +--> shared C1 run --> C2 private Save
Workspace Commit -> update(base=Some) ----+
```

An **end-to-end bounded import** therefore needs a bounded fresh-tree input
path as well as bounded discovery. The current History route reserves one
contiguous inode range after it knows `entries.len()`; the design must account
for that count and preserve parent/serial mapping, final binding order,
reference counts, validation, canonical root and readback. A counted pass,
incremental reservation or operation-owned external records each have distinct
correctness and I/O costs; this note selects none. Any spill or second read
belongs inside the measured operation and must be reported in disk, page-cache
and process-memory domains. A fresh-build-specific extension should leave
Workspace update behavior intact unless a shared C1 change is independently
justified and verified.

## Concurrent Workspace Commits to one Store

The frontier is owned by a native import. Workspace Commit does not consume
that frontier, but it shares C1, C2 and the same per-Store writer authority.
[`Workspace::commit`](../../../../crates/layerfs-workspace/src/commit/operation.rs)
may issue file/metadata save requests before its composite
`HistoryCommand::Commit`; each remote mutation uses the Store's writer
admission. The default persisted budget is two. C2 gives active Saves private
slots and interleaves short SQLite transactions; C5 checks the expected branch
head when publishing a Commit. See [save ownership](../../../architecture/15-multi-writer-storage.md)
and [writer controls](../../../../../docs/roadmap/0.1/0.1.7/concurrency-controls.md).

```text
Same Store, default writer budget = 2

native import:       [========= one service permit for whole call =========]
Workspace::commit:      [file call]   [metadata call]   [History Commit call]
                       each remote mutation takes/releases its own permit

Each admitted content call owns a private C2 Save and uses short SQLite
transactions. A third simultaneous mutation is refused, not queued.
```

The import holds one service writer permit for its public call and opens its
file Save before scanning, so it occupies a C2 private slot during that stage.
Its own frontier must not serialize another admitted writer or hold a SQLite
write transaction while stalled. A frontier may change how long the import
occupies its permit; it does not guarantee fairness, competing-Commit latency
or aggregate RSS. Two Workspace commits on the same branch retain the C5
expected-head conflict rule independently of this import design.

## Decision and proof needed

The first peak-memory treatment should instead transfer the already-owned
`Vec<PreparedEntry>` into `build_namespace` and drop it immediately after
the existing namespace inputs are assembled, before C1 builds the tree. It
preserves current serials, format, validation and worker order, and removes
the entry list from the interval that currently sets peak RSS. A file-job
queue follows only if a measured remaining peak makes that list material.
Neither step is an end-to-end import memory bound. The
[implementation spec](bounded-import-implementation-spec.md) states the
decision gates for the larger fresh-build design.

Before any product or performance claim, freeze a treatment and verify one
real public import with full reopened path, portable-metadata, size and content
readback; preserve source stability checks, object/root identity where the
Core format requires it, Store ownership and cleanup. Measure per-phase RSS
and separately exercise overlapping import plus Workspace Commit for aggregate
RSS, admission, progress and latency. A changed C1 `run` also needs Workspace
update and same-branch conflict coverage. Follow the
[benchmark rules](../../../../../docs/general/benchmark_rules.md): no warm
cache credit, omitted work outside the timer, repeated arm selection or
unqualified speed claim. The existing source-metadata cache state is
unqualified, so the retained diagnostic timings are not an admission baseline.
