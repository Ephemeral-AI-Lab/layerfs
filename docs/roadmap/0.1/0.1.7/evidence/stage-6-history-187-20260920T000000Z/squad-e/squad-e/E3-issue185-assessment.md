# E3 — does #185 help #187, and should it be un-deferred?

**Squad E, task E3.** Every number below is `diagnostic`, not admission evidence.
No timing is reported: the machine may be busy and any wall-clock reading here
would be invalid. Nothing under `core/crates/` or `crates/` was modified; the
217-row benchmark lane and `history-stride1` were not touched.

**Instruments used.** `shared/space.py` for the pack container
(`pack_directory`, `whole_file_records`, `pack_bodies`, `canonical_by_role`,
`stat_space`, `sqlite_space`); `shared/history_corpus.py` for corpus identity and
selection. Two local scripts add only what those do not read:
`e3_population.py` (oracle path-state population) and `e3_native_lane.py` (the
*record* grammar inside a Native group body, which `space.py` deliberately does
not parse).

## 0. Identity of everything measured

| Object | Identity |
| --- | --- |
| Corpus | `/Users/yifanxu/Ephemeral-AI-Lab/deepseek-history-data`, manifest sha256 `03f21acf…4271`, tip `b0a7d2ce…7ed`, 157 checkpoints |
| Lane | `history-stride10` = `{1,11,…,151,157}`, 17 states |
| v0.1.7 Store under analysis | `/tmp/base187/sample.sqlite`, apparent **128,864,256 B**, allocated 134,537,216 B, page_size 4096, page_count 31,461, freelist 28, schema_version 6 |
| v0.1.7 faithful model | `/tmp/s0_l7/sample.sqlite`, apparent **63,737,856 B** (`history.advisory_model = 1`, `advisory_bases = 28,491`, `advisory_depth_limit = 7`) |
| v0.1.6 reference Store | `benchmark-results/repository-history/stride-10/deepseek-stride10/host-runtime/store.sqlite`, sha256 `80c2b10a7ca1513228306e42063611e2b716be053023b65073d0ab67fbee50af`, apparent **49,315,840 B**, allocated 49,336,320 B, schema_version 16 |
| Cutoff | `T = DEFAULT_SMALL_FILE_THRESHOLD_BYTES = 131_072` (`core/crates/layerfs-content/src/policy.rs:14`); `logical_len < T` ⇒ WholeFile, else Chunked (`policy.rs:123`) |

Both Stores named in the brief exist and were read; neither was missing.

Reproduction:

```sh
cd core/benchmark/fs-bench-pro-storage-content/shared && python3 -c "import space; print(space.self_check())"
E=docs/roadmap/0.1/0.1.7/evidence/stage-6-history-187-20260920T000000Z/squad-e/squad-e
python3 $E/e3_population.py --json $E/e3_population.json
python3 $E/e3_native_lane.py /tmp/base187/sample.sqlite \
  benchmark-results/repository-history/stride-10/deepseek-stride10/host-runtime/store.sqlite
python3 $E/e3_crossing_objects.py /tmp/base187/sample.sqlite
```

---

## 1. The two halves, separated — reading confirmed, with one correction

The paper's own claim ledger settles the labels
(`core/docs/architecture/deferred/01-size-transition-delta-hints.md` §3):

| # | Claim | Label |
| --- | --- | --- |
| 11 | A usable cross-role base at the transition | **deferred** |
| 12 | Core should consult the cursor per emitted chunk | **proposed** |

and §1: *"State 1 is the subject of the positional-hint proposal (chapter 14);
state 3 is the part this paper records as **deferred**."* §4 step 3 gates only
the cross-role half on an owner amendment — *"For the cross-role half only: an
owner amendment is required first"* — while step 1 (implement the cursor) has no
such gate. **The brief's reading is CONFIRMED.**

**Correction — the cursor half is not a transition half at all.** The paper's
title and §2 diagrams place both halves under "across the whole-file / chunked
size transition", but chapter 14 §14.2 is explicit that the gap is the
**large → large in-place overwrite**:

> For an **in-place overwrite**, the useful base is *the previous version of this
> exact region*. Core offers the region's left neighbour instead.

The mechanism confirms it: the cursor walks *a chunked base's* extent tree, so it
can only fire when the base is chunked and the result is chunked. At
`small → large` the base is one whole-file object (`set_physical_predecessor`
returns before attaching the cursor, `crates/layerfs-layerstack-store/src/objects.rs:3319-3324`);
at `large → small` the result is one whole-file object and no chunk is emitted.
So the two halves have **disjoint populations**, and only the cross-role half
lives at the transition.

