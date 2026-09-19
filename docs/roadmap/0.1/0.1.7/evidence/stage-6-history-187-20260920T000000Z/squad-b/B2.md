# B2 — base selection as an algorithm (stride10, 17 states)

> **Status: partial, time-boxed.** Every number here is **diagnostic**, not admission
> evidence. §7 states exactly which of the five assigned questions are MEASURED and which
> are NOT RUN. The machine was shared with other diagnostic work: **all figures below are
> BYTE readings and are load-independent; no timing figure is reported.**

Blind half: nothing under root `crates/` was read; no `squad-a/` report was read.

---

## 0. Headline

| | bytes | ratio vs canonical |
| --- | ---: | ---: |
| canonical, whole-file lane (44,148 objects) | 348,460,709 | 1.000x |
| **measured** stored, whole-file lane (today) | **110,941,054** | 3.141x |
| measured, if every whole-file record were FULL | 119,815,884 | 2.908x |
| rule R1 (same-path previous version, 1 slot) | 45,926,982 | 7.587x |
| rule R4_crossfirst (recommended, 4 slots) | 35,937,886 | 9.696x |
| R4_crossfirst, combined with the measured cache elsewhere | 34,839,722 | 10.002x |

**Mechanism finding (measured, not inferred): the advisory route carries ZERO deltas in
this lane.** All 18,344 delta bases the Store holds are *same-save* matches from the
per-save candidate cache; not one crosses a save boundary. §3.

---

## 1. Instruments, and why they are exact rather than proxies

### 1.1 The codec is directly invocable, and it is byte-exact

`layerfs-storage/src/encoding/codec.rs:258 compress_prefix` sets exactly:
`ZSTD_c_compressionLevel=3`, `ZSTD_c_windowLog=profile.window_log()`,
`ZSTD_c_contentSizeFlag=1`, `ZSTD_c_checksumFlag=1`, `ZSTD_c_dictIDFlag=0`,
`ZSTD_c_nbWorkers=0`, and borrows the base with `ZSTD_CCtx_refPrefix`.
At the frozen default cutoff (`small_file_threshold_bytes = 131072`), `policy.rs:166`
gives `window_log = 18` and `whole_file_raw_limit = 131071`.

I rebuilt that call in 60 lines of Rust linked against **the product's own**
`zstd-sys 2.0.16+zstd.1.5.7` rlib and `libzstd.a` (no new dependency, nothing added to any
Cargo workspace):

    rustc +1.85.1 --edition 2021 -O /tmp/b2/codecprobe.rs -o /tmp/b2/codecprobe
      --extern zstd_sys=<harness target>/release/deps/libzstd_sys-9d13631457f814f3.rlib
      -L dependency=<harness target>/release/deps
      -L native=<harness target>/release/build/zstd-sys-2e157bf4e41512a3/out

**Validation (this is the load-bearing check):** every one of the 44,148 whole-file records
was re-encoded — FULL without a prefix, PREFIX with the recorded base as raw prefix — and
compared **per object** with the record width the pack directory reports.

| bucket | objects | measured stored | re-encoded | residual | per-object mismatches |
| --- | --: | --: | --: | --: | --: |
| whole-file WITH base | 18,344 | 17,196,162 | 17,196,162 | **0** | **0** |
| whole-file WITHOUT base | 25,804 | 93,744,892 | 93,744,892 | **0** | **0** |

So there is **no codec proxy** in this report: the delta numbers are the product's own
bytes. (The bucket table in the brief is reproduced exactly.)

### 1.2 The pack directory decodes, and the bucket table reconciles

Header `LFPACK` + two NUL bytes, then version u32, then group count u32; WholeFile (v4)
directory = one u32 per group and the group body is the compact record **after** its 8
framing bytes are dropped; other lanes = start/encoded/decoded/codec (16 B). Written from
`pack/layout.rs` + `pack/assemble.rs`; `/tmp/b2/decode2.py`.

