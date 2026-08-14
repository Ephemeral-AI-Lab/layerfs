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
