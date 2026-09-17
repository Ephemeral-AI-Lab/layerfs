# Report C — complexity of the attribute engine and the reference-ordering subsystem

Read-only static source analysis. HEAD `5e45897dd9f56e7f563aada031a68070a438fb93`, tree
clean. No build, no test run, no timing; the only measured facts quoted are the existing
receipts under `docs/roadmap/0.1/0.1.7/evidence/` (cited per row). Runtime numbers below are
**theirs**, not mine; every mechanism claim carries a `path:line` citation into
`core/crates/layerfs-content/src/` (paths shortened: `A/` = `filesystem/attributes/`,
`R/` = `filesystem/references/`, `F/` = `filesystem/`).

## 0. Variables and bound classes

| Var | Meaning | Defined at |
| --- | --- | --- |
| n | attribute entries (keys) in one tree | — |
| v | bytes of one attribute value | `A/value.rs:26` |
| k | keys in one `lookup_many` wave | `A/read.rs:43` |
| L | page-tree levels (leaf = 0) | `filesystem/limits.rs:24` → `file/mapping/types.rs:17` (= 31) |
| r | ordering rows = distinct touched serials | `R/reduce.rs:57` |
| c | row touches (inserts + updates; `rows_touched`) | `R/reduce.rs:33-35` |
| P | `maximum_pending_records` (default 4,096) | `R/reduce.rs:25`, `F/input.rs:74` |
| B | spilled batches (one per spill) | `R/runs.rs:211` |
| T | live tiers (≤ 32, `MAXIMUM_LEVELS`) | `R/runs.rs:41,223-227` |
| m | merge buffer bytes (default 16 KiB = 170 rows) | `R/runs.rs:37`, `R/merge.rs:71` |
| E | base-record read batch (default 32) | `R/reduce.rs:27` |

Bound classes: **ENFORCED** = an explicit check refuses the violating input; **STRUCTURAL**
= follows from the algorithm's shape and ownership accounting with no dedicated check;
**UNKNOWN** = not determinable by static reading.

## 1. Algorithm inventory

### 1.1 Attributes

