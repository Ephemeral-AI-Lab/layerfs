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
| P1-6 | delete the discarded validation load (`tree.rs:711` + `:764/765`) | constant factor: −(0..2) loads+decodes per edit | unchanged | identical / identical | D27 `nodes_read` = 9; V2 fixtures. Risk: the discarded load validated with **non-root decode context** — corrupt-tree error identity could shift (no test pins it today; the new test must) |
| P1-7 | **comparing cursor + shared page memo** — **not** the fused descent: the chunked route publishes finalized nodes during the split/concat descent (`apply.rs:205-211`), so fusing comparison in would publish before a later segment's Equal verdict is known, breaking the zero-emission-on-Equal contract pinned at `edit_noop.rs:82`; comparison stays its own pass — verdict location unchanged, no dispatch reorder, so the `edit_timing.rs:109/:73` scope pins stay green — but becomes one ordered cursor (PlanReader shape over the chunked `FileState`) whose page demands are memo-served for the construction pass; the memo is id→canonical bytes, **consulted by `load_node` before the increment/demand so `nodes_read` stays honest**, page-capped with evict-all, chunked bases only, one `apply_edits` lifetime, cross-pass only (the cursor's own frontier already prevents intra-pass re-demands) | provider demands `O((⌈L/64K⌉+R)·h)` → `O(h + distinct union-path pages)` (D27 `nodes_read` 9 → ≈7, plus P1-6's −0..2) | unchanged (+ the edit-scoped memo) | identical / identical | V2 baselines; `edit_noop` pins the verdict **and** the zero-emission contract. Risks: error-ordering moves (pin deliberately); the `edit.compare` scope survives (the pass remains — the earlier scope-disappearance flag applied only to the now-rejected fused design). **Lands after P1-8** — the comparing cursor builds on the ordered-cursor machinery |
| P1-8 | one ordered Retain cursor (`apply.rs:154-157` → PlanReader shape) | `O(R·h)` → `O(h + shared)` provider demands | unchanged | identical / identical | V2 `--case shrink` (the only frozen shape with a chunked base → whole-file result; none of the five `measure_edits` fixtures exercises this route). Risk: the frontier must retain not-yet-visited siblings across segment gaps (a naive copy of `traverse`'s early break loses later paths); emission order is not at risk |
| P1-9 | gate `rightmost_payload` for `replacement_len == 0` (the **declared** length, not `removed_len` — inserts/overwrites keep the hint) | deletion path `O(h)` stored loads → 0 | unchanged | identical / identical | V2 `--case delete`; D27 is the negative control (must stay 9) |
| P1-10 | carry ordering state (delete `zero_count_serials`' re-find pass; states carried out of `touched_serials`) | release re-find `O(r)` lookups (≈ `serials_scanned`, 2,001 on the D26 shape) → 0 | unchanged | identical / identical | D26. Risks: a **public API change** and the touched-serial ceiling divisor 8 → 16 (`ordering_bytes/16`); a no-op at the default ceiling |
| P1-12 | `push()` running total (`merge.rs:71-76`) | `O(k²)` → `O(k)` per merge (k ≤ 740 = `MAXIMUM_DIRECTORY_LEAF_ROWS`) | unchanged | identical / identical | review-verifiable only — no counter exists (explicit gap). Risks: the new `Page.widths` field must not perturb `peak_scratch_bytes` (explicit `lease.grow` charges) and must not overload `Entry.bytes` (subtree bytes ≠ row width) |
| P1-13 | merge fanout 4 — **design (a) recommended: one-run-per-tier multiway cascade** (design (b), true size-tiered, rewrites `find`/scan ownership, ~+150 lines) | merge writes `O(r·log₂(r/P))` → `O(r·log₄(r/P))` (still Θ(r log r)); D26 prediction: `rows_written` 25,760 → ~12–15k, `merges` 61 → ~18–24 | merge buffers 3×16 KiB → 5×16 KiB per live merge — **currently outside the ownership account** (flagged to the owner) | identical / **backing spill volume ~halves** | D26: `rows_written` 25,760, `merges` 61, per-doubling ×2.72/2.52/2.36 |
| P1-14 | pre-sized canonical assembly for whole-file edits | unchanged (same bytes once) | **~3n → ~n + 23 B** peak | identical (byte-identical) / identical | V2 memory probe (counting-allocator gate: ≤ ~2n). Risks: aliasing is clean (written only via `&mut dyn Write`, exact reserve ⇒ no realloc), but the timing tree **loses the `content.encode` scope on this route** (pin the new shape); the probe must be an **example**, not a product counter — `edit_transitions.rs:771` pins `EditCounters::default()` on the whole-file route |
| P1-15 | hybrid binary-search-on-restart (sequential resume kept — pure binary search would *worsen* the pinned one-pass sweep) | restart lookup `O(position ≤ 2,048 rows on the D26 shape)` → ~11 probes, each charging `rows_read` | unchanged (stack probe buffer keeps the zero-alloc test green) | identical / identical | D26 restart shape; risk: random 96-B `read_at` can defeat the 170-row buffered read — call count may rise on short restarts (receipt must show reads, not calls) |
| P1-16 | pending-ceiling dial: document ≤ **349,525** spill-free rows (192 B/row under the ×2 charge); boundary test; **any default change is an owner ruling on #178** — recommended document-only until (a) P1-5/13/15 receipts exist (a re-default erases their anchor), (b) a workload shows >4,096-serial operations (P0-3 found none), (c) the owner rules on the unaccounted heap | spill work `O(r·log(r/P))` → 0 while `r ≤ P` | pending map ~0.5 MiB → up to ~64 MiB charged, but the **real BTreeMap heap (~120 B/row ≈ 42 MiB at the top) is invisible to the ownership account** — flagged | identical / backing → 0 while `r ≤ P` | D25 (0 spills at default) vs D26 |

## 2. Unified landing order

```
C1 → V1 → V2 → V3            (prerequisites; C1 re-baselines the counters)
→ P1-2 → P1-1 → P1-3 → P1-4  (navigation+validate; P1-2 before P1-1 so wave
                               attribution is clean; P1-4 last, largest radius)
→ P1-6 → P1-9 → P1-8 → P1-7 → P1-14   (edit path; P1-8 before P1-7 — the
                               comparing cursor builds on the ordered cursor;
                               P1-12 anywhere, independent)
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


## 4. File plan and LOC estimate (pre-implementation; every commit discloses actuals)

### 4.1 Proposed file/folder structure

No new production module is planned — every product change lands in an existing
file, split by its existing responsibility. New *files* appear only as tests'
new cases (inside existing test files), two example additions, and the
append-only evidence directory.

```text
core/crates/layerfs-content/
  src/                                    PRODUCTION (999-line ceiling; lib/mod 200)
    file/edit/apply.rs        465 lines   P1-7, P1-8, P1-9, P1-14
    file/edit/compare.rs       87 lines   P1-7 (may shrink toward 0 as its
                                          per-window loop is absorbed by the
                                          fused descent)
    file/edit/tree.rs         895 lines   P1-6 (net-negative; tightest file —
                                          104 lines of headroom, deletion-only)
    file/content.rs           266 lines   P1-14
    filesystem/objects.rs     167 lines   C1 (read_waves double-count fix)
    filesystem/validate.rs    718 lines   P1-4 (largest single-file addition;
                                          ~280 headroom)
    filesystem/update.rs      498 lines   P1-10
    filesystem/read.rs        275 lines   P1-1 (list_after cursor batching)
    filesystem/references/runs.rs    595  P1-5, P1-13, P1-15 (combined ~400
                                          headroom for all three)
    filesystem/references/merge.rs   260  P1-13, P1-15 (buffers, probe path)
    filesystem/references/reduce.rs  585  P1-10 (carried state; API change)
    filesystem/sorted/page.rs        559  C1, P1-1, P1-3, P1-12
    filesystem/sorted/merge.rs       314  P1-12 (Page.widths running total)
    filesystem/directory/read.rs     315  P1-1 call site (beside inode/read.rs,
                                          attributes/patch.rs, validate.rs —
                                          width plumbing only)
  tests/                                  NO line ceiling; new cases in place
    filesystem_bounds.rs      1047 lines  C1 counters, P1-1's authorized pin
                                          update, P1-3's gate test
    filesystem_ordering.rs    977 lines   P1-5, P1-10, P1-13 exact-count tests
    filesystem_ordering_scan.rs 157 lines P1-15's probe assertions
    edit_* suites                        P1-6..P1-9, P1-14 (counting allocator)
  examples/                               EXCLUDED from production LOC
    edit_memory_probe.rs      NEW  ~120-180 lines   V2 (memory peak vehicle)
    edit_timing_c1.rs         180 lines   V2 (+ --case delete / --case shrink)
    filesystem_timing_c1.rs   604 lines   V1 (+ counters.validation print)

core/crates/layerfs-storage/
  src/cas/store.rs           549 lines   V3 (StoreReadCounters + opens), P1-2
  src/cas/provider.rs        104 lines   P1-2 (per-op ReadSession, RefCell)
  src/cas/read.rs                       P1-2 (session plumbing, ceiling kept)
  tests/                                  P1-2's session-visibility test

docs/roadmap/0.1/0.1.7/evidence/phase1-execution-<stamp>/   append-only receipts
core/docs/architecture/                  same-commit doc updates per item
                                         (P1-16 names 04-filesystem.md and
                                         06-limits.md explicitly)
```

**Contingency:** if any item would push a file past 999 (only `tree.rs` at 895
is near, and P1-6 only deletes there), the repo rule applies — split by
responsibility into a focused named file, never numbered parts.

### 4.2 Production LOC estimate range (core, src/ + shipped SQL only)

Baseline: **18,792** (C1 11,917 · C2 6,112 · telemetry 763; reference 65,417
unchanged by Phase 1). Estimates are the main agent's from the change sketches —
each commit discloses the audited actual.

| commit | estimate | note |
| --- | ---: | --- |
| C1 | 0 ± 5 | two counter fixes, near-net-zero |
| V1 / V2 | 0 | examples only |
| V3 | +10..+20 | StoreReadCounters field + plumbing |
| P1-1 | +20..+45 | width constant + cursor batching |
| P1-2 | +70..+130 | session type + lifecycle (biggest C2 item) |
| P1-3 | +25..+50 | reuses P1-1 machinery |
| P1-4 | +80..+150 | batching + memo (biggest C1 nav item) |
| P1-5 | +2..+8 | truncate at two sites |
| P1-6 | −4..−8 | net-negative (deletion) |
| P1-7 | −30..+40 | may be net-negative as compare.rs is absorbed |
| P1-8 | +40..+70 | ordered cursor |
| P1-9 | +3..+8 | a gate |
| P1-10 | +50..+90 | carried state + the API/divisor change |
| P1-12 | +20..+40 | Page.widths + running total |
| P1-13 | +90..+160 | fanout-4 multiway cascade (biggest ordering item) |
| P1-14 | +35..+65 | pre-sized assembly |
| P1-15 | +45..+80 | stack probe + restart search |
| P1-16 | 0 | docs + test only |
| **net total** | **≈ +450..+950** | dominated by P1-2, P1-4, P1-13 |

Projected end state: core ≈ **19,250..19,750**, C1 carrying nearly all of the
growth (C2 +80..+150 via V3/P1-2 only). Non-production code outside the LOC
comparison: ~17–20 new test cases (+300..+550 test lines), V2's example
(+170..+260 lines across two files), V1/V3 prints (+15..+25), and the receipts.

## 4b. Execution status (partial, 2026-09-18)

> **Not the closing pass.** Five P1 boxes are still open and one is declined, so
> §5's closing self-falsification has **not** been run and nothing below claims it.
> This section records the landed state for the next driver; every number is a
> measured counter from a round receipt under
> [`../evidence/phase1-execution-20260918T090000Z/rounds/`](../evidence/phase1-execution-20260918T090000Z/rounds/).

**Landed** (each its own single-variable commit, receipt and author-verified
`verify-<item>.md`): C1 `7447f87d9` · V1 `2b5e27e65` · V2 `582dea9dd` ·
V3 `aefcd95a5` · V4 `8efdd9291` (a plan refinement: the edit vehicle's own
`EditCounters.nodes_read`) · P1-2 `9ec299f13` · P1-1 `32eda6f29` ·
P1-3 `ff4d6d328` · P1-4 `bfb01f262` · P1-9 `d42cd969e` · P1-5 `70dc75836` ·
P1-12 `9b4eff169` · P1-16 `84ca5c851` · P1-10 `8327f87bb` · P1-6 `360431d10` ·
P1-8 `e9b4d1510` · P1-7 `974b525be` · P1-14 `d6bc1404a`. Tree `7981d03f2`,
pushed; 456 tests green; the 34-test sealed-oracle set green and unchanged.

| item | counter | before → after |
| --- | --- | --- |
| C1 | D2/D5 `objects.read_waves` / `inodes.pages_read` | 4 → 3 / 1 → 5; `order` rows (corrected pair) 11 → 7 waves, 2 → 17 dir pages, 1 → 81 inode pages |
| P1-2 | D21–D24 `readback connection opens` | 3 → 1 |
| P1-1 | D25/D26 `read_waves` / `ino_scratch` | 7 → 5 / 349,820 → 709,388 |
| P1-3 | 13,000-inode fixture provider waves / `inodes.read_waves` | 170 → 107 / 68 → 5 |
| P1-4 | D2 validation waves / pages (D4) | 42 → 2 / 42 → 2 (6 → 1); `inode_demands` identical |
| P1-9 | M2 `edit_nodes_read` | 8 → 7 |
| P1-5 | D26 `runs.rows_read` | 59,007 → 27,777 (residual 33,247 → 2,017) |
| P1-10 | D26 `runs.rows_read` | 27,777 → 25,809 |
| P1-12 | no counter; page partition pinned; `peak_scratch_bytes` +8 B/page slot | — |
| P1-8 | M4 (`edit_timing_c1 --case split`, new) `nodes_read` | 16 → 13 |
| P1-7 | D27 `nodes_read` / `edit_nodes_read` | 9 → 7 / 10 → 8 |
| P1-14 | M1 `peak_delta_bytes` | 262,328 → 131,826 |
| P1-16 | docs + boundary test; nothing moved | — |

**Declined, owner ruling needed:** P1-6 (zero movement on every available row; the
deletion removes the only non-root context check on that node) —
[receipt](../evidence/phase1-execution-20260918T090000Z/rounds/p1-6/receipt.md).

**Incomplete, cause diagnosed:** P1-13 (merge fanout 4) — the multiway cascade was
built, hit one real selection bug and then **data-correctness** failures (the
spilling and non-spilling runs produced different filesystem roots), and was
reverted in full; the tree is clean and D26's anchors are untouched. The cause is
found: my grouped cascade leaves a **stale duplicate of a serial in a
higher-indexed (newer) tier than the current row**, so `find` (first tier that
holds the key) and the newest-first final stream (highest tier wins) disagree — for
serial 26 the probe reads `count=1` from tier 5 and `count=0` from tier 6, both
live. That is an ordering bug in my cascade, **not** a property of the fanout-4
design and **not** the design question the first receipt named. The fix
(newest-group-first draining plus the tier-ordering invariant as a test) is
identified but not written; the receipt records it.

**Not started:** P1-15 — the restart lookup half of the ordering pair. P1-13's
blocker blocks it by its own dependency note ("restart pattern and run lengths
follow the cascade policy"), and the assignment's landing order puts it after
P1-13. **The executable assignment for those five is
[`phase1-continuation-handoff-20260918.md`](phase1-continuation-handoff-20260918.md)**,
which carries each one's anchor, file, sketch, corrected design, tests and risks.

**Plan corrections the receipts produced** (each recorded in its round): the C1
`order` rows were first measured through a stale probe client (receipt §9, driver
fixed and `artifacts.txt` added); P1-1's `list_after` half is declined and its
`patch.rs` half deferred; P1-9's printed `nodes_read` cannot move (V4 added);
P1-12's `peak_scratch_bytes` does move (+8 B/page slot); P1-5's `truncate(level + 1)`
sketch is backwards; P1-3/P1-4/P1-16 each corrected an assumption in their sketches;
P1-8's retained-frontier sketch was replaced by a bounded page cache (the same
Big-O target, the traversal untouched) and **two of its targets are refuted**: M3
is a single retained run so it cannot move, and payload demands are unchanged
because each retained run is served exactly and independently —
[receipt](../evidence/phase1-execution-20260918T090000Z/rounds/p1-8/receipt.md) §2/§3.2.

## 5. The risks the implementer must carry

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
