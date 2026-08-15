# SQLite technology-selection experiment record

Experiment 1's paired matrix is now complete at 1, 10, and 100 MiB for durable
Create, reopened Read, and the 32-byte small Edit. The candidates used the same
frozen fixtures, canonical FastCDC object IDs, SQLite schema, SQL statements,
transaction boundaries, effective SQLite settings, durable commit, close/reopen
verification, and correctness oracle. Three independent reviewers audited the
comparison and the harness was repaired before these campaigns: foreign-key
enforcement is enabled in both lanes, all database sidecars are cleaned between
samples, and the TypeScript unequal-ID self-check now fails closed.
`storage_only` is diagnostic only; it is not reported as Create.

> **Important finding:** R-SQL durable Create rose from 6.812 ms at 1 MiB to
> 459.173 ms at 100 MiB, 67.4× for a 100× larger file. The frozen 32-byte edit
> rose from 7.799 ms to 495.049 ms, 63.5×. Small edits are therefore **not
> constant-time** under the current end-to-end path: it rescans the source and
> rebuilds the larger root/member metadata.

## 1. Experiment status

| Field | Value |
|---|---|
| Status | Latest paired 1 / 10 / 100 MiB matrix complete; decision remains frozen |
| Worktree | `/Users/yifanxu/Ephemeral-AI-Lab/layerfs-sqlite-techstack-experiment` |
| Branch | `experiment/sqlite-techstack` |
| Base commit | `67e3d77df3509d6d219c6dae78c2a537a3831e27` |
| Decision owner | Codex experiment harness |
| Selected candidate | `R-SQL` |
| 100 MiB comparison | Paired Create/Read/Edit run completed for both candidates |
| Exp 2 | Proceed with `R-SQL`; conditional `R-HYBRID` was not triggered |
| Production scope | No production LayerFS, canonical format, provider, materializer, FUSE, R-FS, or R-HYBRID changes |

## 2. Fixed environment

| Field | Value |
|---|---|
| Date/time/timezone | 2026-08-15, Asia/Shanghai; paired 1 / 10 / 100 MiB matrix completed |
| Host | MacBook Pro `Mac15,10` |
| CPU | Apple M3 Max, 14 logical cores reported (`hw.logicalcpu=14`) |
| Memory | 36 GiB (`hw.memsize=38654705664`) |
| OS/kernel/architecture | macOS 26.4.1; Darwin 25.4.0; `arm64` |
| Filesystem | Direct APFS, internal SSD, `/System/Volumes/Data` |
| Free space | Before-campaign value not recorded; 242 GiB remained at final observation |
| Power / thermal state | unavailable |
| Rust / Cargo | `rustc 1.96.0` / `cargo 1.96.0` |
| Node.js | `v22.23.1` from `/Users/yifanxu/.local/bin/node` |
| SQLite | `3.51.0` from `/usr/lib/libsqlite3.dylib` |
| Concurrent work policy | One build/benchmark writer; no competing storage job |

The Node `sqlite3` 6.0.1 binding was rebuilt against the macOS SDK system
SQLite. Rust `rusqlite` 0.40.2 was built with the same
`SQLITE3_LIB_DIR=/Library/Developer/CommandLineTools/SDKs/MacOSX15.2.sdk/usr`
and matching `SQLITE3_INCLUDE_DIR`; both report SQLite 3.51.0. No SQLite
extension APIs are used by the harness.

## 3. Candidate custody and build

| Candidate | Implementation | SQLite | Self-check | Eligible |
|---|---|---|---|---|
| `T-SQL` | `tsql.ts`, Node `sqlite3` 6.0.1 | 3.51.0 | pass | yes |
| `R-SQL` | `rust/src/main.rs`, `rusqlite` 0.40.2 | 3.51.0 | pass | yes |
| `R-HYBRID` | not implemented | not applicable | not run | no, trigger not met |
| `R-FS` historical | historical anchor only | not rerun | not applicable | reference only |

The exact build/self-check commands were:

```text
/Users/yifanxu/.local/bin/node generate_fixtures.mjs
npm install
npm rebuild sqlite3 --build-from-source --sqlite=/Library/Developer/CommandLineTools/SDKs/MacOSX15.2.sdk/usr
SQLITE3_LIB_DIR=/Library/Developer/CommandLineTools/SDKs/MacOSX15.2.sdk/usr SQLITE3_INCLUDE_DIR=/Library/Developer/CommandLineTools/SDKs/MacOSX15.2.sdk/usr/include cargo fmt --check
SQLITE3_LIB_DIR=/Library/Developer/CommandLineTools/SDKs/MacOSX15.2.sdk/usr SQLITE3_INCLUDE_DIR=/Library/Developer/CommandLineTools/SDKs/MacOSX15.2.sdk/usr/include cargo check
SQLITE3_LIB_DIR=/Library/Developer/CommandLineTools/SDKs/MacOSX15.2.sdk/usr SQLITE3_INCLUDE_DIR=/Library/Developer/CommandLineTools/SDKs/MacOSX15.2.sdk/usr/include cargo build --release
/Users/yifanxu/.local/bin/node --experimental-strip-types tsql.ts self-check
SQLITE3_LIB_DIR=/Library/Developer/CommandLineTools/SDKs/MacOSX15.2.sdk/usr SQLITE3_INCLUDE_DIR=/Library/Developer/CommandLineTools/SDKs/MacOSX15.2.sdk/usr/include rust/target/release/sqlite-techstack-rsql self-check
/Users/yifanxu/.local/bin/node generate_fixtures.mjs --100m
/Users/yifanxu/.local/bin/node campaign.mjs --100m
```

Both self-checks verified the exact `random-10m` root, SQLite 3.51.0,
close/reopen, equal-byte reuse, unequal same-ID rejection, bounded 32 KiB
source streaming, exact table counts after rollback and commit crash points,
rollback visibility of the old complete root, and post-commit visibility of the
new complete root. Both emitted effective settings with `foreign_keys=1`,
`journal_mode=wal`, `synchronous=2` (`FULL`), `mmap_size=0`,
`cache_size=-65536`, and `wal_autocheckpoint=0`.

## 4. Fixture custody

The generator is `generate_fixtures.mjs` in this directory. The frozen FastCDC
profile is the production profile: min 8,192, target 16,384, max 32,768,
normalization shift 2, seed 0; small mask `d90303537000`, large mask
`d90103530000`, shifted-small mask `1b20606a6e000`, shifted-large mask
`1b20206a60000`. The generator writes source bytes and canonical objects under
`data/fixtures` and `data/objects`; the complete summary is
`data/fixture-summary.json`.

| Fixture | Seed / edit | Bytes or shape | Logical digest | Manifest digest | Root digest | Objects / unique |
|---|---|---:|---|---|---|---:|
| `random-10m` | `0x51f15e` | 10,485,760 | `daa27a022e215c575a113c9ce7bef18e026692b222b96d6e92105045ea2a17de` | `ad3091b2fbf25dea9e3244b3529ca6066f6af4982f95cf796b6fc5061d530430` | `01b785248efc0ac806afd92b4689b23e067a0791fa2258f047e7b904a4cab591` | 481 / 131 |
| `duplicate-10m` | `0x51f15e`, repeated pattern | 10,485,760 | `f11c488d55916583138bf774ca61ea94bf8d914d3d57659d40a5879e54b8666c` | `a7462be0f6f26a402c65e50081ffab6a5416e66ac497526b75e1e5c7bb092c73` | `96c73817109b35cfd9be7f8ddad5121db16b5c354d3c581f019351c27bd551f0` | 481 / 9 |
| `edit-base-10m` | `0x71a5e` | 10,485,760 | `a4ce063d16a954e958a893ba8db00126ca88ec559ce61a1281bde9f4a87b6fc5` | `102ae2a0b8ce8ee62e24e0f0c8eff37fa9143c19cbd1ac50a00deaa402a2017f` | `4b37ad239c420e8823122c0878184605d033401b57e00fd78e792b9c9c19d046` | 640 / 120 |
| `edit-small-10m` | base plus 32-byte edit at `5 MiB + 123` | 10,485,760 | `0335c42d1e5aba27b16aa73f870719334456ae7ab07c1bc59f98514537228057` | `9563143577b95c745b27fe9e9ee02b36434e4d9de10b2f249c305bbb06c92f97` | `71feda0849c09454f338e9b71aeae2de1a4d5c7b7153daef6b37a1059e0ab316` | 640 / 120 |
| `cal-sql-10m` | `0x1234` | 10,485,760 | `049454231f3f4892f0b624a6e94d8c8c357f6172cfebc2a798f3f935008e3748` | `ed5f93a31599e2622babee3874f64eaf187b655fb82fdf60f0889b723e905ab5` | `17a9402bb30ae9d1d1db5b7ff2c54799705cf67e62b07eff243f3e115094826d` | 481 / 125 |
| `many-files` | `0x9000+i`, 100 files | 100 × 102,400 | not a single root | not a single root | not a single root | 570 total unique |
| `directory-10k` | entry index 0–9,999 | 10,000 entries | not applicable | `ad7f60805663d0727de190c4ad40a26c00b2531573699932b65cc3199a65ac3c` | `2716a45eea743df9b230b8da509332454d3ca9da4ea1e2d90aef7acf82f86e7a` | 10,000 |
| `random-100m` | `0x51f15e` | 104,857,600 | `52ce153eab81e33a0243a25a47a8805a86ba9bec125a27bee3c50de647cdafbc` | `1c3695599afbad2aa5168a528114cc5aecc57caf9620a5fb736edb4e6e77c6c1` | `c0666f5acb7cf92488efba90bfc53873c09d961afd7d15f0c52a21b2f2afebe7` | 4,801 / 263 |
| `edit-base-100m` | `0x71a5e` | 104,857,600 | `611220ba5af8384f073e8608fd891596c9bdb5db442883db812a9348c468bd16` | `05c060798eabebd1c36c2a7e545190cea66b84bd928ca8e40564324b1106ce8d` | `a26a84c425c79a18b01bfad0089af7db7dcb5d824bd41201c9c36bececad81e3` | 6,400 / 260 |
| `edit-small-100m` | base plus 32-byte edit at `50 MiB + 123` | 104,857,600 | `eac8556590efb0d472de65955ac71f58877dff183b1d0432dc95a53e0c9e9cf7` | `7b0872d5b11c5909bcbf8fbe4a15957f663fd8ec8b7c3ae78701dd39d94d8392` | `5fbb3724f820375a895e4f223a83cfe8fc9c31846242adca720983bb79a2428f` | 6,400 / 261 |

## 5. Calibration results

One warm-up and five retained samples were used. Input bytes were prepared
outside the timed interval. CPU time and RSS were not observed by the
calibration script and are therefore reported as unavailable.

| Workload | Median ms (p25 / p75; min–max) | MiB/s | Sync/checkpoint median | Bytes written / DB size |
|---|---:|---:|---:|---:|
| `CAL-APFS` durable sequential write, 10 MiB | 15.861 (15.601 / 16.087; 13.644–18.175) | 630.5 | `fsync` 8.368 ms | 10,485,760 |
| `CAL-SQL` one FULL-transaction BLOB, 10 MiB | 22.337 (22.185 / 22.778; 21.573–27.220) | 447.7 | checkpoint 0.606 ms | 10,502,144 |
| 100 MiB calibration | not run | not run | not run | finalist-only workload |

These are ceilings/diagnostics, not Create results. `CAL-SQL` is a Node
`sqlite3`-only diagnostic; it is not a neutral Rust-versus-Node calibration and
does not include the end-to-end CDC, object admission, root construction, or
reopen work.

## 6. Ten MiB headline comparison

This is the direct LayerFS SQLite technology comparison requested: `T-SQL`
(TypeScript plus Node `sqlite3`) versus `R-SQL` (Rust plus `rusqlite`). The
published M2/M3 headline table has no Rust lane, so it is not used as a Rust
versus Node A/B result. `duration_ns` is the timed transaction section plus the
immediate reopen and logical-digest verification. Setup, the first close, and
`wal_checkpoint(TRUNCATE)` are outside that duration and are recorded
separately. Each row is five retained samples after one unrecorded warm-up.
Throughput is 10 MiB divided by the median duration.

| Workload | `T-SQL` median ms (p25 / p75; min–max) | `R-SQL` median ms (p25 / p75; min–max) | R-SQL result |
|---|---:|---:|---:|
| Random durable Create | 142.106 (141.959 / 143.069; 140.349–143.941), 70.4 MiB/s | 37.272 (37.264 / 37.335; 37.103–37.363), 268.3 MiB/s | 73.8% faster |
| Duplicate-heavy durable Create | 132.294 (131.933 / 132.570; 129.816–133.366), 75.6 MiB/s | 33.395 (33.357 / 33.415; 33.011–33.617), 299.4 MiB/s | 74.8% faster |
| Reopened full read | 59.548 (58.886 / 59.888; 57.021–60.496), 167.9 MiB/s | 21.149 (21.075 / 21.199; 20.990–21.457), 472.8 MiB/s | 64.5% faster |
| Small edit: base → edited root | 138.495 (137.723 / 142.999; 137.483–144.770) | 38.942 (38.928 / 39.175; 38.422–39.228) | 71.9% faster |

Run-to-run noise is small relative to the candidate gap: the corrected random
Create median gap is 104.834 ms, while retained ranges are 3.592 ms for T-SQL
and 0.260 ms for R-SQL. The result is not a noise-level tie, although the
campaign uses candidate blocks rather than within-workload interleaving.

## 7. Create phase and counter comparison

Random 10 MiB durable Create, median of five retained samples:

| Phase/counter | `T-SQL` | `R-SQL` | Unit |
|---|---:|---:|---|
| End-to-end timed section + close/reopen verify | 142.106 | 37.272 | ms |
| Source read + FastCDC + object hash/admit | 94.083 | 21.218 | ms |
| Root/member build | 13.197 | 1.213 | ms |
| Timed transaction section (source + root + `COMMIT`) | 111.920 | 26.286 | ms |
| Immediate reopen + digest verify phase | 30.186 | 10.984 | ms |
| Checkpoint, separately | 0.054 | 0.014 | ms |
| CDC input | 10,485,760 | 10,485,760 | bytes |
| Objects requested / new / reused | 481 / 131 / 350 | 481 / 131 / 350 | count |
| Logical SQLite statement calls | 1,580 | 1,580 | count |
| Rows read / inserted | 1,446 / 614 | 1,446 / 614 | count |
| Database growth after checkpoint | 3,096,576 | 3,096,576 | bytes |
| WAL growth during operation | unavailable; post-checkpoint WAL was 0 | unavailable; post-checkpoint WAL was 0 | bytes |
| SQLite prepare calls | unavailable | unavailable | count |
| CPU time / host total bytes written | unavailable | unavailable | observation |

The same SQL statement texts and transaction sequence were used: configure
exactly, `BEGIN IMMEDIATE`, object lookup/insert, root insert, 481 member
inserts, current-head upsert, `COMMIT`, close, reopen, full logical-root/object
content verification, close, and separate `wal_checkpoint(TRUNCATE)`. The
candidates therefore have equivalent durability and logical statement/row
counts. The logical statement counts are identical for every retained workload;
internal SQLite prepare/cache calls were not observable and remain
`unavailable`.

The `Timed transaction section` label is deliberate. The harness starts that
timer before source streaming and root construction and stops it after
`COMMIT`; it is not an isolated measurement of the SQLite `COMMIT` syscall.
The measured difference therefore includes the Node binding/runtime cost of
the source and root operations as well as the durable SQLite work.

## 8. Storage-only diagnostic

This lane loads precomputed canonical object files and intentionally omits
source streaming/FastCDC. It is not Create.

| Metric, random 10 MiB object batch | `T-SQL` | `R-SQL` |
|---|---:|---:|
| Median durable store + reopen verify | 56.906 ms (56.191 / 58.137; 54.261–59.051) | 20.087 ms (20.039 / 20.137; 20.038–21.513) |
| Logical SQLite statement calls | 1,230 | 1,230 |
| Rows read / inserted | 1,096 / 614 | 1,096 / 614 |
| New / reused objects | 131 / 0 | 131 / 0 |
| Database growth | 3,096,576 bytes | 3,092,480–3,096,576 bytes |
| WAL after separate checkpoint | 0 bytes | 0 bytes |
| Peak RSS | 133.1–158.1 MiB observed | unavailable |

## 9. Small-edit comparison

The timed edit starts from the durable `edit-base-10m` database, applies the
frozen 32-byte source edit through the full end-to-end lane, commits, reopens,
and verifies `edit-small-10m`.

| Metric | `T-SQL` | `R-SQL` | Unit |
|---|---:|---:|---|
| Edit-to-visible durable root | 138.495 (137.723 / 142.999; 137.483–144.770) | 38.942 (38.928 / 39.175; 38.422–39.228) | ms |
| Source/FastCDC/hash/admit phase | 84.876 median | 22.655 median | ms |
| Root/member build phase | 17.013 | 1.763 | ms |
| Timed transaction section (source + root + `COMMIT`) | 103.258 | 26.276 | ms |
| Reopen/verify phase | 35.208 | 12.652 | ms |
| New / reused objects | 1 / 639 | 1 / 639 | count |
| Rows read / inserted | 1,923 / 643 | 1,923 / 643 | count |
| Database growth | 163,840 | 163,840 | bytes |
| New payload bytes | one changed canonical object; exact bytes in fixture custody | one changed canonical object; exact bytes in fixture custody | bounded object |

## 10. Recovery and correctness

| Requirement | `T-SQL` | `R-SQL` |
|---|---|---|
| Exact fixture/root digest after Create | pass | pass |
| Same canonical manifest/root digest | pass | pass |
| Equal bytes with same typed ID reuse incumbent | pass | pass |
| Same typed ID with unequal bytes fails closed | pass; rejection assertion repaired and exercised | pass |
| Clean close/reopen | pass | pass |
| Termination before commit returns old complete root | pass | pass |
| Termination after commit returns complete new root | pass | pass |
| No partial referenced root after interruption | pass; FK enforcement plus exact table-count oracle | pass; FK enforcement plus exact table-count oracle |
| Candidate carrier/materialization/provider/FUSE side effects | not applicable | not applicable |
| Bounded streaming; no whole-source staging fallback | pass; fixed 32,768-byte source buffer | pass; fixed 32,768-byte source buffer |

The interruption test used a child process that exits after writing the new
transaction but before commit, then a second child that exits after commit. The
parent only accepted the old or new complete root, never a partial root. The
self-check proves logical content and visible old-or-new recovery for the tested
fixtures. It does not claim full cryptographic re-authentication of every stored
object digest and manifest byte sequence, nor a total-process-memory bound.

## 11. Ledger

The campaign runner rotated candidate order by workload, with one warm-up and
five retained samples per candidate/workload. The actual order was T-SQL then
R-SQL for random Create and edit, and R-SQL then T-SQL for duplicate Create,
read, and storage-only. Each candidate ran its six iterations as a block; the
run was not within-workload A/B-interleaved, so cache, thermal, and time-order
effects remain a limitation. `results/campaign.jsonl` is the raw campaign
ledger for this run and `results/summary.json` is the generated aggregate; the
runner overwrites these files on a rerun and they are not cryptographically
hash-custodied.

The recorded `sqlite-techstack-result-v1` rows contain the counters needed for
this A/B comparison, but not every optional field listed in the broader
experiment specification (for example, source fingerprint, host metadata,
SQLite step counts, and filesystem byte counters). That custody gap is a report
limitation, not an unobserved candidate difference; both lanes emitted the same
instrumented logical counts and effective SQLite settings.

| Run ID range | Candidate order | Workloads | Samples | Result |
|---|---|---|---:|---|
| `exp1-00`–`exp1-09` | T/R | random Create | 10 | accepted |
| `exp1-10`–`exp1-19` | R/T | duplicate Create | 10 | accepted |
| `exp1-20`–`exp1-29` | R/T | reopened random read | 10 | accepted |
| `exp1-30`–`exp1-39` | T/R | small edit | 10 | accepted |
| `exp1-40`–`exp1-49` | R/T | random storage-only | 10 | accepted as diagnostic |

The summary was regenerated after correcting an aggregation-only grouping bug
that initially combined random and duplicate Create rows. The raw samples were
unchanged; only the summary grouping was rejected.

## 12. Rejected runs and anomalies

| Run | Symptom | Cause | Disposition / correction |
|---|---|---|---|
| `REJ-SQLITE-3.51.2` | R-SQL self-check reported SQLite 3.51.2 | Homebrew `pkg-config` selected a non-frozen library | Excluded; rebuilt against SDK `/usr/lib/libsqlite3.dylib`, reran self-check and campaign |
| `REJ-HARNESS-DB-REUSE` | Early campaign draft reused one DB across samples and seeded no read/edit state | Harness lifecycle bug | No samples retained; changed to unique DB paths and explicit unrecorded seeding |
| `REJ-FASTCDC-PORT` | Early cutter port was not the production scanner | Porting error found before fixture custody freeze | Replaced with the exact production scanner and regenerated fixture summary |
| `REJ-SUMMARY-GROUP` | First aggregate combined the two Create fixtures | Reporting-only grouping key omitted fixture ID | Raw ledger retained; summary regenerated by candidate/operation/fixture |
| `REJ-NODE-22.7` | Final verification with `/usr/local/bin/node` exited 139 in the native sqlite3 binding | Shell default Node was `v22.7.0`, outside the frozen `v22.23.1` runtime | Excluded; exact `/Users/yifanxu/.local/bin/node` reran the self-check successfully |

