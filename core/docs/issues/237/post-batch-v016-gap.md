# #237: remaining 10k gap after ImportBatch

> **Status:** source/evidence comparison plus isolated experiments, not release
> admission. The exact v0.1.6 release arm and current Core Init use the same
> 10,000-file, 300,000,000-byte source manifest and had zero resident source
> **payload** pages at final preflight. Their public APIs and child timer
> boundaries differ, metadata cache residency is unqualified, and Core's current
> public row lost telemetry. No arm is rerun to select a favorable number.

## Side by side

The reference is exact v0.1.6 product source `44cf74848` through its real SDK
`initialize_layerstack`; Core source is `bc944fe63` through daemon native Init.
The [reference arm](v016-head2head-arm.md), [Core integrated arm](service-layout-and-import-batch.md)
and [SQLite comparison](v016-core-sqlite-head2head.md) retain the raw receipts.

| Observation, 10k / 300 decimal MB | v0.1.6 release | Current Core ImportBatch | Interpretation |
| --- | ---: | ---: | --- |
| Public Init | **750.626 ms** | **1,110.332 ms** | **359.706 ms raw gap**, different public surfaces |
| Public throughput | 399.667 MB/s | 270.189 MB/s | Both source payload checks 0/27,503 pages; metadata unqualified |
| Broad import pipeline / file loop | 709.704 ms | 935.124 ms | 225.420 ms arithmetic difference; child boundaries differ |
| Public time outside those broad spans | 40.922 ms | 175.208 ms | 134.286 ms arithmetic difference, not causal attribution |
| Channel receives | 1,203 slabs | `NOT_MEASURED` in integrated source | Earlier matched Core batch prototype received 1,202 |
| Serial C2 `accept` / receiver wait | `NOT_MEASURED` | `NOT_MEASURED` in integrated source | Separate Core count diagnostic: 752.757 / 197.262 ms within 951.323-ms file loop |
| SQLite write COMMITs | 75 traced | `NOT_MEASURED` in integrated speed arm | Separate Core file-Save diagnostics counted 89–90; distinct identities/scopes |
| Combined apparent Store + catalog | 304,553,984 B | 334,249,984 B | Core +29,696,000 B (+9.75%); file size is not device writes |

Core's sampled Service window was 1,106.726 ms, 3.606 ms less than its public
call; this is evidence against attributing hundreds of milliseconds to daemon
transport. Its named Service children leave 85.315 ms unnamed. A separate
[count-only pipeline diagnostic](connection-slot-experiment.md) found 104.414
ms releasing the file Save SQLite connection outside the named finish child.
That number belongs to a different instrumented identity and cannot be
subtracted from the 1,110.332-ms row. Similarly, the 225.420/134.286-ms split
above compares nonidentical timer scopes and does not partition the gap.

## Instrumented exact-release microsteps

One subsequent [v0.1.6 count-only diagnostic](v016-microstep-count-diagnostic.md)
used the exact release product plus temporary aggregate timers on the same
source. Its 768.649-ms public wall is **not** a second speed sample of the
750.626-ms reference. The Core column below is a separate instrumented
ImportBatch pipeline diagnostic, not the clean 1,110.332-ms Core speed arm.

| Count-only region | v0.1.6 | Core ImportBatch | Limit of comparison |
| --- | ---: | ---: | --- |
| Broad pipeline / file loop | 727.788 ms | 951.323 ms | Different endpoints; +223.535 ms arithmetic gap |
| Consumer callback / C2 `accept` | 661.613 ms | 752.757 ms | Different APIs; +91.144 ms arithmetic gap |
| Receiver blocking wait | 65.862 ms | 197.262 ms | Old includes terminal channel close, Core stops at last Done; +131.400 ms is not channel overhead |
| Maximum producer wall | 726.482 ms | 951.226 ms | Both nearly equal their loop endpoints; +224.744 ms |
| Signature work | 96.907 ms over four old producers | 96.164 ms on one Core C2 owner | Same 9,399 small files / 33.747 MB; workers overlap |

Subtracting Core's separately measured 96.164-ms signature wall from its
752.757-ms `accept` sum leaves 656.593 ms, close to the old 661.613-ms
consumer callback. The ~5-ms residual crosses identities and boundaries;
it is a **hypothesis about placement**, not proof that moving signatures
would save 96 ms of public time. Such a move would use the existing four
file constructors and keep one C2/SQLite owner, but producer-side signature
precomputation was previously paused by owner direction. No treatment has
run. The higher Core receiver wait reflects batch arrival pacing; similar
1,202/1,204 batch counts do not explain it by channel call count.

