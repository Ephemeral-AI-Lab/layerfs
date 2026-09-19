# T2, T3 and the floor — what they are, and what they cost to build

**Basis: apparent bytes, `history-stride10`, 17 states.** `[measured]` / `[computed]` / `[est]` as marked.
No timing anywhere.

---

## 1. What each tier physically IS

| tier | apparent | what it *is*, physically | how the number was built |
| --- | --: | --- | --- |
| **T1** | **47,048,435** | Per-object records, **one record per pack group** — today's grammar, unchanged. Whole-file objects carry a cross-path delta base; the SQLite row grammar is lean. | `[computed]` from three measured parts: l7 (measured), B2's R4 lane (measured with the product's own byte-exact codec), v0.1.6's non-pack (measured) |
| **T2** | ~44,374,000 | T1, but the records that *still* have no base are **grouped** — several whole-file records packed into one ~256 KiB zstd frame instead of one frame each. | `[est]` — T1 plus B3's **cap256KiB** ratio 1.1875× applied to the 8,741 objects R4 leaves without a base |
| **T3** | ~31,100,000 | A **different arrangement**: versions of the same path stored as **one per-path chain in a single large frame**, zstd `-19 --long=30`, 254 frames, mean 1,464,553 B, max 11,263,931 B decoded per read. | B1's P1 content 27,184,431 B `[measured]` × 1.0242 + lean non-pack 3,259,108 `[composed]` |
| **floor** | ~21,382,000 | **One zstd `-22 --ultra --long=30` stream over the entire 371,937,306 B union.** A read decodes 372 MB. | B1's L 17,695,928 B `[measured]` × 1.0242 + lean non-pack `[composed]` |

**Only T1 is a target. T2 is a trade. T3 is a different design. The floor is a bound, not a design.**

---

## 2. Complexity, with the touch points

### T1 — three separable pieces, wildly different costs

| piece | what changes | format change? | rough size | blocker |
| --- | --- | --- | --- | --- |
| **R0** declare the base | nothing in core — the **caller** supplies the ids | no | **0 production LOC** | In production the caller is **Stage 7 / Workspace, which has no specification** |
| **R1a** index **capacity** | `SLOTS = 1024`, `REFERENCES = 8192`, `INDEX_BYTES = 128 KiB` in `encoding/delta/candidates.rs:14-16` | **no** | **2 constants** | The module docstring asserts the fixed size as a *design property* ("no growth with the number of admitted objects"), so it is a design change, not a typo fix. Memory cost is per-save. |
| **R1b** index **lifetime** | move the index off the save owner: `cas/lifecycle.rs:90` (`Candidates::new`) → `cas/store.rs:392` (owner dropped) | **no** if in-memory across saves; **yes** if it must survive a process restart | moderate, vs **150–300 LOC + new table + SCHEMA_VERSION 4→5** for the persisted form | The persisted form needs an owner amendment and contradicts C2's stated contract |
| **R2** lean row grammar | `objects` 67.5 B/row + the two indexes `objects_locations` (49.0) and `objects_bases` (86.0) | **yes** — schema change, SCHEMA_VERSION bump | **high** — touches the schema, the location lookup and the read path | Owner amendment. v0.1.6 carries the base id **inside the record**, so it needs neither index — the precedent exists |
| **R3** depth measurement | `DepthCache::cost_of`, `select.rs:94-144` — carry a `u8` offset out of the loop | **no** | **~4 lines, one function** | Owner ruling (product source). **Buys legality, not bytes (~2,248 B)** |

**R3 is by far the cheapest and is a prerequisite for trusting anything else.** R1a is the cheapest
thing that moves real bytes.

### T2 — grouping: no new concept, but a real trade and a selective rule

* **The machinery exists.** Ordinary / Native / PooledMetadata already run `GroupCodec::Zstandard` with
  **one frame per group**; multi-record groups are not a new concept.
* **The touch point is one match arm.** `cas/selection.rs:54-55` seals a whole-file group the moment it
  is occupied — `PackLane::WholeFile | PooledMetadata | Singleton => occupied`. Grouping whole-file
  records means changing that arm and revisiting `GROUP_TARGET` and the per-lane body limits.
* **It must be selective.** B3 measured grouping **2.95 % WORSE** on the 18,344 records that already
  carry a prefix dictionary. So it applies only to the records with no base — a per-record decision or
  a second lane, not a global switch.
* **It costs read amplification: 59.8×** for a single-record read at the 256 KiB cap (median group
  255,391 B, median 29 records).
* **Complexity: moderate.** No new concept, no new format version, one policy arm — but it needs an
  owner ruling on the access cost.
* **The number's weak input:** B3 measured 1.1875× on the **current** no-base population
  (25,804 objects / 266,427,536 B). T2 applies it to the **post-R4** population
  (8,741 objects / 48,131,288 B) — a different size mix. **That single borrowed ratio is why T2 is `[est]`.**

### T3 — per-path chains in large frames: a different design, not a tuning change

* **It needs a concept the Store does not have: a path.** `layerfs-storage`'s contract explicitly denies
  a Workspace/history entity, and there is no path anywhere in the schema.
* **It needs a new physical layout** — a path's versions contiguous, 254 frames of ~1.5 MB mean.
* **It breaks an invariant the whole-file lane relies on:** one record per group is what makes
  per-object stored-size attribution exact (`space.whole_file_records`). At 29 records per group that
  attribution is gone.
* **Codec cost:** level 19 with a 1 GiB window — a memory decision, not just a parameter.
* **Complexity: high.** New key, format change, memory change, access-cost change. **Not a candidate for
  this campaign.**

### floor — not achievable, and not a design

One stream; **a read decodes 372 MB**. It is a lower bound on the information in the corpus, useful for
sanity-checking other numbers and for nothing else. Note that at 21,382,474 B the non-pack overhead
alone would be **15 %** of the Store — the overhead stops being a detail at that scale.

---

## 3. A coupling I had not stated: R1 makes R2 bigger

The non-pack cost is **not independent of delta coverage** — `objects_bases` is an index on
`base_object_id`, so every new delta adds a row to it:

````
                        base rows      non-pack
  base187 (today)          18,344     8,969,965
  l7 (faithful)            34,300    10,277,718
                          +15,956    +1,307,753
  => 82.0 B of non-pack per extra base row
     (dbstat attributes 86.0 B/base-row to objects_bases -- the same quantity)

  at full coverage (44,148 base rows) the index alone would be ~+2.2 MB over today
````

**So R1 and R2 are coupled, and T1's arithmetic only closes because R2 removes the index.** T1 replaces
l7's non-pack (10,277,718) with v0.1.6's (3,259,108) — a Store that carries the base inside the record
and therefore has no `objects_bases` at all. **If R2 is not done, T1 is roughly 2.2 MB worse than
quoted.** Within T1 the effect is small only because R4 (35,407 objects) barely exceeds l7 (34,300):
+1,107 rows ≈ **95,202 B**.

---

## 4. Sequencing, by bytes per unit of complexity

````
  1. R3  depth measurement      ~4 lines, no format change     buys 0 B  (legality)
  2. R0  declare the base       0 core LOC                     buys 65,126,400 B
  3. R1a index capacity         2 constants, no format change  most of R1's coverage
  4. R2  lean row grammar       format change, amendment       buys  7,018,610 B
  5. R1b index lifetime         ownership (or format) change   the remaining 11.7 % of R1
  ------------------------------------------------------------------------------------
     T1 target                                                47,048,435 B   [computed]
  6. T2  grouping (optional)    one policy arm + access trade  buys ~2,674,059 B  [est]
  7. T3  per-path frames        a different design             not this campaign
````

**R0 is 81.87 % of the gap for zero production lines — but only because a caller holds the index, and
in production that caller is an unbuilt layer.** That is the real blocker, and it is an owner question,
not an engineering one.
