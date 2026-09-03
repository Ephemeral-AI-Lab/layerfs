# Same-count file-edit performance family

> **Status:** Implemented and accepted by the alternating A/A campaign
> `issue13-terminal-aa-final-e8226e4c`: 14 timed performance IDs and one separate
> verification group. The fixed environment is MacBook/Docker Desktop/managed
> Linux container/real FUSE; environment and implementation route are not
> scenario-name axes.
> Tracked by [GitHub issue #13](https://github.com/Ephemeral-AI-Lab/layerfs/issues/13).

## Question

How much throughput and Commit work does LayerFS require when bytes change at
the head, middle, tail, or distributed positions while file length stays exact?

Every new operation satisfies:

```text
deleted bytes == inserted bytes
result length  == original length
```

The family is the direct control for count-changing scenarios. It measures the
public production lifecycle, not an internal piece-tree microbenchmark.

## One definition file and one runner

Add exactly one family definition module and one runner:

```text
benchmark/fs-bench-pro/families/edit_same_count.rs
benchmark/fs-bench-pro/run-edit-same-count.sh
```

Reuse shared lifecycle, fixture, container, custody, receipt, and reporting
helpers. Do not create another crate or copy those helpers into the family.

The runner requires an explicit case unless `--all` is supplied and defaults to
performance-only execution:

```text
run-edit-same-count.sh RUN_ID CONTAINER_ID \
  --case overwrite-middle-4k-ops-100 \
  --seed 1 \
  --source candidate \
  --mode performance
```

Prepare the source/image-identity cache once outside selected-run timing:

```text
run-edit-same-count.sh --prepare CONTAINER_ID
```

The cache is keyed by the complete source seal and retains the host binary,
oracle workload, deterministic fixture, product/harness/workload seals, and the
issue-14 custody reference. A selected performance invocation must finish in
under two seconds and performs no build, fixture generation, or environment
discovery.

Modes:

| Mode | Development behavior |
| --- | --- |
| `performance` | Default; run only the selected case/seed/source arm and emit timed receipts; no full digest/root/reopen verifier |
| `verify` | Run the selected case's exact bytes/root/reopen and resource verifier with no performance distribution |
| `admission` | Explicit `--all`; run all cases/seeds and then separate verification for release evidence |

Performance mode still rejects process failure, incomplete operation count,
wrong reported length, timeout, OOM, swap, or cleanup failure. Those are run
validity checks, not a correctness benchmark. No verification wall enters a
performance sample.

## Fixed environment and lifecycle

Every timed sample uses:

```text
MacBook host with host-resident Store and Client
-> Docker Desktop managed Linux container
-> real LayerFS FUSE mount
-> one fresh workload process
-> one Commit and visibility acknowledgement
-> End
```

Fixture generation, Store/Client/container preparation, source sealing, full
verification, fresh reconnect, and report generation are outside timing. Cache
policy, image digest, resource limits, and acknowledgement boundary are exact
baseline/candidate identities.

## Frozen v0.1.0 anchors

Keep the registered IDs and operations unchanged:

| Scenario ID | Report display name | Operation |
| --- | --- | --- |
| `small-edit` | `legacy-overwrite-distributed-10b-ops-1` | One deterministic ten-byte overwrite and one Commit on the 32 MiB fixture |
| `edit16` | `legacy-overwrite-distributed-10b-ops-16-commit-each` | Sixteen distributed ten-byte overwrite/Commit cycles on one 32 MiB fixture |

The clearer display name supplements the frozen raw ID; it does not rename or
redefine historical evidence. These anchors do not consume the new schedule.

## New timed scenario IDs

All new rows use one deterministic 256 KiB file. For each curve, `ops-1` and
`ops-10` are exact prefixes of `ops-100` for the same seed; all selected edits
precede one Commit.

### Head overwrite

```text
overwrite-head-4k-ops-1
overwrite-head-4k-ops-10
overwrite-head-4k-ops-100
```

Replace the first 4 KiB with 4 KiB. Later operations deliberately supersede
earlier head bytes.

### Middle overwrite

```text
overwrite-middle-4k-ops-1
overwrite-middle-4k-ops-10
overwrite-middle-4k-ops-100
```

Replace 4 KiB at deterministic positions within the middle half of the file.
This is the same-count control for middle insert/delete/grow/shrink.

### Tail overwrite

```text
overwrite-tail-4k-ops-1
overwrite-tail-4k-ops-10
overwrite-tail-4k-ops-100
```

Replace the last 4 KiB with 4 KiB. This is the same-count control for append,
truncate, and sparse write-past-EOF.

### Distributed variable-size overwrite

```text
overwrite-distributed-1b-to-4k-ops-1
overwrite-distributed-1b-to-4k-ops-10
overwrite-distributed-1b-to-4k-ops-100
```

Use deterministic seed-derived offsets and lengths from 1 byte through 4 KiB.
Overlaps are allowed and later operations win.

## Pair map

Count-changing results retain `paired_same_count_control_id`:

| Same-count control | Count-changing operations it controls |
| --- | --- |
| `overwrite-head-4k-ops-N` | `prepend-head-4k-ops-N` |
| `overwrite-middle-4k-ops-N` | middle insert, delete, grow-replace, and shrink-replace rows |
| `overwrite-tail-4k-ops-N` | append, truncate, and sparse write-past-EOF rows |

The pair holds fixture, seed, operation count, supplied bytes where applicable,
one-Commit topology, FUSE environment, timing, resource limits, and schema
constant. The intentional difference is whether logical length changes.

## Verification group

`overwrite-fragmented-10b-ops-1000-proof` runs only in `verify` or `admission`
mode and contains:

1. 1,000 increasing disjoint writes;
2. the same offsets in descending order; and
3. 1,000 overlapping writes in a deterministic 64 KiB hotspot.

Retain 100/1,000 checkpoints:

```text
live pieces/ranges and retained metadata:
    value(1000) <= 12 * max(1, value(100))

cumulative comparisons/tree visits:
    value(1000) <= 18 * max(1, value(100))
```

After the universal engine lands, require exactly zero complete interval-map
clones, full interval-map rescans, later-offset rekeys, and complete-file
materializations.

## Performance receipts

Following the v0.1.1 report style, retain per sample:

- complete Create-through-End wall;
- Workspace Create, workload execution, Commit, visibility, and End phases;
- attempted/completed operations and operations per second;
- supplied, unique, overlapping, identical, and superseded bytes;
- supplied-byte throughput where meaningful;
- FUSE request/byte counts and spool allocated/live/superseded/peak bytes;
- piece count/height/charge and tree visits;
- candidate/inserted/reused objects and bytes plus transaction maxima;
- CPU, peak RSS, cgroup peak, swap, timeout/OOM, and cleanup status.

`inner_edit_ns` spans the selected positional write calls only and defines
operations/s and supplied-byte/s. The final file fence is outside that inner
throughput interval but remains inside execution, Commit visibility, and
complete-lifecycle timing.

Verification mode additionally records exact bytes, SHA-256, canonical root,
fresh reopen, and complete cleanup. Missing required receipts invalidate the
run; they are never inferred as zero.

## Baseline and targets

Retained anchors:

```text
small-edit Commit median     4.503 ms, hard <= 6 ms
small-edit complete median  29.653 ms
edit16 complete median     156.446 ms, hard <= 200 ms
host peak RSS               97,124,352 bytes
```

Run three paired seed samples in alternating A/B, B/A, A/B order. When both
arms have the same image/commit/product/harness/workload identity, report A/A
repeatability and make no baseline/candidate improvement claim. For each new
row:

```text
median(candidate / unchanged baseline) <= 1.05
```

Ratios at most `1.05` pass. Ratios above `1.05` through `1.10` are tolerated
passes only with retained phase/counter disposition. Ratios above `1.10` are
no-go. The less-than-2-ms exception applies only to named nonaggregate phases,
never complete lifecycle or family walls. Require at least a
20-percent improvement only when baseline evidence proves a defect and the
retained implementation claims to optimize it.

Frozen new-row operation/s floors are:

```text
head / middle / tail       target 500, tolerated 450
distributed variable-size target 250, tolerated 225
```

The family performance budget for one 42-sample source arm is:

```text
14 timed IDs * 3 seeds
target <= 3.0 s
tolerated <= 3.3 s
hard <= 6.0 s
```

The paired baseline/candidate accounting budget is target/tolerated/hard
6.0/6.6/12.0 seconds. Complete-process RSS targets at most 105 percent of the
retained 97,124,352-byte peak, hard at most 128 MiB, with zero swap.

Separate full-family verification is not part of the development loop or the
performance distribution. Admission runs the frozen 42-row performance
campaign plus only the separately defined fragmentation verifier group; it does
not add 42 unregistered exact-verifier rows. Its timeout is 20 seconds per
verifier invocation.

## Files to read

- [v0.1.2 release plan](README.md)
- [`fs-bench-pro` family format](fs-bench-pro-format.md)
- [Universal edit engine](universal-file-edit-engine.md)
- [Count-changing family](count-changing-file-edits.md)
- [Benchmark contract](../benchmarking.md)
- [`fs-bench-pro` harness](../../../../benchmark/fs-bench-pro/src/main.rs)
- [Existing workload helper](../../../../benchmark/fs-bench-pro/workload.rs)
- [Retained v0.1.1 report](../../../../release-notes/0.1.1/benchmark-results.md)

## Acceptance criteria

- [x] One family module and one runner own all 14 timed IDs and the verifier
  group while reusing shared harness code.
- [x] The runner defaults to one explicit case/seed in performance mode and
  cannot run the full family without `--all`.
- [x] Frozen anchors retain their operation, fixture, timing, schema, and oracle.
- [x] New 1/10/100 schedules are exact prefixes and preserve 256 KiB length.
- [x] Every performance row reports latency, operations/s, phase, I/O, object,
  CPU, memory, and cleanup receipts without verifier work in its timer.
- [x] A/A repeatability, absolute anchor, family-wall, RSS, and zero-swap gates
  pass.
- [x] The explicit verifier proves exact bytes/root/reopen and the 1,000-edit
  structural limits before publication.
