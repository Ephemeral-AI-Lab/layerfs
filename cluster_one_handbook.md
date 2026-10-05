# Cluster One Handbook

> Status: Current general guide.

Written 2026-10-05 against product source `8cbeadef07dc9ac1e79cd59eaee3dca494e2ff87`.
This is an integration guide to the implemented cluster one libraries, not a
release qualification or a new API contract. Source-linked contracts govern.
Benchmark receipts retain their own source identities, stated in section 9.

**Terminology:** cluster one and cluster two are implementation workstreams.
Older source comments use C1 for `layerfs-content`, C2 for `layerfs-storage`,
and C5 for `layerfs-history`. Those component numbers do not mean clusters.

## Reading order

1. Read sections 1–2 for ownership and identities.
2. Use sections 3–4 for the public APIs and construction/publication recipes.
3. Read section 5 before implementing a large-input pipeline.
4. Read sections 6–7 before interpreting read results or acknowledging success.
5. Consult sections 8–9 for configuration, limitations, and measured evidence.

The three highest-priority subjects are **API usage**, **streaming workflow**,
and **completion/failure guarantees**. Cluster two's FUSE implementation,
mutable overlay, generation freezing, and reconciliation are outside this guide.
The caller must nevertheless supply stable inputs and correct expected-state
identities to the cluster one APIs.

## 1. Architecture and ownership

The active Core workspace contains six crates:

| Crate | Owns | Caller uses it for |
| --- | --- | --- |
| `layerfs-content` | Canonical identity, file construction/editing, filesystem trees, logical reads | Turn bytes/edits/namespace rows into canonical objects; read logical data |
| `layerfs-storage` | Reuse, physical encoding, delta selection, packing, bounded Save and Reader | Store canonical objects and acquire authenticated canonical bytes |
| `layerfs-history` | Typed history records and `HistoryCatalog` semantic operations | Inode reservation, genesis, branches, stages, Commits, Layers |
| `layerfs-persistence` | Shared host-local SQLite provider for storage and history | Create/open the Store and execute provider-owned transactions |
| `layerfs-project` | Native namespace Init orchestration | Import a regular directory and publish its genesis LayerStack |
| `layerfs-telemetry` | Explicit timing scopes and reports | Observe the same production operations with timing enabled/disabled |

```text
                      APPLICATION / CALLER
             stable input bytes, namespace rows, authority
                                  |
           +----------------------+----------------------+
           |                                             |
           v                                             v
  +----------------------+                   +----------------------+
  | layerfs-content      |                   | layerfs-history      |
  | canonical construct  |                   | semantic operations  |
  | edit / logical read  |                   | and typed records    |
  +----------+-----------+                   +----------+-----------+
             | FinalizedConsumer                        | HistoryCatalog
             | AuthenticatedObjects                     |
             v                                          |
  +----------------------+                              |
  | layerfs-storage      |                              |
  | Save / Reader        |                              |
  | reuse / encode/pack  |                              |
  +----------+-----------+                              |
             | PackPersistence                          |
             +-------------------+----------------------+
                                 v
                   +----------------------------+
                   | layerfs-persistence        |
                   | storage + history provider |
                   | shared SQLite session      |
                   +-------------+--------------+
                                 v
                          host-local SQLite

  layerfs-project orchestrates content + storage + history for native Init.
  layerfs-telemetry supplies timing; it does not change product semantics.
```

Arrows depict calls/data handoffs, not a complete Cargo dependency graph.
Content construction opens no database or pack; the caller supplies the byte
source and object consumer. History operations do not interpret pack formats.
The persistence adapter owns physical transactions; callers state semantic intent.
Although storage and history share a database session, their semantic operations
have separate transaction boundaries.

The active provider is SQLite. PostgreSQL is an explicitly unavailable selection;
MinIO is not an active alternative backend. Historical plans describing those
backends must not be used as instructions for the current implementation.

Sources: [workspace](core/Cargo.toml),
[content public surface](core/crates/layerfs-content/src/lib.rs),
[storage public surface](core/crates/layerfs-storage/src/lib.rs),
[history contract](core/crates/layerfs-history/src/catalog.rs),
[SQLite implementation round](core/docs/issues/302/SQLITE-IMPLEMENTATION-ROUND.md).

## 2. Data model: know which root you hold

```text
  raw file bytes
       |
       +-- below cutoff --> whole-file object (= file root) --+
       |                                                     |
       +-- chunked --> chunks --> extent tree --> file root --+
                                                             |
                                      +----------------------+
                                      |
                           regular-file inode metadata
                                      |
                directory entries + inode tree + attributes
                                      |
                               FILESYSTEM ROOT
                                      |
                         history Commit or genesis Layer
                                      |
                          Branch / LayerStack references

  Storage independently encodes canonical objects into immutable packs.
  Object IDs describe canonical identity; pack IDs describe physical placement.
```

