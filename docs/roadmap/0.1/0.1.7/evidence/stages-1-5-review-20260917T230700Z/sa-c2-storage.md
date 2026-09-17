# Independent review: C2 storage component (Stages 2-4)

- **Reviewer task id:** sa-c2-storage
- **Target:** `/Users/yifanxu/Ephemeral-AI-Lab/layerfs`, branch `main`, HEAD `f288d2af7ecdc7e00f7df153073398d333461aa3`
- **Scope:** `core/crates/layerfs-storage/src/**`, `core/crates/layerfs-storage/sql/schema.sql`, and the public seam it exposes.
- **Method:** direct source reading of every file in `src/` (44 files) plus the three contracts, which are treated as **claims**. No test was executed and no benchmark was run; every statement below is either a quoted product-code line or is explicitly marked UNVERIFIED.
- **Note on attribution:** a roadmap sentence is reported as *"the report claims X"*; a code line is reported as *"the code at `path:line` does X"*. They are never merged.
- **Production rules checked against the tree:** no inline tests in `src/`; no retry / busy handler / error-driven fallback; no `fsync`/`fdatasync`/`sync_all`/`sync_data`; no WAL; MEMORY journal + `synchronous = OFF` + zero busy timeout; no third-party patch.

## Verdict summary

| # | Required check | Verdict |
|---|---|---|
| 1 | Public seam signatures, ownership, limits, backpressure, cancellation | **PARTIAL** (seam correct; `StoreProvider` flattens every failure to `MissingObject`, F4) |
| 2 | SQLite profile actually applied; no retry/handler/fallback; no fsync; no WAL | **PASS** |
| 3 | Schema enumeration; behaviour at open on older/different schema version | **PASS** (refuses; never migrates; drift between the SQL file and the Rust constant is caught by `create`) |
| 4 | Exact CAS reuse, digest verified on reuse, one delta trial, candidate ordering | **PASS**, with one wording correction (the trial is one *per target*, not one per save, F8) |
| 5 | FULL/pack locator reads, bounds, decompression limits, corrupt-locator handling | **PASS** (one caveat: the pack BLOB is materialised before its width is validated, F3) |
| 6 | Transactions, publication watermark, single acknowledgement, failure isolation | **PASS** |
| 7 | Payload/metadata chain caps, pooled window/index bounds, retained cache bounds | **PASS** (transaction byte/row caps are triggers, not hard caps, F1) |
| 8 | Limits with source/unit/scope | **PASS**, with two deviations recorded (F1, F5) |

---

## 1. Public seam

### 1.1 Signatures

Re-exports, `src/lib.rs:21-30`:

    pub mod cas;
    pub mod encoding;
    pub mod error;
    pub mod pack;
    pub mod policy;
    pub mod sqlite;

    pub use cas::{SaveHandoff, SaveOperation, SaveOutcome, Store, StoreProvider, StoreReadCounters};
    pub use error::{StorageError, StorageResult};
    pub use policy::{SchemaIdentity, StorageCapacities, StoragePolicy, SCHEMA_IDENTITY};

`cas/mod.rs:15-18` adds `pub use owner::{OutcomeCounters, PoolCounters}; pub use provider::StoreProvider; pub use read::ReadCounters; pub use store::{SaveHandoff, SaveOperation, SaveOutcome, Store, StoreReadCounters};`.

`AuthenticatedObjects` is **not** re-exported by this crate: it is defined in C1 and only *implemented* here.

| Item | Exact current signature | Evidence |
|---|---|---|
| `Store::create` | `pub fn create(path: impl AsRef<Path>, policy: StoragePolicy, scope: TimingScope<'_>) -> StorageResult<Self>` | `src/cas/store.rs:109-113` |
| `Store::open` | `pub fn open(path: impl AsRef<Path>, scope: TimingScope<'_>) -> StorageResult<Self>` | `src/cas/store.rs:132` |
| `Store::default_policy` | `pub const fn default_policy() -> StoragePolicy` | `src/cas/store.rs:149` |
| `Store::begin_save` | `pub fn begin_save(&self, scope: TimingScope<'_>) -> StorageResult<SaveOperation>` | `src/cas/store.rs:185` |
| `Store::read_batch` | `pub fn read_batch(&self, ids: &[ObjectId], scope: TimingScope<'_>) -> StorageResult<(Vec<Vec<u8>>, StoreReadCounters)>` | `src/cas/store.rs:202-206` |
| `Store::contains` | `pub fn contains(&self, ids: &[ObjectId], scope: TimingScope<'_>) -> StorageResult<Vec<ObjectId>>` | `src/cas/store.rs:236-240` |
| `SaveOperation::accept` | `pub fn accept(&mut self, object: FinalizedObject) -> StorageResult<()>` | `src/cas/store.rs:291` |
| `SaveOperation::finish` | `pub fn finish(mut self, scope: TimingScope<'_>) -> StorageResult<SaveOutcome>` | `src/cas/store.rs:314` |
| `SaveOperation::read_batch` | `pub fn read_batch(&mut self, ids: &[ObjectId], scope: TimingScope<'_>) -> StorageResult<Vec<Vec<u8>>>` | `src/cas/store.rs:341-345` |
| `SaveOperation::abort` | `pub fn abort(mut self, scope: TimingScope<'_>) -> StorageResult<()>` | `src/cas/store.rs:469` |
| `SaveOperation::pending` | `pub fn pending(&self) -> (usize, u64)` | `src/cas/store.rs:432` |
| `SaveHandoff::new` | `pub fn new(operation: &'a mut SaveOperation) -> Self` | `src/cas/store.rs:516` |
| `impl FinalizedConsumer for SaveHandoff<'_>` | `fn accept(&mut self, object: FinalizedObject) -> Result<(), ContentError>` | `src/cas/store.rs:539-548` |
| `impl AuthenticatedObjects for StoreProvider<'_>` | `fn read_canonical_batch(&self, ids: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>>` | `src/cas/provider.rs:50` |
| `StoreProvider::read_canonical_batch_scoped` | `fn read_canonical_batch_scoped(&self, ids: &[ObjectId], scope: TimingScope<'_>) -> ContentResult<Vec<Vec<u8>>>` | `src/cas/provider.rs:60-64` |
| `StoreProvider::new` / `read_wave` | `pub const fn new(store: &'a Store) -> Self` / `pub fn read_wave(&self, ids: &[ObjectId], scope) -> StorageResult<(Vec<Vec<u8>>, StoreReadCounters)>` | `src/cas/provider.rs:25`, `35-39` |

`SaveOutcome` (`src/cas/store.rs:27-55`) is by value and carries `reused, inserted, packs_created, pack_appends, commits, full_records, prefix_records, delta, chain, pool`; `commits` is documented as the acknowledgement itself (`src/cas/store.rs:38-44`).

### 1.2 Borrowed vs owned bytes; who releases

- The producer is C1: `pub struct FinalizedObject { id, role, canonical: Vec<u8>, references: Vec<ObjectId>, predecessors }` — **owned** bytes, one constructor that computes the identity from the bytes (`core/crates/layerfs-content/src/object/output.rs:88-109`; `pub fn canonical(&self) -> &[u8]` at `:134`).
- Storage borrows them on every path: `let object = &objects[index];` (`src/cas/save.rs:53`), `object.canonical()`, `object.id()`, `object.role()`, `object.canonical_len()`; no per-object canonical copy is made on the prepared path (`src/cas/save.rs:50-54`).
- **Who releases:** the save operation owns the objects while they sit in `PendingBatch`; `SaveOperation::accept` takes the object **by value** (`src/cas/store.rs:291`), `PendingBatch::push` moves it into `objects: Vec<FinalizedObject>` (`src/cas/batch.rs:48-69`), `drain()` hands the whole wave back to `flush` (`src/cas/batch.rs:72-75`), and the allocation is released when the wave's vector is dropped after `save::flush_batch` returns. The seam never returns bytes to the caller.

