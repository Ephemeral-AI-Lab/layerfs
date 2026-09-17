# C1 receipt — counter correctness (`pages_read` batched decodes + `read_waves` double count)

> **Status:** Item receipt. Written once, from the collections in `before/`
> (parent commit `4a86107fc`) and `after/` (C1's own commit), under
> [`../../CONTRACT.md`](../../CONTRACT.md) and the Phase 0 contract it inherits.
> C1 carries **no** Big-O or canonical change: it makes the two counters below
> report the work the operation actually did, and re-baselines every row they
> touch. Row #178 **C1** (its two halves are P1-11's `pages_read` fix and the
> Phase 0 ruling-4 `read_waves` double count).

## 1. What was wrong

| # | Counter | Site | Defect |
| --- | --- | --- | --- |
| 1 | `SortedWork.pages_read` | `filesystem/sorted/page.rs:220-259` `batch_children` | The doc says "Pages read, **including batched ones**", but only the point read charged it (`:185-186`). A grouped fetch charged 0 pages and then decoded `chunk` children without charging them, so every batched decode was invisible. |
| 2 | `ObjectWork.read_waves` | `filesystem/objects.rs:89-108` `read_batch` | The call charged the wave **twice**: `note_read` added one (`:124`) and the next statement added another (`:107`). One grouped demand is one provider call, so it is one wave. |

## 2. The change (one commit, two counter statements)

- `page.rs::batch_children` charges `pages_read += chunk` and `read_waves += 1` on
  the successful grouped path, next to the lease shrink that already proves what
  the call returned. The point-read fallback (`Ok((1, Vec::new(), None))`) stays
  uncounted because its caller point-reads, which already charges.
- `objects.rs::read_batch` drops the second `read_waves` increment; `note_read`
  remains the single wave charge, which is also what the point read uses.

No algorithm, bound, format, emission order or persisted byte changed. The
operation reads exactly the same objects in the same order.

## 3. Before → after on the frozen workload set

Identities: source `4a86107fc`/C1's commit, clean tree over `core/crates`,
release, `cargo +1.85.1`, `--locked`, one worker
(`LAYERFS_CONSTRUCTION_WORKERS=1`), one sample per workload. Every
`elapsed_ns` below is **diagnostic-only** (Phase 0 §6: +17.6% same-binary spread).

### 3.1 Rows the fix moves

| Workload | counter | before | after | Δ |
| --- | --- | ---: | ---: | ---: |
| `c1.directory-update` (D2) | `objects.read_waves` | **4** | **3** | −1 |
| `c1.directory-update` (D2) | `inodes.pages_read` | **1** | **5** | +4 |
| `c1.subtree-remove` (D5) | `objects.read_waves` | **4** | **3** | −1 |
| `c1.subtree-remove` (D5) | `inodes.pages_read` | **1** | **5** | +4 |
| `fs.c1.directory-update` (D7) | boundary `waves` | **4** | **3** | −1 |
| `fs.c1.directory-update` (D7) | inodes `pages_read` | **1** | **5** | +4 |

The wave drop is exactly the double count: each of these operations issues
**one** grouped boundary demand, which the old code charged twice. The page rise
is the formerly invisible batched decode: `inodes.pages_read` was reporting the
single leaf the point read charged while the operation decoded five pages of
that leaf's path.

`fs.pipeline.directory-update` (D9) prints no counter pair; its root and readback
are unchanged (§3.3).

### 3.2 Rows the fix does **not** move (the honest part)

