# #237: SQLite plan audit of the 10k file Save

> **Status:** Research; informative and not a product contract. Read-only plan
> inspection, 2026-09-23. No product edit or new timed operation sample.

## Method and identity

The retained [D11 Store](../../../../docs/roadmap/0.1/0.1.7/evidence/issue237-native-init-research/raw/d11-ingest-counts/manifest.json)
is the completed 10k native Init Store at
`benchmark-results/fs-bench-pro/issue237-d11-ingest-counts-dirty-a/daemon-host/init_namespace/namespace-10000/store.sqlite`.
Its SHA-256 is
`bbfd74023c498d72ed7f42e7aa249d3951adb0f7e34f5e3075357b5aad1834aa`,
file size **333,656,064 B**, SQLite application ID `1279677261`, schema version
10, **4,096-B pages**, and 81,459 pages. It contains 24,683 object rows, 1,262
packs, three Saves, and 8,192 content-signature hints. File Save 1 contributed
**24,364 object rows**, **1,257 packs**, and **7,696 physical groups**. Those
groups average 3.166 rows each; the Store has no `sqlite_stat1` table. The
schema and indexes are retained verbatim in [plans.json](evidence/sqlite-explain/plans.json)
and defined in [schema.sql](../../../crates/layerfs-storage/sql/schema.sql).

I opened the main database with SQLite `mode=ro`, reproduced the product's
single-row `TEMP layerfs_read_scope` as `(save_id=1, publication=0)`, and only
ran `SELECT`, `PRAGMA` reads, `EXPLAIN QUERY PLAN`, and `EXPLAIN`. The script
checks that the Store file size and modification time remain unchanged:
[explain.py](evidence/sqlite-explain/explain.py). Reproduce with
`/usr/bin/python3 core/docs/issues/237/evidence/sqlite-explain/explain.py <retained-store.sqlite> > plans.json`.
The diagnostic compiles INSERT, UPDATE and COMMIT statements with real row
parameters but **never executes** them. It does not time SQL.

This is the product's SQLite engine version: `libsqlite3-sys 0.38.2` links the
system `/usr/lib/libsqlite3.dylib` in the release Service (`otool -L`), and
`pkg-config`, `/usr/bin/sqlite3`, and `/usr/bin/python3` all report SQLite
**3.51.0**. Homebrew Python reports 3.51.2 and was not used. The plans are
still for a **closed, published** Store: the live Save's page residency,
transaction locks, active-slot values, and runtime hit/miss counts are not
recreated. The source D11 performance receipt remains a dirty,
verification-skipped diagnostic with `source-cache-uncontrolled-v1`; this plan
audit cannot promote it to a cold admission result.

## Exact plan shapes

The [JSON evidence](evidence/sqlite-explain/plans.json) retains the exact SQL,
parameter count, every `EXPLAIN QUERY PLAN` row, and VDBE opcode histograms.
The source statements are in [lookup.rs](../../../crates/layerfs-storage/src/sqlite/lookup.rs),
[write.rs](../../../crates/layerfs-storage/src/sqlite/write.rs), and
[ownership.rs](../../../crates/layerfs-storage/src/sqlite/ownership.rs).

| Product statement and tested parameter shape | Plan on D11 Store | Consequence |
| --- | --- | --- |
| `lookup::candidates`, one ID plus pack ceiling | `SEARCH o USING PRIMARY KEY (object_id=?)`; `SEARCH s USING INTEGER PRIMARY KEY`; one-row `SCAN r` of TEMP scope. | Collision validation does one bounded indexed seek per new row. `locations` and `present` use this same SQL, so they have the same physical plan. |
| `lookup::candidates`, 128 IDs plus ceiling | Same two primary-key searches and TEMP scan; SQLite builds an ephemeral 128-ID IN set (128 `IdxInsert` opcodes). | No scan of the 24,683-row `objects` table. The product pages identifiers at 128; one-ID collision probes have no IN-set build. |
| `write::insert_objects`, one row / 128 rows | One `objects` primary-key write. The 128-row shape is `SCAN 128 CONSTANT ROWS` and compiles **128 scalar reads** of the one-row TEMP scope, 2,617 VDBE opcodes; one-row shape has 64 opcodes. | The source emits a scope subquery for every VALUES row. This is a possible small SQL-text/VM simplification, but D11 issued 7,750 statements for 7,696 groups: most statements are small. D12 measured only 71.157 ms for all object INSERT calls. |
| Existing-pack ownership check in `append_pack` | Pack rowid seek, TEMP scope scan, correlated `saves` rowid seek. | It runs for 6,463 D12 appends before `sqlite3_blob_open`; there is no table scan. The BLOB handle and three in-place writes are API calls outside the SQL planner. |
| New pack INSERT / pack read | New pack uses an indexed rowid write plus a TEMP scope read. Pack read seeks pack and save by rowid, then scans the one-row TEMP scope. | The 1,259 pack creations in D12 allocate reserved BLOB capacity, and read lookup has no missing index. |
| `advance_pack` / `saves.pack_ceiling` UPDATE | Both seek a single row by integer primary key. | No index treatment is justified. The pack watermark is advanced per bounded transaction; the save ceiling is updated on a new pack. |
| `COMMIT` | Four VDBE opcodes, including `AutoCommit`. | `EXPLAIN` cannot show pager writes, page eviction, filesystem I/O, or their time. |

