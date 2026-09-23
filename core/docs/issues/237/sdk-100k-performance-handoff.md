# Handoff: measure 100,000-file SDK Init

> **Status:** Next-agent task; **no SDK 100k call has been run**. This is a
> prospective, one-shot **debug-profile diagnostic**, outside the frozen #236
> SDK v2 selection. Keep its `namespace-100000` row `NOT_RUN` in that selection.
> Do not reuse or relabel the old daemon-host 100k attempt.

## Objective and fixed case

Run one real `layerfs_sdk::Client::init_project` on the merged Service/
`ImportBatch` product and record its public operation time, complete command
wall, full reopened readback, lifecycle CPU and peak RSS, and closed/allocated
Store and History size. Record an error or timeout as plainly as a completed
root. This answers whether the **SDK** path handles 100k at the current product
identity; it is not a release admission or a comparison with v0.1.6.

The existing `namespace-100000` case in
[`families/init_namespace.py`](../../../benchmark/fs-bench-pro/families/init_namespace.py)
has **100,000 files, 1,000 data directories plus root, 500,000,000 total
logical bytes including two 100,000,000-byte anchors**, and class counts
`(2, 1_000, 78_998, 15_000, 5_000)`. Use profile
`core-sdk-init-fixture-v2`, route `host-direct-sdk-v2`, seed `1`, the existing
SHAKE fixture plan, 4,096-byte SQLite pages, and the 128-KiB whole-file
cutoff. Packs stay as SQLite BLOBs. Do not change the four existing Init
producers or the one C2/SQLite save owner to get a result. A successful full
oracle must report **101,001 paths, 1,001 directories, 100,000 files and
500,000,000 bytes**, with matching manifest, root and file SHA-256 values.

## Current boundaries to account for before the call

- [`init.SELECTED`](../../../benchmark/fs-bench-pro/families/init_namespace.py)
  contains only 100 and 1,000. The sole active
  [`runner.py`](../../../benchmark/fs-bench-pro/runner.py) rejects 100k at its CLI;
  calling internal `case_run()` directly would mislabel the unregistered row as
  scenario v2. Keep the two-case selector and its old receipts intact. Freeze a
  **separate #237 100k diagnostic selection/schema** and its bounds in a new
  committed note before implementing a narrow explicit mode in this same
  runner. Do not add a second benchmark runner or a daemon-host fallback.
- The runner's Python SDK subprocess watchdog is **15 s** and its registered
  verifier watchdog is **5 s**. The product's
  [`Service::init_project`](../../../crates/layerfs-service/src/project.rs)
  separately uses `MAX_OPERATION_MS = 600,000` (10 minutes). The 15 s limit is
  **not** the product deadline. For the new **ineligible diagnostic only**,
  declare a **610 s outer watchdog** to retain the product's own deadline or
  outcome plus teardown, and a **600 s separate verifier watchdog** before
  the first run. These are safety bounds, not new performance PASS gates. Never
  widen them after an observed miss or claim that a completion passed #236's
  15 s command / 5 s verifier conditions.
- Root [`AGENTS.md`](../../../../AGENTS.md) and
  [`core/benchmark/fs-bench-pro/AGENTS.md`](../../../benchmark/fs-bench-pro/AGENTS.md)
  require **locked Cargo debug binaries only** for SDK Init. Build
  `benchmark_init` and `verify_namespace` in the worktree's own
  `core/target/debug/examples/`; no `--release` or optimization override.
  Pin hashes, source tree, product/harness/Cargo seals and compiler flags.
- The main checkout had unrelated working-tree changes during the 10k checks.
  Use a clean, detached temporary checkout of a pinned `main` commit if it is
  still dirty; never stage, restore or discard another task's files. Remove the
  temporary checkout after moving evidence into the main result archive. No
  long-lived branch or worktree is needed.

## One-shot procedure

1. Commit the 100k diagnostic contract and focused selector/cold-refusal check
   first. The receipt must say `UNREGISTERED_DIAGNOSTIC`, carry a new schema,
   and keep #236 v2's 100k `NOT_RUN` row untouched. The sole runner must launch
   the compiled SDK example, whose `Instant` timer encloses exactly one
   `Client::init_project` call; source scan/read, C1/C2/C5 and publication stay
   inside that timer. Build and verification stay outside it.
2. Prepare the sealed v2 source master **once** outside timing. Make a new
   independent writable byte copy for this attempt, validate every manifest
   path/content hash, invalidate source payload pages after any setup read,
   then perform a nonfaulting whole-input residency check. Require
   `files=100000`, `bytes=500000000`, the computed expected page count and
   `resident_pages=0`. Recheck immediately before SDK-driver launch and record
   the gap. The SDK's `Host::create` runs between launch and the public timer
   and does not read source payload. Metadata residency is still unqualified:
   do not label this a fully cold namespace PASS. Do not use a warmed copy from
   a previous timed run. Fail explicitly on hosts without a proven equivalent
   invalidation/residency backend; do not depend on `sudo purge`.
3. Take **one** public SDK attempt at the pinned identity and fresh output path.
   Record public `operation_ns`, complete process wall, returned Project/
   genesis/root or exact failure/unknown/timeout, SDK call count, subprocess
   exit, cleanup and competing work. Capture driver lifecycle user/system CPU
   and **peak RSS** with an explicitly scoped monitor; the current runner has
   CPU but no memory capture. A fresh per-child resource supervisor may report
   lifecycle peak RSS after normalizing host units. Do not call lifetime
   `ru_maxrss` an operation-phase peak or infer memory from Store/page-cache
   bytes. Keep source page residency, process RSS and filesystem cache separate.
4. If a root is confirmed, run the **full** independent verifier once after
   the timed process exits. Reopen C5/C2, check all 101,001 paths, portable
   metadata, 500 MB of content and source SHA-256 values. Record verifier wall,
   stdout/stderr, exit and any timeout separately. Never shrink/sample the
   oracle or include its cost in the SDK operation time.
5. After close, record Store/History apparent `st_size` and allocated
   `st_blocks * 512`, SQLite `PRAGMA page_size` and `page_count`, Store pack
   rows/`sum(length(data))`, and any retained scratch. Require page size 4096.
   Save exact fixture, source-copy, cold, build, operation, resource, verifier,
   geometry and cleanup receipts plus every `NOT_RUN`/failure status. Put the
   concise report under `core/docs/issues/237/` and full raw files under a
   fresh `benchmark-results/fs-bench-pro/` path; append an issue #237 update.

## Interpretation and stop rule

The [post-merge SDK check](sdk-merge-check-20260924.md) recorded debug 100,
1,000 and unregistered 10,000 operation times of **159.816 ms, 432.069 ms and
6,139.828 ms**. The 10k full verifier took **8.287 s**, already beyond the
registered 5 s limit, so a 100k verifier miss is plausible. The one old
daemon-host 100k research attempt returned `Unknown` at 10.011 s while its
Service later reported success; it had no confirmed caller root or readback
and is **not** an SDK speed baseline. The older [100k route audit](100k-route.md)
predates the SDK runner and contains obsolete module paths and selector counts.

If the 100k public call or full oracle misses the predeclared diagnostic
watchdog, preserve that attempt and report `FAIL`/`TIMEOUT`/`INCOMPLETE` with
the observed wall and last confirmed stage. **Do not rerun the unchanged case
to obtain a faster or completed number.** A completed, verified raw SDK time
can be reported as a diagnostic throughput (`500,000,000 / seconds`, decimal
MB/s), with cache and memory scopes beside it. It does not promote 100k into
the frozen #236 benchmark or establish a matched performance improvement.