| lane | packs | groups | encoded body | decoded body |
| --- | --: | --: | --: | --: |
| WholeFile (v4, Raw) | 437 | 44,148 | 110,941,054 | 110,941,054 |
| Native (v2, Raw) | 31 | 131 | 5,695,678 | 5,695,678 |
| Ordinary (v1, may be zstd) | 17 | 72 | 1,022,476 | 3,026,849 |
| PooledMetadata (v6, may be zstd) | 17 | 1,737 | 2,019,419 | 4,850,523 |
| headers + directories | | | 215,664 | |
| **total, sum of object_packs.data** | 502 | | **119,894,291** | |

Cross-check: the base parsed out of each compact record equals `objects.base_object_id`
for **all 44,148** objects (0 mismatches) — the decoder is not inventing bases.

**The brief's "other lanes (aggregate only) 8,953,237" is exactly
`119,894,291 − 110,941,054`**: it is a residual that contains all 502 pack headers, both
directory areas and the *compressed* non-whole-file bodies. It is not a per-object join and
it is not a naive join; it simply is not decomposable into "other lanes" bodies. My
decomposition of the same 7,884 non-whole-file objects is 8,737,573 B of body plus
215,664 B of framing, and 4,618,813 B of that body is zstd-compressed group bodies that a
per-object join would have overstated. **Negative result recorded: a per-object attribution
of the non-whole-file lanes is not recoverable from this Store without decompressing group
bodies and re-deriving record boundaries; I did not need it and did not do it.**

### 1.3 Joining the Store to the corpus required a hash, and the obvious key is wrong

The corpus oracle's third field is **sha256(file content)**, *not* the layerfs
`ObjectId`; the Store's `objects.object_id` is
`blake3("layerfs/object/v2" + NUL || canonical)` (`object/id.rs:16,25`). Intersection of
the two key spaces: **0 of 44,148**. The join therefore has to be made by *predicting* the
layerfs id from corpus bytes: `canonical = "LFSO" | 0x01 | payload_len:u32be | value_len:u32be
| "LFS5SML" + NUL | 0x0001 | content`, 23 bytes of envelope (`object/codec.rs`,
`file/content.rs`).

    blake3("layerfs/object/v2" + NUL || canonical) over the 44,240 stride10 union contents
      -> matched 44,148 Store objects, ALL role = WholeFile
      -> Store WholeFile objects with no predicted corpus content: 0
      -> predicted ids absent from the Store: 92  (all size >= 131072 -> Chunked, no whole-file object)
      -> canonical_length mismatches over the matched set: 0

This is a second, independent byte-exactness proof: the corpus bytes *are* the stored
payloads. (One more corpus fact worth recording: the 44,240 union contents = 44,148
whole-file **+ 92 chunked file roots**; the empty file
`e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855` is
`Representation::Empty` and stores no object at all.)

---

## 2. The rule space I measured

Every rule is a function `(object, state) -> [base, base, base, base]`, at most
`MAXIMUM_ADVISORY_PREDECESSORS = 4`, in preference order. The simulator mirrors
`select.rs` exactly:

* `eligible(b) := stored && role(b) == role(O) && depth(b) < depth_cap`;
* the **first** eligible entry wins and the cache is then **not** consulted;
* exactly **one** prefix trial; `PREFIX` is kept iff its record is strictly smaller than FULL;
* chain budgets enforced as the code does: `chain_canonical(b) + |O| <= 512 KiB` and
  `chain_encoded(b) + |O| <= 256 KiB`, where the chain sums **include the base itself**
  (`read.rs:113 resolve_dependency -> resolve_charged(root, true)`), and a base that fails a
  budget stores FULL with no retry;
* `depth(O) = depth(b) + 1` on a PREFIX; a FULL record has depth 0 and zero chain cost
  (`depths.record` is only reached on prefix selection).

Whole-file record width: `1 + frame_len` (FULL) or `33 + frame_len` (PREFIX) — the compact
lane drops its 8 framing bytes at assembly, so `1+8+32*has_base+frame-8`.

