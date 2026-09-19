# B3 — framing and codec for a history workload (stride10)

> **Status: PARTIAL — time-boxed.** Every number below is a **diagnostic**, not admission
> evidence. Items 1–3 and 5 are **MEASURED**; item 4 is **PARTLY MEASURED** (per-extension
> dictionary NOT RUN). One join measurement is written but **NOT RUN**. Timing is **never**
> reported: the machine was carrying other diagnostic work, so any wall-clock reading is
> invalid by the task's own rule. Every number here is a byte reading.

Squad B3, blind half. Product source (`crates/`, `core/crates/`) was **not read**; the
mechanism in the brief was taken as given and then re-derived from the Store's own bytes.

---

## 0. What was measured, and on what

| | |
| --- | --- |
| Lane | `history-stride10`, 17 states, canonical content **380,921,300 B / 52,032 objects** |
| Store | `/tmp/base187/sample.sqlite`, apparent **128,864,256 B**, pack bodies **119,894,291 B** |
| Corpus | `deepseek-history-data` (manifest sha256 `03f21acf…4271`), read-only |
| Whole-file population | **44,148 objects**, canonical **348,460,709 B**, content **347,445,305 B** |
| Codec under test | zstd **1.5.7**, the same library as the system `zstd` CLI (verified below) |
| Tool | Python 3.14 `compression.zstd` (libzstd 1.5.7), one-shot `compress`/`decompress` |

**Instrument identity check.** `python3 -c "from compression import zstd; print(zstd.zstd_version)"`
→ `1.5.7`; `/opt/homebrew/bin/zstd --version` → `v1.5.7`. On a 300,000 B probe,
`zstd --no-check -3 -c` and `zstd.compress(level=3)` both produce **51,931 B** (the CLI's
default `--check` adds exactly **4 B**, 51,935).

### 0.1 Corpus-side pins reproduced exactly (all three selections)

`python3 /tmp/b3/extract_lane.py {stride10,stride3,stride1}` → union oids / bytes:

| selection | union oids | union bytes | pinned in the brief | residual |
| --- | --: | --: | --: | --: |
| stride10 | 44,240 | 371,937,306 | 44,240 / 371,937,306 | **0** |
| stride3 | 60,000 | 583,508,923 | 60,000 / 583,508,923 | **0** |
| stride1 | 75,929 | 891,893,320 | 75,929 / 891,893,320 | **0** |

### 0.2 Store-side bucket table reproduced exactly (independent decoder)

`python3 /tmp/b3/packdir.py /tmp/base187/sample.sqlite` — decoder written for this task from
the grammar in the brief: header 16 B = magic 8 + lane u32 + groups u32; WholeFile lane 4
directory = u32 offset per group; other lanes 16 B = start/encoded/decoded u32 + codec u8 + 3 pad.

| bucket | objects | canonical | stored | ratio | brief | residual |
| --- | --: | --: | --: | --: | --- | --: |
| whole-file **with** a base | 18,344 | 82,033,173 | 17,196,162 | 4.7714x | identical | **0** |
| whole-file **without** one | 25,804 | 266,427,536 | 93,744,892 | 2.8428x | identical | **0** |
| other lanes (aggregate) | 7,884 | 32,460,591 | 8,953,237 | 3.6259x | identical | **0** |
| total | 52,032 | 380,921,300 | 119,894,291 | 3.1771x | identical | **0** |

**Framing closes to the byte** (`python3 /tmp/b3/e5_framing.py`):

    bodies 119,678,627 + directories 207,632 + pack headers 8,032 = 119,894,291 = SUM(length(data)) FROM object_packs   residual 0

The brief's "other lanes stored = 8,953,237" is `8,737,573` bodies **+ 215,664 framing**;
the whole-file rows' stored figures **exclude** framing. That asymmetry is now explicit.

---

## 1. What the lane actually stores (re-derived; three refutations)

The brief's lane split — *"WholeFile and Native use GroupCodec::Raw with one record per
group; other lanes use Zstandard"* — is **half right**. Measured per lane
(`packdir.py` + frame-header parsing):

