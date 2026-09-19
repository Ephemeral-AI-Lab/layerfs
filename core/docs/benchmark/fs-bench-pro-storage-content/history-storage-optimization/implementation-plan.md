# Implementation plan and rollout

> **Status:** Proposal; target LayerFS v0.1.7; not a released contract and not a receipt.
> No figure in this document is a measurement.

## 1. Scope

**In:** the harness at
[`core/benchmark/fs-bench-pro-storage-content/`](../../../../benchmark/fs-bench-pro-storage-content/)
and this documentation. **Out:** every product source file. If a step appears to need a
product change, stop and report it rather than widening a product API.

Also out: the 217 admission rows. They keep their registry, cardinality, golden table,
`--lane full` composition and `full` verification default. This lane adds a group and three
lanes, never members.

Paths below are relative to the harness root unless stated otherwise.

## 2. Files

### New — Rust

| file | purpose |
| --- | --- |
| `src/workload/history.rs` | corpus reader: `checkpoint-manifest.json`, `manifest.tsv`, `previous.tsv`, forward blob accumulation, per-state oracle. Every identity fails closed |
| `src/families/history.rs` | the three rows. Rows only — no bodies, per `families/mod.rs`'s rule |
| `src/ops/history.rs` | the driver: authenticate, then the N per-state children (construct → `build_filesystem` → save), then close |

### New — Rust tests and golden

| file | purpose |
| --- | --- |
| `tests/history_declarations.rs` | the declared rows match the corpus: three selections, state counts 17/53/157, the pinned path-state and logical-byte totals |
| `tests/golden/history-expected.tsv` | pinned per-state roots, canonical totals and storage readings, embedded with `include_str!` so the harness identity covers it |

### New — Python

| file | purpose |
| --- | --- |
| `shared/history_corpus.py` | corpus authentication for the receipt: manifest SHA, pinned tip, per-lane pins. Fails closed |
| `shared/test_history_corpus.py` | its self-check |

### Modified

| file | change |
| --- | --- |
| `runner.py` | `--corpus`, the three lanes, the corpus identity in every receipt, the lifted budget for these rows, the storage delta in the derived receipt |
| `src/main.rs` | `--corpus` and the three lanes |
| `src/registry.rs` | `Shape::History`, the `history.*` group, lane membership, a cardinality self-check entry |
| `src/families/mod.rs`, `src/ops/mod.rs`, `src/workload/mod.rs` | registration and `Shape::History` dispatch |
| `src/support/phases.rs` | publish CPU user/system and RSS per phase |
| `shared/phases.py` | read and reconcile those fields |
| `shared/space.py` | the before/after delta shape, and canonical bytes and objects grouped by `object_role` |
| `shared/analyze.py` | the history report shape of [`measurement.md`](measurement.md) §5 |
| `src/workload/expected.rs` | include the history pin table |
| `tests/golden/registry.tsv` | the three rows appear, so the golden comparison covers them |
| `../CONTRACT.md`, `../README.md` | a one-line pointer that `history.*` is a separate claim, not an amendment |

**Unchanged:** `ops/c1.rs`, `ops/c2.rs`, `ops/fs.rs`, `ops/fs_fixture.rs`, `ops/pipeline.rs`,
`workload/artifact.rs`, every existing family, and every existing golden row. This lane needs
no artifact format, no copy ladder and no prepared root.

## 3. The rollout

Seven phases. A phase is entered only when the previous phase's exit gate has **PASSED**.

### Phase 0 — specification and rulings

Deliverables: these five documents, the roadmap specification under
`docs/roadmap/0.1/0.1.7/`, and the sub-issue.

Exit: the README's seven owner decisions are ruled, the three row IDs and lanes are frozen,
and the pins of [`verification.md`](verification.md) §4 are written down. `benchmark_rules.md`
§1 is explicit that a family measured before its specification exists is exploratory with
`admission_eligible=false`, so no product run happens here.

### Phase 1 — corpus reader

Deliverables: `src/workload/history.rs`, `shared/history_corpus.py` and the self-checks. No
registry change, no product run.

