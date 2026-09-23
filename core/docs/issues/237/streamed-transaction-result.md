# #237: streamed 4-MiB waves under longer SQLite transactions

> **Decision: no-go for adoption.** Isolated source commit `ec225c06b` and
> [frozen gates](streamed-transaction-prereg.md) are retained. One matched 10k
> control/candidate pair found a lower one-shot raw public wall, but missed the
> ≥90% transaction-reduction target and materially increased lock hold and
> sampled RSS. Packs remained SQLite BLOBs; database pages stayed 4,096 B and
> the whole-file cutoff stayed 128 KiB. The candidate is not merged into the
> root research product or release branch.

## Mechanism and focused checks

The previous [64-MiB wave](bounded-wave-experiment.md) held many canonical
objects at once and cut file-Save COMMITs 80→7, but ran slower with 247-MB
sampled Service RSS. This prototype kept C2's 512-object/4-MiB pending wave
and its one owner. The native Service consumed each producer slab directly;
its scoped C2 callback held Store arbitration and one SQLite transaction
across successive waves until 56 MiB canonical input, final completion or a
5-ms receiver idle interval. A submitted-byte/row high-water could close a
transaction earlier. Final lane seals and publication shared one transaction
only for streamed saves. Ordinary Save routes retained their previous cadence.

The tradeoff is explicit: a callback can wait for the next producer slab while
holding SQLite write ownership. The in-process registry mutex causes another
local writer to wait. A second process bypasses that mutex and may fail on
SQLite's zero busy timeout. The MEMORY journal/page cache may grow across
waves even though no 56-MiB canonical batch is allocated. A single-writer
10k speed result cannot qualify that behavior for concurrent product use.