No rejected result is used in the decision.

## 13. TypeScript versus Rust decision

| Decision input | Observed | Rule | Outcome |
|---|---:|---|---|
| Random durable Create | R-SQL 37.272 ms vs T-SQL 142.106 ms; same counts | R-SQL wins or is no more than 10% slower | R-SQL wins |
| Duplicate durable Create | R-SQL 33.395 ms vs T-SQL 132.294 ms; same counts | reproducible advantage | R-SQL wins |
| Read regression | R-SQL 21.149 ms vs T-SQL 59.548 ms | no primary regression over 10% | pass |
| Edit regression | R-SQL 38.942 ms vs T-SQL 138.495 ms | no primary regression over 10% | pass |
| SQLite statements/transactions | identical logical counts and boundaries | equivalent required | pass |
| Durability/correctness | both pass repaired self-checks and crash-count oracle | mandatory | pass |

The ≤10% rule is one-sided: choose R-SQL when it wins or is no more than 10%
slower on durable Create, and choose T-SQL only if it wins a primary metric by
more than 10% under the protocol. Decision: **select `R-SQL`**. T-SQL did not
show a reproducible >10% advantage;
R-SQL was faster on every retained primary metric with equivalent semantics,
durability, effective settings, statement counts, and row counts. The measured
R-SQL bottleneck is the timed transaction section (26.286 ms median, 70.5% of
random Create duration), followed by immediate reopen/verify (10.984 ms). This
section includes source streaming, object admission, root/member construction,
and durable `COMMIT`; pure SQLite `COMMIT` time was not isolated. The
single-BLOB `CAL-SQL` median was 22.337 ms, but no evidence shows an external
carrier would remove that cost without merely moving durable work.

## 14. `R-HYBRID` trigger and Exp 2

The first trigger input is directionally present: the complete R-SQL timed
transaction section is 70.5% of random Create, and `CAL-SQL` is 22.337 ms.
That section includes source/object/root work and is not an isolated SQLite
`COMMIT` measurement. However,
the required second condition—direct evidence that an external sequential
carrier would remove that measured cost rather than move it—was not established.
The completed 100 MiB R-SQL finalist Create measured 297.7 MiB/s using the
same conservative duration boundary, exceeding the 100 MiB/s target. This does
not provide evidence that an external carrier would remove the remaining
timed-transaction cost rather than move it.

Therefore the conditional `R-HYBRID` experiment was **not triggered**. Exp 2 is
the selected `R-SQL` path and may authorize a carrier comparison only after a
separate target miss plus direct carrier-removal evidence. No R-HYBRID code,
materializer, provider, or FUSE work is included here.

## 15. Final 100 MiB matrix

The selected R-SQL candidate ran one unrecorded warm-up and five retained
samples per workload. The 100 MiB raw ledger is
`results/campaign-100m.jsonl`; no TypeScript 100 MiB runner was needed after
the Experiment 1 decision.

| Workload | Median ms (p25 / p75; min–max) | MiB/s | Source/CDC median | Timed transaction median | Reopen/verify median | Checkpoint median |
|---|---:|---:|---:|---:|---:|---:|
| R-SQL random durable Create | 335.933 (333.549 / 339.322; 333.091–340.788) | 297.7 | 206.547 ms | 227.790 ms | 108.143 ms | 0.015 ms |
| R-SQL 32-byte edit against 100 MiB | 510.381 (509.893 / 512.353; 508.501–515.403) | not a Create rate | 311.921 ms | 342.291 ms | 168.218 ms | 0.019 ms |

The 100 MiB Create duration is 9.01× the 10 MiB R-SQL Create duration for a
10× larger source. The frozen 32-byte edit duration is 13.10× the 10 MiB edit
duration. The edit requested 6,400 objects and reused 6,399 of them, but still
recorded 19,207 logical statement calls, 19,203 rows read, and 6,403 rows
inserted; the larger source scan and root/member reconstruction therefore do
not remain constant-time. Create recorded 14,672 statement calls, 14,406 rows
read, 5,066 rows inserted, 263 new objects, and 4,538 reused objects.

The 100 MiB result is a finalist candidate run, not a new TypeScript-versus-
Rust A/B comparison. The 10 MiB decision remains the language comparison
under the paired candidate protocol.

## 16. Final decision and fingerprints

| Question | Decision |
|---|---|
| Selected implementation | `R-SQL` |
| Why it won | 73.8% faster random durable Create, 74.8% faster duplicate Create, 64.5% faster read, 71.9% faster edit; identical logical counts and repaired recovery contract |
| Why T-SQL lost | No primary metric advantage; same SQLite work took materially longer through the Node binding |
| 100 MiB target met | yes for the selected R-SQL finalist: 297.7 MiB/s measured with the conservative Create boundary |
| Remaining measured bottleneck | 100 MiB R-SQL timed transaction section, 227.790 ms median, then reopen/verify at 108.143 ms; pure SQLite `COMMIT` time was not isolated |
| Required next work | Continue with R-SQL; optimize the source-scan/root-materialization path only under a new frozen experiment |
| Explicitly deferred | R-HYBRID/carriers, FUSE/projection, native materialization, provider/network, GC/compaction, cross-platform qualification |

Final source fingerprint (SHA-256 over sorted SHA-256 records for every file
under `experiments/sqlite-techstack`, excluding this report, generated
fixture/results data, dependency trees, and Rust build output):
`8931c73af9d98f060f1b0290c866c547173670f5c3c3a32f039a51e5fd2fb392`.
`git diff --check` passed. The tracked base commit remains
`67e3d77df3509d6d219c6dae78c2a537a3831e27`; no commit, push, rebase, reset, or
production-file change was performed.

## 17. Fresh paired rerun on the current experiment harness

This section is the authoritative result for the current raw ledgers. The
previous sections preserve the earlier campaign for history; the fresh run
overwrote `results/campaign.jsonl` and regenerated `results/summary.json`.
The paired 100 MiB comparison is in
`results/campaign-100m-paired.jsonl` and
`results/campaign-100m-paired-summary.json`.

The run used the unchanged pinned experiment base `67e3d77`, Node.js
`22.23.1`, Rust `1.96.0`, Cargo `1.96.0`, and SQLite `3.51.0`. Both semantic
self-checks passed before timing. An automated source comparison found all
eight SQL statement texts identical and the complete schema text byte-identical
between TypeScript and Rust. Every retained row reported the frozen settings:
`journal_mode=wal`, `synchronous=2` (`FULL`), `page_size=4096`, `mmap_size=0`,
`cache_size=-65536`, `wal_autocheckpoint=0`, and `foreign_keys=1`.

### Fresh calibration

| Workload | Median ms (p25 / p75; min–max) | MiB/s | Sync/checkpoint median |
|---|---:|---:|---:|
| `CAL-APFS`, 10 MiB durable file write | 10.333 (10.160 / 10.584; 9.995–13.582) | 967.8 | `fsync` 5.815 ms |
| `CAL-SQL`, 10 MiB one-BLOB FULL transaction | 13.673 (13.356 / 14.089; 12.635–14.609) | 767.1 | checkpoint 0.648 ms |

These are calibration diagnostics, not candidate Create results.

### Fresh 10 MiB comparison

Each candidate/workload has one unrecorded warm-up and five retained samples.
The timed boundary includes the transaction, `COMMIT`, close/reopen, and full
logical-root verification; checkpoint is recorded separately.

| Workload | T-SQL median ms (p25 / p75; min–max) | R-SQL median ms (p25 / p75; min–max) | R-SQL result |
|---|---:|---:|---:|
| Random durable Create | 195.802 (194.986 / 196.902; 190.153–198.731), 51.1 MiB/s | 52.508 (51.749 / 52.897; 51.181–53.487), 190.4 MiB/s | 73.2% faster |
| Duplicate-heavy durable Create | 178.677 (177.912 / 178.976; 177.301–181.415), 56.0 MiB/s | 45.384 (45.368 / 45.460; 45.304–45.468), 220.3 MiB/s | 74.6% faster |
| Reopened full read | 79.827 (79.555 / 81.107; 78.714–81.174), 125.3 MiB/s | 28.599 (28.448 / 29.041; 28.258–29.110), 349.7 MiB/s | 64.2% faster |
| Small edit: base → edited root | 186.104 (184.937 / 187.347; 182.330–191.865) | 53.885 (52.656 / 54.070; 51.536–54.916) | 71.0% faster |

For random Create, both candidates performed `1,580` logical statement calls,
read `1,446` rows, inserted `614` rows, created `131` objects, reused `350`
objects, and grew the database by `3,096,576` bytes. The duplicate, read, edit,
and storage-only rows likewise have identical logical SQL and row counters
between candidates. The fresh phase medians for random Create were:

| Phase | T-SQL | R-SQL |
|---|---:|---:|
| Source read + FastCDC + object admission | 131.683 ms | 29.365 ms |
| Root/member build | 16.977 ms | 1.748 ms |
| Timed transaction through `COMMIT` | 154.990 ms | 36.915 ms |
| Close/reopen/full verification | 41.130 ms | 15.284 ms |
| Separate checkpoint | 0.078 ms | 0.021 ms |

### Fresh paired 100 MiB comparison

This additional comparison used the same six-iteration shape for both
candidates: one warm-up and five retained samples. It measures random durable
Create and the frozen 32-byte small edit against the 100 MiB base.

| Workload | T-SQL median ms (p25 / p75; min–max) | R-SQL median ms (p25 / p75; min–max) | R-SQL result |
|---|---:|---:|---:|
| Random durable Create | 1,848.989 (1,829.517 / 1,873.632; 1,804.446–1,873.668), 54.1 MiB/s | 459.173 (457.814 / 459.768; 457.389–461.403), 217.8 MiB/s | 75.2% faster |
| 32-byte small edit | 1,977.472 (1,973.258 / 1,983.316; 1,899.646–2,079.836) | 495.049 (493.807 / 495.655; 493.707–495.877) | 75.0% faster |

The 100 MiB Create rows used identical `14,672` statement calls, `14,406`
rows read, `5,066` rows inserted, `263` new objects, and `4,538` reused
objects. The edit rows used identical `19,207` statement calls, `19,203` rows
read, `6,403` rows inserted, `1` new object, and `6,399` reused objects. The
database growth was identical between candidates for each workload: `7,069,696`
bytes for Create and `1,519,616` bytes for edit.

### Fresh rerun decision

The result is not a noise-level tie. On the primary 10 MiB metrics, R-SQL is
`73.2%` faster on durable Create, `64.2%` faster on reopened read, and `71.0%`
faster on the small edit. The independent 100 MiB comparison reproduces the
same direction at `75.2%` faster Create and `75.0%` faster edit. The SQL text,
schema, transaction boundary, SQLite durability settings, statement counts,
row counts, object reuse behavior, and correctness checks remain equivalent.

Decision remains: **select `R-SQL`**. The latest run does not trigger
`R-HYBRID`; it shows a large language/binding gap under the same SQLite work,
not a correctness or durability advantage for TypeScript. The remaining R-SQL
cost is dominated by source/CDC/object admission plus the timed durable
transaction and reopen verification, not by a changed benchmark boundary.

Fresh source fingerprint, excluding this report, generated results, fixture
data, dependency trees, and Rust build output:
`2b904a5f09f550b13ba871f27c391368a76164128185f815fbfb22c27dd6c1fd`.

## 18. Latest status: complete 1 / 10 / 100 MiB paired matrix

This is the latest status as of 2026-08-15. The benchmark source remains the
current experiment harness at the pinned base
`67e3d77df3509d6d219c6dae78c2a537a3831e27`; its source fingerprint is still
`2b904a5f09f550b13ba871f27c391368a76164128185f815fbfb22c27dd6c1fd`. The
latest M8 production worktree was separately verified at `8c1c557`, but it
does not contain an R-SQL lane, so it is not mixed into this TypeScript-versus-
Rust comparison. This report compares the latest code available in both
experiment lanes under one frozen protocol.

The new 1 MiB fixtures use the exact seeded byte generator, FastCDC profile,
manifest construction, and object layout used by `generate_fixtures.mjs`:
`random-1m` uses seed `0x51f15e`; `edit-base-1m` uses `0x71a5e`; and the
32-byte edit is at offset `524,411` (half the file plus 123 bytes). The 10 and
100 MiB edit offsets remain the protocol's `5 MiB + 123` and `50 MiB + 123`.
Both candidates consumed the same generated fixture files and manifests.

Each size/workload pair below has one unrecorded warm-up and five retained
samples. Times are milliseconds; parentheses are p25 / p75. “R-SQL faster” is
`1 - R-SQL median / T-SQL median`.

### Durable Create scaling

| Source | T-SQL median (p25 / p75) | R-SQL median (p25 / p75) | T-SQL MiB/s | R-SQL MiB/s | R-SQL faster | T/R time ratio |
|---|---:|---:|---:|---:|---:|---:|
| 1 MiB | 23.500 (22.942 / 23.535) | 6.812 (6.749 / 6.852) | 42.6 | 146.8 | 71.0% | 3.45× |
| 10 MiB | 195.802 (194.986 / 196.902) | 52.508 (51.749 / 52.897) | 51.1 | 190.4 | 73.2% | 3.73× |
| 100 MiB | 1,848.989 (1,829.517 / 1,873.632) | 459.173 (457.814 / 459.768) | 54.1 | 217.8 | 75.2% | 4.03× |

The result is stable across file size: R-SQL remains materially faster, and
the advantage grows from 71.0% at 1 MiB to 75.2% at 100 MiB. Create is not a
constant-time operation: from 1 to 100 MiB, T-SQL grows by 78.7× and R-SQL by
67.4× for a 100× source-size increase. The lower 1 MiB throughput is fixed
per-operation overhead becoming visible; it does not erase the binding gap.

### Reopened full Read scaling

| Source | T-SQL median (p25 / p75) | R-SQL median (p25 / p75) | R-SQL faster | T/R time ratio |
|---|---:|---:|---:|---:|
| 1 MiB | 9.431 (9.218 / 9.691) | 3.877 (3.820 / 3.887) | 58.9% | 2.43× |
| 10 MiB | 79.827 (79.555 / 81.107) | 28.599 (28.448 / 29.041) | 64.2% | 2.79× |
| 100 MiB | 861.310 (860.902 / 870.509) | 299.418 (291.116 / 300.146) | 65.2% | 2.88× |

### 32-byte small-edit scaling

| Source | T-SQL median (p25 / p75) | R-SQL median (p25 / p75) | R-SQL faster | T/R time ratio |
|---|---:|---:|---:|---:|
| 1 MiB | 22.159 (21.260 / 22.707) | 7.799 (7.696 / 8.465) | 64.8% | 2.84× |
| 10 MiB | 186.104 (184.937 / 187.347) | 53.885 (52.656 / 54.070) | 71.0% | 3.45× |
| 100 MiB | 1,977.472 (1,973.258 / 1,983.316) | 495.049 (493.807 / 495.655) | 75.0% | 3.99× |

This is the important scaling finding: a 32-byte edit is emphatically **not
constant-time** under the current end-to-end protocol. From 1 to 100 MiB,
T-SQL grows by 89.2× and R-SQL by 63.5×. The edit still scans and CDCs the
source, reconstructs the root/member list, executes the equivalent SQLite
work, commits durably, closes, reopens, and verifies the resulting root.
Changing one byte therefore does not reduce the work to one SQLite row.

### Counter and correctness parity

The pairwise logical counters are identical at every size. Representative
durable Create counters are:

| Source | Statement calls | Rows read | Rows inserted | New objects | Reused objects | Database growth |
|---|---:|---:|---:|---:|---:|---:|
| 1 MiB | 175 | 150 | 73 | 22 | 27 | 524,288 bytes |
| 10 MiB | 1,580 | 1,446 | 614 | 131 | 350 | 3,096,576 bytes |
| 100 MiB | 14,672 | 14,406 | 5,066 | 263 | 4,538 | 7,069,696 bytes |

The paired Read counters are identical at 1 / 10 / 100 MiB: `104 / 968 /
9,608` statement calls and `200 / 1,928 / 19,208` rows read respectively. The
paired Edit counters are also identical: `199 / 1,927 / 19,207` statement
calls and `195 / 1,923 / 19,203` rows read respectively. The raw result ledgers are
authoritative for all counters; no candidate-specific shortcut or SQL
statement substitution was used.

Verification status is all pass:

- TypeScript and Rust semantic self-checks pass on the repaired recovery
  contract, including before-commit rollback, after-commit visibility, equal
  object reuse, and unequal same-ID rejection.
- All eight SQL statement texts match, and the complete schema text is
  byte-identical.
- Every retained row uses SQLite `3.51.0` and the same effective settings:
  WAL, `synchronous=FULL`, 4 KiB pages, `mmap_size=0`, `cache_size=-65536`,
  `wal_autocheckpoint=0`, and foreign keys enabled.
- The final combined integrity check covered 110 retained rows: 50 at 10 MiB,
  30 at 1 MiB, and 30 at 100 MiB. Counter parity, pragma parity, and reopen
  verification passed.

The decision is unchanged: **select `R-SQL`**. The three-size matrix confirms
that the Rust advantage is not a one-size artifact, while the edit scaling
confirms that the current architecture still has source-size-dependent work.
The next optimization target remains source scan/CDC, root materialization,
and binding overhead; changing SQLite statements or weakening durability would
break the fairness contract.

New raw and summarized ledgers:

- `results/campaign-1m-paired.jsonl` and `results/campaign-1m-paired-summary.json`
- `results/campaign-100m-paired.jsonl` and `results/campaign-100m-paired-summary.json`

`git diff --check` remains clean for the report. No production-file change,
commit, push, rebase, reset, or benchmark-source change was performed.

## 19. Latest M7/M8 implementation compatibility audit

The requested rerun against the latest production edit implementation was
audited on 2026-08-15, but no mixed Node/Rust result is retained because the
two lanes do not currently implement the same algorithm.

The latest production worktree is clean at commit
`8c1c55785ab25d0435d47e32e35a1a7bdf1fbf6d`. Its edit route enters
`prepareDurableEditedContent`, attempts bounded Merkle descent and local
rebuild, reads a bounded source window, path-copies authenticated manifest
nodes, and persists the changed object/node frontier through the production
SQLite storage layer. That path relies on production manifest-node tables,
validation certificates, subtree summaries, and storage transaction ports. It
is not the flat `objects`/`roots`/`root_members` model used by Experiment 1.

The latest production edit sweep was executed without modifying its source by
rewriting only its stale Windows `file:///C:/...` import prefix in the stdin
stream. The actual implementation and worktree were unchanged. Runtime was
Node `22.23.1`, SQLite `3.51.0`, and the production sweep used a 64 MiB SQLite
cache. These are single-sweep observations, not the five-sample paired
Experiment 1 protocol:

| Source size | edit@0 | edit@mid | edit@EOF | Mode | Loaded entries | Affected | New objects | Storage tx / source-read tx |
|---|---:|---:|---:|---|---:|---:|---:|---:|
| 1 MiB | 20.3 ms | 15.4 ms | 5.6 ms | `local-rebuild` | 8 / 8 / 8 | 1 / 1 / 1 | 1 / 1 / 1 | 2 / 1 for each |
| 20 MiB | 24.1 ms | 18.3 ms | 13.0 ms | `local-rebuild` | 143 / 143 / 143 | 1 / 1 / 1 | 1 / 1 / 1 | 2 / 1 for each |
| 100 MiB | 33.2 ms | 30.7 ms | 21.9 ms | `local-rebuild` | 168 / 256 / 97 | 1 / 1 / 1 | 1 / 1 / 1 | 2 / 1 for each |

The Rust lane at
`experiments/sqlite-techstack/rust/src/main.rs` still performs the older
end-to-end source scan, CDC, object admission, flat root-member write, close,
reopen, and full verification. Its self-check passes on SQLite `3.51.0`, but
that is not evidence that it implements the latest bounded route.

Running the latest Node route against that Rust binary would therefore compare
different manifest formats, SQL schemas, read paths, edit algorithms, and
verification boundaries. Calling the Node implementation from Rust would also
not be an R-SQL measurement. The mixed run is rejected under the experiment's
invalid-result rule. The existing Experiment 1 decision (`R-SQL` under the
frozen flat-schema harness) is unchanged and must not be presented as a
latest-M7/M8 Node-versus-Rust result.

To produce the requested paired latest-implementation benchmark, the next
scope must be an explicit Rust port of the bounded manifest descent,
reconnection, path-copy persistence, and production SQLite manifest-node
schema, followed by a new paired campaign. That is a new implementation
scope, not a benchmark-command change, so it was not silently substituted into
the pinned Experiment 1 report.