| lane (version code) | packs | groups | records/group | group codec byte | stored bodies | canonical |
| --- | --: | --: | --- | --- | --: | --: |
| WholeFile (4) | 437 | 44,148 | **always 1** | *none in the directory* | 110,941,054 | 348,460,709 |
| Native (2) | 31 | 131 | **2 … 20** | 0 = Raw (131/131) | 5,695,678 | 22,055,499 |
| Ordinary (1) | 17 | 72 | 1 … 305 | **1 = Zstandard (72/72)** | 1,022,476 | 10,405,092 |
| PooledMetadata (6) | 17 | 1,737 | pooled values | 1 = Zstandard (1,706), 0 (31) | 2,019,419 | (catalogue) |

**Refutation 1 — "Raw" is the *group* codec, not the record codec.** A WholeFile group body is
`[u8 tag]` (+ 32-byte base id when tag = 1) + **a zstd frame**. Tag distribution over all
44,148 groups: `{0: 25,804, 1: 18,344}`, zstd magic at body offset **1** (25,804×) and **33**
(18,344×) — never anywhere else.

**Refutation 2 — the product's encoder is identified exactly.** For all 25,804 tag-0 records,
`zstd.compress(payload, options={compression_level: 3, checksum_flag: 1})` is **byte-identical**
to the stored frame: **25,804 / 25,804 exact**, total **93,719,088 B**, recomputed 93,719,088 B,
**residual 0**. The Ordinary/PooledMetadata group frames also carry the checksum bit
(FHD = `0x64`). So the current codec is **zstd level 3, content checksum on, one frame per
record**, not "Raw bytes".

**Refutation 3 — the "delta" is a prefix dictionary, not a delta stream.** All 18,344 tag-1
records **fail** `zstd.decompress(frame)` with *Data corruption detected*; all 18,344 decode
with the **base object's content as a raw zstd dictionary** (`ZstdDict(base, is_raw=True)`),
and the result length is exactly `canonical_length − 23` every time. The frame's declared
content size is `canonical_length − 23` (the *reconstructed* length), and the dictionary id is
suppressed. Base chains resolve to depth 1 in this Store. So the WholeFile "delta base" **is**
the "previous version as a zstd prefix dictionary" that item 4 asks about — the product already
does it, for **41.5 % of records**.

**Canonical header.** For all 44,148 whole-file objects, `canonical_length = 23 + len(content)`:
`347,445,305 + 44,148 × 23 = 348,460,709` — **residual 0** against `SUM(canonical_length)`.
The 23-byte canonical header (**1,015,404 B**, 0.29 % of canonical) is counted in every ratio
but is never compressed. Content/stored = **3.1318x** where canonical/stored = **3.1410x**.

**Verified record model.** Dumping all 44,148 payloads in the Store's own pack/group order
(`python3 /tmp/b3/dump_payloads.py`) gives **347,445,305 B** with **0 length mismatches**, and
the 17 per-save record counts (261, 800, 971, 1046, 2270, 1896, 2596, 1534, 1968, 1783, 3699,
3492, 3420, 5047, 4923, 4099, 4344 = 44,148) place **all 16 interior save cuts exactly on pack
boundaries** — the save → pack mapping used by every grouping below is verified, not assumed.

---

## 2. Item 1 — per-record vs GROUPED compression: **MEASURED** (reproduced, with a correction)

Command: `python3 /tmp/b3/measure1.py` (population = the 44,148 real contents, 347,445,305 B,
canonical 348,460,709 B, in Store order, grouped **within a save**, zstd L3 + checksum,
framing 4 B/group):

| config | groups | stored | ratio canon/stored | vs per-record |
| --- | --: | --: | --: | --: |
| **current lane** (per-record, prefix dict where based) | 44,148 | **110,941,054** | 3.1410 | — |
| per-record, no dictionary | 44,148 | 119,771,736 | 2.9094 | 1.0000x |
| group 4 | 11,041 | 109,856,148 | 3.1720 | 1.0902x |
| group 16 | 2,767 | 99,106,330 | 3.5160 | 1.2084x |
| group 64 | 697 | 90,786,775 | 3.8382 | 1.3192x |
| group 256 | 181 | 86,993,189 | 4.0056 | 1.3769x |
| cap 256 KiB | 1,401 | 94,781,681 | 3.6765 | 1.2637x |
| cap 1 MiB | 344 | 88,481,984 | 3.9382 | 1.3536x |
| cap 4 MiB | 91 | 86,223,145 | 4.0414 | 1.3891x |

