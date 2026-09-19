# Preparing the retained-history test

> **Status:** Proposal; target LayerFS v0.1.7; not a released contract.
> Governed by [`docs/general/benchmark_rules.md`](../../../../../docs/general/benchmark_rules.md) §6.

**There is almost nothing to prepare, and that is deliberate.** This lane builds no fixture,
holds no prepared master, takes no copy and has nothing to reuse. The expensive work — saving
157 states — *is* the measurement, so moving any of it into preparation would remove the
thing being measured.

## 1. What preparation is

| step | work |
| --- | --- |
| authenticate the corpus | manifest SHA, pinned tip, per-state `manifest_sha256` and oracle sha256, blob digests — all fail closed |
| resolve the selection | the lane's ordered state list, and its pinned path-state and logical-byte totals |
| preflight the output filesystem | free space against the declared Store ceiling (below) |
| open the output directory | fresh, never an existing path |

That is the whole of it. `preparation_wall_ns` for these rows is corpus authentication;
`acquisition_wall_ns` is **zero**, because nothing is copied.

## 2. The corpus

A pinned, read-only copy of the deepseek-harness history, at
`/Users/yifanxu/Ephemeral-AI-Lab/deepseek-history-data`, reached through an explicit
`--corpus` path that fails closed when absent or when any identity disagrees.

| path | contents |
| --- | --- |
| `checkpoint-manifest.json` | the 157 checkpoints, pinned by `03f21acf…` and tip `b0a7d2ce…` |
| `inputs/<sha>/manifest.tsv` | that state's full tree: hex path, mode, blob oid, size |
| `inputs/<sha>/previous.tsv` | the preceding state's full tree |
| `inputs/<sha>/blobs/` | **only the blobs this transition changed** |
| `inputs/<sha>.receipt.json` | `blob_digests`: oid → sha256 for those blobs |
| `oracles/<sha>.json` | the state's full-byte oracle: path → mode, size, sha256, plus directory entries |

`blobs/` holding only the changed set is the one thing a reader must not get wrong: a state's
tree references blobs introduced by **earlier** transitions. The corpus reader accumulates the
blob map forward across the selection and re-identifies every blob it serves against
`blob_digests`. A blob that cannot be identified is refused, never assumed.

The corpus is **not** inside the repository and is never copied into it.

## 3. Where the corpus reading happens — and why it is not timed

Each state's changed bytes are read from the corpus **between** the timed per-state children,
inside the work phase but outside every timer:

```text
read state k's changed blobs      untimed, harness
  ├─ TIMED  construct the changed content
  ├─ TIMED  build_filesystem against the previous root
  └─ TIMED  save
read state k+1's changed blobs    untimed, harness
  ...
```

This is why the row's `operation_ns` is the **sum of the named per-state children** and not
the root: a root would silently include the harness's own corpus reading. The product's timing
tree already supports named children, so this is a declaration rather than new machinery.

Nothing is pre-loaded. A 157-state selection is 4.94 GB of cumulative logical bytes, and
holding it resident would both defeat the memory claim and warm the pages the saves read.

## 4. Disk

The Store is created inside the sample and grows in place. Nothing else is written.

| | expected | ceiling |
| --- | --- | --- |
| `history-stride10` | ~49 MB | declared |
| `history-stride3` | ~64 MB | declared |
| `history-stride1` | ~84 MB | declared |

Expected sizes are v0.1.6's recorded allocated bytes and are **reference points, not
targets**. The preflight requires free space for the declared ceiling plus the declared
factor before state 1, and refuses rather than filling the filesystem: a chain that dies at
state 120 of 157 is worse than a chain that never started.

The ceiling is a *safety* bound, not a gate on the storage claim. The claim is measured and
reported separately in [`measurement.md`](measurement.md) §5.

## 5. Why there is nothing to reuse

Earlier drafts of this lane carried a prepared master per state, a checkpoint layer and a
per-sample copy. All three are gone, and the question "what does the second run reuse?"
resolves to: **nothing, and it should not.**

There is no fixture to rebuild, so a second run costs what the first cost. That is correct
here — the cost is the measurement, not preparation. The rules that reuse exists to enforce
(`benchmark_rules.md` §6: no repeated fixture construction per sample, no post-operation state
in a cache, no half-built entry consumed) are satisfied trivially because there is no cache
in this lane at all.

The one rule that still binds: **the corpus is read-only and is never mutated**, and the Store
a row writes is a fresh file under a fresh `--out` path.
