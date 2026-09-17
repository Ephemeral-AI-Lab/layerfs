# SA review: environment independence, public seams and capacity limits of the C1/C2 core

- Audit target: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs`, branch `main`, HEAD `f288d2af7ecdc7e00f7df153073398d333461aa3`.
- Tree state when read: `git status --porcelain` reported only untracked documentation (`?? docs/roadmap/0.1/0.1.7/evidence/stages-1-5-review-20260917T230700Z/` and four sibling paths under `docs/roadmap/0.1/0.1.7/`); no tracked file was modified.
- Crates in scope: C1 `core/crates/layerfs-content` (15,188 physical lines under `src/`), C2 `core/crates/layerfs-storage` (7,895), `core/crates/layerfs-telemetry` (1,005).
- Method: read-only source inspection plus mechanical extraction of every `pub` item (Appendix A). **No cargo build, test or benchmark was run for this review.** Nothing in this file is a measurement.
- Report vs code: every claim below is attached to a `path:line` and a quoted line. Where a roadmap/handoff document asserts something it is labelled *claims*; reports are not treated as evidence.
- Verdict legend: **READY TODAY** / **ADAPTER WORK REQUIRED** / **CORE BLOCKER** / **UNVERIFIED**.

---

## 0. Verdict summary

| # | Arrangement | Verdict | One-line reason |
|---|---|---|---|
| A | one process/host: app or FUSE adapter -> C1 -> C2 -> local SQLite | **READY TODAY** | C1 opens no database/pack/file (`layerfs-content/src/lib.rs:4-7`); the C1<->C2 bridge exists and is exercised end-to-end (`layerfs-storage/tests/core_pipeline.rs:21-61`) |
| B | Workspace elsewhere, core together: remote Workspace/FUSE -> bounded operation data -> C1+C2+storage service | **ADAPTER WORK REQUIRED** | C1's inputs are borrowed in-process slices with no message-size ceiling (`layerfs-content/src/filesystem/input.rs:129-144`), and the only shipped `OrderingBacking` is local-file (`.../references/backing.rs:169-199`); no core blocker found |
| C | C1 and storage separated: C1 caller <-> bounded output/read batches <-> C2 service + storage | **ADAPTER WORK REQUIRED** (the in-process shim exists; cross-process needs a blocking transport) | both seams are synchronous borrowed trait objects: `StoreProvider<'a>{store: &'a Store}` (`cas/provider.rs:19-21`) and `SaveHandoff<'a>{operation: &'a mut SaveOperation}` (`cas/store.rs:509-512`) |
| C-SQL | C2's SQL backend replaced by *remote* SQL (same C2 code) | **CORE BLOCKER** | C2 opens a *path* only (`sqlite/connection.rs:17-26`), enforces session `PRAGMA` results (`connection.rs:33-43`), introspects `sqlite_master`/STRICT DDL (`sqlite/schema.rs:99-205`) and stores whole pack bodies as SQL `BLOB`s (`sql/schema.sql:34-37`, `sqlite/write.rs:88-97`) |

Cross-cutting facts that drive all four verdicts:

- `layerfs-content/src/lib.rs:16` - `#![forbid(unsafe_code)]`; `layerfs-telemetry/src/lib.rs:13` - `#![forbid(unsafe_code)]`; `layerfs-storage/src/lib.rs:19` - only `#![deny(unsafe_op_in_unsafe_fn)]`, and C2 does contain `unsafe` FFI for zstd (`layerfs-storage/src/encoding/codec.rs:110,118,162,192,266,331,407,445,503,561,600,608`).
- **Zero** `Send`/`Sync` bounds anywhere in the three crates' `src/`: the only matches are `encoding/pool/index.rs:99` ("Synchronizes..."), `cas/owner.rs:585` ("Synchronizes...") and the doc sentence `layerfs-telemetry/src/timer/scope.rs:34` ("`Send` nor `Sync`"). Timing handles are deliberately !Send + !Sync via `thread: PhantomData<*const ()>` (`timer/scope.rs:39`).
- Every public entry point of C1 and C2 takes a `TimingScope<'_>` or is a pure function; see section 1.
- **No `fsync`/`fdatasync`/`sync_all`/`sync_data` call exists in any of the three crates' `src/`**; the only matches are the doc sentences `sqlite/connection.rs:6-7` and `lib.rs:11-12`. Durability is not claimed anywhere.
- Byte order is always explicit, never native, but the two crates chose differently: C1 serializes **big-endian** (e.g. `object/codec.rs:64` `write(writer, &payload_len.to_be_bytes())?;`) while C2 serializes **little-endian** (`pack/assemble.rs:269` `bytes.extend_from_slice(&lane.version().to_le_bytes());`, `encoding/pool/index.rs:255` `i64::from_le_bytes(bytes)`). Grep counts: 113 big-endian hits in C1, 30 little-endian hits in C2, and `to_ne_bytes|from_ne_bytes` is **zero** in both. No cross-crate byte-order contract is stated, and none is needed while C1 and C2 exchange C1-framed canonical object bytes.

---

## 1. Public API surface of the three crates

### 1.1 Crate-root re-exports (exact, current)

**`layerfs-content` (`core/crates/layerfs-content/src/lib.rs`)** - modules and re-exports exactly as written:

```rust
19|pub mod error;
20|pub mod file;
21|pub mod filesystem;
22|pub mod object;
23|pub mod policy;
24|
25|pub use error::{ContentError, ContentResult};
26|pub use file::{
27|    apply_edits, construct_bytes, construct_stream, encode_whole_file_payload, read_all,
28|    read_all_bounded, read_range, whole_file_payload, ConstructedFile, Edit, EditRequest,
29|    EditSource, EditStream, FileContent, FileView, Replacements, MAXIMUM_EDITS_PER_OPERATION,
30|};
31|pub use filesystem::inode::InodeChange;
32|pub use filesystem::{
33|    build_filesystem, update_filesystem, DirectoryRoot, DirectoryUpdate, FilesystemInput,
34|    FilesystemObjects, FilesystemRead, FilesystemResources, FilesystemResult, FilesystemRoot,
35|    InodeIdentity, InodeScope, InodeUpdate, LogicalPath, ObjectWork, PathName, Stat,
36|};
37|pub use object::inode_leaf;
38|pub use object::{
39|    AdvisoryPredecessor, AdvisoryPredecessors, AuthenticatedObjects, DiscardingConsumer,
40|    FinalizedConsumer, FinalizedObject, ObjectId, ObjectRole, PredecessorProvenance, DIGEST_BYTES,
41|    MAXIMUM_ADVISORY_PREDECESSORS, OBJECT_DOMAIN,
42|};
43|pub use policy::{
44|    ConstructionCapacities, ConstructionPolicy, Representation, DEFAULT_CHUNK_DELTA_MAX_DEPTH,
45|    DEFAULT_SMALL_FILE_THRESHOLD_BYTES, DEFAULT_WHOLE_FILE_DELTA_MAX_DEPTH,
46|    MAXIMUM_DELTA_MAX_DEPTH, MAXIMUM_SMALL_FILE_THRESHOLD_BYTES,
47|    MINIMUM_SMALL_FILE_THRESHOLD_BYTES,
48|};
```

Two root re-exports are *modules*, not types: `pub use object::inode_leaf;` (`lib.rs:37`) and every `pub mod` at `lib.rs:19-23`. Unlike `build_filesystem`, the `*_timed` filesystem entry points are not re-exported at the root (they exist at `filesystem/update.rs:79,104` and are reachable as `layerfs_content::filesystem::build_filesystem_timed`).

**`layerfs-storage` (`core/crates/layerfs-storage/src/lib.rs:21-30`)** - exactly:

```rust
21|pub mod cas;
22|pub mod encoding;
23|pub mod error;
24|pub mod pack;
25|pub mod policy;
26|pub mod sqlite;
27|
28|pub use cas::{SaveHandoff, SaveOperation, SaveOutcome, Store, StoreProvider, StoreReadCounters};
29|pub use error::{StorageError, StorageResult};
30|pub use policy::{SchemaIdentity, StorageCapacities, StoragePolicy, SCHEMA_IDENTITY};
```

C2 re-exports only the six `cas` items plus error/policy; `encoding`, `pack` and `sqlite` are public modules reachable by path (`layerfs_storage::pack::layout::PackLane`, `layerfs_storage::sqlite::lookup::ObjectLocation`, ...) but not flattened to the root.

**`layerfs-telemetry` (`lib.rs:16` + `timer/mod.rs:23-25`)** - exactly:

```rust
16|pub mod timer;
23|pub use recording::{MAX_DEPTH, MAX_LABEL_BYTES, MAX_NODES};
24|pub use report::{NodeOutcome, TimingNode, TimingReport};
25|pub use scope::{Active, Pending, Timing, TimingScope};
```

`timer::format` and `timer::json` are private modules (`timer/mod.rs:17-21`), but their renderers are inherent methods on the public `TimingReport`, so `TimingReport::write_text` (`timer/format.rs:14`) and `TimingReport::write_json` (`timer/json.rs:18`) are reachable API; the doc reference at `timer/mod.rs:14-15` is accurate.

### 1.2 Seam types - exact current signatures

```rust
// layerfs-content/src/object/access.rs
29|pub trait AuthenticatedObjects {
31|    fn read_canonical_batch(&self, ids: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>>;
40|    fn read_canonical_batch_scoped(&self, ids: &[ObjectId], scope: TimingScope<'_>) -> ContentResult<Vec<Vec<u8>>>;
50|    fn read_canonical(&self, id: ObjectId) -> ContentResult<Vec<u8>>;
65|    fn read_canonical_scoped(&self, id: ObjectId, scope: TimingScope<'_>) -> ContentResult<Vec<u8>>;

// layerfs-content/src/object/output.rs
163|pub trait FinalizedConsumer {
165|    fn accept(&mut self, object: FinalizedObject) -> ContentResult<()>;

// layerfs-content/src/filesystem/input.rs
129|pub struct FilesystemInput<'a> { ... }
137|    pub directories: &'a [DirectoryUpdate],
139|    pub inodes: &'a [InodeUpdate],
141|    pub new_inodes: &'a [u64],
143|    pub resources: FilesystemResources,

// layerfs-content/src/filesystem/references/backing.rs
34|pub trait OrderingRun {
39|    fn append(&mut self, bytes: &[u8]) -> ContentResult<()>;
41|    fn read_at(&self, offset: u64, buffer: &mut [u8]) -> ContentResult<()>;
43|    fn flush(&mut self) -> ContentResult<()>;
45|    fn len(&self) -> u64;
54|pub trait OrderingBacking {
56|    fn create_run(&mut self) -> ContentResult<Box<dyn OrderingRun>>;
58|    fn held_bytes(&self) -> u64;
60|    fn peak_bytes(&self) -> u64;
62|    fn capacity_bytes(&self) -> Option<u64> { None }
69|    fn cleanup_failed(&self) -> bool { false }
76|    fn release(&mut self) -> ContentResult<()>;

// layerfs-storage/src/cas/store.rs
109|    pub fn create(path: impl AsRef<Path>, policy: StoragePolicy, scope: TimingScope<'_>) -> StorageResult<Self>
132|    pub fn open(path: impl AsRef<Path>, scope: TimingScope<'_>) -> StorageResult<Self>
185|    pub fn begin_save(&self, scope: TimingScope<'_>) -> StorageResult<SaveOperation>
202|    pub fn read_batch(&self, ids: &[ObjectId], scope: TimingScope<'_>) -> StorageResult<(Vec<Vec<u8>>, StoreReadCounters)>
236|    pub fn contains(&self, ids: &[ObjectId], scope: TimingScope<'_>) -> StorageResult<Vec<ObjectId>>
291|    pub fn accept(&mut self, object: FinalizedObject) -> StorageResult<()>
314|    pub fn finish(mut self, scope: TimingScope<'_>) -> StorageResult<SaveOutcome>
341|    pub fn read_batch(&mut self, ids: &[ObjectId], scope: TimingScope<'_>) -> StorageResult<Vec<Vec<u8>>>
469|    pub fn abort(mut self, scope: TimingScope<'_>) -> StorageResult<()>
516|    pub fn new(operation: &'a mut SaveOperation) -> Self

// layerfs-telemetry/src/timer/scope.rs
20|pub struct Timing;
36|pub struct TimingScope<'a, S = Pending> { ... }
140|    pub fn run<T, E, F>(self, operation: F) -> Result<T, E>
172|    pub fn record<T, E, F>(name: impl Into<Cow<'static, str>>, operation: F) -> (Result<T, E>, TimingReport)
194|    pub fn disabled<T, E, F>(name: impl Into<Cow<'static, str>>, operation: F) -> (Result<T, E>, TimingReport)
```

C1's filesystem operation entry points (`layerfs-content/src/filesystem/update.rs`):

```rust
67|pub fn build_filesystem(objects: &mut FilesystemObjects<'_>, input: &FilesystemInput<'_>, backing: Option<&mut dyn OrderingBacking>) -> ContentResult<FilesystemResult>
79|pub fn build_filesystem_timed(objects: &mut FilesystemObjects<'_>, input: &FilesystemInput<'_>, backing: Option<&mut dyn OrderingBacking>, phases: &FilesystemPhases<'_>) -> ContentResult<FilesystemResult>
92|pub fn update_filesystem(objects: &mut FilesystemObjects<'_>, input: &FilesystemInput<'_>, backing: Option<&mut dyn OrderingBacking>) -> ContentResult<FilesystemResult>
104|pub fn update_filesystem_timed(objects: &mut FilesystemObjects<'_>, input: &FilesystemInput<'_>, backing: Option<&mut dyn OrderingBacking>, phases: &FilesystemPhases<'_>) -> ContentResult<FilesystemResult>
```

### 1.3 Complete per-item inventory

Appendix A lists every `pub` item in every `.rs` file under the three crates' `src/` trees - **1,032 declarations across 115 files** (`layerfs-content` 652, `layerfs-storage` 338, `layerfs-telemetry` 42, by the extractor's count) - with file, line and declaration text. Each file is labelled `[public module path]` (reachable from `lib.rs` through `pub mod`, so its `pub` items are nameable by path; 80 of the 115 files) or `[private module]` (35 files whose items join the API only through a re-export or an inherent `impl` on a public type - e.g. every `TimingScope`/`TimingReport` method, which lives in the private modules `timer/scope.rs` and `timer/report.rs` and is still public API).

---

## 2. The seven seams

Notation: *caller* = who invokes it in-tree; *bytes* = ownership/borrow and when it is released.

### 2.1 AuthenticatedObjects (C1 read provider)

| Aspect | Evidence |
|---|---|
| Owner/definition | `layerfs-content/src/object/access.rs:29` `pub trait AuthenticatedObjects {` |
| Callers | `file/read.rs:29`, `:70`; `filesystem/objects.rs:72,79,96`; `filesystem/read.rs:86,193`; `filesystem/inode/read.rs:51,108`; `filesystem/directory/read.rs:126,208,300`; `filesystem/attributes/read.rs:61`, `attributes/value.rs:65`, `attributes/patch.rs:199`, `filesystem/validate.rs:80` |
| Actual API | `read_canonical_batch(&self, ids: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>>` (:31) plus three defaulted methods (:40, :50, :65) |
| Borrowed/owned bytes | `&self` + borrowed `&[ObjectId]`; each answer is an **owned** `Vec<u8>` per demand. C1 keeps no cache: `filesystem/objects.rs:71-75` returns it to the caller and `:89-109` returns the whole `Vec<Vec<u8>>`. Release: ordinary `Drop` of the returned vectors. |
| Ordering precondition | `ids[index]` must map to the same index in the answer; cardinality must be exact (`access.rs:25-27`, enforced at `objects.rs:97-102` with `ContentError::BatchCardinality`) |
| Authentication precondition | Bytes must already be proof-carrying for the requested identity; a provider that cannot establish identity must return `ContentError::IdentityMismatch` (`access.rs:26-28`, `error.rs:47`). **C1 does not re-verify**: the only `ObjectId::for_bytes` calls in C1 are on locally-built output (`object/output.rs:101`, `filesystem/sorted/page.rs:349`, `filesystem/root.rs:25,32`), never on provider answers. |
| Batch count limit | The trait declares none; the two in-tree bounds are `MAXIMUM_READ_DEMANDS = 4_096` (`content/filesystem/objects.rs:20`, enforced `objects.rs:90-95`) and C2's `READ_OBJECT_LIMIT = 4_096` (`storage/policy.rs:80`, enforced `storage/cas/store.rs:207,246,346`) |
| Batch byte limit | **None declared.** A wave is bounded by count x per-object ceiling only: 16 MiB `MAX_CANONICAL_OBJECT_BYTES` (`content/policy.rs:203`), 16 MiB `CANONICAL_LIMIT` (`storage/policy.rs:61`), SQL `CHECK (canonical_length <= 16777216)` (`storage/sql/schema.sql:56`). There is no per-wave byte check in `content/filesystem/objects.rs:89-109` nor `storage/cas/read.rs:42-101`; per-object *chain* budgets (`CHAIN_CANONICAL_LIMIT = 512 * 1024`, `storage/policy.rs:102`; `CHAIN_ENCODED_LIMIT = 256 * 1024`, :104) bound reconstruction work, not the wave total. |
| Backpressure | None: the call is synchronous and either answers or fails. No queue, no retry, no partial answer. |
| Errors/cancellation | `ContentError` (`error.rs:11-133`); a failure ends the caller's operation. No cancellation token: cancellation is the caller dropping the call. |
| Process-local assumptions | None in the trait; no `Send`/`Sync` bound, `&self` only. The in-tree implementation `StoreProvider` holds `&'a Store` (`storage/cas/provider.rs:19-21`). |
| What a future adapter must supply | canonical bytes per `ObjectId` **in demand order**, authenticated against that identity, owned buffers, synchronously, with an explicit refusal (never replacement bytes) when identity cannot be established. |

### 2.2 FinalizedObject / FinalizedConsumer (C1 output)

| Aspect | Evidence |
|---|---|
| Owner/definition | `content/object/output.rs:88` (type), :163 (trait) |
| Producer/caller | every construction path calls `FilesystemObjects::emit` (`filesystem/objects.rs:112-119`), which calls `self.consumer.accept(object)?` (:115); C2 implements the trait at `storage/cas/store.rs:539-548` |
| Actual API | `fn accept(&mut self, object: FinalizedObject) -> ContentResult<()>` (:165); `FinalizedObject::into_parts(self) -> (ObjectId, ObjectRole, Vec<u8>, Vec<ObjectId>)` (:154) |
| Borrowed/owned bytes | `accept` takes the object **by value**: the `Vec<u8>` inside `FinalizedObject` moves to the consumer (`output.rs:91`, `lib.rs:5`: "Ownership of the allocation moves to the consumer; C1 keeps no payload copy"). Release: consuming or dropping the object. `DiscardingConsumer` (:174-213) is the non-persisting sink. |
| Ordering precondition | Emission order is construction order; the C2 consumer is order-tolerant (`cas/save.rs:33-41` handles repeated identities across waves) |
| Authentication precondition | Identity is computed exactly once, at construction: `output.rs:101` `let id = ObjectId::for_bytes(&canonical);` after `codec::decode_bytes_object(&canonical)?` (:100) |
| Batch limits | C1: none per `accept`. C2 `PendingBatch`: `BATCH_OBJECT_LIMIT = 512` and `BATCH_CANONICAL_BYTES_LIMIT = 512 * 1024` (`storage/policy.rs:65,67`), enforced in `cas/batch.rs:48-69`; a single object larger than the byte bound is still admitted when the batch is empty (`batch.rs:50-58`) |
| Backpressure | `SaveHandoff::accept` converts a storage refusal into `ContentError::OutputRejected` and **retains** the original error (`store.rs:540-547`); C1 ends construction on the first error (`output.rs:160-162`) |
| Errors/cancellation | `ContentError::OutputRejected` (`error.rs:84`); the caller recovers the true failure with `take_failure()` (`store.rs:529-531`). No retry, resend or alternate path. |
| Process-local assumptions | none in the trait |
| What a future adapter must supply | a sink that accepts already-finalized objects, in order, one at a time, **synchronously**, and reports refusal by error rather than silent acceptance |

### 2.3 FilesystemInput / FilesystemResources

| Aspect | Evidence |
|---|---|
| Owner/definition | `content/filesystem/input.rs:129` / :56 |
| Caller | the application/FUSE adapter; consumed by `build_filesystem`/`update_filesystem` (`update.rs:67,92`) |
| Actual API | `FilesystemInput<'a>` is all-public fields: `base: Option<FilesystemRootId>` (:131), `scope: InodeScope` (:133), `root_serial: u64` (:135), `directories: &'a [DirectoryUpdate]` (:137), `inodes: &'a [InodeUpdate]` (:139), `new_inodes: &'a [u64]` (:141), `resources: FilesystemResources` (:143). Validation: `FilesystemInput::check` (:157-215) |
| Borrowed/owned bytes | **Entirely borrowed** for the duration of one operation; C1 copies nothing from the slices and returns only `FilesystemResult` (`update.rs:57-64`). Release: the caller's slices drop after the call. |
| Ordering precondition | `directories` strictly increasing by parent (`input.rs:172-178`, `NonCanonicalOrdering`); each update's `changes` strictly sorted/unique by name (`input.rs:38-40`); `inodes` strictly increasing by serial (`input.rs:179-185`); `new_inodes` strictly increasing (:198-200) |
| Identity/authentication precondition | every serial must be in 1..=`MAXIMUM_INODE_SERIAL = i64::MAX as u64` (`identity.rs:20`, checked `input.rs:151-153,191-197`); a declared new serial must never have been exposed before - "uniqueness across the caller's lifetime is the allocator's contract" (`input.rs:11-15`); a build must declare its root as a new inode (`input.rs:211-213`) |
| Batch limits | No count or byte ceiling on the three slices. The declared ceilings bound the **engine**: `scratch_bytes` (default 4 MiB-1, `limits.rs:30`), `maximum_pending_records` (default 4096, `reduce.rs:25`), `merge_buffer_bytes` (default 16 KiB, `runs.rs:30`), `base_read_batch` (default 32, `reduce.rs:27`), `ordering_bytes` (default 64 MiB, `runs.rs:32`). `FilesystemResources::check` (`input.rs:98-125`) rejects only degenerate values (<1024, 0, <ROW_BYTES). Derived: `maximum_touched_serials() = ordering_bytes / 8` (`input.rs:90-95`). |
| Backpressure | none: synchronous; a ceiling that would be crossed is an error (`ResourceUnavailable`, `ObjectLimitExceeded`) |
| Errors/cancellation | `ContentError`; `ContentError::IncompleteOperation` exists for "The operation was cancelled or its final completion did not run" (`error.rs:131-132`) |
| Process-local assumptions | none: pure data plus `InodeScope` (a 32-byte digest, `identity.rs:24`) |
| What a future adapter must supply | a *bounded* serialization of this request. **The adapter, not C1, must bound the message**: `check` validates shape/order/range, never total input size. |

### 2.4 OrderingBacking / OrderingRun