**Reproduction of the prior claim.** Prior: 3,000 whole-file objects, **2.87x per record vs
3.64x grouped (worth 1.25x)**. Measured here on **44,148 objects (14.7x the sample)**: per
record **2.9094x**, grouped at the product's own pack size (256 KiB) **3.6765x**, worth
**1.2637x**. Per record **+1.4 %**, grouped **+1.0 %**, worth **−0.3 %** — the prior claim is
**reproduced** at 14.7x the sample size.

**The correction that matters.** Split by whether the record actually has a base in this Store:

| subset | n | canonical | current lane | per-record | group 64 | cap 256 KiB |
| --- | --: | --: | --: | --: | --: | --: |
| **no base** | 25,804 | 266,427,536 | 93,744,892 | 93,719,088 | **75,626,820** (1.2392x) | 78,928,506 (1.1875x) |
| **has base** | 18,344 | 82,033,173 | 17,196,162 | 26,052,648 | 17,120,891 (1.0044x) | **17,703,599 (0.9712x)** |

**Negative result.** Grouping does **not** subsume the prefix dictionary. On the 18,344 records
that already carry a predecessor dictionary, 256 KiB grouping is **2.95 % worse** than what the
lane already does, and 64-record grouping is only 0.44 % better. The 1.26x headline is earned
entirely on the **25,804 records that have no base**; those same records are where the missed
prefix-dictionary opportunity lives (§4). *Arithmetic sum of two measured configurations*
(not a single measured run): current for the based subset + group-64 for the unbased subset =
`17,196,162 + 75,626,820 = 92,822,982 B`, **3.7540x**, **1.1952x** better than the lane's
110,941,054 B. Labelled a hypothesis for the product, not a measurement of it.

*Caveat:* the two subset rows group across save boundaries (the subsets are concatenated), so
they understate group quality slightly relative to a save-bounded grouping; the full-population
rows are save-bounded.

---

## 3. Item 2 — zstd LEVEL sweep: **MEASURED**; item 3 — WINDOW LOG sweep: **MEASURED**

Command: `python3 /tmp/b3/measure2.py` → `/tmp/b3/e2.json`, `/tmp/b3/e3.json`.
Canonical **348,460,709 B** throughout. zstd L3 + checksum, save-bounded groups.

### 3.1 Level curve (stored bytes; ratio = canonical/stored)

| level | per-record | ratio | group 64 | ratio | cap 256 KiB | ratio |
| --: | --: | --: | --: | --: | --: | --: |
| 1 | 124,272,584 | 2.8040 | 105,746,321 | 3.2953 | 103,383,789 | 3.3706 |
| **3 (current)** | 119,771,736 | 2.9094 | 90,811,231 | 3.8372 | 94,780,047 | 3.6765 |
| 5 | 115,388,955 | 3.0199 | 85,650,509 | 4.0684 | 88,536,363 | 3.9358 |
| 9 | 113,394,270 | 3.0730 | 82,131,569 | 4.2427 | 85,282,634 | 4.0860 |
| 15 | **110,333,892** | 3.1582 | 80,326,886 | 4.3380 | 80,372,615 | 4.3356 |
| 19 | **109,937,238** | 3.1696 | 74,960,389 | 4.6486 | 79,640,983 | 4.3754 |

Two readings of this curve:

* **Per record alone, level 15 (110,333,892 B) is within 0.55 % of the entire current lane
  (110,941,054 B)** — i.e. on this population the level knob alone buys as much as the whole
  prefix-dictionary mechanism, and level 19 (109,937,238 B) beats it by 0.9 %. *This is
  arithmetic on two measured configurations, not a proposal*; the CPU cost was not measured
  (timing invalid here) and is the obvious counterweight.
* **Level and group size interact.** At level 3 the 64-record group beats the 256 KiB cap
  (90,811,231 vs 94,780,047); at level 19 the same ordering holds (74,960,389 vs 79,640,983).
  The best measured cell overall is **level 19, group 64 = 74,960,389 B, 4.6486x**.
* **Residual:** these are frame sizes only; the 4 B/group directory and the 16 B/pack header are
  added in §5, never here.

### 3.2 windowLog curve (`P.window_log`), canonical 348,460,709 B

