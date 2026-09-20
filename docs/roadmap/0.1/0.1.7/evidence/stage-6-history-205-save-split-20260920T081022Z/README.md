# The save path's cost, attributed in time — and the one treatment the split named

> Status: Research; **diagnostic evidence, not release admission**. Round of
> 2026-09-20 continuing [#205](https://github.com/Ephemeral-AI-Lab/layerfs/issues/205),
> which was split out of [#190](https://github.com/Ephemeral-AI-Lab/layerfs/issues/190).
> Admission `INELIGIBLE`, every budget class `NOT_RUN`. Nothing here closes #190 or
> #205.

## What this round answers

`storage.accept_loop` was one span around `for id { operation.accept(object) }` and
50–55 % of a `history.*` row's operation, with **no time attribution at all**: no
span or counter existed for selection, base resolution, delta encoding, compression,
group framing, pack placement, row inserts or commits. The published
elapsed-versus-count correlations (L50/L51, corrected by L52) are correlations over
**counts**; a flat count cannot exonerate work that is not measured.

This round builds the instrument, publishes the split in seconds, and then applies
the owner's fixed order: **exactly one treatment, chosen from what the split names,
pre-registered and matched before it is written**. The split named B1 (do not
re-verify what this save already verified). B1 was **measured to have a population of
exactly zero** and is therefore not pre-registered, not implemented and not claimed.
That negative result is this round's treatment decision, and it is stated with the
count that produced it in §5.

## 1. The instrument

`SaveProfile` (`core/crates/layerfs-storage/src/cas/owner.rs`) adds seven disjoint
nanosecond buckets, each charged at the call site that does that kind of work:

| bucket | charged at |
| --- | --- |
| `resolve` | `delta/select.rs::eligible` (the `depth_of` edge walk), `::acquire` (`resolve_dependency`), `::select` (the post-trial cost walk), `membership.rs::stored_canonical` (reconstruction + identity re-hash) and `::reuse_or_collide` (the byte comparison), `pool_lane.rs` (index sync, value lookup, base acquisition) |
| `full_ns` | `select.rs::select` around `encode_full`; `pool_lane.rs` around `encode_full` |
| `delta_ns` | `select.rs::select` around `encode_prefix`; `pool_lane.rs` around `build` |
| `group_ns` | `placement.rs::seal_group` around `build_group` |
| `place_ns` | `seal_group` around `LanePlacement::select_many` |
| `sql_ns` | `seal_group` around `write::insert_objects`; `write_pack` around `insert_pack`/`append_pack`; `pool_lane.rs` around `insert_group` |
| `commit_ns` | `lifecycle.rs::maybe_commit` and `finish_inner` around `write::commit` |

`resolve` is itself split five ways (`ResolveProfile`: `eligible`, `acquire`, `cost`,
`reuse`, `pooled`) because its parts are different kinds of work with different
treatments. The five parts are asserted to sum to `resolve` exactly on every row
(`split2.py`), so the seven-bucket split is **refined, not re-scaled**, by the
refinement.

**Why an aggregate and not a span per object.** A stride1 row accepts ~10^5 objects
and the driver refuses a timer node per object above 53 states. The buckets are
`u64` fields accumulated into `OutcomeCounters` and published once per state. The
clock reads are ~14 `Instant::now()` pairs per object; the harness bounds that
against the retained uninstrumented samples in §6 rather than asserting it.

### 1.1 One instrumented arm, declared

The instrument is measured on the **instrumented** candidate arm only, as the owner's
brief directs; the retained uninstrumented samples are the control. Two instrumented
builds exist, and the difference between them is the only within-build overhead
evidence there is:

| arm | binary sha256 | case | operation | source |
| --- | --- | --- | --- | --- |
| `instrument` | `61c266f7eb9e…` | stride10 | 16.248 s | run `instrument-history-stride10` |
| `instrument2` | `68f0cc5e687c…` | stride10 | 16.296 s | run `instrument2-history-stride10` |
| `debug2` | — (debug scaffolding build) | stride10 | 16.080 s | run `debug2-history-stride10` |
| `instrument` | `61c266f7eb9e…` | stride1 | 106.700 s | run `instrument-history-stride1` |
| `instrument2` | `68f0cc5e687c…` | stride1 | 101.994 s | run `instrument2-history-stride1` |
| `probe` | `6bda177d4a89…` | stride1 | 103.303 s | run `probe-history-stride1` |
| retained uninstrumented | `1822ec21a9d2…` | stride10 / stride1 | **16.908 / 105.726 s** | L50/L51 |

`instrument2` charges the same number of clock pairs as `instrument` — the sub-split
only renames which bucket a charge lands in — and differs on stride10 by **+0.048 s**
(16.296 against 16.248), which is the cleanest same-machine, same-harness bound in
this round. The probe arm, which additionally inserts one identity per reuse
occurrence into a `BTreeSet`, differs from `instrument2` by **+1.309 s** on stride1;
a micro-benchmark of that exact operation (121,301 insertions of a 32-byte key) costs
**13.1 ms**, so the 1.309 s is not the probe's own work and is reported as
between-run variance, not attributed to the probe.

Set against the retained samples the instrument reads **−0.612 s / −3.732 s**: the
**sign is negative**, so this is not an overhead bound and is not presented as one.
Cross-arm comparison on this machine has a reproducibility band of order 1–4 s on
stride1, which is the honest limit of what these samples can resolve.

## 2. The published split

All three tables below are computed by `split.py` (seven buckets) and `split2.py` (the
five resolve parts) from the product's own counters in `trace.jsonl`; nothing is
inferred from counts, and the remainder is stated as a remainder.

### 2.0 all three rows, and how the save scales

| case | states | objects | operation | scope | resolve | resolve/scope | operation per object |
| --- | --- | --- | --- | --- | --- | --- | --- |
| stride10 | 17 | 52,032 | 16.296 s | 9.312 s | 4.247 s | 45.61 % | 313.2 µs |
| stride3 | 53 | 72,560 | 34.594 s | 18.777 s | 11.584 s | 61.70 % | 476.8 µs |
| stride1 | 157 | 97,788 | 101.994 s | 47.821 s | 36.458 s | 76.24 % | 1043.0 µs |

All three Stores hash to their recorded constants (`4af37932a…`, `f5c7ff5a6…`,
`1635cf7bb…`). Over 17 → 157 versions the operation grows **6.26×** while objects
written grow **1.88×**, so the operation per object grows **3.33×** — and the save
scope grows 5.14× while objects grow 1.88×, i.e. **2.73× per object**. That is the
scaling curve as the save path sees it, and it is unfavourable.

The resolution parts scale in two distinct ways, and the three-row view is what
separates them:

| part | 17 v | 53 v | 157 v | 157/17 | what grows |
| --- | --- | --- | --- | --- | --- |
| eligibility walk | 0.888 s | 2.252 s | 5.186 s | **5.84×** | **cost per walk**: 22.8 → 40.1 → 69.8 µs, while walks stay flat at 0.75 per object |
| pooled lane | 0.732 s | 2.845 s | 8.086 s | **11.05×** | both volume and cost |
| base acquisition | 2.527 s | 4.317 s | 6.633 s | **2.62×** | volume; per *trial* it grows 2.62× against trials' 1.87× |
| exact-reuse verification | 0.096 s | 2.165 s | 16.544 s | **172×** | **volume**: 0.022 → 0.298 → 1.240 events per object, a 56× rise per object, while the cost per event moves only 85 → 100 → 136 µs |

So there are two different problems, not one. Reuse verification is a **volume**
problem — it rises 56× per object as the Store accumulates, and its cost per event
barely moves — which is why the fit at stride1 makes it look like the whole curve
even though it is 1.03 % of the stride10 scope. The eligibility walk is the opposite:
a **cost-per-walk** problem, whose events are already flat per object and whose price
per walk **triples** as versions accumulate. Base acquisition and the pooled lane sit
between the two. A treatment that fixes one of these will not fix the other, and the
stride10 row alone would have hidden the volume term entirely.

### 2.1 stride10 — 17 states, 52,032 objects, operation 16.296 s

Scope (`save_ns − storage.begin`) = **9.312 s**; `storage.accept_loop` node 8.820 s
(94.71 % of scope).

| bucket | seconds | share of scope | ns/object |
| --- | --- | --- | --- |
| resolution + exact-reuse verification | 4.247 | 45.61 % | 81,631 |
| FULL representation encode | 1.599 | 17.17 % | 30,725 |
| delta representation encode | 0.637 | 6.84 % | 12,243 |
| group codec | 0.044 | 0.47 % | 849 |
| pack placement | 0.262 | 2.82 % | 5,044 |
| SQL statements | 1.068 | 11.47 % | 20,530 |
| transaction cadence | 0.421 | 4.52 % | 8,087 |
| **remainder (uncharged)** | **1.034** | **11.10 %** | **19,865** |
| charged total | 8.279 | 88.90 % | — |

### 2.2 stride1 — 157 states, 97,788 objects, operation 101.994 s

`storage.accept_loop` is **not recorded** on this row (the driver's ordinary recording
above 53 states), so the scope overstates it by that node and `harness.index`;
`storage.finish` is recorded and is 4.784 s (10.00 % of scope). Scope = **47.821 s**.

| bucket | seconds | share of scope | ns/object |
| --- | --- | --- | --- |
| resolution + exact-reuse verification | 36.458 | 76.24 % | 372,832 |
| FULL representation encode | 3.348 | 7.00 % | 34,235 |
| delta representation encode | 1.403 | 2.93 % | 14,346 |
| group codec | 0.151 | 0.32 % | 1,549 |
| pack placement | 0.467 | 0.98 % | 4,778 |
| SQL statements | 2.104 | 4.40 % | 21,512 |
| transaction cadence | 0.882 | 1.84 % | 9,016 |
| **remainder (uncharged)** | **3.008** | **6.29 %** | **30,755** |
| charged total | 44.813 | 93.71 % | — |

### 2.3 resolution, split five ways

| part | stride10 s | stride3 s | stride1 s | of stride1 resolve | stride1 ns/obj | early→late growth (stride1) |
| --- | --- | --- | --- | --- | --- | --- |
| eligibility walk (`depth_of`) | 0.888 | 2.252 | 5.186 | 14.23 % | 53,038 | 19,609 → 68,257 (**3.48×**) |
| base acquisition (`resolve_dependency`) | 2.527 | 4.317 | 6.633 | 18.19 % | 67,834 | 54,647 → 74,375 (1.36×) |
| post-trial cost walk | 0.004 | 0.005 | 0.008 | 0.02 % | 83 | 66 → 90 (1.38×) |
| exact-reuse verification | 0.096 | 2.165 | **16.544** | **45.38 %** | 169,184 | 85,249 → 169,408 (**1.99×**) |
| pooled lane lookup + base | 0.732 | 2.845 | 8.086 | 22.18 % | 82,693 | 37,793 → 95,434 (**2.53×**) |
| **resolution total** | **4.247** | **11.584** | **36.458** | 100 % | 372,832 | — |

The five parts sum to the row's own `resolve_ns` exactly on both cases
(`parts==row: YES`, checked per case by `split2.py`).

### 2.4 What the split names

1. **Reuse verification is the largest single part at stride1** — 16.544 s, 34.60 % of
   scope and 45.38 % of resolution, and it is a **volume** term: 1.03 % of the
   stride10 scope becomes 45.38 % of stride1's, because events per object rise 56×
   (0.022 → 1.240) while the cost per event moves only 85 → 136 µs. Per object it
   grows 85,249 → 169,408 ns (1.99×), **37.9 %** of the scope's own per-object growth,
   and in the three-thirds comparison it is the largest absolute part (8.504 s late,
   against 4.790 s for the pooled lane). This is the bucket B1 was designed to attack,
   and §5 measures why attacking it that way saves nothing.
2. **The eligibility walk is a cost-per-walk term and grows fastest of all** — 5.84×
   across the three rows while its walks stay flat at ~0.75 per object, because each
   walk costs 22.8 → 69.8 µs as chains lengthen. It is only 14.23 % of stride1's
   resolution, so a stride1-only view understates it and a stride10-only view
   understates reuse; the three rows are what separate the two mechanisms.
3. **The pooled lane is the largest part at the coarse end** (0.732 s, 59.5 % of
   stride10's resolution) and the fastest-growing in ratio (11.05×), and it is the
   only bucket whose per-object growth outruns the scope's (2.53× against 1.73×),
   contributing **25.9 %** of the scope's per-object growth.
4. **The cost walk is nil** (8 ms at stride1). The depth-cap binding L51 reported is
   real for the *policy* but is not paid for in time at this site.
5. **The group codec is 0.151 s at stride1** (0.32 %). L40's +1.685 s is the *delta*
   of level 19 against level 1, and this figure is the level-1 absolute the brief said
   had never been isolated: codec level is not a lever worth one second.
6. **The eligibility walk's own cost is paid twice for the winning candidate** — once
   to measure depth, then again by `acquire` to rebuild the same chain. That is the
   brief's candidate C. It is **not** treated this round; it is named here because the
   split measured it, and the counting was asked for before any implementation.

## 3. The instrument leaves the store byte-identical

Both instrumented arms reproduce the recorded constants exactly:

| case | recorded store sha256 | measured | state roots |
| --- | --- | --- | --- |
| stride10 | `4af37932aa3391b1…` | equal **YES** | 17 states, first `ff484f8d…`, last `215e1f8a…` |
| stride1 | `1635cf7bbbabdc7f…` | equal **YES** | 157 states, first `6c4dcd62…`, last `fa7d77ba…` |

The probe arm reproduces stride1's constant as well. Bytes, roots, row counts and the
canonical inventory are unchanged by the instrument: it observes, it does not decide.

## 4. Protocol as run

* Serialized under the two global flocks via `with_locks.py`; every resource command
  records a `checks/<label>.json` receipt. Three lock deferrals are retained on disk
  (`lock-deferred-*.json`), all of them `cargo check` or a sample attempt that was
  never taken.
* Quiet preflight per run: no named `cargo`/`rustc`/`fs-bench` competitor and ≥70 %
  CPU idle on the second of two one-second observations; the observations are stored
  in each `perf-declaration.json`.
* One sample per case per arm. Fresh `--output` per run; `collect.py` refuses an
  existing run directory, so nothing was overwritten. `--pre-execute` ran each freshly
  built binary once outside the sample and declared its wall time.
* Diagnostic caps unchanged and never approached: 120 s stride10 (max wall 35.3 s),
  240 s stride3 (wall 60.9 s), 720 s stride1 (max wall 155.2 s). No cap was promoted,
  enlarged or shrunk. All three rows were sampled from one build (`68f0cc5e687c…`),
  and the stride3 row was taken after §2 was first written, because the 17 → 157 pair
  alone could not tell a volume term from a cost-per-unit term and the middle row is
  what separates them (§2.0). It was not taken for a better number, and it did not
  flatter the story: it is the row that shows the save's scope growing 2.73× per
  object.
* `--locked`, Rust 1.85.1, `LAYERFS_CONSTRUCTION_WORKERS=1`, one construction worker,
  no second lane.
* `collect.py` gained one recorded option (`--env KEY=VALUE`, published in the
  declaration as `extra_environment`) so that exactly one arm could enable the
  measurement-only probe. It changes nothing for arms that do not pass it.
* Harness format deviation, recorded and not swept in: the harness carries its
  pre-existing rustfmt hunks, only this round's added lines are formatted, and no
  sample was taken after a reformat of the file its binary identity belongs to.

## 5. The treatment decision: B1 was measured, and its population is zero

B1 reads: *every occurrence that finds an existing row reconstructs that row's chain
to compare stored bytes with offered bytes, so "already reconstructed and compared
equal in this operation" is a reusable proof; a wave-scoped memo is free, a
whole-operation memo is an owner decision about retained bytes — count the repeats
first.* The brief is explicit that this count did not exist and had to be taken
before any memo was written.

**The count is zero.** A measurement-only probe (`ReuseProbe`, enabled solely by
`LAYERFS_STORAGE_REUSE_PROBE=1`) records every identity whose exact-reuse
verification completed and counts the occurrences that find an identity already
recorded in the same operation. It observes the result after the verification and
changes no decision, no byte, no row and no root. Measured:

| case | reuse occurrences (`save.reused`) | probe observations (`save.reuse_repeat`) | repeats |
| --- | --- | --- | --- |
| stride1 | 121,301 | 121,301 | **0** |
| stride10 | 1,121 | 1,121 (debug build, per-call trace) | **0** |

Both figures are taken from the probe arm's own trace
(`delta.profile_reuse_repeat` and the per-state `history.state.<n>.save.reuse_repeat`):
every state is 0, and the whole-run total is 0. The earlier probe attempt that
reported "0" from a build **without** the plumbing is not the evidence — the second,
per-call-traced build is, and it shows the probe reaching `reuse_or_collide` exactly
`save.reused` times (1,121 of 1,121 at stride10) and finding zero repeats.

The mechanism follows from the product, not from the count: within a wave, a repeated
identity is already answered by the wave-local `prepared` map before
`reuse_or_collide` is reached; across waves, a row written earlier is in the wave's
`by_id` snapshot, so the identity is found by lookup rather than re-resolved. The
9.21× growth L52 measured in *reuse events per object written* is growth in the
number of **distinct** identities reaching the reuse path, not in repeated work:
per object, reuse verification still grows 1.99× (§2.4), and that growth is 37.9 % of
the scope's per-object growth — a real and still untreatable-by-memo cost.

Consequence, stated plainly: **a wave-scoped memo and a whole-operation memo would
both save exactly nothing here.** B1 is not pre-registered, not implemented, and no
saving is claimed for it. The owner's rule is to pick a treatment the split names;
the split named B1, and measuring B1 first is what the rule requires. No other lever
was treated this round, so this round ships **no product behaviour change** — only
the instrument, the split and a negative result.

What the split names next, for a later round and without a treatment being claimed
here: the pooled lane's resolution (`pooled_ns`, 8.086 s, the only bucket growing
faster than the scope) and base acquisition (`acquire_ns`, 6.633 s). The reuse bucket
itself remains the largest single part, but its population is distinct identities, so
any treatment must make the *first* verification cheaper — a retained-base question
the owner has reserved — rather than make it repeat-free.

## 6. Checks, as run

Product change (`core/crates/layerfs-storage`), on the final tree:

| check | command | result |
| --- | --- | --- |
| fmt | `cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all -- --check` | exit 0 |
| clippy | `cargo +1.85.1 clippy --manifest-path core/Cargo.toml --locked --all-targets -- -D warnings` | exit 0 |
| test | `cargo +1.85.1 test --manifest-path core/Cargo.toml --locked` | exit 0, **494 passed / 0 failed** |
| boundary | `python3 core/tools/check_product_boundary.py` | exit 0, PASS (122 files) |
| boundary self-test | `python3 core/tools/test_check_product_boundary.py` | exit 0, 6 tests OK |

Harness change: `cargo +1.85.1 test --manifest-path
core/benchmark/fs-bench-pro-storage-content/Cargo.toml --locked` → exit 0, **117
passed / 0 failed**; release build exit 0 with one pre-existing `unused_mut` warning
in `ops/history.rs` that this round did not introduce.

Not run, and why: no CI (`tools/preflight.sh` is permanently retired and was not
used); no verification-mode run, because these rows are diagnostic and admission
`INELIGIBLE` for every arm; no verification-mode run for the stride3 row, for the
same reason as the others.

## 7. Production LOC

| commit | before | after | delta | scope |
| --- | --- | --- | --- | --- |
| `perf(core): the save path's accept cost, attributed in seven disjoint buckets` | 85778 | 85926 | **+148** | core 20361 → 20509 (+148), reference 65417 (0) |
| `bench(#205): publish the save's own nanosecond split per state` | 85926 | 85926 | **0** | harness-only; harnesses are excluded from the production count |
| `perf(core): split resolution into its four disjoint parts` (this round) | 85926 | **85978** | **+52** | core +52, reference 0 |

Method for the +52: `tools/production_loc.py`, nonblank non-comment first-party
product source, counted from the first parent `56ffd020b` and the final staged tree.
Per file: `cas/owner.rs` +46, `cas/membership.rs` +5, `cas/lifecycle.rs` +1,
`cas/mod.rs` 0, `cas/pool_lane.rs` 0, `encoding/delta/select.rs` 0. The harness edits
in `ops/history.rs` and `collect.py` contribute nothing to this number.

## 8. Reproduction

```
EV=docs/roadmap/0.1/0.1.7/evidence/stage-6-history-205-save-split-20260920T081022Z
cargo +1.85.1 build --release --manifest-path \
  core/benchmark/fs-bench-pro-storage-content/Cargo.toml --locked
python3 $EV/with_locks.py <label> python3 $EV/collect.py instrument2 history-stride10 \
  --binary core/benchmark/fs-bench-pro-storage-content/target/release/fs-bench-storage-content \
  --cwd "$PWD" --seal-repo "$PWD" --pre-execute
python3 $EV/split.py instrument2 history-stride10 history-stride3 history-stride1
python3 $EV/split2.py instrument2 history-stride10 history-stride3 history-stride1
# the B1 population, one arm only:
python3 $EV/with_locks.py <label> python3 $EV/collect.py probe history-stride1 \
  --binary …/fs-bench-storage-content --cwd "$PWD" --seal-repo "$PWD" \
  --pre-execute --env LAYERFS_STORAGE_REUSE_PROBE=1
```

Corpus `deepseek-history-data` at tip `b0a7d2ce3b4c19d7452e364b2d7acbfa87e707ed`,
manifest SHA256 `03f21acfb415907f521217e7a972ed512265c8d0c2da0f8034e2ff3014334271`
(verified in the receipts). Selections unchanged: stride10 `range(1,158,10) ∪ {157}`
(17), stride1 157 states.

## 9. What is not claimed

* No admission evidence: all `history.*` rows are diagnostics, admission `INELIGIBLE`,
  O3 `INCOMPLETE`, and every budget class `NOT_RUN`.
* No treatment effect: this round measures a population and finds it empty. It does
  not claim a saving, and it does not credit the negative cross-arm differences
  (−0.612 s / −3.732 s) as an instrument overhead *reduction*.
* No cache claim: no cache was grown, no measured input was pre-touched, no required
  read was moved into setup, no selection was shrunk.
* No pin was hand-edited; `shared/pin_expected.py` freezes only `status == "PASS"`
  receipts and these rows cannot pass.
* The out-of-clock reconciliation gap is untouched and unidentified here.
