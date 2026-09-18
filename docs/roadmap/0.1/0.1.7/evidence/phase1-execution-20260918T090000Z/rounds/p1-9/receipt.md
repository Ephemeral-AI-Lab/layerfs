# P1-9 receipt — a pure deletion never walks the rightmost path

> **Status:** Item receipt. Written once, from [`after/`](after/) (commit
> `d42cd969e`) under [`../../CONTRACT.md`](../../CONTRACT.md). The **before** arm is
> [`../v4/after/`](../v4/after/), collected on `ab82f28a5`, this item's parent.
> V4 exists because the frozen vehicle's `nodes_read` is the *provider-demand*
> count and this item's work is draft-served — see §3.

## 1. The item

`rightmost_payload` computes the physical predecessor hint for the replacement
scan: one `load_node` per level of the left subtree's right boundary. It ran for
every edit, including pure deletions, whose replacement scan never runs (`middle`
is `None` when the declared replacement length is zero). The walk is now folded
into the `else` arm that computes `middle`.

Gating on the **declared** `replacement_len` (not `removed_len`) keeps the hint
for same-length overwrites and pure inserts. The replacement-length source check
stays before the gate, so a caller error is refused before any scan work; a
failing insert performs one walk less before failing, with the same error class.

## 2. Before → after (V4's `edit_nodes_read`, the operation's own load count)

| Row | counter | before | after | predicted |
| --- | --- | ---: | ---: | --- |
| **M2** `--case delete` | `edit_nodes_read` | **8** | **7** | −(rightmost path) = −1 ✔ |
| D27 (40,000-byte overwrite) | `edit_nodes_read` | 10 | **10** | negative control ✔ |
| M3 `--case shrink` | `edit_nodes_read` | 0 | **0** | whole-file route ✔ |
| D27/M2/M3 | `nodes_read` (provider demands) | 9 / 4 / 11 | **9 / 4 / 11** | unchanged |
| D27/M2/M3 | `edited_root` | `b6dca354…` / `7d3eb265…` / `4a45d246…` | identical | identical |

Every other counter on all 29 frozen rows and the three M-rows is identical
between the arms (a diff excluding the two edit-node lines is **empty**).

## 3. The plan's expectation for the printed counter is refuted — and why

The plan expected `edit_timing_c1 --case delete`'s printed `nodes_read` to drop
"strictly lower by the rightmost path length". It does not: that field is
`demanded.len()`, the **provider-demand** count, and the walk's one node on this
shape is a draft the split already built — `EditCounters::nodes_read` charges the
load, the provider is never asked, so the demand count is 4 with and without the
walk. V4 was added for exactly this: `edit_nodes_read` (10/8/0 on the V4 rows)
sees the load, and it moves 8 → 7. The refutation is recorded, not smoothed over;
the item's gain is real but it is one draft load, not one provider round trip.

## 4. The item's tests

| Test | What it pins |
| --- | --- |
| `edit_localized::a_pure_deletion_never_walks_the_rightmost_path` | the deletion's own count is **4** (the split's loads) and its two provider demands are the file state and the mapping path |
| `edit_localized::an_overwrite_still_demands_the_predecessor_path` | the overwrite is **7** loads and **6** demands — the negative pin against over-gating |

**Pre-item failure:** with the gate removed and the tests unchanged, the first
test fails with `left: 5, right: 4` — the walk's extra load, exactly one.

## 5. The eight checks (exit codes)

| # | Command | Exit |
| --- | --- | ---: |
| 1 | `python3 core/tools/check_product_boundary.py` | 0 (116 files) |
| 2 | `python3 -m unittest discover -s core/tools -p 'test_*.py'` | 0 (6 OK) |
| 3 | `python3 -m unittest discover -s tools -p 'test_production_loc.py'` | 0 (17 OK) |
| 4 | `cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all --check` | 0 |
| 5 | `cargo +1.85.1 test --manifest-path core/Cargo.toml --workspace --locked --no-fail-fast` | 0 (**447 passed, 0 failed**) |
| 6 | `cargo +1.85.1 clippy --manifest-path core/Cargo.toml --workspace --locked --all-targets -- -D warnings` | 0 |
| 7 | `python3 tools/production_loc.py --files` | 0 |
| 8 | `git diff --check` | 0 |

Parity: the test diff is `edit_localized.rs` (not one of the seven sealed-oracle
targets); the oracles are green and unchanged, and every emitted root on the
frozen set is identical.

## 6. Production LOC

`P1-9 | +3..+8` was the plan's estimate; the actual is **0** — the gate moves
existing statements and adds only comments, so the counted lines are unchanged.
`core` **18,963 → 18,963 (delta 0)**; `crates/` reference 65,417; combined 84,380.
Method: `tools/production_loc.py` over the first parent (`ab82f28a5`, via
`git archive`) and the staged tree.

## 7. Acceptance

- [x] The counter moved on the item's shape (M2 `edit_nodes_read` 8 → 7) with the negative control unmoved (D27 10) and every other counter identical
- [x] The item's tests pass here and fail on the un-gated tree (`left: 5, right: 4`)
- [x] Roots identical everywhere; parity green and unchanged
- [x] The plan's prediction for the printed `nodes_read` is refuted in writing (§3)
- [x] Eight checks green; LOC disclosed
