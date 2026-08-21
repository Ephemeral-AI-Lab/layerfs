/goal Build a read-only analytical model of LayerFS SQLite row, B-tree,
overflow-page, byte-fixed cache, and write-granularity behavior under 4-KiB,
8-KiB, and 16-KiB page profiles, and decide whether/when a later serial
page-size experiment is justified.

## Scope and sole write authority

Work only in `/Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty`. WP4-M is active
and incomplete. Do not message, interrupt, steer, wait on, inspect partial
artifacts from, or infer a profile winner from it.

You may create exactly:

`/Users/yifanxu/Ephemeral-AI-Lab/layerfs-empty/research/phase-4/while-waiting-phase-4-to-finish/task-f-sqlite-page-overflow/report.md`

If the assigned `report.md` already exists, stop and report the collision; do
not overwrite it.

Everything else is read-only. Do not edit this prompt or other research. Do
not run `sqlite3`, open a database from any language, use `dbstat` or
`sqlite3_analyzer`, parse a database file, run Cargo/tests/builds/benchmarks,
query filesystem allocation, flush/warm caches, instrument VFS/syscalls, or
write under `target/`. Use symbolic arithmetic and already published aggregate
counters only.

If measured WP4-M release rows have started, do not run local shell commands or
write the report until the measured campaign is quiet or complete; web research
and reasoning may continue. Use
`git show d781173a08ab4092eb539c3a0870056e6c6a77ff:<path>` for dirty tracked
files and the recorded sealed F2 source for accepted behavior.

## Non-authority

This task does not authorize `PRAGMA page_size`, database creation/migration,
schema/cache/spill changes, WAL, VFS, `WITHOUT ROWID`, compression, a carrier,
durability change, profile promotion, or a benchmark. Proposed 36-byte
canonical-v2 references and all existing research recommendations remain
hypotheses.

## Read first

1. `research/phase-4/index.md`
2. `research/phase-4/decision-map.md`
3. `research/phase-4/foundations/benchmark-and-evidence.md`
4. `research/phase-4/foundations/invariant-matrix.md`
5. `research/phase-4/foundations/hypothesis-ledger.md`
6. `research/phase-4/storage/sqlite/durability-and-layout.md`
7. `research/phase-4/storage/compression-and-packing.md`
8. `research/phase-4/core/pipeline/full-create-pipeline.md`
9. `research/phase-4/core/canonical/identity-and-hashing.md`
10. `research/phase-4/core/canonical/v2-single-identity.md`
11. `implementation-detail/phase-4/storage/sqlite/spec.md`
12. `implementation-detail/phase-4/storage/sqlite/visible-head.md`
13. `implementation-detail/phase-4/algorithm/spec.md`
14. `implementation-detail/phase-4/algorithm/complexity-analysis.md`
15. `implementation-detail/phase-4/algorithm/tests-and-benchmarks.md`
16. `implementation-detail/phase-4/wp4m/f-series/f1.md`
17. `implementation-detail/phase-4/wp4m/f-series/f2/report.md`
18. `implementation-detail/phase-4/wp4m/f-series/f4/report.md`
19. committed/sealed schema and codec source, using the frozen F2 custody source
    rather than live dirty benchmark code for accepted behavior.

## External research

Use official SQLite documentation/source only for SQLite technical facts:

- database file, B-tree, record, and overflow formats;
- `page_size`, `cache_size`, and `cache_spill`;
- `sqlite3_db_status` and statement-status APIs;
- `dbstat`/`sqlite3_analyzer` only as later measurement tools;
- `WITHOUT ROWID` only if analyzing the duplicate index;
- SQLite's internal-versus-external BLOB study only as a prior.

Prefer version-appropriate SQLite 3.51.0 facts when behavior differs. Do not
use tutorials, blogs, Q&A, aggregators, or other applications' speed.

## Required analysis

### 1. Current schema/profile

Document rowid versus `WITHOUT ROWID`, BLOB primary-key/unique-index behavior,
row columns and provable runtime storage classes, canonical BLOB placement,
metadata/head tables, `FULL + DELETE`, temp/mmap profile, page-size selection,
profile authority, and migration/fresh-database implications.

### 2. Exact formulas

