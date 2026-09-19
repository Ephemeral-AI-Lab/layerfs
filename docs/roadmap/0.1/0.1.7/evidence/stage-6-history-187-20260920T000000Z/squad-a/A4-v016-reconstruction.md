# A4 — Empirical reconstruction of v0.1.6's per-bucket Store numbers

**Squad A4 · diagnostic only · not admission evidence · no product source touched.**

The claim under investigation is that lane `history-stride10` retains the harness history in a
Store 2.65× larger than v0.1.6's for byte-identical content. This report reconstructs v0.1.6's
per-bucket numbers.

**Headline: v0.1.6's stride-10 Store is NOT lost. It is on disk, and it decodes.** The recorded
v0.1.6 totals are reproduced to the byte from the retained artifact, so the per-bucket table is
**exact**, not a bound: there is no reconstruction uncertainty left to bound. The mechanism is
also settled empirically rather than inferred — v0.1.6's whole-file lane carries a **32-byte delta
base identity inside the record**, and **31,108 of 44,141** such records name a base that lives in
a strictly earlier pack (an earlier commit). The 49.3 MB figure is **unreachable without
cross-commit bases** by a factor of at least 2.46×.

---

## 0. Summary of headline numbers (stride-10, 17 states)

| quantity | C1 (`core/crates`, v0.1.7) | v0.1.6 | C1 / v0.1.6 |
|---|--:|--:|--:|
| Store apparent (retained artifact) | 128,864,256 | 49,315,840 | **2.6130×** |
| Store apparent as recorded | 128,864,256 | 49,315,940 | **2.6130×** |
| Store allocated as recorded | 130,863,104 | 49,344,512 | **2.6520×** |
| pack BLOBs `SUM(length(object_packs.data))` | 119,894,291 | 46,056,732 | 2.6032× |
| group bodies | 119,678,627 | 45,860,632 | 2.6096× |
| pack headers + directories | 215,664 | 196,100 | 1.0997× |
| non-pack SQLite bytes | 8,969,965 | 3,259,108 | 2.7523× |
| objects / canonical bytes | 52,032 / 380,921,300 | 51,722 / 380,563,155 | 1.0060 / 1.0009 |
| **whole-file lane, with a declared delta base** | 18,344 obj / 82,033,173 B → 17,196,162 B (4.770×) | **35,904 obj / 285,486,222 B → 16,310,397 B (17.503×)** | |
| **whole-file lane, no base** | 25,804 obj / 266,427,536 B → 93,744,892 B (2.842×) | **8,237 obj / 62,974,073 B → 22,672,881 B (2.778×)** | |
| other lanes (+ headers/directories) | 7,884 obj / 32,460,591 B → 8,953,237 B (3.626×) | 7,581 obj / 32,102,860 B → 7,073,454 B (4.538×) | |

Whole-file lane, on the **44,141 objects present in both Stores with identical canonical bytes**:

| bucket (by v0.1.6 record tag) | objects | canonical | C1 stored | v0.1.6 stored | C1 / v0.1.6 |
|---|--:|--:|--:|--:|--:|
| tag 0 — full, no base | 8,237 | 62,974,073 | 21,842,381 | 22,672,881 | **0.963×** |
| tag 1 — delta | 4,796 | 17,754,920 | 3,692,730 | 3,177,966 | 1.162× |
| tag 2 — delta (chain) | 31,108 | 267,731,302 | 85,405,597 | 13,132,431 | **6.503×** |
| total | 44,141 | 348,460,295 | 110,940,708 | 38,983,278 | **2.846×** |

The 44,141-object join closes exactly: `(21,842,381−22,672,881) + (3,692,730−3,177,966) +
(85,405,597−13,132,431) = 71,957,430 B = 110,940,708 − 38,983,278`. **100.4 % of the whole-file
lane's excess is the tag-2 bucket**; the tag-0 bucket is 3.7 % *cheaper* in C1.

---

## 1. The recorded v0.1.6 totals, with exact source lines

All line references are to this checkout (HEAD `66bce8378`).