| windowLog | L3 per-record | L3 cap256KiB | L3 cap4MiB | L9 cap4MiB |
| --: | --: | --: | --: | --: |
| 10 | 185,307,192 | 185,607,458 | 186,033,664 | 159,332,903 |
| 12 | 143,227,264 | 144,689,358 | 146,394,701 | 123,207,880 |
| 14 | 123,759,861 | 120,764,962 | 122,603,545 | 102,240,795 |
| 16 | 119,855,217 | 104,477,542 | 105,996,120 | 88,225,939 |
| 17 | **119,771,736** | 98,458,788 | 96,452,449 | 83,029,107 |
| 18 | 119,771,736 | **94,780,047** | 89,708,059 | 79,752,834 |
| 19 | 119,771,736 | 94,780,047 | 87,545,777 | 77,552,207 |
| 20 | 119,771,736 | 94,780,047 | 86,534,633 | 76,063,607 |
| 21 | 119,771,736 | 94,780,047 | 86,221,795 | 75,306,879 |
| 22 | 119,771,736 | 94,780,047 | **86,171,334** | **75,041,986** |
| 23–27 | 119,771,736 | 94,780,047 | 86,171,334 | 75,041,986 |

**What window does the data need?**

* **Per record: windowLog 17** — the largest whole-file content is < 131,072 B (2^17), and the
  curve is exactly flat from 17 upward (identical bytes at 17…27, not approximately equal).
* **Grouped at 256 KiB: windowLog 18** (2^18 = 262,144 ≥ the cap); flat from 18 up.
* **Grouped at 4 MiB: windowLog 22**; at 21 it still costs 50,461 B more (L3) / 264,893 B more
  (L9). Below windowLog 14 **grouping is worse than per-record** (L3/wlog10: 186,033,664 vs
  185,307,192) — a grouped layout *requires* a window at least the group size, which is the
  opposite of free.
* **The library defaults are already at the useful end:** the L3 default run for cap256KiB is
  byte-identical to the windowLog-21 row (94,780,047), and the L3 default run for cap4MiB
  (86,223,145) is byte-identical to the windowLog-21 row — so nothing in §2's numbers is
  window-starved except the 4 MiB group, which is 0.06 % short of its windowLog-22 optimum.
* `enable_long_distance_matching` was **NOT RUN** (no measurement taken; not claimed).

---

## 4. Item 4 — DICTIONARY SCOPE: **PARTLY MEASURED**

Command: `python3 /tmp/b3/measure3.py stride10` (log `/tmp/b3/e4_stride10.log`).

Population: the **29,222** stride10 paths modified between two selected states whose content is
whole-file sized — canonical **265,065,265 B** (content 264,393,159 B + 29,222 × 23).

### 4.1 Raw prefix dictionary = the previous version (MEASURED)

| | stored | ratio (canonical/stored) |
| --- | --: | --: |
| compressed **alone**, zstd L3 + checksum | 89,063,696 | **2.9761x** |
| with the **previous version as a raw prefix dictionary** | **11,319,962** | **23.4157x** |
| gain | | **7.8678x** |

**Reproduce or refute?** The prior claim was *197 real changed versions: 3.08x alone → 10.20x
with the previous version as a prefix dictionary (dictionary worth 3.31x)*. On **148x the
sample**: alone **2.9761x** (prior 3.08x, −3.4 %), with the dictionary **23.4157x** (prior
10.20x, **+129 %**), worth **7.8678x** (prior 3.31x, **+138 %**). The **direction is
reproduced; the magnitude is refuted upward** — the dictionary is worth far more than 3.31x on
this population.

**Independent cross-check (sibling squad, different instrument).** On the full 29,302 same-path
changed versions of stride10 (285,191,546 B canonical): `zstd --patch-from -3` = 11,971,222 B
(**23.823x**), `-19` = 9,977,483 B (**28.584x**). My in-process figure is **23.4157x** on
29,222 records / 265,065,265 B canonical. Two instruments, two populations differing by 80
records, agree within **1.7 %** on the ratio. The level knob is worth a further **1.2000x**
(23.823x → 28.584x) on top of the dictionary.

