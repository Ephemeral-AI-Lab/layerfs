# verify-p2-2 — one INSERT per bound chunk of a group's rows

> **author-verified** (single-agent Phase 2: no second reviewer, no subagent).
> Round: [`receipt.md`](receipt.md). Tree: `0a593084c`, arm [`after/`](after/);
> before arm [`../p2-5/after/`](../p2-5/after/) on `7db87bb8b`.

## 1. Reproduction from a clean tree

| # | Command | Exit | Output |
| --- | --- | ---: | --- |
| R1 | `git archive 0a593084c \| tar -x -C /tmp/verify-p2-2`, plus this round's client | 0 | clean P2-2 tree |
| R2 | the probe client built in the archive's own `…/client` | 0 | — |
| R3 | `phase0client c2 8191 default` | 0 | `statements 72`, `inserted 8191`, `commits 31` |
| R4 | `phase0client c2 1023 default` | 0 | `statements 9`, `inserted 1023`, `commits 4` |
| R5 | `cargo test -p layerfs-storage --test statement_batching` | 0 | `3 passed; 0 failed` |
| R6 | `python3 compare_arms.py rounds/p2-5/after rounds/p2-2/after` | 1 (by design) | `steps compared: 40, differing: 3` — D28, D29, X3, each only in `statements` |
| R7 | `python3 pack_bytes_census.py rounds/p2-5/after rounds/p2-2/after` | 0 | 12 stores, identical |

## 2. Falsification answers

**2a — do the new tests fail on the parent tree?** Yes, both moved cases:

```text
test a_save_inserts_its_rows_in_bounded_statements ... FAILED
test the_statement_count_tracks_the_rows_and_not_a_constant ... FAILED
test result: FAILED. 0 passed; 2 failed; 0 ignored; 0 measured; 1 filtered out
```

(the third case is new API and cannot compile there).

**2b — did the counter move in the predicted direction and magnitude?** Predicted
one statement per bound chunk; measured 8,191 → **72** at `k = 128` over 21 sealed
groups, and 1,023 → 9. The ideal `⌈8191/128⌉ = 64` is not reached because rows are
inserted group by group; the receipt states that rather than rounding the result to
the prediction. `commits`, `inserted`, `packs_created`, `pack_appends` and every
other counter on those rows are unchanged, as are the 37 other steps and the 12
stores' bytes.

**2c — parity green and unchanged?** 35/35 sealed-oracle tests; `git diff
7db87bb8b..0a593084c -- '*tests*'` changes `statement_batching.rs` only, and no
parity expectation moved.

**2d — single-variable?** `git show --stat 0a593084c`: `sqlite/write.rs` (the
batched writer and the derivation), `cas/placement.rs` (the group's rows built once
and inserted together), the V5 test file and two architecture papers. One variable:
how many rows one statement carries.

**2e — the item's named risks, one by one.** *`k` derived, not hardcoded*: read from
the connection's limits, pinned by a case that compares it against
`SQLITE_LIMIT_VARIABLE_NUMBER`. *Per-row error attribution*: preserved in kind - a
chunk is atomic, so a failure aborts exactly as a failed row did, with the same
engine/integrity error and no re-run. *`sqlite_master` unchanged*: the schema text
is pinned and verified at open; the arms' stores are byte-identical.

**2f — is elapsed a gate anywhere here?** No; the gate is `statements` with
`commits` held.

## 3. UNVERIFIED

* **No test forces a mid-chunk failure.** The atomicity claim is SQLite's documented
  behaviour for a multi-row `INSERT`; no case injects a constraint failure into a
  chunk, so "the error class is preserved" is read from the engine's semantics plus
  the unchanged error path, not measured.
* **The 128 cap is not exercised as a boundary.** `k` lands on 128 here because the
  engine's limits are large; a deployment with a smaller
  `SQLITE_LIMIT_VARIABLE_NUMBER` would derive a smaller chunk, which the derivation
  case allows but no frozen row exercises.
* **Group-granularity cost is stated, not optimized.** The 8 statements between 64
  and 72 are the sealed-group boundaries; removing them is a transaction-accounting
  change and is deliberately not attempted here.
