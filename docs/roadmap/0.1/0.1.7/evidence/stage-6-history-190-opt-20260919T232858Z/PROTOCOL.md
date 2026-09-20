# #190 optimization execution protocol

> Status: Research; informative and not a product contract.

The owner's current instruction explicitly requests subagents to measure, implement
batching and bounded record reuse, prove correctness/work reduction, confirm on
stride3 and update #190 at each step with LOC. It supersedes the earlier
investigation-only/no-product-change scope for these steps. Subtree format changes,
codec/worker changes and broader optimizations are not assumed necessary.

## Prospectively frozen execution

- Raw diagnostic performance complete-command execution ceilings: **120 seconds
  stride10, 240 seconds stride3**, frozen before any sample. This implements the
  now-requested diagnostic collection under the retained-history lane's separate
  ceiling ruling; it does not change the ordinary 15/25-second gates or retrospectively
  waive a historical miss. A timeout is retained and not enlarged.
- Independent verification has a 60-second hard complete-command ceiling and
  the existing 10/20-second stride10/stride3 verification-work targets. Exceeding
  a target is TARGET_MISS; do not enlarge it. Verification remains sampled as
  implemented by this lane and is not relabelled exhaustive.
- One sample for each baseline/candidate and each stride10/stride3 selection.
  Order: baseline stride10, baseline stride3, candidate stride10, candidate stride3.
  Baseline detailed spans decide whether proposed changes remain justified.
  No stride1 run. No rerun to select a better result.
- Same frozen harness instrumentation across arms, same corpus and behavioral
  switches, `LAYERFS_CONSTRUCTION_WORKERS=1`, `LAYERFS_HISTORY_PHASES=1`. All eight
  inherited history behavior switches are explicitly unset. Record exact environment.
- Cache: fresh growing Store, ordinary intra-chain Store/OS cache residency
  uncontrolled. No priming or cache purge. This is an **INELIGIBLE cold/performance
  admission comparison**. Diagnostic elapsed changes describe these invocations;
  reduced actual read work and correctness require separate evidence. No release
  speedup claim or comparison to a new v0.1.6 run.
- Reuse immutable corpus preparation and incremental locked builds. No prepared
  Store clone exists for this creation lane; each performance child creates a
  fresh Store and a never-used output path. Store/binary hashes are collected
  outside performance and never used to warm an upcoming measured input Store.
- Hold legacy TMPDIR flock, distinct /tmp flock, then core O_EXCL measurement
  lock for resource-sensitive commands. No builds/tests overlap measured runs;
  squads only read/write source while the coordinator owns measurement.
- Pre/post observations record CPU/load/memory and competing resource-sensitive
  processes. A free lock is not a fully quiet-machine proof. No other owner's
  process is interrupted. Preserve noisy/failed samples; do not select retries.
  Prospectively require no named cargo/rustc/fs-bench competitor and at least
  70% aggregate CPU idle in the second one-second observation before launch.
  A deferred preflight is not a performance sample; retain its observation.
  Desktop background activity remains disclosed even when this check succeeds.
- Raw trace/timing/phases/stdout/stderr, command wall/exit/timeout, source and
  binary identities, harness/product manifests, declared limits and every nonpassing
  line remain append-only. Copy the performance trace before verification appends
  to the original, as required by the existing verifier interface.

## Source and LOC

Initial HEAD `9f35c49ad62956f131dc2676787f99d69659686e`. Product worktree initially
matches HEAD; inherited harness instrumentation and unrelated architecture/doc
edits are recorded in initial-status.txt and inherited-harness.patch. Build each
arm once after its source is ready; retain immutable binary copies with source
and dependency manifests. Do not represent dirty worktree diagnostics as clean
release seals.

Counter: `python3 tools/production_loc.py --json`, same counter bytes and source
classification before/after. Initial production LOC: reference **65,417**, core
**20,116** (content 12,512; storage 6,841; telemetry 763), combined **85,533**.
Runtime SQL is included, tests/docs/harness/tools and inline legacy tests excluded.
Retain exact per-file source hashes and final LOC. No commit is required; if a
commit is made, recompute first-parent versus final staged snapshots per AGENTS.md.

## Checkpoints

1. Publish kickoff, run baseline and report phase/read-work attribution (LOC 0).
2. Implement and report bounded parent batching.
3. Implement and report within-operation reuse; one cohesive implementation may
   serve steps 2/3, but its combined measured effect must not be double counted.
4. Run meaningful public-API correctness/structural tests, explicit core locked
   test/example/clippy/fmt checks and boundary guard; run both candidate selections
   and separate verification. Retain every failure and unchanged baseline issue.
5. Independent review; consider deeper changes only if measurements justify them.
   Update issue and append active ledger with exact results, identities and LOC.
