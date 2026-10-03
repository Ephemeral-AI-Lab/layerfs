# Init cause diagnostic: Phase 4.5 and current PostgreSQL/MinIO

The largest observed current cost is the bounded S3 upload window, followed by
CPU/encoding work and PostgreSQL registration. The registration function loops
once per object; the old SQLite path batches object inserts. Read plans and PG
worker queue delay do not explain the dominant gap in these two small fixtures.
No optimization was applied; these are one-shot cause diagnostics, not new SDK
speed samples or admission proofs. Historical strict Init speed FAIL remains.

## Frozen method and identities

Candidate instrumentation commit `fdfbee48fefe8a85916e28f3fd8f86b2f1e69eb1`, tree
`20da30a8325f7af5c864e0982f1e715be5f6a3ea`; clean product/harness identities in
each receipt. Phase 4.5 product remains unmodified at
`7edddbdb8e8512627aed0ed42533ef099d802384`. Only an external diagnostic example
composes its public Service import and returns the existing operation timer.
Candidate calls project init directly and separately reports engine opening.
This is an explicit component diagnostic scope difference: **do not compare
these rows as a matched SDK admission pair**. Baseline Store/history creation
is setup; candidate schema creation is setup. Candidate engine validation and
connection opening are included in its outer operation, and shown separately.

Declared order: 100 baseline, 100 candidate, 1000 baseline, 1000 candidate;
one child per case/arm; no unchanged-arm repeats. Release/locked binaries;
four Init constructors and four bounded uploads; authority `[0x41;16]`, seed
`[0x42;32]`, name `diagnostic-init`. Existing prepared fixture reuse; independent
fresh output and fresh owned PG/MinIO services each arm. Both arms use the same
VM/service limits (2 CPUs and 512 MiB per service, swap disabled). Whole-source
invalidation and residency check: **zero resident pages for all four rows**.
PostgreSQL diagnostic profile adds the shipped pg_stat_statements extension,
nested/planning observation and I/O/WAL timing. Durability settings remain on.
SQLite MEMORY journal and synchronous OFF are observed and unchanged. These
settings describe the experiment, not an excuse or relaxation of the gate.

Complete diagnostic child bound 15,000,000,000 ns; independent verifier bound
9,500,000,000 ns. Verification occurs after counter snapshots and never enters
operation comparisons. Input bytes are 5,000,000 / 20,000,000. Roots match per
size. Lite proof covers all paths/kinds and selected payload bytes, not all
payload bytes. No phase memory peak was measured; lifetime totals cannot fill it.

Raw append-only custody under `benchmark-results/fs-bench-pro/`:
`issue302-cause-campaign-fdfbee48f`, `issue302-cause-{100,1000}-{baseline,candidate}-fdfbee48f`,
`issue302-cause-plans-fdfbee48f`. Immutable executable archives and source,
observer, SQL, dependency/config, fixture and machine identities are in receipts.
Compressed original receipts/observer outputs/plans are retained in `checks/step10-cause-*`.

## Operation and command observations

Raw operands below are ns. Baseline means public Service import; candidate
means engine open plus project init. Candidate project-only timer is a nested
attribution, not a substitute gate scope. Process CPU includes all child threads;
its value may exceed wall on the four-thread Init path. Observer overhead is
included and is not calibrated out.

| Files / arm | Outer operation ns | Process CPU ns | Child launch-to-exit ns / 15 s | Separate verifier ns / 9.5 s | Paths; selected files / bytes | Functional / scratch / input cache |
| --- | ---: | ---: | ---: | ---: | --- | --- |
| 100-baseline | 39,345,500 | 45,553,000 | 1,883,805,917 | 519,008,125 | 102; 53 / 3,354,003 | PASS / PASS / PASS |
| 100-candidate | 198,558,750 | 69,688,000 | 809,593,667 | 742,005,542 | 102; 53 / 3,354,003 | PASS / PASS / PASS |
| 1000-baseline | 132,890,541 | 180,264,000 | 210,168,083 | 86,585,458 | 1011; 70 / 6,430,827 | PASS / PASS / PASS |
| 1000-candidate | 579,322,083 | 269,923,000 | 605,202,917 | 345,485,834 | 1011; 70 / 6,430,827 | PASS / PASS / PASS |

The first baseline command has 1,844,460,417 ns outside its operation timer
(`1,883,805,917 - 39,345,500`). Loader, lifecycle, observer-output and teardown
are not individually observed; do not attribute that residual to SQLite or
claim a faster current operation from the command wall. No resample was used.

## Small-step comparison

Rows are nested operation spans; siblings shown are disjoint within each timer.
The baseline residual contains untimed substeps such as genesis/allocation;
it is not an estimated counterpart for the candidate's explicit scopes.

