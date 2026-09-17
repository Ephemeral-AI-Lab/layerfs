# Phase 1 implementation plan — ordering subsystem and its accounting

> **Status:** Implementation plan (static source reading only: no builds, no tests, no
> measurements, no product changes). HEAD `5c98737c2` (docs-only commits over `625ad7b57`;
> `core/crates` byte-identical to `625ad7b57`), clean except this plan. Items P1-5, P1-10,
> P1-11, P1-13, P1-15, P1-16 of [#178](https://github.com/Ephemeral-AI-Lab/layerfs/issues/178)
> Phase 1. Inputs: the register §2 (Tier 0 entries 4/13/14, Tier 1 16/18/19,
> `component-decoupling/complexity-and-roundtrip-research-20260917.md`), report-C (§2
> decomposition, §4 levers L1-L5), the Phase 0 receipts (`phase0-baseline-20260917T221759Z/`),
> and the cited source. Every runtime figure quotes an existing receipt.

## 0. Scoping and shared anchors

**The anchors live on the forced-64 shape, not on defaults.** P0-3's critical finding
(`receipts/p0-3-counter-baseline.md` §5): at the default ceiling (4,096) the ordering
machinery does **no work** — `order.default` (D25) reports `rows_spilled 0, rows_read 0,
runs_created 0`; forced-64 (D26) reproduces the round-4 grid exactly. P1-5/P1-13/P1-15/P1-10
remove work that **only exists when a caller forces the ceiling down** (or touches > 4,096
serials) — #178's P0-3 note records this as an owner scoping question. So every receipt
below runs **both arms** of the frozen set (`CONTRACT.md` §5): `phase0client order 4000
2000 64` (anchor) and `order 4000 2000 4096` (default arm must stay all-zero — the
no-regression proof); the client prints the `runs.*` block at `client/src/main.rs:207`.
The machinery must stay correct and cheap for any caller-forced ceiling regardless of
P1-16 (`input.rs:98-125` checks only `> 0`).

**Anchor (D26, one update, 4,000-file base, 2,000 rename pairs):** `rows_spilled 3,968`,
`rows_touched 4,001`, `rows_read 59,007`, `rows_written 25,760`, `runs_created 124`,
`merges 61`, `peak_run_bytes 568,320`, `peak_live_runs 5`, `peak_pending 64`. Grid
per-doubling (report-C §2): read ×3.04/×2.98/×2.96 (≈ n^1.58); write ×2.72/×2.52/×2.36
(≈ Θ(r·log₂(r/P))); read−write residual 610/2,356/9,064/33,247 at ×3.86/×3.84/×3.67
(≈ n^1.9). Determinism (§6 X1): work counters bit-identical on the labelled repeat,
elapsed +17.6% — receipts gate on **counters**, never elapsed. Variables: r = distinct
touched serials (2,001); c = touches (4,001); P = pending ceiling; B = spilled batches
(≈ 62); T = live tiers ≤ 32 (`runs.rs:41`); m = merge buffer 16 KiB = 170 rows
(`runs.rs:37`); row = 96 B (`record.rs:26`). Persisted canonical bytes IDENTICAL for every
item (parity guard: `the_pending_threshold_changes_only_where_the_rows_live`,
`tests/filesystem_ordering.rs:139-269`).

---

### P1-5 — targeted scan reset in `spill()`
- **Big-O target**: lookup reads ≈ restarts-per-spill × O(rows before target) — the n^1.9
  residual — **→** one pass per tier per sweep, O(r); `rows_read` per doubling ×3.0 →
  ~×2.1-2.4 (write-term floor). Memory 0 delta (buffers retained longer, never added;
  ≤ T×m, `runs.rs:321-323`). Persisted bytes identical.
- **Before anchor**: D26 `rows_read 59,007`, residual over writes 33,247 (58%), attributed
  to `find` restarts (`runs.rs:348` charge site; the restart-vs-dedup split is bounded,
  not counted — report-C §6).
- **Change sketch**: `references/runs.rs`. `reset_scans()` `:120-122` (`self.scans.clear();`)
  runs at `:231` and `:299` of `spill()`, though a spill into level k replaces only tiers
  ≤ k (`:253-261` takes runs from `levels[..level]`; `:295-298` `for slot in &mut
  self.levels[..level] { *slot = None; } self.levels[level] = Some(run);`). Replace both
  sites with `self.scans.truncate(level + 1)` — scans of tiers > level (run, buffer,
  `resume` position) kept. Invariant: a scan at index i exists only while `levels[i]`
  holds the run it scanned; a tier changes run only in a spill at level ≥ i, which
  truncates `scans[0..=level]` ⊇ scans[i]. Keep the full clear in `consolidate()` `:438`,
  `take_single_handle` `:512`, `release` `:521` (they replace every tier). Fix docs
  `:116-119`, `:543-545`.
- **Tests**: stay green — `filesystem_ordering_scan.rs` both tests (warmup bound `:95-99`,
  one-pass sweep `sweep_reads == total` `:101-111`, restart-parity `:129-157`);
  `filesystem_ordering.rs` threshold parity `:139-269`,
  `a_spilled_lookup_agrees_with_a_full_scan_of_every_tier` `:720-808`,
  `run_lookup_answers_every_serial_in_every_order` `:811-882` (every-order truth proof),
  `tier_carries...` `:272-331`; bounds owner tests. None must change (no test pins reset
  blast radius). NEW: `a_spill_keeps_the_scans_of_tiers_above_its_level` — 12 spills of
  64 rows (live tiers 2, 3), ascending finds over serials 1..=256 (tier 3 cursor at row
  256/512), snapshot `rows_read`; spill batch 13 (lands level 0, **no cascade**, charges
  `rows_written` only); then `find(257)`. Assert `rows_read` delta == 1 (resume) vs ≥ 257
  today (restart from offset 0); cross-check `RecordingBacking.counters().reads`
  (`support/filesystem.rs:394`) flat across the spill for reads serving tier 3.
- **Verification receipt**: both arms per §0. Counter `runs.rows_read` down; rough
  magnitude 59,007 → ~27-35k as the residual collapses to O(r); per-doubling ×3.0 →
  ×2.1-2.4 across the 250/500/1k/2k grid arms (a prediction — the receipt records the
  measured value). `rows_written`/`runs_created`/`merges` unchanged. Determinism: counters
  expected bit-identical (X1 precedent); one labelled repeat; elapsed never gated.
- **Commit slicing + dependencies**: one product commit (± ~6 lines) + new test;
  `core/docs/architecture/04-filesystem.md` updated in the same commit; production LOC
  reported. After P1-11 (clean receipts); before P1-10/P1-13/P1-15 (their marginals are
  meaningless while gratuitous resets dominate).
- **Risks / open questions**: index-parallel scan-vs-run identity — an off-by-one
  (`truncate(level)` vs `level + 1`) keeps a scan whose run was replaced at `levels[level]`
  (stale answers); `:811-882` is the guard. Retained buffers survive spills: total
  unchanged (≤ T×m) but live longer — the client's `coexist` row should record it. The
  restart-vs-dedup split stays unmeasured; P1-5's receipt is its first direct evidence.

### P1-10 — carry ordering state out of `touched_serials`
- **Big-O target**: the zero-count state re-sweep O(r) charged run reads (one ascending
  re-pass over the consolidated run ≈ `serials_scanned` rows) **→** 0 run reads (states
  carried; `visit_newest_first` charges nothing — `RunReader::next` has no charge site,
  `merge.rs:181-183`). Memory: the collection 8 → ~16 B/serial (serial + count-or-delta);
  the declared ceiling charge doubles (below). Persisted bytes identical.
- **Before anchor**: D26 — after `touched_serials` consolidates (`reduce.rs:177`),
  `zero_count_serials` calls `reducer.state(*serial)` per serial (`update.rs:482` →
  `reduce.rs:160-169` → `runs.find`), re-passing the consolidated run ≈ 2,001 charged
  reads inside 59,007. **Honesty:** at the default ceiling there are no runs to re-find
  (D5 `c1.subtree-remove`: 202 rows, 0 spilled) — #178's "release-heavy workloads" verify
  line only bites under a forced ceiling; the receipt uses the forced-64 arm.
- **Change sketch**: `reduce.rs:176-207` `touched_serials` returns `Vec<u64>`; carry state
  out of the visit it already performs (the visitor holds each newest `Row` `:181-199`,
  drains pending keys `:183-193`): return `Vec<(u64, CarriedCount)>`, `CarriedCount =
  New(u64) | Existing(i64)` (mapping mirrors `reduce.rs:165-168`; `update.rs:482-489`
  needs only count/delta). `update.rs:477-496` derives counts from the carried state and
  drops the `state()` call; the defensive `None` arm `:488` becomes unreachable — keep an
  explicit refusal. Ceiling: `maximum_touched_serials()` = `ordering_bytes/8`
  (`input.rs:90-95`, refusal `update.rs:465-473`) charges one u64/serial; at 16 B/serial
  the divisor becomes 16 — a documented, enforced change. `note_serials_scanned`
  unchanged. The release pair `note_removed_binding`→`state` (`release.rs:157-159`) stays:
  a pending-map hit (the note just inserted the row, `reduce.rs:247`), cheap by design
  (report-C L5).
- **Tests**: stay green — threshold parity; bounds `every_reported_owner...`
  (`serials_scanned == 31`, `:527-534`), `released_subtrees...` (`serials_scanned == 3`,
  `:430-434`); `streaming.rs` peak_pending. NEW:
  `carried_state_removes_the_zero_count_re_pass` — spilling reducer (public
  `ReferenceReducer` + `RecordingBacking`, pending 8, N = 200), call `touched_serials`,
  snapshot `runs.rows_read`, derive every count from the carried result; assert
  `runs.rows_read` unchanged after the whole collection (today every run-held serial's
  `state()` charges ≥ 1).
- **Verification receipt**: both arms per §0. Counter `runs.rows_read` down by ≈
  `serials_scanned` (2,001 at the anchor; ~3% of 59,007 standalone, ~7% of the post-P1-5
  value). `serials_scanned`/root unchanged. Determinism: deterministic; repeat labelled.
- **Commit slicing + dependencies**: one commit (reduce.rs + update.rs + input.rs divisor,
  ~+30/−15) + test; the ceiling-arithmetic change is named in the commit message and doc.
  After P1-5 (clean marginal; otherwise the re-sweep is inflated by restarts); no overlap
  with P1-13/P1-15 beyond the module.
- **Risks / open questions**: nothing mutates the reducer between collection and
  derivation in this path (`update.rs:464-496`; report-C L5 verified) — carried state must
  be consumed before `release_zero_count` mutates pending. Public-API change to
  `touched_serials` (one caller). The ceiling-divisor halving refuses some extreme-dial
  callers earlier — owner-visible, stated in the receipt.

### P1-11 — fix `SortedWork.pages_read` (batched decodes never counted)
- **Big-O target**: none — a correctness fix to telemetry. Time/memory/space unchanged;
  the *reported* `pages_read` rises from a lower bound to the true count, and `read_waves`
  counts batched waves.
- **Before anchor**: the doc promises "Pages read, including batched ones"
  (`sorted/page.rs:40-41`) but only the point read increments it (`:185-186`);
  `batch_children` fetches a chunk via `self.objects.read_batch(&ids)?` (`:241`) and the
  caller decodes each child with `decode_page` (`sorted/merge.rs:261`) — neither
  increments. P0-3 stamps every `pages_read` figure with this caveat
  (`p0-3-counter-baseline.md:15-16`, `CONTRACT.md:145-147`); D26's `dir_pages_read 2` is a
  lower bound.
- **Change sketch**: `sorted/page.rs:220-259` `batch_children`, on the successful batch
  path (before `return Ok((chunk, values, Some(lease)))` `:256`): `pages_read += chunk`,
  `read_waves += 1` — one wave per `read_batch`, mirroring `Engine::read` (`:185-186`)
  and `objects.read_batch`'s own wave count (`objects.rs:89,107`). The fallback
  `Ok((1, Vec::new(), None))` `:258` stays uncounted — its caller point-reads
  (`sorted/merge.rs:266`), already counted. One caller only (`sorted/merge.rs:249`, the
  changed-path merge `edit`); read paths (directory/attributes listing) are point-read
  and unaffected.
- **Tests**: stay green — `filesystem_sorted.rs:55` (`pages_read <= 1` on an optional-base
  build: no stored pages, stays 0); `filesystem_attributes.rs:547-559` (read path);
  bounds `every_reported_owner...` (no pages_read assertion). Values CHANGE (re-derive;
  expected to survive as bounds, recalibrate only from the honest count): bounds
  `:209-217` (`directories.pages_read <= depth + 2`, `inodes.pages_read <= 2` — batched
  descent children now count) and `:308-318` (`pages <= directory_leaves + 4` via
  `rename_three`'s returned value `:292` — batched siblings now count). NEW:
  `batched_children_are_counted_as_pages_read` — small change over a branch-level base
  (the 900-entry shape, `filesystem_sorted.rs:38-66`); assert `pages_read` equals the
  pages the counting provider supplied, and `pages_read > read_waves` for any wave
  fetching ≥ 2 children (one wave, N pages) — the equality fails on the current tree.
- **Verification receipt**: both arms per §0 + one wide C1 shape (`fs.c1.directory-update`).
  Counters `directories.pages_read` / `inodes.pages_read` up to the true count
  (shape-dependent: single-leaf bases ~0 delta; branch-level shapes reveal the invisible
  children). The receipt **declares pre-P1-11 `pages_read` columns non-comparable** and
  re-baselines them; `runs.*` unchanged (proof it is telemetry-only). Determinism:
  deterministic.
- **Commit slicing + dependencies**: one tiny commit (+~4 lines) + test, with
  `core/docs/architecture/10-counters.md` (the `SortedWork` contract) updated in the same
  commit; **lands first** so every later receipt (P1-1/P1-3/P1-4 change wave widths
  against this counter; the ordering vehicles print `dir_pages_read` in the same block)
  shares one honest baseline.
- **Risks / open questions**: none mechanical; only baseline confusion — P0-3's
  `pages_read` figures keep their caveat forever. Any bounds recalibration must be
  justified from the honest count, not loosened to pass.

### P1-13 — merge fanout 4 / size-tiered cascade
- **Big-O target**: merge participations per batch log₂(B) → log₄(B) (halved); total merge
  traffic Θ(r·log₂(r/P)) → Θ(r·log₄(r/P)) (≈ ½); `rows_written` per doubling ×2.36-2.72 →
  toward ×2.0. Memory: merge buffers 2×m → up to 5×m live in one merge (4 input readers +
  optional m output staging) = 32 → 80 KiB at default m — **outside** the ownership
  account (`runs.rs:143-147` counts rows, not buffers; the caller's `merge_buffer_bytes`
  dial is the only control). Space (honest axis): backing run-file append volume ≈
  `rows_written`×96 → ~½; `peak_run_bytes` comparable. Persisted bytes identical
  (newest-wins-on-tie reproduced exactly, report-C L3).
- **Before anchor**: D26 `rows_written 25,760`, `merges 61`, `runs_created 124`, identity
  runs = spills + merges + 1 (62/61/1, report-C §2); the cascade re-merges occupied lower
  tiers one by one (`runs.rs:253-289`, strictly 2-way `merge.rs:200-206`); the rebind
  phase's first spill re-merges the whole unbind ladder (report-C §3.1).
- **Change sketch**: `merge.rs:6-9` ("each spilled batch participates in at most
  `log2(batches)` merges") + the cascade `runs.rs:253-289`. Two parity-safe shapes.
  **(a) recommended** — keep one-run-per-tier, first-free-tier ladder and level-order
  newest-wins (`runs.rs:209-210`, early return `:368-370`); replace the sequential 2-way
  cascade with multiway `merge_runs` over up to 4 inputs (batch + ≤ 3 occupied lower
  tiers → output lands 3 tiers up): a cascade into tier k needs ⌈k/3⌉ merge calls instead
  of k−1; tier indices (≤ log₂ B) and the live-run shape (popcount(B)) are unchanged —
  participations halve because each merge absorbs three tiers. Buffers at one merge:
  4 × `RunReader::new(_, buffer_bytes)` (`merge.rs:213-214` shape) + m output staging if
  appends are batched = **5×16 KiB live vs today's 2 readers + row-wise `handle.append`
  (`merge.rs:243`, `backing.rs:314-326`) — the "3×16 KiB" reading additionally credits
  today an output staging buffer it does not have; either way the delta is bounded
  (+48 KiB worst case) and un-accounted — see risks**. **(b) true size-tiered** (≤ 4
  runs/tier, merge on the 4th): tiers ≤ log₄(B) (3 vs 6 at the anchor) but `find`/scan
  ownership must be reworked (multiple runs per tier, age precedence, scans-per-run) — a
  much larger diff (~+150 lines) touching what P1-5/P1-15 just stabilized. The 4-way
  merge: selection over 4 current rows, newest-wins generalizing `merge.rs:222-242`;
  charges `rows_read += Σ inputs`, `rows_written += output` as today.
- **Tests**: stay green — threshold parity; `a_spilled_lookup_agrees...`;
  `run_lookup_answers_every_serial_in_every_order`; scan tests (the fixture comment
  `filesystem_ordering_scan.rs:66-69` "a power-of-two count would collapse to one" is
  only wrong under (b), where powers of four collapse; `live_runs() >= 2` `:80-83` holds:
  12 spills → 3 live runs base-4). Existing that must CHANGE / be re-derived — every
  cascade-count assertion: `filesystem_ordering.rs:298-299` (`tier_carries...`:
  `merges >= 1` — 12 spills → 7 merges under (a), 3 under (b); `peak_level >= 1` — 3
  under (a), 1 under (b)); `filesystem_ordering.rs:646-656` (`a_successful_operation...`:
  `peak_live_runs >= 2`, `rows_written >= appends` — re-derive; risk if the fixture's
  batch count is an exact power of 4 under (b)); `filesystem_bounds.rs:499-516`
  (`every_reported_owner...`: `merges > 0`, `peak_live_runs >= 2`, `runs_created ==
  observed.creates` — the equality holds by construction); `filesystem_ordering.rs:230-241`
  (spilling still happens at pending 1). The P0-3 identity becomes runs = spills +
  multiway-merges + 1 — the new receipt records it. NEW:
  `a_four_way_cascade_merges_three_tiers_in_one_pass` — spill until tiers 0-2 occupied,
  spill again; assert `merges` grew by exactly 1 (not 3), `runs_created` by 2 (batch +
  output), `rows_written` by the union size once (no intermediate rewrite); plus a
  precedence case: a serial written into all three older tiers and the new batch, newest
  wins after the 4-way merge (extends the `:811-882` shape).
- **Verification receipt**: both arms per §0 + the 250/500/1k grid arms for the slope.
  Counters: `rows_written` down toward ×2.0/doubling (anchor 25,760 → ~12-15k); `merges`
  61 → ~18-24 under (a); `runs_created` 124 → ~82-88; `rows_read` down proportionally
  (merge reads dominate it after P1-5); `peak_live_runs` 5 unchanged under (a);
  `peak_run_bytes` ~568 KiB comparable. Determinism: deterministic.
- **Commit slicing + dependencies**: one commit (merge.rs + cascade + tests +
  `core/docs/architecture/04-filesystem.md`). After P1-5; **before P1-15** (restart
  frequency and run lengths follow the cascade policy; under (b) P1-13 rewrites `find`
  and P1-15 rebases). If (b) is chosen, split: multiway `merge_runs` with the (a)
  cascade first, then the size-tiered restructure — one variable per commit.
- **Risks / open questions**: the buffer math is un-accounted memory — the ceiling
  (`reserve`, `runs.rs:150-164`) never sees it; the coexist probe should report live
  merge buffers and the doc must say so. Under (a) a spill does more at once (latency
  spike; larger reserved transients: 4 inputs + output concurrently — enforcement
  unchanged, peak-owned shape changes; bounds
  `merge_inputs_and_output_are_covered_by_the_declared_ceiling` `:678-740` re-verifies).
  Owner question: (a) vs (b) — (b) is what "size-tiered" usually means and yields fewer
  tiers, but it invalidates the one-reader-per-tier design the scan tests pin; recommend
  (a).

### P1-15 — hybrid binary-search-on-restart for ordering lookups
- **Big-O target**: a restart (request behind the cursor) from O(position) sequential
  rows re-read (position ≤ run length, ≤ 2,048 in the anchor's top tier) **→** O(log₂
  rows) single-row probes (~11 for 2,048) + a short sequential tail. Ascending sweeps
  keep the pinned one-pass property (the resume arm is untouched): pure binary search on
  *every* lookup would pay ~log₂(rows) random reads per lookup (2,000 × 11 ≈ 22k > the
  one-pass 2k, report-C L2) and defeat the 170-row buffered read (`merge.rs:117-136`) —
  hence binary **only when a restart actually happens**. Memory +0 (stack
  `[u8; ROW_BYTES]` probe buffer, no heap). Persisted bytes identical.
- **Before anchor**: the restart arm `runs.rs:341-344` (`match scan.resume { Some(resume)
  if serial >= resume => {} _ => scan.reader.start(run, 0)? }`) restarts from offset 0;
  each row re-read charged (`:347-348`). After P1-5 resets are targeted, but restarts
  remain: ~1 per spill for tiers ≤ level, the `entry` find-then-spill discard
  (`reduce.rs:226` find, `:239-240` spill — the position the find just established is
  dropped for the batch's own tier), and the release path's interleaved demands.
  Post-P1-5, restart cost is the remaining lookup residual.
- **Change sketch**: `runs.rs` `find`, restart arm only: row-aligned binary search over
  `[0, run.count)` for the first row with serial ≥ target (rows sorted — `record.rs:3-5`,
  merge output order `merge.rs:221-248`; fixed 96 B — `record.rs:26`; `read_at` at
  arbitrary offsets — `backing.rs:328-333`, seek + `read_exact`; `RunScan::start` already
  accepts and validates any row-aligned offset — `merge.rs:86-99`), probing with a stack
  96-B buffer; then `scan.reader.start(run, probe_offset)` and continue the existing
  compare loop (`:345-361`) unchanged. Each probe row charges `rows_read` (honest: ~11
  charged rows vs ~position today). The `resume` arm `:342` is untouched — that is what
  keeps `sweep_reads == total` true.
- **Tests**: stay green — `filesystem_ordering_scan.rs` in full: one-pass sweep
  `:101-111`, zero-allocation wave incl. the restarting wave `:116-125` (stack probes
  allocate nothing), warmup bound `:95-99`, restart-parity `:129-157` (restart answers
  must equal fresh-scan answers — this item's correctness proof);
  `run_lookup_answers_every_serial_in_every_order` (descending/interleaved orders are
  restart-heavy; answers unchanged); threshold parity. None must change (no test asserts
  restart read counts). NEW: `a_restart_costs_logarithmic_reads_not_linear` — one run of
  N = 2,048 rows, sweep to the end, then `find(1)` (behind cursor): assert `rows_read`
  delta ≤ log₂(N) + tail (≈ 12) vs N today; a mid-run variant (`find(N/2)` after passing
  it) likewise; the answer equals the fresh-scan answer; cross-check
  `RecordingBacking.counters().reads` ≤ ~13 `read_at` calls.
- **Verification receipt**: both arms per §0. Counter `runs.rows_read` down; magnitude
  honestly modest at the anchor — after P1-5 and P1-13 the remaining restarts are ~1/spill
  at ≤ 11 rows each, so a small single-digit-% additional drop, not a step change; the
  unit test is the primary discriminator and the receipt says so. Determinism:
  deterministic.
- **Commit slicing + dependencies**: one commit (find + tests, ~+40 lines). **After
  P1-13** (restart pattern and run lengths follow the cascade policy; under (b) P1-13
  rewrites `find` and this item rebases). Independent of P1-10/P1-16.
- **Risks / open questions**: random reads on the backing — 96-B probes defeat the
  170-row buffered read and OS read-ahead (`backing.rs:328-333`); net `read_at` *call*
  count can rise for short restarts (sequential pays ⌈position/170⌉ calls; binary pays
  ~11 + 1) while decoded rows and bytes always drop for position > ~11 — the receipt
  reports both rows and the backing's read-call counter. Probes must charge `rows_read`
  or the counter becomes dishonest the other way. `RunScan::start`'s validation
  (`merge.rs:87-93`) already rejects misaligned probes.

### P1-16 — pending-ceiling dial: document (and optionally re-default)
- **Big-O target**: for ≤ P touched serials the spill machinery already costs O(0); the
  dial extends the spill-free regime from P = 4,096 to **P ≤ 349,525** under the default
  64 MiB ceiling — O(r) total, zero spills/merges/consolidations/backing I/O (report-C
  L4); above it the Θ(r·log(r/P)) regime applies unchanged. Memory (the crux): the
  account charges 2×96 = 192 B/row of encoded-row equivalents (`charge_pending`,
  `runs.rs:130-139`, `:136 self.reserve(bytes.saturating_mul(2))?` — pending plus the run
  it becomes, `:134-136`), while the real heap is the `BTreeMap<u64, Row>` at ~120+ B/row
  (exact size UNKNOWN, report-C §6) — **at the dial's top, ~42 MiB of real heap is
  invisible to the ownership account**. Space: zero backing volume in the spill-free
  regime. Persisted bytes identical for any P (threshold parity).
- **Before anchor**: `DEFAULT_MAXIMUM_PENDING = 4_096` (`reduce.rs:25`, wired
  `input.rs:74`); `DEFAULT_ORDERING_BYTES = 64 MiB` (`runs.rs:39`); D25 proves the frozen
  set never spills at 4,096 (peak_pending 2,001). Math: 67,108,864 / 192 = 349,525.33 →
  349,525 rows.
- **Change sketch**: **document-only deliverable** — a section in
  `core/docs/architecture/04-filesystem.md` (with `06-limits.md` for the ceiling
  arithmetic) + a #178 comment stating: the dial is
  `FilesystemResources.maximum_pending_records` (`input.rs:59-60`); the spill-free bound
  is `floor(ordering_bytes / (2 × ROW_BYTES))`; the ×2 rationale (`runs.rs:134-136`); the
  real-heap caveat; and that `maximum_touched_serials` (`input.rs:90-95`,
  ordering_bytes/8 = 8.4 M) does not bind before the pending bound. Optional, owner-gated:
  re-default `DEFAULT_MAXIMUM_PENDING`. **Any DEFAULT change is an owner decision recorded
  on #178, not the implementer's.**
- **Tests**: stay green — threshold parity (the proof the dial changes only where the
  rows live); `the_operation_ceiling_is_enforced...` (`filesystem_ordering.rs:393-460`);
  `a_backing_too_small...` (`filesystem_bounds.rs:628-675`); default-resources tests (a
  re-default only widens the no-spill regime; `streaming.rs:218-222` re-checked). NEW
  (required deliverable): `a_high_pending_ceiling_runs_spill_free_to_the_byte_bound` —
  scalable boundary: `ordering_bytes = 192 × 100`, P = 100: an operation touching exactly
  100 serials reports `rows_spilled == 0`, `runs_created == 0`, `peak_pending == 100`,
  same root as a spilling arm (pending 8); the 101st serial spills (`rows_spilled > 0`,
  same root); P beyond the byte bound (P = 200 at the same ceiling) is refused at the
  first spill with `ObjectLimitExceeded` — the 349,525 arithmetic pinned in miniature.
- **Verification receipt**: default arm per §0 (unchanged zeros) + a raised-P diagnostic
  arm (the receipt states exactly what ran; a 4,001-touch operation cannot cross either
  bound, so the boundary evidence is the new test plus the byte-bound refusal shape).
  Counters `rows_spilled`, `runs.*`, `peak_pending`. Determinism: deterministic.
- **Commit slicing + dependencies**: docs commit (production LOC delta 0) + the
  boundary-test commit. Lands last: it documents the post-P1-5/13/15 machinery, and a
  re-default would erase the anchor shape the other items verify against — sequence it
  after their receipts exist either way.
- **Risks / open questions**: the memory arithmetic under the ×2 charge — the ceiling
  "owns" encoded-row bytes, not heap; a high default commits unaccounted memory (~42 MiB
  at the top) on a path whose declared ceiling says 64 MiB; the owner must rule whether
  the map's real footprint belongs in a declared account before any re-default. A
  re-default also changes which workloads exercise the spill machinery at all — P0-3
  found none in the frozen set, so a re-default cannot be justified by measurement yet,
  only arithmetic; and it would leave P1-5/P1-13/P1-15 verifiable only under forced
  ceilings (they stay correct and cheap for any caller-forced P, `input.rs:98-125`).

---

## Dependency order

1. **P1-11** — first, unconditionally: re-baselines `pages_read`/`read_waves` honestly;
   every later receipt prints those columns and must not straddle the semantics change.
2. **P1-5** — removes the n^1.9 residual that inflates every later ordering receipt; the
   P1-10/P1-15 marginals are only meaningful after it.
3. **P1-13** (fanout-4 cascade, design (a)) — changes `rows_written`, `merges`,
   `runs_created` and the cascade shape; precedes P1-15 because restart frequency and run
   lengths follow the cascade policy (and under design (b) it rewrites `find` itself).
4. **P1-15** — last of the run-store changes, against the final cascade shape; its unit
   test is the primary discriminator, the vehicle receipt records the marginal.
5. **P1-10** — after P1-5 (and after P1-13 for the cleanest single-variable delta); its
   public-API and ceiling-divisor changes ride in their own commit.
6. **P1-16** — last: documents the settled machinery; any re-default is a separate owner
   decision on #178, taken only with a workload receipt and a ruling on the unaccounted
   heap.

Every item is a separate, measured, single-variable commit with its own append-only
receipt under #171's frozen contract; the sealed-oracle parity set and the threshold
parity test stay green throughout; production LOC is reported per commit; the
`core/docs/architecture/` doc update rides in the same commit as the code (per
`core/AGENTS.md`).
