# P1-1 receipt — a branch page's children in one wave

> **Status:** Item receipt. Written once, from [`after/`](after/) (commit
> `32eda6f29`) under [`../../CONTRACT.md`](../../CONTRACT.md). The **before** arm is
> [`../p1-2/after/`](../p1-2/after/), collected on `9ec299f13`, which is this item's
> parent commit.

## 1. The item and what it delivers

`BATCH_CHILDREN` **32 → 256**: the widest real demand (a directory branch page's
children are bounded at 232 by the format) rather than the 4,096-id demand
ceiling, which would reserve 33.9 MiB against a 4 MiB lease and be clamped by the
narrowing loop on every call. A full-width reservation is 256 × 8,280 B + one
decode slot ≈ 2.08 MiB.

## 2. Before → after on the frozen set

| Row | counter | before | after | predicted |
| --- | --- | ---: | ---: | --- |
| **D25** `order.default` | `objects.read_waves` | **7** | **5** | ↓ ✔ |
| **D26** `order.forced64` | `objects.read_waves` | **7** | **5** | ↓ ✔ |
| D25/D26 | `inodes.peak_scratch_bytes` | 349,820 | **709,388** | ↑ (bounded by the lease) ✔ |
| D25/D26 | `objects_read` 98 · `bytes_read` 400,354 · `dir_pages_read` 17 · `ino_pages_read` 81 · `emitted` 14 · every spill/run/merge/reference counter | identical | identical | identical ✔ |
| D2, D5, D7 | every counter | identical | identical | unchanged ✔ |

Field-wise on D25: 55 fields compared, **53 identical**, two moved — `read_waves`
7 → 5 and `ino_scratch` 349,820 → 709,388. The same two fields move on D26, and
nothing else moves anywhere in the frozen set (a diff of all 29 D-rows and the
three M-rows between the arms reports exactly those two rows).

**Where the two waves went:** the 4,000-file fixture's *inode* branch has 80
children, so the old width split it 32 + 32 + 16 (three waves) and the new width
reads it in one. The plan predicted the movement would appear in `read_waves` and
that `peak_scratch_bytes` would rise; both are reproduced, and the D1–D6 rows are
unchanged exactly as the plan warned they would be (their branch pages have ≤ 3
children).

## 3. The item's tests

| Test | What it pins |
| --- | --- |
| `a_wide_branch_page_reads_its_children_in_one_wave` | on the 4,000-entry fixture the provider's waves are `[… 32, 32, 16]` (29 waves, boundary 6) before and `[… 80]` (27 waves, boundary 4) after, with the work **bit-identical**: 119 objects demanded, 96 read, 15 directory pages decoded, same root |
| `narrowing_keeps_a_small_scratch_working` | under a 512 KiB lease the 80-child batch narrows (`peak_wave` 80 → 56) instead of refusing, `peak_wave × 8,280 ≤ scratch`, and the emitted root and demanded set are identical to the default-lease run |
| `one_grouped_demand_is_one_charged_wave` | C1's semantics on single calls: one grouped demand = one wave, one point read = one wave |

**Pre-item failures (falsification 2a), reproduced on `9ec299f13` with only the
test file copied in:**

```text
a_wide_branch_page_reads_its_children_in_one_wave  FAILED
  the 80-child branch is one wave, not ceil(80/32):
  [1 … 1, 14, 1 … 1, 32, 32, 16]     left: [] right: [80]
narrowing_keeps_a_small_scratch_working             FAILED
  the batch narrowed: 32 against 32
```

## 4. The pre-authorized pin, and one test generalized

`filesystem_bounds.rs`'s `provider.peak_wave() <= 32` → `<= 256` is the
**pre-authorized** pinned update and the only pinned value this commit changes.

**C1's operation-level wave pin is generalized, not re-pinned.** C1's test
asserted `boundary_waves == 6` on the 4,000-entry fixture. That total is
`point reads + Σ ceil(children / BATCH_CHILDREN)` — a function of the very
constant this item is authorized to raise — so it cannot survive P1-1 while
still being true. The assertion becomes

```rust
assert!(report.boundary_waves <= report.waves.len() as u64);
```

which is **still false before C1** (10 charged waves against 6 provider calls,
the double count) and is width-independent, and the exact per-call semantics move
to the new focused test `one_grouped_demand_is_one_charged_wave`. That test was
run against the **pre-C1** tree (`4a86107fc`) with only the test file copied in
and fails there with `left: 2, right: 1` — the grouped demand charged twice.
C1's receipt carries an appended §10 recording this. The `pages_read >= 15`
assertion is untouched and still holds.

