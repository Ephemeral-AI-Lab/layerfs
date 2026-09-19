# V1 — why the WHOLE-FILE lane is 4,890,202 B behind v0.1.6

**Squad V1 · diagnostic only · not admission evidence · no product source touched.**
Source pin `66bce8378` + the working tree as it stood at 2026-09-19T18:00Z. Every number here is
**`measured`** unless it is explicitly labelled `computed`, `est` or `hypothesis`.
No timing claim is made anywhere: byte readings are load-independent, timings are not.

---

## 0. Headline

The whole-file lane is **not** behind because v0.1.6 compresses better, chooses nearer bases, or
declares more of the bases it already has. Measured, per object, on the two retained Stores:

1. **The two lanes hold exactly the same objects.** 44,141 oids, 348,460,295 canonical bytes,
   **0** canonical-length mismatches, 0 objects on either side only. (`measured`)
2. **The codec is byte-identical.** On the 28,956 objects where the two Stores chose the *same*
   base, the stored frames are **byte-for-byte equal** (28,956 / 28,956), and the total stored
   bytes differ by **exactly 0**. On the 7,108 objects both Stores store FULL, the bytes differ by
   **exactly 0**. The zstd frame headers have identical parameter distributions and the source
   parameters are the same six settings in both generations. (`measured`)
3. **v0.1.6's bases are not nearer.** 33,418 of its 34,046 earlier-state bases sit at the
   *immediately previous selected state* — a **checkpoint gap of 10**, not 1. Its base distance
   distribution is the stride. (`measured`)
4. **The gap is one missing candidate class.** 2,594 objects (21,391,409 B canonical) that v0.1.6
   deltas and T1 stores FULL cost **+6,215,622 B**; 94.5 % of them use a **cross-path** base — an
   object stored at an *earlier* state under a *different* path (rename / new path) or a
   same-save similarity candidate. The default T1 declaration never offers that class. (`measured`)
5. **"More candidates is worse" is an ordering result, not a candidate result.** The T1 squad's own
   four-slot arms gained **−8,625,119 B** by delta-ing 3,987 previously-FULL objects and lost
   **+18,214,699 B** by replacing the base on 22,368 objects — 17,141 of them a *same-path* base
   replaced by a *cross-path* one (`select.rs` returns the first eligible, and the ordered list
   puts cross-path first). The untested arm is v0.1.6's own rule: **declare the same-path previous
   version; fall back to a similarity candidate only when there is none.** (`measured` decomposition)

The lane gap decomposes with **residual 0**:

| class | objects | canonical | v0.1.6 stored | T1 stored | delta |
|---|--:|--:|--:|--:|--:|
| delta in v0.1.6, **FULL** in T1 | 2,594 | 21,391,409 | 1,765,661 | 7,981,283 | **+6,215,622** |
| delta in both | 33,310 | 264,094,813 | 14,544,736 | 14,703,982 | +159,246 |
| **FULL** in v0.1.6, delta in T1 | 1,129 | 9,251,514 | 2,844,885 | 1,360,219 | **−1,484,666** |
| FULL in both | 7,108 | 53,722,559 | 19,827,996 | 19,827,996 | **+0** |
| **total** | **44,141** | **348,460,295** | **38,983,278** | **43,873,480** | **+4,890,202** |

`6,215,622 + 159,246 − 1,484,666 + 0 = 4,890,202` — the whole gap, no residual.

---

## 1. Instruments and reproduction

Nothing under `core/crates/` or `crates/` was modified. The harness tree was **not** edited:
`git status --porcelain` is byte-identical to the state this squad started in, and the only new
paths are the seven scripts below and this document.

| instrument | what it does |
|---|---|
| `/tmp/oidtool` (out-of-tree Rust, **not** in the repo) | maps every corpus `(checkpoint, path)` to a layerfs `ObjectId` using the product's own `encode_bytes_object` + `ObjectId::for_bytes` |
| `v1_lib.py` | decodes both pack directories and the objects table into per-object lane records |
| `v1_forensic.py` | the join, the classification matrix, the gap decomposition, the frame-header census |
| `v1_distance.py` | base distance in selected states and in corpus checkpoints |
| `v1_attribution.py` | candidate-class attribution of the 2,594 and the 1,129 |
| `v1_crosspath.py` | what the cross-path bases actually are |
| `v1_arms.py` | per-object decomposition of the T1 squad's own negative arms |
| `v1_mechanism.py` | what replaced what in the four-slot arm, and concrete examples |

