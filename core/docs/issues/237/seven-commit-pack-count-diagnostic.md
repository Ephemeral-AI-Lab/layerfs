# #237: 64-MiB-wave pack and pager count diagnostic

> **Status:** Research; one preregistered count-driven diagnostic completed.
> It measures the existing seven-COMMIT algorithm, **not** a replacement
> performance arm. The temporary owner-pager FFI was restored after the run.
> SQLite pages remained **4,096 B** and the C1 cutoff remained **128 KiB**.

## Question and identity

The retained [bounded-wave gate sample](bounded-wave-experiment.md) at product
source `343e4e029` had **7 file-Save COMMITs / 5 preparation waves** but a
slower public caller, larger Store and about 247 MB sampled Service RSS. Its
once-per-Save line did **not** print `SaveOutcome.pack_appends` or full pack-
write/SQL time. A separate [4-MiB-wave diagnostic](evidence/per-lane-pack/adjacent-4mib-profile-receipt.json)
found 1,089 file-Save appends and 1,153 encoded-byte queue drains, but that
different source cannot serve as a count on the seven-COMMIT route. Before
choosing the [exact pack-boundary proposal](wave-wide-pack-proposal.md), one
new diagnostic will measure the **actual 64-MiB-wave** population.

The base algorithm is the active seven-COMMIT product in `3e321080a`.
Apply the already archived [aggregate-count source diff](evidence/per-lane-pack/untimed-shared-counts.diff.gz)
as a committed, distinct reporting identity. It prints exact file-Save pack
creates/appends, pack-write and SQL nanoseconds, COMMITs/waves, transaction
row/byte maxima, and queue drains by cause **once** at file-Save completion.
It changes no placement, worker, wave, queue, SQLite cache or page policy.
If practical, add the same reporting-only owner-connection pager probe used
in [D13](../../../../docs/roadmap/0.1/0.1.7/evidence/issue237-native-init-research/d13-pager-instrumentation.diff.gz):
read `CACHE_USED/HIT/MISS/WRITE/SPILL` without reset after acquisition, after
each completed wave and after final publication, then print one aggregate
summary. Pinned rusqlite lacks a safe wrapper; D13's SQLite FFI is **temporary
diagnostic source only**, archived as an exact diff and removed afterward.
It is not adopted into production or presented as a passing Core boundary
check. If it cannot compile or changes the product algorithm, retain the
failed attempt and run **no** public diagnostic with a substituted method.

## One fresh public diagnostic

Run the exact H3 driver SHA-256
`767292e7b5bfc9a06291239b471247a79474b91ce2ca0db8fc9092807e2c09a4`
once with `--case namespace-10000 --independent-source-copy
--fixed-operation-identity` and one new `--out` path; omit `--verify`.
The seed-1 fixture is **10,000 files / 300,000,000 logical bytes including
the 100 MB anchor**. The prepared master may be reused only to create a
new independent writable byte copy. Full hash/invalidation plus immediate
nonfaulting whole-input check must show **0 resident source payload pages**
before the timed call. Directory/inode metadata cache after setup is
unqualified, so the runner's `source-cache-uncontrolled-v1` row remains
exploratory/`INELIGIBLE` for admission. No previous-run warm cache may serve
the operation. SQLite page size, 128-KiB C1 cutoff, 8,192-object/64-MiB
preparation wave, <=8 file-Save COMMITs and physical pack grammar are fixed.
Do not rerun this instrumented identity for a different time or count. Ask
for an exclusive host window before any timed call and retain failures.

Primary evidence is **exact file-Save pack creates/appends**, queue flush
reasons, pack-write and SQL time, and actual-owner pager write/spill/cache
used per completed wave. Record identities, output, source preflight/recheck,
root status, telemetry/cleanup, caller/complete-command wall (diagnostic
only), CPU and sampled RSS, 4-KiB Store geometry and all nonpassing lines.
Per-wave pager readings are boundary samples, not exact within-wave peaks;
SQLite write events are not physical device bytes. COMMIT and nested pack
timers overlap with their parent spans and cannot be added as independent
caller savings. Full verifier is `SKIPPED` by default; no readback or release
PASS follows from this diagnostic. This one diagnostic is beside, never a
promotion or replacement of, the original `343e4e029` gate receipt.

## Attempts and result

Exactly one run used the preregistered command with
`--out benchmark-results/fs-bench-pro/issue237-sevencommit-pack-count-a`.
The aggregate hook was committed at `4854465cd` over the same seven-COMMIT
algorithm. The temporary [pager diff](evidence/seven-commit-count/pager-temporary.diff.gz)
had SHA-256
`0417b1ca4016510b8d3f7a0b703fe95bb70db12876daa376664e74673b28425d`;
the [receipt](evidence/seven-commit-count/receipt.json) records base source
commit `4854465cd3785913133ad2a8d0813e9cfdc66e73`, `source_dirty=true`,
the sole dirty path `cas/store.rs`, instrumented product seal
`0c9a42c61e59ffc8eb64354cd6ce58d9dff16e4b7f502ed3f7cb751f981ed98b`
and harness seal
`6d9a3e2eb2a1eea1f3f0d63f0df15949d6f3bab103657a824b0a04d34482eff8`.
The [manifest](evidence/seven-commit-count/manifest.json) hashes all copied
raw receipts, sidecars, telemetry, temporary diff and the private Store/History
files. The Store remains under the ignored output path in this worktree. The
temporary FFI source was restored to `4854465cd` after copying the evidence;
it is not an adopted product or a passing product-boundary check.

