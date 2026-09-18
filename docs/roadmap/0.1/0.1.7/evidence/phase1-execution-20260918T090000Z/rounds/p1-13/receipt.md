# P1-13 receipt — **blocked** (merge fanout 4, multiway cascade)

> **Status:** **Incomplete and reverted, not landed.** No commit for this item; the
> tree is clean at `92f2350e3` and the full workspace is green.
>
> **"Blocked" was my label and it was too strong.** What is established is that my
> implementation produces wrong rows and I did not find the mechanism. It is *not*
> established that the fanout-4 design cannot be finished, and this receipt does not
> claim it. The owner's question in §4 is a real design question, but it is not
> proven to be *the* cause — see §3.1, which narrows the failure to a stale row
> winning in the merged stream.

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

### 3.1 The failure, narrowed to one serial (2026-09-18, after the revert)

Re-deriving the attempt in a scratch worktree (later removed) and instrumenting the
row path for the failing fixture:

`the_pending_threshold_changes_only_where_the_rows_live` builds with
`maximum_pending_records: 1`, so every row spills. The reducer then fails on
**serial 26** — a newly allocated inode whose retained-binding tally reaches it as
**0**. The instrumented trace of that serial, in order:

```text
spill                              Count count=0     <- the inode exists, not yet bound
merge winner from input 0/2/3      Count count=0
spill                              Count count=1     <- its binding is retained
merge winner from input 1/2/3      Count count=1
spill                              Count count=1
merge winner from input 1/2/3/0    Count count=1     <- still 1 here
merge winner from input 0          Count count=1
merge winner from input 0          Count count=1
merge winner from input 0          Count count=0     <- a STALE row wins
```

So the merge stream emits, for a serial the store already holds at `count=1`, a
**superseded `count=0` row** — i.e. the merged stream is not newest-wins for that
serial. That is a correctness violation of the merge's own contract
(`merge.rs`: "a newer row already incorporates the older effects"), and it is the
mechanism that reaches the reducer as "new inode without binding".

**What I could not determine.** Whether the stale row comes from (a) a
tier-ordering error in my grouped cascade (the accumulator not being newer than the
group it is merged with), (b) the read path (`find` / `visit_newest_first`)
answering from a stale tier for a serial the merged output already covers, or
(c) something else in the merge. A controlled probe of `merge_runs` alone — twelve
spills of eight rows with **every serial carried by every batch**, so every merge
is a genuine three-way collision — came out **correct** (`live_runs=2`, every serial
holding its newest write). That probe passing while the end-to-end fixture fails is
the state I stopped at: the bug is real, reproducible and localized to the
spill/merge/read interaction, but I have not isolated which of the three it is.

Consequences for the disposition:

* The §4 owner question is **not proven to be the cause**. It may still be worth
  ruling on, but it should not be read as the blocker.
* Neither is the item proven finishable. The honest label is **incomplete**: a
  reproducing fixture, an instrumented trace, and no root cause.

## 4. Disposition

The change was reverted in full (`git checkout -- merge.rs runs.rs`); the tree at
`92f2350e3` is clean and `cargo +1.85.1 test --workspace` is **456 passed / 0
failed**. The item is **blocked**, and the honest state is that its premise is
untested:

* **What would unblock it (the next concrete step, not an owner ruling):** isolate
  §3.1's three candidates with the existing `visit_newest_first` probe — read the
  store after each spill and assert, per serial, that the newest spilled row is the
  one that comes back. The moment that assertion fails, the tier index and merge
  group are visible together, which is what distinguishes (a) from (b).
* **What the owner may still want to rule on, independently:** whether the union of
  a three-tier absorption may keep the newest row alone (what I implemented and what
  the plan's §P1-13 text assumes: "newest-wins-on-tie reproduced exactly"), or
  whether the reducer's invariant requires the *per-tier* row to survive a multiway
  merge. The former is what `log4` participations buy; if the latter is required, the
  win is smaller and design (b) is the honest form. **This is a design preference
  question, not the diagnosed cause.**
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
