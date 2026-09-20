# The depth term was a pack-cache scope term: one treatment, measured

> Status: Research; **diagnostic evidence, not release admission**. Round of
> 2026-09-20 on top of the scaling handoff (`c4f757514`). Two arms, one sample per
> case per arm, identical instrumentation (the harness is unmodified), one product
> change. Admission `INELIGIBLE`, every budget class `NOT_RUN`, and one row's
> reconciliation is `INCOMPLETE` — stated below, not rounded into a pass.

## Result

| | baseline (`c4f757514`) | candidate (+1 scope change) | delta |
|---|---:|---:|---:|
| **stride10 operation** (sum of 17 children) | 19.500 s | **16.746 s** | **−2.754 s (−14.1 %)** |
| stride10 complete command (wall) | 38.871 s | 35.792 s | −3.079 s (see the `INCOMPLETE` note) |
| stride10 preparation (declared, untimed) | 19.252 s | 17.102 s | −2.150 s (cache state, not credited) |
| **stride3 operation** (sum of 53 children) | 48.813 s | **37.066 s** | **−11.747 s (−24.1 %)** |
| stride3 complete command (wall) | 74.980 s | 62.355 s | −12.625 s |
| stride3 preparation (declared, untimed) | 26.056 s | 25.172 s | −0.883 s (cache state, not credited) |

The pre-registered bar was **≥ 1 s on stride10 with unchanged stored bytes and a
stride3 non-regression**. Both hold, and stride3 improves rather than regressing, so
the treatment is **retained**. The saving is entirely in the read/update path:

| sub-phase (stride10 / stride3, summed over states) | baseline | candidate | delta |
|---|---:|---:|---:|
| `filesystem` (the read/update path) | 8,113 / 24,907 ms | 5,204 / 13,195 ms | **−2,909 / −11,712 ms** |
| `storage.accept_loop` (the save) | 8,996 / 18,451 ms | 9,131 / 18,425 ms | +135 / −26 ms (noise) |
| `content` | 1,612 / 3,050 ms | 1,614 / 3,045 ms | +2 / −5 ms |
| `storage.finish` | 396 / 1,611 ms | 391 / 1,601 ms | −5 / −10 ms |
| every other named span | — | — | flat |

Per state it grows with depth, as the depth term did: stride10 `filesystem` early
half 158.5 → 116.1 ms/state and late half 760.6 → 475.1 ms/state; stride3 163.5 →
101.3 and 765.1 → 391.2.

## 1. The mechanism, confirmed from counters that already existed

[`mechanism.txt`](mechanism.txt) (script: [`mechanism.py`](mechanism.py)) reads the
retained campaign's per-state traces and its saved Stores. No new sample, no
instrumentation, no product line changed at the time it was written.

**Source fact.** `encoding/delta/read.rs::Resolver::resolve_charged` built a
`PoolReader` once per resolved inode leaf, so its pack cache
(`DEPENDENCY_PACK_CACHE_BYTES`, 4 MiB) and its decoded value cache
(`POOLED_VALUE_CACHE_BYTES`, 512 KiB) were discarded with every leaf. The
decoded-group cache beside it is operation-scoped (`cas/read.rs::ReadSession::groups`),
and it is consulted **after** the pack body is acquired.

**Measured, from the retained artifacts:**

| quantity | stride10 | stride3 |
|---|---:|---:|
| pooled pack fetches (whole run) | 79,784 | 427,384 |
| packs in the run's own saved Store | 255 | 437 |
| **fetches that re-read a pack the same state had already read (floor)** | **≥ 79,529 (99.7 %)** | **≥ 426,947 (99.9 %)** |
| pack bytes copied | 7,985,771,449 (7.99 GB) | 25,105,269,671 (25.11 GB) |
| **copies of the whole pack space** | **176.3×** | **443.6×** |
| decoded-group cache hit rate (operation-scoped, measured) | 0.951 | 0.916 |
| chain edges per record call (state 4 → last) | 0.16 → 0.28 | 0.17 → 0.31 |
| bytes copied per leaf request (state 2 → last) | 20 KiB → 1,085 KiB | 20 KiB → 1,123 KiB |

