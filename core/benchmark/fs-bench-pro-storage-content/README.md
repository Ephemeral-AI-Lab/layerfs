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
| `prepare` | Acquires a prepared artifact once, keyed by a compatibility digest. Nothing in a timed phase rebuilds it. |
| `perf` | One sample per case per arm; fresh output; measurement lock held; complete-command budget enforced per case; a receipt that names the tree it ran on. |
| `verify` | Re-reads the raw artifacts and re-derives flatness, sequence, worst-gate aggregation, budget classification, and — for C2 rows — the space and pack accounting read out of the Store file itself. |
| `report` | Renders the ladders, bands and the four-axis view. Time is printed and never decides. |
| `self-check` | Runs every Python self-check, the registry self-check, the golden comparison and the lock-parity test. |
| `calibrate` | Runs the untimed experiments E1-E4 and records each outcome, refutations included. |

## Evidence layout

```text
benchmark-results/fs-bench-pro-storage-content/   gitignored, development runs
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