```bash
cd /Users/yifanxu/Ephemeral-AI-Lab/layerfs

# 1. the corpus -> ObjectId map (out-of-tree tool; ~40 s, 892 MB hashed once)
cd /tmp/oidtool && CARGO_TARGET_DIR=/tmp/oidtool-target cargo build --release
/tmp/oidtool-target/release/oidtool /Users/yifanxu/Ephemeral-AI-Lab/deepseek-history-data \
  /tmp/oidtool/shas.txt /tmp/oidtool/oids_union.txt /tmp/oidtool/oidmap.txt

# 2. the forensics (read-only; no lane run, no build; ~5 s total)
cd /Users/yifanxu/Ephemeral-AI-Lab/layerfs
E=docs/roadmap/0.1/0.1.7/evidence/stage-6-history-188b-20260920T000000Z/squad-v
python3 $E/v1_forensic.py
python3 $E/v1_distance.py
python3 $E/v1_attribution.py
python3 $E/v1_crosspath.py
python3 $E/v1_arms.py
python3 $E/v1_mechanism.py
```

The corpus→ObjectId tool printed `checkpoints=157 wanted=58313 unique_blobs=75929 rows=763831`
`matches=679097 hashed=75922 no_intro=0 missing_blob=0` — every path of every checkpoint resolved,
and all 44,141 lane-4 oids of both Stores were found (`measured`). **The join is direct**: A4
established that both generations use the same identity function, and this squad's map reproduces
the Stores' own `object_id` values from corpus bytes alone.

Stores read (read-only, `immutable=1`):

| role | path | apparent |
|---|---|--:|
| v0.1.6 (the gate) | `benchmark-results/repository-history/stride-10/deepseek-stride10/host-runtime/store.sqlite` | 49,315,840 |
| T1 default arm (one base) | `/tmp/confirm/sample.sqlite` (identical to `/tmp/r2/sample.sqlite`) | 57,749,504 |
| four slots, measured order | `/tmp/r4/sample.sqlite` | 68,157,440 |
| four slots, depth-ranked | `/tmp/r5/sample.sqlite` | 63,414,272 |

---

## 2. The lane, decoded (`measured`)

| | v0.1.6 | T1 (`/tmp/confirm`) |
|---|--:|--:|
| apparent (`st_size`) | 49,315,840 | 57,749,504 |
| object rows | 51,722 | 52,032 |
| packs (by framing version) | 357 `{1:18, 2:24, 4:266, 6:49}` | 274 `{1:17, 2:31, 4:209, 6:17}` |
| pack BLOBs | 46,056,732 | 52,823,360 |
| pack headers + directories | 196,100 | 211,988 |
| lane bodies | CompactSmall **38,983,278** · Native 3,958,057 · Ordinary 678,339 · Pooled 2,240,958 | WholeFile **43,873,480** · Native 5,695,678 · Ordinary 1,022,795 · Pooled 2,019,419 |
| attributed vs pack BLOB | residual 196,100 (headers/dirs) | residual 211,988 (headers/dirs) |
| **lane-4 objects** | **44,141** | **44,141** |
| **lane-4 canonical** | **348,460,295** | **348,460,295** |

