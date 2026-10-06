# A1: provider-owned initial-acquisition backing

> **Status:** Proposal; target LayerFS v0.1.7; not a released contract.
> Design deliverable A1 of the [S7/S9 remaining plan](IMPLEMENTATION-PLAN-S7-S9-20261006.md),
> written 2026-10-06 against source `8bd03d76243987c365a06453d83c45015f72d4a5`.
> Nothing here is implemented. Section 9 lists two Store-format decisions that
> are the owner's and must be resolved before A2 creates any table.

## 1. What this settles and what it does not

Project stays the single initial-acquisition implementation and gains no engine
dependency. Its input-sized working state moves from file ordering runs into
three operation-scoped tables in the existing host-owned global Store, reached
through one backend-neutral port. The streamed canonical constructors
(`build_directory`, `empty_directory`, `build_table`), acquisition order, native
identity rules and the root-equivalence oracle are unchanged.

This document fixes the ownership boundary, the port's bounded units, the
concrete tables and keys, Store open/creation compatibility, cleanup placement,
accounting and restart disposition. It does not qualify speed, residency or
page I/O: every plan below is an expectation that A2 must confirm with EXPLAIN
and a correlated execution profile on the product SQLite build.

## 2. Facts of the current source that constrain the design

| Fact | Source | Consequence |
| --- | --- | --- |
| `init(&Storage, &dyn HistoryCatalog, InitRequest, timer)`; `Storage` wraps `Arc<dyn PackPersistence>` | [init.rs](../../../crates/layerfs-project/src/import/init.rs), [persistence port](../../../crates/layerfs-storage/src/port/persistence.rs) | Neither input exposes mutable operation state. A capability must be added explicitly. |
| `Handles { storage, history }` share one `Arc<Session>`: one connection behind a mutex taken with `try_lock` | [handles.rs](../../../crates/layerfs-persistence/src/store/handles.rs), [transaction.rs](../../../crates/layerfs-persistence/src/backend/sqlite/transaction.rs) | A third handle over the same Session needs no new database or connection. Cross-thread overlap is the existing one-attempt `Busy`, never a wait. |
| `Session::run(writable, body)` is one `BEGIN [IMMEDIATE]`/`COMMIT` attempt; an uncertain outcome quarantines the Session | same | Every acquisition unit is one short transaction. Nothing spans construction, a Save or host I/O. |
| Open validates `application_id`, `user_version` in {1,2,3}, the exact table set, and `history_meta.schema_source`/`schema_definition` against the compiled scripts | [schema.rs](../../../crates/layerfs-persistence/src/backend/sqlite/schema.rs) | Adding tables to an existing version breaks every existing Store. Compatibility needs new versions (section 6). |
| `temp_store=2` (MEMORY) is part of the checked profile | [profile.rs](../../../crates/layerfs-persistence/src/backend/sqlite/profile.rs) | TEMP tables are resident. Working rows are ordinary tables in the main database. |
| `DiskFull`/`TooBig` map to `Capacity`, lock contention to `Busy`, I/O failure to `Unknown` | [rows.rs](../../../crates/layerfs-persistence/src/backend/sqlite/rows.rs) | Existing typed classes cover acquisition failures; no new taxonomy. |
| The global Store is macOS-only; elsewhere creation returns `BackendUnavailable` | [open.rs](../../../crates/layerfs-persistence/src/store/open.rs) | SQL-backed acquisition executes on the macOS host. Linux exercises Project through external test ports. |
| The owning thread alone feeds jobs and accepts objects; four constructors only construct | [files.rs](../../../crates/layerfs-project/src/import/files.rs) | All port calls come from the owning thread, interleaved with its own Save publications. |

## 3. Ownership and wiring

```text
layerfs-project ──uses──▶ layerfs-storage::port::acquisition   (trait + typed rows, no SQL)
layerfs-persistence ──implements──▶ the same port              (SQL, Session, statements)
host application ──composes──▶ Handles { storage, history, acquisition }
```

- **Port home:** `layerfs-storage/src/port/acquisition/`, beside the pack port.
  It is a separate trait, not new methods on `PackPersistence`: the pack port is
  an immutable-object contract that remote and test providers implement, and
  mutable operation state does not belong in it.
- **Provider:** `layerfs-persistence` adds `AcquisitionProvider { session }` and
  a third public field `Handles::acquisition`. It is constructed by the same
  explicit `create`/`open_writable` that builds the other two handles. There is
  no per-Init open, attach or schema statement.