Exit: the corpus authenticates against every identity in [`README.md`](README.md) §3; the
three selections enumerate exactly 17/53/157 at the right indices; the path-state and
logical-byte pins match — **101,477 / 561,010,345**, **306,861 / 1,676,767,835**,
**904,143 / 4,936,693,030**; a blob that does not identify is refused.

### Phase 2 — the two harness gaps

Independent of the history rows, and each its own commit, so the 220 existing rows carry the
change and are verified against it first:

| | gap | fix |
| --- | --- | --- |
| 2a | CPU is dead code and the RSS sampler is wired to nothing | bracket `cpu_now()` and start the sampler per phase; publish `cpu.user_ns`, `cpu.system_ns`, `phase_peak_bytes`, `incremental_peak_bytes` |
| 2b | storage is published for six rows only, read once from a retained file | the before/after delta shape and the `object_role` split in `space.py` |

Exit: a full-lane re-run carries the new fields on every row, every pinned counter is
unchanged, and `sum(operation_ns)` does not move.

### Phase 3 — `history-stride10` (17 states)

The registry rows, the driver, the gates, the golden table, and the first measured run. The
cheapest phase that exercises every mechanism.

Exit: the row is `PASS`; corpus pins match; the final verification gate passes over all 17
states; the storage readings and the attribution are published; every counter reconciles.

**This phase also measures the baseline from which the lane budgets are declared.** The
per-lane and verification budgets are fixed here and recorded with their source, before any
optimization work begins — `benchmark_rules.md` §11 allows a target that needs an untouched
baseline to be frozen after that baseline, but not after candidate sampling.

### Phase 4 — `history-stride3` (53 states)

Exit: canonical content **589,423,458 B / 73,476 objects**; **306,861** path-states;
**1,676,767,835** logical bytes; Store allocated **below** v0.1.6's 64,024,576 B; the
allocated ÷ cumulative-logical ratio at or better than 26.2×; the full attribution table; the
final verification gate passes over all 53 states.

### Phase 5 — `history-stride1` (157 states)

Exit: canonical content **871,588,115 B / 104,705 objects**; **904,143** path-states;
**4,936,693,030** logical bytes; Store allocated **below** 83,947,520 B; ratio at or better
than 58.8×; the declared lane budget met; the final verification gate passes over all 157
states. Explicitly selectable and never a default.

### Phase 6 — the optimization campaign

Promotion runs one way only, and never backwards:

```text
candidate → stride10 (cheap reject) → stride3 (matched n3 alternating pairs) → stride1 (final validation)
```

Each ledger entry records the mechanism, before/after allocated, apparent and canonical bytes,
the `object_role` split, the read amplification and the verdict — with negative outcomes
preserved. The ledger is created at this phase as `optimization-ledger.md` beside these
documents, mirroring the v0.1.6 campaign's
`docs/roadmap/0.1/0.1.5/issue100/optimization-checklist-and-experiment-ledger.md`; it does not
exist yet.

## 4. Stop rules

- **A canonical pin that does not match at Phase 4 or 5 stops the phase.** It is a finding
  about the migration, not a fixture to adjust.
- **A Store allocated above v0.1.6's recorded bytes is a finding**, not a new baseline: the
  core Store carries strictly less metadata, so it must land below.
- **Per-state work time or peak heap that rises with the number of states is a finding** —
  each save would be rescanning history, or the chain would not be streaming.
- **A row that cannot fit its declared budget is reported with its measured wall.** The budget
  was declared before the run; it is never inflated afterwards to turn a miss into a pass.
- **A residency or reconciliation failure is `INELIGIBLE` or `INCOMPLETE`**, never a quiet
  pass.
- **Nothing in a timed region reads the corpus**, and no constructed object is supplied to a
  measured child.

## 5. Production LOC accounting

`core/tools/check_product_boundary.py` scans only `core/crates/*/src` and `core/crates/*/sql`,
and the repository's LOC rule excludes benchmark harnesses from production LOC. Every commit
in this campaign therefore reports the **production total unchanged, delta 0**, with harness
lines stated separately.
