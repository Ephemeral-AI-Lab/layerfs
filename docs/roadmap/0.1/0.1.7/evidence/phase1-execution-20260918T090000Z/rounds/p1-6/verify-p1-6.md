# verify-p1-6 — an interior join reads its boundary child once

> **author-verified** (single-agent Phase 1: no independent reviewer exists; the
> evidence is the reproducibility of the commands below on the named trees).
> Round: [`receipt.md`](receipt.md) — the decline in §1–§5 and its correction in
> §6. Tree: `360431d10`, arm [`after/`](after/); before arm
> [`../p1-10/after/`](../p1-10/after/) (tree `45cd798f7`).

## 1. Reproduction from a clean tree

| # | Command | Exit | Output |
| --- | --- | ---: | --- |
| R1 | `git archive 360431d10 \| tar -x -C /tmp/verify-p16` | 0 | clean P1-6 tree |
| R2 | `cargo +1.85.1 test --offline --locked --manifest-path core/Cargo.toml -p layerfs-content --test edit_reference an_interior_join` | 0 | `1 passed` — 22 loads, root `57e0a51c…` |
| R3 | the same test with the parent's `tree.rs` | 101 | `left: 24, right: 22` |
| R4 | `cargo +1.85.1 test … --test edit_reference` (all 3, the sealed-oracle parity case included) | 0 | 3 passed |
| R5 | `edit_timing_c1` (no argument / `--case delete` / `--case shrink`) from the archive | 0 ×3 | `nodes_read` 9/4/11 and `edit_nodes_read` 10/7/0 — identical to `after/logs/D27`, `M2`, `M3` |

## 2. Falsification answers

**2a — does the new test fail on the parent tree?** Yes: `left: 24, right: 22` —
the two duplicate boundary loads the item removes. Reproduced in a clean archive of
the parent with only the test file copied in.

**2b — did the counter move in the predicted direction and magnitude, and did
anything else move?** Predicted "0–2 per edit"; measured **−2** on the one oracle
shape that reaches an interior join, with the four equal-height cases unchanged and
every emitted root identical (the parity case compares all of them). On the frozen
set nothing moved: D27/M2/M3 keep both counters, and a diff of all 29 D-rows and
the three M-rows is empty. **The earlier decline's zero was an artifact of the
shapes it measured** — recorded in the receipt §6.1 rather than quietly dropped.

**2c — parity green and unchanged?** `edit_reference`'s parity test
(`the_candidate_reproduces_the_reference_root_and_partition`) covers every oracle
case, including the interior one whose roots must be byte-identical, and passes.
The test diff is `edit_reference.rs`; the seven sealed-oracle targets are untouched
and green.

**2d — single-variable?** One product file (`file/edit/tree.rs`: the `JoinSide`
type and the call-site rewrites) and one test. The two `load_node(_, false)` calls
are still there — the check is the point of the variant.

**2e — error paths.** The non-root check is *preserved*, so a non-canonical
(underfilled) boundary child is refused exactly as before — the failure mode the
decline was protecting. `JoinSide::take` loads with `root = true` when the caller
had no node, so a side without one behaves as it did. `WrongLogicalRole` for a
leaf/branch mismatch is unchanged, and the depth guard (`MappingDepthExceeded`) is
untouched. The whole edit suite (transitions, single, batch, model, noop,
localized, bounds) is green.

**2f — elapsed as a gate?** No. The gate is `EditCounters.nodes_read` and the roots.

## 3. UNVERIFIED

* **The frozen vehicles cannot see this item.** `edit_timing_c1`'s rows join equal
  heights, so both of its counters are unchanged; the evidence is the oracle-shaped
  test, not a frozen row. That is stated rather than worked around by inventing a
  vehicle print for a shape no frozen workload has.
* **The plan's estimate (−4..−8, a net deletion) does not apply.** Keeping the
  non-root check costs +45 production lines; the trade is deliberate and named.
* **No claim that every join site is now single-read.** The two boundary sites are;
  the other loads in `concat_inner` (the taller side itself, and the re-host cases)
  were already single.