- `ObjectId`: canonical object identity. Physical encoding/placement can differ
  without changing the canonical identity.
- File root: the result of constructing one file; it is not a namespace root.
- Filesystem root: describes the namespace and inode state through canonical trees.
- Inode identity: allocation scope plus serial; obtain new serials through
  `reserve_inodes`, not a local counter invented independently of the catalog.
- Commit: immutable history record referring to a filesystem root and ancestry/base.
- Layer: a published filesystem state in a LayerStack; Commit publication and
  Layer publication are separate operations.
- Stage token: exact identity of one saved candidate stage. It is not a generic
  permission to commit whatever stage happens to be current.

Carry roots with their semantic type/context. A valid object ID alone does not
prove that it represents the file, filesystem, or authority the caller intends.

## 3. Public API map and handle setup

### 3.1 Create or open the Store

Use `PersistenceConfig::sqlite(path)` and choose profile/layout explicitly before
creation/open. The default is Durable and Monolithic. `Handles::create` refuses
an existing file; use `open_writable` or `open_read_only` for an existing Store.

| API | Arguments / result | Important requirement |
| --- | --- | --- |
| `Handles::create` | `PersistenceConfig`, `StoragePolicy`, `&HistoryCatalogConfig` → `Result<Handles, PersistenceError>` | Fresh path; valid policy and authority config |
| `Handles::open_writable` | config, binding bytes, cursor key `[u8;32]` → handles | Existing supported Store; matching authority and selected profile |
| `Handles::open_read_only` | Same open arguments | Mutations explicitly refused |
| `Storage::new` | `Arc<dyn PackPersistence>` → `StorageResult<Storage>` | Reads and validates persisted policy |
| `Storage::begin_save` | `&self` → `StorageResult<Save<'_>>` | One producer owner for a Save |
| `Storage::reader` | `&self` → `StorageResult<Reader<'_>>` | Operation-owned authenticated read caches |
| `Handles::checkpoint` | `&self` → `Result<Checkpoint, PersistenceError>` | Explicit profile completion/allocation release; may fail |

The application supplies a stable binding key, nonzero valid catalog incarnation,
and nonzero secret cursor key through `HistoryCatalogConfig`. Retain authority
material outside the catalog; do not log secrets or fabricate identities from
process IDs/time. `Handles.storage` is an `Arc<StorageProvider>`;
`Handles.history` implements `HistoryCatalog`.

Composition below is a recipe, not a complete executable: `config`, `policy`,
and `authority` are checked application inputs, and caller error conversion is
omitted.

```rust
let handles = Handles::create(config, policy, &authority)?;
let storage = Storage::new(handles.storage.clone())?;
let construction = storage.policy().construction();
let capacities = construction.capacities();
```

Source: [configuration](core/crates/layerfs-persistence/src/config.rs),
[open/create](core/crates/layerfs-persistence/src/open.rs),
[handles](core/crates/layerfs-persistence/src/handles.rs),
[storage](core/crates/layerfs-storage/src/storage.rs).

### 3.2 Construction, read, and filesystem APIs

| Task | API / inputs | Output and completion meaning |
| --- | --- | --- |
| Fresh file stream | `construct_stream(policy, &capacities, source: impl Read, &mut consumer, scope)` | `ContentResult<ConstructedFile>`; inspect `root` and `logical_len`; objects have been accepted by consumer |
| Fresh file bytes | `construct_bytes(policy, &capacities, raw, &mut consumer, scope)` | Constructed file; byte input already exists in caller memory |
| Advisory predecessor construction | `construct_bytes_with_predecessor` | Predecessor-assisted construction; use its checked predecessor contract |
| Existing file edits | `apply_edits(policy, &capacities, &reader, EditRequest, &mut consumer, scope)` | New file root/result; no-op may reuse the base root |
| File range read | `read_range(&reader, root, range: Range<u64>, &mut sink, scope)` | Writes logical bytes into a `Write` sink; authentication/read errors propagate |
| Repeated file reads | `FileView::open` then view methods | Holds checked base information for reuse within the operation |
| Build namespace | `build_filesystem(&mut objects, &rows, backing)` | `FilesystemResult`; new filesystem tree objects accepted by consumer |
| Update namespace | `update_filesystem(&mut objects, &rows, backing)` | Updated filesystem result; input carries a base |