The floor is arithmetic and does not depend on any cache model: a state can only
fetch packs that exist, so `fetches − packs` fetches re-read a pack already read.
The depth term is therefore **repeated acquisition of whole committed packs,
discarded at the per-leaf boundary** — not more algorithm per byte, and not a
deeper delta chain. What the counters could *not* say is how close together the
repeats are, i.e. how much of them a bounded window holds. That was the treatment's
question, and [`PREREGISTRATION.md`](PREREGISTRATION.md) fixed the answer's shape
before the first sample.

## 2. The treatment: one scope change, no bound moved

**The pooled metadata reader's lifetime becomes its calling operation's.** No other
cache changes: the ordinary-lane pack cache stays wave-scoped, the decoded
ordinary-group cache stays operation-scoped. Bounds are unchanged (4 MiB pack /
512 KiB value, both still released wholesale at the bound), so retained bytes do not
grow — one reader is live at a time either way. No instrumentation was added: the
candidate arm is the retained harness linked against the treated product crates.

* `BodyCaches` (`encoding/delta/read.rs`) bundles the two stored-body caches a
  reading caller owns and states their lifetimes and the invalidation contract.
* `ReadSession` owns one pooled reader for the whole read operation, exactly as it
  already owned the operation-scoped decoded-group cache. This is where the measured
  saving comes from.
* `MutationOwner::read_batch`, `resolve_location` and the selection input use the
  save's own `pool_reader` — the reader the pooled lane and the depth walk already
  shared and that `write_pack` already releases on every pack write. Measured effect
  on the save path: none (+135 ms stride10, −26 ms stride3, within noise).
* `Store::read_batch` keeps a reader for the one wave it is.
* `PoolReadCounters::since` keeps each chain's reported work its own now that a
  reader outlives one chain, so `filesystem.provider.pooled.*` keeps its per-state
  meaning.

**Invalidation contract (unchanged, and the reason the lifetime is sound).** A pack
body at or below the ceiling a read was authorized under is immutable: a save
creates its packs above the baseline publication watermark and publishes them only
by advancing that watermark, so a committed pack is never rewritten. Every pooled
consult checks the location's pack against the *current* ceiling before the cache
answers (`Resolver::decode_at`, `PoolReader::load_group`), and the ceiling is
re-read per wave rather than pooled. A **writing** owner keeps its own reader and
releases its pack cache on every pack write (`cas::placement::MutationOwner::write_pack`).
Decoded values are never released: an ordinal's value is written once and never moves.

## 3. What the counters did, which is the experiment's answer

[`counters.txt`](counters.txt) (script: [`analyze.py`](analyze.py)):

| counter (whole run) | stride10 baseline → candidate | stride3 baseline → candidate |
|---|---|---|
| `pack_fetches` | 79,784 → **250** (0.003×) | 427,384 → **2,104** (0.005×) |
| `pack_bytes` | 7,985,771,449 → **16,574,832** (0.002×) | 25,105,269,671 → **80,173,074** (0.003×) |
| `value_group_decodes` | 65,337 → 33,749 (0.517×) | 368,074 → 183,120 (0.498×) |
| `physical_record_calls` | 55,676 → 55,676 (**1.000×**) | 248,690 → 248,690 (**1.000×**) |
| `physical_group_decodes` | 2,723 → 2,723 (**1.000×**) | 20,959 → 20,959 (**1.000×**) |
| `physical_group_cache_hits` | 52,953 → 52,953 (**1.000×**) | 227,731 → 227,731 (**1.000×**) |
| `chain_edges` | 16,570 → 16,570 (**1.000×**) | 82,387 → 82,387 (**1.000×**) |
| `leaf_requests` | 11,268 → 11,268 (**1.000×**) | 41,958 → 41,958 (**1.000×**) |

This is the pre-registered signature exactly: the *acquisition* counters collapse
while every *structural* counter is bit-for-bit unchanged. The read path does the
same work and copies almost none of the same bytes twice. The 4 MiB bound does
bind — the candidate still fetches 26 (stride10) / 65 (stride3) packs in its last
state, against 255 / 437 packs in the Store — but the reuse distance fits inside it,
which is what the mechanism reading had left open and what this measurement settles.

