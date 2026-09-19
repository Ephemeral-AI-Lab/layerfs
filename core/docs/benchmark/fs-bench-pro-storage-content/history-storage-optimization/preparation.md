# Preparing the retained-history test

> **Status:** Proposal; target LayerFS v0.1.7; not a released contract.
> Governed by [`docs/general/benchmark_rules.md`](../../../../../docs/general/benchmark_rules.md) §6
> (*Reusable immutable input preparation*) and by
> [`../test_setup_and_cache_discipline.md`](../test_setup_and_cache_discipline.md).

Preparation must be **fast and reusable**, and must never credit a measured phase.
The repository's dividing line is one sentence: *"reuse the bytes, never the
residency."* Everything below follows from it.

## 1. The corpus

A pinned, read-only copy of the deepseek-harness history. On this host it lives at
`/Users/yifanxu/Ephemeral-AI-Lab/deepseek-history-data`, and the harness reaches it
through an explicit `--corpus` path that **fails closed** when absent or when any
identity below disagrees:

| path | contents |
| --- | --- |
| `checkpoint-manifest.json` | the 157 checkpoints, pinned by `03f21acf…` and tip `b0a7d2ce…` |
| `source.git` | the git object store the corpus was derived from |
| `inputs/<sha>/manifest.tsv` | that state's full tree: hex path, mode, blob oid, size |
| `inputs/<sha>/previous.tsv` | the preceding state's full tree, for direct transitions |
| `inputs/<sha>/blobs/` | **only the blobs this transition changed** |
| `inputs/<sha>.receipt.json` | `blob_digests`: oid → sha256 for those blobs |
| `oracles/<sha>.json` | the state's full-byte oracle: path → mode, size, sha256, plus directory entries |

`blobs/` holding only the changed set is the one thing a driver must not get wrong: a
state's tree references blobs introduced by **earlier** transitions. The corpus reader
therefore accumulates the blob map forward across the chain, exactly as the v0.1.6
generator does, and re-identifies every blob it serves (oid → sha256 against
`blob_digests`). A blob that cannot be identified is refused, never assumed.

The corpus is **not** inside the repository. It is referenced by path, identity-checked
on every acquisition, and never copied into the repository tree.

## 2. What one row needs

A row measures *one state's save into the repository that already holds the previous
state*. So a row's prepared input is exactly two things:

| | contents | handed to the child as |
| --- | --- | --- |
| the base | the Store after state *k−1* | `--store` |
| the state | state *k*'s canonical object set, roles and expectations preserved | `--load-prepared` |

The measured operation is then: open the base copy, construct state *k*'s files and
directories, save, close. Nothing about the fixture is built inside the timer.

## 3. The chain

base<sub>k</sub> is produced by row *k*'s own save, so `prepare` walks the chain. Three
strategies were considered:

| strategy | disk, stride-1 | acquisition | row independence |
| --- | --- | --- | --- |
| one master per state (158 Stores) | ~20 GB | one save per row | full |
| **checkpoint every N, replay the rest in `prepare`** | ~2 GB at N=10 | ≤ N−1 untimed saves per row | full — every base is rebuilt and re-sealed |
| one rolling master | ~1 Store | one ordered pass | none; a failure breaks the chain and `--case` becomes unusable |

**Checkpoint + replay is the design.** The replay happens entirely inside the `prepare`
invocation, outside every timer, and the base it produces is sealed and hashed before it
is consumed. A replay that produced a different base is therefore **caught, not
trusted** — the same retirement the round-5 plan applies to *"a loaded fixture is not
byte-identical to a built one."* N is **measured on stride-10 first** and declared in
`chain.json`; it is not assumed here.

Preparation cost must not scale with row count. Without the chain, 227 rows would mean
227 × 157 saves. With it, preparation is O(states).

## 4. Artifact layout

```text
benchmark-results/fs-bench-pro-storage-content/prepared/<case_id>/
  base.sqlite        the Store after state k-1: a checkpoint, or a checkpoint + replay
  objects/           state k's canonical objects, persisted role code beside each
  objects.tsv        insertion order and role code
  members.tsv        ordered member list and persisted Expectation
  state.tsv          full157_index, source sha, tree id, manifest_sha256, oracle_sha256
  chain.json         lane, checkpoint index, replay span, corpus identity
  manifest.json      every file, hashed, written last
  sealed.tsv         harness_sha256 + corpus_manifest_sha256 + source_tip
```