`FilesystemObjects::new(reader, consumer)` joins an `AuthenticatedObjects`
provider and a `FinalizedConsumer`. `PreparedRows` is the filesystem row contract;
use the public inode/directory/serial row-source interfaces and checked filesystem
inputs. `backing` is `Option<&mut dyn OrderingBacking>` from the sorted
filesystem machinery; ordering scratch is an explicit caller-supplied resource.
It is not a hidden payload spool. Read the row validation/ordering requirements
before implementing a new producer. Build requires `base: None`; update requires
`base: Some(...)`. Directory rows are ordered by parent, inode rows by serial,
and new serial rows are sorted/unique. Directory changes describe final name
bindings, not a chronological rename log. Cursors must be replayable and keyed
lookups must agree with them. `FilesystemInput` is a resident helper; a custom
`PreparedRows` can provide bounded backing. `FilesystemResult.root` is a typed
`FilesystemRootId`; use `.object()` when a history request requires `ObjectId`.

`EditRequest` contains `root`, `edits: &dyn EditSequence`, and
`source: &dyn EditSource`. Supply a stable sequence and stable replacement bytes
for the operation; construction does not capture a mutable source for you.
`EditSequence` declares base/final lengths and replayable ordered edits. Coordinates
refer to the current result as edits progress; an edit cannot reach back into bytes
introduced by an earlier replacement in that sequence. Do not feed an arbitrary
raw FUSE write log directly into this interface. `EditSource::replacement_len`
and `read_at` must agree with each edit's declared replacement length.

The core traits are:

```text
  AuthenticatedObjects  : canonical object acquisition for content operations
  FinalizedConsumer     : accept finalized canonical objects, propagate refusal
  PackPersistence       : physical provider port used by Storage
  HistoryCatalog        : semantic history operations owned by a provider
```

These are Rust library interfaces. The active workspace does not expose a
cluster one HTTP/RPC upload endpoint. A process/container boundary needs explicit
transport adapters for object reads, streamed output, backpressure, and outcomes;
those adapters are integration work, not endpoints callers can invoke today.

Sources: [content exports](core/crates/layerfs-content/src/lib.rs),
[file construction](core/crates/layerfs-content/src/file/content.rs),
[filesystem exports](core/crates/layerfs-content/src/filesystem/mod.rs),
[filesystem construction](core/crates/layerfs-content/src/filesystem/update.rs).

## 4. Construction and publication recipes

### 4.1 Fresh file: source to saved root

```text
  caller opens stable source
            |
            v
  storage.begin_save() ----------------------> Save owner
            |                                      ^
            | save.sink()                          |
            v                                      |
  construct_stream -----------------> FinalizedObject acceptance
    policy from Store                  child objects before parents
    capacities from policy                          |
            |                                bounded waves publish
            v                                      |
  ConstructedFile { root, logical_len }              |
            |                                      |
            +-----------------------> save.finish()
                                             |
                                      WriteOutcome / error
                                             |
                                saved root usable by later work
```

1. Begin Save and obtain its consumer adapter.
2. Call the constructor with Store-derived policy/capacities and a timing scope.
3. On construction error, preserve the content error and inspect
   `save.take_failure()` for an original storage error hidden behind
   `ContentError::OutputRejected`. Abandon the failed operation.
4. On construction success, retain the root, release the temporary sink borrow,
   and call `Save::finish()` once.
5. Return success for storage only after finish succeeds. This does not create a
   Commit or advance a Branch.

Illustrative call sequence, with stable `source` and `scope` supplied by caller:

```rust
let save = storage.begin_save()?;
let built = {
    let mut sink = save.sink();
    construct_stream(construction, &capacities, source, &mut sink, scope)
};
// Handle built's error here; recover save.take_failure() when applicable.
let built = built?;
let root = built.root;
let outcome = save.finish()?;
```

`Save::accept` is synchronous: it can perform preparation/publication work when a
bounded wave fills. `SaveSink` is an adapter, not an unbounded asynchronous queue.
Dropping Save is not equivalent to finish and does not roll back earlier
acknowledged publication waves.

### 4.2 Modify a file and update its namespace entry

```text
  committed file root + stable edits/replacement source
                          |
                   apply_edits
                          |
         unchanged subtrees reused + changed objects emitted
                          |
                   new FILE root
                          |
       changed inode/directory rows with filesystem base
                          |
                update_filesystem
                          |
                 new FILESYSTEM root
                          |
                 finish storage Save
                          |
              optional history publication
```

Use a Reader for committed inputs. When a single Save contains new file objects
that subsequent namespace construction must read before finish, use that Save's
`AuthenticatedObjects` implementation. A committed Reader cannot see every
object still pending inside a Save. Arrange the provider/consumer lifetimes
through `FilesystemObjects`; both can refer to the same Save via its public
provider and sink contracts. For example, after file objects have been accepted:

```rust
let mut sink = save.sink();
let mut objects = FilesystemObjects::new(&save, &mut sink);
let fs = update_filesystem(&mut objects, &rows, backing)?;
let candidate_root = fs.root.object();
drop(objects);
drop(sink);
let outcome = save.finish()?;
// Only now stage candidate_root for a logical Commit.
```

This is a call-sequence illustration: `rows`, `backing`, existing `save`, and error
handling come from the caller. Same-save reads may seal pending work; they are
real storage work, not free lookups.

Edits to a chunked file reuse unaffected extents/subtrees and construct changed
pieces plus boundary structure. This does not promise the exact root a fresh
whole-file reconstruction would choose. Small whole-file results may require
assembling the whole resulting value; representation transitions can require
reading more input. Advisory reuse is not permission to skip validation.

### 4.3 Saved candidate to logical Commit

```text
  coherent BranchSnapshot captured by caller
                    |
  stable file/namespace inputs based on that snapshot
                    |
  construct + Save.finish() succeeds
                    |
             candidate filesystem root
                    |
             stage_changes(request)
                    |
          StageRecord with exact token
                    |
  commit_staged({ workspace, token })
                    |
          +---------+-------------------+
          |                             |
       Committed                    UpToDate
   immutable Commit                existing root/head
   Branch advances                 explicit no-op success
```

History transactions remain short and do not span source reads, construction,
uploads, or Save completion. Populate `StageRequest` from the captured state:

| Field | Meaning |
| --- | --- |
| `workspace`, `branch` | Explicit producer incarnation and target Branch |
| `expected_head`, `expected_base`, `expected_root` | Frozen history expectations |
| `construction_base_root` | Root actually used by construction |
| `intended_commit_base` | Base Layer intended for the resulting Commit |
| `candidate_root` | Successfully saved filesystem root |
| `profile`, `scope` | Filesystem profile and inode allocation scope |
| `generation` | Caller-supplied generation represented by the candidate |

Use the returned stage token in `CommitStagedRequest { workspace, token }`.
Match both successful outcomes: `Committed(CommitRecord)` and
`UpToDate { head, root }`. Conflicts and errors are not no-op success.
Use `discard_stage` only for the exact owned token with a known disposition.
`add_layer` is a separate conditional operation when a workflow explicitly
publishes the Commit as a Layer. Inode reservations are consumed even if unused;
do not recycle serials after an aborted construction.

Generation capture and live overlay reconciliation are caller responsibilities;
this section specifies the cluster one history boundary only.

Source: [history requests/outcomes](core/crates/layerfs-history/src/records.rs),
[semantic catalog](core/crates/layerfs-history/src/catalog.rs).

### 4.4 Native namespace Init

`layerfs_project::init(&storage, &catalog, InitRequest, &timer)` orchestrates:

```text
  directory scan -> file construction + file Save.finish()
         -> reserve scope-wide inode serials
         -> build namespace + namespace Save.finish()
         -> initialize_layerstack(genesis root)
         -> Initialized { stack, root, entries, diagnostics, namespace_work, ... }
```

`InitRequest` supplies `source`, writable `scratch_parent`, explicit stack ID and
name, `scope_seed`, and a fixed `Instant` deadline. The result acknowledges the
genesis LayerStack, not just file payload storage. The deadline is shared with
workers; it is not a rollback boundary. Checkpoint is a separate handle lifecycle
operation, not a substitute for either Save finish.

Source: [Init API](core/crates/layerfs-project/src/init.rs).

## 5. Streaming workflow and large-load behavior

### 5.1 File bytes flow progressively

Default construction uses a 128 KiB small-file cutoff. A bounded prefix determines
whether to use a whole-file representation or a chunked stream. The chunked path
uses content-defined chunks with 8 KiB minimum, 16 KiB target, and 32 KiB maximum.
Files below the cutoff use whole-file representation; files at or above it use
chunked representation. Empty input has its special representation.
Chunk payload objects can reach the consumer before end-of-file; mapping nodes
and the file root complete as construction closes.

```text
   INPUT STREAM       CONSTRUCTION       STORAGE           PERSISTENCE

  read next bytes ---> CDC chunk ------> finalized object
       ^                   |                  |
       |              mapping builder         v
       |                   |            pending batch
       |                   |           <=512 objects
       |                   |           <=4 MiB - 1 B ordinary
       |                   |                  |
       |                   |             reuse/select
       |                   |             encode/pack
       |                   |                  |
       |                   |            sealed packs ----> atomic publication
       |                   |                  |            transaction
       |                   |                  |                 |
       |                   |                  <---- acknowledgement
       |                   |                  |
       +----- synchronous backpressure -------+

  EOF -> complete mapping/file root -> accept root -> Save.finish remainder
```

