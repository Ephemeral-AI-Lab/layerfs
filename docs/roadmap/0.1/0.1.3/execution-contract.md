# Phase 1 execution contract, revision 1

This premeasurement supplement binds the twelve canonical specifications and
the ordinary and dedup/reliability execution supplements. It freezes safety,
execution and evidence choices for #22; it contains no measured results.
The canonical 130 timed IDs, three seeds, CDC proof and 28 reliability subcases
remain unchanged. A later change to a measured scenario retains this revision
and its evidence and requires an explicitly versioned replacement.

## Baseline and source custody

The untouched product baseline is
`1e81e9b8cf871324341c221a51b0a0239c580da9`. Phase 1 does not optimize product
algorithms, storage, CDC or acknowledgement semantics. Passive counters and
verification-only fault seams must have separate source identities and retain
the ordinary runtime route. Fault activation is forbidden in performance mode.

Commit the specifications before benchmark implementation. Commit benchmark
sources before admitted collection. Build from the committed sources in an
isolated checkout when unrelated working-tree changes exist; never discard or
commit unrelated changes to satisfy a clean-tree check. Record product, harness,
workload, fixture generator, oracle, report generator, dependency lock, compiler,
binary and image digests, actual revision/tree, Docker configuration and Git
binary version. An old binary is reusable only when its relevant inputs match.
An instrumented baseline remains explicitly instrumented, not relabeled as the
released binary. No candidate comparison is part of this initial campaign.

## Frozen execution and safety profile

The profile is `v013-macos-docker-linux-fuse-ack-window-v1`: macOS host SDK,
Docker Desktop Linux arm64 managed daemon and real FUSE; one measured sample at
a time. Preserve unrelated containers. Each sample receives a fresh owned
container, fresh Client/process/session and independent writable Store clone,
or a fresh output Store for measured initialization. The OS cache is warm or
uncontrolled after hashing/cloning; this is never a cold-I/O claim.

| Resource | Hard ceiling / policy |
| --- | --- |
| Owned runtime CPU | 2 Docker CPUs; one host benchmark process |
| Owned runtime memory | 2 GiB cgroup memory, memory+swap also 2 GiB, zero swap/OOM |
| Host benchmark RSS | 2 GiB native process lifetime high-water |
| Owned runtime processes | 256 pids |
| Workload file / aggregate | 524288000 bytes inclusive / 1073741824 bytes exclusive; stronger family bounds apply |
| One sample Store | 4 GiB allocated and logical bytes, including temporary SQLite files |
| One sample spool / runtime scratch | 2 GiB each; no unexplained residual owned files |
| One fixture plus oracle preparation | 4 GiB; streamed expected bytes, no all-history materialization |
| Shared prepared cache | 24 GiB; explicit maintenance outside measured windows |
| One sample retained output/logs | 64 MiB; truncation is a validity failure |
| Owned concurrent preparation | One; no preparation/build/verification during performance |

These are diagnostic safety limits, not evidence-derived Phase 2 performance
targets. Enforce finite supervision without increasing limits after a miss.
Record the actual phase and completed work on timeout. A failed operation's
partial wall is not an eligible successful-operation latency.

| Lane | Preparation | Product execution per case | Verification per case | Cleanup |
| --- | ---: | ---: | ---: | ---: |
| Ordinary: selected tier 1 or 10 | 600 s | 120 s | 600 s | 60 s |
| Large: tier 100 or 500, or fixed input with at least 100000 files | 1800 s | 600 s | 1800 s | 60 s |
| Retained history, N at least 100 | 600 s | 3600 s | 7200 s | 60 s |
| Reliability short | 600 s | proof only | 600 s | 60 s |
| Reliability extended except endurance | 600 s | proof only | 3600 s | 60 s |
| Sustained activity | 600 s | proof only | 900 s active workload plus 600 s final verification | 60 s |

An individual Create, Commit, End, initialization or workload phase is bounded
by its enclosing case deadline; remaining budget cannot reset between phases.
Full-family performance deadline is the sum of its prescribed per-slot limits,
plus separately bounded preparation and cleanup. The same case uses the same
hard product budget in selected and full modes. Ordinary warm-prepared command
wall retains the aspirational 1–5-second target and is reported even when missed.
The sustained workload must execute for at least 600 monotonic seconds with
completed nonzero work in every 30-second progress window. A stall fails that
gate; neither idle sleeps nor elapsed process lifetime substitute for activity.

## Measurement and observation

Use Rust `Instant` for host/workload durations and causal receipt boundaries.
Inner operation timers surround the named public call or actual tool workload;
preparation, output parsing, monitors and full verification are excluded.
Report Create, execution, inner workload, sync, SDK edits, Commit, visibility,
End, enclosing lifecycle and external command wall separately. Sync is a
subphase of workload, never subtracted from it. Unattributed orchestration is
the checked difference from the enclosing lifecycle, not a hidden speedup.

Reuse the existing `ack-window-v1` observation approach: boundary snapshots,
native lifetime high-water and causally bracketed sampled memory categories.
Poll host RSS and container memory.stat/current/events/swap at nominal 10 ms;
record actual timestamps, sample count and maximum gaps. Samples describe a
broader causal interval and are not exact continuous phase/category maxima.
Use the fresh-container memory.peak as the runtime lifetime total bound. Retain
anonymous/file/dirty/writeback/shmem/kernel/slab categories, host CPU/I/O/RSS,
spool high-water and allocated Store sizes separately. A missed observation
is unavailable with a reason and requires repair if mandatory. No precise
phase-local memory-insensitivity claim is authorized by this profile.

Read incremental passive monitor receipts after acknowledgements before the
512-operation retention ring can evict history. Count actual calls and preserve
Commit IDs/outcomes and phase diagnostics per history step. SDK and tool route
schemas remain distinct. Where a numeric route counter does not exist, use the
general rules' sealed call-graph manifest plus observed runtime tripwires;
never fabricate a counter. Work, storage, FUSE and resource observations required
by a family still must be collected. Census, chunk transcripts, whole-tree
hashes, reopen and fault injection execute only in verification/preparation.

## Campaign and evidence layout

Use `benchmark-results/fs-bench-pro/phase1-v013/` with append-only run folders:
`environment/`, `preparation/`, `performance/`, `verification/`, `results/`.
Keep the existing `run-status.json` convention as the coordinating ledger.
Every slot identifies issue/family/case/seed/mode, source/input/oracle/environment
digests, outcome, raw path, invalidation reason and next action. A planned slot
is never an executed result. Raw failures are sealed and retained before fixes.

Default selection requires one case and seed. Complete-family collection is
explicit `--all`; extended proof selection is explicit. Traverse family case
order from the canonical registry, then seeds 1, 2, 3. Shared controls execute
once. Finish all scheduled performance before the separate final verification
campaign. Static checks do not initialize the product. An exceptional early
verification must record its concrete reason and smallest scope.

Report each case's valid sample count, median and min–max in raw nanoseconds;
report every invalid/failing sample separately with its actual status. No
favorable rerolls, pooling unlike source identities or treating missing values
as zero. Report-only changes consume sealed raw rows. Relevant changes invalidate
only affected evidence, with an explicit ledger entry before recollection.

Harness validity, coverage, custody, required observations, product correctness,
performance, resources and cleanup are separate statuses. Terminal Phase 1
requires all implementation/coverage/evidence gates complete; reproduced product
failures remain failed product findings and Phase 2 dependencies. Keep #21 open.
