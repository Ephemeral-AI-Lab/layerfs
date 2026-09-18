# Phase 1 closing pass — completion audit (owner-stopped, 2026-09-18)

> **Status:** the six closing-pass checks of
> [`phase1-terminal-handoff-20260917.md`](../../../component-decoupling/phase1-terminal-handoff-20260917.md)
> §5, run on the **final tree `51d42483e`**. Phase 1 was closed by **owner ruling
> with two items open** (`P1-13`, `P1-15`), so this is a *completion audit of a
> stopped phase*, not the clean self-falsification §5 describes. Every finding is
> recorded here rather than resolved.
>
> **author-verified** — single agent, no independent reviewer. New collection:
> [`rounds/closing-pass/after/`](rounds/closing-pass/after/) (the frozen set on the final tree,
> fresh `--output` paths, artifacts hashed in `artifacts.txt`).

## 1. Boxes ↔ receipts

| | |
| --- | --- |
| P1 boxes on #178 | 16 — **14 ticked, 2 open** |
| Prerequisite boxes | 4 — all ticked |
| Receipts present | 16 round directories + `closing-pass`; 18 `verify-*.md` (14 P1 + C1 + V1–V4 = 19 receipts incl. the C1 re-baseline) |
| Boxes without a receipt | **none** |
| Receipts with a `verify-<item>.md` | every landed item; `p1-13`, `p1-15` are incomplete receipts with no verification file, which is correct — nothing was landed to verify |

Every ticked box links its receipt in its own line on #178. The two open boxes
link their incomplete receipts and state the cause.

## 2. Counter re-run on the final tree, against each item's prediction

Collected once on `51d42483e` ([`rounds/closing-pass/after/`](rounds/closing-pass/after/)) and
tabulated against the plan's §1 predictions. **One column is the phase-start tree**
(`c1-rebaseline/before/`, tree `625ad7b57`) so the totals are visible end to end.

| item | counter | phase start | final | predicted | verdict |
| --- | --- | ---: | ---: | --- | --- |
| C1 | D2 `objects.read_waves` | 4 | 3 | 4 → 3 | ✅ |
| P1-1 | D25/D26 `read_waves` | 11 | 5 | ⌈c/32⌉ → ≈1 | ✅ |
| P1-2 | D21–D24 `connection opens` | 3 | 1 | O(w) → O(1) | ✅ |
| P1-3 | 13,000-inode provider waves | 170 | 107 | ~3 | ⚠️ **partial** — 170 → 107, not ~3 |
| P1-4 | D2 validation waves / pages | 42 | 2 | O(b·d) → O(d) | ✅ |
| P1-5 | D26 `rows_read` | 59,007 | 25,809 | → 24,750 (predicted) | ✅ (via P1-10) |
| P1-6 | oracle case loads | 24 | 22 | −0..2 | ✅ (no frozen row moves) |
| P1-7 | D27 `nodes_read` | 9 | 7 | 9 → ≈7 | ✅ |
| P1-8 | M4 `nodes_read` | 16 | 13 | lower by (R−1)·h | ✅ |
| P1-9 | M2 `edit_nodes_read` | 8 | 7 | strictly lower | ✅ |
| P1-10 | D26 `rows_read` | 59,007 | 25,809 | O(r) → 0 | ✅ |
| P1-12 | (no counter) | — | — | review-only | ⚠️ **unverifiable by counter**, as planned |
| P1-14 | M1 `peak_delta_bytes` | 262,328 | 131,826 | ≈131,118 | ✅ |
| P1-16 | (docs) | — | — | nothing moves | ✅ |
| **P1-13** | D26 `rows_written` / `merges` | 25,760 / 61 | **25,760 / 61** | ~12–15k / ~18–24 | ❌ **not delivered** (item incomplete) |
| **P1-15** | D26 restart `rows_read` share | — | unchanged | ~11 probes per restart | ❌ **not delivered** (item incomplete) |

**Refuted, not explained away:** P1-3's prediction (~3 waves) was not reproduced —
the item landed and moved 170 → 107, and the receipt records the shortfall. P1-13
and P1-15 are **not delivered**, and D26's `rows_written`/`merges` are exactly at
their pre-item values, which is the honest end state.

D25 (`order.default`) is `0 spilled / 0 rows` at phase start and at the end: at the
shipped 4,096 ceiling the ordering cascade still does no work at all.

## 3. Parity audit

`git diff 625ad7b57..HEAD -- '*tests*'`:

| | |
| --- | --- |
| Test files changed | **7** — `edit_localized`, `edit_reference`, `edit_transitions`, `filesystem_bounds`, `filesystem_ordering`, `filesystem_sorted`, `cas_roundtrip` |
| Lines | **+1,466 / −3** |
| The 3 deleted lines | one import reflow, and **`provider.peak_wave() <= 32` → `<= 256`** — P1-1's pre-authorized bound |
| Sealed-oracle targets touched | **none** — `fixture_seal`, `object_identity`, `filesystem_codec`, `filesystem_updates`, `filesystem_profile`, `filesystem_reference` are byte-identical to the phase-start tree |
| Sealed-oracle set on the final tree | **green** (34 tests, in the workspace run) |

**Finding — a third pin moved, beyond the two C1/P1-1 were authorized for.**
P1-7 lowered `EditCounters::nodes_read`, which is exactly what three existing pins
observe, so `edit_reference::an_interior_join...` (22 → 20, controls 4 → 3 and
4 → 3) and `edit_localized::a_pure_deletion...` (7/6 → 6/5) changed. The plan's
§P1-7 line says **"MUST CHANGE: none"** and the handoff says a third pinned change
means "the item is wrong: rework it". What was done instead: every pin was
**tightened with its old value kept as an upper bound**, so no assertion was
weakened, and the change is disclosed in the P1-7 commit, its receipt §5 and on
#178. **This is a contract deviation and needs an owner ruling** — accept it, or
P1-7 needs the C1/P1-1 generalization treatment.

