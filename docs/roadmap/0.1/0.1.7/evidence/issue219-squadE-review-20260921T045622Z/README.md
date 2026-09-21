# Squad E — independent re-derivation of the #219 campaign's headline arithmetic

> **Status: independent review. Read-only.** No build, no benchmark, no product
> source touched. Exactly one directory was created — this one. Every number below
> was recomputed from **raw files**; no report's prose was accepted as evidence for
> its own claim. The one artifact I exercised beyond reading is the pinned row's own
> `sample.sqlite`, opened `mode=ro` + `PRAGMA query_only=1` and re-parsed from
> scratch (§5).

| | |
| --- | --- |
| reviewer | Squad E (independent) |
| worktree | `/Users/yifanxu/Ephemeral-AI-Lab/layerfs-219-ns10000`, branch `codex/219-ns10000` |
| review directory | `docs/roadmap/0.1/0.1.7/evidence/issue219-squadE-review-20260921T045622Z/` |
| script | [`rederive.py`](rederive.py) — reads only raw files, exits **1** on any mismatch |
| verbatim output | [`rederive-output.txt`](rederive-output.txt) |
| result of the run recorded here | **157 PASS, 41 FAIL, 0 INCOMPLETE** (exit 1) |

Reproduce with:

```sh
cd docs/roadmap/0.1/0.1.7/evidence/issue219-squadE-review-20260921T045622Z
python3 rederive.py > rederive-output.txt 2>&1 ; echo "exit=$?"
```

The script is the review. This README explains the verdicts and records what could
not be checked. Non-passing rows are kept, not dropped — §"Every non-passing row"
below lists all 41 failing checks by name.

---

## 1. Verdict per row

