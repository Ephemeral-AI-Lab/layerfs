# Count-changing file-edit performance family

> **Status:** Complete at exact candidate `c6c14d5a`. Earlier campaigns remain
> immutable historical evidence. The terminal family contains 25 primary IDs, six
> family-owned delete/shrink size-scaling IDs, seven existing primary verifier
> receipts, and one exact reopen verifier per scaling sample in the fixed MacBook/Docker
> Desktop/managed Linux container/real-FUSE environment.
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

## Prospective scope decision

This is an expansion of the existing count-changing family, not a new
canonical CDC chunk-cardinality family. Delete and shrinking replacement both
change logical length and already belong to this runner, workload, lifecycle,
and verifier. The six scaling IDs measure file-size scaling of their existing
POSIX temp-copy/fsync/rename implementation; they do not claim to measure
payload-chunk cardinality.

A canonical CDC chunk-cardinality family is explicitly deferred beyond
v0.1.2. That axis needs separately sealed CDC chunk maps, exact same-byte
controls, payload ObjectId deltas, and owner-side structural-edit cases. Mixing
it into these logical-length cases would leave both families ambiguous.

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

For `fs-bench-pro-edit-performance-v2` and the expanded-family `v3`,
`inner_edit_ns` starts immediately
before the selected filesystem-mutation loop and stops immediately after the
loop. It includes each operation's open, read, write, flush, required
`sync_all`, and rename. It excludes argument/schedule construction, the initial
fixture stat and inode read, workload-process launch, and the final
length/inode/output work. `execution_ns` separately measures workload-process
invocation through output completion. The final file length is still validated
immediately after `inner_edit_ns`. This corrects the superseded `v1` boundary,
which charged an internal per-operation `stat` validity check to throughput.

The expanded family uses performance schema `v3`, verifier schema `v3`, and
family summary/status schema `v4`. The 25-ID `v2`/`v3` attempts remain
immutable and are never pooled with the expanded family.

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

## Family-owned delete/shrink scaling cohort

These six IDs ship and admit with the 25 primary IDs:

| Scenario ID | Initial bytes | Operation |
| --- | ---: | --- |
| `delete-middle-2k-on-1mib-ops-1-scale` | 1,048,576 | Delete 2 KiB at `L/2 - 1024` |
| `delete-middle-2k-on-10mib-ops-1-scale` | 10,485,760 | Delete 2 KiB at `L/2 - 1024` |
| `delete-middle-2k-on-100mib-ops-1-scale` | 104,857,600 | Delete 2 KiB at `L/2 - 1024` |
| `replace-middle-shrink-4k-to-2k-on-1mib-ops-1-scale` | 1,048,576 | Replace 4 KiB at `L/2 - 2048` with 2 KiB |
| `replace-middle-shrink-4k-to-2k-on-10mib-ops-1-scale` | 10,485,760 | Replace 4 KiB at `L/2 - 2048` with 2 KiB |
| `replace-middle-shrink-4k-to-2k-on-100mib-ops-1-scale` | 104,857,600 | Replace 4 KiB at `L/2 - 2048` with 2 KiB |

These six IDs are the complete middle destructive suffix-relocation scaling
cohort for v0.1.2. They support claims about delete and shrinking replacement
through portable temp-copy/fsync/rename only; they do not generalize to every
temp-copy shape or to owner-side structural edits.

Each size uses three seeds and one fresh Store, Branch, Workspace, and workload
process per sample. Fixture byte `i` is exactly
`((i * 29 + floor(i / 7)) mod 251)`. Delete seeds are intentional repeat
labels because they supply no replacement bytes; shrinking replacement bytes
vary by seed.

That fixture is periodic and deliberately exercises sequential FUSE copy,
spool, fsync, rename, and lifecycle scaling. CDC/candidate/object counters are
retained to expose hidden Commit work, but this cohort makes no claim about
unique-content CDC or ObjectId cardinality; that belongs to the deferred
canonical CDC family.

For initial length `L`, the exact algebra is:

| Operation | Final bytes | Copied bytes | Read bytes | Written/FUSE/spool bytes |
| --- | ---: | ---: | ---: | ---: |
| Delete | `L - 2,048` | `L - 2,048` | `L` | `L - 2,048` |
| Shrink | `L - 2,048` | `L - 4,096` | `L` | `L - 2,048` |

Both rows require `commit_payload_bytes_read = 0` and
`commit_cdc_bytes_scanned = L - 2,048`. Candidate/inserted/reused object and
byte counters are reported as medians and min-max ranges so periodic-content
reuse cannot hide Commit work.

