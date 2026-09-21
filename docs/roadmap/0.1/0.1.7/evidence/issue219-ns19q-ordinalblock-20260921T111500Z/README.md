# #219 round 16 — the ordinal reservation is a block: `operation_work_ns` 1283.16 → 1218.88 ms, and the commit term is priced

Pre-registration: `pre-registration.md` with `raw/ordinal-census.py` beside it. Rows:
`benchmark-results/issue219/ns19-Q2-repin-20260921T115200Z`, **PASS, 13/13 gates, 14/14 pinned
counters**, `source_commit` `3bb2d209d`, clean — and the **consequence run**
`ns19-Q1-ordinalblock-20260921T114500Z`, **FAIL, one gate**, `g1.o3-pinned-counters` reading
`pipeline.commits 285 -> 95`. Both are on disk and the red one is reported as red. Landed as
`622eae391` (the change) and `3bb2d209d` (the re-pin). Control: **P2**, not re-run.

## 1. The movement

| instrument | P2 | **Q2** (covering) | Q1 (red, pre-re-pin) | movement |
| --- | ---: | ---: | ---: | ---: |
| **`operation_work_ns`** | 1283.16 ms | **1218.88 ms** | 1215.30 ms | **−64.28 ms, −5.01 %** |
| **CPU user+system** | 1297.49 ms | **1228.33 ms** | 1237.77 ms | **−69.16 ms** |
| `operation_ns` (inclusive) | 1402.71 ms | 1396.26 ms | — | −6.45 |
| complete command | 2278.26 ms | 2221.39 ms | — | −56.87 |
| **`commits`** | 285 | **95** | 95 | **−190** |
| **`diag_commit_total_ns`** | 497.54 ms | **458.26 ms** | 449.93 ms | **−39.28** |
| `diag_begin_ns` | 3.81 ms | **1.79 ms** | 1.75 ms | −2.02 |
| `diag_wave_ns` | 62.67 ms | 65.38 ms | 64.48 ms | +2.71 (window) |
| controls: `statements`, `pack_appends`, `stored_records`, `content_bytes`, `inserted` | 7,666 / 6,603 / 23,910 / 301,171,810 / 25,245 | **identical** | identical | 0 |
| `pack_bytes_written` | 301,864,382 | 301,865,004 | same | **+622** |

**Cumulative against this worktree's clean tree** (A0 ≤ 3487.3 ms): **−65.0 %**. Against the handoff's
1473.8 ms: **−254.9 ms**. The serial floor — commit + pack writes + row inserts + wave + collision +
begin — moves **882.35 → 842.90 ms**.

The two runs agree to 3.6 ms on the formula and to the byte on every control, which is the same
consistency round 15 saw from the count-driven side.

## 2. What was found, and what it is worth

**74 % of this row's COMMITs were one statement.** `cas/pool_lane.rs:119-130` acknowledged the pooled
metadata lane's ordinal reservation *per leaf*, by design clearing `wave_held` for one call so the
reservation is durable before any value uses it. The control row's own Store says how many leaves:
`store_policy.next_ordinal` = 10,164, so 10,163 values over 207 catalogue groups, ~49 fresh values per
leaf. `commits` was 285 = 73 waves (4 MiB of canonical bytes each) + ~207 reservations + the save's own
two.

The treatment keeps the durability rule and changes its granularity — a save's first 4 reservations
exact, then a block of `fresh values x 16` — and the row returns **17 reservations and 95 COMMITs**,
which is the number the census predicted before the run.

**And it prices a COMMIT at 0.21 ms.** Removing 190 of them bought 39.28 ms, so the marginal COMMIT on
this store costs **0.21 ms (Q2) / 0.25 ms (Q1)** — consistent with round 8's ≤ 0.32 ms upper bound,
which bundled a wave's begin, locator query, presence seed and collision check with its commit.

**Which closes the largest term as a lever.** The 458.26 ms that remains is 302 MB across 95 commits,
3.18 MB each, at **659 MB/s** — and at 0.21 ms per transaction the whole remaining transaction
overhead is about **20 ms**. The commit term is byte-bound: no further transaction shaping can take
more than that, and the profile (`journal_mode = MEMORY`, `synchronous = OFF`) was not touched to get
here — `src/sqlite/` is untouched by this round, and the pragmas are cited only as the reason a COMMIT
costs page writes.