| # | row | verdict | basis |
| --- | --- | --- | --- |
| 1 | seven `SaveProfile` buckets + remainder, ns and shares of the accept span and of `operation_ns` | **PASS** | every ns and every share to 4 dp recomputed from `counters.tsv` + `diagnostic-receipt.json`; `shares.tsv` itself recomputed line by line |
| 2 | `sql_ns + commit_ns` share; `resolve + delta + group` share; are "60.3 %" and "0.70 %" supported | **PASS** (and **FAIL** on one other number in the same section — the 349.9 ms `build/encode` cell) | 60.3084 % of span / 60.2610 % of operation; 0.7013 % / 0.7008 % |
| 3 | work identity: 15 shared counters, 0 moved, canonical totals equal | **PASS** | recomputed from the two receipts; the 15/0/15/0 split and both canonical totals reproduce exactly |
| 4 | cadence calibration: 13,084 ns, **7,959 ns**, the 3.9–6.4 % bound, and §13.2's own correction | **FAIL** on 7,959 ns (should be 8,014.74); **PASS** on 13,084 ns, on the 3.9–6.4 % band, and on §13.2's honesty | `cadence-calibration.json` arms; the raw JSON's own derived block is internally inconsistent |
| 5 | pack-append amplification: appends/pack, the factor, and the two cross-method agreements | **PASS** on the mechanism (**7.5917× re-derived from the pinned row's own bytes**, 16,802 and 16,595 both confirmed); **FAIL** on §13.1 (14.44, 7.72×, "UPDATE calls"), on §14.3's band, and on the archived parser | new independent pack-directory parse, §5 |
| 6 | v0.1.6 comparison figures from raw `perf.jsonl` | **PASS** on all six required figures; **FAIL** on the derived `CPU/wall = 1.8934` (it is 1.8931) | `perf.jsonl` line 2 `records[0]` |
| 7 | the seven "corrections to the handoff's premises" | **7.3, 7.4, 7.5, 7.7 PASS**; **7.1 and 7.2 PASS on substance with one non-reproducible number each**; **7.6 PASS on substance, one number wrong** | each checked against the raw field it names |
| 8 | Squad D: the 1-8 / 32-256 / 1024-8192 bands are **weights**, not byte sizes | **PASS**, and Squad D's whole ladder reproduces byte-for-byte | `benchmark/fs-bench-pro/workload/main.rs:424-449` + `358-363`, re-implemented independently |
| 9 | arithmetic in the reports that can be falsified | **FAIL — 17 items** (15 in the collected list + 2 standalone) | see §"Every non-passing row" |
| 10 | §14.2 "two methods agree to the unit" | **PASS** on the equality; **FAIL** on "share no input" | `ranked_table.tsv` vs `counters-packcounters.tsv` |
| 11 | §15/§16 honesty about the campaign's own errors and about prior art | **PASS** — the superseded reasoning (§15.0), the prior art (§16.3) and now the withdrawn artifact (`reconciliation.json` carries an explicit `SUPERSEDED` marker) are all recorded; the stale framings in §12.2/§13.4 are still in the body, and §18.4 records them | §15.0, §16.3, §18.4; `reconciliation.json` sha `c06780091c6c4a44` |
| 12 | §3's phase matrix sourcing | **FAIL** — nine v0.1.7 cells are the *pinned* row's values under a preamble that declares the diagnostic row, and one cell divides one row's numerator by the other's denominator | §3 preamble and the two rows' `phases-perf.json` |
| 13 | Squad C cadence pre-registration (was INCOMPLETE at start; README landed 04:57:42Z) | **PASS** on registration, on "not run", on the identity block, the commit decomposition, the split, the page arithmetic and the git provenance; **FAIL** on one mislabelled rate and on PR-C1's upper bound | Squad C `README.md`, `pre-registration.md`, `commit-decomposition.txt`, `store-geometry.txt`; `git show`/`git log -S` |
| 14 | Squad A §17 (added mid-review, 04:59:19Z) | **PASS** on the rate-range correction, the PR-C1 table and the #216 provenance correction; **FAIL** on attributing the 246.5 ms residual to `advance_pack_if_moved` | §17.2–§17.5 |
| 15 | Squad A §18 — the errata written in response to this review (05:08:57Z) | **PASS** on the substantive corrections (all five corrected values and the nine-cell matrix reproduce cell by cell); **FAIL** on four smaller items: the arithmetic-item count, the row count, the uncorrected §7.1 seal/image sentence, and the untouched body text | §18.1–§18.6 |

At launch Squad C had no `README.md`; the brief said to record that row INCOMPLETE —
it landed at 04:57:42Z, before this review finished, so it was reviewed instead
(row 13). The corpus changed **four times** while this review was running
(Squad A's README 598 → 781 → 880 → 971 lines, the last revision adding the §18
errata written in response to this review; `pack-append-synthesis.json` rewritten;
`reconciliation.json` marked superseded; Squad C completed). Verdicts apply to the revision digested in
`rederive-output.txt` §"corpus digest"; the four report files this review rests on:

| file | sha256 (first 16) | mtime (UTC) |
| --- | --- | --- |
| Squad A `README.md` (971 lines, includes §15–§18) | `72803c1b46b506ea` | 2026-09-21T05:08:57Z |
| Squad A `reconciliation.json` (superseded marker) | `c06780091c6c4a44` | 2026-09-21T05:08:57Z |
| Squad B `README.md` (844 lines) | `c3937e3ff163f757` | 2026-09-21T04:50:20Z |
| Squad C `README.md` (286 lines) + `pre-registration.md` | `c7cbe138e4764adc` / `5e11ac9807e83cfa` | 2026-09-21T04:57:42Z / 04:57:22Z |
| Squad D `README.md` | `c0fd904cc5647395` | 2026-09-21T04:45:01Z |
| pinned row `sample.sqlite` (re-parsed) | `03918d61a9f004b2…` | matches `manifest.json` |

---

## 2. What I confirmed, and by what re-derivation

**The buckets (§1, row 1).** From `counters.tsv`: commit 1,351,360,518 / sql
781,237,539 / full 245,777,362 / place 79,188,910 / group 18,859,112 /
resolve.pooled 5,858,368 / delta 82,960 → **2,482,364,769**, which is also the
instrument's own `pipeline.profile_total_ns`. Shares recomputed by me:
70.1995 % of `accept_span` (3,536,155,750) and 70.1444 % of `operation_ns`
(3,538,935,458); the remainder is `span − seven = 1,053,790,981` = 29.8005 % /
29.7771 %. Every one of the 21 bucket checks and all seven `shares.tsv` rows match
to the printed digit. `resolve.eligible/acquire/cost/reuse` are all zero, so
`resolve` is entirely `pooled_ns`, exactly as the report says.

**The database and decode terms (§2, row 2).** `sql_ns + commit_ns =
2,132,598,057` = **60.3084 %** of the span and **60.2610 %** of `operation_ns`;
`resolve + delta + group = 24,800,440` = **0.7013 %** / **0.7008 %**. "The database
term is 60.3 %" and "decode is 0.70 %" are both **supported** (60.26 % rounds to
60.3 %; 0.70 % is exact to two decimals). 77,762.7 ns/commit, 1.4527 objects/commit
and 17,330.6 bytes/commit also reproduce.