Before public timing, focused checks passed: `streamed_transaction` 3/3
(multiwave/readback, failure rollback, in-process peer completion), existing
`multi_writer` 5/5, `pack_watermark` 2/2 and `persistence_failure` 8/8; the
Service's 2,050-file native import passed. `cargo +1.85.1 fmt --manifest-path
core/Cargo.toml --all --check`, warning-denying scoped Clippy, the product
boundary guard and six Core tool tests passed. Full Core workspace tests,
cross-process contention and the #229 sparse-history guard were not run for
this rejected experiment. The product commit records production LOC
**117,202→117,391** (reference 65,417 unchanged; Core 51,785→51,974).

## One new matched 10k pair

The [prospective identity record](evidence/streamed-transaction/pair/prospective.json)
froze one control then one candidate from the same seed-1 manifest. Both used
the H3 driver, distinct fresh writable byte copies, fixed public operation
identity, and a final 0/27,503 source-payload-resident-page check. The same
three SaveOutcome reporting lines were added to both arms; their normalized
added-line SHA-256 is
`62c64bae4a8237fe5b6b5f6ab7b72c1724e468c465b426e054dd6ece73c42614`.
No per-object stderr I/O was added. The [control](evidence/streamed-transaction/pair/control-reporting.diff.gz)
and [candidate](evidence/streamed-transaction/pair/candidate-reporting.diff.gz)
diffs, product/binary/harness seals and fresh output paths are preserved.
The reporting lines were restored afterward. Both timed verifiers were
`SKIPPED`; complete readbacks ran separately once per arm.

| 10k/300-MB observation | Control `970854f2c` | Streamed `7d05e58f4` | Candidate minus control |
| --- | ---: | ---: | ---: |
| Public Init, one raw sample | 1,193.328459 ms | **1,113.243166 ms** | −80.085293 ms |
| Throughput, 300 decimal MB / public wall | 251.398 MB/s | 269.483 MB/s | +18.085 MB/s raw |
| Complete performance command | 1,651.861292 ms | 2,056.191625 ms | +404.330333 ms |
| Service file loop | 1,026.331833 ms | 930.758375 ms | −95.573458 ms |
| File-Save successful COMMITs | **91** | **10** | −81 / **−89.01%** |
| File-Save COMMIT wall | 323.164125 ms | 245.885624 ms | −77.278501 ms |
| File-Save SQL bucket | 144.792109 ms | 157.575799 ms | **+12.783690 ms** |
| File-Save object INSERT statements | 1,396 | 1,402 | +6 |
| File-Save object INSERT region | 68.850307 ms | 61.883686 ms | −6.966621 ms |
| Stream segments / sum held / longest hold | not applicable | **6 / 928.139083 / 261.890125 ms** | longest exceeds 200-ms gate |
| Largest transaction rows / submitted bytes | not measured | **7,389 / 117,228,487 B** | inside 8,191 / 128-MiB limits |
| Service sampled max RSS, incomplete boundary coverage | 62,291,968 B | **126,894,080 B** | **+64,602,112 B** (61.609 MiB) |
| Content Store apparent / allocated | 333,635,584 / 336,957,440 B | 333,897,728 / 335,609,856 B | +262,144 / −1,347,584 B |
| Pack rows / BLOB capacity / spare | 1,262 / 330,825,728 / 24,856,015 B | 1,263 / 331,087,872 / 25,114,245 B | +1 / +262,144 / +258,230 B |
| Source payload at final check | **0/27,503 resident pages** | **0/27,503 resident pages** | same declared payload state |
| Public row / timed verification | `INCOMPLETE` / `SKIPPED` | `DIAGNOSTIC` / `SKIPPED` | no admission claim |
| Separate full reopened 10k/300-MB readback | **PASS** | **PASS** | exact root, IDs and bytes |

The control lost a daemon telemetry event, so its row is `INCOMPLETE`; the
candidate's telemetry and cleanup passed, but its driver still declares
`source-cache-uncontrolled-v1` because directory/inode metadata residency is
unqualified. Both source checks reported zero payload pages, with
recheck-to-timer gaps of 1.229/8.694 ms. Both lists of observed competing
Docker processes were the same; that does not establish identical host
interference. Lifecycle user+system CPU was 1,842.866/1,744.711 ms under a
Service+daemon window different from the public timer. The complete command
includes setup and cleanup and was **404.330 ms slower** for the candidate;
the raw public delta is one observation, not a variance estimate.

The candidate's COMMIT bucket fell 77.279 ms while its SQL bucket rose
12.784 ms. The 80.085-ms raw public change is consistent with lower COMMIT
work, but the buckets and Service/daemon spans do not form an exact
decomposition of the public caller, and the one-shot pair cannot isolate
causality. The candidate held Store arbitration for **928.139 ms in six
segments**, including producer waits, almost the entire 930.758-ms Service
file loop. This is the concrete multiwriter latency hazard the former
wave-sized transaction boundary avoided. Actual-owner SQLite cache/spill
counters were not collected; the RSS rise must not be labelled journal bytes.

Both [control](evidence/streamed-transaction/pair/control/readback.json) and
[candidate](evidence/streamed-transaction/pair/candidate/readback.json)
reopened readbacks checked 10,101 paths, 10,000 files, 101 directories,
kind/mode/mtime and all 300,000,000 B against the manifest. They returned the
same public root
`e7850f75a65309568e3454f7ab962aa16952d0c02218a55f42f3e2fd44ddf673`
and ordered object-ID SHA-256
`4a9f14a45482c2ae3962f633b3655125278dc816ee9f499803302d76aa241789`.
The [policy check](evidence/streamed-transaction/pair/policy.json) found
4,096-B pages and 131,072-B cutoff in each Store. The candidate used one
extra 256-KiB pack, within the prospectively allowed 1-MiB Store/capacity
growth but not an identical physical layout. Allocated bytes are host
`st_blocks` observations, not SQLite payload or device-write counters.

## Frozen gate decision

The [reproducible summary](evidence/streamed-transaction/pair/summary.json)
marks **no-go**. The candidate passed exact root/IDs/readback, no raw public
slowdown, ≤8,191 rows, ≤128 MiB submitted bytes and ≤1 MiB Store/pack-capacity
growth. It failed **≤9 file-Save COMMITs** (10), **≤200-ms longest lock**
(261.890 ms), and **≤16-MiB sampled Service RSS growth** (+61.609 MiB).
Cross-process zero-busy contention remains untested and release-ineligible.
The historical 518.8-MB/s row was not restored; it lacked a cold-source
contract and is archival context only. Preserve this candidate as an
isolated, rejected transaction-count experiment rather than changing its
thresholds or rerunning the arm.

The [control receipt](evidence/streamed-transaction/pair/control/receipt.json),
[candidate receipt](evidence/streamed-transaction/pair/candidate/receipt.json),
[file-Save control log](evidence/streamed-transaction/pair/control/service.stderr),
[candidate diagnostics excerpt](evidence/streamed-transaction/pair/candidate/service-diagnostics-from-receipt.txt),
[Store geometries](evidence/streamed-transaction/pair/control/geometry.json),
[candidate geometry](evidence/streamed-transaction/pair/candidate/geometry.json)
and [database hashes](evidence/streamed-transaction/pair/db-hashes.json)
retain the raw basis. The runner recycled the candidate's original stderr
after successful telemetry ingestion and kept only its 4,096-character
diagnostic excerpt in the receipt; the file-Save line is complete, but the
later tree-Save line is truncated and its aggregate profile is unavailable.
No sample was repeated to repair that evidence gap.