| Algorithm | path:line | Time | Peak owned memory | I/O trips | Class |
| --- | --- | --- | --- | --- | --- |
| Value emit (one extent-only root: chunk → leaf → state) | `A/value.rs:22-59` | O(v) | O(v): payload + 3 canonical bodies (`:32-45`) | 3 object writes (`:33,46,58`) | ENFORCED 1 ≤ v ≤ 32,768 (`:23-31`; 32 KiB = `file/cdc/gear.rs:17` via `limits.rs:58`) |
| Value read (bounded, whole) | `A/value.rs:62-91` | O(v) | O(v): `Vec::with_capacity(length)` (`:77`) | 1 canonical read + extent read_range (`:68,80-83`); extent-only root = 1 leaf + 1 chunk | ENFORCED v ≤ caller `maximum_bytes` (`:71-76`) |
| Key check (domain+key grammar, total order) | `A/keys.rs:29-58,86-93` | O(domain+key) | O(key) | none | ENFORCED domain ≤ 64 B, key ≤ 255 B, no NUL, UTF-8 (`:33-53`); portable domain restricted to `mode`/`mtime` (`:54-56`) |
| Page codec (encode/decode, exact sizing) | `A/codec.rs:119-183,186-284` | O(page) | O(page) | none (bytes in/out) | ENFORCED page ≤ 8,192 B (`:187-192,306-312`); branch level ≥ 1 and count ≥ 2 (`:161`); row widths fixed 37/36 B + domain + key (`:27,29,114-116`) |
| Page build — `push` (streaming, exact arithmetic, no trial encode) | `A/build.rs:91-144` | O(1) amortized; group split O(1) (size is arithmetic on row widths, `:104-133`; no clone, no candidate encode) | ≤ 2 pending groups/level + 1 sealing (third group seals, `:135-142`) | emit only at seal | STRUCTURAL; entries strictly ascending ENFORCED (`:96-102`) |
| Page build — `seal`/`push_summary` | `A/build.rs:218-292,294-349` | O(rows in group) per sealed page (try_fold sums, `:226-233,250-261`) | as above; summaries `(key,count,id)` per child | 1 object write per page + reference list (`:401-418`) | STRUCTURAL |
| Tail rebalance (2/5 fill rule) | `A/build.rs:353-374,376-398` | O(moves × last-group rows): `entries.insert(0, moved)` memmoves the last group per move (`:369`) | O(page) | none (pre-emit) | ENFORCED non-root pages ≥ 3,277 B (`codec.rs:100-102`, checked on read `A/read.rs:181-183`, `A/patch.rs:208-210`); moves bounded by page constants (≤ ~200 rows/page at ≥ 39 B/row) |
| Build finish (root collapse, branch sealing) | `A/build.rs:147-204` | O(pending groups) | ≤ 3 groups × (L+1) levels ≈ ≤ 24 KiB × 32 worst case | 1 write per remaining page | ENFORCED level ≤ 31 (`:189`) |
| `build_attribute_tree` | `A/build.rs:422-432` | O(n) total | as build | O(n / rows-per-page) page writes | STRUCTURAL |
| Patch application (sorted merge of base stream + patch list) | `A/patch.rs:66-128` | O(n + p): one merge sweep (`:86-125`); preserved keys pass value roots through undecoded (`:119-124`); each `Set` emits a value (3 objects, `:101` via `A/value.rs:22`) | builder pending (above) + one buffered leaf (`:158-161`) | every base page read (`PageCursor.advance`, `:190-233`) + full tree re-emit (`:126`) | ENFORCED patch list strictly sorted (`:76-81`, `A/keys.rs:102-107`); empty patch returns base root unchanged (`:73-75`) |
| Key listing (`visit_keys`/`visit_keys_counted`) | `A/patch.rs:131-155` | O(n) | O(L) pending stack (`:159`) + one leaf | 1 read per page (`:199`) | ENFORCED listing ≤ 4,096 keys (`filesystem/read.rs:241-243`, `limits.rs:94`); fill/summary checks per page (`:208-219`) |
| `lookup_many` (batched descent) | `A/read.rs:43-102` | O(L) waves; per level O(k·log rows) (leaf binary search `:76-79`, branch `partition_point` `:86`) + O(groups) BTreeMap (`:84-94`) | wave ids + one decoded page at a time (`:60-72`) | 1 `read_canonical_batch` per level (`:61,68`) | ENFORCED depth ≤ 31 (`:57-59`); duplicates answered in demand order (`:49,75-81`) |
| `read_portable` / `read_opaque` | `A/read.rs:109-142,145-157` | 1 `lookup_many` + O(v) | O(v) | L waves + 2 value reads (`:130-133`) | ENFORCED mode=4 B, mtime=12 B (`A/portable.rs:60-94`) |

### 1.2 Reference ordering