The value cache is the one bound that did not absorb its whole stream:
`value_group_decodes` halves rather than collapsing, because 512 KiB holds only
~44 decoded groups while a late state needs thousands. Recorded as measured, not as
a proposal: widening it is retained-bytes growth and an owner decision.

## 4. Equivalence: the Stores are byte-identical, against recorded constants

[`equivalence.txt`](equivalence.txt):

* every `history.state.N.root` identity equal between the arms;
* `changed_bytes`, `changed_paths`, `objects`, `inserted` equal in every state;
* both arms' saved Stores hash to the **constants the retained campaign recorded**,
  not to a new sample: stride10 `4af37932aa3391b12269de8130b9c66dc504f64ca78fc3e585f7afddabed8487`,
  stride3 `f5c7ff5a6b4f0821aa9a21ac5250335c4c3fb889637a0c5345c5278caadc2a9e`;
* canonical/value-group inventory equal between the arms (stride10 52,032 objects /
  1,737 groups / 48,925 values; stride3 72,560 / 4,225 / 65,528).

The harness's own footprint axes move anyway, on identical bytes: stride10 allocated
50,171,904 → 50,249,728 and stride3 62,152,704 → 63,160,320 while *apparent* bytes
are equal. That reproduces the retained campaign's finding that allocation is a
property of extents rather than content, and it is why content claims quote apparent
bytes. Store bytes are otherwise unchanged, and the v0.1.6 comparison is unchanged
from L47 because the bytes are unchanged.

### The sampled read-back (identity-matched verification, one invocation per run)

Each of the four measured runs was verified separately, in the harness's own
verification phase, against **the identity its own trace recorded**: the invocation
reads the state roots and the Store path out of that run's measured trace, opens that
Store, and resolves a **declared sample** of 64 paths per state through the product's
read path, comparing presence, kind, size and digest with the corpus oracle. It is a
sample and says so — the row it produces is `INCOMPLETE` by construction, never `PASS`.

| run | verification wall | sampled | mismatches | missing | unexpected | files read |
|---|---:|---:|---:|---:|---:|---:|
| baseline stride10 | 4.681 s | 1,083 of 101,477 path-states | 0 | 0 | 0 | 893 (5,619,947 B) |
| candidate stride10 | **2.911 s** | 1,083 of 101,477 | 0 | 0 | 0 | 893 (5,619,947 B) |
| baseline stride3 | 16.293 s | 3,377 of 306,861 | 0 | 0 | 0 | 2,756 (18,936,332 B) |
| candidate stride3 | **9.712 s** | 3,377 of 306,861 | 0 | 0 | 0 | 2,756 (18,936,332 B) |

Every comparison counter is identical between the arms, and the two gates the phase
applies (`g6.verify-state-count`, `g6.verify-sample-declared`) pass on all four. The
read-back is also where the treatment shows up a second time, in a separate process
and against byte-identical Stores: 4.681 → 2.911 s and 16.293 → 9.712 s. That is
reported as a secondary observation — the verification phase measures nothing and its
row is `INCOMPLETE`.

## 5. The one row that is not clean: stride10's reconciliation is INCOMPLETE

`candidate-history-stride10` reconciles **INCOMPLETE**: 1,911,291,834 ns of the
collector's 35.792 s wall lies outside the child's own clock (tolerance 965,842,212 ns).
The child's *own* phases reconcile — its `unaccounted_ns` is 28.6 ms — and the gap is
the difference between the collector's process wall and the child's first-mark-to-snapshot
clock: 71.9 ms on the baseline stride10 row, 39.5 / 36.3 ms on both stride3 rows,
0.066 / 0.036 / 0.064 s on the retained campaign's three rows, and 1.883 s here.