- **Caller:** `InitRequest` gains `acquisition: &dyn Acquisition`. In A3 this
  **replaces** `scratch_parent`; until then both shapes do not coexist in one
  release of the type. This is an explicit public-contract change of
  `InitRequest`. `init`'s signature, `Initialized`, canonical formats and the
  Store policy are unchanged. A host that already holds `Handles` passes
  `&handles.acquisition`; no caller creates a Store per operation.
- **Dependency edges:** none added. Project → Storage and Persistence → Storage
  exist. Project → Persistence, Project → rusqlite and Persistence → Project
  remain refused by the unchanged boundary guard.
- **Tests:** Project's memory-port tests need an external in-memory
  implementation of the port under `tests/support`, like their existing memory
  metadata and history. That is test support, not a second product backend.

Daemon Workspace and Commit scratch stay in the daemon-local Overlay database.
This port is host-side initial acquisition only.

## 4. Port contract

Every method is one named, bounded semantic unit over typed values, in the style
of `HistoryCatalog`: no SQL text, table name, cursor or connection crosses the
port. Errors are the existing `PersistenceError` classes. A window is bounded by
**rows and bytes**; the provider stops at whichever is reached first and the
caller resumes from the last key it received.

| Unit | Kind | Bound | Access (section 5) |
| --- | --- | --- | --- |
| `begin(facts)` → owner | write | 1 row | insert `init_operation` |
| `placement()` → native directory of mutable backing files, if any | none | constant | none |
| `put_entries(owner, window)` → canonical positions of regular files | write | window | append `init_entry`; upsert `init_native_file` for positioned regular files |
| `unplaced_children(owner, parent, after_name, limits)` | read | window | PK range, one parent |
| `place_children(owner, parent, assignments)` → canonical positions | write | window | PK point updates; native upserts |
| `directories(owner, after_position, limits)` | read | window | partial directory index |
| `directory(owner, position)` | read | 1 row | partial directory index |
| `jobs(owner, after_position, limits)` | read | window | `init_native_file` PK range |
| `complete_files(owner, roots)` | write | window | PK point updates |
| `entries(owner, after_key, limits)` | read | window | `init_entry` PK range |
| `native_files(owner, after_position, limits)` | read | window | `init_native_file` PK range |
| `native_file(owner, position)` | read | 1 row | `init_native_file` PK point |
| `set_directory_roots(owner, roots)` | write | window | PK point updates |
| `advance(owner, phase, facts)` | write | 1 row | `init_operation` point update |
| `discard(owner, budget)` → rows and bytes removed, remaining | write | budget | PK prefix range deletes |
| `release(owner)` | write | 1 row | delete the operation row, refused while working rows remain |
| `work(owner)` | read | 1 row | `init_operation` point read |
| `abandoned(after_operation, limit)` / `discard_abandoned(operation, epoch, budget)` | read / write | window / budget | section 8 |

**Owner and fencing.** `begin` returns `(operation_id, owner_epoch)`. Every
later statement carries both and matches them in its own `WHERE` clause, so a
handle from another Session, a released operation or a different operation
changes zero rows and is reported as `Missing`, never applied.

**Window limits.** Read windows are at most 512 rows and 256 KiB of returned
column bytes. Write windows are at most 4096 rows and 1 MiB of bound bytes.
These are proposed starting values. The write window is deliberately larger
because under the Durable profile each window is one synchronized commit;
A2 freezes both values from its profile evidence, not from this document.

**Failure.** One attempt per unit. `Busy`, `Capacity`, `ReadOnly` and integrity
failures are definite and leave the unit's transaction rolled back. An
uncertain outcome quarantines the Session exactly as it does for storage and
history; the operation cannot continue and its rows are reported as retained.

## 5. Tables, keys and access paths

Three ordinary `STRICT` tables in the main database, created once with the Store
(section 6) and reused by every later Init. All keys start with `operation_id`,
so concurrent operations occupy disjoint key ranges.

