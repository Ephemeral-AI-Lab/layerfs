# Pre-registration — #219 round 20, step 2: the `c1.fs.build-scale` ladder, 100 → 100,000

Written **before** the first ladder run. Commissioned by
[`issue219-ns19-packlimit-and-scaling-handoff.md`](../../issue219-ns19-packlimit-and-scaling-handoff.md)
section 4 and ledger entry L79. This step is an **instrument**, not a treatment: it changes no product
line and claims no arm. Its question is whether the work the 10,000-entry rung measures is flat in the
entry count, which is the half of the pipeline row's 89.65 ms C1 build span that nobody has charted.

## What is measured, and on which binary

`c1.fs.build-scale` builds a filesystem tree and saves nothing: `ops/fs.rs:584` constructs the tree in
memory, emits `fs_build.*` counters and writes **no Store at all**. That matters here because this
ladder runs on the round-20 binary, which carries the `PACK_LIMIT` change of step 1 — and the change
cannot reach any of these rows, because there is no pack, no pack row and no schema in them. The
identity is therefore one binary for the whole round rather than a rebuild between steps.

Four rungs, four invocations, one sample each, fresh `--out` per invocation, `--verify full`:

| case | family | entries | declared bytes | directories | cache | prepared |
| --- | --- | ---: | ---: | ---: | --- | --- |
| `namespace-100-compact-v3` | `c1.fs.build-scale` | 100 | 5,242,880 | 1 | `warm-in-process-fixture` | `input-tree` |
| `namespace-1000-compact-v3` | `c1.fs.build-scale` | 1,000 | 20,971,520 | 10 | `warm-in-process-fixture` | `input-tree` |
| `namespace-10000` | `c1.fs.build-scale` | 10,000 | 314,572,800 | 100 | `warm-in-process-fixture` | `input-tree` |
| `namespace-100000` | `c1.fs.build-scale` | 100,000 | 524,288,000 | 1,000 | `warm-in-process-fixture` | `input-tree` |

## What is already measured, at exactly two rungs

`namespace-10000` has two `PASS`/`--verify full` receipts in this worktree, both at source commit
`dbab10ae2`, both one sample:

| receipt | `phases.operation_ns` | complete command | preparation | `fs_build.operations` | `fs_build.bindings_added` |
| --- | ---: | ---: | ---: | ---: | ---: |
| `ns17-fsbuild-10000-300mb-20260921T031259Z` | 320,582,291 | 358,835,750 | 21,434,459 | 3 | 10,100 |
| `ns17-namespace-10000-full-20260921T031259Z` | 313,133,084 | 348,854,375 | 19,392,208 | 3 | 10,100 |

Mean operation **316.86 ms** for 10,100 bindings = **31.4 us per binding**.

**There is no receipt for `namespace-100-compact-v3`, `namespace-1000-compact-v3` or
`namespace-100000` in this worktree**, at any commit. The 10k rung is the only one with a prior
number, and the two lower rungs and the 100k rung are first measurements.

## The registered prediction

**Structural, and therefore a check rather than a guess.** `PreparedTree::batches(WALK_CEILING)`
(`fs_fixture.rs:500-560`) gives the first batch a capacity of `4,096 - root.changes`, where
`root.changes` is the count of top-level directories, and every later batch 4,096, over a plan of
`entries + directories` bindings. So:

| rung | bindings | registered `fs_build.operations` | derivation |
| --- | ---: | ---: | --- |
| 100 | 101 | **1** | `4,096 - 1 >= 101` |
| 1,000 | 1,010 | **1** | `4,096 - 10 >= 1,010` |
| 10,000 | 10,100 | **3** | `3,996`, then `4,096`, then `2,008` |
| 100,000 | 101,000 | **25** | `3,096`, then 23 x 4,096 and `3,696` |

**The scaling claim, which is the one the commission asks for.** If the build's cost per binding is
flat from 10,000 to 100,000, the 100k rung costs `101,000 x 31.4 us` = **3.17 s**, and the registered
prediction is a point of **3.2 s** with a band of **2.4 s to 4.0 s** — that is, a per-binding cost
within **-24 % / +26 %** of the 10k rung. `namespace-100000` is **not** in `runner.py`'s
`DECLARED_EXCEPTIONS`, so its complete command must also fit the **15 s** limit; at the predicted 3.2 s
that is not in question, and a row that cannot fit is reported as `NOT_RUN` with its measured wall
rather than made to fit.

No point prediction is registered for the 100 and 1,000 rungs. Nothing in this worktree has ever
measured them, and inventing a number for a rung with no prior observation would be the modelling
this campaign exists to avoid. They are run, and reported, without one.

## Refutation

The scaling claim is **refuted** if `namespace-100000`'s `phases.operation_ns` exceeds **4.0 s** — a
per-binding cost more than 26 % above the 10k rung — or if `fs_build.operations` is not **25**, which
would mean the batching, and not the per-binding cost, is what changed. It is reported as **partial**
between the band's edge and the refutation line.

A row that does not `PASS`, a row whose complete command exceeds 15 s, and a rung whose receipt cannot
be produced at all are each reported as themselves, with the measured wall, and none is inferred from
the rungs around it.

## What is deliberately not done here

**The reference harness's 47 `namespace-100000` rows are not a comparison.** They span 279 ms to
105.9 s (~380x), carry `TARGET_MISS` in both arms, `cache_contract: null` and
`verification_status: NOT_RUN` throughout, and come from a different harness and different builds. That
is correction 2 at ten times the size, and the ladder is read against itself and against this
worktree's own two 10k receipts, never against them.

No claim is made about the pipeline row's C1 build span from this ladder unless the ladder's own
numbers support one; the two are different harnesses' rows even where they build the same shape.