It is reported as `INCOMPLETE` and **must not be credited**: it inflates the wall
comparison by ~1.8 s, and the retention decision does not use it. The decision rests
on the operation (−2.754 s, inside the child's clock, corroborated by the −2.909 s
`filesystem` sub-phase and by the counter collapse), all of which is measured inside
the declared phases. What the gap is was not identified: the process start-up probe
(both binaries, four runs each, 35.8–42.2 ms) rules out first-execution cost, and no
second sample of the row was taken to chase it — the protocol forbids re-running a
case for a number, and this round did not re-run one for an explanation either. It
is the only one of seven observations in this family above 0.1 s and the most likely
cause is machine state during the child's post-snapshot evidence writing, but that
is a hypothesis and is labelled as one.

## 6. Cache state, declared

Nothing was invalidated and nothing was pre-touched. The corpus residency probe
(the retained round's diagnostic) reports, per run, the pages already resident when
the chain first asked for each corpus file:

| run | corpus read (declared preparation) | resident before first read | device bytes across the chain |
|---|---:|---:|---:|
| baseline stride10 | 19.050 s | 0 of 62,319 (0.00 %) | 656,510,976 |
| candidate stride10 | 16.919 s | 5,141 of 62,319 (8.25 %) | 518,098,944 |
| baseline stride3 | 25.526 s | 4,298 of 107,780 (3.99 %) | 984,363,008 |
| candidate stride3 | 24.618 s | 7,022 of 107,780 (6.52 %) | 928,137,216 |

The order was fixed before the samples (baseline10, candidate10, baseline3,
candidate3), so the second run of each pair inherits the first's corpus residue and
its preparation is 0.9–2.2 s cheaper. **That difference is preparation, is declared,
and is not credited to the operation.** The operation's own inputs are the Store the
run writes and the harness's declared input assembly, neither of which is corpus-warm.
This is not a cold claim and must not be quoted as one: one sample per case per arm
establishes no distribution.

## 7. Checks

Product change, so the core set ran on the committed tree:

* `cargo +1.85.1 test --manifest-path core/Cargo.toml --locked`: **494 passed, 0
  failed** across 72 test binaries (`checks/core-test-output.txt`).
* `cargo +1.85.1 clippy --manifest-path core/Cargo.toml --locked --all-targets`:
  **clean, no warnings** (`checks/core-clippy.json`).
* `cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all --check`: **clean, exit 0**
  (`checks/core-fmt-output.txt`).
* `core/tools/check_product_boundary.py`: **PASS**, 122 production files, and its
  self-tests **OK, 6 ran** (`checks/product-boundary.txt`).

Harness (unchanged by this round, but the arm's binary links the product crates):

* `cargo +1.85.1 test --manifest-path core/benchmark/fs-bench-pro-storage-content/Cargo.toml
  --locked`: **117 passed, 0 failed** (`checks/harness-test.txt`).
* `cargo +1.85.1 build --release … --locked`: **PASS**; the inherited `unused_mut`
  warning at `src/ops/history.rs:1525` is recorded, not fixed
  (`checks/harness-build.txt`). Harness Clippy and format were **not** re-run: the
  harness source is untouched, and its inherited Clippy set (21 warnings, 1 denied
  `never_loop` at `src/ops/history.rs`) is recorded as inherited.

Every build, test and performance command ran under both global flocks
(`checks/*.json`); every performance run passed the quiet preflight first. **Nothing
was refused, deferred or repeated this round**: the four performance invocations and
the four verification invocations all exited 0 on their first attempt, and no
deferral record exists because no preflight was busy.

## 8. Identities and reproduction

Isolated worktree `/Users/yifanxu/Ephemeral-AI-Lab/layerfs-190-scope`, branch
`codex/190-pooled-scope`, base `c4f757514`. The baseline arm is the retained tree at
`c4f757514` in `/Users/yifanxu/Ephemeral-AI-Lab/layerfs-190-qual` (clean over
`core/crates` and `crates`), binary SHA256
`190424195506d4d54b24d253b483986d9aadf542fe125ae61225d833c2102ab1` — the binary the
retained campaign's own rows came from, reused rather than rebuilt, which is what
makes the two arms differ only by the product change. Candidate binary SHA256
`c7d48057089470e8d65b46d8074e7c849375157bdd1c2d2decedb296adfba9ec`, built from the
treatment commit with a clean source seal. The candidate binary was rebuilt after the
commits: `cargo build --release` was a **no-op** (0.04 s, nothing recompiled) and its
SHA256 was unchanged, so the measured executable is the committed source rather than
a working-tree build. Rust 1.85.1, `--locked`, Python 3.14.3.
Corpus manifest SHA256 `03f21acfb415907f521217e7a972ed512265c8d0c2da0f8034e2ff3014334271`,
tip `b0a7d2ce3b4c19d7452e364b2d7acbfa87e707ed`. All eight behavioural history
switches unset, `LAYERFS_HISTORY_PHASES=1`, `LAYERFS_CONSTRUCTION_WORKERS=1`.

```
python3 with_locks.py perf-baseline-stride10  python3 collect.py baseline  history-stride10
python3 with_locks.py perf-candidate-stride10 python3 collect.py candidate history-stride10
python3 with_locks.py perf-baseline-stride3   python3 collect.py baseline  history-stride3
python3 with_locks.py perf-candidate-stride3  python3 collect.py candidate history-stride3
python3 mechanism.py   # from the retained campaign's artifacts, before any product change
python3 analyze.py     # phases.compose / receipt.budget are the runner's own, imported
python3 subphases.py
python3 with_locks.py verify-baseline-stride10  python3 verify.py baseline  history-stride10
python3 with_locks.py verify-candidate-stride10 python3 verify.py candidate history-stride10
python3 with_locks.py verify-baseline-stride3   python3 verify.py baseline  history-stride3
python3 with_locks.py verify-candidate-stride3  python3 verify.py candidate history-stride3
```

`collect.py` refuses an existing output, does not pre-create the child's `--out`
directory, runs the quiet preflight, and applies the established #190 **diagnostic**
caps (120 s / 240 s) — not admission budgets, not promoted, not enlarged after a miss.

## 9. Limits, and what this does not establish

* One sample per case per arm; within-run and between-arm comparisons only. No n3,
  no best-of, no re-run for a better number, and no re-run to explain the gap in §5.
* Both rows are diagnostics: admission `INELIGIBLE`, O3 `INCOMPLETE`, complete
  command 35.792 s / 62.355 s against the 15 s ordinary and 25 s declared-exception
  classes, so both budget classes are `NOT_RUN`.
* Verification is a **declared sample** of 64 paths per state, not a full read-back,
  and the row it produces is `INCOMPLETE` by construction. A performance result is
  not release admission.
* The v0.1.6 time comparison is untouched; nothing here is subtracted from it.
* **stride1 was not sampled.** The scaling claim (more chunks for the same history)
  is therefore not asserted by this round; stride10 and stride3 carry the depth-term
  claim, and the treatment's value is expected to grow with history length rather
  than demonstrated to.
* The saving is attributed to the read path by the counters and the sub-phase split;
  the part of it that is CPU (decompression avoided) versus BLOB copying is not
  separated by any published counter.
* The stride10 `INCOMPLETE` in §5 bounds that row's wall comparison, not its
  operation.
* No pin was written, no budget class changed, no cap enlarged, no selection shrunk,
  no cold claim invented, and `#190` stays open.

---

## Correction (added by the L50 round, 2026-09-20): the reconciliation gap is explained

§5 above says the 1.883 s outside the child's clock on `candidate-history-stride10`
"was not identified" and offers machine state as a hypothesis. It is now measured,
and the probe that appeared to rule out first-execution cost was wrong because it ran
*after* the first execution: a freshly created executable's **first** run costs
0.7–2.0 s on this machine before `main` (1,959.5 / 669.4 ms first, 34.8–38.9 ms
afterwards), entirely outside `phases::begin`. That explains this round's candidate
gap and also L47's and this round's baseline gaps, which were ~0.07 s because those
binaries had already run. Nothing in the retention decision changes — it rested on
the operation, the `filesystem` sub-phase and the counter collapse, all inside the
child's clock, and this round reproduces the operation delta (−2.754 s → −2.730 s)
under a different harness source. See
[L50's report](../stage-6-history-190-save-20260920T070711Z/README.md) §5.