Scaling cases are an explicitly unpaired, final-candidate supplement: their
purpose is to fit and verify the file-size model, not to compare unequal
fixtures to a 256 KiB same-count control. Their result folder remains under the
count-changing family, with separate `scaling/` performance, summary, and
verification streams.

Those streams are valid only when they bind to the primary candidate's exact
revision/tree, source/product/harness/workload seals, image, and container
lineage within the same terminal campaign. Rows from another candidate identity
cannot be mixed in after collection.

## Pair contract

Every one of the 25 primary results contains a real
`paired_same_count_control_id`. A primary pair holds constant:

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
Scaling rows instead carry the exact sentinel
`not-applicable-scaling-file-size-cohort`; they are never treated as paired.

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

### Primary absolute mutation gate

For every non-anchor 256 KiB temp-copy row, the primary metric is average
mutation time per operation:

```text
median(inner_edit_ns) <= operation_count * 10,000,000 ns
displayed mutation ms/op = median(inner_edit_ns) / operation_count / 1,000,000
```

This 10 ms-per-operation boundary is the sole temp-copy latency admission
threshold: a larger median is no-go. There is no 10–11 ms tolerated band.

Copied-payload MiB/s remains a secondary
diagnostic and cannot independently fail a row whose primary latency gate
passes. Direct-POSIX operation/s gates and the frozen 32 MiB prepend lifecycle
gate remain unchanged.

