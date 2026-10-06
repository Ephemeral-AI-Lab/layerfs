# Provider-owned acquisition backing

> **Status:** Current general guide. S9/A2 checkpoint; S9 remains open. Project does
> not use this port yet, and no speed, RSS, page or cold-cache claim is made.

The global Store can hold the working state of an initial root acquisition in
three operation-scoped tables, reached through one backend-neutral port. This
is the capability [A1](../issues/307/A1-ACQUISITION-CONTRACT.md) designed.
Project's import still runs on file ordering runs
([backed initial acquisition](43-backed-initial-acquisition.md)); moving it onto
this port and removing the run code is A3.

## Boundary

- **Port:** `layerfs_storage::port::acquisition`. The `Acquisition` trait and its
  typed rows contain no SQL, table name, cursor or connection. It is separate
  from `PackPersistence`, which stays an immutable-object contract.
- **Provider:** `layerfs_persistence::AcquisitionProvider`, the third field of
  `Handles`, over the one `Session` the storage and history handles share. No
  unit opens, attaches or creates anything.
- **Dependency edges:** unchanged. Persistence → Storage already existed.

## Schema selection

Acquisition tables are selected when a Store is created,
`PersistenceConfig::with_sqlite_acquisition(SqliteAcquisitionSchema::Tables)`,
and recorded in the schema version. Opening follows the stored version and never
adds them.

| `user_version` | Pack layout | Acquisition tables |
| --- | --- | --- |
| 1, 2, 3 | Monolithic, GroupRows, GroupRowsIndexed | absent |
| 4, 5, 6 | the same layouts | `init_operation`, `init_entry`, `init_native_file` |

Versions 1–3 are created and validated exactly as before. For 4–6 the exact
table set also contains the three tables and the engine's `sqlite_sequence`,
and `history_meta.schema_source`/`schema_definition` are recorded from, and
compared with, the scripts that include `sql/sqlite/acquisition/schema.sql`.
`store_policy` and `history_meta` carry the stored version. On a version 1–3
Store every unit returns `BackendUnavailable` before any SQL; on a read-only
Store `begin` is the Session's `ReadOnly` refusal. There is no migration.

## Tables and keys

All keys begin with `operation_id`, so concurrent operations occupy disjoint
ranges of the shared tables.

- **`init_operation`** — `operation_id INTEGER PRIMARY KEY AUTOINCREMENT`, so an
  identity is never issued twice, even after its row is deleted; `owner_epoch`;
  `phase`; source and binding facts; held, peak and removed row/byte charges.
- **`init_entry`** — `WITHOUT ROWID`, primary key
  `(operation_id, parent_position, name)`. A directory is read only after its
  own position is final, so this key is stable from the moment a child is
  observed, is the unique parent/name binding, and orders rows in acquisition
  order: breadth first, children by name bytes. The root is parent `-1`, empty
  name, position 0. `position` is NULL only for a child of a directory still
  being ordered, which also carries its 60-byte native identity until placed.
  A partial unique index `(operation_id, position) WHERE kind=2` serves the
  directory frontier and directory roots. Only directory rows keep a path.
- **`init_native_file`** — `WITHOUT ROWID`, primary key
  `(operation_id, canonical_position)`, with a unique
  `(operation_id, device, inode)` index as the conflict target. Device and inode
  are 8-byte big-endian BLOBs. The first path to bind an identity inserts the
  row; a later path with byte-equal 44-byte evidence increments `aliases` and
  receives the first position; unequal evidence changes nothing.

## Units

Each unit is one `Session::run` attempt: one short transaction, no wait and no
retry. The owner `(operation, epoch)` is matched against its stored row before
the unit's body, so a released, foreign or earlier-session owner is `Stale`
before any row is read or written. Failures are `AcquisitionError`: the
provider's `PersistenceError`, `Stale`, `Bounds` for a window above the port
maxima or a malformed row, and `Changed { position }` when evidence differs or
an addressed row is not in the required state. A failed unit changes nothing.

- **Write windows** are at most 4096 rows and 1 MiB of payload: `put_entries`,
  `place_children`, `complete_files`, `set_directory_roots`.
- **Read windows** are at most 512 rows and 256 KiB, resumed from the last key:
  `entries`, `unplaced_children`, `directories`, `jobs`, `file_roots`. The row
  count is derived from the byte limit and the largest row the statement can
  return, so the byte bound holds without fetching past it. Path-bearing
  windows are therefore at most 62 rows.
- **Lifecycle:** `begin`, `advance`, `discard` (a budgeted job deleting the
  operation's lowest remaining keys, entries first), `release` (refused while
  rows remain), `work`.

The session's epoch is the identity of the first operation it began. Operations
of other epochs are **abandoned**: nothing adopts or removes them. `abandoned`
lists them and `discard_abandoned` removes one named operation's rows and then
its record. Calling it is the caller's statement that the former owner is
fenced; the provider does not infer that.

## Accounting

A row's payload is its variable bytes: name, native path, native identity and
object identities. A write unit charges exactly what it stores and adjusts the
charge when placement clears an identity or a root is recorded; `discard`
reads the removed rows' payload from the engine and moves it to the removed
totals. These are logical charges of one operation. They are not pages, file
growth, journal bytes or RSS.

## Evidence and limits

Public tests on real Stores cover the version matrix, exact validation and
tamper refusal, acquisition order, identity sharing, wide-directory placement
through name windows, charges equal to the engine's own sums, budgeted cleanup
to zero, owner fencing and abandoned operations. The tree case runs under both
the Durable and the Disposable profile; the others under Disposable only.
`Handles::explain_acquisition` returns the product build's plan of every shipped
statement: all 25 are primary-key or index searches with no scan, temporary
B-tree or automatic index. The two removal statements build a subquery list and
Bloom filter bounded by their row budget. The same 512-row read window cost 4
statements and 9267 VM steps, and the same 512-row removal job 6 statements and
33434 VM steps, with 2000 and with 20000 stored entries; full-scan steps, sorts
and automatic-index rows were zero throughout. Receipts are under
[checks](../issues/307/checks/s9-acquisition-provider/identity.json).

Not established: time, page writes, file growth, journal or synchronization
cost under either profile; row growth when positions and roots are filled
after insertion; behaviour at capacity or under an uncertain engine outcome
(neither was induced); the frozen window values, which remain the proposed
ones; and the unit multiplicity of a real acquisition, which exists only once
A3 ports Project. The Store is macOS-only, so these tests execute no body on
Linux.