Both Stores' lane-4 object sets are identical to the object: `common 44,141, v0.1.6-only 0, T1-only 0,
canonical_length mismatches 0` (`measured`). This is the same fact A4 reported for the registered
lane, and it holds for the current R0+R1a+R2 lane too — R0, R1a and R2 changed the lane's bytes, not
its object set.

Record grammar is the same shape in both: `tag u8 [+ 32-byte base ObjectId] + zstd frame`, with the
two `u32` compact length fields dropped at assembly (v0.1.6 `CompactSmall`, C1 `WholeFile`,
`WHOLE_FILE_COMPACT_DROP = 8`). v0.1.6 writes tags {0,1,2}; C1 writes {0,1}. The C1
`objects.base_object_id` column and the in-record base agree on **44,141 / 44,141** lane-4 rows and
disagree on **0** (`measured`).

---

## 3. Q1 — objects DELTA in v0.1.6 and FULL in ours

**2,594 objects · 21,391,409 B canonical · v0.1.6 stores them in 1,765,661 B · T1 stores them in
7,981,283 B · delta +6,215,622 B.** (`measured`)

A4's old-lane figure (18,271 objects / 208,015,392 B) was measured on `/tmp/base187`, where our
no-base bucket held 25,804 objects. On the current lane the equivalent bucket is 9,702 objects and
the class is 2,594 — **R0 shrank this class by 88 %**, and what is left is 127 % of the whole gap.

Where v0.1.6's base for those 2,594 sits (corpus join, `measured`):

| base class | objects | canonical | v0.1.6 B | T1 B | delta |
|---|--:|--:|--:|--:|--:|
| earlier state, **cross-path**, gap 10 | 1,455 | 11,840,669 | 1,302,770 | 4,115,496 | +2,812,726 |
| earlier state, **cross-path**, gap 6 | 913 | 5,973,234 | 131,576 | 2,872,610 | +2,741,034 |
| earlier state, cross-path, gap 20 / 30 / 16 | 83 | 1,094,497 | 189,321 | 281,939 | +92,618 |
| earlier state, **same-path**, gap 10 / 6 / 20 | 91 | 2,117,895 | 70,892 | 617,902 | +547,010 |
| base written in the **same save** (cross-path) | 52 | 365,114 | 71,102 | 93,336 | +22,234 |
| **total** | **2,594** | **21,391,409** | **1,765,661** | **7,981,283** | **+6,215,622** |

**2,451 of 2,594 (94.5 %) use a cross-path base**; only 91 use a same-path base we could have
declared and did not (619,902 B of the 6,215,622).

---

## 4. Q2 — objects DELTA in BOTH: are our frames bigger?

Of the 33,310 objects that are delta in both Stores (`measured`):

| subset | objects | canonical | v0.1.6 B | T1 B | delta | ratio |
|---|--:|--:|--:|--:|--:|--:|
| **same base chosen** | 28,956 | 251,434,072 | 12,564,994 | 12,564,994 | **+0** | **1.0000** |
| different base chosen | 4,354 | 12,660,741 | 1,979,742 | 2,138,988 | +159,246 | 1.0804 |

On the same-base subset the frames are **byte-identical**, not merely equal in length:

```
same-base frames byte-identical: 28956 of 28956 (100.00%)
same-base frames equal length:   28956 of 28956 (100.00%)
same-base frame-length delta histogram: [(0, 28956)]
```

That is 28,956 independent samples of the pair (content, base) producing the *same bytes* from two
different generations of the writer. **The gap is not codec or framing.** On the different-base
subset we are 1.0804x — 159,246 B over 4,354 objects, i.e. 36.6 B per object.

Add the 7,108 both-FULL objects (delta exactly 0), and **36,064 of the 44,141 lane-4 objects
(81.7 %) are stored byte-for-byte identically by the two generations.** (`measured`)

---

## 5. Q3 — base distance, both Stores

Distance is measured by joining each record's base to the corpus: the object's save state is the
first selected state in which its content appears, the base's state is the nearest *earlier*
selected state in which the base's content appears. (`measured`; the one-save-per-pack check below
validates the save-state attribution.)

**Pack → state attribution is exact**: 266 / 266 v0.1.6 lane-4 packs and 209 / 209 T1 lane-4 packs
contain objects from exactly **one** selected state (`measured`).

### v0.1.6 — 35,904 delta records

| checkpoint gap | records | stored B | canonical B |
|--:|--:|--:|--:|
| 6 (the last interval, 151→157) | 3,766 | 972,362 | 30,498,711 |
| **10 (one stride)** | **29,652** | **13,604,450** | **244,565,343** |
| 16 | 2 | 19,243 | 103,264 |
| 20 | 473 | 331,975 | 1,751,766 |
| 30 | 153 | 130,126 | 927,825 |
| base at no earlier selected state (same save) | 1,858 | — | — |

Ordinal gap (in the 17 selected states): **1 → 33,418 · 2 → 475 · 3 → 153**.
Base path relationship: **same-path 28,339 (83.24 %) · cross-path 5,707 (16.76 %)**.

### T1 — 34,439 delta records

| checkpoint gap | records | stored B | canonical B |
|--:|--:|--:|--:|
| 6 | 2,225 | 707,137 | 23,917,035 |
| 10 | 26,215 | 11,509,413 | 229,726,558 |
| 20 / 30 / 50 / 106 / 110 | 24 | 14,971 | 103,151 |
| base at no earlier selected state (same save) | 5,975 | — | — |

Ordinal gap: **1 → 28,440 · 2 → 10 · 3 → 9 · 5 → 1 · 11 → 4**.
Path relationship: **same-path 28,464 (99.996 %) · cross-path 1 (0.004 %)**.

### The T1 squad's claim is falsified

> "v0.1.6 ran its producer at every commit, so its base is the NEAREST version while T1's selection
> holds only 17 states."

**Falsified (`measured`).** If v0.1.6's producer had supplied adjacent-checkpoint versions, its base
distance would peak at gap 1. It peaks at **gap 10** (29,652 records) and gap 6 (3,766) — the stride
itself. 33,418 of its 34,046 earlier-state bases (98.2 %) sit at **ordinal gap 1**, the same base our
caller already declares. Two further facts close the door:

* **The Stores contain no intermediate-checkpoint whole-file versions at all.** v0.1.6's lane-4 set
  is exactly 44,141 objects / 348,460,295 B — the union of the 17 selected states' whole-file
  content, byte-for-byte the same set as ours (`measured`). A producer that had run at every commit
  would have left its versions in the Store; it did not.
* **100.00 % of both Stores' bases are objects inside that same 44,141-object set** — 35,904 / 35,904
  for v0.1.6, 34,439 / 34,439 for T1 (`measured`). Every base either generation used is an object
  the other generation also holds. There is no availability difference in the object pool.

So the answer to "what would our lane cost if each object used the nearest available version?" is:
**the same as it costs now.** The nearest available version *is* the previous selected state's
version, and our caller already declares it. Nearness is not the variable.

---

## 6. Q4 — availability or quality?

**Availability of a candidate class, not quality of the bases we offer.** (`measured`)

* On 28,956 of the 33,310 both-delta objects (87 %) the base is the same object and the bytes are
  identical. There is nothing to improve there.
* On the 4,354 objects where the bases differ, we are 159,246 B (1.08x) worse.
* The remaining 6,215,622 B is on objects where **we offer no base at all** and v0.1.6 does. 94.5 %
  of those bases are **cross-path** — the content of a *different* path.

What those cross-path bases are, over all 5,707 of v0.1.6's cross-path earlier-state records
(`measured`):

| shape | records | stored B |
|---|--:|--:|
| neither path exists in the other state (rename signature) | 3,539 | 1,505,102 |
| the object's path is new at this state; the base's path still exists | 1,924 | 1,020,442 |
| the object's path existed at the base state with a different object | 244 | 390,083 |

The contents are near-identical, which is why they pay: the gap-6 cross-path class stores
5,973,234 canonical bytes in 131,576 B (**45.4x**) and the gap-10 class 11,840,669 in 1,302,770
(9.1x). Four concrete examples (`measured`):

```
object 0026d015c7b25ddb  canonical 1481  tag 1  frame 188 B
   version at checkpoint 151: snapshots/web/turn-tail-actions/completed.expected.md
   base    at checkpoint 150: snapshots/web/turn-tail-actions/running.expected.md  (frame 751 B)