| Aspect | Evidence |
|---|---|
| Owner/definition | `content/filesystem/references/backing.rs:54` / :34 |
| Caller | `build_filesystem`/`update_filesystem` take `Option<&mut dyn OrderingBacking>` (`update.rs:70,82,95,107`); `RunStore::new(backing, merge_buffer, limit)` (`runs.rs:62-77`) |
| Actual API | see 1.2. Runs are created by the backing, appended to and read at offsets; the backing owns the bytes and reports held/peak/capacity/cleanup state, and is released exactly once by `release` |
| Borrowed/owned bytes | `OrderingRun` "owns the storage it hands out ... therefore never borrows from the caller" (`backing.rs:30-33`). In-memory rows are charged through `RunStore::reserve`/`charge_pending` (`runs.rs:119-153`); a spill reserves **twice** the row bytes so the pending map and the run it becomes both fit (`runs.rs:125`) |
| Ordering/preconditions | `RunStore::check_capacity` refuses at start if the backing's `capacity_bytes()` is smaller than the operation's declared `limit` (`runs.rs:84-98`, `ResourceUnavailable { what: "ordering backing capacity" }`) |
| Batch limits | `DEFAULT_ORDERING_BYTES = 64 MiB` (`runs.rs:32`), `DEFAULT_MERGE_BUFFER_BYTES = 16 KiB` (:30), `MAXIMUM_LEVELS = 32` tiers (:34), `DEFAULT_BACKING_CAPACITY_BYTES = 256 MiB` (`backing.rs:26`), `SCAN_LIMIT = 4_096` directory entries scanned for run naming (`backing.rs:145`) |
| Backpressure | reservation **before** the write: `Account::reserve` fails with `ObjectLimitExceeded` when held+bytes > capacity (`backing.rs:97-112`); a failed `write_all` returns the reservation (`backing.rs:317-322`) |
| Errors/cancellation | `release` is called once by the operation after consumers close; a failing release fails the operation (`backing.rs:10-14`, :76). Ordinary cancellation is handled by `Drop` (`backing.rs:297-304` for the backing, :344-359 for a run, each `remove_file`) |
| Process-local assumptions | **the only shipped implementation is local-file**: `pub struct FileBacking { directory: PathBuf, ... }` (`backing.rs:169-174`), `OpenOptions::new().create_new(true).write(true).read(true).open(&path)` (:249-256), run names `layerfs-ordering-{:08}.run` (:248) in a caller-chosen directory (:178,183). It is **!Send/!Sync**: `account: Rc<Account>` (:172,310) with `Cell`/`RefCell` inside `Account` (:81-92) |
| Escape hatch | `None` is legal: the run store then answers lookups/merges from in-memory state only and "the first spill fails explicitly, because growing without a declared owner is not a fallback" (`runs.rs:58-61`) |
| What a future adapter must supply | an `OrderingBacking` whose storage is reachable from wherever C1 runs, with a declared `capacity_bytes()` >= the operation's ordering ceiling, real `read_at`, and a `release` that reports failure |

### 2.5 Store / SaveOperation / SaveHandoff

| Aspect | Evidence |
|---|---|
| Owner/definition | `storage/cas/store.rs:95` / :271 / :509 |
| Caller | application/runtime adapter; `StoreProvider` is the C2->C1 bridge (`cas/provider.rs:19`) and `SaveHandoff` the C1->C2 bridge (`store.rs:509`) |
| Actual API | `Store`: `create`/`open`, `default_policy`, `policy`, `capacities`, `path`, `pool_index_entries`, `pool_index_bytes`, `begin_save`, `read_batch`, `contains` (`store.rs:107-252`). `SaveOperation`: `accept`, `finish`, `read_batch`, `abort`, `capacities`, `policy`, `baseline_pack_id`, `retained_tail_bytes`, `pending`, four counter accessors and `Drop` (:280-502). `SaveHandoff`: `new`, `failure`, `take_failure`, `operation`, `accept` (:514-548) |
| Borrowed/owned bytes | `accept` moves `FinalizedObject` into the batch; `PendingBatch` "releases their allocations to the caller" on `drain` (`batch.rs:71-75`). `Store::read_batch` returns owned `Vec<Vec<u8>>`. A save writes pack bytes with `&write.bytes` (`owner.rs:827,830`) |
| Ordering/visibility precondition | a save's output becomes visible only when its final transaction advances the watermark: `advance_retained_pack_ceiling` (`owner.rs:915`, `schema.rs:273-283`); readers capture the ceiling once (`store.rs:211-213`) |
| Writer ownership | exactly one attempt: `write::begin_immediate(&connection)?` (`owner.rs:176`), implemented as `connection.execute_batch("BEGIN IMMEDIATE")` (`sqlite/write.rs:51-55`) with `busy_timeout(Duration::ZERO)` (`sqlite/connection.rs:43`) and busy/locked mapped to `StorageError::OwnershipUnavailable` (`connection.rs:66-72`). **Cross-process exclusivity comes from the SQLite file lock, not from any in-process guard**: there is no static or mutex around `begin_save` (`store.rs:185-199`) |
| Batch limits | writes 512 objects / 512 KiB per wave (`policy.rs:65-67`, `batch.rs:48-69`); transaction 8191 rows / 4 MiB-1 (`policy.rs:69-71`, applied `owner.rs:840-857`); reads 4096 ids (`policy.rs:80`, `store.rs:207,246,346`); lookup page 128 ids (`policy.rs:63`, `lookup.rs:58,132`); pack 256 KiB and singleton 16 MiB + 4 KiB (`policy.rs:55,88-90`); five framing lanes (`owner.rs:107-108`) |
| Backpressure | `PendingBatch::push` returns the drained wave (`batch.rs:48`); `SaveOperation::accept` flushes it synchronously (`store.rs:295-299`) - the producer is stopped by `OutputRejected`, never queued |
| Errors/cancellation | one terminal disposition: `finish::terminate` marks terminal plus one cleanup, or quarantines on `UnknownOutcome` (`cas/finish.rs:12-25`). `Drop` makes one best-effort cleanup, never repeated, never on a quarantined save (`store.rs:491-502`) |
| Process-local assumptions | `path: PathBuf` (`store.rs:96`) exposed as `pub fn path(&self) -> &Path` (:164-166); `MutationOwner { connection: Connection, ... }` owns the engine handle (`owner.rs:99-101`) - note **`sqlite/pool.rs` is not a connection pool** but the pooled-metadata catalogue (`sqlite/pool.rs:1-6`); each public call opens its own `Connection` (`store.rs:187,209,247`) and drops it at scope end (`store.rs:121,138`) |
| What a future adapter must supply | a path (today) or a transport plus a `StoreProvider`/`SaveHandoff` shim; plus policy/capacity agreement with the remote side, since `Store::create` writes and `open` validates the persisted policy (`schema.rs:64-135`) |

### 2.6 TimingScope / TimingReport

| Aspect | Evidence |
|---|---|
| Owner/definition | `telemetry/src/timer/scope.rs:36` / `report.rs:196` |
| Actual API | `Timing::record(name, op) -> (Result<T,E>, TimingReport)` (`scope.rs:172-187`), `Timing::disabled(...)` (:194-205); `TimingScope::run` (:140-162), `Active::child` (:93-98), `Active::attach` (:106-110), `is_recording` (:61-69); `TimingReport::write_text`/`write_json` (`format.rs:14`, `json.rs:18`) |
| Borrowed/owned bytes | the scope borrows the `Recording` for 'a (`scope.rs:44-52`); the report **owns** its tree (`report.rs:38-46`); attachment consumes a completed report, never a live scope (`scope.rs:100-110`) |
| Ordering/preconditions | a pending scope is single-use (`run` consumes `self`, `scope.rs:140`); children come only from the running handle, so a child cannot outlive its parent's measured region (`scope.rs:9-13`) |
| Batch limits | `MAX_NODES = 1_024` (`recording.rs:14`), `MAX_DEPTH = 32` (:17), `MAX_LABEL_BYTES = 128` (:20); a child that does not fit is omitted and the node is marked incomplete (`report.rs:104-118`) |
| Backpressure | none - clipping is silent-by-flag and `is_incomplete()` makes it visible (`report.rs:136-138,243-248`) |
| Errors/cancellation | `run` returns the operation's result unchanged (`scope.rs:159-161`); a scope whose node cannot be created runs the operation unmeasured rather than failing (`scope.rs:153-156`) |
| Process-local assumptions | **the handle is !Send + !Sync by construction**: `thread: PhantomData<*const ()>` (`scope.rs:39`) and the doc at :33-35. It reads no env and no file (`lib.rs:8-11`); output goes only through caller-supplied `impl Write` (`format.rs:14`, `json.rs:18`) |
| What a future adapter must supply | a `TimingScope` at every entry point, and - if it wants a tree - one `Timing::record` around the operation, including for calls that must not cross an `.await` |

### 2.7 Error and cancellation surfaces (shared by all seams)

- C1: `ContentError` with 33 variants (`content/error.rs:11-133`), including `OutputRejected` (:84), `BatchCardinality` (:100), `ResourceUnavailable` (:125), `InvalidOrderingRecord` (:130), `IncompleteOperation` (:132).
- C2: `StorageError` with 14 variants (`storage/error.rs:14-80`), including `OwnershipUnavailable` (:38), `UninspectedState` (:44), `UnknownOutcome` (:67), `CleanupFailed` (:72), `Aborted` (:79); `is_unknown_outcome` at :84-86.
- The two are bridged one-way: `impl From<ContentError> for StorageError` (`storage/error.rs:141-145`); the reverse does not exist, which is why `SaveHandoff` has to retain the storage error itself (`store.rs:509-547`).
- Nothing in either crate retries: `finish.rs:1-6` ("A failure whose persistence outcome is unproven quarantines the save instead: nothing is resent, polled or deleted on a guess").

---

## 3. Process-local assumptions

Greps used for this sweep (the only commands run for this section), from the repository root:

```
grep -rn "std::path|PathBuf|Path>|AsRef<Path>" core/crates/*/src
grep -rn "std::fs::|OpenOptions|File::|read_dir|remove_file|create_dir" core/crates/*/src
grep -rn "thread_local|OnceLock|OnceCell|lazy_static|static mut|static " core/crates/*/src
grep -rn "to_ne_bytes|from_ne_bytes|transmute|as *const|as *mut|unsafe" core/crates/*/src
grep -rn "env::var|std::env" core/crates/*/src
grep -rn "thread::spawn|available_parallelism|rayon" core/crates/*/src
grep -rn "Rc<|RefCell|Cell<|Mutex|Arc<" core/crates/*/src
grep -rn "fsync|fdatasync|sync_all|sync_data" core/crates/*/src
```

| # | Assumption | Finding (path:line, quoted) | Classification |
|---|---|---|---|
| 3.1 | hardcoded filesystem paths | **none.** The only path-shaped literal in product source is the run-name pattern `const RUN_NAME_PREFIX: &str = "layerfs-ordering-";` / `const RUN_NAME_SUFFIX: &str = ".run";` (`content/filesystem/references/backing.rs:143-144`), used for names *inside* a caller-supplied directory. No /tmp, no HOME literal, no env var read (`std::env` count = 0). | clean |
| 3.2 | `std::path::Path` in the save/read path | 11 matches in three files: `content/.../backing.rs:20,92,170,178,183,308`; `storage/sqlite/connection.rs:9`; `storage/cas/store.rs:9,96,110,132`. In C2 this is the Store's own identity: `96|    path: PathBuf,` (`store.rs:96`) and `23|    let connection = Connection::open_with_flags(path, flags)?;` (`connection.rs:23`). In C1 it is only the local `FileBacking`. **No `Path` appears in the object save/read data flow** - the flow is `ObjectId`/`Vec<u8>` (`cas/store.rs:291,341`) | construction/identity scope in C2; opt-in local only in C1 |
| 3.3 | native file handles | 6 matches, all in `content/.../backing.rs`: `OpenOptions` (:18), `std::fs::read_dir` (:146), `std::fs::remove_file` (:220,349), `file: std::fs::File` (:307). **C2 has zero `std::fs` usage** beyond rusqlite's own opens | opt-in in C1, none in C2 |
| 3.4 | concrete `FileBacking` outside its trait | **zero** referents outside the defining module. `update.rs:20` imports only the trait; the parameter type is `Option<&mut dyn OrderingBacking>` (`update.rs:70,82,95,107`); `runs.rs:38` `backing: Option<&'r mut (dyn OrderingBacking + 'b)>,` | trait-boundary only |
| 3.5 | SQLite connection ownership/lifetimes | `MutationOwner { connection: Connection, ... }` (`owner.rs:99-100`) **owns** it; `pub fn connection(&self) -> &Connection` (:244-246). `SaveOperation { owner: Option<MutationOwner>, ... }` (`store.rs:271-278`) owns the owner; `Store` does **not** hold a connection (`store.rs:95-105` holds path/policy/capacities/pool_index). Every public call opens and drops its own connection: `store.rs:187-189,209,247`. No `Send`/`Sync` impl anywhere | per-call connection, no pool, no in-process exclusivity |
| 3.6 | thread-local / global state | Two `OnceLock`s, both pure memoization of a hash-derived constant: `static PROFILE_ID: std::sync::OnceLock<ObjectId> = std::sync::OnceLock::new();` (`content/file/mapping/codec.rs:32`) and `static PROFILE_ID: std::sync::OnceLock<[u8; 32]> = std::sync::OnceLock::new();` (`content/file/cdc/gear.rs:30`). Both `get_or_init` a deterministic BLAKE3 digest of frozen constants (`codec.rs:33-49`, `gear.rs:31-48`). **Zero** `thread_local!`, `lazy_static`, `static mut`, atomics | benign memoization |
| 3.7 | host byte order | **no native-endian read anywhere.** C1 writes/reads big-endian (`object/codec.rs:64` `write(writer, &payload_len.to_be_bytes())?;`; `filesystem/references/record.rs:89` `bytes[..8].copy_from_slice(&serial.to_be_bytes());`); C2 writes/reads little-endian (`pack/assemble.rs:269` `bytes.extend_from_slice(&lane.version().to_le_bytes());`; `encoding/decode.rs:163` `let count = u32::from_le_bytes(`). Counts: 113 `to_be_bytes|from_be_bytes` hits in C1, 30 `to_le_bytes|from_le_bytes` hits in C2, **0** `to_ne_bytes|from_ne_bytes` in either. No `transmute`, no `target_endian` cfg, no `#[repr(C)]`/`#[repr(packed)]`/`#[repr(transparent)]` | clean (endian-explicit; the C1/C2 asymmetry is real but is not documented as a contract) |
| 3.8 | `unsafe` | C1 and telemetry `#![forbid(unsafe_code)]` (`content/src/lib.rs:16`, `telemetry/src/lib.rs:13`). C2 permits it (`storage/src/lib.rs:18-19`) and uses it for zstd FFI in one file: `encoding/codec.rs:110,118,162,192,266,331,407,445,503,561,600,608`. The encoder/decoder state is per-workspace (`CompressionWorkspace`/`DecompressionWorkspace`, `codec.rs:150,380`), not global | local FFI, per-operation workspaces |
| 3.9 | caller-supplied resource escape hatches | (a) `FileBacking::new(directory: impl AsRef<Path>)`/`with_capacity(directory, capacity_bytes)` (`backing.rs:178,183`) and `Option<&mut dyn OrderingBacking>` (`update.rs:70`); (b) `Store::create/open(path, ...)` (`store.rs:109,132`); (c) `FilesystemResources` (`input.rs:56-68`); (d) `AuthenticatedObjects`/`FinalizedConsumer` implementations; (e) `sink: &mut dyn Write` (`file/read.rs:23,53,64`) and the renderers (`format.rs:14`, `json.rs:18`). Trust boundaries: a supplied provider is trusted to authenticate (`access.rs:26-28`); `FilesystemResources` is checked only for degeneracy (`input.rs:98-125`), and a caller that declares 64 MiB of ordering over a small backing is refused at `check_capacity` (`runs.rs:84-98`) rather than silently exceeding it | explicit, validated where enforceable |
| 3.10 | thread/process assumptions | **zero** `thread::spawn`, `available_parallelism` or rayon in product source; **zero** `Send`/`Sync` bounds. Concurrency is expressed only as (i) `Arc<Mutex<PoolIndex>>` owned by `Store` (`store.rs:104,126,143`) and passed to the owner (`owner.rs:172`), (ii) `Rc<Account>`+`Cell`/`RefCell` inside `FileBacking` (`backing.rs:81-92,172`), (iii) `RefCell` inside the telemetry `Recording` (`recording.rs:104,123`). The single-writer assumption is *not* enforced in-process; it is enforced by `BEGIN IMMEDIATE` plus a zero busy timeout (`owner.rs:176`, `connection.rs:43`) | single-process by construction, exclusive by SQL lock |
| 3.11 | caller string interpolated into SQL | `sqlite/connection.rs:48-51`: `pub fn pragma_i64(connection: &Connection, name: &str) -> StorageResult<i64> {` / `    let sql = format!("PRAGMA {name}");` / `    Ok(connection.query_row(&sql, [], |row| row.get(0))?)`. No allowlist or escaping; in-tree callers pass constants only (`schema.rs:139-140` `"application_id"`, `"user_version"`), so there is no live injection path today, but the function is `pub` and accepts any string. Same shape with constant sources: `schema.rs:195` `.prepare(&format!("PRAGMA table_info({table})"))?` over the `REQUIRED_TABLES` constant (`schema.rs:22-58`); `cleanup.rs:65,120` and `lookup.rs:59,133` build `?`-placeholder lists with `.join(",")` over a bounded page width | latent; no current in-tree caller passes a non-constant |
| 3.12 | connection profile not re-enforced at acquisition | `cas/owner.rs:169-173` `pub fn acquire(connection: Connection, capacities: StorageCapacities, pool_index: Arc<Mutex<PoolIndex>>) -> StorageResult<Self>` takes **ownership** of a caller-built connection and never calls `connection::configure`, so the verified MEMORY-journal / `synchronous = OFF` / `foreign_keys = ON` profile of `connection.rs:29-45` is a property of `connection::open` only. In-tree, `store.rs:187` always goes through `connection::open`, so there is no live bypass; but the `sqlite::*` modules are public and every function takes a caller-supplied `&Connection` | latent API-level, no in-tree bypass |
| 3.13 | serialized lengths derived from `usize` | exactly three sites widen a `usize` into a persisted `u64`: `object/inode_leaf.rs:267` `value.extend_from_slice(&(count as u64).to_be_bytes());`, `:432` `prefix[15..23].copy_from_slice(&(count as u64).to_be_bytes());`, `:433` `prefix[23..31].copy_from_slice(&(count as u64 * LEAF_ROW_BYTES as u64).to_be_bytes());`. `count` is a bounded row count (`inode_leaf.rs:402` `let count = payload / POOLED_ROW_BYTES;`, guarded at :403). The fail-closed form used elsewhere is `sorted/format.rs:673` `let count = u16::try_from(count).map_err(|_| ContentError::LengthOverflow)?;`. The widening itself is lossless on every Rust target | minor; correct in practice, less explicit than its neighbours |

---

## 4. Limits

