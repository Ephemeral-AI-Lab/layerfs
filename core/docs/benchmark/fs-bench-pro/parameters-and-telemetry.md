# Parameters, telemetry, and receipts

> **Status:** Proposed selector and receipt fields. Values requiring a product
> or workload decision remain unfrozen.

## Selection parameters

Each selection resolves exactly one registered case and records these values.
The names below describe the interface to freeze; only existing fs-bench-pro
arguments retain their current meaning.

| Parameter | Proposed values and rule |
| --- | --- |
| mode (registry/receipt field) | First pass fixes `daemon-host` for #231; no mode switch or empty `storage-direct` adapter. Add explicit mode selection with the first registered direct case. |
| claim scope (registry field, not a new mode) | `public-workflow` for the selected Init/SDK/FUSE operation, or `c1-c2-component` for `storage-direct`. #193's matched Service delivery comparison has its own contract and identity. |
| `run --case ID` / `run --family init_namespace` | One exact case or the three selected first-pass cases, each once; family run builds once. `list`, `verify --run`, and `report --run` are the only other initial verbs. |
| `--seed` / `--repetition` | Exactly the selector allowed by the registered case; pin to `1` and reject other values for admission. A deterministic fixed fixture records seed as not applicable. |
| setup (registry/receipt field) | #231 always creates a fresh Init destination; its immutable source fixture is lazily reused. Later post-initialization cases may use a verified independent byte copy, never an APFS/reflink or cold claim. |
| `--cache-contract` | Required registered cache policy and revision. Unverified state yields `INELIGIBLE` or `INCOMPLETE`, not a passing sample. |
| source identity | Exact source/tree/build/fixture seal in the receipt. No first-pass control/candidate arm, paired scheduler or source-arm flag. Dirty-source iterations are diagnostic only. |
| construction workers | Record the effective value and export/assert `LAYERFS_CONSTRUCTION_WORKERS=1` for commit, capture, and snapshot work. Only the registered namespace initialization case has the documented multi-worker exception. |
| image ID (receipt field) | Resolve and pin the full immutable daemon image ID when the real container route is selected; do not trust a mutable tag. |
| CPU and memory limits (registry/receipt fields) | Freeze daemon/container limits per case; record actual host limits separately. Do not add operator tuning merely to turn a miss into a pass. |
| time budgets (registry/receipt fields) | Preserve product and complete-command bounds; #231 adds a hard 5 s verifier, a 30 s Cargo-build ceiling and a recommended 30 s family cycle. Record every miss. |
| `--out` | Fresh, nonexisting run directory inside the owning worktree. Receipts are append-only and never overwritten. |

