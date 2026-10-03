# #302 step 10 Init v2 diagnostic pair — 2026-10-04

> **Status:** Current benchmark report; strict speed condition FAIL.
> Full step 10/11 and M5 are incomplete. No release or retirement claim.

The corrected direct-engine 100-file case is 6.468038 times its matched raw SDK
baseline time. Both independently verify the namespace and declared content
sample, but the strict owner speed condition fails. The smaller allocation
cannot override that failure. Init has no numeric storage ceiling. Remaining
cases are NOT_RUN and the retained-history direct-port driver is still unbound.
No product optimization is claimed by this harness boundary correction.

## Frozen identity and scope

Source/harness commit `c9165d4dac76756c4116c48ec1cacd46e7fb4a6e`, tree
`6802ca4de9ae5b1c997f711ab08500b07dd4983d`; baseline measured product commit
`7edddbdb8e8512627aed0ed42533ef099d802384` and tree
`9a8a5686fcebeb4c444dfc22429df23e7a46843c`. Both source trees were clean.
Each arm's product/SQL/compilation/dependency seals, binary SHA256 and root Cargo
config hash are in its immutable receipt. Harness seal is
`5892128ff393667cfa88f8ab4f689c070604ca9d0deb7a3c603deefa8c351826`. Fixture SHA256 is
`9b97114260decac04edb93b288b2c1b6037b2d9d7f4127c21186cc21a43201e3`; 100 files, 5,000,000 B, seed 1,
core-sdk-init-fixture-v2 recipe reused outside timing, without payload rehashes.

Case `phase7-init-100-direct-engines-v2`, order baseline then candidate, one
sample each. Baseline is the unmodified 7edddbdb8 SDK example; candidate calls
project::init over PostgreSQL/MinIO. No server crate is removed. Candidate times
engine open, validation and all connections plus Init. The child JSON field
bootstrap_ns in v2 means timed engine open/validation, not schema creation.
Empty runtime schemas are bootstrapped in setup, per plan §3.2, without workload
inputs. The separate verifier is the existing deterministic sampled oracle.

The cache contract is phase7-fresh-services-v1. Both fresh owned volumes start
empty and both input inventories have zero resident pages after invalidation.
Ordinary schema setup state and same-operation reads of newly written data are
declared, as in plan §3.2; no cold populated-store claim is made. Container start
and fresh schema bootstrap are untimed setup. Both VM identities match: Linux
6.12.76-linuxkit/aarch64, 8 CPUs, 4,108,828,672 B VM RAM. Per-service 2 CPUs,
512 MiB, no swap, 256 PID limits; PostgreSQL fsync/synchronous_commit/
full_page_writes remain on. Exact image IDs/settings hashes and observed
competing processes are retained. No phase memory PASS is claimed.

## Family 1 — Init comparison

| Files / case / arm | Raw call or direct operation ns | Complete child command ns / 15 s | Separate verifier ns / 9.5 s | Paths; sampled files / B | Allocation B | Functional / scratch cleanup | Numeric condition |
| --- | ---: | ---: | ---: | --- | ---: | --- | --- |
| 100 / phase7-init-100-direct-engines-v2 / baseline | 38354625 | 75085084 | 43248417 | 102; 53 / 3,354,003 | 7372800 | PASS / PASS | matched baseline |
| 100 / same / candidate | 248079166 | 806643708 | 201371083 | 102; 53 / 3,354,003 | 5804032 | PASS / PASS | strict speed FAIL; Init storage ceiling absent |
| 1,000 / phase7-init-1000-direct-engines-v2 | NOT_RUN | NOT_RUN | NOT_RUN | NOT_RUN | NOT_RUN | NOT_RUN | blocked qualification; no passing campaign |
| 10,000 / phase7-init-10000-direct-engines-v2 | NOT_RUN | NOT_RUN | NOT_RUN | NOT_RUN | NOT_RUN | NOT_RUN | same |
| 100,000 / phase7-init-100000-direct-engines-v2 | NOT_RUN | NOT_RUN | NOT_RUN | NOT_RUN | NOT_RUN | NOT_RUN | same |

The table's complete child command includes process launch, typed operation,
connection release and operation-owned scratch cleanup. It excludes explicitly
untimed service setup and the independent verifier. Service teardown is recorded
separately below; this diagnostic sublane does not assert complete seven-case
harness/cleanup admission. The verifier inventories every path/kind and directory
metadata, and reads every byte of 53 selected files (3,354,003 B), not all 5 MB.

Time difference `248079166 - 38354625 = 209724541 ns`
(+546.803784%; ratio 6.468038). Storage difference
`5804032 - 7372800 = -1568768 B`
(-21.277778%). No averaging/tolerance/waiver.

Candidate allocation: MinIO object data/metadata 5,148,672 B + PG C2 352,256 B
(including pack sequence, heap, indexes and TOAST) + PG C5 303,104 B = 5,804,032 B.
PostgreSQL database 8,681,139 B/WAL 16,777,216 B and MinIO system directory
77,824 B are separately reported excluded server overhead. 28 payload objects
have 5,009,926 B summed lengths. Relation-by-relation and file-by-file allocation
are retained. Collection and CHECKPOINT occur outside timers.

## Family 2 — retained history

| Stride / states / case | Driver / bound | Verifier / bound | Strict total ceiling B | Proof / storage / speed / cleanup |
| --- | --- | --- | ---: | --- |
| 10 / 17 / phase7-history-stride10-direct-engines-v1 | NOT_RUN / 60 s | NOT_RUN / 10 s | <54,278,964 | NOT_RUN; direct-port driver unbound |
| 3 / 53 / phase7-history-stride3-direct-engines-v1 | NOT_RUN / 170 s | NOT_RUN / 20 s | <70,427,034 | NOT_RUN; direct-port driver unbound |
| 1 / 157 / phase7-history-stride1-direct-engines-v1 | NOT_RUN / 170 s | NOT_RUN / 30 s | <92,342,273 | NOT_RUN; run-only, direct-port driver unbound |