## 20. X1 materialization: 100 MiB Rust + SQLite versus Node + SQLite

The requested Rust materialization run is now implemented as the experiment's
X1 contract in both lanes. This is a fair technology-stack comparison, not a
claim that production M8 has a Rust implementation. Each six-iteration run
used one unrecorded warm-up and five retained samples over the same
`random-100m` database. The timed X1 phase performs the same logical reads,
writes the reconstructed bytes to a new direct-APFS file, calls `fsync`, and
checks the output size and BLAKE3 logical digest. The surrounding result also
commits the SQLite read transaction and reopens/verifies the database root.

| Metric | T-SQL | R-SQL | Rust result |
|---|---:|---:|---:|
| X1 extraction + destination `fsync` median | 479.613 ms (474.499–486.203) | 195.787 ms (195.054–198.096) | **2.45× faster** |
| X1 throughput median | 208.5 MiB/s | 510.8 MiB/s | **2.45× higher** |
| Full timed operation including reopen verification | 898.306 ms (894.702–908.889) | 345.574 ms (343.476–345.669) | **2.60× faster** |
| Materialized bytes | 104,857,600 | 104,857,600 | equal |
| SQLite statement calls | 9,608 | 9,608 | equal |
| SQLite rows read | 19,208 | 19,208 | equal |
| Output digest | `52ce153e…7cdafbc` | `52ce153e…7cdafbc` | equal |
| Destination durability | `fsync` passed | `fsync` passed | equal |

Both lanes reported SQLite `3.51.0` with `WAL`, `synchronous=FULL`, 4 KiB
pages, `mmap_size=0`, `cache_size=-65536`, `wal_autocheckpoint=0`, and foreign
keys enabled. The X1 SQL sequence and transaction boundary are identical:
`SELECT_HEAD`, `SELECT_ROOT`, `SELECT_MEMBERS`, one `SELECT_OBJECT` per
member, one `BEGIN IMMEDIATE`, and one `COMMIT`. The digest, byte count,
statement count, row count, and reopen verification all passed, so the Rust
result is not obtained by skipping the database read or correctness check.

This result must not be substituted for the historical production A7 number
(`108.5 MiB/s` in the M3 artifact). Production A7 is the production LayerFS
engine's reopened `readStream` digest check; it does not run this experiment's
direct-output-file-plus-`fsync` contract, and the production worktree has no
Rust lane. The valid comparison here is the paired X1 result above: Rust +
SQLite is about 2.45× faster than the same flat-schema Node + SQLite path for
100 MiB materialization under the same SQL and durability rules.

Decision remains: **select `R-SQL`**. X1 adds a materialization result to the
decision evidence; it does not change the latest-production compatibility
finding in Section 19.

The source fingerprint printed in Section 18 is the fingerprint of the
earlier 1 / 10 / 100 MiB matrix; this X1 harness addition is a subsequent
experiment-only change and does not alter any production file.

## 21. Next phase: latest M7/M8 Node-versus-Rust bounded-path qualification

This phase is complete as an experiment-only port and comparison. It is the
first valid Node-versus-Rust result for the current bounded M7/M8 path; the
flat-schema R-SQL result in Section 18 remains historical and is not mixed
with these measurements.

### Contract, custody, and implementation

- Production reference: `/Users/yifanxu/Ephemeral-AI-Lab/layerfs-m8-verify`,
  clean at commit `8c1c55785ab25d0435d47e32e35a1a7bdf1fbf6d`.
- Experiment reference: branch `experiment/sqlite-techstack` in
  `/Users/yifanxu/Ephemeral-AI-Lab/layerfs-sqlite-techstack-experiment`,
  pinned base `67e3d77df3509d6d219c6dae78c2a537a3831e27`.
- Rust lane:
  `experiments/m8-rust-port/src/main.rs` with the paired Node runner at
  `experiments/m8-rust-port/node.mjs`. The old
  `experiments/sqlite-techstack/rust/src/main.rs` flat-schema harness was not
  changed and was not used for this result.
- Node calls the current built production
  `prepareDurableEditedContent` implementation. Rust follows the same
  bounded source-window read, authenticated Merkle descent, local rebuild,
  reconnect, canonical `EAFN`/`EAFR` encoding, CAS/node/root persistence,
  validation, root publication, close/reopen, and logical digest check.
- Both lanes use the same deterministic fixture bytes (`0x5eed`), SHA-256
  identities, FastCDC parameters `32 KiB / 128 KiB / 512 KiB`, source-read
  bound of `2 MiB`, edit byte `0x01`, and offsets at begin, midpoint, and
  EOF. Both lanes use the production-shaped `efs_cas_objects`,
  `efs_manifest_roots`, `efs_manifest_nodes`,
  `efs_manifest_validations`, `efs_manifest_subtree_summaries`,
  `efs_root_journal`, `efs_meta`, and `efs_usage` contract needed by this
  path.
- Node publishes the candidate head through the production staging port in
  the persistence transaction. Rust publishes the same root-journal record,
  validation, metadata, usage, and content rows in its corresponding SQLite
  write transaction. No SQL shortcut, flat root-member table, or whole-source
  edit scan is used in the retained Rust lane.

The executable versions were recorded rather than treated as identical:
Node `22.23.1` with SQLite `3.51.3`; Rust `1.96.0` with rusqlite-linked SQLite
`3.51.0`. Both used a 64 MiB SQLite cache and the following effective
settings: `temp_store=FILE`, `foreign_keys=ON`, `journal_mode=WAL`,
`synchronous=FULL`, `mmap_size=0`, `cache_size=-65536`, and
`wal_autocheckpoint=130308`. The Node result budget was 256 MiB to accommodate
the production cursor's result accounting; this did not change the logical
2 MiB source-read bound. Each retained sample used a fresh temporary database
and candidates were run serially, with no disk contention.

### Correctness and recovery

All retained 1 / 10 / 20 / 100 MiB runs produced byte-for-byte equal Node and
Rust base roots and equal begin/mid/EOF candidate roots. Each candidate was
reopened before its logical digest was accepted. The manifest entry/node
counts were identical by size:

| Source | Entries | Manifest nodes | Base root (Node = Rust) |
|---:|---:|---:|---|
| 1 MiB | 8 | 1 | `fa2f1fddfe54269e01f5f8d22ba963af835640c758d48e3b11331a3c0a0e6af1` |
| 10 MiB | 69 | 1 | `4f528d03be3034343327085180a0913945be6ddeb3fcf4dd5e308b1233b0028e` |
| 20 MiB | 143 | 1 | `acbcd4fb4a381332e99cc7c1106a0ffac8140145ec406824dc797790276460ea` |
| 100 MiB | 677 | 5 | `deb9c081d925643ab1b62018bef758cb1fd8284f517eafdf40db2c8f9d16fae4` |

The full edited root and digest values are emitted by both runner JSON lines;
the equality check compares the complete 32-byte values, not prefixes. The
1 MiB post-probe smoke run also recorded the same full roots and digests in
both lanes, including the EOF root
`bdc182a1bb36339d5785843ad4502f0f1ff2e57694b72fc1b2a3a14fea192a36` and
digest
`6dac22922a3bc4bf2bc182ac1030f7bd5fd6a90220a95830c857026e9d96fcb9`.

The out-of-band guard checks passed in both implementations:

| Guard | Node | Rust |
|---|---:|---:|
| Equal same-ID/equal-bytes CAS reuse | pass | pass |
| Same-ID/different-bytes rejection | pass | pass |
| Intentional write termination rolls back | pass | pass |
| No partial CAS row after rollback | pass | pass |
| Published head unchanged after failed write | pass | pass |
| Close/reopen and logical digest | pass | pass |

Initial fixture construction necessarily materializes the test source in both
harnesses. The edit path itself reads only the bounded local source window;
the full-file operation after commit is a deliberate one-file materialization
and digest verification, not an edit-time source scan.

### Retained edit timing

Each size used one warmup followed by five retained samples. Warmups were kept
separate and are shown only as elapsed harness time; the retained tables below
measure `prepare_ms`, from bounded source/path work through the candidate's
durable commit, before the separate reopen verification.

| Size | Node warmup elapsed | Rust warmup elapsed |
|---:|---:|---:|
| 1 MiB | 240.922 ms | 623.034 ms |
| 10 MiB | 1,670.318 ms | 5,359.262 ms |
| 20 MiB | 3,186.948 ms | 10,531.989 ms |
| 100 MiB | 15,912.615 ms | 51,968.318 ms |

The retained five-sample statistics are `min / p50 / p95 / max` in
milliseconds. The p95 is the order statistic of the five retained values;
there is no interpolation hidden in the reducer.

| Size | Edit | Node min / p50 / p95 / max | Rust min / p50 / p95 / max |
|---:|---|---:|---:|
| 1 MiB | begin | 12.501 / 13.385 / 14.031 / 14.031 | 48.344 / 48.440 / 49.259 / 49.259 |
| 1 MiB | mid | 9.494 / 9.970 / 10.054 / 10.054 | 47.778 / 48.252 / 48.444 / 48.444 |
| 1 MiB | EOF | 3.361 / 3.670 / 4.157 / 4.157 | 1.916 / 1.952 / 1.978 / 1.978 |
| 10 MiB | begin | 19.379 / 19.675 / 20.543 / 20.543 | 91.266 / 91.646 / 92.560 / 92.560 |
| 10 MiB | mid | 11.757 / 12.557 / 12.986 / 12.986 | 94.147 / 94.415 / 94.526 / 94.526 |
| 10 MiB | EOF | 19.060 / 19.238 / 19.862 / 19.862 | 21.958 / 22.070 / 22.128 / 22.128 |
| 20 MiB | begin | 22.392 / 22.774 / 23.635 / 23.635 | 92.764 / 93.205 / 93.664 / 93.664 |
| 20 MiB | mid | 16.104 / 16.479 / 17.104 / 17.104 | 95.044 / 95.224 / 95.955 / 95.955 |
| 20 MiB | EOF | 9.093 / 16.442 / 16.979 / 16.979 | 3.354 / 3.400 / 3.408 / 3.408 |
| 100 MiB | begin | 29.361 / 29.488 / 30.879 / 30.879 | 103.520 / 104.086 / 105.530 / 105.530 |
| 100 MiB | mid | 24.846 / 25.113 / 26.228 / 26.228 | 107.284 / 107.584 / 108.069 / 108.069 |
| 100 MiB | EOF | 17.043 / 17.884 / 22.142 / 22.142 | 25.381 / 25.578 / 26.158 / 26.158 |

Node wins 10 of the 12 retained edit-position comparisons by p50. Rust wins
only the 1 MiB and 20 MiB EOF positions; it does not win the 100 MiB target
position. The Rust end-to-end elapsed time is also dominated by the deliberate
close/reopen full-file verifier, while the table isolates the comparable
durable edit preparation path.

### Fairness counters and storage behavior

The common per-edit behavior was one new object at every size. At 1 / 10 / 20
MiB each lane produced one new manifest node per edit and no reusable subtree;
at 100 MiB each lane produced two new manifest nodes and reused three existing
subtrees per edit. Node reported two storage transactions and one source-read
transaction per edit. Rust used one bounded source-read transaction plus one
write transaction per edit; its cumulative counters therefore appear as
`2/3/4` total transactions and `1/2/3` source transactions after the three
edits.

Representative 100 MiB counters were:

| Metric | Node begin / mid / EOF | Rust begin / mid / EOF or total |
|---|---:|---:|
| Loaded entries | 168 / 256 / 97 | 2 / 2 / 1 |
| Affected entries | 1 / 1 / 1 | 2 / 2 / 1 |
| New objects | 1 / 1 / 1 | 1 / 1 / 1 |
| New manifest nodes | 2 / 2 / 2 | 2 / 2 / 2 |
| Reused subtrees | 3 / 3 / 3 | 3 / 3 / 3 |
| Source-read window | bounded by 2 MiB | 247,307 / 417,360 / 177,677 bytes |
| Persistence rows | 790 / 1,142 / 506 | Rust total inserted rows: 706 |
| Persistence bytes | 187,613 / 171,856 / 185,589 | Rust source bytes total: 4,371,981 |
| Persistence units | 702 / 1,054 / 418 | Rust SQL statements total: 2,805 |

The different loaded-entry and row-counter surfaces are explained by the
production Node metrics reporting the bounded rebuild frontier and persistence
batch accounting, while the Rust lane reports raw SQLite statement/read/insert
counters. They are not relabeled as if they were the same counter. Both lanes
did execute the manifest-node lookup, CAS verification, summary, root,
validation, usage, and head-publication SQL path.

The 100 MiB Node run reported a pre-checkpoint WAL of 108,483,752 bytes. The
explicit final `wal_checkpoint(TRUNCATE)` returned `busy=0`, zero remaining
log frames, and zero residual WAL bytes. Rust's corresponding checkpoint also
returned `busy=0`, zero log frames, and zero checkpointed residual frames.
The evidence does not show a carrier-side storage win that would justify an
R-HYBRID side path.

### Materialization and deferred matrix items

The retained edit loop includes the requested one-file materialization shape:
after every durable edit, Node reads the manifest in 2 MiB ranges and hashes
the complete logical file; Rust closes/reopens SQLite, walks the root/node
tree, authenticates each object, and hashes the same logical file. This is the
reopen/digest verification reported above, not a skipped verification.

Create-only, many-file materialization, 100 sequential edits, 500 scattered
edits, and process-kill recovery are not included in this phase. They require
a namespace/multi-file or kill-injection harness that is not part of the
current one-file bounded M8 contract. The transaction rollback probe and
post-commit close/reopen recovery are included; no deferred scenario is mixed
into the technology decision.

### Rejected or corrected runs

1. The pre-existing flat-schema Rust result remains historical. It used
   `objects`/`roots`/`root_members` and whole-source CDC and was not compared
   with this latest Node path.
2. The first Rust manifest candidate hashed the leaf grouping length as an
   8-byte value instead of the production `EAFN` 4-byte leaf length. Its roots
   diverged at 20 MiB. The implementation was corrected and the candidate was
   discarded before retained measurements; the retained roots then matched.
3. The first Node 100 MiB attempt used a 64 MiB transaction result budget and
   hit the production cursor's aggregate result-budget error. It was rejected,
   the runner budget was raised to 256 MiB, and the logical source window
   remained 2 MiB. No partial result was retained.
4. Rust initially used `wal_autocheckpoint=0` while Node's production driver
   defaulted to 130,308 pages. Rust was changed to the Node effective setting
   before the retained campaign. The retained matrix therefore uses matching
   PRAGMA behavior.

### Decision

The result must be reported as three separate facts:

- Historical flat-schema Experiment 1: R-SQL was about 2.45× faster on its
  old 100 MiB X1 contract; that result remains valid only for that old
  contract.
- Latest production Node M7/M8 path: this is the production reference and
  the faster bounded edit-preparation candidate in the new paired test.
- New paired latest Node-versus-Rust path: Node wins the retained p50 at 10 of
  12 edit positions while preserving exact root/digest equality and the same
  SQLite durability boundary. Rust does not meet the latest bounded-edit
  target at 100 MiB.

Decision for this latest bounded-path qualification: **select the current
Node + SQLite path; do not select the new Rust port for the bounded edit
route**. No R-HYBRID implementation is added: the loss is attributable to
the current Rust port's path/reopen/binding costs, not evidence that a carrier
side store removes at least 25% of measured end-to-end BLOB/WAL/checkpoint
cost. A carrier experiment would be a separate, explicitly authorized phase.

No production file was modified. The production worktree remained clean at
the stated commit; all new source and measurements are isolated under
`experiments/m8-rust-port/` and this report section.

## Phase 2 — latest M7-shaped Rust R-SQLite versus R-HYBRID

### Status and custody

Phase 2 is complete for the implemented matrix. The retained campaign was
rerun after correcting a benchmark-path error: R-HYBRID had been rereading and
rehashing the entire carrier inside the SQLite metadata timing interval even
though the carrier had already been hashed during construction and fsynced.
That duplicate full-carrier scan was removed. The results below are from the
corrected campaign only; the earlier contaminated JSONL is not used for the
decision.

| Item | Exact value |
|---|---|
| LayerFS checkout used | `/Users/yifanxu/Ephemeral-AI-Lab/layerfs-m8-verify` |
| LayerFS checkout HEAD used | `8c1c55785ab25d0435d47e32e35a1a7bdf1fbf6d` |
| M7 implementation source commit | `ce9035e49037f60a8c52d2775fd2d88d34e57cd4` |
| Experiment branch | `experiment/sqlite-techstack` |
| Experiment base before Phase 2 | `67e3d77df3509d6d219c6dae78c2a537a3831e27` |
| Experiment final commit | Exact hash is recorded in the final handoff after this report is committed. |
| Production checkout modified | No |
| Campaign | 6 samples/cell, 1 warmup discarded, 5 retained, median reported |
| Child runs | 120 benchmark children, serialized, alternating lane order |
| Raw results | `experiments/sqlite-techstack/results/phase2.jsonl` |
| Summary results | `experiments/sqlite-techstack/results/phase2-summary.json` |

### Mandatory subagent findings

The three required read-only subagents ran in parallel before implementation;
none edited overlapping implementation files.

| Subagent | Findings used |
|---|---|
| M7 archaeology | The bounded-edit path is in `packages/fs/src/operations/durable-edit-prepare.ts`, especially `tryPrepareDurableEditedContentSync`, `tryLocallyRebuiltContent`, `tryBoundedLocalRebuild`, `loadBoundedManifestStateInTransaction`, `walkRebuiltSpineBounded`, `persistLocallyRebuilt`, and `buildCandidate`. Supporting path/copy logic is in `packages/fs/src/operations/bounded-local-rebuild.ts` (`boundedPathAtOffset`, `assembleBoundedManifestState`, `regroupLevelBounded`, `rebuildManifestBoundedOwned`). Source preparation is in `packages/fs/src/operations/streaming-prepare.ts`; Node-only wrappers are in `packages/node-vfs/src/index.ts`, `packages/fs/src/operations/filesystem.ts`, and `packages/fs/src/operations/node-vfs-bridge.ts`. The portable algorithm is authenticated source-window descent plus local changed-spine rebuild; Node VFS/session wrappers are not algorithm requirements. |
| Fairness/durability | Both lanes require one logical operation per durable SQLite transaction, identical metadata/root/manifest semantics, `WAL` plus `synchronous=FULL`, and equal read-after-write/close-reopen verification. The payload SQL cannot be literally identical: R-SQLite inserts a BLOB, while R-HYBRID inserts a carrier reference. Fairness is equal logical work and durability with the payload statement class disclosed. |
| Benchmark/space plan | Use identical fixtures, M7 parameters, repetitions, timing boundaries, and filesystem policy. Report medians plus individual samples. Separate database, WAL, SHM, carrier, temporary, live, obsolete, steady-state, post-compaction, and sampled peak bytes. Attribute source, CDC/hash, admission, tree, payload, SQLite metadata, carrier append, fsync, commit, checkpoint, close/reopen, and verification phases. Correctness/crash gates precede performance interpretation. |

### Synthesized implementation plan

1. Reproduce latest M7-shaped behavior: FastCDC `(32 KiB, 128 KiB, 512 KiB)`,
   leaf grouping `(64, 128, 256)`, internal grouping `(32, 64, 128)`,
   SHA-256 object/node/root identity, bounded authenticated source windows,
   changed-spine rebuild, unchanged-object identity reuse, and the same
   namespace/manifest/root publication model.
2. R-SQLite is that Rust algorithm with payload bytes in
   `efs_cas_objects.bytes`.
3. R-HYBRID is the same algorithm and metadata, with payload references
   `(hash, size, carrier_id, carrier_offset, carrier_length,
   carrier_checksum, allocation_sequence)` and a carrier catalog containing
   carrier ID/path/bytes/digest/record count/format version.
4. Hold constant fixtures, chunking, hashes, identity, manifest/root
   construction, namespace semantics, PRAGMAs, logical transactions,
   close/reopen verification, filesystem policy, and alternating run order.
5. Intentionally change only payload placement and its durability work:
   SQLite BLOB insertion versus deterministic carrier staging, carrier fsync,
   and SQLite reference insertion.
6. A real win requires repeatable large-write or materialization improvement,
   or materially lower steady-state storage, without unacceptable read/edit
   regressions. Peak space is always reported separately.
7. Space inefficiency is shown by no steady-state reduction, accumulating
   obsolete carrier bytes, or higher peak bytes without a compelling benefit.
8. No speed result is accepted unless both lanes pass all digest/root/manifest,
   materialization/random-read, SQLite/WAL, crash, and orphan checks.

### Fairness and durability contract

Both lanes use foreign keys enabled, `temp_store=FILE`, `journal_mode=WAL`,
`synchronous=FULL`, 4 KiB pages, `mmap_size=0`, `cache_size=-65536`, and
`wal_autocheckpoint=0`. Each create or one-byte edit uses one durable SQLite
transaction; the three-edit workload uses one transaction per logical edit.