| # | source | line | recorded value |
|---|---|---|---|
| R1 | `docs/roadmap/0.1/0.1.6/evidence/issue153-retained-history-report.md` | 26 | `stride-10 | 17 | 17/17 | 49,344,512 / 49,315,940` |
| R2 | same | 27 | `stride-3 | 53 | 53/53 | 64,024,576 / 64,000,100` |
| R3 | same | 28 | `stride-1 (deepseek-full, 157 commits) | 157 | 157/157 | 83,947,520 / 82,677,860` |
| R4 | same | 43 | stride-3 paired canonical `589,423,458 B / 73,476 objects` (both arms, ratio 1.000) |
| R5 | same | 70 | stride-1 canonical `871,588,115 B / 104,705 objects` |
| R6 | same | 86–87 | `Git53 = 49,332,224 B allocated`, `Git157 = 56,373,248 B allocated` |
| R7 | same | 45–46 | control v0.1.5 stride-3 `65,064,960` / `64,036,964` |
| R8 | `docs/roadmap/0.1/0.1.6/evidence/issue151-experiment-ledger.md` | 1774–1777 | ledger `L31` table: the same four rows as R1–R3 + R7 |
| R9 | `benchmark-results/repository-history/stride-10/deepseek-stride10/verification-result.json` | `retained_disk` | `{"allocated_bytes": 49344512, "apparent_bytes": 49315940}` |
| R10 | `…/stride-3/deepseek-stride3/verification-result.json` | `retained_disk` | `{"allocated_bytes": 64024576, "apparent_bytes": 64000100}` |
| R11 | `…/stride-1/deepseek-full/verification-result.json` | `retained_disk` | `{"allocated_bytes": 83947520, "apparent_bytes": 82686052}` |
| R12 | `/Users/yifanxu/layerfs-v016-control/…/stride-3/deepseek-stride3/verification-result.json` | `retained_disk` | `{"allocated_bytes": 65064960, "apparent_bytes": 64036964}` |

**R11 vs R3 — a recording inconsistency, stated.** The report's stride-1 *apparent* (82,677,860)
is the **performance-phase** value; the verification phase's retained apparent is **82,686,052**,
which is what the retained file measures (82,685,952 + 100 B of identity files). The stride-1
*allocated* agrees in both (83,947,520). Every other row agrees between the two phases. This is a
0.0099 % discrepancy in one cell of one row and does not move any conclusion here.

The C1 side of the claim is squad-level context, not re-derived here: allocated 130,863,104 B
"as recorded", apparent 128,864,256 B, canonical 380,921,300 B over 52,032 objects, 17 states.
`/private/tmp/base187/sample.sqlite` measures apparent 128,864,256 B and `st_blocks × 512 =
262,768 × 512 = 134,537,216 B` **today**; the 130,863,104 B figure is not reproducible from this
file now. Flagged, not resolved — it is outside this squad's deliverable.

---

## 2. The v0.1.6 Store is retained; the format decodes

### 2.1 Where the artifacts are

```
benchmark-results/repository-history/stride-10/deepseek-stride10/host-runtime/store.sqlite  49,315,840 B
benchmark-results/repository-history/stride-3/deepseek-stride3/host-runtime/store.sqlite    64,000,000 B
benchmark-results/repository-history/stride-1/deepseek-full/host-runtime/store.sqlite       82,685,952 B
/Users/yifanxu/layerfs-v016-control/benchmark-results/repository-history/stride-3/
    deepseek-stride3/host-runtime/store.sqlite                                              64,036,864 B
```

`sha256(stride-10 store.sqlite) = 80c2b10a7ca1513228306e42063611e2b716be053023b65073d0ab67fbee50af`
— identical to the hash recorded in `verification-manifest.json`
(`deepseek-stride10/host-runtime/store.sqlite`). Same for stride-3
(`62deedb8…`) and stride-1 (`4241e46b…`). **The retained files are the exact artifacts behind
the recorded numbers**, not copies or later re-runs.

### 2.2 The recorded totals reconcile to the byte

