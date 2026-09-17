# Phase 1 test evaluation — #178 P1-1..P1-16 (2026-09-17T23:00Z)

> **Status:** test-planning record for Phase 1 of
> [#178](https://github.com/Ephemeral-AI-Lab/layerfs/issues/178). Static source
> reading only — no builds, no test runs, no file changes. Every claim cites
> `file:line` in the tree at `625ad7b57`. Registers: #176 §2 (Tier 0 items 1–8,
> 13, 14; Tier 1 items 16–19). Phase 0 anchors:
> [`../phase0-baseline-20260917T221759Z/`](../phase0-baseline-20260917T221759Z/)
> (receipts `p0-3-counter-baseline.md`, `CONTRACT.md` §5 workloads D1–D27).

Test trees read: `core/crates/layerfs-content/tests/` (31 files) and
`core/crates/layerfs-storage/tests/` (24 files), plus the vehicles
(`core/crates/*/examples/`) and the cited product sources.

## 1. The parity guard list — 34 tests, all must stay green UNCHANGED

The sealed-oracle set. No Phase 1 item may change any expected value, fixture
byte, sealed identity or scope name in these 34 tests. The generator
`crates/layerfs-content/tests/stage5_reference_fixtures.rs` is `#[ignore]`d and
env-gated; its read-only counterpart below runs on every `cargo test`.

| Target | Tests (test-file:line) | What it pins | Most sensitive items |
| --- | --- | --- | --- |
| `fixture_seal` | `the_sealed_fixtures_are_exactly_the_frozen_ones` (fixture_seal.rs:55); `the_generator_cannot_reseal_during_an_ordinary_test_run` (:82) | blake3 digest list of `tests/fixtures/filesystem/` vs `filesystem.seal`; generator stays ignored + env-gated | tripwire for all 16 — catches accidental reseals when partition-adjacent work goes wrong (P1-1, P1-12, P1-13) |
| `filesystem_reference` | `construction_matches_the_sealed_reference_roots_and_pages` (filesystem_reference.rs:302); `updates_match_the_sealed_reference_operation` (:309) | per sealed case: root identity, complete reachable object set with page shapes (role, level, count) (filesystem_reference.rs:292-296), read-back logical state incl. removed serials (:360-413) | P1-1 (batched tree reads must not change shapes), P1-3 (neighbour merges), P1-4 (validate restructure), P1-10 (zero-count carry), P1-13 |
| `edit_reference` | `the_candidate_reproduces_the_reference_root_and_partition` (edit_reference.rs:592); `the_oracle_fixtures_are_the_sealed_reference_revision` (:644) | 9 sealed cases (edit_reference.rs:593-603): base root, base partition, edited root, edited partition, surviving leaf identities (edit_reference.rs:611-634) | P1-6 (`unequal-height-join` :597 hits tree.rs:711), P1-7 (`untouched-sibling`, `batch-normalized`), P1-8 (`interior-multi-level`), P1-9 (`root-collapse`), P1-14 |
| `object_identity` | 11 tests: :54, :66, :99, :132, :146, :175, :225, :236, :246, :317, :381 | frozen envelope bytes/identities, 8 MiB field / 16 MiB envelope ceilings, domain separation, advisory predecessors (:381-417) | P1-14 (whole-file envelope route, `encode_whole_file`), P1-9 (predecessor semantics :396-416) |
| `filesystem_codec` | 9 tests: :56, :96, :133, :151, :182, :241, :306, :325, :407 | golden bytes per grammar (inode/directory/root/symlink/attribute), one-grammar invariant, rejection matrix, leaf-ceiling partition math (:325-375), role/flag checks | P1-12 (`a_directory_leaf_never_exceeds_the_page_ceiling` :325-375 pins the width/partition math `push()` recomputes), P1-1 |
| `filesystem_updates` | 6 tests: :23, :95, :160, :205, :251, :306 | read-back exactness, content-only change touches no directory page (:95-157), same-input no-op keeps root (:160-202), last-binding removal (:251-293), refusal publishes nothing (:306-342) | P1-10 (:251), P1-4 (:95), P1-1 |
| `filesystem_profile` | `the_profile_text_names_the_bounds_it_is_hashed_with` (filesystem_profile.rs:24); `a_foreign_profile_is_refused_before_a_root_exists` (:44) | `PROFILE_DESCRIPTION` == text rebuilt from the named bounds; `profile_id` == hash of it | tripwire: no item may move any bound the profile names (P1-16 documents, does not move; the 32-tier cap of P1-13 is not profile text) |

