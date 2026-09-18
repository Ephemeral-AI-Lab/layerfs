# verify-p1-3 — materialized branch children in one wave

> **author-verified** (single-agent Phase 1: no independent reviewer exists; the
> evidence is the reproducibility of the commands below on the named trees).
> Round: [`receipt.md`](receipt.md). Tree: `ff4d6d328`, arm [`after/`](after/);
> before arm [`../p1-1/after/`](../p1-1/after/) (tree `79de8e9da`).

## 1. Reproduction from a clean tree

| # | Command | Exit | Output |
| --- | --- | ---: | --- |
| R1 | `git archive ff4d6d328 \| tar -x -C /tmp/verify-p13/tree` | 0 | clean P1-3 tree |
| R2 | `cargo +1.85.1 test --offline --locked --manifest-path core/Cargo.toml -p layerfs-content --test filesystem_bounds a_branch_materialization` in the archive | 0 | `1 passed` — the six pinned readings reproduce from the archive |
| R3 | the same test with the parent's product source and this commit's test file | 101 | `left: [50, 64]  right: [50, 64, 64]` (§4 of the receipt) |
| R4 | `cargo +1.85.1 test … --test filesystem_bounds` (all 16) in the archive | 0 | 16 passed |

## 2. Falsification answers

**2a — does the new test fail on the parent tree?** Yes: `left: [50, 64]` against
`right: [50, 64, 64]` — the second materialization was point-read there, so no
64-wide wave exists for it. Reproduced in `/tmp/verify-p13/parent`, a clean
archive of `79de8e9da` with only `filesystem_bounds.rs` copied in.

**2b — did the counter move in the predicted direction, and did anything else
move?** Predicted: waves down on a shape that materializes a stored branch, and
**nothing** on the frozen set. Measured: 170 → 107 provider waves, 68 → 5 inode
waves, grouped waves `[50, 64]` → `[50, 64, 64]`, with demanded objects, pages
read and the emitted root identical; and a full counter diff of all 29 frozen
D-rows plus the three M-rows between the arms is **empty**.

**2c — parity green and unchanged?** The test diff in this commit is
`filesystem_bounds.rs` only; the seven sealed-oracle targets are untouched and
were re-run green on the C1 tree
([`../c1-rebaseline/verify-c1.md`](../c1-rebaseline/verify-c1.md) §1).

**2d — single-variable?** Two files: `sorted/page.rs` (the batched branch
expansion) and `filesystem_bounds.rs` (the new test and its fixture). No doc
change is needed: no boundary, format or named bound moves, and the width reused
is the one P1-1 documented.

**2e — error paths.** A child that is absent from the fetched group is
`MissingObject`; a short group is refused by `read_batch`'s cardinality check
before it is zipped; a canonical page over `MAXIMUM_PAGE_BYTES` is still refused
by `decode_page`; a child whose summary disagrees with its parent is still refused
by `check_child`; the subtree-summary check at the end of `page_from_wire` is
untouched. The narrowing path is P1-1's test
(`narrowing_keeps_a_small_scratch_working`), and the refusal path for a lease too
small for even one child is `filesystem_sorted::a_tiny_supported_budget_works_or_refuses_before_allocating`
— both green in the workspace run.

**2f — elapsed as a gate?** No. The gates are the wave counts, the demanded set,
the pages read and the root. The new test takes ~0.7 s in release and ~5 s in
debug; that is a cost note, not a gate.

## 3. UNVERIFIED

* **The fixture is synthetic.** It is a 13,000-inode tree built in-process, not a
  frozen workload; the plan predicted (and the measurement confirms) that no
  frozen row reaches this path. A reader who wants a frozen anchor needs a new
  frozen workload, which Phase 1's contract forbids adding.
* **`batch_children`'s `position`+`remove` is O(C²)** at C = 256 (the plan's own
  risk note (a)). It is unchanged here and not measured.
* **No resident-memory claim**: the group's lease is the batch reservation, not
  RSS.