Measured confirmation that the two populations are disjoint and that the cursor
half is same-role only — see §2.3: **650 of 650** v0.1.6 chunk PREFIX records have
a base that is itself a Native-lane (chunk) object; zero have a whole-file base.

---

## 2. Population each half would touch, measured on the corpus

### 2.1 Chunked population — the "92 chunked files" claim is CORRECTED

Over the 17 selected states, regular files only (mode `100644`/`100755`;
directories `40755` and symlinks `120000` excluded — 15,413 and 135 path-states
respectively, so the exclusion is visible):

| Quantity | Value |
| --- | ---: |
| oracle path-states over 17 states | **101,477** (= the frozen pin, reproduced) |
| regular-file path-states | 85,929 |
| chunked path-states (`size ≥ T`) over 17 states | **105** |
| chunked path-states over states 1…151 (i.e. excluding state 157) | **92** |
| **distinct files ever chunked** | **27** |
| distinct chunked *contents* (path, sha256) | **91** |
| distinct chunk contents (sha256) | **91** |
| chunked path-states unchanged from the previous selected state | 14 |
| chunked path-states that are a change | 91 (= 63 + 17 + 11, below) |

**"92 chunked files" is not a file count.** 92 is exactly the number of chunked
*path-states* in states 1…151, i.e. the 17-state selection with the last state
excluded; the distinct-file count is **27**. *Labelled HYPOTHESIS (H-E3-3):* that
path-state count is the origin of the figure — it is the only corpus count that
equals 92. The Store separately holds **92 `file-state` objects and 92
`extent-leaf` objects** (`canonical_by_role`) against 91 distinct chunked
contents; the +1 there is **unresolved** (see §6).

The brief's store numbers are **CONFIRMED exactly**, twice, by two independent
readings:

| Native (chunk) lane, v0.1.7 | `space.py` | record walk |
| --- | ---: | ---: |
| objects / canonical | **1,098 / 22,055,499** | 1,098 records |
| record bytes | — | 5,690,762 |
| group framing (4 + 4n) | — | 4,916 |
| lane bodies | **5,695,678** | 5,690,762 + 4,916 = 5,695,678 |
| tags | — | `{0 (FULL): 1,098}` — **zero PREFIX** |

`objects WHERE object_role = 2 AND base_object_id IS NOT NULL` = **0 of 1,098**.
The chunk lane's share of the Store is 5,695,678 / 119,678,627 pack bodies =
**4.76 %**, or 4.42 % of the 128,864,256 B apparent size.

### 2.2 The cross-role half's population — 24 crossing events

Between consecutive selected states, same path, regular file in both, and the
`size ≥ T` predicate changing sign:

| Direction | Events | Distinct paths | Result bytes | Base bytes |
| --- | ---: | ---: | ---: | ---: |
| small → large (result is chunked) | **17** | 16 | **2,523,964** | 1,950,129 |
| large → small (result is one whole-file object) | **7** | 6 | **478,194** | 1,002,427 |
| **total** | **24** | **22** | **3,002,158** | 2,952,556 |

The 7 large→small crossings (decoded paths, with the stored cost of each result
object measured in the v0.1.7 Store by exact `canonical_length = result + 23`):

| from → to | result B | objects at that canonical length | stored B | path |
| --- | ---: | ---: | ---: | --- |
| 31 → 41 | 31,857 | 1 | 2,605 | `examples/acp-agent/tests/snapshots/hook-cc-posttool-block/session.jsonl` |
| 51 → 61 | 62,693 | 1 | 19,173 | `packages/ui/tui/src/index.ts` |
| 111 → 121 | 129,985 | 1 | 50,750 | `scripts/snapshots/translation-prompt-v4/request-response.expected.json` |
| 131 → 141 | 37,715 | 1 | 16,069 | `scripts/snapshots/translation-prompt-v4/request-response.expected.json` |
| 131 → 141 | 19,878 | 1 | 6,000 | `packages/host/apiproxy/src/api-proxy.ts` |
| 141 → 151 | 98,045 | 1 | 12,848 | `docs/module-graph.md` |
| 141 → 151 | 98,021 | 1 | 13,045 | `docs/module-graph.zh.md` |
| **total** | **478,194** | 7 | **120,490** | (3.9687x) |

Each result is a **unique** whole-file object (exactly one object at each
canonical length), so this is an exact attribution, not a size-based guess.

**Negative result, and it matters:** `SELECT o.object_role, b.object_role, COUNT(*)
FROM objects o JOIN objects b ON b.object_id = o.base_object_id` returns exactly
two rows — `(1,1,18,344)` and `(6,6,917)`. **The Store contains zero cross-role
deltas of any kind.** Neither half has ever fired in `core/`.