| Algorithm | path:line | Time | Peak owned memory | I/O trips | Class |
| --- | --- | --- | --- | --- | --- |
| Row encode/decode | `R/record.rs:79-106,109-157` | O(1) | O(1) (96-B fixed) | none | ENFORCED fixed 96 B (`:26`), serial ≠ 0 (`:115-117`), reserved-zero and flag checks (`:124-140`) |
| Pending-map accumulation (`entry`) | `R/reduce.rs:224-254` | O(log P) insert; `find` on miss (`:226`); spill when full (`:239-246`) | P rows in `BTreeMap<u64,Row>` (`:57`) + `declared_new` set (`:58`) | 1 `find` per first touch | ENFORCED pending ≤ P (`:239`) |
| Spill (write batch + cascade into first free tier) | `R/runs.rs:211-308` | batch write O(P) (`:237-243`); then one 2-way merge per occupied lower tier (`:253-289`); tier j rewritten every 2^j spills → amortized Θ(P·log B) per spill, Θ(r·log(r/P)) total | pending ×2 charged (`:130-139`), merge inputs + output reserved (`:265-276`); buffers 2×m (`R/merge.rs:213-214`) | 1 run create + P row appends + up to T merges, each re-reading both inputs (`R/merge.rs:213-217`) | ENFORCED tiers ≤ 32 (`:223-227`); owned ≤ 64 MiB default (`:39,150-164`) |
| 2-way merge (`merge_runs`, newest wins on shared serial) | `R/merge.rs:200-260` | O(older+newer rows) | 2 reader buffers × m + output rows | full read of both inputs (`:213-217`) + full write (`:243-247`) | STRUCTURAL (documented ≤ log2(B) merges per batch, `:6-9`) |
| Lookup scan (`find`, resumable one-reader-per-tier) | `R/runs.rs:324-373` | per tier: O(1) range pre-check (`:329`); ascending demands amortize to one pass/tier (`:341-360`, pinned `tests/filesystem_ordering_scan.rs:101-111`); a request behind the cursor restarts from the front, O(position) (`:341-344`) | ≤ T buffers × m (`:46-52,320-323`) | buffered `read_at` per m/96 rows (`R/merge.rs:117-136`); rows charged to `rows_read` (`:348`) | STRUCTURAL resume; ENFORCED buffer = whole rows, never reallocated (`R/merge.rs:70-79`) |
| Scan invalidation on spill | `R/runs.rs:231,299` → `:120-122` | every spill drops **every** tier's scan and buffer | — | next lookup per tier restarts at offset 0 | STRUCTURAL (doc `:117-119` says "whenever a tier's run is replaced"; code clears all tiers — see §5) |
| Tier consolidation (newest-first, `touched_serials` and `finish`) | `R/runs.rs:381-446` | O(live rows) reads+writes: copy of newest tier (`:418-424`) + one merge per older tier (`:425-431`); no-op at ≤ 1 live run (`:382-385`) | output coexists with inputs, reserved (`:407-411`) | re-read + rewrite of every live row | STRUCTURAL |
| `copy_run` (alias-avoidance copy of the newest tier) | `R/runs.rs:566-582` | O(rows) | buffer m | full read (`:575`) + full write (`:576-579`) | STRUCTURAL ("so a merge never aliases its own input", `:565`) |
| `visit_newest_first` (k-way merge over live tiers) | `R/runs.rs:460-500` | O(live rows · log T) | T readers × m (`:464-469`) | one pass per live run | STRUCTURAL; these reads are **not** charged to `rows_read` (`R/merge.rs:181-183` charges nothing) |
| `touched_serials` (consolidate + visit + pending-key merge) | `R/reduce.rs:176-207` | O(r) | returned `Vec<u64>`: 1 u64 per touched serial (`:178`), refused past `ordering_bytes/8` (`F/input.rs:90-95`, `F/update.rs:465-473`) | consolidation + full visit | ENFORCED touched-serial ceiling |
| Final row stream (`FinalRows`: lookahead, waves, bases, serials) | `R/reduce.rs:315-580` | O(r); `fill_wave` merges memory iter + run reader, superseded run rows skipped (`:396-430,419-424`); one `lookup_many` wave per batch for Effect rows only (`:438-457`); base merged into the row (`:459-487`) | lookahead ≤ E rows (`:397`) + run buffer E×96×4 B (`:366`) | 1 base wave per E effect rows (`:446-451`) | ENFORCED effect rows must find a base record (`:475`); root-serial zero-count special case (`:522-532`) |
| Release cursors (`release_zero_count`) | `R/release.rs:51-188` | per directory page: 1 `list_after` page (`:129-136`) + 1 child base wave (`:153`); frontier pops + cursor stack (`:62-64,81-128`); stops at aliased inodes (count > 0, `:173`) | cursor stack O(depth) + `prefetched` BTreeMap (`:71`) + page | pages read + base records; prefetch wave avoids per-serial re-reads (`:71-80,176-177,191-204`) | STRUCTURAL paging (`page_entries`=64, `page_bytes`=8,192 at `F/update.rs:329-330`) |
| Backing (file runs, capacity account) | `R/backing.rs:243-342,97-131` | O(1) append (reserve-then-write, `:314-326`); `read_at` = seek+read_exact (`:328-333`) | `held/peak/capacity` account (`:80-131`) | per-append write; per-read_at one syscall-level trip | ENFORCED backing ≤ 256 MiB default (`:26`) and ≥ ordering limit (`R/runs.rs:95-109`); run-file Drop removes the file (`:344-359`) |

## 2. Measured baseline (receipts, not re-measured)