Provenance: every `src/` constant below was read directly for this review. Test-coverage citations marked [sweep] were reported by a parallel read-only limits sweep (file:line reported there, not re-read here); rows without that mark quote code I read myself. Classification legend: **format width** (a persisted field's own width), **enforced policy** (a declared value the code refuses to exceed), **derived bound** (computed from another limit), **backend restriction** (SQLite/engine), **measured coverage** (largest input actually exercised), **future-owner responsibility** (no limit exists).

### 4.1 Number of file revisions (retained roots vs delta depth)

| Limit | Source (path:line, quoted) | Unit | Scope | Configurability | Boundary behaviour | Largest verified input | Class |
|---|---|---|---|---|---|---|---|
| whole-file delta depth | `core/crates/layerfs-content/src/policy.rs:20` `pub const DEFAULT_WHOLE_FILE_DELTA_MAX_DEPTH: u8 = 8;` | dependency edges | one file's whole-file object chain | policy field; settable via `ConstructionPolicy::new` (`policy.rs:60`), persisted per Store (`storage/policy.rs:148-151`, `storage/sql/schema.sql:20-21`) | exceeding it is not an error: selection declines the base and the record is stored FULL | 16 edges followed in one chain (`core/crates/layerfs-storage/tests/delta_chains.rs:110`) [sweep] | enforced policy |
| chunk delta depth | `core/crates/layerfs-content/src/policy.rs:22` `pub const DEFAULT_CHUNK_DELTA_MAX_DEPTH: u8 = 4;` | dependency edges | one chunk chain | as above | as above | `tests/delta_chains.rs:73,110` [sweep] | enforced policy |
| absolute depth ceiling | `core/crates/layerfs-content/src/policy.rs:24` `pub const MAXIMUM_DELTA_MAX_DEPTH: u8 = 50;` | dependency edges | all payload roles + pooled metadata | policy-field maximum; enforced in `ConstructionPolicy::validated` (`policy.rs:91-100`), the SQL `CHECK` (`storage/sql/schema.sql:20-26`) and `StoragePolicy::validated` (`storage/policy.rs:205-209`) | `ContentError::UnsupportedPolicy { field }` / `StorageError::UnsupportedPolicy` | `tests/policy_capacity.rs:279` asserts a persisted 50 [sweep]; end-to-end acceptance at 50 UNVERIFIED | enforced policy |
| pooled-metadata depth | `core/crates/layerfs-storage/src/policy.rs:125` `pub const DEFAULT_METADATA_DELTA_MAX_DEPTH: u8 = 8;` | dependency edges | pooled inode-leaf chain | policy field, schema v4 | refused above 50 (`policy.rs:205-209`) | 9th leaf stored FULL at the cap (`tests/metadata_chain.rs:124-144`) [sweep] | enforced policy |
| chain walk guard | `core/crates/layerfs-storage/src/encoding/delta/select.rs:113` `if path.len() > usize::from(MAXIMUM_DELTA_MAX_DEPTH) + 1 {` | records walked | one dependency walk | derived from the depth ceiling | `StorageError::Integrity("stored dependency chain depth")` (`select.rs:114`) | `tests/metadata_chain.rs:150` [sweep] | derived bound |
| chain byte budgets | `storage/policy.rs:102` `pub const CHAIN_CANONICAL_LIMIT: u64 = 512 * 1024;`; `:104` `CHAIN_ENCODED_LIMIT = 256 * 1024`; `:123` `METADATA_DECODED_WORK_LIMIT = 32 * 1024 * 1024`; `:127` `METADATA_CHAIN_CANONICAL_LIMIT = 8 * 8_192`; `:129` `METADATA_CHAIN_ENCODED_LIMIT = 17 * 8_193` | canonical/encoded bytes, decoded work | one chain | derived into `StorageCapacities` (`storage/policy.rs:320-323`) | selection declines the edge (`select.rs:290-291`); a reader that still meets it refuses with a typed error (`encoding/delta/read.rs:193-214`, `encoding/pool/read.rs:248-249`) | `tests/delta_chains.rs:115-170`, `tests/metadata_chain.rs:131,194,198` [sweep]; `METADATA_DECODED_WORK_LIMIT` has no test [sweep] | enforced policy |
| **total retained roots / revisions** | **NO LIMIT FOUND.** `storage/sql/schema.sql:27-31` `-- Publication watermark: the highest pack id belonging to a COMPLETED save.` is a *visibility* watermark (advanced only at `cas/owner.rs:915`), not retention; the only `DELETE` in the product is failed-save cleanup (`sqlite/cleanup.rs:65` `DELETE FROM objects WHERE object_id IN (...)` and `:120` `DELETE FROM object_packs WHERE pack_id IN (...)`) | roots/objects | whole Store lifetime | none exists | none — every acknowledged save's objects and packs stay forever | none | **future-owner responsibility** |

### 4.2 Maximum file size

| Limit | Source | Unit | Scope | Configurability | Boundary behaviour | Largest verified input | Class |
|---|---|---|---|---|---|---|---|
| logical length | `core/crates/layerfs-content/src/file/mapping/types.rs:244` `pub logical_len: u64,`, written big-endian at `file/mapping/codec.rs:276` `value.extend_from_slice(&state.logical_len.to_be_bytes());` | bytes | file state | format width u64 | overflow refused earlier as `ContentError::LengthOverflow` (`content/error.rs:18`) | 24 MiB + 1 constructed and read back exactly (`tests/file_read.rs:265-267`) [sweep] | format width |
| extent slice widths | `content/file/mapping/types.rs:67,72` `pub const fn source_offset(self) -> u32` / `pub const fn logical_length(self) -> u32`; range check at `types.rs:51` `if logical_length == 0 \|\| end > crate::file::cdc::MAXIMUM_CHUNK_BYTES as u32 {` | u32 | one extent | derived from the chunk maximum | `ContentError::InvalidRecord("extent slice")` | `tests/file_complete.rs:143-145` [sweep] | derived bound |
| chunk size | `content/file/cdc/gear.rs:13,15,17` `MINIMUM_CHUNK_BYTES = 8_192` / `TARGET_CHUNK_BYTES = 16_384` / `MAXIMUM_CHUNK_BYTES = 32_768` | bytes | one chunk | frozen profile constants | `ObjectLimitExceeded` (`file/mapping/codec.rs:57-62`) | both sides in `tests/file_complete.rs` [sweep] | format width |
| extent page / tree | `content/policy.rs:212` `pub const MAX_MAPPING_ENTRIES: usize = 128;`; `file/mapping/types.rs:13,17,23` `MIN_ENTRIES = 64` / `MAX_LEVEL: u8 = 31` / `MAX_NODE_OBJECT_BYTES = 8_192` | entries, levels, bytes | extent tree node | constants | `NonCanonicalPagePartition` / `MappingDepthExceeded` (`content/error.rs:69,80`) | deepest tree built in tests is 2 levels (`tests/file_read.rs:219-227`) [sweep]; depth-31 capacity UNVERIFIED | format width (tree capacity is a **derived bound, UNVERIFIED**) |
| construction cutoff | `content/policy.rs:14,16,18` `DEFAULT_SMALL_FILE_THRESHOLD_BYTES = 131_072` / `MINIMUM = 131_072` / `MAXIMUM = 1_048_576` | bytes | whole-file vs chunked representation | policy field, power of two only (`policy.rs:83-90`); persisted `sql/schema.sql:18-19` | non-power-of-two ⇒ `UnsupportedPolicy`; a whole-file payload above cutoff−1 (`policy.rs:146` `let raw = (self.small_file_threshold_bytes as usize).saturating_sub(1);`) is refused as `BoundedCapacityExceeded` | `tests/memory_bounds.rs:73-74` (131071 vs 131072) [sweep] | enforced policy |
| derived whole-file sizes | `content/policy.rs:155` `whole_file_canonical_limit: raw.saturating_add(WHOLE_FILE_CANONICAL_OVERHEAD),` (+23, `policy.rs:196`); `policy.rs:31` `DEFAULT_WHOLE_FILE_FRAME_LIMIT = 135_168`; `policy.rs:37` `pub const fn conservative_frame_bound(raw: usize) -> usize { raw + raw / 128 + 1024 }` | bytes | whole-file object and codec frame | derived | capacity checks refuse | `tests/policy_capacity.rs:155-157` [sweep] | derived bound |
| canonical object ceiling | `content/policy.rs:203` `pub const MAX_CANONICAL_OBJECT_BYTES: usize = 16 * 1024 * 1024;`; `:209` `MAX_OBJECT_FIELD_BYTES = 8 * 1024 * 1024`; `object/codec.rs:21` `pub const MAX_PAYLOAD_BYTES: usize = MAX_CANONICAL_OBJECT_BYTES - HEADER_LEN;` (`codec.rs:18` `HEADER_LEN = 9`) | bytes | any canonical object | constants | `ContentError::ObjectLimitExceeded` (`object/codec.rs:82-87`, `:98-103`) | two-sided at 16 MiB and 16 MiB+1 (`tests/object_identity.rs:323,344`) and 8 MiB/+1 (`:256,266`) [sweep] | format width |
| stored canonical limit + SQL | `storage/policy.rs:61` `pub const CANONICAL_LIMIT: usize = 16 * 1024 * 1024;`; `storage/sql/schema.sql:56` `canonical_length INTEGER NOT NULL CHECK (canonical_length > 0 AND canonical_length <= 16777216),` | bytes | one stored object | constant + SQL CHECK | `StorageError::CapacityExceeded` (`encoding/full.rs:112`), SQL CHECK as last guard | `tests/physical_formats.rs:77,98` [sweep] | backend restriction |
| read-time logical bound | `content/file/read.rs:19-24` `pub fn read_all_bounded(reader: &dyn AuthenticatedObjects, root: ObjectId, maximum: u64, sink: &mut dyn Write, scope: TimingScope<'_>)`; `read_all` passes `u64::MAX` (`read.rs:56`) | bytes | one whole-file read | caller-declared | `BoundedCapacityExceeded { what: "read.logical_length" }` (`read.rs:31-37`) | 24 MiB + 1 (`tests/file_read.rs:267`) [sweep] | enforced policy |
| largest file through the whole pipeline | `core/crates/layerfs-storage/tests/core_pipeline.rs:261-268` `let bytes = noise(6 * 1024 * 1024 + 1);` … `assert_eq!(read_back(&store, result.root), bytes);` | bytes | C1→C2→read, plus reopen at `:270-286` | test fixture | — | 6 MiB + 1 | measured coverage |

### 4.3 Maximum file count

| Limit | Source | Unit | Scope | Configurability | Boundary behaviour | Largest verified input | Class |
|---|---|---|---|---|---|---|---|
| inode serial | `content/filesystem/identity.rs:20` `pub const MAXIMUM_INODE_SERIAL: u64 = i64::MAX as u64;` | serials | one allocation scope | constant | `InvalidRecord("inode serial")` (`identity.rs:47-52`; checked `filesystem/input.rs:151-153,191-197`) | over-bound refused (`tests/filesystem_failure.rs:495-543`) [sweep]; **acceptance at the bound untested** | format width |
| namespace reference count | `content/object/inode_leaf.rs:88` `pub namespace_ref_count: u64,` (big-endian at `:120`) | hardlinks/aliases | one inode | format width u64 | derived count saturates to 0 on removal | hardlink alias matrix (`tests/filesystem_hardlinks.rs`) [sweep] | format width |
| inode leaf fill | `content/filesystem/limits.rs:20,22` `MINIMUM_INODE_LEAF_ROWS = 50` / `MAXIMUM_INODE_LEAF_ROWS = 100`; `object/inode_leaf.rs:34` `MAXIMUM_LEAF_ROWS = 100` | rows | one inode leaf | constants | leaf refused (`object/inode_leaf.rs:246,312,362,403`) | 100 rows (`tests/metadata_pool.rs:174-191`), 101 refused (`object/inode_leaf.rs:208`) [sweep] | format width |
| inode branch fanout | `content/filesystem/limits.rs:24,26` `MINIMUM_INODE_BRANCH_CHILDREN = 64` / `MAXIMUM_INODE_BRANCH_CHILDREN = 127` | children | one inode branch | constants | page refused | none at scale [sweep] | format width |
| tree depth (files + dirs + attributes) | `content/filesystem/limits.rs:18` `pub const MAXIMUM_TREE_LEVEL: u8 = 31;`; enforced at `filesystem/directory/read.rs:122` `if depth > crate::filesystem::limits::MAXIMUM_TREE_LEVEL {`, `attributes/codec.rs:158,209` | levels | all three trees | constant | `MappingDepthExceeded` | no test builds a deep tree; deepest verified 2 levels [sweep] | format width (capacity at 31 is **derived, UNVERIFIED**) |
| directory leaf rows | `content/filesystem/limits.rs:57-58` `pub const MAXIMUM_DIRECTORY_LEAF_ROWS: usize = (MAXIMUM_PAGE_BYTES - EMPTY_PAGE_BYTES) / (2 + 1 + 8);` (= 740) | rows | one directory leaf | derived | `NonCanonicalPagePartition` | ceiling+1 refused (`tests/filesystem_codec.rs:336,354-360`) [sweep] | derived bound |
| page size / fill rule | `limits.rs:10` `MAXIMUM_PAGE_BYTES = 8_192`; `:61` `EMPTY_PAGE_BYTES = 44`; `:28` `MINIMUM_FILLED_PAGE_BYTES = 3_277`; `:64-66` `pub const fn filled_page(bytes: usize) -> bool { bytes * 5 >= MAXIMUM_PAGE_BYTES * 2 }` | bytes | one tree page | constants | page refused (`filesystem/sorted/page.rs`, `directory/read.rs:268-273`) | `tests/filesystem_codec.rs:334`, `tests/filesystem_attributes.rs:316,321` [sweep] | format width |
| read wave | `content/filesystem/objects.rs:20` `pub const MAXIMUM_READ_DEMANDS: usize = 4_096;` | objects | one filesystem read wave | constant, mirrored by C2 (`storage/policy.rs:80`) | `ObjectLimitExceeded` (`objects.rs:90-95`) | 4,000-entry directory (`tests/filesystem_bounds.rs:304`) [sweep] | enforced policy |
| verified counts | 4,000-entry directory; 600 released files; 1,500 sorted entries; 4,097 attribute keys; 4,096-object demand [sweep] | — | — | — | — | see citations | measured coverage |

### 4.4 Maximum directory length (names, paths, entries, pages)

| Limit | Source | Unit | Scope | Configurability | Boundary behaviour | Largest verified input | Class |
|---|---|---|---|---|---|---|---|
| name | `content/filesystem/limits.rs:12` `pub const MAXIMUM_NAME_BYTES: usize = 255;`; `filesystem/path.rs:182` `if bytes.len() > MAXIMUM_NAME_BYTES {`; grammar at `path.rs:188-194` rejects `.`/`..`/NUL/`/`/`\\` | **bytes** (UTF-8), not characters | one component | constant | `PathLimitExceeded` / `InvalidPath` (`content/error.rs:107,109`) | 255-byte names (`tests/filesystem_codec.rs:365,371`) [sweep] | format width |
| full path | `limits.rs:14` `pub const MAXIMUM_PATH_BYTES: usize = 4_096;` (`path.rs:156`), `limits.rs:16` `pub const MAXIMUM_PATH_COMPONENTS: usize = 256;` (`path.rs:171`) | bytes, components | one canonical path | constants | `PathLimitExceeded` | **NO TEST FOUND** [sweep] | format width |
| entries per directory | no count ceiling; bounded only structurally by depth (`limits.rs:18`) times leaf fill (`limits.rs:57-58`) | entries | one directory | none | tree depth refusal | 4,000 entries in a directory (`tests/filesystem_bounds.rs:304`) [sweep] | derived bound, scale UNVERIFIED |
| listing page | caller-declared `max_entries`/`max_bytes` (`content/filesystem/directory/read.rs:191-192`), rejected when zero (`:195`), continuation at `:33-38` `pub continuation: Option<PathName>,` | entries + bytes | one listing call | **caller-declared; no library default** | `InvalidRecord("listing limit")` / `ObjectLimitExceeded` when one row cannot fit (`:219-228`) | count 5 / bytes 15 with resumed pages of 4 (`tests/filesystem_read.rs:139-164`) [sweep] | enforced policy |
| traversal validation | `content/filesystem/validate.rs:53` `pub const MAXIMUM_CYCLE_CHECK_ENTRIES: usize = 4_096;`; `:55` `pub const ALLOCATION_CHECK_BATCH: usize = 64;` | entries | one validation pass | constants | `InvalidRecord("cycle check work limit")` | limit−8 accepted / limit+512 refused (`tests/filesystem_bounds.rs:787-805`) [sweep] | enforced policy |


---

### 4.5 Maximum filesystem / workspace size

| Limit | Source | Unit | Scope | Configurability | Boundary behaviour | Largest verified input | Class |
|---|---|---|---|---|---|---|---|
| pack framing | `core/crates/layerfs-storage/src/policy.rs:55` `pub const PACK_LIMIT: usize = 256 * 1024;`; `:57` `GROUP_COUNT_LIMIT = 256`; `:59` `RECORD_COUNT_LIMIT = 8_191`; `:45,53` `GROUP_LIMIT = 65_536` / `GROUP_TARGET = 48 * 1024` | bytes, rows | one pack / one group | constants | `Integrity("pack length")` (`pack/layout.rs:287-289`), `CapacityExceeded` (`pack/assemble.rs:49-53`) | pack: 256 KiB+1 refused (`tests/physical_formats.rs:71`), :99 [sweep]; `GROUP_COUNT_LIMIT` and `RECORD_COUNT_LIMIT` have **no test** [sweep] | enforced policy |
| singleton pack | `storage/policy.rs:88-90` `SINGLETON_FRAMING_SLACK = 4_096` / `SINGLETON_PACK_LIMIT = CANONICAL_LIMIT + SINGLETON_FRAMING_SLACK` (= 16,781,312) | bytes | one singleton pack | derived | `CapacityExceeded` (`encoding/full.rs:207`) | 16 MiB + 4096 header boundary (`tests/physical_formats.rs:77,98`) [sweep] | derived bound |
| pack count | `storage/sql/schema.sql:35` `pack_id INTEGER PRIMARY KEY CHECK (pack_id > 0),`; `sqlite/lookup.rs:166-172` `SELECT COALESCE(MAX(pack_id), 0) FROM object_packs` | packs | whole Store | i64 column; next id is `checked_add` (`cas/owner.rs:187-189` `StorageError::Integrity("pack identifier overflow")`) | overflow refused, not wrapped | UNVERIFIED near the ceiling | format width |
| pooled metadata | `storage/policy.rs:93,95,97` `VALUES_PER_GROUP = 165` / `METADATA_GROUP_LIMIT = 16 * 1024` / `POOLED_LEAF_ROWS_LIMIT = 100`; `sql/schema.sql:40,45` `first_ordinal ... BETWEEN 1 AND 4294967295` and `CHECK (first_ordinal + count <= 4294967296)` | values, bytes | pooled lane | constants + SQL CHECK | `Integrity("metadata ordinal maximum")` (`sqlite/pool.rs:64-67`) | 1,312 groups (`tests/metadata_window.rs:160`) [sweep]; ordinal exhaustion UNVERIFIED | format width |
| ordering + scratch (one operation) | `content/filesystem/references/runs.rs:32` `DEFAULT_ORDERING_BYTES = 64 * 1024 * 1024`; `:34` `MAXIMUM_LEVELS = 32`; `:30` `DEFAULT_MERGE_BUFFER_BYTES = 16 * 1024`; `content/filesystem/references/backing.rs:26` `DEFAULT_BACKING_CAPACITY_BYTES = 256 * 1024 * 1024`; `content/filesystem/sorted/page.rs:23` `MAXIMUM_SCRATCH_BYTES = 4 * 1024 * 1024`; `content/filesystem/limits.rs:30` `DEFAULT_OPERATION_SCRATCH_BYTES = 4 * 1024 * 1024 - 1` | bytes, tiers | one filesystem operation | resource fields + constants; a backing declares its own ceiling | `ObjectLimitExceeded` on a reservation that does not fit; a backing whose capacity is below the declared ceiling is refused before work starts (`runs.rs:84-98`) | custom 8/64 MiB backings (`tests/filesystem_bounds.rs:697,753`) [sweep]; `DEFAULT_BACKING_CAPACITY_BYTES`, `MAXIMUM_LEVELS` untested [sweep] | enforced policy |
| total logical payload / workspace bytes | **NO CAP.** Unique payload is content-addressed with an unbounded pack count (row 3); aliases are bounded only by `namespace_ref_count: u64` (`content/object/inode_leaf.rs:88`); nothing prunes retained objects (`sqlite/cleanup.rs:65,120` is failed-save cleanup only) | bytes | whole Store | none | none | none | **future-owner responsibility** |
| physical footprint (derived) | object bytes live in `object_packs.data` (`sql/schema.sql:34-37`), so DB size ≈ Σ pack bodies + locator rows + catalogue rows; there is no `VACUUM`, no page reclamation and no pack deletion on the success path | bytes | whole Store | none | grows monotonically with saves | not measured | derived, UNVERIFIED |

### 4.6 Attributes and symlinks

| Limit | Source | Unit | Scope | Configurability | Boundary behaviour | Largest verified input | Class |
|---|---|---|---|---|---|---|---|
| attribute domain | `content/filesystem/limits.rs:34` `pub const MAXIMUM_ATTRIBUTE_DOMAIN_BYTES: usize = 64;` | bytes | key domain | constant | `ObjectLimitExceeded` (`attributes/keys.rs:33-38`, `attributes/codec.rs:331-335`) | domain+1 refused (`tests/filesystem_attributes.rs:106`) [sweep] | format width |
| attribute key | `limits.rs:36` `pub const MAXIMUM_ATTRIBUTE_KEY_BYTES: usize = 255;` | bytes | key | constant | as above (`attributes/keys.rs:45-50`) | 255-byte keys (`tests/filesystem_attributes.rs:108,329`) [sweep] | format width |
| attribute value | `limits.rs:40` `pub const MAXIMUM_ATTRIBUTE_VALUE_BYTES: usize = 1024 * 1024;` | bytes | one value root | constant | refused on write (`attributes/value.rs:26-31`) and bounded on read (`attributes/read.rs:166`) | limit+1 refused (`tests/filesystem_failure.rs:550-560`) [sweep]; the "at the bound" case at `:562-565` actually writes 4096 bytes, so the 1 MiB accept side is **untested** (see defects) | enforced policy |
| keys per listing | `limits.rs:47` `pub const MAXIMUM_ATTRIBUTE_KEYS: usize = 4_096;` | keys | one `attribute_keys` listing | constant | `ObjectLimitExceeded` (`attributes/read.rs:241-246`) | 4,097 refused (`tests/filesystem_attributes.rs:603-644`) [sweep] | enforced policy |
| attribute page rows | `attributes/codec.rs:27,29` `LEAF_ROW_OVERHEAD = 37` / `BRANCH_ROW_OVERHEAD = 36`; page/fill from `limits.rs:10,28` | bytes | attribute page | constants | page refused (`attributes/codec.rs:95-97` vs `MAXIMUM_PAGE_BYTES`) | `tests/filesystem_attributes.rs:316,321` [sweep] | format width |
| portable mode/mtime | `attributes/portable.rs:13` `pub const MAXIMUM_NANOSECONDS: u32 = 999_999_999;`; `:29-32` mode mask (`0o777` / `0o1777`) | ns, bits | portable domain | constants | `InvalidRecord` (`portable.rs:35-37,90-92`) | 1e9 rejected (`tests/filesystem_attributes.rs:263`) [sweep] | format width |
| attribute values are extent-only objects | `attributes/value.rs:32-40` builds `FinalizedObject::new(ObjectRole::Chunk, encode_chunk_object(bytes)?)?` plus `encode_node(&ExtentNode::Leaf { ... })`, so a value never re-enters whole-file cutoff classification | objects | every attribute value | by construction | an empty value is refused (`attributes/value.rs:23-25`) | `tests/filesystem_failure.rs:547-565` [sweep] | enforced policy |
| symlink target | `limits.rs:32` `pub const MAXIMUM_SYMLINK_TARGET_BYTES: usize = 4_096;`; header length field is `u16` (`filesystem/symlink.rs:40`) | bytes | one target | constant (the u16 field would allow 65,535; the 4,096 policy binds first) | `InvalidRecord` (`symlink.rs:26-29`), `PathLimitExceeded` on decode (`:70-72`) | 4,097 refused (`tests/filesystem_read.rs:259`) [sweep]; 4,096 accept side **UNVERIFIED** | format width |

### 4.7 Storage capacity actually in force

| Limit | Source | Unit | Scope | Configurability | Boundary behaviour | Largest verified input | Class |
|---|---|---|---|---|---|---|---|
| connection profile | `storage/sqlite/connection.rs:33` `PRAGMA journal_mode = MEMORY` (result verified `:34-36`), `:37` `PRAGMA synchronous = OFF; PRAGMA temp_store = MEMORY;`, `:38` `PRAGMA foreign_keys = ON;` (verified `:39-42`), `:43` `connection.busy_timeout(Duration::ZERO)?` | settings | one connection | fixed | `Integrity("journal mode")` / `Integrity("foreign key enforcement")` | `tests/policy_capacity.rs:341-345` [sweep] | backend restriction |
| unset engine knobs | `connection.rs:29-45` sets only the five statements above: **no** `page_size`, `cache_size`, `max_page_count`, `mmap_size` | — | one connection | none | none | `tests/policy_capacity.rs:346-354` asserts only presence / host default [sweep] | backend restriction — exact `SQLITE_MAX_LENGTH`, `SQLITE_MAX_VARIABLE_NUMBER`, `SQLITE_MAX_PAGE_COUNT` in force are **UNVERIFIED** (no build run) |
| SQL column widths | `sql/schema.sql:35` `pack_id INTEGER PRIMARY KEY`; `:40` `first_ordinal ... BETWEEN 1 AND 4294967295`; `:41` `count ... BETWEEN 1 AND 165`; `:43` `group_number ... BETWEEN 0 AND 255`; `:44` `length(digest) = 32`; `:50` `length(object_id) = 32`; `:54` `object_role BETWEEN 1 AND 13`; `:56` `canonical_length ... <= 16777216`; `:64-65` `group_number < 256`, `record_number < 8191` | rows | schema | backend constants | INSERT refused by `CHECK` | `tests/cas_roundtrip.rs:261` [sweep] | backend restriction |
| row binding casts | `sqlite/write.rs:115-116` `row.group_number as i64,` / `row.record_number as i64,` | rows | every object insert | none locally; the SQL `CHECK` is the only guard | INSERT refused | `tests/cas_roundtrip.rs`, `tests/pack_locator.rs` [sweep] | format width (no local range validation) |
| transaction | `storage/policy.rs:69` `TRANSACTION_ROW_LIMIT = 8_191`; `:71` `TRANSACTION_CANONICAL_BYTES_LIMIT = 4 * 1024 * 1024 - 1`; commit + re-acquire at `cas/owner.rs:840-857`; final watermark commit `:915-917` | rows, bytes | one open transaction | capacities | a failed `COMMIT` becomes `UnknownOutcome` (`sqlite/write.rs:58-64`) | **no test references either constant** [sweep] | enforced policy |
| pending batch | `policy.rs:65,67` `BATCH_OBJECT_LIMIT = 512` / `BATCH_CANONICAL_BYTES_LIMIT = 512 * 1024`; `cas/batch.rs:50-58` | objects, bytes | one preparation wave | capacities | drains at the bound; an object larger than the whole byte bound is admitted into an **empty** batch (`batch.rs:45-58`) | 200 objects (`tests/memory_bounds.rs:105-139`) [sweep] | enforced policy (see defect 5) |
| read wave / SQL page | `policy.rs:80` `READ_OBJECT_LIMIT = 4_096`; `:63` `LOOKUP_PAGE_IDS = 128` (129 bound parameters per statement, `lookup.rs:59-68,133-140`) | objects | one read wave / one statement | capacities | `CapacityExceeded` before a connection opens (`cas/store.rs:259-268`) | 4,096 accepted / over-limit refused (`tests/cas_reuse.rs:296-303`) [sweep] | enforced policy |
| codec workspaces | `encoding/codec.rs:33,35` `ENCODE_WORKSPACE_BYTES = 2 MiB` / `DECODE_WORKSPACE_BYTES = 1 MiB`; `:37,39` `GROUP_LIMIT = 65_536` / `GROUP_FRAME_LIMIT = GROUP_LIMIT + 1024`; `:94,96` `CHUNK_RAW_LIMIT = 32_768` / `CHUNK_FRAME_LIMIT = 33_024` | bytes | one save / one read | constants | `Integrity("bounded Zstandard workspace unavailable")` (`codec.rs:100-102,124-134`) | chunk lanes (`tests/codec_frames.rs:105`) [sweep]; workspace sizes untested [sweep] | enforced policy |
| cleanup page | `policy.rs:73` `CLEANUP_PAGE_ROWS = 128` | rows | failed-save cleanup | constant | paged `DELETE` (`sqlite/cleanup.rs:66-73,121-128`) | **no test** [sweep] | enforced policy |

### 4.8 Operations, resources and concurrency

| Limit | Source | Unit | Scope | Configurability | Boundary behaviour | Largest verified input | Class |
|---|---|---|---|---|---|---|---|
| declared filesystem resources | `content/filesystem/input.rs:70-80` `FilesystemResources::default` = `sorted::MAXIMUM_SCRATCH_BYTES` (4 MiB), 4096 pending records, 16 KiB merge buffer, 32 base batch, 64 MiB ordering; validation `input.rs:98-125` | mixed | one filesystem operation | caller-declared fields | `ResourceUnavailable { what }` per field | `tests/filesystem_bounds.rs`, `tests/filesystem_sorted.rs` [sweep] | enforced policy |
| derived touched set | `input.rs:90-95` `match usize::try_from(self.ordering_bytes / 8) {` | serials | one operation | derived from `ordering_bytes` | `ObjectLimitExceeded` (`filesystem/update.rs:465-473`) | UNVERIFIED at the 8,388,608 default [sweep] | derived bound |
| ordering tiers and buffers | `runs.rs:34` `MAXIMUM_LEVELS = 32`; `runs.rs:30` merge buffer 16 KiB; spill charge doubles the row bytes (`runs.rs:125`) | tiers, bytes | one operation | resource fields | `ResourceUnavailable("ordering tiers")` (`runs.rs:212-216`) | `tests/filesystem_ordering.rs` [sweep] | enforced policy |
| run-name recovery scan | `backing.rs:145` `const SCAN_LIMIT: usize = 4_096;` | entries | run naming | constant | beyond the bound the scan simply sees fewer tokens (not an error) | `tests/filesystem_ordering.rs:870,911,924` [sweep] | enforced policy |
| mapping read waves | `content/file/mapping/read.rs:24,26,33` `READ_WAVE_OBJECTS = 32` / `READ_WAVE_BYTES = READ_WAVE_OBJECTS * cdc::MAXIMUM_CHUNK_BYTES` (= 1 MiB) / `READ_NAVIGATION_WAVE = 32` | objects, bytes | one file read | constants | flush at the bound; `MappingDepthExceeded` past `MAX_LEVEL` (`read.rs:226-228`) | 24 MiB + 1 file, max batch == 32 (`tests/file_read.rs:290-294`) [sweep] | derived bound |
| sorted-tree read batch | `content/filesystem/sorted/page.rs:25` `BATCH_CHILDREN = 32`; budget refusal before allocation (`sorted/budget.rs:38-49`) | children, bytes | one sorted read | constant + budget | batch narrowed to the budget, else a point read | `tests/filesystem_bounds.rs:312` [sweep] | derived bound |
| edit stream | `content/file/edit/input.rs:22` `MAXIMUM_EDITS_PER_OPERATION = 4_096` (`:100-103` refuses); `file/edit/tree.rs:31` `EDIT_DEFERRED_LIMIT = 8 MiB - 1` (`:337-344`), `:33` `DEFERRED_OBJECT_OVERHEAD = 128`; `file/edit/compare.rs:19` `COMPARE_WINDOW_BYTES = 64 KiB` | edits, bytes | one edit operation | constants | `ObjectLimitExceeded` / `BoundedCapacityExceeded` | edits two-sided (`tests/edit_bounds.rs:588-593`); the deferred boundary is analysed but not exercised (`:751-834`) [sweep] | enforced policy |
| save-side caches | `storage/policy.rs:113` `DEPENDENCY_PACK_CACHE_BYTES = 4 MiB`; `:121` `POOLED_VALUE_CACHE_BYTES = 512 KiB`; `:142` `METADATA_INDEX_VALUES = 131_072`; `:131` `METADATA_RECORD_LIMIT = 8_192`; `:135` `INODE_LEAF_LIMIT = 8_192`; `cas/owner.rs:36` `PENDING_VALUES_LIMIT = POOLED_LEAF_ROWS_LIMIT` | bytes, entries | one save | constants | each cache is dropped wholesale past its bound; the value memo fails closed (`owner.rs:513-515`) | index window 1,312 groups (`tests/metadata_window.rs:160`), pooled record 8,144 bytes (`tests/metadata_pool.rs:191`) [sweep]; `DEPENDENCY_PACK_CACHE_BYTES` untested [sweep] | enforced policy |
| selection caches | `storage/encoding/delta/candidates.rs:15-17` `INDEX_BYTES = 128 KiB` / `SLOTS = 1024` / `REFERENCES = 8_192` with compile-time size assertions (`:36-44`); `encoding/delta/select.rs:27` `DEPTH_CACHE_ENTRIES = 4_096`; `encoding/pool/delta.rs:18` `PROGRAM_LIMIT = 64 KiB` | bytes, entries | one save | constants | fixed size, no growth | no test references these constants [sweep] | enforced policy |
| timing report | `telemetry/src/timer/recording.rs:14,17,20` `MAX_NODES = 1_024` / `MAX_DEPTH = 32` / `MAX_LABEL_BYTES = 128` | nodes, levels, bytes | one report | constants | a child that does not fit is omitted and the node is marked incomplete (`report.rs:104-118`) | telemetry tests, C2 `tests/timing.rs:278-279` [sweep] | enforced policy |
| readers and writer ownership | one writer: `sqlite/write.rs:51-55` `BEGIN IMMEDIATE` + zero busy timeout + `OwnershipUnavailable` (`connection.rs:66-72`); readers are **unbounded in number** and each opens its own connection (`cas/store.rs:209`, `:247`) | owners | whole Store | none for reader count | a second writer is refused, never queued | `tests/cas_reuse.rs` [sweep] | **future-owner responsibility** (no reader ceiling) |

### 4.9 Defects and coverage gaps found while building these tables

1. **Two values for one nominal scratch ceiling.** `content/filesystem/limits.rs:30` `pub const DEFAULT_OPERATION_SCRATCH_BYTES: usize = 4 * 1024 * 1024 - 1;` versus `content/filesystem/sorted/page.rs:23` `pub const MAXIMUM_SCRATCH_BYTES: usize = 4 * 1024 * 1024;`. They feed different defaults: `sorted/budget.rs:33-34` returns the −1 value from `default_limit()`, while `filesystem/input.rs:73` sets `FilesystemResources::default().scratch_bytes` to the 4 MiB constant.
2. **Attribute value bound is only half-tested.** `tests/filesystem_failure.rs:562` says `// Exactly at the bound is still an accepted value.` but `:563` is `let at_limit = vec![0x5a_u8; 4096];` — 4096 bytes, not `MAXIMUM_ATTRIBUTE_VALUE_BYTES` (1 MiB).
3. **Inode serial bound is only half-tested.** `tests/filesystem_failure.rs:495-543` verifies only the over-bound case; acceptance at `MAXIMUM_INODE_SERIAL` is untested [sweep].
4. **Symlink bound is only half-tested.** `tests/filesystem_read.rs:259` verifies 4097 refused; 4096 accepted is untested [sweep].
5. **`BATCH_CANONICAL_BYTES_LIMIT` is not a hard cap.** `cas/batch.rs:50-58`: the byte check runs only when the batch is non-empty, so one object larger than the whole 512 KiB bound is accepted.
6. **No revision or payload reclamation.** The only `DELETE`s are failed-save cleanup (`sqlite/cleanup.rs:65,120`); every acknowledged save's objects and packs remain. Nothing in C1 or C2 declares a retention ceiling — this is a capacity limit with no owner at all.
7. **Row ordinals are trusted to the SQL `CHECK`.** `sqlite/write.rs:115-116` casts `usize` to `i64` with no local range check.
8. **Reader count is unbounded** (`cas/store.rs:209,247` opens a connection per call, no pool, no limit) while writer exclusivity is enforced.
9. **Coverage gaps (no test found)** [sweep]: `MAXIMUM_PATH_BYTES`; `MAXIMUM_PATH_COMPONENTS`; `GROUP_COUNT_LIMIT`; `RECORD_COUNT_LIMIT`; `DEPENDENCY_PACK_CACHE_BYTES`; `DEFAULT_BACKING_CAPACITY_BYTES`; `MAXIMUM_LEVELS`; any tree actually 31 levels deep (both filesystem and extent trees); `TRANSACTION_ROW_LIMIT`; `TRANSACTION_CANONICAL_BYTES_LIMIT`; `CLEANUP_PAGE_ROWS`; `ENCODE_WORKSPACE_BYTES`; `DECODE_WORKSPACE_BYTES`; `METADATA_DECODED_WORK_LIMIT`; `INDEX_BYTES`; `DEPTH_CACHE_ENTRIES`; `PROGRAM_LIMIT`; `COMPARE_WINDOW_BYTES`.

### 4.10 Largest inputs actually exercised (measured coverage)

- File/logical bytes: 24 MiB + 1 read (`tests/file_read.rs:265-267`); 24 MiB edit base (`tests/edit_localized.rs:276,423`); 8 MiB (`tests/edit_bounds.rs:772`); 6 MiB + 1 end-to-end through a real Store (`storage/tests/core_pipeline.rs:261-268`, read by me); 5 MiB (`storage/tests/pack_locator.rs:165`); 4 MiB (`storage/tests/persistence_failure.rs:88`, `tests/cas_reuse.rs:100`) [sweep unless noted].
- Canonical object: exactly 16 MiB accepted and 16 MiB + 1 refused (`tests/object_identity.rs:323,344`); 8 MiB field accepted and +1 refused (`:256,266`); 16 MiB + 4096 singleton pack (`storage/tests/physical_formats.rs:77`).
- Dependency chains: 16 edges in one chain (`storage/tests/delta_chains.rs:110`); 8 pooled edges at the cap (`storage/tests/metadata_chain.rs:124-144`).
- Counts: 4,000-entry directory (`tests/filesystem_bounds.rs:304`); 600 released files (`:340`); 1,500 sorted entries (`tests/filesystem_sorted.rs:105`); 4,097 attribute keys (`tests/filesystem_attributes.rs:603`); 1,312 pooled groups (`tests/metadata_window.rs:160`); 4,096-object demand (`storage/tests/cas_reuse.rs:297`); 100-row inode leaf (`storage/tests/metadata_pool.rs:174-191`).
- Deepest tree built anywhere: 2 mapping levels (`tests/file_read.rs:219-227`). No test builds a 31-level tree, so every depth-derived capacity figure in this section is a **derived bound, UNVERIFIED**.

### 4.11 Limits that are decisions, not accidents

Two absences are deliberate and should not be read as defects: (a) there is no cap on the number of `SaveOperation`s or `Store`s — the Store is a path plus policy and the caller owns lifetime; (b) the per-object canonical ceiling (16 MiB) is larger than any declared wave budget on purpose, so that the singleton path can carry one maximum object (`storage/policy.rs:88-90`); the consequence — a wave's total bytes are unbounded by any single constant — is recorded in section 2.1.

## 5. Placement assessment

### 5.1 A - one process/host: app or FUSE adapter -> C1 -> C2 -> local SQLite

**READY TODAY.** Minimum work: none beyond wiring.

- C1 states and honours the boundary: `layerfs-content/src/lib.rs:4-7` - "It opens no database, pack or file: construction emits finalized canonical objects to a caller-supplied bounded consumer, and reads ask a caller-supplied authenticated provider for canonical bytes." The only file I/O in C1 is inside the opt-in `FileBacking` (`backing.rs:146,220,249,349`).
- C2 is an in-process library over one SQLite file: `connection.rs:17-26`, opened read-write without create when not creating (:18-22).
- The composition is real and already used: `storage/tests/core_pipeline.rs:21-43` (construct -> `SaveHandoff` -> `finish`) and :45-61 (read back through `StoreProvider`), with a reopen proof at :270-286.
- The embedded profile is fixed per connection and verified: MEMORY journal with result check (`connection.rs:32-36`), `synchronous = OFF` and `temp_store = MEMORY` (:37), `foreign_keys = ON` with read-back check (:38-42), zero busy timeout (:43).
- Durability is not claimed: `lib.rs:9-13`.

Adapter caveats even here: (i) a `TimingScope` is required at every entry point and is !Send (`scope.rs:39`); (ii) an in-process FUSE callback that reads while a save is open uses a second connection (`store.rs:209`), which is safe only because the watermark hides unpublished packs (`store.rs:211-213`).

### 5.2 B - Workspace elsewhere, core together: [host/container/remote Workspace or FUSE] -> bounded operation data -> [C1+C2+storage]

**ADAPTER WORK REQUIRED.** No core blocker found; the work is a bounded request/response protocol plus an ordering-storage decision.

1. **Bound the request.** C1 declares no ceiling on `FilesystemInput`'s slices (`input.rs:129-144`; `check` at :157-215 validates order, uniqueness and serial range only). The remote adapter must impose and enforce its own byte/count bound. `maximum_touched_serials()` derives from `ordering_bytes` (`input.rs:90-95`) and bounds the engine's touched set, not the input.
2. **Bound the response.** Output is one object per `FinalizedConsumer::accept` (`output.rs:165`), each <= 16 MiB by the canonical envelope (`content/policy.rs:203`; `storage/policy.rs:61`; `schema.sql:56`). There is **no per-operation output ceiling** in C1; the consumer is the bound (C2's is 512 objects / 512 KiB per wave, `storage/policy.rs:65-67`).
3. **Decide where ordering runs.** Either the core keeps ordering in its own container (pass `None` or a local `FileBacking`, `update.rs:70`, `backing.rs:169`) or the remote caller implements `OrderingBacking` over its own storage - the trait is designed for that ("The reducer's runs live wherever the caller puts them", `backing.rs:3-5`). Passing `None` makes the first spill fail explicitly (`runs.rs:58-61`) - acceptable below the in-memory ordering budget, not a fallback.
4. **Reads.** The service side needs an `AuthenticatedObjects` over its own Store; `StoreProvider` already is one (`cas/provider.rs:44-68`).
5. **Timing.** A caller spanning processes cannot share a live scope tree; the composition is `TimingScope::attach` with a completed report (`scope.rs:106-110`).

Explicit-failure requirement: satisfied today for the pieces that exist (journal mode `connection.rs:34-36`, foreign keys :39-42, schema identity `schema.rs:138-147`, STRICT/CHECK shapes `schema.rs:156-205`). It is **not** satisfied for an oversize remote request, because no such check exists - that is the adapter's obligation and it must be a refusal, not a truncation.

### 5.3 C - C1 and storage separated: [C1 caller] <-- bounded output/read batches --> [C2 service + storage]

**ADAPTER WORK REQUIRED** for a cross-process split; the in-process version of exactly this split already exists and is tested.

Already a clean, bounded seam:

- **Output direction.** C1 emits through `FinalizedConsumer` (`output.rs:163-166`); `SaveHandoff` is the product adapter (`store.rs:504-548`). Batches: 512 objects / 512 KiB (`policy.rs:65-67`), drained by `PendingBatch::push` when a bound is reached (`batch.rs:48-58`). `SaveOperation::accept` flushes synchronously (`store.rs:295-299`), so a transport-backed consumer must be **blocking** - the trait has no async form and no `Send` bound.
- **Read direction.** C1 asks for canonical bytes (`access.rs:31`); `StoreProvider` answers through `Store::read_batch` (`provider.rs:35-41`), capped at 4096 ids (`store.rs:207`) with identity re-verified on the C2 side (`cas/read.rs:91-93`: `if ObjectId::for_bytes(&canonical) != *id`). The count ceiling exists; a **byte** ceiling per wave does not (see section 4).
- **Both directions carry only `ObjectId` + canonical bytes**, never paths: `ObjectId` is a 32-byte digest (`object/id.rs:20`), `DIGEST_BYTES = 32` (:13), domain-separated by `OBJECT_DOMAIN` (:16).

Minimum adapter work:

1. A blocking `AuthenticatedObjects` client (demand order, exact cardinality, refusal on unknown) and a blocking `FinalizedConsumer` client (ownership transfer, refusal by error).
2. Its own per-wave byte ceiling, since neither crate declares one.
3. An identity story: C1 does **not** re-hash provider answers (no `for_bytes` comparison in any C1 read path; see 2.1), so a cross-process provider is trusted. An adapter that wants end-to-end proof must re-hash in the client and compare with the requested `ObjectId` - `ObjectId::for_bytes` is public (`object/id.rs:24`).
4. Timing attachment, because a remote call cannot share a live scope tree (`scope.rs:106-110`).

### 5.4 Replacing C2's SQL backend with remote SQL (distinct from 5.3)

**CORE BLOCKER as written; it requires C2 implementation changes, not adapter work.** Moving the whole embedded C2 *service* (5.3) is a smaller change than replacing its SQL backend.

| Aspect | Code | Why remote SQL breaks it |
|---|---|---|
| Open | `connection.rs:17-23`: `pub fn open(path: &Path, create: bool)` -> `Connection::open_with_flags(path, flags)` | the only accepted handle is a local path; there is no DSN/URL abstraction and no connection trait in `lib.rs:21-26` |
| Session profile | `connection.rs:33` `PRAGMA journal_mode = MEMORY`, :37 `PRAGMA synchronous = OFF; PRAGMA temp_store = MEMORY;`, :38 `PRAGMA foreign_keys = ON;`, :43 `connection.busy_timeout(Duration::ZERO)?` | per-connection session settings; the code **reads back and enforces** journal mode (:34-36) and foreign keys (:39-42), so an endpoint that cannot report them fails immediately with `Integrity("journal mode")`/`Integrity("foreign key enforcement")` - explicit failure (correct), but "remote SQL" must be SQLite-compatible and expose these PRAGMAs |
| Schema creation | `schema.rs:15` `include_str!("../../sql/schema.sql")`, executed at :69; the DDL uses `STRICT` (`schema.sql:32,37,47,66`), `WITHOUT ROWID` (:66), `length()` CHECKs (:36,44,50), a partial index (:71-72) and `PRAGMA application_id/user_version` (:10-11) | a non-SQLite server cannot execute or store this DDL |
| Schema validation | `schema.rs:99,108` query `sqlite_master`; :163,181 read `SELECT sql FROM sqlite_master`; :191 requires the DDL text to contain `STRICT`; :195 `PRAGMA table_info({table})`; :156-157 requires the text `CHECK (object_role BETWEEN 1 AND 13)` | introspection is SQLite-catalogue specific and text-based |
| Transaction | `write.rs:51-55` `connection.execute_batch("BEGIN IMMEDIATE")` once; `write.rs:58-64` `COMMIT` (failure => `UnknownOutcome`); `owner.rs:840-857` commits and re-acquires when 8191 rows / 4 MiB-1 are reached | a stateless HTTP-style endpoint cannot hold one transaction across many statements/requests; `IMMEDIATE` lock semantics and the zero-busy-timeout refusal (`connection.rs:66-72`) are SQLite-specific |
| Statement cache | `prepare_cached` at `lookup.rs:68,140`, `pool.rs:159`, `cleanup.rs:46,101` | per-connection cache, meaningless remotely |
| Query shapes | paged `IN (...)` with <=128 placeholders (`lookup.rs:59-68,133-140`, `policy.rs:63`); one pack BLOB by PK (`lookup.rs:152-163`); `SELECT COALESCE(MAX(pack_id), 0) FROM object_packs` (:166-172); an ordinal `MAX(first_ordinal + count)` subquery (`pool.rs:44-55`) | every row's payload is a SQL `BLOB` |
| Payload placement | `schema.sql:34-37` `CREATE TABLE object_packs (pack_id INTEGER PRIMARY KEY ..., data BLOB NOT NULL CHECK (length(data) >= 32))`; written by `write.rs:76-85` `INSERT INTO object_packs (pack_id, data) VALUES (?1, ?2)`; appended by `write.rs:88-97` `UPDATE object_packs SET data = ?2 WHERE pack_id = ?1` | **all object bytes live in the database**; every append rewrites the whole pack BLOB (<= `PACK_LIMIT = 256 * 1024`, `policy.rs:55`; a singleton pack <= `SINGLETON_PACK_LIMIT = CANONICAL_LIMIT + 4 * 1024` = 16 MiB + 4 KiB, `policy.rs:88-90`). Remote SQL would ship payload on every write and again on every read (`lookup::pack_bytes`). Splitting payload into an object store means a new locator->bytes path - a **C2 implementation change** |
| Visibility | the watermark is a single-row UPDATE in the final transaction (`schema.rs:273-283`, `owner.rs:915`), read once per read wave (`store.rs:213`) and applied as `pack_id <= ceiling` in both queries (`lookup.rs:62,134`) | correct only if all readers see the same committed row; a read replica or eventually-consistent endpoint would break `UninspectedState` detection (`owner.rs:179-186`) |
| Storage limits actually in force | SQLite signed `INTEGER` for `pack_id`, `group_number < 256`, `record_number < 8191` (`schema.sql:35,64,65`); `canonical_length <= 16777216` (:56). No `max_page_count`/`page_size` pragma is set anywhere (only journal/synchronous/temp_store/foreign_keys/busy_timeout, `connection.rs:33-43`), and no build-time SQLite limit is declared in `storage/Cargo.toml` | the DB's own build limits are whatever the linked `libsqlite3-sys` provides - **UNVERIFIED** here (no build was run); a remote server would impose its own, possibly smaller, row/BLOB limits |

Minimum C2 changes required for remote SQL (each a code change, not a shim): (1) a connection abstraction that accepts a non-path target; (2) removal or server-side equivalents of the verified session PRAGMAs; (3) a schema-identity check that does not depend on `sqlite_master` text and `PRAGMA table_info`; (4) transaction affinity across requests, or a redesign of the prepare/flush/commit sequence; (5) an explicit per-statement byte bound for pack BLOBs; (6) pack payload moved out of the table (or a server with the same BLOB limits). Until then an unsupported remote backend fails explicitly at `connection.rs:34-36`/:39-42 - correct behaviour, not a silent fallback.

---

## 6. Smallest real local composition using only current public APIs

**Answer: `core/crates/layerfs-storage/tests/core_pipeline.rs`, the `pipeline` function at :21-43, with `read_back` at :45-61.** It is the smallest source in the tree that runs real C1 construction into real C2 storage and reads it back through the public bridge.

```rust
21|fn pipeline(store: &Store, bytes: &[u8]) -> Result<FileResult, StorageError> {
22|    let policy = ConstructionPolicy::frozen_default();
23|    disabled(|scope| {
24|        let mut operation = store.begin_save(scope.child("storage.begin"))?;
25|        let mut handoff = SaveHandoff::new(&mut operation);
26|        let constructed = construct_stream(
27|            policy,
28|            &policy.capacities(),
29|            bytes,
30|            &mut handoff,
31|            scope.child("content"),
32|        );
33|        if let Some(failure) = handoff.take_failure() {
34|            return Err(failure);
35|        }
36|        let constructed = constructed?;
37|        operation.finish(scope.child("storage.finish"))?;
38|        Ok(FileResult {
39|            root: constructed.root,
40|            logical_len: constructed.logical_len,
41|        })
42|    })
43|}
44|
45|fn read_back(store: &Store, root: ObjectId) -> Vec<u8> {
46|    let (values, _) = disabled(|scope| store.read_batch(&[root], scope.child("storage.read")))
47|        .expect("root read");
48|    let canonical = values.into_iter().next().expect("one root");
49|    let mut out = Vec::new();
50|    disabled(|scope| {
51|        read_all(
52|            &StoreProvider::new(store),
53|            root,
54|            &mut out,
55|            scope.child("content.read"),
56|        )
57|    })
58|    .expect("logical read");
59|    let _ = canonical;
60|    out
61|}
```

Store creation in the same test's support module (`layerfs-storage/tests/support/mod.rs:202-203`):

```rust
202|disabled(|scope| Store::create(path, StoragePolicy::frozen_default(), scope.child("store")))
203|    .expect("store create")
```

**Source accuracy against today's signatures: ACCURATE.** Verified call by call:

- `construct_stream(policy, &policy.capacities(), bytes, &mut handoff, scope.child("content"))` matches `layerfs-content/src/file/content.rs:190` `pub fn construct_stream<R: Read>(...)` with `&[u8]: Read` and `&mut SaveHandoff: FinalizedConsumer`; `ConstructionPolicy::capacities` is `content/policy.rs:141`.
- `store.begin_save(scope.child("storage.begin"))` matches `store.rs:185`; `scope.child` requires an `Active` handle, which `disabled(|scope| ...)` supplies (`scope.rs:194-205`).
- `SaveHandoff::new(&mut operation)` (`store.rs:516`) and `take_failure` (:529) match; the failure-routing order at :33-36 is the documented one (`store.rs:504-508`).
- `operation.finish(scope.child("storage.finish"))` matches `store.rs:314` `pub fn finish(mut self, scope: TimingScope<'_>) -> StorageResult<SaveOutcome>`.
- `store.read_batch(&[root], scope.child("storage.read"))` matches `store.rs:202`; `StoreProvider::new(store)` matches `provider.rs:25`; `read_all(&dyn AuthenticatedObjects, ObjectId, &mut dyn Write, TimingScope)` matches `file/read.rs:50-55`.
- The negative control at :209-250 (`FailingAfter`, database-size check) also uses only public API.

Limitations of this example as "the composition": it is a *test*, not an application; it runs C1 and C2 in one process with `StoragePolicy::frozen_default()`, and it never exercises `FilesystemInput`/ordering backing (that composition lives at `layerfs-storage/tests/filesystem_pipeline.rs:690-760`, which is larger). It is source-accurate and is the smallest end-to-end C1->C2 proof in the tree.

---

## Appendix A - complete public item inventory

Every pub fn/struct/enum/trait/type/const/static/use/union declaration in every .rs file under the three crates' src/ trees, with its first line number. Extraction is mechanical (declaration text is joined across up to six continuation lines and truncated at 220 characters with an ellipsis when longer). Each file is labelled either [public module path] - the file is reachable from lib.rs through pub mod declarations, so its pub items are nameable by path - or [private module] - the file is NOT reachable through pub mod, so only items re-exported by a public module, or pub items inside an inherent impl of a public type, are part of the crate API. pub(crate) and other restricted visibilities are excluded.

### A.1 `layerfs-content`

**core/crates/layerfs-content/src/error.rs** [public module path] - 2 pub items

```text
11: pub enum ContentError {
219: pub type ContentResult<T> = Result<T, ContentError>;
```

**core/crates/layerfs-content/src/lib.rs** [public module path] - 7 pub items

```text
25: pub use error::{ContentError, ContentResult};
26: pub use file::{
31: pub use filesystem::inode::InodeChange;
32: pub use filesystem::{
37: pub use object::inode_leaf;
38: pub use object::{
43: pub use policy::{
```

**core/crates/layerfs-content/src/policy.rs** [public module path] - 24 pub items

```text
14: pub const DEFAULT_SMALL_FILE_THRESHOLD_BYTES: u64 = 131_072;
16: pub const MINIMUM_SMALL_FILE_THRESHOLD_BYTES: u64 = 131_072;
18: pub const MAXIMUM_SMALL_FILE_THRESHOLD_BYTES: u64 = 1_048_576;
20: pub const DEFAULT_WHOLE_FILE_DELTA_MAX_DEPTH: u8 = 8;
22: pub const DEFAULT_CHUNK_DELTA_MAX_DEPTH: u8 = 4;
24: pub const MAXIMUM_DELTA_MAX_DEPTH: u8 = 50;
31: pub const DEFAULT_WHOLE_FILE_FRAME_LIMIT: usize = 135_168;
37: pub const fn conservative_frame_bound(raw: usize) -> usize {
43: pub struct ConstructionPolicy {
51: pub const fn frozen_default() -> Self {
60: pub const fn new( small_file_threshold_bytes: u64, whole_file_delta_max_depth: u8, chunk_delta_max_depth: u8, ) -> Self {
81: pub const fn validated(self) -> ContentResult<Self> {
105: pub const fn small_file_threshold_bytes(self) -> u64 {
110: pub const fn whole_file_delta_max_depth(self) -> u8 {
115: pub const fn chunk_delta_max_depth(self) -> u8 {
120: pub const fn representation(self, logical_len: u64) -> Representation {
141: pub const fn capacities(self) -> ConstructionCapacities {
166: pub const fn whole_file_window_log(self) -> i32 {
183: pub enum Representation {
196: pub const WHOLE_FILE_CANONICAL_OVERHEAD: usize = 23;
203: pub const MAX_CANONICAL_OBJECT_BYTES: usize = 16 * 1024 * 1024;
209: pub const MAX_OBJECT_FIELD_BYTES: usize = 8 * 1024 * 1024;
212: pub const MAX_MAPPING_ENTRIES: usize = 128;
218: pub struct ConstructionCapacities {
```

**core/crates/layerfs-content/src/file/content.rs** [private module] - 10 pub items

```text
29: pub struct ConstructedFile {
44: pub enum FileContent {
56: pub const fn logical_len(self) -> u64 {
65: pub fn whole_file_payload(canonical: &[u8]) -> ContentResult<Option<&[u8]>> {
80: pub fn encode_whole_file( capacities: &ConstructionCapacities, bytes: &[u8], ) -> ContentResult<Vec<u8>> {
110: pub fn encode_whole_file_payload(raw: &[u8]) -> ContentResult<Vec<u8>> {
119: pub fn inspect( reader: &dyn AuthenticatedObjects, root: ObjectId, scope: TimingScope<'_>, ) -> ContentResult<FileContent> {
133: pub fn classify(canonical: &[u8]) -> ContentResult<FileContent> {
143: pub fn construct_bytes( policy: ConstructionPolicy, capacities: &ConstructionCapacities, bytes: &[u8], consumer: &mut dyn FinalizedConsumer, scope: TimingScope<'_>, ) -> ContentResult<ConstructedFile> { ...
190: pub fn construct_stream<R: Read>( policy: ConstructionPolicy, capacities: &ConstructionCapacities, mut source: R, consumer: &mut dyn FinalizedConsumer, scope: TimingScope<'_>, ) -> ContentResult<ConstructedFile> { ...
```

**core/crates/layerfs-content/src/file/mod.rs** [public module path] - 5 pub items

```text
13: pub use content::{
17: pub use edit::{
21: pub use mapping::{ExtentBuilder, ReadCounters};
22: pub use read::{read_all, read_all_bounded, read_range};
23: pub use view::FileView;
```

**core/crates/layerfs-content/src/file/read.rs** [private module] - 3 pub items

```text
19: pub fn read_all_bounded( reader: &dyn AuthenticatedObjects, root: ObjectId, maximum: u64, sink: &mut dyn Write, scope: TimingScope<'_>, ) -> ContentResult<ReadCounters> { ...
50: pub fn read_all( reader: &dyn AuthenticatedObjects, root: ObjectId, sink: &mut dyn Write, scope: TimingScope<'_>, ) -> ContentResult<ReadCounters> {
60: pub fn read_range( reader: &dyn AuthenticatedObjects, root: ObjectId, range: Range<u64>, sink: &mut dyn Write, scope: TimingScope<'_>, ) -> ContentResult<ReadCounters> { ...
```

**core/crates/layerfs-content/src/file/view.rs** [private module] - 8 pub items

```text
20: pub struct FileView {
28: pub fn open( reader: &dyn AuthenticatedObjects, root: ObjectId, scope: TimingScope<'_>, ) -> ContentResult<Self> {
47: pub const fn root(&self) -> ObjectId {
52: pub const fn logical_len(&self) -> u64 {
57: pub const fn content(&self) -> FileContent {
62: pub fn file_state(&self) -> ContentResult<Option<FileState>> {
70: pub fn whole_file_bytes(&self) -> ContentResult<Option<&[u8]>> {
85: pub fn read_range( &self, reader: &dyn AuthenticatedObjects, range: Range<u64>, sink: &mut dyn Write, scope: &TimingScope<'_, Active>, ) -> ContentResult<()> { ...
```

**core/crates/layerfs-content/src/file/cdc/gear.rs** [private module] - 11 pub items

```text
13: pub const MINIMUM_CHUNK_BYTES: usize = 8_192;
15: pub const TARGET_CHUNK_BYTES: usize = 16_384;
17: pub const MAXIMUM_CHUNK_BYTES: usize = 32_768;
19: pub const NORMALIZATION_SHIFT: u32 = 2;
21: pub const PROFILE_SEED: u64 = 0;
29: pub fn profile_id() -> [u8; 32] {
53: pub struct CdcCounters {
62: pub struct FastCdc;
66: pub const fn new() -> Self {
75: pub fn scan<R: Read, F: FnMut(&[u8]) -> ContentResult<()>>( self, mut reader: R, mut on_chunk: F, ) -> ContentResult<CdcCounters> {
281: pub const GEAR: [u64; 256] = [ 0x3b5d3c7d207e37dc, 0x784d68ba91123086, 0xcd52880f882e7298, 0xeacf8e4e19fdcca7, 0xc31f385dfbd1632b, 0x1d5f27001e25abe6, ...
```

**core/crates/layerfs-content/src/file/cdc/mod.rs** [public module path] - 1 pub items

```text
7: pub use gear::{
```

**core/crates/layerfs-content/src/file/edit/apply.rs** [private module] - 2 pub items

```text
28: pub struct EditRequest<'a> {
38: pub fn apply_edits( policy: ConstructionPolicy, capacities: &ConstructionCapacities, reader: &dyn AuthenticatedObjects, request: EditRequest<'_>, consumer: &mut dyn FinalizedConsumer, scope: TimingScope<'_>, ...
```

**core/crates/layerfs-content/src/file/edit/compare.rs** [private module] - 3 pub items

```text
19: pub const COMPARE_WINDOW_BYTES: usize = 64 * 1024;
23: pub enum NoOpVerdict {
31: pub fn compare_replacements( view: &FileView, reader: &dyn AuthenticatedObjects, stream: &EditStream, source: &dyn EditSource, scope: TimingScope<'_>, ) -> ContentResult<NoOpVerdict> { ...
```

**core/crates/layerfs-content/src/file/edit/concat.rs** [private module] - 1 pub items

```text
12: pub fn coalesce_adjacent(previous: ExtentSlice, next: ExtentSlice) -> Option<ExtentSlice> {
```

**core/crates/layerfs-content/src/file/edit/finish.rs** [private module] - 3 pub items

```text
13: pub struct EmittedRoot {
25: pub fn emit_empty_representation( consumer: &mut dyn FinalizedConsumer, ) -> ContentResult<EmittedRoot> {
40: pub fn emit_file_state( consumer: &mut dyn FinalizedConsumer, build: MappingBuild, ) -> ContentResult<EmittedRoot> {
```

**core/crates/layerfs-content/src/file/edit/input.rs** [private module] - 30 pub items

```text
22: pub const MAXIMUM_EDITS_PER_OPERATION: usize = 4_096;
26: pub struct Edit {
34: pub const fn new(start: u64, end: u64, replacement_len: u64) -> Self {
43: pub const fn overwrite(start: u64, end: u64) -> Self {
48: pub const fn insert(at: u64, replacement_len: u64) -> Self {
53: pub const fn delete(start: u64, end: u64) -> Self {
58: pub const fn start(self) -> u64 {
63: pub const fn end(self) -> u64 {
68: pub const fn replacement_len(self) -> u64 {
73: pub const fn removed_len(self) -> u64 {
78: pub fn apply_len(self, before: u64) -> ContentResult<u64> {
91: pub struct EditStream {
99: pub fn new(base_len: u64, edits: Vec<Edit>) -> ContentResult<Self> {
142: pub const fn base_len(&self) -> u64 {
147: pub const fn final_len(&self) -> u64 {
152: pub fn len(&self) -> usize {
157: pub fn is_empty(&self) -> bool {
162: pub fn edits(&self) -> &[Edit] {
174: pub trait EditSource {
184: pub struct Replacements {
190: pub fn new() -> Self {
195: pub fn push(&mut self, bytes: Vec<u8>) -> usize {
224: pub struct ReplacementReader<'a> {
233: pub fn new(source: &'a dyn EditSource, index: usize, length: u64) -> Self {
243: pub const fn remaining(&self) -> u64 {
274: pub enum Segment {
298: pub struct Plan<'a> {
308: pub fn new(stream: &'a EditStream) -> Self {
319: pub fn advance(&mut self) -> ContentResult<Option<Segment>> {
387: pub const fn base_end(&self) -> u64 {
```

**core/crates/layerfs-content/src/file/edit/mod.rs** [public module path] - 8 pub items

```text
13: pub use apply::{apply_edits, EditRequest};
14: pub use compare::{compare_replacements, NoOpVerdict, COMPARE_WINDOW_BYTES};
15: pub use concat::coalesce_adjacent;
16: pub use finish::{emit_empty_representation, EmittedRoot};
17: pub use input::{Edit, EditSource, EditStream, ReplacementReader, MAXIMUM_EDITS_PER_OPERATION};
18: pub use input::{Plan, Replacements, Segment};
19: pub use split::slice_of;
20: pub use tree::{EditCounters, EditObjects, EDIT_DEFERRED_LIMIT};
```

**core/crates/layerfs-content/src/file/edit/split.rs** [private module] - 1 pub items

```text
12: pub fn slice_of( extent: ExtentSlice, origin: u64, from: u64, to: u64, ) -> ContentResult<ExtentSlice> {
```

**core/crates/layerfs-content/src/file/edit/tree.rs** [private module] - 25 pub items

```text
31: pub const EDIT_DEFERRED_LIMIT: usize = 8 * 1024 * 1024 - 1;
37: pub struct EditCounters {
109: pub struct EditObjects<'a> {
125: pub fn new( reader: &'a dyn AuthenticatedObjects, consumer: &'a mut dyn FinalizedConsumer, ) -> Self {
143: pub const fn counters(&self) -> EditCounters {
148: pub const fn charged_bytes(&self) -> usize {
153: pub fn load_node(&mut self, summary: NodeSummary, root: bool) -> ContentResult<ExtentNode> {
177: pub fn hold_node(&mut self, node: &ExtentNode) -> ContentResult<NodeSummary> {
242: pub fn settle(&mut self, live: NodeSummary) {
351: pub fn publish_payload(&mut self, object: FinalizedObject) -> ContentResult<ObjectId> {
365: pub fn finish(&mut self, mapping: NodeSummary) -> ContentResult<ObjectId> {
383: pub fn commit(&mut self, root: NodeSummary) -> ContentResult<ObjectId> {
466: pub struct DeferredSink<'a, 'b> {
472: pub fn new(objects: &'b mut EditObjects<'a>) -> Self {
493: pub fn child_summaries(children: &[ChildDescriptor], level: u8) -> ContentResult<Vec<NodeSummary>> {
522: pub fn coalesce(extents: &mut Vec<ExtentSlice>) -> ContentResult<()> {
542: pub fn discard(objects: &mut EditObjects<'_>, summary: Option<NodeSummary>) {
552: pub fn split( objects: &mut EditObjects<'_>, root: NodeSummary, offset: u64, root_context: bool, ) -> ContentResult<(Option<NodeSummary>, Option<NodeSummary>)> {
638: pub fn concat_optional( objects: &mut EditObjects<'_>, left: Option<NodeSummary>, right: Option<NodeSummary>, ) -> ContentResult<Option<NodeSummary>> {
650: pub fn concat( objects: &mut EditObjects<'_>, left: NodeSummary, right: NodeSummary, ) -> ContentResult<NodeSummary> {
808: pub fn root_from_extents( objects: &mut EditObjects<'_>, extents: Vec<ExtentSlice>, ) -> ContentResult<NodeSummary> {
824: pub fn root_from_children( objects: &mut EditObjects<'_>, children: Vec<NodeSummary>, ) -> ContentResult<Option<NodeSummary>> {
844: pub fn emit_leaf( objects: &mut EditObjects<'_>, extents: Vec<ExtentSlice>, ) -> ContentResult<NodeSummary> {
859: pub fn emit_branch( objects: &mut EditObjects<'_>, children: Vec<NodeSummary>, ) -> ContentResult<NodeSummary> {
898: pub fn read_state( reader: &dyn AuthenticatedObjects, root: ObjectId, ) -> ContentResult<(FileState, NodeSummary)> {
```

**core/crates/layerfs-content/src/file/mapping/build.rs** [private module] - 15 pub items

```text
29: pub struct MappingBuild {
65: pub struct ExtentBuilder {
74: pub fn new(capacities: &ConstructionCapacities) -> Self {
85: pub const fn logical_len(&self) -> u64 {
90: pub const fn extent_count(&self) -> u64 {
95: pub const fn chunks(&self) -> u64 {
100: pub const fn nodes(&self) -> u64 {
105: pub const fn peak_pending(&self) -> usize {
110: pub fn pending_entries(&self) -> usize {
119: pub fn push_chunk( &mut self, raw: &[u8], predecessor: Option<ObjectId>, consumer: &mut dyn FinalizedConsumer, ) -> ContentResult<ObjectId> {
144: pub fn push_extent( &mut self, extent: ExtentSlice, consumer: &mut dyn FinalizedConsumer, ) -> ContentResult<()> {
178: pub fn finish(mut self, consumer: &mut dyn FinalizedConsumer) -> ContentResult<MappingBuild> {
316: pub fn build_streaming<R: Read>( capacities: &ConstructionCapacities, source: R, consumer: &mut dyn FinalizedConsumer, ) -> ContentResult<MappingBuild> {
332: pub fn emit_empty_leaf( consumer: &mut dyn FinalizedConsumer, build: &mut MappingBuild, ) -> ContentResult<NodeSummary> {
347: pub fn emit_file_state( consumer: &mut dyn FinalizedConsumer, mapping_root: NodeSummary, ) -> ContentResult<ObjectId> {
```

**core/crates/layerfs-content/src/file/mapping/codec.rs** [private module] - 10 pub items

```text
20: pub const CHUNK_MAGIC: &[u8; 8] = b"LFS4CHK\0";
31: pub fn profile_id() -> ObjectId {
56: pub fn encode_chunk_object(bytes: &[u8]) -> ContentResult<Vec<u8>> {
87: pub fn decode_chunk_payload(value: &[u8]) -> ContentResult<&[u8]> {
101: pub fn encode_node(node: &ExtentNode) -> ContentResult<Vec<u8>> {
160: pub fn decode_node(canonical: &[u8]) -> ContentResult<ExtentNode> {
165: pub fn decode_node_with_context(canonical: &[u8], root: bool) -> ContentResult<ExtentNode> {
267: pub fn encode_file_state(state: FileState) -> ContentResult<Vec<u8>> {
286: pub fn decode_file_state(canonical: &[u8]) -> ContentResult<FileState> {
337: pub const fn chunk_canonical_len(raw: usize) -> usize {
```

**core/crates/layerfs-content/src/file/mapping/mod.rs** [public module path] - 4 pub items

```text
10: pub use build::{build_streaming, emit_empty_leaf, emit_file_state, ExtentBuilder, MappingBuild};
11: pub use codec::{
16: pub use read::{
19: pub use types::{
```

**core/crates/layerfs-content/src/file/mapping/read.rs** [private module] - 5 pub items

```text
24: pub const READ_WAVE_OBJECTS: usize = 32;
26: pub const READ_WAVE_BYTES: usize = READ_WAVE_OBJECTS * cdc::MAXIMUM_CHUNK_BYTES;
33: pub const READ_NAVIGATION_WAVE: usize = 32;
37: pub struct ReadCounters {
174: pub fn read_range( reader: &dyn AuthenticatedObjects, state: FileState, range: Range<u64>, sink: &mut dyn Write, scope: &TimingScope<'_, Active>, ) -> ContentResult<ReadCounters> { ...
```

**core/crates/layerfs-content/src/file/mapping/types.rs** [private module] - 21 pub items

```text
13: pub const MIN_ENTRIES: usize = 64;
15: pub const MAX_ENTRIES: usize = MAX_MAPPING_ENTRIES;
17: pub const MAX_LEVEL: u8 = 31;
19: pub const MINIMUM_ROOT_ENTRIES: usize = 2;
21: pub const MINIMUM_ROOT_LEAF_ENTRIES: usize = 0;
23: pub const MAX_NODE_OBJECT_BYTES: usize = 8_192;
27: pub struct ExtentSlice {
43: pub fn new( payload_object_id: ObjectId, source_offset: u32, logical_length: u32, ) -> ContentResult<Self> {
62: pub const fn payload_object_id(self) -> ObjectId {
67: pub const fn source_offset(self) -> u32 {
72: pub const fn logical_length(self) -> u32 {
79: pub struct ChildDescriptor {
90: pub enum ExtentNode {
113: pub fn level(&self) -> u8 {
121: pub fn logical_len(&self) -> u64 {
135: pub fn extent_count(&self) -> u64 {
146: pub fn entry_count(&self) -> usize {
154: pub fn references(&self) -> Vec<ObjectId> {
171: pub fn validate(&self, root: bool) -> ContentResult<()> {
242: pub struct FileState {
257: pub struct NodeSummary {
```

**core/crates/layerfs-content/src/filesystem/identity.rs** [public module path] - 8 pub items

```text
20: pub const MAXIMUM_INODE_SERIAL: u64 = i64::MAX as u64;
24: pub struct InodeScope(ObjectId);
28: pub const fn from_object(id: ObjectId) -> Self {
33: pub const fn object(self) -> ObjectId {
40: pub struct InodeIdentity {
47: pub fn new(scope: InodeScope, serial: u64) -> ContentResult<Self> {
55: pub const fn scope(self) -> InodeScope {
60: pub const fn serial(self) -> u64 {
```

**core/crates/layerfs-content/src/filesystem/input.rs** [public module path] - 10 pub items

```text
25: pub struct DirectoryUpdate {
34: pub fn check(&self) -> ContentResult<()> {
47: pub struct InodeUpdate {
56: pub struct FilesystemResources {
90: pub fn maximum_touched_serials(&self) -> usize {
98: pub fn check(&self) -> ContentResult<()> {
129: pub struct FilesystemInput<'a> {
157: pub fn check(&self) -> ContentResult<()> {
218: pub fn value_for(&self, serial: u64) -> Option<InodeValue> {
226: pub fn update_for(&self, parent: u64) -> Option<&DirectoryUpdate> {
```

**core/crates/layerfs-content/src/filesystem/limits.rs** [public module path] - 20 pub items

```text
10: pub const MAXIMUM_PAGE_BYTES: usize = 8_192;
12: pub const MAXIMUM_NAME_BYTES: usize = 255;
14: pub const MAXIMUM_PATH_BYTES: usize = 4_096;
16: pub const MAXIMUM_PATH_COMPONENTS: usize = 256;
18: pub const MAXIMUM_TREE_LEVEL: u8 = 31;
20: pub const MINIMUM_INODE_LEAF_ROWS: u64 = 50;
22: pub const MAXIMUM_INODE_LEAF_ROWS: u64 = 100;
24: pub const MINIMUM_INODE_BRANCH_CHILDREN: u64 = 64;
26: pub const MAXIMUM_INODE_BRANCH_CHILDREN: u64 = 127;
28: pub const MINIMUM_FILLED_PAGE_BYTES: usize = 3_277;
30: pub const DEFAULT_OPERATION_SCRATCH_BYTES: usize = 4 * 1024 * 1024 - 1;
32: pub const MAXIMUM_SYMLINK_TARGET_BYTES: usize = 4_096;
34: pub const MAXIMUM_ATTRIBUTE_DOMAIN_BYTES: usize = 64;
36: pub const MAXIMUM_ATTRIBUTE_KEY_BYTES: usize = 255;
38: pub const PORTABLE_ATTRIBUTE_DOMAIN: &str = "portable";
40: pub const MAXIMUM_ATTRIBUTE_VALUE_BYTES: usize = 1024 * 1024;
47: pub const MAXIMUM_ATTRIBUTE_KEYS: usize = 4_096;
57: pub const MAXIMUM_DIRECTORY_LEAF_ROWS: usize = (MAXIMUM_PAGE_BYTES - EMPTY_PAGE_BYTES) / (2 + 1 + 8);
61: pub const EMPTY_PAGE_BYTES: usize = 44;
64: pub const fn filled_page(bytes: usize) -> bool {
```

**core/crates/layerfs-content/src/filesystem/mod.rs** [public module path] - 10 pub items

```text
21: pub use identity::{InodeIdentity, InodeScope};
22: pub use input::{DirectoryUpdate, FilesystemInput, FilesystemResources, InodeUpdate};
23: pub use objects::{FilesystemObjects, FilesystemPhases, ObjectWork};
24: pub use path::{LogicalPath, PathName};
25: pub use read::{DirectoryListing, FilesystemRead, FilesystemReadWork, Resolved, Stat};
26: pub use root::{profile_id, scope_for_seed, FilesystemRoot, FilesystemRootId};
27: pub use sorted::{DirectoryRoot, SortedWork, MAXIMUM_SCRATCH_BYTES};
28: pub use symlink::SymlinkTarget;
29: pub use update::{
33: pub use validate::{check, CheckedInput, FilesystemTopology};
```

**core/crates/layerfs-content/src/filesystem/objects.rs** [public module path] - 15 pub items

```text
20: pub const MAXIMUM_READ_DEMANDS: usize = 4_096;
24: pub struct ObjectWork {
38: pub struct FilesystemObjects<'a> {
46: pub fn new( reader: &'a dyn AuthenticatedObjects, consumer: &'a mut dyn FinalizedConsumer, ) -> Self {
58: pub const fn work(&self) -> ObjectWork {
66: pub fn reader(&self) -> &'a dyn AuthenticatedObjects {
71: pub fn read(&mut self, id: ObjectId) -> ContentResult<Vec<u8>> {
78: pub fn read_shared(reader: &dyn AuthenticatedObjects, id: ObjectId) -> ContentResult<Vec<u8>> {
89: pub fn read_batch(&mut self, ids: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>> {
112: pub fn emit(&mut self, object: FinalizedObject) -> ContentResult<ObjectId> {
134: pub struct FilesystemPhases<'a> {
140: pub const fn disabled() -> Self {
145: pub const fn new( scope: &'a layerfs_telemetry::timer::TimingScope<'a, layerfs_telemetry::timer::Active>, ) -> Self {
152: pub fn is_recording(&self) -> bool {
157: pub fn phase<T>( &self, name: &'static str, body: impl FnOnce() -> ContentResult<T>, ) -> ContentResult<T> {
```

**core/crates/layerfs-content/src/filesystem/path.rs** [public module path] - 16 pub items

```text
16: pub struct PathName {
22: pub fn new(value: &str) -> ContentResult<Self> {
27: pub fn from_bytes(bytes: &[u8]) -> ContentResult<Self> {
35: pub fn as_bytes(&self) -> &[u8] {
40: pub fn as_str(&self) -> &str {
67: pub struct LogicalPath {
73: pub fn new(value: &str) -> ContentResult<Self> {
78: pub fn from_bytes(bytes: &[u8]) -> ContentResult<Self> {
86: pub fn root() -> Self {
91: pub fn is_root(&self) -> bool {
96: pub fn as_bytes(&self) -> &[u8] {
101: pub fn as_str(&self) -> &str {
106: pub fn component_count(&self) -> usize {
115: pub fn components(&self) -> impl Iterator<Item = &[u8]> {
125: pub fn split_last(&self) -> Option<(Self, PathName)> {
144: pub fn join(&self, name: &PathName) -> Self {
```

**core/crates/layerfs-content/src/filesystem/read.rs** [public module path] - 17 pub items

```text
28: pub struct FilesystemReadWork {
39: pub struct Resolved {
48: pub struct Stat {
71: pub use crate::filesystem::directory::read::ListingPage as DirectoryListing;
74: pub struct FilesystemRead<'a> {
82: pub fn new( reader: &'a dyn AuthenticatedObjects, root: FilesystemRootId, ) -> ContentResult<Self> {
95: pub const fn root(&self) -> FilesystemRoot {
100: pub const fn work(&self) -> FilesystemReadWork {
105: pub fn resolve(&mut self, path: &LogicalPath) -> ContentResult<Resolved> {
134: pub fn stat(&mut self, path: &LogicalPath) -> ContentResult<Stat> {
139: pub fn list( &mut self, path: &LogicalPath, after: Option<&PathName>, max_entries: usize, max_bytes: usize, ) -> ContentResult<ListingPage> { ...
162: pub fn lookup_names( &mut self, path: &LogicalPath, names: &[PathName], ) -> ContentResult<Vec<Option<u64>>> {
182: pub fn lookup_inodes(&mut self, serials: &[u64]) -> ContentResult<Vec<Option<InodeValue>>> {
188: pub fn readlink(&mut self, path: &LogicalPath) -> ContentResult<SymlinkTarget> {
198: pub fn read_portable(&mut self, path: &LogicalPath) -> ContentResult<PortableMetadata> {
209: pub fn read_attribute( &mut self, path: &LogicalPath, key: &AttributeKey, maximum_bytes: usize, ) -> ContentResult<Option<Vec<u8>>> {
232: pub fn attribute_keys(&mut self, path: &LogicalPath) -> ContentResult<Vec<AttributeKey>> {
```

**core/crates/layerfs-content/src/filesystem/root.rs** [public module path] - 19 pub items

```text
13: pub const ROOT_VALUE_BYTES: usize = 116;
15: pub const ROOT_MAGIC: [u8; 8] = *b"LFS6FSR\0";
17: pub const ROOT_VERSION: u16 = 1;
19: pub const ROOT_ROLE: u8 = 6;
21: pub const PROFILE_DESCRIPTION: &[u8] = b"layerfs/namespace-profile/scoped-inline/v1\0scope32;serial8;inode81;leaf50-100;branch64-127;page8192;depth31;directory-fill2/5";
24: pub fn profile_id() -> ObjectId {
29: pub fn scope_for_seed(seed: [u8; 32]) -> InodeScope {
37: pub struct FilesystemRootId(pub ObjectId);
41: pub struct FilesystemRoot {
50: pub fn new( profile: ObjectId, scope: InodeScope, root_serial: u64, inode_table: ObjectId, ) -> ContentResult<Self> {
70: pub const fn profile(self) -> ObjectId {
75: pub const fn scope(self) -> InodeScope {
80: pub const fn root_inode(self) -> InodeIdentity {
85: pub const fn inode_table(self) -> ObjectId {
90: pub const fn with_inode_table(self, inode_table: ObjectId) -> Self {
98: pub fn encode(self) -> ContentResult<Vec<u8>> {
112: pub fn decode(canonical: &[u8]) -> ContentResult<Self> {
152: pub const fn role(self) -> ObjectRole {
157: pub const fn references(self) -> [ObjectId; 1] {
```

**core/crates/layerfs-content/src/filesystem/symlink.rs** [public module path] - 10 pub items

```text
12: pub const SYMLINK_MAGIC: [u8; 8] = *b"LFS4LNK\0";
14: pub const SYMLINK_VERSION: u16 = 1;
16: pub const SYMLINK_ROLE: u8 = 5;
20: pub struct SymlinkTarget {
26: pub fn new(bytes: Vec<u8>) -> ContentResult<Self> {
34: pub fn as_bytes(&self) -> &[u8] {
39: pub fn encode(&self) -> ContentResult<Vec<u8>> {
51: pub fn decode(canonical: &[u8]) -> ContentResult<Self> {
83: pub fn finalize(&self) -> ContentResult<FinalizedObject> {
89: pub fn emit_symlink( objects: &mut crate::filesystem::objects::FilesystemObjects<'_>, target: SymlinkTarget, ) -> ContentResult<ObjectId> {
```

**core/crates/layerfs-content/src/filesystem/update.rs** [public module path] - 6 pub items

```text
32: pub struct FilesystemUpdateCounters {
57: pub struct FilesystemResult {
67: pub fn build_filesystem( objects: &mut FilesystemObjects<'_>, input: &FilesystemInput<'_>, backing: Option<&mut dyn OrderingBacking>, ) -> ContentResult<FilesystemResult> {
79: pub fn build_filesystem_timed( objects: &mut FilesystemObjects<'_>, input: &FilesystemInput<'_>, backing: Option<&mut dyn OrderingBacking>, phases: &FilesystemPhases<'_>, ) -> ContentResult<FilesystemResult> {
92: pub fn update_filesystem( objects: &mut FilesystemObjects<'_>, input: &FilesystemInput<'_>, backing: Option<&mut dyn OrderingBacking>, ) -> ContentResult<FilesystemResult> {
104: pub fn update_filesystem_timed( objects: &mut FilesystemObjects<'_>, input: &FilesystemInput<'_>, backing: Option<&mut dyn OrderingBacking>, phases: &FilesystemPhases<'_>, ) -> ContentResult<FilesystemResult> {
```

**core/crates/layerfs-content/src/filesystem/validate.rs** [public module path] - 8 pub items

```text
30: pub struct ValidationWork {
53: pub const MAXIMUM_CYCLE_CHECK_ENTRIES: usize = 4_096;
55: pub const ALLOCATION_CHECK_BATCH: usize = 64;
59: pub struct FilesystemTopology {
68: pub fn load( reader: &dyn AuthenticatedObjects, base: Option<FilesystemRootId>, scope: InodeScope, root_serial: u64, ) -> ContentResult<Self> {
100: pub fn table(self) -> InodeTable {
109: pub struct CheckedInput<'a> {
119: pub fn check<'a>( reader: &dyn AuthenticatedObjects, input: &'a FilesystemInput<'a>, unreachable: &BTreeMap<u64, ()>, work: &mut ValidationWork, ) -> ContentResult<CheckedInput<'a>> {
```

**core/crates/layerfs-content/src/filesystem/attributes/build.rs** [public module path] - 7 pub items

```text
24: pub struct AttributeBuildWork {
60: pub struct AttributeTreeBuilder {
74: pub fn new() -> Self {
86: pub const fn work(&self) -> AttributeBuildWork {
91: pub fn push( &mut self, objects: &mut FilesystemObjects<'_>, entry: AttributeEntry, ) -> ContentResult<()> {
147: pub fn finish(mut self, objects: &mut FilesystemObjects<'_>) -> ContentResult<ObjectId> {
422: pub fn build_attribute_tree( objects: &mut FilesystemObjects<'_>, entries: impl Iterator<Item = ContentResult<AttributeEntry>>, ) -> ContentResult<(ObjectId, AttributeBuildWork)> {
```

**core/crates/layerfs-content/src/filesystem/attributes/codec.rs** [public module path] - 18 pub items

```text
15: pub const ATTRIBUTE_MAGIC: [u8; 8] = *b"LFS4MET\0";
17: pub const ATTRIBUTE_VERSION: u16 = 1;
19: pub const ATTRIBUTE_LEAF_ROLE: u8 = 9;
21: pub const ATTRIBUTE_BRANCH_ROLE: u8 = 10;
23: pub const EMPTY_PAGE_BYTES: usize = 44;
25: pub const NODE_HEADER_BYTES: usize = 31;
27: pub const LEAF_ROW_OVERHEAD: usize = 37;
29: pub const BRANCH_ROW_OVERHEAD: usize = 36;
33: pub struct AttributeEntry {
42: pub enum AttributePage {
65: pub const fn level(&self) -> u8 {
73: pub fn bytes(&self) -> ContentResult<usize> {
95: pub fn fits(&self) -> ContentResult<bool> {
100: pub fn filled(&self) -> ContentResult<bool> {
105: pub fn subtree_count(&self) -> u64 {
114: pub fn row_bytes(key: &AttributeKey, overhead: usize) -> usize {
119: pub fn encode_attribute_page(page: &AttributePage) -> ContentResult<Vec<u8>> {
186: pub fn decode_attribute_page(canonical: &[u8]) -> ContentResult<AttributePage> {
```

**core/crates/layerfs-content/src/filesystem/attributes/keys.rs** [public module path] - 9 pub items

```text
18: pub const PORTABLE_KEYS: [&[u8]; 2] = [b"mode", b"mtime"];
22: pub struct AttributeKey {
29: pub fn new(domain: String, key: Vec<u8>) -> ContentResult<Self> {
61: pub fn domain(&self) -> &str {
66: pub fn key(&self) -> &[u8] {
71: pub fn is_portable(&self) -> bool {
76: pub fn is_mode(&self) -> bool {
81: pub fn is_mtime(&self) -> bool {
102: pub fn check_patch_order(patches: &[(AttributeKey, Option<Vec<u8>>)]) -> ContentResult<()> {
```

**core/crates/layerfs-content/src/filesystem/attributes/mod.rs** [public module path] - 7 pub items

```text
11: pub use build::{build_attribute_tree, AttributeBuildWork, AttributeTreeBuilder};
12: pub use codec::{decode_attribute_page, encode_attribute_page, AttributeEntry, AttributePage};
13: pub use keys::AttributeKey;
14: pub use patch::{apply_patches, AttributePatch, AttributePatchWork};
15: pub use portable::PortableMetadata;
16: pub use read::{
19: pub use value::{emit_value, read_value};
```

**core/crates/layerfs-content/src/filesystem/attributes/patch.rs** [public module path] - 6 pub items

```text
21: pub enum AttributePatch {
38: pub fn key(&self) -> &AttributeKey {
47: pub struct AttributePatchWork {
66: pub fn apply_patches( reader: &dyn AuthenticatedObjects, objects: &mut FilesystemObjects<'_>, base: ObjectId, patches: &[AttributePatch], ) -> ContentResult<(ObjectId, AttributePatchWork)> {
131: pub fn visit_keys( reader: &dyn AuthenticatedObjects, root: ObjectId, visitor: impl FnMut(&AttributeKey, &ObjectId) -> ContentResult<()>, ) -> ContentResult<()> {
144: pub fn visit_keys_counted( reader: &dyn AuthenticatedObjects, root: ObjectId, work: &mut AttributePatchWork, mut visitor: impl FnMut(&AttributeKey, &ObjectId) -> ContentResult<()>, ) -> ContentResult<()> {
```

**core/crates/layerfs-content/src/filesystem/attributes/portable.rs** [public module path] - 7 pub items

```text
13: pub const MAXIMUM_NANOSECONDS: u32 = 999_999_999;
17: pub struct PortableMetadata {
28: pub fn validate(&self, kind: InodeKind) -> ContentResult<()> {
43: pub fn mode_bytes(&self, kind: InodeKind) -> ContentResult<[u8; 4]> {
49: pub fn mtime_bytes(&self) -> ContentResult<[u8; 12]> {
60: pub fn decode_mode(bytes: &[u8], kind: InodeKind) -> ContentResult<u32> {
76: pub fn decode_mtime(bytes: &[u8]) -> ContentResult<(i64, u32)> {
```

**core/crates/layerfs-content/src/filesystem/attributes/read.rs** [public module path] - 6 pub items

```text
22: pub struct AttributeReadWork {
32: pub fn lookup( reader: &dyn AuthenticatedObjects, root: ObjectId, key: &AttributeKey, work: &mut AttributeReadWork, ) -> ContentResult<Option<ObjectId>> {
43: pub fn lookup_many( reader: &dyn AuthenticatedObjects, root: ObjectId, keys: &[AttributeKey], work: &mut AttributeReadWork, ) -> ContentResult<Vec<Option<AttributeEntry>>> {
109: pub fn read_portable( reader: &dyn AuthenticatedObjects, root: ObjectId, kind: InodeKind, work: &mut AttributeReadWork, ) -> ContentResult<PortableMetadata> {
145: pub fn read_opaque( reader: &dyn AuthenticatedObjects, root: ObjectId, key: &AttributeKey, maximum_bytes: usize, work: &mut AttributeReadWork, ) -> ContentResult<Option<Vec<u8>>> { ...
160: pub fn read_value_bounded( reader: &dyn AuthenticatedObjects, root: ObjectId, key: &AttributeKey, work: &mut AttributeReadWork, ) -> ContentResult<Option<Vec<u8>>> {
```

**core/crates/layerfs-content/src/filesystem/attributes/value.rs** [public module path] - 2 pub items

```text
22: pub fn emit_value(objects: &mut FilesystemObjects<'_>, bytes: &[u8]) -> ContentResult<ObjectId> {
59: pub fn read_value( reader: &dyn crate::object::AuthenticatedObjects, root: ObjectId, maximum_bytes: usize, ) -> ContentResult<Vec<u8>> {
```

**core/crates/layerfs-content/src/filesystem/directory/codec.rs** [public module path] - 10 pub items

```text
18: pub enum DirectoryPage {
39: pub const fn level(&self) -> u8 {
47: pub fn subtree_count(&self) -> u64 {
55: pub fn subtree_bytes(&self) -> u64 {
67: pub fn page_bytes(page: &DirectoryPage) -> ContentResult<usize> {
92: pub fn decode_directory_page(canonical: &[u8]) -> ContentResult<DirectoryPage> {
130: pub fn encode_directory_page(page: &DirectoryPage) -> ContentResult<Vec<u8>> {
173: pub fn filled_page(bytes: usize) -> bool {
178: pub const fn role(level: u8) -> u8 {
187: pub fn page_shape(canonical: &[u8]) -> ContentResult<(u8, u64)> {
```

**core/crates/layerfs-content/src/filesystem/directory/mod.rs** [public module path] - 3 pub items

```text
7: pub use codec::{decode_directory_page, encode_directory_page, DirectoryPage};
8: pub use read::{list_after, lookup, lookup_many, DirectoryReadWork, ListingPage};
9: pub use update::{apply_bindings, build_directory, empty_directory};
```

**core/crates/layerfs-content/src/filesystem/directory/read.rs** [public module path] - 6 pub items

```text
22: pub struct DirectoryReadWork {
33: pub struct ListingPage {
41: pub fn lookup( reader: &dyn AuthenticatedObjects, root: DirectoryRoot, name: &PathName, ) -> ContentResult<Option<u64>> {
51: pub fn lookup_counted( reader: &dyn AuthenticatedObjects, root: DirectoryRoot, name: &PathName, work: &mut DirectoryReadWork, ) -> ContentResult<Option<(PathName, u64)>> {
108: pub fn lookup_many( reader: &dyn AuthenticatedObjects, root: DirectoryRoot, names: &[PathName], work: &mut DirectoryReadWork, ) -> ContentResult<Vec<Option<u64>>> {
187: pub fn list_after( reader: &dyn AuthenticatedObjects, root: DirectoryRoot, after: Option<&PathName>, max_entries: usize, max_bytes: usize, work: &mut DirectoryReadWork, ...
```

**core/crates/layerfs-content/src/filesystem/directory/update.rs** [public module path] - 5 pub items

```text
12: pub use crate::filesystem::sorted::finish::DirectoryRoot as DirectoryRootHandle;
15: pub fn empty_directory(objects: &mut FilesystemObjects<'_>) -> ContentResult<DirectoryRoot> {
22: pub fn apply_bindings( objects: &mut FilesystemObjects<'_>, base: Option<DirectoryRoot>, changes: impl Iterator<Item = ContentResult<(PathName, Option<u64>)>>, scratch_limit: usize, observe: &mut dyn FnMut(Option<u64>, O ...
33: pub fn build_directory( objects: &mut FilesystemObjects<'_>, entries: impl Iterator<Item = ContentResult<(PathName, u64)>>, ) -> ContentResult<(DirectoryRoot, SortedWork)> {
47: pub fn check_change_order(changes: &[(PathName, Option<u64>)]) -> ContentResult<()> {
```

**core/crates/layerfs-content/src/filesystem/inode/codec.rs** [public module path] - 10 pub items

```text
19: pub enum InodePage {
38: pub const fn level(&self) -> u8 {
46: pub fn subtree_count(&self) -> u64 {
54: pub fn subtree_bytes(&self) -> u64 {
63: pub fn decode_inode_page(canonical: &[u8]) -> ContentResult<InodePage> {
100: pub fn encode_inode_page(page: &InodePage) -> ContentResult<Vec<u8>> {
142: pub fn page_bytes(page: &InodePage) -> ContentResult<usize> {
154: pub fn filled_page(rows: usize, level: u8) -> bool {
163: pub const fn maximum_rows(level: u8) -> u64 {
172: pub const fn role(level: u8) -> u8 {
```

**core/crates/layerfs-content/src/filesystem/inode/mod.rs** [public module path] - 3 pub items

```text
7: pub use codec::{decode_inode_page, encode_inode_page, InodePage};
8: pub use read::{lookup, lookup_many, InodeReadWork, InodeTable};
9: pub use update::{apply_changes, apply_inode_values, build_table, InodeChange};
```

**core/crates/layerfs-content/src/filesystem/inode/read.rs** [public module path] - 4 pub items

```text
19: pub struct InodeReadWork {
30: pub struct InodeTable {
38: pub fn lookup( reader: &dyn AuthenticatedObjects, table: InodeTable, serial: u64, work: &mut InodeReadWork, ) -> ContentResult<Option<InodeValue>> {
85: pub fn lookup_many( reader: &dyn AuthenticatedObjects, table: InodeTable, serials: &[u64], work: &mut InodeReadWork, ) -> ContentResult<Vec<Option<InodeValue>>> {
```

**core/crates/layerfs-content/src/filesystem/inode/update.rs** [public module path] - 7 pub items

```text
17: pub enum InodeChange {
34: pub const fn serial(self) -> u64 {
41: pub const fn value(self) -> Option<InodeValue> {
50: pub fn apply_inode_values( objects: &mut FilesystemObjects<'_>, base: Option<ObjectId>, changes: impl Iterator<Item = ContentResult<(u64, Option<InodeValue>)>>, scratch_limit: usize, ) -> ContentResult<(ObjectId, SortedWork)> {
60: pub fn apply_changes( objects: &mut FilesystemObjects<'_>, base: Option<ObjectId>, changes: &[InodeChange], ) -> ContentResult<(ObjectId, SortedWork)> {
76: pub fn build_table( objects: &mut FilesystemObjects<'_>, rows: impl Iterator<Item = ContentResult<(u64, InodeValue)>>, ) -> ContentResult<(ObjectId, SortedWork)> {
89: pub fn check_change_order(changes: &[InodeChange]) -> ContentResult<()> {
```

**core/crates/layerfs-content/src/filesystem/references/backing.rs** [public module path] - 8 pub items

```text
26: pub const DEFAULT_BACKING_CAPACITY_BYTES: u64 = 256 * 1024 * 1024;
34: pub trait OrderingRun {
54: pub trait OrderingBacking {
169: pub struct FileBacking {
178: pub fn new(directory: impl AsRef<Path>) -> Self {
183: pub fn with_capacity(directory: impl AsRef<Path>, capacity_bytes: u64) -> Self {
202: pub fn runs(&self) -> u64 {
207: pub fn owns_storage(&self) -> bool {
```

**core/crates/layerfs-content/src/filesystem/references/merge.rs** [public module path] - 10 pub items

```text
16: pub struct Run {
29: pub struct MergeWork {
47: pub struct RunReader<'a> {
58: pub fn new(run: &'a Run, buffer_bytes: usize) -> Self {
71: pub const fn rows(&self) -> u64 {
76: pub const fn run_offset(&self) -> u64 {
84: pub fn seek_from(run: &'a Run, buffer_bytes: usize, offset: u64) -> ContentResult<Self> {
107: pub fn rewind(&mut self) {
117: pub fn next(&mut self) -> ContentResult<Option<Row>> {
144: pub fn merge_runs( backing: &mut dyn crate::filesystem::references::backing::OrderingBacking, older: &Run, newer: &Run, buffer_bytes: usize, work: &mut MergeWork, ) -> ContentResult<Run> { ...
```

**core/crates/layerfs-content/src/filesystem/references/mod.rs** [public module path] - 6 pub items

```text
10: pub use backing::{FileBacking, OrderingBacking, OrderingRun};
11: pub use merge::{merge_runs, MergeWork, Run, RunReader};
12: pub use record::{Row, ROW_BYTES};
13: pub use reduce::{
17: pub use release::{release_zero_count, ReleaseWork};
18: pub use runs::{RunStore, DEFAULT_MERGE_BUFFER_BYTES};
```

**core/crates/layerfs-content/src/filesystem/references/record.rs** [public module path] - 9 pub items

```text
26: pub const ROW_BYTES: usize = 96;
28: pub const ROW_VERSION: u8 = 1;
30: pub const TAG_COUNT: u8 = 1;
32: pub const TAG_EFFECT: u8 = 2;
42: pub enum Row {
65: pub const fn serial(self) -> u64 {
72: pub const fn value(self) -> Option<InodeValue> {
79: pub fn encode(self) -> ContentResult<[u8; ROW_BYTES]> {
109: pub fn decode(bytes: &[u8; ROW_BYTES]) -> ContentResult<Self> {
```

**core/crates/layerfs-content/src/filesystem/references/reduce.rs** [public module path] - 24 pub items

```text
25: pub const DEFAULT_MAXIMUM_PENDING: usize = 4_096;
27: pub const DEFAULT_BASE_BATCH: usize = 32;
31: pub struct ReferenceWork {
55: pub struct ReferenceReducer<'r, 'b> {
65: pub fn new( maximum_pending: usize, backing: Option<&'r mut (dyn OrderingBacking + 'b)>, merge_buffer: usize, ordering_bytes: u64, ) -> Self {
81: pub fn check_backing_capacity(&self) -> ContentResult<()> {
86: pub fn work(&self) -> ReferenceWork {
94: pub fn declare_new(&mut self, serial: u64) -> ContentResult<()> {
106: pub fn is_new(&self, serial: u64) -> bool {
111: pub fn note_retained_binding(&mut self, serial: u64) -> ContentResult<()> {
134: pub fn note_removed_binding(&mut self, serial: u64) -> ContentResult<()> {
146: pub fn note_value(&mut self, serial: u64, value: InodeValue) -> ContentResult<()> {
160: pub fn state(&mut self, serial: u64) -> ContentResult<Option<PendingState>> {
176: pub fn touched_serials(&mut self, _batch: usize) -> ContentResult<Vec<u64>> {
210: pub fn note_serials_scanned(&mut self, serials: u64) {
215: pub fn pending_rows(&self) -> usize {
220: pub fn release(&mut self) -> ContentResult<()> {
261: pub fn finish<'a>( &mut self, reader: &'a dyn AuthenticatedObjects, table: InodeTable, base_batch: usize, root_serial: u64, ) -> ContentResult<FinalRows<'a>> { ...
288: pub enum PendingState {
307: pub struct FinalChange {
315: pub struct FinalRows<'r> {
375: pub const fn work(&self) -> ReferenceWork {
380: pub fn next_change(&mut self) -> ContentResult<Option<FinalChange>> {
583: pub const fn default_merge_buffer() -> usize {
```

**core/crates/layerfs-content/src/filesystem/references/release.rs** [public module path] - 2 pub items

```text
22: pub struct ReleaseWork {
51: pub fn release_zero_count( reader: &dyn AuthenticatedObjects, table: InodeTable, reducer: &mut ReferenceReducer<'_, '_>, starting: &[u64], base_batch: usize, page_entries: usize, ...
```

**core/crates/layerfs-content/src/filesystem/references/runs.rs** [public module path] - 23 pub items

```text
30: pub const DEFAULT_MERGE_BUFFER_BYTES: usize = 16 * 1024;
32: pub const DEFAULT_ORDERING_BYTES: u64 = 64 * 1024 * 1024;
34: pub const MAXIMUM_LEVELS: usize = 32;
37: pub struct RunStore<'r, 'b> {
62: pub fn new( backing: Option<&'r mut (dyn OrderingBacking + 'b)>, merge_buffer: usize, limit: u64, ) -> Self {
84: pub fn check_capacity(&self) -> ContentResult<()> {
101: pub const fn limit_bytes(&self) -> u64 {
114: pub const fn pending_bytes(&self) -> u64 {
119: pub fn charge_pending(&mut self, rows: u64) -> ContentResult<()> {
132: pub fn owned_bytes(&self) -> u64 {
139: pub fn reserve(&mut self, bytes: u64) -> ContentResult<()> {
163: pub const fn work(&self) -> MergeWork {
168: pub fn is_empty(&self) -> bool {
173: pub fn live_runs(&self) -> usize {
188: pub fn run_bytes(&self) -> u64 {
200: pub fn spill(&mut self, pending: &BTreeMap<u64, Row>) -> ContentResult<()> {
307: pub fn find(&mut self, serial: u64) -> ContentResult<Option<Row>> {
376: pub fn consolidate(&mut self) -> ContentResult<()> {
444: pub fn single_run(&self) -> Option<&Run> {
455: pub fn visit_newest_first( &self, mut visitor: impl FnMut(Row) -> ContentResult<bool>, ) -> ContentResult<()> {
502: pub fn take_single_handle( &mut self, ) -> Option<Box<dyn crate::filesystem::references::backing::OrderingRun>> {
512: pub fn release(&mut self) -> ContentResult<()> {
576: pub fn visit_run( run: &Run, buffer_bytes: usize, mut visitor: impl FnMut(Row) -> ContentResult<()>, ) -> ContentResult<()> {
```

**core/crates/layerfs-content/src/filesystem/sorted/budget.rs** [public module path] - 10 pub items

```text
16: pub struct Budget {
24: pub fn new(limit: usize) -> Rc<Self> {
33: pub fn default_limit() -> usize {
38: pub fn reserve(self: &Rc<Self>, bytes: usize) -> ContentResult<Lease> {
59: pub fn peak(&self) -> usize {
64: pub fn used(&self) -> usize {
70: pub struct Lease {
77: pub fn grow(&mut self, bytes: usize) -> ContentResult<()> {
97: pub fn shrink(&mut self, bytes: usize) {
106: pub const fn bytes(&self) -> usize {
```

**core/crates/layerfs-content/src/filesystem/sorted/finish.rs** [public module path] - 4 pub items

```text
140: pub struct DirectoryRoot(pub ObjectId);
147: pub fn apply_directory_changes( objects: &mut FilesystemObjects<'_>, base: Option<DirectoryRoot>, changes: impl Iterator<Item = ContentResult<(crate::filesystem::path::PathName, Option<u64>)>>, scratch_limit: usize, obse ...
165: pub fn apply_inode_changes( objects: &mut FilesystemObjects<'_>, base: Option<ObjectId>, changes: impl Iterator<Item = ContentResult<(u64, Option<crate::object::inode_leaf::InodeValue>)>>, scratch_limit: usize, ) -> ContentResult<(ObjectId, SortedWork)> {
175: pub fn emit_empty_directory(objects: &mut FilesystemObjects<'_>) -> ContentResult<DirectoryRoot> {
```

**core/crates/layerfs-content/src/filesystem/sorted/format.rs** [public module path] - 14 pub items

```text
22: pub const EMPTY_PAGE_BYTES: usize = 44;
24: pub const NODE_HEADER_BYTES: usize = 31;
26: pub const NODE_VERSION: u16 = 1;
28: pub const DIRECTORY_MAGIC: [u8; 8] = *b"LFS6NSP\0";
30: pub const INODE_MAGIC: [u8; 8] = *b"LFS6INT\0";
32: pub const DIRECTORY_LEAF_ROLE: u8 = 1;
34: pub const DIRECTORY_BRANCH_ROLE: u8 = 2;
36: pub const INODE_LEAF_ROLE: u8 = 7;
38: pub const INODE_BRANCH_ROLE: u8 = 8;
40: pub const DIRECTORY_LEAF_SERIAL_BYTES: usize = 8;
42: pub const BRANCH_CHILD_BYTES: usize = 32;
44: pub const NAME_LENGTH_BYTES: usize = 2;
46: pub const INODE_BRANCH_ROW_BYTES: usize = 40;
48: pub const DEFAULT_PAGE_ITEMS: usize = 234;
```

**core/crates/layerfs-content/src/filesystem/sorted/mod.rs** [public module path] - 2 pub items

```text
9: pub use finish::{
12: pub use page::{SortedWork, MAXIMUM_SCRATCH_BYTES};
```

**core/crates/layerfs-content/src/filesystem/sorted/page.rs** [public module path] - 3 pub items

```text
23: pub const MAXIMUM_SCRATCH_BYTES: usize = 4 * 1024 * 1024;
25: pub const BATCH_CHILDREN: usize = 32;
36: pub struct SortedWork {
```

**core/crates/layerfs-content/src/object/access.rs** [private module] - 1 pub items

```text
29: pub trait AuthenticatedObjects {
```

**core/crates/layerfs-content/src/object/codec.rs** [public module path] - 8 pub items

```text
14: pub const OBJECT_MAGIC: [u8; 4] = *b"LFSO";
16: pub const BYTES_KIND: u8 = 1;
18: pub const HEADER_LEN: usize = 9;
21: pub const MAX_PAYLOAD_BYTES: usize = MAX_CANONICAL_OBJECT_BYTES - HEADER_LEN;
26: pub const fn canonical_len(value_len: usize) -> ContentResult<usize> {
50: pub fn encode_bytes_object_to<W: Write>(value: &[u8], writer: &mut W) -> ContentResult<()> {
70: pub fn encode_bytes_object(value: &[u8]) -> ContentResult<Vec<u8>> {
81: pub fn decode_bytes_object(canonical: &[u8]) -> ContentResult<&[u8]> {
```

**core/crates/layerfs-content/src/object/id.rs** [private module] - 7 pub items

```text
13: pub const DIGEST_BYTES: usize = 32;
16: pub const OBJECT_DOMAIN: &[u8] = b"layerfs/object/v2\0";
20: pub struct ObjectId([u8; DIGEST_BYTES]);
24: pub fn for_bytes(canonical: &[u8]) -> Self {
32: pub fn from_bytes(bytes: &[u8]) -> ContentResult<Self> {
44: pub const fn as_bytes(&self) -> &[u8; DIGEST_BYTES] {
49: pub const fn to_bytes(self) -> [u8; DIGEST_BYTES] {
```

**core/crates/layerfs-content/src/object/inode_leaf.rs** [public module path] - 33 pub items

```text
24: pub const INODE_VALUE_BYTES: usize = 73;
26: pub const INODE_LEAF_MAGIC: [u8; 8] = *b"LFS6INT\0";
28: pub const INODE_LEAF_VERSION: u16 = 1;
30: pub const NODE_HEADER_BYTES: usize = 31;
32: pub const LEAF_ROW_BYTES: usize = 81;
34: pub const MAXIMUM_LEAF_ROWS: usize = 100;
36: pub const MINIMUM_LEAF_ROWS: usize = 50;
38: pub const MAXIMUM_NODE_OBJECT_BYTES: usize = 8_192;
40: pub const POOLED_PREFIX_BYTES: usize = 44;
42: pub const POOLED_ROW_BYTES: usize = 12;
44: pub const POOLED_VALUE_MAGIC: [u8; 8] = *b"LFSIVL1\0";
46: pub const POOLED_VALUE_BYTES: usize = 81;
48: pub const POOLED_VALUE_CANONICAL_BYTES: usize = 94;
52: pub enum InodeKind {
63: pub const fn code(self) -> u8 {
72: pub const fn from_code(code: u8) -> ContentResult<Self> {
84: pub struct InodeValue {
101: pub fn validate(self, is_root: bool) -> ContentResult<()> {
117: pub fn encode_inode_value(value: InodeValue) -> [u8; INODE_VALUE_BYTES] {
127: pub fn decode_inode_value(bytes: &[u8]) -> ContentResult<InodeValue> {
144: pub fn encode_pooled_value(value: &[u8; INODE_VALUE_BYTES]) -> ContentResult<Vec<u8>> {
155: pub fn decode_pooled_value(canonical: &[u8]) -> ContentResult<[u8; INODE_VALUE_BYTES]> {
171: pub struct InodeLeafRow {
180: pub struct InodeLeaf {
192: pub fn encode(&self) -> ContentResult<Vec<u8>> {
212: pub fn decode(canonical: &[u8]) -> ContentResult<Self> {
228: pub fn row_count(&self) -> usize {
233: pub fn row_bytes(&self) -> u64 {
358: pub fn pooled_physical_length(canonical_length: usize) -> ContentResult<usize> {
370: pub fn pooled_body(canonical: &[u8], ordinals: &[u32]) -> ContentResult<Vec<u8>> {
386: pub struct PooledRow {
394: pub fn decode_pooled_body(body: &[u8]) -> ContentResult<(Vec<u8>, Vec<PooledRow>)> {
438: pub fn rebuild_leaf( prefix: &[u8], rows: &[PooledRow], values: &[[u8; INODE_VALUE_BYTES]], ) -> ContentResult<Vec<u8>> {
```

**core/crates/layerfs-content/src/object/mod.rs** [public module path] - 7 pub items

```text
13: pub use crate::policy::{MAX_CANONICAL_OBJECT_BYTES, MAX_OBJECT_FIELD_BYTES};
14: pub use access::AuthenticatedObjects;
15: pub use codec::{
19: pub use id::{ObjectId, DIGEST_BYTES, OBJECT_DOMAIN};
20: pub use inode_leaf::{
26: pub use output::{DiscardingConsumer, FinalizedConsumer, FinalizedObject, ObjectRole};
27: pub use predecessor::{
```

**core/crates/layerfs-content/src/object/output.rs** [private module] - 20 pub items

```text
13: pub enum ObjectRole {
47: pub const fn code(self) -> u8 {
66: pub const fn from_code(code: u8) -> ContentResult<Self> {
88: pub struct FinalizedObject {
98: pub fn new(role: ObjectRole, canonical: Vec<u8>) -> ContentResult<Self> {
112: pub fn with_references(mut self, references: Vec<ObjectId>) -> Self {
118: pub fn with_predecessors(mut self, predecessors: AdvisoryPredecessors) -> Self {
124: pub const fn id(&self) -> ObjectId {
129: pub const fn role(&self) -> ObjectRole {
134: pub fn canonical(&self) -> &[u8] {
139: pub fn canonical_len(&self) -> usize {
144: pub fn references(&self) -> &[ObjectId] {
149: pub fn predecessors(&self) -> &AdvisoryPredecessors {
154: pub fn into_parts(self) -> (ObjectId, ObjectRole, Vec<u8>, Vec<ObjectId>) {
163: pub trait FinalizedConsumer {
174: pub struct DiscardingConsumer {
182: pub const fn new() -> Self {
191: pub const fn objects(self) -> u64 {
196: pub const fn canonical_bytes(self) -> u64 {
201: pub const fn peak_object_bytes(self) -> u64 {
```

**core/crates/layerfs-content/src/object/predecessor.rs** [private module] - 13 pub items

```text
14: pub const MAXIMUM_ADVISORY_PREDECESSORS: usize = 4;
18: pub enum PredecessorProvenance {
29: pub struct AdvisoryPredecessor {
36: pub const fn id(self) -> ObjectId {
41: pub const fn provenance(self) -> PredecessorProvenance {
48: pub struct AdvisoryPredecessors {
54: pub fn new() -> Self {
61: pub fn push(&mut self, id: ObjectId, provenance: PredecessorProvenance) -> ContentResult<()> {
77: pub fn len(&self) -> usize {
82: pub fn is_empty(&self) -> bool {
87: pub fn ids(&self) -> impl Iterator<Item = ObjectId> + '_ {
92: pub fn entries(&self) -> &[AdvisoryPredecessor] {
97: pub fn explicit(id: ObjectId) -> ContentResult<Self> {
```

`layerfs-content` pub declarations (all src files): **652**; publicly reachable files: 50 of 69.

### A.2 `layerfs-storage`

**core/crates/layerfs-storage/src/error.rs** [public module path] - 3 pub items

```text
14: pub enum StorageError {
84: pub fn is_unknown_outcome(&self) -> bool {
154: pub type StorageResult<T> = Result<T, StorageError>;
```

**core/crates/layerfs-storage/src/lib.rs** [public module path] - 3 pub items

```text
28: pub use cas::{SaveHandoff, SaveOperation, SaveOutcome, Store, StoreProvider, StoreReadCounters};
29: pub use error::{StorageError, StorageResult};
30: pub use policy::{SchemaIdentity, StorageCapacities, StoragePolicy, SCHEMA_IDENTITY};
```

**core/crates/layerfs-storage/src/policy.rs** [public module path] - 50 pub items

```text
15: pub const FORMAT_PROFILE: u8 = 1;
18: pub const APPLICATION_ID: i64 = 1_279_677_261;
27: pub const SCHEMA_VERSION: i64 = 4;
31: pub struct SchemaIdentity {
39: pub const SCHEMA_IDENTITY: SchemaIdentity = SchemaIdentity {
45: pub const GROUP_LIMIT: usize = 65_536;
53: pub const GROUP_TARGET: usize = 48 * 1024;
55: pub const PACK_LIMIT: usize = 256 * 1024;
57: pub const GROUP_COUNT_LIMIT: usize = 256;
59: pub const RECORD_COUNT_LIMIT: usize = 8_191;
61: pub const CANONICAL_LIMIT: usize = 16 * 1024 * 1024;
63: pub const LOOKUP_PAGE_IDS: usize = 128;
65: pub const BATCH_OBJECT_LIMIT: usize = 512;
67: pub const BATCH_CANONICAL_BYTES_LIMIT: u64 = 512 * 1024;
69: pub const TRANSACTION_ROW_LIMIT: u64 = 8_191;
71: pub const TRANSACTION_CANONICAL_BYTES_LIMIT: u64 = 4 * 1024 * 1024 - 1;
73: pub const CLEANUP_PAGE_ROWS: usize = 128;
80: pub const READ_OBJECT_LIMIT: usize = 4_096;
85: pub const WHOLE_FILE_CANONICAL_OVERHEAD: usize = 23;
88: pub const SINGLETON_FRAMING_SLACK: usize = 4_096;
90: pub const SINGLETON_PACK_LIMIT: usize = CANONICAL_LIMIT + SINGLETON_FRAMING_SLACK;
93: pub const VALUES_PER_GROUP: usize = 165;
95: pub const METADATA_GROUP_LIMIT: usize = 16 * 1024;
97: pub const POOLED_LEAF_ROWS_LIMIT: usize = 100;
102: pub const CHAIN_CANONICAL_LIMIT: u64 = 512 * 1024;
104: pub const CHAIN_ENCODED_LIMIT: u64 = 256 * 1024;
113: pub const DEPENDENCY_PACK_CACHE_BYTES: usize = 4 * 1024 * 1024;
121: pub const POOLED_VALUE_CACHE_BYTES: usize = 512 * 1024;
123: pub const METADATA_DECODED_WORK_LIMIT: u64 = 32 * 1024 * 1024;
125: pub const DEFAULT_METADATA_DELTA_MAX_DEPTH: u8 = 8;
127: pub const METADATA_CHAIN_CANONICAL_LIMIT: u64 = 8 * 8_192;
129: pub const METADATA_CHAIN_ENCODED_LIMIT: u64 = 17 * 8_193;
131: pub const METADATA_RECORD_LIMIT: usize = 8_192;
133: pub const METADATA_MATCH_BUDGET_BYTES: usize = 128 * 1024;
135: pub const INODE_LEAF_LIMIT: usize = 8_192;
142: pub const METADATA_INDEX_VALUES: usize = 131_072;
145: pub struct StoragePolicy {
155: pub const fn frozen_default() -> Self {
167: pub const fn new( format_profile: u8, small_file_threshold_bytes: u64, whole_file_delta_max_depth: u8, chunk_delta_max_depth: u8, ) -> Self {
183: pub const fn with_metadata_depth(mut self, metadata_delta_max_depth: u8) -> Self {
189: pub fn validated(self) -> StorageResult<Self> {
214: pub const fn format_profile(self) -> u8 {
219: pub const fn small_file_threshold_bytes(self) -> u64 {
224: pub const fn whole_file_delta_max_depth(self) -> u8 {
229: pub const fn chunk_delta_max_depth(self) -> u8 {
234: pub const fn metadata_delta_max_depth(self) -> u8 {
239: pub const fn construction(self) -> ConstructionPolicy {
256: pub struct StorageCapacities {
305: pub fn from_policy(policy: StoragePolicy) -> StorageResult<Self> {
337: pub const fn delta_depth_for_role(self, role: layerfs_content::ObjectRole) -> u8 {
```

**core/crates/layerfs-storage/src/cas/batch.rs** [private module] - 7 pub items

```text
15: pub struct PendingBatch {
24: pub fn new(capacities: StorageCapacities) -> Self {
34: pub fn len(&self) -> usize {
39: pub fn canonical_bytes(&self) -> u64 {
48: pub fn push(&mut self, object: FinalizedObject) -> StorageResult<Option<Vec<FinalizedObject>>> {
72: pub fn drain(&mut self) -> Vec<FinalizedObject> {
81: pub fn pending_canonical(&self, id: ObjectId) -> Option<&[u8]> {
```

**core/crates/layerfs-storage/src/cas/dependencies.rs** [private module] - 5 pub items

```text
20: pub struct Availability {
26: pub fn new(present: impl IntoIterator<Item = ObjectId>) -> Self {
33: pub fn inserted(&mut self, id: ObjectId) {
38: pub fn known(&self, id: ObjectId) -> bool {
47: pub fn validate( &mut self, connection: &Connection, object: &FinalizedObject, ceiling: i64, pending: impl Fn(ObjectId) -> bool, ) -> StorageResult<()> { ...
```

**core/crates/layerfs-storage/src/cas/finish.rs** [private module] - 1 pub items

```text
12: pub fn terminate(owner: &mut MutationOwner, error: StorageError) -> StorageError {
```

**core/crates/layerfs-storage/src/cas/membership.rs** [private module] - 2 pub items

```text
15: pub fn stored_canonical( owner: &mut MutationOwner, location: ObjectLocation, ) -> StorageResult<Vec<u8>> {
30: pub fn reuse_or_collide( owner: &mut MutationOwner, object: &FinalizedObject, location: ObjectLocation, ) -> StorageResult<()> {
```

**core/crates/layerfs-storage/src/cas/mod.rs** [public module path] - 4 pub items

```text
15: pub use owner::{OutcomeCounters, PoolCounters};
16: pub use provider::StoreProvider;
17: pub use read::ReadCounters;
18: pub use store::{SaveHandoff, SaveOperation, SaveOutcome, Store, StoreReadCounters};
```

**core/crates/layerfs-storage/src/cas/owner.rs** [private module] - 23 pub items

```text
73: pub struct OutcomeCounters {
99: pub struct MutationOwner {
148: pub struct PoolCounters {
169: pub fn acquire( connection: Connection, capacities: StorageCapacities, pool_index: std::sync::Arc<std::sync::Mutex<crate::encoding::pool::PoolIndex>>, ) -> StorageResult<Self> {
239: pub fn baseline_pack_id(&self) -> i64 {
244: pub fn connection(&self) -> &Connection {
253: pub fn pending_member(&self, id: ObjectId) -> bool {
260: pub fn note_reuse(&mut self) {
265: pub fn retained_tail_bytes(&self) -> StorageResult<usize> {
284: pub fn seal_pending(&mut self, ids: &[ObjectId]) -> StorageResult<()> {
308: pub fn read_batch(&mut self, ids: &[ObjectId]) -> StorageResult<Vec<Vec<u8>>> {
320: pub fn resolve_location(&mut self, location: lookup::ObjectLocation) -> StorageResult<Vec<u8>> {
343: pub fn offer( &mut self, object: &FinalizedObject, advisory: &[ObjectId], availability: &mut Availability, ) -> StorageResult<()> {
437: pub fn delta_counters(&self) -> DeltaCounters {
444: pub fn chain_counters(&self) -> ChainCounters {
449: pub fn candidate_index_bytes(&self) -> usize {
762: pub fn pool_counters(&self) -> PoolCounters {
767: pub fn seal_group( &mut self, lane: PackLane, availability: &mut Availability, ) -> StorageResult<()> {
840: pub fn maybe_commit(&mut self) -> StorageResult<()> {
863: pub fn finish(&mut self) -> StorageResult<OutcomeCounters> {
923: pub fn abandon(&mut self) -> StorageResult<()> {
942: pub fn mark_terminal(&mut self) {
950: pub fn quarantine(&mut self) {
```

**core/crates/layerfs-storage/src/cas/provider.rs** [private module] - 3 pub items

```text
19: pub struct StoreProvider<'a> {
25: pub const fn new(store: &'a Store) -> Self {
35: pub fn read_wave( &self, ids: &[ObjectId], scope: TimingScope<'_>, ) -> StorageResult<(Vec<Vec<u8>>, StoreReadCounters)> {
```

**core/crates/layerfs-storage/src/cas/read.rs** [private module] - 2 pub items

```text
24: pub struct ReadCounters {
42: pub fn read_objects( connection: &Connection, ids: &[ObjectId], ceiling: i64, capacities: &StorageCapacities, workspace: &mut DecompressionWorkspace, ) -> StorageResult<(Vec<Vec<u8>>, ReadCounters)> { ...
```

**core/crates/layerfs-storage/src/cas/save.rs** [private module] - 1 pub items

```text
22: pub fn flush_batch(owner: &mut MutationOwner, objects: Vec<FinalizedObject>) -> StorageResult<()> {
```

**core/crates/layerfs-storage/src/cas/store.rs** [private module] - 33 pub items

```text
28: pub struct SaveOutcome {
76: pub struct StoreReadCounters {
95: pub struct Store {
109: pub fn create( path: impl AsRef<Path>, policy: StoragePolicy, scope: TimingScope<'_>, ) -> StorageResult<Self> {
132: pub fn open(path: impl AsRef<Path>, scope: TimingScope<'_>) -> StorageResult<Self> {
149: pub const fn default_policy() -> StoragePolicy {
154: pub fn policy(&self) -> StoragePolicy {
159: pub fn capacities(&self) -> StorageCapacities {
164: pub fn path(&self) -> &Path {
172: pub fn pool_index_entries(&self) -> usize {
177: pub fn pool_index_bytes(&self) -> usize {
185: pub fn begin_save(&self, scope: TimingScope<'_>) -> StorageResult<SaveOperation> {
202: pub fn read_batch( &self, ids: &[ObjectId], scope: TimingScope<'_>, ) -> StorageResult<(Vec<Vec<u8>>, StoreReadCounters)> {
236: pub fn contains( &self, ids: &[ObjectId], scope: TimingScope<'_>, ) -> StorageResult<Vec<ObjectId>> {
271: pub struct SaveOperation {
291: pub fn accept(&mut self, object: FinalizedObject) -> StorageResult<()> {
314: pub fn finish(mut self, scope: TimingScope<'_>) -> StorageResult<SaveOutcome> {
341: pub fn read_batch( &mut self, ids: &[ObjectId], scope: TimingScope<'_>, ) -> StorageResult<Vec<Vec<u8>>> {
406: pub fn capacities(&self) -> StorageCapacities {
411: pub fn policy(&self) -> StoragePolicy {
416: pub fn baseline_pack_id(&self) -> StorageResult<i64> {
424: pub fn retained_tail_bytes(&self) -> StorageResult<usize> {
432: pub fn pending(&self) -> (usize, u64) {
437: pub fn delta_counters(&self) -> DeltaCounters {
445: pub fn pool_counters(&self) -> crate::cas::owner::PoolCounters {
453: pub fn chain_counters(&self) -> ChainCounters {
461: pub fn candidate_index_bytes(&self) -> usize {
469: pub fn abort(mut self, scope: TimingScope<'_>) -> StorageResult<()> {
509: pub struct SaveHandoff<'a> {
516: pub fn new(operation: &'a mut SaveOperation) -> Self {
524: pub fn failure(&self) -> Option<&StorageError> {
529: pub fn take_failure(&mut self) -> Option<StorageError> {
534: pub fn operation(&mut self) -> &mut SaveOperation {
```

**core/crates/layerfs-storage/src/encoding/codec.rs** [public module path] - 23 pub items

```text
33: pub const ENCODE_WORKSPACE_BYTES: usize = 2 * 1024 * 1024;
35: pub const DECODE_WORKSPACE_BYTES: usize = 1024 * 1024;
37: pub const GROUP_LIMIT: usize = 65_536;
39: pub const GROUP_FRAME_LIMIT: usize = GROUP_LIMIT + 1024;
52: pub struct CodecProfile {
60: pub const fn native() -> Self {
69: pub const fn whole_file(capacities: &StorageCapacities) -> Self {
78: pub const fn raw_limit(self) -> usize {
83: pub const fn frame_limit(self) -> usize {
88: pub const fn window_log(self) -> i32 {
151: pub struct CompressionWorkspace {
158: pub fn new() -> StorageResult<Self> {
175: pub fn workspace_bytes(&self) -> usize {
180: pub fn compress(&mut self, profile: CodecProfile, raw: &[u8]) -> StorageResult<Vec<u8>> {
242: pub fn compress_prefix( &mut self, profile: CodecProfile, raw: &[u8], prefix: &[u8], ) -> StorageResult<Vec<u8>> {
325: pub fn compress_group(&mut self, raw: &[u8]) -> StorageResult<Vec<u8>> {
381: pub struct DecompressionWorkspace {
395: pub fn new() -> StorageResult<Self> {
423: pub fn workspace_bytes(&self) -> usize {
428: pub fn decompress( &mut self, profile: CodecProfile, frame: &[u8], raw_length: usize, ) -> StorageResult<Vec<u8>> {
485: pub fn decompress_prefix( &mut self, profile: CodecProfile, frame: &[u8], raw_length: usize, prefix: &[u8], ) -> StorageResult<Vec<u8>> { ...
551: pub fn decompress_group(&mut self, frame: &[u8], raw_length: usize) -> StorageResult<Vec<u8>> {
633: pub fn compress_group_body( workspace: &mut CompressionWorkspace, raw: &[u8], ) -> StorageResult<Option<Vec<u8>>> {
```

**core/crates/layerfs-storage/src/encoding/decode.rs** [private module] - 3 pub items

```text
28: pub fn decode_canonical( pack: &[u8], location: &ObjectLocation, capacities: &StorageCapacities, base: Option<&[u8]>, workspace: &mut DecompressionWorkspace, ) -> StorageResult<Vec<u8>> { ...
159: pub fn group_records(group: &[u8]) -> StorageResult<Vec<&[u8]>> {
176: pub fn framed_record(group: &[u8], ordinal: usize) -> StorageResult<&[u8]> {
```

**core/crates/layerfs-storage/src/encoding/full.rs** [private module] - 6 pub items

```text
25: pub struct EncodedRecord {
40: pub fn width(&self) -> usize {
46: pub fn raw_payload(canonical: &[u8], role: ObjectRole) -> StorageResult<&[u8]> {
77: pub fn encode_full( canonical: &[u8], role: ObjectRole, capacities: &StorageCapacities, workspace: &mut CompressionWorkspace, ) -> StorageResult<EncodedRecord> {
87: pub fn encode_prefix( canonical: &[u8], role: ObjectRole, base_id: ObjectId, base_raw: &[u8], capacities: &StorageCapacities, workspace: &mut CompressionWorkspace, ...
217: pub fn lane_body_limit(lane: PackLane, capacities: &StorageCapacities) -> usize {
```

**core/crates/layerfs-storage/src/encoding/mod.rs** [public module path] - 3 pub items

```text
12: pub use codec::{
16: pub use decode::{decode_canonical, framed_record, group_records};
17: pub use full::{encode_full, encode_prefix, lane_body_limit, raw_payload, EncodedRecord};
```

**core/crates/layerfs-storage/src/encoding/delta/candidates.rs** [public module path] - 7 pub items

```text
15: pub const INDEX_BYTES: usize = 128 * 1024;
29: pub struct Candidates {
57: pub fn signature(raw: &[u8]) -> [u64; 8] {
85: pub fn new() -> StorageResult<Self> {
109: pub fn live_bytes(&self) -> usize {
114: pub fn insert(&mut self, id: ObjectId, signature: [u64; 8]) {
139: pub fn find(&self, target_id: ObjectId, signature: &[u64; 8]) -> Option<ObjectId> {
```

**core/crates/layerfs-storage/src/encoding/delta/read.rs** [public module path] - 10 pub items

```text
27: pub struct ChainCounters {
41: pub struct Resolver<'a> {
53: pub fn new( connection: &'a Connection, ceiling: i64, capacities: &'a StorageCapacities, packs: &'a mut BTreeMap<i64, Vec<u8>>, workspace: &'a mut DecompressionWorkspace, counters: &'a mut ChainCounters, ...
77: pub const fn packs_read(&self) -> u64 {
82: pub fn resolve(&mut self, id: ObjectId) -> StorageResult<Vec<u8>> {
93: pub fn resolve_dependency(&mut self, id: ObjectId) -> StorageResult<Vec<u8>> {
105: pub fn resolve_at(&mut self, root: ObjectLocation) -> StorageResult<Vec<u8>> {
232: pub fn accumulate(total: &mut ChainCounters, chain: ChainCounters) {
269: pub fn locator_key(location: &ObjectLocation) -> (i64, usize, usize) {
282: pub fn record_width(pack: &[u8], location: &ObjectLocation) -> StorageResult<u64> {
```

**core/crates/layerfs-storage/src/encoding/delta/record.rs** [public module path] - 8 pub items

```text
15: pub const FULL_TAG: u8 = 0;
17: pub const PREFIX_TAG: u8 = 1;
26: pub struct ParsedRecord<'a> {
38: pub fn record_width(lane: PackLane, base: Option<ObjectId>, frame_length: usize) -> usize {
49: pub fn encode( lane: PackLane, raw_length: usize, base: Option<ObjectId>, frame: &[u8], ) -> StorageResult<Vec<u8>> {
98: pub fn parse( lane: PackLane, record: &[u8], canonical_length: usize, ) -> StorageResult<ParsedRecord<'_>> {
161: pub fn parse_native(record: &[u8]) -> StorageResult<ParsedRecord<'_>> {
238: pub const fn compact_framing_is_dropped(lane: PackLane) -> bool {
```

**core/crates/layerfs-storage/src/encoding/delta/select.rs** [public module path] - 11 pub items

```text
31: pub struct DeltaCounters {
56: pub struct ChainCost {
65: pub struct DepthCache {
71: pub fn new() -> Self {
76: pub fn len(&self) -> usize {
81: pub fn is_empty(&self) -> bool {
86: pub fn depth_of(&mut self, connection: &Connection, id: ObjectId) -> StorageResult<Option<u8>> {
94: pub fn cost_of( &mut self, connection: &Connection, id: ObjectId, ) -> StorageResult<Option<ChainCost>> {
147: pub fn record(&mut self, id: ObjectId, cost: ChainCost) {
157: pub struct SelectInput<'a> {
196: pub fn select( input: &mut SelectInput<'_>, id: ObjectId, canonical: &[u8], role: ObjectRole, advisory: &[ObjectId], encode: &mut CompressionWorkspace, ...
```

**core/crates/layerfs-storage/src/encoding/pool/delta.rs** [public module path] - 4 pub items

```text
18: pub const PROGRAM_LIMIT: usize = 64 * 1024;
21: pub fn validate(instructions: &[u8], count: usize, output_length: usize) -> StorageResult<()> {
72: pub fn apply( base: &[u8], instructions: &[u8], count: usize, output_length: usize, ) -> StorageResult<Vec<u8>> {
125: pub fn build( base_id: ObjectId, base: &[u8], target: &[u8], remaining: &mut usize, ) -> StorageResult<Option<Vec<u8>>> {
```

**core/crates/layerfs-storage/src/encoding/pool/index.rs** [public module path] - 10 pub items

```text
25: pub struct PoolIndex {
33: pub fn new() -> Self {
42: pub fn len(&self) -> usize {
47: pub fn is_empty(&self) -> bool {
52: pub fn live_bytes(&self) -> usize {
60: pub fn invalidate(&mut self) {
72: pub fn note_group( &mut self, first_ordinal: u32, values: &[[u8; INODE_VALUE_BYTES]], ) -> StorageResult<()> {
100: pub fn sync( &mut self, connection: &Connection, capacities: &StorageCapacities, ceiling: i64, reader: &mut PoolReader, workspace: &mut DecompressionWorkspace, ...
165: pub fn find( &mut self, connection: &Connection, capacities: &StorageCapacities, ceiling: i64, reader: &mut PoolReader, workspace: &mut DecompressionWorkspace, ...
259: pub fn value_of(canonical: &[u8]) -> StorageResult<[u8; INODE_VALUE_BYTES]> {
```

**core/crates/layerfs-storage/src/encoding/pool/leaf.rs** [public module path] - 9 pub items

```text
26: pub const POOLED_FULL_TAG: u8 = 0;
28: pub const POOLED_DELTA_TAG: u8 = 1;
32: pub enum PooledRecord<'a> {
49: pub fn encode_full(body: &[u8]) -> StorageResult<Vec<u8>> {
65: pub fn parse(record: &[u8]) -> StorageResult<PooledRecord<'_>> {
96: pub fn physical_length(canonical_length: usize) -> StorageResult<usize> {
101: pub fn canonical_length(rows: usize) -> StorageResult<usize> {
114: pub fn ordinals(body: &[u8]) -> StorageResult<Vec<u32>> {
129: pub fn rows(body: &[u8]) -> StorageResult<Vec<PooledRow>> {
```

**core/crates/layerfs-storage/src/encoding/pool/mod.rs** [public module path] - 4 pub items

```text
11: pub use index::PoolIndex;
12: pub use leaf::{PooledRecord, POOLED_DELTA_TAG, POOLED_FULL_TAG};
13: pub use read::PoolReader;
14: pub use value_group::BuiltGroup;
```

**core/crates/layerfs-storage/src/encoding/pool/read.rs** [public module path] - 10 pub items

```text
27: pub struct PoolReader {
38: pub fn new() -> Self {
46: pub fn begin_chain(&mut self) {
51: pub fn retained_bytes(&self) -> usize {
56: pub fn decoded_work(&self) -> u64 {
65: pub fn chain_encoded_bytes(&self) -> u64 {
70: pub fn group_values( &mut self, connection: &Connection, capacities: &StorageCapacities, ceiling: i64, workspace: &mut DecompressionWorkspace, row: &pool::ValueGroupRow, ...
90: pub fn group_value( &mut self, connection: &Connection, capacities: &StorageCapacities, ceiling: i64, workspace: &mut DecompressionWorkspace, row: &pool::ValueGroupRow, ...
202: pub fn leaf_body( &mut self, connection: &Connection, capacities: &StorageCapacities, ceiling: i64, workspace: &mut DecompressionWorkspace, root: ObjectLocation, ...
316: pub fn leaf_canonical( &mut self, connection: &Connection, capacities: &StorageCapacities, ceiling: i64, workspace: &mut DecompressionWorkspace, root: ObjectLocation, ...
```

**core/crates/layerfs-storage/src/encoding/pool/value_group.rs** [public module path] - 6 pub items

```text
19: pub struct BuiltGroup {
31: pub fn build( values: &[Vec<u8>], workspace: &mut CompressionWorkspace, ) -> StorageResult<BuiltGroup> {
68: pub fn authenticate(body: &[u8], digest: ObjectId) -> StorageResult<()> {
79: pub fn decode(body: &[u8], expected: usize) -> StorageResult<Vec<Vec<u8>>> {
96: pub fn canonical_value(value: &[u8; 73]) -> StorageResult<Vec<u8>> {
101: pub const fn lane() -> PackLane {
```

**core/crates/layerfs-storage/src/pack/assemble.rs** [public module path] - 8 pub items

```text
18: pub const FULL_TAG: u8 = 0;
21: pub fn frame_group(records: &[Vec<u8>]) -> StorageResult<Vec<u8>> {
29: pub fn frame_group_bounded(records: &[Vec<u8>], limit: usize) -> StorageResult<Vec<u8>> {
84: pub fn framed_group_length(records: usize, payload: usize) -> StorageResult<usize> {
98: pub fn framed_length(records: &[Vec<u8>]) -> StorageResult<usize> {
111: pub fn build_group( lane: PackLane, records: &[Vec<u8>], workspace: Option<&mut CompressionWorkspace>, ) -> StorageResult<EncodedGroup> {
194: pub fn assemble(lane: PackLane, groups: &[EncodedGroup]) -> StorageResult<Vec<u8>> {
215: pub fn assemble_consuming(lane: PackLane, groups: Vec<EncodedGroup>) -> StorageResult<Vec<u8>> {
```

**core/crates/layerfs-storage/src/pack/layout.rs** [public module path] - 30 pub items

```text
19: pub const PACK_MAGIC: [u8; 8] = *b"LFPACK\0\0";
21: pub const HEADER_LEN: usize = 16;
23: pub const DIRECTORY_ENTRY_LEN: usize = 16;
25: pub const WHOLE_FILE_ENTRY_LEN: usize = 4;
27: pub const WHOLE_FILE_COMPACT_DROP: usize = 8;
30: pub const VERSION_ORDINARY: u32 = 1;
32: pub const VERSION_NATIVE: u32 = 2;
34: pub const VERSION_WHOLE_FILE: u32 = 4;
36: pub const VERSION_POOLED_METADATA: u32 = 6;
38: pub const VERSION_SINGLETON: u32 = 7;
42: pub enum PackLane {
57: pub const ALL: [Self; 5] = [ Self::Ordinary, Self::Native, Self::WholeFile, Self::PooledMetadata, Self::Singleton, ]; ...
66: pub const fn version(self) -> u32 {
77: pub const fn index(self) -> usize {
88: pub const fn for_role(role: ObjectRole) -> Self {
107: pub const fn pack_limit(self) -> usize {
115: pub const fn directory_is_starts_only(self) -> bool {
120: pub const fn body_limit(self) -> usize {
129: pub const fn group_count_limit(self) -> usize {
141: pub enum GroupCodec {
150: pub struct EncodedGroup {
167: pub fn body_size(&self, lane: PackLane) -> StorageResult<usize> {
183: pub const fn directory_entry_len(lane: PackLane) -> usize {
199: pub fn append_fits( lane: PackLane, groups: &[EncodedGroup], group: &EncodedGroup, ) -> StorageResult<bool> {
216: pub fn assembled_length(lane: PackLane, groups: &[EncodedGroup]) -> StorageResult<usize> {
238: pub struct PackHeader {
246: pub fn parse_header(bytes: &[u8]) -> StorageResult<PackHeader> {
301: pub struct GroupView {
313: pub fn group_view(bytes: &[u8], header: PackHeader, group: usize) -> StorageResult<GroupView> {
451: pub fn record_range( count: usize, ends: &[u8], group_length: usize, ordinal: usize, ) -> StorageResult<(usize, usize)> {
```

**core/crates/layerfs-storage/src/pack/mod.rs** [public module path] - 3 pub items

```text
9: pub use assemble::{
13: pub use layout::{
19: pub use placement::{LanePlacement, PlacedGroup, SelectedWrite};
```

**core/crates/layerfs-storage/src/pack/placement.rs** [public module path] - 6 pub items

```text
22: pub struct SelectedWrite {
35: pub struct PlacedGroup {
44: pub struct LanePlacement {
50: pub fn new() -> Self {
55: pub fn retained_bytes(&self, lane: PackLane) -> StorageResult<usize> {
66: pub fn select_many( &mut self, lane: PackLane, groups: Vec<EncodedGroup>, next_pack_id: &mut i64, ) -> StorageResult<Vec<SelectedWrite>> {
```

**core/crates/layerfs-storage/src/sqlite/cleanup.rs** [public module path] - 2 pub items

```text
22: pub struct CleanupReport {
35: pub fn abandon(connection: &Connection, baseline_pack_id: i64) -> StorageResult<CleanupReport> {
```

**core/crates/layerfs-storage/src/sqlite/connection.rs** [public module path] - 5 pub items

```text
17: pub fn open(path: &Path, create: bool) -> StorageResult<Connection> {
29: pub fn configure(connection: &Connection) -> StorageResult<()> {
48: pub fn pragma_i64(connection: &Connection, name: &str) -> StorageResult<i64> {
54: pub fn is_busy(error: &rusqlite::Error) -> bool {
66: pub fn ownership_error(error: rusqlite::Error) -> StorageError {
```

**core/crates/layerfs-storage/src/sqlite/lookup.rs** [public module path] - 7 pub items

```text
18: pub struct ObjectLocation {
36: pub fn pages(ids: &[ObjectId]) -> impl Iterator<Item = &[ObjectId]> {
52: pub fn locations( connection: &Connection, ids: &[ObjectId], ceiling: i64, ) -> StorageResult<Vec<ObjectLocation>> {
88: pub fn location( connection: &Connection, id: ObjectId, ceiling: i64, ) -> StorageResult<Option<ObjectLocation>> {
126: pub fn present( connection: &Connection, ids: &[ObjectId], ceiling: i64, ) -> StorageResult<Vec<ObjectId>> {
152: pub fn pack_bytes(connection: &Connection, pack_id: i64) -> StorageResult<Vec<u8>> {
166: pub fn highest_pack_id(connection: &Connection) -> StorageResult<i64> {
```

**core/crates/layerfs-storage/src/sqlite/mod.rs** [public module path] - 4 pub items

```text
12: pub use cleanup::CleanupReport;
13: pub use lookup::ObjectLocation;
14: pub use pool::ValueGroupRow;
15: pub use write::{ObjectRow, TransactionState};
```

**core/crates/layerfs-storage/src/sqlite/pool.rs** [public module path] - 7 pub items

```text
20: pub struct ValueGroupRow {
44: pub fn ordinal_end(connection: &Connection) -> StorageResult<u64> {
64: pub fn next_ordinal(connection: &Connection) -> StorageResult<u32> {
70: pub fn insert_group(connection: &Connection, row: &ValueGroupRow) -> StorageResult<()> {
92: pub fn group_for(connection: &Connection, ordinal: u32) -> StorageResult<Option<ValueGroupRow>> {
154: pub fn for_each_group( connection: &Connection, from: Option<u32>, mut visit: impl FnMut(ValueGroupRow) -> StorageResult<()>, ) -> StorageResult<()> {
171: pub fn group_count(connection: &Connection) -> StorageResult<i64> {
```

**core/crates/layerfs-storage/src/sqlite/schema.rs** [public module path] - 7 pub items

```text
15: pub const SCHEMA_SQL: &str = include_str!("../../sql/schema.sql");
64: pub fn create(connection: &Connection, policy: StoragePolicy) -> StorageResult<StoragePolicy> {
89: pub fn validate( connection: &Connection, expected: Option<StoragePolicy>, ) -> StorageResult<StoragePolicy> {
138: pub fn identity(connection: &Connection, expected: SchemaIdentity) -> StorageResult<()> {
254: pub fn retained_pack_ceiling(connection: &Connection) -> StorageResult<i64> {
273: pub fn advance_retained_pack_ceiling(connection: &Connection, ceiling: i64) -> StorageResult<()> {
286: pub fn existing_object_count(connection: &Connection) -> StorageResult<i64> {
```

**core/crates/layerfs-storage/src/sqlite/write.rs** [public module path] - 8 pub items

```text
18: pub struct TransactionState {
27: pub struct ObjectRow {
51: pub fn begin_immediate(connection: &Connection) -> StorageResult<()> {
58: pub fn commit(connection: &Connection) -> StorageResult<()> {
67: pub fn rollback(connection: &Connection) -> StorageResult<()> {
76: pub fn insert_pack(connection: &Connection, pack_id: i64, bytes: &[u8]) -> StorageResult<()> {
88: pub fn append_pack(connection: &Connection, pack_id: i64, bytes: &[u8]) -> StorageResult<()> {
100: pub fn insert_object(connection: &Connection, row: &ObjectRow) -> StorageResult<()> {
```

`layerfs-storage` pub declarations (all src files): **338**; publicly reachable files: 28 of 39.

### A.3 `layerfs-telemetry`

**core/crates/layerfs-telemetry/src/timer/format.rs** [private module] - 1 pub items

```text
14: pub fn write_text(&self, mut writer: impl Write) -> io::Result<()> {
```

**core/crates/layerfs-telemetry/src/timer/json.rs** [private module] - 1 pub items

```text
18: pub fn write_json(&self, mut writer: impl Write) -> io::Result<()> {
```

**core/crates/layerfs-telemetry/src/timer/mod.rs** [public module path] - 3 pub items

```text
23: pub use recording::{MAX_DEPTH, MAX_LABEL_BYTES, MAX_NODES};
24: pub use report::{NodeOutcome, TimingNode, TimingReport};
25: pub use scope::{Active, Pending, Timing, TimingScope};
```

**core/crates/layerfs-telemetry/src/timer/recording.rs** [private module] - 3 pub items

```text
14: pub const MAX_NODES: usize = 1_024;
17: pub const MAX_DEPTH: u8 = 32;
20: pub const MAX_LABEL_BYTES: usize = 128;
```

**core/crates/layerfs-telemetry/src/timer/report.rs** [private module] - 24 pub items

```text
10: pub enum NodeOutcome {
19: pub const fn is_error(self) -> bool {
38: pub struct TimingNode {
61: pub fn new(name: impl Into<Cow<'static, str>>, elapsed: Duration) -> Self {
74: pub fn with_outcome(mut self, outcome: NodeOutcome) -> Self {
80: pub fn with_incomplete(mut self, incomplete: bool) -> Self {
92: pub fn with_children(mut self, children: Vec<TimingNode>) -> Self {
104: pub fn push_child(&mut self, child: TimingNode) -> bool {
121: pub fn name(&self) -> &str {
126: pub fn elapsed(&self) -> Duration {
131: pub fn outcome(&self) -> NodeOutcome {
136: pub fn is_incomplete(&self) -> bool {
141: pub fn children(&self) -> &[TimingNode] {
146: pub fn node_count(&self) -> usize {
151: pub fn levels(&self) -> usize {
196: pub struct TimingReport {
202: pub const fn disabled() -> Self {
207: pub fn from_root(root: TimingNode) -> Self {
212: pub fn root(&self) -> Option<&TimingNode> {
217: pub fn into_root(self) -> Option<TimingNode> {
222: pub const fn has_root(&self) -> bool {
227: pub fn node_count(&self) -> usize {
235: pub fn levels(&self) -> usize {
243: pub fn is_incomplete(&self) -> bool {
```

**core/crates/layerfs-telemetry/src/timer/scope.rs** [private module] - 10 pub items

```text
20: pub struct Timing;
23: pub struct Pending;
26: pub struct Active;
36: pub struct TimingScope<'a, S = Pending> {
61: pub fn is_recording(&self) -> bool {
93: pub fn child(&self, name: impl Into<Cow<'static, str>>) -> TimingScope<'_, Pending> {
106: pub fn attach(&self, report: Option<TimingReport>) {
140: pub fn run<T, E, F>(self, operation: F) -> Result<T, E> where F: FnOnce(&TimingScope<'_, Active>) -> Result<T, E>, {
172: pub fn record<T, E, F>( name: impl Into<Cow<'static, str>>, operation: F, ) -> (Result<T, E>, TimingReport) where F: FnOnce(&TimingScope<'_, Active>) -> Result<T, E>, { ...
194: pub fn disabled<T, E, F>( name: impl Into<Cow<'static, str>>, operation: F, ) -> (Result<T, E>, TimingReport) where F: FnOnce(&TimingScope<'_, Active>) -> Result<T, E>, { ...
```

`layerfs-telemetry` pub declarations (all src files): **42**; publicly reachable files: 2 of 7.

**Appendix A total: 1032 pub declarations across the three crates (115 src files).**