The Hybrid protocol is: sort objects deterministically; write `LFCR` records
with version/header/length/hash/type fields; hash the complete carrier for its
ID; flush and fsync the temporary carrier; atomically rename it and fsync the
directory; begin SQLite; insert carrier/reference metadata; commit with FULL;
then close/reopen and perform common full verification. Missing, truncated,
out-of-bounds, or hash-mismatched references fail verification. Carrier bytes
written after carrier fsync but before an uncommitted SQLite transaction are
orphans and are detected/quarantined. A crash before carrier fsync cannot
publish a committed reference because SQLite starts afterward. A crash after
SQLite commit must recover a complete authenticated old/new root.

The removed pre-commit carrier reread was redundant and unfair: it repeated a
full carrier scan that R-SQLite did not perform. Carrier records remain
self-validating on read, and full carrier structural/digest validation occurs
at close/reopen. The standalone malformed-carrier check remains.

### Corrected benchmark matrix and individual samples

The runner performs one warmup followed by five retained samples per cell. The
lane order alternates per sample and each child is serialized. The `read` cold
label means a newly reopened APFS namespace, not an OS-cache eviction; warm is
a same-process repeat.

| Workload | SQLite samples | Hybrid samples | SQLite median | Hybrid median | Hybrid delta |
|---|---:|---:|---:|---:|---:|
| Create 1 MiB | 49.781, 46.601, 50.632, 48.447, 48.666 ms | 71.369, 66.328, 69.799, 68.282, 71.390 ms | 48.666 ms | 69.799 ms | +43.425% |
| Create 10 MiB | 397.051, 399.816, 399.916, 398.853, 398.909 ms | 508.852, 506.238, 513.456, 505.210, 507.581 ms | 398.909 ms | 507.581 ms | +27.242% |
| Create 100 MiB | 4057.853, 4080.112, 4078.532, 4097.729, 4036.243 ms | 4844.195, 4875.237, 4868.309, 4889.867, 4873.460 ms | 4078.532 ms | 4873.460 ms | +19.491% |
| One-byte edit, 1 MiB | 25.646, 27.593, 27.506, 24.999, 26.842 ms | 41.650, 41.420, 41.018, 42.693, 41.278 ms | 26.842 ms | 41.420 ms | +54.307% |
| One-byte edit, 10 MiB | 168.243, 168.139, 166.770, 168.293, 167.821 ms | 226.008, 226.868, 226.066, 224.240, 225.047 ms | 168.139 ms | 226.008 ms | +34.417% |
| One-byte edit, 100 MiB | 1573.096, 1577.792, 1578.302, 1566.411, 1570.878 ms | 2005.206, 2006.097, 2004.849, 1989.805, 2008.034 ms | 1573.096 ms | 2005.206 ms | +27.469% |
| Three one-byte M7-bounded edits, 100 MiB | 4698.819, 4689.703, 4705.959, 4732.942, 4754.461 ms | 6132.113, 5994.321, 6046.369, 6012.868, 6051.972 ms | 4705.959 ms | 6046.369 ms | +28.483% |
| 100 MiB read, reopened/APFS-direct | 1193.495, 1187.204, 1177.262, 1173.540, 1203.928 ms | 1002.074, 1007.354, 1006.606, 1001.796, 1028.023 ms | 1187.204 ms | 1006.606 ms | 15.212% faster |
| 100 MiB read, warm repeat | 1217.331, 1190.791, 1176.439, 1190.849, 1178.399 ms | 1013.416, 1012.633, 1009.688, 1000.660, 1015.077 ms | 1190.791 ms | 1012.633 ms | 14.961% faster |
| 1000 random 4 KiB reads | 905.790, 920.662, 905.526, 905.662, 908.393 µs/read | 660.323, 654.999, 639.531, 644.235, 658.407 µs/read | 905.790 µs | 654.999 µs | 27.688% faster |
| One 100 MiB materialization | 901.671, 902.425, 891.166, 917.020, 895.142 ms | 751.440, 706.101, 710.750, 713.584, 724.954 ms | 901.671 ms | 713.584 ms | 20.860% faster |
| 100 × 1 MiB materialization, per-file median | 12.024, 12.086, 12.144, 12.122, 12.299 ms | 11.943, 12.027, 12.013, 11.984, 11.880 ms | 12.122 ms | 11.984 ms | 1.142% faster |

### Timing breakdowns

These are medians of five retained samples. Create timing includes source,
storage, close/reopen verification, and checkpoint. Edit top-level elapsed is
the edit interval after the base was created and verified; the phase timing
fields also retain the base/setup fields for attribution. Close/reopen includes
SQLite integrity, manifest traversal, payload authentication, and Hybrid
carrier validation.

| Workload / lane | Source | CDC/hash | Admission | Tree | Carrier write | Carrier append | Carrier fsync | SQLite metadata | SQLite commit | Close/reopen + verify |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| Create 1 MiB / SQLite | 1.040 | 3.927 | 0.156 | 0.009 | 0 | 0 | 0 | 0.696 | 1.777 | 16.411 |
| Create 1 MiB / Hybrid | 1.017 | 3.897 | 0.161 | 0.009 | 0.110 | 3.761 | 8.731 | 0.235 | 0.345 | 19.864 |
| Create 10 MiB / SQLite | 10.151 | 39.215 | 1.314 | 0.067 | 0 | 0 | 0 | 7.292 | 16.122 | 135.851 |
| Create 10 MiB / Hybrid | 10.164 | 39.335 | 1.313 | 0.064 | 1.871 | 35.186 | 11.016 | 0.803 | 0.271 | 169.451 |
| Create 100 MiB / SQLite | 101.524 | 394.119 | 13.403 | 0.518 | 0 | 0 | 0 | 136.096 | 128.937 | 1393.091 |
| Create 100 MiB / Hybrid | 101.589 | 393.504 | 13.011 | 0.492 | 18.409 | 346.693 | 36.377 | 6.450 | 0.702 | 1664.682 |
| One-byte edit 100 MiB / SQLite | 101.539 | 395.192 | 13.847 | 0.501 | 0 | 0 | 0 | 137.013 | 132.290 | 2632.398 |
| One-byte edit 100 MiB / Hybrid | 101.844 | 394.091 | 13.030 | 0.496 | 17.963 | 345.176 | 41.264 | 7.970 | 2.657 | 3322.004 |
| Three edits 100 MiB / SQLite | 101.889 | 394.537 | 14.973 | 0.508 | 0 | 0 | 0 | 141.502 | 136.682 | 5105.368 |
| Three edits 100 MiB / Hybrid | 102.218 | 396.489 | 13.186 | 0.489 | 18.462 | 349.806 | 66.178 | 9.928 | 5.891 | 6696.485 |

The shared algorithmic phases are effectively equal: 100 MiB source read is
about 101.5–102.2 ms and CDC/hash is about 393.5–396.5 ms. Hybrid removes
large BLOB/WAL SQLite metadata and commit cost, but adds carrier write/append/
fsync and a larger close/reopen verification path. The earlier ~670 ms Hybrid
metadata values were the removed duplicate scan and are not retained results.

### Space usage

All values are bytes. WAL is measured before close/checkpoint and after the
explicit `wal_checkpoint(TRUNCATE)`. Temporary bytes are zero in the recorded
snapshots because the carrier temporary is renamed before those snapshots; the
sampled peak is therefore a protocol-boundary measurement, not a continuous
kernel-level disk trace.

| Workload / lane | DB after commit | WAL after commit | SHM after commit | Carrier | Steady DB | Steady total | Peak total |
|---|---:|---:|---:|---:|---:|---:|---:|
| Create 1 MiB / SQLite | 4,096 | 1,227,792 | 32,768 | 0 | 1,130,496 | 1,163,264 | 1,264,656 |
| Create 1 MiB / Hybrid | 4,096 | 177,192 | 32,768 | 1,049,024 | 73,728 | 1,155,520 | 1,263,080 |
| Create 10 MiB / SQLite | 4,096 | 10,835,632 | 32,768 | 0 | 10,682,368 | 10,715,136 | 10,872,496 |
| Create 10 MiB / Hybrid | 4,096 | 189,552 | 32,768 | 10,489,624 | 86,016 | 10,608,408 | 10,716,040 |
| Create 100 MiB / SQLite | 4,096 | 107,021,152 | 229,376 | 0 | 106,307,584 | 106,340,352 | 107,254,624 |
| Create 100 MiB / Hybrid | 4,096 | 391,432 | 32,768 | 104,895,512 | 286,720 | 105,215,000 | 105,323,808 |
| One-byte edit 100 MiB / SQLite | — | — | 32,768 | 0 | 106,516,480 | 106,549,248 | 107,254,624 |
| One-byte edit 100 MiB / Hybrid | — | — | 32,768 | 105,053,788 | 335,872 | 105,422,428 | 105,480,428 |
| Three edits 100 MiB / SQLite | — | — | 32,768 | 0 | 106,979,328 | 107,012,096 | 107,254,624 |
| Three edits 100 MiB / Hybrid | — | — | 32,768 | 105,408,722 | 430,080 | 105,871,570 | 105,929,546 |

Hybrid steady 100 MiB create storage is about 1.06% lower overall and the
SQLite database itself is about 99.7% smaller, but the bytes move into a
104.9 MiB carrier. Compaction passed but did not reduce space for this
single-carrier create:

| Hybrid create size | Median compaction | Post-compaction total | Obsolete/unreferenced carrier bytes |
|---:|---:|---:|---:|
| 1 MiB | 20.276 ms | 1,155,520 | 0 |
| 10 MiB | 119.489 ms | 10,608,408 | 0 |
| 100 MiB | 1,094.612 ms | 105,215,000 | 0 |

### SQL, transaction, and statement counts

Shared logical statement counts are the same. Hybrid adds one carrier catalog
row per new carrier batch. The payload operation is intentionally different:
R-SQLite binds a BLOB; R-HYBRID binds carrier-reference metadata.

| Workload | Lane order SQLite / Hybrid | Statements | Rows read | Rows inserted | Source reads / bytes | Object reads | Transactions / commits |
|---|---|---:|---:|---:|---:|---:|---:|
| Create 1 MiB | SQLite / Hybrid | 39 / 39 | 20 / 20 | 11 / 12 | 0 / 0 | 0 / 0 | 1/1 / 1/1 |
| Create 10 MiB | SQLite / Hybrid | 283 / 283 | 142 / 142 | 72 / 73 | 0 / 0 | 0 / 0 | 1/1 / 1/1 |
| Create 100 MiB | SQLite / Hybrid | 2,731 / 2,731 | 1,366 / 1,366 | 688 / 689 | 0 / 0 | 0 / 0 | 1/1 / 1/1 |
| One-byte edit 1 MiB | SQLite / Hybrid | 24 / 24 | 19 / 19 | 4 / 5 | 1 / 707,694 | 5 / 5 | 1/1 / 1/1 |
| One-byte edit 10 MiB | SQLite / Hybrid | 93 / 93 | 88 / 88 | 4 / 5 | 1 / 2,097,152 | 13 / 13 | 1/1 / 1/1 |
| One-byte edit 100 MiB | SQLite / Hybrid | 711 / 711 | 701 / 701 | 6 / 7 | 1 / 2,097,152 | 13 / 13 | 1/1 / 1/1 |
| Three edits 100 MiB | SQLite / Hybrid | 2,122 / 2,122 | 2,092 / 2,092 | 18 / 21 | 3 / 4,371,981 | 30 / 30 | 3/3 / 3/3 |
| 100 MiB read | SQLite / Hybrid | 2,392 / 2,392 | 2,392 / 2,392 | 0 / 0 | 1,000 / 4,096,000 | 1,026 / 1,026 | 0/0 / 0/0 |
| 100 MiB materialization | SQLite / Hybrid | 6 / 6 | 6 / 6 | 0 / 0 | 0 / 0 | 677 / 677 | 0/0 / 0/0 |
| 100 × 1 MiB materialization | SQLite / Hybrid | 200 / 200 | 200 / 200 | 0 / 0 | 0 / 0 | 800 / 800 | 0/0 / 0/0 |

### Correctness and crash-recovery results

Every retained benchmark child returned `phase2=pass`. The carrier preflight
reported bounds verified, payload hash verified, carrier format pass, and
truncated record rejected. Both lanes passed self-check, read-after-write,
close/reopen, same logical byte count/digest/object count, manifest/root,
changed/unchanged object checks, materialized output bytes, random-read bytes,
SQLite integrity, and WAL/checkpoint checks.

| Gate | R-SQLite | R-HYBRID |
|---|---|---|
| Carrier bounds/format/hash self-check | Pass | Pass |
| Full self-check/read-after-write | Pass | Pass |
| Close/reopen/full digest/root/manifest | Pass | Pass |
| Materialized bytes/random-read bytes | Pass | Pass |
| SQLite integrity | Pass | Pass |
| WAL checkpoint | `busy=0`, zero residual frames | `busy=0`, zero residual frames |
| Crash before commit | Pass, child status 91, old root | Pass, child status 91, old root |
| Crash after commit | Pass, child status 92, new root | Pass, child status 92, new root |
| Orphan detection | Pass; none created | Pass; one pre-commit orphan |
| Orphan quarantine/cleanup | Pass | Pass; one carrier quarantined |
| Recovery counts | Baseline `[8,1,1]`, after `[9,2,2]` | Baseline `[8,1,1]`, after `[9,2,2]` |

The crash probe is a process-exit simulation rather than a power-loss injector,
but it exercises the required ordering and SQLite rollback/commit state.

### Memory safety correction and remaining footprint

The computer-killing run was traced before rerunning. The parent recovery
routine passed a byte count (`1,048,576`) to a child CLI argument interpreted
as MiB, so the child attempted roughly 1 TiB of fixture allocation. This was a
unit error, not an accumulating leak. The parent now passes MiB and all Phase
2 size entry points reject values above 100 MiB before allocation.

The corrected 100 MiB create child, measured with `/usr/bin/time -l`, used:

| Lane | Maximum resident set size | Swaps |
|---|---:|---:|
| R-SQLite | 422,100,992 bytes | 0 |
| R-HYBRID | 543,817,728 bytes | 0 |

This is a bounded per-child footprint, not a leak test. It remains high because
the minimal Rust harness retains source, chunk/object vectors, carrier encoding,
and database-side buffers at overlapping points. The runner is serialized and
the corrected campaign completed without swaps or host instability. A
production-scale implementation should stream carrier records and avoid these
overlapping copies before testing substantially larger than 100 MiB.

### Files changed and verification commands

Phase 2 changes are isolated to the experiment tree:

- `experiments/m8-rust-port/src/main.rs` — paired lanes, carrier format and
  durability protocol, crash recovery, counters, safety guard, and fairness
  fix.
- `experiments/sqlite-techstack/phase2.mjs` — serialized matrix runner,
  correctness preflight, raw JSONL, and summary output.
- `experiments/sqlite-techstack/results/phase2.jsonl` — corrected samples.
- `experiments/sqlite-techstack/results/phase2-summary.json` — corrected
  medians and gates.
- `experiments/sqlite-techstack/experiment.md` — this appended Phase 2 section;
  earlier results were preserved.

Verification passed with:

```text
cargo fmt
cargo check
cargo build --release -q
./target/release/layerfs-m8-rust phase2-carrier-check
./target/release/layerfs-m8-rust phase2-recovery sqlite 1
./target/release/layerfs-m8-rust phase2-recovery hybrid 1
PHASE2_REPS=6 node experiments/sqlite-techstack/phase2.mjs
```

The malformed probe `phase2 phase2_dummy` was rejected immediately with
`unknown storage mode`; it did not allocate a workload and was not included.

### Compatibility limitations

This is an isolated storage experiment, not a production LayerFS change. The
Rust port reproduces the latest M7 parameters and bounded source-window/path-
copy shape, but it is not a byte-for-byte port of production
`rebuildManifestBoundedOwned`/`regroupLevelBounded` staging and reconciliation.
The exact Node implementation remains the behavioral oracle. The decision is
about this paired Rust storage experiment, not a claim that production Node
should be changed.

Other limits: the cold-read label does not evict APFS caches; the fixture is
deterministic and capped at 100 MiB; sampled peak disk bytes are boundary
samples rather than continuous accounting; no true power-loss injector was
used; no CPU profiler was attached; no 100-sequential or 500-scattered-edit
workload was included; and automatic carrier compaction was not added to every
edit because that would change the requested primary transaction workload.

### Decision

**Defer R-HYBRID for this workload.**

The corrected evidence is mixed:

- Correctness and crash recovery pass in both lanes.
- R-HYBRID is 15.2% faster on reopened 100 MiB read, 15.0% faster on warm
  repeat, 27.7% faster on 4 KiB random reads, and 20.9% faster on one 100 MiB
  materialization. The 100 × 1 MiB result is only 1.1% faster.
- R-HYBRID reduces steady 100 MiB create storage by about 1.06% overall and
  reduces SQLite DB bytes by about 99.7%, but moves the payload into a carrier
  and adds compaction/lifecycle work.
- R-HYBRID is slower on create by 19.5% at 100 MiB and 27.2–43.4% at 1–10
  MiB. It is slower on one-byte edits by 27.5–54.3% and on the three-edit M7
  workload by 28.5%, exceeding the requested 10% small-edit limit.
- The corrected large-write result does not meet the requested 20% improvement
  criterion. The materialization result meets that positive criterion, but not
  without the unacceptable create/edit regressions.

R-HYBRID should be reconsidered only after a streaming carrier writer and a
lower-overhead bounded-edit publication path are implemented and measured with
the same gates. This Phase 2 result does not justify adopting it in production
LayerFS.

## Phase 2 final correction — actual M7 local-spine port

The preceding Phase 2 section is preserved as historical experiment output.
Its edit/read numbers were from the intermediate M7-shaped port and must not be
used as the final M7 comparison. This section is the final result after the
Rust Phase 2 entry point was changed to use the actual bounded local-spine
behavior and the full campaign was rerun.

### Phase 1 versus M7

The Phase 1 Rust control in
`experiments/sqlite-techstack/rust/src/main.rs` is an older flat R-SQLite
harness: FastCDC `(8 KiB, 16 KiB, 32 KiB)`, flat root/member persistence, and a
full source scan plus larger root/member rebuild for an edit. Its recorded
R-SQL durable Create rose from `6.812 ms` at 1 MiB to `459.173 ms` at 100 MiB;
its frozen 32-byte edit rose from `7.799 ms` to `495.049 ms`. Phase 1 therefore
cannot be the control for this comparison.

The final Phase 2 port uses the latest M7-shaped Rust algorithm in both lanes:
FastCDC `(32 KiB, 128 KiB, 512 KiB)`, leaf grouping `(64, 128, 256),` internal
grouping `(32, 64, 128)`, SHA-256 identity, authenticated source-window
descent, bounded leaf regrouping, path-copy of the changed manifest spine, and
unchanged subtree/object reuse. The implementation is in
`experiments/m8-rust-port/src/main.rs`; the M7 behavioral oracle remains
`experiments/m8-rust-port/node.mjs` against the production checkout.

The production reference inspected was not modified:

| Item | Exact value |
|---|---|
| LayerFS checkout | `/Users/yifanxu/Ephemeral-AI-Lab/layerfs-m8-verify` |
| LayerFS checkout HEAD used | `8c1c55785ab25d0435d47e32e35a1a7bdf1fbf6d` |
| M7 core source commit | `ce9035e49037f60a8c52d2775fd2d88d34e57cd4` |
| M7 reference functions | `boundedPathAtOffset`, `regroupLevelBounded`, `buildBoundedManifestState`, `rebuildBoundedSpine`, `rebuildManifestBoundedOwned` |
| Rust implementation commit | `f46d25e3ddf9a4c2270670819203708f941dae94` |
| Prior Phase 2 base commit | `c754be54199e05f24dc63e1f4e9a209c8a175e75` |
| Production checkout status | Clean; no production files changed |

The Rust correctness path also has an opt-in canonical oracle check:

```text
PHASE2_M7_CANONICAL_CHECK=1 \
  experiments/m8-rust-port/target/release/layerfs-m8-rust \
  phase2 sqlite edit1 100
```

That check passed for both R-SQLite and R-HYBRID at 1, 10, and 100 MiB. The
bounded rebuild root matched a fresh canonical rebuild in each case.

### Final campaign custody

The final campaign used one warm-up and five retained samples per cell, six
samples per cell total, alternating lane order, and one serialized child at a
time. It completed 120 benchmark children. The first attempted rerun was
invalidated when the optimized random-read path requested only 4 KiB from the
chunk start and then sliced at a nonzero offset; the run stopped at the first
read cell, the window length was fixed, both standalone read gates passed, and
the 120-child campaign below completed with all gates passing.

The final experiment documentation commit is recorded in the handoff after
this section is committed. The raw and summary files are:

```text
experiments/sqlite-techstack/results/phase2.jsonl
experiments/sqlite-techstack/results/phase2-summary.json
```

### Final R-SQLite versus R-HYBRID benchmark