| Workload | counters before → after |
| --- | --- |
| `order.default` (D25) | all identical: `spilled 0`, `rows_read 0`, `rows_written 0`, `runs 0`, `merges 0`, `dir_pages_read 2`, `ino_pages_read 1`, `objects_read 98`, `read_waves 11`, `bytes_read 400354`, `peak_pending 2001` |
| `order.forced64` (D26) | all identical: `spilled 3968`, `rows_read 59007`, `rows_written 25760`, `runs_created 124`, `merges 61`, `dir_pages_read 2`, `ino_pages_read 1`, `objects_read 98`, `read_waves 11`, `bytes_read 400354`, `peak_backing 568320` |
| `edits.*` (D10–D24), D27 | roots, emitted objects and canonical bytes identical (D27 `nodes_read 9`, `edited_root b6dca354…`) |
| `c2.ceiling` / `c2.small` (D28/D29) | unchanged (C2 is not touched: the double count is in C1's boundary, not `Store::read_batch`) |

**The `order` rows prove the wave fix is narrow**: their boundary waves are 11
both before and after, because the probe's read path batches through
`lookup_many` (which counts its own waves) rather than through
`FilesystemObjects::read_batch`. Phase 0 recorded the D25/D26 `read_waves 11`
values, and they stand — the ×2 inflation Phase 0's ruling 4 predicted applies to
the boundary's grouped demands, which is exactly the D2/D5/D7 rows above.

### 3.3 Parity

Every frozen row's **root identities** are bit-identical before and after
(`c1.directory-update 820dcf462ad2279e…`, `c1.subtree-remove 3855544b746bae6a…`,
`fs.* 681ab7e59a612a51…`, `order.* 14 emitted`, D27 `edited_root b6dca354…`).
The 34-test sealed-oracle parity set is green and unchanged (§5).

## 4. The counter, pinned

`filesystem_bounds::a_grouped_demand_is_one_wave_and_every_page_it_decoded`
(the item's new test) drives the three-name rename over a 4,000-entry directory
and asserts the exact boundary readings:

| reading | pre-item tree | C1's tree |
| --- | ---: | ---: |
| `ObjectWork.read_waves` (5 point reads + 1 grouped demand) | **10** | **6** |
| `SortedWork.pages_read` (the grouped call's 14 decoded children) | **1** | **15** |

The pre-item figure was reproduced by stashing only the two product edits and
re-running the new test on the unchanged test file: it failed at the wave
assertion with `left: 10, right: 6`, which is the double count plus the batched
demand's own child reads. The page assertion (`pages_read >= 15`) fails on that
tree as well (`1`).

## 5. The eight checks (exit codes, C1's tree)

| # | Command | Exit | Counts |
| --- | --- | ---: | --- |
| 1 | `python3 core/tools/check_product_boundary.py` | 0 | 116 production files scanned |
| 2 | `python3 -m unittest discover -s core/tools -p 'test_*.py'` | 0 | 6 tests OK |
| 3 | `python3 -m unittest discover -s tools -p 'test_production_loc.py'` | 0 | 17 tests OK |
| 4 | `cargo +1.85.1 fmt --manifest-path core/Cargo.toml --all --check` | 0 | — |
| 5 | `cargo +1.85.1 test --manifest-path core/Cargo.toml --workspace --locked --no-fail-fast` | 0 | **436 passed, 0 failed** |
| 6 | `cargo +1.85.1 clippy --manifest-path core/Cargo.toml --workspace --locked --all-targets -- -D warnings` | 0 | — |
| 7 | `python3 tools/production_loc.py --files` | 0 | see §6 |
| 8 | `git diff --check` | 0 | — |

Sealed-oracle parity set, each binary re-run at C1's tree: `fixture_seal` 2,
`filesystem_reference` 2, `edit_reference` 2, `object_identity` 11,
`filesystem_codec` 9, `filesystem_updates` 6, `filesystem_profile` 2 — **34
passed, 0 failed, unchanged**.

Item suites beyond the workspace run: `filesystem_bounds` 12, `filesystem_sorted`
6, `filesystem_read` 4, `filesystem_attributes` 11, `filesystem_ordering` 12,
`filesystem_ordering_scan` 2, `filesystem_topology` 17, `filesystem_updates` 6,
`filesystem_limits` 7, `filesystem_timing` 2 — all green.

## 6. Production LOC

`C1 | 0 ± 5` was the plan's estimate; the actual is **+1**.

`core` **18,792 → 18,793 (delta +1)**; `objects.rs` 102 → 101 (−1),
`sorted/page.rs` 449 → 451 (+2); `crates/` reference 65,417 → 65,417 (0);
combined 84,209 → 84,210 (+1). Method: `tools/production_loc.py` counted the
first parent (`4a86107fc`, via `git archive`) and the staged tree; the test file
is outside the production scope.

## 7. Determinism (labelled diagnostics, both arms)

| # | Re-run | Result |
| --- | --- | --- |
| X1 | `order.forced64` repeat | every counter bit-identical (`spilled 3968` … `read_waves 11`); `elapsed_ns` 152,628,083 → 156,409,125 (**+2.5%**) and again 180,771,458 on the diagnostic arm (**+18%** from the first) |
| X2 | `c1.subtree-remove` repeat | every counter bit-identical (`waves 3`, `inodes.pages_read 5`); `elapsed_ns` 328,750 → 238,791-class spread, diagnostic only |

Counters are deterministic; elapsed is not, and no Phase 1 box is gated on it.

## 8. Finding reported, not fixed (the handoff's rule)

While building the test above, a **third** counter defect was found that is
**not** in the plan's C1 scope and is **not** fixed here:

- **`SortedWork.pages_read` loses whole engines' work.** `Engine::apply_root`
  (`filesystem/sorted/finish.rs:19-117`) returns `engine.work`, and
  `update.rs:239-249` accumulates exactly that value once per directory update.
  A second engine instantiated inside `apply_root` (the 80-child branch merge of
  the 4,000-entry rename fixture, measured with a `batch_children` probe:
  `count=80` at level 1 in groups of 32/32/16, i.e. 81 pages) is charged into
  *its own* engine whose `work` never reaches the returned value. The fixture
  reports `pages_read 15` where 96+ pages were decoded.

This is a third counter-correctness defect of the same family as C1's two, it is
outside the two statements C1 authorizes, and fixing it here would bundle a
second variable into C1's commit. It is reported on #178 for an owner ruling
(same treatment as C1, or `measured-and-declined`); no later Phase 1 receipt may
quote an absolute `pages_read` total as complete until it is settled.
