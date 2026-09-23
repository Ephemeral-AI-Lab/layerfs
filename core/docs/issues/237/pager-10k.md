# #237: actual 10k Save connection pager behavior

> **Status:** Research; informative and not a product contract. One
> reporting-only perf diagnostic, 2026-09-23. No cache policy was changed and
> no speed comparison is claimed.

## Question and method

The [SQLite EXPLAIN audit](sqlite-explain.md) found that Core's actual system
SQLite defaults to a 2,000-page cache with spilling enabled, while v0.1.6
configured 32 MiB and spilling OFF. Those fresh-connection pragmas did not
measure the 10k file Save. The [D13 preregistration](pager-10k-prereg.md)
therefore required actual-owner `sqlite3_db_status` counters before considering
a cache-policy pair.

The temporary [instrumentation diff](../../../../docs/roadmap/0.1/0.1.7/evidence/issue237-native-init-research/d13-pager-instrumentation.diff.gz)
read `CACHE_USED`, `CACHE_HIT`, `CACHE_MISS`, `CACHE_WRITE`, and `CACHE_SPILL`
without resetting them, after Save acquisition, after each completed
preparation wave, and after final publication before connection release. It
also read the Save owner's actual `PRAGMA page_size/cache_size/cache_spill`.
Pinned `rusqlite` has no safe `db_status` wrapper, so this one instrumented
build temporarily used the existing SQLite FFI; the product source was
**restored afterward**, and the exact diff is retained. No test-only hook was
committed. The first release build used this worktree's private Cargo target
and completed in **7.153 s**; the operation reused its exact archived binaries.

The source was the sealed prepared 10k fixture, reused solely for setup.
It contains 10,000 files and **300,000,000 logical bytes total**, including
the 100-MB anchor. The [cold driver](../../../../docs/roadmap/0.1/0.1.7/evidence/issue237-native-init-research/cold_diagnostic.py)
used a fixed stack and scope seed derived from the fixture manifest, but fresh
transport/session keys. The independent full payload hash/invalidation and
immediately preceding nonfaulting whole-input recheck each found **0 resident
pages of 27,503**; the recheck finished **6.316 ms** before the caller timer.
The fresh Store kept **4,096-B SQLite pages**. Verification was SKIPPED for
this exploratory row. The official receipt retains `source-cache-uncontrolled-v1`
because payload residency alone does not qualify directory/inode metadata:
this is a **DIAGNOSTIC**, not a cold admission PASS.

## Both retained attempts

| Output | Timed samples | Result |
| --- | ---: | --- |
| [Attempt A receipt](evidence/pager-d13/attempt-a/receipt.json) | 0 | `NOT_RUN`: an APFS clone copy preserved file metadata but changed 101 copied directory mtimes, so fixture validation stopped before source preflight or public timing. |
| [Attempt B receipt](evidence/pager-d13/attempt-b/receipt.json) | 1 | Public C5 root returned in **1.530461375 s** (196.02 decimal MB/s); complete performance command **2.470422459 s**; telemetry and daemon/Service cleanup PASS; verifier SKIPPED. |

For setup repair, I set only those 101 directory mtimes back to the sealed
manifest. `init.prepare` then accepted the copy. A separate [setup cold
check](evidence/pager-d13/setup-repair-cold.json) rehashed/invalidated all
300 MB and found zero resident payload pages; Attempt B repeated that full
preflight and the immediate recheck independently. The aborted receipt remains
unchanged. There was no failed performance sample to replace or select from.

## Live Save counters

The completed [raw receipt](evidence/pager-d13/attempt-b/receipt.json)
retains all three Save lines under `telemetry.diagnostics.service`. `CACHE_USED`
is approximate bytes on that connection. SQLite returns no high-water value
for this status code; the maximum below is the largest **wave-boundary sample**,
not an exact within-wave peak. Hit/miss/write/spill values are deltas from the
post-acquisition reading to final publication. Wave maxima are differences
between successive boundary readings, not per-object measurements.

| Save (new objects) | Commits / sampled waves | Cache setting | Max boundary cache bytes | Hits / misses | Dirty page writes / spills |
| --- | ---: | --- | ---: | ---: | ---: |
| File (24,364) | 79 / 74 | 2,000 pages; spill threshold 20,000 pages | **8,767,488** | **718,990 / 3,838** | **94,513 / 0** |
| Prerequisite (11) | 4 / 1 | same | 707,584 | 202 / 20 | 157 / 0 |
| Tree (308) | 21 / 1 | same | 1,926,144 | 12,970 / 227 | 803 / 0 |

Every owner reported page size **4,096 B**, `mmap_size=0`, and zero failed
`db_status` reads. The file Save's largest single completed wave recorded
**189 cache misses**, **1,458 dirty page writes**, and **0 spills**.
File-Save misses were **0.531%** of hits plus misses; even if every miss
corresponded to a different database page, they name at most
`3,838 × 4,096 = 15,720,448 B` of page loads, and some may be compulsory.
The **94,513 page-write events** amount to 387,125,248 page bytes counted by
SQLite; they are not a device-byte or durability measurement and may include
rewrites across the 79 required acknowledgements. Spill OFF cannot remove a
spill that did not occur. A larger page cache might change some misses, but
these counters do not show repeated reads or a substantial critical-path cost.

The [closed Store geometry](evidence/pager-d13/attempt-b/geometry.json) is
334,168,064 B apparent, 335,609,856 B allocated, **24,683 objects**, 1,264
packs, 331,350,016 B reserved pack capacity, and 305,977,813 B declared used.
The file Save contributed 24,364 objects and 1,259 packs. The retained Store
SHA-256 is `8fd57614351aafd68a9ada91239faabf62ff51c2e6d39004da62e2f0c8720284`.
The [evidence manifest](evidence/pager-d13/manifest.json) hashes the copied
receipts, sidecars, telemetry, geometry, Store and instrumentation diff. The
full Store remains in this isolated worktree's ignored raw output.

The receipt's `external_resources` reports **1.121 s user** and **1.028 s
system** CPU for the combined Service/daemon lifecycle. It explicitly has
`rss_phase_peak=null`: no phase-local peak RSS was captured, and a lifetime
`ru_maxrss` would not substitute. A future policy comparison would have to
measure both the Save's page-cache use and process peak RSS; D13 alone cannot
certify a memory bound for a larger cache.

## Decision

**Do not run the proposed 32-MiB/spill-OFF pair now.** D13 found **zero
spills** on the actual file Save, all 74 sampled wave boundaries, and both
subsequent Saves. The default cache already retained roughly 8.8 MB at the
largest sampled boundary, and only 3,838 misses were counted amid about
719,000 hits. The reference's 32-MiB setting is therefore an unproven
product-policy hypothesis, not a demonstrated route to the 578.245-ms
(518.8-MB/s) public target. This decision follows the preregistered rejection
rule; no default/large-cache arm, warm-cache operation, or page-size change was
made. The instrumented D13 operation time is **not** compared as a treatment
against D9, D11, D12 or the historical reference.

If a later native workload shows positive actual-owner spills or substantial
repeat cache misses, preregister a new one-sample-per-arm pair with equal cold
source state, fresh Stores, bounded memory, 4-KiB pages, identical public route,
and full Store/readback evidence. For this 10k workload, the next useful cost
question is the product work that publishes about 7,700 groups and commits
around 80 transactions, not a missing SQLite index or the absent spill term.
