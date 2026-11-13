# Store

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

The one global Store: every project's history and immutable objects in one SQLite file or one PostgreSQL schema, written by one writer at a time.

| | |
| --- | --- |
| Layer | [Engine](../README.md) |
| Crates | `layerfs-persistence` |
| Source | [`store/`](../../../../engine/crates/layerfs-persistence/src/store), [`backend/`](../../../../engine/crates/layerfs-persistence/src/backend), [`history/`](../../../../engine/crates/layerfs-persistence/src/history), [`metadata/`](../../../../engine/crates/layerfs-persistence/src/metadata), [`objects/`](../../../../engine/crates/layerfs-persistence/src/objects), [`storage/`](../../../../engine/crates/layerfs-persistence/src/storage), [`blocks/`](../../../../engine/crates/layerfs-persistence/src/blocks), [`sql/`](../../../../engine/crates/layerfs-persistence/sql) |
| Uses | [Core: Physical storage](../../core/physical-storage/README.md), [Core: History](../../core/history/README.md), [Core: Telemetry](../../core/telemetry/README.md) |
| Used by | [Store calls](../store-calls/README.md), [Project Init](../project-init/README.md), [Commit](../commit/README.md), [Daemon](../daemon/README.md), [Control: Stores and projects](../../control/stores-and-projects/README.md), [Control: Backups](../../control/backups/README.md), [Control: Doctor](../../control/doctor/README.md) |
| Flows | [A call, end to end](../../flows/call.md), [Project Init](../../flows/init.md), [A Commit](../../flows/commit.md), [Fork and merge](../../flows/merge.md) |

## Picture

```text
 StoreSelection --open--> Handles
                            |
     +----------------------+----------------------+
     v                      v                      v
 storage                 history                acquisition (Init)
 StorageProvider /       HistoryProvider        working tables
 PostgresStorage         (core HistoryCatalog)
 (core PackPersistence)     |
 metadata/ objects/         |
 storage/ (bounds)          |
     |                      |
     +---- Mechanics -------+---- HistoryStatements
                 |
        Engine::Sqlite(Session) | Engine::Postgres(Session)
                 |                        |
   admission turn -> dir flock      admission turn -> store_lane row
   -> BEGIN IMMEDIATE               -> READ COMMITTED write
                 v                        v
        one Store file (WAL)       one schema in a PostgreSQL database
```

Code above `backend/` holds no SQL. History, the Metadata and Objects bounds
and publication validation are shared; they reach an engine through two
traits, `Mechanics` and `HistoryStatements`
([`seam.rs`](../../../../engine/crates/layerfs-persistence/src/backend/seam.rs)).
History SQL is written once as SQLite text and runs on PostgreSQL unchanged
except the statements PostgreSQL replaces
([`statements.rs`](../../../../engine/crates/layerfs-persistence/src/backend/postgresql/history/statements.rs)).

## What it does

