# T1 — implementation specification

> **Status:** Specification. **Nothing here is implemented.** R0–R3 are proposals; each needs an owner
> ruling before any `core/crates/` line changes.
> Index: [`README.md`](README.md). Tracking: [#188](https://github.com/Ephemeral-AI-Lab/layerfs/issues/188),
> a sub-issue of [#187](https://github.com/Ephemeral-AI-Lab/layerfs/issues/187).
> Source pin: `66bce8378`. **No timing figure appears here** — access cost is in bytes per read.

## 1. What T1 is

**T1 keeps today's physical grammar.** Per-object records, one record per pack group, the same pack
lanes, the same codec. What changes is (a) **which objects are offered a delta base**, (b) **how large
and how long-lived the candidate index is**, and (c) **how many bytes the SQLite row grammar spends per
object**. No new storage concept, no new key, no path.

**Target: 47,048,435 B apparent for `history-stride10` = 0.9540x v0.1.6's 49,315,840 B.**

## 2. Why T1 is the right shape — the mechanism

A whole-file record is compressed **alone** unless it is given a base. Measured on real corpus bytes:

| regime | ratio |
| --- | --: |
| compressed alone | **2.976x** |
| compressed with a stored version as a **raw prefix dictionary** | **23.416x** |

**The dictionary is worth 7.8678x.** The whole-file lane today:

````
  44,148 objects / 348,460,709 B canonical  ->  110,941,054 B stored  (3.141x)

  25,804 obj / 266,427,536 B  ################################  93,744,892 B  (2.842x)  NO BASE
  18,344 obj /  82,033,173 B  #########                         17,196,162 B  (4.770x)  same-save base

  the 76.5 % of canonical bytes with no base are 84.5 % of the lane's stored bytes
````

T1 moves those bytes from the first regime to the second.

## 3. R0 — declare the base

**What changes: nothing in `core/`.** `AdvisoryPredecessors` already rides the C1/C2 boundary with four
slots and provenance tags, and is already consumed at `cas/save.rs:100` → `encoding/delta/select.rs:342`,
which probes the ids in order and returns the first eligible one.

**What is missing is the caller that holds the index.** The harness proved a
`BTreeMap<Vec<u8>, ObjectId>` — one key per path — is sufficient: **128,864,256 → 63,737,856 B**,
`delta.no_candidate` 26,847 → 10,878, `prefix_selected` 18,344 → 34,300.

**The route is already proven end-to-end in this very Store, for a different role:**
`filesystem/sorted/page.rs:380-390` is a live producer and the sole source of the **917 cross-save
InodeLeaf bases** (917 of 1,738 rows, 52.76 %).

**Ordering matters and is measured** (B2, with the product's own byte-exact codec, whole-file lane):

| order | stored |
| --- | --: |
| today | 110,941,054 |
| same-path **previous** first | 45,926,982 |
| **cross-path best, cross-path 2nd, same-path prev, same-path earlier** | **35,937,886** |
| ...plus the measured cache elsewhere | **34,839,722** |

**"Try all four and keep the smallest frame" is worse by 433,443 B** — it deepens chains past the depth
cap. **Acceptance: the whole-file lane's stored bytes fall from 110,941,054 to <= 34,839,722 B.**

**Blocker — `open — required`:** in production the caller is **Stage 7 / Workspace, and the Stage 7
specification does not exist**. The entire in-tree normative text is one row
(`component-decoupling/implementation-issues.md:21`).

## 4. R1 — the index: capacity first, then lifetime

The product **already contains the right index** — `encoding/delta/candidates.rs` is a content-keyed
similarity index (16-byte window, rolling 257, `mix()`, eight smallest hashes, >=2-of-8 match). It
mentions no path, no save, no commit. Two things stop it serving a cross-save delta:

````
  objects reaching an ELIGIBLE base       (of 44,148 whole-file objects / 348,460,709 B canonical)

  today's per-save cache    content key, 1 save    18,433   41.8 %  ########
  caller per-path map       PATH key               29,056   65.8 %  #############
  per-path chain, any ver.  PATH key               29,087   65.9 %  #############
  store index, K = the product's OWN 8-hash signature
                            CONTENT key            37,200   84.3 %  #################
                                                                     |
                                         65.1 % of these match at a DIFFERENT PATH
````

**The content key strictly dominates the path key: +8,144 objects (+28.0 %), +40,143,296 B** — and it
needs no path at all.

````
  +18,767 objects from a persistent, unbounded content index
  |-- LIFETIME alone  (persist the cache exactly as it is)     +2,199    11.7 %  ##
  |-- CAPACITY        (make the index big enough)             +16,568    88.3 %  ###################
````

**Negative result: merely persisting the existing cache buys +11.9 %. Capacity is binding.** The two
constants are `SLOTS = 1024` and `REFERENCES = 8192` (`candidates.rs:14-16`); `INDEX_BYTES` is public at
128 KiB.

| piece | what changes | format change | size |
| --- | --- | --- | --- |
| **R1a capacity** | `SLOTS`, `REFERENCES`, `INDEX_BYTES` | **no** | **2 constants** |
| **R1b lifetime** | move the index off the save owner (`cas/lifecycle.rs:90` → `cas/store.rs:392`) | no if in-memory; **yes** if it must survive a restart | moderate; persisted form ~150–300 LOC + new table + SCHEMA_VERSION 4→5 |

**`open — required`:** the module docstring asserts the fixed size and the one-save lifetime as *design
properties* ("no growth with the number of admitted objects"; "it never outlives the operation that
filled it"), so widening them is a design change, not a typo fix. The persisted form contradicts C2's
stated contract (`layerfs-storage/src/lib.rs:5-7` denies a Workspace or history entity) and needs an
amendment.

## 5. R2 — lean the SQLite row grammar

| | today | v0.1.6 |
| --- | --: | --: |
| non-pack bytes | **10,277,718** | **3,259,108** |
| per object | **197.5 B** | **63.0 B** |
| share of the Store | 16.1 % | 6.6 % |

The non-pack term is `objects` (67.5 B/row) plus two indexes — `objects_locations`
(`CREATE UNIQUE INDEX ... ON objects(pack_id, group_number, record_number)`, 49.0 B/row) and
`objects_bases` (`CREATE INDEX ... ON objects(base_object_id) WHERE base_object_id IS NOT NULL`,
86.0 B/base-row). v0.1.6 carries the base identity **inside the record**
(`tag u8 [+ 32-byte base oid] + frame`), so it needs neither index.

**Why this is not optional — the term does not shrink with the content:**

````
                       pack blob      non-pack     apparent    non-pack share
  today              119,894,291     8,969,965   128,864,256        7.0 %
  l7 faithful         53,460,138    10,277,718    63,737,856       16.1 %
  T1 target           43,789,327     3,259,108    47,048,435        6.9 %
  v0.1.6              46,056,732     3,259,108    49,315,840        6.6 %
````

**Coupling: R1 makes R2 bigger.** `objects_bases` is an index on `base_object_id`, so every new delta
adds a row — measured at **82.0 B of non-pack per extra base row**, matching dbstat's 86.0 B/base-row:

````
                        base rows      non-pack
  base187 (today)          18,344     8,969,965
  l7 (faithful)            34,300    10,277,718
                          +15,956    +1,307,753
````

**T1's arithmetic only closes because R2 removes that index.** If R2 is not done, T1 is roughly **2.2 MB
worse** than quoted.

**Acceptance: non-pack bytes <= 3,259,108 B at 52,032 objects.** **`open — required`:** schema change →
SCHEMA_VERSION bump → owner amendment.

## 6. R3 — fix the depth *measurement*

**This is a correctness fix, not a size lever.** It buys **~2,248 B**.

The faithful model is rejected with `Integrity("dependency chain depth")` (`delta/read.rs:159-168`).
**The bounds themselves are consistent** — the writer produces edges <= cap and the reader refuses
exactly edges > cap (E4 measured the product's own `delta_chains` suite: 9 passed, including a cap-2
policy storing *and reading back* a 2-edge chain).

The defect is `DepthCache::cost_of` (`select.rs:94-144`): the cache-hit exit at `:102-104` does not push
the hit onto `path`, yet `:132-135` computes `depth = cost.depth + position` — so every level of that
walk is recorded **one edge short**. **Proved by the writer's own output:** `/tmp/s0_l7`, whose policy
says `whole_file_delta_max_depth = 8`, holds **4 role-1 objects at 9 edges whose bases sit at true
depth 8**; a base at depth == cap is ineligible.

**Minimal fix (described, NOT applied):** carry a `u8` offset out of the loop — 1 for the `:102-104`
break, 0 for the `:118-124` chain-root break — and add it at `:133-135`. **~4 changed lines, one
function, one file, no format change.** Widening the reader would *compensate*, not repair.

**`open — required`:** product source change; owner ruling.

## 7. What is NOT in T1

| excluded | why |
| --- | --- |
| Grouping (T2) | 2.95 % **worse** on the records that already carry a dictionary, and 59.8x read amplification |
| Per-path frames (T3) | a different design; needs a path concept the Store does not have |
| The one-stream floor | a lower bound; a read decodes 372 MB |
| [#185](https://github.com/Ephemeral-AI-Lab/layerfs/issues/185) | both halves bound at **2,213,938 B = 2.78 %** of the gap, and it touches cause 1 for **0 B** |
| Trained dictionaries | 1.14–1.17x on a held-out split, where the raw prefix dictionary is 7.87x |
| Same-path "any earlier version" | +1.5 % (B2), +31 objects (E2). The win is sideways, not backwards |
| Any codec/level change | the codec is byte-identical to v0.1.6's and v0.1.7 is 2.27 % *better* |

## 8. Verification plan — what would prove T1 wrong

1. **R0:** the whole-file lane's stored bytes fall to <= 34,839,722 B, and `delta.no_candidate` falls
   from 26,847 to <= 10,878, with `absent_candidates` and `ineligible_candidates` reported.
2. **R1:** re-run E2's coverage census against the widened index and confirm >= 37,200 objects reach an
   eligible base. **Falsifier:** if capacity is not binding, widening it moves nothing.
3. **R2:** `non-pack bytes <= 3,259,108` at 52,032 objects.
4. **R3:** E4's falsifier F1 — one Store, cap 2, chain `Q -> B1 -> root` across saves; inside one save
   accept `W` with `explicit(B1)` then `Z` with `explicit(Q)`. With the defect: `trials == 2` and
   reading `Z` fails. With the fix: `Z` is FULL, `ineligible_candidates == 1`, `trials == 1`, all
   readable. **If the unfixed product does not abort, the mechanism is wrong.**
5. **Gate:** re-run `history-stride10` and confirm apparent <= 49,315,840 B — **and that the verify
   phase runs.** The current lane's verify phase is not built, and `ops/history.rs:426-428` refuses
   `--phase verify` for `history.*`, so **no read-back currently runs and a PASS does not cover
   readability.**
6. **Confirmation on stride3**, per the guardrail. **No stride3 confirmation exists today.**

## 9. Standing caveats

- The registered `history-stride10` number **is not a product measurement**: the driver declares no
  base. Cause 1 is the harness.
- The `l7` arm is **not gate-clean** (only `g1.o1` and `g4.swaps` ran; zero `g1.o2`/`g1.o4` records).
- **The margin is thin: 2,267,405 B, 0.954x.** Three independent models agree the schema decides it:
  B4's blind floor is 56,162,647 B under the real schema and **47,648,422 B** with a lean one; B1's
  blind P2 lands at **0.995x–1.051x** of the gate.
- **Production LOC: 84936 -> 84936 (delta 0)** for every commit in this campaign so far. Harness lines
  are stated separately.
