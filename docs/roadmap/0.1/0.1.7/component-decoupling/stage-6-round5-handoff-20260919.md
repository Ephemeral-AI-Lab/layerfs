# Stage 6 round 5 — handoff, and what the round did not finish

> **Status:** Handoff record; target LayerFS v0.1.7. It is the detailed companion to
> [`stage-6-round5-continuation-prompt.md`](stage-6-round5-continuation-prompt.md), which is
> the paste-ready entry point for the successor. The tracked assignment is
> [#184](https://github.com/Ephemeral-AI-Lab/layerfs/issues/184).
>
> **Measured and estimated are kept apart.** Every figure in §2 is a measurement, quoted
> from the round-5 closure run and named as such. Every figure in §4 and §5 is an
> **estimate** made from this round's own measured costs, and is labelled `[E]`.
>
> Rounds 1–4, their prompts, their handoffs and their receipts are the historical record and
> are not edited. Stage 6's acceptance issue
> [#171](https://github.com/Ephemeral-AI-Lab/layerfs/issues/171) is **closed** and was not
> reopened.

> **Forward pointer, 2026-09-19.** The successor's assignment has moved on: the
> retained-history storage lane is a separate claim under a separate contract, and its
> paste-ready entry point is [`history-storage-prompt.md`](history-storage-prompt.md). The
> continuation prompt this document is the companion to is superseded.

> **The preparation half landed, 2026-09-19 (round 5b).** This document's §4, §5.1 and §5.2
> leave V6 and V9–V11 open and estimate them at 12–18 hours; they are done, together with
> the registry declaration that deletes `PHASE_SPLIT_FAMILIES`. **Its measured figures stand
> as the round-5 record and are not edited**; the successor's status, the new numbers and
> the misses that remain are in
> [`../evidence/stage-6-round5b-20260919T000000Z/`](../evidence/stage-6-round5b-20260919T000000Z/README.md).

## 1. Where the tree is

```text
HEAD   2df19949a  the round-5 receipt's corrected before/after table
       2df19949a  docs(#184): correct the lane wall in the receipt's before/after table
       c90a5900f  docs(#184): record the checks that ran, with their counts
       0016c2876  docs(#184): declare the quick-mode arms' identity rather than implying a match
       473a0b1c0  docs(#184): the round-5 closure receipt, and what the harness still does not do
       0ef2bed6e  bench(#184): restore `load_reused_proof`, removed by the digest-key commit
       eedbaa5f8  bench(#184): the prepared-master key is the product identity plus the recipe version
       cb3dabbe1  bench(#184): publish the four phases, pin the oracle, and stop over-verifying
       0654d3afd  docs(#184): the record of the three round-5 rulings
production LOC   84936 combined (core 19519 / reference 65417), unchanged by every commit
                 above: no product source changed in this round
```

**The tree is clean.** An unrelated documentation change — a new
`core/docs/architecture/deferred/` paper with its index entry (tracked by #185), plus
`core/docs/benchmark/fs-bench-pro-storage-content/history-storage-optimization/` — appeared
during the round. It is not this round's work, it was left untouched, and it was set aside
for the duration of the closure run so the seal is clean. **Owner ruling (2026-09-19): commit
it, unmodified.** It is committed byte-identical in `575c6c9ae` and `9214557ce`; the per-file
sha256s are in those commit messages. It is not evidence for any round-5 claim.

## 2. What landed, measured

The closure run: `python3 runner.py perf --lane full --out <dir>` at `0ef2bed6e`, clean tree.
Receipt at
[`../evidence/stage-6-round5-20260919T000000Z/`](../evidence/stage-6-round5-20260919T000000Z/README.md).

| | round 4c | round 5 |
| --- | ---: | ---: |
| admission rows `PASS` / `FAIL` | 217 / 0 | **217 / 0** |
| lane wall | 415.821 s | **263.776 s** (−36.6%) |
| `sum(operation_ns)` (the golden number) | not published | **76.184 s** |
| preparation | ~180 s [D] | **98.226 s** |
| verification | ~180 s [D] | **85.513 s** |
| cleanup | not measured | **0.000077 s** (5,917 ns max per row) |
| `handoff_ns` | not published | **1.127 s** |
| largest complete command | 9.952 s | 11.471 s (all `PASS`) |
| verification disagreements | 0 | **0** |
| product counters moved | — | **0** |

`T4` — the round's falsifier — held: over the 161 rows that publish a `timing.json` in both
runs, `sum(operation_ns)` went **63.497 s → 66.673 s (+5.00%)**. Nothing fell.

**What produced the 152 s:** the oracle, entirely. `c2.delta.cdc-locality`'s verification
went from 12.1 s to 19 ms per 500-tier row (V4), and `c2.reuse.cross-file`'s required O2 is
now verified once per distinct `(root, expectation)` pair (V5). **Preparation was not
touched**: 98.226 s of round-5 work is the same work round 4c did.

## 3. Item status

| # | item | state |
| --- | --- | --- |
| V1 | six phase fields published; `verify` re-derives and fails closed | **done** |
| V2 | O1 as a pinned expected identity | **done** (212 of 217 admission rows; 5 are O7) |
| V3 | O3 counts gated against pinned values | **done** (217 of 217 pin ≥1 counter) |
| V4 | unrequired O2 removed from the four families | **done** |
| V5 | required O2 deduplicated by distinct pair | **done** |
| V6 | expectation digests persisted for every family | **partial** — the delta family only |
| V7 | `--reuse-pass` | **done** (0.02 s; four refusal paths exercised) |
| V8 | mode ladder `full` / `sample` / `none` | **done, slow** — see §4.3 |
| V9 | prepared masters for the remaining fixture-heavy families | **not started** |
| V10 | pack the object set into one file | **not started** |
| V11 | remove the per-member base clone | **not started** |
| V12 | digest key = product identity + recipe version | **done** |
| V13 | `handoff_ns` published rather than absorbed | **done** |

### 3.1 Completion, by four denominators

| denominator | basis | % |
| --- | --- | --- |
| item list (V1–V13) | 9 done, V6 half, V9–V11 zero, equal weight | **73%** |
| stated targets met | 5 of 10 | **50%** |
| lane cost closed | 415.821 → 200 s is a 215.8 s gap; 152.0 s achieved | **70%** |
| the round's two halves | honest numbers ≈ 100%; cheap campaign ≈ 50% | **~75%** |

**The defensible headline is ~70%**, with the caveat that matters more than the figure:
the "honest numbers" half is essentially finished and the "cheap campaign" half is half
done, and the half that is not done is the larger one.

## 4. What is left — work items `[E]`

Estimates are made from this round's own measured costs: a full lane is **4.4 min** plus
2 s of `verify`; `prepare --lane full` is **75.39 s**; an incremental harness build is ~4 s.
Implementation and the counter-equality re-checks dominate; measurement does not.

| item | code | validation | wall-clock `[E]` | buys `[E]` |
| --- | --- | --- | --- | --- |
| V11 remove the per-member base clone | ~30 lines | `prepare` + 1 lane | 0.5–1 h | `prepare` cost only |
| V10 packed single-file object set | ~120–150 lines | 1 lane | 2–3 h | load ≤ 1.0 s/row |
| V9a `c1.edit.*` + `c1.cdc.chunk-count` (56 rows) | ~150 lines | targeted + 1 lane | 3–5 h | −52.7 s preparation |
| V9b `c2.reuse.*` + `read.waves` + `footprint` (34 rows) | ~200 lines | targeted + 1 lane | 3–4 h | −25.8 s |
| V9c `c1.fs.build-scale` / namespace (8 rows) | ~100 lines | targeted + 1 lane | 1.5–2.5 h | −10.0 s |
| registry declares "needs a master" | ~40 lines | self-check + golden TSV | 0.5–1 h | kills the hand-maintained `PHASE_SPLIT_FAMILIES` trap |
| V6 remainder | ~80 lines | falls out of V9 | 1 h | S3's last case |

**≈ 12–18 focused hours**, 4–6 full-lane validation runs. If V9–V11 land, the lane is
projected at **~190–210 s** (the ≤200 s target becomes reachable) and lane preparation at
**~25–35 s**.

### 4.1 Where the 98.226 s is, by family (measured)

```text
c1.edit.length-changing      29.73 s   (32 rows)   <- V9a
c2.reuse.workspace           11.99 s   (14 rows)   <- V9b
c1.cdc.chunk-count           11.49 s   (12 rows)   <- V9a
c1.edit.length-preserving    11.47 s   (12 rows)   <- V9a
c1.fs.build-scale             9.95 s    (8 rows)   <- V9c
c2.read.waves                 6.51 s    (4 rows)   <- V9b
c2.reuse.cross-file           3.99 s   (10 rows)   <- V9b
c2.delta.cdc-locality         3.93 s   (20 rows)   already has a master: 0.19 s/row
c2.footprint                  3.26 s    (6 rows)   <- V9b
c1.construct.*                5.70 s    (8 rows)   must NOT be prepared
```

**34 of 217 rows exceed the 1.0 s per-row target**; the worst is 5.448 s. The delta family
is the existence proof of what the others would reach.

### 4.2 Why the quick lane is not fast (V8, measured)

`--verify none` is **254.6 s** against 263.8 s. It skips the *deferred* verification
invocation, which only `c2.delta.cdc-locality` has. For the other 200 admission rows the
oracle is a second, unmeasured, byte-identical operation **inside** the performance
invocation, and omitting it is a driver-contract change, not a runner flag.

## 5. Hard blockers — four decisions, not four work items

These cannot be resolved by more implementation. Each needs an owner ruling before the
successor spends time on it.

1. **T1 may be physically unreachable for ~15 rows at the 500 MiB tier.** Loading a 500 MiB
   object set means reading it *and* re-identifying every object — `FinalizedObject::new`
   hashes on load and `Artifact::read` hashes again — which is ~0.6–1.0 s `[E]` before any
   other work in the phase. Lazy loading would break the declared
   `warm-in-process-fixture` cache state, which `AGENTS.md` §1 forbids. **Ruling needed:**
   a tier-scaled preparation target, an allowed cache-state change, or those rows accepted
   as `INCOMPLETE`.
2. **T2 and T3 pull against each other.** `prepare --lane full` passes at **75.39 s**
   *because only 20 masters exist*. Producing ~98 would cost roughly today's preparation for
   those families (~87 s) plus write and validate — **100–140 s `[E]`**, over the 90 s
   target, while being the thing that makes the 200 s lane target reachable. The acquisition
   is untimed and once per digest, so a ≤ 90 s ceiling on it may be the wrong shape.
   **Ruling needed: which target governs.**
3. **The quick lane ≤ 70 s is in tension with the round's own measurement rule.** Omitting an
   in-process oracle is exactly what makes a row `INCOMPLETE`, which is the rule that keeps
   an iteration run from being admission evidence. **Ruling needed:** whether the driver
   contract changes, or the 70 s target is withdrawn.
4. **`FilesystemRead::inode` (owner ruling 2) was not done.** It is scoped into #184, needs a
   product-source change — which this round otherwise forbids — and a test that pins both
   sides (`PathNotFound` for an unbound name, `MissingObject` for a provider that does not
   hold the tree's own root). It blocks nothing else, and guessing at the classification is
   explicitly forbidden.

### 5.1 Two smaller open items

- **The harness identity does not cover the harness's own Python.** A receipt names the Rust
  binary's sha256, both lockfiles and the registry table; a Python-only harness change leaves
  every one of them unchanged. Stated so a reader is not misled by `harness_binary_sha256`;
  fixing it is a scope question, not a defect in the round.

## 6. Standing constraint, not a blocker

Every V9–V11 change must leave all **1,757 pinned counters** and **212 identity digests**
identical, and a newly published counter needs a bootstrap lane plus a re-freeze of
`tests/golden/expected.tsv`. That is checkable in one command:

```sh
python3 shared/compare_runs.py <baseline-run> <new-run>   # verdict IDENTICAL or DRIFT
python3 shared/pin_expected.py digests --run <run> --out <t.tsv>
```

It is the reason the remaining work is hours rather than minutes, not a risk of failure.

## 7. Rules the successor inherits unchanged

### 7.1 Defects may be fixed; features may not (owner ruling, 2026-09-19)

**The continuation agent may fix defects it finds, within scope, and may not introduce new
features.** Fixing a defect is preferred to recording it.

- **In scope to fix:** any defect in `core/benchmark/**` or its documentation, **including a
  pre-existing one** that the round merely exposed. Three were fixed in the first half
  (56 rows publishing no operation time; a prepared master sealed by another binary being
  consumed anyway; `load_reused_proof` deleted by a sibling commit) and a fourth is recorded
  rather than fixed because its fix is a scope question, not a defect.
- **Also in scope:** the items this assignment names — V6, V9, V10, V11 and the registry
  declaration. They are the assignment, not new capability.
- **Not in scope:** adding capability the breakdown does not name; changing the product's API
  or behaviour beyond the scoped `FilesystemRead::inode` item; relaxing a gate, a limit or a
  budget; widening the harness's surface with a new verb, a new evidence format or a new
  aggregate. When a defect's fix would need one of those, **stop and report it as a blocker**
  rather than widening the round.

The guardrails below still decide whether a fix is admissible, and a bug fix is no exception:
a defect that can only be fixed by making `operation_ns` fall, by moving a counter, by
shrinking a workload or by relaxing a limit is not a bug fix — it is a blocker.

- **No product source change.** `core/benchmark/**` plus harness documentation. If a step
  appears to need one, stop and report it. The one scoped exception is
  `FilesystemRead::inode`, and it needs its own confirmation first (§5.4).
- **`operation_ns` must not fall.** T4 is a falsifier: if the golden number improves,
  measured work has moved into setup and the change is rejected.
- **Never shrink a workload, relax a limit, inflate a timeout or add a worker.**
- **A timed phase never retains, and a prepared master is preparation reuse, never a cold
  claim.** `prepared-dewarmed` rows still de-warm and still gate `resident_pages == 0` and
  `disk_read_bytes`.
- **A sampled or omitted row is `INCOMPLETE`, never `PASS`.**
- **Fresh output, append-only everything.** A harness change invalidates the harness
  identity; re-run into a **new** directory.
- **No aggregate gate, no CI workflow, no wrapper.** `tools/preflight.sh` stays retired.
- **Verify the tree you changed and report exactly which checks ran and which did not.**
- Do not re-open owner rulings 1–3 (the golden number reports and does not gate; the
  `inode` classification is its own item; the digest key is the product identity plus a
  declared fixture-recipe version).

## 8. Reproduction

```sh
H=core/benchmark/fs-bench-pro-storage-content
cargo +1.85.1 build --release --manifest-path $H/Cargo.toml --locked
cargo +1.85.1 test  --locked --manifest-path $H/Cargo.toml            # 92 passed / 0 failed
python3 -m unittest discover -s $H/shared -p 'test_*.py'              # 112 tests, OK
python3 $H/runner.py self-check                                       # PASS
python3 $H/runner.py prepare                                          # 75.39 s, once per digest
python3 $H/runner.py perf  --lane full --out /tmp/r5
python3 $H/runner.py verify --run /tmp/r5
python3 $H/runner.py report --run /tmp/r5
python3 $H/runner.py calibrate --out /tmp/r5
python3 $H/shared/compare_runs.py <round-4c-run> /tmp/r5             # verdict IDENTICAL
python3 core/tools/check_product_boundary.py                          # PASS, 120 files
python3 tools/production_loc.py                                       # 84936 combined
```

`cargo +1.85.1 clippy` and `cargo +1.85.1 fmt` are **not** gates for this workspace and were
not made ones: the harness is not rustfmt-clean at HEAD (24 files, most untouched by this
round) and running `fmt` over the workspace would bury a five-commit round in a 24-file
reformat that changes no behaviour.
