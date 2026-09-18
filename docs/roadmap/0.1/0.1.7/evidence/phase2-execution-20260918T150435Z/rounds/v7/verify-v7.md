# verify-v7 — the save connection's cache profile (spill half blocked)

> **author-verified** (single-agent Phase 2: no second reviewer, no subagent).
> Round: [`receipt.md`](receipt.md). Tree: `8f0fda297`, arm [`after/`](after/);
> before arm [`../v6/after/`](../v6/after/) on `3e7b3db80`.

## 1. Reproduction from a clean tree

| # | Command | Exit | Output |
| --- | --- | ---: | --- |
| R1 | `git archive 8f0fda297 \| tar -x -C /tmp/verify-v7`, plus this round's client | 0 | clean V7 tree |
| R2 | the probe client built in the archive's own `…/client` | 0 | — |
| R3 | `phase0client c2 8191 default` | 0 | `inserted 8191 … statements 8191` and `c2 save-connection profile page_size 4096 cache_size 2000 cache_spill 20000 mmap_size 0` — identical to `after/logs/D28` |
| R4 | `phase0client c2 20000 diagnostic-small-cache` | 0 | `dbstatus cache_spill ok=true value=15588` — identical to `after/logs/Y1` |
| R5 | `cargo test --test connection_profile` in the archive | 0 | `6 passed; 0 failed` |
| R6 | `python3 compare_arms.py rounds/v6/after rounds/v7/after --skip Y1` | 1 (by design) | `steps compared: 36, differing: 3` — D28, D29, X3, each differing only by the added line |

## 2. Falsification answers

**2a — does the new test fail on the parent tree?** Yes, by not compiling:

```text
error[E0599]: no method named `connection_profile` found for struct `SaveOperation`
  --> crates/layerfs-storage/tests/connection_profile.rs:105:33   (and :139)
```

**2b — did the counter move in the predicted direction and magnitude?** The
instrument's prediction is that the profile becomes observable on the save's own
connection and that nothing else moves. Measured: four pragma values on D28, D29
and X3; 33 other steps bit-identical, and the 3 that differ carry only the added
line. The *spill* value is not predicted because it is not reachable (§4 of the
receipt) - recorded as a blocker rather than reported as a zero.

**2c — parity green and unchanged?** 35/35. `git diff 3e7b3db80..8f0fda297 --
'*tests*'` extends `connection_profile.rs`; no parity test re-pinned.

**2d — single-variable?** `git show --stat 8f0fda297`: one `Pragma` variant
(`sqlite/connection.rs`), the profile type and accessor (`cas/store.rs`), the new
cases (`tests/connection_profile.rs`) and §16.6 of the study. One variable: the
save-connection profile reading.

**2e — the item's named risk (a reading that becomes a knob)?** The accessor only
reads: no product path writes a pragma from it, no environment variable or
per-open option was added (plan §4f), and the boundary guard plus the profile
verification at acquisition are untouched. `P2-1` will change the profile
constants in `configure`, not through this accessor.

**2f — is elapsed a gate anywhere here?** No; the gate is the printed profile and
the counter diff.

## 3. UNVERIFIED

* **The spill counter on the product's own save connection is UNVERIFIED and
  unverifiable in this tree** - that is the blocker, stated in receipt §4 with the
  three pieces of evidence (no safe rusqlite binding, `#![deny(unsafe_code)]` with
  one audited module, the boundary guard's rule).
* **The 15,588 figure is not comparable to P0-2's 1,032.** Different row width and
  row count in the control; the round claims liveness, not a magnitude.
* **`cache_spill 20000` is SQLite's threshold encoding, not a boolean.** Read on
  this toolchain: `PRAGMA cache_spill` returns the page threshold while spilling
  is on and `0` when off. The receipt states the encoding; no case pins the
  current value, which `P2-1` will set to `0`.