## 2. Per-item test map

Legend: **Green** = existing tests pinning the affected behaviour (must stay
green). **UPDATE** = existing test asserts current behaviour the item changes
(pinned-behaviour change — do it deliberately, say so in the commit message,
round-3/4 precedent). **NEW** = required new test with its discriminating
assertion (must fail on a wrong/pre-item implementation).

### P1-1 · Batch filesystem-tree page reads (`BATCH_CHILDREN` 32 → toward 4,096)
- **Green:** `filesystem_reference` (both — shapes identical); `filesystem_sorted.rs:104` `leaf_and_branch_boundaries_are_partitioned_canonically` (counts 1..1500, incl. 740/741/742 at the split ceiling); `filesystem_bounds.rs:300` `a_small_change_reads_its_own_path...` (upper bounds only: demanded ≤ 3·(leaves)+8 :311-314, pages ≤ leaves+4 :315-318 — batching must not over-read); `filesystem_updates.rs:95` (content-only change emits no directory page); `filesystem_attributes.rs:464+` (reader still charges its pages).
- **UPDATE:** `filesystem_bounds.rs:219` `provider.peak_wave() <= 32` — the only test that pins the grouped width. Its fixture (depth 8, ~5 children/page) keeps waves ≤ 32 either way, so it stays green, but the bound goes stale the moment `BATCH_CHILDREN` (sorted/page.rs:28) rises. Recommended deliberate update: assert against the new constant (or `MAXIMUM_READ_DEMANDS`, objects.rs:20) so the pin keeps meaning. Also `file_read.rs:245-255` pins `node_batches_read == 1 + leaves.div_ceil(READ_NAVIGATION_WAVE)` (mapping-read wave = 32, mapping/read.rs:40) — a different constant; if an implementer raises both, this exact-equality fails. Keep them separate.
- **NEW:** `filesystem_tree_navigation_reads_a_wide_branch_in_one_wave` — a fixture whose changed-path merge must load > 32 sibling pages in one level (e.g. a 4,000-entry directory update, the `wide(4_000,..)` shape of filesystem_bounds.rs:54): assert `provider.peak_wave() > 32` and `result.counters.objects.read_waves < pages_read` (today point reads at directory/read.rs:208, inode/read.rs:51, patch.rs:199 increment `read_waves` per page — page.rs:185-186 — so waves == pages today → fails today).

### P1-2 · Pool the read connection/session per operation
- **Green (storage):** `visibility.rs:109` `an_unrelated_reader_cannot_see_an_open_save_but_sees_it_after_acknowledgement` (per-wave `VisibilityCeiling` refusal :144-155); `visibility.rs:306` `the_watermark_survives_reopen_and_still_hides_a_later_open_save`; `visibility.rs:358` `a_pooled_read_refuses_a_value_group_above_the_captured_ceiling`; `visibility.rs:497` `the_pooled_lane_supplies_the_owners_ceiling_at_every_read_site` (source-text scan of `src/cas/owner.rs` regions for `i64::MAX` / `self.ceiling`); `cas_roundtrip.rs:204` `a_read_wave_is_bounded_by_the_declared_ceiling`; `connection_profile.rs:16/33/44/62` (profile verified on acquisition — a pooled connection must still verify); `provider_errors.rs:115`; `core_pipeline.rs`, `edit_pipeline.rs`, `filesystem_pipeline.rs` round-trips.
- **UPDATE:** none strictly. Watch: `visibility.rs:497-527` locates regions by markers `fn select_pooled(` / `fn sync_pool_index(` in owner.rs — if pooling restructures those functions the scan's regions move; keep the names or update the markers in the same commit.
- **NEW:** `read_waves_share_one_connection` — run two `store.read_batch` waves in one operation and assert the new connection counter (see §3 prerequisite) reports 1 open, both results correct; today `cas/store.rs:211`/`:247` open per wave → counter reads 2 → fails today. The per-wave ceiling capture (`cas/read.rs:49-67`: locations collected at `i64::MAX`, refusal above ceiling) must remain per wave, not per operation.