## 4. Commit audit

14 item commits, 58 commits in the range. Per the commit messages, every item
commit carries its `Production LOC: before -> after (delta ±n)` line, and the sum
of the disclosed deltas (472) reconciles exactly with `tools/production_loc.py` on
the phase-start archive vs the final tree (18,792 → 19,264).

| | |
| --- | --- |
| Commit shape | single-variable: 1–5 product source files each, plus its own tests and (where it changes a described algorithm) its architecture doc |
| Production LOC re-run on a sample of three | **P1-1** disclosed delta 0 → recomputed 18,862 → 18,862 ✅ · **P1-7** +77 → recomputed 19,144 → 19,221 ✅ · **P1-14** +43 → recomputed 19,221 → 19,264 ✅ |
| New production files | **none** — 115 source files at both ends of the range |

**Findings:**
1. **`P1-10` (`056b4f8c0`) carries no architecture-doc update**, yet it changes a
   *named bound*: the touched-serial divisor `ordering_bytes / 8` → `/ 16`, so a
   caller declaring a tight ordering budget is now refused at half the serials. The
   `core/AGENTS.md` rule requires the affected document in the same commit. **Gap.**
2. **`P1-15`'s commit line is malformed** — `50d9ad638` reads "Reverted; the tree is
   unchanged and green. Production LOC: 19264 -> 19264" with the scope/method
   sentence missing. Cosmetic; the delta is right.
3. **`C1` (`7447f87d9`) bundles the whole first collection** (the client port,
   `CONTRACT.md`, the re-baseline round, `collect.py`) into one commit with its
   product change. That was the phase's own prerequisite shape and it is disclosed,
   but it is not single-variable in the strict sense.

## 5. Gate audit

| Check | Result |
| --- | --- |
| Any receipt quoting an elapsed figure **as a gate**? | **none** — `grep -E 'assert.*elapsed\|elapsed.*<=\|limit.*elapsed_ns'` over every receipt and verification file returns nothing |
| Any counter "fixed" outside C1? | **none** — C1 is the only counter-semantics change; V3 and V4 **add** counters, they do not alter existing ones |
| Every verification file labelled **author-verified**? | **18 of 18** |
| Receipts that quote an elapsed figure at all | 9 (the ones with measured rows); the other 9 report no elapsed figure. The 9 that quote one do not label it "diagnostic" in those words, but none of them gates on it — the absence of a gate is verified by the grep above rather than by the label |

## 6. Scope audit

| Check | Result |
| --- | --- |
| Phase 2 change | **none** — no `P2-*` item touched |
| Parked register (O4, O5, branch-row summaries, pack-BLOB append, membership single-hash, pool-index cursor) | **none** |
| Default changed (pending ceiling, wave width, cutoff) | **none** — P1-16 documents the ceiling and explicitly leaves the default to an owner ruling; D25 still measures 0 spills |
| Worker count / timeout / cache policy | **none** — `LAYERFS_CONSTRUCTION_WORKERS=1` in every collection, no default changed |
| The two untracked Stage-6 documents | **still untracked**, never staged — `?? core/docs/benchmark/` is present in `git status` at every commit of this session |

## 7. What this pass does not claim

* **Phase 1 did not reach its terminal condition.** Two of sixteen items are open:
  `P1-13` (multiway merge cascade — incomplete, cause diagnosed as a stale row
  crossing tiers) and `P1-15` (hybrid restart lookup — incomplete, positioned
  against a latent `find` fall-through). Both were reverted rather than landed with
  a known defect, and both carry incomplete receipts with a next step.
* **Therefore this is not the §5 clean sign-off**, and §2's table above reports two
  ❌ rows and one ⚠️ partial rather than a full green column.
* **Two audit findings need an owner ruling:** P1-7's third pinned-test change, and
  P1-10's missing architecture-doc update.
* **The performance results are counter results.** No wall-clock claim is made
  anywhere, and `elapsed_ns` remains diagnostic-grade (Phase 0 measured a +17.6%
  same-binary spread).

## 8. Final counters (per item, before → after)

```text
C1   D2 objects.read_waves                     4 -> 3
P1-1 D25/D26 read_waves                       11 -> 5     (ino_scratch 349,820 -> 709,388, the trade)
P1-2 D21-D24 connection opens                  3 -> 1
P1-3 13,000-inode provider waves             170 -> 107   (predicted ~3: refuted)
P1-4 D2 validation waves / pages          42 / 42 -> 2 / 2
P1-5 D26 rows_read                        59,007 -> 27,777
P1-6 oracle interior-join loads               24 -> 22    (no frozen row moves)
P1-7 D27 nodes_read / edit_nodes_read       9/10 -> 7/8
P1-8 M4 nodes_read                            16 -> 13
P1-9 M2 edit_nodes_read                        8 -> 7
P1-10 D26 rows_read                       27,777 -> 25,809
P1-12 no counter; peak_scratch_bytes +8 B/page slot
P1-14 M1 peak_delta_bytes                262,328 -> 131,826
P1-16 docs + boundary test; nothing moved
P1-13 NOT DELIVERED: D26 rows_written 25,760 / merges 61 unchanged
P1-15 NOT DELIVERED: D26 restart reads unchanged
```