### 2.3 The cursor half's population and reach — 650 of 1,098, all same-role

Chunked path-states by how they were produced:

| Class | Path-states | Result bytes |
| --- | ---: | ---: |
| in-place chunked edit (both states chunked, content differs) | **63** | **18,274,423** |
| small → large crossing | 17 | 2,523,964 |
| first appearance / new file (incl. state 1) | 11 | 3,693,614 |
| unchanged (exact CAS reuse, no new object) | 14 | — |
| **total chunked path-states** | **105** | |

Only the 63 in-place edits have a chunked base, so only they are reachable by the
cursor. Decoding the v0.1.6 reference Store's Native lane with the same record
walker:

| Native lane | v0.1.7 | v0.1.6 |
| --- | ---: | ---: |
| records | 1,098 | 1,098 |
| raw bytes | 22,032,441 | 22,032,441 |
| FULL (tag 0) records / raw / stored | 1,098 / 22,032,441 / 5,690,762 | 448 / 8,604,206 / 2,378,527 |
| **PREFIX (tag 1) records / raw / stored** | **0** | **650 / 13,428,235 / 1,574,526** |
| group framing | 4,916 | 5,004 |
| **lane bodies** | **5,695,678** | **3,958,057** |
| objects-table cross-check (records in body vs rows) | 131/131 groups | 153/153 groups |

`prefix_base_lane = {"native": 650}` — **every one of the 650 PREFIX records has a
base that is itself a Native-lane object.** The cross-role half contributed
**0 B** to this lane.

### 2.4 A1's "650 chunk PREFIX records worth 1,737,621 B" — VERIFIED, with a units correction

A1's §4.4 table is reproduced **exactly** (650 / 13,428,235 / 1,574,526; 448 /
8,604,206 / 2,378,527; lane bodies 3,958,057 vs 5,695,678). One correction to the
brief's phrasing:

* the 650 PREFIX records' **own** stored bytes are **1,574,526**;
* **1,737,621 B** is the whole **Native-lane body** difference
  (5,695,678 − 3,958,057), which is what restoring those 650 records would
  recover — it is larger than 1,574,526 because the 448 records that stayed FULL
  also cost more than the 650 they displaced.

The decomposition is exact: record bytes 5,690,762 − 3,953,053 = **1,737,709**;
group framing 5,004 − 4,916 = **+88**; lane bodies 1,737,709 − 88 = **1,737,621**,
residual 0.

---

## 3. Bound on the saving from each half

Model inputs are the brief's measured dictionary effect, restated on this corpus
as frame ratios (headers excluded, so the arithmetic is comparable):

* v0.1.7 chunk FULL frames: raw 22,032,441 / stored 5,685,272 = **3.8754x**.
  This is the ratio measured *on the chunk lane*; B3's 2.9761x "compressed alone"
  and 23.4157x "with the previous version as a raw prefix dictionary" are
  whole-file-lane figures and are **not** substituted here.
* v0.1.6 chunk PREFIX frames: raw 13,428,235 / stored 1,550,476 = **8.6607x**.
  This is a *measured* selection on the *same 1,098 records with the same raw
  bytes*, not an extrapolation from the 23.416x whole-file dictionary figure.

### 3.1 Cursor half (same-role Chunk → Chunk) — **1,737,621 B**

```
cursor half bound = v0.1.6 lane bodies − v0.1.7 lane bodies
                  = 3,958,057 − 5,695,678
                  = 1,737,621 B   (= 2.1844 % of the 79,548,416 B gap)
```

**This is the bound, not a floor**, and the reason is structural, not
conservative: the 448 records that stayed FULL split into ~310 produced by the
28 path-states that have **no chunked base at all** (17 crossings + 11 first
appearances, 6,217,578 B of result bytes ÷ 20,066 B average chunk ≈ 310 —
*modelled*) and ~138 in-place chunks whose trial lost. v0.1.6 *already had the
cursor*, so those ~138 lost with the cursor present; a cursor implemented exactly
as chapter 14 proposes would not recover them either.

*Labelled HYPOTHESIS (H-E3-1):* the ~310/~138 split. It is a division by the
measured average chunk size, not a per-object attribution. The **1,737,621 B**
figure does not depend on it.

### 3.2 Cross-role half — ceiling **476,317 B**

*Large → small* is **measured**, not modelled: the seven result objects cost
**120,490 B** in total. Even a delta that made them free saves at most that.