## 3. Prediction scorecard, and the three corrections made before the run

| # | registered | measured | |
| --- | --- | --- | --- |
| 1 | `commits` 78–95 | **95** | hit, at the edge |
| 2 | `diag_commit_total_ns` 350–430 ms | **458.26** | **missed high**: the movement is −39, not −68 to −148. Clause 2 (≥ 480 ms) did **not** fire |
| 3 | `diag_begin_ns` 1.0–1.6 ms | **1.79** | missed high by 0.2, right order |
| 4 | `operation_work_ns` 1150–1215 ms | **1218.88** | missed high by 4 ms (Q1's 1215.30 landed inside) |
| 5 | controls identical | `pack_bytes_written` **+622** | clause 5 fired as written; explained below |
| 6 | pins + digest + PASS | **13/13, 14/14**, digest unchanged | clause 4 fired once, on `commits`, and was handled by the declared re-pin |

**The +622 bytes is the ordinals themselves, and it is the one control that could not be identical.**
The pooled leaf *records* live in the ordinary lane and their groups are compressed when that helps, so
a leaf body whose ordinal bytes changed (`2` → `1025`) compresses to a slightly different size. 622
bytes over 207 leaves is 3 bytes per leaf. `statements`, `pack_appends`, `packs_created`,
`stored_records`, `content_bytes` and `inserted` are identical to the byte, which is what the control
was for.

**Three corrections were made to the registered rule before any row was run**, each from a measurement
in the product's own suites, and they are recorded in the pre-registration rather than only here:

1. a **fixed 1,024-ordinal block** was measured on `metadata_window.rs`'s 1,312-leaf fixture and handed
   out **134,645 ordinals for 131,200 values** — 924 wasted per block, because the block is not a
   multiple of what a leaf needs. The rule became `fresh values x 16`;
2. the **retained window must count values, not reservations**: counting the block pulled that
   fixture's crossing group three leaves early, which is the index releasing candidates it still holds
   and storing duplicate physical values. `reserve_ordinals` no longer carries the window arithmetic;
   `ownership::note_window` is charged once per leaf with what that leaf handed out;
3. the block's **unused tail is released when the save publishes**, by a compare-and-swap on the
   reservation this save made (`ownership::release_ordinals`). Without it a partly-consumed final block
   left a 600-ordinal hole in that fixture. A second writer that reserved in between makes the swap
   fail and the tail is not reclaimed: a wasted tail, never an ordinal handed out twice.

One further measurement is recorded because it is why the first four reservations stay exact: with
contiguous ordinals a **one-value leaf's COPY/INSERT program is 50 bytes against a 57-byte FULL**, and
with the next block's first ordinal it is **57 against 57** — a tie, and a tie stores FULL.

## 4. Checks as run, and what was not run

- `cargo +1.85.1 test --locked --manifest-path core/Cargo.toml --no-fail-fast` — **630 passed / 0 failed**.
- `-p layerfs-storage` alone: **224 passed / 0 failed**, including `metadata_pool`, `metadata_window`,
  `metadata_pool_index`, `metadata_chain`, `metadata_fingerprint_collision` and
  `catalogue_statement_reuse`.
- `clippy --locked --manifest-path core/Cargo.toml --all-targets` clean;
  `fmt --manifest-path core/Cargo.toml --all --check` clean;
  `core/tools/check_product_boundary.py` **PASS** (194 production files).
- Harness release build from the repository root before each run; `--verify full --no-build`; one
  sample per case per arm; fresh `--out`.
- **Not run:** the harness's own suite beyond the golden pin, the reference `crates/` workspace, any
  other harness case or lane, and any further sample of this arm.
- **No pragma changed.** `src/sqlite/connection.rs` is untouched.

## 5. Production LOC

**31616 -> 31684 (delta +68)** for `622eae391`, and **31684 -> 31684 (delta 0)** for the re-pin.
Method `python3 tools/production_loc.py --root <tree>`, first parent against each committed tree.