There is no requirement to retain an entire large file before publishing its
chunks. There is also no whole-import transaction: earlier acknowledged,
reference-closed batches can survive a later failure. A final root is published
logically through history only after its required storage work succeeds.

### 5.2 Native Init worker pipeline

```text
  [scan all directory entries and file jobs]
                     |
              shared FIFO job queue
                     |
       +-------------+-------------+-------------+
       |             |             |             |
    worker 1      worker 2      worker 3      worker 4
    file stream   file stream   file stream   file stream
       |             |             |             |
    local batch   local batch   local batch   local batch
       +-------------+-------------+-------------+
                     |
             bounded sync_channel
                 FOUR batch slots
                     |
            single receiver / Save owner
                     |
            storage.accept(object)
                     |
             SQLite publication waves

  A full channel blocks senders. A slow Save slows construction upstream.
  Done(index, file_root) follows that producer's emitted objects.
```

Ordinary producer batches hold at most 256 KiB canonical bytes, 512 objects,
512 completion events, and 1,024 total events. A canonical object larger than
256 KiB flushes the ordinary batch and travels as a single-object message,
subject to the 16 MiB canonical limit. Thus four queue slots are a message-count
bound, not a universal 1 MiB byte bound. Worker-held batches and a blocked send
also contribute to live memory.

Scheduling is FIFO, without size-aware prioritization. Four long files can occupy
all four workers. The four-worker exception is native Init; do not copy it into
Commit/capture/snapshot paths that require a single construction producer.

### 5.3 Bounds and what they do not imply

| Stage | Current named bound / behavior |
| --- | --- |
| Small-file decision | 128 KiB default prefix/cutoff; policy is persisted |
| Canonical object | At most 16 MiB |
| Pending Save batch | 512 objects; ordinary canonical wave at most 4 MiB minus 1 byte |
| Ordinary pack | 256 KiB maximum assembled body |
| Metadata pooled pack | New pooled packs 64 KiB; accepted v22 pooled format 128 KiB |
| Large singleton pack | Up to canonical limit plus 4,096 bytes framing slack |
| Ordinary publication | 8,191 rows; canonical and physical bytes separately capped at 4 MiB minus 1 byte |
| Large-object publication | Explicit singleton treatment; ordinary-byte cap is not an all-object guarantee |
| Native Init file | At most 4 GiB per regular file |
| Namespace scan | Retained entries/jobs and per-directory child sorting grow with file count |

These are stage/format bounds, not a whole-process memory ceiling. Save can hold
the next incoming object while processing a preceding wave. Multiple workers,
encoding workspaces, caches, namespace metadata, SQLite and OS page cache all
have their own lifetimes. No claim of constant total memory or bounded OS cache
follows from a bounded chunk buffer.

Native Init first discovers the namespace; its payload processing streams after
discovery. It accepts regular files/directories and refuses symlinks/other native
file types. It checks opened-file identity and length/mtime stability, but callers
must still provide stable input: those checks are not a general snapshot service.
Ordering scratch for filesystem construction does not remove the retained scan
collections. A fully file-count-bounded importer remains additional work.

Sources: [worker scan](core/crates/layerfs-project/src/scan.rs),
[batch/channel](core/crates/layerfs-project/src/batch.rs),
[storage bounds](core/crates/layerfs-storage/src/policy.rs),
[construction](core/crates/layerfs-content/src/file/content.rs).

## 6. Localized reads, authentication, and reuse

```text
  read_range(file_root, [offset, end))
               |
         authenticate file root
               |
      +--------+--------------------+
      |                             |
  whole-file                    chunked file
  acquire object                follow covering extent paths
  return slice                  acquire required payload IDs
      |                             |
      +---------------+-------------+
                      v
               storage Reader
           locate -> acquire physical bytes
             -> decode/delta dependencies
             -> authenticate canonical objects
                      |
                   Write sink
```

Ranges must satisfy `start <= end <= logical_len`; there is no implicit EOF
clamp. `read_range` returns `ReadCounters`; use `read_all_bounded` when an explicit
maximum is needed for a full read. A sink can receive some bytes before a later
error, so a failed read must not be treated as a complete output.

A small logical range can cost more physical I/O: the whole small-file object,
a complete encoded group, a chunk, delta bases, and control metadata may be
required. Reader may choose whole-pack acquisition based on density/size/reuse.
Localized means demand-driven object/range work, not one SQLite byte read per
requested user byte.