| Behaviour | Detail | Source |
| --- | --- | --- |
| What one Store holds | Every project (LayerStack), its Layers, Branches and Commits, and every pack, object location, inode value block and binding hint block they reach. Nothing converts a Store from one kind to the other. | [`history.sql`](../../../../engine/crates/layerfs-persistence/sql/sqlite/history.sql), [`metadata.sql`](../../../../engine/crates/layerfs-persistence/sql/sqlite/metadata.sql), [`schema.sql`](../../../../engine/crates/layerfs-persistence/sql/postgresql/schema.sql) |
| Selection | `StoreSelection` is `Sqlite(PersistenceConfig)` or `Postgres(PostgresSelection)`. Without the `postgresql` feature the PostgreSQL arm is `BackendUnavailable`. | [`selection.rs`](../../../../engine/crates/layerfs-persistence/src/store/selection.rs), [`config.rs`](../../../../engine/crates/layerfs-persistence/src/store/config.rs) |
| Open | `Handles::open_writable` / `open_read_only` with a binding key and a nonzero cursor key; the open checks the table set, binding, catalog id and recorded schema. A read-only open refuses every write before `BEGIN`. | [`open.rs`](../../../../engine/crates/layerfs-persistence/src/store/open.rs), [`postgresql.rs`](../../../../engine/crates/layerfs-persistence/src/store/postgresql.rs) |
| Handles | `storage` (pack publication, reads, allocation), `history` (`HistoryProvider`) and `acquisition` (Init's working tables) share one session: one connection and one admission queue. | [`handles.rs`](../../../../engine/crates/layerfs-persistence/src/store/handles.rs) |
| File-only operations | `Handles::file` gives `StoreFile` for SQLite (`snapshot`, `heads_digest`, idle `checkpoint`); `None` for PostgreSQL. | [`selection.rs`](../../../../engine/crates/layerfs-persistence/src/store/selection.rs) |
| Close | `Handles::close` closes admission; queued and later calls fail `Closed`. | [`admission.rs`](../../../../engine/crates/layerfs-persistence/src/backend/admission.rs) |
| History catalog | `HistoryProvider` implements the core `HistoryCatalog`; each call is one transaction (`run(false, ..)` to read, `run(true, ..)` to write) on either engine. | [`catalog.rs`](../../../../engine/crates/layerfs-persistence/src/history/catalog.rs), [`transaction.rs`](../../../../engine/crates/layerfs-persistence/src/history/transaction.rs) |
| Allocation | `reserve` advances `next_pack_id` and `next_ordinal` in one write; a publication may hand back an unused ordinal tail only while the cursor still ends there. | [`metadata/allocation.rs`](../../../../engine/crates/layerfs-persistence/src/metadata/allocation.rs) |
| Locations and signatures | `locate` accepts only rows asked for, once each; `object_location` is insert-only, so the first publication of an object wins and later ones report it `lost`. The signature ring is an upsert by slot that keeps the newer stamp. | [`locations.rs`](../../../../engine/crates/layerfs-persistence/src/metadata/locations.rs), [`signatures.rs`](../../../../engine/crates/layerfs-persistence/src/metadata/signatures.rs) |
| Value and hint blocks | Inode values and binding hints live in append-only blocks of at most 4,000 bytes and 255 entries, each ending in a CRC32C checked on every decode. Encoding is field packing only, so each entry expands to its exact canonical bytes. | [`blocks/`](../../../../engine/crates/layerfs-persistence/src/blocks), [`metadata/values.rs`](../../../../engine/crates/layerfs-persistence/src/metadata/values.rs) |
| Object reads | `read_packs`, `read_pack_selection` and `read_scoped_pack` read rows that never change once visible, so they run without `BEGIN`. | [`objects/read.rs`](../../../../engine/crates/layerfs-persistence/src/objects/read.rs) |
| Storage publication | `publish` validates a batch without SQL, then writes packs, locations, value blocks, the released ordinal tail and signatures in one short write. | [`publication.rs`](../../../../engine/crates/layerfs-persistence/src/storage/publication.rs) |
| The publish lane | `publish_lane()` holds one admission turn plus the cross-process writer (SQLite) or the `store_lane` row (PostgreSQL) across several short transactions, so a Commit's head check, merge step and publish run as one step. | [`sqlite/lane/lane.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/lane/lane.rs), [`postgresql/lane/lane.rs`](../../../../engine/crates/layerfs-persistence/src/backend/postgresql/lane/lane.rs) |
| Diagnosis | Read-only audits over one pinned snapshot; see [Diagnostics](diagnostics.md). | [`diagnostic/`](../../../../engine/crates/layerfs-persistence/src/diagnostic) |

### History calls

```text
 layer_stack (project)                 branch "feature"
   head_layer_id ---> L2                 base_layer_id ---> L1
                      | parent           head_commit_id --> C3
                      v                                     | parent
                      L1 <---- base_layer_id of C1,C2,C3    v
                      |                                     C2 -> C1
                      v
                      L0 (genesis)
```

| Catalog call | One write transaction does | Refusals and outcomes | Source |
| --- | --- | --- | --- |
| `initialize_layerstack` | Insert the stack and its genesis Layer | name or identity taken: `Integrity` | [`layerstack.rs`](../../../../engine/crates/layerfs-persistence/src/history/layerstack.rs) |
| `fork` | Insert one Branch row at a Layer, or at a Commit on the source Branch's first-parent chain; nothing is copied | Commit not in history: `NotInHistory` | [`branch.rs`](../../../../engine/crates/layerfs-persistence/src/history/branch.rs) |
| `commit_branch` | Check the expected head and base, insert or verify the Commit, record its lineage, advance the Branch | moved head: `HeadMoved` unless `force` with the same base; same root: `UpToDate` | [`commit.rs`](../../../../engine/crates/layerfs-persistence/src/history/commit.rs) |
| `add_layer` | Publish a Branch head Commit as a Layer on the stack head and record the Layers it overwrote | `HeadMoved`, `StackMoved`, `BaseMismatch`; outcomes `UpToDate`, `NoChanges` | [`layerstack.rs`](../../../../engine/crates/layerfs-persistence/src/history/layerstack.rs) |
| `merge_branch` | Check both sides still sit at the merged nodes, then insert a new Branch: a fast-forward to a Layer or Commit, or a merge Commit with `second_parent` | moved side: `StackMoved` / `HeadMoved` | [`merge.rs`](../../../../engine/crates/layerfs-persistence/src/history/merge.rs) |
| `reserve_inodes` | Consume a half-open inode range for a scope; serials are never recycled | past `i64::MAX`: `Capacity` | [`allocation.rs`](../../../../engine/crates/layerfs-persistence/src/history/allocation.rs) |
| `record_bindings`, `record_file_digests` | Append binding hint blocks; insert digests of chunked files only | — | [`hints.rs`](../../../../engine/crates/layerfs-persistence/src/history/hints.rs), [`digests.rs`](../../../../engine/crates/layerfs-persistence/src/history/digests.rs) |
| `remove_layerstack` | Delete one project's totals, lineage, Branches, Commits, Layers and stack row; packs, objects, digests and its allocator scope stay | unknown stack: `Missing(LayerStack)` | [`removal.rs`](../../../../engine/crates/layerfs-persistence/src/history/removal.rs) |

A Commit id derives from its root, parent, base Layer, second parent and
metadata; an identical row is verified, not inserted again. A forced Commit
records the head it displaced. LWW decisions and merge units are encoded in
the `lww` column ([`facts.rs`](../../../../engine/crates/layerfs-persistence/src/history/facts.rs)).
The merged tree itself is built before `merge_branch` by
[Store calls](../store-calls/README.md).

`layer_lineage`, `commit_lineage`, `absorbed` and `reach` answer
`merge_base`, `is_ancestor`, `commit_history` and `ancestry_contains` by
walking run segments and merges, never single Commits; answers are exact and
have no budget. A Store written before these tables gains them at writable
open, filled once by `backfill_lineage`
([`lineage.rs`](../../../../engine/crates/layerfs-persistence/src/history/lineage.rs),
[`ancestry.rs`](../../../../engine/crates/layerfs-persistence/src/history/ancestry.rs),
[`lineage_backfill.rs`](../../../../engine/crates/layerfs-persistence/src/history/lineage_backfill.rs)).

### The publish lane in a Commit

```text
 Store-half Commit (store-service)                   lane   Store writer
 capture; construct candidate; Save                    -    short writes
 read head; moved? merge with head                     -    reads
 Save.finish; count; binding hints; validate           -    short writes
 history.publish_lane()  ---------------------------> HELD
   re-read head; moved again? merge what landed        |    read, writes
   commit_branch: head check, insert, advance          |    1 write tx
 drop(lane)  <---------------------------------------- free
 install the new base
```

`branch_snapshot` reads a Branch with its stack, base root and head Commit in
one statement. Work that grows with the Commit runs before the lane
([`commit.rs`](../../../../engine/crates/layerfs-store-service/src/store/commit.rs)).
`fork`, `merge_branch` and `add_layer` check their expected heads inside their
own write transaction, which the one writer already orders.

## Invariants

| Rule | Enforced by |
| --- | --- |
| One global database per Store; no sharding or second database | One `Session` per open; the exact table set is checked at open: [`sqlite/schema.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/schema.rs), [`postgresql/schema.rs`](../../../../engine/crates/layerfs-persistence/src/backend/postgresql/schema.rs) |
| One writer at a time: FIFO admission per session, then a cross-process lock (a directory `flock` on Linux and macOS, or the `store_lane` row) | [`admission.rs`](../../../../engine/crates/layerfs-persistence/src/backend/admission.rs), [`writer_admission.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/lane/writer_admission.rs), [`postgresql/session/profile.rs`](../../../../engine/crates/layerfs-persistence/src/backend/postgresql/session/profile.rs) |
| Waiting for a turn is readiness, never a retry; the queue has no time limit and a holder that unwinds releases its turn | [`admission.rs`](../../../../engine/crates/layerfs-persistence/src/backend/admission.rs) |
| Short holds: only head check, merge step and publish run under the lane | [`commit.rs`](../../../../engine/crates/layerfs-store-service/src/store/commit.rs) |
| Reads never take the lane or the cross-process lock | [`sqlite/session/transaction.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/session/transaction.rs), [`read_service.rs`](../../../../engine/crates/layerfs-store-service/src/store/read_service.rs) |
| Disposable / WAL / synchronous=OFF is the only permitted SQLite profile; product openers select it explicitly and refuse anything else | [`store_service.rs`](../../../../engine/crates/layerfs-store-service/src/store_service.rs), [`init.rs`](../../../../engine/crates/layerfs-project/src/store/init.rs), [`live.rs`](../../../../engine/crates/layerfs-project/src/store/live.rs), [`install.rs`](../../../../engine/crates/layerfs-daemon/src/install.rs) |
| One attempt per operation: `busy_timeout=0`, no busy handler, no retry or replay; `Unknown` quarantines the session and every later call fails without SQL | [`records.rs`](../../../../engine/crates/layerfs-persistence/src/backend/records.rs), [`sqlite/session/transaction.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/session/transaction.rs) |
| Published packs, units, locations and value blocks never change; a block overlapping a stored one fails its publication | Triggers and insert-only statements: [`objects.sql`](../../../../engine/crates/layerfs-persistence/sql/sqlite/objects.sql), [`sqlite/metadata/values.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/metadata/values.rs) |
| Pack ids never go back; inode serials are never recycled | [`metadata/allocation.rs`](../../../../engine/crates/layerfs-persistence/src/metadata/allocation.rs), [`history/allocation.rs`](../../../../engine/crates/layerfs-persistence/src/history/allocation.rs) |
| Every value a writer depends on is fixed at creation or read inside its own write transaction | [`seam.rs`](../../../../engine/crates/layerfs-persistence/src/backend/seam.rs), [`commit.rs`](../../../../engine/crates/layerfs-persistence/src/history/commit.rs) |
| Open never converts or migrates beyond the named additive steps (`add_binding_hints`, `add_lineage`, `add_merge_scratch`, `add_file_digests`) | [`open.rs`](../../../../engine/crates/layerfs-persistence/src/store/open.rs) |
| Every opener of a SQLite Store shares one kernel and a local filesystem | [`kernel_guard.rs`](../../../../engine/crates/layerfs-store-service/src/kernel_guard.rs) |
| Every opener holds whole-Store authority; there is no per-Branch scoping between openers | The handles expose the whole catalog: [`handles.rs`](../../../../engine/crates/layerfs-persistence/src/store/handles.rs) |

Durable persistence: the `Durable` SQLite variant still compiles, but its
execution is `NOT_RUN — disabled by owner until explicit reauthorization`.
Every product opener selects Disposable by name, and new code must too: the
Rust enum's `#[default]` attribute still sits on the retained `Durable`
variant, so an unnamed profile is a defect. A Disposable Store survives
process termination,
and a kernel or VM crash can lose or damage it
([`config.rs`](../../../../engine/crates/layerfs-persistence/src/store/config.rs)).
A PostgreSQL Store has no profile; its write setting is `synchronous_commit`.

## Limits and failures

| Limit or failure | Value or error | Source |
| --- | --- | --- |
| Platform for SQLite | macOS or Linux; elsewhere `BackendUnavailable` | [`open.rs`](../../../../engine/crates/layerfs-persistence/src/store/open.rs) |
| Direct opening for Store calls | Linux, Store directory on ext4, xfs or btrfs; else `Unsupported`, no fallback | [`kernel_guard.rs`](../../../../engine/crates/layerfs-store-service/src/kernel_guard.rs) |
| Cross-process writers (Linux and macOS) | Directory `flock` on the Store's parent directory; an uncoordinated raw SQLite writer still gets SQLite `Busy` | [`writer_admission.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/lane/writer_admission.rs) |
| PostgreSQL without the feature | `BackendUnavailable` | [`selection.rs`](../../../../engine/crates/layerfs-persistence/src/store/selection.rs) |
| PostgreSQL lane wait | `lock_timeout_ms`, default 30,000; then `Busy` | [`config.rs`](../../../../engine/crates/layerfs-persistence/src/store/config.rs) |
| Rows per publication or `reserve` | `TRANSACTION_ROW_LIMIT` 8,191 | [`publication.rs`](../../../../engine/crates/layerfs-persistence/src/storage/publication.rs) |
| Bytes per publication | `publication_bytes`: 4 MiB − 1 canonical and physical on SQLite, 16 MiB − 1 on PostgreSQL; a single payload pack with one object is exempt | [`publication.rs`](../../../../engine/crates/layerfs-persistence/src/storage/publication.rs), [`postgresql/storage/provider.rs`](../../../../engine/crates/layerfs-persistence/src/backend/postgresql/storage/provider.rs) |
| Ids per `locate`, `read_packs`, `inode_values` | at most 4,096 | [`locations.rs`](../../../../engine/crates/layerfs-persistence/src/metadata/locations.rs), [`objects/read.rs`](../../../../engine/crates/layerfs-persistence/src/objects/read.rs) |
| Ordinals | end ordinal at most 2^32; else `Capacity` | [`metadata/allocation.rs`](../../../../engine/crates/layerfs-persistence/src/metadata/allocation.rs) |
| Signature ring | 8,192 slots | [`signatures.rs`](../../../../engine/crates/layerfs-persistence/src/metadata/signatures.rs) |
| SQLite disk margin | a pack batch must leave `STORE_HEADROOM_BYTES` (64 MiB) after counting its bytes twice; else `Capacity` | [`headroom.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/lifecycle/headroom.rs) |
| Schema version 1 Store (SQLite `user_version` 16–18, 22–24) | refused at open, naming the offline `tools/store_blocks.py migrate`; PostgreSQL version 1 is `Integrity` | [`records.rs`](../../../../engine/crates/layerfs-persistence/src/backend/records.rs), [`connection.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/session/connection.rs) |
| Work in the lane that grows with tree size, history or Branch count | a defect; lineage walks follow runs, not history length | [`lineage.rs`](../../../../engine/crates/layerfs-persistence/src/history/lineage.rs) |

| `BackendError` | `PersistenceError` | `HistoryError` | Meaning |
| --- | --- | --- | --- |
| `Busy` | `Busy` | `Busy` | Another writer held the lock |
| `Closed` | `Refused "Closed"` | `OwnershipUnavailable` | Store closed before this call's turn |
| `Rejected` | `Refused "Rejected"` | `OwnershipUnavailable` | PostgreSQL refused a new connection |
| `Cancelled` | `Refused "Cancelled"` | `OwnershipUnavailable` | A signal interrupted the writer wait (Linux and macOS) |
| `ReadOnly` | `Refused "ReadOnly"` | `ContinuityUnavailable` | Write on a read-only session |
| `Capacity` | `Refused "Capacity"` | `Capacity("persistence")` | A bound or the disk margin was reached |
| `Integrity` | `Malformed` | `Integrity("persistence")` | Stored or supplied data broke a rule |
| `SchemaVersion1` | `Refused` (names the conversion) | `Integrity` | A schema version 1 SQLite Store |
| `UnsupportedSchemaVersion` | `Refused` | `Unsupported` | Diagnostic open of an unknown layout |
| `Unknown` | `Uncertain` | `UnknownOutcome` | Outcome unknown; session quarantined |

Source: [`records.rs`](../../../../engine/crates/layerfs-persistence/src/backend/records.rs).

## Demonstration

| Test or example | Command | Platform | Shows |
| --- | --- | --- | --- |
| [`heads.rs`](../../../../engine/crates/layerfs-persistence/tests/heads.rs) | `cargo +1.85.1 test --locked -p layerfs-persistence --test heads` | macOS, Linux (no-sidecar check Linux only) | Project, fork and Commit change the heads exactly; a snapshot has the same heads |
| [`writer_admission.rs`](../../../../engine/crates/layerfs-persistence/tests/writer_admission.rs) | `cargo +1.85.1 test --locked -p layerfs-persistence --test writer_admission` | macOS, Linux (`Cancelled` Linux only) | FIFO order, `Closed` on close, writers queued behind the lane, self re-entry refused, the directory lock across processes and concurrent opens, its hand-over, and `Cancelled` |
| [`diagnostics_during_lane.rs`](../../../../engine/crates/layerfs-persistence/tests/diagnostics_during_lane.rs) | `cargo +1.85.1 test --locked -p layerfs-persistence --test diagnostics_during_lane` | any | Counter reads return while another thread holds the lane |
| [`layerstack_removal.rs`](../../../../engine/crates/layerfs-persistence/tests/layerstack_removal.rs) | `cargo +1.85.1 test --locked -p layerfs-persistence --test layerstack_removal` | macOS, Linux; also PostgreSQL when `LAYERFS_TEST_POSTGRES_URL` is set | Removing one of two projects leaves the other whole, frees the name and takes its logical bytes out of the totals |
| [`binding_hint_blocks.rs`](../../../../engine/crates/layerfs-persistence/tests/binding_hint_blocks.rs) | `cargo +1.85.1 test --locked -p layerfs-persistence --test binding_hint_blocks` | macOS, Linux | Hints merge across blocks without repeats |
| [`postgresql_acquisition.rs`](../../../../engine/crates/layerfs-persistence/tests/postgresql_acquisition.rs) | `LAYERFS_TEST_POSTGRES_URL=… cargo +1.85.1 test --locked -p layerfs-persistence --features postgresql --test postgresql_acquisition` | a PostgreSQL server; prints `NOT_RUN` without the variable | Creating a PostgreSQL Store, refusing a second create, and Init's acquisition over it |

## Pages

| Page | What it covers |
| --- | --- |
| [SQLite](sqlite.md) | The SQLite backend: schema, sessions, lane, profile, checkpoints, seal, snapshot, heads, acquisition and merge records. |
| [PostgreSQL](postgresql.md) | The PostgreSQL backend: schema, open checks and authority, connections, transactions, publication, acquisition, access and status. |
| [Diagnostics](diagnostics.md) | Read-only Store audits: observations, the SQLite and PostgreSQL audits, table coverage and the structural fallback. |