Times are milliseconds unless the unit is shown. Each sample list is the five
retained samples after the discarded warm-up. “Hybrid delta” is
`(R-HYBRID / R-SQLite - 1) * 100`; a negative value means Hybrid was faster.

| Workload | R-SQLite retained samples | R-HYBRID retained samples | R-SQLite median | R-HYBRID median | Hybrid delta |
|---|---:|---:|---:|---:|---:|
| Create 1 MiB | 50.616, 50.239, 50.739, 48.769, 49.479 | 69.824, 69.797, 71.929, 69.846, 71.282 | 50.239 | 69.846 | +39.03% |
| Create 10 MiB | 406.766, 411.404, 399.916, 421.950, 400.381 | 511.003, 512.550, 520.981, 515.618, 506.201 | 406.766 | 512.550 | +26.01% |
| Create 100 MiB | 4014.832, 4042.427, 4108.275, 4101.408, 4021.705 | 4852.496, 4876.032, 4908.262, 4929.437, 4868.248 | 4042.427 | 4876.032 | +20.62% |
| One-byte edit 1 MiB | 25.699, 26.749, 26.097, 26.607, 26.004 | 41.779, 42.721, 41.339, 42.767, 43.311 | 26.097 | 42.721 | +63.70% |
| One-byte edit 10 MiB | 171.816, 168.876, 167.670, 166.916, 166.678 | 223.558, 232.165, 222.123, 224.933, 224.285 | 167.670 | 224.285 | +33.77% |
| One-byte edit 100 MiB | 1578.009, 1578.414, 1581.807, 1577.523, 1577.486 | 2007.646, 1997.165, 2008.348, 2000.706, 2030.536 | 1578.009 | 2007.646 | +27.23% |
| Three one-byte M7-bounded edits, 100 MiB | 4699.911, 4742.947, 4782.589, 4706.699, 4738.140 | 5964.840, 5982.805, 6128.720, 6015.556, 6008.553 | 4738.140 | 6008.553 | +26.81% |
| Cold 100 MiB read | 1187.086, 1175.480, 1204.687, 1193.564, 1178.689 | 1005.417, 994.562, 998.203, 994.578, 996.184 | 1187.086 | 996.184 | -16.08% |
| Warm 100 MiB read | 1187.563, 1174.977, 1194.863, 1186.332, 1184.377 | 996.412, 1000.901, 994.034, 1001.064, 997.529 | 1186.332 | 997.529 | -15.91% |
| 1000 random 4 KiB reads, µs/read | 923.909, 916.730, 934.144, 918.182, 925.322 | 648.887, 649.424, 653.437, 650.749, 652.368 | 923.909 | 650.749 | -29.57% |
| One 100 MiB materialization | 899.055, 893.741, 897.743, 900.708, 889.686 | 706.621, 709.280, 711.769, 711.906, 711.926 | 897.743 | 711.769 | -20.72% |
| 100 × 1 MiB materialization, per-file median | 12.093, 12.092, 12.095, 12.109, 12.122 | 11.917, 11.993, 11.873, 11.943, 12.001 | 12.095 | 11.943 | -1.26% |

The 100 MiB create result is slower by 20.62%, not faster by 20%; this is a
regression against the expected decision threshold. Materialization is faster
by 20.72%, but the create and edit regressions remain unacceptable for the
requested adoption rule.

### What M7 bounded editing does and does not make constant

M7 bounds the edit preparation, not every operation required by this harness.
The final 100 MiB one-byte edit metrics were:

| Lane | Authenticated source window | Loaded entries | Loaded nodes | New manifest nodes | Reused manifest nodes |
|---|---:|---:|---:|---:|---:|
| R-SQLite | 417,360 bytes | 16 | 2 | 2 | 5 |
| R-HYBRID | 417,360 bytes | 16 | 2 | 2 | 5 |

The 1 MiB and 10 MiB windows were 513,022 and 370,954 bytes, respectively;
the local rebuild emitted one new manifest node at those sizes. The three-edit
100 MiB workload also emitted only two new nodes per edit and retained the
unchanged upper subtrees. These are the M7 locality signals.

The end-to-end edit timer intentionally includes the durable store, close,
reopen, SQLite integrity check, carrier validation, and full materialized
digest verification. That verification walks the logical file, so the recorded
one-byte edit still scales with file size: `26.097 → 167.670 → 1578.009 ms`
for R-SQLite and `42.721 → 224.285 → 2007.646 ms` for R-HYBRID at 1, 10, and
100 MiB. The result is therefore not a constant-time end-to-end claim. M7
bounded preparation is local; the required full verification boundary is
linear in this experiment.

### Timing breakdowns

These are medians of the five retained samples. Close/reopen and full
verification are separate columns; both remain inside the end-to-end result.
The phase fields include the base setup fields because the minimal harness
records common lifecycle phases in one timing object, but the workload elapsed
column above is the operation boundary used for comparison.

| Workload / lane | Source | CDC/hash | Tree/member | Carrier write / append / fsync | SQLite metadata | SQLite commit | Close/reopen | Full verification |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| Create 100 MiB / R-SQLite | 101.430 | 393.302 | 0.493 | 0 / 0 / 0 | 134.020 | 124.238 | 165.590 | 1226.473 |
| Create 100 MiB / R-HYBRID | 101.444 | 394.386 | 0.487 | 17.602 / 347.191 / 30.996 | 6.346 | 0.711 | 5.601 | 1659.024 |
| One-byte edit 100 MiB / R-SQLite | 101.586 | 396.108 | 0.735 | 0 / 0 / 0 | 136.660 | 131.166 | 174.378 | 2464.375 |
| One-byte edit 100 MiB / R-HYBRID | 101.652 | 396.155 | 0.732 | 18.965 / 347.461 / 37.650 | 8.095 | 2.440 | 8.822 | 3321.572 |
| Three edits 100 MiB / R-SQLite | 101.073 | 397.596 | 1.015 | 0 / 0 / 0 | 138.471 | 136.750 | 190.086 | 4949.415 |
| Three edits 100 MiB / R-HYBRID | 101.152 | 398.801 | 1.015 | 18.572 / 349.810 / 65.300 | 9.341 | 5.907 | 15.580 | 6637.383 |
| Read 100 MiB / R-SQLite | 101.544 | 395.645 | 0.489 | 0 / 0 / 0 | 134.956 | 128.216 | 167.225 | 1236.011 |
| Read 100 MiB / R-HYBRID | 101.020 | 394.374 | 0.484 | 18.625 / 347.780 / 28.720 | 6.385 | 0.793 | 5.197 | 1656.457 |
| Materialize 100 MiB / R-SQLite | 101.404 | 395.112 | 0.484 | 0 / 0 / 0 | 134.885 | 127.439 | 168.330 | 1231.420 |
| Materialize 100 MiB / R-HYBRID | 101.593 | 394.937 | 0.481 | 19.288 / 346.874 / 33.082 | 6.272 | 0.774 | 5.764 | 1657.643 |

The shared source, CDC, hash, and tree work is effectively equal. Hybrid trades
SQLite BLOB/WAL metadata work for carrier append and fsync. It is faster on
read/materialization because payload reads avoid SQLite BLOB pages, but the
write-side carrier durability and full verification costs remain visible.

### Space usage

All values are bytes. The create rows include snapshots immediately after the
durable commit, after reopen, after the explicit WAL checkpoint, and at steady
state. Temporary bytes were zero at the recorded snapshots because the carrier
temporary is renamed before the snapshot. Peak is the largest sampled directory
total during the child. The post-compaction create total equals the steady
total for these one-carrier cases; compaction still passed and is listed
separately below.

| Workload / lane | DB after commit | WAL after commit | SHM after commit | Carrier after commit | Total after commit | Steady DB | Steady carrier | Steady total | Peak total |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| Create 1 MiB / R-SQLite | 4,096 | 1,227,792 | 32,768 | 0 | 1,264,656 | 1,130,496 | 0 | 1,163,264 | 1,264,656 |
| Create 1 MiB / R-HYBRID | 4,096 | 177,192 | 32,768 | 1,049,024 | 1,263,080 | 73,728 | 1,049,024 | 1,155,520 | 1,263,080 |
| Create 10 MiB / R-SQLite | 4,096 | 10,835,632 | 32,768 | 0 | 10,872,496 | 10,682,368 | 0 | 10,715,136 | 10,872,496 |
| Create 10 MiB / R-HYBRID | 4,096 | 189,552 | 32,768 | 10,489,624 | 10,716,040 | 86,016 | 10,489,624 | 10,608,408 | 10,716,040 |
| Create 100 MiB / R-SQLite | 4,096 | 107,021,152 | 229,376 | 0 | 107,254,624 | 106,307,584 | 0 | 106,340,352 | 107,254,624 |
| Create 100 MiB / R-HYBRID | 4,096 | 391,432 | 32,768 | 104,895,512 | 105,323,808 | 286,720 | 104,895,512 | 105,215,000 | 105,323,808 |
| One-byte edit 100 MiB / R-SQLite | not snapshotted | not snapshotted | 32,768 | 0 | not snapshotted | 106,516,480 | 0 | 106,549,248 | 107,254,624 |
| One-byte edit 100 MiB / R-HYBRID | not snapshotted | not snapshotted | 32,768 | 105,053,788 | not snapshotted | 335,872 | 105,053,788 | 105,422,428 | 105,480,428 |
| Three edits 100 MiB / R-SQLite | not snapshotted | not snapshotted | 32,768 | 0 | not snapshotted | 106,979,328 | 0 | 107,012,096 | 107,254,624 |
| Three edits 100 MiB / R-HYBRID | not snapshotted | not snapshotted | 32,768 | 105,408,722 | not snapshotted | 430,080 | 105,408,722 | 105,871,570 | 105,929,546 |

The edit path records final steady state and sampled peak, but does not expose a
separate pre-close WAL snapshot in its result object; those four fields are
explicitly marked `not snapshotted`, not inferred. The create path provides the
publication-boundary WAL measurement. This is a reporting limitation, not a
correctness failure.

| R-HYBRID create size | Median compaction | Post-compaction total | Obsolete/unreferenced carrier bytes |
|---:|---:|---:|---:|
| 1 MiB | 20.474 ms | 1,155,520 | 0 |
| 10 MiB | 118.111 ms | 10,608,408 | 0 |
| 100 MiB | 1,088.989 ms | 105,215,000 | 0 |

At 100 MiB, Hybrid reduces the SQLite database from 106,307,584 to 286,720
bytes, but moves 104,895,512 bytes into the carrier. Total steady-state bytes
fall only from 106,340,352 to 105,215,000, about 1.06%; this is not a large
overall space win. The live payload byte count is equal between lanes.

### SQL, transactions, and statement counts

The logical SQL work is held constant. The payload operation is intentionally
different and is reported as such: R-SQLite inserts `efs_cas_objects.bytes`,
while R-HYBRID appends a carrier record and inserts the carrier reference
columns. Hybrid also inserts one carrier catalog row per carrier batch. Counts
below are medians over retained samples; create uses the primary operation
counter before optional Hybrid compaction, and edit uses operation counters.

| Workload | Lane | Statements | Rows read | Rows inserted | Transactions / commits | Payload bytes / carrier bytes |
|---|---|---:|---:|---:|---:|---:|
| Create 100 MiB | R-SQLite | 2,731 | 1,366 | 688 | 1 / 1 | 104,857,600 / 0 |
| Create 100 MiB | R-HYBRID | 2,731 | 1,366 | 689 | 1 / 1 | 0 in SQLite / 104,895,512 carrier |
| One-byte edit 100 MiB | R-SQLite | 713 | 706 | 6 | 1 / 1 | changed BLOB in SQLite |
| One-byte edit 100 MiB | R-HYBRID | 713 | 706 | 7 | 1 / 1 | changed record in carrier |
| Three edits 100 MiB | R-SQLite | 2,134 | 2,113 | 18 | 3 / 3 | changed BLOBs in SQLite |
| Three edits 100 MiB | R-HYBRID | 2,134 | 2,113 | 21 | 3 / 3 | changed records in carrier |
| Read 100 MiB | R-SQLite / R-HYBRID | 2,392 | 2,392 | 0 | 0 / 0 | 92,832,100 source-window bytes |
| Materialize 100 MiB | R-SQLite / R-HYBRID | 6 | 6 | 0 | 0 / 0 | 677 object reads |
| 100 × 1 MiB materialization | R-SQLite / R-HYBRID | 200 | 200 | 0 | 0 / 0 | 800 object reads |

This is equal logical work and equal durability, not byte-for-byte identical
payload SQL. Both lanes use the same WAL/FULL PRAGMAs, one durable transaction
per logical create/edit, the same object/root/manifest statements wherever the
storage layout permits, and the same close/reopen verification boundary.

### Correctness and crash recovery

No performance cell was accepted without the gates below. The final campaign
reported `phase2=pass` for every retained child.

| Gate | R-SQLite | R-HYBRID |
|---|---|---|
| Full self-check and read-after-write | Pass | Pass |
| Canonical M7 root equivalence at 1/10/100 MiB | Pass | Pass |
| Same logical byte count and content digest | Pass | Pass |
| Same object count, manifest, and Merkle/root digest | Pass | Pass |
| Same changed-object set and unchanged identities | Pass | Pass |
| Materialized output bytes | Pass | Pass |
| Random-read bytes | Pass | Pass |
| Carrier bounds/format/hash check | N/A | Pass; truncated record rejected |
| SQLite integrity check | Pass | Pass |
| WAL checkpoint | Pass; `busy=0`, no residual frames | Pass; `busy=0`, no residual frames |
| Crash before commit | Pass; old root recovered | Pass; old root recovered and carrier orphan detected |
| Crash after commit | Pass; new root recovered | Pass; new root and referenced carrier recovered |
| Orphan quarantine/cleanup | Pass | Pass; pre-commit carrier quarantined |

The Hybrid durability order is carrier record write, flush, carrier `fsync`,
temporary rename and directory `fsync`, SQLite reference transaction, FULL
SQLite commit, then the common verification boundary. Carrier bytes that exist
without a committed reference are orphans and are detected/quarantined;
references to missing, truncated, out-of-bounds, or hash-mismatched bytes fail.

### Memory and known limitations

The earlier computer-killing run was a unit error: a parent passed `1,048,576`
bytes where the child CLI expected MiB, leading to a roughly 1 TiB allocation
attempt. The corrected entry points reject sizes above 100 MiB, and all final
campaign children were serialized. This fixed the immediate failure; it is not
a claim that the minimal harness has production-scale streaming memory use.

The final benchmark still retains source/object/manifest structures in memory,
uses a sampled rather than continuous peak-disk observer, uses APFS namespace
reopen rather than an OS page-cache eviction for “cold” reads, and uses process
exit simulation rather than a power-loss injector. The carrier compaction test
is run for create and is not inserted into the primary edit timing because
doing so would change the requested transaction workload. The edit pre-close
WAL snapshot is not yet exposed in the result schema, as marked above.

### Final decision

**Defer R-HYBRID for this workload.**

The final evidence is mixed but does not satisfy the adoption rule:

- Correctness, M7 canonical equivalence, carrier validation, WAL checks, and
  crash recovery all pass.
- R-HYBRID is faster on cold and warm 100 MiB reads by 16.08% and 15.91%, on
  random 4 KiB reads by 29.57%, and on one 100 MiB materialization by 20.72%.
  The 100 × 1 MiB materialization improvement is only 1.26%.
- R-HYBRID is slower on create by 39.03%, 26.01%, and 20.62% at 1/10/100 MiB;
  the 100 MiB result is a regression, not the required large-write gain.
- One-byte edits regress by 63.70%, 33.77%, and 27.23%, and the three-edit M7
  workload regresses by 26.81%, exceeding the allowed 10% regression.
- Steady-state total storage improves by only about 1.06% at 100 MiB, even
  though SQLite database bytes fall by about 99.7%; the payload is in a
  similarly sized carrier and lifecycle/compaction costs remain.

R-HYBRID should be reconsidered only after a streaming carrier writer and a
lower-overhead publication/verification path are implemented and measured with
the same M7 and crash gates. It is not adopted in production LayerFS.

## Phase 2 timing-boundary correction — M7 edit versus full lifecycle

The earlier Phase 2 edit table reported an end-to-end value while describing it
as edit latency. This correction keeps that end-to-end value, but separates the
five requested boundaries for a prepared 1/10/100 MiB file:

1. `M7 bounded local edit`: authenticated path load, bounded source-window
   read, local FastCDC/hash work, local regrouping, and path-copy.
2. `payload persistence`: mode-specific payload/reference write, carrier append
   and carrier fsync where applicable, plus SQLite metadata work through the
   point immediately before `tx.commit()`.
3. `SQLite commit`: the durable SQLite commit with the shared WAL and
   `synchronous=FULL` settings.
4. `close/reopen`: close the connection and open the same database again.
5. `full verification`: SQLite integrity, root/manifest validation, every
   payload hash, logical digest, carrier validation, and orphan check.

The total is measured from the beginning of the bounded edit through the end of
full verification. It does not include building the already-prepared base
fixture. Each cell has six serial samples, alternating lane order. All samples
passed the edit correctness gates. The optional full canonical M7 oracle was
disabled for these performance samples; when enabled, its full-rebuild time is
reported separately as `m7_canonical_check_ms` and is excluded from the bounded
local-edit value.

### Corrected one-byte edit timing medians

All values are milliseconds. The full verification column is intentionally
size-dependent; it is not part of the M7 bounded-edit algorithm.

| File size | Lane | M7 bounded local edit | Payload persistence | SQLite commit | Close/reopen | Full verification | Total end-to-end |
|---:|---|---:|---:|---:|---:|---:|---:|
| 1 MiB | R-SQLite | 4.824 | 1.865 | 2.111 | 2.934 | 11.349 | 26.943 |
| 1 MiB | R-HYBRID | 4.466 | 11.104 | 1.387 | 2.194 | 18.247 | 40.938 |
| 10 MiB | R-SQLite | 10.989 | 2.074 | 2.350 | 4.220 | 114.809 | 166.070 |
| 10 MiB | R-HYBRID | 8.792 | 11.392 | 1.825 | 3.125 | 164.518 | 221.468 |
| 100 MiB | R-SQLite | 12.814 | 3.219 | 4.205 | 6.788 | 1,239.488 | 1,585.432 |
| 100 MiB | R-HYBRID | 9.458 | 13.546 | 1.600 | 3.541 | 1,659.769 | 2,005.970 |

The individual total end-to-end samples were retained as follows, in execution
order; no sample was removed as an outlier:

| File size | R-SQLite samples (ms) | R-HYBRID samples (ms) |
|---:|---|---|
| 1 MiB | 28.0, 26.9, 27.7, 25.3, 26.1, 27.0 | 42.0, 39.9, 42.0, 39.9, 39.2, 46.3 |
| 10 MiB | 173.1, 171.0, 165.9, 166.3, 164.7, 165.0 | 222.6, 221.4, 221.3, 221.5, 219.8, 221.6 |
| 100 MiB | 1,576.6, 1,595.2, 1,581.1, 1,196.5, 1,602.5, 1,589.8 | 1,998.7, 2,012.8, 2,011.5, 2,019.9, 1,623.4, 2,000.5 |

### Interpretation

The corrected measurement shows that the Rust M7 local edit is bounded over
these sizes: R-SQLite grows from 4.824 ms to 12.814 ms and R-HYBRID from
4.466 ms to 9.458 ms while the logical file grows 100x. The size-dependent
part of the old result is full verification, which grows from roughly 11–18 ms
at 1 MiB to roughly 1.2–1.7 seconds at 100 MiB.

R-HYBRID is faster in the bounded local-edit and SQLite-commit components at
10/100 MiB, but its carrier write/fsync and full carrier-aware verification
costs are higher. The total verified edit therefore remains slower by about
52.0% at 1 MiB, 33.4% at 10 MiB, and 26.5% at 100 MiB. This does not change the
Phase 2 decision to defer R-HYBRID; it corrects which phase is responsible for
the observed size scaling.

The release binary now emits both the per-edit `timing` object and the summed
`operation_timing` object. The existing phase2 summarizer also reports medians
for these fields rather than treating `elapsed_ms` as the only edit metric.

## Phase 2 experiment specification — M7-bounded local edit is mandatory

This specification governs all future R-SQLite versus R-HYBRID edit runs. The
primary comparison must inherit the latest M7 bounded local-edit behavior. A
run that uses the older flat R-SQLite edit path, rescans the complete source,
or rebuilds the complete manifest is not an M7 comparison and must not be
reported as one.

### Required algorithm

For a one-byte or other M7-supported local edit, both Rust lanes must execute
the same portable algorithm:

1. Load the authenticated manifest path for the edit offset.
2. Read only the bounded source window required to determine the affected
   FastCDC boundary and local leaf grouping.
3. Rechunk and rehash the affected local region using the M7 parameters:
   FastCDC `(32 KiB, 128 KiB, 512 KiB)`, leaf grouping `(64, 128, 256)`,
   internal grouping `(32, 64, 128)`, and SHA-256 identity.
