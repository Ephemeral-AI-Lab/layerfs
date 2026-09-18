# verify-p1-16 — the pending-ceiling dial

> **author-verified.** Round: [`receipt.md`](receipt.md). Tree: `84ca5c851`, arm
> [`after/`](after/); before arm [`../p1-12/after/`](../p1-12/after/) (tree
> `ca0cf985a`).

## 1. Reproduction from a clean tree

| # | Command | Exit | Output |
| --- | --- | ---: | --- |
| R1 | `git archive 84ca5c851 \| tar -x -C /tmp/verify-p16` | 0 | clean P1-16 tree |
| R2 | `cargo +1.85.1 test --offline --locked --manifest-path core/Cargo.toml -p layerfs-content --test filesystem_ordering a_high_pending` | 0 | `1 passed` — all four arms reproduce |
| R3 | `cargo +1.85.1 test … --test filesystem_ordering` (all 14) | 0 | the threshold-parity and ceiling-enforcement cases stay green |

## 2. Falsification answers

**2a — does the new test fail on the pre-item tree?** No: it pins behaviour the
product already has (the plan's own framing — P1-16 is a *documentation* item with
a boundary test, not a product change). The failure mode it guards is a **default
change**: with `DEFAULT_MAXIMUM_PENDING` widened past the byte bound the refusal
arm would fail, which is precisely why the default is left alone and the ruling is
asked for on #178. Stated plainly rather than dressed up as a parent-tree failure.

**2b — did a counter move?** None may, and none did: a diff of all 29 D-rows and
the three M-rows between the arms is empty.

**2c — parity green and unchanged?** The test diff is `filesystem_ordering.rs`;
the seven sealed-oracle targets are untouched and green.

**2d — single-variable?** Three files: two architecture documents and one test.
No product source, no manifest, no default.

**2e — error paths.** The refusal path *is* the item's evidence: a pending map
whose spill cannot be reserved inside the declared ceiling fails with
`ObjectLimitExceeded { limit }` and the test asserts the limit equals the declared
`ordering_bytes`. The existing ceiling-enforcement and
`a_backing_too_small_for_the_declared_ceiling_is_refused_up_front` cases are green.

**2f — elapsed as a gate?** No elapsed figure is used.

## 3. UNVERIFIED

* **The 349,525-row figure is arithmetic, not a measurement.** No workload in the
  frozen set (or available here) touches 349,525 serials; the test pins the same
  arithmetic at 102 rows. The document says so by deriving the number from the two
  declared constants rather than presenting it as observed.
* **The ~42 MiB unaccounted heap at the top of the dial is an estimate.** The
  planning report itself marks the `BTreeMap` per-row size UNKNOWN; the document
  repeats that caveat instead of quoting the estimate as fact.
* **No owner ruling was taken.** The default stays 4,096; the receipt asks for the
  ruling on #178.
