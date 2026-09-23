# #237: prospective combined C1 + C2 10k experiment

> **Status:** Research; preregistered before either public sample. This page
> will retain every attempt and result. It does not assert a speedup yet.

## Question and fixed method

The isolated fixed-identity pairs found a C1 direct-build signal and a
dependency-aware C2 admission signal, each with equal canonical roots and
full reopened readback. Their caller times came from different source trees
and windows, so adding their savings is invalid. This experiment measures the
**incremental C2 change on top of C1** on this research worktree.

The control is current branch source at `181973312`, including the adopted C1
direct fresh-build path but no C2 queue. The candidate will apply only the
bounded C2 admission source and external tests from isolated commit
`60ced47d1`, with its source diff retained. No SQLite page, file-worker,
channel, wave, transaction or deadline policy changes belong to this pair.
Both arms use the byte-identical root
`docs/roadmap/0.1/0.1.7/evidence/issue237-native-init-research/cold_diagnostic.py`
with `--fixed-operation-identity --verify`, the same Core runner/family source,
and the sealed `namespace-10000` seed-1 fixture manifest. The driver derives
the same 16-byte stack and 32-byte scope seed for both; each receipt retains
`operation-identity.json`. Transport/session keys and fresh Store/History
files remain independent. Product, compilation, harness, fixture and binary
seals are recorded for each arm.

Each arm gets one release-profile public `ImportNativeDirectory` 10k sample
at a fresh output path. Its 10,000 files total **300,000,000 logical bytes**,
including the 100 MB anchor. Before each timer the research driver independently
rehashes every source file, invalidates payload pages, and immediately performs
a nonfaulting whole-input residency recheck; both must report **0 resident
pages of 27,503**. The prepared workspace is reused only outside the timer.
This proves source *payload* residency; directory/inode metadata cache remains
unqualified, so the original runner's `source-cache-uncontrolled-v1` and
`admission_eligible=false` labels remain. Database `PRAGMA page_size` must be
**4,096 B** in each fresh Store. No verification wall is in the public caller
time or complete performance command. The existing full verifier runs
separately afterward at its unchanged 5 s watchdog; retain a timeout/failure.

Order and intended outputs:

1. Control at `181973312` product source, output
   `benchmark-results/fs-bench-pro/issue237-r2-c1-control`.
2. Apply the C2 candidate as a distinct product commit, then candidate output
   `benchmark-results/fs-bench-pro/issue237-r2-c1-c2-candidate`.

Do not rerun either arm for a better time, cleaner telemetry, or a larger
spread sample. Keep `NOT_RUN`, `INCOMPLETE`, cleanup failures and verifier
failures. The original public caller timer and complete command wall are
reported separately, along with lifecycle CPU and sampled RSS limitations.

## Decision and separate proof

Require both public calls to return a confirmed C5 root; compare exact root
and the full stored object-ID sets under the fixed scope. The reopened
`verify_namespace` child must check all 10,101 paths, their kind/mode/mtime,
History root and SHA-256 of all 300 MB of file content. Record Store apparent
and allocated bytes, page count, object and pack counts, pack capacity/used/
spare bytes and per-save geometry. A treatment that is faster but changes the
canonical root, breaks readback, or worsens dense Store space is not adopted.
The separate #229 sparse-history lane is still required for a compactness
claim; it is not inferred from this dense Init pair. A telemetry drop or
unqualified metadata cache keeps its receipt nonadmissible, even if the raw
caller time improves. The 518.8 MB/s historical row requires ≤0.578245 s for
300 MB; no combined result is assumed to reach it.

## Attempts and outcomes

One control and one candidate were run, with no retry. Both used the same
frozen public stack/scope and fixture manifest; each independent preflight
and immediate recheck found **0 resident source payload pages of 27,503**.
Recheck-to-timer gaps were **1.779 ms** and **1.099 ms**. The original runner
still labels metadata cache uncontrolled. The [raw control](evidence/combined-c1-c2/control/receipt.json),
[raw candidate](evidence/combined-c1-c2/candidate/receipt.json) and
[evidence manifest](evidence/combined-c1-c2/manifest.json) retain their
source/harness/binary identities, cold sidecars, fixed IDs, telemetry, separate
verifiers and closed-Store SHA-256s. Full SQLite Stores remain in the named
private result directories rather than this documentation tree.

| Measure | C1 control | C1 + C2 candidate | Candidate difference |
| --- | ---: | ---: | ---: |
| Public Init | 1.310979458 s | **1.252324750 s** | −0.058654708 s (−4.474%) |
| 300 MB decimal throughput | 228.836 MB/s | **239.554 MB/s** | +10.718 MB/s |
| Complete performance command, before verifier | 2.248638458 s | 2.177525041 s | −0.071113417 s |
| Service + daemon lifecycle user/system CPU | 1.063406 / 0.902669 s | 1.000536 / 0.893685 s | combined −0.071854 s |
| Separate reopened verifier | PASS, 2.176 s | PASS, 2.164 s | outside throughput |
| Store apparent / allocated bytes | 333,914,112 / 335,609,856 | 333,897,728 / 335,609,856 | −16,384 / 0 B |
| Pack capacity / used bytes | 331,087,872 / 305,973,639 | 331,087,872 / 305,973,799 | 0 / **+160 B** |
| Pack count / object count | 1,263 / 24,683 | 1,263 / 24,683 | 0 / 0 |
| SQLite page size | 4,096 B | 4,096 B | 0 |

Both public calls returned the exact root
`e7850f75a65309568e3454f7ab962aa16952d0c02218a55f42f3e2fd44ddf673`.
The complete ordered object-ID digests match
(`4a9f14a45482c2ae3962f633b3655125278dc816ee9f499803302d76aa241789`).
Both existing verifiers reopened Store and History, checked the published
root and all **10,101 paths** with kind/mode/mtime, and SHA-256 checked all
**300,000,000 file bytes**. The [control geometry](evidence/combined-c1-c2/control-geometry.json)
and [candidate geometry](evidence/combined-c1-c2/candidate-geometry.json)
record all three Saves. Closed Stores do not reveal actual pack-write or
object-INSERT call counts, so this pair does not claim a measured call-count
reduction from the queue mechanism.

**Qualification is incomplete.** Each daemon lost one telemetry event, so
both official receipts are `INCOMPLETE` despite returned roots, full verifier
PASS and cleanup PASS. Source *payload* pages were absent immediately before
the calls, but directory/inode metadata was not independently qualified.
The treatment's SQLite file was 16 KiB smaller and reserved pack capacity
equal, while declared pack use rose **160 B** and spare capacity fell by
the same amount. Under the preregistered no-worse condition for *each* space
field, pack-used bytes miss that strict check; do not turn the smaller file
into an unqualified compactness PASS. #229 sparse-history readback/space has
not yet been run at this combined identity. The raw 4.474% time reduction is
evidence for further research, not a release or 518.8 MB/s result. At
1.252325 s the caller still needs **0.674080 s** less to reach the historical
578.245-ms time.
