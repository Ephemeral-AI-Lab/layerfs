# Stage 6 measurement harness — C1/C2 structural complexity

The executable half of [#171](https://github.com/Ephemeral-AI-Lab/layerfs/issues/171).
It measures `layerfs-content` (C1) and `layerfs-storage` (C2) **through their public
APIs only**, against the frozen case registry in
[`core/docs/benchmark/fs-bench-pro-storage-content/`](../../docs/benchmark/fs-bench-pro-storage-content/CONTRACT.md).

```text
claim_kind = structural-complexity
```

The question is whether the C1/C2 core is algorithmically sound — correct at every
declared boundary, scaling as its architecture claims, and bounded in memory, CPU
and space. It is **not** a comparison against v0.1.6, and no receipt here supports
one. Every gate is absolute and single-arm, and `elapsed_ns` never decides a gate.

## How to run

```sh
H=core/benchmark/fs-bench-pro-storage-content

python3 $H/runner.py self-check                 # every Python self-check + registry + lock parity
python3 $H/runner.py list --lane smoke          # the twenty-case development lane
python3 $H/runner.py perf  --lane smoke --out /tmp/run-smoke
python3 $H/runner.py perf  --lane full  --out /tmp/run-full
python3 $H/runner.py verify --run /tmp/run-smoke
python3 $H/runner.py report --run /tmp/run-smoke
python3 $H/runner.py calibrate --out /tmp/run-smoke   # E1-E4, untimed
```

`perf` builds with `cargo +1.85.1 build --release --locked`, exports
`LAYERFS_CONSTRUCTION_WORKERS=1` itself, holds the measurement lock for the whole
invocation, and **refuses an existing `--out` path**. Every case writes its raw
trace; the runner derives a receipt from it and never edits either.

## What each verb guarantees

| Verb | Guarantee |
| --- | --- |
| `list` | Prints the binary's own generated registry, so what is listed is what is registered. |
| `prepare` | Acquires the prepared artifacts a selection needs, once. A family that declares a phase split is acquired for real, into `prepared/<case_id>/`, sealed with the producing harness digest; a family that does not is recorded `not-produced` **with that reason** rather than faked. The family is read from the binary's own registry, so no family list is maintained by hand. |
| `perf` | One sample per case per arm; fresh output; measurement lock held; complete-command budget enforced per case; a receipt that names the tree it ran on. A row whose driver declares `oracle_phase: verify-invocation` is run in phases: the performance invocation is budgeted on its own complete command, and verification is a **second, unmeasured invocation** charged to its own 60 s budget. |
| `verify` | Re-reads the raw artifacts and re-derives flatness, sequence, worst-gate aggregation, budget classification, and — for C2 rows — the space and pack accounting read out of the Store file itself. |
| `report` | Renders the ladders, bands and the four-axis view. Time is printed and never decides. |
| `self-check` | Runs every Python self-check, the registry self-check, the golden comparison and the lock-parity test. |
| `calibrate` | Runs the untimed experiments E1-E4 and records each outcome, refutations included. |

## The four measurement phases

`benchmark_rules.md` §6 requires setup, performance, verification and cleanup to use
separate timing and resource scopes. The harness names them:

| | phase | what runs | state |
| --- | --- | --- | --- |
| a | preparation | acquire the fixture — build it, or copy/load a prepared artifact — and de-warm | partial: acquired per case for the families that declare a phase split, with `acquisition_wall_ns` recorded |
| b | **work** | the operation the row claims | measured; `timing.json` is the product's own tree |
| c | verification | the oracle: a second, byte-identical operation and its read-back | a separate unmeasured invocation for rows that declare it, with its own 60 s budget |
| d | cleanup | destroy the per-case copy, close | not yet separated |

**What is inside the timer is fixed per case shape** by
`test_setup_and_cache_discipline.md` §2.2 — a driver does not choose it:

| shape | inside the timer |
| --- | --- |
| `c1.construct.*` | `construct_bytes` over a slice |
| `c1.edit.*` | `apply_edits` — base read through the provider |
| `c1.fs.*` | `update_filesystem` (+ `Store::open` in pipeline mode) |
| `c2.save.*` (fresh run, no base) | `Store::create` + save |
| `c2.reuse.*`, `c2.delta.*`, `c2.pool.*` | `Store::open` on a sample copy + save |
| `c2.read.waves` | `Store::open` on a sample copy + read wave |
| `pipeline.*` | `update_filesystem` + `Store::open` + save + ack |

So a fixture is an **input**, never part of the measurement. `c2.delta.*` does not
measure the chunking of its members; `c1.edit.*` does not measure the construction of
its base; even in `pipeline.*`, the base is setup and the measured C1 half is the edit.
A driver that builds its fixture inside the timer is measuring setup, which is why a
row handed no artifact fails closed with that reason instead of silently rebuilding.

**A half-built acquisition is never consumed.** An artifact directory that is present
but unsealed is refused with its reason, so an interrupted acquisition cannot be
mistaken for a prepared one.

## Evidence layout

```text
benchmark-results/fs-bench-pro-storage-content/   gitignored, development runs
  prepared/<case_id>/          the acquired artifact: objects/, base.sqlite, members.tsv,
                               objects.tsv, sealed.tsv, acquisition/
  prepared/manifest-<stamp>.json   append-only acquisition record
  <run>/<case_id>/
    timing.json      byte-verbatim product receipt (never edited)
    trace.jsonl      the harness trace, flat layerfs-trace-v1
    receipt.json     derived: identity, gates, statuses, counters, budget
  <run>/run.json       selection, identity, tally
  <run>/manifest.json  every retained file, hashed
  <run>/verification.json  the re-derivation, append-only
  <run>/report.txt     the four-axis human view
docs/roadmap/0.1/0.1.7/evidence/<stamp>/   admission evidence, append-only
```

## The measurement rules this harness enforces

**A timed phase pays for its own work from a declared cache state.** Every row
declares one of `warm-in-process-fixture`, `prepared-dewarmed` or
`created-in-sample`; states are never pooled. A row claiming a de-warmed read
carries `resident_pages == 0` and `disk_read_bytes >= 0.9 x requested`, because
`mincore` alone cannot distinguish a cache-served read from a device read.

**Timed phases do not retain.** Every measured phase runs with a non-retaining
consumer, so no harness allocation inside the heap window can be charged to the
product, and every oracle is a **second, unmeasured, byte-identical operation**
into a `TreeStore`. The two roots must match: a replay that failed to reproduce the
measured operation is caught by a gate rather than trusted.

**The counted instrument is the allocator, not RSS.** The counting `GlobalAlloc`
gates the O(1)-memory claim; the 10 ms RSS sampler is a bound and an anomaly
detector, because at that interval it cannot cover any phase under ~200 ms. A
missed sample or an excessive gap makes the phase peak unavailable and the row
`INELIGIBLE` — never quietly fast.

**One clock.** `CLOCK_MONOTONIC_RAW` (id 4), Rust and Python alike. `Sigma self_ns
== root.elapsed_ns` is a tautology of the product's own tree arithmetic and is
**not** presented as an `attach` detector; what `window.rs` checks is containment,
sibling non-overlap and root enclosure, which can actually fail.

**Nothing is written to product source.** No counter, hook, accessor, feature flag
or visibility change — `Store::path()` is public already, which is why no
`Store::size()` exists. All resource observation lives here.

## Registry

220 rows = **217 admission + 3 diagnostic**. `component.primitives` is registered,
runs and is receipted, and is excluded from admission and from every count: under
`structural-complexity` its receipt is diagnostic and cannot gate.

`--smoke` is one tier per family: twenty cases. `--smoke` is twenty and not
twenty-one because `c2.delta.boundaries` is a registered sub-lane of a family, not
a family, and `pipeline.*` is a registry group rather than a family.

### Two readings this harness had to fix, recorded rather than assumed

* **The bracketed profile list is one case.** `c1-families.md` section 3.1 writes
  `[-compact-v2|-mixed-v4]`; the frozen parsing rule says that is one case rendered
  with a tier-selected profile. `families::profile_for_tier` fixes which variant
  each tier takes, and `tests/golden/registry.tsv` names the profile every row
  actually got, so the ruling is visible in a diff.
* **C1-1 and C1-2 share one ID list.** Case IDs are globally unique, so the second
  family's IDs carry a `-chunked` infix, and the families differ by construction
  entry point: `construct_bytes` (the payload is an in-memory slice) against
  `construct_stream` (the payload arrives through `impl Read`). Both run the same
  1/10/100/500 MiB ladder, and the route each row took is a gate, not a label.

## Statuses a row can carry

`PASS`, `FAIL`, `TARGET_MISS`, `INCOMPLETE`, `INELIGIBLE`, `NOT_RUN`. A row whose
driver does not exist yet is `NOT_RUN` with the driver named — never `PASS`, and
never silently dropped from the report.

## Reuse

The harness keeps its own lockfile, guarded by `shared/test_lock_parity.py`: every
entry the harness lock carries must match an identical `(version, checksum)` entry
in `core/Cargo.lock`, a one-sided checksum is refused, and a harness-only registry
package is refused. On the first run it found twelve shared packages that had
resolved ahead of the product seal; every one was pinned back with
`cargo update --precise`, and the comparison now reports 46 shared entries, 0
mismatches, and 0 product entries the harness does not link.


## What is not yet true here — see [#184](https://github.com/Ephemeral-AI-Lab/layerfs/issues/184)

Assigned to Stage 6 round 5, with the executable plan in
[`stage-6-round5-implementation-plan.md`](../../../docs/roadmap/0.1/0.1.7/component-decoupling/stage-6-round5-implementation-plan.md).
Recorded here so a reader is not misled by the sections above:

- **`operation_ns` is not published.** The budget still classifies the process wall, so
  preparation and verification are billed to a performance budget; there is no
  `preparation_wall_ns`, `verification_wall_ns`, `cleanup_wall_ns` or
  `complete_command_ns`. Measured on the round-3b lane: 394.57 s of wall for 39.15 s of
  operation.
- **The phase split covers one family**, `c2.delta.cdc-locality`. The other fixture-heavy
  families still build their fixture inside their single invocation, which is why 90% of
  that lane is not measured operation.
- **`--reuse-pass` does not exist**, although `AGENTS.md` §2 mandates it, and neither do
  the verification modes `full` / `sample` / `none`. A sampled or omitted row will be
  `INCOMPLETE`, **never `PASS`**, so an iteration run cannot be mistaken for admission
  evidence.
- **The report's `time max ms` column is the complete-command wall**, not the operation.
  Read `timing.json` for operation time.
- **56 of 217 passing rows publish no `timing.json`** at all, because `ops/fs.rs` does
  not call `write_timing`: `c1.many-tiny` 20, `c1.tree.construct-traverse` 12,
  `c1.change-locality` 12, `c1.fs.build-scale` 8, `c1.tree.namespace-mutation` 4.
  The figure was 44 of 194 when this section was written; round 4b closed twelve more
  rows that publish none (the eight `tiny-unlink`/`tiny-bulk-delete` and the four
  `namespace-*` walk-ceiling tiers), so the gap grew with the pass count rather than
  shrinking.
- **The oracle over-verifies and may also under-verify.** Four C2 families do a
  byte-exact read-back their frozen oracle does not require (`c2.delta.cdc-locality` is
  O1 + O3), while the drivers gate "replay root == measured root" — self-consistency,
  not O1's pinned expected root — and the delta counters are written without a gate
  against pinned values, which is what O3 means.
