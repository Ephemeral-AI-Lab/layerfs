# Stage 6 round 5, continuation — successor prompt

> **Status:** Paste-ready prompt for the round-5 **continuation** agent. It is the entry
> point. The detailed status is
> [`stage-6-round5-handoff-20260919.md`](stage-6-round5-handoff-20260919.md) and the tracked
> assignment is [#184](https://github.com/Ephemeral-AI-Lab/layerfs/issues/184).
> The first half's receipt is
> [`../evidence/stage-6-round5-20260919T000000Z/`](../evidence/stage-6-round5-20260919T000000Z/README.md).
> Stage 6's acceptance issue
> [#171](https://github.com/Ephemeral-AI-Lab/layerfs/issues/171) is **closed**; do not
> reopen it. Rounds 1–4 and the round-5 prompt, plan and receipt are the historical record
> and are not edited.

> **Superseded, 2026-09-19.** This prompt's work is **done**: round 5b measured it and the
> successor's assignment has moved on. The retained-history storage lane is a separate claim
> under a separate contract — read
> [`history-storage-prompt.md`](history-storage-prompt.md) instead. This document stays as
> the historical entry point for the round-5 continuation and is not edited further.

---

You are the Stage 6 round-5 continuation agent for LayerFS, working in
`/Users/yifanxu/Ephemeral-AI-Lab/layerfs`. Your assignment is
[#184](https://github.com/Ephemeral-AI-Lab/layerfs/issues/184): **make the benchmark campaign
cheap and its numbers honest.**

**The honest-numbers half is finished. The cheap half is half done, and the half that is not
done is the larger one.** Round 5's first half closed 9 of the 13 breakdown items and left
V6 partial and V9–V11 untouched. Everything the round saved came from the oracle
(verification ~180 s → 85.5 s); **preparation is byte-for-byte the same work round 4c did**
(98.2 s of a 263.8 s lane, against a 25 s target). That is your work.

**One agent, working alone. No subagents. No codex.** Everything happens in your session.

## Read first, in this order

1. `AGENTS.md` §1–§4 (measurement contract, reuse, budgets, production LOC, no CI)
2. `core/AGENTS.md` (product-source purity, file ceilings, the checks to run)
3. [`stage-6-round5-handoff-20260919.md`](stage-6-round5-handoff-20260919.md) — **the whole
   status: what landed, where the 98.2 s is, the work items with estimates, and the four
   decisions that are not yours to make**
4. `docs/general/benchmark_rules.md` §6 (separate setup / performance / verification /
   cleanup) and §15
5. the frozen specification under `core/docs/benchmark/fs-bench-pro-storage-content/` —
   especially `test_setup_and_cache_discipline.md` §2.2 (what is inside the timer, per case
   shape), §2.1, §3 (the prepared-master build order, layout and manifest) and §3.1 (the
   reuse rules), and `gates_and_oracles.md` §4–§5
6. the harness `core/benchmark/fs-bench-pro-storage-content/README.md`, including its
   closing "What is not yet true here"
7. [`../evidence/stage-6-round5-20260919T000000Z/`](../evidence/stage-6-round5-20260919T000000Z/README.md)
   — the baseline you are improving

## Where the tree is

```text
HEAD   2df19949a  the round-5 first-half receipt, corrected
       0ef2bed6e  the closure run's tree (clean seal, harness binary fb967c78…)
production LOC   84936 combined (core 19519 / reference 65417)
```

The tree is clean. An unrelated documentation change that appeared during the first half has
been committed unmodified on the owner's instruction (`575c6c9ae`, `9214557ce`) and is not
yours to edit.

## Reproduce before you read code

```sh
H=core/benchmark/fs-bench-pro-storage-content
cargo +1.85.1 build --release --manifest-path $H/Cargo.toml --locked
cargo +1.85.1 test  --locked --manifest-path $H/Cargo.toml          # 92 passed / 0 failed
python3 -m unittest discover -s $H/shared -p 'test_*.py'            # 112 tests, OK
python3 $H/runner.py self-check                                     # PASS
python3 $H/runner.py prepare                                        # 75.39 s
python3 $H/runner.py perf  --lane full --out /tmp/base              # ~264 s
python3 $H/runner.py verify --run /tmp/base                         # 0 disagreements
python3 core/tools/check_product_boundary.py                        # PASS, 120 files
python3 tools/production_loc.py                                     # 84936
```

## What is done — do not redo it

V1 the four phases and the six published fields · V2/V3 the pinned-constant oracle
(`tests/golden/expected.tsv`, 1,969 rows, `include_str!`-embedded) · V4 the unrequired
read-back removed from the four families whose frozen oracle does not ask for O2 · V5 the
required O2 deduplicated by distinct `(root, expectation)` pair · V7 `--reuse-pass` ·
V8 the mode ladder · V12 the digest key · V13 `handoff_ns`. Details in the handoff §3.

**Measured baseline to beat, and not to break:** 217 of 217 admission rows `PASS`, 0 `FAIL`,
lane **263.776 s**, `sum(operation_ns)` **76.184 s**, **0** product counters moved against
round 4c, 0 verification disagreements, all budgets `PASS` (max complete command 11.471 s).

## What is left, in the order it pays

| order | item | rows | preparation it removes |
| ---: | --- | ---: | ---: |
| 1 | **V11** remove the per-member base clone in `delta_fixture` | 20 | `prepare` cost only |
| 2 | **V10** pack the object set into one file (`Artifact::write` writes one file per object today) | — | prerequisite for V9's load target |
| 3 | **V9a** prepared masters for `c1.edit.*` + `c1.cdc.chunk-count` | 56 | **52.7 s** |
| 4 | **V9b** prepared masters for `c2.reuse.*` + `c2.read.waves` + `c2.footprint` | 34 | **25.8 s** |
| 5 | **V9c** prepared masters for `c1.fs.build-scale` / namespace | 8 | **10.0 s** |
| 6 | **the registry declares "needs a master"**, so `PHASE_SPLIT_FAMILIES` stops being hand-maintained | — | removes a known trap |
| 7 | **V6 remainder** — persist expectation digests for every family that builds one | — | S3's last case |

`c1.construct.*` **must not be prepared**: the construction *is* the measured operation.
The delta family already has a master and spends **0.19 s per row** of preparation; that is
the number the other six reach.

**Before you build V9, read the handoff §5.2.** Producing ~98 masters is estimated at
100–140 s for `prepare --lane full`, over its 90 s target, while being the thing that makes
the 200 s lane target reachable. T2 and T3 pull against each other and the choice is the
owner's.

## The four decisions — take a ruling, do not guess

1. **T1 for ~15 rows at the 500 MiB tier.** Loading a 500 MiB object set and re-identifying
   every object is ~0.6–1.0 s before any other work, against a 1.0 s per-row target. Lazy
   loading would break the declared `warm-in-process-fixture` state. Tier-scaled target,
   allowed cache-state change, or those rows `INCOMPLETE`?
2. **T2 versus T3** — see above. Which target governs?
3. **The quick lane ≤ 70 s** is in tension with the rule that omitting an in-process oracle
   is what makes a row `INCOMPLETE`. Change the driver contract, or withdraw the target?
4. **`FilesystemRead::inode`** (owner ruling 2) was not done. It needs a product-source
   change and a test pinning both sides — `PathNotFound` for an unbound name, `MissingObject`
   for a provider that does not hold the tree's own root. Report a blocker rather than
   guessing.

Also open, and not blockers: the harness identity does not cover the harness's own Python;
and the unrelated documentation in the working tree.

## The rules that will decide your work

- **Fix defects; do not add features** (owner ruling, 2026-09-19). A defect in
  `core/benchmark/**` or its documentation — **including a pre-existing one this round merely
  exposed** — is in scope to fix, and fixing it beats recording it. The items this assignment
  names (V6, V9, V10, V11, the registry declaration) are the assignment, not new capability.
  Out of scope: capability the breakdown does not name, any product API or behaviour change
  beyond the scoped `FilesystemRead::inode` item, relaxing a gate, a limit or a budget, and
  widening the harness's surface with a new verb, evidence format or aggregate. If a defect
  can only be fixed that way, **stop and report it as a blocker**. A bug fix is not exempt
  from the guardrails below: one that would make `operation_ns` fall, move a counter, shrink a
  workload or relax a limit is a blocker, not a fix.
- **No product source change.** This is `core/benchmark/**` plus harness documentation. The
  one scoped exception is decision 4 above, and it needs its own confirmation first.
- **`operation_ns` must not fall.** T4 is a falsifier: if the golden number improves,
  measured work has moved into setup and the change is **rejected**. This is the single most
  important guard in the round.
- **Every V9–V11 change must leave all 1,757 pinned counters and 212 identity digests
  identical.** `python3 shared/compare_runs.py <baseline> <new>` must print
  `verdict IDENTICAL`. A new counter needs a bootstrap lane and a re-freeze of
  `tests/golden/expected.tsv` (`shared/pin_expected.py`).
- **Never shrink a workload, relax a limit, inflate a timeout or add a worker.**
- **A timed phase never retains, and a prepared master is preparation reuse, never a cold
  claim.** `prepared-dewarmed` rows still de-warm and still gate `resident_pages == 0` and
  `disk_read_bytes`.
- **A sampled or omitted row is `INCOMPLETE`, never `PASS`.**
- **Fresh output, append-only everything.** A harness change invalidates the harness
  identity; re-run into a **new** directory. The first half's receipts are not edited.
- **No aggregate gate, no CI workflow, no wrapper.** `tools/preflight.sh` stays retired.
- **Verify the tree you changed and report exactly which checks ran and which did not.**

## Traps you will otherwise pay for

- `TreeStore::write_to_dir` / `load_from_dir` is **lossy** — canonical bytes only, everything
  re-wrapped as `ObjectRole::Chunk` with no references and no predecessors.
  `src/workload/artifact.rs` is the lossless format; do not go back.
- The artifact layout, manifest schema and immutability step are fixed by
  `test_setup_and_cache_discipline.md` §3 (`seal: chmod removes 0o222`). Follow the spec.
- Warm preparation is floored by *"read the object set and hash it"* —
  `FinalizedObject::new` hashes on load, and `Artifact::read` re-identifies every object
  again. This is why T1 is tight at 500 MiB.
- **Do not rebuild the harness binary while a lane is running.** Children are spawned from
  `target/release/fs-bench-storage-content`; a rebuild mid-lane silently mixes two binaries
  and invalidates the run. This happened once in the first half.
- `PHASE_SPLIT_FAMILIES` in `runner.py` is hand-maintained and must stay in sync with the
  drivers' `oracle_phase` declarations. Item 6 above exists to delete it.
- `Store::contains` answers **per lookup page** (`LOOKUP_PAGE_IDS = 128`) and refuses a demand
  above `StorageCapacities::read_objects` (4,096). Dedup the input and page the query;
  `workload::expected::present_all` does both.
- The report's `operation max ms` column is now the **operation**; `complete_command_ns` in
  `run.json` is the wall. `run.json`'s `wall_ns` is the whole `perf` invocation and is not
  the sum of the rows' complete commands.
- `MAXIMUM_WALK_ENTRIES = 4096` is the ceiling a large single build trips; the error text is
  `InvalidRecord("cycle check work limit")`.
- `ChildDescriptor::cumulative_logical_end` is cumulative **within its own page**.
  `OpenPack::assembled` **includes** the pack header. A wave's membership snapshot is stale
  after any seal in that wave.
- `pack_cache` and `pool_reader` are two different caches. `TreeStore` is not enough for an
  update chain — use `SharedStore`/`SharedReader` or `PairProvider`.
- `FilesystemResources::maximum_pending_records` is 4,096; above it an operation needs a
  caller-supplied `FileBacking` or it fails `ResourceUnavailable { what: "ordering backing" }`.
- The page size is **16,384 bytes** — read it, never hardcode it. This shell's working
  directory is not stable across invocations; use absolute paths. `timeout(1)` is not
  installed. zsh does not word-split unquoted variables (`${=ARGS}`).

## Definition of done

Preparation `<= 1.0 s` per row at every tier (or the owner's ruling on the 500 MiB floor);
lane `sum(preparation_wall_ns) <= 25 s`; `prepare --lane full <= 90 s` once per digest (or
the owner's ruling on which target governs); the largest single verification invocation
`<= 5 s`; the lane `<= 200 s` warm with full verification; **`sum(operation_ns)` published
and not fallen**; every pinned counter and identity digest identical; `--reuse-pass` and the
mode ladder still failing closed; a fresh full lane into a **new** directory with `verify`,
`report`, `calibrate` and a new dated evidence directory; and the #184 checkboxes updated to
what actually holds, with the misses stated as plainly as the passes.
