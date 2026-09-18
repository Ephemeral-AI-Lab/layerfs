# verify-p1-10 — the ordering state carried out of `touched_serials`

> **author-verified** (single-agent Phase 1: no independent reviewer exists; the
> evidence is the reproducibility of the commands below on the named trees).
> Round: [`receipt.md`](receipt.md). Tree: `8327f87bb`, arm [`after/`](after/);
> before arm [`../p1-16/after/`](../p1-16/after/) (tree `9c0cb6f9e`).

## 1. Reproduction from a clean tree

| # | Command | Exit | Output |
| --- | --- | ---: | --- |
| R1 | `git archive 8327f87bb \| tar -x -C /tmp/verify-p110` | 0 | clean P1-10 tree |
| R2 | the probe client built in the archive's own `…/client` | 0 | — |
| R3 | `phase0client order 4000 2000 64` | 0 | `rows_read 25809`, `serials_scanned 4001`-equivalent (`rows_touched 4001`), `rows_written 25760`, `runs_created 124`, `merges 61` — identical to `after/logs/D26` |
| R4 | `phase0client order 4000 2000 4096` | 0 | `spilled 0`, `rows_read 0`, everything else identical to `after/logs/D25` |
| R5 | `cargo +1.85.1 test … --test filesystem_ordering carried_state` | 0 | `1 passed` |

## 2. Falsification answers

**2a — does the new test fail on the parent tree?** It cannot compile there: the
carried-state API did not exist (`touched_serials` returned `Vec<u64>`). The
parent-tree evidence for the movement is the frozen row itself — D26's
`rows_read` 27,777 in the before arm against 25,809 here — which is a measurement,
not an assertion. Stated as such rather than dressed up as a failing test.

**2b — did the counter move in the predicted direction and magnitude, and did
anything else move?** Predicted −`serials_scanned` (2,001); measured **−1,968**
(the difference is the serials still in the pending map, whose `state()` was a map
hit and never charged a run read). Nothing else moved: a diff of every other
frozen row is empty, and D25 is identical because it never spills.

**2c — parity green and unchanged?** The test diff is `filesystem_ordering.rs`;
the seven sealed-oracle targets are untouched and green. The semantics suites
(threshold parity, hardlink/release, ceiling enforcement) are green.

**2d — single-variable?** Three product files, all in one subsystem
(`references/reduce.rs`, `update.rs`, `input.rs`) plus the test. The divisor change
is part of the same variable: the collection's element size is what the ceiling
charges for.

**2e — error paths.** The refusal point moves with the divisor: a caller whose
touched set exceeds `ordering_bytes / 16` is refused with
`ObjectLimitExceeded { what: "ordering.touched_serials" }` before the collection is
built, and `the_operation_ceiling_is_enforced…` is green. The release pair
(`note_removed_binding` → `state`) is unchanged and its hardlink cases are green;
`state()` on an absent serial still answers `None` (absence, not an error).

**2f — elapsed as a gate?** No: the gate is `rows_read` and the roots.

## 3. UNVERIFIED

* **The 16 B/serial figure is a charge, not a measurement.** `PendingState` is
  larger than 8 bytes (`value: Option<InodeValue>` plus a discriminant), so the
  divisor is a *declared* per-element charge in the same sense the pending rows'
  ×2 is; no instrument measures the real heap.
* **Callers with a tight ordering ceiling are refused earlier than before.** That
  is the intended consequence of the divisor change and is stated; no test in the
  tree pins the old refusal point (the ceiling-enforcement test uses the default
  ceiling).
* **The release path still calls `state()`** (a pending-map hit by design); its
  cost is unchanged and not measured here.