## Count-driven findings and treatment decisions

| Mechanism | Evidence | Decision |
| --- | --- | --- |
| Small-file signature on single C2 owner | [Signature diagnostic](signature-owner-diagnostic.md): 9,399 calls, 33,747,000 B, 96.164 ms; zero delta trials/FULL losses/rescans. Two byte-identical faster-signature variants were no faster in a one-pass pure-function check. | The proposed duplicate-scan fix cannot help this 10k fixture; no source change adopted. |
| File Save SQLite connection close | [Connection-slot pair](connection-slot-experiment.md): close 66.443→0.000458 ms, public 1,153.709→1,069.652 ms raw. Both full readbacks PASS; Store apparent +507,904 B, sampled Service RSS +1,507,328 B. | Keep isolated. The pair had no preregistered space/RSS adoption gate, and both public rows lost telemetry. |
| Object locator INSERT query shape | [Single-scope CTE pair](sql-bulk-admission-result.md): `insert_objects_ns` 69.379→63.860 ms (−5.519 ms/−7.96%), below the prospective 15% gate; Store +528,384 B. Raw public −200.584 ms mostly tracked variable connection close (−149.704 ms). | Reject. EXPLAIN opcode reduction did not translate to the required measured SQL win. |
| Exact paged collision lookup | [128-ID candidate pair](collision-lookup-batching.md): public 1,092.106→1,194.798 ms, Store +778,240 B; both readbacks PASS. Timed collision-query wall not exported. | Reject on preregistered speed and Store-size gates. |
| Larger C2 waves for fewer COMMITs | [Earlier 80→7 trial](bounded-wave-experiment.md): count target met but public time 1.364→1.440 s and sampled RSS rose strongly. | Reject large-buffer version; transaction count alone is not a speed result. |
| Streamed 4-MiB waves under longer transactions | [Matched pair](streamed-transaction-result.md): file COMMITs 91→10, public 1,193.328→1,113.243 ms raw. Longest lock 261.890 ms, sampled RSS +64,602,112 B, Store apparent +262,144 B. Both readbacks PASS. | Reject: missed frozen ≤9 COMMIT, ≤200-ms lock and ≤16-MiB RSS growth gates. |
| Pack BLOB physical writes | [Source/evidence feasibility](pack-blob-write-feasibility.md): current format needs preallocated BLOBs for later appends; prior exact-fit saved only 9.085 ms of pack-write wall while growing Store/RSS. | No new 10k pair; no narrow high-impact current-format treatment found. |

The signature, SQL, COMMIT, pack-write and close timers come from **different
count-only identities**. They overlap in places and cannot be added as a
promised saving. In a pipeline diagnostic, the file loop was nearly exactly
C2 `accept` plus receiver wait; scoped worker join was 0.007 ms. The v0.1.6
connection remains open across Init, while Core normally opens/closes a private
connection per Save. The native scan's entire 47.525-ms current child is also
smaller than the 359.706-ms gap, and it is not an old/new matched scan delta.

## Archived gap and stopping point

The owner accepted the reorganized Core `ImportBatch` source and its
1,110.332-ms / 270.189-MB/s observation as the research stopping point on
2026-09-23. The raw 359.706-ms v0.1.6 gap remains documented, not closed.
No later treatment source was adopted. The
[known-length C1 candidate](known-length-native-init-stopped.md) was stopped
before a public 10k sample; its prospective record and diffs are retained.

The exact v0.1.6 release now has one **count-only** microstep diagnostic at
closer owner/producer boundaries. Its original 750.626-ms public speed arm
was not repeated as a new speed sample. The one-slot connection result
offers a concrete ~84-ms raw treatment but needs a qualified space/RSS decision
and full Core checks at any selected source identity. No warm cache from an
earlier run may enter a timed arm; every arm creates a fresh Store/process and
uses an independent source copy with final zero-resident-payload verification.

All experiments retain the 4,096-byte SQLite page and 128-KiB whole-file
cutoff. They keep pack payloads in SQLite BLOBs and one C2/SQLite owner.
