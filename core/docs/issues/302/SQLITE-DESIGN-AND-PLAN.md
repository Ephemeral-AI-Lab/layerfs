# Shared persistence, embedded durable SQLite first

> Status: Current owner-directed design and execution plan, 2026-10-04.
> Supersedes the active PostgreSQL/MinIO implementation direction and optimization
> campaign. Historical plans, changes, seals, failures and receipts remain intact.

## Direction and boundaries

Implement SQLite only. PostgreSQL is an explicit unavailable selection, not a
functioning backend, fake success or error-driven fallback. Rename the existing
layerfs-metadata crate to layerfs-persistence, replacing it rather than adding a
parallel engine crate. Remove layerfs-s3 and MinIO from the active product,
composition, setup, trace and accounting paths. No more MinIO performance runs.
The already-started rolling diagnostic finished; its evidence is retained and
owned services are down. Existing Git snapshots preserve the previous source.

One C1 construction/encoding/pack implementation, one C2 save/read flow, one
storage-provider flow and one history-provider flow. Backend directories contain
persistence mechanics only. Content, storage and history domain source import
no database/network engine. Adapt/reuse optimized reference algorithms; do not
create backend-specific copies of whole operation trees.

Init keeps application-supplied storage/history handles, stack authority, name,
scope seed, scratch parent and deadline. It does not restore Service, grants or
protocol envelopes. SQLite is embedded in the host application process and its
files are host-local. A sandbox cannot invoke a host Rust handle: the host
endpoint/application-adapter and its transport/authentication remain a separate,
explicit later decision. Removal of a server layer does not eliminate authority,
read-only access, history identity, concurrency or integrity requirements.

Existing root crates/ and the clean baseline checkout at7edddbdb8 remain
reference, not product dependencies or fallback implementations. Active native
engine code moves out of domain crates; reference retention and actual active
retirement are recorded separately. No destructive Git reset/rollback.

## Concrete responsibility map

```
core/crates/layerfs-storage/src/
  storage.rs location.rs policy.rs error.rs
  port/{mod.rs,persistence.rs,pack.rs,publication.rs,allocation.rs}
  save/ read/ encoding/ pack/
core/crates/layerfs-history/src/
  catalog.rs error.rs identity/ query/ records/ (domain contracts and types)
core/crates/layerfs-persistence/src/
  lib.rs config.rs open.rs handles.rs
  storage_provider.rs publication.rs
  objects/{mod.rs,packs.rs,read.rs}
  metadata/{mod.rs,policy.rs,locations.rs,allocation.rs,pooling.rs,signatures.rs}
  history/{mod.rs,provider.rs,catalog.rs,allocation.rs,layerstack.rs,layer.rs,
           branch.rs,commit.rs,staging.rs,rows.rs}
  backend/{mod.rs,records.rs}
  backend/sqlite/{mod.rs,connection.rs,transaction.rs,query.rs,rows.rs,
                  blob.rs,schema.rs,publish.rs}
core/crates/layerfs-persistence/sql/sqlite/
  objects.sql metadata.sql history.sql
  queries/{objects/,metadata/,history/}
core/crates/layerfs-persistence/tests/
  support/ objects/ metadata/ history/ publication/ backend/sqlite/
  publication_contract.rs sqlite_durability.rs sqlite_publication_bounds.rs
core/crates/layerfs-project/
  src/ (same direct Init/domain flow)
  tests/ examples/ (compose embedded SQLite handles)
```

This is a responsibility map, not an instruction to create empty modules. The
PostgreSQL marker can live in the public backend-selection enum; no empty driver
or PostgreSQL SQL tree is needed now. Selecting it returns BackendUnavailable.
lib.rs/mod.rs contain declarations/reexports/thin delegation only, <=200
physical lines; production files including SQL <=999. Tests are external.
A small typed internal backend seam exposes bounded scalar records, statements,
transaction outcomes and publication primitives; no generic ORM/plugin machinery.

Shared storage validates semantic bounds/results and delegates execution. Shared
history owns operation control/identity/conditional transition logic, with one
acknowledged transaction per operation. SQLite owns SQL, connection settings,
error classification, row conversion, BLOB mechanics and checkpoint execution.
Reuse the existing history operation logic and SQL semantics without retaining
rusqlite types/SQL in domain/shared high-level flows.

