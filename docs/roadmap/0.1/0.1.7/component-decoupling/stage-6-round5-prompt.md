# Stage 6 round 5 — successor prompt

> **Status:** Paste-ready prompt for the round-5 agent. It is the entry point. The
> detailed routing is
> [`stage-6-round5-implementation-plan.md`](stage-6-round5-implementation-plan.md) and
> the tracked assignment is [#184](https://github.com/Ephemeral-AI-Lab/layerfs/issues/184).
> Stage 6's acceptance issue
> [#171](https://github.com/Ephemeral-AI-Lab/layerfs/issues/171) is **closed**; do not
> reopen it. Rounds 1–4 are the historical record and are not edited.

---

You are the Stage 6 round-5 agent for LayerFS, working in
`/Users/yifanxu/Ephemeral-AI-Lab/layerfs`. Your assignment is
[#184](https://github.com/Ephemeral-AI-Lab/layerfs/issues/184): **make the benchmark
campaign cheap and its numbers honest.**

**One agent, working alone. No subagents. No codex.** Everything happens in your session.

## Read first, in this order

1. `AGENTS.md` §1–§4 (measurement contract, reuse, budgets, production LOC, no CI)
2. `core/AGENTS.md` (product-source purity, file ceilings, the checks to run)
3. `docs/general/benchmark_rules.md` — especially §6 (separate setup / performance /
   verification / cleanup) and §15 (fast loop and terminal admission)
4. the frozen specification under `core/docs/benchmark/fs-bench-pro-storage-content/`
   — especially `test_setup_and_cache_discipline.md` §2.2 (what is inside the timer,
   per case shape), §2.1 (why preparation is mandatory, not an optimisation) and §3
   (the prepared-master build order, layout and manifest), and
   `gates_and_oracles.md` §4–§5 (the per-family oracle)
5. the harness `core/benchmark/fs-bench-pro-storage-content/README.md`, including its
   closing "What is not yet true here" section
6. [#184](https://github.com/Ephemeral-AI-Lab/layerfs/issues/184) and
   [`stage-6-round5-implementation-plan.md`](stage-6-round5-implementation-plan.md)
7. the round-4 evidence, which is the baseline you are improving:
   [`../evidence/stage-6-round4c-20260919T000000Z/`](../evidence/stage-6-round4c-20260919T000000Z/README.md),
   then `stage-6-round4-20260919T000000Z/` for the phase split and the kill arm

## Where the tree is

```text
HEAD   e613c5c0a  the round-5 re-basing and this prompt
       39627f3cc  the round-4c closure receipt
       90bbb617d  the removal rows' O4 oracle        <- the round-4c run's tree
       6435cf429  fix: an unbound name is a logical absence
production LOC   84936 combined (core 19519 / reference 65417)
```

Stage 6's acceptance is met: **217 of 217 admission rows `PASS`, 0 `FAIL`**, lane
**415.821 s**, all nine #171 checkboxes satisfied, W1/W2/W3/W4 green. Your work does
**not** change any verdict — every budget already passes (largest complete command
9.952 s against 15 s). It changes what the campaign **costs** and what its numbers
**mean**.

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
```

## The model you are implementing

Four phases, published separately in every receipt. **Real work is the golden number.**

| | phase | field | role |
| --- | --- | --- | --- |
| a | preparation | `preparation_wall_ns` (+ `acquisition_wall_ns`) | published, **not** the golden number |
| b | **real work** | **`operation_ns`** | **the golden benchmark number** — the report's primary axis, tracked round to round, must not fall |
| c | verification | `verification_wall_ns` | **skipable**; its own 60 s budget |
| d | cleanup | `cleanup_wall_ns` | published and bounded; not skipable, not the golden number |
| | process wall | `complete_command_ns` | the lifecycle/cleanup ceiling |
| | harness work inside the timer | `handoff_ns` | so harness overhead is visible rather than absorbed into the golden number |

`verify` must re-derive all of these and **fail closed** if they do not reconcile with
the wall inside a declared tolerance. Publish `sum(operation_ns)` for the lane as well
as per row, so a row that got faster at another row's expense is visible.

## How to simplify — the three mechanisms, in order

### S1. Verify what the frozen oracle requires, and not more

`gates_and_oracles.md` §5 freezes a per-family oracle. Four C2 families — the four with
the largest verification cost — do a byte-exact read-back their oracle **does not ask
for**:

| family | frozen oracle | today |
| --- | --- | --- |
| `c2.delta.cdc-locality` | **O1 + O3** | full read-back: 12.125 s, not required |
| `c2.reuse.workspace` | **O1 + O5** | full read-back, not required |
| `c2.footprint` | **O6 + O1** | full read-back, not required |
| `c2.pool.cold-warm` | **O1 + O3** | full read-back, not required |

Families whose oracle **does** include O2 — `c1.construct.*`, `c1.edit.*`,
`pipeline.*`, `c2.read.waves` — keep their read-back. `c1.many-tiny` already uses the
specification's frozen `TreeSample`.

Removing the unrequired read-back is **compliance, not relaxation**: no contract change,
no new stamp, no sampling. But it requires the oracle those families *do* need, which
may be missing:

- **O1 for real.** The drivers gate "replay root == measured root", which is
  self-consistency. O1 is *"expected root `ObjectId`, from a frozen constant or
  recomputed off the product path"* — so **pin the expected result root per case**. It
  is deterministic from the recipe and commits to the whole tree transitively, because
  the root identity digests canonical bytes that reference children by id.
- **O3 for real.** The delta counters are written but no gate compares them to
  **pinned** values, which is what "chunk counts match" means.

The specification also sanctions the cheap oracle for an expensive family (§4): *"the
parity set stays green plus the pinned identity constants match"*, with the
sealed-oracle parity set pinned at **35** external tests.

### S2. Where O2 is required, verify each distinct object once

`dedup-cdc-overwrite-500` accepts **110,022 occurrences over ~3,665 distinct objects** —
30x structural redundancy. Split the read-back into two claims that were conflated:

1. **store integrity** — decode each *distinct* object once and check it re-identifies
   to the bytes its name claims;
2. **traversal correctness** — per member, check the mapping tree resolves to the right
   `(object, offset, length)` sequence, and assemble that member's digest from the
   verified payload cache.

That is **complete** and about a fifth of the cost, against a 10% sample that proves
10%.

### S3. Stop hashing what the harness already owns

`Expectation::of` hashes the fixture bytes. `delta_members` does it in preparation and
`workspace`'s oracle loop repeats it over the same 2.10 GB. `src/workload/artifact.rs`
already persists expectations for the one split family — extend that to every family
that builds one.

Then **S4** `--reuse-pass` removes the phase entirely for an unchanged case, and **S5**
the deterministic 10% quick mode makes iteration cheap while proving only 10%.

## The breakdown

| # | item | depends on | acceptance |
| --- | --- | --- | --- |
| V1 | Publish the six phase fields; `verify` re-derives them | — | every receipt carries all six; reconciliation fails closed |
| V2 | Pin the expected result root per case (O1) | V1 | the gate compares a pinned constant, not a replay |
| V3 | Pin the counts (O3) | V1 | counts gated against pinned values |
| V4 | Remove the unrequired O2 from the four families | V2, V3 | every counter identical to round 4c; verification drops |
| V5 | Deduplicate the required O2 by distinct object | V4 | every counter identical; <= 5 s for the big rows |
| V6 | Persist expectation digests for every family | V1 | no `Expectation::of` over fixture bytes in any phase |
| V7 | `--reuse-pass` | V1 | fails closed on schema/identity/hard-limit/wall mismatch; records `reused_proof_identities` and an omission |
| V8 | Mode ladder `full` / `sample` / `none`, deterministic 10% | V1–V7 | sampled and omitted rows are `INCOMPLETE`, never `PASS` |
| V9 | Prepared masters for the remaining fixture-heavy families | V1 | every one acquires once per digest; `prepare --lane full <= 90 s` |
| V10 | Pack the object set into one file instead of one per object | V9 | load `<= 1.0 s` per row |
| V11 | Remove the per-member base clone and the double allocation | V9 | preparation `<= 1.0 s` per row |
| V12 | Digest key = product identity + fixture-recipe version | V9 | a measurement-plumbing-only harness change does **not** invalidate a master |
| V13 | Remove the per-object `cloned_object` handoff, or declare `handoff_ns` | V1 | the golden number is product work or the tax is published |

Order: **V1 first** — it is mechanical, it needs no cache, and it produces the number
that shows whether everything else worked. Then V2–V6 (the oracle), then V9–V13
(preparation), then V7–V8 (the modes).

## The simplification target

| line | round-4c | target | verified by |
| --- | ---: | ---: | --- |
| per-row preparation | 0.03–11.95 s | **<= 1.0 s** | `preparation_wall_ns` |
| lane preparation | ~180 s [D] | **<= 25 s** | `sum(preparation_wall_ns)` |
| `prepare --lane full`, once per digest | ~180 s+ [D] | **<= 90 s** | the prepare manifest |
| largest single verification invocation | 12.7 s [M] | **<= 5 s** | `verification_wall_ns` |
| lane verification | ~180 s [D] | **<= 108 s** (60%) | `sum(verification_wall_ns)` |
| per-row cleanup | not measured | **<= 0.5 s** | `cleanup_wall_ns` |
| **lane, warm, full verification** | **415.821 s** [M] | **<= 200 s** | the run wall |
| lane, warm, quick mode | — | **<= 70 s** | the run wall |
| **operation (golden)** | **unknown** | **published, and must not fall** | `sum(operation_ns)` |
| budgets | already pass (max 9.952 s) | keep passing | no tier shrunk |

The lane numbers are **conditional on publishing the operation total**: the ~180 s /
~180 s preparation-versus-verification split is derived from a 50/50 assumption anchored
on **one** round-3b row, and the round-4c lane has ten rows the round-3b lane never ran.
**Judge your work on the per-row targets**, which are the robust ones.

## The rules that will decide your work

- **No product source change.** This is `core/benchmark/**` plus harness documentation.
  If a step appears to need one, stop and report it. The one exception already scoped
  is `FilesystemRead::inode` (see below) — take a ruling, do not assume.
- **`operation_ns` must not fall.** T4 is a falsifier: if the golden number improves,
  measured work has moved into setup and the change is **rejected**. This is the single
  most important guard in the round.
- **Never shrink a workload, relax a limit, inflate a timeout or add a worker.** The
  budgets already pass; there is nothing to make fit.
- **A timed phase never retains, and a prepared master is preparation reuse, never a
  cold claim.** `prepared-dewarmed` rows still de-warm and still gate
  `resident_pages == 0` and `disk_read_bytes`.
- **A sampled or omitted row is `INCOMPLETE`, never `PASS`**, so an iteration run cannot
  be mistaken for admission evidence.
- **Fresh output, append-only everything.** A harness change invalidates the harness
  identity; re-run into a **new** directory.
- **No aggregate gate, no CI workflow, no wrapper.** `tools/preflight.sh` stays retired.
- **Verify the tree you changed and report exactly which checks ran and which did not.**

## Rulings (owner, 2026-09-19) — do not re-open these

1. **The golden number reports; it does not gate.** `operation_ns` is the report's
   primary axis, the number compared across rows and tracked round to round, and the one
   number that must not fall. It does **not** decide a gate and does **not** drive the
   budget: D1's rationale is a **+17.6%** same-binary spread that cannot separate O(n)
   from O(log n), so counters, heap and disk keep deciding. Do not make `elapsed_ns`
   gate-decide; that would be a `CONTRACT.md` change and it is not this round's.
2. **`FilesystemRead::inode` is in scope, but not without its own confirmation.** It also
   reports `MissingObject` for a serial the inode table does not hold. Round 4c
   deliberately left it, because in the replacement a serial the table lacks *after* the
   directory page named it is a torn tree rather than an absent path. It is scoped into
   #184 as its own item: confirm which class it is, with a test that pins both sides,
   before changing product source — and report a blocker rather than guessing.
3. **The digest key is the product identity plus a declared fixture-recipe version.**
   Record the producer binary in the manifest as provenance and validate it on load, but
   do **not** put it in the key. Putting it in the key means every harness edit
   invalidates all masters, you are permanently cold, and the whole round buys nothing.

## Traps you will otherwise pay for

- `TreeStore::write_to_dir` / `load_from_dir` is **lossy** — it writes only canonical
  bytes and re-wraps everything as `ObjectRole::Chunk` with no references and no
  predecessors. `src/workload/artifact.rs` is the lossless format; do not go back.
- The artifact layout, manifest schema and immutability step are **already fixed** by
  `test_setup_and_cache_discipline.md` §3 (`seal: chmod removes 0o222`). Follow the
  spec, not a document that invents its own.
- Warm preparation is floored by *"read the object set and hash it"* —
  `FinalizedObject::new` hashes on load, and lazy loading would break the declared
  `warm-in-process-fixture` cache state. Expect T1 to be **tight for the C1 edit 500 MiB
  rows** and comfortable for the delta rows (artifacts 14.9–119.9 MB measured).
- `PHASE_SPLIT_FAMILIES` in `runner.py` is a hand-maintained family set that must stay
  in sync with the drivers' own `oracle_phase` declarations. `DECLARED_EXCEPTIONS` is a
  hand-maintained ID list and has already been wrong about itself once.
- The report's `time max ms` column is `max(row.wall_ns)` — the complete-command wall.
  Read `timing.json` for operation time.
- `MAXIMUM_WALK_ENTRIES = 4096` is the ceiling a large single build trips, not
  `check_build_reachability`; the error text is
  `InvalidRecord("cycle check work limit")`.
- `ChildDescriptor::cumulative_logical_end` is cumulative **within its own page**, not
  an absolute file offset. `OpenPack::assembled` **includes** the pack header. A wave's
  membership snapshot is stale after any seal in that wave (`MutationOwner::sealed_rows`).
- `pack_cache` and `pool_reader` are two different caches. `TreeStore` is not enough for
  an update chain — use `SharedStore`/`SharedReader` or `PairProvider`.
- `FilesystemResources::maximum_pending_records` is 4,096; above it an operation needs a
  caller-supplied `FileBacking` or it fails
  `ResourceUnavailable { what: "ordering backing" }`, which is *not* the walk ceiling.
- The page size is **16,384 bytes** — read it, never hardcode it. This shell's working
  directory is not stable across invocations; use absolute paths. `timeout(1)` is not
  installed. zsh does not word-split unquoted variables (`${=ARGS}`).

## Definition of done

All six phase fields published and re-derived; the golden number published and
**unchanged**; the unrequired read-back gone and the required one deduplicated, with
every counter identical to round 4c; preparation `<= 1.0 s` per row; `--reuse-pass` and
the mode ladder working and failing closed; a fresh full lane into a **new** directory
with `verify`, `report`, `calibrate` and a new dated round-5 evidence directory; the
before/after non-operation share in one table; and the #184 checkboxes all holding.
