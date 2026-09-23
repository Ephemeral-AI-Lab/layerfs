# #237: exact pack-fit buffering in the seven-COMMIT 10k Init

> **Status:** Research; preregistered before a placement source edit or new
> public performance sample. This page will append every attempt, including
> failures and `NOT_RUN`. SQLite database pages stay **4,096 B** and the C1
> default file cutoff stays **128 KiB**.

## Question and fixed identities

The seven-COMMIT algorithm's separate
[count diagnostic](seven-commit-pack-count-diagnostic.md) measured **1,259
file-Save pack creations and 1,024 appends** on five 64-MiB preparation
waves. Its queue drained 1,190 times at the encoded-byte bound, 124 times
at a lane switch and six times at wave/final boundaries; demanded reads
caused zero drains. The actual owner connection saw **82,483 SQLite
`CACHE_WRITE` events and zero spills**, with 95.690 ms in whole pack writes
and 271.961 ms in COMMIT. This was a dirty reporting-only identity, not a
speed control or proof that all 1,024 appends can be removed. The earlier
[bounded-wave public pair](bounded-wave-experiment.md) achieved seven
COMMITs but slowed the caller, enlarged the Store and increased sampled RSS.

The **new matched control** will use this worktree's committed aggregate
pack/queue-count source at `ea56de525` (product algorithm inherited from
`343e4e029`; the temporary owner-pager FFI has been restored). The candidate
will branch from that exact control and change only C2 queue/placement plus
directly affected external tests and architecture docs. Both arms print the
same once-per-file-Save `SaveOutcome` counts and use the same H3 driver and
fixture. The candidate's exact commit and source/binary/harness seals will
be pinned in the retained receipts before any comparison is accepted.

## Prospective physical treatment

Keep at most **one pending receiving pack per ordinary, native and whole-file
lane** inside a preparation wave. Each lane retains no more than 256 KiB of
encoded group bodies and 512 locator members, for a simultaneous ceiling of
768 KiB encoded plus bounded member metadata. Pooled metadata and singleton
groups remain immediate. No queued group crosses a COMMIT.

`LanePlacement` already owns the exact append/new decision through
`pack::layout::append_fits`. The queue will project the lane's current open
pack assembled length/group count plus its pending groups, using the same
reserved directory/header and group-count rules. A new group that fits joins
the pending pack. A group that **does not fit** causes that lane's pending
groups to be materialized through the existing `select_many`/incremental
BLOB writer **once**, then starts the next pending pack. A pack that was
partially written at a prior wave boundary may require one append in the new
wave; demanded same-save reads and the 512-row cap can also force partial
materialization. This treatment seeks to remove avoidable repeated appends
*within* a wave, not legitimate boundary writes or the near-byte-floor
capacity-drain count itself.

No locator row may precede the SQLite `object_packs` row it references.
Before exact reuse, same-save read, advisory/winner delta-base acquisition
or collision validation demands queued bytes, materialize the relevant
pending pack; for this first candidate a demand may drain all lanes, as the
existing barrier already does. Direct-reference availability may recognize
accepted open/queued identities. Drain all pending packs before wave
validation, candidate-index flush and COMMIT; the transaction continues to
commit before Store arbitration is released. On definite failure, pack rows
and locators roll back together. No schema, pack framing, codec, canonical
object identity, worker count, 8,192-object/64-MiB wave, 128-KiB cutoff or
4-KiB DB page change belongs to this pair. Physical pack-ID order and dense
space may change; that is measured, not assumed equivalent.

## One-sample method and decision

Run exactly **one** release-profile public `namespace-10000` Init per arm,
control first and candidate second, at fresh output/Store paths. Use H3
driver SHA-256
`767292e7b5bfc9a06291239b471247a79474b91ce2ca0db8fc9092807e2c09a4`
with `--independent-source-copy --fixed-operation-identity`, without
`--verify` during performance. The sealed seed-1 fixture is **10,000 files,
300,000,000 logical bytes including the 100-MB anchor**. Each arm gets its
own new writable byte copy; full hash/invalidation and an immediate
nonfaulting whole-input recheck must find **zero resident payload pages**.
Only prepared workspace setup is reused. Product file reads and all Store
writes remain inside the public timer; no cache from a prior run may serve
it. Directory/inode metadata warmed by each copy/setup remains unqualified,
so both rows are exploratory/`INELIGIBLE` under the runner's unchanged cache
contract. Request an exclusive host window before either timed arm. Preserve
failures, telemetry loss, cleanup failure and skipped verification; do not
repeat an arm to improve its number.

After timing, run each arm's **own archived release verifier once** against
its closed Store/History and the sealed manifest. Its wall is separate from
throughput. Compare exact fixed-scope root, complete object-ID digest,
10,101 paths, kind/mode/mtime and SHA-256 of all 300 MB. Measure exact file-
Save pack creations/appends, flush reasons, pack-write/SQL/COMMIT time,
commits **<=8**, maximum transaction rows/bytes and wave lock, caller and
complete-command wall, Service/daemon CPU and sampled RSS, SQLite 4-KiB page
size, apparent/allocated Store bytes, pack capacity/used/spare bytes and
pack count. These timings nest; do not add pack-write time to SQL or infer a
caller saving by subtraction. The previous 64-MiB diagnostic's **1,024**
appends motivate the experiment but are **not** substituted for the new
matched control count.

