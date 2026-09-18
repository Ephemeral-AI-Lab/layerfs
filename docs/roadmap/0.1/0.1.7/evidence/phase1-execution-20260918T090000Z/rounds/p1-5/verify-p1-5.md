# verify-p1-5 — a spill resets only the tiers it replaces

> **author-verified** (single-agent Phase 1: no independent reviewer exists; the
> evidence is the reproducibility of the commands below on the named trees).
> Round: [`receipt.md`](receipt.md). Tree: `70dc75836`, arm [`after/`](after/);
> before arm [`../p1-9/after/`](../p1-9/after/) (tree `53bbad138`).

## 1. Reproduction from a clean tree

| # | Command | Exit | Output |
| --- | --- | ---: | --- |
| R1 | `git archive 70dc75836 \| tar -x -C /tmp/verify-p15/tree` | 0 | clean P1-5 tree |
| R2 | the probe client built in the archive's own `…/client` | 0 | — |
| R3 | `phase0client order 4000 2000 64` | 0 | `rows_read 27777`, `rows_written 25760`, `runs_created 124`, `merges 61`, `peak_run_bytes 568320` — identical to `after/logs/D26` |
| R4 | `phase0client order 4000 2000 4096` | 0 | `spilled 0`, `rows_read 0`, everything else identical to `after/logs/D25` |
| R5 | `cargo +1.85.1 test … --test filesystem_ordering a_spill_keeps` | 0 | `1 passed` |
| R6 | the same test with the parent's `runs.rs` | 101 | `a kept scan resumes at its cursor: 257 rows read for one row` |

## 2. Falsification answers

**2a — does the new test fail on the parent tree?** Yes: `257 rows read for one
row`, the full restart the item removes. Reproduced in a clean archive of
`53bbad138` with only the test file copied in.

**2b — did the counter move in the predicted direction and magnitude, and did
anything else move?** Predicted: `rows_read` 59,007 → ~27–35k with
`rows_written`/`runs_created`/`merges` unchanged. Measured 27,777 with all three
unchanged, and a diff of every other frozen row is **empty** (D25 identical
because it never spills). The residual over the write term falls to 2,017, i.e.
O(rows touched), which is the Big-O claim.

**2c — parity green and unchanged?** The test diff is `filesystem_ordering.rs`;
the seven sealed-oracle targets are untouched and green. The ordering suites that
pin behaviour rather than counts — threshold parity, spilled-vs-full-scan
agreement, the every-order truth proof, the scan suite — are green in the
workspace run.

**2d — single-variable?** Two files: `references/runs.rs` (the targeted reset and
its two call sites) and `filesystem_ordering.rs` (the new test), plus the
same-commit architecture-doc paragraph. The full clear remains at the three sites
that replace every tier.

**2e — error paths.** A stale scan would answer with the *wrong row*, not an
error — which is why the invariant is index-parallel and why the every-order truth
proof (`run_lookup_answers_every_serial_in_every_order`) and the
spilled-vs-full-scan agreement test are the guards; both are green. The
off-by-one the plan warned about (`truncate(level)` vs `level + 1`) is avoided by
construction: the implementation clears the prefix `[0, level]` and keeps
`[level+1, …]`, and the test's thirteenth spill lands at level 0 so the kept scan
belongs to a tier whose run is provably untouched. `find` on an absent serial is
still absence (`a_restarted_scan_returns_the_same_rows_as_a_fresh_one`).

**2f — elapsed as a gate?** No: the gate is `rows_read` and the residual.

## 3. UNVERIFIED

* **The per-doubling ×3.0 → ×2.1–2.4 grid was not re-collected.** The plan's
  verification names the 250/500/1k/2k forced-64 arms; this receipt measures the
  4,000/2,000 forced-64 row (the frozen `order.forced64`) and the default row.
  The grid is available through `collect.py … order-250 order-500 order-1000
  order-2000` and is not claimed here.
* **The restart-vs-dedup split stays unmeasured**, as the planning report says:
  this receipt shows the aggregate residual collapsing, not the split.
* **Retained buffers live longer.** The plan flags that scans kept across a spill
  hold their buffers longer (total unchanged, ≤ tiers × merge buffer); no counter
  in the frozen set reports it and none is claimed.