| Step, ns | 100 Phase 4.5 | 100 current | 1000 Phase 4.5 | 1000 current |
| --- | ---: | ---: | ---: | ---: |
| history.import_scan | 816,417 | 972,791 | 4,694,666 | 5,581,750 |
| history.import_files | 26,527,208 | 114,663,292 | 96,682,375 | 430,059,958 |
| history.import_finish_save | 5,149,333 | 31,067,208 | 16,182,417 | 89,351,375 |
| history.prerequisites | 61,083 | 41,916 | 58,250 | 45,083 |
| history.finish_prerequisite_save | 724,917 | 4,673,209 | 631,834 | 4,440,500 |
| history.finish_tree_save | 1,207,500 | 4,936,958 | 3,737,084 | 11,200,083 |
| history.reserve_inodes | UNAVAILABLE: no separate old span | 2,601,667 | UNAVAILABLE: no separate old span | 2,908,417 |
| history.initialize_layerstack | UNAVAILABLE: no separate old span | 3,713,917 | UNAVAILABLE: no separate old span | 4,024,125 |
| Whole recorded import/init | 39,260,167 | 166,310,458 | 132,848,750 | 551,672,042 |

Current engine-opening components, outside its project timer but inside its
outer operation:

| Open step, ns | 100 current | 1000 current |
| --- | ---: | ---: |
| engine.metadata_open | 12,520,625 | 9,634,292 |
| engine.history_open | 18,144,708 | 16,049,208 |
| engine.s3_connect | 677,958 | 757,667 |
| engine.storage_policy | 887,500 | 1,194,625 |

Current C2 save stages below aggregate the three acknowledged saves. Spans
such as admission contain reservations, and registration_ready contains upload
and metadata registration. **Do not sum inclusive rows into a headline.**

| C2 inclusive stage, ns | 100 | 1000 |
| --- | ---: | ---: |
| begin | 2,912,460 | 2,359,625 |
| membership | 14,201,875 | 54,224,334 |
| admission | 24,960,667 | 90,998,749 |
| pack_flush | 684,917 | 2,554,334 |
| registration_ready | 106,309,834 | 365,997,292 |
| upload_window | 89,605,958 | 290,809,582 |
| metadata_register | 15,371,584 | 69,714,582 |
| pack_reserve | 5,336,294 | 10,906,459 |
| ordinal_reserve | 1,073,875 | 3,164,792 |
| finish_close | 1,872,667 | 2,199,834 |

The upload window is 53.88% / 52.71% of project-init wall
(`89,605,958 / 166,310,458`, `290,809,582 / 551,672,042`). It is directly observed
elapsed time at the bounded upload/drain call, not the sum of four overlapping
request clocks. The ring retains all three saves; no entries were omitted.

## Actual transactions, statements and SQLite VM work

SQLite operation-only observer scope excludes schema/setup/cleanup. PROFILE
counts executions, not every sqlite3_step (row iteration makes more step calls).
VM counters reset on each profile. No observer class omissions, fullscan steps,
sorts or reprepares in either operation. SQLITE VM steps are not CPU instructions
and PostgreSQL has no equivalent reported here.

| Counter | 100 Phase 4.5 | 1000 Phase 4.5 |
| --- | ---: | ---: |
| C2 statement executions | 685 | 3,532 |
| C5 statement executions | 12 | 12 |
| Total statement executions | 697 | 3,544 |
| C2 VM steps | 56,351 | 324,778 |
| C5 VM steps | 375 | 375 |
| Total VM steps | 56,726 | 325,153 |
| BEGIN / COMMIT / ROLLBACK | 20 / 19 / 1 | 28 / 27 / 1 |
| C2 object INSERT executions (batched) | 30 | 102 |
| Total sqlite3_step calls | 1,100 | 5,726 |
| Total interposed step wall ns | 16,445,000 | 58,151,000 |

Thus old SQL transaction starts are **20 / 28**, including one C2
rollback, not the previous source-derived estimate. SQLite C2 COMMIT step wall
is 12,039,000 / 41,991,000 ns across 17 / 25 commits. That is observed transaction
work with MEMORY/OFF unchanged; it is not evidence of fsync. Direct SQLite blob
API time, statement prepare time and per-worker distribution are not separately
captured. SQL profile nanoseconds are millisecond-quantized and not used as
precise elapsed attribution; exact interposed step/exec clocks are separate.

