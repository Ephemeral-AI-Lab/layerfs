# 03 — The mutation hot path

> **Status:** Proposal; target LayerFS 0.1.7; not a released contract.
> Part of the [cluster two design set](README.md). Written 2026-10-05 against
> `main` `f96d97651`. "Fast" is a design priority here, not a result: no number
> in this document is a measurement of the design. Statement and page figures
> are design targets to be confirmed by count. Claim labels are defined in the
> [entry point](README.md#claim-labels).

Review revision 2026-10-05: supersedes the algorithms and bounds of design
`334fc743751b9a181e670d0601a24fb3169208f9` where identified below. Product
source remains pinned to `f96d97651`; no implementation or new measurement
accompanies this revision. Required corrections and proof obligations are
tracked in [README](README.md#required-corrections-before-implementation).

Owner update 2026-10-05: one local overlay SQLite database per daemon, initialized
once before readiness; Workspace rows are namespaced within it. Bash Exec has
no automatic runtime timeout. This supersedes the per-Workspace-file proposal;
shared writer/pager/failure accounting and fair admission apply below.

The active engine service/schema candidates are specified in
[Daemon SQLite](daemon-sqlite.md). This document retains the hot-path obligations,
historical source baselines and diagnostic targets: those observations do not
establish shared-writer throughput. One daemon database removes per-Workspace
bootstrap work; it does not remove its serial writer, page/BLOB work or fair
admission requirement. [operation contracts](README.md#primary-design-documents) defines ordinary
Exec and terminal unmount; [FUSE](fuse.md) defines mounted request behavior.

## 1. The path

[owner requirement; revised proposed contract]

A successful FUSE mutation is admitted fairly, validates a consistent view,
submits a bounded job to the shared SQL owner, updates its affected state in one
overlay transaction, then replies after that
transaction commits. No base-payload read, canonical construction, Save, history
publication, checkpoint or sync belongs to ordinary payload mutation.

A transaction is an atomicity boundary, not proof of short service time. The
replacement representation in [02 §5](02-base-overlay.md#5-payload-replacement-required)
must bound spatial-fragment handling, journal/dirty pages and B-tree work. Each
bounded service unit releases the scheduler for other runnable requests.

Do not batch requests and acknowledge them before a shared transaction commits:
a later failure would lose already acknowledged operations. Pending resource
waits are explicit and cancellation-aware, not blocking FUSE dispatch workers.
Base attributes/name checks and serial refill can require upstream calls; they
run outside the overlay critical section, with snapshot validation before write.
No request repeats after a persistence outcome merely because admission changed.

## 2. What disappears

[source-verified for the left column; original removal goals]

R1–R8 supersede physical-operation claims in the replacement column. None is
evidence of an implemented path; pressure/lifetime/scheduling corrections above
are prerequisites.

Paths are under `core/crates/` unless marked.

| On the acknowledgement path today | Source | In the replacement |
| --- | --- | --- |
| A new file per 4 KiB page: create, five `fstat`, `fallocate`, direct write, read back, two SHA-256 passes, compare | `layerfs-workspace/src/backing/active/pages.rs:447-698` | SQLite page writes |
| Unlink of every replaced page file | `layerfs-workspace/src/backing/active/pages.rs:749-824` | Pages return to the freelist |
| `index.maintain` twice, two reclamation passes and a compaction plan per mutation | `layerfs-workspace/src/backing/active/generation.rs:679-844` | Off the path (§7) |
| Host maintenance before every write over 128 bytes | `layerfs-workspace/src/backing/payload.rs:719` | Off the path |
| One `inval_inode` to the kernel per WRITE | `layerfs-workspace/src/filesystem/write.rs:398-406` | None |
| One upstream `ReserveInodes { count: 1 }` per create and mkdir | `layerfs-workspace/src/filesystem/active_create.rs:116-140` | In-memory range |
| An upstream negative lookup before create | `layerfs-workspace/src/filesystem/active_create.rs:73` | Cached base lookup |
| About twelve acquisitions of the Workspace mutex per write, one held across storage I/O | `layerfs-workspace/src/filesystem/active_file.rs:35-241` | One acquisition |
| `EBUSY` when another callback is in flight | `layerfs-workspace/src/runtime/coherence.rs:490-493` | A wait |
| A node-table scan on FORGET and RELEASE | `layerfs-workspace/src/runtime/state.rs:429-465` | A map update |
| A `format!` per callback for a disabled trace; a second copy of every READ | `layerfs-fuse/src/adapter.rs:251-255`; `layerfs-workspace/src/filesystem/read_origin.rs:31-45` | Not built |
| From the #305 prototype: a WAL `stat` after every write and an inline `PASSIVE` checkpoint | `core/experiment/real-tree/src/overlay/db.rs:97`, `:136-155` on `codex/phase7-experiment-305` | No write-ahead log |
| From the #305 prototype: `BEGIN` and `COMMIT` around every read | same file, `:106-113` | Reads are bare statements under the mutex |

## 3. Cost contract; old counts withdrawn

[proposed design; no measured counts]

The two/five-statement WRITE targets and few-page examples at 334fc7437 describe
selected sequential cases only. They do not hold for its immutable extent
boundaries: alternating one-byte writes followed by a 128 KiB overwrite imply
approximately 65,536 BLOB writes and 65,536 gap inserts in one transaction.
The replacement must define operations/pages from its representation, not from
a claim that a byte-bounded request touches few existing fragments.

| Operation | Required mechanism / growth to account |
| --- | --- |
| First partial overwrite | Write data/validity only; base attributes may be fetched outside lock; no base payload |
| Repeated/fragmented overwrite | Bounded cells or normalized range replacement; index depth/splits counted |
| Tiny append | Bounded tail growth; no whole-log reconstruction; one transaction per acknowledged request |
| Create/link/unlink/rename | Atomic namespace effects; count every inode/name/index update and upstream miss/refill |
| Nonzero shrink | Atomic logical cutoff; cleanup outside acknowledgement; no busy interval proportional to discarded data |
| Zero truncate | Stream swap; retire ownership updated safely; debt handled by admission policy |
| Sparse write | Data plus hole semantics; hole-aware Commit prerequisite required |
| Write during Commit | Active state independent of captured data; generation never cached in a handle |

## 4. Append example and sustained cost

The prior 100-byte append trace proposed two 4 KiB pager writes. Arithmetic is
8,192 / 100 = 81.92 times logical bytes, before journaling/index/split costs.
This is illustrative pager traffic, not measured device write amplification:
writeback may merge repeated writes. Flat per-append work does not prove service
rate, latency or residency under four writers plus a tail reader.

A fixed-size incremental BLOB write is supported by pinned rusqlite; growth needs
bounded SQL/binding. The old `data = data || ?` example produces TEXT, incompatible
with a STRICT BLOB column. Select BLOB-preserving tail growth and prove arbitrary
binary input. See [02 §5](02-base-overlay.md#5-payload-replacement-required).

## 5. Repeated edits

No lifetime edit counter, extent-count refusal or whole-file rewrite threshold
is introduced. Latest active bytes overwrite the active representation. Capture
holds stable input; failure resolution and orphan ownership have separate bounded
lifetimes ([04 §7–§8](04-concurrency-commit.md#7-open-unlinked-files)).

Costs to expose include B-tree depth, cells/pages touched, journal retention,
read composition depth and active/captured/garbage storage. Flat counts against
edit index alone do not cover a fragmented overlapping request or shared queues.
Ordinary append of an existing committed log uses a tail edit; new/full replacement
construction pays its content length once at Commit. Cluster-one representation
transitions and replacement compare/construct passes remain real work.

## 6. Exceptional work and scheduling

Base attribute/name misses and serial-range refill may wait on upstream resources
outside the lock. Directory emptiness checks use bounded merged pages and a
mutation guard; requests for that inode are deferred without occupying workers.
Capture/cleanup ownership must be stable while an operation yields.

Neither a Condvar nor a conventional mutex proves that a request waits behind
at most one step. Record queued request count, queue wait, service units,
same-inode wait and resource wait separately. Fair runnable admission must give
other inodes service when two callers target a guarded inode.

## 7. Maintenance

[proposed design correction R6]

MEMORY/OFF has no SQLite checkpoint. Garbage still requires work, including
retired rows, discarded ranges and dead streams. Its lifetime is protected by
capture/orphan/operation ownership, not merely a row generation label.

Use per-Workspace reclaim debt with conservative reserved admission headroom.
Physical page use includes data, indexes, scratch, captured/orphan retention and
allocation needed to complete/reclaim state. Measure actual allocated/free-page
deltas; a stream's byte length does not reveal exact releasable pages because
inline rows and indexes share pages. The exact reservation algorithm is a required
S0 contract, not supplied by `max_page_count` alone.

Maintenance performs page/work-bounded steps and fairly yields between them.
The shared overlay owner fairly interleaves per-Workspace reclaim/mutation/read/
capture/scratch jobs; it never runs a whole Workspace cleanup as one job. It must make progress under the declared sustained write rate;
if incoming discard debt exceeds service capacity, report pressure explicitly.
One truncated file/retired generation can create a large debt burst, so a
high-water threshold by itself is not a fixed garbage bound.

Withdraw the old rules that each mutation reclaims after COMMIT before unlocking
and that a quota-threatening mutation loops until it fits. They charge arbitrary
old debt to an unrelated tiny write. Pressure admission may wait in a bounded,
fair, cancellation-aware queue or return a typed resource result before mutation;
define the policy and delay limits prospectively. Never retry a failed SQL write
after reclaiming. Post-COMMIT maintenance cannot change the accepted write result.

The daemon database's `max_page_count` is global, not a per-Workspace quota, and
does not reserve VM physical disk. Track logical per-Workspace admission and add
aggregate reservation/headroom and scratch accounting so one Workspace cannot
exhaust others' guaranteed allocations. Device failure/exhaustion remains a
shared failure boundary, reported honestly. Freed pages can be reused; NONE
vacuum keeps shared database high-water allocation until daemon teardown.

Individual work units have page/byte bounds, not guaranteed device latency.
Dirty-page throttling and page faults can lengthen a mutex hold; bound residency
and report service time rather than assert a wall ceiling from SQL statement count.
The WAL alternative additionally needs bounded checkpoint service and recovery
context; a passive checkpoint is not assumed always to finish within a page-step
budget. No sync policy is changed by this document.

## 8. Proposed targets

[proposed design requirements; counts/time limits need prospective specifications]

| # | Target / proof shape |
| --- | --- |
| T1 | Each successful ordinary mutation has one committed overlay transaction; no checkpoint, sync, construction or base-payload read. Report all exceptional admission/upstream work separately |
| T2 | Freeze representation-derived SQL/BLOB/page ceilings, including overlap, index splits and validity metadata. Historical 2/5 WRITE counts are not universal ceilings |
| T3 | Count work at 100, 512, 4,097, 10,240 and 100,000 edits; include alternating-byte fragmentation then full-window overwrite and full Commit |
| T4 | Zero contention EBUSY with four Execs and activity throughout Commit; unrelated requests progress when both initial workers encounter a busy inode |
| T5 | Bounded service units plus starvation-free scheduler. Report queue/service/resource/busy wait separately; no unsupported one-step total-wait ceiling |
| T6 | Matched overlay/passthrough full workloads, one sample per arm with equal declared caches; proposed factor 1.25 is unqualified until prospectively frozen |
| T7 | Separate logical open/mount/true/no-change Commit/terminal-unmount wall, plus physical reclaim debt against the 100 ms reporting line, event-resolution timers |
| T8 | Aggregate attributable residency independent of file length, plus actual reclaim debt/allocation/headroom; no lifetime cgroup peak substitution |

Count diagnostics precede speed claims. Full-workload acceptance and measurement
rules are in [07 §5](07-implementation-validation.md#5-validation).



No overlay measurement is admissible until the owner amends the hosting rule
(O-1).
