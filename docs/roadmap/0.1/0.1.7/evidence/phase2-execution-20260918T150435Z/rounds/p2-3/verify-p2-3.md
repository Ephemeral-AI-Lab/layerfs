# verify-p2-3 — `locking_mode = EXCLUSIVE`: measured and declined

> **author-verified** (single-agent Phase 2). Round: [`receipt.md`](receipt.md).
> Trees: parent `0a593084c`; candidate `/tmp/p23-lock` = that tree plus
> [`attempt/candidate.patch`](attempt/candidate.patch). No product commit exists, by
> design.

## 1. Reproduction

| # | Command | Exit | Output |
| --- | --- | --- | --- |
| R1 | `git archive 0a593084c \| tar -x -C /tmp/p23-lock`, apply `attempt/candidate.patch` | 0 | candidate tree |
| R2 | `cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p layerfs-storage --no-fail-fast` | 101 | **180 passed, 3 failed** — all in `tests/visibility.rs` |
| R3 | `cargo +1.85.1 test … --test visibility` | 101 | the three named cases, log in `attempt/failing-tests.log` |
| R4 | the client built in the candidate tree, `phase0client c2 8191 default` | 0 | every counter identical to `../p2-2/after` |

## 2. Falsification answers

**2a — does a new test fail on the parent tree?** The item has no new test because
it is declined; what the candidate produced instead is three *existing* pinned cases
failing, which is stronger evidence than a new case would have been.

**2b — did the counter move in the predicted direction?** No counter moved — the
same save reads identically, which the gate itself predicted ("commits / statements
/ wall"). With no movement and a broken contract, the item has nothing to land.

**2c — parity?** The sealed-oracle set is untouched (no product byte changed on the
parent tree). The failures are in the storage crate's own contract cases.

**2d — single-variable?** The candidate adds one pragma and its read-back in
`MutationOwner::acquire`, nothing else (`attempt/candidate.patch`).

**2e — the item's named risks.** *Write-owner only / a read-only Store must not take
it*: honoured in the candidate (the pragma is applied in the save owner's
acquisition, never in `connection::open`). *The multi-store test surface*: exercised,
and it refuses the change. *`busy_timeout = 0` stays*: untouched.

**2f — elapsed as a gate?** No timing was used; the decline is on identity plus the
contract failures.

## 3. UNVERIFIED

* No out-of-process reader was exercised; the failures are same-process connections.
* The exact lock error each failing case returned is only in the log; the receipt
  does not claim which of `SQLITE_BUSY`/`SQLITE_LOCKED` was raised in each.