4. Rebuild only the affected local groups and path-copy the changed manifest
   spine.
5. Reuse unchanged object identities and unchanged upper manifest subtrees.
6. Publish the new root through the same logical transaction boundary in both
   lanes.

The implementation must remain semantically equivalent to the M7 reference
functions `boundedPathAtOffset`, `regroupLevelBounded`,
`buildBoundedManifestState`, `rebuildBoundedSpine`, and
`rebuildManifestBoundedOwned` from LayerFS source commit
`ce9035e49037f60a8c52d2775fd2d88d34e57cd4`.

### Locality invariants

Each retained edit sample must emit and record these counters:

| Counter | M7 requirement |
|---|---|
| `source_bytes` | Bounded local-window bytes, not the logical file size |
| `scan_window_bytes` | Bounded by the M7 local rebuild window for the edit shape |
| `loaded_entries` | Only entries on the affected local frontier and required ancestors |
| `loaded_nodes` | Only the affected manifest path/frontier |
| `new_manifest_node_count` | Only rebuilt local nodes and their copied ancestors |
| `reused_manifest_node_count` | Unchanged subtrees remain reused |
| `changed_object_set` | Equal between R-SQLite and R-HYBRID |

For the current one-byte workload, the 100 MiB gate is expected to remain in
the same locality class as the accepted run: approximately a 2 MiB source
window, a sub-megabyte scan window, 16 loaded entries, 2 loaded nodes, 2 new
manifest nodes, and 5 reused manifest nodes. These are guardrails for the
algorithm, not promises of identical numbers for every edit shape.

The bounded-edit phase may increase modestly with file size because boundary
context, hashing, allocation, and runtime costs are real. It must not become
proportional to the complete file size. The report must not call the result
“constant time”; it must report locality counters and the measured scaling.

### Forbidden substitutions

The following invalidate an M7 bounded-edit result:

- full source scan before the local edit;
- complete FastCDC rechunking of the logical file;
- complete leaf/member rebuild;
- complete manifest/root rebuild when the M7 path is applicable;
- materializing the full file solely to apply a local edit;
- using the Phase 1 flat R-SQLite algorithm as the primary control;
- silently falling back to a full rebuild while retaining the label
  `M7-bounded`;
- charging a full canonical-oracle rebuild to the bounded-edit timer.

If the edit shape is outside the supported M7 bounded case, the run must be
classified explicitly as `M7 fallback`, recorded separately, and excluded
from the one-byte bounded-edit comparison. The fallback may still be useful as
a separate workload, but it cannot be used to claim M7 locality.

### Storage-lane contract

R-SQLite and R-HYBRID must share the complete M7 edit preparation through the
new root candidate. They must use identical fixtures, edit offsets, chunking,
hashes, object identity, manifest construction, transaction boundaries,
SQLite PRAGMAs, close/reopen policy, and verification rules.

Only payload persistence changes:

- R-SQLite stores the new payload bytes in the SQLite payload representation.
- R-HYBRID appends the same logical payload bytes to a self-validating carrier
  and stores the carrier ID, offset, length, payload hash, logical size, and
  verification information in SQLite.

The carrier write, carrier flush/fsync, SQLite commit, and verification costs
must remain visible in their own timing fields. Durability must not be weakened
to preserve the M7 locality result.

### Timing and reporting contract

Every bounded-edit sample must report these phases separately:

```text
m7_bounded_local_edit_ms
payload_persistence_ms
sqlite_commit_ms
close_reopen_ms
full_verification_ms
total_end_to_end_ms
```

`m7_bounded_local_edit_ms` ends after the local root candidate is built and
must exclude the optional full canonical-oracle rebuild. `payload_persistence_ms`
ends immediately before SQLite commit. `sqlite_commit_ms` contains the durable
SQLite commit with `synchronous=FULL`. `close_reopen_ms` covers only closing and
reopening the database. `full_verification_ms` includes the required complete
correctness pass, including carrier validation for R-HYBRID. The total covers
the complete lifecycle from local edit through full verification.

The report must show both the bounded phase and the full lifecycle for 1 MiB,
10 MiB, and 100 MiB files. It must also show individual samples, medians,
source-window bytes, scan-window bytes, changed-object sets, and any fallback
classification. A size-dependent full verification phase is expected and must
not be misattributed to the M7 local-edit algorithm.

### Acceptance gates before performance interpretation

No performance result is valid unless both lanes pass:

- M7 canonical root equivalence for the supported edit;
- read-after-write and close/reopen verification;
- equal logical byte count, object count, manifest, and root digest;
- equal changed-object set and unchanged-object identities outside the edit;
- equal materialized output and random-read bytes;
- SQLite integrity and WAL/checkpoint checks;
- carrier bounds, truncation, offset/length, hash, and orphan checks;
- crash-before-commit and crash-after-commit recovery;
- the recorded locality invariants above.

The canonical M7 oracle may perform a fresh full rebuild, but it must be a
separate correctness measurement. It must never be included in
`m7_bounded_local_edit_ms` or used to turn a local edit into an apparent
full-file edit.

## Phase 2 optimization specification — faster without memory or storage regression

This is the follow-up implementation spec for improving R-HYBRID. It inherits
the M7-bounded local-edit contract above; it does not authorize replacing the
M7 algorithm with a faster full-scan path. The objective is to reduce storage
protocol and verification overhead while preserving bounded working memory,
durability, logical storage semantics, and the existing R-SQLite control.

### Baseline and optimization target

The current 100 MiB one-byte-edit medians identify the target clearly:

| Phase | R-SQLite | R-HYBRID | Optimization target |
|---|---:|---:|---|
| M7 bounded local edit | 12.814 ms | 9.458 ms | Preserve M7 locality; do not trade it away |
| Payload persistence | 3.219 ms | 13.546 ms | Remove avoidable carrier copies/syscalls |
| SQLite commit | 4.205 ms | 1.600 ms | Preserve the current advantage |
| Close/reopen | 6.788 ms | 3.541 ms | Preserve the current advantage |
| Full verification | 1,239.488 ms | 1,659.769 ms | One bounded-memory streaming pass |
| Total lifecycle | 1,585.432 ms | 2,005.970 ms | Close the 420.538 ms gap without hiding work |

The first candidate is successful only if it improves the R-HYBRID baseline
without increasing its peak RSS, temporary bytes, steady-state bytes, or
obsolete carrier bytes. The existing R-SQLite lane remains the latest-M7
control; it must not be weakened or made artificially slower.

### Required optimization shape

Implement the smallest candidate that covers these two measured costs.

1. **Streaming carrier publication.** Build carrier records into one
   deterministic segment stream per logical operation. Update the carrier
   digest while records are appended; do not reread the completed carrier just
   to calculate its identity. If a new carrier file is required, write one
   temporary segment, sync it, atomically rename it, and sync the directory
   once. If an existing append-only segment is used, append the operation's
   records and sync that file once before opening the SQLite transaction.
2. **One-pass carrier verification.** Verify carrier framing, bounds,
   truncation, record hash, referenced object hash, and orphan status in one
   sequential streaming pass. Use a fixed-size buffer and an ordered merge of
   carrier records with SQLite references ordered by `(carrier_id, offset)`;
   do not load the entire carrier or the entire reference table into memory.
3. **No duplicate payload materialization.** The edited payload may exist in a
   bounded edit buffer while it is hashed and written, but the implementation
   must not retain a second full-file byte array, carrier byte array, or full
   verification copy. A carrier record is written once and read once per
   verification pass.
4. **Shared-path preservation.** Keep source-window reading, FastCDC, hashing,
   object admission, M7 regrouping, path-copying, root construction, and
   SQLite metadata semantics identical in both lanes. Only the payload storage
   operation may differ.

`sync_data` may replace `sync_all` only where the resulting crash semantics are
identical for the exact file-creation or append case and the existing
crash-before-commit, crash-after-commit, truncation, and orphan tests pass. A
weaker sync primitive must not be introduced solely because it is faster.

### Memory contract

The optimization must have bounded working memory with respect to logical file
size. It may use the M7 source window, a fixed carrier I/O buffer, SQLite's
configured page/cache memory, and the currently processed payload record. It
must not use full-file `read_to_end`, a full-carrier `Vec`, an unbounded
reference map, or a cache keyed by every object in the file.

The benchmark runner must retain the existing safety guards:

- reject requested fixture sizes above 100 MiB;
- run one benchmark child at a time;
- use explicit size checks before allocation;
- record peak RSS for every retained sample;
- terminate and mark the sample invalid if a process exceeds the configured
  memory ceiling rather than allowing the host to swap or become unstable.

The candidate fails the memory gate if median or maximum peak RSS is higher
than the current R-HYBRID baseline for the same workload, or if peak RSS grows
proportionally with file size after accounting for the existing SQLite cache
and M7 window. The exact baseline values must be captured from the same machine
and build before the candidate run; this avoids choosing an arbitrary absolute
limit.

### Storage and durability contract

The optimization must not increase persistent or transient storage merely to
gain speed.

For every operation, record separately:

```text
database_bytes
wal_bytes
shm_bytes
carrier_bytes
temporary_bytes
live_payload_bytes
obsolete_carrier_bytes
peak_total_bytes
steady_state_total_bytes
post_compaction_total_bytes
```

The candidate fails the storage gate if any of the following occurs against
the current R-HYBRID baseline for the same fixture and operation:

- higher steady-state total bytes;
- higher post-compaction total bytes;
- additional live payload copies;
- unbounded obsolete carrier growth;
- higher sampled peak total bytes without an explicitly reported reason;
- a temporary full-carrier or full-file duplicate.

Carrier publication must continue to follow this ordering:

```text
append carrier record(s)
flush and sync carrier
begin SQLite transaction
insert/update carrier references
commit SQLite with synchronous=FULL
close/reopen and verify
```

Bytes present in the carrier but absent from committed SQLite metadata remain
orphans. Recovery must detect and quarantine or reclaim them without deleting
any referenced payload. A SQLite reference to a missing, truncated, or
hash-mismatched carrier record remains a hard correctness failure.

### Timing instrumentation

The candidate must preserve the existing phase boundaries and add only the
minimum counters needed to explain the optimization:

```text
m7_bounded_local_edit_ms
carrier_record_encode_ms
carrier_append_ms
carrier_digest_ms
carrier_sync_ms
sqlite_metadata_ms
sqlite_commit_ms
close_reopen_ms
full_verification_ms
total_end_to_end_ms
peak_rss_bytes
peak_total_storage_bytes
```

`carrier_digest_ms` must be zero or near-zero incremental work during the
append, not a hidden second full-carrier scan. `full_verification_ms` remains
inside `total_end_to_end_ms`; it may not be moved outside the benchmark to
manufacture a speedup. Any skipped, cached, or incremental verification must
be labeled and measured separately from the required full-verification result.

### Benchmark matrix

Run the candidate and the unchanged baseline with the same serialized runner,
fixtures, filesystem, build mode, cache policy, repetitions, lane order, and
close/reopen policy:

- Create/write: 1, 10, and 100 MiB.
- One-byte edit: 1, 10, and 100 MiB.
- Three one-byte M7-bounded edits: 100 MiB.
- Cold and warm 100 MiB read.
- 1,000 random 4 KiB reads.
- One 100 MiB materialization.
- 100 × 1 MiB materialization.
- Crash before commit, crash after commit, truncation, and orphan cleanup.

Use one warm-up and the existing retained repetitions. Report every sample,
the median, and the phase/counter breakdown. Do not retain a speed result from
a cell whose correctness, memory, or storage gate failed.

### Acceptance criteria

The optimized candidate is eligible for a decision only if all correctness and
crash gates from the M7 specification pass and all of these additional gates
pass:

1. The M7 bounded-edit counters remain in the same locality class at 1, 10,
   and 100 MiB.
2. Peak RSS is no higher than the current R-HYBRID baseline for each workload.
3. Peak, steady-state, and post-compaction storage are no higher than the
   current R-HYBRID baseline, except for measurement noise documented from
   individual samples.
4. R-HYBRID's full lifecycle improves by at least 20% over the current
   R-HYBRID baseline in the targeted write/edit workloads, or it reaches parity
   with R-SQLite without regressing the read/materialization wins.
5. No regression exceeds 10% for random reads, small edits, or materialization
   workloads that already favor R-HYBRID.
6. The result remains valid under `synchronous=FULL`, carrier durability
   ordering, close/reopen verification, and the same SQLite WAL policy.

If the candidate is faster only because it omits full verification, uses weaker
durability, retains more memory, creates duplicate payload storage, or leaves
unbounded carrier garbage, reject it even if the headline elapsed time falls.

### Stop conditions

Do not tune FastCDC, tree fanout, object identity, manifest semantics, or the
M7 bounded window in this optimization pass. Those changes would create a new
algorithm comparison rather than isolate the storage-layout improvement. Stop
after the streaming carrier and one-pass verifier are measured; only then
consider a separate M7 algorithm experiment.

## Phase 2 optimization result — streaming carrier and bounded verifier

**Run date:** 2026-08-15

This section supersedes the earlier Phase 2 defer decision for the specific
streaming-carrier candidate. The production LayerFS checkout remained
untouched at `8c1c55785ab25d0435d47e32e35a1a7bdf1fbf6d`; all changes are
confined to this experiment branch and its report/runner.

### Candidate implemented

The candidate keeps the shared M7 edit path and changes only the R-HYBRID
publication and validation work:

- `CarrierWriter` emits deterministic LFCR records through a 128 KiB
  `BufWriter`, updates SHA-256 while appending, and does not build a full
  carrier `Vec` or reread a completed carrier to calculate its ID.
- Carrier durability remains temporary-file write, flush, `sync_all`, atomic
  rename, carrier-directory sync, SQLite reference transaction, and
  `synchronous=FULL` commit. Existing deterministic carrier files are checked
  with the streaming validator before a temporary duplicate is removed.
- The validator uses a fixed buffer, checks framing, lengths, bounds,
  truncation, record hashes, carrier digest, and ordered SQLite references by
  `(carrier_id, carrier_offset)`. Carrier creation stages only missing object
  hashes, avoiding duplicate publication of already-admitted payloads.
- `carrier_record_encode_ms`, `carrier_append_ms`, `carrier_digest_ms`,
  `carrier_sync_ms`, `carrier_verify_ms`, `peak_rss_bytes`, and
  `peak_total_storage_bytes` are emitted by the runner. Each child is wrapped
  with macOS `/usr/bin/time -l` for per-sample RSS.

The optional canonical M7 check was also corrected so sequential three-edit
checks rebuild from the already-edited logical bytes rather than resetting to
the original fixture. That is test-only plumbing and is excluded from
`m7_bounded_local_edit_ms` and the default performance campaign.

### Campaign custody and correctness

The release binary was run through the serialized Phase 2 runner with one
warm-up and five retained samples per cell, six samples per cell, alternating
lane order, and one child at a time. The campaign completed 120 children; all
retained children reported `phase2=pass`. Raw samples and the derived medians
are retained in:

```text
experiments/sqlite-techstack/results/phase2.jsonl
experiments/sqlite-techstack/results/phase2-summary.json
```

Additional current-binary gates passed for both lanes:

| Gate | Result |
|---|---|
| Carrier format, bounds, payload hash, truncation rejection | Pass |
| Canonical M7 root equivalence, one-byte edits at 1/10/100 MiB | Pass |
| Canonical M7 root equivalence, three edits at 100 MiB | Pass |
| Changed-object and unchanged-identity checks | Pass |
| Read-after-write, close/reopen, digest, SQLite integrity | Pass |
| Crash before commit and crash after commit | Pass |
| Hybrid orphan detection and quarantine | Pass |
| WAL checkpoint and carrier durability ordering | Pass |

The crash checks are process-exit simulations at the existing transaction
boundaries, not a power-loss or filesystem fault injector. The benchmark's
"cold" read is also the existing APFS reopened-namespace condition, not a
page-cache eviction.

### End-to-end medians

Times are milliseconds unless the unit is shown. The delta is
`(R-HYBRID / R-SQLite - 1) * 100`; negative means Hybrid is faster. These are
medians of the five retained samples after the one discarded warm-up.

| Workload | R-SQLite | R-HYBRID | Hybrid delta |
|---|---:|---:|---:|
| Create 1 MiB | 48.250 | 68.233 | +41.41% |
| Create 10 MiB | 399.018 | 500.897 | +25.53% |
| Create 100 MiB | 2,889.523 | 3,405.885 | +17.87% |
| One-byte edit 1 MiB | 19.346 | 32.342 | +67.18% |
| One-byte edit 10 MiB | 120.250 | 161.998 | +34.72% |
| One-byte edit 100 MiB | 1,127.584 | 1,427.862 | +26.63% |
| Three M7-bounded edits, 100 MiB | 3,365.383 | 4,276.873 | +27.08% |
| Cold 100 MiB read | 856.791 | 716.745 | -16.35% |
| Warm 100 MiB read | 852.193 | 716.692 | -15.90% |
| 1,000 random 4 KiB reads, µs/read | 663.887 | 468.637 | -29.41% |
| One 100 MiB materialization | 645.561 | 512.109 | -20.67% |
| 100 × 1 MiB materialization, per-file median | 9.967 | 9.205 | -7.64% |

The retained end-to-end samples, in execution order, were:

| Workload | R-SQLite samples | R-HYBRID samples |
|---|---|---|
| Create 1 MiB | 47.535, 48.458, 48.250, 48.881, 47.769 | 68.233, 68.000, 67.699, 71.617, 70.824 |
| Create 10 MiB | 415.929, 400.740, 397.597, 398.800, 399.018 | 501.363, 496.745, 498.207, 500.897, 501.943 |
| Create 100 MiB | 2,889.523, 2,913.527, 2,858.769, 2,863.857, 2,917.494 | 4,805.742, 3,405.885, 3,404.484, 3,400.407, 3,435.678 |
| Edit 1 MiB | 19.430, 19.346, 18.317, 19.449, 17.945 | 31.837, 32.394, 32.342, 32.458, 31.269 |
| Edit 10 MiB | 123.592, 120.250, 120.290, 119.208, 119.852 | 163.508, 161.998, 163.620, 160.445, 161.017 |
| Edit 100 MiB | 1,124.282, 1,131.139, 1,115.844, 1,160.302, 1,127.584 | 1,450.295, 1,427.862, 1,425.960, 1,433.439, 1,421.335 |
| Three edits, 100 MiB | 3,370.965, 3,327.956, 3,331.929, 3,365.383, 3,365.714 | 4,295.553, 4,285.976, 4,253.175, 4,276.873, 4,255.460 |
| Cold read, 100 MiB | 856.791, 873.929, 842.573, 875.232, 845.499 | 729.459, 724.701, 706.035, 709.753, 716.745 |
| Warm read, 100 MiB | 866.925, 862.615, 843.159, 852.193, 846.954 | 727.982, 718.074, 709.771, 716.692, 714.140 |
| Random read, µs/read | 663.949, 686.364, 654.337, 653.080, 663.887 | 469.963, 478.275, 463.382, 468.637, 465.227 |
| Materialize, 100 MiB | 639.903, 643.909, 663.464, 645.561, 649.717 | 509.275, 510.692, 513.627, 512.109, 515.329 |
| 100 × 1 MiB, ms/file | 10.016, 9.975, 9.916, 9.951, 9.967 | 9.124, 9.965, 9.205, 9.781, 9.100 |

### M7 phase boundaries and carrier timing

The shared bounded-edit phase remains local. For the one-byte workload, the
median M7 phase is `3.340 → 7.852 → 9.066 ms` for R-SQLite and
`3.157 → 6.385 → 6.625 ms` for R-HYBRID at 1/10/100 MiB. The required full
verification remains inside the total and is reported separately:

| One-byte edit | M7 local edit | Payload persistence | SQLite commit | Close/reopen | Full verification | Total |
|---|---:|---:|---:|---:|---:|---:|
| 1 MiB / R-SQLite | 3.340 | 1.298 | 1.621 | 2.601 | 7.930 | 19.346 |
| 1 MiB / R-HYBRID | 3.157 | 10.474 | 0.999 | 2.170 | 13.145 | 32.342 |
| 10 MiB / R-SQLite | 7.852 | 1.408 | 2.315 | 3.037 | 82.801 | 120.250 |
| 10 MiB / R-HYBRID | 6.385 | 11.583 | 1.118 | 2.246 | 118.776 | 161.998 |
| 100 MiB / R-SQLite | 9.066 | 2.097 | 3.316 | 5.580 | 879.784 | 1,127.584 |
| 100 MiB / R-HYBRID | 6.625 | 12.120 | 1.239 | 2.761 | 1,179.876 | 1,427.862 |

At 100 MiB, the carrier-specific Hybrid medians were:

| Workload | Encode | Append/write | Incremental digest | Carrier sync | Carrier verify | SQLite metadata |
|---|---:|---:|---:|---:|---:|---:|
| Create | 0.011 | 242.127 | 0.000 | 13.160 | 1,374.443 | 4.448 |
| One-byte edit | 0.013 | 245.108 | 0.001 | 23.092 | 926.026 | 5.613 |
| Three edits | 0.012 | 246.057 | 0.001 | 40.842 | 1,867.281 | 6.572 |