| profile | apparent: file + `branch-id` + `layer-id` | recorded | residual |
|---|---|--:|--:|
| stride-10 | 49,315,840 + 34 + 66 = **49,315,940** | 49,315,940 | **0** |
| stride-3 | 64,000,000 + 34 + 66 = **64,000,100** | 64,000,100 | **0** |
| control stride-3 | 64,036,864 + 34 + 66 = **64,036,964** | 64,036,964 | **0** |

| profile | allocated: `(store_blocks + 8 + 8) × 512` | recorded | residual |
|---|---|--:|--:|
| stride-10 | (96,360 + 16) × 512 = **49,344,512** | 49,344,512 | **0** |
| stride-3 | (125,032 + 16) × 512 = **64,024,576** | 64,024,576 | **0** |
| stride-1 | (163,944 + 16) × 512 = **83,947,520** | 83,947,520 | **0** |
| control stride-3 | (127,064 + 16) × 512 = **65,064,960** | 65,064,960 | **0** |

(The recorded "apparent" is the sum of the host-runtime directory's file sizes, identity files
included; the recorded "allocated" is `du -sk` of that directory, which on this filesystem is
exactly `(Σ st_blocks) × 512`. Both reproduce with **zero** residual for four of four profiles on
allocated and three of four on apparent — stride-1 apparent is the R11/R3 discrepancy above.)

### 2.3 Format check before decoding (as instructed)

* Magic is the **same**: `LFPACK\0\0` at offset 0, 16-byte header, version u32 LE at 8, group
  count u32 LE at 12 (`crates/layerfs-layerstack-store/src/objects/pack.rs:11,44`;
  `core/crates/layerfs-storage/src/pack/layout.rs:19,252`). All 357 + 575 + 1,059 + 575 packs in
  the four retained Stores parse with that magic; **0 bad magic**.
* Version sets differ, and this matters: v0.1.6 uses `{1 Legacy, 2 Native, 3 Small,
  4 CompactSmall, 5 Metadata, 6 PooledMetadata}` (`pack.rs:116-144`); C1 uses
  `{1 Ordinary, 2 Native, 4 WholeFile, 6 PooledMetadata, 7 Singleton}`
  (`layout.rs:30-38`). Observed: v0.1.6 `{1:18, 2:24, 4:266, 6:49}`; C1 `{1:17, 2:31, 4:437,
  6:17}`. No version 3 or 5 pack occurs in any retained v0.1.6 Store.
* **Lane 4 is the same lane by content but not by grammar.** In C1, lane 4 is `WholeFile` and the
  delta base is a *column* (`objects.base_object_id`). In v0.1.6, lane 4 is `CompactSmall` and
  the base is *in the record*: `tag u8 [+ 32-byte base ObjectId] + zstd frame`
  (`crates/layerfs-layerstack-store/src/objects/delta.rs:20-32,51-80,81-108`), with the two u32
  length fields dropped at assembly (`assemble_compact_small`, `admission.rs:522-534`).
  Tag 0 = full; tags 1 and 2 = delta (`delta.rs:88`: `(kind == 0) == base.is_none()`).
  Tag 1 is written when the base came from `small_anchor` (the session candidate cache **or**
  `prior_ids[0]`), tag 2 only from `small_predecessor` (the chain format) — both via
  `prior_ids[0]` resolved through `db.object_locations(&predecessors)`
  (`admission.rs:415-421`), i.e. **the committed DB**.
* **Lane-4 ≡ C1's whole-file lane, by object set.** C1's lane-4 objects are exactly its
  `object_role = 1` objects: 44,148 objects / 348,460,709 B. v0.1.6's lane 4 holds 44,141 /
  348,460,295 B. **44,141 of them are the same oids with byte-identical
  `canonical_length`** (0 mismatches); C1 has 7 extra objects totalling 414 B.

### 2.4 Four independent validations of the decode

1. **The decoder reproduces the published C1 bucket table exactly.** Run on
   `/private/tmp/base187/sample.sqlite` it returns whole-file *with* base = 18,344 /
   82,033,173 / 17,196,162 (4.770×) and *without* = 25,804 / 266,427,536 / 93,744,892 (2.842×) —
   identical to the table supplied in the brief, and `119,894,291 − 17,196,162 − 93,744,892 =
   8,953,237`, exactly the brief's "other lanes" row (which is a **residual**: 6,718,154 B of
   other-lane bodies + 215,664 B of headers/directories).