Whole-pack acquisitions validate the complete pack length/hash. Selected-unit
acquisitions validate relevant controls and returned canonical objects; they do
not claim a hash audit of unread pack bytes. Reader caches are bounded and
operation-owned; eviction costs reacquisition rather than correctness.

Localized writes are expressed through `apply_edits` and filesystem updates.
For example, replacing a middle range of a chunked file preserves the unaffected
sides while constructing the replacement and necessary boundary structure:

```text
  BASE FILE:       [ unchanged left ][ removed range ][ unchanged right ]
                           |                                |
                           |        replacement bytes       |
                           |               |                |
                           |           chunk/construct      |
                           v               v                v
  RESULT:          [ reused extents ][ new extents    ][ reused extents  ]
                           \               |               /
                            +---- rebuilt boundary/tree ---+
                                           |
                                      new file root
```

Published canonical objects and packs are immutable; changed state creates new
objects and reuses unchanged identities, rather than updating old pack BLOBs.
The caller decides how to collect stable edits from its own mutable state.

## 7. Completion and failure contract

```text
  A. CONSTRUCTION COMPLETE     B. STORAGE COMPLETE     C. HISTORY COMPLETE
  file/filesystem root         Save.finish succeeds   Commit transition succeeds
  objects accepted by sink --> required output saved -> Branch refers to candidate

  A does not imply B. B does not imply C.
  checkpoint concerns persistence lifecycle; it does not replace A, B, or C.
```

| Event | Caller interpretation |
| --- | --- |
| Constructor fails | No complete construction result; earlier accepted objects may exist |
| Sink returns `OutputRejected` | Check original typed error through `Save::take_failure`; do not treat as success |
| Save becomes terminal | Further accepts refused; abandon the failed operation |
| Save dropped before finish | Remaining output is not acknowledged as complete; prior publications can remain |
| One publication transaction fails definitely | Its unacknowledged changes abort under provider contract; older successful waves remain |
| Storage/history outcome is uncertain | Preserve exact context and refusal; do not automatically resend/delete on a guess |
| History expected state conflicts | Candidate may be saved without becoming the current Commit |
| `Committed` / `UpToDate` | Explicit history success; interpret the typed result |
| Checkpoint fails | Lifecycle completion failed; report it independently of earlier successful transitions |

The product performs one attempted operation. No automatic busy retry,
refresh/reprepare, alternate backend, or error-driven algorithm fallback is
provided. Match typed error/outcome variants, not error-string heuristics.
A caller implementing later reconciliation must use an explicitly designed
protocol; this handbook does not grant an uncertain-outcome replay procedure.

Provider atomicity is per semantic/publication operation, not per multi-file
import. Namespace Init can leave saved objects or consumed inode reservations
after a later error; it does not acknowledge genesis until history initialization
succeeds. Do not implement guessed cleanup of shared immutable objects.

Source: [Save lifecycle](core/crates/layerfs-storage/src/save/operation.rs),
[Core failure rules](core/AGENTS.md),
[history errors](core/crates/layerfs-history/src/error.rs).

## 8. Persistence profiles, physical layout, and integration limits

| Choice | Current behavior |
| --- | --- |
| Durable | Default; disk-backed SQLite WAL, synchronous FULL, macOS full synchronization |
| Disposable | Disk-backed MEMORY journal / synchronous OFF; runtime atomicity without crash durability |
| Monolithic | Creation layout schema1; complete pack BLOBs |
| GroupRows | Creation layout schema2; independently stored complete encoded units |
| GroupRowsIndexed | Creation layout schema3; group rows with covering mapping index |

Opening supports deterministic schema1/2/3 selection from the Store. Layout is a
creation choice, not an implicit migration request. Profile/authority/policy
compatibility is validated. Unsupported backends/platforms fail explicitly.
The active SQLite provider currently requires macOS; portable content/storage
contracts do not imply this provider is available inside a Linux sandbox.

Durable and Disposable share bounded main-file reservation arithmetic, custody
checks, preallocation primitive, and cleanup logic. Durable retains its main-file
handle and WAL checkpoint/synchronization lifecycle; Disposable uses temporary
allocation handles and has no WAL checkpoint. Custom WAL preallocation was
experimented with and withdrawn. It is not part of the current implementation.

`Handles::checkpoint()` completes the selected lifecycle and releases unused
allocation; Durable checkpoints WAL. WAL commits already obey the declared
synchronization profile. Checkpoint is not the moment that pending Save objects
are accepted, nor the logical Commit API. Do not extend durability claims beyond
the declared profile. Disposable may lose acknowledged data or suffer corruption
on crash/power loss.

