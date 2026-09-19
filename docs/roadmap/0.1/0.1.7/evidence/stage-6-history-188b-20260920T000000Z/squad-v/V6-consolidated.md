# V6 — consolidated: what the gap is, and whether T2/T3 are needed

**Diagnostic.** Source pin @66bce8378@ + the working tree. **No timing number appears here** — the
machine was shared; every cost is bytes or production lines. Squads V1–V4 + one measurement by the parent.

## 1. The gap against v0.1.6 — it was a declaration rule, not a missing layer

**V1**, per-object over the 44,141 shared whole-file objects, residual 0:

| class | objects | v0.1.6 -> T1 |
| --- | --: | --: |
| delta in v0.1.6, **FULL** in T1 | 2,594 | 1,765,661 -> 7,981,283 = **+6,215,622** |
| delta in both, **same base** | 28,956 | **frames byte-identical, +0** |
| delta in both, different base | 4,354 | +159,246 |
| **FULL** in v0.1.6, delta in T1 | 1,129 | **-1,484,666** (we win) |
| FULL in both | 7,108 | +0 |

**Three falsifications, all measured:**

1. **"v0.1.6's base is nearer" — FALSE.** Its base-distance histogram peaks at checkpoint gap **10**, the
   same as ours; 100.00 % of both Stores' bases lie inside the same object set. There is no richer pool.
2. **"Better codec / bigger window" — FALSE.** 28,956 same-base frames are byte-identical; frame-header
   censuses match entry-for-entry; all frames are single-segment so no window descriptor is written.
3. **"More candidates is worse" — an ORDERING result.** The four-slot arms *gained* 8,625,119 B and
   *lost* 18,214,699 B by **replacing** 17,141 good same-path bases. The class is worth 8.6 MB; the
   order cost 18.2 MB.

**V4 independently falsified the T1 report's Stage 7 diagnosis**: v0.1.6's Store holds exactly **17
commits** and the same 44,141 whole-file objects. The gap was v0.1.6's **declaration rule** —
@admission.rs:441-486@, *use the declared predecessor when there is one; consult the similarity cache
only when @anchor.is_none()@* — and our default arm declared the same-path version and nothing else.

## 2. The arm, run by the parent — **-6,402,048 B**, harness-only, 0 product lines