2. **It reproduces v0.1.6's independently recorded canonical totals.** stride-3 decodes to
   589,423,458 B / 73,476 objects = R4 exactly; stride-1 decodes to 871,588,115 B / 104,705
   objects = R5 exactly.
3. **Every lane-4 record has a valid zstd frame at the computed offset.** 44,141 / 44,141 frames
   begin `28 B5 2F FD`; 0 failures. A wrong framing offset would fail this.
4. **Records actually decompress.** With the `zstd` CLI: 4 tag-0 frames decode to exactly
   `canonical_length − 23` bytes, and 4 tag-2 frames decode **using their named base's raw
   payload as a raw-content prefix dictionary** to exactly `canonical_length − 23` bytes — 8/8 OK,
   0 mismatches. Example: `tag 2, canonical 6,846 B, frame 34 B, base canonical 6,844 B → 6,823 B`
   — a 6.8 KB file version stored in **34 bytes** against its predecessor.
   (`canonical_length = raw + 23` is `content::encode_small` + the 13-byte object header:
   `crates/layerfs-content/src/file/content.rs:69-78`.)

---

## 3. The reconstructed per-bucket table — exact, with bounds stated

### 3.1 v0.1.6 `history-stride10` (17 states) — decoded, not inferred

| bucket | objects | canonical B | stored B | ratio |
|---|--:|--:|--:|--:|
| whole-file lane **WITH** a delta base (tags 1+2) | 35,904 | 285,486,222 | 16,310,397 | **17.503×** |
| — tag 1 (`small_anchor` route) | 4,796 | 17,754,920 | 3,177,966 | 5.587× |
| — tag 2 (`small_predecessor` chain route) | 31,108 | 267,731,302 | 13,132,431 | **20.387×** |
| whole-file lane **WITHOUT** a base (tag 0) | 8,237 | 62,974,073 | 22,672,881 | 2.778× |
| other lanes + all headers/directories | 7,581 | 32,102,860 | 7,073,454 | 4.538× |
| **total** | **51,722** | **380,563,155** | **46,056,732** | **8.263×** |

Closure: `16,310,397 + 22,672,881 + 7,073,454 = 46,056,732 = SUM(length(object_packs.data))`;
`46,056,732 + 3,259,108 (non-pack SQLite) = 49,315,840` = the retained file; `+ 100` identity
files `= 49,315,940` = R1/R9.

Lane breakdown: v1 Ordinary 4,847 obj / 2,013,482 B → 678,339 B; v2 Native 1,098 / 22,055,499 →
3,958,057; v4 CompactSmall 44,141 / 348,460,295 → 38,983,278; v6 PooledMetadata 1,636 / 8,033,879
→ 2,240,958. Pack header+directory total 196,100 B.

### 3.2 The bounds — and why they collapse to a point

The brief asked for explicit bounds on the reconstruction. Because the artifact is retained and
hash-matched to the verification manifest (§2.1), **the per-bucket table above is exact: the
bound width is 0 bytes.** Every byte of the recorded 49,315,940 is attributed (§3.1 closure,
residual 0).

Two *interpretive* bounds remain, and both are stated with their support:

* **B1 — the tag→semantics mapping.** Width: at most the 350 same-pack tag-1 records
  (2,885,045 B canonical / 304,124 B stored, **1.86 %** of the delta-stored bytes). Their base is
  in the same pack, so pack ordering alone cannot call them cross-commit; the code can
  (`admission.rs:421` resolves bases from the committed DB), but they are the only records where
  a competing explanation (the per-save session candidate cache) survives. Treating all 350 as
  intra-save changes the "cross-commit" coverage from 35,904 to **35,554 records** and from
  285,486,222 B to **282,601,177 B** canonical — still 81.1 % of the lane's bytes.
* **B2 — the 7 C1-only lane-4 objects.** Width: 7 objects / 414 B canonical / ~0 stored bytes.
  Immaterial at every ratio quoted here.