---

## 3. MEASURED MECHANISM: the advisory route is dead on the construction path

Attribution of the 18,344 measured bases (`/tmp/b2/exp1b.py`, `/tmp/b2/exp1c.py`):

| class | objects | canonical |
| --- | --: | --: |
| base whose content first appears in the **same** state (per-save cache) | **18,344** | 82,033,173 |
| base that is the same-path previous selected state's version (advisory) | **0** | 0 |
| stored FULL, no base | 25,804 | 266,427,536 |

Broken down: 70 of the 18,344 are in state 1 — where no previous state exists at all — and
the remaining 18,274 all have a base whose content first appears in the **same state as the
dependent**, i.e. created earlier in the same save. Zero cross-save bases.

Direct evidence, not inference: of the 74 paths that changed between the first two selected
states, six sampled all have a same-path previous version present in the Store and **all six
are stored FULL** with no base. And the census over `k>1` objects:

| has same-path advisory | has a base | objects | canonical |
| --- | --- | --: | --: |
| yes | no | 16,821 | 203,382,268 |
| no | yes | 6,053 | 21,330,531 |
| no | no | 8,792 | 61,894,196 |
| **yes** | **yes** | **12,221** | **60,528,085** |

12,221 objects have a same-path advisory *and* took a base from the cache. Under
`acquisition()`-first control flow that is only possible if the advisory list was **empty**
(an eligible advisory would have been returned and the cache never consulted). It is empty:

    grep -rn 'with_predecessors' core/crates/layerfs-content/src
      file/edit/apply.rs:121                       <- the EDIT path
      filesystem/sorted/page.rs:383                <- an Ordinary-lane tree role

`select.rs:204-222` short-circuits every tree role to `encode_full` *before* the advisory is
ever read, and `filesystem/sorted/page.rs` only ever attaches a predecessor to such a role.
The construction path this lane uses (`construct_files` / `build_filesystem`, spec §3)
attaches none. Therefore, for `history-*`:

> **The advisory route (`cas/save.rs:100 -> select.rs:342`) contributes 0 bytes. The only
> live route is the per-save candidate cache, which by construction cannot reach a previous
> state's version. The measured cache contribution is 119,815,884 − 110,941,054 =
> 8,874,830 B (7.4 % of the no-delta total).**

This is the mechanism behind the 2.65x gap, and it is a **C1** gap, not a C2 one: C2's
selection is fine; nothing ever hands it a cross-save candidate. Every rule below is
therefore a **counterfactual** that requires C1 to populate the advisory list on the
construction path. That is the actionable recommendation; no product source was modified.

---

## 4. The five questions

### Q1 — same-path previous version: MEASURED

Rule `R1_prev` = `[content at (same path, previous selected state)]`.

| | objects | canonical | stored | ratio |
| --- | --: | --: | --: | --: |
| covered by PREFIX | 28,560 | 255,082,308 | — | — |
| not covered (no eligible advisory) | 15,588 | 93,378,401 | — | — |
| **whole lane** | **44,148** | **348,460,709** | **45,926,982** | **7.587x** |

Arithmetic: `348,460,709 / 45,926,982 = 7.587`; against today's `110,941,054` the rule saves
`110,941,054 − 45,926,982 = 65,014,072 B` (58.6 %). Outcomes: `prefix 28,560`,
`full_wins 11`, `chain_budget 10`, `no_eligible 15,567`.
Delta-able census (a same-path earlier version exists at all): **29,042 objects /
263,910,353 B (75.7 % of canonical)**; 15,106 / 84,550,356 B have none.

### Q2 — same-path ANY earlier version: MEASURED, and it barely helps