**The work identity (§10, row 3).** Both receipts intersected: **15** shared
counters, **0** moved, **15** published only by the diagnostic (all
`pipeline.profile_*` + `pipeline.accept_span_ns`), **0** only by the pinned row.
`canonical_bytes_total` and `canonical_objects_total` are 302,231,057 and 25,245 in
both, the per-role object map is identical, both rows carry 13 gates, all PASS, and
both are `status: PASS`. The identity diff set is exactly
`{harness_binary_sha256, source_commit, source_dirty_files, started_utc}` — four, as
claimed. This is the campaign's one clean controlled contrast and it holds.

**The pack-append amplification (§13–§16, row 5) — the strongest result in the
corpus, and I re-derived it myself.** I did not trust `pack_stats.json`. I read
`core/crates/layerfs-storage/src/pack/layout.rs` (header 16 B, 16-byte directory
entry `body_start/encoded/decoded/flags`, 4-byte start-only entry for the compact
whole-file lane, `WHOLE_FILE_COMPACT_DROP = 8`) and re-parsed all **1,250 pack
directories** out of the pinned row's own `sample.sqlite` (sha256 checked against
`manifest.json`). Result, byte for byte:

| lane | packs | writes | bytes written | final bytes | factor |
| --- | ---: | ---: | ---: | ---: | ---: |
| WholeFile | 102 | 9,444 | 1,260,350,294 | 24,613,232 | 51.21× |
| PooledMetadata | 3 | 207 | 25,584,656 | 726,096 | 35.24× |
| Native | 1,142 | 7,130 | 1,004,612,191 | 276,058,548 | 3.64× |
| Ordinary | 3 | 21 | 2,318,196 | 625,356 | 3.71× |
| **all** | **1,250** | **16,802** | **2,292,865,337** | **302,023,232** | **7.5917×** |

**0 format violations**: every directory is continuous, every directory start equals
`16 + entry·group_count`, no pack has trailing bytes, and the assembled length at the
last write equals the stored body length for all 1,250 packs. `SUM(length(data)) =
302,023,232` equals the receipt's `pack_bodies_bytes` exactly. The 302,023,232 figure
is therefore confirmed twice over and the old `304,427,008` (dbstat page bytes) is
dead. `15,552 / 1,250 = 12.4416` appends per pack reproduces; the equal-increment
model gives 6.7208×–7.2208×; the **measured** value is 7.5917×, which is what §15/§16
now quote. The two cross-method agreements also hold: `1,250 + 15,552 = 16,802` is
`ranked_table.tsv` row 2's call count, and `pipeline.statements = 16,595` is row 5's.

**Squad D's central claim (row 8).** `namespace_relative_weight`
(`benchmark/fs-bench-pro/workload/main.rs:424-449`) returns `lower + floor((2·role+1)·width/(2·count))`
from the triples `Tiny (1,8)`, `Small (32,256)`, `Medium (1_024,8_192)`; the byte
size is a separate Hamilton pass at `:358-363` (`size = 1 + floor(distributable ·
weight / weight_sum)`). I re-implemented both from the source and got
`weight_sum = 2,555,546`, `distributable = 199,990,101`, `extra = 4,214`, bands
79–627 / 2,505–20,035 / 80,684–640,537, 456 chunked files, 9,444 whole-file, and role
canonical bytes 275,868,750 and 24,652,248 — **identical to the v0.1.7 receipt**
(`ns17-final`, chunk 275,868,750, whole-file 24,652,248, file-state 456). The bands
are weights, not byte sizes: **PASS**, and the closure to the byte is real.

**The v0.1.6 row (row 6).** From `perf.jsonl` line 2 `records[0]`:
`layerstack_init_ns = 944,880,958`; user 1,036,730,875 + system 752,036,959 =
**1,788,767,834**; `initialize_admission_transactions = 73`;
`store_canonical_objects = 25,158`; `store_canonical_bytes = 302,182,831`; max
transaction 4,149,860. All six required figures are exactly right, and the derived
ratios 3.745× / 1.837× / 1.0035× / 1.0002× / 238.05× / ~239× reproduce.

