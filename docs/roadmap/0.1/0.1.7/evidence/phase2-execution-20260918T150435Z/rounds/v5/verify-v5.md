# verify-v5 — the statement counter for object-row inserts

> **author-verified** (single-agent Phase 2: no second reviewer, no subagent; the
> evidence is the reproducibility of the commands below on the named trees).
> Round: [`receipt.md`](receipt.md). Tree: product commit `464807178`, arm
> [`after/`](after/); before arm [`../p2-0/after/`](../p2-0/after/) on `ed5ab5d95`.

## 1. Reproduction from a clean tree

| # | Command | Exit | Output |
| --- | --- | ---: | --- |
| R1 | `git archive 464807178 \| tar -x -C /tmp/verify-v5`, plus this round's `client/src/main.rs` | 0 | clean V5 tree |
| R2 | `cargo +1.85.1 build --release --offline --locked --manifest-path core/Cargo.toml --examples` | 0 | examples built |
| R3 | the probe client built in the archive's own `…/client` | 0 | — |
| R4 | `phase0client c2 8191 default` | 0 | `inserted 8191 … commits 31 statements 8191 … pool_groups 8191` — identical to `after/logs/D28` (only `elapsed_ns` differs) |
| R5 | `phase0client c2 1023 default` | 0 | `inserted 1023 … commits 4 statements 1023 …` — identical to `after/logs/D29` |
| R6 | `cargo test --test statement_batching` in the archive | 0 | `2 passed; 0 failed` |
| R7 | `python3 compare_arms.py rounds/p2-0/after rounds/v5/after` | 1 (by design) | `steps compared: 35, differing: 2` — D28 and D29, each differing **only** by the added field |

## 2. Falsification answers

**2a — does the new test fail on the parent tree?** Yes, by not compiling: the
parent's `SaveOutcome` has no `statements` field. Copied into the `0a1d74742`
archive and built:

```text
error[E0609]: no field `statements` on type `SaveOutcome`
  --> crates/layerfs-storage/tests/statement_batching.rs:38:17   (and :42, :57, :63)
```

Stated plainly: for a counter that does not exist on the parent tree, a compiled
failing assertion is not available — the parent-tree *measurement* is the frozen
row itself (D28 printed `inserted 8191` and could not print a statement count),
and the new tests pin the identity on the instrumented tree.

**2b — did the counter move in the predicted direction and magnitude?** V5's
prediction is not movement but *existence plus calibration*: the counter must read
≈ rows at 8,191 rows. Measured 8,191 (exactly) and 1,023 at 1,023 rows, equal to
`inserted` in both. Nothing else moved: 33 of 35 measurement steps are
bit-identical, and the two that differ do so only by the added field.

**2c — parity green and unchanged?** 35/35 sealed-oracle tests pass.
`git diff ed5ab5d95..464807178 -- '*tests*'` adds `statement_batching.rs` and
changes nothing else — no re-pinned parity test.

**2d — single-variable?** `git show --stat 464807178`: `cas/owner.rs` (the field),
`cas/placement.rs` (the charge), `cas/store.rs` (the surfacing), `sqlite/write.rs`
(the writer's report), the new test, and `10-counters.md`. One variable: the
statement counter. No behavior change, no format change, no bound moved.

**2e — the item's named risk (the counter could lie)?** The charge is taken from
the writer's return value, not incremented by the caller's loop, so a batch that
issues one statement for `k` rows cannot be mis-counted by the loop; and the tests
cross-check the charge against `SELECT COUNT(*) FROM objects` on an external
connection, which is the engine's own answer rather than the counter's. The named
control (an all-reuse save charges 0) is a test, not an argument.

**2f — is elapsed a gate anywhere here?** No. The gate is `statements` at two row
counts; `elapsed_ns` is reported as diagnostic (`CONTRACT.md` §2.4).

## 3. UNVERIFIED

* **No independent statement count.** The tests compare the counter against the
  `objects` table's row count, which is one statement per row *by construction* on
  the current tree; they do not instrument SQLite to count statements
  independently. `P2-2` will need that stronger check (a multi-row INSERT has no
  row-count proxy for its statement count), and the plan for it is in the item's
  receipt when it lands.
* **The `c2.small` re-run was not repeated.** `X3` repeats D28 only; D29's row is
  a single sample, as the frozen-set discipline allows (one sample per case per
  arm).
* **`measure_components.rs` does not print the new field.** The vehicle its
  author would use for a storage-level view is not part of the frozen set; the
  frozen `c2` rows carry the field instead.