The [#231 first-pass spec](issue-231/SPEC.md) owns the initial CLI and speed
limits. It lazily prepares only a selected source fixture and removes owned
temporary output after proof. Do **not** port legacy control/candidate arms,
`--perf-samples`, a separate prepare/prune command, benchmark-only telemetry
profile or second sampling/output protocol. Telemetry selection uses the
product assembly's existing environment contract below. There is exactly one
performance sample per case at its exact source identity and one registered
seed/repetition. Parallel agents use different worktrees with private mutable
targets/fixtures/results; a worktree-local run lock never blocks another
worktree. The runner cannot omit a registered row.

## Use the existing `layerfs-telemetry` path

The crate deliberately leaves configuration and output ownership to application
assembly. Reuse that assembly and its public APIs; do not add a benchmark sampler,
timer tree, encoder, local spool, or telemetry-only product hook.

Read the [telemetry API](../../../crates/layerfs-telemetry/README.md),
[instrumentation guide](../../../crates/layerfs-telemetry/USAGE.md), and
[runtime integration contract](../../architecture/14-service-runtime.md) as the
implementation reference. The native `Runtime` currently bounds each timing
tree to 256 nodes, 32 levels, and 256 KiB; it reserves eight recording slots in
a 4 MiB pool and emits at most 16 KiB per record. Preserve those application
limits and report a clipped tree as incomplete; do not add a benchmark-specific
limit override.

The existing daemon and Service assembly reads these settings:

| Existing setting | Meaning |
| --- | --- |
| `LAYERFS_TELEMETRY=off` | Default. `Runtime::disabled`; no timer, monitor, output worker, or native telemetry record. |
| `LAYERFS_TELEMETRY=forward` | Enabled. Bounded `LFT1` JSON lines go to process stderr; this is the proposed benchmark collection route. |
| `LAYERFS_TELEMETRY=local` / `both` | Existing operational-retention modes. Use only in a separately declared output-profile selection; they are not the benchmark evidence sink. |
| `LAYERFS_RUN_ID`, `LAYERFS_NAMESPACE` | Required non-secret event identity when enabled; use the same run ID and distinct recorded namespaces for participating processes. |
| `LAYERFS_TELEMETRY_DIRECTORY` | Required only for `local` / `both`; never point it at append-only benchmark evidence. |

Enabled setup that lacks a run ID or nonzero namespace, selects an unsupported
mode, or fails to start `Runtime` returns to `Runtime::disabled()` after a
bounded diagnostic. Therefore the runner must confirm actual operation and
run-summary records in captured stderr; setting the environment is not proof
that telemetry was active.

The current [daemon](../../../crates/layerfs-daemon/src/config.rs) and
[Service](../../../crates/layerfs-service/src/native/config.rs) assemblies select
timing, CPU and RSS, a 100 ms monitor interval, 600 history slots, 32 windows,
and `OutputConfig::forward()` bounds.
Do not add per-case interval, sampler, queue, or encoder knobs. Record the exact
environment, source/build identity, and configured profile with the run. If the
application's existing configuration changes, freeze and compare the changed
product configuration explicitly.

- **`daemon-host`:** set the existing environment on each production process.
  The daemon and Service already use `Runtime`, `OperationRecorder`, and
  `Runtime::publish`; capture stderr separately from protocol stdout. Keep each
  process's LFT1 events keyed by run, role, namespace, PID, and incarnation.
- **`storage-direct`:** the benchmark process must use the crate's public native
  `Runtime` and `OperationRecorder::run` around the public C1-to-C2 operation,
  passing the supplied `TimingScope` through real product APIs and publishing the
  returned `Diagnostic`. Enable the existing `native` feature; configure the
  same timer/CPU/RSS, 100 ms monitor, window, and `OutputConfig::forward()` values
  used by the current application assembly, with the recorded run identity and
  collector role. Honor `LAYERFS_TELEMETRY=off|forward` using `Runtime::disabled`
  or that same native configuration; do not define another environment switch.
  Do not implement another timer or CPU/RSS sampler. Existing Stage 6
  `support/instruments.rs` readings stay under their frozen Stage 6 contract and
  remain separate from LFT1. If the new Runtime path cannot emit required
  telemetry on a platform, the telemetry-required row is `NOT_RUN`/`INCOMPLETE`;
  do not fill the gap with a substitute.
- **All modes:** `OutputMode::Forward` sends the crate's native bounded `LFT1`
  records to stderr. Follow the
  [ingestion and retention contract](telemetry-ingestion-and-retention.md):
  preserve the exact product event lines in one `telemetry.lft1` case file,
  parse them into the receipt, then recycle redundant capture and operational
  files only after validation of the retained event file. Record cleanup in
  the final receipt and manifest. Never rewrite the
  event schema or treat a derived summary as the sole raw evidence.
  `Runtime::publish` is asynchronous and best effort. Require the expected
  operation/run-summary records, capture integrity and zero reported
  dropped/failed/overflow loss for a telemetry-complete row. Startup
  diagnostics, missing records or output loss make telemetry evidence
  incomplete while the known product result remains unchanged.

Telemetry on/off and output destination are part of the run identity even though
they are existing environment settings, not benchmark CLI switches. `off` is a separate diagnostic
profile; it emits no product timing/CPU/RSS and cannot satisfy a telemetry-on
selection.

Interpret the emitted values precisely:

- Timer nodes are inclusive monotonic elapsed time. A parent includes its children;
  do not add parent and child, infer CPU from elapsed, or claim a distributed
  wall interval from separate machines.
- CPU is the shared process's cumulative user/system delta from the first to last
  covered sample. Check those timestamps against the window's opened/closed
  timestamps: samples can straddle the operation boundary. It is not exclusive
  CPU attributed to the operation. Missing samples, gaps, changed process
  incarnation/source, or fewer than two valid samples mean unavailable, never
  zero.
- Memory is sampled process RSS maximum, not heap size and not an exact phase
  peak. `memory.peak` is a lifetime high-water unless it has a verified reset.
  Keep the crate's process RSS, external cgroup domains, file cache, spool, and
  Store disk separate. The crate does not collect cgroup, heap-allocation, or
  disk-occupancy metrics; retain any already-required benchmark instruments for
  those distinct domains without relabeling them as `layerfs-telemetry`. Do not
  combine Stage 6's exact phase-boundary CPU readings with these sampled
  process-window deltas or change the existing Stage 6 receipts.
- Short operations may not contain enough 10–1000 ms native monitor samples
  for a CPU delta or an RSS peak. `resource_status=sampled` alone does not
  establish boundary coverage: window opening can reuse a sample taken before
  the operation. Check the actual first/last and opened/closed timestamps.
  Preserve that limitation. Do not add sleeps, combine several cases into one
  observation window, or report unavailable as zero. Keep the crate's coverage
  fields and classify required-but-unavailable telemetry as incomplete.

The benchmark runner still measures complete-command wall using its existing
monotonic clock and captures cgroup/cache/storage evidence using the established
benchmark tools. Those are separate measurement scopes, not a replacement for
product telemetry. The crate's operation-window RSS/CPU is never used to invent
an end-to-end clock or container memory figure.

## One sample and four timing scopes

The [macro/micro map](pipeline-and-modes.md#four-macro-timing-scopes) defines
M1 public workflow, M2 delivery, M3 C1/C2 and M4 C5. These are report
headings, not fields in the native `LFT1` protocol. The benchmark records
exactly **one** caller operation duration for each registered case at one source identity. The
caller starts immediately before the frozen public entrypoint and stops after
its promised acknowledgement. Its raw `operation_ns` and `sample_count=1` are
the performance headline. The runner records setup, complete-command wall,
verification and cleanup on separate clocks and fields. Do not present the
single observation as a median, range or percentile.

`LFT1` supplies independent daemon and Service local operation trees. Each
node has an inclusive `elapsed_ns`, outcome and possible `incomplete` flag;
those trees are diagnostic subviews, not a cross-process tree rooted at the
caller. A nested or repeated call inside the workload is one micro event of
the same sample. Record its label, process/clock domain, invocation count,
individual raw durations and completeness. If a report also gives a sum of
multiple invocations, label it `sum_work_ns` and never treat it as elapsed
caller wall: calls can overlap and parent spans include children. Missing
Workspace or C5 duration spans are `null` with a reason, not inferred by
subtracting other process timers.

The human per-case table shows case, mode, declared workload/cache state,
`sample_count=1`, raw caller `operation_ns`, complete-command wall and separate
performance, verification, telemetry and cleanup statuses. A linked micro
table shows available M1 steps, daemon/Service M2 roots, M3 C1/C2 labels and
M4 history labels with their source and `elapsed_ns`. It does not claim four
exclusive totals or produce more performance samples to fill the table.

## Minimum receipt identity

For a telemetry-on case, keep one raw `telemetry.lft1` product-event file and
one harness receipt. A telemetry-off case records that no product telemetry
was selected. The receipt points to any retained event file and records the
parser version and per-producer source ranges/counts; the manifest hashes the
retained files. Bind the raw evidence and receipt to:

- mode, registered claim scope, family/case, seed/repetition, case revision,
  source identity, cache contract, and exact existing telemetry environment; reference
  #193 evidence separately when attributing daemon-to-host delivery;
- source commit/tree/dirty state, product and harness identities, compilation
  and dependency seals, locked manifests, binary hashes and image ID;
- prepared-input and expected-result digests, setup method, copy identity,
  worker/concurrency settings, effective CPU/memory/PID limits, and host/OS;
- operation start/end/boundary, product elapsed, full command wall, and separate
  setup/startup/verification/cleanup durations;
- telemetry source/role/incarnation, requested fields, first/last timestamps,
  sample count, gaps, largest gap, CPU delta validity, sampled RSS and report
  loss/omission status;
- cache invalidation method and version, required source set, residency result,
  any device-read evidence required by the registered cold contract, and
  `PASS`/`FAIL`/`INELIGIBLE`/`INCOMPLETE`/`NOT_RUN` statuses separately for
  operation, verification, cleanup, and eligibility.

Never derive one missing field from another metric or collapse all status into a
single `PASS` bit.
