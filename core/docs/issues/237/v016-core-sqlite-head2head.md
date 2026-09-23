# #237: v0.1.6 versus Core SQLite on one 10k source

> **Status:** Read-only, source-aware comparison of the one-shot
> [common-source research protocol](v016-v017-common-source-prereg.md).
> This is a diagnostic, not a release-admitted cold speed pair. No product code,
> SQLite page size, pack location or public sample was changed for this analysis.

## Identities and limits

Both products imported independent writable byte copies of the same closed
10,000-file / 300,000,000-B master (manifest SHA-256
`c1d7937c9f90d3585558e6d20b35183a737530d386667d08badc3121a6b3d55e`).
The immediate pre-call checks found zero resident source **payload** pages in
both arms; directory/inode metadata residency was unqualified. The reference
used the peeled `v0.1.6` release `44cf74848` and a benchmark-only diagnostic
binary ([identity](evidence/v016-core-sqlite/v016/identity.json)); Core used
the integrated C1/C2 source at `7f2124ba0` with temporary reporting-only
edits, product seal `d9b17e338d61a33a8f907d07e2758b7ac83c9e246212dbef200b59f5a0472d74`
([receipt](evidence/v016-core-sqlite/core/receipt.json)). The old public call
was **750.625833 ms**; the Core caller was **1,380.218125 ms**, a raw difference
of **629.592292 ms**. The APIs, canonical formats and Store ownership models
still differ. Reference status is `DIAGNOSTIC`; Core is `INCOMPLETE` because
the daemon telemetry lost an event. Both public performance arms skipped full
verification. Separate post-run full readbacks **PASS** against the source
manifest ([reference](evidence/v016-core-sqlite/v016/readback-corrected-pass.json),
[Core](evidence/v016-core-sqlite/core/readback-separate-pass.json)); the
reference's earlier external-ID parse-only readback attempt is retained as a
[failed attempt](evidence/v016-core-sqlite/v016/readback-initial-failed.json).
Neither performance row is a fully cold, admissible speed claim.

I ran [the read-only probe](evidence/v016-core-sqlite/explain_compare.py) once
after both operations closed their databases. It opens each file with
`mode=ro`, asserts **4,096-B pages** and the expected application ID, hashes
the file, records table/index counts and `dbstat` B-tree cells/bytes, then
compiles actual version-specific hot SQL with `EXPLAIN QUERY PLAN` and VDBE
`EXPLAIN`. INSERT, UPDATE and COMMIT are compiled, **never executed**.
The [raw plans](evidence/v016-core-sqlite/plans.json) pin all SQL text, bind
counts, plans, opcode histograms and three database SHA-256 values. Both release
binaries link `/usr/lib/libsqlite3.dylib`; `/usr/bin/python3` uses the same
SQLite **3.51.0** ([linkage](evidence/v016-core-sqlite/binary-linkage.txt)).
Default `python3` elsewhere on this host uses 3.51.2 and was not used for
EXPLAIN. The [evidence manifest](evidence/v016-core-sqlite/manifest.json)
hashes copied raw receipts, trace logs, geometry and the probe.

| Closed database | SHA-256 prefix | 4-KiB pages | Apparent B | Allocated B (`st_blocks × 512`) |
| --- | --- | ---: | ---: | ---: |
| v0.1.6 combined Store | `79c8b732d645` | 74,354 | 304,553,984 | 318,783,488 |
| Core C2 content Store | `726497c0748b` | 81,458 | 333,651,968 | 335,609,856 |
| Core C5 History catalog | `9a908de0b3b` | 21 | 86,016 | 86,016 |
| Core **combined** DB footprint | — | 81,479 | **333,737,984** | **335,695,872** |

The Core combined apparent footprint is **29,184,000 B (9.58%)** larger.
Both have exactly **24,683 object rows**. v0.1.6 has **1,693** pack rows and
**301,646,854 B** of pack BLOB data. Core has **1,262** pack rows with
**330,825,728 B reserved BLOB capacity**, of which **305,969,540 B** is
declared used and **24,856,188 B** is spare
([Core geometry](evidence/v016-core-sqlite/core/store-geometry.json)).
The capacity difference is **29,178,874 B**: 4,322,686 B more used data and
24,856,188 B spare. It accounts for almost all of the apparent file-size
difference, but these two formats do not encode identical pack bytes. Allocated
bytes are the host filesystem's post-run block accounting, not distinct SQLite
payload or measured device writes.