Grid: 4,000-file base, forced `maximum_pending_records = 64`, rename pairs; workload =
2 changes per pair (one unbind + one rebind of the same serial), one directory,
`inodes=[]`, `new_inodes=[]` (`verify-R2-F8-scaling.md:163-170` citing the client).
All four sources agree exactly on every counter (`verify-R2-F8-scaling.md:101-110`;
round-2 grid `stages-1-5-review-20260917T230700Z/diagnostics/diag-run.log:4-7`).

| pairs | spilled | rows_read | rows_written | runs | peak_owned B | read−write |
| ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 250 | 448 | 2,198 | 1,588 | 14 | 66,432 | 610 |
| 500 | 960 | 6,684 | 4,328 | 30 | 139,008 | 2,356 |
| 1,000 | 1,984 | 19,960 | 10,896 | 62 | 284,160 | 9,064 |
| 2,000 | 3,968 | 59,007 | 25,760 | 124 | 568,320 | 33,247 |

(`ordering-scaling.log:4-9`; read−write is my arithmetic on those columns.)

Receipt-stated: rows_read grows ~×3.0 per doubling, 2,198 → 59,007 over three doublings
= 26.85× ⇒ ≈ n^1.58 (`verify-R2-F8-scaling.md:121-123`, `stage-5-report.md:709-712`);
read-over-write at 2,000 pairs = 59,007/25,760 = 2.29×; peak owned bytes grow exactly
×2.0 per doubling (linear in rows: 568,320 B / 96 B = 5,920 rows).

Decomposition (arithmetic on the receipt plus the charge sites):
- `rows_written` growth ×2.72 / ×2.52 / ×2.36 per doubling — the tiered-merge rewrite
  term. Every merge charges one write per output row (`R/merge.rs:247`), the spill batch
  one per row (`R/runs.rs:241`), `copy_run` one per row (`R/runs.rs:578`). Class
  Θ(r·log2(r/P)): each batch climbs ≤ log2(B) tiers (`R/merge.rs:6-9`), tier j is fully
  rewritten every 2^j spills.
- `runs` created = spills + merges + copies exactly (7+6+1, 15+14+1, 31+30+1, 62+61+1):
  one consolidation (1 `copy_run`) per operation, and merges ≈ spills − 1 — the cascade
  plus the rebind phase's first spill re-merging the whole unbind ladder (both phases
  sweep the *same* serials, so 2 touches/serial ÷ 64-row map ⇒ 62 batches for 2,000
  pairs).
- `rows_read` − `rows_written` = 610 / 2,356 / 9,064 / 33,247, growing ×3.86 / ×3.84 /
  ×3.67 per doubling ≈ n^1.9 — **above** the merge class. Merge and copy reads equal
  their writes per row except dedup shrinkage (`R/merge.rs:217` charges both input
  counts; output ≤ inputs on shared serials, `:221-242`), so this residual is dominated
  by `find` scan reads (`R/runs.rs:348`) — the lookup path, not the merge, supplies most
  of the excess over the O(n log n) floor (the review expected ~×2.1/doubling for pure
  O(n log n); see `verify-R2-F8-scaling.md:139-146`).

## 3. Why it scales at n^1.58 (mechanisms, source-reading)

1. **Two-phase re-climb.** The rename workload touches every serial twice (unbind sweep,
   then rebind sweep, both ascending — the sorted change list interleaves by name:
   all `f…` removals before all `r…` additions). Each phase refills the 64-row map from
   empty, so B ≈ 2·r/P batches climb the tier ladder twice; the rebind phase's first
   spill lands above the whole unbind ladder and re-merges it (`R/runs.rs:218-227`
   first-free-tier, `:253-289` one-by-one merges).
2. **Every spill resets every tier scan.** `spill` calls `reset_scans()` twice
   (`R/runs.rs:231,299`), which clears the whole scan vector (`:120-122`) — including
   tiers whose runs were untouched (a spill into level k replaces only `levels[k]` and
   clears below, `:295-298`). A lookup whose cursor was passed must then restart that
   tier from offset 0 (`:341-344`), re-reading all rows before the target. Restarts
   happen ~per spill (≈ r/P times) and each costs O(rows before the target in the
   scanned run) → the ≈ n^1.9 residual in §2.
