# verify-p2-7 — consolidation adopts its newest run

> **author-verified** (single-agent Phase 2: no second reviewer, no subagent).
> Round: [`receipt.md`](receipt.md). Tree: `b2abb6455`, arm [`after/`](after/);
> before arm [`../p2-6/after/`](../p2-6/after/) on `57c4cf3bd`.

## 1. Reproduction from a clean tree

| # | Command | Exit | Output |
| --- | --- | ---: | --- |
| R1 | `git archive b2abb6455 \| tar -x -C /tmp/verify-p2-7` | 0 | clean P2-7 tree |
| R2 | the probe client built in the archive's own `…/client` | 0 | — |
| R3 | `phase0client order 4000 2000 64` | 0 | `rows_read 25681`, `rows_written 25632`, `runs_created 123`, `merges 61`, `emitted 14` — identical to `after/logs/D26` |
| R4 | `cargo test -p layerfs-content --test filesystem_ordering_consolidate` | 0 | `1 passed; 0 failed` |
| R5 | `python3 compare_arms.py rounds/p2-6/after rounds/p2-7/after` | 1 (by design) | `steps compared: 37, differing: 2` — D26 and X1 |
| R6 | `python3 pack_bytes_census.py rounds/p2-6/after rounds/p2-7/after` | 0 | 12 stores, identical |

## 2. Falsification answers

**2a — does the new test fail where the property does not hold?** Yes, and this is
the item's central evidence: in a scratch tree with one `append` aimed at a
pre-existing run inside `consolidate`, the case fails (`0 passed; 1 failed`). On
the item's own tree it passes. The control *inside* the case (a pre-seal handle
refusing a post-seal append) shows the seal itself is live.

**2b — did the counter move in the predicted direction and magnitude?** Predicted:
one fewer run, and the copied run's rows not rewritten. Measured on D26:
`runs_created` 124 → 123, `rows_written` 25,760 → 25,632 (−128), `rows_read`
25,809 → 25,681 (−128, the copy read what it wrote). Nothing else moved, on D26 or
on the other 35 steps, and the emitted row stream is unchanged (the case compares
it directly). D25 is unchanged because it does not spill.

**2c — parity green and unchanged?** 35/35 sealed-oracle tests; `git diff
57c4cf3bd..b2abb6455 -- '*tests*'` adds one test binary and extends the recording
backing with a seal, touching no existing expectation.

**2d — single-variable?** `git show --stat b2abb6455`: `runs.rs` (the copy removed,
the adoption in), the new case and the backing's seal support, and §"Consolidation"
of the filesystem paper. One variable: whether consolidation rewrites its newest
input.

**2e — the item's named risk (aliasing)?** Probed by the sealed-input case (any
append into an input fails the consolidation) and by the scratch-tree control
(which shows the case fires when a write does land). The merge's own contract is
unchanged: it creates its output run and appends only there.

**2f — is elapsed a gate anywhere here?** No. `elapsed_ns` on D26 moved 146.8 µs →
154.3 µs in the *wrong* direction on a single sample and is diagnostic-grade
(`CONTRACT.md` §2.4); the gate is the three work counters and the row stream.

## 3. UNVERIFIED

* **The peak-bytes claim is not re-measured.** Removing a copy must lower the
  physical peak (one fewer run coexisting), but `peak_run_bytes`/`peak_backing`
  are logical accounts and did not move; the backing's own `peak_bytes` is a
  test-support figure and no frozen row reports it.
* **A scratch-tree control is not a committed test.** The demonstration that the
  case fails when a merge writes into an input lives in `/tmp/p2-7-control`, not in
  the repository: making it a committed test would require product code that
  deliberately aliases, which the product must not contain. The seal control inside
  the case is the committed half.
* **The case's fixture is one shape** (twelve spills of 64 rows, several live
  tiers). A consolidation of exactly two tiers, or of tiers whose newest run is
  the smallest, is not separately exercised.
