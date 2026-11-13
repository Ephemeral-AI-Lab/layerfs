# Diagnostics

> **Status:** Current architecture guide for LayerFS 2.0.0; maintained with the source.

Read-only audits of a [Store](README.md): one pinned snapshot, streamed observations, and engine checks, with no migration, checkpoint, repair or lane.

| | |
| --- | --- |
| Source | [`diagnostic/`](../../../../engine/crates/layerfs-persistence/src/diagnostic), [`sqlite/schema_diagnostic.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/schema_diagnostic.rs), [`postgresql/schema_diagnostic.rs`](../../../../engine/crates/layerfs-persistence/src/backend/postgresql/schema_diagnostic.rs), [`diagnostic_objects.sql`](../../../../engine/crates/layerfs-persistence/sql/postgresql/diagnostic_objects.sql) |

## Observations

```text
 caller (control doctor)
   Options { quick, deadline, cancelled }
   observer: FnMut(Observation)          <- streamed, caller keeps findings
        |
        v
 SqliteHandles::open_diagnostic(path)    PostgresHandles::open_diagnostic*
   .diagnose(options, observer)            .diagnose(options, observer)
        |  open failed?                         |
        v                                       v
 sqlite_structure(path, ..)               online physical checks
```

| Part | Meaning | Source |
| --- | --- | --- |
| `Observation { id, outcome, message }` | One stage, such as `store.snapshot`, `store.identity`, `store.schema`, `store.database.structure`, `store.history.lineage` or `store.ordinal_blocks`; messages never carry credentials | [`observation.rs`](../../../../engine/crates/layerfs-persistence/src/diagnostic/observation.rs) |
| `Outcome` | `Pass`, `Fail` (a concrete inconsistency), `Skipped` (excluded on purpose), `Unknown` (coverage not established); unsupported coverage is never a pass | same |
| `Options.quick` | Skip exhaustive pack, canonical object and namespace or content reads | same |
| `Options.deadline`, `cancelled` | Checked at SQL interruption and bounded readers; expiry leaves coverage incomplete, never a corruption verdict | same |
| `Resolver` | Bounded host resolution supplied by the control layer for PostgreSQL | same |

Control resolves the saved Store and its placement. A SQLite audit runs only
in the kernel that holds the file; elsewhere control sends the worker
operation `store.diagnose`
([`worker_server.rs`](../../../../control/crates/layerfs-control/src/worker_server.rs);
[Control: Doctor](../../control/doctor/README.md)).

## SQLite audit

```text
 diagnose: one read transaction (run(false)) on the original file
   pin view: history_meta incarnation     --> store.snapshot (WAL included)
   shipped schema objects vs actual       --> store.schema
   catalog id, binding, incarnation       --> store.identity
     == admitted authority?  else Fail; dependent checks blocked
   declared storage classes per table     --> store.database.types
   integrity_check(100) | quick_check(100) --> store.database.structure
   foreign_key_check (streamed)           --> store.database.foreign_keys
   scan::run: inventory and decoders      --> store.history.*, store.objects.*
 watcher thread: deadline or cancel -> interrupt the connection; joined
```

| Rule | Source |
| --- | --- |
| The pinned transaction includes committed WAL frames; a long audit keeps them in the log. | [`sqlite.rs`](../../../../engine/crates/layerfs-persistence/src/diagnostic/sqlite.rs) |
| `open_diagnostic` never changes journal or checkpoint settings and never migrates. | [`open.rs`](../../../../engine/crates/layerfs-persistence/src/store/open.rs) |
| Saved schema text and every actual schema object are compared with the shipped format in the same transaction, so rewriting the saved definition after dropping a trigger does not pass. | [`sqlite/schema_diagnostic.rs`](../../../../engine/crates/layerfs-persistence/src/backend/sqlite/schema_diagnostic.rs) |
| `quick_check` excludes uniqueness and index/table equivalence; engine output is capped at 100 messages, and reaching the cap is `Unknown` coverage. | [`sqlite.rs`](../../../../engine/crates/layerfs-persistence/src/diagnostic/sqlite.rs) |

## Structural fallback

When read-only admission or the logical audit fails, `sqlite_structure`
opens the file `READ_ONLY` with `query_only`, `busy_timeout=0` and runs
`integrity_check(100)` or `quick_check(100)` in one transaction. It is a
separate observation, not the logical audit's snapshot, and it clears
nothing. Only `SQLITE_CORRUPT`, `SQLITE_NOTADB` or non-`ok` check rows
confirm a structural failure; busy, permission, I/O, a missing file or an
unknown application schema do not. A clean result says nothing about schema,
authority, objects or history
([`sqlite_structure.rs`](../../../../engine/crates/layerfs-persistence/src/diagnostic/sqlite_structure.rs)).

## PostgreSQL audit

```text
 connection(selection, resolver, options)          preflight session
   hosts in configured order; each resolved address; Unix/TCP fallback;
   TLS by the configured policy and system trust     store.connection.*
   no overall deadline? one 10 s admission budget for both sessions
 open_diagnostic_admitted                          pinned session
   REPEATABLE READ READ ONLY; snapshot fingerprint   store.snapshot
   schema, catalog id, binding, incarnation again    store.schema/.identity
   live required objects vs 210 signatures           store.schema.objects
   inventory and decoders                            store.history.*, ...
 full mode only, after the snapshot ends:
   per relation: verify_heapam / bt_index_check      store.database.relations
```

| Rule | Source |
| --- | --- |
| Authentication, database or schema access and a wrong saved binding are access or authority failures; dependent coverage is blocked and no corruption is inferred. The second open validates the saved identity again. | [`postgresql.rs`](../../../../engine/crates/layerfs-persistence/src/diagnostic/postgresql.rs) |
| Admission compares the saved schema source and column definition with shipped constants on the server, from permission-independent catalogs; nothing unbounded is returned to the client to compare. | [`postgresql/schema_diagnostic.rs`](../../../../engine/crates/layerfs-persistence/src/backend/postgresql/schema_diagnostic.rs) |
| Required constraints, indexes, triggers and trigger functions are compared on the server with 210 normalized signatures from the shipped DDL; extra indexes are allowed; a different deparse on another server major is `Unknown`, not corruption. Nothing is created to do this. | [`postgresql_contract.rs`](../../../../engine/crates/layerfs-persistence/src/diagnostic/postgresql_contract.rs), [`postgresql_contract_expected.rs`](../../../../engine/crates/layerfs-persistence/src/diagnostic/postgresql_contract_expected.rs) |
| Heap tables, materialized views and sequences get `verify_heapam(check_toast, skip=none)`; ready, valid B-tree indexes get `bt_index_check(heapallindexed)`. Other relations are `Unknown` without a call. | [`postgresql.rs`](../../../../engine/crates/layerfs-persistence/src/diagnostic/postgresql.rs) |
| Missing amcheck, a missing signature or denied execution is `Unknown`; SQLSTATE `XX001`/`XX002` or returned heap rows are a `Fail` for that relation. amcheck is never installed, `bt_index_parent_check` never called, offline checksums never claimed. Heap output is capped at 100 rows. | same |
| Randomized multi-host admission is refused rather than changing the saved selection's meaning. | [`driver.rs`](../../../../engine/crates/layerfs-persistence/src/backend/postgresql/session/driver.rs) |
| Cancellation is carried by a scoped, joined watcher; server cancel and `ROLLBACK` get up to one more second, and an unacknowledged release is reported. Nothing is replayed. | [`driver.rs`](../../../../engine/crates/layerfs-persistence/src/backend/postgresql/session/driver.rs), [`postgresql.rs`](../../../../engine/crates/layerfs-persistence/src/diagnostic/postgresql.rs) |

Reads already admitted in the pinned transaction can stay valid after access
is revoked; a later denied read leaves coverage incomplete. Losing the backend
during a check is a provider failure, never invented corruption. Reports omit
passwords, credential-bearing URLs and raw driver parameters.

## Inventory and coverage

| Category | Validator | Quick | Source |
| --- | --- | --- | --- |
| `history_meta`, `store_policy`, `scope_allocator`, `store_totals` | Catalog identity and incarnation, frozen policy, allocation bounds, totals = sums of retained Layers, Commits, winning locations and pack descriptors | yes | [`catalog.rs`](../../../../engine/crates/layerfs-persistence/src/diagnostic/catalog.rs) |
| `layer_stack`, `branch`, `layer`, `"commit"` | Typed rows, identity derivation, heads, bases, parents, acyclic graph; namespace qualification once per root, scope and profile context | rows and graph yes; namespace no | [`history.rs`](../../../../engine/crates/layerfs-persistence/src/diagnostic/history.rs) |
| Commit facts | LWW and merge-unit grammar streamed in bounded windows | yes | [`facts.rs`](../../../../engine/crates/layerfs-persistence/src/diagnostic/facts.rs) |
| `layer_lineage`, `commit_lineage`, `absorbed`, `reach` | Rebuilt independently from first parents; saved run labels validated, never invented; exact overwrite counts | yes | [`lineage.rs`](../../../../engine/crates/layerfs-persistence/src/diagnostic/lineage.rs) |
| `pack`, `pack_unit`, `object_location` | Descriptors, framing of every record, canonical identity and every pooled or delta dependency through the existing readers | shape only | [`scan.rs`](../../../../engine/crates/layerfs-persistence/src/diagnostic/scan.rs), [`reader.rs`](../../../../engine/crates/layerfs-persistence/src/diagnostic/reader.rs) |
| `inode_value_block`, `binding_hint_block`, `content_signature`, `file_digest` | 4,000-byte bound before extraction, CRC and framing including unused blocks; ring arithmetic; file bytes hashed through a bounded sink | yes, except file bytes | [`auxiliary.rs`](../../../../engine/crates/layerfs-persistence/src/diagnostic/auxiliary.rs) |
| `init_*`, `merge_operation`, `merge_record` | Row shapes, charges and owner relations; owners are never expired or deleted | yes | [`acquisition.rs`](../../../../engine/crates/layerfs-persistence/src/diagnostic/acquisition.rs) |

Scans use actual row keys, not contiguous ids, and include retained rows
outside current heads ([`scan.rs`](../../../../engine/crates/layerfs-persistence/src/diagnostic/scan.rs)).
Duplicate physical copies, first-wins forward locators, allocator gaps,
removed projects and retained unreachable data are legitimate. Advisory
blocks, signatures and digests are reported apart from authenticated content
loss. Graph closure and indexed scratch live in a private SQLite temporary
file outside the Store, removed when its connection closes
([`scratch.rs`](../../../../engine/crates/layerfs-persistence/src/diagnostic/scratch.rs)).
Quick checks can still take time that grows with the Store.

## Overwrite reconstruction

```text
 for each publication at height h (source Commit S):
   n       = newest Layer height in S's ancestry (immutable edges)
   earlier = greatest earlier publication height whose source is a local
             Commit ancestor of S, or a Commit first absorbed at or below n
   covered = min(max(n, earlier), h - 1)
   expected overwrote_count = (h - 1) - covered
```

This is the graph equation behind `covered` in
[`history/lineage.rs`](../../../../engine/crates/layerfs-persistence/src/history/lineage.rs)
and `add_layer` in
[`history/layerstack.rs`](../../../../engine/crates/layerfs-persistence/src/history/layerstack.rs),
evaluated before the publication joins the rebuilt absorption map. It reads
neither saved reach answers nor saved run labels, and invalid authoritative
graphs block it explicitly
([`lineage.rs`](../../../../engine/crates/layerfs-persistence/src/diagnostic/lineage.rs)).

## Tests

| Test | Command | Shows |
| --- | --- | --- |
| [`store_diagnostic.rs`](../../../../engine/crates/layerfs-persistence/tests/store_diagnostic.rs) | `cargo +1.85.1 test --locked -p layerfs-persistence --test store_diagnostic` | Audits of real Disposable Stores with live WAL; damage found in full mode and named as excluded in quick mode |
| [`diagnostic_overwrite.rs`](../../../../engine/crates/layerfs-persistence/tests/diagnostic_overwrite.rs) | `cargo +1.85.1 test --locked -p layerfs-persistence --test diagnostic_overwrite` | Exact overwrite reconstruction, including late publication and retained forks |
| [`sqlite_structure_diagnostic.rs`](../../../../engine/crates/layerfs-persistence/tests/sqlite_structure_diagnostic.rs) | `cargo +1.85.1 test --locked -p layerfs-persistence --test sqlite_structure_diagnostic` | Raw structural errors stay distinct from unsupported schemas |
| [`postgresql_doctor_physical.rs`](../../../../engine/crates/layerfs-persistence/tests/postgresql_doctor_physical.rs) | `LAYERFS_TEST_POSTGRES_URL=… cargo +1.85.1 test --locked -p layerfs-persistence --features postgresql --test postgresql_doctor_physical` | Per-relation amcheck coverage; a missing extension is `Unknown` |

The `diagnostic_*.rs` tests beside them damage one table family each
(framing, dependencies, content, namespace, history, facts, inventory) and
check that the audit reaches it. Without `LAYERFS_TEST_POSTGRES_URL` the
PostgreSQL tests return without checking anything.

## Related

- [Store](README.md): the tables and invariants being audited.
- [SQLite](sqlite.md), [PostgreSQL](postgresql.md): the sessions the audits open.
- [Control: Doctor](../../control/doctor/README.md): targets, worker routing and how findings are reported.