| arm | apparent | whole-file lane | @no_candidate@ | @trials@ | @prefix_selected@ | @ineligible@ |
| --- | --: | --: | --: | --: | --: | --: |
| default | 57,749,504 | 43,873,480 | 10,732 | 34,488 | 34,439 | 572 |
| **fallback (v0.1.6's rule)** | **51,347,456** | **37,347,557** | 7,326 | 37,886 | 37,797 | 2,585 |

**v0.1.6's whole-file lane is 38,983,278 B. Ours is now 37,347,557 B — we beat it by 1,635,721 B.**

**The residual inverts:**

@```
  gap +2,031,616 = 1.0412x v0.1.6
    whole-file      -1,635,721   -80.5 %   <- WE WIN
    native          +1,737,621    85.5 %
    ordinary          +344,456    17.0 %
    pooled            -221,539   -10.9 %   <- WE WIN
    pack framing       +15,680
    non-pack        +1,791,119    88.2 %
    residual 0
@```

It began as **81.87 % a missing base declaration**; it is now **88.2 % the non-pack row grammar** and
**85.5 % the chunk lane**.

**Honest correction:** V1 estimated 49,124,385 B for this arm. The measurement is **51,347,456** —
**2,223,071 B optimistic**. @ineligible_candidates@ tripled (572 -> 2,585); the second-order chain-depth
interaction is real and the estimate did not model it.

## 2.1 V4 ran FOUR arms, and corrected two things in this document

| arm | apparent | @prefix_selected@ | @no_candidate@ | @ineligible@ |
| --- | --: | --: | --: | --: |
| default (T1 best) | 57,749,504 | 34,439 | 10,732 | 572 |
| **fallback, 2 candidates** | **51,347,456** | 37,797 | 7,326 | 2,585 |
| fallback, 4 candidates | 51,519,488 (**+172,032 worse**) | 37,821 | 7,296 | 2,185 |
| fallback, @DEPTH_LIMIT=5@ | 55,869,440 (**4,521,984 worse**) | — | 8,558 | 236 |

**Why V1's estimate missed by 2,223,071 B — the per-object transition (V4, residual 0):**

@```
  based->based  33,950:  15,652,232 -> 15,284,282   -367,950   (pack regrouping, hypothesis)
  based->FULL      489:     411,969 ->  2,467,138  +2,055,169   <- THE REGRESSION
  FULL->based    3,847:  11,437,331 ->  3,224,189  -8,213,142
  FULL->FULL     5,855:  16,371,948 -> 16,371,948           0
  net lane -6,525,923;  apparent -6,402,048;  residual 0
@```

**489 objects that HAD a base lose it because their base became an ineligible (deep) chain node** — the
watch item fired. V1's 8,625,119 is the **gross** gain (8,213,142 + 367,950 = 8,581,092, within 0.5 %).

**@DEPTH_LIMIT=5@ removes the ineligibility (236) and is 4.5 MB worse** — it suppresses 3,745 good
declarations to avoid 2,349 bad ones. **L7 has no harness-side fix.**

**Two corrections to earlier sections of this document:**

1. **R1b is NOT additive with L1.** V4: *R1b is L1's product form — the same bytes, not a second
   645,974.* T1's framing of R1b as "the only remaining form of R1" is right about the **form** and
   wrong as an **addition**. §4's "off the critical path" stands; §3 must not add it.
2. **V4's H-V4-1 (basename reuse) is WITHDRAWN** in favour of V1's mechanism — the same 2,594 objects.
   **Take the max, never the sum.**

## 3. V3's levers, re-based — and the caveat that matters

**V3 costed everything on the PRE-fallback Store (57,749,504).** Re-based on the measured
51,347,456:

| lever | bytes | cost | basis |
| --- | --: | --- | --- |
| **codec level 3 -> 19, whole-file** | **~4,270,443** | **2 constants, no format change** | **est** — base-less half measured, based half modelled |
| chunk lane bases (L4) | 1,737,621 | +160-200 LOC, no amendment | measured, identical content both sides |
| @base_object_id@ into the record | 1,204,224 | ~150-300 LOC, 9 files, **SCHEMA_VERSION 5** | measured by table rebuild + VACUUM |
| VACUUM at close | 385,024 | a close-time step | measured net of v0.1.6's own reclaim |
| @GROUP_LEVEL@ 1 -> 19 | 261,819 | one constant | measured |

@```
  PRE-fallback  (V3's own baseline)  57,749,504 -> 49,890,373   vs gate  -574,533
  POST-fallback (as measured)        51,347,456 -> 43,488,325   vs gate +5,827,515
@```

**V3's own conclusion — "everything except T2/T3 sums to 49,370,181, 54,341 B above the gate; T2 is the
only certain closer" — was reached without the fallback arm, which did not exist when V3 started.**

**Two independent paths now exist, and they differ by exactly the codec level:**

@```
  V4's conservative path (no codec-level change, no new ruling for it):
      57,749,504 -> L1 51,347,456 -> +L4 49,609,835 -> +L5 48,372,843 = 0.9809x   MARGIN 942,997
  this document's path (adds V3's codec level 3->19, est, + VACUUM + GROUP_LEVEL):
      51,347,456 -> 43,488,325 = 0.8819x                                          MARGIN 5,827,515
@```

**The codec level is the whole difference — ~4.27 MB.** V4 did not cost it; V3 did, as @est@ on the
pre-fallback record mix. **Neither path includes T2 or T3.**

**Caveat, and it is not small: this is a naive sum, not a measurement.** The codec-level figure was
computed on the pre-fallback record mix (9,702 FULL + 34,439 PREFIX); after the fallback arm the mix is
13,318 + 37,797, so it does not transfer directly. The @base_object_id@ lever is *conservative* — more
base rows means a bigger column to remove. **The combination must be re-measured before it is relied on.**

## 4. Are T2, T3 and the others worth it? — mostly no

| tier | verdict |
| --- | --- |
| **T2 grouping** | **NOT NEEDED.** V3: @256 KiB stored, product's own seal rule — L3 gives 1.0635x (**FAIL**), L19 gives 0.9900x (PASS). It closes the gate **only** at level 19, and costs **78.7x** median single-record read amplification, a **new whole-file format**, and it makes @space.whole_file_records@ impossible. It was the "only certain closer" **only** because the fallback arm was missing. |
| **T3 per-path chains** | **DO NOT START.** ~18,405,000 B est, but 1,200-2,500 LOC est, a path entity the contract denies (@lib.rs:5-7@), up to 11,263,931 B decoded per read, a @SCHEMA_VERSION@ bump, and the same correspondence gap. Name it as the fallback in #189. |
| **ordinary lane / tree-role bases** | **NEGATIVE RESULT.** 5,055 tree records / 2,049,437 B. Existing grouping = 619,520 B (3.31x); per-record with a previous-same-role dictionary = 893,229 B; best-of-20 oracle = 783,045 B — **both worse than grouping**. A 405 B average record cannot carry 33 B of prefix framing (8.1 %). Perfect-oracle upper bound <= 332,291 B. The lane's 344,456 B gap is **content volume**, not representation. |
| **#185** | **STILL NO.** Its scope is the crossings — large->small **120,490 B = 1.43 %** measured. The 20.6 % belongs to chapter 14's **cursor** half, which was never deferred and needs no amendment. |
| **R1b (persisted index, ruling B)** | **OFF THE CRITICAL PATH.** V4 costed it at 645,974 B for a new table and a @SCHEMA_VERSION@ bump; the fallback arm recovered 6.4 MB of the same coverage from the caller side with no format change. |

## 4.1 What the T1 target would take — and it is not on either path

**V4: the T1 target of 47,048,435 B is NOT reachable on the conservative path — short 1,324,408 B.** The
only route V4 identifies is recovering **L7, the 2,055,169 B regression** (489 objects that lost a base
to an ineligible deep node), which would give 46,317,674 = 0.9392x. **That is a product depth-accounting
fix with no identified mechanism**, and the harness-side attempt to fix it (@DEPTH_LIMIT=5@) made things
4.5 MB worse.

**So: the gate is reachable and the target is not.** #188 should be re-scoped accordingly, not closed.

## 5. The two things that now decide the outcome

1. **The codec level is the biggest remaining lever and the cheapest to write — but its cost is CPU, and
   this lane already fails its complete-command budget.** The measured @budget.complete-command@ is
   **40.0 s against a 15 s ceiling**, recorded as @NOT_RUN@ rather than shrunk to fit. Raising level 3 to
   19 makes an already-over-budget command slower. **The level lever trades storage bytes against a
   budget that is already failing**, and the CPU cost is **unmeasured** — no timing may be taken on a
   shared machine. This needs a quiet, declared-cache measurement before it is accepted.
2. **The combination is a sum of separately-measured levers, not a measurement.** The next run must
   apply them together on one binary and read the result.

## 6. Standing caveats

- **No stride3 confirmation exists.** Nothing here is a gate claim.
- The verify phase is a **declared sample** (64 paths per state, 1,083 of 86,064 units) and reports
  @INCOMPLETE@, not PASS.
- V3's based-half codec figures are **est** — its model is byte-exact on tag-0 (residual 0) but only
  113/400 on tag-1 (@refPrefix@).
- The fallback arm's @ineligible_candidates@ tripled; the mechanism is not fully explained.
- **Production LOC delta 0.** The harness change is uncommitted; nothing under @core/crates/@ was
  changed by this round.
