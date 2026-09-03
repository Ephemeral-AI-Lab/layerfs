# Count-changing file-edit performance family

> **Status:** Proposed v0.1.2 family: 25 timed performance IDs and four
> separate verification groups. The fixed environment is MacBook/Docker
> Desktop/managed Linux container/real FUSE; environment and implementation
> route are not scenario-name axes.
> Tracked by [GitHub issue #15](https://github.com/Ephemeral-AI-Lab/layerfs/issues/15).

## Question

How much throughput and Commit work does LayerFS require when head, middle,
tail, sparse, or unequal-replacement operations change a file's logical length?

Every new operation satisfies:

```text
deleted bytes != inserted bytes
result length  != prior length
```

Each row declares its same-count control so length-changing cost can be
separated from position, supplied bytes, operation count, environment, and
lifecycle overhead.

## One definition file and one runner

Add exactly one family definition module and one runner under the existing
benchmark:

```text
benchmark/fs-bench-pro/families/edit_count_changing.rs
benchmark/fs-bench-pro/run-edit-count-changing.sh
```

Reuse shared lifecycle, fixture, container, custody, receipt, and reporting
helpers. Do not create another crate or duplicate those helpers.

The runner requires an explicit case unless `--all` is supplied and defaults to
performance-only execution:

```text
run-edit-count-changing.sh \
  --case insert-middle-4k-ops-100 \
  --seed 1 \
  --mode performance
```

Modes:

| Mode | Development behavior |
| --- | --- |
| `performance` | Default; run only the selected case/seed/source arm and emit timed receipts; no full digest/root/reopen verifier |
| `verify` | Run the selected exact byte/length/root/reopen/resource verifier with no performance distribution |
| `admission` | Explicit `--all`; run all cases/seeds and then separate verification for release evidence |

Performance mode still rejects process failure, incomplete operation count,
wrong reported final length, timeout, OOM, swap, or cleanup failure. No verifier
wall enters a performance sample.

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
arm identities. Terminal acceptance uses distinct sealed baseline and candidate
containers; identical-source diagnostics use alternating A/A labels on one
prepared daemon and make no improvement claim.

The benchmark daemon is one-shot. After each measured process disconnects, the
runner observes its clean container exit before starting the next arm. Baseline
and candidate use distinct containers created within 60 seconds of each other
and receive the same retained untimed preconditioning sample before collection.

## Frozen v0.1.0 anchor

Keep the registered row unchanged:

| Scenario ID | Report display name | Operation |
| --- | --- | --- |
| `prepend-temp-copy-rename` | `legacy-prepend-head-10b-on-32m-temp-copy-rename` | Prepend ten bytes to the frozen 32 MiB file through temp-copy, fsync, and rename |

The display name supplements the frozen raw ID; it does not redefine historical
evidence or consume a new schedule.

## New timed scenario IDs

All new curves use one deterministic 256 KiB file. For each curve, `ops-1` and
`ops-10` are exact prefixes of `ops-100` for the same seed; all operations happen
in one fresh process before one Commit.

Operations without a direct POSIX insert/delete primitive use deterministic
temp-copy, fsync, and rename inside the workload. Append, truncate, and
write-past-EOF use their direct filesystem operations. Scenario names describe
the file transformation rather than that implementation detail.

### Head prepend

```text
prepend-head-4k-ops-1
prepend-head-4k-ops-10
prepend-head-4k-ops-100
```

Insert 4 KiB at offset zero. Pair with `overwrite-head-4k-ops-N`.

### Tail append

```text
append-tail-4k-ops-1
append-tail-4k-ops-10
append-tail-4k-ops-100
```

Append 4 KiB at the current EOF. Pair with `overwrite-tail-4k-ops-N`.

### Middle insertion

```text
insert-middle-4k-ops-1
insert-middle-4k-ops-10
insert-middle-4k-ops-100
```

Insert 4 KiB at the current midpoint. Pair with
`overwrite-middle-4k-ops-N`.

### Middle deletion

```text
delete-middle-2k-ops-1
delete-middle-2k-ops-10
delete-middle-2k-ops-100
```

Delete 2 KiB centered on the current midpoint. Pair with the supplemental
same-count control `overwrite-middle-2k-ops-N`.

### Tail truncation

```text
truncate-tail-2k-ops-1
truncate-tail-2k-ops-10
truncate-tail-2k-ops-100
```

Remove 2 KiB from the current tail. Pair with the supplemental same-count
control `overwrite-tail-2k-ops-N`.

The 2 KiB destructive schedules are a prospective correction made before
evidence collection: 100 exact 4 KiB removals cannot fit in the fixed 256 KiB
fixture. Matching 2 KiB same-count controls hold fixture, seed, byte quantity,
position, operation count, process/Commit topology, and environment constant;
they are issue-3 controls rather than additional issue-2 timed family members.

### Sparse write past EOF

```text
sparse-write-past-eof-gap-60k-payload-4k-ops-1
sparse-write-past-eof-gap-60k-payload-4k-ops-10
sparse-write-past-eof-gap-60k-payload-4k-ops-100
```

Write 4 KiB at `current_EOF + 60 KiB`, producing a 60 KiB logical-zero gap and
4 KiB supplied payload. Pair with `overwrite-tail-4k-ops-N` and record logical
zero bytes separately from supplied bytes.

### Growing middle replacement

```text
replace-middle-grow-2k-to-4k-ops-1
replace-middle-grow-2k-to-4k-ops-10
replace-middle-grow-2k-to-4k-ops-100
```

Delete 2 KiB and insert 4 KiB for a `+2 KiB` delta. Pair with
`overwrite-middle-4k-ops-N`.

### Shrinking middle replacement

```text
replace-middle-shrink-4k-to-2k-ops-1
replace-middle-shrink-4k-to-2k-ops-10
replace-middle-shrink-4k-to-2k-ops-100
```

Delete 4 KiB and insert 2 KiB for a `-2 KiB` delta. Pair with
`overwrite-middle-2k-ops-N` so supplied byte quantity also remains exact.

## Pair contract

Every result contains `paired_same_count_control_id`. A pair holds constant:

- 256 KiB fixture identity;
- seed and deterministic replacement-byte stream;
- head, middle, or tail intent;
- operation count and one-Commit topology;
- Docker/FUSE environment, cache policy, limits, timing, and schema; and
- supplied bytes where the operation permits an exact match.

It deliberately varies deleted versus inserted length. Temp-copy transformations
may replace an inode, while direct append/truncate and owner-side implementation
checks preserve it; every scenario therefore uses its own exact inode and
canonical-root oracle. Cross-expression comparison requires final byte/digest
equality only where the declared transformations are equivalent.

## Verification groups

These run only in `verify` or `admission` mode:

| Verification ID | Required proof |
| --- | --- |
| `insert-middle-4k-on-8m-proof` | Exact 8 MiB prefix/suffix preservation around a 4 KiB middle insertion |
| `delete-middle-4k-on-8m-proof` | Exact suffix relocation and no lost surrounding bytes after middle deletion |
| `rewrite-full-grow-8m-to-12m-proof` | Full replacement grows to deterministic 12 MiB, reads no irrelevant old payload after universal-engine lowering, and leaves exact final state |
| `rewrite-full-shrink-8m-to-4m-proof` | Full replacement shrinks to deterministic 4 MiB and reclaims all superseded ephemeral state |

The focused timed rows measure latency and throughput; these larger groups prove
boundary correctness and resources without contaminating performance timing.

## Performance receipts

Following the v0.1.1 report style, retain per sample:

- complete Create-through-End wall;
- Workspace Create, workload execution, Commit, visibility, and End phases;
- attempted/completed operations and operations per second;
- copied/read payload for temp-rewrite operations and payload throughput;
- supplied, inserted, deleted, overlapping, superseded, and logical-zero bytes;
- FUSE request/byte counts and spool allocated/live/superseded/peak bytes;
- piece count/height/charge and tree visits;
- candidate/inserted/reused/final objects and bytes plus transaction maxima;
- CPU, peak RSS, cgroup peak, swap, timeout/OOM, and cleanup status.

Verification mode additionally records exact length, bytes, zero ranges,
SHA-256, expression-appropriate root/inode behavior, fresh reopen, and complete
cleanup. Missing receipts invalidate the run.

## Baseline and targets

Retained prepend anchor:

```text
complete median                 223.763 ms, hard <= 250 ms
edit through visibility median 204.163 ms
Commit median                    18.808 ms
kernel read bytes                33,554,432
kernel write/spool bytes         33,554,442
```

Run three paired seed samples in alternating order. For each new row:

```text
median(candidate / unchanged baseline) <= 1.05
```

The frozen baseline is source `a7583306`, workload SHA-256
`ce1e14e7c3078190085311c9b6a558bba6caa86a4930a2e26095ddf2de220ffc`.
Baseline and candidate retain identical LayerFS product sources; the comparison
therefore attributes changes only to the portable container workload algorithm,
not to the universal edit engine. Admission also runs one exact baseline
byte/root/reopen proof beside the complete candidate verifier set.

Any pair above `1.10` requires phase/counter disposition. Require at least a
20-percent improvement only when baseline evidence proves a defect and the
retained implementation claims to optimize it.

Provisional family performance budget for one 75-sample source arm:

```text
25 timed IDs * 3 seeds
target <= 10 s
hard   <= 20 s
```

The paired baseline/candidate accounting budget is 20/40 seconds. The 256 KiB
fixture keeps 100-operation temp-rewrite curves bounded while the frozen 32 MiB
prepend retains the large-file control. Freeze final operation/s and payload-
throughput floors after the unchanged implementation is measured.

The family applies a 128 MiB physical-spool high-water ceiling, intentionally
stricter than the universal engine's 1 GiB product safety ceiling.

The unchanged-source pre-publication baselines showed that required
per-operation fsync makes one shared copied-byte rate invalid. The prospective
floors are therefore frozen by schedule and transformation:

| Schedule | Target | Tolerated | Hard |
| --- | ---: | ---: | ---: |
| ops-1 | 50 MiB/s | 45 MiB/s | 30 MiB/s |
| ops-10 | 75 MiB/s | 67.5 MiB/s | 40 MiB/s |
| +4 KiB ops-100 prepend/insert | 135 MiB/s | 121.5 MiB/s | 100 MiB/s |
| +2 KiB ops-100 grow replacement | 110 MiB/s | 99 MiB/s | 80 MiB/s |
| -2 KiB ops-100 delete/shrink | 55 MiB/s | 49.5 MiB/s | 40 MiB/s |

The selected pre-freeze baseline was 56.4 MiB/s for one fsynced 256 KiB rewrite;
the first complete run later recorded 44.3-44.7 MiB/s and is correctly retained
as a no-go rather than used to move the frozen threshold. Linux
`copy_file_range` produced the same FUSE request count and no improvement, so
portable streaming temp-copy remains authoritative.

Sparse growth allocates no live RAM or physical spool proportional to the zero
gap. Complete-process RSS targets at most 105 percent of the retained
97,124,352-byte peak, hard at most 128 MiB, with zero swap.

Separate full-family verification is not part of the development loop or the
performance distribution. Its provisional admission timeout is 40 seconds.

## Files to read

- [v0.1.2 release plan](README.md)
- [`fs-bench-pro` family format](fs-bench-pro-format.md)
- [Universal edit engine](universal-file-edit-engine.md)
- [Same-count family](same-count-file-edits.md)
- [Benchmark contract](../benchmarking.md)
- [`fs-bench-pro` harness](../../../../benchmark/fs-bench-pro/src/main.rs)
- [Existing workload helper](../../../../benchmark/fs-bench-pro/workload.rs)
- [Retained v0.1.1 report](../../../../release-notes/0.1.1/benchmark-results.md)

## Acceptance criteria

- [ ] One family module and one runner own all 25 timed IDs and four verifier
  groups while reusing shared harness code.
- [ ] The runner defaults to one explicit case/seed in performance mode and
  cannot run the full family without `--all`.
- [ ] The frozen prepend retains operation, fixture, timing, schema, and oracle.
- [ ] Every 1/10/100 curve is an exact prefix and declares its same-count control.
- [ ] Every performance row reports latency, operations/s, phase, I/O, object,
  CPU, memory, and cleanup receipts without verifier work in its timer.
- [ ] Sparse zero, pair, relative regression, anchor, family-wall, RSS, and
  zero-swap gates pass.
- [ ] Explicit verification proves exact bytes/length/root/inode/reopen and all
  four large/adversarial groups before publication.