```sql
CREATE TABLE init_operation (
 operation_id INTEGER PRIMARY KEY AUTOINCREMENT,
 owner_epoch INTEGER NOT NULL CHECK(owner_epoch>0),
 phase INTEGER NOT NULL CHECK(phase BETWEEN 1 AND 7),
 source_device BLOB NOT NULL CHECK(length(source_device)=8),
 source_inode BLOB NOT NULL CHECK(length(source_inode)=8),
 stack_id BLOB NOT NULL CHECK(length(stack_id)=16),
 scope BLOB NOT NULL CHECK(length(scope)=32),
 entries INTEGER NOT NULL DEFAULT 0 CHECK(entries>=0),
 serial_start INTEGER CHECK(serial_start>=0),
 held_rows INTEGER NOT NULL DEFAULT 0 CHECK(held_rows>=0),
 held_bytes INTEGER NOT NULL DEFAULT 0 CHECK(held_bytes>=0),
 peak_rows INTEGER NOT NULL DEFAULT 0, peak_bytes INTEGER NOT NULL DEFAULT 0,
 removed_rows INTEGER NOT NULL DEFAULT 0, removed_bytes INTEGER NOT NULL DEFAULT 0,
 failure INTEGER, genesis_root BLOB CHECK(length(genesis_root)=32)
) STRICT;

CREATE TABLE init_entry (
 operation_id INTEGER NOT NULL REFERENCES init_operation(operation_id),
 parent_position INTEGER NOT NULL CHECK(parent_position>=-1),
 name BLOB NOT NULL CHECK(length(name)<=255),
 position INTEGER CHECK(position>=0),
 kind INTEGER NOT NULL CHECK(kind BETWEEN 0 AND 2),
 canonical_position INTEGER CHECK(canonical_position>0),
 metadata_root BLOB CHECK(length(metadata_root)=32),
 content_root BLOB CHECK(length(content_root)=32),
 native_path BLOB CHECK(length(native_path)<=4096),
 PRIMARY KEY(operation_id,parent_position,name)
) STRICT, WITHOUT ROWID;
CREATE UNIQUE INDEX init_entry_directory
 ON init_entry(operation_id,position) WHERE kind=1;

CREATE TABLE init_native_file (
 operation_id INTEGER NOT NULL REFERENCES init_operation(operation_id),
 canonical_position INTEGER NOT NULL CHECK(canonical_position>0),
 device BLOB NOT NULL CHECK(length(device)=8),
 inode BLOB NOT NULL CHECK(length(inode)=8),
 evidence BLOB NOT NULL CHECK(length(evidence)=44),
 native_path BLOB NOT NULL CHECK(length(native_path) BETWEEN 1 AND 4096),
 aliases INTEGER NOT NULL DEFAULT 0 CHECK(aliases>=0),
 file_root BLOB CHECK(length(file_root)=32),
 PRIMARY KEY(operation_id,canonical_position)
) STRICT, WITHOUT ROWID;
CREATE UNIQUE INDEX init_native_file_identity
 ON init_native_file(operation_id,device,inode);
```

The column list is the design; exact kind codes, the `phase`/`failure`
enumerations and the name/path byte limits are taken from the existing Content
constants when A2 writes `schema.sql`.

### Why these keys

- **Stable entry identity is `(operation_id, parent_position, name)`.** A
  directory is scanned only after its own position is final, so a child's parent
  position is known the moment the child is observed, whatever order the host
  enumerates it in. That key is therefore stable before the child's own position
  exists. It is also the unique `(parent, binary name)` binding, and its order
  *is* acquisition order: breadth first, children by name bytes. One B-tree
  serves identity, uniqueness and ordered streaming, with no table fetch. The
  root is `(operation, -1, x'')` at position 0.
- **`position` is assigned, not derived.** It is NULL only for children of a
  directory wider than the resident window, between their insertion and their
  placement. Consumers streaming in key order also count rows and require the
  stored position to equal the count, so a missing row is an integrity failure
  rather than a silently shifted serial.
- **Directory frontier.** The partial unique index holds directory rows only and
  gives keyset windows by position and a point read of one directory's path.
  Non-directory rows carry no path: a child's path is its directory's path plus
  its name, read once per directory group.
- **Native identity.** `init_native_file` is keyed by canonical position, so the
  job feed, root completion and the inode-table merge are covering PK ranges and
  inserts append in increasing position. The `(device, inode)` unique index is
  the conflict target. Positions are assigned in globally increasing order, so
  the first path to reach the upsert is the canonical one by construction; later
  paths increment `aliases` only when their 44-byte evidence (length, mode,
  mtime, ctime) is byte-equal, and an unequal evidence changes nothing and is
  the existing `InvalidInput`. Device and inode are 8-byte big-endian BLOBs:
  exact unsigned values, never squeezed into a signed integer.
