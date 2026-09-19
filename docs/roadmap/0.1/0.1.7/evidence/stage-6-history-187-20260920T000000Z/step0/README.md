# Step 0 — does the gap dissolve? **Partly. 81.9 % of it is the harness; 18.1 % is not.**

Issue #187, sub-issue of #186. Everything here is **diagnostic**, not admission evidence.
HEAD `66bce8378` + an uncommitted harness-side change to
`core/benchmark/fs-bench-pro-storage-content/src/ops/history.rs` (production LOC delta **0**).

## 1. What was run

One binary, one variable. `LAYERFS_HISTORY_ADVISORY` makes the driver declare, for each
constructed whole-file content root, the **previous version's content root of the same path**
as an `OriginalBase` advisory predecessor — the declaration
`layerfs-content/src/file/edit/apply.rs:120-122` makes when it produces a new version as an
edit. `LAYERFS_HISTORY_DEPTH_LIMIT` caps the depth the driver will declare at.

```sh
H=core/benchmark/fs-bench-pro-storage-content
cargo +1.85.1 build --release --manifest-path $H/Cargo.toml --locked
C=/Users/yifanxu/Ephemeral-AI-Lab/deepseek-history-data
B=$H/target/release/fs-bench-storage-content
LAYERFS_CONSTRUCTION_WORKERS=1                       $B --case history-stride10 --out /tmp/s0_off2 --corpus $C
LAYERFS_CONSTRUCTION_WORKERS=1 LAYERFS_HISTORY_ADVISORY=1 LAYERFS_HISTORY_DEPTH_LIMIT=7 $B --case history-stride10 --out /tmp/s0_l7 --corpus $C
LAYERFS_CONSTRUCTION_WORKERS=1 LAYERFS_HISTORY_ADVISORY=1 LAYERFS_HISTORY_DEPTH_LIMIT=4 $B --case history-stride10 --out /tmp/s0_l4 --corpus $C
```

The machine was **not** quiet (eight analysis subagents). **No timing from these runs is usable
and none is reported.** Byte readings are load-independent and are.

## 2. The result

| arm | advisory bases | prefix_selected | full_records | no_candidate | apparent (B) | max chain depth |
| --- | --: | --: | --: | --: | --: | --: |
| `off` — the registered lane | 0 | 18,344 | 32,771 | **26,847** | **128,864,256** | 8 (InodeLeaf only) |
| `l4` — faithful, depth ≤ 4 | 26,629 | 33,220 | 17,895 | 11,972 | 68,669,440 | — |
| `l7` — faithful, depth ≤ 7 | 28,491 | 34,300 | 16,815 | 10,878 | **63,737,856** | 9 (4 objects) |
| `on` — faithful, unlimited | — | — | — | — | **ABORTED** at 38,912,000 | 9 (62 objects) |
| limits 8 / 10 / 12 / 16 | — | — | — | — | **ALL ABORTED** | — |

```
apparent      128,864,256 -> 63,737,856   = -65,126,400 B  (-50.5 %)
delta.no_candidate  26,847 -> 10,878      = -15,969 objects
prefix_selected     18,344 -> 34,300      = +15,956 objects  (+87.0 %)
```

**`delta.no_candidate = 26,847` with `absent_candidates = 0`, `ineligible_candidates = 0`,
`work_exceeded = 0` is the whole of cause 1.** The Store was not *seeing and declining* bases —
it was being **supplied none** on this path. The advisory route is the only cross-save route
(`cas/save.rs:100` → `encoding/delta/select.rs:342`), and **the driver left it empty for file
content**.

**Scope correction (Squad E1).** The advisory list is *not* globally empty. There are three product
producers; `filesystem/sorted/page.rs:380-390` is **live** and is the sole source of the **917
cross-save InodeLeaf bases** in this very Store (917 of 1,738 InodeLeaf rows, 52.76 %), obtained only
from the advisory slice. So the mechanism is proven end-to-end here for a different role, and the
defect is narrower and better located than "the list is empty": **the file-content path never declares,
and three of the four roles that producer covers are then dropped by the ten-role short circuit at
`select.rs:204-222`.**