object 0037e791ef60bd6e  canonical 719   tag 1  frame 118 B
   version at checkpoint 121: packages/schedule/tool-schedule/tsconfig.json
   base    at checkpoint 120: packages/client/connection/tsconfig.client.json      (frame 364 B)
object 00218e3529373d16  canonical 4065  tag 1  frame 1431 B
   version at checkpoint 121: packages/client/ui-command/tests/browser-plugin.client.spec.ts
   base    at checkpoint 120: packages/client/runtime/tests/invariant.client.spec.ts (frame 1003 B)
object 0020874e5ad0528d  canonical 31133 tag 1  frame 4206 B
   version at checkpoint 51: examples/headless-agent/tests/snapshots/pty-tools/session.jsonl
   base    at checkpoint 45: examples/acp-agent/tests/snapshots/both-mode-turn/tool-schemas.expected.json
```

**And the T1 squad's own negative arms prove the class is worth money.** Decomposed per object
against the default arm (`measured`):

| four slots, measured order | objects | bytes |
|---|--:|--:|
| objects the default stored FULL that the arm delta'd (**the win**) | 3,987 | **−8,625,119** |
| objects the default delta'd that the arm stored FULL | 203 | +652,730 |
| objects whose base the arm **replaced** | 22,368 | **+18,214,699** |
| — of which worse | 18,994 | +19,146,682 |
| — of which better | 3,038 | −931,983 |
| **lane delta** | | **+10,242,310** |

| four slots, depth-ranked | objects | bytes |
|---|--:|--:|
| the win | 3,977 | −8,637,827 |
| dropped | 187 | +614,239 |
| base replaced | 18,643 | +13,508,132 |
| **lane delta** | | **+5,484,544** |

Both arms close exactly to the measured lane totals (54,115,790 and 49,358,024 against the default's
43,873,480). **The candidate class the arm added is worth 8.6 MB. The arm lost 18.2 MB by applying
it to objects that already had a better base.** The path relationship of the replaced bases
(`measured`, heuristic path classification):

```
default=same-path -> arm=cross-path   17141 objects   +18,729,981 B   <- the entire loss
default=none      -> arm=cross-path    4425 objects    -7,765,475 B   <- the win
default=none      -> arm=same-path      213 objects      -932,915 B   <- the win
default=cross-path-> arm=cross-path    4286 objects      -576,008 B
default=same-path -> arm=none           166 objects      +629,532 B
(other classes: 327 objects, +132,000 B)
```

v0.1.6's own selector does the opposite of the arm
(`crates/layerfs-layerstack-store/src/objects/admission.rs:441-486`, read, not measured): it uses the
declared predecessor when there is one and consults the session similarity cache **only when
`anchor.is_none()`** — i.e. only for objects with no predecessor. The T1 default arm declares the
same-path previous version and nothing else (`src/ops/history.rs:1591-1598`), so the missing half is
the fallback.

---

## 7. Q5 — the zstd parameters

**Hypothesis H-W ("v0.1.6's window log is larger, and that alone explains part of the gap") is
FALSIFIED, by source and by measurement.** (`measured`)

Source (read, not inferred):

| parameter | v0.1.6 `CompactSmall` | C1 `WholeFile` |
|---|---|---|
| compression level | 3 | 3 |
| windowLog | **18** (`ContentProfile::Small`, `pack.rs:408-414`) | **18** (`WHOLE_FILE_WINDOW_LOG_SMALL`, `policy.rs:27,166-171`, default 131,072 cutoff) |
| contentSizeFlag | 1 | 1 |
| checksumFlag | 1 | 1 |
| dictIDFlag | 0 | 0 |
| nbWorkers | 0 | 0 |
| prefix | `refPrefix` (base raw payload) | `refPrefix` (base raw payload) |

Frame headers, parsed out of all 44,141 frames in each Store (`measured`):

```
v0.1.6   bad magic 0
         (fcs_flag, single_segment, checksum, dict_id_flag) -> {(1,1,1,0): 43051, (2,1,1,0): 292, (0,1,1,0): 798}
         windowLog histogram: {None: 44141}      FCS == canonical_length-23: 44141 of 44141