## SQL shape on the actual closed Stores

The v0.1.6 source is
[`objects/read.rs`](../../../../crates/layerfs-layerstack-store/src/objects/read.rs),
[`objects/admission.rs`](../../../../crates/layerfs-layerstack-store/src/objects/admission.rs),
[`objects.rs`](../../../../crates/layerfs-layerstack-store/src/objects.rs), and
[schema v10](../../../../crates/layerfs-layerstack-store/sql/schema/v10.sql).
Core uses [lookup](../../../crates/layerfs-storage/src/sqlite/lookup.rs),
[write](../../../crates/layerfs-storage/src/sqlite/write.rs),
[schema](../../../crates/layerfs-storage/sql/schema.sql), and the separate
[History catalog](../../../crates/layerfs-history/src/sqlite/layerstack.rs).

| Comparable operation | v0.1.6 plan and compiled VDBE size | Core plan and compiled VDBE size | Interpretation |
| --- | --- | --- | --- |
| One object-ID lookup | `objects` primary-key seek; **15** opcodes. | `objects` primary-key seek, `saves` rowid seek, one-row TEMP scope scan; **42** opcodes. | No content-table scan or missing hot index. Core checks Save publication/visibility in the query. |
| 128-ID lookup | Same primary-key seek per ID; **407** opcodes. | Same object seek plus Save/scope checks; **433** opcodes. | Both build an ephemeral IN set. The number of calls and returned rows matters more than these static sizes. |
| 128-row object INSERT | One `WITHOUT ROWID` object primary-key write; **829** opcodes. | One `(object_id,save_id)` primary-key write; **2,617** opcodes and **128 scalar TEMP-scope subqueries** compiled from the VALUES list. | Core binds Save ownership once per row in SQL text. It also stores role and Save ID. Opcode counts are not elapsed time. |
| One pack INSERT | Pack rowid write with assembled BLOB bound; **18** opcodes. | Pack rowid write plus `packs_save` index, `zeroblob(capacity)`, scope lookup; **50** opcodes, followed by incremental BLOB API writes outside EXPLAIN. | v0.1.6 can insert multiple assembled packs in one statement (≤1 MiB); Core creates one reserved row per pack. |
| Append to pack | Rowid-seeking `UPDATE object_packs SET data=?2`; **20** opcodes, then the full BLOB is replaced. | Rowid ownership seek plus Save/scope checks; **39** opcodes, then incremental BLOB writes outside EXPLAIN. | Core's changed-span write avoids v0.1.6's whole-pack rewrite. Comparing just the plans would miss the dominant byte behavior. |
| Pack read | Rowid seek; **10** opcodes. | Pack and Save rowid seeks plus one-row scope scan; **31** opcodes. | No pack-table scan. |
| C5 genesis publication | Stack ID and name use primary-key/unique-name indexes. Layer INSERT compiles **119** opcodes in the combined Store. | Same indexed stack searches. Layer INSERT compiles **180** opcodes in separate History DB; all its shown FK checks use indexes. | Both create one stack and one genesis layer. C5 is a separate Core database, so a content-Store-only size comparison would be wrong. |
| `COMMIT` | **4** opcodes. | **4** opcodes. | EXPLAIN cannot reveal dirty-page flush, cache misses/spills or COMMIT wall time. |

The old 128-row INSERT's EQP also lists scans of `workspace_stages`, `layers`
and `commits` while compiling its foreign-key behavior. Those tables have
**0, 1 and 0 rows** in this closed 10k Store. Core's repeated TEMP scope is
one row. Neither is a large content scan; no index addition follows from
these plans. v0.1.6's `objects` key is `object_id`; Core's is
`(object_id,save_id)`. Core also maintains `packs_save` for **1,262** pack rows
and `signatures_save` for **8,192** ring entries. The exact indexes and
physical B-tree cell/byte counts are in the raw plans; the larger Core content
table is not evidence of an object lookup scan.