| Current PostgreSQL counter | 100 | 1000 |
| --- | ---: | ---: |
| C2 frontend statements | 24 | 41 |
| C5 frontend statements | 20 | 20 |
| Total frontend statements / Sync / ReadyForQuery | 44 | 61 |
| C2 implicit + C5 explicit transactions | 24 + 3 = 27 | 41 + 3 = 44 |
| Read / potentially writable transactions | 11 / 16 | 16 / 28 |
| Nested server statement executions | 828 | 5,057 |
| Nested object INSERT executions | 346 | 2,003 |
| Object INSERT server execution ns | 7,087,669 | 39,631,220 |
| Top-level server execution ns (inclusive functions) | 15,397,966 | 63,128,247 |
| Top-level planning ns | 3,818,251 | 4,721,836 |
| Total PG caller wall ns | 49,203,666 | 115,977,541 |
| Total PG worker queue ns | 232,457 | 376,793 |
| Total PG driver-future wall ns | 48,701,374 | 115,237,872 |
| Shared WAL syncs / sync time ns | 17 / 2,687,000 | 29 / 5,597,000 |

Frontend counts reconcile exactly to the product's physical wire counters and
pg_stat_statements top-level product entries. Each C2 query is an autocommit
transaction; C5 has one observed BEGIN-read/COMMIT and two BEGIN-write/COMMIT
pairs. This transaction total is derived from **observed issued statements plus
the provider transaction boundaries**, rather than a standalone transaction
observer. The database-wide xact_commit deltas are 31 / 48, including observation
sessions, and must not be mislabeled as product transactions. Both read and
write counts include engine opening. mutating_calls marks provider context,
not a count of INSERTs or of transactions.

Nested statement counts include PL/pgSQL expressions and FK-trigger probes;
they are not frontend round trips. Counts are real executions, but are not
cross-engine equivalent to SQLite VM steps. Nested execution times overlap the
parent function: never add them to the top-level total. No pg_stat_statements
deallocations occurred. Server stats are captured before the verifier.
Shared WAL/I/O deltas include observation/background activity, and may have
publication lag; they are descriptive, not an exact per-transaction breakdown.
WAL sync time is too small here to explain the principal gap. Driver minus
executor time includes protocol, parsing/planning, commits, scheduling and
handoff; it cannot be called pure network latency.

The two current sizes issue these C2 calls: policy 2/2, locate 5/10, signatures
1/1, value-group page 2/2, reserve 9/17, register 5/9. C5 retains 20 statements
for both sizes. Registration caller wall 15,274,583 / 69,296,584 ns; function
execution 11,763,250 / 57,765,373 ns. Its per-object INSERT loop is directly
observed: 346 / 2003 executions, about 20.5 / 19.8 microseconds each on average.
FK pack probes run 348 / 2023 times, plus signature FK probes 93 / 939 times.
Old SQLite uses 30 / 102 object INSERT statements for the same canonical
346 / 2003 objects. Fewer frontend calls do not mean less database work.

## S3 protocol observations

Counts are operation-only; four connections are established during engine open.
Every PUT received a 100-continue response. Request clocks aggregate concurrent
requests; **they overlap and are not elapsed operation wall**.

| S3 counter / aggregate request ns | 100 | 1000 |
| --- | ---: | ---: |
| PUTs | 27 | 95 |
| GETs | 5 | 18 |
| PUT body bytes | 5,009,646 | 20,038,708 |
| GET body bytes | 1,003,519 | 3,824,434 |
| PUT validation/hash ns | 15,128,247 | 60,505,627 |
| PUT signing ns | 546,295 | 1,609,914 |
| PUT header-write ns | 393,332 | 1,132,539 |
| PUT wait for 100-continue ns | 42,563,790 | 111,617,784 |
| PUT body-write ns | 1,562,415 | 5,380,454 |
| PUT final-response-head ns | 227,498,586 | 729,485,330 |
| GET response-head ns | 4,594,665 | 19,412,290 |
| GET response-body ns | 1,771,043 | 12,662,252 |

Final PUT response-head waiting averages 8.43 / 7.68 ms per request; continue
waiting averages 1.58 / 1.17 ms. Body-write time is much smaller. This supports
investigating per-request acknowledgement/protocol cost and number of small
packs first. It does **not** distinguish MinIO service CPU, storage, Docker VM,
network transit or client scheduling within response wait. No MinIO server-side
trace or disk-wait observer was captured. Skipping required acknowledgements,
adding workers, weakening durability or warming data is not an optimization.

## Post-proof plans, allocation and limits

EXPLAIN (ANALYZE, BUFFERS, WAL, TIMING OFF, FORMAT JSON) runs after proof, on a
warm result; it is a separate labeled plan diagnostic, not a speed resample.
The 256-hit/256-miss locate arrays are representative declared parameters, not
replays of captured production binds. Existing one-object INSERT and conflict
plans use an independent disposable copied schema, original indexes/checks and
FK, with rollback. That schema is removed; original canonical inventory remains
2003 objects / 20,187,652 bytes. Setup/plan/cleanup SQL and raw plans are retained.
No broad ANALYZE or index forcing alters the timed rows.