Typical database artifact roles:

```text
  application-selected location/
      store.sqlite           combined content + metadata + history database
      store.sqlite-wal       SQLite WAL sidecar while applicable (Durable)
      store.sqlite-shm       SQLite shared-memory sidecar while applicable

  operation-selected scratch parent/
      operation-owned ordering scratch, managed by native Init
```

The base filename is caller-selected. Sidecar existence/size changes with SQLite
lifecycle; this is not a fixed directory template. There is no required external
payload-pack folder in the active SQLite composition.

## 9. Attached benchmark results and qualification boundaries

These tables reproduce retained matched-arm results; no new measurements were
run for this handbook. Product seconds exclude separate proof time. All pairs
use one release/locked sample per arm, pinned identities, and declared cold input
contracts. They are benchmark-family evidence, not daemon/FUSE/network tests.

**Reference caveat:** the pinned original reference `7edddbdb8` uses MEMORY/OFF,
including comparisons against Durable candidates. These pairs do not isolate
Durable overhead against an equally durable reference. Different profile rows
also have independent matched references; their difference is not a controlled
cross-profile overhead measurement.

### 9.1 Namespace Init: latest retained full family

Product source `197d2fb7d`, shared main-file allocation. The current handbook
product's `core/crates` source tree matches that restored product after the WAL
experiment was withdrawn. Receipts still retain `197d2fb7d`; they are not relabeled
as new measurements at `8cbeadef0`.

| Profile | Files | Reference s | Candidate s | Difference | Complete command s | Proof s | Allocated bytes / limit | Joint gate |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | --- | --- |
| Durable | 100 | 0.043420250 | 0.079759708 | +83.6924% | 0.942955666 | 0.518297292 | 5,255,168 / 7,372,800 | FAIL |
| Durable | 1,000 | 0.120408541 | 0.201566000 | +67.4017% | 0.307985209 | 0.031205458 | 20,545,536 / 23,101,440 | FAIL |
| Durable | 10,000 | 1.475158041 | 2.492429625 | +68.9602% | 3.363680625 | 0.331556791 | 305,070,080 / 314,773,504 | FAIL |
| Durable | 100,000 | 6.114136834 | 7.724523333 | +26.3387% | 15.160584292 | 1.134068458 | 514,965,504 / 518,029,312 | FAIL |
| Disposable | 100 | 0.041980292 | 0.038747750 | -7.7001% | 0.066342500 | 0.018800584 | 5,222,400 / 7,372,800 | PASS |
| Disposable | 1,000 | 0.132300250 | 0.129258375 | -2.2992% | 0.230414041 | 0.033392375 | 20,537,344 / 23,101,440 | PASS |
| Disposable | 10,000 | 1.577107625 | 1.645276292 | +4.3224% | 2.532887541 | 0.348281167 | 305,074,176 / 307,265,536 | PASS |
| Disposable | 100,000 | 5.920051500 | 5.558569958 | -6.1061% | 12.225680958 | 1.023206791 | 514,940,928 / 518,029,312 | PASS |

Durable's four failures are the retained **candidate <= 1.10 × reference** speed
ceiling only. Both profiles passed independent proof, final storage, cold input,
cleanup, 30-second complete-command cap, and 19-second separate-proof cap.
Init uses Monolithic layout and four construction workers. Proof checks every
path/kind/directory metadata and deterministic selected full file metadata/content;
it is not an exhaustive reread of every payload byte. Roots match references.

Evidence: [full Init report](core/docs/issues/302/SQLITE-SHARED-ALLOCATION-20261005.md),
[compact receipts](core/docs/issues/302/checks/shared-allocation1/).

### 9.2 History stride 10 → 3 → 1

History rows belong to the earlier optimized product identity, not a history
qualification at the later shared-allocation/current source. Strides correspond
to 17, 53, and 157 history states. Smaller strides mean more states/work; these
are complete history-family workloads, not one Commit latency measurements.

| Profile | Stride | States | Reference s | Candidate s | Difference | Candidate complete s | Candidate proof s | Joint gate |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- |
| Durable | 10 | 17 | 32.630223375 | 32.356111208 | -0.8401% | 44.885041583 | 3.332632167 | PASS |
| Durable | 3 | 53 | 69.063050500 | 69.114259292 | +0.0741% | 81.654476917 | 6.761100583 | PASS |
| Durable | 1 | 157 | 182.112717542 | 180.930537000 | -0.6491% | 194.080112084 | 17.494827625 | PASS |
| Disposable | 10 | 17 | 32.365454042 | 30.989778083 | -4.2504% | 44.295895208 | 3.961798750 | PASS |
| Disposable | 3 | 53 | 66.640188041 | 63.645626250 | -4.4936% | 75.851236375 | 6.318967292 | PASS |
| Disposable | 1 | 157 | 176.928075958 | 169.221686000 | -4.3557% | 180.657216208 | 16.128925042 | PASS |