**The premise corrections (row 7).** 7.3 (page-cache evidence is on the v0.1.6 row:
543,040 / 34,604,032 against a 33,554,432 target, and *both* v0.1.7 receipts publish
neither field), 7.4 (`profile: ""` is `("profile", case.profile.to_string())` at
`src/main.rs:504`, and `registry.rs:399-400` defines the column as the
fixture-profile variant "empty when the row has none"), 7.5 (the pinned `run.json`
really does carry `registry_self_check.status: "FAIL"`, `exit_code: 1`, expected
`[…,2,4]` vs actual `[…,2,5]`, while the row itself passed) and 7.7 (v0.1.6
`layerfs-perf-v1` under `benchmark/fs-bench-pro` vs v0.1.7 `layerfs-core-receipt-v1`
under `core/.../fs-bench-pro-storage-content`, `cache_contract: null` vs a
dewarmed row) all check out.

**§15.0 and §16.3 are honest.** §15.0 keeps the falsified first version of §15 in the
record and states why it was wrong; §16.3 says the amplification "is not a discovery",
quotes the stage-6-history-209 round's own numbers, and I verified those numbers in
that round's README (45,794 appends, 177,640-byte average, 262,112 maximum, ~8.1 GB,
58.0 µs COMMIT at the same fsync-free profile). I also verified the two provenance
claims by git: `7075f338` (#216) touches none of `cas/lifecycle.rs`,
`cas/placement.rs`, `cas/pool_lane.rs`, and its parent already contains
`advance_pack_if_moved` (line 151), `maybe_commit` (160) and the "never outlives the
step" rule (165); `git log -S` finds `eb319aaa9` (2026-09-21 02:56:40). The
`LAYERFS_STORAGE_COMMIT_EVERY` knob appears **0 times** anywhere under `core/` or
`crates/`.

---

## 3. Every non-passing row, with its reason

Names are the check ids in `rederive-output.txt`.

### 3.1 Squad A §2/§3 arithmetic

* **`R2.build_encode_ms`** — §3's cell "build / encode 349.9 ms =
  `full_ns` + `delta_ns` + `group_ns` + `resolve_ns` + `place_ns`". The sum is
  349,766,712 ns = **349.767 ms**, not 349.9 ms.
* **`R12.matrix_row_mix`, `R12.matrix_children_mix`** — §3 declares "v0.1.7 = the
  diagnostic row above", but nine of its v0.1.7 cells are the **pinned2** row's
  values: acquisition 8.1 ms (diagnostic 5.2), preparation 913.2 (898.8), setup/store
  open 3.6 (2.7), verification 338.7 (338.1), cleanup 459 ns (541), command window
  4,841.6 (4,791.6), run wall 5,045.4 (4,986.1), handoff 41.7 (44.3) and the timing
  children 11.2 ms (11.7). The last of those is worse than a mix: the cell
  "11.2 ms of 3,538.9 ms = 0.32 %" divides **pinned2's** numerator by the
  **diagnostic's** denominator. The §8 gate claims "every matrix cell carries a field
  name or `NOT_MEASURED`"; the field names are right, the *row* is not stated.

### 3.2 The cadence calibration (row 4)

* **`R4.middle`** — "the fixed per-transaction cost is
  `(454,763,917 − 444,408,875) / (1,366 − 74) = 7,959 ns`". The division is
  **8,014.74 ns**. `cadence-calibration.json` carries the same wrong 7,959 in its own
  `derived` block, so this is a raw-file error the report copied, not a transcription
  slip.
* **`R4.applied_low`, `R4.applied_high`** — the same JSON's
  `applied_to_17378_commits_ns = [138,293,022, 227,393,512]` is not the product of its
  own stated range: 7,959 × 17,378 = **138,311,502** and 13,084 × 17,378 =
  **227,373,752** (implied 7,957.94 and 13,085.14 ns/txn). Three of the four numbers in
  that `derived` block cannot be reproduced from the file's own arms.
* **`R9.01`, `R9.02`** — the same two items, kept in the collected list.

The **13,084 ns** value (13,083.51 before rounding) and the **3.9–6.4 %** band both
reproduce — the band holds with the file's own applied values (3.9078 %–6.4255 %) and
also with the corrected middle value (3.936 %). So the headline *bound* survives; the
intermediate that produces its lower end does not. Row 4 is FAIL because the brief
asked for both numbers, and one of them is not reproducible.

### 3.3 The pack-append section (row 5)

* **`R5.section13_writes_per_pack`, `R5.section13_factor`** — §13.1 says "Squad B
  measured **16,802 UPDATE calls** … **14.44 writes per pack** … at `k = 14.44` that
  is **7.72×**: approximately **2.34 GB**". 16,802 / 1,250 = **13.4416**; 14.44 appears
  in no raw file. The 7.72× is the `(k+2)/2` model applied to *all* writes per pack,
  which counts the 1,250 `insert_pack` creations as if each pack were rewritten an
  extra time; the measured factor is **7.5917×** (2.29 GB). §16.1 corrects the
  measured-vs-derived status of the amplification but leaves §13.1's 7.72×/2.34 GB and
  its "14.44" standing.
* **`R5.section13_label`, `R9.11`** — the same §13.1 table labels rank 2 as
  `UPDATE object_packs SET data = ?2 …` with 16,802 calls. `ranked_table.tsv` row 2
  is `UPDATE … + INSERT INTO object_packs …` and its own basis text says "the create
  form is 1,250 of the 16,802 calls". Squad A read `ranked_table.txt`, whose
  fixed-width truncation hides the `+ INSERT` half (Squad C §6.1 then repeated the
  mislabel).
* **`R5.ranked_label`** — `ranked_table.tsv` still labels row 2 `EXACT` after
  Squad B's §12 downgraded it to `DERIVED-EXACT`; Squad A §16.5 quotes the corrected
  label, the archived table does not.
* **`R5.archived_parser`** — the evidence directory's `pack_parse.py` **cannot
  reproduce its own `pack_stats.json`**. Its whole-file branch computes each group's
  body as `directory_start − 8`; the directory stores absolute starts, so the
  assembled length it computes for pack 5 is 12,313,704 against a stored body of
  253,041 and its own `assert acc == len(data)` fires on the first whole-file pack.
  I verified the *numbers* are right by re-deriving them myself (they are, to the
  byte), and the per-pack dump it wrote to `/tmp/squadB/packs.json` matches my
  figures — so the archived script is not the script that produced the archived
  output. A reader who reruns the directory gets an `AssertionError`, not the table.
* **`R9.09`, `R9.10`, `R9.12`** — the same three items plus §14.3: its
  "6.72×–7.22×, now measured input and derived factor" band **excludes** the measured
  7.5917×. §16.1 supplies the measured value but does not retract §14.3's band.

### 3.4 The v0.1.6 comparison and the premise corrections (rows 6–7)

* **`R6.cpu_per_wall` / `R9.04`** — §4's table prints `CPU / wall = 1.8934` for
  v0.1.6. 1,788,767,834 / 944,880,958 = **1.8931**. (The v0.1.7 side, 0.9286,
  is right.)
* **`R7.6.diagnostic_divisor` / `R9.05`** — correction 7.6 says "at the
  diagnostic row's measured 0.9286 the same arithmetic gives **1,926.5 ms**";
  1,788.8 / 0.9286 = **1,926.34 ms**. The correction's substantive point (use the
  row's own ratio, the handoff's 1.88× holds) is right, and 1,788.8 / 0.94 = 1,902.98 ms
  reproduces the handoff's ~1,903 ms.