This policy is prospective. All earlier MiB/s-gated evidence retains its
original status and is not reinterpreted.

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
median(candidate complete_lifecycle_ns)
------------------------------------------------ <= 1.05
median(unchanged baseline complete_lifecycle_ns)
```

The frozen baseline is source `a7583306`, workload SHA-256
`ce1e14e7c3078190085311c9b6a558bba6caa86a4930a2e26095ddf2de220ffc`.
Baseline and candidate images carry the same final-candidate product seal and
revision but distinct workload/source seals. The baseline carries the frozen
v0.1.2 temp-copy workload while the candidate carries the release workload, so
the comparison measures the combined workload change: 64 KiB to 1 MiB
streaming buffers plus removal of per-operation `stat` from the mutation-only
timer. It does not attribute improvement to either change in isolation or to
the universal edit engine. Admission also runs one exact baseline
byte/root/reopen proof beside the complete candidate verifier set.

Any pair above `1.05` requires phase/counter disposition; `1.10` remains the
no-go boundary. Require at least a
20-percent improvement only when baseline evidence proves a defect and the
retained implementation claims to optimize it.

The exact `c6c14d5a` admission has five tolerated rows:

- `append-tail-4k-ops-100` (`1.076766`): execution (`1.067x`), visibility
  (`1.078x`), and teardown (`1.101x`) account for the spread; all structural,
  byte-work, and request counters are unchanged.
- `delete-middle-2k-ops-100` (`1.096621`, the family maximum): execution is
  `1.104x` and Commit/visibility `1.088x`; the candidate issues fewer FUSE
  writes (`0.804x`) with identical bytes, objects, CDC work, and spool work, so
  the result is streaming/Commit wall variance rather than added work.
- `insert-middle-4k-ops-1` (`1.079309`): the short single-operation execution
  phase is `1.252x`, while create/end and every work counter remain equivalent;
  fixed per-process/FUSE overhead dominates this small sample.
- `replace-middle-shrink-4k-to-2k-ops-10` (`1.056754`): create is `1.088x` and
  execution `1.100x`, while the candidate uses `0.600x` the FUSE write requests
  with identical logical, CDC, object, and spool work.
- `sparse-write-past-eof-gap-60k-payload-4k-ops-1` (`1.053121`): execution is
  faster (`0.887x`); Commit (`1.118x`) and teardown (`1.361x`) explain the
  tolerated lifecycle result, with no counter evidence of additional sparse
  payload work.

These are ratio-of-medians dispositions, not medians of paired ratios. None
claims improvement. The terminal evidence is
`benchmark-results/fs-bench-pro/edit-count-changing/final-v012-count-changing-c6c14d5a`,
manifest SHA-256
`491da0d15babd56b38eef00e85f282f318e0f44a847ee5a0a7b289733d979e97`.
It contains 150 primary rows, 45 controls, 18 scaling rows, seven primary
verifier receipts, and 18 scaling verifier receipts. Its anchor replay custody
manifest is `6c9145ae590d58dced850aa836c273036af07ae39842a214cad1b5eb110d284c`.

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

The strict 10 ms/op mutation gate is an explicit owner decision: absolute
256 KiB mutation latency is the primary usability measure, while calculated
copied-payload MiB/s is secondary. The exact no-tolerance formula is
`median(inner_edit_ns) <= operation_count * 10,000,000 ns`; individual
operations inside a batch are not timed separately.

The family applies a 128 MiB physical-spool high-water ceiling, intentionally
stricter than the universal engine's 1 GiB product safety ceiling.

The earlier copied-byte floors remain visible only to explain and reproduce the
superseded evidence:

| Schedule | Target | Tolerated | Hard |
| --- | ---: | ---: | ---: |
| ops-1 | 50 MiB/s | 45 MiB/s | 30 MiB/s |
| ops-10 | 75 MiB/s | 67.5 MiB/s | 40 MiB/s |
| +4 KiB ops-100 prepend/insert | 135 MiB/s | 121.5 MiB/s | 100 MiB/s |
| +2 KiB ops-100 grow replacement | 110 MiB/s | 99 MiB/s | 80 MiB/s |
| -2 KiB ops-100 delete/shrink | 55 MiB/s | 49.5 MiB/s | 40 MiB/s |

The selected pre-freeze baseline was 56.4 MiB/s for one fsynced 256 KiB rewrite;
the first complete run later recorded 44.3-44.7 MiB/s and is correctly retained
as a no-go rather than used to move the frozen threshold. A selected Linux
`copy_file_range` diagnostic produced the same FUSE request
count and no improvement, so portable streaming temp-copy remains authoritative.

### Scaling classification

The 1 MiB absolute mutation time is measured and reported without a
predeclared 10 ms target: the 256 KiB data plus fixed FUSE/fsync/rename overhead
does not justify that bound prospectively. The 10 MiB and 100 MiB cases must
show non-superlinear behavior. For delete and shrink independently, the median
100 MiB copied-payload rate may degrade by no more than 10 percent from the
median 10 MiB rate:

```text
rate_100m >= 0.90 * rate_10m
mutation latency ~= fixed overhead + file_size / sustained copy rate
```

Every scaling row reports N, mutation/workload/Commit/complete-lifecycle
median and min-max, exact copied/read/written bytes, secondary effective copy
rate, CPU, process/cgroup RSS, spool high-water, swap, OOM, and cleanup. Exact
byte/digest/canonical-root/fresh-reconnect/FUSE-reopen verification runs
separately for all 18 samples. RSS, cgroup memory, and physical spool are hard
limited to 128 MiB; swap, OOM, timeout, and cleanup failures are never
tolerated. Each scaling performance or verifier process has a 40-second hard
timeout with no tolerated timeout band; the scaling cohort has no aggregate
latency target.

Overall family admission is the worst status across the 25 primary directional
member gates, the delete and shrink 100 MiB-versus-10 MiB scaling gates, all
seven existing primary verifier receipts, all 18 scaling verifier receipts,
and every resource/cleanup gate. A missing row or receipt is ineligible.
Selected performance and verify commands may succeed, but always record
`admission_eligible = false`; only complete directional `admission --all` can
be release-eligible. Count-changing A/A remains a diagnostic repeatability mode
and is rejected as terminal admission.

Sparse growth allocates no live RAM or physical spool proportional to the zero
gap. Complete-process RSS targets at most 105 percent of the retained
97,124,352-byte peak, hard at most 128 MiB, with zero swap.

Separate full-family verification is not part of the development loop or the
performance distribution. Existing primary proof commands retain their
40-second timeout; each scaling proof independently uses the same hard timeout.

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

- [x] One family module and one runner own all 31 timed IDs, seven existing
  primary verifier receipts, and 18 scaling reopen verifiers while reusing
  shared harness code.
- [x] The runner defaults to one explicit case/seed in performance mode and
  cannot run the full family without `--all`.
- [x] The frozen prepend retains operation, fixture, timing, schema, and oracle.
- [x] Every 1/10/100 curve is an exact prefix and declares its same-count control.
- [x] Every performance row reports latency, operations/s, phase, I/O, object,
  CPU, memory, and cleanup receipts without verifier work in its timer.
- [x] Sparse zero, pair, relative regression, anchor, family-wall, scaling,
  RSS, and
  zero-swap gates pass.
- [x] Explicit verification proves all seven primary receipts (prepend, sparse,
  four large/adversarial proofs, and baseline semantics) and all 18 scaling
  samples before publication.