### P1-3 · Batch `page_from_wire` per-child point reads
- **Green:** `filesystem_reference` (merge outputs byte-identical); `filesystem_sorted.rs:104` (partitions); `filesystem_topology.rs` (all); `filesystem_bounds.rs:334` (released-subtree page bounds).
- **UPDATE:** none found — no test pins the child-read wave count of `page_from_wire` (page.rs:470 `self.read(child, false)`).
- **NEW:** `neighbour_merge_reads_children_in_waves` — the wide underfull-chain shape (a merge that loads many siblings): assert `counters.*.read_waves < pages_read` for the merge (today equal, one wave per child) while the resulting root equals the point-read arm's root on the same input.

### P1-4 · Batch validate lookups + memo pages across the three walks
- **Green:** `filesystem_bounds.rs:545` `validation_reads_are_charged_to_the_operation` (`validation.objects_read > 0`, `inode_demands > 0`, `read_waves > 0` :570-582 — after batching, waves drop but must stay > 0); `filesystem_bounds.rs:743` `the_cycle_check_work_limit_is_reachable_and_reported` (`entries_examined == limit` EXACTLY at the ceiling :804-808 — memo must not change entry accounting); `filesystem_bounds.rs:848` (ceiling-capped rebinding refusal); `filesystem_topology.rs` (cycle/parent refusals, all cases :179-699); `filesystem_timing.rs:125-127` (the `validate` phase name survives); `filesystem_updates.rs:251`.
- **UPDATE:** none mandatory. **Danger pin:** `filesystem_attributes.rs:556-559` — "a second stat charges its own read" (`directory.pages_read > after_first`) pins NO page memoization on the public reader path. P1-4's memo must stay scoped to the validation operation; if it leaks to `FilesystemRead`, this becomes a pinned-behaviour change (avoid).
- **NEW:** `validation_lookups_are_batched_not_per_binding` — the 200-entry rename shape (filesystem_ordering.rs:726-745): assert `validation.read_waves < validation.inode_demands` (today lookups are per-binding single demands, validate.rs:175 → equal → fails today) with the root unchanged.

