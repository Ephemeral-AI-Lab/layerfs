# Phase 1 implementation plan — the edit-path items (P1-6, P1-7, P1-8, P1-9, P1-12, P1-14)

> **Status:** implementation plan, static source reading only (no builds, tests,
> measurements or product changes). Tree `625ad7b57`, clean. Items are
> [#178](https://github.com/Ephemeral-AI-Lab/layerfs/issues/178) Phase 1 boxes
> from the [complexity/round-trip register](../../../component-decoupling/complexity-and-roundtrip-research-20260917.md)
> §2 Tier 0 entries 5–8, 15 (C1 half), Tier 1 entry 17. Before anchors: the
> [P0-3 counter baseline](../phase0-baseline-20260917T221759Z/receipts/p0-3-counter-baseline.md).
> **Every item emits byte-identical persisted bytes** — no canonical root, page
> partition, profile or `sqlite_master` text changes; the sealed-oracle parity
> set stays green per commit. Paths below are under `core/crates/layerfs-content/`.

Context: `apply_edits` (`src/file/edit/apply.rs:38-120`) opens the view, runs
`compare_replacements` (`apply.rs:64-77`), then matches representation
(`src/policy.rs:120-126`: `== 0` empty, `< cutoff` whole-file, else chunked).
`nodes_read` counts in `EditObjects::load_node` (`src/file/edit/tree.rs:154`)
and in any counting provider (`read_canonical` defaults to a one-id batch,
`src/object/access.rs:54-66`) — the provider count is what `edit_timing_c1`
prints (`examples/edit_timing_c1.rs:153`). No test anywhere pins an exact
`nodes_read` value; existing read pins are `<=` bounds or identity pins.

**Two frozen-vehicle gaps (honest findings; both need prerequisite commit V1):**

1. **No memory observable for P1-14.** `measure_edits` prints emitted
   objects/bytes only, and the whole-file route deliberately reports
   `EditCounters::default()` (pinned by `tests/edit_transitions.rs:771`), so a
   product counter would break that pin. **V1 nominates** a new
   `examples/edit_memory_probe.rs`: the `small` fixture (`measure_edits.rs:196-200`,
   base 65,536, overwrite 512 at cutoff/4) under a counting `GlobalAlloc`
   reporting peak live requested bytes across `apply_edits` (BEFORE row at the
   pre-change tree, AFTER at P1-14's tree). Examples are the sanctioned
   non-product location (`core/AGENTS.md`); frozen vehicles untouched.
2. **No frozen `edits.*` fixture exercises P1-8's or P1-9's discriminating
   shapes.** The five fixtures (`measure_edits.rs:196-218`): whole→whole-file
   (`small`), chunked→chunked (`chunked`, `batch`, `large-to-small` at exactly
   `final_len == cutoff`), whole→chunked (`small-to-large`). None is a chunked
   base with a whole-file result (P1-8), and no vehicle prints `nodes_read` on
   a pure deletion (P1-9; P0-3 §4). **V1 nominates** two added
   `edit_timing_c1` fixtures — `--case delete` (3.3 MB chunked base, delete
   40,000 mid-file, result stays chunked; P1-9 positive anchor) and `--case
   shrink` (same base, delete to below cutoff; P1-8 positive anchor) — same
   printed fields, BEFORE rows at the pre-change tree. `edit_timing_c1` is
   already the "extra" vehicle outside CONTRACT §5 (collected as D27 via
   `collect.py extra`, phase0 `README.md:87`), so this is a labelled vehicle
   change, not a frozen-set mutation.

---

### P1-6 — delete the discarded validation load in `concat_inner`

- **Big-O target:** time: removes one redundant `load_node` (decode + demand +
  summary re-check) per height-mismatched join side; nodes_read −1 per fired
  branch, 0–2 per edit (#178). Memory unchanged. Persisted bytes IDENTICAL —
  the loaded node was discarded; `:713`/`:765` re-load it inside the recursion.
- **Before anchor:** `nodes_read`, printed only by **D27** (`edit_timing_c1`,
  **9**; P0-3 §4/§7). The concat path runs on every chunked-route edit:
  `edits.c1/pipeline chunked`, `batch`, `large-to-small` (D11/D13/D14/D21/D23/D24)
  and D27. Whether D27 takes a height-mismatched join is not statically
  decidable — its drop is 0–2. The guaranteed shape is `edit_reference`'s
  `unequal-height-join`, `height-growth`, `root-collapse` oracle cases.
- **Change sketch:** `src/file/edit/tree.rs:711` `objects.load_node(last, false)?;`
  (result dropped; re-loaded at `:713` `concat_inner(objects, last, right, …)`)
  and mirror `:764` `objects.load_node(first, false)?;` (re-loaded at `:765`).
  Delete both. Their only live effects are the `nodes_read` increment
  (`tree.rs:154`) and a summary cross-check (`tree.rs:163-168`) that the
  recursive re-load repeats on the same stored object.
- **Tests:** stay green — `edit_reference.rs:592` (nine cases, esp.
  `unequal-height-join`, `height-growth`, `root-collapse`, pinning root +
  partition + survivors through these branches); `edit_bounds.rs:131`
  `join_occupancy_at_every_boundary`, `:183`
  `a_join_of_two_full_pages_stays_canonical`; `edit_localized.rs:275/422/471`
  (`<=` bounds); `edit_single.rs:365` (`root_demands == 1`). NEW:
  `tree_concat_sibling_demanded_once` — replay the `unequal-height-join`
  oracle fixture (or a hand-built two-height base) through `apply_edits` with
  `support::Counted` (`tests/support/mod.rs:528-577`); assert the boundary
  sibling page identity appears **exactly once** in `counts.ids` (today twice).
- **Verification receipt:** `edit_timing_c1` (D27 shape), `nodes_read`
  9 → ≤ 9 (expected −0..2); `edits.c1.chunked` root/bytes unchanged. Work
  counters deterministic (P0-3 §6); elapsed not a gate.
- **Commit slicing:** one commit, no dependencies (`tree.rs` only;
  895 → 893 physical lines, < 999).
- **Risks / open questions:** the deleted load validated the child summary
  with a **non-root** decode context while the recursive re-load may use a
  root context (`tree.rs:668/699` pass `true`); on a corrupt tree the error
  could move or weaken (under-full non-root page passing root checks). No test
  pins corrupt-concat error identity; zero risk on valid input since summaries
  derive from just-decoded parents (`child_summaries`, `tree.rs:493-514`).

### P1-7 — fuse `compare_replacements` into the split descent (chunked route)

- **Big-O target:** time/pages: today the no-op check pays one root-down
  traversal per 64 KiB window per Replace segment (`compare.rs:52-63`; window
  `compare.rs:19`) — W×h pages — plus page demands the split descent repeats
  (`apply.rs:64-77` pre-dispatch; split loads at `tree.rs:574`). After: one
  descent; mapping pages O(h) once per edit instead of O(W·h)+O(h); payload
  demands unchanged (compared base bytes must still be read). Memory
  unchanged (bounded windows). Persisted bytes IDENTICAL.
- **Before anchor:** `nodes_read` = **9** at **D27** (`measure_edits` does not
  print it — P0-3 §4). Workloads: D27, `edits.c1/pipeline chunked`, `batch`,
  `large-to-small`. The window multiplier is pinned by `edit_noop.rs:152`
  (2 MiB base, 1 MiB replacement = 16 windows today).
- **Change sketch:** `apply.rs:62-77` — compute `representation` first; for
  `Representation::Chunked`, replace the pre-dispatch compare with a comparing
  sink carried by the split loop in `replace_chunked` (`apply.rs:234-307`):
  when the descent (`tree.rs:552-635`) loads a leaf covering part of the
  removed range, demand the covered payloads in bounded waves (the `Wave`
  shape, `src/file/mapping/read.rs:77-190`) and compare against the
  replacement source in stream order. Preserve `compare.rs:42-48` exactly:
  `len != removed_len` → `Differs`; short source → `InvalidEdit { what:
  "replacement bytes" }`; first differing byte → `Differs`. If all edits
  compared equal, return `ConstructedFile { root: view.root(), … }`,
  settle/discard all drafts, **zero emission** — same result as today.
  Whole-file/empty-result routes keep `compare_replacements` unchanged (no
  descent exists there to fuse into).
- **Tests:** stay green — `edit_noop.rs:88/106/128` (Equal verdict, whole-file
  bases, untouched route), `:152` (early stop; `counts.objects() > 1` still
  holds), `:184` (length change ≠ no-op); `edit_timing.rs:109` (`edit.compare`
  scope pin survives: whole-file base keeps the standalone compare), `:73`
  (`edit.split`/`edit.finish` still reported), `:135` (on/off equivalence);
  `edit_reference.rs:592`; `edit_bounds.rs:336` (source-end failure, error
  kind preserved). MUST CHANGE: none textually — but the chunked-route timing
  tree loses the top-level `edit.compare` child; add a new pin
  `chunked_comparison_is_reported_inside_the_split_pass`. NEW: (a)
  `chunked_equal_edit_demands_no_page_twice` — chunked base, byte-identical
  replacement, `Counted` provider: base root demanded once, result root ==
  base root, zero emitted objects, every mapping-page demand ≤ 1 (today
  compare+split duplicate path pages); (b)
  `window_fusion_drops_repeated_root_demands` — a > 64 KiB equal replacement:
  mapping root demanded exactly once for the whole edit (today once per
  window).
- **Verification receipt:** `edit_timing_c1` (D27): `nodes_read` 9 → strictly
  lower (compare's mapping-page demands disappear; payloads remain);
  `edited_root` unchanged for the Differs fixture; `edits.c1.chunked` root and
  canonical bytes bit-identical. Elapsed diagnostic only.
- **Commit slicing:** one commit. Depends on P1-6/P1-9 only for attribution
  hygiene (each `nodes_read` delta lands on its own receipt); logically
  independent of P1-8.
- **Risks / open questions:** **Equal-verdict preservation is the item**: the
  fused sink must reproduce `compare_replacements`' procedure bit-for-bit
  (precheck, bounded ordered comparison, `Differs` on first difference,
  `Equal` only if every segment compares equal); `edit_noop` must not see one
  changed verdict. Error ordering: compare errors today surface before split
  work; fused, a bad source fails after the descent started (still fails
  wholesale, no root emitted — pinned by `edit_bounds.rs:336`). Corrupt-tree
  coverage errors move from `LengthMismatch` (`compare.rs:64-69`) to split
  summary errors — no test pins this. Open: sink placement (`tree.rs` vs a
  `Plan`-driven visitor in `compare.rs`) — `tree.rs` (895 lines) stays < 999.

### P1-8 — one ordered cursor for Retain segments in `assemble_final`

- **Big-O target:** time/pages: R retain segments × (root-down traversal ≈ h
  path pages) → ~h pages once for the union path (segments are in ascending
  base order by `Plan`, `src/file/edit/input.rs:298-390`; shared ancestors
  demanded once; payloads deduped per wave by `Wave::push`,
  `mapping/read.rs:102-118`). Provider demands O(R·h) → O(h + shared). Memory
  unchanged (per-wave bounds `mapping/read.rs:24-40`). Persisted bytes
  IDENTICAL.
- **Before anchor:** no frozen `edits.*` row exercises this route with a
  chunked base (gap 2) — `edits.c1.small` runs `assemble_final` over a
  **whole-file** base where `view.read_range` is a payload slice
  (`src/file/view.rs:102-111`), so its counters cannot move. Negative anchors
  that must stay bit-identical: D27 (chunked route, untouched) and
  `edits.c1.small` (root + 65,559 canonical bytes). Positive anchor: V1's
  `--case shrink` BEFORE row. The route is unit-pinned today by
  `edit_transitions.rs:593` (large→small: chunked base, whole-file result) —
  all upper bounds, so a drop keeps them green.
- **Change sketch:** `apply.rs:152-165` — each `Segment::Retain` calls
  `view.read_range(reader, base.0..base.1, &mut out, scope)`
  (`apply.rs:154-157`), and every chunked call re-runs `mapping::read_range` →
  `traverse` from the root (`mapping/read.rs:199-325`). Replace with an
  ordered cursor built once per `assemble_inner` when `view.file_state()` is
  `Some(state)` (whole-file bases keep the slice path): a `RetainCursor`
  holding the `FileState`, the per-level frontier (the `Frontier` shape,
  `mapping/read.rs:68-75`) **retained across segments** — entries after the
  active range's end stay in the frontier instead of being dropped by
  `traverse`'s early `finished` break (`mapping/read.rs:299-320`) — plus one
  `Wave`. `read_segment(range, sink, scope)` asserts ascending non-overlapping
  ranges, validates `range.end <= logical_len` per segment
  (`mapping/read.rs:206-212`), and resumes the level sweep. `PlanReader`
  (`apply.rs:377-465`) is the structural precedent for a plan-driven adapter.
  New home: `src/file/edit/segments.rs` or `mapping/read.rs` (352 lines).
- **Tests:** stay green — `edit_transitions.rs:593` (all four case families,
  incl. `to-empty`'s exact `acquired.len() == 1`), `:771` (whole-file result
  still reports `EditCounters::default()`), `edit_single.rs:263`,
  `edit_model.rs:179` (conversions match model + fresh construction),
  `edit_reference.rs:592`, `object_identity.rs`, `fixture_seal.rs:55`,
  `filesystem_reference.rs:302/309`. MUST CHANGE: none. NEW: (a)
  `retained_segments_share_one_descent` — chunked base, two retain segments
  separated by a replacement (the large→small shape): with `Counted`, the
  mapping-root identity demanded **exactly once** for the whole assembly
  (today once per segment); (b) `straddling_payload_demanded_once_across_segments`
  — a payload straddling two retain segments is demanded once (today twice).
- **Verification receipt:** V1 `edit_timing_c1 --case shrink`, `nodes_read`
  (provider-level, route-independent): BEFORE → AFTER, strictly lower by
  ≈ (R−1)·h path pages; `edited_root` and bytes unchanged. Negative: D27 = 9
  unchanged; `edits.c1.small` root/bytes unchanged.
- **Commit slicing:** one commit; **lands independently of P1-7** (P1-7 =
  chunked route + compare dispatch; P1-8 = the `WholeFile` arm's assembly
  loop). Both edit `apply_edits`'s body — land sequentially to avoid textual
  conflicts, P1-7 first.
- **Risks / open questions:** **emission order is not at risk** — this route
  emits exactly one object and the cursor only reorders *provider demands*;
  canonical bytes are unchanged by construction. Demand order changes (one
  ascending sweep vs per-segment descents); no test pins demand order on this
  route (`edit_localized`'s ordered payload-index assertions are chunked-route
  tests, unaffected). Real design risk: the frontier must keep not-yet-visited
  siblings across the segment gap — naively copying `traverse`'s early break
  loses the path to later ranges; the cursor must filter lazily against the
  active range, never truncate. Per-segment coverage accounting
  (`payload_bytes_read != requested`, `mapping/read.rs:220-222`) must be
  preserved per segment.

### P1-9 — gate `rightmost_payload` for pure deletions

- **Big-O target:** time/pages: for each edit with `replacement_len == 0`, the
  O(h) stored rightmost walk (`apply.rs:328-353`, one `load_node` per level)
  disappears — nodes_read −(path length) per pure-deletion edit. Memory
  unchanged. Persisted bytes IDENTICAL — the predecessor is an advisory
  physical hint consumed only by `builder.push_chunk(chunk, predecessor, …)`
  in the replacement scan (`apply.rs:282-286`), which never runs when
  `replacement_len == 0` (`apply.rs:271-273`), and it is outside hashed bytes.
- **Before anchor:** `nodes_read` = **9** at D27 — an overwrite
  (`replacement_len = 40,000`), so D27 is the **negative control: must stay
  9**. Frozen positive workloads: `edits.c1/pipeline large-to-small` (pure
  deletion) and `batch` (one delete of three edits) — none prints
  `nodes_read`; the positive anchor is V1's `--case delete` BEFORE row.
- **Change sketch:** `apply.rs:257-271` — the walk runs at `:257-262`
  (`match left { Some(left) => … rightmost_payload(&mut objects, left) … }`)
  before the source-length check `:266-270` and the `replacement_len == 0`
  branch `:271-273`. Gate it: compute the predecessor only when
  `replacement_len > 0` (fold the walk into the existing `else` arm of
  `let middle = if replacement_len == 0 { None } else { … }`). Keep the
  replacement-length error check (`:266-270`) exactly where it is, so the
  failing-insert case (`edit_timing.rs:188`, `edit_bounds.rs:336`) performs
  the same work before failing.
- **Tests:** stay green — `edit_single.rs:159`
  `complete_deletion_returns_the_empty_representation`; `edit_transitions.rs:593`
  large→small and to-empty (incl. `to-empty`'s exact `acquired.len() == 1`,
  which already proves no payload walk there — the empty representation
  returns before assembly); `edit_batch.rs:133/157`; `edit_model.rs:207`
  (deletion-merge stays canonical); `edit_reference.rs:592`;
  `object_identity.rs`; `fixture_seal.rs:55`. MUST CHANGE: none. NEW: (a)
  `a_pure_deletion_never_walks_the_rightmost_path` — chunked base, one
  mid-file `Edit::delete`, `Counted` provider: the demanded multiset equals
  the split-only demands (no extra right-boundary leaf demands); (b)
  `an_overwrite_still_demands_the_predecessor_path` — negative pin against
  over-gating.
- **Verification receipt:** V1 `edit_timing_c1 --case delete`, `nodes_read`:
  BEFORE → AFTER, strictly lower by the rightmost path length; `edited_root`
  unchanged. Negative: D27 stays 9; `edits.c1.large-to-small` root/bytes
  unchanged.
- **Commit slicing:** one commit; no dependencies (`apply.rs` only;
  independent of P1-6/P1-7 — landing before P1-7 keeps each receipt's delta
  attributable).
- **Risks / open questions:** the walk's only side effect beyond the hint is
  summary validation of the left subtree's right boundary (`tree.rs:163-168`);
  gating removes that redundant check — the same nodes were already validated
  by the split that produced `left`, a no-op on valid input. The gate must key
  on `replacement_len` (declared), not `removed_len`, so same-length overwrites
  and pure inserts keep the hint (`edit_localized.rs:529` append case).

### P1-12 — `push()` running total in the sorted merge

- **Big-O target:** time: the width sum recomputed after every append
  (`src/filesystem/sorted/merge.rs:71-76`) is O(k) per push → O(k²) per page
  fill; with a running total O(1) per push → O(k). k ≤ **740** =
  `MAXIMUM_DIRECTORY_LEAF_ROWS` = `(8192 − 44)/11` (`src/filesystem/limits.rs:104`,
  `sorted/format.rs:22`); branches cap at `DEFAULT_PAGE_ITEMS = 234`
  (`format.rs:48`), inode leaves at 100. Memory: +1 `usize` per live `Page`.
  **Persisted bytes IDENTICAL** — `fits` sees the same `size` at the same
  moments, so split points (`nearest_half`, `merge.rs:85`) are unchanged.
- **Before anchor:** no counter measures CPU work; the frozen workloads
  exercising `push` are the sorted-engine rows — `c1.directory-update`,
  `c1.hardlink-move`, `c1.subtree-remove` (D2/D4/D5), `fs.c1.directory-update`
  (D7), `order.default`/`order.forced64` (D25/D26). Honest receipt: **every
  work counter and the root identity are unchanged; the improvement is not
  counter-observable in the frozen set** — elapsed is diagnostic-only; parity
  is the gate.
- **Change sketch:** `merge.rs:70-76` recomputes
  `let size = EMPTY_PAGE_BYTES + page.entries.iter().map(|e| F::width(&e.key, page.level)).sum();`
  after every `append_entry`. Add `widths: usize` to `Page`
  (`sorted/page.rs:85-93`), maintained at the mutation sites: `append_entry`
  (`page.rs:520-542`, `+= F::width`) — the single funnel every entry passes
  through (`push`, `edit`, `merge`, `page_from_wire` all call it); the split
  drain in `push` (`merge.rs:87` `page.entries.drain(split..)` — subtract
  drained widths once, O(k) per split; `right` accrues via `append_entry`);
  `Engine::page(level)` (`page.rs:164-175`, starts 0). `persist_entry`
  (`page.rs:376-393`) never changes a key, so widths are stable across
  persistence. Reuse the field in `Engine::node` (`page.rs:293-298`) to drop
  its own O(k) sum. The `nearest_half` widths vec (`merge.rs:80-84`) stays —
  once per split.
- **Tests:** stay green — `filesystem_reference.rs:302/309` (sealed roots and
  page partitions through the merge engine — the strongest pins);
  `filesystem_sorted.rs`, `filesystem_ordering.rs`, `filesystem_topology.rs`,
  `filesystem_updates.rs`, `fixture_seal.rs:55`, `object_identity.rs`.
  MUST CHANGE: none. NEW: `page_width_running_total_matches_recomputed_sum` —
  over the sorted fixtures, after each of many pushes (both levels):
  `EMPTY_PAGE_BYTES + page.widths == EMPTY_PAGE_BYTES + Σ F::width(...)`, and
  the split decision identical to a from-scratch recomputation.
- **Verification receipt:** `measure_filesystem --mode c1 --case
  directory-update --entries 200` (D7) and `filesystem_timing_c1 --case
  directory-update` (D2): all `SortedWork` counters (pages_read/created/reused,
  `peak_scratch_bytes`) and the root **bit-identical**; elapsed diagnostic-only,
  expected lower. Direction: no counter moves — stated plainly.
- **Commit slicing:** one commit; fully independent of the edit-path items
  (different subsystem); may land in parallel.
- **Risks / open questions:** the new `Page` field must not perturb scratch
  accounting — charges are explicit at `lease.grow` (`page.rs:525-539`), so
  `peak_scratch_bytes` should be untouched; the receipt must confirm it
  bit-identical. Branch-level `Entry.bytes` already means subtree bytes, not
  row width (`page.rs:456` vs `:477`), so the total must be its own field —
  do not overload `bytes`. Verify no other direct `page.entries` mutation
  exists (this reading found only the `push` drain; re-grep in review).

### P1-14 — assemble the whole-file edit into one pre-sized canonical buffer

- **Big-O target:** memory: peak live ~3n → ~n + O(1), n = `final_len`. Today:
  `out` (n, `apply.rs:143-150`) alive while `encode_whole_file` builds `value`
  (n+10, `src/file/content.rs:98-102`) and `encode_bytes_object` wraps it
  (n+13, `src/object/codec.rs:70-75`) ≈ 3n+23. After: one buffer of
  `canonical_len(10+n) = n+23` (`codec.rs:26-44`) holding envelope + value
  header + payload, assembled in place. Time: saves two n-byte copies.
  **Persisted bytes IDENTICAL** — same envelope (`codec.rs:50-67`), same
  `WHOLE_MAGIC`/version header (`content.rs:24-26`), same payload order, same
  object id.
- **Before anchor:** **no memory observable is printed by any frozen vehicle**
  (gap 1), and a product counter is the wrong instrument because
  `edit_transitions.rs:771` pins `EditCounters::default()` on this route.
  **Nomination (prerequisite V1):** `examples/edit_memory_probe.rs` (the
  `small` fixture) under a counting `GlobalAlloc` reporting peak live
  requested bytes across `apply_edits`; BEFORE row at the pre-change tree
  (expected ≈ 3n ≈ 197 KB live), AFTER at P1-14's tree (≈ n ≈ 66 KB).
  Secondary parity anchors: `edits.c1.small` (root + 65,559 canonical bytes,
  D10) and `edits.pipeline.small` (byte-for-byte readback, D20).
- **Change sketch:** `apply.rs:87-110` (WholeFile arm) + a new helper beside
  `encode_whole_file` (`content.rs:82-111`), e.g.
  `begin_whole_file_object(capacities, payload_len) -> ContentResult<Vec<u8>>`:
  both capacity checks with today's `what` strings and limits
  (`construction.whole_file`, `content.rs:91-97`;
  `construction.whole_file_canonical`, `:103-109` — defensive on this route
  since `final_len < threshold`; keep for error-surface parity), then
  `try_reserve_exact(canonical_len(WHOLE_VALUE_HEADER + payload_len))` and
  write the 13-byte envelope + 10-byte value header. In `assemble_inner`
  (`apply.rs:135-173`): drop the local `out` allocation; the pre-sized
  canonical buffer **is** the sink — `Plan`/`view.read_range`/
  `append_replacement` append into it unchanged (the exact reserve means
  `extend_from_slice` never reallocates); the final length check
  (`apply.rs:166-171`) becomes `canonical.len() == HEADER_LEN +
  VALUE_LEN_BYTES + WHOLE_VALUE_HEADER + final_len`. `FinalizedObject::new(
  ObjectRole::WholeFile, canonical)` (`apply.rs:98`) **moves** the buffer — no
  clone; `with_predecessors` (`apply.rs:99-101`) unchanged. `encode_whole_file`
  stays for `construct_bytes` (`content.rs:172-175`) and the C2 vehicle lane
  (`measure_edits.rs:435/442`) — complete construction is out of scope.
- **Tests:** stay green — `edit_reference.rs:592`; `object_identity.rs:99/132/225`
  (frozen fixture identities; sink == single-allocation encoder);
  `edit_transitions.rs:102` (whole-file results compared by root against
  fresh construction — the exact-bytes oracle); `edit_single.rs:61`
  (`expect_final` family); `edit_model.rs:122/179`; `edit_noop.rs`;
  `fixture_seal.rs:55`; `filesystem_reference.rs` (untouched route).
  MUST CHANGE: none. NEW: `whole_file_edit_emits_the_reference_bytes` — the
  `small` fixture edit's emitted object equals
  `encode_whole_file(capacities, &expected_final_bytes)` byte-for-byte
  (independent construction), plus the memory-probe receipt as the measured
  gate.
- **Verification receipt:** V1 `edit_memory_probe`, `peak_live_bytes` during
  `file.edit`: BEFORE ≈ 3n → AFTER ≈ n (n = 65,536; deterministic allocation
  accounting for fixed input). Parity: `edits.c1.small` root + emitted
  canonical bytes bit-identical; `edits.pipeline.small` readback
  byte-for-byte. Elapsed diagnostic only.
- **Commit slicing:** one commit **after** V1 (the probe and its BEFORE row
  must exist first); logically independent of P1-8, but both rewrite
  `assemble_inner` — land P1-8 first, P1-14 as a pure buffer-swap on top.
- **Risks / open questions:** **aliasing/ownership** — the buffer is written
  only through `&mut dyn Write` sinks and never borrowed while a source slice
  is read: a whole-file base read borrows `view.canonical` (a different
  buffer, `view.rs:103-110`); a chunked base writes from freshly demanded
  payloads; `append_replacement` copies through a 16 KiB stack window
  (`apply.rs:186-201`). No simultaneous alias exists; the new invariant is
  "the exact reserve precedes all appends", enforced by `try_reserve_exact`
  failing closed with today's `BoundedCapacityExceeded` path.
  `FinalizedObject::new` re-validates framing (`src/object/output.rs:100`), so
  a mis-sized assembly fails loudly. The timing tree loses the separate
  `content.encode` child on this route (encode fused into assembly) — no test
  pins it there; note in the receipt. Keep `logical_len` semantics
  (`apply.rs:107`).

---

## DEPENDENCY ORDER

1. **V1 (prerequisite, vehicle-only, production LOC delta 0):**
   `examples/edit_memory_probe.rs` + the two `edit_timing_c1` fixtures
   (`--case delete`, `--case shrink`); collect BEFORE rows for all three
   (memory peak; two `nodes_read` baselines) plus unchanged D27. Nothing else
   rides in this commit.
2. **P1-6** (`tree.rs` only; smallest, zero-dependency).
3. **P1-9** (`apply.rs` gate; negative control D27, positive V1 `--case delete`).
4. **P1-7** (compare fusion; after 2–3 so each `nodes_read` delta is
   attributable to one receipt; re-verify `edit_noop` verdict parity).
5. **P1-8** (ordered cursor; independent of 4 but sequenced after it to avoid
   `apply_edits` textual conflicts; positive anchor V1 `--case shrink`).
6. **P1-14** (pre-sized canonical buffer; after 5 as a pure sink/ownership
   change on top of the cursor; measured by V1 `edit_memory_probe`).
7. **P1-12** (sorted merge; fully independent — any position, may run in
   parallel with the edit-path items).

Every commit: single-variable, its own verification receipt from the frozen
set (work counters deterministic, elapsed diagnostic-only), the parity set
green (`edit_reference`, `edit_noop`, `edit_single`, `edit_batch`,
`edit_localized`, `edit_transitions`, `object_identity`, `fixture_seal`,
`filesystem_reference`), `cargo +1.85.1 test/clippy/fmt --manifest-path
core/Cargo.toml --locked` + `python3 core/tools/check_product_boundary.py`,
and a production LOC before/after line in the commit message.
