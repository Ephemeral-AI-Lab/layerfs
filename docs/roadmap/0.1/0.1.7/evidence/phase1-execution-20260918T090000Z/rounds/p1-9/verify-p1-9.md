# verify-p1-9 — a pure deletion never walks the rightmost path

> **author-verified** (single-agent Phase 1: no independent reviewer exists; the
> evidence is the reproducibility of the commands below on the named trees).
> Round: [`receipt.md`](receipt.md). Tree: `d42cd969e`, arm [`after/`](after/);
> before arm [`../v4/after/`](../v4/after/) (tree `ab82f28a5`).

## 1. Reproduction from a clean tree

| # | Command | Exit | Output |
| --- | --- | ---: | --- |
| R1 | `git archive d42cd969e \| tar -x -C /tmp/verify-p19` | 0 | clean P1-9 tree |
| R2 | `cargo +1.85.1 build --release --offline --locked --manifest-path core/Cargo.toml --examples` | 0 | vehicles rebuilt |
| R3 | `edit_timing_c1 --case delete` | 0 | `nodes_read: 4`, `edit_nodes_read: 7`, root `7d3eb265…` — identical to `after/logs/M2` |
| R4 | `edit_timing_c1` (no argument) | 0 | `nodes_read: 9`, `edit_nodes_read: 10` — identical to `after/logs/D27` |
| R5 | `--case shrink` | 0 | `nodes_read: 11`, `edit_nodes_read: 0` — identical to `after/logs/M3` |
| R6 | `cargo +1.85.1 test … --test edit_localized` | 0 | 7 passed (both new tests) |
| R7 | the same tests against the un-gated product source | 101 | `left: 5, right: 4` |

## 2. Falsification answers

**2a — do the new tests fail on the pre-item tree?** Yes: with the gate removed
and the test file unchanged, `a_pure_deletion_never_walks_the_rightmost_path`
fails at `left: 5, right: 4` — the walk's one extra `load_node`. The second test
(over-gating) passes on both trees by design: it pins that the walk *stays* for an
overwrite, so it is a guard, not a falsification.

**2b — did the counter move in the predicted direction, and did anything else
move?** Predicted: one walk fewer per pure deletion, nothing else. Measured on
V4's `edit_nodes_read`: M2 8 → 7; D27 10 (negative control) and M3 0 unchanged;
`nodes_read` and every root unchanged; a diff of all 29 frozen rows and the three
M-rows excluding the two edit-node lines is **empty**. The plan's prediction that
the *printed* `nodes_read` would drop is refuted (§3 of the receipt) and recorded.

**2c — parity green and unchanged?** The test diff is `edit_localized.rs`; the
seven sealed-oracle targets are untouched and green (re-run on the C1 tree,
[`../c1-rebaseline/verify-c1.md`](../c1-rebaseline/verify-c1.md) §1). Every
emitted root on the frozen set is bit-identical.

**2d — single-variable?** Two files: `file/edit/apply.rs` (the gate) and
`edit_localized.rs` (the two tests). No doc change: no boundary, format or named
bound moves, and the hint's semantics are unchanged for the edits that consume it.

**2e — error paths.** A failing insert (declared length ≠ source length) is still
refused with `InvalidEdit { what: "replacement length" }`, now before the walk
rather than after it — the same class at the same check, with one walk less work;
`edit_timing.rs`'s and `edit_bounds.rs`'s failing-insert cases are green. The
empty case (`removed_len == 0` with no tail) and the whole-file route are
untouched (M3 identical). No malformed-input path changes.

**2f — elapsed as a gate?** No. The gate is `edit_nodes_read` and the roots.

## 3. UNVERIFIED

* **One draft load is not one provider round trip.** The item removes a
  `load_node` call; on the available shape that node is a draft, so no provider
  demand disappears. A shape where the rightmost path is *stored* would show a
  demand drop too; none is in the frozen set or in V2, and I did not build one.
* **The walk's other effect is summary validation of the left boundary**
  (`tree.rs:163-168`), which the gate removes for pure deletions — the plan calls
  it redundant because the split already validated those nodes, and no test pins
  it either way. Stated, not proven.