Define page size, reserved/usable bytes `U`, record payload `P`, local-payload
thresholds, table-leaf rules, overflow payload `U-4`, exact overflow-page
rounding, cell/pointer/record header overhead, and separate ordinary table and
unique-index behavior. Cite official formulas. Keep reserved bytes,
distribution, serial widths, and fan-out symbolic where unavailable.

### 3. Scenario tables

For 4096/8192/16384-byte pages model:

- a representative object near the observed average, labeled as representative;
- current 68-byte references and hypothetical 36-byte references;
- K64/F64, K59/F101, K256/F256 as parameters, never winners;
- usable/local/overflow bytes and pages where supportable;
- mapping references/cells per page;
- table and unique-index depth direction;
- page-cache entries under one byte-fixed budget;
- dirty-page/VFS-call direction;
- edit/range amplification and storage direction.

Do not compute total overflow pages from the average as if nonlinear rounding
were linear:

```text
overflow_pages(average(P_i)) != average(overflow_pages(P_i))
```

unless the actual distribution is available in already published evidence.

### 4. Reconcile sealed observations

Use published values such as 5,372 objects, 105,291,554 canonical bytes,
4096-byte pages, 26,677 final pages, 26,676 dirty writes, about 6,675 spills,
109,268,992 apparent DB bytes, and about 19,600.8 average canonical bytes.
Recompute simple equations, then explicitly list unavailable distribution,
resident-dirty, journal/temp peak, APFS, and physical-media facts.

Never infer physical media from page counts, VFS requests, wall, logical
length, or allocation.

### 5. Cache fairness and workload risks

Hold cache bytes, not page count, fixed across page sizes. Explain why fewer
spills may move writes into COMMIT, why larger pages can amplify small edits
and ranges, why cache-used snapshots are not true high-water, and which status
counters can/cannot test the model. Page size and cache policy must remain
separate experiments.

### 6. Dependency and target contribution

Decide whether execution belongs after WP4-M, WP4-P, canonical-v2 decision, or
a new object-size-distribution observation. Quantify whether reference-width/KF
uncertainty materially affects the model relative to roughly 105 MiB of chunk
objects.

Use accepted F2 `659.593 ms` as performance authority and F4 only for
attribution. Bound gross SQLite opportunity without adding nested or
independent medians, and report possible contribution toward 500, 400,
333.333, and conditional 250-ms levels.

## Evidence rules

Label every material claim `Observed(source/API)`, `Derived(equation)`,
`Hypothesis`, or `Unavailable(reason/source)`. Every variable needs units and
provenance. Never mix observed and hypothetical operands silently.

## Required report

Write only the assigned `report.md`, containing:

1. terminal disposition and earliest permissible execution point;
2. custody/evidence hierarchy;
3. current schema and physical profile;
4. exact SQLite payload/overflow equations;
5. input table with provenance/units;
6. separate 4K/8K/16K scenarios;
7. separate v1/v2 and K/F parameter scenarios;
8. table B-tree versus unique-index analysis;
9. byte-fixed cache/spill model;
10. reconciliation with sealed observations;
11. edit/scrub/reconstruction/range risks;
12. storage/durability implications and unavailable facts;
13. target contribution bounds;
14. prospective serial experiment handoff or exact missing-data/stop rule;
15. primary/local citations.

If advanced, the handoff must specify 4K control versus 8K first, 16K only
after a prospective reason, fresh DB before schema, one page-size variable,
byte-fixed cache, exact semantic equality, `FULL + DELETE`, one transaction and
COMMIT, supported pager/VFS/storage observations, protected operations,
balanced serial release pairs after WP4-M, independent recomputation, and
retain/revise/revert gates. Do not fabricate a campaign for a deferred result.

End with exactly one disposition:

- `READY_FOR_LATER_SERIAL_PAGE_EXPERIMENT`
- `NEEDS_OBJECT_SIZE_DISTRIBUTION`
- `DEFER_UNTIL_CORE_PROFILE_FREEZE`
- `NO_GO_PAGE_SIZE`

## Completion

Complete only when official formulas, units, average nonlinearity, table/index
separation, byte-fixed cache, v1/v2/KF separation, dependency order, and one
disposition are present; no physical-I/O inference or partial WP4-M result is
used; local links resolve; no other file changed; and the final response gives
the report path, disposition, earliest execution point, blocker/causal model,
and SHA-256.