Independently confirmed by Squad A2 from the Store's SQL alone: of the 18,344 whole-file objects
with a base in the baseline, **18,344 / 82,033,173 B are same-save and 0 / 0 B are cross-save**;
the only cross-save bases that exist at all are 917 InodeLeaf objects. Two instruments, zero residual.

## 3. The bucket tables, side by side

Decoded from the pack directory (`step0/decode_packs.py`; grammar from
`pack/layout.rs` + `pack/assemble.rs`). v0.1.6 measured from the **retained** Store
`benchmark-results/repository-history/stride-10/deepseek-stride10/host-runtime/store.sqlite`
(sha256 `80c2b10a…` = the verification manifest — Squad A4).

Stored figures are **lane bodies**, framing excluded; the totals row adds the framing back so it
reconciles with `space.pack_bodies`. v0.1.6's split is Squad A4's, decoded from the in-record
`tag` byte because v0.1.6 has no `base_object_id` column — its whole-file lane total is
**38,983,278 B**, which this decoder reproduces exactly.

| bucket | v0.1.6 obj / canonical / stored | ratio | `off` obj / canonical / stored | ratio | `l7` obj / canonical / stored | ratio |
| --- | --- | --: | --- | --: | --- | --: |
| whole-file **with** a base | 35,904 / 285,486,222 / 16,310,397 | **17.503×** | 18,344 / 82,033,173 / 17,196,162 | 4.770× | 34,300 / 271,589,818 / 16,082,951 | **16.887×** |
| whole-file **without** | 8,237 / 62,974,073 / 22,672,881 | 2.778× | 25,804 / 266,427,536 / 93,744,892 | 2.842× | 9,848 / 76,870,891 / 28,427,582 | 2.704× |
| other lanes | 7,581 / 32,102,860 / 6,877,354 | 4.668× | 7,884 / 32,460,591 / 8,737,573 | 3.715× | 7,884 / 32,460,591 / 8,737,573 | 3.715× |
| framing | — / — / 196,100 | — | — / — / 215,664 | — | — / — / 212,032 | — |
| **total** | 51,722 / 380,563,155 / 46,056,732 | 8.263× | 52,032 / 380,921,300 / 119,894,291 | 3.183× | 52,032 / 380,921,300 / 53,460,138 | 7.154× |

**The `off` column reproduces the brief's §3.1 table to the byte** — 18,344 / 82,033,173 /
17,196,162 and 25,804 / 266,427,536 / 93,744,892 — and §3.1's "other lanes, aggregate only"
**8,953,237 = 8,737,573 bodies + 215,664 framing**. Residual 0.

**The headline of the whole investigation is in the `with a base` row.** Declaring the previous
version as a base takes the whole-file delta ratio from **4.770× to 16.887×**, against v0.1.6's
**17.503×** — a 3.5 % agreement, from two independent code bases, on the same corpus.
That is not a coincidence and it is not a bound: it is the same mechanism, measured twice.

### 3.1 A decoder bug the fail-closed module caught

The first decoder read the compact whole-file directory as **base-relative** offsets. The compact
directory stores each group start as an **absolute** offset into the pack
(`assemble.rs` seeds the running offset at `HEADER_LEN + 4 * groups` and emits it as it stands).
Base-relative offsets still give the right length for **every group but the last** — the base
cancels in `end − start` — and the last group comes out short by exactly the directory base.
On `history-stride10` that is a **183,584 B** undercount of the whole-file lane, and it is silent.

It is now fixed, and the reason it was found is the reason the reading was moved into
`shared/space.py` as `pack_directory()` / `whole_file_records()`: those functions **refuse**
(`Incomplete`) a directory that claims more groups than the blob can hold, rather than clamping.
The one-off script that did not refuse produced a plausible, wrong table. **The §3.1 table in the
brief was right and the first decoder was wrong**; after the fix the `off` column reproduces it to
the byte. 30 unit tests, 136 in the module's suite, all pass.

## 4. Root-cause register — the residuals sum to the gap

Basis: apparent bytes, both sides (the allocated axis is unusable — §6).

