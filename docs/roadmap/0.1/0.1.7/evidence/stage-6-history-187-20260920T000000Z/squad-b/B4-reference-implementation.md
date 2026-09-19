# B4 — a reference implementation that produces the smallest Store

> **Status: PARTIAL, time-boxed.** Written at the parent's time-box. Every number below is
> labelled **MEASURED** (read out of the corpus or the real Store), **MODELLED** (produced by the
> reference model in `b4-scripts/`), or **NOT RUN**. Nothing here is admission evidence; this is a
> `diagnostic` lane artefact. No file under `core/crates/` or `crates/` was read from the legacy
> tree or modified.

## 0. The single number

| | bytes (apparent, `page_count × page_size`) |
| --- | --: |
| current lane `history-stride10`, as recorded | **130,863,104** allocated / **128,864,256** apparent (MEASURED, `/tmp/base187`) |
| v0.1.6, 17 states (the §7 gate) | **49,344,512** allocated / **49,315,940** apparent (cited, not re-run) |
| **MODELLED FLOOR — global planner, real grammar, real SQLite schema** | **56,162,647** |
| **MODELLED SHIPPABLE — single pass, save order, same-path + cross-path candidates** | **60,298,813** |
| MODELLED FLOOR with a lean object table (16 B/row, no dependency index) | 47,648,422 |

**The headline number is 56,162,647 B apparent** for the whole Store — a 2.29× reduction against
the recorded lane (128,864,256 B) and **13.8 % ABOVE the 49,344,512 B v0.1.6 gate**. Under the
real pack grammar *and the real SQLite schema*, this corpus does **not** admit a Store below
v0.1.6's recorded size; the residual is almost entirely the SQLite object/edge/index cost, not
the payload. §7 gives the arithmetic for that.

---

## 1. What was measured

### 1.1 Corpus pins reproduced (MEASURED)

```sh
python3 docs/roadmap/0.1/0.1.7/evidence/stage-6-history-187-20260920T000000Z/squad-b/b4-scripts/b4_pins_and_chains.py
```

* selection `range(1,158,10) ∪ {157}` = 17 states, indices 1,11,…,151,157
* union over the 17 trees: **44,240 distinct oids / 371,937,306 B** — both pins reproduced exactly
* change classes over the 16 transitions: **modified 29,302 · added 16,265 · removed 6,850**
* 16,235 distinct paths; 12,681 paths carry more than one version
* **35,467** distinct oids are a non-first version of some path (35,381 of them are whole-file)

### 1.2 The real Store decoded (MEASURED)

`b4_decode_packs.py` decodes `object_packs` with the grammar of
`core/crates/layerfs-storage/src/pack/{layout,assemble}.rs` (16 B header, 4 B whole-file
directory entry, 16 B other-lane entry, compact whole-file records = tag(1) + [base 32] + frame)
and joins `objects(pack_id, group_number, record_number)` to it.

| lane | packs | groups | pack bytes | record bytes |
| --- | --: | --: | --: | --: |
| whole-file (v4) | 437 | 44,148 | 111,124,638 | 110,941,054 |
| native (v2, chunks) | 31 | 131 | 5,698,270 | 5,685,846 |
| pooled metadata (v6) | 17 | 1,737 | 2,047,483 | body 2,019,419 |
| ordinary (v1) | 17 | 72 | 1,023,900 | body 1,022,476 |
| **total** | **502** | | **119,894,291** | |

Whole-file framing check: 437×16 + 44,148×4 = **183,584** = 111,124,638 − 110,941,054 ✓ (exact).
Total pack framing overhead = **225,496 B**.