### 1.3 Batch count/byte limits and backpressure

`src/cas/batch.rs:48-69`:

    pub fn push(&mut self, object: FinalizedObject) -> StorageResult<Option<Vec<FinalizedObject>>> {
        let length = object.canonical_len() as u64;
        if !self.objects.is_empty()
            && (self.objects.len() >= self.object_limit
                || self.canonical_bytes.saturating_add(length) > self.byte_limit)
        {
            let drained = self.drain();
            ...

Backpressure is synchronous and deterministic: `SaveOperation::accept` flushes the drained wave before returning (`src/cas/store.rs:295-296`), so the producer blocks on real storage work rather than growing a queue. The declared bounds are `BATCH_OBJECT_LIMIT = 512` (`src/policy.rs:65`) and `BATCH_CANONICAL_BYTES_LIMIT = 512 * 1024` (`src/policy.rs:67`), taken by value in `PendingBatch::new` (`src/cas/batch.rs:24-31`).

**Declared exception:** one object larger than the whole byte bound is accepted into an empty batch — documented at `src/cas/batch.rs:44-47` (`"An object larger than the whole byte bound is still accepted into an empty batch"`). The byte bound is therefore a *wave* bound, not a per-object bound; the per-object bound is `CANONICAL_LIMIT = 16 * 1024 * 1024` (`src/policy.rs:61`), enforced in `src/encoding/full.rs:111-117`.

### 1.4 Error and cancellation semantics

- `accept` after finish/terminal → `StorageError::Aborted` (`src/cas/store.rs:292-294`); it does not silently drop the object.
- Every preparation failure crosses one boundary: `SaveOperation::terminate` (`src/cas/store.rs:482-488`) → `finish::terminate` (`src/cas/finish.rs:12-25`): `UnknownOutcome` → `owner.quarantine()`; anything else → `mark_terminal()` + one `abandon()`; a cleanup failure is returned as `CleanupFailed { original, cleanup }` (`src/cas/finish.rs:20-23`).
- `SaveHandoff` converts a storage failure into `ContentError::OutputRejected` and **retains** the original error for one retrieval (`src/cas/store.rs:540-547`, `take_failure` at `:529`). There is no resend.
- Cancellation: `abort` sets `terminal`, runs one `abandon()`, sets `finished` and drops the owner (`src/cas/store.rs:469-480`). `Drop` performs at most one best-effort `abandon()`, and `MutationOwner::abandon` returns immediately once `cleanup_attempted || quarantined` (`src/cas/store.rs:491-502`, `src/cas/owner.rs:923-927`).

### 1.5 Finding F4 — the C1 bridge erases every failure class

`src/cas/provider.rs:50-57`:

    fn read_canonical_batch(&self, ids: &[ObjectId]) -> ContentResult<Vec<Vec<u8>>> {
        Timing::disabled("storage.read", |scope| {
            self.read_wave(ids, scope.child("storage.read"))
        })
        .0
        .map(|(values, _)| values)
        .map_err(|_| ContentError::MissingObject)
    }

and the scoped form at `src/cas/provider.rs:65-67` does the same. Every storage failure — `VisibilityCeiling`, `CapacityExceeded`, `Integrity`, `Engine`, and an authentication mismatch — is reported to C1 as `MissingObject`. The C1 trait contract at `core/crates/layerfs-content/src/object/access.rs:26-28` states: *"Returning bytes that do not belong to the requested identity is a contract violation; a provider that cannot establish identity must return [`ContentError::IdentityMismatch`] instead."* The Store does establish identity (`src/cas/read.rs:91-93`), but it reports the failure of that check as `MissingObject`, so an integrity fault is indistinguishable from an absent object at this seam. **Verdict: PARTIAL.** Not a retry or a fallback, but a loss of failure class at the only product bridge.

---

## 2. SQLite profile, retries, fsync, WAL

### 2.1 Profile applied at connection open — PASS

`src/sqlite/connection.rs:17-45`:

    pub fn open(path: &Path, create: bool) -> StorageResult<Connection> {
        let flags = if create {
            OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE
        } else {
            OpenFlags::SQLITE_OPEN_READ_WRITE
        };
        let connection = Connection::open_with_flags(path, flags)?;
        configure(&connection)?;
        Ok(connection)
    }

    pub fn configure(connection: &Connection) -> StorageResult<()> {
        let journal: String =
            connection.query_row("PRAGMA journal_mode = MEMORY", [], |row| row.get(0))?;
        if !journal.eq_ignore_ascii_case("memory") {
            return Err(StorageError::Integrity("journal mode"));
        }
        connection.execute_batch("PRAGMA synchronous = OFF; PRAGMA temp_store = MEMORY;")?;
        connection.execute_batch("PRAGMA foreign_keys = ON;")?;
        let foreign_keys: i64 = connection.query_row("PRAGMA foreign_keys", [], |row| row.get(0))?;
        if foreign_keys != 1 {
            return Err(StorageError::Integrity("foreign key enforcement"));
        }
        connection.busy_timeout(Duration::ZERO)?;
        Ok(())
    }

- **journal mode:** MEMORY, and the *result* is verified, not assumed (`:32-36`). Not WAL, not OFF.
- **synchronous:** OFF, with `temp_store = MEMORY` (`:37`).
- **busy timeout:** `connection.busy_timeout(Duration::ZERO)?` (`:43`). rusqlite 0.40.2 forwards this verbatim to `sqlite3_busy_timeout` (`~/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/rusqlite-0.40.2/src/busy.rs:76` — `let r = unsafe { ffi::sqlite3_busy_timeout(self.db, timeout) };`); SQLite's contract for that call is that an argument ≤ 0 turns off all busy handlers, so a lock is an immediate `SQLITE_BUSY`. `Cargo.toml:13` pins `rusqlite = "=0.40.2"`, so this is the exact call in the built tree.
- Every connection in the crate goes through `open`/`configure`: `Store::create` (`src/cas/store.rs:118`), `Store::open` (`:135`), `begin_save` (`:187`), `read_batch` (`:209`), `contains` (`:247`). There is no second open path.

### 2.2 busy/locked handling is a typed failure, not a wait — PASS

`src/sqlite/connection.rs:54-72`:

    pub fn is_busy(error: &rusqlite::Error) -> bool {
        matches!(
            error,
            rusqlite::Error::SqliteFailure(inner, _)
                if matches!(
                    inner.code,
                    rusqlite::ErrorCode::DatabaseBusy | rusqlite::ErrorCode::DatabaseLocked
                )
        )
    }

    pub fn ownership_error(error: rusqlite::Error) -> StorageError {
        if is_busy(&error) {
            StorageError::OwnershipUnavailable
        } else {
            StorageError::Engine(error)
        }
    }

and `src/sqlite/write.rs:51-55`: `connection.execute_batch("BEGIN IMMEDIATE").map_err(ownership_error)` — one attempt, one mapping, no loop.

### 2.3 No retry, no busy handler, no error-driven fallback — PASS

- `MutationOwner::maybe_commit` (`src/cas/owner.rs:840-857`) commits and then re-runs `BEGIN IMMEDIATE` for the *next* bounded transaction of the same save. That is a new transaction, not a retry of a failed one; the comment at `src/cas/owner.rs:846-848` records the ordering reason.
- `src/sqlite/cleanup.rs:45` and `:100` loop over *pages*, not over failed attempts; `src/sqlite/cleanup.rs:90` states `"A failure here is surfaced as it is; the attempt is never retried and no outer wrapper resumes it."`
- The only `or_else` sites are `QueryReturnedNoRows → Ok(None)` mappings (a missing row is not an error): `src/sqlite/schema.rs:168-171`, `src/sqlite/schema.rs:186-189`, `src/sqlite/pool.rs:105-108`. No site dispatches on an error to choose a different algorithm, decoder or backend.
- No `busy_handler` call exists anywhere in `src/` (only `busy_timeout(Duration::ZERO)` at `src/sqlite/connection.rs:43`).
- Minor (F7): two observability accessors swallow a poisoned mutex and report zero — `src/cas/store.rs:173` and `:177-182` (`self.pool_index.lock().map(|index| index.len()).unwrap_or(0)`). This is reporting, not a second attempt.

### 2.4 fsync / WAL — PASS

A whole-crate grep for `fsync|fdatasync|sync_all|sync_data|journal_mode|WAL|busy_handler|retry|PRAGMA` over `src/**/*.rs` returns only: the module docs `src/lib.rs:11-12`, `src/sqlite/connection.rs:6-7`; the four real pragmas in `configure`; and `PRAGMA table_info` in `src/sqlite/schema.rs:195`. There is **no** `File::sync_all`, `sync_data`, `fsync`, `fdatasync`, WAL pragma, checkpoint or durable manifest in the crate. `sql/schema.sql` contains no such statement either. `COMMIT` is required and the RAM rollback journal is retained (`src/sqlite/write.rs:57-64`).

---

## 3. Schema and schema identity

### 3.1 Enumeration from `sql/schema.sql` (72 lines, no triggers, no views)

File-level identity, `sql/schema.sql:10-11`:

    PRAGMA application_id = 1279677261;
    PRAGMA user_version = 4;

**`store_policy`** — `sql/schema.sql:13-32`, `STRICT`, one row:

| Column | Declared width + constraint |
|---|---|
| `id` | INTEGER PRIMARY KEY CHECK (id = 1) |
| `format_profile` | INTEGER NOT NULL CHECK (format_profile = 1) |
| `small_file_threshold_bytes` | INTEGER NOT NULL CHECK (BETWEEN 131072 AND 1048576) |
| `whole_file_delta_max_depth` | INTEGER NOT NULL CHECK (BETWEEN 0 AND 50) |
| `chunk_delta_max_depth` | INTEGER NOT NULL CHECK (BETWEEN 0 AND 50) |
| `metadata_delta_max_depth` | INTEGER NOT NULL CHECK (BETWEEN 0 AND 50) |
| `retained_pack_ceiling` | INTEGER NOT NULL CHECK (>= 0) |

**`object_packs`** — `sql/schema.sql:34-37`, `STRICT`: `pack_id INTEGER PRIMARY KEY CHECK (pack_id > 0)`, `data BLOB NOT NULL CHECK (length(data) >= 32)`.

**`metadata_value_groups`** — `sql/schema.sql:39-47`, `STRICT`: `first_ordinal INTEGER PRIMARY KEY CHECK (BETWEEN 1 AND 4294967295)`; `count INTEGER NOT NULL CHECK (BETWEEN 1 AND 165)`; `pack_id INTEGER NOT NULL REFERENCES object_packs(pack_id) ON DELETE CASCADE`; `group_number INTEGER NOT NULL CHECK (BETWEEN 0 AND 255)`; `digest BLOB NOT NULL CHECK (length(digest) = 32)`; table CHECK `(first_ordinal + count <= 4294967296)`; `UNIQUE (pack_id, group_number)`.

**`objects`** — `sql/schema.sql:49-66`, `STRICT, WITHOUT ROWID`: `object_id BLOB NOT NULL PRIMARY KEY CHECK (length(object_id) = 32)`; `object_role INTEGER NOT NULL CHECK (BETWEEN 1 AND 13)`; `canonical_length INTEGER NOT NULL CHECK (> 0 AND <= 16777216)`; `base_object_id BLOB` CHECK `(IS NULL OR (length = 32 AND base_object_id <> object_id))` `REFERENCES objects(object_id) ON DELETE NO ACTION`; `pack_id INTEGER NOT NULL REFERENCES object_packs(pack_id)`; `group_number INTEGER NOT NULL CHECK (>= 0 AND < 256)`; `record_number INTEGER NOT NULL CHECK (>= 0 AND < 8191)`.

**Indexes** — exactly two explicit ones: `CREATE UNIQUE INDEX objects_locations ON objects(pack_id, group_number, record_number)` (`sql/schema.sql:68-69`) and `CREATE INDEX objects_bases ON objects(base_object_id) WHERE base_object_id IS NOT NULL` (`sql/schema.sql:71-72`). Implicit engine indexes: the `UNIQUE (pack_id, group_number)` auto-index of `metadata_value_groups`, and the WITHOUT-ROWID primary key index of `objects`.

**Triggers:** none. A grep for `CREATE TRIGGER|CREATE VIEW` over `sql/schema.sql` matches nothing; `schema.rs:107-116` additionally rejects *any* other non-`sqlite_%` table at open, and `REQUIRED_TABLES` names exactly four tables (`src/sqlite/schema.rs:22-58`).

### 3.2 Schema-version constants

- `src/policy.rs:18`: `pub const APPLICATION_ID: i64 = 1_279_677_261;`
- `src/policy.rs:27`: `pub const SCHEMA_VERSION: i64 = 4;`
- `src/policy.rs:39-42`: `pub const SCHEMA_IDENTITY: SchemaIdentity = SchemaIdentity { application_id: APPLICATION_ID, user_version: SCHEMA_VERSION };`
- `src/sqlite/schema.rs` defines **no** version constant of its own; it imports the identity (`src/sqlite/schema.rs:11`) and embeds the SQL file with `pub const SCHEMA_SQL: &str = include_str!("../../sql/schema.sql");` (`src/sqlite/schema.rs:15`). The task's phrase "the schema-version constant in `src/sqlite/schema.rs`" is therefore inexact: the constant lives in `src/policy.rs:27`.

**Drift guard:** the SQL file hard-codes `user_version = 4` while the Rust constant is also 4. `create` executes the file and then calls `validate` (`src/sqlite/schema.rs:69`, `:85`), which calls `identity(connection, SCHEMA_IDENTITY)`; a divergent pair would be refused at create time rather than shipped.

### 3.3 What happens at open with an older/different schema version — **refuse, never migrate, never silently accept**

`Store::open` (`src/cas/store.rs:132-146`) → `schema::validate(&connection, None)` (`src/sqlite/schema.rs:89-135`) → `identity` (`src/sqlite/schema.rs:138-147`):

    pub fn identity(connection: &Connection, expected: SchemaIdentity) -> StorageResult<()> {
        let application_id = pragma_i64(connection, "application_id")?;
        let user_version = pragma_i64(connection, "user_version")?;
        if application_id != expected.application_id || user_version != expected.user_version {
            return Err(StorageError::UnsupportedPolicy {
                field: "schema identity",
            });
        }
        Ok(())
    }

There is no `ALTER TABLE`, no version-branch migration, no upgrade path and no "accept and repair" anywhere in the crate (grep for `ALTER`, `migrat` in `src/` returns only the doc comments saying old Stores are rejected: `sql/schema.sql:9` *"Older Stores are rejected rather than migrated."*, `src/policy.rs:26`, `src/cas/store.rs:108`). A file that carries application id 1279677260 / user_version 10 (the reference product) is refused with `UnsupportedPolicy { field: "schema identity" }`; this is the exact behaviour pinned by `tests/cas_roundtrip.rs:177-186`.

Additional open-time refusals *before any object work* (`src/sqlite/schema.rs:89-135`): a missing required table/column shape (`:94-96`, `:198-205`), a table that is not `STRICT` (`:191-193`), a missing required index (`:97-106`), an unexpected fifth table (`:107-116`), a watermark ahead of storage (`:119-125`, `"publication watermark is ahead of storage"`), and a conflicting `expected` policy (`:127-133`). The role-ceiling text is required verbatim as well: `const REQUIRED_CONSTRAINTS: &[(&str, &str)] = &[("objects", "CHECK (object_role BETWEEN 1 AND 13)")];` (`src/sqlite/schema.rs:156-157`) with the reason at `:149-155`.

**Note (not a defect, a dependency):** STRICT tables require SQLite ≥ 3.37, and `Cargo.toml:13` does **not** enable rusqlite's `bundled` feature, so the built product links the host `libsqlite3` (the tree itself records this in `examples/memory_ledger.rs:422`). An old host library fails at `create`/`open` with an explicit engine error; there is no version probe and no silent downgrade. **UNVERIFIED:** I did not execute a build or read the linked library's version from the product binary.

---

## 4. Exact reuse (CAS)

### 4.1 Detection and reuse

`src/cas/save.rs:22-75` — one batched membership query per wave (`let locations = lookup::locations(owner.connection(), &ids, i64::MAX)?;` at `:27`), an in-wave duplicate rule at `:41-49`:

    if let Some(prior) = prepared.get(&objects[index].id()).copied() {
        if objects[prior].canonical() != objects[index].canonical() {
            return Err(StorageError::Collision(objects[index].id()));
        }
        owner.note_reuse();

and three resolutions: existing row → `membership::reuse_or_collide` (`:55-59`); identity held in an unfinished group → `owner.seal_pending` then the ordinary verified path (`:60-68`); otherwise → `owner.offer` (`:69-73`). There is no one-query-per-object loop.

### 4.2 The digest **is** verified on reuse; bytes are compared too — PASS

`src/cas/membership.rs:14-46`:

    pub fn stored_canonical(owner: &mut MutationOwner, location: ObjectLocation) -> StorageResult<Vec<u8>> {
        let canonical = owner.resolve_location(location)?;
        if ObjectId::for_bytes(&canonical) != location.object_id {
            return Err(StorageError::Integrity("stored object identity"));
        }
        Ok(canonical)
    }

    pub fn reuse_or_collide(owner: &mut MutationOwner, object: &FinalizedObject, location: ObjectLocation) -> StorageResult<()> {
        if location.object_id != object.id() { return Err(StorageError::Integrity("membership identity")); }
        if location.canonical_length != object.canonical_len() { return Err(StorageError::Collision(object.id())); }
        let stored = stored_canonical(owner, location)?;
        if stored == object.canonical() { Ok(()) } else { Err(StorageError::Collision(object.id())) }
    }

So on reuse the stored record is fully reconstructed, its identity is **recomputed** from the reconstructed bytes and compared with the row key, its length is compared with the offered length, and the bytes are compared with the offered bytes. Nothing is trusted. The offered side cannot be forged either: `FinalizedObject` has exactly one constructor, which computes `id = ObjectId::for_bytes(&canonical)` (`core/crates/layerfs-content/src/object/output.rs:98-109`), and its fields are private (`:88-94`).

The same authentication is applied to every reconstructed object on the read path: `src/cas/read.rs:91-93`, `src/encoding/delta/read.rs:176-178`, `src/encoding/pool/read.rs:129-131`.

### 4.3 Exactly one delta trial — **per target**, not per save (F8)

`src/encoding/delta/select.rs:275-301`:

    // Exactly one trial: the acquired base is read once, authenticated, and used
    // for one prefix frame. Nothing here retries with another candidate.
    let base = acquire(input, base_id)?;
    ...
    input.counters.trials = input.counters.trials.saturating_add(1);
    let prefix = encode_prefix(canonical, role, base_id, base_raw, input.capacities, encode)?;
    if prefix.record.len() < full.record.len() { ... Ok(prefix) } else { ... Ok(full) }

`select` is called once per missing object (`src/cas/owner.rs:355`, `:426-433`), so a save that admits N objects may run up to N trials. The roadmap says exactly this — *"Preserve at most one PREFIX trial per target; do not add a second candidate after a completed losing trial"* (`physical-encoding-and-packing.md:175-176`). The pooled lane follows the same rule: `src/cas/owner.rs:541-553` performs one base acquisition, one `build`, one comparison, and otherwise stores FULL. A trial that fails the cost comparison selects the already-prepared FULL record — a policy outcome, never error recovery (`src/encoding/delta/select.rs:6-8`).

### 4.4 Candidate ordering

- **Advisory list order is authoritative for acquisition:** `fn acquisition` iterates `for id in advisory { if probe(input, *id, role, depth_cap)? { return Ok(Some(*id)); } }` (`src/encoding/delta/select.rs:342-354`).
- **CHUNK** considers exactly the first supplied candidate: `ObjectRole::Chunk => match advisory.first().copied()` (`src/encoding/delta/select.rs:247-253`).
- **WHOLE_FILE** consults the caller's list first and only then the bounded FULL-winner cache: `_ => match acquisition(...) { Some(id) => ..., None => { let found = input.candidates.find(id, &signature(raw)); ... } }` (`src/encoding/delta/select.rs:254-266`).
- **Cache ordering and tie-break** are in `src/encoding/delta/candidates.rs:139-165`: highest signature overlap wins, ties break on the smaller identity (`overlap > count || (overlap == count && entry.id < id)`), so the choice does not depend on insertion order. Entry admission requires ≥ 2 shared signature hashes and excludes the target itself (`:149-156`).
- **Eligibility** (`src/encoding/delta/select.rs:356-374`) requires present, same role, and `depth < depth_cap`; every rejection is counted (`absent_candidates` / `ineligible_candidates`) and never becomes a failure.

---

## 5. FULL and pack locator reads

### 5.1 FULL encoding bounds — `src/encoding/full.rs`

- Canonical bound: `if canonical.is_empty() || canonical.len() > CANONICAL_LIMIT` → `CapacityExceeded { what: "encoding.canonical_length" }` (`:111-117`), `CANONICAL_LIMIT = 16 * 1024 * 1024` (`src/policy.rs:61`).
- Raw-payload framing is *found and checked* before a framed record exists: `raw_payload` rejects a whole-file role whose canonical payload is absent and a chunk role whose value does not start with `CHUNK_MAGIC` (`:46-74`).
- Raw bound: `if raw.is_empty() || raw.len() > profile.raw_limit()` → `CapacityExceeded` (`:145-154`).
- Lane planning: compact whole-file is used only when the *complete* planned record fits a normal pack (`contribution = HEADER_LEN + WHOLE_FILE_ENTRY_LEN + body;` `if contribution <= capacities.pack_limit`), otherwise singleton, whose contribution is re-checked against `singleton_pack_limit` (`:190-213`). The losing candidate is dropped before the other is built (`:199-203`).
- `lane_body_limit(lane, capacities)` maps each lane to its own bound (`:217-225`), and `owner.offer` refuses a record body above it (`src/cas/owner.rs:358-366`).

### 5.2 Pack control area — `src/pack/layout.rs`

`parse_header` (`:246-297`) validates, in order: header present; magic `LFPACK\0\0`; framing version ∈ {1,2,4,6,7} (any other → `UnsupportedPolicy { field: "pack framing version" }`, so v3/v5 are refused and never trial-decoded, `:258-269`); group count in `1..=GROUP_COUNT_LIMIT` (`:270-277`); `bytes.len() >= HEADER_LEN + directory * group_count` (`:284-286`); `bytes.len() <= lane.pack_limit()` (`:287-289`); singleton group count == 1 (`:290-292`).

`ordinary_group_view` (`:325-389`) validates **the whole directory before selecting any body**: per-entry start must equal the running offset (`"group directory continuity"`), `end = body_start + encoded` must not exceed the pack, zero encoded/decoded refused (`"group extent"`), the codec field must be consistent (`0 if encoded == decoded`; `1 if encoded <= decoded && decoded <= lane.body_limit()`), Native/Singleton must be Raw, `decoded > lane.body_limit()` refused, and the final offset must equal `bytes.len()` (`"pack trailing bytes"`). `group >= header.group_count` is refused up front (`:314-316`).

`whole_file_group_view` (`:391-448`) re-checks the pack length, requires every declared start to equal the running start, requires each range to be non-empty (`size < 2`) and inside the pack, and takes the last group's end from `bytes.len()`.

`record_range` (`:451-488`) validates `count ∈ 1..=RECORD_COUNT_LIMIT`, `ordinal < count`, `group_length <= SINGLETON_PACK_LIMIT`, `ends.len() == 4 * count`, strictly increasing ends, every end within the record area, and `previous == area_length` at the end.

### 5.3 Reconstruction bounds and corrupt-locator handling — `src/encoding/decode.rs`

- `canonical_length == 0 || > CANONICAL_LIMIT` → refused (`:35-38`); header and group view validated before any body is touched (`:39-43`).
- Ordinary lane: the group body is decompressed or taken raw, its length must equal the declared decoded length, the record tag must be `FULL` (a PREFIX tag there is corruption, `"record tag is not FULL"`), and the rebuilt canonical length must equal the locator's (`:45-64`).
- WholeFile/Native/Singleton: the record is parsed by its lane's grammar, and the declared base presence must agree with the bytes actually supplied — `(true, None) => Integrity("prefix record without base bytes")`, `(false, Some(_)) => Integrity("FULL record with base bytes")` (`:137-156`); singleton additionally requires `record_number == 0` and a role of WholeFile or Chunk (`:106-129`).
- Pooled metadata locators are refused by this decoder and must go through the pool reader (`:131-133`).
- `framed_record`/`group_records` re-validate the record directory on every extraction (`:159-192`).
- `lookup::decode_location` refuses a negative length/ordinal, an unknown role code and a base identity that is not 32 bytes (`src/sqlite/lookup.rs:105-123`); `ObjectRole::from_code` rejects codes above 13 (`core/crates/layerfs-content/src/object/output.rs:81`). A locator whose pack row is absent is `Integrity("pack row is missing")` (`src/sqlite/lookup.rs:159-162`); a locator above the read ceiling is `VisibilityCeiling` (`src/cas/read.rs:61-66`).

### 5.4 Decompression expansion limits — `src/encoding/codec.rs`

- Every output allocation is exact-size and pre-checked: `fn output(size: usize, limit: usize) -> StorageResult<Vec<u8>> { if size == 0 || size > limit { return Err(resource()); } ... }` (`:136-144`).
- Payload frames: `frame.len() > profile.frame_limit()`, `raw_length == 0 || raw_length > profile.raw_limit()` refused before the call (`:434-440`, `:492-500`); the *frame header* is then parsed and required to be a single Zstandard frame with `frameContentSize == raw_length`, `windowSize <= 1 << profile.window_log()`, `dictID == 0`, `checksumFlag == 1` (`:445-454`, `:504-511`); the decoded byte count must equal `raw_length` (`:465-474`, `:529-538`). `parse_frame_header` also refuses reserved descriptor bits and a frame whose declared compressed size is not the whole slice (`:600-627`).
- Group bodies: `raw_length > GROUP_LIMIT` and `frame.len() > GROUP_FRAME_LIMIT` refused (`:551-558`), header `windowSize > GROUP_LIMIT` refused (`:565`), output exact-size (`:575`). There is no "grow to whatever the frame claims" path anywhere.
- `profile.raw_limit()` for whole file is `capacities.whole_file_canonical_limit - WHOLE_FILE_CANONICAL_OVERHEAD` (`:69-75`), i.e. derived from the persisted cutoff, with the chunk profile fixed at 32 KiB raw / 33 024-byte frame (`:60-66`, `:94-96`).

### 5.5 Chain reconstruction bounds — `src/encoding/delta/read.rs:109-224`

Iterative, no recursion. Per-chain budgets restart at `resolve_charged` (`:114-115`). Depth is refused while walking (`if chain.len() > usize::from(role_depth)`, `:147-149`); role agreement and strict chronology are required (`:152-157`); each charged object's canonical length and cumulative canonical/encoded work are checked against the chain budgets before materialisation (`:192-218`); every reconstructed object is authenticated (`:176-178`). The pool reader enforces the same shape for pooled leaves: depth (`src/encoding/pool/read.rs:223-225`), role and chronology (`:228-238`), per-chain canonical/encoded work (`:244-252`), leaf record width (`:289-291`, `:309-311`), and `output_length == physical_length(canonical_length)` for deltas (`:273-275`).

---

## 6. Transactions, visibility and failure isolation

### 6.1 Private early output + publication watermark

- The watermark is the persisted `store_policy.retained_pack_ceiling` (`sql/schema.sql:27-31`), read by `schema::retained_pack_ceiling` (`src/sqlite/schema.rs:254-267`) and advanced only from `MutationOwner::finish_inner` before the commit: `crate::sqlite::schema::advance_retained_pack_ceiling(&self.connection, self.ceiling)?; write::commit(&self.connection)?;` (`src/cas/owner.rs:915-917`).
- `advance_retained_pack_ceiling` is monotone and cardinality-checked: `UPDATE store_policy SET retained_pack_ceiling = ?2 WHERE id = ?1 AND retained_pack_ceiling < ?2`, refusing `affected > 1` (`src/sqlite/schema.rs:273-283`).
- Ordinary reads capture the ceiling once and apply it to every acquired location: `let ceiling = schema::retained_pack_ceiling(&connection)?;` (`src/cas/store.rs:213`, `:248`) → `lookup::locations(..., pack_id <= ?)` (`src/sqlite/lookup.rs:62`, `:134`), plus an explicit refusal `if location.pack_id > ceiling { return Err(StorageError::VisibilityCeiling { .. }) }` (`src/cas/read.rs:61-66`) — a not-yet-published record is reported as a visibility refusal, never as missing. The same ceiling gates pooled groups (`src/encoding/pool/read.rs:125-130`) and in-save index reads (`src/cas/owner.rs:489-496`, `:597-603`).
- In-save reads have no ceiling by design and read the owner's own open transaction (`src/cas/owner.rs:307-317`; `src/cas/store.rs:341-403`), including sealing a still-open group on demand rather than retaining a second copy of its payload (`src/cas/owner.rs:273-305`).
- Acquisition refuses to treat another attempt's live output as its own baseline: `let published = crate::sqlite::schema::retained_pack_ceiling(&connection)?; if published != baseline_pack_id { return Err(StorageError::UninspectedState { ceiling: published, highest_pack_id: baseline_pack_id }); }` (`src/cas/owner.rs:177-186`). Mid-save, a bounded commit may release the lock; the packs it committed stay invisible because the watermark has not moved.

### 6.2 Exactly one acknowledged finish — PASS

`finish` consumes the operation: `pub fn finish(mut self, scope: TimingScope<'_>) -> StorageResult<SaveOutcome>` (`src/cas/store.rs:314`). Its body runs `owner.finish()` and only an `Ok` sets `self.finished = true; self.owner = None;` (`:322-327`); every other outcome goes through `self.terminate(error)` (`:328`). `MutationOwner::finish` refuses a terminal owner with `Aborted` (`src/cas/owner.rs:863-866`). There is no boolean "acknowledged" field that could disagree with the returned value — the type comment at `src/cas/store.rs:38-44` records why. A second `finish` is impossible (the value is moved), and a second `accept` after success returns `Aborted` (`src/cas/store.rs:292-294`).

Acknowledgement ordering inside `finish_inner` (`src/cas/owner.rs:883-920`): seal every lane; if nothing was written and nothing published, release the acquisition with `ROLLBACK` and return without a `COMMIT` (`:896-900`); if an earlier bounded commit already published the packs, roll back the empty acquisition and open a dedicated final transaction for the watermark (`:901-911`); then advance the watermark and `COMMIT` once (`:915-917`).

### 6.3 A definite failure leaves previous data intact and removes only this operation's writes — PASS

`src/sqlite/cleanup.rs:35-149` deletes, in two paged passes under `BEGIN IMMEDIATE`, rows `WHERE pack_id > ?1` with `baseline_pack_id` as the ownership proof — object rows newest locator first (`:46-49`: `ORDER BY pack_id DESC, group_number DESC, record_number DESC LIMIT ?2`), then pack rows (`:101-104`). Everything at or below the baseline is untouched, and the watermark never moves on failure. Metadata catalogue rows are removed by the schema's own `ON DELETE CASCADE` (`sql/schema.sql:42`) with `PRAGMA foreign_keys = ON` verified at open (`src/sqlite/connection.rs:38-42`).

Ordering correctness follows from the schema: `objects.pack_id REFERENCES object_packs(pack_id)` is NO ACTION, so packs are removed only after their object rows; `base_object_id REFERENCES objects(object_id) ON DELETE NO ACTION` plus the descending locator order means a dependent is always deleted before its base. The writer also guarantees that no stored object can depend on a later locator: `if locator_key(&location) >= locator_key(&current) { return Err(StorageError::Integrity("dependency chronology")); }` (`src/encoding/delta/read.rs:155-157`).

One attempt only: `cleanup_attempted` is set before any fallible step (`src/cas/owner.rs:923-935`), `mark_terminal`/quarantine are idempotent (`:937-956`), and `Drop` re-calls `abandon` which returns `Ok(())` immediately afterwards (`src/cas/store.rs:491-502`). A failure whose outcome is unproven is never cleaned: `COMMIT`/`ROLLBACK` errors are wrapped as `UnknownOutcome` (`src/sqlite/write.rs:58-73`) and `finish::terminate` quarantines instead of deleting (`src/cas/finish.rs:13-17`).

**Observation (F1, see §8):** cleanup's own doc comment claims each transaction "is bounded by the writer's own row and canonical-byte limits" (`src/sqlite/cleanup.rs:8-12`); that is a *trigger* discipline, not a hard cap — see F1.

---

## 7. Chain limits, pooled window/index bounds, retained cache bounds

### 7.1 Payload chain caps

| Cap | Value | Declared | Enforced |
|---|---|---|---|
| Whole-file depth | policy (default 8, max 50) | `src/policy.rs:224-227`, `core/crates/layerfs-content/src/policy.rs:20,24` | `src/encoding/delta/select.rs:232-233`, `src/encoding/delta/read.rs:139-149` |
| Chunk depth | policy (default 4, max 50) | `src/policy.rs:229-231` | same |
| Pooled-metadata depth | policy (default 8, max 50) | `src/policy.rs:125,233-236` | `src/cas/owner.rs:715-731`, `src/encoding/pool/read.rs:216-225` |
| Chain canonical bytes | 512 KiB | `CHAIN_CANONICAL_LIMIT` `src/policy.rs:102` | `src/encoding/delta/read.rs:193-204`, `select.rs:282-298` |
| Chain encoded bytes | 256 KiB | `CHAIN_ENCODED_LIMIT` `src/policy.rs:104` | `src/encoding/delta/read.rs:212-218`, `select.rs:286-298` |
| Metadata chain canonical | 8 × 8192 = 65 536 | `src/policy.rs:127` | `src/encoding/pool/read.rs:246-252`, `owner.rs:738-742` |
| Metadata chain encoded | 17 × 8193 = 139 281 | `src/policy.rs:129` | `src/encoding/pool/read.rs:246-252`, `owner.rs:751-755` |
| Metadata decoded work per chain | 32 MiB | `METADATA_DECODED_WORK_LIMIT` `src/policy.rs:123` | `src/encoding/pool/read.rs:135-140` |

Role→depth mapping is per role, not one shared number: `delta_depth_for_role` maps Chunk→`chunk_delta_max_depth`, InodeLeaf→`metadata_delta_max_depth`, everything else→`whole_file_delta_max_depth` (`src/policy.rs:337-343`). A refused candidate is counted (`work_exceeded`) and selects FULL, so no stored object may depend on bytes a later read could not reconstruct (`src/encoding/delta/select.rs:278-297`).

### 7.2 Pooled value window / index bounds

| Bound | Value | Evidence |
|---|---|---|
| Retained index entries | 131 072, whole-window reset on overflow | `METADATA_INDEX_VALUES` `src/policy.rs:136-142`; reset at `src/encoding/pool/index.rs:80-83`, `:131-134` |
| Values per group | 165 | `VALUES_PER_GROUP` `src/policy.rs:93`; `sql/schema.sql:41`; `src/encoding/pool/value_group.rs:35-37` |
| Pooled leaf rows | 100 | `POOLED_LEAF_ROWS_LIMIT` `src/policy.rs:97`; `MAXIMUM_LEAF_ROWS` (`layerfs-content/src/object/inode_leaf.rs:34`); enforced at `src/encoding/pool/read.rs:327-329` |
| Per-save value memo | 100 entries, cleared per leaf, fails closed if exceeded | `PENDING_VALUES_LIMIT` `src/cas/owner.rs:36`; check `owner.rs:513-515`; clear `owner.rs:535` |
| Decoded value cache per reader | 512 KiB, released wholesale | `POOLED_VALUE_CACHE_BYTES` `src/policy.rs:121`; `src/encoding/pool/read.rs:147-153` |
| Ordinal space | `u32`, first 1, exclusive end 2^32 | `src/sqlite/pool.rs:44-67`; `sql/schema.sql:40,45` |
| Catalogue access | streamed, one row at a time | `src/sqlite/pool.rs:154-168`; `src/encoding/pool/index.rs:224-248` |

### 7.3 Retained caches owned by one save / one read

| Cache | Bound | Evidence |
|---|---|---|
| Admitted-FULL winner cache | 1024 slots + 8192 refs ≤ 128 KiB, fixed size, compile-time asserted | `src/encoding/delta/candidates.rs:14-44`, `109-111` |
| Chain-depth cache | 4096 entries, dropped whole on overflow | `src/encoding/delta/select.rs:26-27`, `:147-153` |
| Dependency pack cache | 4 MiB, released wholesale | `DEPENDENCY_PACK_CACHE_BYTES` `src/policy.rs:113`; `src/encoding/delta/read.rs:246-266` |
| Pooled reader pack cache | same 4 MiB rule | `src/encoding/pool/read.rs:186-199` |
| In-flight group per lane | 5 lanes, one `PendingGroup` each, sealed by framed length ≥ `GROUP_TARGET` 48 KiB | `src/cas/owner.rs:205-211`, `:368-381`; `GROUP_TARGET` `src/policy.rs:53` |
| Open pack tail per lane | exactly one, retained as framed groups | `src/pack/placement.rs:42-46`, `:55-60`; measured by `retained_tail_bytes` (`src/cas/owner.rs:265-271`) |

---

## 8. Limits with source, unit and scope

| Limit | Value | Unit | Scope | Source |
|---|---|---|---|---|
| `CANONICAL_LIMIT` | 16 777 216 | canonical bytes | one object, whole product | `src/policy.rs:61`; enforced `src/encoding/full.rs:111-117`; schema `sql/schema.sql:55-56` |
| Whole-file canonical limit | cutoff − 1 + 23 (default 131 071 + 23 = 131 094) | canonical bytes | whole-file role under the persisted cutoff | `core/crates/layerfs-content/src/policy.rs:146,155` |
| `whole_file_frame_limit` | max(compressBound(raw), 135 168) | frame bytes | whole-file codec | `core/crates/layerfs-content/src/policy.rs:31,150-154` |
| Chunk raw / frame | 32 768 / 33 024 | bytes | chunk codec | `src/encoding/codec.rs:94-96` |
| `PACK_LIMIT` | 262 144 | packed bytes | ordinary, native, whole-file, pooled packs | `src/policy.rs:55`; `src/pack/layout.rs:107-112` |
| `SINGLETON_PACK_LIMIT` | 16 777 216 + 4 096 = 16 781 312 | packed bytes | one-oversized-record packs | `src/policy.rs:88-90`; `src/pack/layout.rs:109`; planned `src/encoding/full.rs:204-212` |
| `GROUP_LIMIT` / group body | 65 536 | framed bytes | ordinary/native/whole-file groups | `src/policy.rs:45`; `src/encoding/codec.rs:37`; `src/pack/assemble.rs:29-55` |
| `METADATA_GROUP_LIMIT` | 16 384 | framed bytes | pooled metadata group | `src/policy.rs:95`; `src/pack/assemble.rs:144-155` |
| `GROUP_COUNT_LIMIT` | 256 (singleton 1) | groups | one pack | `src/policy.rs:57`; `src/pack/layout.rs:129-136` |
| `RECORD_COUNT_LIMIT` | 8 191 | records | one group | `src/policy.rs:59`; `src/pack/assemble.rs:30-32`; `src/pack/layout.rs:457` |
| `METADATA_RECORD_LIMIT` / `INODE_LEAF_LIMIT` | 8 192 | record / canonical bytes | one pooled leaf record | `src/policy.rs:131,135`; `src/encoding/pool/leaf.rs:50-56`; `src/encoding/pool/read.rs:289-291` |
| `PROGRAM_LIMIT` | 65 536 | instruction bytes | one pooled COPY/INSERT program | `src/encoding/pool/delta.rs:18`; `validate` `:21-28` |
| `METADATA_MATCH_BUDGET_BYTES` | 131 072 | comparison bytes | one pooled trial | `src/policy.rs:133`; `src/cas/owner.rs:546-547` |
| Object id / digest width | 32 | bytes | row key, base, catalogue digest | `sql/schema.sql:50,44,59` |
| Locator widths | group_number 0..255, record_number 0..8191 | ordinals | one object row | `sql/schema.sql:64-65` |
| `canonical_length` column | 1..16 777 216 | bytes | one object row | `sql/schema.sql:55-56` |
| `pack_id` | > 0, monotone `baseline+1`, checked add | signed i64 | one Store | `sql/schema.sql:35`; `src/cas/owner.rs:187-189`; `src/pack/placement.rs:89-95` |
| Pool ordinal | 1..=4 294 967 295, exclusive end 2^32 | u32 | one Store catalogue | `sql/schema.sql:40,45`; `src/sqlite/pool.rs:44-67` |
| Batch | 512 objects / 512 KiB canonical | per wave | one save operation | `src/policy.rs:65,67`; `src/cas/batch.rs:24-31` |
| Transaction | 8 191 rows / 4 194 303 canonical bytes | trigger, not hard cap (F1) | one open transaction | `src/policy.rs:69,71`; `src/cas/owner.rs:840-844` |
| Lookup / cleanup page | 128 ids or rows | per statement | membership, presence, cleanup | `src/policy.rs:63,73`; `src/sqlite/lookup.rs:36-38` |
| Read wave | 4 096 demanded objects | ids | one `read_batch` / `contains` / in-save read | `src/policy.rs:80`; `src/cas/store.rs:259-268`, `:346` |
| Cache/read bounds | 128 KiB winner cache; 4 096 depth entries; 4 MiB pack cache; 512 KiB decoded-value cache; 131 072 index entries | bytes/entries | per operation or per reader | see §7.3 |
| Concurrent saves | no explicit limit; exclusivity is `BEGIN IMMEDIATE` with `busy_timeout = 0` → the loser gets `OwnershipUnavailable` | — | one Store file | `src/cas/owner.rs:176`; `src/sqlite/connection.rs:43,66-72` |

### Finding F1 — the transaction byte/row bound is a trigger, not a cap

`src/cas/owner.rs:840-844`:

    pub fn maybe_commit(&mut self) -> StorageResult<()> {
        if self.transaction_open
            && (self.transaction.rows >= self.capacities.transaction_rows
                || self.transaction.bytes >= self.capacities.transaction_bytes)
        {

The counters are incremented **before** the check and by whole units: `self.transaction.bytes += member.canonical_length as u64;` (`src/cas/owner.rs:819`) and `self.transaction.bytes += write.bytes.len() as u64;` (`:835`). Therefore one maximal object (16 MiB canonical) or one singleton pack (up to 16 781 312 bytes) inside a single transaction exceeds the declared `TRANSACTION_CANONICAL_BYTES_LIMIT = 4 * 1024 * 1024 - 1` (`src/policy.rs:71`) by that object's own size, and the in-memory rollback journal holds it. The same is true of the cleanup claim in `src/sqlite/cleanup.rs:8-12`. This is a bounded and intended overrun (the alternative is refusing a legal object), but the constant is not the hard transaction bound it reads as, and the roadmap table's line *"SQL transaction | Up to 8,191 submitted rows and 4 MiB minus 1 canonical bytes"* (`admission-and-persistence.md:344`) overstates it by one unit of work.

### Finding F5 — the reference "pack INSERT ≤ 1 MiB" bound has no counterpart here

The roadmap lists *"Pack INSERT statement | Up to 1 MiB of BLOB data, with separately supported oversized RAW singleton handling"* among the bounds to preserve (`admission-and-persistence.md:345`). The code inserts the whole pack BLOB in one statement with no size guard of its own — `INSERT INTO object_packs (pack_id, data) VALUES (?1, ?2)` (`src/sqlite/write.rs:76-85`) — so a singleton pack of up to `SINGLETON_PACK_LIMIT` (`src/policy.rs:88-90`) is bound in one statement. The deviation is not recorded in that document's deviation list (which covers other items).

---

## 9. Whole-file / whole-table / whole-window reads and their bounds

Every place the component materialises a whole artefact, with the bound or its absence. "**No declared bound**" means the code path has no constant that caps that allocation.

| # | Read | Bound | Evidence |
|---|---|---|---|
| 1 | One pack BLOB by primary key | **No pre-read bound.** `lookup::pack_bytes` materialises `SELECT data FROM object_packs WHERE pack_id = ?1` into a `Vec<u8>` with no length check; the width is validated only afterwards by `parse_header` (`bytes.len() > lane.pack_limit()`). For product-written rows ≤ 16 781 312 bytes; a corrupt or foreign row is read whole first. | `src/sqlite/lookup.rs:152-163`; validation `src/pack/layout.rs:287-289` |
| 2 | One read wave's result | **Count-bounded, not byte-bounded.** `check_read_demand` refuses `ids.len() > read_objects` (4096) before opening a connection; the returned `Vec<Vec<u8>>` has no aggregate byte cap, so a legal wave of 4096 maximal whole-file objects materialises the sum of their canonical bytes (up to ≈ 4096 × 131 094 B at the default cutoff, and up to `whole_file_canonical_limit` per object under a raised cutoff). | `src/cas/store.rs:259-268`, `:202-233`; `src/cas/read.rs:72-96` |
| 3 | One object in a same-save read | Same bound as #2, plus a copy of every value still pending in the batch (`bytes.to_vec()`) | `src/cas/store.rs:346-358` |
| 4 | One dependency/base reconstruction | Bounded: per-chain canonical ≤ 512 KiB, encoded ≤ 256 KiB, depth ≤ role cap; each object ≤ `CANONICAL_LIMIT` | `src/encoding/delta/read.rs:139-218` |
| 5 | One pooled leaf chain | Bounded: leaf canonical ≤ 8192, record ≤ 8192, chain canonical ≤ 65 536, encoded ≤ 139 281, depth ≤ metadata cap | `src/encoding/pool/read.rs:244-313` |
| 6 | One pooled value group | Bounded: group body ≤ 16 KiB, ≤ 165 values, group digest authenticated before use | `src/pack/assemble.rs:144-155`; `src/encoding/pool/value_group.rs:68-93`; `src/encoding/pool/read.rs:135-146` |
| 7 | Decoded value cache | Bounded 512 KiB, released wholesale | `src/encoding/pool/read.rs:147-153` |
| 8 | Dependency/pool pack caches | Bounded 4 MiB each, released wholesale | `src/encoding/delta/read.rs:252-259`; `src/encoding/pool/read.rs:187-193` |
| 9 | Metadata catalogue scan | **Streamed, not materialised.** `for_each_group` hands one decoded row to the visitor at a time | `src/sqlite/pool.rs:154-168`; used by `src/encoding/pool/index.rs:224-248` |
| 10 | Catalogue endpoint query | Single scalar aggregate | `src/sqlite/pool.rs:44-55` |
| 11 | Locator page | Bounded 128 ids per statement, with the placeholder list built from the actual page width | `src/sqlite/lookup.rs:36-45`, `:58-69` |
| 12 | Cleanup page | Bounded 128 rows per statement and by the (trigger-based) transaction counters | `src/sqlite/cleanup.rs:44-56`, `:100-111` |
| 13 | `schema::create`/open validation | Reads `sqlite_master` rows for four tables and two indexes only, plus `PRAGMA table_info` per table | `src/sqlite/schema.rs:94-116`, `:194-197` |
| 14 | `highest_pack_id` / `existing_object_count` | Single scalar aggregates | `src/sqlite/lookup.rs:166-172`; `src/sqlite/schema.rs:286-295` |
| 15 | Encoding group framing | Bounded by `RECORD_COUNT_LIMIT` and the lane body limit before the output vector is allocated | `src/pack/assemble.rs:29-75` |
| 16 | One assembled pack | Bounded by the lane's pack limit before allocation | `src/pack/assemble.rs:231-244` |
| 17 | One FULL/PREFIX record | Bounded by `CANONICAL_LIMIT` / profile raw and frame limits | `src/encoding/full.rs:111-213` |
| 18 | Pooled COPY/INSERT program build | Bounded by `(target.len() + 1).min(GROUP_LIMIT)` and the match budget | `src/encoding/pool/delta.rs:125-225` |

**F2 (the only genuinely unbounded aggregate above)** is #2: a wave is bounded in *object count* but not in *bytes*, and #1/#3 inherit it.

---

## 10. Findings

| Id | Severity | Finding | Evidence |
|---|---|---|---|
| F1 | medium | The transaction row/canonical-byte bounds are checked *after* accumulation and are exceeded by one whole object or pack, so the declared "4 MiB − 1 canonical bytes per transaction" is not a hard cap. Recorded in the roadmap table as if it were. | `src/cas/owner.rs:819`, `:835`, `:840-844`; `src/policy.rs:69-71`; `admission-and-persistence.md:344` |
| F2 | medium | A read wave is bounded by object count only (`READ_OBJECT_LIMIT`); there is no aggregate byte ceiling on the returned values, and `canonical_bytes` is reported but never enforced. | `src/cas/store.rs:259-268`; `src/cas/read.rs:72-96` (bound absent) |
| F3 | medium | A pack BLOB is materialised from SQLite before any width validation; the pack-limit check happens in `parse_header` on the already-allocated buffer. | `src/sqlite/lookup.rs:152-163`; `src/pack/layout.rs:287-289` |
| F4 | medium | `StoreProvider` reports every storage failure — including integrity/identity failures, visibility refusals and capacity refusals — as `ContentError::MissingObject`, while the C1 trait contract reserves `IdentityMismatch` for a provider that cannot establish identity. | `src/cas/provider.rs:50-57`, `:60-68`; `core/crates/layerfs-content/src/object/access.rs:26-28` |
| F5 | low | The reference bound "pack INSERT ≤ 1 MiB of BLOB" is not implemented or reconciled; one statement binds up to 16 781 312 bytes. | `src/sqlite/write.rs:76-85`; `src/policy.rs:88-90`; `admission-and-persistence.md:345` |
| F6 | low | The pooled delta producer's program limit is `target.len() + 1`, i.e. exactly the FULL record width, so a maximal program is built and then rejected by the caller — wasted encode work, no correctness impact. | `src/encoding/pool/delta.rs:138-140`; `src/cas/owner.rs:551-553` |
| F7 | low | Two observability accessors report 0 when the shared pooled-index mutex is poisoned, so a poisoned index is indistinguishable from an empty one. No retry, no second attempt. | `src/cas/store.rs:172-182` |
| F8 | info | "Exactly one delta trial per save" is not what the code or the contract says: it is **one trial per target object** (and one per pooled leaf), so a save may run many trials. Review wording should be corrected rather than the code. | `src/encoding/delta/select.rs:275-301`; `src/cas/owner.rs:541-553`; `physical-encoding-and-packing.md:175-176` |
| F9 | info | Prepare-time lookups deliberately use `i64::MAX` as the ceiling (`save.rs` membership, `owner.offer` dependency validation, `seal_pending` re-read). This is safe only because acquisition refuses a Store whose watermark is behind its highest pack, and because the watermark cannot move during this save; the invariant is not restated at those call sites. | `src/cas/save.rs:27`, `:62`; `src/cas/owner.rs:352-354`; invariant `src/cas/owner.rs:177-186` |
| F10 | info | The schema identity is duplicated between `sql/schema.sql:11` (`user_version = 4`) and `src/policy.rs:27` (`SCHEMA_VERSION = 4`); drift would be caught at `create` by `validate`→`identity`, so the duplication is guarded, not silent. | `sql/schema.sql:10-11`; `src/policy.rs:27`; `src/sqlite/schema.rs:85`, `:138-147` |

## 11. What I did not verify (UNVERIFIED)

- **No build, test, clippy or benchmark was run.** Every behavioural statement is a reading of the source at HEAD `f288d2af`; runtime behaviour (actual pragma values on a real file, actual journal memory, actual allocations) is not measured here.
- The claim that `sqlite3_busy_timeout(0)` disables the busy handler is SQLite's documented contract, not something I exercised; what is verified from the tree is that rusqlite 0.40.2 forwards the value verbatim and that the product passes `Duration::ZERO`.
- The host `libsqlite3` version and whether it satisfies the STRICT-table requirement (≥ 3.37) is UNVERIFIED; a system `sqlite3` CLI reports 3.51.0 but that is not proof of the library the product links.
- The roadmap documents' own performance/measurement claims for Stages 2-4 are outside this review's scope and are not repeated here.
- Whether a real sequence of `COMMIT`→crash leaves exactly the described state is not tested (no process-kill experiment was run); the source-level ordering in §6 is what is verified.