* **`R7.1c.seal_and_image`** — correction 7.1's last sentence asserts "The product
  seal `b3e3cb7453…` and the image `sha256:d152ced8…` **do match**". **No v0.1.7
  artifact records a product seal or an image at all.** `b3e3cb7453` occurs only in
  the v0.1.6 (root `crates/`) family — `perf.jsonl`, the two verification files, the
  binary-archive identity and two prose files — and the v0.1.7 rows record
  `product_lock_sha256` (the `core/Cargo.lock` hash), a different field. The first
  half of 7.1 (source commit and dirty flag) is verified and right; the second half is
  an assertion with no raw field behind it. See §4.
* **`R7.2.sixteen_times` / `R9.06`** — correction 7.2 says the true byte gap is
  "**sixteen times closer**" than the handoff's "within 0.33 %". The two gaps are
  0.3346 % and 0.0160 %, a ratio of **20.96×**, not 16×. The correction's substance
  (301,171,810 is `pipeline.content_bytes`; the canonical total is 302,231,057;
  48,226 bytes **above** v0.1.6, not below) is verified and right.
* **`R9.scaled_floor`** — §12.1 prints `396,227,057 × 302,231,057 / 268,435,456 =
  446,106,000 ns`; the exact value is **446,111,419 ns**, which is what
  `blob-calibration.json` itself carries. §12.1's residual (1,686,492,057) and the
  JSON's (1,686,486,638) differ by the same 5,419 ns. Harmless to the 4.78× conclusion,
  but §12.1 and its own raw file disagree.
