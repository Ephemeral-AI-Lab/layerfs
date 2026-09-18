# verify-v4 — the edit vehicle's own node-load count

> **author-verified.** Round: [`receipt.md`](receipt.md). Tree: `f5e53f312`.

## 1. Reproduction from a clean tree

| # | Command | Exit | Output |
| --- | --- | ---: | --- |
| R1 | `git archive f5e53f312 \| tar -x -C /tmp/verify-v4` | 0 | clean V4 tree |
| R2 | `cargo +1.85.1 build --release --offline --locked --manifest-path core/Cargo.toml --example edit_timing_c1` | 0 | vehicle rebuilt |
| R3 | `edit_timing_c1` (no argument) | 0 | `nodes_read: 9`, `edit_nodes_read: 10` — identical to `after/logs/D27` |
| R4 | `--case delete` / `--case shrink` | 0 ×2 | `4`/`8` and `11`/`0` — identical to `after/logs/M2`, `M3` |

## 2. Falsification answers

**2a — does the observable exist on the pre-item tree?** No: the parent tree's
vehicle prints no `edit_nodes_read` line, which is the gap V4 closes (the same
"the observable is absent" form V1/V2/V3 recorded).

**2b — did any counter move?** None may: the change is a print. Every
counter-bearing line of D1–D29 and M1–M3 is identical between the `p1-4` arm and
this arm apart from the new field.

**2c — parity?** The commit touches one example file; no test target of the
sealed-oracle set is affected.

**2d — single-variable?** One file, one added `println!`.

**2e — error paths.** The vehicle has none on this path; `--case nope` still exits
2 with `unsupported case nope` (unchanged from V2).

**2f — elapsed as a gate?** No elapsed figure is used.

## 3. UNVERIFIED

* The new field is **not** a whole-operation counter for the whole-file route
  (M3 = 0 by design); a reader who wants a non-zero figure there needs the route's
  counters unpinned, which `edit_transitions.rs` forbids.
* No claim is made that `edit_nodes_read` counts *reads*: it counts `load_node`
  calls, including ones served from drafts — which is precisely why it sees the
  work P1-9 removes.
