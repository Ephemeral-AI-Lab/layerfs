# Implementation plan and rollout

> **Status:** Proposal; target LayerFS v0.1.7; not a released contract and not a
> receipt. No figure in this document is a measurement.

## 1. Scope

**In:** the harness at
[`core/benchmark/fs-bench-pro-storage-content/`](../../../../benchmark/fs-bench-pro-storage-content/)
and this documentation. **Out:** every product source file. If a step appears to need a
product change, stop and report it rather than widening a product API — `Store::path()`
is already public, and that is enough for the runner to `stat` the Store.

Also out: the 217 admission rows. They keep their registry, their cardinality, their
golden table, their `--lane full` composition and their `full` verification default.
This campaign adds lanes, never members.

Paths below are relative to the harness root unless stated otherwise.

## 2. Files

### New — Rust

| file | purpose |
| --- | --- |
| `src/workload/history.rs` | corpus reader: `manifest.tsv` / `previous.tsv`, forward blob accumulation across transitions, per-state tree and pinned `Expectation`, identity fails closed |
| `src/families/history.rs` | the 227 rows: three lanes, `tier` = campaign index, `tier_label` = `full157_index`, `Shape::History`. Rows only — no bodies, per `families/mod.rs`'s rule |
| `src/ops/history.rs` | the driver: load the prepared base and state objects, open the copy, construct, save, gates, phase marks, cleanup |

### New — Rust tests and golden

| file | purpose |
| --- | --- |
| `tests/history_declarations.rs` | the declared rows match the corpus: 17/53/157, indices, tier labels, lane membership |
| `tests/golden/history-expected.tsv` | pinned O1 roots, O3 counts and O6 attribution per row, embedded with `include_str!` so the harness identity covers it |

### New — Python

| file | purpose |
| --- | --- |
| `shared/history_corpus.py` | corpus authentication and per-state/transition acquisition; fails closed on manifest SHA, tip, per-state hashes, oracle hashes and `blob_digests` |
| `shared/history_chain.py` | lane → ordered states, base assignment, checkpoint spacing, chain compatibility digest |
| `shared/test_history_corpus.py` | self-check for the above |
| `shared/test_history_chain.py` | self-check for chain assignment, checkpoint arithmetic and digest stability |

### Modified

| file | change |
| --- | --- |
| `runner.py` | `--corpus`, the three lanes, chain-aware `prepare`, per-lane prepare budget, `sample` as the history default, preparation resource gates |
| `src/main.rs` | the three lanes, `--corpus`, `--history-profile`; generalise `--emit-input`/`--load-input` to `--emit-prepared`/`--load-prepared`, keeping the old names as aliases |
| `src/registry.rs` | `Shape::History`, the new group, lane membership, a second cardinality self-check array |
| `src/families/mod.rs`, `src/ops/mod.rs`, `src/workload/mod.rs` | module registration and `Shape::History` dispatch |
| `src/workload/artifact.rs` | carry the base Store and the chain identity in the artifact manifest |
| `src/workload/expected.rs` | include the history pin table |
| `src/gates.rs` | history gate helpers |
| `src/support/phases.rs` | publish preparation's heap, RSS and data bytes |
| `shared/phases.py` | read and reconcile those fields |
| `shared/copyladder.py` | chain-level ENOSPC preflight; the two row classes |
| `shared/space.py` | the pack-body split by `object_role`, and the cumulative retained-storage curve |
| `shared/analyze.py` | the history report shape of [`measurement.md`](measurement.md) §4 |
| `shared/pin_expected.py` | produce `history-expected.tsv` the same way it produces `expected.tsv` |
| `tests/golden/registry.tsv` | the new rows appear, so the golden comparison covers them |
| `../CONTRACT.md`, `../README.md` | a one-line pointer that the history lanes are a separate claim, not an amendment |

**Unchanged:** `ops/c1.rs`, `ops/c2.rs`, `ops/fs.rs`, `ops/fs_fixture.rs`,
`ops/pipeline.rs`, all of `support/` except `phases.rs`, every existing family, and every
existing golden row.

## 3. The rollout