Locate hit/miss uses sequential object scanning plus hash join and sorting at
this small cardinality: 0.375 / 0.261 ms execution, zero shared block reads.
Signatures scans 939 rows and sorts in 0.152 ms. Value groups returns 20 rows in
0.080 ms with indexed/bitmap access. History meta uses its PK in 0.050 ms.
Schema definition takes 1.325 ms execution plus 1.522 ms planning. The isolated
object INSERT plan is 0.423 ms, including one FK trigger; the conflict plan is
0.119 ms. These warm single-plan observations are not average per-row costs or
estimates of savings. Real nested measurements above supply the loop cost.
SQLite EXPLAIN QUERY PLAN uses object primary-key and save integer-primary-key
lookups, plus the single-row temp scope. Operation fullscan/sort counters are zero.
Adding an index is not supported as the first intervention by these measurements.

Post-proof SQLite plan collection initially omitted its second bound parameter
(pack ceiling); that failure is retained. Corrected the diagnostic call and
completed SQLite plans; no performance child or successful PG plan was repeated.

| Allocated storage B | 100 Phase 4.5 | 100 current | 1000 Phase 4.5 | 1000 current |
| --- | ---: | ---: | ---: | ---: |
| Total | 7,372,800 | 5,808,128 | 23,101,440 | 21,725,184 |

Current allocation = MinIO regular-file blocks + original-owner C2 and C5
relation/index/TOAST/sequence allocation: 5,152,768 + 352,256 + 303,104 =
5,808,128 B; 20,570,112 + 851,968 + 303,104 = 21,725,184 B. PG database/WAL and
MinIO system directory are separately retained/excluded under the existing
accounting contract. There is no declared numeric Init storage ceiling, so
these observations cannot qualify admission. All four functional, residency,
child-budget, verifier-budget and scratch-cleanup checks pass. All admission
cases, step 10/11/M5 completion and direct history binding retain their prior
open/NOT_RUN disposition. The prior strictly-slower speed receipt remains FAIL.

## Revised optimization order, supported by these diagnostics

1. **S3 per-request acknowledgement path and pack count.** Largest measured
   elapsed span (89.6/290.8 ms). Next cause measurement should add correlated
   client/server request IDs and MinIO request handling/storage wait before
   choosing request coalescing or Expect handling. Any change must preserve
   bounded packing, default four uploads, acknowledgements and storage gates.
2. **Set-based PostgreSQL object registration.** Replace the per-object loop
   with bounded set operations only if lost-ID/order/conflict/dependency semantics
   are preserved. Actual loop cost 7.1/39.6 ms, within 11.8/57.8 ms registration
   function work. Test the observed mechanism before changing read indexes.
3. **Reduce unnecessary reservations and sequential PG calls.** 9/17 C2 reserve
   calls cost 6.4/14.0 ms caller wall; source overwrites unused pack reservations
   per wave. Preserve bounds, monotonic identities and atomicity. C5 reserve/
   genesis each crosses several statements; combine only within existing atomic
   semantics. These are worthwhile, but not the largest observed cost.
4. **Encoding, validation and reread work.** Admission spans 25.0/91.0 ms and
   GETs reread 1.0/3.8 MB. Upload validation hashes cost 15.1/60.5 ms aggregate.
   These overlap and cannot be added. Add count/byte attribution before changing
   read/encode paths; no data warming or window enlargement.
5. **Startup validation/connection reuse.** Engine open 32.2/27.6 ms, including
   history schema inspection. Optimize lifecycle/reuse only where the real SDK
   contract permits it; never move required timed opening into setup to pass.

Ranks are intervention hypotheses, not promised additive savings. No optimization
was applied during this measurement campaign.

## Checks and source size

Bounded instrumentation covering checks: metadata 37 PASS; repaired
storage/project/S3 278 PASS; all-target core Clippy/fmt PASS; boundary 473 files
PASS; tools 21 PASS; diagnostic harness 3 PASS; substrate 7 PASS. Initial compile,
probe recursion, private API and fixture-collision failures are retained in the
instrumentation checks/progress. Release builds matched the frozen code. No CI
or retired aggregate gate ran. Owned services are removed after evidence capture.

Instrumentation commit fdfbee48f: **Production LOC: 143238 -> 143542 (delta +304)**,
exact snapshots with tools/production_loc.py, product Rust/shipped SQL only.
Reference 65417 unchanged; core 77821 -> 78125; old path 6025 unchanged;
new path 7448 -> 7752; rest core 64348 unchanged. Growth is bounded telemetry.
The report/evidence commit records its own exact unchanged production count.