History uses GroupRowsIndexed/schema3, product freeze `ba6499a61`. Disposable
receipt checkpoints are `ba6499a61` / `3d0e2a752` / `a340aba3b` for strides 10/3/1;
Durable's harness checkpoint is `702f7a38f`, with the same older product freeze.
All six pass source and per-state database cold, independent roots/inventory and
bounded representative-content proof, cleanup, and approved limits. History uses
one construction worker. Durable complete-command caps are 120/340/600 seconds
and separate-proof caps 24/24/60; Disposable's earlier caps are 60/170/300 and
12/12/30. These explicit owner-approved family limits must not be copied as
general benchmark defaults. The relative speed ceiling remains 1.10x.

Exact compact evidence: [Durable stride10](core/docs/issues/302/checks/owner-closure1/durable-history10-stats.json),
[stride3](core/docs/issues/302/checks/owner-closure1/durable-history3-stats.json),
[stride1](core/docs/issues/302/checks/owner-closure1/durable-history1-stats.json);
[Disposable stride10](core/docs/issues/302/checks/locator-clock-final10/comparison.json),
[stride3](core/docs/issues/302/checks/locator-clock-final3/comparison.json),
[stride1](core/docs/issues/302/checks/locator-clock-final1/comparison.json).

Storage gates used approved ceilings, not the stricter original storage targets:

| Stride | Approved ceiling B | Original target B | Disposable allocated B | Durable allocated B |
| --- | ---: | ---: | ---: | ---: |
| 10 | 54,278,964 | 49,344,512 | 50,692,096 | 50,724,864 |
| 3 | 70,427,034 | 64,024,576 | 64,245,760 | 64,278,528 |
| 1 | 92,342,273 | 83,947,520 | 85,172,224 | 85,204,992 |

Both profiles passed the approved storage ceilings and missed all three original
storage targets. Preserve that distinction when reporting PASS.

History benefits from demand-driven reads, reuse, indexed group mappings, and
cursor/cache improvements. Native Init writes a fresh namespace and exercises a
different write-heavy Monolithic path. Durable history competitiveness therefore
does not imply Durable Init has passed its speed gate.

Evidence: [opportunity-loop results](core/docs/issues/302/SQLITE-OPPORTUNITY-LOOP-20261005.md),
[owner closure results](core/docs/issues/302/SQLITE-OWNER-CLOSURE-RESULTS-20261005.md).

### 9.3 Rejected follow-up and remaining qualification

The separate bounded WAL-reservation experiment at `a8e93276f` had a fresh Durable
10,000-file pair: reference 1.544939125 s, candidate 2.667909375 s (+72.6870%).
Relative speed FAIL; proof/storage/cold/cleanup/absolute deadlines PASS. Additional
cause instrumentation found no useful improvement. The treatment was withdrawn
at `8cbeadef0`; the other seven experimental Init cells are NOT_RUN. This failed
experiment must not replace the retained family or be hidden as a successful
optimization.

[Durable write-cause report](core/docs/issues/302/SQLITE-DURABLE-WRITE-CAUSE-20261005.md)
contains the evidence and withdrawal. Issue #302 is closed / NOT_PLANNED; closure
is not a claim that Durable Init satisfies its retained speed ceiling or that
PostgreSQL/MinIO integration is complete.

## 10. How to use and maintain this handbook

Before integrating a caller:

1. Select explicit authority, persistence profile, and creation layout.
2. Use Store-derived construction policy and capacities.
3. Supply stable bytes/edits/namespace rows through public contracts.
4. Preserve bounded consumption/backpressure; do not accumulate an entire upload.
5. Match construction, Save, history, and checkpoint outcomes separately.
6. Keep history transactions outside construction/transport work.
7. Preserve exact tokens and uncertain outcomes; do not invent retry/cleanup rules.
8. Validate the caller through public API integration tests, including failure and
   same-save read cases. Cross-process transport needs its own integration proof.

Maintainers must update this guide when public APIs, boundaries, formats, or named
bounds change, and advance the source pin explicitly. Link new benchmark evidence
with its actual identity/status rather than editing historical receipts. Follow
[root agent rules](AGENTS.md), [Core rules](core/AGENTS.md), and
[documentation policy](docs/general/documentation-policy.md).