The near-zero incremental digest field is the important instrumentation
result: the candidate hashes headers and payloads during the append stream;
there is no hidden full-carrier digest scan. The remaining carrier write and
verification costs are visible rather than moved outside the lifecycle.

### RSS and storage

Peak RSS is the median of the five retained `/usr/bin/time -l` measurements;
peak storage is the median sampled directory total emitted by the child.

| Workload / size | R-SQLite RSS | R-HYBRID RSS | R-SQLite peak storage | R-HYBRID peak storage |
|---|---:|---:|---:|---:|
| Create 100 MiB | 311,345,152 | 221,478,912 | 107,254,624 | 105,323,808 |
| One-byte edit 100 MiB | 311,050,240 | 222,347,264 | 107,254,624 | 105,480,428 |
| Three edits 100 MiB | 311,361,536 | 222,461,952 | 107,254,624 | 105,929,546 |
| Read 100 MiB | 310,984,704 | 221,282,304 | 107,254,624 | 105,323,808 |
| Materialize 100 MiB | 311,083,008 | 220,921,856 | 107,254,624 | 105,323,808 |
| 100 × 1 MiB | 8,159,232 | 6,127,616 | 1,264,656 | 1,263,080 |

The candidate is below the available same-machine historical Hybrid RSS
measurement of 543,817,728 bytes and below the current R-SQLite control in
every listed cell. The historical pre-optimization RSS was manually sampled,
not collected by the new per-child field, so this is a strong regression check
but not a perfectly paired pre/post RSS capture. Peak storage is also sampled
at lifecycle boundaries; temporary carrier bytes can be present between those
snapshots.

### Acceptance decision

**Accept the streaming-carrier candidate as the Phase 2 experiment result;
do not promote it directly to production LayerFS.**

Against the prior corrected 100 MiB R-HYBRID baseline, the optimized medians
improve by 30.15% for create, 28.88% for one-byte edit, and 28.82% for three
M7-bounded edits. This meets the required 20% targeted-write improvement.
The candidate also preserves the read/materialization advantage, lowers the
measured RSS and sampled peak storage, keeps `synchronous=FULL`, and passes
the carrier, canonical, reopen, integrity, WAL, orphan, and crash gates.

It is still slower than the R-SQLite control by 17.87% on 100 MiB create,
26.63% on one-byte edit, and 27.08% on three edits. That is allowed by the
optimization rule because the candidate clears the 20% improvement threshold
against the prior Hybrid baseline, but it is not parity.

Two limitations remain explicit. First, the streaming carrier validator is a
single fixed-buffer carrier pass, but the full verification lifecycle still
performs the separate materialized logical-digest walk before that validator;
the candidate removes full-carrier allocation and the duplicate digest scan,
not every logical payload read. Second, the minimal Rust harness still builds
fixture/object/manifest structures in memory and samples disk usage at
boundaries, so this result establishes the storage-path improvement rather
than production-grade bounded-memory proof. The follow-up below removes the
full fixture payload map from the Phase 2 base path and changes edit
verification to an explicitly labeled incremental contract. A production
adoption would still need the exact production M7 implementation, paired
automated pre/post RSS and continuous temporary-space observation, and
fault-injection coverage beyond process-exit crash simulation.

## Phase 2 follow-up — incremental verification and streamed fixture setup

**Run date:** 2026-08-15

This follow-up implements the only remaining optimization with a credible
path to materially faster edits: after a newly published state has passed the
full verifier, each edit verifies SQLite integrity, the new root and journal
reference, new manifest nodes, changed payloads, and the new hybrid carrier
only. The full logical digest and full carrier scrub remain available through
the existing verifier. Set `PHASE2_FULL_SCRUB_EVERY=N` to run that full scrub
after every N edits; the output labels this as
`incremental+periodic-full-scrub` and reports `full_scrub_ms` separately.

The persisted `efs_verification_state` record makes incremental verification
fail closed unless the preceding root, logical digest, and journal generation
come from a matching full verification boundary. Unchanged manifest nodes and
payloads are therefore trusted only from that recorded boundary, rather than
silently treated as newly verified. Create, recovery, and compaction retain
the full verifier by default.

The Phase 2 fixture now streams deterministic bytes through a fixed buffer,
FastCDC, and hashing, retaining chunk metadata and a deterministic source
cursor instead of a full payload map. Edit digest oracles are also streamed;
random-read oracles regenerate only the requested chunk window. The optional
`PHASE2_M7_CANONICAL_CHECK=1` path intentionally retains a full canonical
oracle because it is a correctness cross-check, not the production benchmark
path.

The official paired runner was executed with one warm-up and two retained
samples per cell (`PHASE2_REPS=3`):

| 100 MiB workload | R-HYBRID | R-SQLite | Hybrid delta |
|---|---:|---:|---:|
| Create | 1,973.0 ms | 1,862.7 ms | 5.9% slower |
| One-byte edit | 307.8 ms | 341.3 ms | 9.8% faster |
| Three edits | 909.5 ms | 1,014.9 ms | 10.4% faster |

The same retained samples measured peak RSS of 6.6 MiB versus 93.1 MiB on
create, 11.0 MiB versus 93.0 MiB on one-byte edit, and 13.0 MiB versus
91.7 MiB on three edits for R-HYBRID versus R-SQLite. Hybrid remains slower
on create because deterministic source/CDC/hash and carrier publication still
dominate that path, but the edit path now beats SQLite in this run without
weakening durability or carrier publication ordering. A full scrub every
edit was separately exercised at 1 MiB in both lanes and passed; it reports
three full scrubs for the three-edit workload.

This changes the edit guarantee from full verification on every lifecycle to
incremental verification plus periodic full verification. If a caller requires
a full logical scrub after every edit, the incremental speed result does not
apply and a 50% improvement is not supported by this experiment. Changing
the database alone is unlikely to produce that improvement now: the 100 MiB
create cost is dominated by shared source/CDC/hash work, while the edit cost
is dominated by hybrid carrier publication and durability work.
SQLite-specific incremental verification is only about 4 ms for Hybrid and
43 ms for SQLite in the retained one-byte sample.

## Phase 2 closure audit — latest authoritative paired rerun

**Run date:** 2026-08-15

This section supersedes the numeric tables in the preceding optimization-result
section. The earlier section recorded an intermediate rerun; the tables below
come from the final source state after the paired logical-equivalence checks and
per-workload storage snapshots were added.

### Custody, subagent findings, and synthesis

- LayerFS production reference: commit
  `8c1c55785ab25d0435d47e32e35a1a7bdf1fbf6d`; M7 source reference
  `ce9035e49037f60a8c52d2775fd2d88d34e57cd4`. The production checkout was not
  modified.
- Experiment branch: `experiment/sqlite-techstack`; report-only base commit:
  `52ebd7771bbdde6a5422cd7637aefd731d67ff29`. The final experiment commit is
  supplied in the handoff after this report is committed.
- The three mandatory read-only explorers found the same smallest safe scope:
  retain the shared M7 path and durability ordering; stream the carrier with a
  fixed buffer and incremental digest; validate it in one bounded sequential
  pass; and add timing, memory, storage, and paired-equivalence evidence.
  Explorer 1 confirmed the temporary-file, sync, rename, directory-sync, and
  SQLite-reference ordering. Explorer 2 found that a full carrier allocation or
  unbounded reference map would be the principal memory risks. Explorer 3
  confirmed that both lanes use the latest M7 bounded source window, local
  regrouping, changed-spine path-copy, and unchanged-subtree reuse.
- The synthesized plan was therefore limited to
  `experiments/m8-rust-port/src/main.rs`, the minimum result validation in
  `experiments/sqlite-techstack/phase2.mjs`, and this preserved report. No
  FastCDC, tree fanout, object identity, manifest semantics, M7 window, SQLite
  durability mode, or production file was changed.

The final command was:

```text
PHASE2_REPS=6 node phase2.mjs
```

It ran serialized child processes, discarded one warm-up per cell, retained
five samples per cell, alternated lane order, and serialized
`results/phase2.jsonl` plus `results/phase2-summary.json`. There were 120 child
samples, and every retained child passed `phase2=pass`. The runner's
`validatePairedResults` compared roots, logical digests, object counts, manifest
counts, edit offsets, changed-object sets, unchanged-identity sets, and M7
locality counters for every retained pair; those comparisons are computed from
the Rust results rather than hardcoded pass values.

### Carrier functions inspected and implementation

The exact carrier and lifecycle functions inspected were:

`CarrierWriter::new`, `CarrierWriter::append`, `CarrierWriter::finish`,
`stage_carrier`, `validate_carrier_file`, `read_carrier_record`,
`phase2_prepare_objects`, `phase2_persist_manifest`, `phase2_store`,
`phase2_read_payload`, `phase2_materialized_digest`, `phase2_verify`,
`stage_compaction_carrier`, `phase2_compact`, `phase2_edit_entries`,
`phase2_local_rebuild`, `phase2_space`, `phase2_orphan_files`,
`phase2_quarantine_orphans`, `phase2_carrier_check`,
`phase2_recovery_record`, and `phase2_crash_child`. In the runner, the paired
validation path is `validatePairedResults`; execution and summarization remain
in `run` and `summarize`.

The implementation is deliberately small:

- R-HYBRID writes LFCR records through a fixed 128 KiB `BufWriter`, updates the
  carrier SHA-256 as records are appended, and avoids a full-carrier `Vec` and
  duplicate post-write digest scan.
- Durability remains append, flush, `sync_all`, atomic rename,
  carrier-directory sync, SQLite reference insertion/update, and
  `synchronous=FULL` commit, followed by close/reopen verification.
- Carrier validation uses a fixed buffer, checks framing, lengths, bounds,
  truncation, record hashes, carrier identity, and ordered SQLite references.
- M7 edit results now compute changed-object and unchanged-identity digests,
  assert that the unaffected prefix/suffix identities remain unchanged, and
  expose those fields to the paired runner.
- Edit, read, and materialization records now include a steady-state storage
  snapshot from the existing `phase2_space` helper. Create already reports
  steady-state and post-compaction storage. These snapshots are outside the
  timed operation boundaries.

Tracked source/report files changed for this experiment are:

- `experiments/m8-rust-port/src/main.rs`
- `experiments/sqlite-techstack/phase2.mjs`
- `experiments/sqlite-techstack/experiment.md`

The unrelated dirty `experiments/sqlite-techstack/rust/src/main.rs`, the
untracked `experiments/sqlite-techstack/rust/src/workspace.rs`, and the
untracked `experiments/npm-cas-cdc/` directory were preserved.

### End-to-end retained samples and medians

Times are milliseconds except the random-read row. Samples are the five
retained values after the discarded warm-up. Delta is
`(R-HYBRID / R-SQLite - 1) * 100`; negative means Hybrid is faster.

| Workload | R-SQLite retained samples / median | R-HYBRID retained samples / median | Hybrid delta |
|---|---|---|---:|
| Create 1 MiB | 27.224500, 26.633917, 27.496333, 27.720125, 27.799542 / **27.496333** | 37.582417, 38.069875, 38.999916, 39.346625, 42.110375 / **38.999916** | +41.84% |
| Create 10 MiB | 200.772959, 200.544625, 200.010041, 200.445042, 210.263625 / **200.544625** | 219.095083, 217.952792, 223.388792, 219.813208, 217.590208 / **219.095083** | +9.25% |
| Create 100 MiB | 1,967.245959, 2,016.581375, 2,884.168709, 2,005.253375, 1,994.773292 / **2,005.253375** | 2,032.805750, 2,016.951541, 2,685.428750, 2,049.379875, 1,997.017291 / **2,032.805750** | +1.37% |
| One-byte edit 1 MiB | 17.243084, 18.438666, 18.811542, 18.923042, 18.278958 / **18.438666** | 30.030875, 27.401084, 29.431375, 29.118208, 29.116666 / **29.118208** | +57.92% |
| One-byte edit 10 MiB | 121.679875, 117.824209, 119.459000, 119.426084, 118.952292 / **119.426084** | 136.771750, 136.586750, 137.586833, 138.894458, 137.723875 / **137.586833** | +15.21% |
| One-byte edit 100 MiB | 1,128.319958, 1,122.743208, 1,112.038000, 1,116.480250, 1,095.617042 / **1,116.480250** | 1,199.475541, 1,171.236375, 1,179.556500, 1,173.559041, 1,172.958542 / **1,173.559041** | +5.11% |
| Three M7-bounded edits, 100 MiB | 3,280.687750, 3,283.543792, 3,285.454084, 3,290.586625, 3,283.520958 / **3,283.543792** | 3,513.518750, 3,549.592875, 3,551.435333, 3,522.090084, 3,521.817000 / **3,522.090084** | +7.26% |
| One 100 MiB materialization | 618.258500, 621.673667, 617.862125, 618.505625, 618.675209 / **618.505625** | 510.918708, 500.911417, 497.134500, 494.467417, 498.548667 / **498.548667** | -19.39% |
| 100 × 1 MiB materialization, per-file median | 9.989875, 9.978917, 9.985625, 9.993417, 9.996166 / **9.989875** | 9.096125, 9.005083, 9.045541, 9.164375, 9.009291 / **9.045541** | -9.45% |

Read workloads have three measurements per retained sample:

| Read measurement | R-SQLite retained samples / median | R-HYBRID retained samples / median | Hybrid delta |
|---|---|---|---:|
| Cold 100 MiB read, ms | 821.774500, 819.977666, 898.437708, 825.454709, 822.198958 / **822.198958** | 696.290083, 698.176250, 694.375000, 708.523166, 698.438875 / **698.176250** | -15.08% |
| Warm 100 MiB read, ms | 815.723834, 818.361500, 828.059666, 823.849958, 821.207250 / **821.207250** | 696.675458, 696.377541, 694.872958, 708.490875, 696.351708 / **696.377541** | -15.20% |
| 1,000 random 4 KiB reads, µs/read | 632.122625, 632.225834, 648.521250, 645.134958, 641.136834 / **641.136834** | 452.343417, 451.251917, 449.802459, 460.268917, 453.956083 / **452.343417** | -29.45% |

Relative to the current R-HYBRID baseline recorded in the objective, the
candidate improves the 100 MiB targeted workloads by 58.31% for create, 41.55%
for one-byte edit, and 41.38% for three edits. It clears the required 20%
within-Hybrid improvement threshold, but remains slower than R-SQLite in those
workloads by the deltas shown above.

### Timing breakdown

The required boundaries are preserved: M7 bounded local edit, payload
persistence, SQLite commit, close/reopen, full verification, and total. Full
verification remains inside total.

| Workload / lane, 100 MiB | M7 local edit | Payload persistence | SQLite commit | Close/reopen | Full verification | Total |
|---|---:|---:|---:|---:|---:|---:|
| One-byte / R-SQLite | 8.923209 | 2.124750 | 3.732000 | 6.073583 | 873.248459 | 1,116.480250 |
| One-byte / R-HYBRID | 6.716958 | 11.866084 | 1.165250 | 3.080709 | 928.253541 | 1,173.559041 |
| Three edits / R-SQLite | 19.148084 | 5.025792 | 8.954458 | 15.422708 | 2,566.307958 | 3,283.543792 |
| Three edits / R-HYBRID | 13.778750 | 34.824875 | 3.735542 | 8.393334 | 2,794.094709 | 3,522.090084 |

Carrier-specific medians at 100 MiB were:

| Workload / R-HYBRID | Record encode | Append | Incremental digest | Carrier sync | Carrier verify | SQLite metadata |
|---|---:|---:|---:|---:|---:|---:|
| Create | 0.012125 | 251.090168 | 0.000167 | 11.491958 | 467.623000 | 4.459792 |
| One-byte edit | 0.011959 | 241.551375 | 0.000584 | 21.887083 | 464.664292 | 5.344292 |
| Three edits | 0.011245 | 241.629583 | 0.001124 | 42.320583 | 933.523334 | 6.545707 |

The corresponding R-SQLite SQLite-metadata medians were 96.873042 ms for
create, 95.920209 ms for one-byte edit, and 98.122542 ms for three edits. The
near-zero digest field is evidence that carrier identity is computed during the
append stream; it is not a hidden full-carrier reread.

### Peak RSS and disk-space tables

Peak values are medians of the five retained `/usr/bin/time -l` measurements or
the sampled directory totals. All byte fields are bytes.

| Workload / size | R-SQLite peak RSS | R-HYBRID peak RSS | R-SQLite peak total | R-HYBRID peak total |
|---|---:|---:|---:|---:|
| Create 100 MiB | 311,099,392 | 221,298,688 | 107,254,624 | 105,323,808 |
| One-byte edit 100 MiB | 311,033,856 | 222,035,968 | 107,254,624 | 105,480,428 |
| Three edits 100 MiB | 311,066,624 | 222,593,024 | 107,254,624 | 105,929,546 |
| Read 100 MiB | 311,001,088 | 221,085,696 | 107,254,624 | 105,323,808 |
| Materialize 100 MiB | 311,066,624 | 221,134,848 | 107,254,624 | 105,323,808 |
| 100 × 1 MiB | 7,634,944 | 6,029,312 | 1,264,656 | 1,263,080 |

The detailed 100 MiB storage snapshots were:

| Workload / lane | SQLite DB | WAL | SHM | Carrier | Temporary | Live payload | Obsolete carrier | Peak total | Steady total | Post-compaction |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| Create / R-SQLite | 106,307,584 | 0 | 32,768 | 0 | 0 | 104,857,600 | 0 | 107,254,624 | 106,340,352 | 106,340,352 |
| Create / R-HYBRID | 286,720 | 0 | 32,768 | 104,895,512 | 0 | 104,857,600 | 0 | 105,323,808 | 105,215,000 | 105,215,000 |
| One-byte / R-SQLite | 106,516,480 | 0 | 32,768 | 0 | 0 | 105,015,820 | 0 | 107,254,624 | 106,549,248 | not measured |
| One-byte / R-HYBRID | 335,872 | 0 | 32,768 | 105,053,788 | 0 | 105,015,820 | 0 | 105,480,428 | 105,422,428 | not measured |
| Three edits / R-SQLite | 106,979,328 | 0 | 32,768 | 0 | 0 | 105,370,642 | 0 | 107,254,624 | 107,012,096 | not measured |
| Three edits / R-HYBRID | 430,080 | 0 | 32,768 | 105,408,722 | 0 | 105,370,642 | 0 | 105,929,546 | 105,871,570 | not measured |
| Read / R-SQLite | 106,307,584 | 0 | 32,768 | 0 | 0 | 104,857,600 | 0 | 107,254,624 | 106,340,352 | not measured |
| Read / R-HYBRID | 286,720 | 0 | 32,768 | 104,895,512 | 0 | 104,857,600 | 0 | 105,323,808 | 105,215,000 | not measured |
| Materialize / R-SQLite | 106,307,584 | 0 | 32,768 | 0 | 0 | 104,857,600 | 0 | 107,254,624 | 211,197,952 | not measured |
| Materialize / R-HYBRID | 286,720 | 0 | 32,768 | 104,895,512 | 0 | 104,857,600 | 0 | 105,323,808 | 210,072,600 | not measured |

Materialization steady totals include the required 100 MiB output file in the
workload directory; the two lanes produce equal output bytes, so the comparison
remains fair. The carrier temporary-file field was zero in the post-operation
snapshots. Create is the only workload that invokes the existing compaction
path, so post-compaction is measured there; a full post-compaction table for
edit/read/materialization was not manufactured.

For the 100 × 1 MiB workload, steady totals were 2,211,840 bytes for
R-SQLite and 2,204,096 bytes for R-HYBRID; post-compaction was not run for that
repeated materialization workload.

### SQL, transaction, and statement-count comparison

The operation columns exclude the close/reopen verification pass; the full
columns include the full result counters. R-HYBRID create full counters include
the required post-compaction verification, while its primary operation counters
match the R-SQLite logical operation except for one carrier metadata row.

| Workload / counters | R-SQLite operation: statements / rows read / rows inserted / tx / commits | R-HYBRID operation: statements / rows read / rows inserted / tx / commits | R-SQLite full | R-HYBRID full |
|---|---|---|---|---|
| Create 100 MiB | 2,048 / 683 / 688 / 1 / 1 | 2,048 / 683 / 689 / 1 / 1 | 2,048 / 683 / 688 / 1 / 1 | 2,731 / 1,366 / 689 / 2 / 2 |
| One-byte edit 100 MiB | 713 / 706 / 6 / 1 / 1 | 713 / 706 / 7 / 1 / 1 | 2,761 / 1,389 / 694 / 2 / 2 | 2,761 / 1,389 / 696 / 2 / 2 |
| Three edits 100 MiB | 2,134 / 2,113 / 18 / 3 / 3 | 2,134 / 2,113 / 21 / 3 / 3 | 4,182 / 2,796 / 706 / 4 / 4 | 4,182 / 2,796 / 710 / 4 / 4 |
| Read 100 MiB | 2,392 / 2,392 / 0 / 0 / 0 | 2,392 / 2,392 / 0 / 0 / 0 | 4,440 / 3,075 / 688 / 1 / 1 | 4,440 / 3,075 / 689 / 1 / 1 |
| One 100 MiB materialization | 6 / 6 / 0 / 0 / 0 | 6 / 6 / 0 / 0 / 0 | 2,054 / 689 / 688 / 1 / 1 | 2,054 / 689 / 689 / 1 / 1 |
| 100 × 1 MiB materialization | 200 / 200 / 0 / 0 / 0 | 200 / 200 / 0 / 0 / 0 | 229 / 210 / 11 / 1 / 1 | 229 / 210 / 12 / 1 / 1 |

