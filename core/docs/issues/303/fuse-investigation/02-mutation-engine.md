# Mutation engine investigation: SQL service, payload units and namespace bursts

> **Status:** Research; informative and not a product contract.
> Reviewed product source: `f96d97651be5299f153ccde2bc8d921dd58807ad`.
> Historical design: `334fc743751b9a181e670d0601a24fb3169208f9`.
> Current proposal: working 303 documents revised by owner on 2026-10-05.
> No source edits, builds, tests, workload runs or benchmarks produced this report.

The shared SQLite owner can remove private pager/file-publication work and
amortize daemon initialization across tool calls. It does not establish a fast
mutation engine by itself. The strongest direction is indexed metadata with
generation-selective capture, bounded payload units with exact validity,
request-local transaction completion, fair service and lifetime-aware incremental
reclamation. Payload layout, failed-capture composition and aggregate service
capacity remain load-bearing prerequisites.

Read with [daemon engine](../daemon-sqlite.md), [FUSE](../fuse.md),
[mutation obligations](../03-mutation-hot-path.md),
[Commit](../workspace-api/commit.md) and [integration](../06-cluster-one-integration.md).
**Implemented** below means inspected source at the product pin; **proposal**
means a mechanism requiring implementation/proof; **diagnostic** means historical
experiment evidence; **inference** means cost/interleaving reasoning rather than
a measured result. None of the proposed SQL overlay paths is implemented by the
excluded reference Workspace code.

## 1. Scope and non-negotiable semantics

[owner requirements]

One daemon initializes one overlay SQLite database before readiness. Workspace
namespaces, payload, operation scratch and reclamation all use that database;
there are no per-Workspace or Commit scratch database files. One tool call may
perform a huge mutation burst and Commit its full affected state. Its prepared
base includes `.git`/index, ignored dependencies, symlinks, caches and output.
Mount must not reconstruct or reinstall that base.

Owner clarification 2026-10-05: both per-tool-call and per-task modes are required,
with per-tool-call expected commonly. A Workspace can serve many sequential or
concurrent calls and repeated incremental Commits without remounting. Commands
can be short or long-lived in either mode. Shared SQL service, active/captured/
orphan ownership and reclamation must remain sustainable during that actual
lifetime; no presumed quick command or imminent unmount is a resource proof.

Ordinary Bash accesses ordinary mounted syscalls: no install/git/build-specific
shortcut or automatic Exec timeout. Keep permissions, write-through cached FUSE
with kernel writeback off, the locally published capture frontier, stable inode serials
and terminal unmount with automatic local cleanup. The host application embeds
current cluster-one runtime libraries/adapters; no legacy server is restored.

There is no total file/change/edit/file-size/Commit-size/runtime/flow cap.
Processing windows and admission budgets remain finite, as do SQLite/platform
offsets, format rules, memory, descriptors and physical storage. Reporting real
resource pressure is required; silently reducing a full workload is not a remedy.

## 2. Source and primary-document anchors