```
this lane, apparent                    128,864,256
v0.1.6, apparent                        49,315,840
GAP                                     79,548,416   = 2.6130x
```

| # | cause | bytes | % of gap | measurement |
| --: | --- | --: | --: | --- |
| 1 | **Harness declared no cross-commit base** | **65,126,400** | **81.87 %** | `off` 128,864,256 → `l7` 63,737,856 |
| 2 | Remaining pack-blob gap (coverage, incl. framing) | 7,403,406 | 9.31 % | pack blob `l7` 53,460,138 vs v0.1.6 46,056,732 |
| 3 | **Non-pack SQLite overhead** | 7,018,610 | 8.82 % | apparent − pack blob: `l7` 10,277,718 vs v0.1.6 3,259,108 |
| | **total** | **79,548,416** | **100 %** | **residual 0** |

(An earlier draft split a 15,932 B framing delta out of cause 3; framing is already inside the pack
blob, so the honest three-cause split is the one above and it closes to the byte.)

Cause 3 in per-object terms: `l7` **197.5 B/object** (10,277,718 / 52,032) vs v0.1.6
**63.0 B/object** (3,259,108 / 51,722) — **+134.5 B/object**, of which v0.1.6 pays nothing
because it stores the base identity *inside* the record (lane-4 grammar: `tag u8 [+ 32-byte base oid]
+ zstd frame`) while v0.1.7 carries `base_object_id`, `pack_id`, `group_number`,
`record_number` as columns plus an index. **This cause is not reachable by any harness change.**

Cause 2 is the depth-7 handicap plus first-appearance versions. 13,896,404 canonical bytes sit in
v0.1.6's with-base bucket and in `l7`'s without-base bucket (Squad A4, per-object join over the
44,141 common whole-file oids). Moving them at `l7`'s own measured ratios would save
13,896,404 × (1/2.704 − 1/16.887) = **4,315,905 B**, leaving ~3.1 MB in the other lanes
(`l7` 8,737,573 vs v0.1.6 6,877,354 = +1,860,219) and in the 310-object /
358,145-canonical-bytes difference.

## 5. A product defect, found by Step 0 — **reported, not patched**

The faithful arm is **rejected by the product**:
`INCOMPLETE | product error: Integrity("dependency chain depth")` (`delta/read.rs:159-168`).

Measured, not inferred:

* `off` arm: max chain depth **8**, reached only by InodeLeaf (22 objects). Readable. **PASS.**
* `on` arm: max chain depth **9** — 301 objects at depth 8, **62 at depth 9**, all `role=1`
  (WholeFile). One of them is read back, and the read refuses. **ABORT.**
* Depth limits **7 completes; 8, 10, 12 and 16 all abort.** (See the gate caveat below: "completes"
  means the lane saved all 17 states, **not** that the Store is readable.)

**The bounds are consistent — this is NOT a bound off-by-one.** (Corrected by Squad E4, which
measured it against the product's own suite: `cargo +1.85.1 test -p layerfs-storage --test delta_chains`
→ **9 passed**, including `a_reduced_depth_stops_at_its_own_boundary` where a cap-2 policy stores *and
reads back* a 2-edge chain.) The writer admits a candidate with `depth < depth_cap`
(`select.rs:373`) so it produces edges ≤ cap; the reader (`read.rs:162-179`) evaluates
`chain.len() > role_depth` only while the current node still has a base, so it refuses exactly
edges > cap and accepts edges == cap. Both say 8. **The writer is not exceeding its policy — it is
mis-measuring the depth.**

**Mechanism — [H], now PROVED by the writer's own output (Squad E4).**
`DepthCache::cost_of` (`select.rs:94-144`) walks from the queried identity down to a chain root
**or to an already-cached entry**. In the cached case the terminating entry is *not pushed onto
`path`* (the exit at `:102-104`), yet the depth is computed as `cost.depth + position` at
`:132-135` with `position` starting at 0 for `path[n-1]` — the object whose base is the cached
entry. Its true depth is `cost.depth + 1`. Every level of that walk is therefore recorded **one edge
short**, so `eligible` admits a candidate whose *true* depth equals the cap and the child is written
at `cap + 1`.