The object set uses the harness's existing lossless artifact format
(`src/workload/artifact.rs`): bytes **plus** the role they were finalized under, because
offering bytes under a guessed role stores the wrong envelope. The older
`TreeStore::write_to_dir` round trip keeps only bytes and re-wraps everything as chunks,
which is why it is not used here.

The seal carries the **external** corpus identity, because this is the first row class
whose input the repository does not own.

### 4.1 What the second run does

Preparation is reused through the seal the harness already writes. `runner.py`
`acquire()` resolves one case at a time:

| state of `prepared/<case_id>/` | outcome |
| --- | --- |
| `sealed.tsv` present and its `harness_sha256` matches | `reused` — **no child invocation at all** |
| the seal names another harness binary | `stale`, refused |
| the directory exists without a seal | `not-produced`, refused: an interrupted acquisition is never consumed |
| absent | one child invocation, then the seal is written |

`prepare` writes an append-only `manifest-<stamp>.json` recording `acquired`, `reused` or
`failed` per case, so reuse is **visible in the evidence** rather than inferred from a
shorter wall. A second run of the same lane is therefore N `reused` rows and no work.

Three additions this campaign needs:

1. **The key must carry the corpus identity.** Today the seal records only
   `harness_sha256`. A history artifact must also be invalidated by
   `corpus_manifest_sha256`, the pinned tip, the per-state `manifest_sha256` and oracle
   sha256, and the checkpoint spacing N. Without them a changed corpus silently reuses a
   stale base — which is the failure the seal exists to prevent.
2. **Reuse is chain-level, not row-level.** base<sub>k</sub> is row *k+1*'s input, so
   `prepare` reuses the checkpoints it already holds and replays only the missing links.
   A second run of the same lane replays none.
3. **A selected run prepares only the prefix it needs.** `benchmark_rules.md` §6:
   *"Resolve the requested case, tier, seed and source arm before preparation. A selected
   run MUST NOT prepare all families or all four tiers."* For row *k* that is states
   1..*k−1* and nothing after *k*. The chain dependency is declared; it does not license
   preparing the whole lane.

`--rebuild-prepared` is the explicit escape hatch when a digest changes, and it is the
only route that discards a sealed entry.

## 5. The copy rung, and the two row classes

`shared/copyladder.py` fixes the rungs. For this campaign only two are legal:

| rung | cost | warms cache? | legal when |
| --- | --- | --- | --- |
| **R1** `apfs-clonefile-cow-v1` | single-digit ms, size-independent | no data read at all | only when the row does **not** gate allocated bytes |
| **R2** `closed-quiescent-byte-copy` | ~1.1 s at 626 MB | yes, then de-warmed | the default, and required wherever R1 is refused |

R1 is refused for an allocation-gated row because a COW clone's `st_blocks` double-counts
blocks shared with its master, which is why `copyladder.rung_for_setup` already raises
rather than warns. This campaign's headline **is** allocated bytes, so:

| row class | rung | `allocation_attribution` | gates |
| --- | --- | --- | --- |
| `history-*-alloc` | R2 + de-warm | `exclusive` | O6 allocated bytes + O1 + sampled O4 |
| `history-*-fast` | R1 + de-warm | `shared-with-master` | logical fields only + O1 + sampled O4 |

**The two classes are different arms and are never pooled.** R0 (mmap the master
read-only, no copy) is not available here: the save writes the Store.

## 6. De-warming

A copy is warm the moment it is written, and a clone **inherits the master's
residency** even though `clonefile` read nothing. Both classes are de-warmed before the
clock starts, with the sequence already implemented in `shared/residency.py` and
`src/support/instruments.rs`:

```text
open(O_RDONLY) -> mmap(PROT_READ, MAP_SHARED)
  -> mincore                      -> resident_first      # does not fault pages in
  -> msync(MS_INVALIDATE) only if resident_first > 0
  -> mincore                      -> resident_pages
  -> require resident_pages == 0 and pages_checked == expected_pages
```