The schema already stores `objects` as `WITHOUT ROWID` with primary key
`(object_id, save_id)`, so membership by ID has a covering locator row without
an extra secondary index. The remaining explicit indexes, `packs_save` and
`signatures_save`, serve other ownership/cleanup uses. Reintroducing an
`objects_save` index would add a B-tree write per object and would not improve
the hot ID query. `ANALYZE` is likewise unsupported by this evidence: the
existing key seeks are already selected, and ANALYZE would be additional timed
work if the operation required it.

## Where the time is, and what the plans cannot prove

The separate [D12 count-driven diagnostic](c2-detail-diagnostic.md) measured
**771.409 ms** in 24,562 single-owner accept calls. Its whole file Save spent
**188.689 ms in disjoint SQL** and **227.521 ms in COMMIT/BEGIN** across 80
commits. Nested spans include 123.805 ms in pack-write calls, 71.157 ms in
object INSERT calls, and 43.306 ms in the collision candidate query; do not
add those nested spans to the SQL total. The 43.306-ms query total is a direct
ceiling on removing its current local cost, not a prediction of caller-wall
savings. D11 and D12 used different instrumented source identities and are
not a matched speed pair. `EXPLAIN` returns access paths, **not** elapsed time
or a causal decomposition of the Service's concurrent producer/owner pipeline.

The largest single D12 outlier outside accept is connection release at
350.786 ms. D11's **entire finish call** was 54.223 ms, so this variation is
not a stable query-plan cost. Deferring close past the public timer would move
work out of the measured operation and is not a speed treatment.

### Page-cache policy difference

The v0.1.6 profile explicitly sets `cache_size=-32768` (32 MiB) and
`cache_spill=OFF` ([reference profile](../../../../crates/layerfs-layerstack-store/src/schema.rs)).
Core sets MEMORY journal, synchronous OFF, memory temporary storage, foreign
keys ON, and a zero busy timeout, but leaves page-cache size and spill at the
engine defaults ([Core profile](../../../crates/layerfs-storage/src/sqlite/connection.rs)).
On this system SQLite 3.51.0, both a new read-only connection to D11 and a
fresh in-memory connection after Core's configured pragmas reported
`cache_size=2000` pages (**7.8125 MiB**) and `cache_spill=20000` pages
(**78.125 MiB** at 4 KiB/page). These are **not** a readout of D11/D12's
timed Save owner: neither receipt printed
[`SaveOperation::connection_profile`](../../../crates/layerfs-storage/src/cas/store.rs),
and no per-connection spill/hit/miss counter was retained.

SQLite says a dirty spill requires the cache's page count to exceed **both**
the configured cache size and the `cache_spill` threshold
([official pragma reference](https://www.sqlite.org/pragma.html#pragma_cache_spill)).
The ordinary D12 preparation wave is bounded by 512 objects and just under
4 MiB of canonical bytes; D11 averaged roughly 16 new 256-KiB packs per
79 file-Save commits. Those facts make an 80-MiB spill threshold look unlikely
to be crossed in a typical wave, but they do not prove the maximum dirty-page
count. BLOB reservation, indexes and other live pages also consume cache.
A larger same-operation cache can be an honest **product-policy experiment**
with cold source pages and reported RSS, but the current plans and receipts do
not show that it would help. It is distinct from warming pages in setup.

## Ranked decision

1. **First, count pager behavior on the actual Save connection.** In one
   labelled 10k diagnostic, read `sqlite3_db_status` cache spill, write,
   hit/miss, and high-water cache-used counters before and after each wave,
   plus pack/row/COMMIT counts. Do not add telemetry per object. If actual
   spills occur, or recurrent cache misses and page rewrites correlate with
   the 227.5-ms transaction bucket, preregister a bounded 32-MiB page-cache /
   spill-OFF product-policy arm against an unchanged cold-source control.
   Keep 4-KiB pages and report peak process/page-cache memory and Store bytes.
   If spills are zero and misses insignificant, reject this treatment.
2. **Then, if SQL itself is material, test one statement simplification.**
   Bind the already owned Save ID once into the multi-row object INSERT instead
   of compiling one TEMP-scope scalar subquery per row; retain the same ownership
   and constraint checks. A separate bounded count diagnostic should establish
   actual INSERT widths and prepare/step cost before a timed arm. The full
   D12 INSERT span is only 71 ms, so this cannot supply a several-hundred-ms
   improvement alone. Removing `append_pack`'s ownership query or retaining
   BLOB handles is more semantically delicate and is bounded above by the
   123.805-ms whole pack-write span.
3. **Do not add an index or alter the 4-KiB page size.** Existing plans use
   primary keys and there is no hot table scan. The Store's ~24k object graph,
   7.7k physical groups, 1.26k packs, 300-MB payload write and 79–80
   transaction acknowledgements are structural work. Reaching the 10k
   0.578-s target needs measured reduction of the public operation's critical
   path; an EXPLAIN-only query tweak is not a credible 0.3-s C2 saving.
