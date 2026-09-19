# Preparing the retained-history test

> **Status:** Proposal; target LayerFS v0.1.7; not a released contract.
> Governed by [`docs/general/benchmark_rules.md`](../../../../../docs/general/benchmark_rules.md) §6
> and §11.

**There is almost nothing to prepare, and that is deliberate.** This lane builds no fixture,
holds no prepared master, takes no copy and has nothing to reuse. The expensive work — saving
157 states — *is* the measurement, so moving any of it into preparation would remove the thing
being measured.

## 1. What preparation is, exactly

| step | work | failure |
| --- | --- | --- |
| authenticate the corpus | `sha256(checkpoint-manifest.json)` equals `03f21acf…`; `tip` equals `b0a7d2ce…`; the checkpoint list has 157 entries | `ManifestIdentity`, `SourceTip`, `CheckpointCount` |
| resolve the selection | the lane's indices are strictly ascending, start at 1, end at 157, and have the declared length (17 / 53 / 157) | `SelectionShape` |
| record per-state identity | each selected `manifest_sha256`, its oracle path, and the oracle's sha256 | `TreeManifest`, `OracleMissing` |
| preflight the output filesystem | free space against the declared Store ceiling × the declared factor | `OutputSpace` |
| open the output directory | a fresh path, never an existing one | `OutputExists` |