## 5. Two halves of the plan's sketch: declined, recorded

**5.1 `list_after` frontier batching — declined.** The plan's sketch batches the
children of a branch the DFS pops. A listing is **bounded**: it stops on the first
entry that would exceed `max_entries` or `max_bytes`, so a batched branch demands
children the walk may never visit — the provider work is done and the pages are
never decoded. The branch page carries only its own subtree totals
(`node_subtree_count`/`node_subtree_bytes`), **not per-child summaries**, so no
static guard can prove that every child will be visited before the wave is
issued. The frozen callers list with `(16 entries, 4,096 bytes)`: on a directory
whose root branch has *f* children, the walk needs 2 pages today and the batch
would demand *f*+1 (up to 233). Declining is the work-preserving choice; the
alternative — batching only listings that cannot be truncated — has no static
test, so it is not implemented on a guess.

**5.2 `patch.rs` `PageCursor::advance` descent batching — deferred.** The plan's
own risk note (c) records it as invisible on every frozen row (the frozen
attribute trees are one page) and explicitly permits deferring that half; it would
land with no counter evidence and no frozen anchor. Recorded on #178 for the
owner rather than landed unmeasured.

Both are reported on #178 in the same comment as the receipt.

## 6. The eight checks (exit codes)

| # | Command | Exit |
| --- | --- | ---: |
| 1 | `python3 core/tools/check_product_boundary.py` | 0 (116 files) |
| 2 | `python3 -m unittest discover -s core/tools -p 'test_*.py'` | 0 (6 OK) |
| 3 | `python3 -m unittest discover -s tools -p 'test_production_loc.py'` | 0 (17 OK) |
| 4 | `cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all --check` | 0 |
| 5 | `cargo +1.85.1 test --manifest-path core/Cargo.toml --workspace --locked --no-fail-fast` | 0 (**443 passed, 0 failed**) |
| 6 | `cargo +1.85.1 clippy --manifest-path core/Cargo.toml --workspace --locked --all-targets -- -D warnings` | 0 |
| 7 | `python3 tools/production_loc.py --files` | 0 |
| 8 | `git diff --check` | 0 |

Sealed-oracle parity set: green and unchanged (`git diff --stat 9ec299f13..32eda6f29
-- '*tests*'` is `filesystem_bounds.rs` only, not one of the seven targets).

## 7. Determinism

`X1` (`order.forced64` repeat) and `X2` (`c1.subtree-remove` repeat) are
bit-identical to their gate samples on every counter, including the moved ones
(`read_waves 5`, `ino_scratch 709,388`). `elapsed_ns` moved 175.0 ms → 157.5 ms on
the same binary and is diagnostic-only; no line of this receipt is gated on it.

## 8. Production LOC

`P1-1 | +20..+45` was the plan's estimate (which assumed new batching call sites);
the actual is **0** — the constant is the same one line, its new documentation is
comment text, and the rest of the commit is tests and the architecture doc.
`core` **18,862 → 18,862 (delta 0)**; `crates/` reference 65,417 → 65,417;
combined 84,279 → 84,279. Method: `tools/production_loc.py` over the first parent
(`9ec299f13`, via `git archive`) and the staged tree.

## 9. Acceptance

- [x] The counter moved in the predicted direction (D25/D26 `read_waves` 7 → 5) with the predicted memory rise
- [x] No other counter moved (53 of 55 fields identical on D25; every other row identical)
- [x] The item's two new tests pass here and fail on the parent tree (output in §3)
- [x] The only pinned update is the pre-authorized width bound; C1's pin is generalized with its semantics preserved and re-proved on the pre-C1 tree
- [x] Architecture doc (`04-filesystem.md`) updated in the same commit
- [x] Eight checks green; parity untouched; determinism labelled; two declined halves recorded

## 10. Correction (appended 2026-09-18, by P1-4's commit)

P1-4 (`bfb01f262`) generalized one assertion of §3's test. Nothing above is
edited; this section records what changed and why.

`a_wide_branch_page_reads_its_children_in_one_wave` asserted
`provider.demands() == 119`. The provider is shared with validation, whose own
lookups are demanded through it, so that total legitimately drops when P1-4's
record memo removes the repeated reads (119 → 107) — it is a cross-subsystem
total, not a P1-1 counter. The assertion becomes the invariant that survives
(every object the operation read was demanded from the provider) and P1-1's own
counters stay pinned: the 80-wide wave, `boundary_waves == 4`, `objects_read` 96,
`pages_read` 15 and the root. The wave assertion is unaffected because validation's
own batch on that fixture is narrower than 32.
