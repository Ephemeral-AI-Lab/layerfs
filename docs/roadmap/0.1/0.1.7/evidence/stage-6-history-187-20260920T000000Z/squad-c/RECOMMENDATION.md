# Recommendation, and the stride10 target

**Basis: apparent bytes (`st_size`), `history-stride10`, 17 states.** Everything is **[measured]**
unless marked **[computed]** or **[est]**. No timing anywhere — the machine was shared.

---

## 0. The recommendation in one line

**Stop treating this as a codec problem. It is an INDEX problem.** The product already contains the
right index — a content-keyed similarity index — and it is the wrong *size* and lives for the wrong
*length of time*. Enlarging it is 88 % of the win; persisting it is 12 %. No branch, no commit, no
revision and no history entity is required anywhere.

### R0 · Declare the base — **0 lines of core** · 65,126,400 B (81.87 % of the gap)

`AdvisoryPredecessors` already rides the C1/C2 boundary with 4 slots and is already consumed
(`cas/save.rs:100` → `select.rs:342`). What is missing is only **the caller that holds the index**.
The harness proved a `BTreeMap<Vec<u8>, ObjectId>` is sufficient: **128,864,256 → 63,737,856 B**.

### R1 · The index — **capacity first, then lifetime** · the rest of the coverage

This is the finding that changed the recommendation. Measured by E2 over all 44,148 whole-file objects:

````
  objects reaching an ELIGIBLE base            (of 44,148 whole-file objects / 348,460,709 B canonical)

  today's per-save cache    content key, 1 save    18,433   41.8 %  ########
  caller per-path map       PATH key               29,056   65.8 %  #############
  per-path chain, any ver.  PATH key               29,087   65.9 %  #############
  store index, K = the product's OWN 8-hash signature
                            CONTENT key            37,200   84.3 %  #################
                                                                     |
                                         65.1 % of these match at a DIFFERENT PATH
````

**The content key strictly dominates the path key: +8,144 objects (+28.0 %), +40,143,296 B** — and it
needs no path, no branch, no commit, no revision, no history entity of any kind. It is the same
mechanism `candidates.rs` already implements (16-byte window, rolling 257, `mix()`, 8 smallest hashes,
≥2-of-8 match).

````
  +18,767 objects from a persistent, unbounded content index
  |
  |-- LIFETIME alone  (persist the cache exactly as it is)     +2,199    11.7 %  ##
  |-- CAPACITY        (make the index big enough)             +16,568    88.3 %  ###################
````

**Negative result worth acting on:** merely persisting the product's existing cache buys **+11.9 %**
and only 6,418 cross-save matches. **"Make the cache persistent" is not the fix. Capacity is binding.**
The two constants are `SLOTS = 1024` and `REFERENCES = 8192` (128 KiB of live index — smaller than one
state of this lane).

### R2 · Lean the SQLite row grammar — **7,018,610 B** · this is what decides the gate

### R3 · Fix the depth *measurement* — **~4 lines, one function, no format change**

`DepthCache::cost_of` records every level of a cache-hit walk one edge short. **It buys legality, not
bytes** (~2,248 B), but nothing above can be trusted until it is fixed: `/tmp/s0_l7` holds 4 objects at
9 edges whose bases sit at true depth 8.

### R4 · Grouping — **optional**, ~2,674,059 B [est] for a 59.8× read amplification

**Not recommended as part of the gate.** B3 measured it 2.95 % *worse* on the records that already
carry a dictionary.

### Not recommended: #185

Both halves bound at **2,213,938 B = 2.78 %** of the gap, and it touches cause 1 for **0 B**.

---

## 1. Why it is more storage efficient — the mechanism

### 1a. Two compression regimes

````
   ALONE  (what 25,804 of the 44,148 objects get today)
   +-----------+                    nothing to refer to
   | version k |--> zstd L3 -->  2.976x   -->  0.336 stored B per canonical B
   +-----------+

   WITH A BASE  (what the other 18,344 get, and what R0/R1 extend)
   +-----------+                    +----------------+
   | version k |--> zstd L3 -->    | version k-1    |  already stored; used as a
   +-----------+    23.416x        | RAW PREFIX     |  raw prefix dictionary
                                   | DICTIONARY     |
                                   +----------------+
                                   0.0427 stored B per canonical B

   the dictionary is worth 7.8678x  (B3, 29,222 real records; my full-population
                                     sweep agrees at 23.823x within 1.7 %)