That is the whole of it. `preparation_wall_ns` for these rows is corpus authentication;
`acquisition_wall_ns` is **zero**, because nothing is copied. The refusal names are the
`HistoryError` variants of
[`implementation-plan.md`](implementation-plan.md#21-srcworkloadhistoryrs--new) §2.1.

## 2. The corpus

A pinned, read-only copy of the deepseek-harness history, at
`/Users/yifanxu/Ephemeral-AI-Lab/deepseek-history-data`, reached through an explicit
`--corpus` path that fails closed when absent or when any identity disagrees.

| path | contents |
| --- | --- |
| `checkpoint-manifest.json` | the 157 checkpoints: `index`, `sha`, `tree`, `manifest_sha256`, `logical_bytes`, `files` |
| `inputs/<sha>/manifest.tsv` | that state's full tree: hex path, mode, blob oid, size |
| `inputs/<sha>/previous.tsv` | the preceding state's full tree |
| `inputs/<sha>/blobs/` | **only the blobs this transition changed** |
| `inputs/<sha>.receipt.json` | `blob_digests`: oid → sha256 for those blobs |
| `oracles/<sha>.json` | the state's full-byte oracle: path → mode, size, sha256, plus directory entries |

**Accumulation is the part that must not be got wrong.** `blobs/` holds only the changed set,
but a state's tree references blobs introduced by **earlier** transitions. The reader keeps a
forward map across the whole selection and re-reads nothing. Every blob it serves is re-hashed
and compared with both its oid and its `blob_digests` entry; a blob that does not identify is
refused, never assumed, and a state whose tree needs an oid the map lacks is a failure rather
than a partial tree.

The corpus is **not** inside the repository and is never copied into it. Every path the reader
opens is read-only, and the corpus is never mutated — that is the one §6 rule that still binds
in full.

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

This is why the row's `operation_ns` is the **sum of the named per-state children** and not the
root: a root would silently include the harness's own corpus reading. The product's timing tree
already supports named children, so this is a declaration rather than new machinery, and it is
owner decision 2 in the [README](README.md#7-owner-decisions--ruled-2026-09-19).

Nothing is pre-loaded. A 157-state selection is 4.94 GB of cumulative logical bytes, and holding
it resident would both defeat the memory claim and warm the pages the saves read.

## 4. What the prepare phase publishes

The `Phase::Prepare` invocation writes `phases-prepare.json` and a trace with the pins the rest
of the run is checked against:

| counter | meaning |
| --- | --- |
| `history.states` | the selection's length |
| `history.logical_bytes` | cumulative logical bytes, from the manifest |
| `history.path_states` | cumulative path-states, from the manifest |
| `history.manifest_sha256` · `history.source_tip` | the corpus identity the run is pinned to |
| `history.oracle_sha256.{ordinal}` | one per state, so a changed oracle invalidates the run |

No Store is created and no output is written. `phases::preparation_is_the_invocation()` already
declares the whole invocation as preparation, so `preparation_wall_ns` is a measurement rather
than a remainder.

## 5. Disk

The Store is created inside the sample and grows in place. Nothing else is written.

| | expected | ceiling |
| --- | --- | --- |
| `history-stride10` | ~49 MB | declared |
| `history-stride3` | ~64 MB | declared |
| `history-stride1` | ~84 MB | declared |

Expected sizes are v0.1.6's recorded allocated bytes and are **reference points, not targets**.
The preflight requires free space for the declared ceiling × the declared factor before state 1,
and refuses rather than filling the filesystem: a chain that dies at state 120 of 157 is worse
than a chain that never started.

The ceiling is a **safety** bound, not a gate on the storage claim. The claim is measured and
reported separately in [`measurement.md`](measurement.md) §4–5, and a run that exceeds its
ceiling is reported with the measured size, never trimmed to fit.

## 6. How preparation is measured

Preparation is a phase like any other and is published like one.

| axis | instrument | status |
| --- | --- | --- |
| time | `phases-prepare.json`, with `preparation_ns` declared as the whole invocation and `acquisition_ns` zero; `shared/phases.py` reconciles it against the runner's process wall | exists |
| memory | the counting `GlobalAlloc` and the 10 ms RSS sampler | **the sampler is wired to no row today**; Phase 2a closes it |
| CPU | `cpu_now()` from `getrusage(RUSAGE_SELF)` | **instrumented but never published**; Phase 2a closes it |
| disk | the Store file's own readings, taken before state 1 and after the last state | Phase 2b adds the before/after shape |

Preparation's numbers are published and never charged to a row's performance budget. For this
lane they are expected to be small enough that the interesting measurement is the work phase —
but "expected" is not "measured", and the fields are published so the expectation can be
falsified.

## 7. Why there is nothing to reuse

Earlier drafts of this lane carried a prepared master per state, a checkpoint layer and a
per-sample copy. All three are gone, and the question "what does the second run reuse?" resolves
to: **nothing, and it should not.**

There is no fixture to rebuild, so a second run costs what the first cost. That is correct here
— the cost is the measurement, not preparation. The §6 rules that reuse exists to enforce (no
repeated fixture construction per sample, no post-operation state in a cache, no half-built
entry consumed) are satisfied trivially because there is no cache in this lane at all.

The rules that still bind, and are tested in §8: the corpus is read-only and never mutated, the
Store a row writes is a fresh file under a fresh `--out` path, and no expected-result data is
ever supplied to a mutation.

## 8. What corpus authentication must be tested for

| # | test | expected |
| --- | --- | --- |
| 1 | **invalidation** — a manifest with one byte changed | refused with `ManifestIdentity`, before any state runs |
| 2 | **identity** — a wrong `tip` | refused with `SourceTip` |
| 3 | **truncation** — a checkpoint list with 156 entries | refused with `CheckpointCount` |
| 4 | **selection shape** — a lane whose indices are not ascending, or do not start at 1 and end at 157 | refused with `SelectionShape` |
| 5 | **blob corruption** — a `blobs/` entry whose bytes do not match its oid or its `blob_digests` entry | refused with `BlobIdentity`, naming the oid |
| 6 | **missing predecessor** — a state whose tree needs an oid the forward map lacks | refused, never a partial tree |
| 7 | **oracle drift** — an `oracles/<sha>.json` whose sha256 differs from the recorded one | the run's identity records the difference and the row is `INCOMPLETE` |
| 8 | **absent corpus** — a `--corpus` path that does not exist | refused with `CorpusMissing`, never a default path |
| 9 | **read-only** — the corpus's mtimes and hashes are unchanged after a full run | equal before and after |

Tests 1–6 and 8 are unit-level and need no product run. Test 9 is checked once per lane and
recorded in the evidence directory.