The M7 edit source-window counters also matched: one edit read 2,097,152
source bytes in one source transaction; three edits read 4,371,981 source bytes
in three source transactions. Read workloads performed 1,000 source reads and
transactions and 92,832,100 source bytes in each lane. Materialization read 677
objects; repeated 1 MiB materialization read 800 objects in each lane. The
small row-count differences are the carrier metadata rows, not skipped logical
payload work.

### Correctness, durability, and crash recovery

All retained pairs passed the runner's computed equality checks for base root,
final root, logical digest, object count, manifest-node count, and materialized
logical bytes. Every edit pair also matched offset, M7 locality counters,
changed-object set, and unchanged-identity set. Focused canonical M7 checks
passed for one-byte edits at 1/10/100 MiB and for three sequential edits at 100
MiB in both lanes.

The independent carrier self-check reported `bounds=verified`,
`carrier_format_check=pass`, `payload_hash=verified`, and
`truncated_record=rejected`. SQLite integrity, WAL/checkpoint, read-after-write,
close/reopen, carrier bounds, truncation, payload hash, and carrier digest gates
all passed. The configured path remained WAL with `synchronous=FULL`,
`foreign_keys=ON`, `mmap_size=0`, and the existing cache/source-window policy.

Crash/recovery results for both lanes were baseline counts `[8,1,1]`,
crash-before-commit status `91`, and after-commit status `92`, with recovered
counts `[9,2,2]`. Both lanes passed old-or-new visibility, referenced-payload,
SQLite-integrity, crash-before-commit, and crash-after-commit checks. SQLite
quarantined zero orphan carrier files; R-HYBRID quarantined one deliberately
uncommitted carrier file. No unquarantined orphan remained.

### Limitations and final decision

- Crash tests are process-exit simulations at the existing pre-commit and
  post-commit boundaries, not power-loss or filesystem-fault injection.
- The cold-read condition is the existing APFS reopened-namespace condition,
  not an explicit page-cache eviction test.
- RSS is measured by `/usr/bin/time -l`; disk usage is sampled at lifecycle
  boundaries, not continuously during the temporary-file interval. The current
  Rust harness still builds fixture/object/manifest structures in memory, so
  this is a storage-path experiment rather than production-grade bounded-memory
  proof.
- Materialization storage totals include the output file; post-compaction is
  measured only on create. The non-create rows therefore report steady-state
  and peak snapshots, but not an invented post-compaction result.
- R-HYBRID remains slower than R-SQLite on create and durable edit workloads,
  although the candidate improves the prior R-HYBRID baseline by more than 20%
  on each targeted 100 MiB write/edit workload.

**Decision: Defer R-HYBRID for the overall workload.** Keep the streaming
carrier and bounded verifier as a valid experiment result because it clears the
within-Hybrid optimization gate and preserves the read, random-read, and large
materialization advantages. Do not promote it to production or select it as the
default storage layout while the write/edit lane is still slower than
R-SQLite; a future adoption would need paired production-scale benchmarks,
continuous temporary-space/RSS capture, full post-compaction coverage for the
remaining workloads, and stronger fault injection.

## Phase 2 alternate architecture — stable append-only payload log (R-LOG)

**Run date:** 2026-08-15
**Command:** `PHASE2_REPS=4 node phase2.mjs`
**Samples:** one warm-up discarded, three retained per cell, 120 retained
child samples across three lanes

The user-requested alternative to R-HYBRID was implemented as R-LOG. It keeps a
single stable, page-independent `carriers/payload.log` and appends authenticated
LFCR records. SQLite stores each object's log offset, length, checksum, and the
committed log high-water mark. The writer flushes and `sync_all`s the log and
its directory before the SQLite transaction commits. Recovery truncates an
uncommitted tail back to the SQLite high-water mark; a shorter-than-committed
log fails closed. The log also carries a chained digest, so a full verifier can
detect reordering, truncation, or replacement rather than trusting offsets
alone.

This is materially different from one carrier per generation, but it exposes a
hard cost: every mandatory full scrub walks the entire historical log. The
append path is cheap; the required verification path is not.

### Subagent findings and smallest safe plan

- Ohm found that the existing CAS table is already a BLOB primary-key
  `WITHOUT ROWID` table. The redundant SQLite object-admission preflight was
  removed and the remaining hot lookups use prepared-statement caching.
- Jason found that the edit digest oracle had been charged to the operation
  timer even though it is only a correctness oracle. It is now reported
  separately; the full scrub remains inside the operation lifecycle. The
  materialized digest also no longer hashes the same payload a second time.
- Maxwell proposed the stable append-only log tested here: SQLite offsets plus
  checksum, log sync before commit, committed high-water mark, and crash-tail
  truncation. That design was implemented and passed the crash gates.

The shared M7 path, FastCDC profile, manifest fanout, object identities,
SQLite WAL mode, `synchronous=FULL`, source-window bound, and durability order
were not weakened. The official runner forces
`PHASE2_FULL_SCRUB_EVERY=1`, so the full scrub is included in every edit total.

### Retained samples and medians

Times are milliseconds unless noted. Each cell shows the three retained samples
followed by the median. The raw JSONL contains the complete per-process record,
including roots, digests, counters, timing fields, RSS, storage, and correctness
fields.

| Workload | R-SQLite | R-HYBRID | R-LOG |
|---|---:|---:|---:|
| Create 1 MiB | 36.713, 33.717, 33.098 / **33.717** | 46.907, 44.703, 47.332 / **46.907** | 44.951, 50.524, 47.377 / **47.377** |
| Create 10 MiB | 230.271, 224.925, 229.454 / **229.454** | 256.470, 270.630, 255.499 / **256.470** | 286.806, 286.783, 284.521 / **286.783** |
| Create 100 MiB | 2,239.421, 2,262.619, 2,364.638 / **2,262.619** | 2,344.225, 2,417.892, 2,464.540 / **2,417.892** | 2,662.951, 2,721.008, 2,686.406 / **2,686.406** |
| One-byte edit 1 MiB | 21.680, 22.500, 22.964 / **22.500** | 36.574, 37.574, 35.809 / **36.574** | 46.281, 45.990, 44.892 / **45.990** |
| One-byte edit 10 MiB | 110.022, 109.554, 108.976 / **109.554** | 130.324, 126.654, 128.968 / **128.968** | 254.929, 268.767, 254.140 / **254.929** |
| One-byte edit 100 MiB | 1,051.251, 1,003.092, 1,039.817 / **1,039.817** | 1,056.224, 1,035.294, 1,061.115 / **1,056.224** | 2,339.400, 2,363.506, 2,361.828 / **2,361.828** |
| Three M7-bounded edits, 100 MiB | 3,073.239, 3,094.379, 3,102.581 / **3,094.379** | 3,180.176, 3,146.365, 3,170.106 / **3,170.106** | 7,121.735, 7,128.655, 7,027.881 / **7,121.735** |

R-LOG's 100 MiB read samples were:

| Read measurement | R-SQLite | R-HYBRID | R-LOG |
|---|---:|---:|---:|
| Cold read, ms | 887.248, 885.609, 860.153 / **885.609** | 696.068, 673.998, 679.982 / **679.982** | 696.150, 681.012, 675.325 / **681.012** |
| Warm read, ms | 878.812, 875.236, 864.721 / **875.236** | 1,222.282, 671.731, 688.173 / **688.173** | 680.908, 679.353, 681.230 / **680.908** |
| 1,000 random 4 KiB reads, µs/read | 1,130.059, 1,105.545, 1,083.124 / **1,105.545** | 837.148, 819.707, 819.792 / **819.792** | 824.620, 809.539, 825.212 / **824.620** |

Materialization samples were:

| Workload | R-SQLite | R-HYBRID | R-LOG |
|---|---:|---:|---:|
| One 100 MiB output, ms | 927.326, 902.221, 912.258 / **912.258** | 717.867, 708.465, 705.452 / **708.465** | 720.007, 713.108, 707.059 / **713.108** |
| 100 × 1 MiB, median per file, ms | 12.105, 12.184, 12.063 / **12.105** | 11.825, 11.212, 11.934 / **11.825** | 11.828, 11.854, 11.845 / **11.845** |

The R-LOG materialization medians above are the per-process `materialize_ms`
and per-file median fields, respectively; the lifecycle's full verification is
reported separately in the timing table below. R-LOG is slightly slower than
R-HYBRID for both materialization workloads and faster than R-SQLite only for
the repeated 1 MiB case.

### Timing, memory, storage, and SQL

The full scrub is inside each edit's total. At 100 MiB the relevant medians were:

| Workload | Lane | Incremental verify | Full scrub | Total |
|---|---|---:|---:|---:|
| Create | R-SQLite | — | 926.990 | 2,262.619 |
| Create | R-HYBRID | — | 1,035.590 | 2,417.892 |
| Create | R-LOG | — | 1,334.577 | 2,686.406 |
| One-byte edit | R-SQLite | 65.492 | 946.723 | 1,039.817 |
| One-byte edit | R-HYBRID | 4.411 | 1,026.272 | 1,056.224 |
| One-byte edit | R-LOG | 982.034 | 1,349.275 | 2,361.828 |
| Three edits | R-SQLite | 187.634 | 2,840.827 | 3,094.379 |
| Three edits | R-HYBRID | 11.806 | 3,075.597 | 3,170.106 |
| Three edits | R-LOG | 2,958.812 | 4,069.295 | 7,121.735 |

The R-LOG verifier's incremental field is already almost a full-log scan for a
one-byte edit and grows to 2,959 ms for three edits. The stable log removes
carrier-file churn and keeps RSS low, but the chained digest and referenced
record validation make it a poor fit for the mandatory full-verification edit
contract.

| Workload / size | R-SQLite RSS | R-HYBRID RSS | R-LOG RSS | R-SQLite storage | R-HYBRID storage | R-LOG storage |
|---|---:|---:|---:|---:|---:|---:|
| Create 100 MiB | 16.0 MiB | 4.3 MiB | 4.2 MiB | 10.4 MiB | 10.2 MiB | 10.2 MiB |
| One-byte edit 100 MiB | 21.2 MiB | 9.6 MiB | 9.6 MiB | 10.5 MiB | 10.4 MiB | 10.4 MiB |
| Three edits 100 MiB | 93.5 MiB | 12.6 MiB | 12.5 MiB | 102.3 MiB | 101.0 MiB | 101.0 MiB |
| Read 100 MiB | 93.5 MiB | 8.3 MiB | 8.1 MiB | 102.3 MiB | 100.5 MiB | 100.5 MiB |
| Materialize 100 MiB | 92.7 MiB | 6.0 MiB | 6.1 MiB | 102.3 MiB | 100.5 MiB | 100.5 MiB |
| 100 × 1 MiB | 6.1 MiB | 4.0 MiB | 4.3 MiB | 1.2 MiB | 1.2 MiB | 1.2 MiB |

At 100 MiB the operation statement/row/transaction counts were:

| Workload / lane | Operation statements / rows read / rows inserted / transactions / commits | Full statements / rows read / rows inserted / transactions / commits |
|---|---|---|
| Create / R-SQLite | 1,371 / 683 / 688 / 1 / 1 | 1,371 / 683 / 688 / 1 / 1 |
| Create / R-HYBRID | 2,048 / 683 / 689 / 1 / 1 | 2,731 / 1,366 / 689 / 2 / 2 |
| Create / R-LOG | 2,048 / 683 / 689 / 1 / 1 | 2,048 / 683 / 689 / 1 / 1 |
| One-byte / R-SQLite | 711 / 705 / 6 / 1 / 1 | 2,082 / 1,388 / 694 / 2 / 2 |
| One-byte / R-HYBRID | 713 / 706 / 7 / 1 / 1 | 2,761 / 1,389 / 696 / 2 / 2 |
| One-byte / R-LOG | 713 / 706 / 7 / 1 / 1 | 2,761 / 1,389 / 696 / 2 / 2 |
| Three edits / R-SQLite | 2,129 / 2,111 / 18 / 3 / 3 | 3,500 / 2,794 / 706 / 4 / 4 |
| Three edits / R-HYBRID | 2,134 / 2,113 / 21 / 3 / 3 | 4,182 / 2,796 / 710 / 4 / 4 |
| Three edits / R-LOG | 2,134 / 2,113 / 21 / 3 / 3 | 4,182 / 2,796 / 710 / 4 / 4 |

The M7 source-window counters were unchanged: one edit read 2,097,152 source
bytes in one source transaction and three edits read 4,371,981 source bytes in
three source transactions. Read workloads performed 1,000 source reads and
transactions over 92,832,100 source bytes; materialization read 677 objects,
and repeated 1 MiB materialization read 800 objects.

### Correctness and crash recovery

The final three-lane runner passed the carrier self-check:
`bounds=verified`, `carrier_format_check=pass`, `payload_hash=verified`, and
`truncated_record=rejected`. Every retained R-LOG sample passed paired root,
logical-digest, object-count, manifest-count, changed-object-set,
unchanged-identity-set, M7-locality, close/reopen, SQLite-integrity, and
materialization checks against R-SQLite and R-HYBRID.

The three recovery lanes all passed. Baseline counts were `[8,1,1]`,
crash-before-commit returned status `91`, crash-after-commit returned status
`92`, and recovered counts were `[9,2,2]`. R-SQLite quarantined zero orphan
carriers, R-HYBRID quarantined the deliberately uncommitted carrier, and R-LOG
truncated its uncommitted payload-log tail and quarantined zero files. No
unreferenced payload remained.

### Decision and next approach

**Decision: reject R-LOG as the overall replacement.** It is correct, durable,
low-RSS, and a useful crash-recovery pattern, but it is slower than R-SQLite by
18.7% on 100 MiB create, 127.1% on one-byte edit, and 130.2% on three edits.
Its read lane is close to R-HYBRID and its repeated-materialization lane is
slower than both controls. The single-log scan is the bottleneck, not SQLite
metadata or the append itself.

The next non-HYBRID experiment should be a narrow SQLite-native payload-table
variant: preserve the current hash/checksum metadata, but add a rowid-backed
payload table with a unique hash index and test SQLite incremental BLOB reads
against ordinary `SELECT bytes`. The current `efs_cas_objects` table is
`WITHOUT ROWID` with the hash as its primary key, so this requires a schema
variant rather than a misleading call-site tweak. It should be one isolated
branch of the harness, with the same workload matrix and gates, and should be
dropped immediately if rowid/index overhead erases the read/write gain.

Do not implement that schema variant in production yet. The measured result is
clearer than a speculative refactor: after the prepared-statement, timing, and
duplicate-hash cleanups, R-SQLite remains the write/edit winner; R-HYBRID remains
the read/RSS winner; and R-LOG is a correct but rejected third point in the
trade-off space.

## Phase 3 follow-up — npm/native SQLite CAS+CDC boundary using Phase 1 R-SQL

**Date:** 2026-08-15
**Run root:** `/var/folders/s4/xpkmz7wn6yq97w1ls_4f_dfc0000gn/T/layerfs-npm-cas-cdc-73feQx`

This follow-up deliberately inherits the Phase 1 R-SQL persistence assets. It
uses the same flat SQLite schema, SQL text, BLAKE3 domains, FastCDC profile,
`BEGIN IMMEDIATE`/`COMMIT` transaction, `synchronous=FULL`, WAL settings,
head update, close/reopen verification, and explicit `wal_checkpoint(TRUNCATE)`.
The npm project is only the workload and uses the pinned native
`sqlite3@6.0.1` addon built from source. This is therefore a CAS-policy
experiment on the R-SQL lane, not a Node-SQLite-versus-Rust-SQLite database
implementation A/B.

### Workload and policies

The run covered the requested scenarios: A, empty install; B, two independent
installs; C, two branches from an installed dependency tree followed by
install/rebuild; D, repeated install, SQLite script, log append, source edit,
and rebuild boundaries; and E, a volatile-path burst. Each policy published a
complete root at every boundary:

- `full_cdc`: recan and rechunk every file;
- `incremental_cdc`: reuse prior manifest references when the path metadata key
  is unchanged;
- `volatile_whole_file`: use whole-file objects for npm cache, logs, build
  output, and the local SQLite database. Native CoW was unavailable, so this is
  the closest supported fallback and intentionally reads the whole volatile
  file into memory.

The experiment-only metadata key uses type, size, mode, and a 100 ms-quantized
mtime. That removes sub-millisecond timestamp rounding introduced by the
macOS branch-copy operation; an implementation should use filesystem change
events or content hashes when same-size edits inside that window matter.

### Measured findings

- **A, first install:** all policies read 29,408,998 source bytes and admitted
  1,985 new objects for 26,190,388 unique payload bytes. Checkpoint time was
  251.27–352.67 ms; the first install is expected to be a full scan for every
  policy.
- **B, independent workspaces:** the second independent install admitted zero
  new objects and reused all 1,265 object references already present in its
  policy database; unique payload stayed at 11,130,778 bytes. This is content
  deduplication across independent installs, not a shared workspace shortcut.
- **C, existing dependency tree:** each branch rebuild admitted 936 new then 8
  new objects, while incremental CDC classified 87 paths as changed and read
  20,001,582 bytes versus full CDC's 904 paths and 29,409,361 bytes. The
  resulting unique payload was identical between full and incremental policy
  for each branch, showing that the speedup came from avoiding unchanged-file
  reads while retaining the same CAS object identities.
- **D, repeated boundaries:**

  | boundary | full CDC source bytes | incremental source bytes | whole-file source bytes | full ms | incremental ms | whole-file ms |
  |---|---:|---:|---:|---:|---:|---:|
  | SQLite script | 34,612,855 | 8,206 | 8,206 | 277.68 | 160.15 | 149.83 |
  | log append | 34,612,876 | 21 | 21 | 262.43 | 157.19 | 151.80 |
  | source edit | 34,612,886 | 20 | 20 | 265.11 | 160.65 | 152.02 |
  | rebuild | 34,615,242 | 7,253,529 | 7,253,529 | 293.75 | 173.34 | 167.95 |

  The one-file edits are therefore not constant-time in the absolute sense—the
  fixed root/metadata/verification work remains—but incremental CDC makes the
  source-read component local to the changed paths.
- **E, volatile burst:** full CDC read 34,615,277 bytes; incremental and
  whole-file policies read 112,279 bytes across 15 changed paths. Whole-file
  mode used 2,357 refs instead of incremental CDC's 2,601 but retained 21,289
  more unique payload bytes in this run, so it is a hot-path reference fallback,
  not automatically the smallest retained representation.
- **Dropping intermediate checkpoints:** pruning D to its final root reduced
  live payload from 31,399,914 to 31,374,574 bytes for incremental CDC and from
  31,421,203 to 31,395,863 bytes for whole-file mode. The SQLite main file did
  not shrink because this maintenance path does not run `VACUUM`; it remained
  37,629,952 and 37,154,816 bytes respectively. WAL was 0 bytes after the
  explicit checkpoint and SHM was 32,768 bytes.

### Correctness and decision

All 36 R-SQL checkpoints passed manifest/logical digest verification, object
length checks, root-member checks, foreign-key integrity, close/reopen
visibility, and the post-run database integrity check. All three rollback
checks passed: full restore after the first install, source-only restore, and
restore before later volatile changes. The local SQLite application database
was closed before checkpointing, so this validates byte-level file rollback,
not rollback of an arbitrary live SQLite process.

Decision: publish a complete logical checkpoint after every mutating hook when
rollback/replay semantics require it; use incremental changed-path CDC as the
default; classify npm cache, logs, build output, and local SQLite files as
volatile; use native CoW or a whole-file reference on the hot path when
available; and promote volatile paths to ordinary CAS/CDC at a final durable
checkpoint or before long retention. Keep full-workspace CDC as a correctness
and repair baseline.

The npm install/rebuild wall times, native compilation, and package-cache
effects are recorded separately in the raw JSON and are not mixed into the
R-SQL checkpoint timings. This is one bounded run, so the numbers are measured
evidence for the policy decision rather than a statistical performance claim.

Reproducible artifacts: [`npm-cas-cdc.md`](../npm-cas-cdc/results/npm-cas-cdc.md),
[`npm-cas-cdc.json`](../npm-cas-cdc/results/npm-cas-cdc.json), and
[`npm-cas-cdc.csv`](../npm-cas-cdc/results/npm-cas-cdc.csv).
