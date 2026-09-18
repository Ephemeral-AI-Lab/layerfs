# Stage 6 round 4 — successor prompt

> **Status:** Paste-ready prompt for the round-4 Stage 6 agent. It is the entry
> point; the detailed routing is
> [`stage-6-round4-handoff-20260919.md`](stage-6-round4-handoff-20260919.md), which
> supersedes the routing in
> [`stage-6-round3-handoff-20260919.md`](stage-6-round3-handoff-20260919.md) and
> supersedes nothing else. Rounds 1–3 are the historical record and are not edited.

---

You are the Stage 6 agent for LayerFS issue
[#171](https://github.com/Ephemeral-AI-Lab/layerfs/issues/171) (**open**), working in
`/Users/yifanxu/Ephemeral-AI-Lab/layerfs`. Parent is
[#165](https://github.com/Ephemeral-AI-Lab/layerfs/issues/165). The next child,
[#172](https://github.com/Ephemeral-AI-Lab/layerfs/issues/172) (Stage 7 runtime), is
**not yours**.

**One agent, working alone. No subagents. No codex.** Everything happens in your
session.

## Read first, in this order

1. `AGENTS.md` §1–§4 (measurement contract, reuse, budgets, production LOC, no CI)
2. `core/AGENTS.md` (product-source purity, file ceilings, the checks to run)
3. `docs/general/benchmark_rules.md` (§1 freeze the claim, §5 timing boundaries,
   §6 separate setup/performance/verification/cleanup, §7 coherent families,
   §10 honest memory attribution, §11 freeze gates before measuring, §13 evidence
   custody, §15 fast loop and terminal admission)
4. the frozen specification under
   `core/docs/benchmark/fs-bench-pro-storage-content/` (seven files)
5. the harness `core/benchmark/fs-bench-pro-storage-content/README.md`
6. `docs/roadmap/0.1/0.1.7/component-decoupling/stage-6-round4-handoff-20260919.md`
   — **your assignment**
7. round-3 evidence, in this order:
   `docs/roadmap/0.1/0.1.7/evidence/stage-6-round3-20260919T000000Z/README.md`
   (the four product fixes, the WP-5 phase split, the A/B, WP-7's real cause) then
   `docs/roadmap/0.1/0.1.7/evidence/stage-6-round3b-20260919T000000Z/README.md`
   (the closure receipt and the C2-4 measurements)

## Reproduce before you read code

```sh
H=core/benchmark/fs-bench-pro-storage-content
cargo +1.85.1 build --release --manifest-path $H/Cargo.toml --locked
cargo +1.85.1 test --locked --manifest-path $H/Cargo.toml          # 80 Rust tests
python3 -m unittest discover -s $H/shared -p 'test_*.py'           # 102 Python tests
python3 $H/runner.py self-check
python3 $H/runner.py perf --lane smoke --out /tmp/smoke
python3 $H/runner.py report --run /tmp/smoke
cargo +1.85.1 test --locked --manifest-path core/Cargo.toml        # 472 product tests
python3 core/tools/check_product_boundary.py
python3 tools/production_loc.py
```

## Where the tree is

```text
HEAD   6aee06b01  the round-3b closure receipt and this prompt
       3c9313e3e  the C2-4 workspace-reuse driver
       2b0184746  the round-3 evidence record
       1ae8dd374  the C1-3 decrease fixture correction
       0d70fc884  the mapping-origin fix
       258a3e7ad  the pack-header fix
       7d5f67cf4  the mid-wave seal fix
production LOC   84934 combined (core 19517 / reference 65417)
```

Round 3 measured **194 PASS / 0 FAIL / 26 NOT_RUN** of 220 (admission 194 / 0 / 23)
in 394.565 s, verified with 0 disagreements, call-graph PASS over 120 product files
and runtime tripwires PASS over 167 stores.

## Your work, in order

The full statements, including the counter readings you need, are in the round-4
handoff §4. In short:

1. **WP-1 — the setup/performance/verification phase split. 5 rows.** Largest
   architectural change; unblocks honest budgets for every long row. The measured
   split for `dedup-cdc-overwrite-500` is 11.545 s fixture construction, 0.021 s
   base + de-warm, 5.021 s measured, 11.985 s oracle. Moving verification into a
   second unmeasured invocation is necessary but not sufficient; fitting 15 s also
   needs preparation out of the performance invocation. **Do not shrink a tier and
   do not enlarge a timeout.**
2. **WP-2 — the `c2.pool.cold-warm` driver. 2 rows.** Configuration declared in
   `families/c2_pool.rs`: 512 leaves × 100 rows, cold and warm never pooled.
3. **WP-3 — the `pipeline.*` driver. 4 rows.** A composition of the existing C1
   drivers (`ops/c1.rs::edit`, `ops/fs.rs::fs_build`) with the C2 save path.
4. **WP-4 — the four walk-ceiling tiers.** One `build_filesystem` over 10,100 or
   101,000 bindings walks past `MAXIMUM_WALK_ENTRIES = 4096`. Grow by batches of
   ≤4,096 **new** bindings, never restating an existing directory binding.
   **Do not shrink the tier.**
5. **WP-5 — the declared-process-kill arm.** Harness-only; touches no product
   source. Record the outcome **including a failure**.
6. **WP-6 — ask the owner the one open question.** Is a retaining measured phase
   admissible for an update-shaped row? Eight `tiny-unlink`/`tiny-bulk-delete`
   rows close on the answer. Record the answer either way.
7. **WP-7 — closure.** Only after the above: a fresh full lane into a **new**
   directory, `verify`, `report`, `calibrate`, a new dated evidence directory, and
   a fresh matrix. Then re-check the nine #171 acceptance checkboxes.

## The rules that will decide your work

- **Product bug fixes are allowed; new features are not.** The test: does the
  product's own code or the frozen spec already state the behaviour that is
  failing? If yes it is a bug; if no it is out of scope. Every such commit reports
  `Production LOC: <before> -> <after> (delta <signed>)` and passes
  `cargo +1.85.1 test/clippy/fmt --manifest-path core/Cargo.toml --locked` plus
  `core/tools/check_product_boundary.py`.
- **Any product change invalidates the product seal; any harness change invalidates
  the harness identity.** Re-run into a **new** directory; never into an existing
  one. Round 3 produced two evidence directories for exactly this reason.
- **A timed phase never retains, and every oracle is a second, unmeasured,
  byte-identical operation.** The two roots must agree.
- **One sample per case per arm. Fresh output. Append-only everything.**
- **Never shrink a workload, relax a limit, inflate a timeout or add a worker to
  turn a number green.** A selection that cannot fit is `NOT_RUN` with its measured
  wall time.
- **`elapsed_ns` never gate-decides.** Counters, heap and disk do.
- **A measurement can be ruined by interference and look like a regression.**
  Re-measure a suspect row alone before believing it, and record the discarded
  reading rather than dropping it.
- **Do not re-open a Stage 5 row. Do not promote a Stage 5 `NOT_RUN` or owner-WAIVED
  row. Do not run `tools/preflight.sh`, restore it, or create an aggregate gate, CI
  workflow or wrapper.** There is no CI; verify the tree you changed with the
  commands that cover it and report exactly which ran and which did not.
- **`DECLARED_EXCEPTIONS` in `runner.py` is a hand-maintained ID list** and it has
  been wrong about itself once already. Check it against the receipts rather than
  trusting a handoff's description of it.

## Traps you will otherwise pay for

- `ChildDescriptor::cumulative_logical_end` is cumulative **within its own page**,
  not an absolute file offset. Round 3 fixed the one absolute reader.
- `OpenPack::assembled` is the canonical assembled length **including** the pack
  header; it starts at `HEADER_LEN`, not zero.
- A wave's membership snapshot is stale after any seal in that wave;
  `MutationOwner::sealed_rows` is the set that fixes it, and `seal_group` is the
  only place rows are written.
- The C1-3 `decrease` row's base is the declared chunk-dense run, not noise; a
  64 KiB noise window holds the same two-or-three extents a 64 KiB zero run does.
- `MAXIMUM_WALK_ENTRIES = 4096` is the ceiling that fires on a large single build,
  not `check_build_reachability`. The error text is
  `InvalidRecord("cycle check work limit")`.
- `pack_cache` and `pool_reader` are two different caches; invalidate both on
  write. `TreeStore` is not enough for an update chain — use
  `SharedStore`/`SharedReader` or `PairProvider`.
- `FilesystemResources::maximum_pending_records` is 4,096; above it an operation
  needs a caller-supplied `FileBacking` or it fails
  `ResourceUnavailable { what: "ordering backing" }`, which is *not* the walk
  ceiling and looks like one.
- The page size is **16,384 bytes**: read it, never hardcode it.
- `--case` overrides the lane selection entirely.
- This shell's working directory is not stable across invocations; use absolute
  paths. `timeout(1)` is not installed. zsh does not word-split unquoted variables
  (`${=ARGS}` if you build an argument list).

## Definition of done

Every #171 acceptance checkbox satisfied **by a receipt that reproduces on the
committed tree**; every one of the 217 registered cases `PASS` or recorded
`FAIL`/`INCOMPLETE`/`INELIGIBLE`/`NOT_RUN` with its measured state and a written
reason; every commit's production LOC reproducible; and **#171 closed** with a
final comment naming the tree, the receipts and the matrices. Round 3 left
checkboxes 1, 2 and 5 PARTIAL, so **#171 stays open until every one holds.**