### P1-5 · Targeted scan reset in `spill()` (reset `scans[0..=level]`, not all)
- **Green:** `filesystem_ordering.rs:721` `a_spilled_lookup_agrees_with_a_full_scan_of_every_tier` (spilled root == unspilled root :800-807 — the parity guard #176 names); `filesystem_ordering.rs:139` `the_pending_threshold_changes_only_where_the_rows_live` (spilled == unspilled root :259-267); `filesystem_ordering.rs:811` `run_lookup_answers_every_serial_in_every_order` (ascending/descending/interleaved/cycled :847-880); `filesystem_ordering_scan.rs:61` `lookups_allocate_nothing_after_the_tiers_are_built` — all three assertions: warmup ≤ `live_tiers*2+4` (:96-99, an upper bound — fewer resets only help), one-pass sweep `rows_read == total` EXACTLY (:110-111), zero-allocation restarting wave (:121-125 — a targeted reset must not allocate); `filesystem_ordering_scan.rs:129` restart correctness.
- **UPDATE:** none — the scan suite's bounds are ceilings or invariants of the reader, not of the reset policy.
- **NEW:** `a_spill_resets_only_the_tiers_it_merged` — 12 spills of 64 rows (the scan fixture), one full ascending sweep (== total, pins the resume), then one more spill of a batch whose serials land in level 0, then demand only serials held by tiers ≥ 1: under the fix only `scans[0..=0]` reset, so `rows_read` for the third phase equals tier-0's row count only; today `scans.clear()` (runs.rs:120-122, called at :231 and :299) restarts every tier → strictly more rows. Assert the exact tier-0 count.

### P1-6 · Delete the discarded validation load (tree.rs:711, mirror :764/765)
- **Green:** `edit_reference` (all 9 cases; `unequal-height-join` :597 is the case that traverses tree.rs:711); `edit_single.rs:293` `an_edit_reads_and_decodes_the_base_root_once` — its fixture (9 KB) never reaches the concat path, so its "no identity is demanded twice" (:337-340) holds today and must keep holding; `edit_single.rs:365` chunked variant; `edit_localized.rs:353-357` (distinct set == path + payloads + file state); `edit_bounds.rs:80-99` canonical partitions.
- **UPDATE:** none.
- **NEW:** `an_unequal_height_join_demands_no_identity_twice` — port edit_single.rs:297-324's Counting provider onto the unequal-height-join shape (140 extents + 2.5 MB append, edit_reference.rs:260-263): assert every identity demanded at most once (edit_single.rs:337-340 pattern). Today the boundary child is loaded, dropped and re-loaded (tree.rs:711→713) → fails today. Vehicle: D27 `edit_timing_c1` `nodes_read` 9 → 7–9.

### P1-7 · Fuse `compare_replacements` into the split descent
- **Green:** `edit_noop.rs:88` `an_equal_replacement_preserves_the_base_root` (Equal verdict → base root); `edit_noop.rs:71` (empty stream: `counts.objects() == 1` :84); `edit_noop.rs:106` several equal replacements; `edit_noop.rs:152` long-prefix mismatch (compare stops early, `counts.objects() > 1` :177-180 — compare + replay both charged); `edit_timing.rs:109` `an_equal_replacement_reports_the_comparison_scope` (the `edit.compare` scope :120-123, no `content.chunk` :124-127); `edit_reference` (`untouched-sibling`, `batch-normalized`).
- **UPDATE (conditional):** `edit_timing.rs:120-123` pins the `edit.compare` scope name. If the fusion absorbs the comparison into the descent's scope, this is a deliberate pinned-behaviour change (scope rename) — preferred alternative: keep an `edit.compare` child scope inside the fused traversal so no test changes.
- **NEW:** `one_traversal_serves_compare_and_split` — a > 64 KiB-window replacement over a 3.3 MB chunked base (the D27 shape): count per-identity demands (edit_single Counting pattern); today each 64 KiB window re-traverses root-down (apply.rs:64-77 + compare.rs:52-63), so an interior branch identity is demanded once per window → count > 1; assert == 1. All `edit_noop` Equal-verdict cases must stay green in the same commit.

### P1-8 · One ordered cursor for Retain segments
- **Green:** `edit_reference` (`untouched-sibling`, `interior-multi-level` — partitions + survivors); `edit_localized.rs:529` append ≤ 3 pages (:540-546); `edit_localized.rs:572` `many_localized_edits_keep_each_edit_on_its_own_path` (worst per-edit pages ≤ 4 :596-600); `edit_batch.rs:157` `many_small_edits_are_applied_in_one_pass`; `edit_model.rs` (model equivalence); `edit_bounds.rs:250` frontier does not grow with edit count (:318-333).
- **UPDATE:** none found (no test counts per-segment traversals).
- **NEW:** `retain_segments_share_one_ordered_read` — 16 alternating replaces over an 8 MB base (the edit_bounds.rs:282-294 stream shape): assert no retained interior mapping identity is demanded more than once across all segments (today each Retain segment traverses root-down, apply.rs:154-157 → interior branches demanded R times → fails today). Vehicle: D27 + the `edits.c1.batch` case once `measure_edits` prints `nodes_read` (§3).

### P1-9 · Gate `rightmost_payload` for pure deletions
- **Green:** `edit_single.rs:159` `complete_deletion_returns_the_empty_representation`; `edit_model.rs:207` `a_deletion_that_merges_two_slices_stays_canonical`; `edit_reference` `root-collapse` (canonical root + partition + survivors); `object_identity.rs:381` (advisory predecessors still travel — the payload is advisory, outside hashed bytes); `edit_transitions.rs` (deletion transitions' acquired-set and charge assertions :549-567).
- **UPDATE:** none.
- **NEW:** `a_pure_deletion_never_walks_for_a_predecessor` — a deletion-only stream over a multi-level base (root-collapse shape): assert via the Counting provider that no payload identity is demanded and `nodes_read` drops by the spine length. Today the O(h) stored walk runs before the `replacement_len == 0` check (apply.rs:257-271) → payloads are demanded → fails today.

### P1-10 · Carry ordering state instead of re-finding (`zero_count_serials`)
- **Green:** `filesystem_bounds.rs:462` `every_reported_owner...` — `serials_scanned == 31` EXACTLY (:528-530: the collection semantics must not change), counter-vs-backing equalities `runs_created == observed.creates` (:500), `rows_written >= observed.appends` (:506), `merges > 0` (:501 — spill cascades alone keep this true), `rows_read > 0` (:508); `filesystem_bounds.rs:334` (`serials_scanned == 3`, rows_touched bounds :430-445); `filesystem_topology.rs:539-547` (`rows_touched == 2`); `filesystem_ordering.rs:139` and `:589` (roots, cleanup-once, peak bounds).
- **UPDATE:** none. Note: `touched_serials` (reduce.rs:176-204) calls `runs.consolidate()` and walks every row — the re-finding cost. Its removal drops `runs.*` counters; no test pins their absolute values, only the equalities above (which compare product counters to what the backing itself observed — invariant under the change).
- **NEW:** `collection_does_not_consolidate_runs` — three spills of disjoint serial ranges followed by an update that reaches `zero_count_serials` (filesystem_bounds.rs:334 subtree-remove shape, pending forced small): assert `runs_created == spills` — today the collection consolidates, adding one `copy_run` (runs.rs:418-424 reached via :379-427) → `spills + 1` → fails today. Keep `serials_scanned` semantics (§2 P1-10 Green) asserted in the same test.

### P1-11 · Fix the `SortedWork.pages_read` counter (batched decodes never increment it)
- **Green:** the only exact-equality assertion on `pages_read` is `filesystem_attributes.rs:547` (`== 0`, a zero-state — unaffected); all others are ceilings (`filesystem_sorted.rs:55` `<= 1` — no base, unaffected; `filesystem_bounds.rs:210/215` `<= depth+2` / `<= 2` — bounds on the true read set, they hold once the undercount is fixed) or monotonic (`filesystem_attributes.rs:558,596` — rises only help).
- **CORRECT-able (must rise, no test changes):** the Phase 0 anchor values — D1–D6 rows (`c1.directory-update` dir pages 1/0/1, `c1.subtree-remove` 1/1/0 etc.), D7 (`fs.c1`: directories pages_read 1, inodes 1), D25/D26 (`directories.pages_read 2`, `inodes.pages_read 1`). Every `pages_read` in p0-3-counter-baseline.md §2–§5 is a lower bound until P1-11 lands; re-baseline the receipt set in P1-11's own commit (append-only: new collection, not an edit).
- **NEW:** `batched_child_decodes_are_counted_as_pages_read` — a fixture whose changed-path merge decodes ≥ 2 children through `batch_children` (merge.rs:244-263; the 4,000-entry rename shape): compute the true page count by decoding the tree, assert `work.pages_read` covers them (today the batched decodes are invisible — page.rs:40-41 documents "including batched ones" but only `read()` at :185-186 increments → fails today).

### P1-12 · `push()` running total (sorted/merge.rs:71-76)
- **Green:** `filesystem_codec.rs:325-375` (ceiling quotient + size-vs-partition refusal math); `filesystem_sorted.rs:104-155` (canonical partitions at 1, 49, 50, 51, 99, 100, 101, 740, 741, 742, 1500 — the split-at-ceiling shapes a wrong running total would misplace); `edit_bounds.rs:80-99` (64..128 occupancy); `filesystem_reference` (sealed shapes).
- **UPDATE:** none.
- **NEW:** **none possible** — see §5 gap: the optimization (O(k²)→O(k) in-memory, k ≤ 740) has no counter and cannot be observed without a test-only hook, which `core/AGENTS.md` forbids in `src/`. The partition suites above catch every *wrong* result; the performance claim is verified by review only.

### P1-13 · Merge fanout 4 / size-tiered policy (references/merge.rs:6-9)
- **Green:** `filesystem_ordering.rs:139` (spilled == unspilled root identity — the threshold parity test #176 names); `filesystem_ordering.rs:272` `tier_carries_keep_the_newest_row_and_accumulate_counts` (`merges >= 1` :298, `peak_level >= 1` :299, newest-wins :301-304, counter-vs-backing :310-319); `filesystem_ordering.rs:589` (`peak_live_runs >= 2` :653-656, releases == 1, roots); `filesystem_bounds.rs:678` `merge_inputs_and_output_are_covered_by_the_declared_ceiling` (`peak_run_bytes <= ordering_bytes` :730-733 — more live tiers must still fit the declared ceiling; the ≤ 32-tier cap must hold); `filesystem_ordering_scan.rs:61` (all three assertions: with fanout-4, 12 batches leave `live_runs() >= 2` — 12 = 3 fours, no quaternary collapse — and the warmup bound, one-pass sweep and zero-allocation wave are reader invariants, not tier-shape invariants).
- **UPDATE:** none required — **no existing test asserts cascade arithmetic** (`runs == spills + merges + 1` appears only in receipts: D26 spilled 3,968 / runs 124 / merges 61). The prose at filesystem_ordering_scan.rs:66-69 ("like a binary counter") becomes stale — update the comment, not the assertions. `filesystem_bounds.rs:219`'s wave pin is unaffected.
- **NEW:** `merge_participations_follow_the_fanout` — 16 spills of 1 row each, disjoint serials: assert `work.merges == 4` (16/4 level-1 merges, no level-2 merge) — today the binary cascade performs 15 → fails today. Also assert `work.peak_level` stays within the 32-tier cap.

### P1-14 · Whole-file edit: one pre-sized canonical buffer (~3n → ~n)
- **Green:** `edit_transitions.rs:772` `a_whole_file_result_reports_no_counters_and_that_is_recorded` (whole-file route publishes `EditCounters::default()` :787-797 — P1-14 must not add counter charges to this route); `edit_single.rs:159`; `edit_model.rs:179` `conversions_match_the_model_and_their_fresh_construction`; `edit_reference` (`root-collapse`, `batch-normalized` cross the cutoff); `object_identity.rs:99/132/246/317` (envelope bytes, ceilings); `edit_noop`.
- **UPDATE:** none (bytes identical by design — `edit_model` and the oracle hold).
- **NEW:** `whole_file_edit_peak_allocation_is_linear` — a 512 KiB whole-file edit (large→small at cutoff 131,072) under the counting global allocator (the filesystem_ordering_scan.rs:22-46 precedent, summing `layout.size()` instead of calls): assert total allocated for the edit ≤ ~2× final length + 64 KiB; today out + value + canonical ≈ 3n (apply.rs:143-150 + content.rs:98-102) → fails today. Emitted root must equal `construct_bytes` of the same final bytes (model oracle).

### P1-15 · Hybrid binary-search-on-restart for ordering lookups
- **Green:** `filesystem_ordering_scan.rs:61` — the pinned one-pass ascending sweep `sweep_reads == total` EXACTLY (:110-111: **pure** binary search would make this ≫ total; the hybrid keeps the sequential resume — this is the guard the issue names), the zero-allocation wave incl. restarting requests (:116-125 — `read_at` into a reused buffer must not allocate), warmup bound (:96-99); `filesystem_ordering_scan.rs:129` restart correctness; `filesystem_ordering.rs:811` (every order agrees with the newest-first truth); `filesystem_ordering.rs:721` (spilled/unspilled root).
- **UPDATE:** none.
- **NEW:** `a_restarting_lookup_reads_logarithmically_not_positionally` — one tier of 4,096 rows; demand the last serial (advance the cursor), then serial 1 (restart): assert `rows_read` for the restart ≤ 13 (log₂ 4,096 + 1); today the restart rescans from offset 0 → ≈ 4,096 → fails today. Same test re-asserts the ascending sweep count == total (pins that the resume path was not replaced).

### P1-16 · Pending-ceiling dial decision (document or re-default)
- **Green:** `filesystem_ordering.rs:139` (threshold changes only where the rows live); `filesystem_bounds.rs:678` (peak covered by the declared ceiling); `filesystem_ordering.rs:777-779` asserts only `default > 200`, not the 4,096 value (reduce.rs:25) — a re-default does not break it.
- **UPDATE:** none if documented-only. If the default is re-defaulted: the `order.default` anchor (D25) flips to spill-free-by-design and needs a fresh collection; the round-4 grid receipts (forced 64) remain the spill-path anchor unchanged.
- **NEW:** `a_high_pending_ceiling_runs_spill_free_within_the_byte_ceiling` — an update touching N serials (e.g. 8,000) with the ceiling raised per the documented dial: assert `rows_spilled == 0`, `runs_created == 0`, `peak_pending == N`, and pending bytes + peak run bytes ≤ `ordering_bytes` (the ×2 charge of runs.rs:136/39 — 64 MiB ⇒ ≤ ~349k rows). This is the "one high-ceiling test" #178 asks for.

## 3. Verification-vs-test strategy

**Verifiable by deterministic in-test counters (list: counter → assertion):**

| Item | Counter (source) | In-test assertion |
| --- | --- | --- |
| P1-1 | `ObjectWork.read_waves` + `CountingProvider.waves()/peak_wave()` (support/filesystem.rs:601-660) | waves < pages on a wide-branch fixture; `peak_wave() > 32` (new constant) |
| P1-3 | `SortedWork.read_waves` (page.rs:185-186) | `read_waves < pages_read` on a neighbour-merge fixture |
| P1-4 | `ValidationWork` via `result.counters.validation` (already test-visible, filesystem_bounds.rs:570-582) | `read_waves < inode_demands`; `entries_examined == limit` stays exact |
| P1-5 | `runs.rows_read` via `store.work()` (runs.rs) | exact rows for the reset pattern (§2 P1-5 NEW); sweep == total stays exact |
| P1-6/7/8/9 | edit `nodes_read` (`EditCounters`, tree.rs:39/154) + per-id demand counts (Counting provider) | per-id demand count == 1 where the item removes a re-read; `nodes_read` drops by the removed loads |
| P1-10 | `runs.rows_read/rows_written/runs_created` + `RecordingBacking` | `runs_created == spills` (no collection consolidate) |
| P1-11 | `SortedWork.pages_read` | covers batched decodes exactly (page-inventory equality) |
| P1-13 | `runs.merges`, `runs.rows_written` | `merges == 4` for 16 single-row spills; root identity vs unspilled arm |
| P1-14 | allocation bytes (counting global allocator, scan-test precedent) | allocated ≤ ~2n for a whole-file edit (today ~3n) |
| P1-15 | `runs.rows_read` | restart ≤ log₂(rows)+1; sweep == total unchanged |
| P1-16 | `ReferenceWork` (`rows_spilled`, `peak_pending`) + `peak_run_bytes` | spill-free under the raised dial, inside the byte ceiling |

**Needs vehicle receipts (frozen Phase 0 workloads, CONTRACT.md §5):** P1-1's
`read_waves` drop on D2–D5 (c1 rows: waves 4 → fewer); P1-5's per-doubling
`rows_read` ratio on the `order.forced64` grid shape (D26: 59,007 → toward the
~×2.1–2.4 write-term floor — the n^1.9 claim is a multi-size ratio, only a
receipt shows it); P1-13's per-doubling `rows_written` (toward ×2); P1-6/7/8/9
against D27 (`edit_timing_c1` `nodes_read` = 9, printed at
edit_timing_c1.rs:153) — **not** `measure_edits`, which does not print it;
P1-11's re-baselined `pages_read` on D1–D9, D25, D26; P1-14's byte-identity is
in-test, its wall/peak story stays diagnostic-grade.

**Needs new counters/vehicles — nominated prerequisite commits (smallest
honest changes, each its own single-variable commit with LOC delta):**
1. **`ValidationWork` in a vehicle** (P0-3 gap, blocks P1-4's receipt):
   `filesystem_timing_c1` already holds `result.counters.validation` but prints
   no validation line (its output section prints objects/directories/inodes/
   references only). Add one output line — an `examples/` change, no `src/`.
2. **`nodes_read` in `measure_edits`** (P0-3 gap, blocks per-case edit
   receipts): the counter exists on the constructed result; print it in the c1
   and pipeline modes — `examples/` change. Until then P1-6/7/8/9 verify against
   D27's single shape plus the in-test assertions.
3. **Connection-open counter for P1-2** (product telemetry, legitimate `src/`
   per core/AGENTS.md): add an opens counter to the store read path
   (`cas/store.rs` open sites :211/:247), expose it in `StoreReadCounters`,
   print it in the c2/pipeline vehicles. Counter-only commit; P1-2 then has
   both an in-test and a receipt observable.
4. *(Optional, P1-15 receipt-grade)* a restart-vs-resume split counter on run
   scans — #176 §6 records that no counter separates them today; in-test exact
   assertions already discriminate, so this is optional.

## 4. The eight-check discipline (per landing commit)

From `core/AGENTS.md` "Checks and completion" + repository `AGENTS.md` §4 — no
CI, no aggregate gate; verify the tree actually changed:

1. `cargo +1.85.1 fmt --check --manifest-path core/Cargo.toml --locked`
2. `cargo +1.85.1 clippy --manifest-path core/Cargo.toml --locked` (warnings denied)
3. `cargo +1.85.1 test --manifest-path core/Cargo.toml --locked` (full workspace)
4. `python3 core/tools/check_product_boundary.py` (999/200-line caps, markers)
5. `python3 -m unittest discover -s core/tools -p 'test_*.py'` (guard self-tests)
6. **Production LOC** `before -> after (delta ±N)` in the commit message, first
   parent vs committed tree, reproducible method (test/docs-only commits still
   report the unchanged total)
7. **Architecture doc updated in the same commit** where an algorithm or named
   bound moves (P1-1: `BATCH_CHILDREN`; P1-5: reset policy; P1-13: tier policy;
   P1-16: the dial) — advance the doc's source pin, never silently re-date
8. **The item's own receipt** — single-variable, identities pinned, one sample
   per case per arm, from the frozen Phase 0 workloads, append-only

Suites each commit must *specifically* run beyond the workspace suite — the
sealed-oracle 34 (§1) for **every** item, plus: P1-1 → `filesystem_bounds`,
`filesystem_sorted`, `filesystem_read`, `filesystem_attributes`; P1-2 → storage
`visibility`, `cas_roundtrip`, `connection_profile`, `provider_errors`,
`core_pipeline`, `edit_pipeline`, `filesystem_pipeline`; P1-3 →
`filesystem_sorted`, `filesystem_topology`; P1-4 → `filesystem_bounds`,
`filesystem_topology`, `filesystem_timing`, `filesystem_attributes`; P1-5,
P1-13, P1-15, P1-16 → `filesystem_ordering`, `filesystem_ordering_scan`;
P1-6..P1-9 → `edit_reference`, `edit_noop`, `edit_single`, `edit_localized`,
`edit_bounds`, `edit_transitions`, `edit_model`, `edit_batch`, `file_read`,
`edit_timing`; P1-10 → `filesystem_ordering`, `filesystem_bounds`,
`filesystem_topology`, `filesystem_updates`; P1-11 → `filesystem_sorted`,
`filesystem_bounds`, `filesystem_attributes` (+ re-baseline receipts); P1-12 →
`filesystem_codec`, `filesystem_sorted`, `edit_bounds`; P1-14 →
`edit_transitions`, `edit_single`, `edit_model`, `object_identity`,
`edit_reference`.

## 5. Gaps and risks

1. **P1-12 has no regression test for the optimization itself** — only for
   wrong results (partition suites). An accidental O(k²) reintroduction is
   invisible to every test; no counter exists and test-only hooks are forbidden
   in `src/`. Verified by review; say so in the commit message.
2. **P1-2 has no observable until prerequisite #3 lands** — connection opens
   are countable nowhere today (cas/store.rs:211/:247). Sequence the counter
   commit first.
3. **P1-10 has no frozen vehicle exposing `runs.*` on a release-heavy
   workload** — `c1.subtree-remove` (D5) prints references/removals but no
   `runs.*`; the order probes print `runs.*` but are rename-shaped. The in-test
   exact assertion (§2 P1-10 NEW) substitutes; nominate a vehicle only if a
   receipt is required.
4. **D27 is the only `nodes_read` vehicle and exercises one edit shape** (a
   single 40 KB overwrite mid-file, edit_timing_c1.rs:89-114): P1-9 (deletions)
   and P1-8 (multi-segment) are unrepresented in any `nodes_read`-printing
   vehicle — prerequisite #2 closes this.
5. **Tight pins that could force pinned-behaviour changes**: (a)
   `filesystem_attributes.rs:556-559` "a second stat charges its own read" —
   P1-4's memo must stay validation-scoped or this pin breaks deliberately;
   (b) `edit_timing.rs:120-123` pins the `edit.compare` scope name — P1-7 should
   keep the child scope inside the fused traversal; (c) `filesystem_bounds.rs:219`
   `peak_wave() <= 32` goes stale (not red) when P1-1 raises the width — update
   it deliberately; (d) `edit_transitions.rs:787-797` pins zero counters on the
   whole-file route — P1-14 must not charge it. None of these makes an item
   impossible; each names the exact test to touch if the implementation crosses it.
6. **Sequencing risk**: P1-11 must land and re-baseline before P1-1/P1-3/P1-4
   receipts compare `pages_read`/`read_waves` anchors — every Phase 0
   `pages_read` is an undercount until then (#178's own "do early" note).
7. **P1-5/P1-13's anchor exists only when the ceiling is forced** (order.default
   D25: 0 spills, 0 runs — P0-3 §5): the cascade is invisible at defaults, so
   the grid receipts (forced 64) and the new in-test fixtures carry the whole
   verification; at default settings a regression of P1-5/P1-13 is
   unobservable in behaviour and only the counter assertions (§3) catch it.
8. **`the_pooled_lane_supplies_the_owners_ceiling_at_every_read_site`
   (visibility.rs:497-527) is a source-text scan** — it pins region markers in
   `owner.rs`; P1-2 restructuring that file must keep the marker names or
   update the scan in the same commit (a deliberate, mechanical pin change).
