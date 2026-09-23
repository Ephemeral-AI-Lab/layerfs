# #237: file Save connection release and one idle Store connection

> **Status: isolated research candidate, not selected for the root product.**
> One count-only residual diagnostic and one matched control/candidate 10k pair
> are retained below. Every public row returned a root, but telemetry was
> `INCOMPLETE` and source directory/inode metadata residency was unqualified.
> The candidate's physical Store and sampled Service RSS both increased.

## Residual diagnosis against v0.1.6

The exact v0.1.6 release holds one SQLite connection in its long-lived
`StoreDb`. Core had opened a private connection for each `SaveOperation` and
closed it in `finish`, before native Init returned. Core's named
`history.import_finish_save` child ended before the close, leaving that work in
the public operation but outside the named child.

One [preregistered count-only diagnostic](evidence/pipeline-residual/prereg.json)
on the reorganized `ImportBatch` source added aggregate timers in Service only;
its [exact patch](evidence/pipeline-residual/source.diff.gz) was removed after
the run. The fresh independent source copy had **0/27,503 resident payload
pages** at preflight and final recheck. Its public 1,189.858-ms time is a
diagnostic, **not** a further speed sample of the integrated source. The
[receipt](evidence/pipeline-residual/receipt.json) is `INCOMPLETE` because of
telemetry loss; verification was skipped. The [Service log](evidence/pipeline-residual/service.stderr)
and [cold sidecars](evidence/pipeline-residual/cold-recheck.json) retain the
raw observations:

| Count-only region | Observation |
| --- | ---: |
| File loop | 951.323 ms |
| Receiver wait / C2 accept | 197.262 / 752.757 ms, nearly the entire loop |
| Batches / objects | 1,204 / 24,562 |
| Scoped worker join after receiver completion | 0.007 ms |
| Four producer wall sum / channel send-call sum | 3,803.727 / 2,055.669 ms; overlapping workers |
| Source reads | 29,952 calls, 300,000,000 B, 1,057.566-ms summed syscall wall across four workers |
| File open and metadata calls | 260.079-ms four-worker sum |
| File Save `finish` / owner drop / SQLite connection release | 113.220 / 104.422 / **104.414 ms** |

The worker with the 100-MB anchor processed 119 files / 139.236 MB; the other
three processed 3,157–3,377 files / 53–54 MB each. All four ran about 950–951
ms, so the receiver's terminal join was not the gap. Compared with the earlier
same-source v0.1.6 pipeline's 709.704 ms, Core's file loop remained materially
longer, but the old pipeline and Core child have different boundaries. The old
producer nonblocked remainder (2,577.802 − 806.749 = 1,771.053 ms) and this
Core diagnostic's worker wall minus send calls (3,803.727 − 2,055.669 =
1,748.058 ms) are close four-worker sums, **not** a pure construction or
matched CPU comparison. The source-open/read details and owner `accept` remain
overlapping contributors, not additive wall-time savings.

## Store-scoped connection treatment

The isolated candidate gives `Store` and its clones one shared idle SQLite
connection slot. `begin_save` takes it if available; otherwise it opens a
private connection. A successfully published save returns its committed
connection when the slot is empty. Concurrent active saves still own separate
connections. Aborted, failed and unknown-outcome saves never return one. The
existing acquisition verifies the connection profile and overwrites its TEMP
read scope for the next save. The connection closes when the final Store owner
ends, just as v0.1.6's long-lived Store connection does. The file Save's
connection is reused by the subsequent prerequisite and tree Saves in this
Init; it is not merely held idle through the whole operation.

The [frozen pair protocol](evidence/connection-slot/prereg.json) selected one
control then one candidate with identical minimal Service `finish` logging,
the same H3 public route and fixture identity. Each arm created a fresh Store
and an independent writable source copy; full payload hash/invalidation and
immediately preceding nonfaulting checks found **0/27,503 resident source
payload pages** in each. The pair kept 4,096-byte SQLite pages, the 128-KiB
whole-file cutoff, pack payload in SQLite BLOBs, four existing C1 constructors
and one C2 owner. The [control](evidence/connection-slot/control/receipt.json)
and [candidate](evidence/connection-slot/candidate/receipt.json) receipts and
exact [control](evidence/connection-slot/control/source.diff.gz) and
[candidate](evidence/connection-slot/candidate/source.diff.gz) patches pin
the actual built identities. In-timer verification was `SKIPPED`.