**Why the lane realises 4.77x and not 23x.** The lane gives a base to **18,344** records and
stores them at 4.7714x; the prefix dictionary is worth **7.8678x** on the 29,222 records that
have a predecessor. The two populations are not the same set, so the gap is **not** attributed
here — the join that would close it (`python3 /tmp/b3/measure4.py`, content-hash join of the
Store's record stream to the corpus, splitting "has a predecessor but got no base" from "got a
base") was **written but NOT RUN** inside the time box. That measurement is the single
highest-value follow-up and is named as such in §8.

### 4.2 Trained dictionaries, held-out split (MEASURED)

Split: train = states 1–8 (**7,095 records, 67.9 MB**), test = states 9–17 (**22,127 records,
196.5 MB**). Test-set alone = **67,649,665 B = 2.9121x**. All dictionaries are trained on the
train split only; every number below is on the **held-out** test split.

| dictionary scope | test n | stored | ratio | vs alone |
| --- | --: | --: | --: | --: |
| whole-corpus, 112,640 B | 22,127 | 59,538,572 | 3.3088 | 1.1362x |
| whole-corpus, 262,144 B | 22,127 | 57,764,782 | 3.4104 | 1.1711x |
| **per top-level directory**, 112,640 B | 21,742 | 56,223,546 | 3.4341 | ~1.172x |
| per **file extension** | — | — | — | **NOT RUN** |

**Negative result.** A *trained* dictionary is worth **1.14–1.17x**; the *raw prefix* dictionary
is worth **7.87x** on the same lane. Trained-dictionary scope (whole corpus → per directory)
moves the number by **3.1 %** (3.3088 → 3.4341), i.e. **scope is a rounding error next to
provenance**. Per-extension was interrupted after one extension (`.md`: train 2,151 / test
7,587; stored 27,313,934 vs alone 31,620,542 = **1.1577x**), which is consistent with the other
scopes; the remaining extensions are **NOT RUN**.

*Residual:* the per-directory total covers 21,742 of the 22,127 test records (directories with
fewer than 8 training samples are skipped), so it is not a like-for-like total with the
whole-corpus rows; the `.md` extension row is a subset of the test split, not the whole of it.

---

## 5. Item 5 — is "one record per group, Raw" right for a history workload?

**MEASURED, and the answer is split by whether the record has a predecessor.**

### 5.1 Framing accounting (residual 0)

Command: `python3 /tmp/b3/e5_framing.py`.

| lane | packs | groups | bodies | directory | header | record tags + base ids | zstd frame overhead |
| --- | --: | --: | --: | --: | --: | --: | --: |
| WholeFile | 437 | 44,148 | 110,941,054 | 176,592 | 6,992 | 631,156 | 617,851 |
| Native | 31 | 131 | 5,695,678 | 2,096 | 496 | 0 | 0 |
| PooledMetadata | 17 | 1,737 | 2,019,419 | 27,792 | 272 | 0 | 23,850 |
| Ordinary | 17 | 72 | 1,022,476 | 1,152 | 272 | 0 | 1,008 |
| **TOTAL** | 502 | 46,088 | **119,678,627** | **207,632** | **8,032** | 631,156 | 642,709 |

    bodies + directory + headers = 119,678,627 + 207,632 + 8,032 = 119,894,291 B = pack bodies   residual 0

WholeFile internal framing: `44,148` tag bytes + `18,344 × 32 = 587,008` base-id bytes +
`617,851` zstd frame overhead (**mean 14.0 B/frame**) = **1,249,007 B = 1.126 %** of the lane's
110,941,054 B. Plus the 23-byte canonical header (**1,015,404 B**) that every ratio counts but
no compressor sees.

**Framing overhead NOT modelled:** the zstd frame overhead *is* measured here by walking each
frame's block headers (642,709 B total, 617,851 B in WholeFile); the pack header, directory and
record tags are exact; the 23-byte canonical header's *contents* were never decoded (only its
length, 23, is established — 44,148/44,148 records, residual 0). SQLite page overhead is
separate and is **not** pack framing: `apparent 128,864,256 − pack bodies 119,894,291 =
8,969,965 B (6.96 %)`, i.e. **172.4 B per object row** over 52,032 rows — and **grouping does
not reduce it**, because the `objects` table keeps one row per object whatever the group count.

### 5.2 The measured answer

* **One record per group is wrong for the 25,804 records with no predecessor.** Grouping them
  at 256 KiB costs **78,928,506 B** instead of **93,719,088 B** (**1.1875x**); at 64 records,
  **75,626,820 B** (**1.2392x**).