* **`R9.synthetic_51279`, `R9.07`** — §12.2's ledger row "per-commit engine cost —
  **not anomalous** — synthetic **51,279 ns/txn** vs product 77,763 ns/txn, 1.52×"
  cites "§12, §9". **51,279 appears in no raw artifact of the #219 corpus** — I walked
  all four squad directories and the string occurs only in Squad A's own README prose.
  It is presented as the synthetic per-commit cost and
  it carries a verdict; it is unsourced. Squad B's own measured per-commit work is
  112.3 µs per seal cycle, and Squad C's rerun of the same statement gives 26,000 ns
  median for the append — neither is 51,279 ns/txn.

### 3.5 Squad A §14.2, §15/§16 and §17 (rows 10, 11, 14)

* **`R10.independence`** — §14.2's "**Two methods that share no input** agree exactly
  on both". The agreement is real (16,802 = 16,802, 16,595 = 16,595) and §14.2's table
  and §16.5's restatement are supported by the two files. But the methods are not
  input-disjoint: Squad B counted **groups** in the pinned store and multiplied by the
  one-group-one-write invariant; the product counter counts **writes**, in the other
  (work-identical) diagnostic run. Their agreement validates Squad B's `select_many`
  invariant — valuable, and exactly what §16.5 says in its last line — but it is one
  fact checked twice, not two independent measurements.
