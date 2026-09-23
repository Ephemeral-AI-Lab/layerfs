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

No combined-arm public sample has been taken yet.
