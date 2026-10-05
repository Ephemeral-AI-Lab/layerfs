# 06 — Cluster one integration contract

> **Status:** Proposal; target LayerFS 0.1.7; not a released contract.
> Part of the [cluster two design set](README.md). Written 2026-10-05 against
> `main` `f96d97651`. Signatures and line numbers in §2 were read at that
> commit. Nothing was compiled or run. The bridge operations in §3 marked "new"
> do not exist. Claim labels are defined in the
> [entry point](README.md#claim-labels).

Read with the two handbooks, which govern cluster one itself:
[`cluster_one_handbook.md`](../../../../cluster_one_handbook.md) and
[`cas_cdc_deltaencoding_handbook.md`](../../../../cas_cdc_deltaencoding_handbook.md).
"Cluster one" and "cluster two" are workstreams. C1, C2 and C5 in older
documents and source comments mean `layerfs-content`, `layerfs-storage` and
`layerfs-history`.

## 1. The contract in brief

[proposed design]

```text
  daemon (Linux)                         bridge                     store host (macOS)
  ------------------------------------   ------------------------   --------------------------------
  layerfs-content: FilesystemRead,       ReadObjects(ids)       --> Reader::read_objects      (base)
     FileView, read_range                                           or Save::read_objects     (same Save)
        over the base client             Attributes(items)      --> FileView::open + portable metadata
  layerfs-content: construct_stream,     GetPolicy              --> Storage::policy
     apply_edits, update_filesystem      SaveBegin              --> Storage::begin_save
        into a remote FinalizedConsumer  SaveAccept(objects)    --> admission check, Save::accept
                                         SaveFinish             --> Save::finish
  Commit orchestration                   StageChanges           --> HistoryCatalog::stage_changes
                                         CommitStaged           --> HistoryCatalog::commit_staged
                                         DiscardStage           --> HistoryCatalog::discard_stage
  Workspace open                         GetBranch              --> HistoryCatalog::branch_snapshot
  create path (background refill)        ReserveInodes          --> HistoryCatalog::reserve_inodes
```

What cluster two guarantees to cluster one:

- one construction worker per Commit;
- objects reach a Save only from a captured, immutable generation;
- construction policy and capacities come from the Store, never from a constant;
- no call is made inside an overlay transaction;
- one attempt per operation: no retry, no resend, no guessed cleanup;
- no file inside a cluster one crate is changed.

## 2. Cluster one APIs cluster two calls

[implemented and source-verified] Paths are under `core/crates/`.

### 2.1 Store host side

| API | Source | What cluster two relies on | Bound |
| --- | --- | --- | --- |
| `Handles::open_writable(config: PersistenceConfig, binding: &[u8], cursor_key: [u8; 32]) -> Result<Handles, PersistenceError>`; `open_read_only(..)` | `layerfs-persistence/src/open.rs:43-56` | One SQLite connection per `Handles`. macOS only (`:16`, `:64`) | — |
| `pub struct Handles { pub storage: Arc<StorageProvider>, pub history: HistoryProvider }` | `layerfs-persistence/src/handles.rs:6-11` | `history` implements `HistoryCatalog` | — |
| `Storage::new(metadata: Arc<dyn PackPersistence>) -> StorageResult<Storage>`; `policy()`; `capacities()` | `layerfs-storage/src/storage.rs:30`, `:46`, `:50` | Reads and validates the persisted policy | — |
| `Storage::begin_save(&self) -> StorageResult<Save<'_>>` | `storage.rs:54` | "Separate handles may write concurrently." One Save per `Storage` value (`save/state.rs:72-75`) | — |
| `Save::accept(&self, object: FinalizedObject) -> StorageResult<()>` | `save/operation.rs:32` | Synchronous. May run a whole publication wave. An error makes the Save terminal | Pending batch 512 objects, 4 MiB − 1 (`policy.rs:42`) |
| `Save::finish(self) -> StorageResult<WriteOutcome>`; `Save::take_failure(&self) -> Option<StorageError>` | `save/operation.rs:53`, `:99` | Storage is complete only after `finish`. Earlier waves survive a later failure or a drop | Publication at most 8,191 rows (`policy.rs:36`) |
| `impl AuthenticatedObjects for Save<'_>` | `save/provider.rs:90-94` | A read through the Save sees its pending objects | — |
| `Storage::reader(&self) -> StorageResult<Reader<'_>>`; `Reader::read_objects(&self, ids: &[ObjectId]) -> StorageResult<Vec<Vec<u8>>>` | `storage.rs:58`; `read/provider.rs:25-29` | Authenticated canonical bytes in demand order | 4,096 ids and 32 MiB per demand (`policy.rs:81-83`) |
| `HistoryCatalog::branch_snapshot(&self, id: BranchId) -> HistoryResult<Option<BranchSnapshot>>` | `layerfs-history/src/catalog.rs:77`; `records.rs:58-73` | Branch record, head root, base root, effective root, scope, profile. **No root serial** | — |
| `reserve_inodes(&self, request: &ReserveRequest) -> HistoryResult<Reservation>` | `catalog.rs:131`; `records.rs:139-147`, `:301-307` | A half-open range `[start, start + count)`; consumed even if unused | Serials end at `i64::MAX` |
| `stage_changes(&self, request: &StageRequest) -> HistoryResult<StageRecord>` | `catalog.rs:119`; `records.rs:241-265` | One stage per Workspace; a fresh token. It does **not** check that the head is still the expected one and does **not** verify that the candidate root was saved | — |
| `commit_staged(&self, request: &CommitStagedRequest) -> HistoryResult<CommitStagedOutcome>` | `catalog.rs:122`; `records.rs:268-274`, `:336-347` | One transaction: verify the token, compare-and-swap the head, delete the stage. `Committed(CommitRecord)` or `UpToDate { head, root }` | — |
| `discard_stage(&self, request: &DiscardRequest) -> HistoryResult<DiscardOutcome>` | `catalog.rs:128`; `records.rs:277-283`, `:368-373` | Exact-token removal: `Removed` or `Absent` | — |
| `stage(&self, workspace: WorkspaceId)`; `commit(&self, id: CommitId)` | `catalog.rs:90`, `:84` | Reads. Their use to settle an unknown outcome needs an owner ruling (O-4) | — |

Failure vocabulary: `HistoryError` (`layerfs-history/src/error.rs:60-116`),
including `Busy`, `HeadMoved`, `StageChanged`, `UnknownOutcome`, and
`WithStage { cause, stage }` where `stage` is `Absent`, `Retained` or
`AcknowledgedUnknown` (`error.rs:49-56`).

### 2.2 Daemon side

| API | Source | What cluster two relies on | Bound |
| --- | --- | --- | --- |
| `trait AuthenticatedObjects { fn read_canonical_batch(&self, ids: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>>; … }` | `layerfs-content/src/object/access.rs:33-36` | The base client implements it. Absence is `MissingObject`; anything else is `ProviderFailure`. The trait has no `Send` or `Sync` bound | One grouped demand per call |
| `trait FinalizedConsumer { fn accept(&mut self, object: FinalizedObject) -> ContentResult<()>; }` | `object/output.rs:191-194` | The Commit's sink implements it by sending to the Save session. An error ends construction once | Synchronous |
| `FilesystemRead::new(reader, root)`; `resolve_child(parent: u64, name: &PathName)`; `resolve_inode(serial)`; `list_inode(serial, after, max_entries, max_bytes) -> ListingPage`; `readlink_inode(serial)`; `read_portable_inode(serial)` | `filesystem/read.rs:82`, `:139`, `:160`, `:196-215`, `:276`, `:265` | Lookup by parent serial and name, listing by serial. This is what lets a renamed base directory keep its children without copying them. `ListingPage { entries: Vec<(PathName, u64)>, continuation: Option<PathName> }` (`filesystem/directory/read.rs:33-38`) | Caller-bounded page |
| `read_range(reader, root: ObjectId, range: Range<u64>, sink: &mut dyn Write, scope) -> ContentResult<ReadCounters>`; `FileView::open(reader, root, scope)` | `file/read.rs:60-66`; `file/view.rs:28` | `start <= end <= logical_len`; no clamp at end of file | — |
| `construct_stream<R: Read>(policy, &capacities, source: R, consumer: &mut dyn FinalizedConsumer, scope) -> ContentResult<ConstructedFile>` | `file/content.rs:279-285` | New or fully replaced files | Prefix up to the cutoff, then streaming |
| `apply_edits(policy, &capacities, reader, request: EditRequest<'_>, consumer, scope) -> ContentResult<ConstructedFile>`; `EditRequest { root, edits: &dyn EditSequence, source: &dyn EditSource }` | `file/edit/apply.rs:28-45` | Edits ordered, non-overlapping, in current-result coordinates. Replacement bytes are read in a compare pass and again in a construct pass (`:62-74`). A no-op returns the base root | Unfinished mapping nodes at most 8 MiB − 1, else refusal (`file/edit/tree.rs:31`, `:358-363`) |
| `trait EditSequence { base_len; final_len; len; edit_at(index) }`; `trait EditSource { replacement_len(index); read_at(index, offset, buffer) }` | `file/edit/input.rs:93-106`, `:115-121` | Index-addressed and replayable | Caller-owned |
| `update_filesystem(objects: &mut FilesystemObjects<'_>, input: &impl PreparedRows, backing: Option<&mut dyn OrderingBacking>) -> ContentResult<FilesystemResult>` | `filesystem/update.rs:94-98` | Directory rows by parent, inode rows by serial, new serials sorted and unique. Reference counts are derived, never trusted | One directory's changes are one resident `Vec` (`filesystem/input.rs:29-34`) |
| `trait RowSource`, `trait PreparedRows` | `filesystem/rows/source.rs:48-83` | Replayable cursors plus keyed lookups `directory_for`, `value_for`, `new_position` | Caller-owned |
| `pub struct FilesystemRootId(pub ObjectId)` | `filesystem/root.rs:37` | The candidate root for `StageRequest` is `result.root.0` | — |

## 3. Bridge operations

[source-verified for "exists"; proposed for "new"]

The transport exists and is independent of cluster one: Noise
`KK_25519_AESGCM_SHA256` over TCP
(`core/crates/layerfs-bridge/src/adapters/native/connection.rs:37`), 16 KiB
frames (`contract/request.rs:14`), one request envelope with a deadline
(`contract/request.rs:60-68`), and typed failure codes including `Unknown`, `Busy`,
`HeadMoved` and `StageChanged` (`contract/outcome.rs:5-28`). None of it is in
the active workspace, and no host endpoint is bound to today's cluster one.

| Operation | Wire shape today | Host handler today | Work needed |
| --- | --- | --- | --- |
| `GetBranch` | `HistoryQuery::GetBranch` (`contract/history.rs:80`) | Bound to the retired `layerfs_history::sqlite` | Rebind to `HistoryProvider`; add the root serial to the reply |
| `ReserveInodes` | `HistoryCommand::ReserveInodes` (`contract/history.rs:268`) | Retired API | Rebind |
| `StageChanges`, `CommitStaged`, `DiscardStage` | `HistoryCommand::*` (`contract/history.rs:237-266`); the failure wire carries the observed stage (`:514-520`) | Retired API | Rebind |
| `GetStage`, `GetCommit` | `HistoryQuery::*` (`contract/history.rs:126`, `:94`) | Retired API | Rebind; used only if O-4 permits |
| **`ReadObjects { ids, save: Option<session> }`** | none | none | **New.** Reply: canonical bytes in demand order. At most 4,096 ids and 32 MiB per call, cluster one's own window |
| **`Attributes { items: [(kind, content_root, metadata_root)] }`** | none; the nearest is `Inspect::InodeAttributes { serial }` (`contract/request.rs:246-248`), one inode per call and keyed by root | none | **New.** Reply: `(logical_len, mode, mtime_seconds, mtime_nanoseconds)` per item |
| **`GetPolicy`** | none | none | **New.** The persisted `ConstructionPolicy` |
| **`SaveBegin` → session**, **`SaveAccept { session, objects }`**, **`SaveFinish { session }`**, **`SaveAbort { session }`** | none | none | **New.** One host `Save` per session. `SaveAccept` is acknowledged after every object in it was accepted, which is the backpressure |
| Workspace control: open, mount, exec, commit, status, unmount, close | `contract/request.rs:107-153` | These are daemon endpoints | Kept; the daemon serves them concurrently |
| `SaveFile`, `PreparedChanges`, `ConstructPortableMetadata`, `ConstructSymlink`, `Inspect::*`, `ReadFile` | exist (`contract/request.rs:71-99`, `:241-259`; `contract/history.rs:164-183`) | Retired API | **Deleted.** They belong to host-side construction and semantic base reads |

A finalized object on the wire is its role code and canonical bytes, plus the
advisory predecessors the save path consumes
(`core/crates/layerfs-content/src/object/output.rs:156-185`). The identity is
**not** sent as authority: the host computes it.

## 4. The store host

[proposed design]

```text
  bridge listener --> one handler per connection --> job queue --> ONE store owner thread
                                                    two classes     owns Handles (writable) and,
                                                    reads first     per Save session, one Storage
```

- **One owner thread.** A second in-process caller of one `Handles` does not
  wait; it gets `Busy` (`state.try_lock()`,
  `core/crates/layerfs-persistence/src/backend/sqlite/transaction.rs:148-150`),
  and `Storage` is not `Sync` (`layerfs-storage/src/storage.rs:19-27`). A single
  owner thread turns every caller's request into a queued job, so no caller
  ever sees `Busy`.
- **Reads before writes.** `ReadObjects` and `Attributes` jobs are taken before
  `SaveAccept` jobs. A FUSE read therefore waits behind at most one job in
  progress: one accepted batch, which can be one publication wave.
- **Save sessions interleave.** Each session owns one `Storage` value over the
  shared provider. The owner thread serves `SaveAccept` batches from different
  sessions in arrival order, so Commits of different Workspaces progress
  together. **Unverified:** that interleaving two Saves this way behaves as the
  comment on `begin_save` says. If it does not, sessions run one at a time; a
  Commit then waits for another Workspace's Commit, and no mount waits for
  either.
- **Parallel readers are an option, not the plan.** Extra `open_read_only`
  handles on other threads can serve base reads while a Save runs, but only
  under the Durable profile; under Disposable a second connection collides with
  the writer. Which profile the integrated Store runs is owner question O-5.
  The single owner thread works under both.
- **Admission check for `SaveAccept`.** For each object the host recomputes the
  identity from the canonical bytes, checks the claimed role against the bytes,
  and re-derives the object's direct references by decoding it. A sender's
  claim of role or references is never trusted. This check does not exist
  today (§6).
- **Quarantine.** After one unknown persistence outcome the session refuses
  every later call (same file, `:153-155`). The host reports that state on
  every request; recovery is an operator reopen, not an automatic one.

## 5. Caller obligations

[source-verified in the handbooks; restated for this design]

1. Construction policy and capacities come from the Store
   (`GetPolicy`). The daemon validates them and never substitutes its own.
2. Inputs are stable for the whole operation. The captured generation is the
   stable input; cluster one "does not capture a mutable source for you".
3. Edits are a normalised final state, never the FUSE write log
   ([04 §4](04-concurrency-commit.md#4-construction-from-the-captured-state)).
4. The consumer applies backpressure. The daemon does not collect finalized
   objects before sending them.
5. On `ContentError::OutputRejected`, the original storage error is recovered
   with `Save::take_failure` on the host and returned in the `SaveAccept`
   failure. The operation is abandoned.
6. Storage complete, history complete and overlay installed are separate
   acknowledgements.
7. `stage_changes` does not prove the candidate was saved. `SaveFinish` must
   have succeeded first.
8. A stage is discarded only by its exact token and only with a known
   disposition. Inode serials are never recycled.
9. Equal logical bytes do not imply equal roots across construction routes. A
   Commit that changes nothing is recognised by `apply_edits` returning the
   base root and by `UpToDate`, not by comparing roots in the caller.

## 6. Prerequisites outside cluster two

Issue #302 is closed, and the integration item the prepared plan called C9 was
never sent. Each row needs an owner and a home (O-12).

| # | Prerequisite | Evidence | Needed for | Without it |
| --- | --- | --- | --- | --- |
| P1 | Admission check: derive role and direct references from canonical bytes on the host | [unresolved] no such check was found; the storage admission code was not read in full | Option C's trust boundary ([01 §6](01-architecture.md#6-trust-boundary)) | Fall back to host-side construction (option A) |
| P2 | `layerfs-content` builds for `aarch64-unknown-linux-musl` | [unresolved] its source has no platform gate; not compiled | Running construction and base reads in the daemon | Option A |
| P3 | `EDIT_DEFERRED_LIMIT` stops refusing | [source-verified] `layerfs-content/src/file/edit/tree.rs:31`, `:358-363` | "No edit-count limit" at Commit: a heavily fragmented edit of one file is refused today | The mount accepts any number of edits, and the Commit of such a file fails with `BoundedCapacityExceeded`. That failure would be reported plainly, not masked |
| P4 | A hole segment in the edit and stream inputs | [source-verified] `Edit` is `start, end, replacement_len` only (`file/edit/input.rs:28-33`) | Committing sparse files at a cost proportional to their data | A hole is committed as zeros through the chunker; cost grows with logical length |
| P5 | A length-only read of a file root | [source-verified] the locator holds `canonical_length` (`layerfs-storage/src/location.rs:7-20`); nothing public exposes it | A cheap `Attributes` | The host opens each file root; for a file below the cutoff that reads the whole file, on the host only |
| P6 | Bounded input for one directory's changes | [source-verified] `DirectoryUpdate.changes` is a `Vec` | A directory with millions of changed names in one Commit | Memory at Commit grows with the largest directory's change count |
| P7 | Declared resources for new directories | [source-verified] new parents are limited to `ordering_bytes / 1024` (`filesystem/update.rs:535-552`) | Commits that create very many directories | Cluster two sizes `FilesystemResources` from its captured row counts; the resident map remains |
| P8 | Bundled SQLite for the Linux daemon only | [source-verified] `rusqlite =0.40.2` is locked without `bundled` | The overlay | No overlay in a static image |
| P9 | The hosting rule | [source-verified] `docs/general/benchmark_rules.md:14-20` forbids Docker-owned SQLite | Any measurement of this design | No admissible measurement (O-1) |
| P10 | Rule text for unknown outcomes | [source-verified] `core/AGENTS.md:156-161` | Resolving `Uncertain` | `Uncertain` stays terminal for Commit (O-4) |
| P11 | Handbook correction | [source-verified] `FilesystemRootId` has no `.object()`; `cluster_one_handbook.md:207`, `:325` would not compile | Readers of the handbook | Use `.0` |

`core/AGENTS.md:156-161` also still says "PostgreSQL/MinIO use their own
default profiles". That sentence is stale; correcting committed rules is the
owner's change to make.

## 7. Failure boundaries

[proposed design]

| Failing part | Mount | Commit in flight | Other Workspaces |
| --- | --- | --- | --- |
| A base read: transport loss, timeout, missing object, identity mismatch | That request gets `EIO`. Overlay data stays readable and writable | Construction fails definitely; fold | Unaffected unless they need the same host |
| The store host is down | Reads of uncached base content fail with `EIO`; cached content and the overlay keep working; creates continue until the serial range is empty | By phase ([04 §6](04-concurrency-commit.md#6-outcomes)) | Same |
| The store host is quarantined | Base reads fail | A definite failure before staging; `Uncertain` from staging on | Same |
| One overlay database fails or fills | That Workspace only ([02 §12](02-base-overlay.md#12-quota-disk-full-and-errors)) | That Workspace only | Unaffected |
| The daemon dies | Every Workspace of that daemon is lost | Lost; a stage row may remain on the host | — |
| An object fails authentication in the daemon | `EIO`; the object is not cached | Construction fails definitely | — |

**No lease today.** A base root stays readable because nothing can be deleted
from the Store. If a collector is ever added, a Workspace must hold a lease on
its base root and on the content roots of its open unlinked files, and this
contract must be extended before the collector ships.

## 8. Platform constraints

[source-verified]

| Constraint | Source |
| --- | --- |
| The persistence provider opens only on macOS: a runtime refusal, plus `F_PREALLOCATE` and `F_TRANSFEREXTENTS` reservation | `layerfs-persistence/src/open.rs:16`, `:64`; `backend/sqlite/connection.rs:187`; `backend/sqlite/allocation.rs:61-70`, `:123` |
| The FUSE mount and the commands are Linux | `layerfs-fuse/Cargo.toml` (`fuser` under `cfg(target_os = "linux")`) |
| aarch64 builds of the bridge carry the ARMv8 AEAD profile or do not compile | `layerfs-bridge/src/adapters/native/connection.rs:19-26`; root `AGENTS.md` §4 |
| The cluster two crates are outside the core workspace and have no lock entries | `core/Cargo.toml:19-28` |