| 10k / 300-MB observation | Control | One-slot candidate | Candidate change |
| --- | ---: | ---: | ---: |
| Public Init | **1,153.709 ms** | **1,069.652 ms** | **−84.057 ms / −7.29%** raw |
| Throughput, decimal MB/s | 260.031 | 280.465 | +20.434 raw |
| File loop | 981.129 ms | 964.914 ms | −16.216 ms; schedule sensitive |
| File Save `finish` whole call | 73.143 ms | 11.460 ms | −61.683 ms |
| File Save connection release | **66.443 ms** | **0.000458 ms** | one idle slot retained |
| File Save COMMITs | 90 | 90 | same |
| File Save SQL / COMMIT buckets | 151.241 / 252.693 ms | 150.260 / 253.617 ms | similar, separate buckets |
| Total apparent Store + History | 333,737,984 B | 334,245,888 B | **+507,904 B** |
| Pack rows / used bytes | 1,262 / 305,969,448 B | 1,264 / 305,978,126 B | +2 / +8,678 B |
| Pack slack | 24,856,280 B | 25,371,890 B | **+515,610 B** |
| Sampled Service RSS maximum | 65,126,400 B | 66,633,728 B | **+1,507,328 B**; boundary not covered |
| Status | `INCOMPLETE` telemetry | `INCOMPLETE` telemetry | no admission claim |

Both separate [control](evidence/connection-slot/control/readback.json) and
[candidate](evidence/connection-slot/candidate/readback.json) reopened
readbacks passed for all 10,000 files / 300 MB outside the performance timer.
The public root was identical:
`e7850f75a65309568e3454f7ab962aa16952d0c02218a55f42f3e2fd44ddf673`.
All 24,683 stored `(object ID, role, canonical length)` tuples were identical
under ordered SHA-256
`7169c2525c3a5d920c0a18fa63d93bceb5b9b16d27ca1bc2cde7d3e2c098059c`.
Both closed Stores used 4,096-byte pages and had 24,683 object rows; the
[control](evidence/connection-slot/control/store-geometry.json) and
[candidate](evidence/connection-slot/candidate/store-geometry.json) geometry
receipts include page counts, pack headers and Store hashes. Their allocated
Store sizes were both 335,609,856 B. The
[control](evidence/connection-slot/control/database-policy.json) and
[candidate](evidence/connection-slot/candidate/database-policy.json) policy
checks also record 4,096-byte History pages and the 131,072-byte small-file
cutoff. The extra apparent candidate bytes track
two additional packs and more pack slack, not bytes in the idle RAM
connection; altered scheduling is a plausible cause, not proven by one pair.

## Decision and limits

The pair's preregistration did **not** set a numeric or no-worse Store/RSS
adoption gate. None is imposed retroactively. If the earlier exact-pack-fit
experiment's separate no-worse conditions were applied as a conservative
screen, this candidate would miss both. The raw public improvement and nearly
eliminated connection release make the mechanism worth keeping isolated for
review, but it is **not selected for the root product**. The candidate remains
319.026 ms slower than the earlier exact v0.1.6 same-source 750.626-ms public
diagnostic; that reference used a different public surface, so this is a raw
target gap, not a matched treatment delta. The historical 518.8-MB/s release
row used an uncontrolled source cache and is not a cold-source benchmark.

Neither telemetry-incomplete row qualifies as release admission. The sample
RSS maxima lack full phase boundary coverage, and the source metadata cache
state was not qualified. No public arm was rerun. The final source candidate
removes the identical Service diagnostic line; this pair measures its prior
instrumented source identity, not the clean commit's speed.

## Check status of the isolated clean source

The product-boundary guard passed over 266 production Rust/SQL files, its six
tool tests passed, and locked Core formatting passed. Both pair binaries were
built with the H3 runner's exact release target list and `--locked`. A later
locked full Core workspace test command was **interrupted (exit 130)** while
`metadata_window` was next, after the observed earlier suites passed. It may
have overlapped the start of another team's timed experiment, so it is not a
complete test result and that other row must be audited for interference.
Warning-denying Clippy was **not run**. These gaps remain because this
Store/RSS-increasing candidate was not selected for root integration.