**The proof is in the artifact:** `/tmp/s0_l7` — whose persisted policy says
`whole_file_delta_max_depth = 8` — holds **exactly 4 role-1 objects at 9 edges whose bases sit at
true depth 8**. A base at depth == cap is ineligible, so those bases can only have been admitted on a
depth value ≤ 7 while the truth was 8. `/tmp/base187` holds 0 such objects.

**Minimal fix (described, NOT applied):** carry a `u8` offset out of the loop — 1 for the
`:102-104` cache-hit break, 0 for the `:118-124` chain-root break — and add it at `:133-135`.
**~4 changed lines, one function, one file, no format change.** Widening the reader would *compensate*,
not repair: it raises the effective policy to cap+1 and does not bound a compounding shortfall.

**What the fix buys is LEGALITY, NOT BYTES.** The 4 children revert to FULL ≈ **2,248 B**
(+0.0035 %). It cannot unlock more under a cap of 8, because every base at depth ≥ 8 is refused
regardless — so an "unlimited declaration" arm **cannot beat the depth-7 arm while the cap is 8**.
An earlier draft of this report attributed 4,315,905 B to this fix; **that was wrong** and the figure
has been removed from the proposal, which now rests on base *selection* (B2's R4) instead.

**Gate caveat (Squad E4).** The `l7` arm is **not gate-clean**. Its trace records only
`g1.o1-chain-complete` PASS, `g4.swaps` PASS and `g1.o3-pinned-counters` INCOMPLETE; there are
**zero `g1.o2` and `g1.o4` records in either trace**, and `ops/history.rs:426-428` refuses
`--phase verify` for `history.*`. **No read-back ever ran, so the 4 unreadable objects were never
touched and "l7 completes" carries no readability evidence.** The *size* reading stands — bytes are
bytes — but the *PASS* is a PASS of the gates that ran, not of the Store's readability.
**Prediction, unmeasured:** the §7 gate over `/tmp/s0_l7` would abort at the first of those 4 files
it samples.

Why it was latent: it needs a chain longer than the cap. In the `off` arm the WholeFile lane never
exceeds depth 1, because the per-save candidate cache is the only other route and it is
whole-file-only and dropped at the end of each save (`cas/lifecycle.rs:90`, `cas/store.rs:392`;
Squad A2). **No producer declared cross-commit whole-file bases, so the two bounds were never
pushed apart.** The reference tree has no such asymmetry: it declines to write at
`prior.depth + 1 > CHAIN_EDGES = 8` and reads to `EDGES = 50` (Squad A3).

Per the guardrails this needs an **owner ruling**: it is a `core/crates/` change.

## 6. The allocated-bytes axis is not reproducible — **the §9 blocker, resolved**

`st_blocks × 512` for the **same, closed, unmodified** Store file:

| file | first reading | later readings |
| --- | --: | --: |
| `/tmp/base187` (apparent 128,864,256) | 135,118,848 (harness) | 135,192,576, 130,799,104, **130,799,104**, then **134,537,216** |
| `/tmp/s0_l4` (apparent 68,669,440) | 84,807,680 (harness) | **68,988,160** |
| `/tmp/s0_off2` (apparent 128,864,256, byte-identical content) | 134,651,904 (harness) | 134,651,904 |

Range observed on one file: **130,799,104 … 135,192,576 B = 4,393,472 B**. Two Stores with
identical apparent size and identical content read **262,768** and **262,992** blocks.

**What is reproducible:** `PRAGMA page_count × page_size = st_size` exactly, on every arm
(31,461×4096 = 128,864,256; 15,561×4096 = 63,737,856; 16,765×4096 = 68,669,440), with
`freelist_count` = 26–28 pages (**106–115 KB**). That is the honest allocated quantity:
**the file is exactly `st_size` bytes, and the only genuine slack is the freelist.**

**Recommendation.** The gate must be decided on **apparent bytes** (`st_size`), optionally with
the freelist stated, and **never** on `st_blocks × 512`. The recorded v0.1.6 pair
(49,344,512 / 49,315,940) differs by 28,572 B = 0.058 %; this lane's readings differ by up to
4.4 MB = 3.4 %. The two generations' *allocated* readings are therefore not the same quantity,
which is a separate reason the registered gate number cannot be used as written.

## 7. The exit criterion, stated plainly

* **allocated < 49,344,512 B — NOT MET.** `l7` reads 64,233,472 B. (And the axis is unusable.)
* **"the two generations are not comparable" — NOT SUPPORTED.** Squad A4 proved the opposite for
  the byte claim: same 17 states (both 561,010,345 logical bytes / 101,477 path-states), the same
  44,141 lane-4 whole-file oids, the same 348,460,295 B lane-4 canonical. The Stores are
  like-for-like artifacts.

What *is* true is narrower and needs a ruling: **the registered `history-stride10` number measures
a driver that declares no cross-commit base.** It is not a product measurement of the retained-history
workload, and #186's 2.65× should not be carried as one.

## 8. Everything ruled out, with the number that ruled it out

| ruled out | by |
| --- | --- |
| A codec or zstd-parameter regression | A1: parameters byte-identical; on the 44,141 common whole-file objects the FULL rate is **0.977288×** — v0.1.7 is **2.27 % better** |
| The 128 KiB cutoff / representation churn (#185's axis) | A2/A4: identical lane-4 object set, 44,148 vs 44,141 objects, +414 B |
| A workload difference | A4: both selections sum to 561,010,345 logical bytes; union/canonical = 1.024 |
| The whole-file depth cap being the blocker | A1: v0.1.7 already builds chains to its depth-8 cap (22 objects) |
| `MAXIMUM_DELTA_MAX_DEPTH = 50` as the operative bound | A1 (N5): the operative bound is 8 on **both** sides |
| Chunk chain closure (1 MiB → 512 KiB) | A1 (N2): provably non-binding, max chain 164,885 B < 256 KiB |
| Metadata depth 16→8 and closure 128 KiB→65,536 | A1: ceiling = the whole 2,240,958 B pooled lane, and measured **221,539 B smaller** |
| `GROUP_TARGET` 48 KiB | A1 (N8): +19,564 B of framing = 0.025 % |
| Tree-role advisories mattering | A2: **inert** — `select.rs:204-222` returns `encode_full` before reading the advisory; 0 bases across 4,770 DirectoryLeaf / 92 FileState / 17 FilesystemRoot / … |
| `PredecessorProvenance::ReusedRange` being unconstructed | A2: constructed in one test; three product sites construct `AdvisoryPredecessors`. One unused *variant*, not the mechanism |
| The candidate cache being able to reach a previous state | A2: per-save, dropped at `cas/store.rs:392`; v0.1.6's persists across sessions (`schema.rs:77`, asserted by `small_candidate_tests.rs:256-317`) |
| The recorded v0.1.6 totals being wrong | A4: reconciled to the byte, residual 0, against the retained artifact and its sha256 |
| A stride1 effect | Not run. **No stride1 number was taken.** |

## 9. Checks NOT run, and why

* **`runner.py perf` / the registered lane.** The verify phase, golden table,
  `tests/history_declarations.rs`, lane wiring and per-lane complete-command ceilings are not built.
* **`history-stride3` confirmation.** The guardrail says iterate on stride10 and confirm on
  stride3; stride3 was not run, so **no stride3 confirmation exists** and no stride3 claim is made.
* **Any product-source change.** Guardrail: owner ruling required.
* **`cargo fmt --check`.** Not clean on this tree; the harness file was not reformatted.
* **The 217-row lane.** Untouched — registry, cardinality, golden table, `--lane full` and
  `full` verification default unchanged. `--lane full` remains 220 rows, `--smoke` 20.
* **Timing.** The machine was loaded by eight analysis subagents; no timing is reported.

## 10. Production LOC

`core/benchmark/` is its own Cargo workspace and is **not** product source.
**Production LOC: 84936 -> 84936 (delta 0)**; harness lines stated separately.
No lockfile move, no new dependency.