The 10k row is **`DIAGNOSTIC`**, with one sample, a confirmed public root,
telemetry/cleanup PASS, separate verification **`SKIPPED`**, and the runner's
`source-cache-uncontrolled-v1`, `admission_eligible=false` label. The raw
public call took **1.429112750 s** and the complete performance command
**2.393916459 s**; these are diagnostic observations, **not** a speed pair
against `343e4e029`. The fresh independent byte copy's full hash/invalidation
and immediate nonfaulting recheck both found **0 resident payload pages of
27,503** across 300,000,000 B; the latter ended **1.799167 ms** before the
timer. Directory/inode metadata cache remains unqualified. Build mode was
`changed-product`, release build wall **3.356035167 s**, PASS. No public
sample was repeated or substituted for the older seven-COMMIT gate receipt.

| Actual file-Save count/work | One 64-MiB-wave diagnostic |
| --- | ---: |
| Inserted / exact reused objects | 24,364 / 198 |
| Preparation waves / COMMITs | **5 / 7** |
| Pack creations / appends | **1,259 / 1,024** |
| Queue drains: capacity / lane / boundary / demand / immediate | **1,190 / 124 / 6 / 0 / 0** (1,320 total) |
| Pack bytes submitted | 300,878,973 B |
| Whole `write_pack` calls, including their SQL and cache invalidation | **95.690169 ms** |
| Disjoint Save SQL / COMMIT buckets | **148.078499 / 271.961084 ms** |
| BEGIN plus next-pack cursor | 0.187207 ms |
| Largest accounted transaction | 8,333 rows / 134,219,218 B |
| Longest completed `with_wave` arbitration hold | 265.564625 ms |

The queue's byte/capacity cause is **90.15% of its 1,320 drain events**;
lane switching is **9.39%**. The adjacent 4-MiB profile's 1,089 appends and
1,347 drains are different-source diagnostic facts, not a matched change
attributable to the larger wave. The exact current **1,024 appends** are a
material call population for an exact pack-fit experiment, while this row
does **not** prove how many are avoidable: a pack can legitimately append
after a wave boundary, a demanded read, or a partially filled last pack.
The 95.690-ms `write_pack` region contains some of the 148.078-ms SQL bucket,
so those values cannot be added. Pack creation, locator insertion and reading
source bytes remain required even if every avoidable append were removed.

The actual file-Save owner connection reported `PRAGMA page_size=4096`,
`cache_size=2000` pages, `cache_spill=20000` pages and `mmap_size=0`.
All **five** completed-wave pager readings succeeded (`invalid=0`). Across
acquisition through final publication it recorded **82,483 `CACHE_WRITE`
events and zero `CACHE_SPILL` events**; the largest completed-wave write
delta was **18,399** and largest spill delta **0**. The largest boundary
`CACHE_USED` reading was **8,767,488 B**. The write count × 4,096 is
337,850,368 page-event bytes, **not** distinct pages, physical device bytes
or a durability claim. The boundary used reading is not an exact within-wave
RSS or page-cache peak. This row gives no spill-pressure reason to change the
SQLite cache policy as part of the next pack experiment. The independent
[closed Store geometry](evidence/seven-commit-count/geometry.json) also
confirms 4,096-B pages: **334,163,968 B apparent**, 1,264 packs,
331,350,016 B reserved pack capacity, 305,977,861 B declared used and
24,683 object rows. Its complete ordered object-ID digest matches the
earlier seven-COMMIT fixture, but a full reopened oracle was not requested
for this count-only identity.

Service/daemon lifecycle CPU was **1.051898 s user + 0.918020 s system**.
The Service local monitor sampled a maximum **238,338,048-B RSS** in 15
samples with a 104.613-ms maximum gap and no boundary coverage; the receipt's
phase-local RSS peak remains null. Neither is a complete memory peak. This
diagnostic changes only reporting and did not change 4-KiB page size, 128-KiB
cutoff, worker count, queue/pack algorithm or the seven-COMMIT cadence.

**Decision:** proceed only to a separately preregistered exact pack-fit
candidate. It should target the **1,024 current appends** and pack-write/SQL
work while keeping a bounded pending pack per lane; do not aim to eliminate
the near-byte-floor 1,190 capacity drains themselves. Require exact root and
full readback, <=8 COMMITs, no warm prior-run source cache, and explicit
RSS/Store-space accounting. This count diagnostic is evidence for choosing
that mechanism, not evidence that it will speed up Init or recover the
historical 518.8 MB/s row.