- **Counts.** `aliases` counts only bindings inside the acquired root. Alias
  entries still consume their own position and serial; holes are never reused.

### Statement inventory and expected plans

| Statement | Expected plan |
| --- | --- |
| children of one parent after a name | `SEARCH init_entry USING PRIMARY KEY (operation_id=? AND parent_position=? AND name>?)` |
| entries after a key | `SEARCH init_entry USING PRIMARY KEY (operation_id=? AND (parent_position,name)>(?,?))` |
| directories after a position; one directory | `SEARCH init_entry USING INDEX init_entry_directory (operation_id=? AND position>?)` / `position=?` |
| update one entry by key | `SEARCH init_entry USING PRIMARY KEY (… AND name=?)` |
| native files after a position; one by position | `SEARCH init_native_file USING PRIMARY KEY (operation_id=? AND canonical_position>?)` / `=?` |
| native upsert | insert with `ON CONFLICT(operation_id,device,inode) DO UPDATE SET aliases=aliases+1 WHERE evidence=excluded.evidence RETURNING canonical_position,aliases` |
| delete entries up to a key; native files up to a position | PK range `SEARCH` with an upper bound |

No statement uses `OFFSET`, a temporary B-tree or a scan without the operation
prefix. A throwaway probe of this schema on the system SQLite 3.51.2 printed
exactly these plans and confirmed the upsert's three outcomes, the exact 64-bit
identity round trip and that a deleted highest `operation_id` is not reissued.
That probe used a different SQLite build and an in-memory database. It is a
design sanity check, not A2 evidence, and is not retained as a receipt.

## 6. Store creation and open compatibility

Acquisition tables are a **creation-time schema selection**, recorded in the
schema version, never added to a Store that was opened.

| `user_version` | Pack layout | Acquisition tables | Status |
| --- | --- | --- | --- |
| 1, 2, 3 | Monolithic, GroupRows, GroupRowsIndexed | absent | existing; validation byte-for-byte unchanged |
| 4, 5, 6 | the same three layouts | present, acquisition schema 1 | new; created only when explicitly selected |

- **Creation.** `PersistenceConfig` gains a creation-only selection of the
  acquisition schema, beside the existing pack-layout selection. Selected, the
  bootstrap transaction also runs `sql/sqlite/acquisition/schema.sql`, and
  `history_meta.schema_source`/`schema_definition` are recorded from the
  scripts actually run, exactly as today.
- **Open.** The stored version selects both layout and acquisition presence.
  Validation stays exact: the table set for 4–6 is the layout's set plus the
  three tables and `sqlite_sequence` (created by `AUTOINCREMENT`), and source and
  definition must equal the compiled scripts for that version. Any other version
  is the existing `Integrity` refusal.
- **No migration.** A version 1–3 Store opens as before and has no acquisition
  capability: `begin` returns `BackendUnavailable` before any SQL. Nothing
  upgrades it implicitly. No integrity check is relaxed.
- **Read-only.** `begin` and every write unit are refused by the existing
  `ReadOnly` check before `BEGIN`.
- **Durability.** Working rows are ordinary rows of the global Store and follow
  its selected profile. Under Durable every write window is a WAL commit with
  the profile's full synchronization; under Disposable none is synchronized and
  nothing survives a crash. Neither profile is changed for working rows.

## 7. Flow, cleanup placement and accounting

A3 ports the import to this flow. Order, identities and the published root are
required to be identical to today's.

1. **Check and begin.** `check_root`; refuse a source that contains
   `placement()` using the ancestor-identity check A4 added; `begin`.
2. **Scan with attributes.** One prerequisite Save is open for the scan. Each
   observed child has its metadata root, and for a symlink its target root,
   constructed and accepted as it is observed, so its row is inserted complete
   and the separate prerequisite pass over every entry disappears. A directory
   that fits the resident 512-child window is ordered in memory and appended
   with final positions in one or more write windows. A wider directory is
   inserted unplaced, then placed by name keyset windows. Positioned regular
   files reach the native upsert in increasing position. The prerequisite Save
   is finished after the scan.
3. **Files.** The owner feeds the fixed job queue from `jobs` windows and
   records completed roots with `complete_files`. Later paths are rechecked from
   `entries` plus one `native_file` point read per alias.
4. **Tree.** `reserve_inodes`, then the tree Save: entries stream by key into
   `build_directory`, each directory root is recorded with
   `set_directory_roots`; then entries and native files merge by position into
   `build_table`.