No other bound is needed. In particular the recorded *allocated* number is not a bound problem:
it reproduces exactly as `(96,360 + 8 + 8) × 512` (§2.2).

---

## 4. FALSIFICATION TEST: is 49.3 MB reachable without cross-commit delta bases?

**Answer: NO. 49,344,512 B is unreachable without cross-commit bases, by a factor of at least
2.46×. The leading hypothesis is confirmed, not killed — and it is no longer a hypothesis, it is a
decoded physical fact.**

### 4.1 H1 — "v0.1.6 declared cross-commit delta bases" — **ACCEPTED (measured)**

| evidence | number |
|---|--:|
| lane-4 records carrying a 32-byte base identity | 35,904 / 44,141 = **81.34 %** by count |
| lane-4 canonical bytes carried by those records | 285,486,222 / 348,460,295 = **81.93 %** |
| tag-2 records (the `small_predecessor` chain route — the **only** producer is `prior_ids[0]` → `db.object_locations`) | 31,108 |
| tag-2 records whose base sits in a **strictly lower pack id** | **31,108 / 31,108 = 100 %** |
| tag-1 records whose base sits in a strictly lower pack id | 4,446 / 4,796 = 92.7 % (350 same-pack) |
| base chains longer than one edge (up to `CHAIN_EDGES = 8`) | 25,213 of 35,904 |

Packs are created in save order and a save can only append to its own last pack, so a base in a
strictly lower pack id was written by an **earlier commit**. Corroborated by the writer:
`admission.rs:415-421` resolves `prior_ids[0]` through `db.object_locations`, i.e. objects
already committed; `read.rs:614-616` enforces `base_location.pack > location.pack ⇒ error`.

### 4.2 H0 — "49.3 MB is reachable without cross-commit bases" — **REJECTED**