## Count and SQL diagnosis

Before the verifier, C2 reports policy1, locate5, bodies0, catalogue2,
signatures1, reserve9 (2 ordinal reservations), register5, PUT28 and GET9.
Payload sent/received = 5,009,926 / 1,869,804 B; metadata written = 18,770 B;
one pooled pack, two value groups, 4,096 directory B, zero forced seals.
Locator hits/misses 1,089/566; pack hits/misses 118/9. These are actual counters,
not unique-object numbers or timings.

PG C2: 1 connection, 24 operations/Sync/Ready, zero simple Query, protocol
sent/received 90,256/38,295 B. The 24 exchanges reconcile as two policy reads
(open and Storage::new), 5 locate + 2 catalogue + 1 signature + 9 reserves +
5 registrations. Each ordinary C2 request is one implicit transaction;
register/reserve may execute several server statements inside one exchange.

PG C5: 1 connection, 20 operations/Sync/Ready, zero simple Query, protocol
sent/received 4,316/32,668 B. Open validation is one read transaction with
6 exchanges (BEGIN, tables, metadata count, metadata row, definition, COMMIT).
First inode allocation is 6 and genesis initialization 8, each one write
transaction with three control exchanges (BEGIN, NOWAIT authority lock, COMMIT).
Thus 20 C5 exchanges contain 8 control and 12 body exchanges, across three
transactions; no connection retry or guessed rollback occurs.

S3: 4 connections, 37 requests (28 PUT/9 GET), and 28 additional 100-continue
replies before body submission. Timed engine open/validation is 34,174,750 ns;
the remaining operation work is 213,904,416 ns. This arithmetic is diagnostic
attribution only; the gate uses the entire 248,079,166 ns. Bypassing Server has
not eliminated network/durability/serialization/acknowledgement work. One fewer
policy query or batching a control exchange alone is not evidence of closing
the measured 209,724,541 ns gap.

Post-proof warm-service query-plan diagnostics run each cause query once, with
no treatment resample. Locator membership uses a hash join/sequential scan for
this small cardinality (128 rows), with a primary-key index-only input scan;
metadata bodies use pack_pkey bitmap lookup (4 rows); pooled catalogue uses its
primary key (2 rows); the 93-row signature ring uses scan/sort. Their measured
server execution is 1.074, 0.078, 0.031 and 0.066 ms respectively. These plans
include diagnostic input subqueries and warm post-verifier buffers; they do not
model parameter planning or latency at large registered cases. Full plans and
buffers are in checks/step10-v2-query-plans.json. No index/hint/profile changes.

## Checks, failures and custody

First slice covering checks: project all targets 7 PASS/0 failed/0 ignored;
locked all-target core Clippy/fmt PASS; boundary 470 files PASS; tools 21 PASS.
Focused harness tests: Init2, history4, substrate7, contract5, residency10 PASS.
V2 example repair: release build PASS 0.89 s; owning project all-target Clippy
PASS 4.59 s; fmt PASS; contract5 PASS. No production source changes; functional
product proofs are reused by unchanged source from the first slice. No CI or
retired preflight is run. Remote/cloud, Linux OpenSSL and off-platform Init
qualification remain deferred.

Retained failures: first baseline build selector FAIL before compilation (the
verifier belongs to server); corrected baseline release build PASS 19.680266458 s.
Candidate initial release build PASS 18.675087834 s, both /30 s. Creation-inclusive
v1 pair remains a diagnostic strict time FAIL, with its missing VM identity and
scope asymmetry; neither v1 arm is relabelled or repeated unchanged. V2 strict
speed FAIL remains. Full step 10/11/M5 and all six other selections remain open.

Raw append-only runs:
`benchmark-results/fs-bench-pro/issue302-init100-baseline-c9165d4da/` and
`benchmark-results/fs-bench-pro/issue302-init100-candidate-c9165d4da/`.
Each retained manifest was independently hash-checked. Compact lossless copies
are checks/step10-v2-{baseline,candidate}-*.gz; source/result manifests are never
overwritten. Prospective freeze is issue302-init-contract-freeze-c9165d4da.
Reproduce with runner.py run --case phase7-init-100-direct-engines-v2 --arm
{baseline,candidate} --out <fresh worktree result path>, baseline additionally
--baseline-root target/phase7-baseline/layerfs. Persistent claims forbid another
sample at this exact source/harness/fixture/arm identity; a legitimate mechanism
change needs a new prospectively frozen matched pair, retaining these failures.

Commits 92d78ca1c and c9165d4da each have exact first-parent/staged/committed
Production LOC: 143238 -> 143238 (delta +0). Reference 65417, core 77821,
old path 6025, new path 7448, rest-core 64348 remain unchanged. Counting method:
python3 tools/production_loc.py --json --root <snapshot>, shipped SQL included;
tests/docs/tools/harness/examples excluded. This evidence-only commit also has
unchanged production LOC, with the exact staged comparison recorded separately.


## Owned-service teardown

After verification, storage accounting and labelled post-proof SQL diagnostics,
phase7_services.down removed only this worktree's labelled containers, volumes
and network: PASS, 1209505875 ns in its separate cleanup window. No foreign
resource was touched. The original raw performance receipts are not rewritten
to add this later cleanup interval. Both services are now down; future work
uses phase7_services.up for a fresh setup. Exact result is retained under
checks/step10-v2-service-teardown.json.
