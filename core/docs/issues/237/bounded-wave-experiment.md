# #237: bounded C2 preparation waves at 10k

> **Status:** Research; preregistered before the candidate source edit and
> completed with one control/candidate public pair. The commit target passed,
> while throughput, RSS and Store space worsened. SQLite database pages stayed
> **4,096 B** and the C1 default cutoff stayed **128 KiB**.

## Why this candidate

The integrated source `480de7464` accepts 512 objects / 4 MiB canonical
bytes per C2 preparation wave. `MutationOwner::with_wave` holds Store
arbitration, does all dependency, group and collision work for that wave under
one SQLite write transaction, then commits **before releasing arbitration**.
The earlier [D13 file Save](pager-10k.md#live-save-counters) reported **79
commits over 74 waves**, and an earlier D12 diagnostic charged **227.521 ms**
to all file-Save `COMMIT` calls. These are separate source identities from the
integrated baseline: they motivate the mechanism, not a predicted speedup.

The [Save-wide transaction idea](single-transaction-experiment.md) was rejected
without a build or public sample. It would release arbitration with an SQLite
write transaction still open. With zero busy timeout, another writer could fail
between waves. Its exact uncommitted diff is retained as a failed feasibility
approach. This candidate preserves `with_wave`'s commit-before-unlock rule.

## One prospective treatment

One product change combines two parts of the same commit-cadence mechanism:

1. Raise the **preparation** batch to at most **8,192 objects / 64 MiB minus
   one byte**. Keep the sealed-group FIFO separately at **512 locator rows /
   256 KiB encoded bodies**, including its dependency flush barriers. Widen
   transaction accounting to **16,383 rows / 192 MiB minus one byte** for one
   wave and fail closed if a completed wave exceeds either figure; record the
   actual maximum rows and bytes. Keep all worker,
   source, pack grammar, 128 KiB file cutoff and 4 KiB DB page settings fixed.
2. During `finish_inner`, seal remaining lane groups and publish under one
   arbitration hold and one write transaction. Every pending group must be
   placed; all `wave_rows` written by the final seals must be collision-validated
   before publication. Reset the in-wave flag on every error so rollback and
   cleanup can proceed. The final COMMIT remains inside the public timer.

From 24,562 file-Save accepts, the 8,192-object cap implies **at least three
waves**, while roughly 300 MB of canonical file content implies at least five
64 MiB waves. The larger object cap leaves room for byte-driven waves that
flush before 4,096 objects; a 4,096-object cap would make six waves possible
only with essentially no early byte flush. If at most six waves suffice, slot
reservation plus those wave commits plus one coalesced final publication yields
at most **eight file-Save commits**. The primary mechanistic
target is exact `SaveOutcome.commits <= 8`, at least a 90% reduction from the
old ~79–80. The bound may miss if size mix causes more than six waves, a
dependency forces an extra write step, or the final seal cannot coalesce. A
miss remains a recorded outcome; the arm is never rerun for a better count.

The larger wave may increase pending canonical memory, SQLite MEMORY-journal
RSS, write-lock hold time and competing-writer latency. Those costs are part
of the decision, and a fast single-writer 10k row cannot establish multiwriter
product fitness. Definite failures must still roll back the open wave or final
transaction; unknown COMMIT outcomes stay unknown, without retry.

## Matched one-sample protocol

The instrumented control identity is `c5d9e8af3`, descended from integrated
source `480de7464` with **identical once-per-file-Save count-only Service
instrumentation**; the candidate layers only this treatment over that
control. The [shared diagnostic diff](evidence/single-transaction/shared-saveoutcome-instrumentation.diff.gz)
adds aggregate `SaveOutcome.preparation_waves`, peak accounted transaction
rows/bytes and longest `with_wave` arbitration hold. It prints these with
exact commits, inserted objects, COMMIT nanoseconds, BEGIN/pack-cursor
nanoseconds and submitted pack bytes **once after file Save finish**, still
inside the public caller timer. There is no per-wave stderr I/O. The
control's `max_wave_lock_ns` covers preparation waves; the candidate's also
covers coalesced finalization, so report that scope difference. Source and
build identities will be recorded for both arms. The copied H3 driver SHA-256 is
`767292e7b5bfc9a06291239b471247a79474b91ce2ca0db8fc9092807e2c09a4`.

Run one release-profile public `namespace-10000` Init per arm with
`--independent-source-copy --fixed-operation-identity` and fresh output/Store
paths. Both arms use the sealed seed-1 fixture of **10,000 files and
300,000,000 logical bytes including the 100 MB anchor**. Each gets an
independent writable byte copy of the prepared workspace. The driver hashes,
invalidates and immediately checks all source payload pages at zero residency
before the timed call. New paths prevent metadata reuse *from the preceding
sample*, but copying and hashing may leave directory/inode metadata cached.
`/usr/sbin/purge` failed with `Operation not permitted`; the driver has no
purge option. Therefore metadata cache is **unqualified**, and both rows stay
exploratory/`INELIGIBLE`, never cold admission PASS. The product's source read
and Store writes remain inside the public timer. Verification runs separately.
Coordinate a host window before either sample; record any concurrent work.
Do not repeat a sample or drop a failure.

Compare exact root and full object-ID digest under the fixed stack/scope.
Reopen each Store and verify every manifest path, kind/mode/mtime and all 300 MB
of content. Record caller and complete-command wall, Service/daemon CPU and
RSS, SQLite page/cache/spill observations where available, closed Store
apparent/allocated bytes and pack capacity/used/spare bytes. A dense 10k
result does not prove #229 sparse-history compactness; that remains unrun until
measured on its own fixture. Record the **0.578245 s** / 518.8 MB/s historical
time as missed if the candidate caller is slower.

Focused candidate checks before the public arm: multiwriter behavior,
same-save reads after a wave, rollback after private output, pack-watermark
allocation, reopened readback and the existing persistence-failure boundary.
Report test failures as such, including any large-wave lock or RSS regression.
At final identity, run the owning Core checks once; none is a substitute for
the independent full manifest verifier.

## Attempts and outcome

One control and one candidate public sample ran in the owner's cleared host
window, control first. Both commands used the exact H3 driver named above,
`--case namespace-10000 --independent-source-copy
--fixed-operation-identity`, `--out` set respectively to
`benchmark-results/fs-bench-pro/issue237-h3-bounded-wave-control` and
`benchmark-results/fs-bench-pro/issue237-h3-bounded-wave-candidate`. Neither
used `--verify` during the public run; each runner receipt records verifier
`SKIPPED`. No sample was repeated. The [evidence manifest](evidence/bounded-wave/manifest.json)
hashes copied raw receipts, logs, cold sidecars, independent readback proofs,
the [candidate source/test diff](evidence/bounded-wave/candidate-source-and-tests.diff.gz),
and the two private Store files. The Store files and fresh source copies remain
under those ignored output directories, not in Git.

| 10k outcome | Instrumented control `c5d9e8af3` | Candidate `343e4e029` | Candidate change |
| --- | ---: | ---: | ---: |
| Public caller | 1.364419666 s | **1.439506625 s** | **+75.086959 ms**, slower |
| Throughput, 300,000,000 decimal B / caller | 219.874 MB/s | **208.405 MB/s** | −11.469 MB/s |
| Complete performance command | 2.314021583 s | 2.452547750 s | +138.526167 ms |
| Exact file-Save `SaveOutcome.commits` | **80** | **7** | −73 / **−91.25%**, target met |
| File-Save preparation waves | 75 | 5 | −70 |
| File-Save COMMIT total | 228.067586 ms | **276.718708 ms** | **+48.651122 ms** |
| File-Save BEGIN/pack-cursor total | 1.994460 ms | 0.279624 ms | −1.714836 ms |
| Largest accounted transaction | 634 rows / 8,507,482 B | **8,354 rows / 134,192,375 B** | Inside candidate 16,383-row / 201,326,591-B limits |
| Longest `with_wave` arbitration hold | 22.279167 ms | **267.891833 ms** | +245.612666 ms; candidate includes finalization |
| File-Save pack bytes submitted | 300,880,428 B | 300,878,967 B | −1,461 B |

The control [receipt](evidence/bounded-wave/control/receipt.json) is
`INCOMPLETE`: the daemon lost one telemetry event, although its public call
returned a confirmed root and Service/daemon cleanup passed. The candidate
[receipt](evidence/bounded-wave/candidate/receipt.json) is `DIAGNOSTIC` with
telemetry and cleanup PASS. Both have the runner's
`source-cache-uncontrolled-v1`, `admission_eligible=false` label. Each arm
used a different new writable byte copy; both full hash/invalidation
[preflights](evidence/bounded-wave/control/cold-preflight.json) and immediate
[rechecks](evidence/bounded-wave/candidate/cold-recheck.json) found **0
resident payload pages of 27,503**. Recheck-to-timer gaps were **6.117042
ms** and **1.192500 ms**. Directory/inode metadata residency was unqualified
after copy/setup, and the host could not run `purge`; these are exploratory
comparisons, not cold admission results. The receipts retain the same
background Docker processes in `competing_work`; no root-task build or timed
measurement ran in this window.

The named Service `history.import_files` child was **1.165398834 →
1.210108708 s** (+44.709874 ms), and `history.import_finish_save` was
**3.866000 → 69.518167 ms** (+65.652167 ms). They are Service-clock children,
not a decomposition of the public daemon caller; the Save's COMMIT total
overlaps them and must not be added to either. The larger finalization child
and the higher COMMIT total are consistent with larger transaction work,
but one pair cannot prove which part caused the 75.087 ms caller slowdown.
The shared diagnostic did **not** emit the full owner-accept profile, so that
profile is `NOT_MEASURED` for this pair; a prior source identity's count profile
is not substituted for it.

Lifecycle Service/daemon CPU was **1.003643/0.861452 s user/system** for
control and **1.059972/0.930702 s** for candidate. The Service local
monitor's sampled maximum RSS was **59,965,440 → 247,103,488 B** (about
4.12×); its 14–15 samples had largest gaps about 108–109 ms and did not
cover both phase boundaries. The receipt's rusage peak is a lifetime value,
so `rss_phase_peak=null`; neither figure proves a complete phase peak or
isolates SQLite page cache from heap. The candidate nonetheless has a
material sampled-RSS and lock-hold regression. The actual timed connection's
cache hit/miss/spill counters were `NOT_MEASURED` in this pair.