Six stages. A stage is entered only when the previous stage's exit gate has **PASSED**,
and no stage is entered on an assumption.

### Stage 0 — specification

Deliverables: these five documents, the roadmap specification under
`docs/roadmap/0.1/0.1.7/`, and the sub-issue.

Exit: the README's six owner decisions are ruled, the lane and row IDs are frozen, and
the pins of [`verification.md`](verification.md) §3 are written down. `benchmark_rules.md`
§1 is explicit that a family measured before its specification exists is exploratory with
`admission_eligible=false`, so no product run happens here.

### Stage A — corpus and chain

Deliverables: `history_corpus.py`, `history_chain.py` and their self-checks. Python only;
no registry change, no product run.

Exit: the corpus authenticates against every identity in [`README.md`](README.md) §2; the
three selections enumerate exactly 17/53/157 at the right indices; the corpus-derived
pins match (**101,477 / 561,010,345**, **306,861 / 1,676,767,835**, **904,143 /
4,936,693,030**); the chain ENOSPC preflight refuses a chain that will not fit.

### Stage R10 — stride-10 shakedown (17 rows)

The cheapest stage that exercises every mechanism: registry rows, corpus reader, driver,
gates, golden table, one measured run, and the first published `cleanup_wall_ns` in the
harness.

Exit: 17/17 rows `PASS`; corpus pins match; the storage curve is monotone in state index;
every row's complete command ≤ 15 s; the per-row and per-lane preparation budgets
**measured and declared**; R1 and R2 rows never pooled; `verify` reconciles every row;
every save advances the Store watermark, so no state is a silent no-op.

`history-stride10` has no recorded canonical total, so this stage's canonical numbers
become the first-run pins of [`verification.md`](verification.md) §3(c).

### Stage R3 — stride-3 development track (53 rows)

Exit: canonical content **589,423,458 B / 73,476 objects**; **306,861** path-states;
**1,676,767,835** logical bytes; the full attribution table reproduced; every counter
consistent with R10's mechanism. R3 then becomes the default iteration lane for
optimization candidates, as v0.1.6's own stride-3 contract made it the fast development
track.

### Stage R1 — stride-1 qualification track (157 rows)

Exit: canonical content **871,588,115 B / 104,705 objects**; **904,143** path-states;
**4,936,693,030** logical bytes; lane wall and prepare-chain cost inside their declared
budgets; explicitly selectable and never a default.

### Stage O — the optimization campaign

This is what the directory is named for. Promotion runs **one way only**:

```text
candidate → R10 (cheap reject) → R3 (matched n3 alternating pairs) → R1 (final validation)
```

A candidate that skipped a stage is not promoted. Each ledger entry records the
mechanism, the rung, the cache state, before/after allocated, apparent and canonical
bytes, the category-gap delta, the read amplification, and the verdict — with negative
outcomes preserved. The ledger is created at this stage as `optimization-ledger.md`
beside these documents, mirroring the v0.1.6 campaign's
`docs/roadmap/0.1/0.1.5/issue100/optimization-checklist-and-experiment-ledger.md`; it
does not exist yet.

## 4. Stop rules

- **A canonical pin that does not match at R3 or R1 stops the stage.** It is a finding
  about the migration, not a fixture to adjust.
- **`sum(operation_ns)` must not fall** when preparation is optimised. If it does,
  measured work moved into setup and the change is rejected.
- **A row that cannot fit its budget is `NOT_RUN`** with its measured wall and reason.
  No tier is shrunk, no timeout inflated, no worker added.
- **A residency gate failure is `INELIGIBLE`**, never a quiet pass.
- **Nothing in a timed phase rebuilds a fixture**, and nothing reused is presented as a
  cold claim.

## 5. Production LOC accounting

`core/tools/check_product_boundary.py` scans only `core/crates/*/src` and
`core/crates/*/sql`, and the repository's LOC rule excludes benchmark harnesses from
production LOC. Every commit in this campaign therefore reports the **production total
unchanged, delta 0**, with harness lines stated separately. The 999-line ceiling does not
bind here, though the harness's own convention of one cohesive module per concern still
does.