3. **Merge re-reading.** Each 2-way merge re-reads both inputs fully and rewrites the
   union (`R/merge.rs:213-217,243-247`) — the Θ(r·log2(r/P)) floor, stated as the
   honest reason in `stage-5-report.md:710-712`.
4. **Consolidation rewrite.** `touched_serials` consolidates every live row (copy +
   merges, `R/runs.rs:381-446`) and `finish` consolidates again (no-op once single,
   `:382`, `R/reduce.rs:268`).
5. **State re-sampling.** `zero_count_serials` first streams every row
   (`R/reduce.rs:181-199` via `visit_newest_first`), then calls `reducer.state(serial)`
   per touched serial (`F/update.rs:482` → `R/reduce.rs:160-169` → `find`), re-passing
   the consolidated run once more.

## 4. Opportunity register

Baseline for every lever: the §2 grid. "Parity-safe" = spilled and unspilled runs are
**provably result-identical**: `the_pending_threshold_changes_only_where_the_rows_live`
builds the same operation with pending ceiling 1 (forcing spills) and a large map (no
spills) and asserts the same canonical root and value
(`tests/filesystem_ordering.rs:139-269`, roots at `:259-266`). Any change that preserves
`find`/`merge`/stream answers — pure internal restructuring — keeps that proof's
invariant.

- **(L1) Targeted scan reset — keep scans of untouched tiers.**
  Cost today: `reset_scans()` on both spill paths (`R/runs.rs:231,299`) clears all T
  scans; each cleared tier whose run survived costs a front-restart
  (`R/runs.rs:341-344`) — the dominant n^1.9 term in §2. Mechanism: drop only the scans
  at indices ≤ the spilled level (those runs were replaced/cleared, `:295-298`); tiers
  above keep run, buffer and position. Math: an ascending sweep then costs one pass per
  tier total instead of one pass per spill-epoch: restart reads drop from
  ~O(r²/P)-blended to O(r). Tradeoff: none in allocations (buffers kept longer, not
  added — the zero-allocation wave property `tests/filesystem_ordering_scan.rs:121-125`
  still holds); code must track tier identity vs scan index. Parity-safe (find answers
  unchanged; positions are per-run state, `R/runs.rs:543-545`).
- **(L2) Binary search inside runs for restarts.**
  Rows are sorted by serial (`R/record.rs:3-5`; merge output order `R/merge.rs:221-248`)
  and fixed 96 B (`R/record.rs:26`); the backing reads at arbitrary offsets
  (`R/backing.rs:41,328-333`); `RunScan::start` already accepts any row-aligned offset
  (`R/merge.rs:86-99`). A restart today is O(position) sequential re-read
  (`R/runs.rs:343`); a row-aligned binary search is O(log2 rows) single-row `read_at`
  calls (11 for 2,048 rows). Tradeoff — the sequential-sweep property: ascending demands
  currently amortize to one row per lookup (documented `R/runs.rs:530-538`, pinned
  `tests/filesystem_ordering_scan.rs:101-111`); switching *all* lookups to binary search
  would pay log2(rows) random reads per lookup (2,000 lookups × 11 ≈ 22k reads > the
  one-pass 2k), and random `read_at` defeats the 170-row buffered sequential read
  (`R/merge.rs:122-127`). The hybrid — scan while `serial ≥ resume`, binary-search only
  on restart — strictly dominates. Parity-safe (same rows returned).
- **(L3) Merge fanout k / size-tiered policy.**
  Today the cascade is strictly 2-way: `merge_runs` takes exactly one older + one newer
  run (`R/merge.rs:200-206`) and a spill merges occupied tiers one by one
  (`R/runs.rs:265-289`); each batch participates in ≤ log2(B) merges (`R/merge.rs:6-9`).
  A k-way merge (one heap over k readers, one output pass) gives ≤ log_k(B) participations:
  merge traffic scales by log_k/log2 — k = 4 halves it (n·log2 → n·log4). Costs: k
  reader buffers × m bytes per merge (vs 2 today, `R/merge.rs:213-214`) — buffer bytes
  are the caller's `merge_buffer_bytes` per reader, outside the run-byte ownership
  account (`R/runs.rs:143-147` counts rows, not buffers); more simultaneous live inputs
  raise `merge_input_bytes` and the reserved peak (`R/runs.rs:253-292`), which presses
  the 64 MiB ceiling for large stores. Tradeoff: wider merges write fewer, larger runs —
  fewer tiers, but each spill does more work at once (latency spike per spill). Parity:
  k-way with newest-wins-on-tie reproduces `R/merge.rs:222-242` semantics exactly →
  identical roots; parity-safe.
