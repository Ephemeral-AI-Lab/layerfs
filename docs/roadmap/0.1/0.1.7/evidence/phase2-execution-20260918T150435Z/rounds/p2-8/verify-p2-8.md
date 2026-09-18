# verify-p2-8 — the open lane keeps its assembled length

> **author-verified** (single-agent Phase 2: no second reviewer, no subagent).
> Round: [`receipt.md`](receipt.md). Tree: `8dc5b582e`, arm [`after/`](after/);
> before arm [`../v7/after/`](../v7/after/) on `8f0fda297`.

## 1. Reproduction from a clean tree

| # | Command | Exit | Output |
| --- | --- | ---: | --- |
| R1 | `git archive 8dc5b582e \| tar -x -C /tmp/verify-p2-8` | 0 | clean P2-8 tree |
| R2 | `cargo +1.85.1 build --release --offline --locked --manifest-path core/Cargo.toml --examples` | 0 | examples built |
| R3 | `cargo test -p layerfs-storage --test pack_locator` | 0 | `8 passed; 0 failed` |
| R4 | `python3 compare_arms.py rounds/v7/after rounds/p2-8/after` | 0 | `steps compared: 37, differing: 0` |
| R5 | `python3 pack_bytes_census.py rounds/v7/after rounds/p2-8/after` | 0 | `stores compared across 2 arms: 12, identical: True` |

## 2. Falsification answers

**2a — does the test fail on the parent tree?** Yes, by not compiling: the parent's
`append_fits` takes the group slice, so the extended case's running-total argument
is rejected (`E0061`, one error). Stated plainly: for a fit probe whose *shape*
changed, the parent-tree evidence cannot be a failing assertion; it is the
signature mismatch plus the frozen-set identity.

**2b — did the counter move in the predicted direction and magnitude?** P2-8's
prediction is **identity**, not movement - the item removes re-measurement, not
placement decisions - and it is measured twice over: 37/37 frozen steps
bit-identical, and 12 stores identical in pack rows, pack bytes, object rows and
pack-body digests. A repartition would show up in both.

**2c — parity green and unchanged?** 35/35 sealed-oracle tests. `git diff
<parent>..8dc5b582e -- '*tests*'` shows `pack_locator.rs` only, and its assertions
are unchanged (the probe must equal the canonical predicate); the call shape moved
with the API it tests.

**2d — single-variable?** `git show --stat 8dc5b582e`: `pack/layout.rs` (the
probe), `pack/placement.rs` (the state and its maintenance), `cas/placement.rs`
(the caller of `retained_bytes`), the extended boundary case, and §18.4 of the
physical-writing paper. One variable: where the assembled length lives.

**2e — the item's named risk (an off-by-one at `pack_limit`)?** The boundary case
probes the limit in three lanes, including `WholeFile` whose directory entry width
differs from the ordinary one, and it recomputes the canonical length at each
probe rather than trusting the maintained total. It fails if the total drifts by
one byte anywhere, which is exactly the risk named in the plan.

**2f — is elapsed a gate anywhere here?** No, and the receipt says the removed work
has no counter today rather than quoting a wall time as evidence.

## 3. UNVERIFIED

* **The O(g²) → O(g) claim is structural, not measured.** No counter prices the
  removed `assembled_length` passes; the complexity claim is read from the code and
  the architecture document, and the only instrument that would show it is `elapsed`,
  which is diagnostic-grade.
* **`retained_bytes`' new signature is a public-API change** (`LanePlacement` is
  reachable through the public `pack` module). Nothing in the workspace or the
  tests calls it with the old shape; a downstream caller outside this repository
  would need the lane argument removed.
* **The amended commit.** §6 of the architecture document was added by amending the
  item commit before this round was collected; the arm was collected on the
  original tree, whose product source is byte-identical (only the paper changed).
