# P1-5 receipt — a spill resets only the scans of the tiers it replaces

> **Status:** Item receipt. Written once, from [`after/`](after/) (commit
> `70dc75836`) under [`../../CONTRACT.md`](../../CONTRACT.md). The **before** arm is
> [`../p1-9/after/`](../p1-9/after/), collected on `53bbad138`, this item's parent.
> **Landing-order deviation, declared:** P1-5 landed before the edit-path group
> (P1-8/P1-7/P1-14). The ordering counters are independent of those items — the
> `order` rows are driven by the reducer and the sorted engines, not by
> `apply_edits` — and this receipt's before arm is the tree immediately preceding
> it, so nothing is confounded.

## 1. The item

`spill()` cleared **every** tier's lookup scan, though a spill into level *k*
replaces only the runs of tiers `[0, k]`. Every tier above *k* keeps the run it
had, so its scan's cursor was still valid; clearing it made the next demand on
such a tier restart from the front of its run and re-read the rows the cursor had
already passed. The two spill sites now drop the replaced tiers' scans only, and
the full clear stays where every tier's run is replaced (consolidate,
`take_single_handle`, `release`).

## 2. The plan's sketch was wrong (recorded, not followed)

The plan specified `self.scans.truncate(level + 1)`. `Vec::truncate(n)` keeps
`0..n`, so that call keeps **exactly the replaced tiers' scans** and drops the ones
above — the opposite of the sketch's own stated intent ("scans of tiers > level
kept"). Implemented as the intent: the replaced prefix is set to `None`. With the
literal sketch, `rows_read` did not move at all (59,007 → 59,007), which is how
the error surfaced.

## 3. Before → after on the frozen set

| Row | counter | before | after | predicted |
| --- | --- | ---: | ---: | --- |
| **D26** `order.forced64` | `runs.rows_read` | **59,007** | **27,777** | ~27–35k ✔ |
| D26 | residual over `rows_written` (25,760) | 33,247 | **2,017** | O(r) ✔ |
| D26 | `rows_written` / `runs_created` / `merges` | 25,760 / 124 / 61 | identical | unchanged ✔ |
| D26 | every peak (`peak_run_bytes` 568,320, `peak_live_runs` 5, `peak_backing`) | — | identical | unchanged ✔ |
| **D25** `order.default` | every counter | 0 spills / 0 rows | **identical** | no scan to keep ✔ |
| D1–D29 except D26 | every counter | — | **identical** (0-row diff) | unchanged ✔ |

Roots, emitted objects and bytes are identical everywhere. The residual collapse
is the item: the per-doubling ×3.0 lookup term is gone, leaving the write term.

## 4. The item's test

`filesystem_ordering::a_spill_keeps_the_scans_of_tiers_above_its_level`: twelve
64-row spills in ascending serial order, an ascending sweep to row 256 (the top
tier's cursor), a thirteenth spill that lands at level 0 **without cascading**
(asserted: only its own 64 rows are written), then `find(257)`.

| | rows read for that one demand |
| --- | ---: |
| this tree | **1** |
| parent tree `53bbad138` (full clear) | **257** |

The test lives in `filesystem_ordering.rs`, not the scan suite: that suite counts
**process-global** allocations (`lookups_allocate_nothing_after_the_tiers_are_built`)
and a second allocating test in the same binary perturbs it — found by running it
and fixed by moving the test, not by weakening the allocation test.

## 5. The eight checks (exit codes)

| # | Command | Exit |
| --- | --- | ---: |
| 1 | `python3 core/tools/check_product_boundary.py` | 0 (116 files) |
| 2 | `python3 -m unittest discover -s core/tools -p 'test_*.py'` | 0 (6 OK) |
| 3 | `python3 -m unittest discover -s tools -p 'test_production_loc.py'` | 0 (17 OK) |
| 4 | `cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all --check` | 0 |
| 5 | `cargo +1.85.1 test --manifest-path core/Cargo.toml --workspace --locked --no-fail-fast` | 0 (**448 passed, 0 failed**) |
| 6 | `cargo +1.85.1 clippy --manifest-path core/Cargo.toml --workspace --locked --all-targets -- -D warnings` | 0 |
| 7 | `python3 tools/production_loc.py --files` | 0 |
| 8 | `git diff --check` | 0 |

Parity: the test diff is `filesystem_ordering.rs` (not one of the seven
sealed-oracle targets); the ordering suites — the threshold-parity test, the
spilled-vs-full-scan agreement test, the every-order truth proof and the
one-reader-per-tier scan tests — are all green, and every emitted root on the
frozen set is identical.

## 6. Determinism

`X1` (`order.forced64` repeat) is bit-identical to its gate sample on every
counter, including `rows_read 27,777`; `X2` likewise. `elapsed_ns` moved
143.7 ms → 153.3 ms between the arms (and 149–165 ms across repeats) and is
diagnostic-only — no line of this receipt is gated on it.

## 7. Production LOC

`P1-5 | +2..+8` was the plan's estimate; the actual is **+6**
(`references/runs.rs`).
`core` **18,963 → 18,969 (delta +6)**; `crates/` reference 65,417; combined
84,380 → 84,386. Method: `tools/production_loc.py` over the first parent
(`53bbad138`, via `git archive`) and the staged tree.

## 8. Acceptance

- [x] The counter moved in the predicted direction and magnitude (59,007 → 27,777; residual 33,247 → 2,017)
- [x] No other counter moved (0-row diff across D1–D29 outside D26; D25 identical)
- [x] The new test passes here (1 row) and fails on the parent tree (257 rows)
- [x] Roots identical; parity green and unchanged
- [x] The plan's wrong sketch is recorded and the correct implementation explained
- [x] Eight checks green; architecture doc updated in the same commit; LOC disclosed
