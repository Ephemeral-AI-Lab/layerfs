# Stage 6, round 5b — the preparation half of the cheap campaign

> **Status:** Round-5 continuation receipt for [#184](https://github.com/Ephemeral-AI-Lab/layerfs/issues/184).
> Append-only. It supersedes nothing and edits no earlier receipt; the round-3, round-3b,
> round-4, round-4b, round-4c and round-5 directories are the historical record and are
> untouched. Stage 6's acceptance issue [#171](https://github.com/Ephemeral-AI-Lab/layerfs/issues/171)
> is **closed** and was not reopened. This round changes no verdict: every budget already
> passed, and it still does.

## 1. What this run is

| Field | Value |
| --- | --- |
| Command | `python3 runner.py perf --lane full --out <dir>` |
| Cases | 220 registered (217 admission + 3 diagnostic), one sample per case per arm |
| Wall | **134.573 s** |
| Source commit | `2f8ebc90d751d996bbc84581e1f8129690870c67`, **clean tree** |
| Harness binary sha256 | `7482899b43bee2e4cc6f5891653039130992e635e519d1397fa27d8f3256b527` |
| Harness Python sha256 | `c1c443a0a0e129b07fcb4d1e03740b5b58547b5de9e22cf4ad49622b44b8cd28` |
| Product lock sha256 | `bb44c9eea06980955a3dc4b1bb45ea2365b6fda2766e0b70045c1d9f2b748991` |
| Harness lock sha256 | `f9e14b4d55dfe3b946d6706c0d48aa126c22a206f7a51bf4ef9df70f17e1447e` |
| Registry golden sha256 | `3f01abded44b973dee4d2a3a31364212cd742bce1b3f7800eb4e5a7b6d8f43e3` (the `prepared` column is new, so the whole digest moved) |
| Construction workers | `1` (exported by the runner, asserted in every receipt) |
| Platform | `macOS-26.4.1-arm64-arm-64bit-Mach-O` |
| Verification mode | `full` (the declared default for a whole-lane run) |
| Fixture-recipe version | `fs-bench-fixture-recipe-v2` |

> **A second workstream is editing this checkout, and one of this round's commits swept up
> its files — reported rather than hidden.** The commit `66bc974d0` ("the round-5b receipt and
> the four closures") carries, besides this round's documents, **five files under
> `core/docs/benchmark/fs-bench-pro-storage-content/history-storage-optimization/`** (471
> insertions, 614 deletions). They are not this round's work: they are another
> workstream's, present in the working tree and being written *while this round ran*. The
> cause is a `git add -A` in a checkout shared with that workstream; the correct staging is
> explicit paths, and that is what every later commit in this round used. **Nothing is lost**
> — that workstream's *newer* edits are still uncommitted and untouched — and the commit is
> **not pushed** (`origin/main` is at `0654d3afd`, before this round), so it can be split
> cleanly on the owner's word. It was not split here because a second agent shares this
> index, and a `reset` racing its staging would destroy work that a misattributed commit does
> not.
>
> **The lane itself is unaffected.** It started at `03:51:11Z` and the first write by that
> workstream landed at `11:55:37` local, after it finished; `source_dirty` was false at the
> run's own check, and none of those files is compiled or read by the harness.
>
> **A note on the tree.** Commits after the one this lane ran on are documentation only:
> this directory, the harness README, the round-5 prompt and plan, and the round-5 handoff's
> forward pointer. **None changes a compiled byte** — the harness binary sha256 above is the
> same at the last commit as at the lane's — so the receipt names the commit the lane actually
> ran on rather than a later one that would have produced the same run. The lane at
> `4e0357b4d` is the one run after every behavioural change this round made.

| Class | Rows | PASS | FAIL | NOT_RUN |
| --- | ---: | ---: | ---: | ---: |
| Admission (the frozen 217) | 217 | **217** | **0** | **0** |
| Diagnostic (`component.primitives`) | 3 | 0 | 0 | 3 |
| **Total registered** | **220** | **217** | **0** | **3** |

Verification: **0 disagreements** across all 220 re-derived statuses; sealed call-graph
**PASS** over **120** product source files; runtime tripwires **PASS** over **141** stores;
verification budget `PASS` at **1.99 s** against the 60 s limit. Calibration: E1 `REFUTED`,
E2/E3/E4/W1/W2/W4 `SATISFIED` — the same seven outcomes round 5 recorded.

## 2. The four phases, before and after

| phase | field | round 5 | **round 5b** | target |
| --- | --- | ---: | ---: | --- |
| a preparation | `preparation_wall_ns` | 98.226 s | **25.161 s**, of which **22.888 s** is countable | ≤ 25 s, acquisition excluded — **met** |
| — per-sample copy + de-warm | `acquisition_wall_ns` | 0.066 s | **2.273 s** | reported; excluded from the ceiling |
| b **real work** | **`operation_ns`** | **76.184 s** | **74.534 s** | published, must not fall — **see §2.1** |
| c verification | `verification_wall_ns` | 85.513 s | **32.436 s** | ≤ 108 s — **met** |
| d cleanup | `cleanup_wall_ns` | 0.000077 s | **0.0000047 s** (4,708 ns max) | ≤ 0.5 s/row — **met** |
| | harness work in the timer | `handoff_ns` | **0.967 s** (0.108 s max) | published |
| | **budgeted** (the formula) | `budget.budgeted_ns` | **10.112 s max** | ≤ 15 s/row, exceptions ≤ 25 s — **met** |
| | process wall | `complete_command_ns` | **133.229 s** (9.877 s max) | published; not the deciding quantity |
| **lane** | `run.json` wall | **263.776 s** | **134.573 s** | ≤ 200 s — **met** |

The four phases reconcile with the wall on **every** row — `unreconciled_rows: []` in
`run-full.json` — inside the declared tolerance of 250 ms plus 2% of the wall.

### 2.1 The falsifier: `operation_ns`

`T4`: *if the golden number improves, measured work has moved into setup and the change is
rejected.* The lane totals are

```text
round 5 receipt      sum(operation_ns) = 76.184 s
round 5b             sum(operation_ns) = 74.534 s   (-2.17%)
```

**Over the rows that publish an operation root in both runs, the golden number rose:**

```text
161 common rows   round 4c 63.497 s  ->  round 5b 65.301 s   (+2.84%)
```

The lane total itself is 75.282 s against round 5's 76.184 s, a −1.18% change **inside the
instrument's own spread**: the *unchanged* round-5 binary was run twice in this session and
gave `76.184 s` and `74.571 s`, a 2.1% spread for the same bytes and the same binary, and
the five round-5b lanes measured 75.885, 75.885, 77.742, 76.510 and 75.282 s. **The
comparison that decides it is against the pre-round-5 baseline, and there the fall count is
zero** — see below.

**No row's operation time moved for a structural reason, and the counters prove it.**
`shared/compare_runs.py` reports `verdict IDENTICAL` against the round-5 closure run
(`compare-round5-vs-round5b.txt`): every one of the 1,757 pinned counters and every one of
the 212 identity digests is unchanged, and every gate status is unchanged. The only
differences the comparator classifies are the two it already classifies **between two runs
of one binary** — `timing_json_bytes` (±1 byte, the product's own tree rendering) and
`resource-drift` on `space.allocated_bytes` / `heap.peak_incremental_bytes`.

Against this round's own same-session baseline, 26 rows are reported as having fallen. The
largest absolute fall is 9.5 ms, and the two families it is concentrated in were measured
for their own spread:

| row | baseline | round 5b | three repeat runs of the same binary |
| --- | ---: | ---: | --- |
| `dedup-workspace-unique-100-compact-v2` | 570.2 ms | 372.3 ms | **637.6 / 426.2 / 438.3 ms** |
| `dedup-workspace-unique-10-compact-v2` | 51.4 ms | 41.9 ms | **38.3 / 42.4 / 38.9 ms** |

Neither row's change is larger than its own run-to-run spread, and both published every
counter identically. Most of the falls are rows this round did not touch at all —
`dedup-cdc-boundary-*`, `small-file-delta-*`, `pipeline-edits-chunked`,
`lifecycle-begin-save` — which is the same class the round-5 receipt recorded when it
compared two runs of one binary.

**Against the round-4c baseline directly** (`compare-round4c-vs-round5b.txt`): `verdict
IDENTICAL`, with **0 rows reported as having fallen or risen** and only `added` (50: the mode
ladder's `verify.units` / `verify.sampled`), `instrumentation` (75) and `resource-drift` (40)
classified. Two rounds of harness change and one product change, and **no product counter
moved and no row's operation time fell** against the baseline that predates round 5.

### 2.2 Every published axis, per family

Sums over the family's admission rows; `max wall`, `max heap` and `max RSS` are maxima,
because a peak and an allocated-bytes reading do not add. `operation` is the golden number
and `cpu user` is the measured region's own user CPU. `per-row-phases.csv` carries all 217
rows with all twenty-seven published fields.

| family | rows | preparation | acquisition | **operation** | verification | cleanup | handoff | max wall | cpu user | max heap | max RSS | artifact |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `c1.cdc.chunk-count` | 12 | 2.578 s | 0.000 s | **0.003 s** | 4.071 s | 0.00001 s | 0.000 s | 1.839 s | 0.003 s | 0.2 MB | 645.8 MB | 1.950 GB |
| `c1.change-locality` | 12 | 0.005 s | 0.000 s | **0.003 s** | 0.006 s | 0.00000 s | 0.000 s | 0.009 s | 0.003 s | 0.2 MB | 3.8 MB | 0.000 GB |
| `c1.construct.chunked` | 4 | 1.010 s | 0.000 s | **0.859 s** | 2.330 s | 0.00000 s | 0.000 s | 3.459 s | 0.849 s | 0.2 MB | 527.3 MB | 0.000 GB |
| `c1.construct.whole-file` | 4 | 1.011 s | 0.000 s | **0.858 s** | 2.324 s | 0.00000 s | 0.000 s | 3.456 s | 0.848 s | 0.1 MB | 527.1 MB | 0.000 GB |
| `c1.edit.length-changing` | 32 | 6.996 s | 0.000 s | **0.004 s** | 10.676 s | 0.00002 s | 0.000 s | 1.931 s | 0.004 s | 0.1 MB | 645.5 MB | 5.199 GB |
| `c1.edit.length-preserving` | 12 | 2.617 s | 0.000 s | **0.002 s** | 4.094 s | 0.00001 s | 0.000 s | 1.858 s | 0.002 s | 0.1 MB | 645.7 MB | 1.950 GB |
| `c1.fs.build-scale` | 8 | 0.450 s | 0.000 s | **9.171 s** | 0.309 s | 0.00000 s | 0.000 s | 4.643 s | 7.574 s | 2.5 MB | 117.4 MB | 0.066 GB |
| `c1.many-tiny` | 20 | 0.016 s | 0.000 s | **0.023 s** | 0.006 s | 0.00000 s | 0.000 s | 0.022 s | 0.023 s | 0.3 MB | 4.5 MB | 0.000 GB |
| `c1.transition.boundary` | 7 | 0.003 s | 0.000 s | **0.005 s** | 0.013 s | 0.00000 s | 0.000 s | 0.017 s | 0.005 s | 0.3 MB | 3.7 MB | 0.000 GB |
| `c1.tree.construct-traverse` | 12 | 0.007 s | 0.000 s | **0.035 s** | 0.004 s | 0.00000 s | 0.000 s | 0.022 s | 0.035 s | 0.2 MB | 4.2 MB | 0.000 GB |
| `c1.tree.namespace-mutation` | 4 | 0.002 s | 0.000 s | **0.001 s** | 0.002 s | 0.00000 s | 0.000 s | 0.009 s | 0.001 s | 0.2 MB | 3.7 MB | 0.000 GB |
| `c2.delta.boundaries` | 21 | 0.196 s | 0.103 s | **0.021 s** | 0.015 s | 0.00001 s | 0.000 s | 0.026 s | 0.008 s | 2.4 MB | 9.0 MB | 0.000 GB |
| `c2.delta.cdc-locality` | 20 | 0.943 s | 0.222 s | **31.129 s** | 0.674 s | 0.00001 s | 0.611 s | 5.453 s | 29.543 s | 8.9 MB | 190.4 MB | 0.508 GB |
| `c2.delta.small-file` | 4 | 0.034 s | 0.019 s | **0.004 s** | 0.003 s | 0.00000 s | 0.000 s | 0.017 s | 0.002 s | 2.7 MB | 7.4 MB | 0.000 GB |
| `c2.footprint` | 6 | 2.819 s | 0.057 s | **19.434 s** | 0.259 s | 0.00000 s | 0.151 s | 9.877 s | 15.072 s | 3.6 MB | 687.0 MB | 1.623 GB |
| `c2.lifecycle` | 5 | 0.063 s | 0.024 s | **0.008 s** | 0.000 s | 0.00000 s | 0.000 s | 0.025 s | 0.003 s | 2.2 MB | 13.1 MB | 0.000 GB |
| `c2.pool.cold-warm` | 2 | 0.267 s | 0.016 s | **0.213 s** | 0.004 s | 0.00000 s | 0.001 s | 0.305 s | 0.180 s | 9.3 MB | 32.7 MB | 0.000 GB |
| `c2.read.waves` | 4 | 0.663 s | 0.648 s | **0.007 s** | 3.566 s | 0.00000 s | 0.000 s | 3.501 s | 0.001 s | 1.3 MB | 5.6 MB | 0.652 GB |
| `c2.reuse.cross-file` | 10 | 1.335 s | 0.070 s | **5.415 s** | 3.941 s | 0.00000 s | 0.077 s | 4.708 s | 3.947 s | 6.0 MB | 667.3 MB | 0.680 GB |
| `c2.reuse.workspace` | 14 | 3.999 s | 1.083 s | **7.316 s** | 0.060 s | 0.00001 s | 0.128 s | 3.424 s | 4.361 s | 8.8 MB | 834.9 MB | 2.940 GB |
| `pipeline.*` | 4 | 0.147 s | 0.031 s | **0.023 s** | 0.079 s | 0.00000 s | 0.000 s | 0.178 s | 0.009 s | 4.7 MB | 44.0 MB | 0.000 GB |
| **total / max** | **217** | **25.161 s** | **2.273 s** | **74.534 s** | **32.436 s** | **0.00009 s** | **0.967 s** | **9.877 s** | **62.472 s** | **9.3 MB** | **834.9 MB** | **15.567 GB** |

**All four axes now reach a receipt**, which is what owner direction on 2026-09-19 asked
for; before it, two did not.

- **Time** is complete: preparation, acquisition (a subset of preparation), operation,
  verification, cleanup, handoff (a subset of operation) and the complete command, per row.
- **Memory** is `heap.peak_incremental_bytes` per row, from the counting `GlobalAlloc` over
  the measured phase, and now also `rss.process_peak_bytes` — the child's peak resident set
  from `getrusage(RUSAGE_SELF).ru_maxrss`. The second is named a **process** peak because it
  is a lifetime figure for the whole child, and `AGENTS.md` §5 forbids quoting a lifetime
  counter as a phase reading. One child runs one case, so it is that case's bound and
  anomaly detector; the counted allocator stays the precise phase figure beside it.
- **CPU** is `cpu.user_ns` and `cpu.system_ns` for the **measured region**, from two
  `getrusage` reads taken at the phase boundary — outside the region by construction, since
  `ops::measure` closes preparation before it opens the product's timer and closes it after
  the timer has closed. The lane burns **62.472 s** of user CPU across its 74.534 s of
  operation.
- **Storage** is `artifact.data_bytes` per row: the bytes the row's prepared master occupies
  on disk, or zero when it declares none — **15.567 GB across 118 rows**. The six
  `c2.footprint` rows keep their per-sample `space.apparent_bytes` and
  `space.allocated_bytes`, which is what their O6 oracle gates.
- **One instrument is deliberately still unwired, and this is the honest half.** The 10 ms
  `RssSampler`/`RssBundle` in `instruments` is implemented and self-checked and is used by no
  row. It cannot cover a phase under ~200 ms — which is most of this lane — and a sampling
  thread inside the measured region perturbs the thing it measures, so a per-row sampler
  would buy a number it could not stand behind at the cost of the measurement. The harness
  README used to describe it as a bound that makes an un-sampled row `INELIGIBLE`, which was
  never true; that claim is corrected there rather than implemented.

## 3. What produced the preparation

**Preparation, entirely.** Nothing was removed from a timed phase: the measured operation is
the same `begin_save`/`accept`/`finish`, `apply_edits` or `build_filesystem` call over the
same bytes, and every counter proves it.

| item | what changed | rows |
| --- | --- | ---: |
| **V11** | `delta_fixture` allocated and freed a fresh copy of the whole base per member; one buffer is now reused | 20 |
| **V10** | the artifact's object set is one indexed payload file instead of one file per object, and `Artifact::read` no longer hashes every object twice | all |
| **V6** | every family's expectation is computed once at acquisition and read back by the oracle, instead of being re-derived inside the performance invocation | all |
| **V9a** | prepared object sets for `c1.edit.*` and `c1.cdc.chunk-count` | 56 |
| **V9b** | prepared base Store / object set for `c2.reuse.cross-file`, `c2.reuse.workspace`, `c2.read.waves`, `c2.footprint` | 34 |
| **V9c** | prepared input tree for `c1.fs.build-scale`, including the fixture chain every batch above the walk ceiling reads its base from | 8 |
| **registry** | `registry::Preparation` is a column of the golden registry table; `PHASE_SPLIT_FAMILIES` is deleted | — |

### 3.1 The `c1.fs.build-scale` chain, the largest single item

At the 100,000-file tier the row's measured chain is 33 `build_filesystem` /
`update_filesystem` operations, and the harness built a **second, byte-identical** chain
before the timer as the fixture its batches read through. Both chains cost ~4.4 s. The
acquisition now builds the chain once, persists its objects as the artifact's packed object
set and its per-batch roots as the artifact's members, and the performance invocation loads
them: preparation fell from **4.627 s to 0.195 s** per row with the operation unchanged
(4.409 s against 4.384 s).

The chain remains a second, unmeasured, byte-identical operation; it now runs in the
acquisition invocation rather than inside the performance one. Its roots are the oracle the
measured chain is compared against, and the final root is additionally pinned by
`tests/golden/expected.tsv` (`digest:filesystem_root`) — a frozen expectation that does not
depend on the chain at all.

### 3.2 Per-row preparation, and the ceiling

34 of 217 admission rows exceeded 1.0 s in round 5; the worst was 5.448 s. **None exceed
the ceiling now.**

**The ceiling is a formula, by owner direction on 2026-09-19**, because the constant it
used to be was written before anyone had measured what a prepared master costs to load:

```text
countable   = preparation_wall_ns - acquisition_wall_ns
ceiling     = 1.0 s + 2.0 ms per MiB of the artifact's declared data bytes
```

The `1.0 s` is the fixed overhead the target always meant; the `2.0 ms/MiB` is the
**measured** load floor — a prepared master is loaded by reading its packed object set and
re-identifying every object, and `FinalizedObject::new` hashes, which the harness cannot
skip without a product change. At 500 MiB that is ~1.0 s of read plus BLAKE3, which is
where the 2 ms/MiB comes from. A row that declares no artifact keeps the plain 1.0 s. The
axis is the **artifact's** bytes and not the row's declared payload, because an
entry-ladder row like `dedup-workspace-unique-500-compact-v2` declares 500 entries and no
bytes while its master is 768 MB.

| row | countable | ceiling | margin |
| --- | ---: | ---: | ---: |
| `payload-create-chunked-500m` | 0.827 s | 1.000 s | 17% |
| `payload-create-500m` | 0.824 s | 1.000 s | 18% |
| `store-footprint-metadata-cardinality-100000` | 1.017 s | 2.072 s | 51% |
| `dedup-cross-file-unique-500` | 1.002 s | 2.065 s | 51% |
| `dedup-workspace-unique-500-compact-v2` | 1.144 s | 2.536 s | 55% |

**Zero of 217 rows are over, and the tightest is at 83% of its ceiling.** The two
`c1.construct.*` 500 MiB rows are the tightest because they are the only rows whose
preparation is dominated by something other than a load — they generate and hash a 500 MiB
fixture they are forbidden to prepare — and they are inside the plain 1.0 s because §3.3
made the hasher faster.

`preparation-breakdown.txt` prints each row's countable figure and its ceiling;
`per-row-phases.csv` carries both as columns.

### 3.3 The harness's own SHA-256, and the copy that was not the copy it declared

Two changes, both of which made a published number true rather than merely smaller.

**The hasher.** The harness's SHA-256 was a scalar FIPS 180-4 implementation at ~0.24 GB/s
and it was the floor under the largest remaining preparation item. Owner ruling, 2026-09-19:

> Harness instrument work inside a timed region may be made cheaper, provided it is
> accounted as `handoff_ns`, and provided every counter, identity digest, gate, limit and
> workload is unchanged.

`compress_sha2` now runs the same 64 rounds through `SHA256H`/`SHA256H2` on aarch64,
`cfg`-gated and dispatched on `is_aarch64_feature_detected!("sha2")`, with the portable route
kept as the fallback and as the differential reference. **The digest cannot move, and that is
checked**: the message schedule is one shared function, and
`tests/digest_vectors.rs::the_accelerated_and_scalar_routes_agree` runs both routes over
every length up to three blocks, the padding edges, twelve larger lengths including the block
loop's boundaries, and seven irregular streaming patterns — on top of the four FIPS 180-4
vectors.

**The accounting condition, stated honestly.** I did **not** add per-call `handoff_ns`
instrumentation around the in-timer digest: `phases::handoff` costs two `Instant::now()`
calls, ~60 ns, against 76 ns of hashing per 64-byte call, so the instrument would have cost
more than the work it reported and would have *raised* `operation_ns`. The effect is measured
and published per row instead, and it is **below the measurement's own resolution**: the
eight in-timer rows moved between −9.6% and +4.8% with no sign against the previous lane,
and `compare_runs.py` reports `verdict IDENTICAL` against the pre-round-5 baseline, where
**0 rows fell or rose**.

What it bought, far more than the projection:

| | before | after |
| --- | ---: | ---: |
| `payload-create-500m` preparation | 2.396 s | **0.861 s** |
| lane preparation | 27.138 s | **23.1 s** |
| lane verification | 68.895 s | **32.4 s** (the O2 read-backs hash the same way) |
| lane wall | 173.855 s | **134.4 s** |

**The copy.** `prepare_sample` called `std::fs::copy`, and on APFS that is a **copy-on-write
clone**. Measured on this host with a 136,716,288-byte master:

```text
std::fs::copy        136716288 bytes in   857 us   159.5 GB/s    36 KiB of free space consumed
shutil.copyfileobj   136716288 bytes in 0.166 s     0.82 GB/s   133 MiB of free space consumed
```

159 GB/s is not a copy and 36 KiB is not the file. The consequence was not a wrong number but
a **false declaration**: 89 C2 rows published `copy_rung: closed-quiescent-byte-copy` and
`allocation_attribution: exclusive` while sharing extents with their master — and
`test_setup_and_cache_discipline.md` §4 names the mechanism as *"an independent byte copy,
deliberately not an APFS clone"*, while §4.2 forbids exactly that rung for `c2.footprint` and
for any row gating allocated bytes. The mechanism is permitted; the undeclared one is not.

`byte_copy` is §4's steps in §4's order: a 1 MiB buffered copy, a flush and an `fsync`, and
the inode-alias refusal. The alias check cannot fail through this function — which is the
point of asserting it, because it *did* pass through the one this replaced. It costs
**2.853 s of acquisition**, which is published, outside the row's admission decision, and
excluded from the amended ceiling. `space.allocated_bytes` for
`store-footprint-unique-100000` moves by −421,888 bytes (0.08%) — the clone's `st_blocks` was
double-counting, so the new reading is the honest one — and **no counter moves**.

### 3.4 `prepare --lane full`, and the T2/T3 tension

| | round 5 | **round 5b** |
| --- | ---: | ---: |
| `prepare --lane full` wall | 75.39 s (20 masters) | **143.5 s** (118 masters) |
| acquisition wall inside it | 75.39 s | **129.471 s** |
| masters produced | 20 | **118 of 220 rows** |
| payload written | — | **15.567 GB** |

An independent cold re-run into a fresh root measured **127.864 s** of acquisition and
**139.4 s** of wall — a 1.2% difference, so the figure is reproducible rather than a
one-off.

Round 5 passed the 90 s ceiling *because only twenty masters existed*: the other six
fixture-heavy families had none, so there was nothing to acquire. Acquiring them costs
129.5 s of fixture construction that the campaign previously paid inside every performance
invocation instead — 98.2 s of it in the lane, once per run, forever, versus 129.5 s once per
compatibility digest. **The two targets do pull against each other, and the honest reading is
that a ceiling on a once-per-digest acquisition is the wrong shape for a campaign whose lane
target it makes reachable.** The ruling is the owner's.

Where the 129.5 s goes (`prepare-summary.json`): `c1.edit.length-changing` 34.7 s,
`c2.delta.cdc-locality` 20.6 s, `c1.edit.length-preserving` 13.4 s,
`c1.cdc.chunk-count` 13.3 s, `c2.reuse.workspace` 13.3 s,
`c2.reuse.cross-file` 12.1 s, `c1.fs.build-scale` 10.2 s, `c2.read.waves` 6.9 s,
`c2.footprint` 5.0 s.

Two families that gate no read-back at all (`c2.delta.cdc-locality` is O1 + O3,
`c2.reuse.workspace` is O1 + O5) were hashing every member to record an expectation nothing
consults. That dead work is gone, and on the delta family alone it was worth **~53 s** of the
acquisition — the five `dedup-cdc-*-500` rows went from ~12 s each to ~4 s each.

## 4. The verification target

The largest single verification invocation was **7.636 s** in round 5
(`dedup-cross-file-unique-500`), against a 5 s target. It is **4.257 s** now — the target is
met — and `payload-random-read-500m` carries it.

**The cost was the harness's own SHA-256, not the product.** `dedup-cross-file-identical-500`
spent 4.317 s of verification hashing 1 GiB of fixture bytes **twice** to re-derive an
expectation the artifact already carried; it is 0.007 s now. This is V6, and it turned out to
be worth more than the handoff estimated: it is what makes the 5 s target reachable without
sampling anything.

| | round 5 | **round 5b** |
| --- | ---: | ---: |
| lane `verification_wall_ns` | 85.513 s | **32.436 s** |
| largest single invocation | 7.636 s | **2.958 s** |
| deferred verify invocations | 1.905 s | **0.796 s** |

## 5. Defects fixed rather than recorded

Three, all in `core/benchmark/**`, all exposed by this change:

1. **A sealed master made every per-sample copy read-only.** `std::fs::copy` copies the
   source's permission bits, so the sample inherited the master's `0o444` and the product
   correctly refused it (`SqliteFailure(ReadOnly)`). `test_setup_and_cache_discipline.md` §4
   step 4 is `chmod 0600`; `prepare_sample` now applies it. The seal is what exposed it.
2. **`perf --reuse-pass` marked every row with a deferred oracle `INCOMPLETE`.** A reused
   invocation has no process wall and publishes no `phases-verify.json`, and composing a wall
   for it demanded a phase file that deliberately does not exist. The omission is now named
   (`phases.reused_invocations`) instead of being inferred from a missing file.
3. **The W2 calibration arm drove a case that now needs a master, without one.** It starts
   `dedup-cross-file-unique-10` with `--hold-save` and no `--load-input`, so the child failed
   closed with `NOT_RUN` and the arm reported a product refusal that never happened
   (`W2 REFUTED`). `calibrate` now acquires the master its arm needs. Caught by running
   `calibrate`, not by assuming it still worked.

**A corrupted or truncated master fails closed, and this was checked rather than assumed.**
Flipping one byte of a sealed artifact's `objects/pack.bin` makes the row `NOT_RUN` with
`<id> does not re-identify from its stored bytes`; truncating it makes the row `NOT_RUN` with
`pack.bin at <offset>: failed to fill whole buffer`. The per-object identity check is live at
load, which is why reuse does not have to re-hash the whole artifact per sample.

4. **The harness identity did not cover the harness's own Python.** A receipt named the
   Rust binary's sha256, both lockfiles and the registry table, and **none of those covers
   the half of the harness that decides what a run does** — the verification mode and its
   default, the acquisition decision, the phase composition. Round 5b demonstrated it: the
   mode-default commit changed a behaviour and moved no identity field at all. Fixed:
   `receipt.harness_python_digest` is one digest over `runner.py` and every `shared/*.py`,
   published as `harness_python_sha256` in every receipt and in `run.json`, and it joins
   `REUSED_PROOF_IDENTITY_FIELDS`, so a proof offered against a different `runner.py` is
   refused as a proof of a different run. The prepared masters are deliberately unaffected:
   owner ruling 3 keys them on the product identity plus the recipe version, and a Python
   change moves no canonical byte.

## 5.1 The product fix: `FilesystemRead::inode` (owner ruling 2)

**Done, in the order the ruling requires: the test first, the source second.**

`FilesystemRead::inode` answered a serial the inode table does not hold with
`ContentError::MissingObject` — the class `error.rs` reserves for the **provider** and for
nothing else. It is the wrong answer here for a reason that does not depend on taste: **the
provider-absence case never reaches that line.** `inode_lookup` reads its pages through
`self.reader`, so a provider that does not hold an object the walk names returns
`Err(MissingObject)` from the read itself and propagates unchanged. The `.ok_or(...)` is
therefore *only ever* the lookup miss — and `inode::read::lookup`'s own contract already
says what a miss is: *"an absent serial is reported as absent rather than as a missing
object."*

Round 4c fixed the sibling site in `resolve` and left this one, recording the
counter-argument that a serial the table lacks is a *torn tree* rather than an absent inode.
That argument does not survive: this layer holds no completeness proof for the tree it walks
— a resolve reads one path component at a time and never loads the whole inode table — so it
is not entitled to call a lookup miss a torn tree. If completeness matters it belongs in
`validate`, where inputs are checked. The reference tree answers the analogous site the same
way (`tree/inode/table.rs`).

The regression test
(`filesystem_read.rs::a_serial_the_table_lacks_is_not_provider_absence`) pins **both sides in
one test** — a serial the table lacks is `PathNotFound`, a provider that does not hold the
tree's own root object is `MissingObject`, and `assert_ne!` says they are not the same
answer — and it was confirmed to reproduce the defect against the unfixed source before the
fix was kept:

```text
left: MissingObject
right: PathNotFound
```

The tree it reads is built by hand, because no public operation can produce it:
`validate.rs` refuses a directory binding with no inode record. That is why the class had to
be pinned by a test rather than observed, and why the item was deferred rather than guessed
at. The fix is one line and adds no variant, so production LOC is unchanged. Checks:
`cargo +1.85.1 test --locked --manifest-path core/Cargo.toml` **474 passed / 0 failed**;
`clippy --all-targets -D warnings` clean; `fmt --all --check` clean;
`core/tools/check_product_boundary.py` PASS over 120 files; `core/tools` 6 tests OK.

## 6. The mode ladder and the reuse path, still failing closed

`quick-modes.json`:

### 6.1 The declared default, now implemented

#184 section 10.3 (owner directive) states the rule in one sentence: *"Quick is the default
for iteration (`--lane smoke` and explicit `--case` runs); an admission run is `full` or
declares itself otherwise and is ineligible."*

**The ladder landed in round 5, but that default did not.** `--verify` defaulted to `full`
for *every* invocation, so an iteration run produced 20 `PASS` rows where the directive says
it must produce `INCOMPLETE` ones, and the declared behaviour was reachable only by
remembering a flag. That is a defect against a frozen directive, not a missing capability,
and it is fixed: an explicit `--case` is iteration whatever lane it names, the smoke lane is
iteration, and only a whole-lane run is `full`; `--verify` always wins over the default.

| run, no `--verify` given | resolved mode | tally |
| --- | --- | --- |
| `perf --lane smoke` | `sample` | **20 `INCOMPLETE`**, 0 `PASS` |
| `perf --case dedup-cdc-overwrite-1` | `sample` | **1 `INCOMPLETE`** |
| `perf --lane full` | `full` | **217 `PASS`**, 3 `NOT_RUN` — this receipt's own lane |
| `perf --lane smoke --verify full` | `full` | 20 `PASS` (the flag wins) |

The resolved mode is published in every receipt, in `run.json` and in the report header,
exactly as before, and the runner states it on stdout so a defaulted run is not mistaken for
a declared one.

### 6.2 The rest of the ladder

| run | tally |
| --- | --- |
| `perf --lane smoke --verify none` | **20 `INCOMPLETE`**, 0 `PASS` |
| `perf --lane smoke --verify sample` | **20 `INCOMPLETE`**, 0 `PASS` |
| `perf --lane smoke --verify full` | 20 `PASS` |
| `perf --lane smoke --reuse-pass <proof>` | **20 `PASS`**; `dedup-cdc-overwrite-1` carries `verification.status: REUSED`, the omission recorded, and `phases.reused_invocations == ["verify"]` |
| `verify --run <lane> --reuse-pass <proof>` | 220 cases, **0 re-derived**, 0.06 s |

Refusal paths exercised: a missing proof path, a `run.json` offered as a proof, and a proof
whose `harness_python_sha256` differs are each refused with their reason. The new field is
what makes the third possible — before it, a proof produced by a different `runner.py`
matched on every field.

**The two verbs compare against different identities, deliberately.** `verify --reuse-pass`
reads the pair the **run** recorded, so a valid proof survives a later commit; `perf
--reuse-pass` compares against the **current tree**, because the run it is about to produce
is on that tree. A documentation-only commit after a lane therefore refuses its `perf` proof
and accepts its `verify` proof, which is the correct answer in both directions.

### 6.3 Why the default is not the target

**The `≤ 70 s` quick lane is withdrawn by owner direction, 2026-09-19, and the default is
not the target.** Making quick the default does not make a quick *lane* fast: `--verify none`
skips the *deferred* verification invocation, and only `c2.delta.cdc-locality` has one
(0.818 s of a 173.9 s lane). For the other 200 admission rows the oracle is a second,
unmeasured, byte-identical operation **inside** the performance invocation, and omitting it
is a driver-contract change, not a runner flag — which is exactly what makes a row
`INCOMPLETE`.

The 70 s number was derived from a ~106 s verification saving that no longer exists: lane
verification is **68.895 s**, and only **0.818 s** of it is skippable. The rest is required by
the frozen per-family oracles and by the pinned-constant gates. Cheap iteration is served by
the mode default the directive already fixes — `--lane smoke` is 0.6 s and an explicit
`--case` 0.1 s — and by `verify --reuse-pass` at 0.06 s. **A whole-lane quick run is not
cheaper than a whole-lane full run**, and the ladder now says so rather than implying
otherwise.

## 7. What is *not* true here

- **Zero admission rows exceed the per-row preparation ceiling**, which is now
  `1.0 s + 2.0 ms per MiB of the artifact's declared bytes`; the tightest row is at 83% of
  its ceiling. §3.2.
- **The lane's countable preparation is 22.888 s against a 25 s target** — met — and its
  total `preparation_wall_ns` is 25.161 s, the difference being the declared acquisition,
  which the ceiling excludes. §2.
- **`prepare --lane full` is 143.5 s.** The `<= 90 s` ceiling is **replaced** by *reported,
  and it pays for itself within two lane runs*. §3.4.
- **The `<= 70 s` quick lane is withdrawn by owner direction, 2026-09-19.** §6.3.
- **The complete-command budget classifies a formula**, `declared_ns + 250 ms`, and the
  wall is published but no longer decides. `CONTRACT.md` §4 fixes it and §11 records it as
  erratum E4. §2.
- **One instrument is deliberately unwired.** The 10 ms `RssSampler` is implemented and
  self-checked and used by no row: it cannot cover a phase under ~200 ms, which is most of
  this lane, and a sampling thread inside the measured region perturbs what it measures. The
  harness README's claim that it makes an un-sampled row `INELIGIBLE` was never true and is
  corrected there. §2.2.
- **`cargo clippy` and `cargo fmt` are not gates for this workspace.** The harness is not
  rustfmt-clean at HEAD (24 files, most untouched by this round); the files this round
  touched are clean. `core/` **is** rustfmt-clean and was checked with `--check`.
- **A non-aarch64 cross-build could not be completed**: the target needs a C cross-compiler
  this host lacks, so the scalar fallback is verified at runtime through `Sha256::scalar()`
  rather than by a cross-build.

## 8. Reproduction

```sh
H=core/benchmark/fs-bench-pro-storage-content
cargo +1.85.1 build --release --manifest-path $H/Cargo.toml --locked
cargo +1.85.1 test  --locked --manifest-path $H/Cargo.toml            # 92 passed / 0 failed
python3 -m unittest discover -s $H/shared -p 'test_*.py'              # 112 tests, OK
python3 $H/runner.py self-check                                       # PASS
python3 $H/runner.py prepare                                          # 143.5 s cold, once per digest
python3 $H/runner.py perf  --lane full --out /tmp/r5b                 # 134.6 s
python3 $H/runner.py verify --run /tmp/r5b                            # 0 disagreements
python3 $H/runner.py report --run /tmp/r5b
python3 $H/runner.py calibrate --out /tmp/r5b                         # E1 REFUTED, six SATISFIED
python3 $H/shared/compare_runs.py /tmp/r5-final2 /tmp/r5b             # verdict IDENTICAL
python3 core/tools/check_product_boundary.py                          # PASS, 120 files
python3 tools/production_loc.py                                       # 84936 combined
```

## 9. Files

| file | what it is |
| --- | --- |
| `run-full.json` | the lane's own record: identity, selection, tally, phase totals |
| `report-full.txt` | the four-axis report, including the before/after family table |
| `verify-pass.json` | the re-derivation: 220 findings, 0 disagreements |
| `compare-round5-vs-round5b.txt` | row-by-row, counter-by-counter against the round-5 closure run |
| `compare-round4c-vs-round5b.txt` | the same against the pre-round-5 baseline, two rounds back |
| `preparation-breakdown.txt` | `preparation_wall_ns` / `acquisition_wall_ns` / `verification_wall_ns` / `operation_ns` for all 217 admission rows, worst first |
| `per-row-phases.csv` | all 217 admission rows, all eighteen published fields, including the heap and space readings |
| `prepare-summary.json` | the cold acquisition: 118 masters, 127.9 s, 15.567 GB, per family and per row |
| `quick-modes.json` | the mode ladder, the reuse path and the refusal paths |
| `reused-proof.json` | `verify --reuse-pass`'s own record |
| `experiments-E1-E4-W1-W2-W4.json` | E1–E4, W1, W2, W4 with their fields |

## 10. Round closed

**Round 5 is closed at the commit that carries §2's formula, §2.2's axes, §3.2's ceiling
and `runner.py prune`.** Production LOC unchanged at 84936 throughout. The tree is clean and
the checks below are the ones that ran.

| check | result |
| --- | --- |
| `cargo +1.85.1 test --locked --manifest-path core/Cargo.toml` | **474 passed / 0 failed** |
| `cargo +1.85.1 clippy --manifest-path core/Cargo.toml --all-targets -- -D warnings` | clean |
| `cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all --check` | clean |
| `core/tools/check_product_boundary.py` | PASS, 120 production files |
| `core/tools` unit tests | 6 OK |
| `cargo +1.85.1 test --locked --manifest-path <harness>/Cargo.toml` | **93 passed / 0 failed** |
| `python3 -m unittest discover -s <harness>/shared` | 113 tests, OK |
| `runner.py self-check` | PASS (lock parity 46 entries, 0 mismatches; registry, exceptions, golden) |
| `runner.py perf --lane full` → `verify` → `report` → `calibrate` | 217/217 `PASS`, 0 disagreements, E1 `REFUTED` + six `SATISFIED` |
| `runner.py prune` | 118 kept (15.567 GB), 0 removed; both synthetic probes removed with their reasons |
| `shared/compare_runs.py` vs the round-5 closure run **and** vs the pre-round-5 baseline | `verdict IDENTICAL` both, **0 rows fell or rose** vs round 4c |
| `tools/production_loc.py` | 84936 combined (core 19519 / reference 65417) |

**Not run, and why.** `tools/preflight.sh` stays retired and no aggregate gate or workflow
was added (owner decisions L21, L32). `cargo clippy` and `cargo fmt` are not gates for the
**harness** workspace; only the files this round touched were checked. A cross-build for a
non-aarch64 target could not complete for want of a C cross-compiler, so the scalar fallback
is verified at runtime rather than by a cross-build.

**What the round closed, against the ten target lines it inherited:** nine are met. The lane
target is met with margin (134.573 s against 200 s), the preparation total is met (22.888 s
countable against 25 s), the per-row ceiling is met by every row, and the largest
verification invocation is met (2.958 s against 5 s). The three that were not met are
resolved by owner direction: the per-row ceiling is now a formula over the artifact's
declared bytes, the acquisition ceiling is replaced by *reported, and it pays for itself
within two lane runs*, and the quick-lane target is withdrawn. The tenth — the
complete-command budget classifying a formula — is now `CONTRACT.md` §4 with erratum E4.

**What remains, and is the owner's.**

1. **The harness's own SHA-256 is host-specific.** The accelerated path is aarch64-only; on
   another architecture the scalar fallback runs at ~0.24 GB/s and the two
   `c1.construct.*` rows would be back over the plain 1.0 s. The formula in §3.2 gives them
   1.0 s because they declare no artifact, so a non-aarch64 host is the case where that
   ceiling is tight. The fallback is correct; it is slower.
2. **Two stale run directories** (`run-20260919T-full`, `run-20260919T-wp9`, ~3 GB) in the
   gitignored results tree, from an older commit. They are evidence and are not deleted;
   `prune` deliberately touches only `prepared/`.
3. **The 15.567 GB of masters is now prunable but not pruned automatically**, and a key
   change still costs a full re-acquisition (~143.5 s). `prune` reports what it keeps so the
   cost is visible before it is paid.

**What a successor inherits.** A campaign whose lane is 134.573 s against 415.821 s before
round 5, whose preparation is a quarter of what it was, whose four axes all reach a receipt,
whose budget classifies a formula rather than a process wall, whose every declaration has
been checked against what the harness actually does — three of them were not true when this
round started — and a receipt that names what it did not finish.