5. **Cleanup before publication.** With every window consumer finished and the
   tree Save still unfinished, `discard` removes the operation's entry and
   native rows in budgeted jobs until none remain. A failed `discard` drops the
   Save unpublished and is `ProjectError::Cleanup` with the operation, rows and
   bytes still held. This keeps today's rule: nothing final exists until
   working state is gone.
6. **Publish and release.** Finish the tree Save, `initialize_layerstack`, then
   `release` the operation row and return. The tables and indexes stay.

**Failure disposition.** A definite failure in steps 1–4 makes the working rows
consumer-free: Init runs `discard` once and `release`, and returns the original
error, or `Cleanup` carrying it when the discard fails. An uncertain Store or
history outcome leaves rows and the operation row in place, because the
quarantined Session can execute nothing further; Init returns the original
uncertain error and the retained operation identity. Nothing is retried.

**Accounting.** Each write unit updates `held_rows`/`held_bytes` and their peaks
in the same transaction, from the bytes it bound; `discard` moves them to
`removed_*`. These are exact logical charges of one operation. They are not
pages, file growth or RSS. Page count, freelist and file allocation are Store
observations around the operation and are attributable to it only when no other
writer interleaved. `NamespaceWork` reports the charges and the resident window
capacities; its file-run fields are retired with the run code.

**Cost shape.** For E entries, D directories, U native identities, A later paths
and wide-directory children V: about E + 2U + A + D + V row writes and
E + U + V row reads through windows, each O(log(E + U)) with sequential locality
for all but the identity index and wide-directory inserts. Cleanup is E + U row
deletions in bounded range jobs. Row bytes are written twice under WAL. A
Durable Store pays one synchronized commit per write window, roughly
(E + 2U + A + D + V) / 4096 commits before cleanup. That is real cost this
design adds over unsynchronized scratch files, and E2/E3 must measure it.

## 8. Restart and abandoned operations

An operation never resumes. Its Saves and cursors are process state.

- `owner_epoch` is the `operation_id` of the first operation the current
  Session began. It needs no host randomness and no extra table, and it differs
  for every Session that ever began an operation.
- Rows whose epoch is not the current Session's are **abandoned**. No open,
  `begin` or timer touches them.
- `abandoned` lists them in bounded pages. `discard_abandoned` removes one
  named operation's rows within a budget. Calling it is the host's explicit
  statement that the prior owner is fenced. The provider cannot establish that,
  and a process exit observed by nobody is not such a statement. Which host
  component makes it, and on what evidence, is R3's work.
- Under Disposable a crash can lose or corrupt the whole Store. Abandoned-row
  handling claims nothing beyond that profile.

## 9. Decisions and checks carried forward

**Owner decisions required before A2 changes Store creation:**

1. **Default or opt-in.** Recommended: acquisition tables are **opt-in at
   creation**. Versions 1–3 and every proof pinned to their schema identity stay
   bit-identical; a host that will Init selects the acquisition schema when it
   creates the Store. The alternative makes 4–6 the default for all new Stores.
2. **Existing version 1–3 Stores after A3.** Once the run code is removed they
   cannot Init, because the plan forbids two selectable acquisition algorithms.
   Recommended: Init on such a Store is an explicit typed refusal, and an
   explicit owner-invoked upgrade operation is a separate later decision, not
   part of A2/A3. The alternative is to design that upgrade now.

**Checks A2 must make on the product build, with retained receipts:**

- `AUTOINCREMENT` and `sqlite_sequence` under the Store's defensive
  configuration, and the exact table set and recorded definition for 4–6.
- EXPLAIN and a correlated execution profile for every statement above,
  including rows fetched, the foreign-key lookup on each child insert and
  caller-loop multiplicity.
- Row growth when `position`, `content_root` and `file_root` are filled after
  insertion, and its effect on pages written.
- The frozen read and write window values under both profiles.
- Create, reopen, read-only refusal, version 1–3 refusal, stale-owner refusal,
  capacity failure and uncertain-outcome retention through the public port.

**Unchanged obligations:** no per-Init database, attach, table creation or drop;
no TEMP-memory whole root; no weakened integrity check; no retry; no
`VACUUM` or reopen in cleanup; the boundary guard is not edited to admit an
edge. S9 stays incomplete: A2, A3 and qualification are not delivered by this
document.