| Closed Store geometry | Control | Candidate | Difference |
| --- | ---: | ---: | ---: |
| SQLite page size | 4,096 B | 4,096 B | fixed |
| Apparent file bytes | 333,647,872 | **334,716,928** | **+1,069,056 B** |
| Allocated file bytes reported by `st_blocks` | 335,609,856 | 335,609,856 | 0 B |
| Pack count | 1,262 | **1,266** | +4 |
| Pack BLOB capacity | 330,825,728 B | **331,874,304 B** | +1,048,576 B |
| Declared pack used | 305,969,620 B | **305,986,031 B** | +16,411 B |
| Pack spare | 24,856,108 B | **25,888,273 B** | +1,032,165 B |
| Object rows | 24,683 | 24,683 | 0 |

The closed [control](evidence/bounded-wave/control/geometry.json) and
[candidate](evidence/bounded-wave/candidate/geometry.json) Stores have the
**same complete ordered object-ID SHA-256**
`4a9f14a45482c2ae3962f633b3655125278dc816ee9f499803302d76aa241789`.
The public roots also match exactly:
`e7850f75a65309568e3454f7ab962aa16952d0c02218a55f42f3e2fd44ddf673`.
After the timed runs, each arm's own archived release verifier binary was
run **once** against its reopened Store, History and sealed manifest. Both
[control](evidence/bounded-wave/control/independent-readback.json) and
[candidate](evidence/bounded-wave/candidate/independent-readback.json)
returned `PASS`: **10,101 paths, 10,000 files, 101 directories and all
300,000,000 logical bytes** with kind/mode/mtime and SHA-256 checked. Their
3.093089/2.173513 s walls are outside the public and complete-performance
times. The original randomly chosen History cursor capability was not
retained by the performance runner; the separate read-only verifier used a
fresh nonzero capability and did no History pagination, so it did not replay
or claim that original capability. The manifest SHA-256 and root identity
were unchanged.

Focused candidate checks passed at `343e4e029`: `c2_bulk_admission` 4/4,
`multi_writer` 5/5, `pack_watermark` 2/2, `persistence_failure` 8/8 and
`storage_limits` 5/5, including same-save reads, rollback and reopened
readback. The combined locked Cargo command completed in **14.412 s**;
the control release build took **17.355 s** (first use), and the candidate
changed-product release build **3.162 s**, each under the 30 s build bound.
The full Core workspace test/Clippy suite and a matched #229 sparse-history
Store were **not run** for this rejected treatment. Old-Store Service behavior
was not separately exercised. These limits are retained, not inferred away
from the dense 10k proof.

**Decision:** the mechanism met its <=8-commit gate and preserved canonical
root/readback, but it **did not improve throughput**. It increased caller time,
sampled RSS, longest lock hold, apparent Store bytes and pack slack. Keep it as
an isolated measured research branch; do **not** adopt this candidate into the
main #237 product tree. Its **1.439507 s** caller remains **0.861262 s** slower
than the historical 0.578245 s time associated with 518.8 MB/s, and that
historical row itself lacked a cold-cache contract. Reducing COMMIT count
alone was the wrong next speed optimization on this host and route.
