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

## 9. Correction (appended 2026-09-18, before any Phase 1 box was ticked)

**A measurement-identity defect invalidated this receipt's `order` rows in §3.2,
and this section supersedes that table's `order.default`/`order.forced64` line.**
Nothing above is edited: the old values stay, and the correction is this dated
section, as [`../../CONTRACT.md`](../../CONTRACT.md) §3 requires.

### 9.1 What was wrong

`collect.py`'s `CLIENT` constant pointed at
`/tmp/layerfs-phase1-target/release/phase0client`, and **no step of the driver
rebuilt that path**: the build step `B2` writes
`client/target/release/phase0client`. The probe client links `layerfs-content`/
`layerfs-storage` statically, so the `/tmp` copy was frozen at the tree it was
first built from — the pre-C1 tree. Both of C1's arms ran that same stale binary,
so §3.2's "all identical" reading was self-consistent but **blind to C1's own
product change**; it was not evidence that C1 leaves the `order` rows alone.

### 9.2 The corrected pair (both clients built from their own tree)

Each arm's client was built in a **fresh `git archive` of its own commit**, into
its own target directory; `artifacts.txt` in each arm records the commit and the
binary's sha256. Collections:
[`order-rows-correction-20260918/`](order-rows-correction-20260918/)
(`before/` = `4a86107fc`, client `a7325329a6961b47…`; `after/` = `7447f87d9`,
client `c41d9a3658d25229…`), five commands per arm, exit codes in each
`commands.tsv`.

| Row | counter | stale pair (§3.2) | **corrected before** | **corrected after** | Δ |
| --- | --- | ---: | ---: | ---: | ---: |
| `order.default` (D25) | `directories.pages_read` | 2 → 2 | **2** | **17** | +15 |
| `order.default` (D25) | `inodes.pages_read` | 1 → 1 | **1** | **81** | +80 |
| `order.default` (D25) | `objects.read_waves` | 11 → 11 | **11** | **7** | −4 |
| `order.forced64` (D26) | `directories.pages_read` | 2 → 2 | **2** | **17** | +15 |
| `order.forced64` (D26) | `inodes.pages_read` | 1 → 1 | **1** | **81** | +80 |
| `order.forced64` (D26) | `objects.read_waves` | 11 → 11 | **11** | **7** | −4 |

Both rows move in the direction C1's own two statements predict, and for the same
reason as D2/D5: the batched decode was invisible (`pages_read` rises) and the
grouped demand was charged twice (waves fall). Every other field of both rows is
identical between the corrected arms — `spilled`, `rows_read` 59,007,
`rows_written` 25,760, `runs_created` 124, `merges` 61, `peak_run_bytes`,
`peak_live_runs`, `peak_pending`, `peak_backing`, `emitted` 14,
`dir_pages_created` 11, `dir_scratch`, `ino_pages_created` 2, `ino_pages_reused`
40, `ino_scratch`, `objects_read` 98, `bytes_read` 400,354, `rows_touched`,
`final_values`, `final_removals`. The labelled repeat (`X1`) is bit-identical to
its arm's gate sample on every work counter.

### 9.3 The `c2` rows are confirmed, not assumed

§3.2 also claimed D28/D29 unchanged. That claim was re-measured with correctly
built clients: `c2.ceiling` (8,191 rows) and `c2.small` (1,023 rows) are
**identical in every work counter** in both arms (`inserted`, `reused`,
`packs_created` 33/5, `pack_appends`, `commits` 31/4, `full_records`,
`prefix_records`, `pool_*`); only `elapsed_ns` moved (diagnostic). C1's two
statements live in C1's boundary and the sorted page batch, and the C2 save route
reaches neither — now measured rather than argued.

### 9.4 Consequence for the Phase 1 anchors

* The plan's P1-1 before-anchor "D25 `order.default` 98 objects / **11 waves**
  (post-C1 arithmetic)" is **superseded**: on the C1 tree the same row reads
  **7 waves**, 17 directory pages and 81 inode pages. P1-1's receipt must use the
  corrected C1-tree row as its before value.
* Phase 0's published D25/D26 values (2 pages / 1 page / 11 waves) remain valid
  **for the Phase 0 tree**: Phase 0 built its own client from its own tree. They
  are not comparable with any post-C1 row, which is exactly the straddle
  [`../../CONTRACT.md`](../../CONTRACT.md) §4.1 forbids.
* §3.1's rows (D2/D5/D7) and §3.3's root identities are unaffected: those rows
  come from the core examples, which `B1` rebuilds in `core/target` every arm.

### 9.5 The driver fix

`collect.py`'s `CLIENT` now names the artifact `B2` builds
(`client/target/release/phase0client`), and every `build` arm writes an
`artifacts.txt` with the sha256 of each binary it will run, so a stale artifact
is visible in the round directory rather than invisible. Recorded as driver
correction 2 in [`../../ROUND-README.md`](../../ROUND-README.md). The `v1` round
is the first round collected with the corrected driver; its `after/artifacts.txt`
records client `b0866eb1a9060618…` and the four example hashes.