* **`R11.superseded_marking` — PASS, after a fix made during this review.** At
  04:49:35Z `reconciliation.json` carried the verdict §15.0 says was falsified
  ("*The amplification inflates `sql_ns` … and NOT `commit_ns`*", plus
  "largest_unattributed_term: `commit_ns` at 34.2 % … an INCOMPLETE row, not a
  pass"), was listed in no raw-file table, and contradicted §15. That was the
  finding. The file now (sha `c06780091c6c4a44`, 05:08:57Z) carries an explicit
  `SUPERSEDED` key that retracts the verdict, names the mechanism and records the
  246.5 ms as NOT_MEASURED; the stale text is deliberately kept as the record,
  exactly as §15.0 keeps its own falsified first version. Re-checked: **PASS**.
* **`R11.novelty_overclaim`** — §16.3 corrects §13's framing in a later section, but
  §13.1 ("**Derived amplification** … *derived* — the store keeps only final pack
  lengths, so the intermediate lengths cannot be read back" — §16.1.2 says that is
  false), §13.4 ("the only candidate this campaign has found that can carry the
  residual") and §12.2 ("what survives is the **per-row metadata structure** … the
  campaign should attack next") all still stand unamended, and §15.3/§16.4 now name a
  different next step (seal coalescing). A reader who stops at §12.2 or §13.4 gets the
  superseded answer.
* **`R14.diff_attribution`** — §17.2's table: "`profile_commit_ns` − Squad B's
  `COMMIT` total = **246,460,518**, attributable only to
  `advance_pack_if_moved` (1,250 statements) — NOT_MEASURED". 1,250 statements at
  Squad B's ~1 µs price can carry ~1.25 ms of a 246.5 ms residual; the cell both
  attributes it and labels it NOT_MEASURED. Squad C states the same residual
  correctly ("flagged rather than explained").
* **`R9.14`** — Squad B's §4/§13.1 divide by "the **3,585.8 ms** `operation_ns`".
  The measured row it names, `ns17-pinned2`, has `operation_ns = 3,575,832,667`
  (3,575.8 ms); 3,585.5 ms is `ns17-final`. The 54.6 % share is therefore taken
  against a denominator that is not the named row's (the shift is 0.28 %, so the share
  moves from 54.6 % to 54.7 %).
* **`R9.15`** — `ranked_table.txt`'s "sum of the 11 rows with an EXACT call count and
  a MEASURED ns/call: 1,956.7 ms". The eleven products sum to **1,956.6 ms**
  (1956.615 before rounding).
* **`R9.13`** — Squad B §12(e)(4): "`commits − appends = 17,378 − 16,802 = 576`, and
  576 is 207 (`reserve_ordinals`) + 207 (`write_value_groups`) + 1 + 1 + **367**".
  That sum is **783**. 576 = 367 + 207 + 1 + 1; one of the two 207s does not belong.
  Squad C's independent decomposition (§1) has the same terms summed correctly
  (1 + 16,595 + 207 + 207 + 367 + 1 = 17,378, where the two 207s live on different
  sides of the 16,802 line), which is how I could localise the error.

### 3.6 Squad C (row 13)

* **`R13.readme_factor`** — §3's last line: "Per byte of final pack body the row pays
  4.474 ns; per byte the pager is handed (DERIVED, **7.28×**) it pays **0.589 ns**".
  4.4744 / 7.2843 = 0.6143; 0.589 = 4.4744 / **7.5917**. 0.589 is Squad B's measured
  factor, and Squad C's own raw `commit-decomposition.txt` labels it that way ("Squad
  B"); the README sentence attaches this squad's 7.28× equal-growth estimate to it.
* **`R13.prc1_prediction`** — PR-C1's `pipeline.commits` prediction of **1,600–2,800**
  is justified as "≤ 5 lane-tail steps per wave (≈577 waves) + 207 ordinal + 207
  value-group + 367 flush + 2 fixed". That mechanism permits 5 × 577 = 2,885 wave
  steps, i.e. up to **3,668** commits. It is labelled a prediction, so this is a loose
  bound rather than a false measurement — but the upper bound is not derived from the
  sentence that claims to derive it.
* Everything else in Squad C's row reproduces: the 17,378 decomposition sums; the
  records-per-group histogram sums to 16,595 (10,081 single, 5,662 double);
  `store-geometry.txt` agrees with my own SQL on 1,250 packs / 302,023,232 bytes /
  207 groups / max stamp 9,444; the three-way split of `commit_ns` sums to
  1,351,360,518 at both ends (138,311,502 + 886,153,042 + 326,895,974 and
  227,373,752 + 962,824,311 + 161,162,455); the per-commit micro-figures
  (77.76 = 8.0–13.1 + 51.0–55.4 + 9.3–18.8 µs) divide correctly; the page arithmetic
  (537,062 / 559,782 / 73,736 pages) is right; the charge sites are exactly
  `write::commit` at `:169`/`:256` with `SaveProfile::charge` at `:170`/`:257`
  and **no charge in `begin_write`**; the pre-registration names one difference, an
  identity, expected movement and refutations; **no receipt is newer than the
  pre-registration**, so nothing was run.

### 3.7 Squad A §18 — the errata written in response to this review (row 15)

At 05:08:57Z, after this review directory appeared, Squad A appended **§18 ERRATA**,
which declares itself to win wherever it disagrees with an earlier section. I checked
it as a document in its own right:

* **`R15.errata_exists`, `R15.errata_values` — PASS.** All five corrected values are
  the ones I re-derived: 14.44 → **13.4416**; "16,802 `UPDATE` calls" → **15,552
  `UPDATE` + 1,250 `INSERT`**; 7.72× → **7.5917× / 2,292,865,337 B**; 7,959 →
  **8,014.74**; CPU/wall 1.8934 → **1.8931**.
* **`R15.nine_cells` — PASS.** §18.1's nine-cell table reproduces cell by cell from
  the two `run.json` files (5.2 / 898.8 / 2.7 / 338.1 / 541 / 4,791.6 / 4,986.1 / 44.3 /
  11.7 for the diagnostic row; 8.1 / 913.2 / 3.6 / 338.7 / 459 / 4,841.6 / 5,045.4 /
  41.7 / 11.2 for pinned2), and it correctly labels the timing-children cell as mixed.
* **`R15.item_count` — FAIL.** §18's opening sentence says row 9 FAILs with **18**
  arithmetic items. This review has **17** R9 items (15 in the collected list + 2
  standalone). The miscount was mine in the previous revision of this README and Squad A
  repeated it; the corrected figure is 17.
* **`R15.row_count` — FAIL (cosmetic).** §18 says "the fifteen rows it was given". The
  brief listed nine required rows; this review defines rows 1–14.
* **`R15.seal_not_corrected` — FAIL.** §18.6 defers to §4 of this review but does
  **not** correct §7.1's sentence "The product seal `b3e3cb7453…` and the image
  `sha256:d152ced8…` do match", which remains in the body with no raw field behind it.
  The one claim this review could not verify is the one item the errata leaves standing.
* **`R15.body_unchanged` — FAIL (a documentation-style choice, not a live error).**
  §13.1 still reads "14.44 writes per pack" and "7.72x / ~2.34 GB", §14.3 still bands
  "6.72x–7.22x", §12.1 still prints 446,106,000 ns, §12.2 still prints 51,279 ns/txn,
  §4 still prints 1.8934, §7.6 still prints 1,926.5 and §3 still prints 349.9 ms. §18
  declares itself to win on disagreement, so a reader who reaches §18 is correctly
  served — but the numbers are still in the body, and a quotation lifted from §13.1 or
  §12.2 alone is still wrong.

---

## 4. The single most load-bearing claim I could NOT verify

**That the two arms are the same product.** The handoff's §1, and Squad A's §7.1 and
§10 after it, assert that both rows carry product seal `b3e3cb7453…` and image
`sha256:d152ced8…`. **No v0.1.7 artifact records either field.** Both v0.1.7
receipts record `product_lock_sha256` (the `core/Cargo.lock` hash) and a harness
binary sha256; the seal string occurs only in the v0.1.6 (`crates/`) family and in
prose. The handoff itself says the arms "live in different workspaces (`crates/` vs
`core/`), so their compilation/dependency seals differ by construction" — so a
*shared product seal* is not something a reader can check, and §10's "same product
seal, same image" is the one clause of the campaign's cleanest contrast that has no
raw field behind it.

