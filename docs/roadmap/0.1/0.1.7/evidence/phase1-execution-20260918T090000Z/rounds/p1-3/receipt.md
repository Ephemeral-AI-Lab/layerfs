# P1-3 receipt — a materialized branch page's children in one wave

> **Status:** Item receipt. Written once, from [`after/`](after/) (commit
> `ff4d6d328`) under [`../../CONTRACT.md`](../../CONTRACT.md). The **before** arm is
> [`../p1-1/after/`](../p1-1/after/), collected on `79de8e9da`, this item's parent.

## 1. The item

`page_from_wire` expanded a stored branch page by point-reading each child
(`self.read(child, false)`), so a neighbour merge or root collapse that
materialized a page with *f* children paid *f* waves. It now chunks the branch
rows by `BATCH_CHILDREN` and fetches each chunk through `batch_children` — the
merge's own template — decoding, context-checking and charging each child exactly
as before. `materialize` is only reached for **stored** nodes (`pending` nodes
return early), so this is a read-pattern change, never an emission change.

## 2. The discriminating measurement (the item's new test)

A 13,000-inode tree gives the inode table two stored level-1 branch pages; a
300-name release drops one of them below the 64-child fill rule, so the merge that
repairs it materializes both pages. One of those materializations already went
through the merge's batched path; the other is this item's.

| Reading | before (`79de8e9da`) | after (`ff4d6d328`) | predicted |
| --- | ---: | ---: | --- |
| provider waves | **170** | **107** | ↓ ✔ |
| grouped waves (> 32) | `[50, 64]` | **`[50, 64, 64]`** | both materializations grouped ✔ |
| `inodes.read_waves` | 68 | **5** | ↓ ✔ |
| objects demanded | 303 | 303 | identical ✔ |
| `inodes.pages_read` | 134 | 134 | identical ✔ |
| emitted root | `78a3b902…` | `78a3b902…` | identical ✔ |

The 63 waves removed are exactly the second materialization's children.

## 3. The frozen set did not move

No frozen row reaches a stored branch materialization: the D1–D6 trees are one
directory page plus a two-level inode table, and D25's changed path reuses stored
pages. Measured rather than argued: a counter diff of all 29 D-rows and the three
M-rows between [`../p1-1/after/`](../p1-1/after/) and [`after/`](after/) is
**empty** — every counter identical, including the two rows P1-1 moved. The
plan predicted exactly this and it is stated as the no-regression gate, not as
evidence of the item.

## 4. Falsification

`a_branch_materialization_reads_children_in_one_wave` was run against the parent
tree `79de8e9da` with **only the test file copied in**:

```text
a_branch_materialization_reads_children_in_one_wave  FAILED
  left: [50, 64]   right: [50, 64, 64]
```

The six pinned readings (grouped waves, wave count, inode waves, demanded,
pages read, root) all come from that one fixture, which is why the test is the
item's primary evidence and the frozen set only its guard.

## 5. The eight checks (exit codes)

| # | Command | Exit |
| --- | --- | ---: |
| 1 | `python3 core/tools/check_product_boundary.py` | 0 (116 files) |
| 2 | `python3 -m unittest discover -s core/tools -p 'test_*.py'` | 0 (6 OK) |
| 3 | `python3 -m unittest discover -s tools -p 'test_production_loc.py'` | 0 (17 OK) |
| 4 | `cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all --check` | 0 |
| 5 | `cargo +1.85.1 test --manifest-path core/Cargo.toml --workspace --locked --no-fail-fast` | 0 (**444 passed, 0 failed**) |
| 6 | `cargo +1.85.1 clippy --manifest-path core/Cargo.toml --workspace --locked --all-targets -- -D warnings` | 0 |
| 7 | `python3 tools/production_loc.py --files` | 0 |
| 8 | `git diff --check` | 0 |

Parity: `git diff --stat 79de8e9da..ff4d6d328 -- '*tests*'` is
`filesystem_bounds.rs` only (not one of the seven sealed-oracle targets); the
oracles are green and unchanged.

## 6. Determinism

`X1`/`X2` are bit-identical to their gate samples on every counter. The new test
is deterministic by construction (same fixture, same numbers on repeated runs:
170/107 measured twice for each arm while preparing this item). Elapsed figures in
the round are diagnostic only.

## 7. Production LOC

`P1-3 | +25..+50` was the plan's estimate; the actual is **+24**
(`sorted/page.rs` 451 → 475).
`core` **18,862 → 18,886 (delta +24)**; `crates/` reference 65,417 → 65,417;
combined 84,279 → 84,303. Method: `tools/production_loc.py` over the first parent
(`79de8e9da`, via `git archive`) and the staged tree.

## 8. Acceptance

- [x] The new test passes here and fails on the parent tree (output in §4)
- [x] The counter moved in the predicted direction on the fixture that reaches the path (170 → 107 waves; 68 → 5 inode waves) with demanded/pages/root identical
- [x] The frozen set is counter-identical (the plan's no-regression gate), measured across all 29 D-rows
- [x] Eight checks green; parity untouched; LOC disclosed
- [x] No architecture-doc change needed: the documents do not describe the
      materialization read pattern, and this commit changes no boundary, format or
      named bound (the width it reuses is documented by P1-1)