- **(L4) Pending ceiling vs merge work.**
  Default P = 4,096 rows (`R/reduce.rs:25`) = 393,216 B of encoded rows; the ordering
  ceiling is 64 MiB (`R/runs.rs:39`), which owns 699,050 rows of run storage at 96 B, or
  ~349,525 pending rows under the deliberate ×2 pending charge (`R/runs.rs:130-139`,
  `:136`). Raising P toward the byte ceiling (or making the bound byte-based) makes any
  workload touching ≤ ~350k serials run entirely in memory: O(r) time, zero spills,
  merges, consolidations and backing I/O; the grid's 2,000-pair workload (2,001 distinct
  serials) would not spill at all at the default P = 4,096 — the n^1.58 receipt measures
  the **forced** P = 64 regime (`verify-R2-F8-scaling.md:169-170`). Costs: heap for the
  pending `BTreeMap<u64, Row>` — in-memory `Row` ≈ 96-112 B plus map-node overhead
  (exact size UNKNOWN; ~120 B/row estimate ⇒ ~42 MiB at 350k rows), vs today's 4,096-row
  ≈ 0.5 MiB; the ceiling stays enforced by `reserve` (`R/runs.rs:150-164`). Parity-safe
  by the threshold test (`tests/filesystem_ordering.rs:259-266`).
- **(L5) Reducer re-sampling round trips.**
  `touched_serials` visits every newest row (`R/reduce.rs:181-199`) and then
  `zero_count_serials` re-finds each serial via `state` (`F/update.rs:482`) — a full
  extra pass over the consolidated run (≈ r reads) plus its restart exposure. The visit
  could carry the states out (the rows are already in hand; nothing mutates the reducer
  between the two calls in this path — `F/update.rs:464-496`). Same shape, cheaper:
  `release_zero_count` calls `note_removed_binding` then immediately `state`
  (`R/release.rs:157-159`) — cheap (the note just inserted the row into pending,
  `R/reduce.rs:247`) but the same pattern to keep uniform. Parity-safe (same rows
  consulted, same counts derived).
- **(L6) `copy_run` re-copy in consolidation.**
  `consolidate` copies the newest tier before merging older tiers under it
  (`R/runs.rs:418-424`, rationale `:565` "so a merge never aliases its own input").
  Cost: one full read+write of the newest tier per consolidation (`:575-578`); at the
  grid that is 128 rows (runs = spills + merges + 1 in §2), and in general the newest
  tier holds 2^(trailing zeros of B) batches — one batch in the common case, and
  consolidation is a no-op when a single run remains (`:382`). The spill cascade already
  merges an *owned* run repeatedly without a copy (`:283` merges `&run` then rebinds
  `run`), so moving the newest tier's handle into `combined` appears to remove the copy;
  UNKNOWN whether an ownership subtlety (output-vs-input handle aliasing) motivated it —
  the comment asserts one. Parity: byte-identical output rows → same roots; parity-safe
  if semantics preserved.
- **(L7, found while reading) Patch-application full rebuild.**
  `apply_patches` re-reads every base page and re-emits the whole tree per patch
  (`A/patch.rs:83-126`), regardless of patch size — O(n) reads + O(n) page writes for a
  one-key `chmod`. Preserved value roots pass through undecoded (`:119-124`), so the
  cost is page I/O and rebuild, not value decoding. Stage-5 trees are small (portable
  mode/mtime plus bounded generic data, `core/AGENTS.md` scope note), so this is a
  scaling note, not a measured defect. No receipt measures it. Parity-safe by
  construction for any page-level reuse optimization (same canonical partition — the
  builder's exact-sizing arithmetic `A/build.rs:104-133` and the 2/5 rebalance
  `:353-374` decide the partition deterministically).

## 5. Round trips (repeated reads/rewrites)

1. Every merge re-reads both inputs and rewrites the union (`R/merge.rs:213-217,243-247`);
   a row is rewritten once per tier climbed (`R/merge.rs:6-9`).