The whole-file lane's budget is fixed by the record: `46,056,732 (pack blob) − 6,877,354 (other-lane
bodies) − 196,100 (headers/directories) = 38,983,278 B` for 348,460,295 B of canonical content —
a required **8.9387×**. The best full-compression ratio v0.1.6 actually achieves, measured on its
own 8,237 base-free records, is `62,974,073 / 22,672,881 = 2.7775×`.

| counterfactual | lane-4 bodies | store apparent | vs recorded 49,315,940 |
|---|--:|--:|--:|
| **all-full at v0.1.6's own measured no-base ratio** | 348,460,295 / 2.7775 = **125,457,961** | **135,790,523** | **2.7535×** |
| **hard lower bound**: C1's measured cost of the *same* 44,141 objects (a later-generation compressor, and it *still* uses 18,344 bases) | ≥ **110,940,708** | ≥ **121,273,270** | **≥ 2.4591×** |
| what the record actually is | 38,983,278 | 49,315,940 | 1.000× |

The second row is a genuine lower bound for a zero-base v0.1.6: the encoder only keeps a delta when
`delta.len() + 32 < frame.len()` (`admission.rs:505`), so removing every base can only *increase*
the total, and C1's compressor beats v0.1.6's on the identical tag-0 set (2.883× vs 2.7775×).
Shortfall of the most generous counterfactual: **86,474,683 B**.

### 4.3 The coverage the record requires

Model: a fraction `f` of the lane's canonical bytes delta-encoded at v0.1.6's measured delta ratio
(17.5033×), the rest full at its measured full ratio (2.7775×). Solving
`348,460,295 × [f/17.5033 + (1−f)/2.7775] = 38,983,278`:

**`f = 0.819279 = 81.93 %`.** The measured coverage is `285,486,222 / 348,460,295 = 81.9279 %`.
The model reproduces the recorded store size to the byte, and the required coverage equals the
measured coverage — the recorded total *is* the measured delta coverage, nothing else. With
`f = 0` the same model gives 135.8 MB (§4.2).

### 4.4 What the intra-save route can carry — the bound that matters

C1's own context describes the per-save delta candidate cache as the only non-advisory route. In
v0.1.6 that route is the session `small_candidates` cache (`admission.rs:454-486`), which can
only reach objects written **earlier in the same save**. Empirically it accounts for **at most 350
records / 2,885,045 B canonical / 304,124 B stored = 1.86 %** of the delta-stored bytes. The other
**16,006,273 B (98.14 %)** of delta storage comes from records whose base lives in a strictly
earlier pack. **A per-save cache cannot produce this Store.**

---

## 5. Corroboration at other selections (retained and decoded)

| store | objects / canonical | pack blob | with-base bucket | no-base bucket | other lanes |
|---|---|--:|---|---|---|
| v0.1.6 stride-10 (17) | 51,722 / 380,563,155 | 46,056,732 | 35,904 obj / 285,486,222 B → 16,310,397 (17.503×) | 8,237 / 62,974,073 → 22,672,881 (2.778×) | 7,581 / 32,102,860 → 7,073,454 |
| v0.1.6 stride-3 (53) | 73,476 / 589,423,458 (= R4) | 59,339,891 | 50,799 / 448,573,201 → 22,309,478 (20.107×) | 8,969 / 74,223,414 → 26,361,720 (2.816×) | 13,708 / 66,626,843 → 10,668,693 |
| v0.1.6 stride-1 (157) | 104,705 / 871,588,115 (= R5) | 75,761,473 | 66,049 / 659,506,803 → 30,092,014 (21.916×) | 9,349 / 81,498,166 → 28,647,694 (2.845×) | 29,307 / 130,583,146 → 17,021,765 |
| v0.1.5 control stride-3 | 73,476 / 589,423,458 | 59,358,460 | 50,813 / 448,557,516 → 22,337,476 (20.081×) | 8,955 / 74,239,099 → 26,352,314 (2.817×) | 13,708 / 66,626,843 → 10,668,670 |

The mechanism is stable across selections and generations of the reference line: the no-base
bucket is compressed at **2.78–2.85× in all four Stores** (v0.1.5 control, v0.1.6 at 17/53/157
states, and C1 at 2.842×). The delta bucket's ratio grows with history length
(17.5× → 20.1× → 21.9×) because longer histories find closer predecessors. **The v0.1.5 control
and v0.1.6 stride-3 are byte-identical in canonical content (589,423,458 B / 73,476 objects) and
differ by 0.03 % in pack bytes (59,358,460 vs 59,339,891)** — so nothing here is specific to the
v0.1.6 candidate; it is the reference line's design.

---

## 6. Are the two generations COMPARABLE?

**Yes for the byte question this claim asks, and no for a like-for-like product comparison. Both
statements need to be separated, and the second needs the owner ruling.**

### 6.1 What is comparable — and it is what the claim is about

1. **Same workload.** The v0.1.6 stride-10 receipts sum to `logical_bytes = 561,010,345` over 17
   states, and C1's run receipt says `history_logical_bytes: 561010345`, `history_path_states:
   101,477`, `history_states: 17` — the same three figures the v0.1.6 report's §1 row (R1's line
   26) records. Per-state `changed_bytes` in C1's `trace.jsonl` and per-state `logical_bytes` in
   v0.1.6's `performance-step-*.json` are the same series.
2. **Same whole-file object set.** 44,141 oids common to both Stores with **0**
   `canonical_length` mismatches; C1's extra 7 objects are 414 B.
3. **Same lane-4 canonical bytes**: 348,460,295 B in both (over the common set).
4. **Both are on-disk Store files** measured the same way (apparent = Σ file sizes, allocated =
   `st_blocks × 512`), and both reconcile to their records with zero residual.

So the **2.613× apparent / 2.652× allocated** claim is a same-workload, same-quantity comparison.

### 6.2 What is NOT comparable

1. **The Stores do not hold the same objects.** 52,032 vs 51,722 objects; 6,598 C1-only oids
   (10,341,324 B canonical) and 6,288 v0.1.6-only oids (9,983,179 B). All of that difference is in
   the **metadata lanes**: C1 stores 6,786 objects / 10,405,092 B in v1 Ordinary and **none** in
   v6; v0.1.6 stores 4,847 / 2,013,482 in v1 and **1,636 / 8,033,879 in v6 PooledMetadata**. The
   metadata object identities differ between generations by construction — a 0.09 % canonical
   difference, but a real one. The whole-file lane is unaffected (7 objects / 414 B).
2. **The lane grammars differ** (§2.3): lane 4 is `WholeFile` + a `base_object_id` column in C1
   versus `CompactSmall` + an in-record 32-byte base in v0.1.6. "With a delta base" means the same
   *thing* in both, but it is not the same *field*, and a naive schema-level join between the two
   would fail.
3. **Non-pack SQLite bytes are 2.75× larger in C1** (8,969,965 vs 3,259,108 B), i.e. 7.0 % of
   C1's store versus 6.6 % of v0.1.6's. Part of the 2.65× is schema/index/page overhead, not
   payload. On the recorded allocated pair the 81,518,592 B excess decomposes exactly:
   pack BLOBs 119,894,291 − 46,056,732 = **73,837,559 B (90.6 %)**; non-pack SQLite
   8,969,965 − 3,259,108 = **5,710,857 B (7.0 %)**; allocation slack
   (130,863,104 − 128,864,256) − (49,344,512 − 49,315,840) = 1,998,848 − 28,672 = **1,970,176 B
   (2.4 %)**. Sum: 73,837,559 + 5,710,857 + 1,970,176 = 81,518,592. ✓
4. **The harness generations differ** (containerised `repository_history` runner for v0.1.6,
   host-side `fs-bench-storage-content` for C1). This affects *timing* claims, not byte claims —
   and no timing is reported here.
5. **One recorded cell disagrees with its own artifact** (stride-1 apparent, R3 vs R11, 8,192 B).

### 6.3 What the owner ruling would need to cover

* Whether "byte-identical content" is to be read as **the whole Store** (which is not byte-identical:
  §6.2.1) or as **the whole-file payload lane** (which is: 44,141 oids, 348,460,295 B, 0
  mismatches). The 2.65× claim is true under the second reading and true a fortiori under the first.
* Whether the comparison is **apparent**, **allocated**, or **pack BLOBs** — the three give 2.613×,
  2.652× and 2.603× respectively. All three exceed 2.6×, so no reading rescues the claim.
* Whether a v0.1.6 figure that depends on **in-record cross-commit delta bases** (a design C1 does
  not have in lane 4) is an admissible baseline for a C1 gate, or whether the correct baseline is
  C1's own no-base cost (2.7775× measured) — the choice changes the target from 49.3 MB to
  ~135.8 MB.

**Exit criterion.** `allocated < 49,344,512 B` is **not met** by the retained C1 artifact
(130,863,104 B as recorded; 134,537,216 B by `st_blocks` today). The second branch — "the
generations shown not comparable" — **is not established either**: they are comparable for the
byte question (§6.1) and the gap is real and mechanism-attributed (§0, §4). An owner ruling is
still required, but it is required to decide *which baseline is admissible* (§6.3), not to excuse
an unmeasured difference.

---

## 7. Negative results and hypotheses explicitly ruled out

| # | ruled out | how | number |
|---|---|---|---|
| N1 | "v0.1.6's stride-10 Store is lost" | found at `benchmark-results/repository-history/stride-10/deepseek-stride10/host-runtime/store.sqlite`, sha256 = the verification manifest's | 49,315,840 B |
| N2 | "the two generations store different content" (as the explanation) | oid join on lane 4 | 44,141 common, **0** canonical-length mismatches, 7 C1-only objects / 414 B |
| N3 | "v0.1.6 just compresses better" | measured on the identical tag-0 object set | v0.1.6 2.7775× vs C1 2.883× — **C1 is the better compressor** |
| N4 | "the difference is SQLite/schema overhead" | decoded both | non-pack 3,259,108 vs 8,969,965 B = 7.0 % of the excess |
| N5 | "C1 is uniformly worse" | per-bucket join | C1 is **0.963×** (3.7 % cheaper) on the 8,237 base-free objects |
| N6 | "a per-save candidate cache could have produced 49.3 MB" | pack-ordering census of bases | intra-save route ≤ 350 records / 304,124 B = **1.86 %** of delta storage |
| N7 | "v0.1.6's tag 2 might mean something else" | decompressed tag-2 records with the named base as a raw prefix dictionary | 4/4 reproduce exactly `canonical_length − 23`; 6,846 B version in a **34 B** frame |
| N8 | "the brief's 'other lanes' row is a measured body sum" | arithmetic | it is the **residual** `119,894,291 − 17,196,162 − 93,744,892 = 8,953,237` = 6,718,154 B bodies + 215,664 B headers/directories |
| N9 | "stride-10's per-state receipts give the retained Store" | compared | step 17 records 50,218,540 B (performance phase, sha `ec6846e2…`); the retained file is the verification-phase artifact (sha `80c2b10a…`), 902,700 B smaller |
| N10 | "the recorded v0.1.6 numbers are self-inconsistent" | reconciled four profiles | allocated residual **0** in 4/4; apparent residual 0 in 3/4 (stride-1 off by 8,192 B, §1) |

**Hypotheses that remain open (labelled as such, not measured):**

* **H-A (open).** Whether v0.1.6's *advisory* predecessor stream (`prior_ids`) for this harness is
  the same stream C1 feeds to `cas/save.rs:100`, or a richer one. The Stores cannot answer this —
  the predecessor stream is a caller-side input. What the Stores prove is that v0.1.6 *used* it on
  81.34 % of records and C1 on 41.55 %. Closing this needs the harness-side predecessor
  instrumentation, not a Store decode.
* **H-B (open).** Why C1's base-free set (25,804 objects / 266,427,536 B) is 5,304 objects /
  1,303,766 B larger than v0.1.6's (the ~246 B-average objects that v0.1.6 delta-encodes and C1
  does not). This is a *coverage* question on small files, worth ~1.3 MB of canonical content here;
  it is not the explanation of the 72.3 MB tag-2 gap and must not be reported as one.

---

## 8. Caveats and residuals

* Every number here is **diagnostic**, decoded from retained artifacts. No admission claim.
* No product source was modified; `core/crates/` and `crates/` are untouched. No commits.
* No lane registry, cardinality or golden table was touched. `history-stride1` was not optimised
  or re-run; its Store was decoded read-only.
* The C1 "allocated 130,863,104 B" figure is squad-level context; the retained
  `/private/tmp/base187/sample.sqlite` measures `st_blocks × 512 = 134,537,216 B` today. Not
  resolved here.
* Timing is not reported anywhere in this document. The machine is shared; byte readings are
  load-independent, wall-clock readings would not be.
* The 2.65× headline uses C1 allocated 130,863,104 / v0.1.6 allocated 49,344,512 = 2.6520×. On the
  retained artifacts' apparent bytes it is 2.6130×; on pack BLOBs 2.6032×. All three are reported.

---

## 9. Reproduction

```bash
cd /Users/yifanxu/Ephemeral-AI-Lab/layerfs

