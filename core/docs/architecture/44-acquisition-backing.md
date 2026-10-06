# Provider-owned acquisition backing

> **Status:** Current general guide. S9 acquisition checkpoint; S9 remains open.
> Component measurements and their limits are linked below.

The latest owner-selected work reduction narrows insertion inputs, maps reads
directly into typed windows, adds a narrow directory-binding stream and combines
final disposal with operation release. The selected schema, Store durability,
window maxima and single Project importer remain. Its prospective source-qualified
measurement is [the work reduction checkpoint](../issues/307/SPACE-AND-SCALING-PLAN-20261006.md#owner-selected-work-reduction-checkpoint).

The global Store can hold the working state of an initial root acquisition in
three operation-scoped tables, reached through one backend-neutral port. This
is the capability [A1](../issues/307/A1-ACQUISITION-CONTRACT.md) designed.
Project's import is its one consumer and has no other acquisition algorithm
([backed initial acquisition](43-backed-initial-acquisition.md)).

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

Each unit is one attempt: one short transaction or owned single-statement read
snapshot, no wait and no retry. Writes and two-pass path reads use `Session::run`
and match the owner `(operation, epoch)` before the body. Single-statement reads
drive an indexed payload LEFT JOIN from that owner match in the same SQLite
snapshot. A valid empty window has an ownership-only NULL sentinel; a stale
owner returns no row and cannot enter the payload seek. A released, foreign or
earlier-session owner is `Stale` before working rows are accessed. Failures are `AcquisitionError`: the
provider's `PersistenceError`, `Stale`, `Bounds` for a window above the port
maxima or a malformed row, and `Changed { position }` when evidence differs or
an addressed row is not in the required state. A failed unit changes nothing.

- **Write windows** are at most 4096 rows and 1 MiB of payload: `put_entries`,
  `place_children`, `complete_files`, `set_directory_roots`. The port fixes the
  payload a caller is charged per entry or placed row, `WRITE_ROW_BYTES` plus its
  name and native path bytes, so a caller sizing a window by that charge is
  never refused by the provider's own.
- **Read windows** are at most 512 rows and 256 KiB, resumed from the last key:
  `entries`, `unplaced_children`, `directories`, `jobs`, `file_roots`. The row
  count is derived from the byte limit and the largest row the statement can
  return, so the byte bound holds without fetching past it. Path-bearing
  windows are therefore at most 62 rows. Every window bound is written
  `LIMIT ?n+0`: the engine's planner reads a plainly bound LIMIT, which
  re-prepared the statement on every execution and let its plan follow the
  value. The expression keeps the one generic plan that `explain` reports.
- **Lifecycle:** `begin`, `advance`, `discard` (a budgeted job deleting the
  operation's lowest remaining keys, entries first), `dispose` (the same bounded
  removal with atomic operation release once empty), `release` (refused while
  rows remain), `work`. Existing adapters default `dispose` to one `discard`
  and explicitly report that release remains; Project then releases only after
  successful empty cleanup. SQLite includes the final release in its acknowledgment.

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

Single-statement owned reads have separate `SqlWork.read_snapshots` and
`read_snapshot_ns` observations. Explicit transactions/commits keep their own
counters; no read COMMIT or synchronization is fabricated. Successful SQL
snapshots can return a typed stale-owner refusal after finding no owner row.
Snapshot wall includes preparation, execution, mapping and lease release and
overlaps statement wall. It is not additive CPU attribution.

## Work reduction proof checkpoint

The insertion input binds161/97/225 parameters for dependency/native/entry SQL,
instead of257 for each. It retains a fixed32-row classified input, one binding
allocation and original dependency/refusal boundaries. Native inserts are omitted
when no positioned regular file exists. No uniqueness/evidence check is removed.
The same32 independent regular files use7 statements/7476 VM steps at2000 and
20000 stored files, compared with the retained7/7828 profile. A512-row full-entry
read uses1 statement/10277 VM steps at both populations, compared with4/9271:
fewer execution boundaries do not imply fewer VM steps. Direct typed mapping and
the narrow binding projection address copying separately.

Public proof covers truncated prefixes, directory conflicts, native evidence and
first-failure ordering, valid-empty/stale/foreign/read-only owner distinctions,
ordered/bounded windows and unsigned native identities. Bounded final disposal
checks exact charges, one write acknowledgment, stale-owner refusal after release
and another live operation's unchanged rows/roots. Existing mode0 compatibility,
schema fingerprints and constructor/root oracles remain covered. No new latency,
physical-write or allocation PASS follows from these count proofs. Raw checks are
under [work reduction checks](../issues/307/checks/acquisition-work-reduction-20261006/).

## Evidence and limits

Public tests on real Stores cover the version matrix, exact validation and
tamper refusal, acquisition order, identity sharing, wide-directory placement
through name windows, charges equal to the engine's own sums, budgeted cleanup
to zero, owner fencing and abandoned operations. The tree case runs under both
the Durable and the Disposable profile; the others under Disposable only.
`Handles::explain_acquisition` returns the product build's plan of every shipped
statement. The original 25-statement point/window plans and the revised root
window plans are retained at their own source identities. Stored-state reads
and updates use the operation-prefixed primary keys or directory index. Root
updates additionally materialize and scan a fixed 32-row SQL input; that is
bounded statement scratch, not a scan of the stored population. The original removal statements built a subquery list and
Bloom filter bounded by their row budget. The same 512-row read window cost 4
statements and 9267 VM steps, and the same 512-row removal job 6 statements and
33434 VM steps, with 2000 and with 20000 stored entries; full-scan steps, sorts
and automatic-index rows were zero throughout. Receipts are under
[checks](../issues/307/checks/s9-acquisition-provider/identity.json).

A3 ran Project's Init over the provider and correlated the whole Session's
profile. That showed one automatic re-prepare per window statement execution,
which the provider-only profile had not counted. With the `+0` bound the plans
are textually identical, the window costs 4 statements and 9271 VM steps, the
removal job 6 statements and 33438 VM steps at both populations, and
re-prepares are zero for write, read and removal units; the profile test now
asserts that. Creating a Store with the acquisition tables costs 5 more
statements, 18 more full-scan steps and 2 more sorts than one without, once, in
schema creation and validation. For 1000 files in 10 directories one Init made
30 read units, 5 write units, 1 removal job and 1 release, and the Session
executed about 3160 more statements and 391 000 more VM steps than the
run-backed import did on the same source. Receipts are under
[A3 checks](../issues/307/checks/s9-acquisition-port/identity.json).

The [first component measurement](../issues/307/NAMESPACE-INIT-ACQUISITION-RESULTS-20261006.md)
records four 100/1000-file pairs at source45e2b09e8, before the execution changes
below. All functional/cold-content/cleanup checks pass; three relative-speed
gates fail. Those receipts remain unchanged and do not qualify the revised source.

The [revised-source checkpoint](../issues/307/ACQUISITION-WINDOW-FIX-RESULTS-20261006.md)
retains three speed FAILs and the unchanged Disposable100 PASS at4c03b41bf;
complete speed acceptance remains open.

Not established: physical page writes, peak file/journal
growth or synchronization-call cost under either profile; row growth when positions and roots are filled
after insertion; behaviour at capacity or under an uncertain engine outcome
(neither was induced); and the frozen window values, which remain the
proposed ones. The unit multiplicity above is one small shape, not a
qualification workload. The Store is macOS-only, so these tests execute no body
on Linux.


## Window execution correction (2026-10-06)

Public port methods, table/version selection, ownership, window maxima and
transaction boundaries are unchanged. `put_entries` and `place_children` lease
their two cached statements once per unit and keep their original ordered
per-row executions. Native aliases, evidence mismatch and first-failure order
therefore retain the same semantics. No statement/transaction is retained across
construction, caller I/O or a later unit.

`complete_files` and `set_directory_roots` now feed fixed 32-slot `VALUES` inputs
into indexed `UPDATE ... FROM` statements. NULL positions pad unused slots.
The file target is searched by `(operation_id,canonical_position)`; the directory
target by its partial `(operation_id,position)` index with `kind=2`. No schema,
index, database, per-operation TEMP table or extra writer is introduced. The
existing 4096-row/1-MiB public write window is processed completely through as
many bounded SQL inputs as it needs; 32 is an internal execution window, not a
new total-operation limit.

RETURNING order is unspecified, so the consumer sorts the at-most32 returned
positions and checks every requested position. A repeated position ends the
current distinct input; its next execution then sees the filled row and refuses
as before. An earlier missing row wins over a later duplicate. Any refusal rolls
back the entire existing unit, including previous SQL inputs; uncertain execution
sets the same quarantine fence. Logical root charges are applied only after all
inputs succeed. Inputs and returned-identity checks are O(32) resident and
O(32 log32) work per SQL input; target updates are O(K logN) for K supplied roots
and N operation-scoped stored rows, plus bounded input/engine scratch. Over a
complete acquisition the existing O(E+U) removals and backing ownership still apply.
No N-sized scan or N-by-K join is used.

The shared `prepared.rs` executor records statement/VM/row/error counters on
every execution and prepare/drop phases on actual lease acquisition/release.
COMMIT remains a subset of statement/transaction wall; component times overlap
and are never additive. Fixed-input fullscan steps are reported rather than
hidden: the32-root profile is5 statements/2176 VM steps/31 constant-input scan
steps, with0 sorts/automatic-index rows/reprepares at both2000 and20000 native rows.
The source-qualified plans, public atomicity/alias proofs and before/after unit
diagnostics are retained under [execution-fix checks](../issues/307/checks/init-acquisition-fix-20261006/).
The separately registered revised-source sample retains its actual partial result;
no diagnostic timing is promoted into speed acceptance.

## Independent entry execution inputs (2026-10-06)

The revised provider groups up to32 independent observed entries into fixed
`VALUES` inputs. Its indexed dependency statement checks the parent/name key,
directory-position index and both native keys in the same transaction as the
inserts. Within-input duplicate keys, directory positions, native identities or
native canonical positions end the prefix. A malformed row also ends it. Existing
rows and aliases end the independent prefix at the earliest input slot; that row
executes once in the original native-upsert/entry-insert order. This is a planned
dependency boundary, before a mutation attempt, not replay after an error.

A proven independent prefix uses one native INSERT and one entry INSERT, without
RETURNING rows or assuming engine iteration/return order. Its canonical native
positions are the original input positions. Only entirely new native identities
are grouped; existing identities retain the original evidence/alias/path rules.
The same short public unit encloses every input and dependency row and applies
its exact logical charge once. A refusal rolls back the entire unit. Existing
owner/epoch, abandoned custody and unknown-session quarantine checks are unchanged.
There is no schema, profile, index, public limit, database or constructor change.

For K input entries and N stored indexed rows, classifier/insertion work is
O(K logN) plus actual key/BLOB/index/page work. At most32 candidate entries and
four sets of at most32 borrowed keys are resident; repeated dependency entries
can classify up to32 candidates each, a fixed factor rather than a growing
population scan. SQL scans only fixed input rows. Packed native evidence is60
bytes per input; names, paths and object IDs stay borrowed through binding.
SQLite's own binding/materialization copies and journal/page costs remain real.
Public output positions remain bounded by the unchanged4096-row write unit.

Plans and actual runtime counters, ordered-refusal/rollback proofs and final
component qualification are retained under
[entry-window checks](../issues/307/checks/init-entry-window-20261006/selection.json).
This source description establishes no complete-operation speed, phase RSS,
physical-I/O or sustained-rate result. Historical speed FAILs remain unchanged.

The final real-Store profile for32 fresh regular files is7 statements/7828 VM,
zero measured fullscan/sort/autoindex/reprepare at both2000 and20000 stored rows.
The predecessor unit used68 statements/5353 VM. The extra indexed preflight
raises VM work46.236% while reducing execution count89.706%; speed acceptance
therefore depends on the separately registered complete-operation checkpoint.
All102 provider and157 Project/Storage/SDK test bodies pass; both-platform
changed-scope no-run/Clippy, source boundary, formatting and40 tool tests pass.
The changed provider executes on macOS; Linux compilation establishes no global
Store runtime support. Exact failures/check budgets and custody limits remain
in the linked check folder.

## Monolithic restoration, 2026-10-06

The schema7/10 [payload-layout experiment](45-immutable-payload-segments.md) is
withdrawn. Active acquisition uses original schema4–6 and the same Durable
Session; no placement/crash guarantee change is selected.


## Acquisition page reclamation and scaling, 2026-10-06

Implemented source scope is described in
[the prospective space/scaling record](../issues/307/SPACE-AND-SCALING-PLAN-20261006.md).
New acquisition-table Stores select incremental auto-vacuum before schema creation;
retained mode0 Stores still open without conversion. Canonical objects and pack
layouts are unchanged. Pointer-map pages are included in allocation. Each normal
nonempty discard/release pays at most512 free-page removals inside its existing
atomic unit. `Handles::reclaim_space` exposes one bounded job for residual debt;
it refuses unsupported mode0 without migration and preserves one-attempt outcomes.
WAL/FULL/fullfsync and MEMORY/OFF retain their scoped guarantees. Checkpoint alone
still does not compact internal SQLite free pages. Completion measurements pay
residual reclamation before final checkpoint/close.

Path-bearing windows inspect at most512 indexed scalar lengths and then fetch
only the actual prefix within the existing256-KiB column budget, in one snapshot.
Cleanup selects one inclusive endpoint with a job-local offset at most4095;
each successful job removes that prefix, so the offset never grows with N.
The short final prefix uses a reverse indexed seek. Range DELETE removes the
explicit IN-subquery list/Bloom filter; SQLite still buffers bounded deletion
keys/RETURNING values. Endpoint work is O(log N+K), deletion/index work
O(K log N), and physical maintenance is bounded per job. Exact row/byte charges,
entry-first order, owner fencing, rollback and uncertain custody are preserved.
The new-source plans/profiles and frozen cold measurements decide qualification;
implementation alone does not establish speed, allocation or S7/S9 acceptance.


The final-source correction batches live cleanup reclamation: it accumulates
less than512 free pages as reuse headroom, runs one at-most512-page job when
that threshold is reached, and reclaims a smaller tail at release or explicit
maintenance. This reduces repeated SQLite statement expiration/repreparation;
its source and runtime counters determine the actual effect. The v1 campaign
is retained, including all gate failures. The prospective final source uses
space-scaling-v2 cases with identical inputs, cold state,30/19s caps and gates.


The final measured [space/scaling checkpoint](../issues/307/SPACE-AND-SCALING-RESULTS-20261006.md) retains zero free-page
debt and empty acquisition tables in all final Stores. Three of eight latency
screens pass; zero strict allocation/joint screens pass. Matched public-reference
counts attribute essentially all extra VM work to acquisition, with immutable
publication unchanged. Large-case latency and S7/S8/S9 acceptance remain open.


## Streaming candidate disposition, 2026-10-07

The ordinary source is restored to the incumbent product at4a207cea1. Bounded
streaming Init was implemented and verified at5a9704610,890a144ff and1d3b6d3ae,
but the final source regressed the incumbent's large-case time and missed the
strict time/allocation gates. The single ordinary importer again records file
roots in acquisition SQL before the separate tree Save. The reviewed candidate
is retained on local branch `codex/init-streaming-candidate` in the attached
measurement worktree; it is not selected by an error or benchmark switch.
All source pins, proofs and failed receipts remain in
[the results](../issues/307/INIT-STREAMING-RESULTS-20261007.md).