*Small → large* is a **ceiling**, because the 17 crossing results are not
individually identifiable in the Store:

```
result bytes                     R = 2,523,964 B   (17 crossings)
chunks                           k = R / 20,066 ≈ 126
FULL   stored = R/3.8754 + 5k  = 651,916 B
PREFIX stored = R/8.6607 + 37k = 296,089 B
saving ceiling = 651,916 − 296,089 = 355,827 B
```

```
cross-role ceiling = 120,490 + 355,827 = 476,317 B  (= 0.5988 % of the gap)
```

**Assumptions, all of which favour the saving:** every crossing result byte lands
in a fresh chunk object (no exact CAS reuse); the whole-file base yields the same
prefix ratio as v0.1.6's *chunk-to-chunk* bases; one base object suffices. The
third is the weakest: at `large → small` the old content is spread over many
chunk objects plus a mapping tree, and the delta format admits **one** base used
as a raw prefix dictionary — the paper's G6 calls this case *"plausible but
unmeasured"*, and it cannot reproduce the issue100 example (133,273 → 129,991,
Git 5,492 B depth-one delta), which is a delta *stream* against the whole old
blob, not a prefix dictionary against one chunk.

### 3.3 Both halves together

```
1,737,621 + 476,317 = 2,213,938 B = 2.7831 % of the 79,548,416 B gap
```

---

## 4. Does #185 touch cause 1? — **No. Plainly no.**

Cause 1 is **65,126,400 B (81.87 %)** of the gap: *the driver declared NO
cross-commit delta base*. The measured shape of it:

* whole-file lane: 44,148 objects, 348,460,709 canonical, **110,941,054 stored**;
* 18,344 of them carry a base, and **all 18,344 bases are role 1 (WholeFile)** —
  same-role, from the per-save candidate cache;
* the harness's own faithful model fixes it by declaring the previous version's
  content root, and `history.rs:628-632` says why the unmodified driver does not:
  *"This driver instead calls `construct_bytes`, which builds
  `FinalizedObject::new(role, canonical)` with **no predecessors at all**."*

That pair is **WholeFile → WholeFile and same-role**. #185 supplies no such pair:

* the **cursor half** walks a *chunked* base and feeds `select()`'s
  `ObjectRole::Chunk` arm only (`select.rs:247`); a whole-file target takes the
  `_ =>` arm at `:254`. The cursor can never name a base for a whole-file
  object.
* the **cross-role half** admits `FileState ↔ WholeFile`. A `FileState` is not a
  whole-file object; admitting it changes which *role mismatch* is tolerated, not
  which *cross-save base exists*.

So: **#185 touches cause 1 for 0 B.** Its entire reachable population is
2,213,938 B, and it is disjoint from the 65,126,400 B. The handoff's warning is
confirmed in both directions: the two are different axes and neither substitutes
for the other.

---

## 5. Ruling — recommendation per half

### Cursor half — **NO, do not promote it for #187's sake.**

The action is not "un-defer": it is *not deferred*, so nothing needs lifting. It
is **proposed**, and the available action is *implement it*. The arithmetic says
implementing it does not move #187:

| | bytes | % of the 79,548,416 B gap |
| --- | ---: | ---: |
| measured (v0.1.6 reference selection on the same 1,098 records) | 1,737,621 | **2.1844 %** |
| #187 cause 1, untouched by it | 65,126,400 | 81.87 % |

It is a legitimate change on its own merits — chapter 14 §14.5 bounds it
(forward-only span order, 4,096-descriptor ceiling, exhaustion is not an error,
one hint, unchanged depth eligibility), §14.4 shows every consumer already exists
so the change is one producer, and §4 step 1 requires no storage-format change.
But it should be justified by §14.6's own four cases (incompressible in-place,
compressible in-place control, append-only, dissimilar rewrite), **none of which
is this corpus**, and it must be measured against the read-side cost §14.7 states
honestly (`O(d+1)` reads per chained chunk, `d ≤ 4`).

**Do not spend #187's budget on it, and do not cite it as an #187 remedy.**

### Cross-role half — **NO, do not un-defer it.**

| | bytes | % of gap |
| --- | ---: | ---: |
| large → small, **measured** stored cost of the entire population | 120,490 | 0.1515 % |
| small → large, modelled ceiling | ≤ 355,827 | ≤ 0.4473 % |
| total ceiling | ≤ 476,317 | ≤ 0.5988 % |

An owner amendment is required before any of it (paper §4 step 3), and the
amendment must declare candidate count (one), trial count (one), role pair and
depth/closure/encoded budgets. **0.60 % of the gap, with the large→small half
measuring 120,490 B in total, does not buy an amendment** — and the honest
framing is worse than the ceiling: the paper's own C improvement *"is plausible
but unmeasured"*, and one base object cannot express "the whole old chunked
content", which is what the crossing actually needs.