| rule | stored | ratio | covered obj | covered canonical |
| --- | --: | --: | --: | --: |
| `R2_nearest4` (4 nearest earlier versions) | 45,269,285 | 7.698x | 28,911 | 259,704,252 |
| `R2_allearlier` (all earlier, nearest-first) | 45,230,840 | 7.704x | 28,936 | 259,956,503 |
| `R2_anchor` (oldest first, then nearest) | 45,231,132 | 7.704x | 28,936 | 259,956,503 |

**Residual against R1: 45,926,982 − 45,230,840 = 696,142 B (1.5 %).** 371 more objects and
4,874,195 more canonical bytes are covered, but the extra bases are *worse* bases: a version
four states back compresses an order of magnitude less well than the immediately previous
one. **Negative result: widening the same-path window from 1 to 4 slots buys 1.5 %, and
ordering it oldest-first buys nothing (45,231,132 vs 45,230,840, +292 B).**

### Q3 — cross-path similarity: MEASURED

Index: the product's own signature (`candidates.rs`: 16-byte window, 257 rolling hash, `mix`)
extended to the 32 smallest distinct hashes, computed by a second Rust probe
(`/tmp/b2/sigprobe.rs`, same mixing, so it is what the product's own index could compute).
Candidate set for object `O` created at state `k`: contents with `first_state < k` (i.e.
already stored when `O` is written), sharing >= 4 of 32 hashes, ranked by overlap then size,
top 8 kept, top 4 used.

**The number the question asks for: 33,210 of the 43,887 non-first-state objects (75.7 %)
have at least one already-stored object at a DIFFERENT path sharing >= 4 of 32 shingles.**
(Buckets larger than 400 entries — 261,810 hash lookups — were skipped, so this is a lower
bound.)

| rule | stored | ratio | covered obj | covered canonical |
| --- | --: | --: | --: | --: |
| `R3_cross4` (4 best cross-path) | 36,405,522 | 9.572x | 33,064 | 299,379,884 |
| `R4_hybrid` (prev, then 2 cross, then earlier) | 36,235,811 | 9.616x | 35,279 | 298,406,719 |
| `R4_crossfirst` (2 cross, then prev, then earlier) | **35,937,886** | **9.696x** | 35,407 | 300,329,421 |

Cross-path coverage is 300,329,421 B canonical (86.2 %) against same-path's 259,704,252
(74.5 %). **Residual of cross-path over the best same-path rule:
45,230,840 − 35,937,886 = 9,292,954 B (20.5 %)** — i.e. cross-path is worth about 13x more
than widening the same-path window.

### Q4 — ordering in the 4 slots: MEASURED, and the answer is counter-intuitive

The code does **one** trial on the **first eligible** entry. I measured both readings:

* `first` = the list's preference order decides (what the code does today);
* `best` = trial all eligible entries and keep the smallest record (what a
  "compare the candidates" change would do).

| rule | `first` | `best` | delta (best − first) |
| --- | --: | --: | --: |
| `R2_nearest4` | 45,269,285 | 45,137,650 | −131,635 |
| `R2_allearlier` | 45,230,840 | 45,082,142 | −148,698 |
| `R3_cross4` | 36,405,522 | 36,808,459 | **+402,937** |
| `R4_hybrid` | 36,235,811 | 36,371,329 | **+135,518** |
| `R4_crossfirst` | **35,937,886** | 36,371,329 | **+433,443** |

**Negative result: "pick the smallest frame per object" is WORSE for the cross-path rules.**
Greedily minimising one object's record deepens its chain, and a deeper chain is what makes
the *next* version's base ineligible under the depth cap; the loss lands on later objects.
The depth histograms show it directly (cap 8, `first` ordering):

    R4_crossfirst  depths {0:8741, 1:9257, 2:7358, 3:5866, 4:4274, 5:3211, 6:2300, 7:1527, 8:1614}
    R4_hybrid      depths {0:8869, 1:7711, 2:6275, 3:5311, 4:4342, 5:3534, 6:2848, 7:2329, 8:2929}
    R1_prev        depths {0:15588,1:8987, 2:6284, 3:4454, 4:3189, 5:2294, 6:1594, 7:1054, 8:704}

**Best preference order measured: `[cross-path best, cross-path second, same-path previous,
same-path earlier]`** (`R4_crossfirst/first`), 35,937,886 B. Putting the same-path previous
version first (`R4_hybrid/first`, 36,235,811 B) costs **297,925 B** — the cross-path match is
usually the better base, so it must occupy slot 1. Slot order is worth up to 433,443 B
(1.2 %) in the `best`-ordering comparison.

### Q5 — chain length limits: **NOT RUN** (sweep not executed)

I did **not** run the depth-cap sweep (cap in 1..8), so I have **no measured size-vs-depth
curve** and I will not invent one. What I do have, measured:

* with `depth_cap = 8` (the frozen default) the maximum achieved depth is exactly **8**, and
  704 (R1) to 2,929 (R4_hybrid) objects sit at that boundary — the cap is **binding**, it is
  not slack;
* the cap costs coverage directly: `no_eligible` is 15,567 (R1), 15,216 (R2_nearest4),
  11,052 (R3_cross4), 8,706 (R4_crossfirst) objects, and `chain_budget` refusals are a
  separate, much smaller 6–14 objects;
* the depth histogram is monotone-decreasing except at the cap, which is the signature of
  chains being cut off rather than of chains being naturally shallow.

**Recommendation, labelled as a HYPOTHESIS because the sweep was not run:** keep
`whole_file_delta_max_depth = 8` until the sweep exists. The histogram says raising it would
recover objects at depth 8, and the read cost is exactly `d` record reads for a depth-`d`
chain, so the trade is real — but I have no measured bytes-per-depth point, and I will not
report a number I did not take.

---

## 5. Recommended rule and its measured total

**R4_crossfirst** — advisory list, in this order:

1. the best already-stored content at a **different path** by 32-hash signature overlap
   (>= 4/32), ties broken by size closeness;
2. the second such cross-path candidate;
3. the content at the **same path in the previous selected state** (today's rule);
4. the nearest other earlier same-path version.

*Inputs:* the new content's bytes (in hand at C1), the 32-hash min-hash signature of those
bytes (C1 already computes an 8-hash signature of exactly this shape in C2's cache), the
previous state's tree (path -> content root, already read back through the Store between
states), and C2 metadata (role, canonical length, chain depth) for the candidates.
*Cost model:* 4 candidate probes (metadata only) + exactly **one** prefix trial per object,
as today; the chain budgets and the depth cap are unchanged; the extra cost is the candidate
index lookups, not extra codec work.

| | objects | canonical | stored | ratio |
| --- | --: | --: | --: | --: |
| whole-file lane, R4_crossfirst | 44,148 | 348,460,709 | **35,937,886** | **9.696x** |
| same, with the measured cache kept for objects the rule does not fire on | 44,148 | 348,460,709 | **34,839,722** | **10.002x** |
| measured today | 44,148 | 348,460,709 | 110,941,054 | 3.141x |

Arithmetic: `348,460,709 / 35,937,886 = 9.6964`; `348,460,709 / 34,839,722 = 10.0018`.
Saving against today: `110,941,054 − 35,937,886 = 75,003,168 B (67.6 %)`; against the
combined figure `110,941,054 − 34,839,722 = 76,101,332 B (68.6 %)`.
Outcomes: `prefix 35,407`, `no_eligible 8,706`, `full_wins 28`, `chain_budget 7`.

### Residuals on that recommendation (all measured)

* **Regression against the cache.** Because the first eligible advisory suppresses the cache,
  a rule can *lose* bytes on objects the cache serves better. Over the objects the rule
  decides, R4_crossfirst gives up **269,264 B** (R1: 75,149 B; R3_cross4: 295,424 B) and
  gains **76,370,596 B**. The regressions are real and are the price of the ordering.
* **The combined figure is an estimate, not a simulation.** It assumes the cache behaves on
  non-fired objects exactly as it did in the measured run. It does not: an object the rule
  stores as FULL still enters the cache, and one stored as PREFIX does not, so the cache
  population changes. Labelled as an assumption; the pure no-cache figures (35,937,886) are
  the apples-to-apples comparison and are what I recommend quoting.
* **The rule is a counterfactual.** Nothing in the current product populates the advisory
  list on this lane (§3), so 35,937,886 B is what the rule *would* achieve, not what the
  Store contains.
* **Cross-path recall is a lower bound.** Hash buckets > 400 entries were skipped
  (261,810 lookups) and only the top 8 candidates were kept.
* **The 92 chunked file roots (>= 131,072 B) are outside this rule.** Their role is `Chunk`,
  and `select.rs:247` considers only `advisory.first()` for that role; 1,098 `Chunk` objects
  hold 22,055,499 canonical bytes in the Native lane. Not measured here.
* **stride3 confirmation: NOT RUN.** Nothing in this report is confirmed on the 53-state
  selection. stride1 was not touched, per the guardrail.

---

## 6. Exact commands

    # instrument build (product's own zstd-sys, nothing added to any Cargo workspace)
    T=core/benchmark/fs-bench-pro-storage-content/target
    rustc +1.85.1 --edition 2021 -O /tmp/b2/codecprobe.rs -o /tmp/b2/codecprobe --extern zstd_sys=$T/release/deps/libzstd_sys-9d13631457f814f3.rlib -L dependency=$T/release/deps -L native=$T/release/build/zstd-sys-2e157bf4e41512a3/out
    rustc +1.85.1 --edition 2021 -O /tmp/b2/sigprobe.rs -o /tmp/b2/sigprobe

    python3 /tmp/b2/decode.py            # pack directory -> per-object stored width + base
    python3 /tmp/b2/index2.py            # corpus index (union 44,240 / 371,937,306 B, matches the brief)
    python3 /tmp/b2/join.py              # blake3 predicted ids -> 44,148/44,148 join
    python3 /tmp/b2/validate_proxy.py    # byte-exact re-encode of all 44,148 records
    python3 /tmp/b2/exp1.py              # baseline attribution + delta-able census
    python3 /tmp/b2/exp1c.py             # advisory-vs-outcome census
    python3 /tmp/b2/exp2a.py             # 32-hash signatures (2.2 s)
    python3 /tmp/b2/exp2b.py             # cross-path candidates (8.6 s)
    python3 /tmp/b2/precompute.py        # 44,148 FULL + 118,870 PREFIX frames (13.6 s)
    python3 /tmp/b2/exp3b.py             # all rules, both orderings -> results_rules.json

Baseline Store: `/tmp/base187/sample.sqlite` (apparent 128,864,256 B). No lane was re-run;
no timing was taken; nothing under `core/crates/`, `crates/` or the lane registry was
modified. All artifacts live under `/tmp/b2/` (diagnostic scratch, not evidence).

---

## 7. MEASURED vs NOT RUN

| question | status |
| --- | --- |
| 1. same-path previous version | **MEASURED** — 45,926,982 B, 7.587x, 28,560 objects covered |
| 2. same-path any earlier version | **MEASURED** — 45,230,840 B best case; +696,142 B over Q1 only |
| 3. cross-path similarity | **MEASURED** — 33,210 objects (75.7 %) have a stored cross-path near-duplicate; rule 35,937,886 B, 9.696x |
| 4. ordering in the 4 slots | **MEASURED** — cross-first beats prev-first by 297,925 B; "smallest frame per object" is *worse* by up to 433,443 B |
| 5. chain length limits | **NOT RUN** — no depth sweep; histogram at cap 8 only, hypothesis labelled |
| stride3 confirmation | **NOT RUN** |
| chunked (Native-lane) objects | **NOT MEASURED** (1,098 objects / 22,055,499 B canonical) |
| mechanism: advisory route dead on the construction path | **MEASURED** — 0 of 18,344 bases cross a save boundary |