**Never touch-every-page.** Reading the file to "flush" it is why v0.1.6 paid a measured
18.57 s of acquisition for a 100k-file fixture, and it warms the very pages it claims to
evict. `resident_first != 0` is not a failure; it is the evidence that the instrument is
live. `mincore` alone cannot separate a cache-served read from a device read, which is
why a de-warmed row also carries `disk_read_bytes`.

A row whose residency gate fails is `INELIGIBLE` — never quietly fast.

## 7. Measuring preparation itself

Preparation is work. It is published on all three axes, and today only two of them are.

| axis | instrument | status |
| --- | --- | --- |
| **time** | `phases-prepare.json`: the whole `prepare` invocation is declared preparation, with `acquisition_ns` separate; the runner records the process wall and `shared/phases.py` reconciles them | exists |
| **disk** | `st_blocks`, `data_bytes` per artifact, `preflight_enospc` | exists for one copy; **needs** a chain preflight against the projected total, and a published per-artifact and prepared-root total |
| **memory** | counting `GlobalAlloc` (`peak_incremental_bytes`), 10 ms RSS sampler, `lifetime_peak_rss_bytes` | **missing for `prepare`** |

**The memory gap.** The instruments are started by the driver around the *measured*
phase. The `prepare` invocation runs a driver too, but `delta_prepare`
(`src/ops/c2.rs:818`) publishes only `prepare.objects`, `prepare.members` and
`prepare.base_bytes`. The fix is harness-only: the `prepare` invocation opens a heap
window and starts the RSS sampler for its whole body and publishes
`preparation_peak_heap_bytes`, `preparation_peak_rss_bytes` and
`preparation_allocations` in `phases-prepare.json`; `shared/phases.py` carries them into
the receipt; the runner gates per row and per lane.

**The chain ENOSPC preflight.** The ladder already refuses a single copy that would not
fit. A chain that dies at state 120 of 157 is worse than a chain that never started, so
`prepare` requires free space for the **projected chain total** plus the declared
factor, before state 1.

**Per-case reuse is published too.** `prepare` reports `acquired`, `reused` or `failed`
per case with its own wall, so a fast run that reused every artifact is distinguishable
from a run that did nothing.

Preparation is never inside a timer, and its cost is never charged to a row's
performance budget. It is published so the campaign's real cost is visible.

## 8. Preparation targets

Restated for this campaign against a **named lane composition**, because a 227-row lane
is not the 217-row lane and round-5 targets T2/T3 do not transfer.

| # | target | note |
| --- | --- | --- |
| P1 | `preparation_wall_ns <= 1.0 s` per row | tight for the largest states; measured on stride-10 before stride-1 is attempted |
| P2 | lane `sum(preparation_wall_ns)` inside a declared budget | declared per lane after stride-10 is measured |
| P3 | `prepare --lane history-stride<N>` inside a declared budget, once per compatibility digest | T3's 90 s does not hold for 157 states |
| P4 | `preparation_peak_heap_bytes <= B` per row | B declared after stride-10; the chain must not be built in memory |
| P5 | prepared-root `data_bytes` inside the declared disk budget | the stride-10 extrapolation decides whether stride-1 needs a larger N |

No target is a measurement until a lane reports one.

## 9. What the preparation must be tested for

`benchmark_rules.md` §6 names five obligations, and each gets a test:

1. **invalidation** — a changed corpus identity, harness binary or registry refuses the
   entry rather than consuming it;
2. **corruption** — a manifest whose per-file hashes disagree is refused;
3. **concurrent and interrupted publication** — the manifest is written last, publication
   is an atomic rename, and an unsealed directory is never consumed;
4. **cross-family reuse** — a history artifact is never offered to a non-history row;
5. **sample-to-master isolation** — the master is never opened writable, is never
   released to a sample process (`master_path_released_to_sample: false`), and is
   re-verified by stat identity afterwards.

The master is validated **once per acquisition**, not per sample. Repeated source
rehashing per sample is forbidden by `benchmark_rules.md` §6, and the harness's own
recorded cost of the path that does it is roughly four full content passes — about
2–3 s of setup for a 626 MB sample.