### On the systemic defect the brief raises

The brief is right that the *class* is systemic, and this task adds one measured
instance on each side of the boundary:

* **producer declares nothing** — `build.rs:326` `push_chunk(chunk, None, …)` on
  the `small → large` route (re-read at the pin); the harness driver
  `construct_bytes` with no predecessors (`history.rs:628`);
* **producer declares the wrong thing** — `replace_chunked` offers
  `rightmost_payload(left)`, one id shared by every chunk (chapter 14 §14.2);
* **consumer discards what is declared** — `select()`'s `_ =>` arm probes the
  advisory and the tree roles short-circuit it (§13.6: `UnchangedPrefix` is set
  and never consumed).

But the systemic *class* is not the systemic *cost*. The measured cost sits
overwhelmingly in the whole-file lane (110,941,054 of 119,678,627 pack bodies =
**92.7 %**), and #185's two halves address 2.78 % of the gap between them.
Fixing the class where it is expensive — a same-role, cross-save whole-file
base — is a **different** change from #185, and it is the one the A/B already
priced at 65,126,400 B.

### What evidence is missing

1. **No core-side A/B of the cursor exists.** 1,737,621 B is v0.1.6's *reference*
   selection on the same 1,098 records with byte-identical raw payloads
   (22,032,441 both sides) — strong, but it is not a `core/` measurement. The
   product-side number needs the producer implemented.
2. **The chunk lane's own delta counters were not re-read.** `/tmp/base187/trace.jsonl`
   carries no `delta.*` counters (the `s0_l7` trace does). The brief's
   `no_candidate = 26,847` etc. are whole-Store figures; the chunk lane's
   `trials`/`full_losses` split is **not measured here**.
3. **The small → large ceiling is modelled, not measured per crossing** (§3.2).
   It needs the crossing results identified object-by-object, which needs the
   mapping tree decoded.
4. **The +1 discrepancy (92 file-states / 92 extent-leaves vs 91 distinct chunked
   contents) is unexplained.** *Labelled HYPOTHESIS (H-E3-2):* one content has two
   mappings because `replace_chunked` (splice) and `stream_combined` (full
   re-chunk) can emit different extent layouts for the same bytes. Not tested.
5. **No timing.** Per the campaign rules, none may be reported from this machine.
6. **`s0_l7`'s `delta.ineligible_candidates = 22`** is consistent with the 24
   crossing events / 22 distinct crossing paths measured here, but identity was
   **not** proven — the counter also admits depth rejections. Reported as
   corroboration only.

---

## 6. Negative results, kept

* **Zero cross-role deltas exist in either Store.** `(target_role, base_role)` over
  all based objects is `(1,1,18,344)` and `(6,6,917)`; nothing else.
* **Zero chunk deltas in v0.1.7**: 0 of 1,098 chunk objects carry a
  `base_object_id`, and all 1,098 Native records are tag 0.
* **Grouping is not the answer and neither is a trained dictionary** — carried
  from B3 and not re-derived; the raw prefix dictionary is 7.87x where a trained
  dictionary is 1.14–1.17x.
* **The "92 chunked files" figure does not survive contact with the corpus.** 27
  distinct files; 105 chunked path-states; 91 distinct chunked contents.
* **The cursor half cannot reach 42 of the 105 chunked path-states at all**,
  because their base is a whole-file object or does not exist. Any claim that
  the cursor "would fix the chunk lane" is false for 40 % of it.
* **#185 does not touch cause 1 for 0 B** — restated as a negative because the
  temptation to cite it there is the main risk this assessment closes.

## 7. Answer in one line each

1. Cursor half — **proposed, not deferred**; reading confirmed; correction: it is
   a **large → large** mechanism, not a transition mechanism.
2. Population — **27** distinct chunked files / **105** chunked path-states /
   **1,098** Native objects (22,055,499 canonical / 5,695,678 stored, verified);
   **24** crossings / **22** paths / 3,002,158 result bytes; **650** of 1,098
   chunk records reachable, **650 of 650** via a same-role chunk base.
3. Bound — cursor **1,737,621 B** (measured); cross-role **≤ 476,317 B** (of which
   120,490 B measured); both **2,213,938 B = 2.78 %** of the gap.
4. Cause 1 — **not touched, in either half, for 0 B.**
5. Ruling — **NO** to promoting the cursor half for #187; **NO** to un-deferring
   the cross-role half.