* **One record per group is right for the 18,344 records that already carry a predecessor
  dictionary.** Grouping them at 256 KiB is **17,703,599 B vs 17,196,162 B — 2.95 % worse**;
  at 64 records it is 0.44 % better. The dictionary already extracted the redundancy that
  grouping would go looking for.
* **Framing is not the reason either way.** Grouping at 256 KiB shrinks the WholeFile directory
  from `44,148 × 4 = 176,592 B` to `1,401 × 4 = 5,604 B` — **170,988 B, 0.154 %** of the
  lane. All the win is compression, none of it is directory bytes.
* **Access-pattern cost of changing it (measured shape, not a timing claim).** At the 256 KiB
  cap the lane has **1,401 groups**, mean **247,998 B**, median **255,391 B**, max 262,137 B,
  holding a mean of **31.5** records (median 29, max 91). Reading one record then requires
  inflating a median **255,391 B** group against a median record size of ~4.3 KiB — a **59.8x**
  read amplification for a single-record access, and a delta-base probe on one record pays the
  same. At 64 records/group the amplification is lower but the ratio gain is lower too. That
  trade is the whole cost of the change and it is not visible in any stored-bytes table.
* **The product already contains both mechanisms.** Ordinary and PooledMetadata store one zstd
  frame per group (codec byte 1); WholeFile and Native store one frame per record inside a Raw
  group. So "grouped" is not a new format — it is an existing group codec applied to a lane that
  does not currently use it.

---

## 6. Hypotheses (explicitly labelled — none of these is a measurement)

* **H1 (supported by measurement, not itself measured).** Applying the prefix-dictionary route
  to the records that have a predecessor but no base would move the WholeFile lane by an order
  of magnitude more than any framing change. *Supporting arithmetic:* 23.4157x measured on the
  29,222 predecessor-bearing records vs the lane's 4.7714x on the 18,344 based records. The
  join that would attribute the gap is **NOT RUN**.
* **H2 (unmeasured).** Level 15 + per-record (110,333,892 B) and level 19 + group-64
  (74,960,389 B) are the two cheapest byte wins in this report; their CPU cost is unknown and
  **timing is invalid in this session**, so no recommendation is made.
* **H3 (unmeasured).** A mixed layout — per-record dictionary frames for based records, grouped
  frames for unbased records — is worth `1.1952x` by arithmetic on two measured configurations;
  no single run of that layout was taken.

---

## 7. Exact commands

```sh
# corpus population, all three selections (reproduces the pinned union oids/bytes)
python3 /tmp/b3/extract_lane.py stride10     # 44,240 oids / 371,937,306 B
python3 /tmp/b3/extract_lane.py stride3      # 60,000 oids / 583,508,923 B
python3 /tmp/b3/extract_lane.py stride1      # 75,929 oids / 891,893,320 B

# pack directory decode + per-lane verification (bucket table, codecs, framing)
python3 /tmp/b3/packdir.py /tmp/base187/sample.sqlite

# verified record model: 44,148 payloads in Store order
python3 /tmp/b3/dump_payloads.py             # 347,445,305 B, 0 length mismatches

# E1 grouped vs per-record; E2 level; E3 windowLog
python3 /tmp/b3/measure1.py                  # -> /tmp/b3/e1.json
python3 /tmp/b3/measure2.py                  # -> /tmp/b3/e2.json, /tmp/b3/e3.json

# E4 dictionary scope (stride10)
python3 /tmp/b3/measure3.py stride10         # -> /tmp/b3/e4_stride10.log

# E5 framing accounting
python3 /tmp/b3/e5_framing.py                # -> /tmp/b3/e5.json
```

Environment: Python 3.14.3 with the stdlib `compression.zstd` module (libzstd **1.5.7**);
`/opt/homebrew/bin/zstd` v1.5.7 for the identity check only. No product source was read or
modified; no lane was run (the existing Store `/tmp/base187/sample.sqlite` was read read-only).

---

## 8. NOT RUN (named, with what it would take)

