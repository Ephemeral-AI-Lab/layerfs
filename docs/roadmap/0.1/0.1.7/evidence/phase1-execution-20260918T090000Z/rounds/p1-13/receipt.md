# P1-13 receipt — **blocked** (merge fanout 4, multiway cascade)

> **Status:** **Blocked, reverted, not landed.** No commit for this item; the tree
> is clean at `92f2350e3` and the full workspace is green.
> This receipt records the attempt, the exact failing artifacts and the
> dispositions the owner needs. It is not a decline of the item's premise: the
> premise holds, the implementation has a correctness bug I did not resolve.

## 1. The item, as planned

Merge participations `log2(B)` → `log4(B)`: replace the sequential two-way cascade
in `Runs::spill` (`runs.rs`) with a multiway `merge_runs` over up to 4 inputs — the
spilled batch plus the three occupied tiers below it — so a cascade into tier `k`
costs `⌈k/3⌉` merges instead of `k − 1`. Design (a) of the plan; design (b)
(true size-tiered) is explicitly a much larger rewrite. Target: D26 `rows_written`
25,760 → ~12–15k, `merges` 61 → ~18–24.

## 2. What was built

* `merge.rs`: `MERGE_FANOUT = 4`; `merge_runs(backing, inputs: &[&Run], …)` takes up
  to four runs, newest first, keeps one `RunReader` per input, and picks the
  minimum serial with the **first** (newest) input winning a serial more than one
  input holds. Both call sites updated (`spill`'s cascade and the release-path
  consolidation in `runs.rs`).
* `runs.rs`: `spill` collects every occupied tier below the target at once and
  merges it in groups of at most `MERGE_FANOUT − 1` older runs plus the batch,
  reserving `Σ input rows × ROW_BYTES` (the bound the reserve can prove) and
  keeping the `merge_input_bytes` accounting per group.

## 3. What failed

Building the fanout-4 cascade broke **seven** tests; six were then fixed by
correcting one real bug in the multiway selection, and three remain, all of them
data-correctness failures rather than count assertions:

| Test | Failure after the selection fix |
| --- | --- |
| `filesystem_ordering::a_high_pending_ceiling_runs_spill_free_to_the_byte_bound` | "spilling is planned work, not a different result": the spilling and non-spilling runs produce **different filesystem roots** (`a0fb02c1…` vs `c04f986b…`) |
| `filesystem_ordering::the_pending_threshold_changes_only_where_the_rows_live` | `InvalidRecord("new inode without binding")` during the spilling build |
| `filesystem_bounds::every_reported_owner_is_nonzero_where_work_happened_and_zero_where_it_did_not` | `left: 28, right: 0` — an owner the operation should have reported as untouched |

The bug the fix removed, recorded because it is the trap this item hides: in the
multiway loop I first advanced **only** the winner's reader past a duplicated
serial, so an older input's superseded row was written after the newer one and a
"new inode without binding" reached the reducer. Advancing every reader whose
current row carries the winning serial removed that specific failure (7 tests → 3)
but did not restore result equality.

**The remaining failure is a real difference in the merged row set, not a counter
difference.** The old cascade merges the batch with the *nearest* occupied tier
first and walks upward; grouping three tiers at once changes which row wins when a
serial appears in more than two inputs, and the reducer's "a newer row already
incorporates the older effects" invariant is evidently not preserved by
first-input-wins alone across a three-way absorption. That is a genuine design
question about the merged representation (which rows the union must carry when
three tiers carry the same serial), not a coding slip.

## 4. Disposition

The change was reverted in full (`git checkout -- merge.rs runs.rs`); the tree at
`92f2350e3` is clean and `cargo +1.85.1 test --workspace` is **456 passed / 0
failed**. The item is **blocked**, and the honest state is that its premise is
untested:

* **What the owner needs to rule on:** whether the union of a three-tier absorption
  may keep the newest row alone (what I implemented and what the plan's §P1-13 text
  assumes: "newest-wins-on-tie reproduced exactly"), or whether the reducer's
  invariant requires the *per-tier* row to survive a multiway merge. The former is
  what `log4` participations buy; if the latter is required, the win is smaller and
  design (b) is the honest form.
* **What is not claimed:** nothing about `rows_written`, `merges` or `runs_created`.
  D26's anchors (`rows_written` 25,760, `merges` 61, `runs_created` 124) are intact
  because no part of this attempt was committed.
* **What would finish it:** a documented precedence rule for a serial carried by
  more than two tiers, a unit test that pins it (the plan's
  `a_four_way_cascade_merges_three_tiers_in_one_pass` asserts counts, not
  precedence, and the precedence case it sketches is exactly the one that failed
  here), and then the count re-derivations the plan lists.

## 5. Evidence

| # | Command | Exit | Output |
| --- | --- | ---: | --- |
| R1 | `cargo +1.85.1 test --manifest-path core/Cargo.toml --locked -p layerfs-content --test filesystem_ordering --test filesystem_bounds` (with the cascade) | 101 | 7 failed (the table above, before the selection fix) |
| R2 | the same after the selection fix | 101 | 3 failed: the two root-equality/dropped-row failures and the owner-count failure |
| R3 | `git checkout -- …/references/merge.rs …/references/runs.rs` then `cargo +1.85.1 test --workspace --locked --no-fail-fast` | 0 | **456 passed / 0 failed** |

The failing runs are not stored in this directory: they ran on uncommitted working
trees, and the commands above reproduce them from the described diff. Stated that
way rather than as a receipt for a tree that no longer exists.