T1       bad magic 0
         (fcs_flag, single_segment, checksum, dict_id_flag) -> {(1,1,1,0): 43051, (2,1,1,0): 292, (0,1,1,0): 798}
         windowLog histogram: {None: 44141}      FCS == canonical_length-23: 44141 of 44141
```

The two censuses are **identical, entry for entry**. Every frame in both Stores is
`single_segment = 1`, so no window descriptor is written at all: the decoder derives the window
from the frame content size, and the encoder's `windowLog` is not the binding parameter for payloads
below the 128 KiB cutoff. Checksum is on, dictionary id is 0 in both, and the content size is
recorded exactly as `canonical_length − 23` in 88,282 of 88,282 frames.

The byte-identity result of §4 (36,064 objects stored identically, 28,956 of them frame-for-frame)
is the direct consequence: **the two writers are the same function on the same inputs.**

---

## 8. The gap decomposed (residual 0)

The three terms the brief asked for, plus the fourth that is exactly zero:

| # | term | objects | canonical | bytes |
|---|---|--:|--:|--:|
| 1 | **objects we do not delta at all** (delta in v0.1.6, FULL in T1) | 2,594 | 21,391,409 | **+6,215,622** |
| 2 | **objects both delta where ours are bigger** | 4,354 | 12,660,741 | **+159,246** |
| 2b | objects both delta with the *same* base (not a term: exactly 0) | 28,956 | 251,434,072 | +0 |
| 3 | **objects v0.1.6 does not delta where we do** | 1,129 | 9,251,514 | **−1,484,666** |
| 4 | objects FULL in both | 7,108 | 53,722,559 | **+0** |
| | **residual** | | | **0** |

`6,215,622 + 159,246 − 1,484,666 + 0 = 4,890,202 = 43,873,480 − 38,983,278`. ✓

Term 3 is a real T1 win and it is worth stating: on 1,129 objects our declared same-path base beats
v0.1.6's FULL record by 1,484,666 B. 177 of them use an earlier same-path base; 952 use a base the
**product's own per-save candidate index** supplied (see §10).

---

## 9. What the gap is not

| # | ruled out | how | number |
|---|---|---|---|
| N1 | different object sets | oid join, both directions | 44,141 common, 0 either-only, 0 length mismatches |
| N2 | codec / framing / zstd parameters | same-base frames byte-compared | 28,956 / 28,956 identical; header censuses identical |
| N3 | v0.1.6 compresses FULL records better | both-FULL subset | delta **exactly 0** over 7,108 objects / 53,722,559 B |
| N4 | v0.1.6's bases are nearer versions | corpus base-distance join | gap 10 → 29,652 records; ordinal gap 1 → 33,418 |
| N5 | unselected-checkpoint versions are available to v0.1.6 and not to us | base-membership census | 100.00 % of both Stores' bases are inside the same 44,141-object set |
| N6 | the lane-4 object set moved under R0/R1a/R2 | lane-4 set re-decoded on the current Store | still 44,141 / 348,460,295 |
| N7 | "declaring more candidates is inherently worse" | per-object decomposition of the four-slot arms | win −8,625,119 B, displacement loss +18,214,699 B |
| N8 | `base_object_id` disagrees with the in-record base | full lane-4 cross-check | 0 of 44,141 disagree |

**Hypotheses that remain open, labelled as such:**

* **H-V1 (open, `hypothesis`).** v0.1.6's caller-side predecessor stream (`prior_ids[0]`) is not
  readable from the Stores; A4's H-A stands. What this squad adds is the *shape* of what it resolved
  to: 2,451 of the 2,594 are cross-path, and 3,539 of the 5,707 cross-path earlier-state bases have
  the rename signature. Whether the v0.1.6 harness tracked renames, or supplied a
  previous-commit-version that happened to be stored under another path, cannot be decided from the
  Store.
* **H-V2 (open, `est`).** Whether a fallback-only declaration reproduces the four-slot arm's
  8,625,119 B win without its 18,214,699 B loss. §11 prices it; it is **not** measured.

---

## 10. One correction to the T1 squad's framing

The 188 receipt says "`Candidates` is owned by ONE SAVE … the gap is ACROSS saves, which an in-save
index cannot reach at any size", and reports `no_candidate` = 10,732 as "the R1 opportunity: objects
the caller still offers no base for". The forensics qualify that:

* **The product's per-save index is already doing real work.** 5,975 of our 34,439 lane-4 delta
  records (17.4 %) use a base that was written **in the same save** at a **different path** —
  19,603,783 canonical bytes stored in 3,832,680 B, of which 1,721,204 B is on objects v0.1.6 stored
  FULL (`measured`). The declared bases account for ≈28,464 records (`history.advisory_bases` =
  29,192, the harness's own counter).
* **So the missing half is specifically the cross-save fallback**, not "a caller" in general. v0.1.6's
  `Candidates` is *session*-owned (`schema.rs:77`, `idle_small_candidates`, taken and returned across
  saves), which is why 2,938 of its 4,796 tag-1 records have bases from an earlier selected state.
  R1a widened our per-save index to 32,768 slots and bought 28,672 B — consistent with the index
  being structurally in-save, not with capacity being the binding constraint.
* **The two Stores agree on the class**: v0.1.6's own intra-save route (tag 1, same-save) is only
  1,858 records; ours is 5,975. We are already ahead on the intra-save half. The 6.2 MB is on the
  cross-save half.

---

## 11. The lever this identifies — proposed, not applied

**Proposal (harness-side, one variable, no product line):** in
`core/benchmark/fs-bench-pro-storage-content/src/ops/history.rs:1591-1598`, keep the default's
single same-path predecessor when `previous.is_some()`, and when it is `None` fall back to the
`SimilarityIndex` already built at `history.rs:279-346` — up to
`MAXIMUM_ADVISORY_PREDECESSORS` cross-path candidates. That is v0.1.6's own rule
(`admission.rs:441-486`) and it is **the one arm the two negative results do not cover**. It was
*not* run by this squad: the harness source had been written 25 minutes before this analysis began and
is shared with the squad that owns it, so this document reports the arm rather than taking it.

**Priced from the four-slot arm's own per-object decomposition (`est`, not measured):**

| | bytes |
|---|--:|
| T1 whole-file lane, measured (default) | 43,873,480 |
| the four-slot arm's win on objects the default stored FULL (3,987 objects) | −8,625,119 |
| **whole-file lane under a fallback-only declaration** | **35,248,361** (`est`) |
| T1 Store, measured | 57,749,504 |
| **Store under a fallback-only declaration** | **49,124,385** (`est`) |
| v0.1.6 gate | 49,315,840 |
| T1 target | 47,048,435 |

`est` carries these caveats, stated rather than buried: (a) it assumes the same candidates are
found and accepted for the same objects — the index is the same object and the declaration is the
same list for objects with no predecessor; (b) it assumes no second-order chain-depth interaction,
which the two arms show is real (the measured-order and depth-ranked arms differ by 4,757,766 B on
the same candidate set); (c) it takes the win only and none of the +652,730 B "base dropped" term,
because a fallback-only declaration never removes a declared base. **It is a hypothesis about a run
that has not happened, not a gate claim.** The margin to the gate is 191,455 B, so a run is required
before any claim.

**Product-side change, reported not made.** Nothing in §11 requires a product line: the fallback
candidate list is caller-supplied through the existing `AdvisoryPredecessors` API. If the arm
measures well and the campaign then wants the correspondence to exist without a harness, that is
R1b (a persisted, cross-save index) and it needs **ruling B** — this document does not authorise it,
and no line under `core/crates/` was touched.

**One measurement the campaign should take before acting:** the ordered arms' `ineligible_candidates`
rose 572 → 1,563 (measured order) and 572 → 1,116 (depth-ranked) while `no_candidate` fell
10,732 → 6,897 / 6,892 (`measured`, the harness's own counters). A fallback-only arm should move
`no_candidate` without moving `ineligible_candidates` much; if it does not, the win is being
bought with ineligible probes and the estimate above is optimistic.

---

## 12. Labels, limits, and what was not done

* Every number in this document is **diagnostic**, decoded from retained artifacts. No admission
  claim, no gate claim, no timing claim.
* `measured`: §2, §3, §4, §5, §6 (except the pricing), §7, §8, §9, §10, and the arm decompositions
  in §6. `computed`: the closure identities (arithmetic over measured terms). `est`: §11's two
  projected totals. `hypothesis`: H-V1, H-V2, H-W (falsified).
* **No lane was run.** `history-stride10`, `history-stride3` and `history-stride1` were not executed;
  the four Stores above were read, not produced, by this squad. The 217-row lane, the lane registry
  and every golden table are untouched. `history-stride1` was not opened. No stride-3 confirmation
  exists, so nothing here supports a gate claim.
* **No product or harness source was modified.** `git status --porcelain` after this squad's work is
  identical to before it; the only new paths are the seven scripts and this file under
  `docs/roadmap/0.1/0.1.7/evidence/stage-6-history-188b-20260920T000000Z/squad-v/`.
* The corpus→ObjectId tool lives **outside the repository** at `/tmp/oidtool` and path-depends on
  `core/crates/layerfs-content`. It was built with `CARGO_TARGET_DIR=/tmp/oidtool-target`, so the
  harness's own target directory and the T1 squad's built binary are untouched.
* The "same-path / cross-path" classification in §3, §5 and §6 is a **path-set intersection at the
  two states**; a content that appears at several paths at once can be classified conservatively.
  The base-presence counts (base `None` vs not) are exact; the path labels are the heuristic part.
  The 213-object "default=none → arm=same-path" class in §6 is a known artefact of that heuristic
  (a same-save base has no earlier state to compare against and is labelled `none`).
* The whole-file lane is **58.0 %** of the remaining 8,433,664 B gap. This document prices the lane;
  it does not touch the objects table (17.5 %), the native lane (20.6 %) or pack framing (1.4 %).