## Actual call counts and pager limits

These are **operation counters/traces**, not derived from EXPLAIN. The
[reference trace](evidence/v016-core-sqlite/v016/stderr.txt) counts every
SQLite statement in its public call; Core's
[SaveOutcome log](evidence/v016-core-sqlite/core/service.stderr) counts each C2
Save. Their coverage differs, so stage labels stay visible.

| Work | v0.1.6 one public call | Core one public call |
| --- | ---: | ---: |
| Object rows inserted | 24,683 | 24,683 (24,364 file, 11 prerequisite, 308 tree) |
| Object-row INSERT statements | **639**, exact SQL trace | **1,414** C2 `SaveOutcome` calls (1,408 + 2 + 4); **2.21×** old |
| Pack INSERT | **590** multi-row SQL statements for 1,693 final pack rows | **1,262** one-row creates; one SQL statement per row |
| Pack append | **408** whole-BLOB `UPDATE` statements | **1,237** changed-span BLOB appends (1,037 file + 200 tree), each with a source-required ownership SELECT |
| Object SELECT | **438** traced statements; the fresh-ID filter skipped **24,683** absent IDs and queried only **42** candidate IDs | At least **24,683** per-new-row collision lookups are source-required across the three Saves, plus wave lookups; file-Save collision queries cost **43.921 ms** |
| Transaction counts | **75 BEGIN / 75 COMMIT** traced, of which diagnostic cohort counters attribute 73 to pipeline and one to publication; one reserve transaction is additional | **107 total**: 80 file + 4 prerequisite + 21 tree = **105** exact C2 commits, plus **2 C5 catalog write commits derived from source**, not traced here |
| Measured COMMIT bucket | **223.015 ms** for the 74 diagnostic cohort commits (additional reserve excluded) | **223.730 ms file**, **226.570 ms** across all three C2 Saves; separate C5 commits unmeasured here |
| Measured disjoint C2 SQL bucket | Not available from this reference diagnostic | **138.887 ms file**, **144.361 ms** across three C2 Saves |

v0.1.6's target connection was configured for a **32 MiB** cache with
`cache_spill=OFF` ([profile](../../../../crates/layerfs-layerstack-store/src/schema.rs),
[receipt](evidence/v016-core-sqlite/v016/receipt.json)). Core's current
[connection profile](../../../crates/layerfs-storage/src/sqlite/connection.rs)
does not set either knob. The fresh Core timed owner's cache/spill counters
were **not measured**. The `cache_size=2000` and `cache_spill=20000` in the
plan JSON are from **new read-only connections**, not either operation's live
owner. An earlier distinct Core [pager diagnostic](pager-10k.md) saw zero
spills; it cannot be assigned to this arm. Every closed DB in this comparison
reports 4 KiB pages.

## Decision from this comparison

Core's current full C2 COMMIT bucket is within about **3.6 ms** of the
reference's tracked cohort bucket despite **30 more** reported C2 commits
(and an unpriced old reserve). The public gap is **629.6 ms**. Transaction
count alone therefore does not explain the observed gap. The separate
[80-to-7-commit treatment](bounded-wave-experiment.md) was slower, which is
consistent with this finding; it is a different identity and not part of the
head-to-head timing. Large producer/consumer spans overlap, so the remainder
cannot be assigned to one SQL statement class by subtraction.

The actionable difference is **bulk physical admission while packs remain
SQLite BLOBs**: v0.1.6 grouped many packs and sorted locator rows into fewer
SQL statements, whereas Core's current Save still makes more row inserts and
pack API calls. Preserve Core's incremental BLOB append and 4-KiB pages; test
a bounded multi-pack/multi-locator publication design with the same source
and exact per-stage call counters. The extra per-row Core collision query and
TEMP-scope scalar subqueries are concrete smaller targets, bounded by the
measured **43.9-ms collision** and **65.5-ms object INSERT** file-Save spans
before overlap and new work. Plans alone justify no speed estimate.