Whole-file bucket split (reproduces the campaign's table exactly):

| bucket | objects | canonical | stored | ratio |
| --- | --: | --: | --: | --: |
| with a base | 18,344 | 82,033,173 | 17,196,162 | 4.77× |
| without | 25,804 | 266,427,536 | 93,744,892 | 2.84× |

`objects` table: 52,032 rows / 380,921,300 canonical B. **Every whole-file canonical length is
payload + 23 B** (44,148/44,148 agree) — the 13-byte `LFSO` envelope plus the 10-byte
`LFS5SML\0` value header.

### 1.3 SQLite, by page (MEASURED, `dbstat`)

| structure | bytes |
| --- | --: |
| `object_packs` (bodies + overflow slack) | 120,983,552 |
| `objects` | 3,514,368 |
| `objects_locations` | 2,547,712 |
| `objects_bases` (partial, 18,344 rows) | 1,576,960 |
| `metadata_value_groups` + autoindex | 118,784 |
| `sqlite_schema` + `store_policy` | 8,192 |
| freelist (28 pages) | 114,688 |
| **total** | **128,864,256** = page_count 31,461 × 4,096 ✓ |

Non-pack = 128,864,256 − 119,894,291 = **8,969,965 B**. Per-row costs, which the brief's suggested
model ("16 B per object row") does not match: `objects` **67.5 B/row**, `objects_locations`
**49.0 B/row**, `objects_bases` **86.0 B/base-row** — **172 B per object** in total.

### 1.4 Codec validated against the real Store (MEASURED)

`b4_frames.rs` is a standalone Rust binary (its own `/tmp` workspace; **no repo file touched**)
that reproduces `encoding/codec.rs` exactly: static 2 MiB `ZSTD_CCtx`, level 3, `windowLog` 18,
contentSize 1, checksum 1, dictID 0, nbWorkers 0, `ZSTD_CCtx_refPrefix` for the prefix route.

* object identity: `object_id = BLAKE3("layerfs/object/v2\0" ‖ canonical)`
  (`object/id.rs`). **44,148 of 44,240** corpus canonical ids land in the Store; the 92 misses are
  exactly the blobs ≥ 128 KiB (the chunked lane). **0 of 44,240** match `sha256(payload)` — the
  identity is not a raw content digest.
* **44,146 of 44,148** whole-file records are reproduced **byte-exact in length** by
  `1 + 32·[base] + frame(payload, base_payload)`, total residual **−8 B on 110,941,054 B**
  (7.2 × 10⁻⁸). The two exceptions are +4 B each (unexplained; see §8).

**The model is therefore calibrated**: configured with the real base assignment it reproduces the
real Store.

---

## 2. The model

`b4_store_model.py` assembles, per policy:

1. per-object record = `1 + 32·[base] + frame`;
2. whole-file packs by the real placement rule (append while `assembled + 4 + body ≤ 262,144` and
   `groups < 256`, one record per group);
3. native + metadata lanes held at their **measured** values (5,698,270 + 3,071,383) — one
   variable at a time;
4. SQLite non-pack from the measured per-row costs above, anchored on the real `dbstat` numbers;
5. depth cap **8** (`whole_file_delta_max_depth`), chain limits **512 KiB canonical / 256 KiB
   encoded** (`CHAIN_*_LIMIT`), delta taken only when `1 + 32 + frame_p < 1 + frame_f`
   (`select.rs` compares complete records).

Candidate sets: **108,116** same-path ordered pairs (any earlier version of the same path) and
**248,511** cross-path pairs from a bottom-32 minhash sketch over 16-byte shingles (top 6 partners
per object). 356,627 prefix frames were computed with the instrument.

## 3. Results — whole-file lane record bytes (MODELLED)

| policy | record bytes | bases | FULL | ratio vs canonical |
| --- | --: | --: | --: | --: |
| FULL only (no deltas) | 119,815,884 | 0 | 44,148 | 3.18× |
| **REAL policy (MEASURED)** | **110,941,054** | 18,344 | 25,804 | 3.43× |
| S1 previous version only, one pass, cost-compared | 48,138,017 | 28,433 | 15,715 | 7.91× |
| S2 any earlier same-path version, one pass | 46,178,100 | 29,056 | 15,092 | 8.25× |
| S3 same-path + cross-path, one pass (save order) | 40,580,789 | 39,196 | 4,952 | 9.39× |
| S4 global savings greedy (depth-aware, offline) | 36,317,850 | 40,651 | 3,497 | 10.49× |

S4 depth histogram: 1:4,818 2:5,913 3:6,614 4:6,534 5:6,010 6:5,087 7:3,678 8:1,997 — the cap
binds, so the floor is **not** depth-limited by accident.

## 4. Results — total modelled Store (MODELLED)

| policy | wf records | wf packs | native | metadata | pack total | SQLite non-pack | **TOTAL apparent** |
| --- | --: | --: | --: | --: | --: | --: | --: |
| REAL | 110,941,054 | 111,301,118 | 5,698,270 | 3,071,383 | 120,070,771 | 8,856,984 | **128,927,755** |
| FULL only | 119,815,884 | 120,176,492 | 5,698,270 | 3,071,383 | 128,946,145 | 6,848,787 | 135,794,932 |
| S1 | 48,138,017 | 48,494,561 | 5,698,270 | 3,071,383 | 57,264,214 | 9,435,209 | **66,699,423** |
| S2 | 46,178,100 | 46,534,612 | 5,698,270 | 3,071,383 | 55,304,265 | 9,488,344 | **64,792,609** |
| S3 | 40,580,789 | 40,937,029 | 5,698,270 | 3,071,383 | 49,706,682 | 10,592,131 | **60,298,813** |
| **S4 (floor)** | 36,317,850 | 36,673,914 | 5,698,270 | 3,071,383 | 45,443,567 | 10,719,080 | **56,162,647** |

**Model validation residual.** Configured with the real policy the model returns
128,927,755 B against the real 128,864,256 B: **+63,499 B (+0.049 %)**, made of
**+176,480 B** of pack-boundary difference (my emission order yields 430 whole-file packs against
the real 437) and **−112,981 B** of SQLite difference (the model excludes the real file's 28-page
free list, 114,688 B, and rounds row costs by +1,707 B). The model reproduces the real Store to
within 0.05 % when configured to match it.

## 5. ACHIEVABLE FLOOR vs SHIPPABLE TARGET

* **Shippable (S3): 60,298,813 B.** Single pass in save order, each object delta'd against the
  best *already-stored* candidate: the previous version of its path (the route the product already
  has), any earlier version of the same path, and a bounded content-similarity index over stored
  objects. No lookahead, no global plan.
* **Achievable floor (S4): 56,162,647 B.** The same candidate set, but chosen by a global
  saving-ordered forest that may pick a base stored *later* in the save. This needs an offline
  planner or a second pass.
* **What the difference is made of: 4,136,166 B**, entirely in the whole-file lane
  (40,580,789 → 36,317,850 records): 1,455 more objects get a base, and the ones that do get a
  better one. The SQLite cost *rises* by 126,949 B (more base ids to index), so the net Store
  difference is 4,136,166 − 126,949 = **4,009,217 B**.
* **The true floor is ≤ 36,317,850** for the whole-file lane: S4 is a savings greedy over a
  *bounded* candidate set (356,627 pairs), so it is an upper bound on the optimum, not the optimum.

## 6. Sensitivity to the assumptions I am least sure about

| # | assumption | if wrong | Δ on the 56,162,647 B headline |
| --: | --- | --- | --: |
| 1 | SQLite per-object cost = the **measured core schema** (172 B/object incl. indexes) | lean schema, 16 B/row + 16 B location entry, no dependency index → non-pack 2,204,855 B | **−8,514,225 → 47,648,422** |
| 2 | native + metadata lanes held at their measured 8,769,653 B | the 92 chunked files (24,492,001 B of content) can also take bases; or a different metadata object model | not modelled (NOT RUN) |
| 3 | candidate set = same-path ∪ top-6 minhash | a wider set lowers S4; all-pairs is infeasible here | floor is an upper bound; direction is downward |
| 4 | pack boundaries from my emission order | real ordering | ±176,480 B (±0.15 %) |
| 5 | codec = level 3, windowLog 18, checksum | validated to 8 B in 110.9 MB | ±8 B |
| 6 | delta only when `frame_p + 32 < frame_f` | a different rule | small; the rule is the product's |

**Sensitivity #1 dominates.** The whole gap between the modelled floor and v0.1.6's 49,344,512 B
is the SQLite object/edge/index cost: 10,719,080 B modelled versus ~2.2 MB for the brief's 16 B/row
grammar. Payload-side, the model is already at 45.4 MB of pack bodies, **below** v0.1.6's entire
recorded Store.

## 7. The gap to the v0.1.6 gate, as arithmetic

```
modelled floor total            56,162,647
  pack bodies                   45,443,567   (36,673,914 whole-file + 8,769,653 other lanes)
  SQLite non-pack               10,719,080
v0.1.6 recorded apparent        49,315,940
residual                        +6,846,707   (+13.9 %)

Two independent readings of where that residual can live:
  (a) payload side: the modelled pack bodies (45,443,567) are 3,872,373 B BELOW v0.1.6's
      ENTIRE recorded file (49,315,940). The residual is therefore not payload.
  (b) bookkeeping side: the same model under the brief's 16 B/row grammar returns 47,648,422 B,
      which is 8,514,225 B below the modelled floor and 1,667,518 B BELOW v0.1.6.
      (49,315,940 - 47,648,422 = 1,667,518.)
=> the whole residual is consistent with the SQLite object/edge/index cost, not with compression.
```

This is a **negative result**: with the real grammar and the real schema, no base-selection policy
reaches 49,344,512 B on this corpus. Either v0.1.6's per-object bookkeeping was materially leaner
than the core's (I may not read `crates/` to check), or its recorded 17-state Store is not
byte-comparable to this one. It is reported, not explained.

## 8. Negative results and ruled-out hypotheses

1. **`object_id ≠ sha256(content)`** — 0/44,240 hits. BLAKE3 over the canonical object gives
   44,148/44,240; the 92 misses are exactly the ≥128 KiB blobs. Ruled out with numbers.
2. **The prefix is the base's *payload*, not its canonical bytes.** Canonical-prefix model:
   28,383/44,148 exact, residual **−5,148 B**. Payload-prefix model: 44,146/44,148 exact, residual
   **−8 B**. Ruled out.
3. **The real Store never chains deltas.** Depth histogram over all 52,032 objects:
   `{0: 25,896, 1: 18,344}` — every recorded base is a FULL record. Its 4.77× is single-step only.
4. **"The path was modified" does not explain the real base assignment.** 35,381 whole-file objects
   have an immediate same-path predecessor; only 18,344 have a base. The first-appearance
   predictor gives 29,034 predicted / 18,344 actual / 12,213 intersection. The rule behind
   18,344 is **not identified here** (mechanism is squad A's); this is the single largest lever in
   the campaign: the same route, applied to every object that has a predecessor, is S1 = 48.1 MB
   of records against the real 110.9 MB.
5. **A savings-ordered forest without an incremental depth constraint is wrong.** Unconstrained
   Kruskal reached **depth 92**; dropping every node deeper than 8 collapsed it to 15,721 edges /
   98,669,039 B — *worse* than the plain ordered greedy. The depth cap must be enforced at edge
   acceptance (S4 does).
6. **"16 B per object row" is not what the real Store costs.** `dbstat`: `objects` 67.5 B/row,
   `objects_locations` 49.0 B/row, `objects_bases` 86.0 B/base-row.
7. **Two records are 4 B larger than the codec model predicts** (oids
   `ad4c368e…` 787 B and `1a760484…` 1,700 B, both with a base). Unexplained; 8 B of 110.9 MB.

## 9. NOT RUN (stated plainly)

* No new lane run was taken. All Store bytes are the existing `/tmp/base187/sample.sqlite`; no
  TIMING reading was taken anywhere in this work (byte readings only).
* stride3 was **not** modelled — the model is stride10-only. The stride3 confirmation is NOT RUN.
* The 92 chunked files were **not** re-modelled (no CDC implementation); the native and metadata
  lanes are the measured constants.
* The all-pairs floor was **not** computed; S4 is a bounded-candidate-set greedy.
* v0.1.6's own numbers are **cited, not re-run**, and `crates/` was not read.

## 10. Reproduction

```sh
C=/Users/yifanxu/Ephemeral-AI-Lab/deepseek-history-data
S=docs/roadmap/0.1/0.1.7/evidence/stage-6-history-187-20260920T000000Z/squad-b/b4-scripts
cargo build --release --offline --manifest-path /tmp/b4tool/Cargo.toml   # b4_frames.rs
python3 $S/b4_pins_and_chains.py            # pins: 44,240 oids / 371,937,306 B
python3 $S/b4_decode_packs.py               # pack directory -> 502 packs / 119,894,291 B
python3 $S/b4_candidates_samepath.py        # 108,116 same-path pairs
python3 $S/b4_candidates_crosspath.py       # 248,511 cross-path pairs
/tmp/b4tool/target/release/b4tool frames /tmp/b4/blobs_payload.tsv 8 < jobs.tsv > frames.tsv
python3 $S/b4_store_model.py                # the totals in §4
python3 $S/b4_global_savings.py             # the S4 floor
```

*Every figure in this document is `diagnostic`. None is admission evidence.*
