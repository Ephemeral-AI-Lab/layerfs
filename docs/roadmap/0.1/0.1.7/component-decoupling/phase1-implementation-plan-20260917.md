# Phase 1 implementation plan (2026-09-17): algorithm / call-pattern, #178

> **Status:** The implementation plan for Phase 1 of
> [#178](https://github.com/Ephemeral-AI-Lab/layerfs/issues/178), built from four
> read-only planning agents over the current tree (`625ad7b57`) and anchored on
> the Phase 0 baseline receipts. Detail lives in the four reports under
> [`../evidence/phase1-planning-20260917T230000Z/`](../evidence/phase1-planning-20260917T230000Z/);
> this document is the consolidated checklist, the Big-O targets, the unified
> landing order and the rulings. The executable assignment is
> [`phase1-terminal-handoff-20260917.md`](phase1-terminal-handoff-20260917.md).

## 0. Phase 0 rulings (owner-overridable on #178; defaults the handoff follows)

1. **P2-1/O1 is deprioritized.** P0-2 determined **no spilling occurs** at 1× or
   4× the transaction ceiling (proven with a live control), so the write-path
   premise is unsupported; the read-path cache question waits for P1-2 by the
   plan's own dependency. Phase 2's business — Phase 1 does not touch it.
2. **The missing verification vehicles are built first** as separate,
   vehicle-only prerequisite commits (V1–V3 below), per the Phase 0 agent's
   second disposition.
3. **P1-5/P1-13/P1-15 stay anchored on the forced-64 shape.** P0-3 proved the
   ordering machinery does **zero work at the default 4,096 ceiling** (0 spills,
   0 rows); the forced shape reproduces the round-4 grid exactly. The items are
   therefore *correctness-under-stress* improvements whose regression guard is
   the new exact in-test counter assertions, not default-setting behaviour.
4. **A second counter bug is ruled into the prerequisite:** `read_batch`
   double-counts `read_waves` (`objects.rs:107` increments beside
   `note_read`'s `:124`) — one grouped demand is one wave. Fixed with P1-11 in
   the counter-correctness commit; P0-3's `read_waves` values for batch reads
   were inflated ×2 and are re-baselined append-only.

## 1. The checklist, with Big-O targets and before anchors

Notation: `c` children per branch level (real max 232) · `d` tree depth · `b`
bindings · `w` provider waves · `h` mapping height · `R` retain segments · `r`
ordering rows · `P` pending ceiling · `k` rows per page merge (≤ 740) · `n`
file bytes. **Space** = persisted canonical bytes: **identical before and after
every item** — the parity invariant, guarded by the 34-test sealed-oracle set.
"Trips" = provider/SQL round trips.

### Prerequisites (vehicle/counter commits; production LOC ≈ 0 except C1)

| id | item | Big-O / effect | gate |
| --- | --- | --- | --- |
| **C1** | counter correctness: P1-11 (`pages_read` counts batched decodes, `page.rs:40-41` vs `:185-186`) **+** the `read_waves` double-count fix (`objects.rs:107/:124`) | no complexity change — makes every later receipt honest | new pinned tests for both counters; append-only re-baseline of the affected Phase 0 rows |
| **V1** | `filesystem_timing_c1` prints the already-public `counters.validation` (~8 example lines) | exposes `ValidationWork` (Phase 0 `NOT_EXPOSED`) | D1–D6 re-baseline before P1-4's product commit |
| **V2** | `edit_memory_probe` example + `edit_timing_c1 --case delete/--case shrink` fixtures | exposes edit `nodes_read` beyond D27=9 and a memory-peak observable for P1-14 | baselines collected before P1-6/P1-9/P1-14 |
| **V3** | connection-opens counter in `StoreReadCounters` (product telemetry, `cas/store.rs:211/:247`) + vehicle print | exposes P1-2's before/after (today unobservable) | before-baseline: opens = O(waves) |

### The sixteen items

| id | item | time before → after | memory before → after | space (canonical / backing) | before anchor |
| --- | --- | --- | --- | --- | --- |
| P1-1 | wave width 32 → **256** (not 4,096: 4,096×8,280 B = 33.9 MiB vs the 4 MiB lease — unreachable; 256 reserves 2.08 MiB) + `list_after` cursor batching | waves/level `⌈c/32⌉` → `⌈c/256⌉ ≈ 1` (c ≤ 232); trips `O(c/32)` → `O(1)` per level | decoded batch grows **within the same 4 MiB lease**; nested-batch recursion degrades gracefully (documented trade) | identical / identical | D25 `order.default` 98 objects / 11 waves (post-C1 arithmetic) |
| P1-2 | pooled per-operation read session — `RefCell` in `StoreProvider` (per operation), **not** `Store` (keeps `Sync`; provider becomes `!Sync`, mirroring C1's `Rc<Cell>` budget); per-wave visibility-ceiling capture kept | fixed cost (connection + ~5 pragmas + 1 MiB workspace) `O(w)` → `O(1)` per op; ceiling query stays `O(w)` (required semantics) | workspace 1 MiB × w transient → 1 MiB × 1 held | identical / identical | V3 before-baseline |
| P1-3 | batch `page_from_wire`'s per-child point reads (`page.rs:470`) | materialization waves ~`f₁+f₂` (up to ~470 single-page) → ~3 | unchanged | identical / identical | no frozen workload exercises it — gate is the new unit test + frozen-row counter identity |
| P1-4 | batch validate's lookups (`lookup_many`) + validation-scoped page memo across the three walks | inode lookup trips `O(b·d)` (≈210 root descents on D2) → `O(d)` waves; page decodes 3× → 1× | +memo of walk pages, validation-scoped (must NOT leak — `filesystem_attributes.rs:556-559` pins the second-stat charge) | identical / identical; `entries_examined` must stay **bit-identical** | V1 D1–D6 (validate phase: D2 102 µs, the largest D1–D6 phase) |
| P1-5 | targeted scan reset in `spill()` — clear `scans[0..=level]` only (`runs.rs:231/299`) | lookup reads worst `O(r²/P)` (the n^1.9 residual) → `O(r)` | retained buffers within the same ≤32×16 KiB bound | identical / identical | D26 forced-64: `rows_read` 59,007 (per-doubling ×3.0) |
| P1-6 | delete the discarded validation load (`tree.rs:711` + `:764/765`) | constant factor: −(0..2) loads+decodes per edit | unchanged | identical / identical | D27 `nodes_read` = 9; V2 fixtures |
| P1-7 | fuse `compare_replacements` into the split descent (comparing sink; **Equal verdict preserved exactly**) | `O((⌈L/64K⌉+R)·h)` → `O(h)` | unchanged | identical / identical | V2 baselines; `edit_noop` pins the verdict |
| P1-8 | one ordered Retain cursor (`apply.rs:154-157` → PlanReader shape) | `O(R·h)` → `O(h)` | unchanged | identical / identical | V2 `--case shrink` |
| P1-9 | gate `rightmost_payload` for `replacement_len == 0` | deletion path `O(h)` stored loads → 0 | unchanged | identical / identical | V2 `--case delete` |
| P1-10 | carry ordering state (delete `zero_count_serials`' re-find pass) | release re-find `O(r)` lookups → 0 | unchanged | identical / identical | D26 |
| P1-12 | `push()` running total (`merge.rs:71-76`) | `O(k²)` → `O(k)` per merge (k ≤ 740) | unchanged | identical / identical | review-verifiable only — no counter exists (explicit gap) |
| P1-13 | merge fanout 4 / size-tiered cascade | merge writes `O(r·log₂(r/P))` → `O(r·log₄(r/P))` (still Θ(r log r)) | merge buffers 3×16 KiB → 5×16 KiB per live merge | identical / **backing spill volume ~halves** | D26: `rows_written` 25,760, `merges` 61, per-doubling ×2.72/2.52/2.36 |
| P1-14 | pre-sized canonical assembly for whole-file edits | unchanged (same bytes once) | **~3n → ~n peak** | identical (byte-identical) / identical | V2 memory probe (counting-allocator gate: ≤ ~2n) |
| P1-15 | hybrid binary-search-on-restart (sequential resume kept — pure binary search would *worsen* the pinned one-pass sweep) | restart lookup `O(position)` → `O(log rows)` | unchanged | identical / identical | D26 restart shape; optional restart-vs-resume counter |
| P1-16 | pending-ceiling dial: document ≤ ~349k spill-free rows; boundary test; **any default change is an owner ruling on #178** | spill work `O(r·log(r/P))` → 0 while `r ≤ P` | pending map ~0.5 MiB → up to ~64 MiB (ceiling-bounded; the explicit time↔memory trade) | identical / backing → 0 while `r ≤ P` | D25 (0 spills at default) vs D26 |

## 2. Unified landing order

```
C1 → V1 → V2 → V3            (prerequisites; C1 re-baselines the counters)
→ P1-2 → P1-1 → P1-3 → P1-4  (navigation+validate; P1-2 before P1-1 so wave
                               attribution is clean; P1-4 last, largest radius)
→ P1-6 → P1-9 → P1-7 → P1-8 → P1-14   (edit path; P1-12 anywhere, independent)
→ P1-5 → P1-13 → P1-15 → P1-10        (ordering; within-group order is load-bearing)
→ P1-16                      (last: documents the settled machinery)
```

## 3. Test strategy (from `test-evaluation.md`)

- **Parity guard: 34 tests** across the 7 sealed-oracle targets
  (`fixture_seal` 2, `filesystem_reference` 2, `edit_reference` 2,
  `object_identity` 11, `filesystem_codec` 9, `filesystem_updates` 6,
  `filesystem_profile` 2) — green and **unchanged** through everything.
- **Pinned-behaviour changes, pre-authorized with reasons:** the C1
  counter-semantics change (re-baseline); P1-1 updates
  `filesystem_bounds.rs:219` (`peak_wave() <= 32` → the new width). The other
  flagged pins are **avoided by design**: P1-7 keeps the `edit.compare` child
  scope; P1-4's memo is validation-scoped; P1-14 does not charge the whole-file
  route's counters.
- **15 new tests**, one per item except P1-12 (no counter, no test hooks
  possible — review-only, explicit gap), each designed to fail on the pre-item
  implementation — e.g. P1-5's exact tier-0-only `rows_read` after a targeted
  reset; P1-13's `merges == 4` for 16 single-row spills (today 15); P1-15's
  restart ≤ log₂(rows)+1 with the ascending sweep still exactly `== total`;
  P1-14's allocation-bytes ≤ ~2n under the counting allocator.
- **Verification is by deterministic work counters, never elapsed** (Phase 0
  proved bit-identical counter re-runs and a +17.6% same-binary elapsed spread
  on the forced shape). Receipts ride the frozen Phase 0 workload set.

## 4. The risks the implementer must carry

1. **Anchor honesty:** no `pages_read`/`read_waves` receipt may straddle C1's
   semantics change; the re-baseline is append-only.
2. **Error-order (P1-4):** batched prefetch can surface a corrupt-page error
   before an earlier semantic error on multi-error inputs — the plan's design
   keeps demand order; any residual change must be pinned deliberately.
3. **Ownership shapes:** P1-2's `RefCell` in `StoreProvider` (not `Store`);
   P1-14's pre-sized buffer must not change emission order (canonical order is
   pinned).
4. **The forced-64 scoping:** at default settings P1-5/P1-13/P1-15 regressions
   are behaviourally invisible — the exact in-test assertions are the only
   guard; do not weaken them to make landing easier.
5. **P1-12** is review-verifiable only — its commit message must show the
   arithmetic (`Σ widths` maintained incrementally) and cite the loop.