Mechanism screen: at least **50% fewer matched file-Save pack appends** while
preserving <=8 COMMITs and exact readback. Speed adoption additionally needs
a faster public caller and no worse Store apparent/allocated bytes, pack
capacity/used bytes or material sampled-RSS increase; a sparse #229 history
guard and full final-source checks remain separate required proofs before
product adoption. The historical 518.8-MB/s time is 0.578245 s for the same
300-MB numerator, but its source cache was unqualified. Report it as missed
unless a new qualified row actually reaches it.

Focused pre-sample checks cover exact pack-fit boundaries, interleaved
lanes, queued predecessor and duplicate, same-save reads, rollback,
interleaved writers, pack watermark and reopened bytes. A failed test or
unmet source barrier is retained; if it blocks trustworthy timing, record
`NOT_RUN` rather than rerun or work around it.

## Attempts and result

At preregistration, no candidate source commit, build or public sample existed.
The following single matched attempt completes that prospective pair.

### Exact-pack-fit attempt

The committed control was `ea56de525cf703e3925f761d49c0a8b2ee977129`
(product seal
`434de9ecc7f87c25a86f096d8416b1b712b80910bd99ec40a506a30ab482e250`);
the committed candidate was `e724ce1bf8a03abac5d75405d392a77a4a249fd2`
(product seal
`93b29aaa9bbb93adcac2b49e3b70868873cc2e1056434a3eb8ed8259c6a6b058`).
Both were clean sealed source trees. The harness seal was
`6d9a3e2eb2a1eea1f3f0d63f0df15949d6f3bab103657a824b0a04d34482eff8`,
the driver SHA-256 matched the preregistration, and the fixture manifest was
`c1d7937c9f90d3585558e6d20b35183a737530d386667d08badc3121a6b3d55e`.
The [candidate source/test/architecture diff](evidence/exact-pack-fit/candidate-source-and-tests.diff.gz)
is retained with the [raw evidence manifest](evidence/exact-pack-fit/evidence-manifest.json).
The source diff changed C2 queue/placement and focused tests, not C1's 128-KiB
cutoff or the SQLite 4-KiB page policy.

Exactly one control-first/candidate-second pair ran with the commands below;
`--verify` was omitted from both public performance arms. Each command had a
fresh output path, private Store and independent writable byte copy. There
were no retries or substituted rows.

```sh
python3 docs/roadmap/0.1/0.1.7/evidence/issue237-native-init-research/cold_diagnostic.py --case namespace-10000 --independent-source-copy --fixed-operation-identity --out benchmark-results/fs-bench-pro/issue237-h3-exact-pack-control
python3 docs/roadmap/0.1/0.1.7/evidence/issue237-native-init-research/cold_diagnostic.py --case namespace-10000 --independent-source-copy --fixed-operation-identity --out benchmark-results/fs-bench-pro/issue237-h3-exact-pack-candidate
```

Both source preflights and the immediate pre-timer rechecks found **0 resident
payload pages of 27,503** over 300,000,000 B. Recheck-to-timer gaps were
1.473500 and 1.031000 ms. The copy method was
`independent-byte-copy-v1` for both. Copy/setup and the later verifier were
outside the timed public call. Metadata residency remains unqualified, so
both rows retain the runner's `source-cache-uncontrolled-v1` /
`admission_eligible=false` state. This is not a fully cold acceptance pair.

| One 10k public arm | Control | Candidate | Candidate change |
| --- | ---: | ---: | ---: |
| Public caller | 1.517590917 s | 1.482207834 s | −35.383083 ms raw |
| Throughput, 300,000,000 decimal B / caller | 197.682 MB/s | 202.401 MB/s | +4.719 MB/s raw |
| Complete performance command | 2.541294958 s | 2.427053417 s | −114.241541 ms raw |
| File-Save preparation waves / COMMITs | 5 / **7** | 5 / **7** | unchanged; <=8 retained |
| File-Save pack creations / appends | 1,257 / **1,054** | 1,259 / **21** | appends −1,033 / **−98.01%** |
| Queue capacity / lane / boundary drains | 1,201 / 98 / 6 | 1,264 / 0 / 6 | lane drains removed |
| Whole pack-write region | 101.714279 ms | 92.629665 ms | −9.084614 ms |
| Disjoint Save SQL bucket | 159.880871 ms | 147.020295 ms | −12.860576 ms |
| File-Save COMMIT bucket | 287.169042 ms | 303.847542 ms | +16.678500 ms |
| Pack bytes submitted | 300,879,063 B | 300,854,735 B | −24,328 B |
| Maximum accounted transaction | 8,532 rows / 134,228,654 B | 8,102 rows / 134,163,778 B | within fixed bounds |
| Longest `with_wave` lock hold | 302.340083 ms | 283.034667 ms | −19.305416 ms raw |

