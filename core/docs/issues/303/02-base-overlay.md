# 02 — Committed base plus live overlay

> **Status:** Proposal; target LayerFS 0.1.7; not a released contract.
> Part of the [cluster two design set](README.md). Written 2026-10-05 against
> `main` `f96d97651`. The schema and algorithms are proposals; statement and
> page figures are design targets and SQLite file-format arithmetic, not
> measurements. Claim labels are defined in the
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

The active engine detail is [Daemon SQLite](daemon-sqlite.md): readiness, typed
Workspace-prefixed rows/indexes, connection candidates, BLOB/validity layouts,
bounded SQL-owner service and shared accounting. This document retains the
base/view contracts and source limitation inventory. Its payload/schema sketches
are candidates, not a frozen replacement algorithm. Public lifecycle is in
[operation contracts](README.md#primary-design-documents) and mounted behavior in [FUSE](fuse.md).

## 1. Model

[proposed design]

```text
                         Workspace operation
                                 |
                      semantics / read planning          layerfs-workspace
                                 |
                +----------------+----------------+
                |                                 |
          committed base                     live overlay
          root R, immutable                  generations A (active) and C (captured)
          canonical objects                  Workspace-keyed rows in overlay.sqlite
                |                                 |
          layerfs-content                    layerfs-overlay
          over the base client               one shared connection, fair owner jobs
                |                                 |
          store host, global SQLite          container-local file
                +----------------+----------------+
                                 |
                  view = A over C over base(R)

  Commit input            =  C over base(R)
  after a known Commit    =  A over base(R'),  where base(R') == C over base(R)
```

**Base identity and binding.** A Workspace is opened on a Branch. The daemon
obtains `HistoryCatalog::branch_snapshot` through the store host and keeps, in
memory, the `BaseBinding`: Branch, head Commit, base Layer, effective root `R`,
allocation scope, profile, and the root directory serial (read from the root
object; it is not a field of `BranchSnapshot`,
`core/crates/layerfs-history/src/records.rs:58-73`). The binding is replaced
only by install. The base needs no lease: cluster one has no deletion path, so
a root stays readable ([source-verified] pack rows are immutable by trigger,
`core/crates/layerfs-persistence/sql/sqlite/objects.sql:9-12`). The day a
collector exists, a lease becomes mandatory; that is recorded in
[06 §7](06-cluster-one-integration.md#10-failure-boundaries).

**Opening the overlay does not copy or enumerate the base.** Daemon startup
initializes the shared database. Workspace open inserts bounded logical state
and binds the already prepared complete root, including `.git`, ignored
dependencies/caches/output and symlinks. Necessary authenticated root metadata
can require I/O; open neither reconstructs nor reinstalls that tree. Namespace
rows describe changes; ownership, operation records and reclamation rows are runtime state.

## 2. Database placement and profile

[proposed candidate; one database does not imply one connection]

One file per daemon: `<overlay_dir>/overlay.sqlite`, in a private directory
(mode 0700) on container disk or a named volume, outside the mount. The disposable
profile creates/initializes it once before readiness; it does not reopen a prior
process's overlay. Schema/PRAGMAs/prepared statements are reused for all Workspaces.
Workspace open inserts small namespace state and binds the immutable base; it
creates no database, payload copy or namespace-wide resident index.

| Setting | Value | Reason |
| --- | --- | --- |
| Connections | One daemon overlay-owner connection; callers enqueue bounded jobs | One shared pager; sequential owner transactions. Unexpected BUSY/LOCKED are defects, not retry permission |
| `locking_mode` | `EXCLUSIVE` | The file lock is taken once; no lock system call per transaction |
| `journal_mode` | `MEMORY` | Rollback works inside the process, so a failed statement or a full disk rolls one transaction back. No journal file, no write-ahead log, **no checkpoint** |
| `synchronous` | `OFF` | Root `AGENTS.md` §4 forbids `fsync` on Workspace backing. This is the only setting under which SQLite issues no sync call |
| `page_size` | 4,096 (starting value; frozen by the count diagnostic of [07](07-implementation-validation.md)) | Smallest rewrite for small rows |
| `cache_size` | One fixed daemon pager allowance, 8 MiB starting candidate | Suggested shared pager allowance; journal/OS cache/other allocations also count (§12) |
| `auto_vacuum` | `NONE` | Freed pages are reused before the file grows. The file is unlinked at daemon teardown |
| `mmap_size` | 0 | Reads stay on the ordinary path |
| `temp_store` | `MEMORY` | Every query is a keyed seek or a keyset page |
| `secure_delete`, `foreign_keys` | `OFF` | The bundled build defaults `foreign_keys` on; set it explicitly |
| `max_page_count` | Daemon overlay allocation ceiling when configured | Global page allocation only; logical Workspace quotas require accounting |
| `application_id`, `user_version` | Set | Schema identity |
| Tables | `STRICT` | — |

Every setting is applied and read back once at daemon database startup; a value that does not read back
is a startup failure, not a substitute.

**Declared profile.** Runtime atomicity of each SQL transaction while the
daemon lives. Nothing across a daemon crash, an OS or VM crash, or power loss.
A Workspace does not survive its daemon. This is the shape of cluster one's own
Disposable profile.

**Alternative if a Workspace must survive a daemon crash (owner question
O-3):** a WAL profile also requires persisted base/capture/allocator/session
recovery context; changing the journal mode alone does not supply it. WAL adds
log writes and checkpoint work, with bounded scheduling/growth required rather
than a checkpoint charged to each mutation. A WAL writer with a bounded reader
pool is a possible alternative, not selected
here. It needs snapshot lifetime/aggregate cache/WAL/checkpoint/recovery proofs;
neither multiple connections nor WAL supplies those bounds by itself. The current
MEMORY/exclusive candidate serves every bounded job through its one owner.

**Linkage.** `rusqlite =0.40.2` with the `bundled` feature, which compiles
SQLite 3.53.2 and adds no new package (`cc` is already locked). It must be
enabled only for the Linux daemon so that the host's linked SQLite, which is
part of every cluster one receipt's binary scope, does not change. Not
established: that it builds for `aarch64-unknown-linux-musl`.

## 3. Schema direction; replacement payload contract required

[proposed design]

Keep generation-keyed inode and name rows, byte-ordered BLOB names, stream
indirection and a reclaim queue. The five-table schema at 334fc7437 is **not** a
complete implementation contract: payload normalization, orphan ownership,
cutoffs, retirement accounting and failure composition require additional
state/index design before the byte/lifetime slices.

Prefix every key with a daemon-local Workspace namespace `ws`. Retain
lookup-leading keys `(ws, ino, gen)` and `(ws, parent, name, gen)`, and add
capture-selective indexes `(ws, gen, ino)` and `(ws, gen, parent, name)`.
Payload cells use `(ws, stream, cell_offset)` and owner/operation records/reclaim tables
also constrain `ws` (operation records additionally identifies the operation). A cursor fixes
its generation/domain at capture, orders by the required inode/name key and
terminates independently of subsequent active writes. Retirement must also use
an explicit generation/key range rather than scan an expanding active tail.
Index update/split work counts in mutation and memory diagnostics.

A candidate payload layout uses bounded cells with a byte-validity mask: written
bytes override lower content, unwritten bytes below the inherited cutoff fall
through, and unwritten bytes above it are zeros. This avoids base copy-up even
for disjoint writes in one cell. Cell size, SQL/BLOB operations and mask handling
must be selected with count diagnostics and the proofs in §5; they are not frozen
here. An alternative extent replacement algorithm is acceptable only with an
explicit bound on prior fragments visited and pages dirtied per request.

Base/capture binding and allocators may remain in memory for the disposable
profile. Stream ownership and orphan/failure representation must enforce the
lifetime rules of [04](04-concurrency-commit.md). A crash-resumable profile needs
persisted recovery context; changing the journal PRAGMA alone does not supply it.

## 4. Generations and visibility

[proposed design; invariants required, replacement lifetimes not implemented]

The live namespace resolves active changes over captured changes over the
committed base. Capture uses the same admission/transaction order as mutations;
no accepted request is split across generations. The captured content and
membership domain must remain stable until its construction is finished.

The prior two-live-versions argument applied only to ordinary namespace keys.
It did not bound open-unlinked pins, invisible garbage, composition depth or
failed captures. [04 §7–§8](04-concurrency-commit.md#7-open-unlinked-files) requires
separate bounded orphan ownership and failure resolution. No version-chain bound
is claimed until those algorithms and their sustained progress proof exist.

Install replaces the base binding only after exact history success and preserves
bytes, names, attributes and inode serials. It must not loop over every open
unlinked inode under the transition mutex. Any preparation uses bounded steps
with a short final transition and a proved lifetime rule.

## 5. Payload: replacement required

[proposed design correction R1]

The old immutable extent boundaries are withdrawn. A 128 KiB request can
intersect 65,536 alternating one-byte extents and 65,536 gaps, causing one BLOB
write per covered part and one insertion per gap. Request byte size therefore
did not imply one new row or a small statement/page count. Repeated full-range
rewrites retained that fragmentation.

### Required WRITE contract

- Preserve exactly the bytes supplied, without base-payload acquisition.
- Bound operations and journal/dirty pages by the request window and declared
  representation parameters, including B-tree depth/splits; no linear walk of
  previous spatial fragmentation inside the window.
- Preserve lower bytes and holes using explicit validity or equivalent metadata.
- Update bytes, size and mtime in one transaction before acknowledgement.
- Do not acknowledge a request belonging to a transaction that may later fail.
- No lifetime edit/extent counter, refusal threshold or whole-file reconstruction
  is introduced to obtain these bounds.

The bounded-cell candidate of §3 touches the cells intersecting the request and
updates their data/validity atomically. Before selection, specify its exact
schema, inherited-range rules, maximum cell/page work and capture/orphan ownership.
A normalized extent alternative needs an equally explicit bounded algorithm;
a set-based SQL statement alone does not bound rows visited or pages dirtied.

### Append and BLOB I/O

Small tail growth may rewrite a bounded tail, never the whole log. The prior
100-byte example claimed two 4 KiB page writes: 8,192 / 100 = 81.92 times the
logical bytes as **proposed pager traffic**, not measured device amplification.
OS writeback may coalesce writes; journal, index and split costs still count.
Flat cost against append index is not a sustained-throughput proof.

`rusqlite 0.40.2` supplies positional fixed-length BLOB writes; it cannot resize
a BLOB through that API. Tail growth needs bounded SQL replacement/binding.
Use BLOB-preserving operations: `data || ?` produces TEXT and is incompatible
with the old STRICT BLOB column. Verify binary tails including NUL and arbitrary
bytes. See [SQLite BLOB writes](https://sqlite.org/c3ref/blob_write.html) and
[STRICT tables](https://sqlite.org/stricttables.html).

### Truncate and sparse files

Zero truncate can swap the stream. Nonzero truncate must atomically change a
logical cutoff; discarded payload is reclaimed afterward. Shrink/regrow must
never expose discarded bytes, including inherited bytes. If active/captured
state shares a stream, cutoff/ownership rules protect capture while cleanup runs.
The old multi-transaction delete-before-reply algorithm is withdrawn: it paused
the inode for O(discarded extents) and allowed cleanup to race capture/retirement.

Sparse writes store data and hole metadata, with no row per zero byte. Current
cluster one would still stream holes as zeros at Commit, O(logical length).
Hole-aware canonical construction/read/edit input is a required integration
change ([06 P4](06-cluster-one-integration.md#9-prerequisites));
a sparse mutation/read proof is not a sparse Commit proof.

### Read and capture contract

A request copies a bounded, consistent plan under the Workspace mutex, retaining
immutable base/stream references before releasing it. Base fetches occur afterward.
Orphan and failed-capture composition have explicitly bounded read depth; they
cannot depend on an unbounded chain of past generations. Byte/page counters must
include validity metadata, any boundary normalization and all copies.

## 6. Names and inodes

[proposed design]

**Lookup `(parent, name)`.** If the overlay holds no `directory_entry` row at all (a
counter in `Workspace.core`), no SQL runs. Otherwise one descending seek on
`(parent, name)`. A row with `ino IS NULL` is a miss. No visible row: ask the
base, unless `parent` was created in the overlay and is not yet committed
(`born > folded`), in which case it has no base children and the answer is a
miss without leaving the daemon.

**Attributes of `ino`.** One descending seek on `(ino)`. No visible row: the
base attributes (§7).

**Enumeration.** A merge of two name-ordered keyset sequences: the overlay's
`directory_entry` rows for the directory and the base listing page
(`FilesystemRead::list_inode(serial, after, max_entries, max_bytes)`). A
whiteout drops a base name. The cursor is the last name returned. Memory is one
page of each sequence; a directory of any size is listed with bounded memory.

**Mutations.** Each accepted ordinary request commits its own transaction.
The table describes logical effects; physical statement/page bounds require
counting, including index work and scheduler admission.

| Operation | Rows written |
| --- | --- |
| create, mkdir, symlink | `directory_entry` insert (with `below` from the existence check the operation already made); `inode` insert with a serial from the in-memory range and `born = active`; parent `inode` upsert for its mtime |
| unlink | If `below = 0` and the row is in the active generation: delete the `directory_entry` row. Otherwise upsert a whiteout. Target `inode` upsert with `nlink - 1`; parent upsert |
| A file that loses its last name and is not open | If `born = active` and it has no captured row: delete the `inode` row. Otherwise keep the row with `nlink = 0`, `stream = NULL`, `lower_len = 0`. In both cases its stream goes to `reclaim` |
| rmdir | As unlink, after the emptiness check below |
| rename | Source: delete or whiteout as unlink. Destination: upsert pointing at the **same serial**. A replaced destination loses a link as in unlink. Both parents upserted. No descendant row is created: a moved directory's children are still found by its serial, in the overlay and in the base |
| link | `directory_entry` insert; target `inode` upsert with `nlink + 1`; parent upsert |
| chmod, utimens | `inode` upsert |

A file created and removed inside one generation leaves no row. A removed and
recreated directory gets a new serial, so it has no base children and needs no
opaque marker.

**Directory emptiness** (rmdir, rename over a directory). A directory born in
the overlay is checked with one seek. A base directory is empty only if every
base name has a whiteout; the check enumerates the merged view and stops at the
first visible child. The directory is protected against child mutations during the check. Busy
requests are parked as deferred replies, without consuming dispatch workers. `rm -rf` of a base directory of N entries costs N
unlinks and one O(N) check, so the check is amortised constant per removed
entry.

**Refused at the mutation, not at Commit.** Cluster one's inode model is
narrower than POSIX. Anything it cannot express is refused when the command
asks, so a Commit never fails later for a reason the command could have been
told:

| Request | Result | Cluster one source |
| --- | --- | --- |
| Name that is not UTF-8, or contains `\` | `EILSEQ` | `core/crates/layerfs-content/src/filesystem/path.rs:178-196` |
| Name over 255 bytes; symlink target over 4,096 bytes | `ENAMETOOLONG` | `filesystem/limits.rs:12`, `:43` |
| `mknod` of a device, FIFO or socket | `EPERM` | `object/inode_leaf.rs:50-58` |
| setuid, setgid or sticky bit on a file | `EPERM` | `filesystem/attributes/portable.rs:28-39` |
| `link` to a directory or a symlink | `EPERM` | `object/inode_leaf.rs:99-112` |
| `chown` | `EPERM`; ownership is the configured command identity | no uid or gid in the inode value |

## 7. Reading the base

[proposed design]

The daemon runs `layerfs-content`'s read code (`FilesystemRead`, `FileView`,
`read_range`) over a **base client** that implements `AuthenticatedObjects`:

```text
read_canonical_batch(ids)
   for each id: BaseCache hit?  -> bytes
   misses -> one ReadObjects(ids) call to the store host
   authenticate every returned object: BLAKE3(domain || bytes) == id
   insert into BaseCache; return in demand order
```

| Need | Calls | Note |
| --- | --- | --- |
| Child by parent serial and name | `FilesystemRead::resolve_child(parent, &name)` | One object per directory-tree level, then the inode-table levels |
| Inode by serial | `resolve_inode(serial)` | Inode leaves hold 50–100 inodes, so neighbours share a fetch |
| Listing page | `list_inode(serial, after, max_entries, max_bytes)` | Keyset; names in byte order |
| Symlink target | `readlink_inode(serial)` | — |
| File bytes | `FileView::open` then `read_range` | A file below 128 KiB is one whole-file object |
| Size, mode, mtime | `Attributes` call to the store host, batched per directory | See below |

**Why an `Attributes` call exists.** A cluster one inode value holds a kind, a
reference count, a content root and a metadata root: no size
(`core/crates/layerfs-content/src/object/inode_leaf.rs:83-93`). The size of a
base file is known only from its file root, which for a file below the cutoff
is the whole file. Without help, a `stat` of every small base file would pull
that file's content across the bridge. The store host answers
`Attributes[(kind, content_root, metadata_root)] -> (length, mode, mtime)`
locally and the daemon caches the answer under those two immutable identities.

## 8. Caches: owner, key, visibility, invalidation

[proposed design]

| Cache | Owner | Key | Holds | Invalidated by |
| --- | --- | --- | --- | --- |
| `BaseCache` | daemon, shared by all Workspaces | `ObjectId` | Authenticated canonical bytes | Nothing, ever. Objects are immutable; eviction is by byte budget |
| Attribute cache | daemon, shared | `(content_root, metadata_root)` | `(length, mode, mtime)` | Nothing; both keys are immutable |
| SQLite page cache | daemon overlay owner | page number | All Workspace overlay/operation records pages | One shared allowance with eviction; no isolation of hot pages between Workspaces |
| Append hint | `Workspace.core`, per recently written inode, bounded | `ino` | The stream's end offset, so a sequential append skips the two overlap queries | Maintained in the mutating critical section; dropped on eviction |
| Kernel dentry, attribute and page caches | kernel, per mount | — | — | [05 §4](05-fuse-assessment.md#4-coherence-a-lifetime-is-not-a-design) |

No cache holds overlay rows outside the SQLite page cache, and no cache is
keyed by root. That is why install needs no cache invalidation and why a new
Workspace on a new head starts warm.

Every cache has a configured byte budget, and every benchmark receipt must
declare its state (root `AGENTS.md` §1–§2): a warm `BaseCache` is setup reuse,
never a cold claim.

## 9. Bounded queries

[proposed design correction R2]

Newest-key lookups retain their lookup-leading index. Listing uses name-ordered
keyset pages and generation visibility. Construction uses generation-selective
indexes/domain membership from §3, not a scan across all keys followed by a
client-side generation filter. Replay fixes the same captured domain each time.

Two versions per key do not imply a factor-of-two scan bound: C may hold one
changed inode while A gains 95,021 disjoint entries. The previous query shapes
could scan active/retired rows and chase continuously increasing active keys.
Record visited and returned rows separately, including every replayed pass.

Cluster one's directory-change Vec and resident new-parent map remain present
in source. They contradict bounded Commit memory and require [06 P6/P7](06-cluster-one-integration.md#9-prerequisites).
Paged overlay queries alone do not remove their materialization.

## 10. Limitation inventory

Each old limit is [source-verified] on the pinned baseline unless marked.
The old replacement/validation proposals are superseded by R1–R8; this table
preserves the source inventory, not evidence that limits have been removed. Paths are under
`core/crates/`; "root" paths are under the repository-root `crates/`.

| Old limit | Source | Owning mechanism | User-visible consequence | Replacement | Remaining real limit | Planned proof (not run) |
| --- | --- | --- | --- | --- | --- | --- |
| `MAX_EDITS_PER_FILE = 4,096` | root `layerfs-workspace-core/src/file_edit.rs:5` before `ead812e78` (removed 2026-09-12) | Per-file pending-edit counter | The 4,097th edit before a Commit failed | No new lifetime counter; R1/R3/R4 (proposed) | — | 10,240 and 100,000 writes to one file: statements and pages per write flat against the write index |
| `MAX_PIECES_PER_FILE = 8,193` | root `layerfs-workspace-core/src/file_edit.rs:5`, `:1008-1009` | Piece table per file | About 4,096 separated edits of one file refused | Bounded payload representation, no count refusal; R1 (proposed) | Disk quota | Same test with scattered offsets |
| Emergent edit cap between 8,192 and 10,240 writes | `layerfs-workspace/src/commit/active_reconcile.rs:34-53`, `:120-132`; threshold from `core/docs/issues/286/FAMILY4-CHECKPOINT-20260930.md:29-30` (document, not re-measured) | Post-publication reconcile charges every dirty extent to an 8 MiB budget | Commit refused **after** it was published upstream | Bounded construction and short install; R3/R4 (proposed) | — | The 10,240-write case commits and installs |
| Checkpoint-like work per mutation | `layerfs-workspace/src/backing/active/pages.rs:447-698`; `layerfs-workspace/src/backing/active/generation.rs:679-844` | Each mutation publishes a full copy-on-write index revision: page files created, written, read back, hashed; reclamation and a compaction plan inline | Latency on every write; never measured | Own SQL transaction, bounded journal/pages; R1/R6 (proposed) | — | Per-operation counters: zero file creates, unlinks, sync calls and maintenance steps on the acknowledgement path |
| WAL `stat` and inline `PASSIVE` checkpoint after every write | #305 prototype, `core/experiment/real-tree/src/overlay/db.rs:97`, `:136-155` on `codex/phase7-experiment-305` | Size check on the mutating thread; a busy checkpoint failed the mutation | A write could fail because a checkpoint was busy | MEMORY/OFF overlay candidate; no WAL checkpoint (proposed) | — | The profile reads back `journal_mode = memory` |
| `EditCheckpoint` per SDK edit | root `layerfs-workspace/src/file_io.rs:214-225`; root `layerfs-workspace/src/lifecycle.rs:886` | Clone of the node and the path map for rollback | Cost grows with the Workspace per edit | Own SQL transaction and defined mounted mutation scope (proposed) | — | Not applicable to the mount path |
| One memory budget for all Workspaces, 8 MiB (16 MiB in the sandbox) | `layerfs-workspace/src/types.rs:5`; `backing/budget.rs:22-32`; `layerfs-sandbox/src/docker.rs:209-212` | Charge ledger for resident structures | `ENOSPC` on write, create or lookup; refused Commit | Per-Workspace plus aggregate memory/resource accounting; R8 (proposed) | Configured memory | Two Workspaces: one fills its quota, the other is unaffected |
| Dirty identities and changed names charged per mutation | `layerfs-workspace/src/runtime/state.rs:299-355` | Resident dirty frontier | `ENOSPC` growing with changed files and names | Rows plus streamed/backed Commit metadata; R3 (proposed) | Disk quota | Install replay of 95,021 entries completes |
| Directory population: 128 entries per page, a charged cookie per listed entry | `types.rs:7`; `filesystem/directory.rs:18-27`; `runtime/state.rs:19` | Per-handle cookie maps | `ENOSPC` while listing large directories | Bounded reply/cursor state; mounted enumeration proof (proposed) | — | 100,000-entry directory listed with bounded memory |
| 128 open handles | `runtime/state.rs:17`; `filesystem/open.rs:97-103` | Fixed handle table | The 129th open fails with `ENOSPC` | Configured descriptors plus bounded orphan ownership; R4/R8 (proposed) | Descriptor limit of the commands | 10,000 simultaneously open files |
| 32 captures, 160 pinned revisions, 32 metadata roots | `backing/active/index.rs:23-24`; `backing/metadata.rs:25` | Fixed arrays | `Busy` or `Capacity` | Bounded capture/composition and independent orphan state; R4 (proposed) | — | — |
| Index depth 7; key 272 bytes; value 512 bytes | `backing/active/keyed.rs:10-14` | Hand-built tree | `Capacity` | SQLite rows/indexes with declared physical resource bounds (proposed) | SQLite database size | — |
| 4 GiB per file | `layerfs-bridge/src/contract/request.rs:16`; `filesystem/write.rs:219-221` | Bridge contract constant | `ENOSPC` past 4 GiB | Remove inherited Workspace cap; sparse Commit prerequisite R3 (proposed) | Disk quota; inherited cap removal already required | A 5 GiB sparse file: write at the end, read back |
| 1 GiB private backing | `layerfs-sandbox/src/docker.rs:207-216` | Launch constant | `ENOSPC` | Configured resources and shared physical headroom; R6/R8 (proposed) | Disk | — |
| A write of 129 bytes allocates 8 KiB | `backing/segments.rs:25-37` | One file per write, 4 KiB header | Quota consumed by small writes | Bounded payload/validity layout; measure actual allocation R1 (proposed) | — | Storage growth reported in bytes for the small-write cases |
| Commit input: prepared stream at most 256 MiB | `layerfs-bridge/src/contract/prepared_stream.rs:58` | Whole stream sized up front | Large Commit refused | Streamed full affected state; R2/R3 (proposed) | One directory's changed names in one `Vec` (cluster one, §11) | 95,021-entry Commit |
| One upstream call at a time per daemon | `layerfs-workspace/src/runtime/host.rs:37-40`; `layerfs-workspace/src/runtime/state.rs:690-698` | Atomic flag | `EBUSY` for a second base read | Demand capacity and fair bounded call scheduling; R5 (proposed) | Configured pool size | Two concurrent base reads |
| Mutation refused while another callback replies | `runtime/coherence.rs:482-493` | Reply permits | `EBUSY` returned to applications | Deferred runnable admission; R5 (proposed) | — | Four Execs writing: zero `EBUSY` |
| Whole node-table scan on every FORGET and close | `runtime/state.rs:429-465` | Resident node table | Cost grows with referenced inodes | Lookup counts with attributable memory; R8 (proposed) | Kernel inode cache size | — |
| One host round trip per created inode | `filesystem/active_create.rs:119-128` | No local allocator | Latency per create | Reserved serial range; fair refill/resource admission R5 (proposed) | Serial space `1 ..= i64::MAX` | Zero upstream calls on the create path, by counter |

The plan's item "128 affected extents" is not a cap: `MAX_AFFECTED` is a scan
page size in a loop that continues (`backing/active/extents.rs:393-422`).


## 11. What grows with what

[proposed design requirements; not achieved bounds]

| Dimension | Required accounting / current limitation |
| --- | --- |
| Request memory | Request buffers, payload/validity work, dirty pages, MEMORY journal and reply copies; cache_size alone is not a transaction or process ceiling |
| Construction | Fixed processing windows plus explicit operation records; deferred nodes, directory changes and new-parent map require cluster-one changes |
| Sessions/queues | Host-enforced byte/count limits before allocation; aggregate every live Save's indexes/caches and blocked messages |
| Versions | Captured domain plus bounded active/failure composition and independent orphan state; repeated pins are forbidden |
| OperationRecord and disk | Overlay, Commit operation records, unreclaimed data, captured/orphan data and shared physical-disk reservation all count |
| Caches | Base/attributes, pager, kernel inode/dentry/FUSE pages, overlay file pages and host caches have separate owners and lifetimes |

Name/kind/portable-metadata constraints, serial space, canonical object limits,
SQLite/platform ceilings and configured resource budgets remain real limits.
`EDIT_DEFERRED_LIMIT` and resident directory/new-parent structures are required
corrections, not accepted permanent size restrictions. The inherited 4 GiB
Workspace file constant is removed by the owner's no-artificial-cap requirement.

## 12. Quota, disk full and errors

[proposed design correction R6/R8]

`max_page_count` limits the shared daemon database's page allocations; it neither
enforces per-Workspace quotas nor reserves shared
physical disk nor predicts every allocation from payload length. Account operation records,
pins/capture, metadata/index splits and conservative admission headroom. Derive
reclaim progress from actual allocation/freelist observations. Inline rows and
indexes share pages; queuing a stream does not reveal exact garbage pages.

No tiny write loops through old garbage under the mutation mutex until it fits.
Use the fair admission/pressure policy of [03 §7](03-mutation-hot-path.md#7-maintenance).
Declare resource waits/refusals; do not claim ENOSPC means only live payload
exceeds quota. Captured/orphan retention, reserves and physical device exhaustion
can also constrain admission. A successful mutation outcome is not changed into
a failed write by an independently failing post-COMMIT maintenance step.

`SQLITE_FULL` is handled through transaction rollback and ENOSPC. IOERR/corruption in the shared database
can fail-stop all daemon Workspaces; logical admission refusal remains local. Unexpected BUSY/LOCKED are defects, with no
busy retry. Base acquisition errors remain one-attempt EIO. Prove rollback and
accepted-write preservation through public behavior, not assumed PRAGMA effects.

Ordinary overlay writes and cached FUSE reads populate guest file cache. A pager
allowance does not bound that residency or the MEMORY journal. Select an explicit
bounded dirty/clean-page policy for backing and mounted content, aggregate all
memory domains, and report phase-local cgroup figures. Hints remain hints and
root no-sync rules remain in force; any needed policy amendment is O-7.
See [SQLite cache_size](https://sqlite.org/pragma.html#pragma_cache_size).

## 13. Isolation, bootstrap and cleanup

[owner decision; proposed implementation]

Workspace ownership is logical, enforced by namespace/incarnation routing and
Workspace-prefixed indexed queries. Metadata, payload, operation records and reclaim state
live in the same daemon database. No per-Workspace connection/file/schema exists.
SQLite writer/pager, physical disk and database failure are shared. Logical
admission can refuse one Workspace without failing another, but database corruption
or unknown unsafe overlay state may require daemon-wide fail-stop.

Open allocates a namespace, inserts bounded state and attaches the base binding;
no base scan or payload acquisition is required for the overlay itself. Mount,
process startup and necessary authorization/base-binding calls have separate costs.
No claim of zero or measured bootstrap latency is made.

Terminal unmount first fences requests/captures and makes the namespace unreachable, then
reclaims its rows in bounded fair background jobs. Do not reuse its namespace key
while stale jobs/rows exist. File allocation can retain its high-water value for
later reuse; terminal Workspace unmount does not unlink the shared database. OperationRecord and
orphan state participate in the same ownership and reclaim accounting.

### Indexed access and complexity

SQLite B-trees support indexed point/range seeks. Required paths are inode lookup
`(ws, ino)`, name lookup `(ws, parent, name)`, generation enumeration `(ws, gen, ...)`
and payload lookup `(ws, stream, offset)`. A point seek is logarithmic in the index
size; a listing/range costs a seek plus returned/visited rows, O(log N + k), not
O(log N) for arbitrarily many results. BLOB bytes, splits, masks and authentication
have additional real cost. General content search/grep still reads searched data.

Use prepared statements and transaction-local consistent reads, bounded keyset
pages, a shared pager, bounded immutable base/attribute caches and bounded append
hints. Keep payload BLOBs out of metadata-only query projections. Confirm query
plans use the intended prefix/index and avoid unbounded scans/temp sorting; cache
misses cost I/O. No eagerly built map/tree proportional to all Workspace files
is required at open. SQLite's B-tree does not remove cluster-one Vec/map/refusal
prerequisites or bound OS cache/journal memory on its own.
