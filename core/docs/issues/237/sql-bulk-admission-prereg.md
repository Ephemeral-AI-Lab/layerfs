# #237: bounded SQLite physical-admission experiment

> **Status:** source and read-only plan analysis; no treatment source, timed arm,
> or speed claim yet. Research worktree `codex/issue237-sql-bulk-admission`
> starts at `970854f2c` (integrated `ImportBatch` product at `bc944fe63`).

## Fixed question and boundaries

Can Core's single C2 owner reduce SQLite object/pack admission work for the
10,000-file, 300,000,000-byte native Init, without increasing wave size, RSS,
or Store space? Keep packs inside SQLite BLOBs, 4,096-byte database pages,
128-KiB small-file cutoff, four existing Init constructors, one C2 owner,
bounded arbitration, in-place pack append, and exact canonical/root/readback
behavior. The earlier 80-to-7-COMMIT candidate missed its speed gate and grew
sampled RSS; its larger waves are not this treatment.

The v0.1.6 common-source release arm issued **639** multirow object INSERTs and
**590** multi-pack INSERTs for **1,693** pack rows. An earlier Core source made
**1,414** object INSERTs and **1,262** one-row pack creates. Those are
different source identities from the integrated `ImportBatch` baseline, whose
exact C2 counters are **pending**. The [side-by-side evidence](v016-core-sqlite-head2head.md)
does not license copying the earlier Core counts into this experiment.

## Read-only SQL plan probe

The [probe](evidence/sql-bulk-admission/explain_alternatives.py) compiled SQL
against the retained integrated Core Store whose SHA-256 is
`b9c609bd17514eafa427361e94643b6bc432c8fcdee5122ec198c087b5098bb1`.
SQLite was 3.51.0; the Store reported 4,096-byte pages. The script opened the
main database read-only, created only a connection-local TEMP scope row, and
used `EXPLAIN`/`EXPLAIN QUERY PLAN`; it executed no main-database write. Its
[raw plans](evidence/sql-bulk-admission/plans.json) are structural counts, not
wall-time measurements.

| Object INSERT shape | 1 row | 16 rows | 64 rows | 128 rows |
| --- | ---: | ---: | ---: | ---: |
| Current `VALUES`, one scalar scope subquery per row | 64 | 377 | 1,337 | 2,617 |
| `WITH incoming VALUES ... INSERT SELECT` and one scope join | 83 | 188 | 524 | 972 |

Cells are compiled VDBE opcode counts. Both shapes bind six values per row.
The one-scope shape uses one constant-row coroutine and one scan of the one-row
TEMP scope. Current SQL compiles one TEMP-scope scalar subquery per inserted
row. For a singleton the candidate compiles **more** opcodes; the actual
placement-width distribution decides whether it helps. Multirow `zeroblob`
pack creation compiles 50, 133, and 361 opcodes for 1, 4, and 16 rows;
in-place BLOB writes are outside EXPLAIN. No plan shows an unindexed hot
lookup, and no opcode count is a latency ratio.

## Source constraints and prospective measurements

`cas/placement.rs::place_groups` currently inserts locator rows after every
bounded group placement and validates cross-save collisions at the wave end.
`sqlite/write.rs::insert_objects` pages each placement at at most 128 rows;
pack creation is one `zeroblob` INSERT and one BLOB handle per created pack.
Every append writes only new body/directory/control spans. Deferring locator
INSERT across placements could fill larger SQL pages, but a same-save delta
base or repeated identity may need its locator before the wave ends. Any such
buffer must flush before those reads and before the wave's collision check and
COMMIT; it may hold IDs/locators only, never an extra payload copy. The existing
`wave_rows` vector already holds those row descriptors through the wave.

**First diagnostic, before treatment:** one exact integrated-source count-only
10k run, with once-per-Save `SaveOutcome` for file, prerequisite, and tree
Saves. Record inserted/reused rows, object INSERT calls, pack creates/appends,
COMMIT count/wall, SQL and named profile spans, root, Store geometry, and
source-payload residency. A second count-only build prints one
`LFS237_ADMISSION` line from `place_groups` after placement selection, with
locator rows, pack writes, creates and appends for that call. Keep the entire
stderr, report call-width histograms and maxima, archive the exact temporary
instrumentation diff, and restore product source before the treatment. The
public wall of either diagnostic is not a speed point. The root task owns the
host timing window.

**Treatment choice after that diagnostic:** first consider replacing the
repeated TEMP-scope scalar subqueries in the existing bounded multirow object
INSERT with a single-scope `INSERT SELECT` when actual placement widths justify
it. Multi-pack `zeroblob` INSERT is worthwhile only if actual placement calls
often create more than one pack. Wave-level locator deferral needs a complete
same-save read-barrier proof before implementation; an unsafe or mostly
single-row path will be reported as a rejected design, not forced into a
candidate. Freeze the exact treatment and source diff in an append-only section
here before either timed arm.