The control [receipt](evidence/exact-pack-fit/control/receipt.json) is
**`INCOMPLETE`** because its daemon telemetry lost an event; its public call
nevertheless returned the above root and cleanup of both processes passed.
Its stderr and telemetry are retained. The candidate
[receipt](evidence/exact-pack-fit/candidate/receipt.json) is `DIAGNOSTIC`, with
telemetry and cleanup PASS. Both record performance verifier `SKIPPED`.
The control's `competing_work` also lists another worktree's Service PID
18750 and daemon PID 18751 under `issue237-packfit-control` while this arm's
own Service/daemon were PIDs 18758/18759 under `issue237-cutoff512`. The
candidate snapshot lists no other LayerFS workload. Therefore the raw caller
difference is **not a causal speed result**. Neither arm may be repeated to
obtain a cleaner number. The Service named `history.import_files` child was
1.300000250 → 1.257865167 s, but its timing is not a substitute for the
public caller and the Save's nested timer buckets must not be added.

The independently reopened [control](evidence/exact-pack-fit/control/independent-readback.json)
and [candidate](evidence/exact-pack-fit/candidate/independent-readback.json)
readbacks each ran **once**, after timing, with that arm's own archived
release verifier binary. Both returned PASS for **10,101 paths, 10,000 files,
101 directories and all 300,000,000 bytes**, including kind, mode, mtime and
SHA-256. Their walls were 3.122564125 and 2.200725709 s, outside the
performance timer. The original random History cursor capability was not
retained by the performance runner; the read-only verifier used a fresh
nonzero capability and no History pagination, as in the earlier bounded-wave
proof. Public roots match exactly at
`e7850f75a65309568e3454f7ab962aa16952d0c02218a55f42f3e2fd44ddf673`.
The closed Stores have the same complete ordered object-ID digest
`4a9f14a45482c2ae3962f633b3655125278dc816ee9f499803302d76aa241789`.

| Closed Store and sampled resources | Control | Candidate | Candidate change |
| --- | ---: | ---: | ---: |
| SQLite page size | **4,096 B** | **4,096 B** | fixed |
| Apparent Store bytes | 333,643,776 | **334,172,160** | **+528,384 B** |
| Allocated Store bytes (`st_blocks`) | 335,609,856 | 335,609,856 | 0 B |
| Pack count | 1,262 | **1,264** | **+2** |
| Pack capacity / declared used | 330,825,728 / 305,969,503 B | 331,350,016 / 305,977,727 B | **+524,288 / +8,224 B** |
| Pack spare | 24,856,225 B | 25,372,289 B | +516,064 B |
| Object rows | 24,683 | 24,683 | 0 |
| Sampled Service maximum RSS | 231,997,440 B | **248,889,344 B** | **+16,891,904 B** |
| Service/daemon lifecycle user CPU | 1.091254 s | 1.092873 s | +0.001619 s |
| Service/daemon lifecycle system CPU | 1.190676 s | 0.987701 s | −0.202975 s |

The RSS maxima are local-monitor samples (16/15 samples, largest gaps
104.933/103.995 ms) without both phase boundaries; the receipt's phase RSS
peak is null. CPU and sampled RSS are also affected by the unequal external
work visible during the control arm. The closed
[geometry](evidence/exact-pack-fit/control/geometry.json) records each Store's
page, pack and object totals; the private Store/History files remain under the
ignored output directories and their SHA-256 values are in the evidence
manifest. The exact file-Save count line is in the retained control stderr
and candidate receipt telemetry diagnostics. The appends reduction is a
reproducible count mechanism, not a demonstrated 1,033-call wall saving.

Focused `c2_bulk_admission` (5/5), `pack_locator` (10/10) and
`pack_watermark` (2/2) passed for candidate source. The same targeted command
also ran `visibility`: 6/9 passed and three tests failed because their
5-MiB setup assumes an early committed pack before `finish`, while the
64-MiB preparation wave does not close at 5 MiB. They saw `ceiling=0,
highest=0`, not a wrong published object or failed reopened readback. This
fixture premise was already stale for the seven-COMMIT control algorithm;
the failures remain a verification gap and were not patched or rerun for this
experiment. Full Core tests/clippy/fmt and the sparse #229 guard were not run
for candidate adoption; no release admission is claimed.

**Decision:** The candidate met the 50% append reduction and <=8-COMMIT
mechanism screens by a large margin, but failed the preregistered no-worse
Store apparent/pack capacity/used and sampled-RSS screens. The control arm's
telemetry loss and genuine concurrent workload, plus unqualified metadata
cache, also prevent a causal speed claim. Keep this in the isolated research
branch; do not adopt it into the root product. The raw 202.401 MB/s candidate
still misses the historical 518.8 MB/s figure, whose source cache was itself
unqualified. The remaining time is dominated by work outside avoidable
pack appends; investigate measured construction, Store commit and import
phases before another treatment.