| ID | Source | What is established |
| --- | --- | --- |
| S1 | [FUSE adapter 114–128](https://github.com/Ephemeral-AI-Lab/layerfs/blob/f96d97651be5299f153ccde2bc8d921dd58807ad/core/crates/layerfs-fuse/src/adapter.rs#L114), [WRITE flags 557–572](https://github.com/Ephemeral-AI-Lab/layerfs/blob/f96d97651be5299f153ccde2bc8d921dd58807ad/core/crates/layerfs-fuse/src/adapter.rs#L557) | Reference request window/background configuration; cached WRITE refusal; these are not the proposed overlay implementation |
| S2 | [Creation 73–140](https://github.com/Ephemeral-AI-Lab/layerfs/blob/f96d97651be5299f153ccde2bc8d921dd58807ad/core/crates/layerfs-workspace/src/filesystem/active_create.rs#L73) | Reference checks base child and reserves one inode through an upstream history call per new inode |
| S3 | [Filesystem final bindings 1–34](https://github.com/Ephemeral-AI-Lab/layerfs/blob/f96d97651be5299f153ccde2bc8d921dd58807ad/core/crates/layerfs-content/src/filesystem/input.rs#L1), [row contract 48–83](https://github.com/Ephemeral-AI-Lab/layerfs/blob/f96d97651be5299f153ccde2bc8d921dd58807ad/core/crates/layerfs-content/src/filesystem/rows/source.rs#L48) | Final unique sorted name changes, replayable cursors/keyed agreement; one directory change vector still resident |
| S4 | [Edit refusal 31](https://github.com/Ephemeral-AI-Lab/layerfs/blob/f96d97651be5299f153ccde2bc8d921dd58807ad/core/crates/layerfs-content/src/file/edit/tree.rs#L31), [enforcement 353–363](https://github.com/Ephemeral-AI-Lab/layerfs/blob/f96d97651be5299f153ccde2bc8d921dd58807ad/core/crates/layerfs-content/src/file/edit/tree.rs#L353), [replacement comparison 62–75](https://github.com/Ephemeral-AI-Lab/layerfs/blob/f96d97651be5299f153ccde2bc8d921dd58807ad/core/crates/layerfs-content/src/file/edit/apply.rs#L62) | Deferred nodes can refuse; replacement compare is real work before construction |
| S5 | [Touched budget 86–99](https://github.com/Ephemeral-AI-Lab/layerfs/blob/f96d97651be5299f153ccde2bc8d921dd58807ad/core/crates/layerfs-content/src/filesystem/input.rs#L86), [collection 177–207](https://github.com/Ephemeral-AI-Lab/layerfs/blob/f96d97651be5299f153ccde2bc8d921dd58807ad/core/crates/layerfs-content/src/filesystem/references/reduce.rs#L177), [limit check 652–660](https://github.com/Ephemeral-AI-Lab/layerfs/blob/f96d97651be5299f153ccde2bc8d921dd58807ad/core/crates/layerfs-content/src/filesystem/update.rs#L652) | Total-operation resident touched set and refusal, not merely a batch window |
| S6 | [Pinned rusqlite positional BLOB source, 27–54](https://docs.rs/crate/rusqlite/0.40.2/source/src/blob/pos_io.rs), [BLOB module, 201–225](https://docs.rs/crate/rusqlite/0.40.2/source/src/blob/mod.rs) | Fixed-length positional reads/writes and connection-borrowed BLOB handles; read locally, no dependency modification |
| Q1 | [SQLite transactions](https://www.sqlite.org/lang_transaction.html) | One simultaneous writer; automatic transaction/statement lifetime; error state must be handled |
| Q2 | [SQLite atomic commit](https://www.sqlite.org/atomiccommit.html) | Original-page journaling, dirty pages and retained cache/exclusive-mode mechanisms; durable explanation is not a MEMORY/OFF guarantee |
| Q3 | [SQLite BLOB open](https://www.sqlite.org/c3ref/blob_open.html), [BLOB write](https://www.sqlite.org/c3ref/blob_write.html) | Rowid requirement, expiration, fixed length, write constraints |
| Q4 | [SQLite file format](https://www.sqlite.org/fileformat2.html), [WITHOUT ROWID](https://www.sqlite.org/withoutrowid.html) | Leaf/index/overflow representation; metadata and large payload have different layout costs |
| Q5 | [SQLite PRAGMAs](https://www.sqlite.org/pragma.html) | Pager suggestion, cache spill, MEMORY crash semantics, global page ceiling |
| Q6 | [SQLite savepoints](https://www.sqlite.org/lang_savepoint.html), [WAL](https://www.sqlite.org/wal.html) | Inner RELEASE is not independent completion; WAL still has one writer and reader-dependent checkpoint progress |
| Q7 | [SQLite query planning](https://www.sqlite.org/queryplanner.html) | Indexed seeks, covering indexes and output-sized range work |

Live official documentation was read on 2026-10-05. Use the pinned linked SQLite
build's capabilities when implementing; current website text is a mechanism
reference, not a binary seal or benchmark result.

## 3. What the single writer actually pays

[primary-source facts Q1/Q2/Q5; inference]

MEMORY means the rollback journal is in RAM, not that the main database is an
in-memory database. OFF sync removes a durability barrier, not SQL execution,
page copying, allocation, index maintenance or database writes. The journal can
retain original pages for rollback; the pager may spill dirty pages. `cache_size`
is not a journal, process or guest file-cache limit. MEMORY can leave the database
corrupt after a process crash; disposable restart must discard it.

An acknowledged write must reach an unambiguous transaction outcome. FULL,
IOERR, NOMEM and interrupt do not imply the same automatic rollback state.
Inspect transaction state, explicitly complete rollback where appropriate, and
fail-stop on an unsafe unknown result. No retry after reclaim or error-driven
profile change is justified. The request result is fixed before later maintenance.

```text
FUSE request -> queue owned bytes -> runnable owner job
                                     |
                  prepared seeks + validate current view/authority
                                     |
                    BEGIN -> rows/BLOBs/indexes -> close handles
                                     |
                       COMMIT definite success -> reply
                                     |
                  definite failure -> rollback / preserve result
                  unsafe unknown   -> exact refusal / fail-stop
```

Let `s_i` be measured owner service time of request class i and `lambda_i` its
arrival rate. A necessary stability condition is `sum(lambda_i * s_i) < 1`,
including reads, capture/scratch and maintenance serviced by that owner. This is
a capacity model, not a throughput estimate. Bursts above service rate accumulate
queue wait even when every transaction is individually small.

Use per-Workspace byte/work accounting and starvation-free service among reads,
writes, capture and cleanup. Prepared statements eliminate repeated parsing;
metadata cache hits eliminate selected lookups. Neither changes writer capacity.
Avoid holding a Workspace mutex while waiting for owner service. Deferred inode
or resource waiters must not consume both FUSE dispatch threads. A page budget
is a work bound; device writeback throttling can still extend service wall.

One database does not force one connection. MEMORY/exclusive with one owner is
the current candidate. A normal-locking WAL writer plus short-lived read pool is
an alternative, not adopted: it can overlap reads, retains one writer, and adds
snapshot/cache/WAL/checkpoint/recovery accounting. An exclusive WAL connection
cannot simply be combined with independent concurrent reader connections.
No checkpoint, VACUUM, ANALYZE or PRAGMA optimization belongs to each mutation.

## 4. Payload units: cost comparison and concrete candidates

[proposal and arithmetic; no measured page costs]

Let `W` be request bytes, `C` cell bytes, `P` SQLite page bytes, `K` preexisting
fragments intersected, and `h` index depth. A byte-bounded request may still touch
K tiny extents. Metadata seeks have O(h) index work; every touched row/index/page
and BLOB byte has additional cost.

| Representation | First partial overwrite | Dense scattered edits then full-window overwrite | Append | Main cost/risk |
| --- | --- | --- | --- | --- |
| Exact non-overlapping extents with permanent boundaries, old design | Written bytes only | O(K) row/BLOB operations retained across rewrites | Bounded tail possible | Alternating bytes in 128 KiB produce about 65,536 extent updates plus 65,536 gap insertions |
| Normalized extents with overlap removal | Boundary preservation plus written bytes | Removes fragments but this request still visits/deletes O(K) | Coalescing may help | Set-based DELETE is still output/page-sized work; cannot claim constant critical section |
| Fixed cells plus byte mask | Allocate touched active cells, write valid bytes only | O(ceil((W+C-1)/C)) cells, independent of K | Existing fixed tail incremental writes | First-touch allocation, bitmap traffic and cell-boundary/index amplification |
| Inline small-file/tail plus fixed cells | Bounded inline value/mask replacement | Promotion must bound touched units and copies | Rewrite only bounded tail | Two representations and atomic promotion/capture ownership need proof |

### 4.1 Fixed-cell candidate

Logical offset determines cell; each active cell stores `C` bytes plus `ceil(C/8)`
validity bytes. On first touch after capture allocate new active data/mask with
no base copy-up. Initialize validity false, store only requested bytes as valid.
Invalid bytes inherit captured/base content below the inherited cutoff and read
as zero above it. Written zeros remain valid zeros. Full-cell replacement can
mark all bytes valid, avoiding per-byte mask accumulation, while preserving one
atomic data/validity outcome.

For candidate `C=4096`, a byte mask is 512 bytes: 12.5% of cell payload before
row/index overhead. One fresh one-byte active touch allocates 4,608 bytes of
representation, not one byte. A fully aligned 128 KiB write touches 32 cells,
allocating 147,456 data/mask bytes; a maximally misaligned request touches 33,
allocating 152,064. These are representation arithmetic, not physical allocation
or device traffic. Increasing C reduces per-request BLOB calls but increases
one-byte first-touch allocation; the mask fraction stays 12.5%.

At 4 KiB SQLite pages, a 4,608-byte record cannot fit solely in one page and uses
overflow representation. Smaller inline records can share leaves; masks in a
separate row can share metadata pages but add seeks/atomic update work. Do not
equate cell size with one SQLite page or equate one-byte BLOB modification with
one device-byte write. File-format placement also depends on record headers,
other columns, rowids, leaf fill and splits (Q4).

Captured data and masks remain immutable. New active ownership may share lower
content references, but may not update captured BLOBs. BLOB handles are request-
local, not append-cache entries: UPDATE of any column of their row expires them,
and expiration does not undo preceding BLOB changes. Sequence data/mask SQL and
BLOB operations carefully; close writable handles before COMMIT. Separate data
and mask rows is a concrete option, with both updated in the same transaction.
An indexed payload column cannot be opened writable through incremental BLOB I/O
(Q3). Keep payload bytes out of identity/location indexes.

Cell masks also need a read-cost contract. Enumerating alternating valid/invalid
bytes into one base range call per gap recreates tens of thousands of calls for
one bounded READ. An alternative read plan obtains the bounded mixed base range
once, then overlays local valid bytes and explicit zeros; this reads some base
bytes that will be discarded and adds bounded copying. Fully valid cells need
no base read. Both strategies are read-path candidates, never base copy-up on
WRITE. Freeze logical/base/canonical amplification and grouped demands explicitly;
canonical object acquisition can exceed the requested byte window.

### 4.2 Inline small-file candidate with bounded transitions

A new file with no inherited bytes can hold its complete content inline only up
to a selected representation threshold `T <= C`; no validity mask is necessary
when every byte below its size is supplied, while sparse gaps still require
explicit semantics. A base partial edit still needs exact validity or ranges.
Store arbitrary bytes as BLOB, not TEXT concatenation. Growth replaces a value
no larger than T; this is O(T) copying, not zero copying. Incremental BLOB I/O
cannot resize existing BLOBs (Q3).

Promotion to cells atomically copies at most T existing local bytes into the
declared cells/masks and updates ownership. It does not read the base. Captured
inline state is retained for that operation; active promotion allocates its own
units. Crossing T during a request must include all copies/index/page work in
that request's bound. Demotion is optional background work and cannot trigger a
whole-file rewrite on every shrink/regrow. T is a physical-layout threshold, not
an edit/count/file-size refusal.

For a growing log, completed cells stay fixed and only the current tail grows;
no whole-log reconstruction during ordinary append. A 100-byte append that
dirties two 4 KiB pages would imply 81.92 pager bytes per logical byte before
other work. That is a hypothetical page-count ratio. OS coalescing and actual
SQLite traffic need independent counters; it is not measured device amplification.

## 5. Namespace storms and net-delta accounting

[proposal over implemented final-binding contract S3; inference]

Use lookup-leading `(ns,parent,name,gen)` and `(ns,serial,gen)` primary keys,
with generation-leading capture indexes. Extra indexes charge every insertion,
deletion and split, but prevent tiny captured changes from scanning a huge active
namespace. A covering listing index holding binding/kind can avoid one inode
seek per returned name; retaining duplicate values costs space and mutation work.
Prepared SQL must use namespace prefixes and verified query plans (Q7).

| Operation | Required work / lower bound | Optimization that preserves semantics |
| --- | --- | --- |
| create/mkdir/symlink | Name validation/existence, new inode serial, binding, parent metadata, indexes | Reserved serial range and immutable base miss cache; no reserve RPC for every inode |
| hard link | One new name, target link count and parent metadata; content shared | Stable inode serial; count names, do not duplicate payload |
| rename ordinary file or populated directory | Atomic source/destination bindings and parent metadata; replaced target link/ownership changes | Parent-serial addressing avoids descendant path rewrite |
| rename over directory / rmdir | Emptiness/cycle checks can require tree/name reads | Bounded indexed merge/check state, guard only necessary namespace operations |
| unlink | Name whiteout/delete, target count and lifetime, parent metadata | Created-then-removed name with no lower binding can cancel active name row |
| rm -rf / delete wide tree | At least number of names affected and traversal checks | Process ordinary unlinks; retain correct net state; no giant DELETE before reply |
| enumerate M names | O(log N + M) plus merge/filter work | Keyset pages, no OFFSET skip or per-name resident cookie table |

Renaming one populated directory by serial can avoid touching its descendants;
renaming M independent children remains O(M) requests/effects. Replacing an empty
directory still needs identity/type/cycle checks. Delete-and-recreate needs a new
serial so old handles and children cannot become the new inode. No bulk command
recognition bypasses these semantics.

Net delta reduces persistent change state after ordinary operations; it cannot
skip acknowledged intermediate effects. If a temporary name was created and
deleted in the active generation with no lower binding, remove its active name
change, but keep orphan payload while open and keep immutable captured rows when
capture saw it. For rename A->B->A, final bindings may cancel only when existence,
replaced identities and metadata agree; parent mtimes can still differ. Hard-link
reference effects must be derived from final bindings, not replayed event counts.
Chmod/writeback mtime changes are real metadata even when final file bytes equal
base. Avoid a per-mutation timestamp/event log masquerading as required history.

```text
create temp -> write -> rename over target -> unlink another name
     |             |                  |                 |
  ordinary atomic namespace/data request effects, replies after own COMMIT
                              |
                  latest generation rows = final bindings
                              |
            capture fixed membership + immutable data ownership
                              |
           canonical namespace reducer derives final reference counts
```

The current cluster-one `DirectoryUpdate.changes` is a Vec; caller keyset paging
does not remove that materialization. Separately S5 computes a total touched set:
`ordering_bytes/16` is a total-operation refusal; reducer `touched_serials(_batch)`
collects a full Vec and update checks its length afterward. It is not a bounded
processing window. Backed membership/reducer changes are necessary; detailed
construction validation belongs to the lifecycle investigation. Increasing the
resource limit scales memory rather than solving the no-total-cap requirement.

## 6. Capture, truncate and lifetime transitions

[proposal; incomplete composition algorithm is a blocker]

Capture retains generation rows/stream custody and changes active generation,
without copying every affected row or payload. Its index fixes the generation
and terminal membership, so later active insertions cannot extend captured
iteration. Scratch is operation-keyed metadata in the same database. Constructors
release SQL ownership before CDC/hash/transport and use bounded scratch lookups.

Zero truncate swaps active stream ownership and schedules old unreachable state.
Nonzero truncate atomically installs a visibility cutoff, then reclaims discarded
ranges incrementally. Shrink/regrow must never reveal discarded bytes, including
partially valid boundary cells and inherited content. A later write after cutoff
may recreate valid bytes; stale cleanup must not delete them. Every deletion job
therefore names exact stream/incarnation/range ownership, not merely inode/offset.

```text
active stream S: size/cutoff and valid units
             |
       capture owns S -----------> constructor reads S immutably
             |
       active G+1 owns S2 + exact inherited references
             |
       truncate changes visibility, not captured ownership
             |
       reclaim only when captured/read/orphan/operation owners are gone
```

Open-unlinked content needs independent bounded custody. Repeated captures cannot
pin one generation per Commit indefinitely for a retained log descriptor.
Definite failed Commit must not synchronously replay a large smaller stream
while its inode is busy, and cannot accumulate unbounded failed generations.
A bounded representation and consolidation progress proof are still missing.
Cell layout alone does not solve this. Resolve those lifetimes before describing
capture as both cheap and sustainable under continuous appends/failures.

S4 is another boundary: overlay normalization does not remove cluster-one
deferred-node refusal, replacement comparison work or hole-as-zero construction.
A partial edit can reuse immutable base pieces, but full new/replaced content
pays its bytes, sparse Commit needs hole-aware inputs, and namespace reference
reduction pays affected/released state. No cheap-whole-tree Commit claim follows.

## 7. Batching and coalescing: safe boundary versus changed contract

[research alternatives; current baseline remains one transaction per mutation]

| Technique | Feasibility | Visibility/capture/failure condition |
| --- | --- | --- |
| Several row/BLOB changes inside one FUSE request | Baseline | One atomic request; bound work/pages and reply after transaction success |
| Prepared statements, pager retention, append hints | Baseline-compatible | Cache exact identity/generation; invalidate hints on capture/truncate; no persistent Blob handle |
| Merge ranges/metadata within one pending request | Conditional optimization | Preserve all bytes, size, ordering and metadata; not merge distinct acknowledged operations |
| Execute many requests under outer transaction, reply after inner RELEASE | Invalid | Later outer rollback can lose successful replies; savepoints do not fix it |
| Execute pending requests in bounded group, all replies after outer COMMIT | Research only | Changes current request transaction rule; bounded journal/reply delay and exact order/capture barrier required |
| Delay successful replies until timer/chunk threshold | Not baseline | Adds mutation latency; a sequential logger may never provide another pending write |
| Kernel writeback or acknowledge a userspace buffer before SQL commit | Rejected under current requirements | Changes daemon-acknowledged capture/failure semantics |

Q6 establishes that inner RELEASE does not independently complete an outer
transaction. For a prospective bounded pending group, no observer may get a reply
based on uncommitted state; drain capture after the group's outcome, never between
its constituent effects. On FULL/unknown result all still-pending outcomes need
correct dispositions. Cross-Workspace grouping also enlarges failure/service
coupling. It requires a revised contract and proof, not a silent optimization.

Repeated synchronous writes from one process provide no pending group before
reply. Grouping primarily helps independent concurrent producers and does not
eliminate per-cell/index operations. Prefer baseline-compatible prepared SQL,
physical layout and cache reuse first; do not use speculative batching to claim
the single writer is already fast enough.

## 8. Incremental reclamation and physical pressure

[proposal; primary Q4/Q5; inference]

Bounded row count is insufficient if each BLOB owns a long overflow chain.
Select deletion batches using conservative byte/page/work weights and indexed
cursors, update custody/reclaim state transactionally, commit, then yield fairly.
Retirement and closed-namespace cleanup must have generation/namespace indexes,
not scan newly active rows. Reads retain exact referenced custody before yielding.

```text
terminal unmount / install / last-owner release
                       |
            fence logical target + enqueue owned debt
                       |
        owner: indexed bounded select -> DELETE -> cursor/charge -> COMMIT
                       |
              yield to read/write/capture jobs
                       |
            repeat until target no longer owns rows
```

Pages returned to the freelist can be reused; this does not mean bytes returned
to the host filesystem. Shared database high-water allocation persists under
NONE vacuum. `max_page_count` applies to the database, not individual Workspaces.
Inline/index pages are shared, so sum(payload lengths) is not exact reclaimable
page accounting. Reserve conservative logical/physical headroom and observe
actual freelist/allocation progress. No foreground tiny-write garbage loop.

Captures, orphan data and scratch can be live pressure, not garbage. Backlog
stability requires service capacity greater than incoming reclaim debt over the
declared workload window; a huge truncate/unmount is a burst of debt. Fairness
does not make that burst disappear. Resource admission is explicit before a write,
post-COMMIT cleanup cannot reverse it, and unsafe shared overlay failure is a
daemon-wide boundary. Guest file-cache growth and dirty-page throttling remain
separate from pager/journal limits; no durability or cold-state claim uses hints.

## 9. Priority and proof plan

| Priority | Direction | Evidence / exit proof before performance claim |
| --- | --- | --- |
| P0 | Exact payload/validity/cutoff and generation custody | Binary writes incl. zeros; alternating-byte then full-window overwrite; capture concurrent with boundary-cell truncate/regrow; bounded visited units/pages |
| P0 | Bounded orphan and failed-capture composition | Continuous logger through repeated success/failure/conflict; no full-log merge stall, no increasing read depth/version count |
| P0 | Fair shared owner and deferred FUSE requests | Two same-inode parked requests plus unrelated mutation/read; several Workspaces' independent progress; service/queue bytes and waits attributed |
| P0 | Backed cluster-one total-state structures | Full affected-state Commit with fixed processing windows and no edit/total row refusal; lifecycle report owns complete blocker list |
| P0 | Reclaim custody/reserved physical headroom | Old cleanup versus regrow/capture/read/unmount; real FULL/rollback; shared pressure has explicit dispositions |
| P1 | Inline tiny-file/tail versus cell sizes | Counters of stored/allocated bytes, BLOB copies/calls, journal/dirty pages, index splits; arbitrary binary tail and bounded promotion proof |
| P1 | Prepared/covering SQL and immutable cache hits | Query plans, rows visited and base calls; many tiny files, wide directory churn and net-delta cases |
| P1 | Full per-tool-call burst workflow | Complete prepared root; full dependency replay, build/git/log workloads; separate mount/Exec/Commit/unmount and eventual cleanup debt |
| P2 | WAL reader pool alternative | Snapshot lifetimes, read/write overlap, WAL/checkpoint growth and aggregate caches; remains one writer |
| P2 | Bounded pending group commit research | Revised semantics first; no early acknowledgement, capture barrier, failure isolation, bounded wait/journal; sequential tiny writer remains covered |

Historical full preparation is 130,045 entries / 3,475,776,149 regular-file bytes,
source HEAD `639ed015397290b3745d163aafe02ffee4aa3f84`, manifest
`98fd26440b9087bcfd5c23bec9e5497434a2dcb9a27fc85ad0b823ba9c516658`.
Install replay is 95,021 entries / 2,126,509,110 bytes. The preparation report on
`codex/phase7-experiment-305` at `1451b68a720bbe2175a103dd9b35693ad05e2be1`
is historical evidence, not a new workload scan. #305 B's proof failure and
unrun Stage C prevent treating SQLite/prototype concurrency as qualification.

Start future work with deterministic public-behavior interleavings and labeled
count diagnostics, without changing product paths for a benchmark. Future timing
requires the repo measurement/report rules: exact seals, equal declared cold
contracts with residency enforcement, setup clone reuse, fresh append-only output,
one sample per case/arm, complete-command budgets and retained non-passing cells.
No proposed throughput, zero-amplification, cheap-whole-tree Commit or unlimited
physical-resource claim is made here.