## Bounded persistence contract

Replace MetadataStore + ObjectStore with PackPersistence. Each publication unit
contains sealed payload/metadata bodies, pack descriptors, insert-if-absent
locators, pooled catalogue/signature updates, and allocation/window changes.
SQLite commits the whole bounded unit in one database transaction. There is no
remote upload-before-register phase or HTTP signing/authentication path.

Canonical identities, exact reuse/collision refusal, dependency closure,
first-wins handling, framing, ordinal monotonicity and unknown outcomes stay.
C2 forms/submits batches in save/publication.rs; adapter publication.rs
executes/validates them across Objects and Metadata. These are distinct responsibilities, not duplicate
construction/encoding algorithms. No transaction contains an entire namespace.
All twelve logical tables share one host-local SQLite Store database. C2 and
C5 still have separate bounded acknowledged transaction boundaries; sharing a
file does not make the whole namespace operation one transaction. No persistent
SQL files/directories mirror: filesystem structures remain canonical objects.

Objects: pack (complete sealed payload AND metadata bodies, ID/digest/length/domain).
Metadata: store_policy, object_location, metadata_value_group, content_signature.
History: history_meta, scope_allocator, layer_stack, layer, commit, branch,
workspace_stage. Engine-specific allocation mechanisms are not domain groups.
An import catalogue is separate operation-owned scratch, explicitly accounted
and cleaned, not an additional persistent filesystem table.

Limits are validated against actual SQLite runtime variable/SQL-length/column
limits. Multi-row locator insertion splits statements within the bounded atomic
unit, not the logical transaction. Construction waves keep their existing
canonical/row limits, fixed producer/Init constructor rules, bounded dependency
cache and backpressure. Define persisted-byte and represented-canonical-byte
charges separately; do not double-credit or hide binding copies. A single
oversized-object exception remains separately bounded and declared.

Read-only handles refuse mutation before SQL. Writes make one attempt; Busy is
immediate, no busy handler/retry. A definite failure rolls back. A possibly
committed result quarantines that handle and returns UnknownOutcome, without a
guessed rollback, replay, cleanup or automatic fallback. An application can
explicitly reopen a durable history authority only after schema/catalog/binding/
incarnation validation; acknowledged high-water reservations never recycle.

## Reuse the optimized SQLite physical writer

- Already sealed bounded packs: INSERT final bytes once. No redundant zeroblob
  followed by writes for this case.
- Genuinely appendable packs: reserve bounded capacity and use incremental BLOB
  writes for new body bytes, directory entries and control bytes only.
- Published pack rows are immutable: body, digest, length and domain never
  change after acknowledgement. Incremental appends apply only to genuinely
  unpublished construction. Never update companion columns in a growing BLOB
  row; that can rewrite the whole value. Unpublished bounded staging can use
  operation-owned temporary BLOB rows, then one final immutable INSERT, without
  adding persistent filesystem tables or changing a published sealed digest.
- Reuse/adapt existing pack placement, selected increments and SQLite BLOB writer
  where semantics match. Do not copy the whole legacy CAS operation tree into
  each backend, or wrap it with extra whole-pack/workspace copying.
- Preserve pack hashing. C2 computes the final sealed digest from bounded
  construction state, and publication verifies body/digest/length atomically.
  Hash calculation may traverse bounded existing segments without constructing
  a redundant whole growing BLOB copy. Final digests and canonical/group checks
  remain. A previous published digest never describes subsequently mutated bytes.
- Published readers use immutable pack descriptors. Private construction caches
  invalidate on their owner's append before serving private bytes; no
  error-triggered refresh/retry or stale-cache success.
- Batch/index dependency lookups and keep output buffers bounded. No namespace-
  sized payload transfer or spool.

## Selected durability profile

New profile identity: sqlite-wal-full-macos-fullfsync-v1. The old MEMORY/OFF
receipts keep their original identity/status and are not relabeled or pooled.