````

### 1b. Where the bytes actually sit today

````
  whole-file lane: 44,148 objects / 348,460,709 B canonical  ->  110,941,054 B stored  (3.141x)

  25,804 obj / 266,427,536 B  ################################  93,744,892 B  (2.842x)  NO BASE
  18,344 obj /  82,033,173 B  #########                         17,196,162 B  (4.770x)  same-save base

  the 76.5 % of canonical bytes with no base are 84.5 % of the lane's stored bytes
````

### 1c. Why R2 (the schema) decides the gate

````
                       pack blob      non-pack     apparent    non-pack share
  today              119,894,291     8,969,965   128,864,256        7.0 %
  l7 faithful         53,460,138    10,277,718    63,737,856       16.1 %
  T1 target           43,789,327     3,259,108    47,048,435        6.9 %
  v0.1.6              46,056,732     3,259,108    49,315,840        6.6 %

  non-pack is ~197.5 B per object row (objects 67.5 + locations 49.0 + bases 86.0)
  x 52,032 objects. It does NOT shrink with the content, so halving the content
  DOUBLES its share -- which is why it, and not the codec, decides the gate.
  v0.1.6 carries the base identity INSIDE the record (tag + 32-byte base oid),
  so it needs neither the bases table nor a separate locations index: 63.0 B/object.
````

---

## 2. The waterfall

````
  128,864,256  ################################################################  registered lane
               |
               |  R0  declare the base            -65,126,400   (81.87 % of the gap)
               v
   63,737,856  ################################                                 [measured]
               |
               |  R1  index: capacity + lifetime   -9,670,811
               v
   54,067,045  ###############################                                  [computed]
               |
               |  R2  lean row grammar             -7,018,610
               v
   47,048,435  ##############################                                   TARGET
   49,315,840  ###############################                                  v0.1.6
               ^ margin 2,267,405 B = 0.954x
````

R1's 9,670,811 B is B2's **cross-path-first, 4-slot** rule measured with the product's **own byte-exact
codec**: whole-file lane **44,510,533 → 34,839,722**. It is **conservative** — E2's content index
reaches 37,200 objects against R4's 35,407.

---

## 3. The target for stride10

| tier | target (apparent) | vs v0.1.6 | composition | status |
| --- | --: | --: | --- | --- |
| v0.1.6 (the gate) | 49,315,840 | 1.000× | retained artifact | **measured** |
| **T1 — clears the gate** | **47,048,435** | **0.954×** | l7 + R1 + R2 | **[computed]** from measured parts |
| T2 — + grouping | ~44,374,000 | 0.900× | T1 + R4 | **[est]** one estimated input |
| T3 — B1's P1 | ~31,100,000 | 0.631× | 27,841,060 content + lean overhead | content **measured**, overhead composed |
| floor — B1's L | ~21,382,000 | 0.434× | 18,123,366 content + lean overhead | **lower bound**; a read decodes 372 MB |

**T1 is the number to hold the campaign to.** T3 needs the grouping change and an access-cost ruling
(≤ 11.26 MB decoded per read at 254 group frames). The floor is not a target: it is one zstd stream
over the whole union and any read decodes the whole 372 MB.

**What makes T1 credible rather than optimistic:** three independent models land in the same band.
B4's blind floor is **56,162,647 B** under the real schema and **47,648,422 B** with a lean one; B1's
blind P2 lands at **0.995×–1.051×** of the gate on the measured overhead; and my own decomposition
gives 47,048,435. **All three say the schema is the deciding term** — which is exactly why R2 is not
optional.

---

## 4. What is NOT in the target

* **The depth fix** buys legality (~2,248 B), not bytes. E4: under a cap of 8 every base at depth ≥ 8
  is refused, so an unlimited declaration cannot beat the depth-7 arm.
* **#185** — 2.78 % of the gap, 0 B of cause 1.
* **Trained dictionaries** — B3: 1.14–1.17× on a held-out split, where the raw prefix dictionary is 7.87×.
* **Same-path "any earlier version"** — B2: +1.5 %. E2: +31 objects. The win is sideways, not backwards.
* **Any product change at all.** R0–R4 are proposals; every one of them needs an owner ruling, and the
  registered lane's number is not a product measurement until R0 is real.