Why it is load-bearing: the campaign's stated target — the **1.88× CPU term** and
every cross-arm ratio in §3/§4 (3.745× wall, 1.837× CPU, 60.3 % vs 73 transactions) —
rests on the two arms being the same product doing comparable work. The within-row
claims (60.3 %, 0.70 %, 7.5917×, the buckets, the work identity between the two
v0.1.7 rows) do **not** rest on it, and those are the claims I could verify. What the
campaign needs is one recorded field per row — the product seal and the image id in
the v0.1.7 receipt's `identity` — or an explicit statement that the v0.1.7 seal is a
different value and the comparison is cross-product by construction.

Runner-up: §14.2's "two methods that share no input" (§3.5) — verifiable as an
*agreement*, not verifiable as *independence*.

---

## 5. What I did to the pinned store, exactly

```
file:   core/benchmark/fs-bench-pro-storage-content/benchmark-results/issue219/
        ns17-pinned2-20260921T031259Z/pipeline-namespace-10000/sample.sqlite
mode:   sqlite3 URI mode=ro, PRAGMA query_only=1, read-only SELECTs
sha256: 03918d61a9f004b292deafeadbea6992f870b8bdd96b3a631d4bc3309408db29
        (recomputed and compared with the row's manifest.json)
```

Two statements were issued against it: `SELECT COUNT(*) FROM object_packs` and
`SELECT SUM(length(data)) FROM object_packs`, plus `SELECT pack_id, data FROM
object_packs ORDER BY pack_id` streamed once for the directory re-parse. Nothing was
written, no copy was made, and `/tmp/squadB/*` (Squad B's copies) was only read.

---

## 6. What a reader should do with this review

1. **Quote the pack-append amplification.** It is the one number in this campaign
   that I re-derived from the primary artifact and could not break: 7.5917×,
   byte-exact, on the pinned row's own store, with the per-lane skew that localises it
   (WholeFile + PooledMetadata rewrite 56.1 % of the bytes to persist 8.4 % of them).
2. **Do not quote** §13.1's 14.44 / 7.72× / 2.34 GB / "16,802 UPDATE calls",
   §14.3's 6.72–7.22× band, §12.2's 51,279 ns/txn, cadence-calibration.json's 7,959 ns
   and `[138,293,022, 227,393,512]`, §4's 1.8934, §7.6's 1,926.5 ms, §7.2's "sixteen
   times", §12.1's 446,106,000 ns, or §3's 349.9 ms — each is listed above with the
   raw value that replaces it.
3. **Read Squad A §18 before quoting Squad A.** The errata (written at 05:08:57Z in
   response to the first revision of this review) corrects every material number above
   and declares itself to win on disagreement — but it does not correct §7.1's
   seal/image sentence, and the body text still carries the old numbers.
4. **Re-run `rederive.py` after any further edit to the corpus.** Squad A's README
   changed three times (598 → 781 → 880 → 971 lines), `reconciliation.json`
   gained its superseded marker, and Squad C's directory was still being written while
   this review ran; the verdicts here are pinned to the digests in §1.
5. **Treat the 3.9–6.4 % cadence bound as the price of the COMMIT boundary only**, as
   Squad A §13.2 and Squad C §5 now both say — and note that Squad A §9's own raw
   JSON prices its lower end with a division that does not produce 7,959.
6. **Do not treat §14.2 as independent confirmation** of the pack-write count; treat
   it as the product counter agreeing with Squad B's group-count-times-invariant.

*No build was run. No benchmark was run. No pre-existing file was modified. One
directory was created: this one.*