On every relevant macOS connection configure and read back journal_mode=WAL,
synchronous=FULL (2), foreign_keys=ON, fullfsync=ON and checkpoint_fullfsync=ON.
Pin busy_timeout=0, mmap_size=0 and a finite SQLite page-cache budget. Required
profile/capability mismatches fail explicitly. Record runtime version, compile
options, VFS/platform, page size, actual limits and every selected setting.

Pin wal_autocheckpoint=1000 pages (read back), record journal_size_limit and
actual WAL high-water allocation; checkpoint triggers/work occur inside the
publication/transaction acknowledgement/lifecycle scopes that perform them.
A passive checkpoint can be obstructed by readers; record pending/completed
frames and Busy rather than pretending the WAL is bounded or enlarging limits.
Explicit final checkpoint/close work is separately timed and remains inside
the declared complete-command scope. Database, WAL and SHM allocation plus
checkpoint work are counted honestly. Do not move required work into setup.

WAL/FULL requests WAL synchronization per commit; fullfsync selects macOS's
stronger full-sync mechanism where the VFS supports it. Verify settings and
observe sync mechanics where available. Process-kill recovery tests prove
process-crash behaviour, not physical power-loss durability or hardware honesty.
No unexplained durability claim beyond the recorded SQLite/VFS/OS contract.

Primary basis: https://www.sqlite.org/wal.html and
https://www.sqlite.org/pragma.html (synchronous, fullfsync,
checkpoint_fullfsync, wal_autocheckpoint).

## Workstream A: database publication first

A0. Commit this design/plan and preserve the stopped campaign evidence.
A1. Replace/rename adapter and active workspace wiring; PostgreSQL selection
fails explicitly, MinIO paths cease to be runnable active selections. Record
production relocation/retirement separately; keep historical evidence usable.
A2. Implement backend-neutral bounded publication/read/allocation contracts and
adapt the existing C2 save/read/placement path. Reuse append/final-insert writer.
A3. Implement SQLite connection/profile/schema/transaction/row/BLOB mechanics
and one shared provider over the Objects/Metadata groups. Verify actual engine limits and atomic units.
A4. Move/adapt one shared history flow into persistence; domain history becomes
engine-independent. Preserve explicit read-only authority/identity/concurrency,
conditional transitions, reservation persistence and quarantine semantics.
A5. Compose direct Project Init with SQLite handles; remove Service/MinIO setup.
A6. Cover atomic body+locator publication, rollback, first-wins/conflicts,
increment-only BLOB work, one-shot sealed inserts, limits/backpressure, foreign
identity/refusal, checkpoint settings/allocation and acknowledged recovery.
Run scoped owning core checks once at the frozen implementation; failures stay.
A7. Prospectively freeze a separately identified durable direct-API SQLite arm
and old optimized reference. Release/locked, one sample per case/arm, exact
source/compilation/dependency/profile/cache/workload identities, fresh outputs,
independent bounded proof. Report each scope, sync/checkpoint/BLOB/SQL/VM work,
allocation and non-passing line. No old35.5ms expectation/guarantee or relabel.

## Workstream B: file-count qualification, separately

The current importer is **not demonstrated bounded in namespace entry count**.
scan.rs streams payloads through construct_stream with four constructors and
four ImportBatch channel slots. Ordinary batches target256KiB, with a separately
bounded single oversized canonical object. These are not a total process-memory
bound. scan.rs also retains all PreparedEntry records/all Job paths, a directory
frontier and each directory's collected/sorted children. namespace.rs builds
whole serial/inode vectors, a directory-binding BTreeMap and DirectoryUpdate
vector. FileBacking ordering scratch does not remove these allocations.

B0. Add focused namespace_scaling external tests and actual counters for retained
entries/path bytes/jobs/frontier/children, serial/inode vectors and directory
bindings/updates. Separately account worker, pack, binding, database-cache and
OS/page-cache residency; lifetime peaks never become phase peaks.
B1. Register fixed-budget file-count cases prospectively. Default small cases
and explicit larger cases retain one sample/identity and append-only receipts.
Cases unable to fit stay NOT_RUN; do not reduce work, grow deadlines, or warm
inputs. Do not claim whole-importer bounds from payload admission alone.
B2. If measured entry-count collections exceed intended bounds, implement typed
catalogue records/port plus bounded ordered paging/job admission, preserving
canonical ordering, inode allocation, mutation checks and cleanup:

```
layerfs-project/src/catalogue/{mod.rs,records.rs,port.rs}
layerfs-project/src/scan_jobs.rs
layerfs-project/src/namespace_pages.rs
layerfs-persistence/src/backend/sqlite/import_catalogue.rs
layerfs-persistence/sql/sqlite/import_catalogue.sql
layerfs-project/tests/{namespace_catalogue.rs,namespace_streaming.rs}
```

Catalogue disk may grow with entries; record/clean it and verify resident bounds.
No workspace-sized payload spool. The current FilesystemInput/build_filesystem
accepts whole slices: paging followed by recollecting all rows into Vec does not
solve growth. A bounded iterator/page builder boundary may be required, with
unchanged canonical/inode semantics. SQLite stays behind a typed port. This
second-stage file sketch is conditional responsibility mapping, not scaffolding.

## Evidence and completion

Old PostgreSQL/MinIO campaign is stopped/superseded, not retrospectively passed.
Completed rolling rows:100 reference35,505,041/current199,277,084ns;1000
reference126,823,166/current414,721,792ns; both strict speed FAIL, functional/cache/
cleanup PASS. The already-started count diagnostic and seals are preserved.
Prior C5 PG Busy/autovacuum failure remains historical, not a SQLite result.

At this design commit A1-A7 and B0-B2 are NOT_RUN/incomplete. Implementation and
new durable measurement receipts determine completion; documentation alone is
not a working SQLite backend or a bounded whole-namespace claim. No aggregate
preflight/CI, third-party edits, push, PR or merge. Every local commit carries
exact parent/staged/committed production LOC and migration classification.

## Revised goal and acceptance direction (2026-10-04)

The user explicitly requests an active goal through actual terminal success:
the SQLite-only solution must pass all four Init tiers (100/1000/10000/100000)
and retained-history stride10/3/1 (17/53/157 states). Candidate comparison time
must be <=1.10 times the matched unmodified7edddbdb8 Phase4.5 time. Exact integer
arithmetic is `10*candidate_ns <= 11*baseline_ns`. This replaces the old strict
speed inequality for this new treatment only. Previous PG/MinIO FAIL receipts
are immutable. Required durability work is inside the comparison scope/lifecycle.

The precise new timing/resource/cache/correctness contract must be frozen before
samples. The proposed Init scope is complete child launch-to-exit, including
fresh Store creation/opening, real Init, required checkpoint and final close;
internal operation/bootstrap/commit/checkpoint spans are explanatory and may
overlap. Both arms pay their own fresh database lifecycle. No after-the-fact
subtraction of inconvenient terms. Counts/EXPLAIN and smaller-step comparisons
are mandatory for large gaps, using cause diagnostics rather than speed repeats.

Two contract points are pending direct user clarification: whether unchanged
historical60/170/170s history bounds are a scoped exception to current15/25s
rules; and the numeric Init allocation criterion (proposed: no greater than
matched Phase4.5 total allocation). Existing history ceilings remain54,278,964 /
70,427,034 /92,342,273B. Missing contract/proof/accounting work cannot become PASS.

An active goal is created without a token budget. A1-A6 now have material local
SQLite implementation and functional tests; A7 and all seven competitive
benchmark decisions remain NOT_RUN. B0 count instrumentation is implemented;
B1 larger-tier qualification and conditional B2 paging remain incomplete.

Init's prospective contract is now SQLITE-STEP10-CONTRACT.md: conservative final
DB/WAL/SHM allocation<=matched Phase4.5 final total, plus separate before-checkpoint
allocation; complete fresh-database lifecycle is included. History deadlines
remain pending and no history driver/sample is substituted. New runner registers
all seven, uses per-child wait4 accounting and fails unresolved history calls
before setup. No SQLite benchmark receipt has yet been taken.

Source retirement completed in b604389a4 after bd9ededba:140936->137448(-3488),
active core27987 unchanged. The prior checkpoint/LOC evidence remains unchanged.