# the decoder + per-bucket tables (read-only; no build, no run, ~10 s)
python3 docs/roadmap/0.1/0.1.7/evidence/stage-6-history-187-20260920T000000Z/squad-a/a4_v016_reconstruct.py \
  benchmark-results/repository-history/stride-10/deepseek-stride10/host-runtime/store.sqlite \
  benchmark-results/repository-history/stride-3/deepseek-stride3/host-runtime/store.sqlite \
  benchmark-results/repository-history/stride-1/deepseek-full/host-runtime/store.sqlite \
  /Users/yifanxu/layerfs-v016-control/benchmark-results/repository-history/stride-3/deepseek-stride3/host-runtime/store.sqlite \
  /private/tmp/base187/sample.sqlite

# artifact identity
shasum -a 256 benchmark-results/repository-history/*/deepseek-*/host-runtime/store.sqlite
python3 -c "import json;print(json.load(open('benchmark-results/repository-history/stride-10/deepseek-stride10/verification-result.json'))['retained_disk'])"
stat -f '%z %b' benchmark-results/repository-history/stride-10/deepseek-stride10/host-runtime/store.sqlite
du -sk benchmark-results/repository-history/stride-10/deepseek-stride10/host-runtime
```

No lane was run for this report. `/tmp/base187/sample.sqlite` was read, not produced, here.