2. Spill cascades re-merge all occupied lower tiers on every spill into a free tier
   (`R/runs.rs:253-289`): 62 spills ⇒ 61 merges + 1 copy at 2,000 pairs (§2 runs identity).
3. Lookup restarts: any spill drops every tier scan (`R/runs.rs:231,299,120-122`) and a
   passed cursor restarts from the front (`:341-344`) — the measured read−write residual.
4. `entry` finds then spills in the same call (`R/reduce.rs:226` find, `:239-240`
   spill): the scan position the find just established is discarded by the spill's reset.
5. Consolidation rewrites every live row once (copy + merges, `R/runs.rs:381-446`),
   twice per operation when both `touched_serials` and `finish` see > 1 live run
   (`R/reduce.rs:177,268`).
6. State re-sampling: `visit_newest_first` yields every row, then `state` re-finds each
   serial (`F/update.rs:482`); the release path re-samples per child after its own note
   (`R/release.rs:157-159`); `frontier_base` re-reads are already deduplicated by the
   prefetch map (`:71-80,191-204`).
7. Attribute patch read-back: full base-tree stream + full rebuild per patch
   (`A/patch.rs:83-126`); `Set` values are re-emitted as 3 objects each
   (`A/patch.rs:101`, `A/value.rs:32-58`).
8. Portable read: one `lookup_many` wave per level + two whole-value reads per inode
   (`A/read.rs:115-133`), each value read = 1 canonical + extent wave
   (`A/value.rs:68,80-83`).

## 6. Honesty notes

- I measured nothing and ran nothing. Every runtime figure cites
  `ordering-scaling.log:4-9`, `verify-R2-F8-scaling.md` (which reproduced the grid with
  a bit-identical binary, `:12-25,54-55`) or `diag-run.log:4-7`. Elapsed columns are
  single samples (receipt's own caveat, `verify-R2-F8-scaling.md:112-125,217-222`); no
  performance claim is made here.
- The reads−writes decomposition (§2) is arithmetic on published counters plus the
  charge sites (`R/merge.rs:217,247`, `R/runs.rs:241,348,575-578`). The split between
  scan-restart re-reads and merge dedup shrinkage is **not** separately counted by any
  existing counter; I bounded dedup (≤ superseded rows ≈ r/2 in this shape) by reading
  the merge semantics, but the exact split is UNKNOWN without new instrumentation.
- My tier-ladder arithmetic (batch counts, per-tier rewrite frequency) explains the
  magnitudes (e.g. runs = spills + merges + 1 holds exactly in all four rows) but I did
  not simulate the workload; per-phase constants are estimates, not measurements.
- Doc-vs-code tension: `R/runs.rs:117-119` says scans are reset "whenever a tier's run
  is replaced"; `spill` resets scans for *all* tiers including untouched ones
  (`:231,299` with `:120-122`). The doc understates the blast radius; this is finding
  L1, not a defect report.
- `stage-5-report.md:710-712` attributes the superlinearity to the O(n log n) tiered
  merge; that is the class of the *write* term (×2.36-2.72/doubling observed), while the
  larger read residual (≈ n^1.9) comes from the lookup-restart path. Both statements are
  consistent with the receipts; the report's phrasing does not separate them.
- UNKNOWN (not checked): exact in-memory size of `Row`/`BTreeMap` nodes (L4 RAM cost);
  whether `copy_run`'s aliasing rationale is load-bearing (L6); real-world attribute
  tree sizes beyond the Stage-5 portable scope (L7 relevance); the `filesystem/` callers
  of `lookup`/`read_opaque` outside this subsystem (only `read.rs`/`patch.rs` paths were
  read); `MAXIMUM_TREE_LEVEL` was resolved to 31 via `file/mapping/types.rs:17` as
  recorded in `verify-R2-F8-scaling.md:47-48` and confirmed by grep here, but the
  mapping module itself was not otherwise read.
- The grid's P = 64 was forced by the diagnostics client
  (`verify-R2-F8-scaling.md:169-170`); at the default P = 4,096 the same workload would
  not spill. The regime is real for any operation touching > 4,096 serials (default), or
  at any P under a caller's smaller ceiling.
