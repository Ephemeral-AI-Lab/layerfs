# P1-4 receipt — validation's record demands behind one memo

> **Status:** Item receipt. Written once, from [`after/`](after/) (commit
> `bfb01f262`) under [`../../CONTRACT.md`](../../CONTRACT.md). The **before** arm is
> [`../p1-3/after/`](../p1-3/after/), collected on `96c51d0df`, this item's parent.
> The observable is V1's print (`filesystem_timing_c1`'s `validation:` line), which
> is exactly the vehicle this item's plan nominated.

## 1. What the item does

Validation demanded base inode records one at a time (`lookup_many` over a single
serial), so an update binding *k* stored children paid *k* root descents, and the
three walks (decision loop, alias walk, effective-cycle walk) re-demanded the same
serials. `ValidationState` memoizes the records; `check` prefetches every serial
its loop will demand in **one** grouped demand; the memo is threaded through the
other two walks.

The accounting rule that makes the counters honest: **the batch pays the pages and
waves it reads; the demand charge is paid where the demand is made.** A memo hit
therefore charges one demand and no I/O, which is why `inode_demands` and
`objects_read` are bit-identical and only the I/O moves. Absence is not memoized —
a serial with no stored record keeps today's lookup and today's charge.

## 2. Before → after on the frozen set

| Row | counter | before | after | predicted |
| --- | --- | ---: | ---: | --- |
| **D2** `c1.directory-update` | `validation.read_waves` | **42** | **2** | ↓ sharply ✔ |
| **D2** | `validation.inode_pages_read` | **42** | **2** | ↓ sharply ✔ |
| **D2** | `validation.inode_demands` | 21 | **21** | identical ✔ |
| **D2** | `validation.objects_read` | 21 | **21** | identical ✔ |
| **D4** `c1.hardlink-move` | `validation.read_waves` | **6** | **1** | ↓ ✔ |
| **D4** | `validation.inode_pages_read` | **6** | **1** | ↓ ✔ |
| **D4** | `validation.inode_demands` | 6 | **6** | identical ✔ |
| D1, D3, D5, D6 | every `validation` field | 0 | 0 | identical ✔ |

`entries_examined` is 0 on all six rows both before and after (none of them
renames a directory), and `directory_pages_read` is 0 throughout — the plan
predicted that the directory-page half of the memo would be invisible on D1–D6 and
said so; it is stated here rather than presented as a movement.

**Nothing else moved:** a counter diff of all 29 D-rows and the three M-rows
between the arms, excluding the `validation:` line, is **empty**. Every root and
every emitted byte is identical.

## 3. The item's test

`binding_lookups_are_batched_per_phase`: renaming every file of a 500-entry and a
1,500-entry directory demands **1,001** and **3,001** records respectively with the
**same** wave count (≤ 4) and pages an order of magnitude below the demand count.
On the parent tree the same test fails at `left: 2002, right: 6002` — the wave
count follows the demands, 2 per demand.

The plan's test (ii) (`the_three_walks_share_one_record_set`) is covered by the
frozen rows rather than by a new test: D2's 21 demands are made by the decision
loop and again by the effective-cycle walk, and the receipt's 42 → 2 movement is
that sharing measured end to end. Test (iii) (`memo_preserves_the_work_limit_charge`)
is covered by the existing `the_cycle_check_work_limit_is_reachable_and_reported`
and `a_directory_whose_subtree_exceeds_the_entry_ceiling_cannot_be_rebound`, which
pin `entries_examined` at the refusal point and pass unchanged.

## 4. One cross-subsystem assertion generalized (recorded)

P1-1's `a_wide_branch_page_reads_its_children_in_one_wave` pinned
`provider.demands() == 119`. The provider is shared with validation, whose own
lookups are demanded through it, so that total legitimately drops when the memo
removes repeated reads — it is not a P1-1 counter. The assertion becomes the
invariant that survives (every object the operation read was demanded from the
provider), and P1-1's own counters (`objects_read` 96, `pages_read` 15, the
80-wide wave, `boundary_waves` 4, the root) stay pinned. Appended to P1-1's receipt
as §10. This is the same class of correction as C1's §10: a test pinning a total
that crosses a subsystem boundary the later item legitimately changes.

## 5. Error-order risk (the item's largest)

The prefetch reads pages the lazy loop might never have reached, so an input with
a corrupt base page **and** an earlier semantic error can surface the corruption
first. Error **classes** are unchanged, and the topology suite — every acceptance
and refusal case — is green. No test pins which of two errors on one input is
reported first (verified by reading the suite: its refusal cases are
single-error). On a refusal path the prefetch may also charge demands the loop
never makes, so `inode_demands` is bit-identical on the success path and may
differ on an error path; the frozen rows are all success paths.

## 6. The eight checks (exit codes)

| # | Command | Exit |
| --- | --- | ---: |
| 1 | `python3 core/tools/check_product_boundary.py` | 0 (116 files) |
| 2 | `python3 -m unittest discover -s core/tools -p 'test_*.py'` | 0 (6 OK) |
| 3 | `python3 -m unittest discover -s tools -p 'test_production_loc.py'` | 0 (17 OK) |
| 4 | `cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all --check` | 0 |
| 5 | `cargo +1.85.1 test --manifest-path core/Cargo.toml --workspace --locked --no-fail-fast` | 0 (**445 passed, 0 failed**) |
| 6 | `cargo +1.85.1 clippy --manifest-path core/Cargo.toml --workspace --locked --all-targets -- -D warnings` | 0 |
| 7 | `python3 tools/production_loc.py --files` | 0 |
| 8 | `git diff --check` | 0 |

Parity: `git diff --stat 96c51d0df..bfb01f262 -- '*tests*'` is
`filesystem_bounds.rs` only; the seven sealed-oracle targets are untouched and
green.

## 7. Production LOC

`P1-4 | +80..+150` was the plan's estimate; the actual is **+77**
(`filesystem/validate.rs` 576 → 653).
`core` **18,886 → 18,963 (delta +77)**; `crates/` reference 65,417 → 65,417;
combined 84,303 → 84,380. Method: `tools/production_loc.py` over the first parent
(`96c51d0df`, via `git archive`) and the staged tree.

## 8. Acceptance

- [x] The counter moved in the predicted direction (D2 waves 42 → 2, pages 42 → 2; D4 6 → 1) with `inode_demands`/`objects_read` bit-identical
- [x] No other counter moved (0-row diff across D1–D29 and M1–M3 outside the validation line)
- [x] The item's test passes here and fails on the parent tree (`2002` vs `6002`)
- [x] Roots and emitted bytes identical; parity green and unchanged
- [x] Error-order risk and the refusal-path demand difference stated, not hidden
- [x] Eight checks green; LOC disclosed; the one generalized cross-subsystem assertion recorded
