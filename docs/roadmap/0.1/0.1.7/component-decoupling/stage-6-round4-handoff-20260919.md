# Stage 6 round-4 handoff — 2026-09-19

> **Status:** Active implementation routing. This is the executable assignment for
> the successor Stage 6 agent, written at the end of round 3. It **supersedes the
> routing** in [`stage-6-round3-handoff-20260919.md`](stage-6-round3-handoff-20260919.md)
> — that document's rules still bind **except where §4 below amends them** — and it
> supersedes nothing else. Rounds 1, 2 and 3 are the historical record and are not
> edited.
>
> **Issue:** [#171](https://github.com/Ephemeral-AI-Lab/layerfs/issues/171) — **open**.
> Parent [#165](https://github.com/Ephemeral-AI-Lab/layerfs/issues/165). Next child is
> [#172](https://github.com/Ephemeral-AI-Lab/layerfs/issues/172) (Stage 7) — **not yours**.
>
> **How you work:** one agent, working alone. **No subagents. No codex.** Everything
> happens in your session.
>
> **Entry point:** [`stage-6-round4-prompt.md`](stage-6-round4-prompt.md) is the
> paste-ready prompt that dispatches this document. Start a successor session with
> that file; use this one for the detail.

## 1. Where the tree actually is

```text
HEAD   3c9313e3e  the C2-4 workspace-reuse driver
       2b0184746  the round-3 evidence record
       1ae8dd374  the C1-3 decrease fixture correction
       0d70fc884  the mapping-origin fix
       258a3e7ad  the pack-header fix
       7d5f67cf4  the mid-wave seal fix
        d2035dfd8  the round-2 evidence record and the round-3 handoff
production LOC   84934 combined (core 19517 / reference 65417)
```

Reproduce before reading code:

```sh
H=core/benchmark/fs-bench-pro-storage-content
cargo +1.85.1 build --release --manifest-path $H/Cargo.toml --locked
cargo +1.85.1 test --locked --manifest-path $H/Cargo.toml          # 80 Rust tests
python3 -m unittest discover -s $H/shared -p 'test_*.py'           # 102 Python tests
python3 $H/runner.py self-check
python3 $H/runner.py perf --lane smoke --out /tmp/smoke
cargo +1.85.1 test --locked --manifest-path core/Cargo.toml        # 472 product tests
```

## 2. What round 3 measured

`runner.py perf --lane full` — 220 cases in **394.565 s**, into a **new** directory
at commit `3c9313e3e`:

| Class | Rows | PASS | FAIL | NOT_RUN |
| --- | ---: | ---: | ---: | ---: |
| Admission (the frozen 217) | 217 | **194** | **0** | 23 |
| Diagnostic (`component.primitives`) | 3 | 0 | 0 | 3 |
| **Total registered** | **220** | **194** | **0** | **26** |

Round 1 was 122 / 3 / 95; round 2 was 167 / 3 / 50. `runner.py verify` re-derived
all 220 statuses with **0 disagreements**, sealed call-graph **PASS** over 120
product files, runtime tripwires **PASS** over **167** stores.

Round 3 fixed **four product defects** and **one harness fixture defect**, and
implemented the **C2-4 driver**. Read
[`../evidence/stage-6-round3-20260919T000000Z/README.md`](../evidence/stage-6-round3-20260919T000000Z/README.md)
for the four product fixes (each with a regression test confirmed to reproduce the
original failure against the previous source) and
[`../evidence/stage-6-round3b-20260919T000000Z/README.md`](../evidence/stage-6-round3b-20260919T000000Z/README.md)
for the closure receipt and the C2-4 measurements.

**The 23 admission rows that are not `PASS`.**

| Rows | Cause |
| ---: | --- |
| 8 | `tiny-unlink` / `tiny-bulk-delete` — an update reads back what it emits |
| 5 | `dedup-cdc-{overwrite,insert,delete,scattered,common-body}-500` — 28.4–29.6 s vs a 15 s limit |
| 4 | `namespace-{10000,100000}[-text-v1]` — `InvalidRecord("cycle check work limit")` |
| 4 | `pipeline.*` — no driver |
| 2 | `c2.pool.cold-warm` — no driver |

## 3. The three corrections round 3 makes to the round-3 handoff

1. **The budget rows were never declared.** The round-3 handoff says the five
   `dedup-cdc-*-500` rows "measure 27.98–29.63 s against the 25 s declared
   exception". `runner.py:DECLARED_EXCEPTIONS` holds seven IDs and none of them is
   a dedup row; round 2's own `run-full.json` records exactly those seven. Both
   rounds classify these rows against the **15 s** limit. Do not repeat the 25 s
   premise.
2. **WP-5's prescription is necessary but not sufficient.** Measured split for
   `dedup-cdc-overwrite-500`: fixture construction **11.545 s**, base + de-warm
   0.021 s, measured phase **5.021 s**, oracle replay + read-back **11.985 s**.
   Moving verification into a second unmeasured `verify`-mode invocation — what
   the handoff asks for and what `benchmark_rules.md` §6 requires — leaves
   ~16.5 s, still above 15 s. Fitting 15 s also requires moving fixture
   construction out of the performance invocation, which §6 and owner decision D2
   already ask for. That is a phase-boundary change touching every driver.
3. **WP-7's failure is not the build ceiling.** It is
   `MAXIMUM_WALK_ENTRIES = 4096` (`filesystem/validate.rs`), reached by one
   `build_filesystem` over 10,100 and 101,000 bindings. A single build states its
   own bindings, so the batched-growth route the handoff describes is the right
   shape; the *ceiling* it names is not the one that fires.

## 4. The work, in the order that unblocks the most

### WP-1 — WP-5, the phase split. **5 rows.** Do this first; it is the largest
architectural change and it unblocks honest budgets for every long row.

The child must separate **preparation**, **performance** and **verification** into
separate invocations or separate measured scopes, per `benchmark_rules.md` §6 and
D2. The complete-command budget then covers the performance invocation, and
preparation is published as its own field (`acquisition_wall_ns`), which the
harness already declares the concept for (`prepare`, `--emit-input`/`--load-input`,
`D2`).

The minimal shape that fits the five rows: the `perf` invocation runs the measured
phase against a fixture that was acquired once and reused, and the replay and
read-back move to a second, unmeasured `verify`-mode invocation whose wall is
charged to the 60 s verification budget. **Do not shrink a tier and do not enlarge
a timeout.** If the numbers still do not fit 15 s, the honest outcome is `NOT_RUN`
with the measured wall time plus an owner question about declaring the rows — the
round-3 handoff already says a longer exception is an owner decision.

### WP-2 — `c2.pool.cold-warm`. **2 rows.** No driver.

`Shape::Pool { cold }` is `Unimplemented("pooled-lane")`. The case configuration is
declared in `families/c2_pool.rs`: **512 leaves × 100 rows**, cold and warm
reported separately and never pooled. The counters the specification names are the
`metadata_value_groups` catalogue and `Store.pool_index`; the public accessors are
`Store::pool_index_entries` and `Store::pool_index_bytes`. The objects are pooled
inode leaves (`ObjectRole::InodeLeaf`), which the filesystem builder emits — so the
fixture needs a build large enough to produce 512 pooled leaves, and
`FilesystemResources::maximum_pending_records` is 4,096, so above that the
operation needs a caller-supplied `FileBacking` or it fails
`ResourceUnavailable { what: "ordering backing" }` — which is *not* the walk
ceiling and looks like one.

### WP-3 — `pipeline.*`. **4 rows.** No driver.

`Shape::Pipeline(_)` is `Unimplemented("pipeline")`. `PipelineOp::{EditsSmall,
EditsChunked, EditsLargeToSmall, FilesystemBuild}` each need C1 construction plus
C2 save acknowledgement in one region. The C1 drivers already exist
(`ops/c1.rs::edit`, `ops/fs.rs::fs_build`) and the C2 save path is
`ops/c2.rs`; this is a composition of two existing halves, not new machinery.

### WP-4 — the four walk-ceiling tiers. **4 rows.**

One `build_filesystem` over 10,100 or 101,000 bindings walks past
`MAXIMUM_WALK_ENTRIES = 4096`. The route is proven: growth by **new** files and
**new** directories never charges the whole-tree walk, because
`check_parent_aliases` only walks when a batch rebinds an existing directory or
symlink. Build 4,096 bindings, then add in batches of ≤4,096 new bindings, never
restating an existing directory binding. Declare the multi-operation structure in
the row's notes. **Do not shrink the tier.** Note that the 100,000 tier carries
500 MB of content; measure the complete command before promising it fits.

### WP-5 — WP-8, the process-kill arm. **One arm.**

A declared wait state in the **harness** child is ordinary harness work: it touches
no product source, no hook, no feature flag and no fault-injection surface in
`core/crates/*/src`. Implement it, record the outcome **including a failure**, and
keep W1 and W3 green. This is the only part of acceptance checkbox 5 that has never
run.

### WP-6 — §5's owner question. **8 rows, and it is a question, not work.**

A filesystem **build** runs to completion against a non-retaining consumer and an
empty provider — measured. A filesystem **update** does not: it demands an object
it emitted earlier in the same operation, so a `DiscardingConsumer` cannot serve
it. The overlay is already written and proven in `src/workload/providers.rs`
(`SharedStore`/`SharedReader`) — it is what makes the unmeasured replay of those
same inputs succeed — but using it inside `measure_update` puts a retaining
consumer inside a timed phase, which the binding rules forbid. **Ask the owner
whether a retaining measured phase is admissible for an update-shaped row, and
record the answer either way.**

### WP-7 — closure.

Only after the above: a fresh full lane into a **new** directory, `verify`,
`report`, `calibrate`, a new dated evidence directory, and a fresh matrix. Then
re-check the nine #171 acceptance checkboxes. Round 3 left **1, 2 and 5 partial**;
**#171 stays open until every one holds.**

## 5. Traps, so you do not pay for them again

Rounds 1–3's traps all still apply. These are the round-3 additions.

1. **`MAXIMUM_WALK_ENTRIES = 4096` is the ceiling that fires on a large single
   build, not `check_build_reachability`.** They are different checks with the same
   numeric value, and the error text is `InvalidRecord("cycle check work limit")`.
2. **`ChildDescriptor::cumulative_logical_end` is cumulative within its own page.**
   It is not an absolute file offset. `traverse` was the only absolute reader and
   round 3 fixed it; if you add a second reader, rebase on the page origin.
   `types.rs`, `build.rs`, `child_summaries` and `edit::tree::split` are all
   page-relative and correct.
3. **`OpenPack::assembled` is the canonical assembled length including the pack
   header.** It starts at `HEADER_LEN`, not zero. A total without the header lets a
   pack assemble up to 16 bytes past `PACK_LIMIT` and then refuses itself.
4. **A wave's membership snapshot is stale after any seal in that wave.**
   `MutationOwner::sealed_rows` is the set that fixes it; `seal_group` is the only
   place rows are written, so recording there covers every seal path. The set is
   cleared per wave and is bounded by the members one wave can seal.
5. **The C1-3 `decrease` row's base is the declared chunk-dense run, not noise.**
   A 64 KiB noise window holds two or three extents under this profile, which is
   what a 64 KiB zero run also chunks to, so the direction was a coin flip.
   `tests/fixture_declarations.rs` fails if the frozen profile stops cutting the
   declared pair at its minimum.
6. **A measurement batch can be ruined by interference and look like a
   regression.** A five-row batch reported 30–72 s for `dedup-cdc-overwrite-500`;
   the same row measured alone is 29.0 s, and a controlled A/B against the round-2
   commit gives 29.321 s. Re-measure a suspect row alone before believing it, and
   record the discarded reading rather than dropping it.
7. **`--case` overrides the lane selection entirely**; `DECLARED_EXCEPTIONS` is a
   hand-maintained ID list that must stay in sync with the case IDs, and it has
   been wrong about itself once already (§3.1).
8. `pack_cache` and `pool_reader` are two different caches; invalidate both on
   write. `TreeStore` is not enough for an update chain — use
   `SharedStore`/`SharedReader` or `PairProvider`. The page size here is
   **16,384 bytes**: read it, never hardcode it. This shell's working directory is
   not stable across invocations; use absolute paths.

## 6. Rules that bind you, restated because they are the ones this stage leans on

- **Product bug fixes are allowed; new features are not.** The test: does the
  product's own code or the frozen spec already state the behaviour that is
  failing? Production LOC delta is reported on every commit.
- **A timed phase never retains, and every oracle is a second, unmeasured,
  byte-identical operation.** The two roots must agree.
- **One sample per case per arm. Fresh output. Append-only everything.**
- **Never shrink a workload, relax a limit, inflate a timeout or add a worker to
  turn a number green.** A selection that cannot fit is `NOT_RUN` with its measured
  wall time.
- **`elapsed_ns` never gate-decides.** Counters, heap and disk do.
- **Do not re-open a Stage 5 row. Do not promote a Stage 5 `NOT_RUN` or owner-WAIVED
  row. Do not run `tools/preflight.sh`, restore it, or create an aggregate gate or CI
  replacement.**
