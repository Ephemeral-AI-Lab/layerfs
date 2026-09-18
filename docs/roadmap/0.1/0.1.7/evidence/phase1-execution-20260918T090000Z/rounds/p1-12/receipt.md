# P1-12 receipt — a page keeps a running total of its row widths

> **Status:** Item receipt. Written once, from [`after/`](after/) (commit
> `9b4eff169`) under [`../../CONTRACT.md`](../../CONTRACT.md). The **before** arm is
> [`../p1-5/after/`](../p1-5/after/), collected on `a3dbeef88`, this item's parent.
> P1-12 has **no counter**: the plan says so explicitly ("no counter measures CPU
> work"), and this receipt says so plainly rather than dressing a memory movement
> up as the item's gain.

## 1. The item and the incremental arithmetic

`Engine::push` re-summed every key's encoded width after every append to decide
whether the page still fit — O(k) per append, O(k²) per page fill, k ≤ 740
(`MAXIMUM_DIRECTORY_LEAF_ROWS`). `Page` now carries `widths: usize`, maintained by
`append_entry`, the single funnel every row passes through (`push`, `edit`,
`merge`, `page_from_wire`). The arithmetic is exactly

```text
   widths(page, level) = Σ F::width(&entry.key, level)   over entry in page.entries
   append:   widths += F::width(&entry.key, level)
   drain:    widths -= F::width(&entry.key, level)        (the row moves to `right`)
   new page: widths = 0
```

and `push` reads `EMPTY_PAGE_BYTES + page.widths` where it used to re-sum. The
figure is a page's **row widths**; `Entry::bytes` is a *subtree's* encoded bytes
and is a different number — the plan's own warning, honoured by giving the total
its own field. A re-grep for direct `page.entries` mutations found only
`append_entry`'s push and `push`'s drain.

## 2. The item's test (the observable, since no counter exists)

`filesystem_sorted::page_widths_running_total_matches_the_recorded_partition`
pins the exact page partition — page bytes and row counts — of two fixtures:

| entries | recorded partition |
| ---: | --- |
| 740 | `[[179, 3], [4118, 194, 4118, 194, 7436, 352]]` |
| 1,500 | `[[359, 7], [4118, 194 ×6, 7100, 336]]` |

The partition *is* what the total feeds (the fill test and the split point), so a
running total that drifts by one row width moves a number here. **Falsification:**
with the `append_entry` increment removed, the test fails (the pages never split);
with only the drain subtraction removed it passes, because the left page is
finalized immediately after the drain and `node()` reads `count`/`bytes` rather
than `widths`. The subtraction is kept so the invariant holds for any page that is
appended to again — stated, not claimed as measured.

## 3. Before → after on the frozen set

| Row | counter | before | after | note |
| --- | --- | ---: | ---: | --- |
| D2, D3, D4, D5 | `directories`/`inodes` `peak_scratch_bytes` | e.g. 67,708 / 67,644 | **67,716 / 67,652** | **+8 B each** |
| D25, D26 | `dir_scratch` | 376,110 | **376,158** | +48 B = 6 pages × 8 |
| D25, D26 | `ino_scratch` | 709,388 | **709,396** | +8 B = 1 page |
| every other counter, every row | — | — | **identical** | roots and emitted bytes identical |

**The plan's risk note is refuted and its Big-O line confirmed.** It said the new
field "must not perturb scratch accounting" and that `peak_scratch_bytes` should
be untouched, while its own memory target says "+1 `usize` per live `Page`". The
measurement settles it: `Engine::page` reserves `size_of::<Page>()`, so every page
slot's declared charge grows by 8 bytes. It is a reservation inside the same
lease — no work moves — and it is recorded rather than smoothed over.

## 4. The eight checks (exit codes)

| # | Command | Exit |
| --- | --- | ---: |
| 1 | `python3 core/tools/check_product_boundary.py` | 0 (116 files) |
| 2 | `python3 -m unittest discover -s core/tools -p 'test_*.py'` | 0 (6 OK) |
| 3 | `python3 -m unittest discover -s tools -p 'test_production_loc.py'` | 0 (17 OK) |
| 4 | `cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all --check` | 0 |
| 5 | `cargo +1.85.1 test --manifest-path core/Cargo.toml --workspace --locked --no-fail-fast` | 0 (**449 passed, 0 failed**) |
| 6 | `cargo +1.85.1 clippy --manifest-path core/Cargo.toml --workspace --locked --all-targets -- -D warnings` | 0 |
| 7 | `python3 tools/production_loc.py --files` | 0 |
| 8 | `git diff --check` | 0 |

Parity: the test diff is `filesystem_sorted.rs`; the seven sealed-oracle targets
are untouched and green, and every root and emitted byte on the frozen set is
identical — the strongest available statement for an item with no counter.

## 5. Production LOC

`P1-12 | +20..+40` was the plan's estimate; the actual is **+2**
(`sorted/page.rs` 475 → 481, `sorted/merge.rs` 282 → 278 — the O(k) sum collapsed
to one line).
`core` **18,969 → 18,971 (delta +2)**; `crates/` reference 65,417; combined
84,386 → 84,388. Method: `tools/production_loc.py` over the first parent
(`a3dbeef88`, via `git archive`) and the staged tree.

## 6. Acceptance

- [x] The change is implemented with the plan's arithmetic, its own field and the single funnel
- [x] A test pins the observable (the partition) and fails when the total is not maintained
- [x] The frozen set is measured: one explainable movement (+8 B per page slot in `peak_scratch_bytes`) and nothing else
- [x] The plan's contradicting risk note is refuted in writing
- [x] No counter is claimed where none exists — stated plainly on #178
- [x] Eight checks green; architecture doc updated in the same commit; LOC disclosed