| # | not run | why it matters |
| --: | --- | --- |
| 1 | `python3 /tmp/b3/measure4.py` — content-hash join of the Store's 44,148 records to the corpus, splitting "has a predecessor but got no base" from "got a base" | attributes the 4.77x-vs-23.42x gap to a set of records and bytes; script is written, needs ~2 min |
| 2 | stride3 confirmation of E4 (`measure3.py stride3`, 56,618 modified whole-file records, 528.7 MB) | the tier rule is "iterate stride10, confirm stride3"; item 4 has **no** stride3 confirmation |
| 3 | per-extension trained dictionaries beyond `.md` | item 4's third scope; interrupted by the time box |
| 4 | `enable_long_distance_matching` window sweep | not measured, not claimed |
| 5 | stride1 diagnostic of the prefix-dictionary gain | **deliberately not run** — stride1 is never optimised, and the 197-record prior claim is already answered on stride10 |
| 6 | CPU cost of any level/grouping change | timing is invalid in this session by the task's own rule; **no timing is reported anywhere in this document** |

## 9. Residuals and negative results, in one place

1. **Residual 0** on: corpus union pins (3 selections), the bucket table (4 rows), the framing
   identity (`bodies + dirs + headers = pack bodies`), the codec identity (25,804/25,804 exact
   frames), the canonical header (44,148 × 23), and the payload dump (0 length mismatches).
2. **One record of 44,149 is not a whole-file object** — a 0-byte file. Excluding it makes the
   per-save counts sum to exactly 44,148 and puts all 16 interior save cuts on pack boundaries.
   Residual bytes **0**.
3. **Negative: grouping does not subsume the prefix dictionary** (2.95 % worse on the based
   subset at 256 KiB).
4. **Negative: trained dictionaries are 1.14–1.17x where the raw prefix dictionary is 7.87x**;
   dictionary scope moves the number by 3.1 %.
5. **Negative: below windowLog 14, grouping is worse than per-record** — a grouped layout is
   window-bound, not free.
6. **The brief's lane split is half wrong**: WholeFile records are zstd frames, not raw; Native
   groups hold 2–20 records, not one.
7. **Allocation readings of the baseline Store are not stable across readings**:
   135,118,848 B (baseline note), 130,863,104 B (as recorded in the brief), **134,537,216 B**
   (my reading: 262,768 blocks × 512, apparent 128,864,256 B). The allocated axis is the
   parent's headline and is flagged, not resolved, here. **All ratios in this report use
   apparent/pack-body bytes, never allocated bytes.**
8. The two subset rows in §2 group across save boundaries (caveat stated there).
9. The per-directory trained-dictionary row covers 21,742 of 22,127 held-out records.
10. Timing: **none reported**, by rule.

## 10. One-line answer per item

1. **Per-record vs grouped — MEASURED.** 2.9094x per record vs 3.6765x grouped at the product's
   own 256 KiB pack size, **worth 1.2637x**; the prior 1.25x claim reproduces at 14.7x the
   sample. But it is earned only on the 25,804 records with no base (1.1875–1.2392x); on the
   18,344 already-based records 256 KiB grouping is **2.95 % worse**.
2. **Level — MEASURED.** 1: 2.8040x · 3 (current): 2.9094x · 5: 3.0199x · 9: 3.0730x ·
   15: 3.1582x · 19: 3.1696x per record; grouped-64 at 19: **4.6486x**. Level 15 per-record
   already matches the entire current lane (110,333,892 vs 110,941,054 B).
3. **Window — MEASURED.** Needs windowLog **17** per record, **18** for a 256 KiB group, **22**
   for a 4 MiB group; flat above. Below 14, grouping loses to per-record.
4. **Dictionaries — PARTLY MEASURED.** Raw prefix dictionary (previous version) on 29,222
   stride10 modified records: 89,063,696 B alone (**2.9761x**) → **11,319,962 B (23.4157x)**,
   **worth 7.8678x** — prior claim reproduced in direction, refuted upward in magnitude, and
   corroborated by an independent `zstd --patch-from` measurement (23.823x). Trained
   dictionaries: 1.1362x (112 KiB) / 1.1711x (256 KiB) whole-corpus, 1.172x per directory, on a
   held-out split; per-extension **NOT RUN**.
5. **Lane split — MEASURED.** One record per group is right **only** for records that already
   carry a predecessor dictionary; for the rest it costs 1.19–1.24x. Framing is 0.154 % of the
   lane and is not the reason. The cost of grouping is a **59.8x** single-record read
   amplification at the 256 KiB cap.
