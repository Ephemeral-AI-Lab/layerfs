# P1-13 receipt — **blocked** (merge fanout 4, multiway cascade)

> **Status:** **Incomplete and reverted, not landed.** No commit for this item; the
> tree is clean at `92f2350e3` and the full workspace is green.
>
> **"Blocked" was my label and it was too strong.** The mechanism is now diagnosed
> (§3.1): my grouped cascade leaves a stale duplicate of a serial in a
> higher-indexed tier than the current row, so `find` and the newest-first final
> stream disagree. That is an ordering bug in my cascade, **not** a property of the
> fanout-4 design, and it is **not** the owner's design question. The item is
> incomplete with a known cause and a known next step — not blocked.

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

**Root cause found (2026-09-18, second derivation).** The tier contents at the
moment of failure, read directly out of each live run by the probe:

```text
tiers = [ ..., None, None,
          Some((32, 1, 40, Some(1))),   <- tier 5: serial 26 present, count = 1
          Some((40, 1, 40, Some(0))) ]  <- tier 6: serial 26 present, count = 0
```

Both tiers are live. `find(serial 26)` walks the tiers in index order, so it stops
at tier 5 and answers **count = 1**. The final row stream reads newest-first, which
for a tiered store means the **highest occupied tier wins**, so it answers from
tier 6 and emits **count = 0** — the superseded row.

That is the bug, stated exactly: the grouped cascade left a **stale duplicate of a
serial in a higher-indexed (newer) tier than the current row**. The store's own
contract is that a lower tier can never hold an older row for a serial than a
higher one does; my cascade broke it. The consequence is two read paths that
disagree — `find`'s "first tier that holds the key" against
`visit_newest_first`'s "highest tier wins" — and whichever one the reducer uses, the
other is wrong.

It is an ordering bug **in my cascade**, not a property of the fanout-4 design: the
grouped loop must leave the accumulator holding the newest data and install it at
the tier the batch targeted, and mine evidently does not when `older_runs` is longer
than `MERGE_FANOUT - 1` (the fallback grouping is the half that only runs on the
deep cascades the failing fixtures exercise; the flat probes that passed never
entered it).

### 3.2 The fix attempt (2026-09-18, third pass): one real bug found and fixed,
the failure still standing

Two things were built and measured in a scratch worktree (removed afterwards):

1. **The invariant test.** `every_tier_read_path_agrees_after_every_spill`
   (`filesystem_ordering_scan.rs`): after every spill, for every serial, the
   `find` answer and the newest-first scan answer must be equal. It passes on the
   unmodified tree, so it is a valid guard rather than a restatement of the bug.
2. **The fanout-4 cascade rewritten cleanly** (multiway `merge_runs` over up to 4
   inputs plus the drain below).

**The real bug, found by the first version of that rewrite.** The drain grouped the
older runs by indexing a vector it was draining:

```rust
let group = &older_runs[cursor..cursor + take];   // WRONG: the queue shrinks
```

`older_runs.drain(..take)` in the merge loop removes the group just consumed, so the
next index slice is taken from **wrong offsets** — an *older* group becomes
`inputs[0]` and therefore "newer" than the accumulator, and a superseded row wins.
The trace shows it directly: at a spill into level 6 the merge
`inputs=[(8,18,25), (8,10,17), (16,2,40), (32,1,32)]` produced `(40,1,40)` with
serial 26 at **`count=0`**, and that run was installed at the newest tier. Fixed by
taking each group out of the queue (`older_runs.drain(..take).collect()`) instead of
indexing it. This is a genuine defect in the code I had written, and it is exactly
the class §3.1 predicted.

**The failure still stands after that fix.** The stale row remains, and the third
pass narrowed its *origin* one step further:

```text
PROBE entry serial=26 about to spill the map (pending has it: false)
PROBE spill pending serial26 count=Some(0) level=1
PROBE retained_binding serial=26 (pending has it: false)
PROBE entry serial=26 find=Some(0)
PROBE retained_binding serial=26 before increment count=Some(0)
```

A spill that **does not hold serial 26 in its pending map** nevertheless writes a
`Count count=0` row for it, and the reducer then adopts that zero back from the run
when the binding is retained. So the stale row is *entering the store through the
spill path* from a source I did not identify — most likely an entry that the
reducer inserts into (or fails to remove from) the map outside the
`entry()`/`note_retained_binding()` path I instrumented. That is where the next pass
should look first, and it is a narrower place than "the merge".

**What is still not done:** the fix. Two candidate sites are now excluded (the merge
selection and the group offsets — the latter fixed), and the spill's input set is
the remaining suspect. The item stays **incomplete with a diagnosed cause and one
defect fixed in the attempt**, not blocked.

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

* **The fix, and its guard (the next concrete step):** drain the older runs
  newest-group-first into the accumulator and install the result at the tier the
  batch targeted, then add the invariant as a test: after every spill, no serial may
  appear in a lower-indexed tier when a higher-indexed tier holds it too. That is
  the assertion §3.1 shows failing.
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