**Matched public pair, once a treatment is frozen:** one control from the
integrated baseline and one candidate, one sample each in that order, same
seed-1 manifest SHA-256
`c1d7937c9f90d3585558e6d20b35183a737530d386667d08badc3121a6b3d55e`.
Use `cold_diagnostic.py --case namespace-10000 --independent-source-copy
--fixed-operation-identity` in this worktree, two fresh Stores, one distinct
writable byte copy per arm, full source hash/eviction and final residency check
of **0/27,503 payload pages** immediately before each public call. Use the
fast performance-only lane (`verification=SKIPPED` during timing), preserve
both raw receipts and every failure, then separately reopen each Store for
complete 10,000-file/300-MB readback and exact root/object-ID comparison.
Record source/product/binary/harness seals, source-copy method, database page
size, Store apparent/allocated bytes, pack used/capacity/slack, Service CPU/RSS,
and C2 count/profile diagnostic at the same source identities. Metadata cache
residency remains unqualified under this driver, so a passing raw pair is
exploratory rather than fully cold release admission. The root task grants the
host window; no public sample starts before that grant.

## Placement-width diagnostic and treatment freeze, 2026-09-23

The first invocation used `/usr/bin/python3` (3.9.6) and failed on a runner
type annotation during import, before fixture copy, build or public operation.
The [failure record](evidence/sql-bulk-admission/python-invocation-failure.json)
is retained. The root task continued the same host window for the first actual
operation using repository `python3` (3.14.3) at the original fresh output
path. This was one count-only 10k run, with no sample retry.

Its [receipt](evidence/sql-bulk-admission/placement-width/receipt.json) reports
`INCOMPLETE` because Service/daemon telemetry was lost, `verification=SKIPPED`,
public wall 1.165772542 s, and complete performance command 2.117366625 s.
These times are **not speed samples**. Both Service and daemon cleanup passed;
the [final source recheck](evidence/sql-bulk-admission/placement-width/cold-recheck.json)
found **0/27,503** resident source payload pages. The source was an independent
writable byte copy, and the fixed public root is in the retained
[operation identity](evidence/sql-bulk-admission/placement-width/operation-identity.json).
The reporting-only product seal was
`5a76a5f8dbb6b4a927789180caed11e0ccbd1a9a3767db1650fd6b2627661494`;
its exact [diff](evidence/sql-bulk-admission/placement-count-diagnostic.diff.gz)
was restored after the run. It is not the treatment source.

The [parser](evidence/sql-bulk-admission/analyze_widths.py) derives the
following from the retained [raw Service stderr](evidence/sql-bulk-admission/placement-width/service.stderr)
and the closed Store's per-Save object and pack rows. Its
[machine-readable census](evidence/sql-bulk-admission/placement-width/placement-width.json)
can be reproduced while the ignored Store at the path in
[geometry](evidence/sql-bulk-admission/placement-width/store-geometry.json)
is retained. The Store SHA-256 is
`f7e0fca60f2bc8a0f633d1d7056b2125dcd3a4120772c53dbb1c466f4f6708c0`;
its page size is **4,096 B**.

| Save | `place_groups` calls | locator rows | Derived 128-row INSERT statements | Pack creates in traced calls | Pack appends |
| --- | ---: | ---: | ---: | ---: | ---: |
| File | **1,346** | **24,364** | **1,400** | 1,260 | 1,119 |
| Prerequisite | 2 | 11 | 2 | 2 | 0 |
| Tree | 2 | 308 | 4 | 1 | 1 |

The tree Save has two additional pool-lane pack rows outside `place_groups`;
all 1,260 file and two prerequisite pack creates agree with the Store's
per-Save pack-row census. The file Save's 1,346 calls have widths: **1,193**
at 9–16 rows, **115** at 1–8, **3** at 17–32, **10** at 65–128 and **25**
above 128; min 1, max 434. Exactly **18** file calls create two packs,
**1,224** create one, and **104** create none. Multi-pack SQL within the
current placement call could remove at most 18 file pack-INSERT statements,
so it is rejected for this treatment. The 1,400 file INSERT count belongs
only to this count-only run; another same-source diagnostic reported 1,393,
showing that scheduling can change placement boundaries.

**Frozen treatment:** change only `sqlite/write.rs::object_insert_sql` to
emit the read-only-probed single-scope `WITH incoming ... INSERT SELECT` SQL
shape, retaining the same six bound values per row, 128-row maximum chunk,
caller-supplied row order, one `temp.layerfs_read_scope` row, statement
counter, `affected == page.len()` check, same transaction and pack writes.
Update the SQL-length limit calculation to include the candidate's fixed
prefix and per-row text so lower SQLite limits cannot be overrun. Keep all
pack creation, BLOB append, group placement, wave and collision logic
unchanged. The candidate should reduce SQL VM work per ordinary locator
statement; it does **not** claim fewer object INSERT calls or COMMITs.

Run the focused external batching, same-save read, multiwriter and rollback
checks on this exact source before the pair. Build a clean integrated control
from `970854f2c` in its own worktree and the candidate in this one, each with
its own Cargo target. The public pair remains one sample per arm, control
first, with the H3 independent source copies, fixed operation identity and
0-resident-payload requirement above. Record exact C2 count/profile at each
arm through the same temporary reporting-only instrumentation, or mark it
`NOT_MEASURED`; never borrow the earlier diagnostic timers. No arm is run
until the root task grants the next host window.

## Terminal result, 2026-09-23

The root task granted one control-then-candidate host window. Both arms ran
once and returned the same root; no arm was repeated. The treatment missed
its locator-region and Store/RSS gates, despite a faster one-shot raw public
call. The temporary SaveOutcome reporting patch was restored in both
worktrees. See the [full rejected result and table](sql-bulk-admission-result.md)
and [pair manifest](evidence/sql-bulk-admission/pair/evidence-manifest.json).
