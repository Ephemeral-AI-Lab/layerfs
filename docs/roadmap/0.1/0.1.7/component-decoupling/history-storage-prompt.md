# Retained history storage — successor prompt

> **Status:** Paste-ready prompt for the retained-history storage agent. It is the entry
> point. The detailed specification is
> [`core/docs/benchmark/fs-bench-pro-storage-content/history-storage-optimization/`](../../../../../core/docs/benchmark/fs-bench-pro-storage-content/history-storage-optimization/README.md)
> and the tracked assignment is [#186](https://github.com/Ephemeral-AI-Lab/layerfs/issues/186),
> a sub-issue of [#184](https://github.com/Ephemeral-AI-Lab/layerfs/issues/184).
> Stage 6's acceptance issue [#171](https://github.com/Ephemeral-AI-Lab/layerfs/issues/171)
> is **closed**; do not reopen it. Rounds 1–5b and their prompts, plans, handoffs and
> receipts are the historical record and are not edited.

---

You are the retained-history storage agent for LayerFS, working in
`/Users/yifanxu/Ephemeral-AI-Lab/layerfs`. Your assignment is
[#186](https://github.com/Ephemeral-AI-Lab/layerfs/issues/186): **save a real repository
history into one Store, and measure what it costs to retain.**

This is a **new benchmark claim**, not a harness repair. Round 5b closed the harness work:
four phases published and reconciling on all 217 admission rows, lane 137.650 s,
`sum(operation_ns)` 76.942 s, budgets already passing. **Nothing you do changes a 217-row
verdict.** You add a group of three rows under a separate contract, in a separate lane.

**One agent, working alone. No subagents. No codex.** Everything happens in your session.

## Read first, in this order

1. `AGENTS.md` §1–§4 (measurement contract, reuse, budgets, production LOC, no CI)
2. `core/AGENTS.md` (product-source purity, file ceilings, the checks to run)
3. `docs/general/benchmark_rules.md` — especially §1 (specify the claim before implementing),
   §6 (separate setup / performance / verification / cleanup) and §15 (fast loop and terminal
   admission)
4. **the five campaign documents**, in this order:
   [`README.md`](../../../../../core/docs/benchmark/fs-bench-pro-storage-content/history-storage-optimization/README.md)
   (the claim, the three rows, the pins, the seven open decisions),
   [`preparation.md`](../../../../../core/docs/benchmark/fs-bench-pro-storage-content/history-storage-optimization/preparation.md),
   [`verification.md`](../../../../../core/docs/benchmark/fs-bench-pro-storage-content/history-storage-optimization/verification.md),
   [`measurement.md`](../../../../../core/docs/benchmark/fs-bench-pro-storage-content/history-storage-optimization/measurement.md),
   [`implementation-plan.md`](../../../../../core/docs/benchmark/fs-bench-pro-storage-content/history-storage-optimization/implementation-plan.md)
   — the last one carries the file specs you will implement
5. the harness `core/benchmark/fs-bench-pro-storage-content/README.md`, including its closing
   "What is not yet true here" section, and `core/docs/benchmark/fs-bench-pro-storage-content/CONTRACT.md`
   §4 and §11 (the budget formula, erratum E4)
6. [#186](https://github.com/Ephemeral-AI-Lab/layerfs/issues/186), then
   [#184](https://github.com/Ephemeral-AI-Lab/layerfs/issues/184) for the harness conventions
   it established
7. the v0.1.6 record you are pinned against:
   [`../evidence/`](../evidence/) is not it — it is
   `docs/roadmap/0.1/0.1.6/evidence/issue153-retained-history-report.md` (ledger `L31`)

## Where the tree is

```text
HEAD   8c3686b21  docs(#184): report that 66bc974d0 swept up another workstream's files
       66bc974d0  docs(#184): the round-5b receipt and the four closures
       2f8ebc90d  bench(#184): close the budget formula, the two unwired axes, and the master prune
       4f7c1221d  docs(#184): close round 5
production LOC   84936 combined (core 19519 / reference 65417)
```

**The tree is clean.** Your work changes no product source, so every commit reports
`Production LOC: 84936 -> 84936 (delta 0)` with the harness lines stated separately.

The 217-row lane is **not yours to change**. It keeps its registry, cardinality, golden
table, `--lane full` composition and `full` verification default.

## Reproduce before you read code

```sh
H=core/benchmark/fs-bench-pro-storage-content
cargo +1.85.1 build --release --manifest-path $H/Cargo.toml --locked
cargo +1.85.1 test --locked --manifest-path $H/Cargo.toml
python3 -m unittest discover -s $H/shared -p 'test_*.py'
python3 $H/runner.py self-check
python3 $H/runner.py perf --lane smoke --out /tmp/smoke
python3 $H/runner.py report --run /tmp/smoke
cargo +1.85.1 test --locked --manifest-path core/Cargo.toml
python3 core/tools/check_product_boundary.py
python3 tools/production_loc.py
ls /Users/yifanxu/Ephemeral-AI-Lab/deepseek-history-data   # the corpus must be present
```

If the corpus is absent, **stop and report** — do not substitute another source, and do not
proceed with a reduced selection.

## The model you are implementing

**Three rows, one per selection. One Store per row, created inside the sample and grown in
place. No prepared artifact, no copy, no checkpoint.**

| row | selection | states | cumulative logical bytes |
| --- | --- | --: | --: |
| `history-stride10` | `range(1,158,10) ∪ {157}` | 17 | 561,010,345 |
| `history-stride3` | `range(1,158,3)` | 53 | 1,676,767,835 |
| `history-stride1` | all checkpoints | 157 | 4,936,693,030 |

```text
create the Store
for each state k, in order:
    read state k's changed bytes from the corpus      untimed, harness
      construct the changed content                   TIMED
      build_filesystem against the previous root      TIMED
      save                                            TIMED
close
```

Four phases, published separately in every receipt. **Real work is the golden number.**

| | phase | field | role |
| --- | --- | --- | --- |
| a | preparation | `preparation_wall_ns` (`acquisition_wall_ns` is **zero** here) | corpus authentication only |
| b | **real work** | **`operation_ns`** | **the golden number — the sum of the N named per-state children** |
| c | verification | `verification_wall_ns` | the final gate, its own declared budget |
| d | cleanup | `cleanup_wall_ns` | closes the Store; the Store itself is removed with the run directory |
| | process wall | `complete_command_ns` | the declared lane ceiling |
| | harness work inside a timer | `handoff_ns` | so harness overhead is visible, not absorbed |

**`operation_ns` is the sum of the children, not the root.** The root would include the
harness's own corpus reading between the children. This is owner decision 2 in the README.

`verify` must re-derive all of these and **fail closed** if they do not reconcile with the wall
inside the declared tolerance.

## The tier policy (owner direction — do not re-open)

| tier | states | role | optimization | bug fix |
| --- | --: | --- | --- | --- |
| `history-stride10` | 17 | smoke | **P0** — first place anything is tried | **P0** |
| `history-stride3` | 53 | intermediate | **P0** — must also pass | **P0** |
| `history-stride1` | 157 | run only | **never optimized** | run to confirm |

```text
iterate on stride10          P0, cheap reject
  → confirm on stride3       P0, matched n3 alternating pairs
    → run stride1 once       confirmation only, never tuned
```

## The breakdown

| phase | deliverable | exit gate |
| --- | --- | --- |
| **0** | the five documents, a roadmap specification, the sub-issue | the seven README decisions ruled; rows, lanes and pins frozen. **No product run** — `benchmark_rules.md` §1 makes a family measured before its specification exists exploratory |
| **1** | `src/workload/history.rs`, `shared/history_corpus.py`, their self-checks | corpus authenticates; 17/53/157 enumerate at the right indices; the three path-state pins match; an unidentifiable blob is refused |
| **2** | the remaining harness gap: the Store's before/after delta and the `object_role` split in `shared/space.py` | a 217-lane re-run carries the new fields, every counter unchanged, `sum(operation_ns)` unmoved |
| **3** | `history-stride10` rows, driver, gates, golden, first measured run | row `PASS`; the final gate passes over all 17 states; **smoke tier live; the per-tier and verification budgets declared here** |
| **4** | `history-stride3` | canonical 589,423,458 B / 73,476 objects; allocated below 64,024,576 B; ratio ≥ 26.2×; **intermediate tier live** |
| **5** | `history-stride1` | canonical 871,588,115 B / 104,705 objects; allocated below 83,947,520 B; ratio ≥ 58.8×; **run-only tier** |
| **6** | the optimization ledger | promotion one way only; stride1 never a target |

Phase 2a is **already closed** by `2f8ebc90d` — CPU, `process_peak_rss_bytes` and the budget
formula are wired. Do not redo it. The 10 ms `RssSampler` stays deliberately unwired.

## The pins

| row | path-states | logical bytes | canonical content | canonical objects |
| --- | --: | --: | --: | --: |
| `history-stride10` | 101,477 | 561,010,345 | *first-run pin* | *first-run pin* |
| `history-stride3` | 306,861 | 1,676,767,835 | 589,423,458 B | 73,476 |
| `history-stride1` | 904,143 | 4,936,693,030 | 871,588,115 B | 104,705 |

Reference points, **not** gates and **not** comparisons: v0.1.6 Store allocated / apparent
49,344,512 / 49,315,940 at 17 states, 64,024,576 / 64,000,100 at 53, 83,947,520 / 82,677,860 at
157. Git53 = 49,332,224 B, Git157 = 56,373,248 B, cited rather than re-run.

## The rules that will decide your work

- **Storage is the falsifier; time is a one-sided tripwire.** The canonical content is pinned
  byte-identical across generations and the Store format is preserved, so the core Store should
  land **below** v0.1.6's bytes. A speed-up is guaranteed by the removed container, FUSE mount,
  spool and Commit envelope: **passing the time comparison proves nothing, failing it proves a
  flaw.**
- **The final gate is all-or-nothing.** For every state: root equals its pin, tree equals
  `oracles/<sha>.json`, a deterministic 10 % of its files reads back byte-exact. A disagreement
  fails the row — "156 of 157" is a failure, not a pass.
- **Nothing in a timed region reads the corpus.** The reading happens between the children.
- **No constructed object is supplied to a measured child.** `InodeValue` references
  `content_root` by id; the content objects are what the child produces.
- **The budget is lifted for this family, and a lifted budget is still a declared ceiling.**
  Declare the per-tier and verification budgets at Phase 3 and never inflate one afterwards to
  turn a miss into a pass.
- **A canonical pin that does not match stops the phase.** It is a finding, not a fixture tweak.
- One sample per case per arm; fresh `--output`; append-only receipts; the measurement lock
  held; no tier shrunk, no timeout inflated, no worker added, no case dropped.

## Rulings (owner, 2026-09-19) — do not re-open these

1. **The family's budget is lifted.** The ≤ 15 s per-row complete-command rule does not apply
   to these three rows. The lane ceilings are declared instead.
2. **The tiers are stride10 (smoke, P0), stride3 (intermediate, P0), stride1 (run only).**
   stride1 is **never optimized**: no constant, threshold, buffer, batch or policy value is
   changed because of a stride1 number, and a stride1 failure while both lower tiers pass is a
   scaling finding to investigate at stride10/stride3, not to patch at stride1.
3. **This lane builds no prepared storage.** `Preparation::InProcess`; nothing under
   `prepared/`. A second run costs what the first cost, and that is correct here.

## Traps you will otherwise pay for

1. **Treating the v0.1.6 time comparison as a gate.** It is a labelled one-sided tripwire.
2. **Wrapping the whole chain in one timer.** The corpus reading lands inside it and
   `operation_ns` becomes partly harness work. Use one named child per state.
3. **Burying `Store::create` in the chain.** It sits in state 1's child and is published as
   `history.state.1.create_ns` so the fixed cost is visible and subtractable.
4. **Pre-loading the corpus.** 4.94 GB resident would defeat the memory claim and warm the very
   pages the saves read.
5. **Assuming the ≤ 15 s rule still applies.** It does not; the declared lane ceiling does.
6. **Supplying constructed objects to the child.** That moves measured work into preparation.
7. **Reading the Store once from `verify`.** `space.footprint()` is called once today and
   reports only the end state; a before/after pair and a curve need the reading inside the
   invocation.
8. **A naive pack-by-role join.** `object_packs.data` is a whole pack holding many objects, so
   `SUM(length(data)) GROUP BY object_role` multiply-counts. Use `objects.canonical_length`
   grouped by `objects.object_role`, plus one pack-framing overhead number.
9. **The sample's missing endpoint.** `sampled_indices` takes every tenth with
   `take(ceil(n/10))`, which for 53 units omits the last. Owner decision 4.
10. **Making `sample` a `PASS` without the ruling.** In the 217 a sampled row is `INCOMPLETE`;
    this lane may differ only because the README freezes it before collection.
11. **Optimizing stride1.** Forbidden, and it is how a benchmark starts measuring its tuning.
12. **Touching the 217.** The group is `history.*`, its own lanes, cardinality 3, outside the
    217 and outside `--lane full`.

## Definition of done

- Phases 0–6 complete, each with its exit gate recorded.
- The 15 acceptance items of [#186](https://github.com/Ephemeral-AI-Lab/layerfs/issues/186) §7
  satisfied, and every one that is not reported as not-run with its measured wall and reason.
- A fresh dated evidence directory under `docs/roadmap/0.1/0.1.7/evidence/`, with the run
  documents, the per-row table and the falsifier readings.
- The optimization ledger opened at Phase 6 with its first entries, negative outcomes included.
- `Production LOC: 84936 -> 84936 (delta 0)` in every commit message, with harness lines
  stated separately.
- Every command you ran named in the handoff, and every check you did **not** run named with
  the reason. No claim of "CI green" — this repository runs no CI and `tools/preflight.sh` is
  permanently retired.

## Before your first product run

**The seven owner decisions in the README §7 must be ruled.** Until they are, the affected
gates are proposed and no row measured under them is admission evidence. Ask for the rulings
rather than assuming them; Phase 0 exists for exactly this.
