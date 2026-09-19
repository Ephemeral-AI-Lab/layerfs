# A1 — policy and codec diff: v0.1.7 vs v0.1.6

Squad A1 of the retained-history storage-loss investigation
([#187](https://github.com/Ephemeral-AI-Lab/layerfs/issues/187), sub-issue of #184).
Lane `history-stride10`. Repo HEAD `66bce8378`.

> **Every number in this document is a `diagnostic`.** None of it is admission
> evidence, and none of it is a gate result. The allocation figures are
> load-independent byte readings; **no timing figure appears here at all**,
> because the machine is shared and a timing reading taken now would not be valid.

---

## 1. The question I was asked

Is the v0.1.7 codec/policy **set** a superset of v0.1.6's with the same defaults —
or did a default **regress**?

**Answer, plainly: the codec parameter set is byte-identical between the two
generations and did not regress. Three policy defaults did regress (two in the
pooled-metadata lane, one in the candidate-cache lifetime), and NONE of them can
account for the 2.61x Store.** The 79.5 MB excess is 90.5 % one thing: the
whole-file lane's delta coverage collapsed from 81.93 % to 23.54 % of its
canonical bytes, while the bytes it does store as FULL compress **2.27 % better**
than v0.1.6's.

The arithmetic that says so is §5. It closes to the byte.

---

## 2. Instruments and exact commands

Two read-only instruments were written for this squad (both are analysis code;
neither is product source and neither is under `core/crates/` or `crates/`):

| file | what it does |
| --- | --- |
| `squad-a/decode_packs.py` | decodes the pack directory of **either** generation and joins it to `objects` / `metadata_value_groups`, per lane |
| `squad-a/accounting.py` | the reconciliation, per-class rates, the class-confusion matrix and the counterfactual |

```sh
cd /Users/yifanxu/Ephemeral-AI-Lab/layerfs

# both Stores, per-lane table (v0.1.7 first, v0.1.6 second)
python3 docs/roadmap/0.1/0.1.7/evidence/stage-6-history-187-20260920T000000Z/squad-a/decode_packs.py \
  /tmp/base187/sample.sqlite \
  benchmark-results/repository-history/stride-10/deepseek-stride10/host-runtime/store.sqlite

# reconciliation + class rates + counterfactual
python3 docs/roadmap/0.1/0.1.7/evidence/stage-6-history-187-20260920T000000Z/squad-a/accounting.py
```

Outputs are retained beside the scripts as `decode_packs.txt` and `accounting.txt`.

No lane was re-run for this report. Both Stores were already on disk:

| | v0.1.7 | v0.1.6 |
| --- | --- | --- |
| path | `/tmp/base187/sample.sqlite` | `benchmark-results/repository-history/stride-10/deepseek-stride10/host-runtime/store.sqlite` |
| provenance | the existing baseline run of `--case history-stride10` | the **retained host-runtime Store of the #153 campaign** (ledger L31), hashed in that run's `performance-manifest.json` as `ec6846e2…` |

**The v0.1.6 Store is a real find and it is what makes this report arithmetic
rather than speculation.** Every previous v0.1.6 figure was quoted from
`issue153-retained-history-report.md`; the Store itself is still in the receipt
tree, so v0.1.6's per-lane, per-object and per-class numbers can be *measured*
rather than cited. `objects` in that file has no `object_role` and no
`base_object_id`, so the class of a whole-file record is recovered from the
record's own tag byte (§4.3).

### 2.1 Source pin: `crates/` really is v0.1.6

```sh
git diff --stat 7fab1027a HEAD -- crates/
#  .../layerfs-content/examples/rope_edit_oracle.rs | 314 +++
#  .../layerfs-content/examples/rope_edit_timing.rs |  97 +++
#  .../examples/stage5_component_reference.rs      | 225 +++
#  .../tests/stage5_reference_fixtures.rs          | 924 +++
#  .../tests/deep_history_diagnostic.rs            | 581 +++
#  5 files changed, 2141 insertions(+)
```

The v0.1.6 pin `7fab1027a` is an ancestor of HEAD, and between it and HEAD
**only test and example files were added under `crates/`**. Every file this report
reads from the reference tree is byte-identical to the pin:

```
SAME  crates/layerfs-content/src/file/content.rs
SAME  crates/layerfs-content/src/file/cdc/gear.rs
SAME  crates/layerfs-layerstack-store/src/objects/pack.rs
SAME  crates/layerfs-layerstack-store/src/objects/admission.rs
SAME  crates/layerfs-layerstack-store/src/objects/delta.rs
SAME  crates/layerfs-layerstack-store/src/objects/small_candidates.rs
SAME  crates/layerfs-layerstack-store/src/schema.rs
SAME  crates/layerfs-layerstack-store/src/objects/read.rs
```

So "v0.1.6" below means "the product source at `7fab1027a`", verified, not
assumed.

---

## 3. Field-by-field policy and codec table

`DIFF` marks a value that is not identical between the generations. Every
non-diff row was read from source on both sides, not inferred.

### 3.1 Representation and construction cutoff

| field | v0.1.6 (source) | v0.1.7 (source) | DIFF |
| --- | --- | --- | --- |
| small-file cutoff | `content::SMALL_LIMIT = 131072` (`file/content.rs:8`) | `DEFAULT_SMALL_FILE_THRESHOLD_BYTES = 131_072` (`content/policy.rs:14`) | **no** |
| cutoff semantics | exclusive: `!bytes.is_empty() && bytes.len() < SMALL_LIMIT` (`content.rs:210`) | exclusive: `logical_len < small_file_threshold_bytes` (`policy.rs:123`) | **no** |
| representation set | `Content::{Small{logical_len}, Chunked(FileStateV3)}` (`content.rs:80-84`) | `Representation::{Empty, WholeFile, Chunked}` (`policy.rs:182-190`) | **naming only** |
| zero-length file | `Small` is rejected for `len == 0`; empty goes to the rope | explicit `Representation::Empty` | **naming only** |
| extra whole-file *owner* form | `WHOLE_LIMIT = 2 MiB`, `encode_whole`/`whole_bytes` for native chunk slices in `SMALL_LIMIT..=WHOLE_LIMIT` (`content.rs:11,14,26`) | no equivalent on the write path | **dead on the v0.1.6 write path — see N4** |
| configurable cutoff | none; `SMALL_LIMIT` is a constant | `ConstructionPolicy::new` accepts a power of two in `131072..=1048576` (`policy.rs:16-18,81-102`) | **new capability, default unchanged** |

### 3.2 Delta depth bounds

| field | v0.1.6 | v0.1.7 | DIFF |
| --- | --- | --- | --- |
| whole-file depth | `delta::CHAIN_EDGES = 8` (`objects/delta.rs:7`); `small_predecessor` rejects when `prior.depth + 1 > 8` (`read.rs:669`) → accepted base depth ≤ 7, new depth ≤ 8 | `DEFAULT_WHOLE_FILE_DELTA_MAX_DEPTH = 8` (`policy.rs:20`); `eligible` requires `depth < 8` (`select.rs:373`) → base ≤ 7, new ≤ 8 | **no** |
| chunk depth | hard-coded `depth >= 4` → reject (`objects/admission.rs:699`) | `DEFAULT_CHUNK_DELTA_MAX_DEPTH = 4` (`policy.rs:22`); same `depth < cap` rule | **no** |
| pooled-metadata depth | `read::METADATA_EDGES = 16` (`objects/read.rs:148`); admission requires `base.depth < 16` (`admission.rs:1746`) → new depth ≤ **16** | `DEFAULT_METADATA_DELTA_MAX_DEPTH = 8` (`storage/policy.rs:141`); `cost.depth >= 8` → skip (`cas/pool_lane.rs:315,332`) → new depth ≤ **8** | **YES — halved** |
| `MAXIMUM_DELTA_MAX_DEPTH` | no such constant; no configurable policy at all | `50` (`policy.rs:24`) | **new; validation bound only** |
| where 50 comes from | `whole.rs:19  pub(super) const EDGES: usize = 50` — but `whole.rs` is documented *"Read compatibility for previously compacted Stores; writers exist only in tests"* | — | **not a v0.1.6 write default — see N5** |

The whole-file depth is the one that matters, and it is **8 on both sides**. The
`50` in v0.1.7 is a validation ceiling, not a default.

### 3.3 Zstandard parameters, per lane

| field | v0.1.6 | v0.1.7 | DIFF |
| --- | --- | --- | --- |
| payload level | `(ZSTD_c_compressionLevel, 3)` (`pack.rs:1213`) | `(ZSTD_cParameter::ZSTD_c_compressionLevel, 3)` (`encoding/codec.rs:214,288`) | **no** |
| payload window log | `ContentProfile::window()`: `Small => 18`, else `20` (`pack.rs:405-411`) | `CodecProfile::whole_file` → `whole_file_window_log()` = 18 at the default cutoff; `CodecProfile::native()` → 20 (`codec.rs:76-91`, `policy.rs:166-172`) | **no** |
| payload frame flags | `contentSizeFlag: 1, checksumFlag: 1, dictIDFlag: 0, nbWorkers: 0` (`pack.rs:1214-1218`) | identical sequence (`codec.rs:216-219`) | **no** |
| payload raw limit — whole file | `ContentProfile::Small.raw_limit() = 131071` (`pack.rs:390`) | `whole_file_canonical_limit − WHOLE_FILE_CANONICAL_OVERHEAD = 131094 − 23 = 131071` (`codec.rs:87`) | **no** |
| payload frame limit — whole file | `135168` (`pack.rs:397`) | `max(conservative_frame_bound(131071), 135168) = max(133118, 135168) = 135168` (`policy.rs:150-154`) | **no** |
| payload raw limit — chunk | `NATIVE_RAW_LIMIT = 32_768` (`pack.rs:233`) | `CHUNK_RAW_LIMIT = 32_768` (`codec.rs:110`) | **no** |
| payload frame limit — chunk | `NATIVE_FRAME_LIMIT = 33_024` (`pack.rs:234`) | `CHUNK_FRAME_LIMIT = 33_024` (`codec.rs:112`) | **no** |
| group-body level | `ZSTD_getCParams(1, raw.len(), 0)` (`pack.rs:1230`) | `ZSTD_getCParams(GROUP_LEVEL = 1, raw.len(), 0)` (`codec.rs:57,348`) | **no** |
| group-body window log cap | `parameters.windowLog.min(16)` (`pack.rs:1231`) | `parameters.windowLog.min(GROUP_WINDOW_LOG_MAX = 16)` (`codec.rs:59,349`) | **no** |
| group-body frame flags | `contentSizeFlag: 1, checksumFlag: 1, noDictIDFlag: 1` (`pack.rs:1234-1240`) | identical (`codec.rs:355-362`) | **no** |
| group-body retention rule | `compressed.len() + 16 <= length` (`pack.rs:1030`) | `frame.len() + 16 <= raw.len()` (`codec.rs:657`) | **no** |
| group-body frame limit | `GROUP_LIMIT + 1024` implied by the `ZSTD_compressBound` guard (`pack.rs:1244`) | `GROUP_FRAME_LIMIT = GROUP_LIMIT + 1024` (`codec.rs:55`) | **no** |
| encode workspace | per-role: `2 MiB` small, `8 MiB` whole, `1 MiB` native (`pack.rs:1245,1252,1253`) | one `ENCODE_WORKSPACE_BYTES = 2 MiB` per save (`codec.rs:49`) | **memory only, not bytes** |

The parameter sequences are **the same list of zstd `c_` parameters with the same
values**; only the Rust module and the error type differ.

### 3.4 Chain work budgets

| field | v0.1.6 | v0.1.7 | DIFF |
| --- | --- | --- | --- |
| whole-file canonical closure | `CHAIN_CANONICAL_LIMIT = 512 * 1024` (`delta.rs:8`); `prior.canonical_closure + target <= 512 KiB` (`read.rs:673`) | `CHAIN_CANONICAL_LIMIT = 512 * 1024` (`storage/policy.rs:102`); `chain.canonical_bytes + canonical.len() <= 512 KiB` (`select.rs:290`) | **no** |
| whole-file encoded closure | `CHAIN_ENCODED_LIMIT = 256 * 1024` (`delta.rs:9`); `encoded_closure + delta.len() + 41 <= 256 KiB` (`admission.rs:507`) | `CHAIN_ENCODED_LIMIT = 256 * 1024`; `chain.encoded_bytes + canonical.len() <= 256 KiB` (`select.rs:291`) | **no (different accounting, same constant — see N3)** |
| chunk canonical closure | `raw_closure + raw.len() <= 1024 * 1024` (1 MiB) (`admission.rs:700-702`) | `chain_canonical_limit = 512 KiB` (`select.rs:290`) | **YES — 1 MiB → 512 KiB, non-binding (N2)** |
| chunk encoded closure | none stated separately | `chain_encoded_limit = 256 KiB` | **new, non-binding (N2)** |
| pooled-metadata canonical closure | `METADATA_CLOSURE = 128 * 1024` (`read.rs:149`) | `METADATA_CHAIN_CANONICAL_LIMIT = 8 * 8192 = 65_536` (`storage/policy.rs:143`) | **YES — halved** |
| pooled-metadata encoded closure | `encoded_closure > 17 * 8193` (`read.rs:279`) | `METADATA_CHAIN_ENCODED_LIMIT = 17 * 8193 = 139_281` (`policy.rs:145`) | **no** |
| pooled-metadata per-node limit | `location.canonical_length > 8192`, `record.len() > 8193` (`read.rs:277-280`) | `METADATA_RECORD_LIMIT = 8192` (`policy.rs:147`) | **no** |
| pooled-metadata decoded work | `maximum_lookups * 16 * 1024 < 32 MiB` (`metadata.rs:152`) | `METADATA_DECODED_WORK_LIMIT = 32 MiB` (`policy.rs:139`) | **no** |

### 3.5 Group, pack and directory limits

| field | v0.1.6 | v0.1.7 | DIFF |
| --- | --- | --- | --- |
| `GROUP_LIMIT` | `65_536` (`pack.rs:6`) | `65_536` (`storage/policy.rs:45`) | **no** |
| `PACK_LIMIT` | `256 * 1024` (`pack.rs:7`) | `256 * 1024` (`policy.rs:55`) | **no** |
| `GROUP_COUNT_LIMIT` | `256` (`pack.rs:8`) | `256` (`policy.rs:57`) | **no** |
| `RECORD_COUNT_LIMIT` | `8191` (`pack.rs:9`) | `8_191` (`policy.rs:59`) | **no** |
| `CANONICAL_LIMIT` | `16 * 1024 * 1024` (`pack.rs:10`) | `16 * 1024 * 1024` (`policy.rs:61`) | **no** |
| group sealing target | **none** — a group is closed only by the pack limit / group count (`admission.rs:518-520`) | `GROUP_TARGET = 48 * 1024`, framed-length projection (`policy.rs:53`, `cas/selection.rs:62-66`) | **YES — new** |
| ordinary directory entry | 16 B: start/encoded/decoded u32 + codec u8 + 3 pad (`pack.rs:66-79`) | 16 B, identical (`pack/layout.rs:23,339-370`) | **no** |
| compact whole-file entry | 4 B start-only (`pack.rs:600-604`) | `WHOLE_FILE_ENTRY_LEN = 4` (`layout.rs:25,277-285`) | **no** |
| compact drop | `group.bytes[0]` + `group.bytes[9..]` = drop 8 (`pack.rs:606-607`) | `WHOLE_FILE_COMPACT_DROP = 8` (`layout.rs:27`, `assemble.rs:252-253`) | **no** |
| ordinary record tag | `Some(0)` = FULL only (`pack.rs:654`) | `FULL_TAG = 0` only for Ordinary (`delta/record.rs:112`) | **no** |
| native record framing | `[tag u8][raw_len u32][base 32 if tag=1][frame]` (`pack.rs:314-330`) | identical (`delta/record.rs:79-91`) | **no** |
| whole-file record framing | `[tag u8][raw_len u32][frame_len u32][base 32 if tag≠0][frame]` (`delta.rs:96-107`) | identical (`record.rs:63-91`) | **no** |
| delta accept margin | `delta.len() + 32 < frame.len()` (`admission.rs:505`); native `37 + frame.len() < 5 + full.len()` (`admission.rs:756`) | `prefix.record.len() < full.record.len()` (`select.rs:302`) — algebraically the same 32-byte margin | **no** |

### 3.6 Lanes and framing versions

| | v0.1.6 (`pack.rs:116-143`) | v0.1.7 (`pack/layout.rs:29-53`) | DIFF |
| --- | --- | --- | --- |
| v1 | `Version::Legacy` — ordinary multi-record groups | `VERSION_ORDINARY` | **same grammar** |
| v2 | `Version::Native` — chunk records, Raw only | `VERSION_NATIVE` — Raw only | **same grammar** |
| v3 | `Version::Small` — non-compact whole-file, 16 B entries | **rejected** | **dropped** |
| v4 | `Version::CompactSmall` — 4 B entries, drop 8 | `VERSION_WHOLE_FILE` | **same grammar, renamed** |
| v5 | `Version::Metadata` | **rejected** | **dropped** |
| v6 | `Version::PooledMetadata` | `VERSION_POOLED_METADATA` | **same grammar** |
| v7 | — | `VERSION_SINGLETON` — one oversized record, `SINGLETON_PACK_LIMIT = 16 MiB + 4096` | **new** |
| oversized records | handled inside the existing versions by an `oversized` flag (`pack.rs:80-88`) | own lane with its own pack limit (`full.rs:196-213`) | **different placement, same bytes** |

Neither Store contains a v3 or v5 pack (§4.2), so the dropped versions cost this
workload nothing.

### 3.7 Content-defined chunking

| field | v0.1.6 | v0.1.7 | DIFF |
| --- | --- | --- | --- |
| `MINIMUM_CHUNK_BYTES` | `8_192` | `8_192` | **no** |
| `TARGET_CHUNK_BYTES` | `16_384` | `16_384` | **no** |
| `MAXIMUM_CHUNK_BYTES` | `32_768` | `32_768` | **no** |
| `NORMALIZATION_SHIFT` | `2` | `2` | **no** |
| `PROFILE_SEED` | `0` | `0` | **no** |
| masks | `0x0000_d903_0353_7000` / `0x0000_d901_0353_0000` / `0x0001_b206_06a6_e000` / `0x0001_b202_06a6_0000` | identical | **no** |
| `GEAR` table | 256 × u64 | 256 × u64 | **no — compared element by element, `identical=True`** |

A comment-stripped diff of `cdc/gear.rs` between the generations contains **no
algorithmic change**: `CoreError`→`ContentError` renames, `u64::try_from(read)`→
`read as u64`, and a visibility change on `GEAR`. The partition function is the
same function.

### 3.8 Candidate cache and batching

| field | v0.1.6 | v0.1.7 | DIFF |
| --- | --- | --- | --- |
| `SLOTS` | `1024` (`small_candidates.rs:5`) | `1024` (`delta/candidates.rs:16`) | **no** |
| `REFERENCES` | `8192` (`small_candidates.rs:6`) | `8_192` (`candidates.rs:17`) | **no** |
| `INDEX_BYTES` | `128 * 1024` (`small_candidates.rs:4`) | `128 * 1024` (`candidates.rs:15`) | **no** |
| `WINDOW` / `mix` / `signature` | 16, same mixer, same 8 smallest hashes (`small_candidates.rs:8,34-66`) | identical (`candidates.rs:19,46-81`) | **no** |
| `find` rule | overlap ≥ 2, ties on smaller id (`small_candidates.rs:98-124`) | identical (`candidates.rs:139-165`) | **no** |
| **cache owner / lifetime** | **session**: pooled on the `StoreDb` as `idle_small_candidates` and handed to a save by `take_small_candidates`, returned by `return_small_candidates` (`schema.rs:77,368-386`; `objects.rs:2292-2295,2591-2595`) — **it survives across saves, so state k can reach state k−1's version through it** | **one save operation**: `Candidates` is documented and built as `owned by one save operation` (`candidates.rs:1-8,28`) | **YES — lifetime only; every capacity and rule is identical** |
| transaction rows / bytes | `ADMISSION_BATCH_COUNT = 8191`, `ADMISSION_BATCH_BYTES = 4 MiB − 1` (`objects.rs:35-36`) | `TRANSACTION_ROW_LIMIT = 8_191`, `TRANSACTION_CANONICAL_BYTES_LIMIT = 4 MiB − 1` (`policy.rs:69,71`) | **no** |
| read wave | `OBJECT_PAGE_COUNT = 128` (`objects.rs:33`) | `LOOKUP_PAGE_IDS = 128`, `READ_OBJECT_LIMIT = 4_096` (`policy.rs:63,80`) | **new larger read wave** |
| pending batch | — | `BATCH_OBJECT_LIMIT = 512`, `BATCH_CANONICAL_BYTES_LIMIT = 512 KiB` (`policy.rs:65,67`) | **new memory bound** |
| pooled value groups | `VALUES_PER_GROUP = 165` (`metadata.rs:9`) | `VALUES_PER_GROUP = 165` (`policy.rs:93`) | **no** |
| pooled group body limit | `body.len() > 16 * 1024` (`metadata.rs:441`) | `METADATA_GROUP_LIMIT = 16 * 1024` (`policy.rs:95`) | **no** |
| pooled leaf rows | `(8192 − 44) / 81 = 100` (`metadata.rs:146-147`) | `POOLED_LEAF_ROWS_LIMIT = 100` (`policy.rs:97`) | **no** |
| metadata index values | `INDEX_VALUES = 131_072` (`metadata.rs:194`) | `METADATA_INDEX_VALUES = 131_072` (`policy.rs:158`) | **no** |

### 3.9 Schema

| field | v0.1.6 | v0.1.7 | DIFF |
| --- | --- | --- | --- |
| `APPLICATION_ID` | `0x4c46_534c` = `1_279_677_260` (`schema.rs:10`) | `1_279_677_261` (`storage/policy.rs:18`) | **+1** |
| `SCHEMA_VERSION` | `10` (`schema.rs:11`) | `4` (`policy.rs:27`) | **different numbering, same accepted set** |
| `store_policy` table | absent | present, one row, `format_profile = 1` (`policy.rs:15,21-27`) | **new** |
| `objects` columns | `object_id, canonical_length, pack_id, group_number, record_number` | `+ object_role, + base_object_id` | **new columns** |
| `object_packs.data` | `BLOB NOT NULL` | `BLOB NOT NULL CHECK (length(data) >= 32)` | **stricter check** |

The two Stores in §4 carry exactly these two schemas, so the format *is* preserved
for every lane this workload writes.

---

## 4. What the two Stores actually contain

### 4.1 Headline bytes

| | v0.1.7 | v0.1.6 | Δ |
| --- | --: | --: | --: |
| apparent (`st_size`) | **128,864,256** | **49,315,840** | **+79,548,416 (2.6130x)** |
| allocated (`st_blocks`×512) | 135,118,848 as recorded at run time (`trace.jsonl` seq 117); 134,537,216 re-stat now | 49,336,320 | +85,782,528 / +85,200,896 |
| pack BLOBs `SUM(length(object_packs.data))` | 119,894,291 | 46,056,732 | +73,837,559 |
| pack count | 502 | 357 | +145 |
| `objects` rows / canonical | 52,032 / 380,921,300 | 51,722 / 380,563,155 | +310 / +358,145 |

**Two caveats, stated rather than smoothed.**

1. The v0.1.6 report records apparent **49,315,940**; the retained file is
   **49,315,840** — a **100 B** difference. This report uses the file's own size
   and therefore reports the excess as **79,548,416**, not 79,548,316.
2. `st_blocks` is not stable across readings of the same file: the v0.1.7 run
   recorded 135,118,848 and a re-stat now returns 134,537,216, a **581,632 B**
   difference on a file whose `st_size` did not move. The allocated axis is
   therefore quoted but **every ratio and every reconciliation below uses apparent
   bytes**. The brief's "130,863,104 as recorded" is a third `st_blocks` reading
   from another run of the same lane; it is not comparable and is not used.

### 4.2 Per-lane table, both generations

`stored` is the sum of the **distinct** group bodies a lane's objects (and, for
the pooled lane, `metadata_value_groups`) name. It plus the pack framing equals
the pack BLOB total, exactly:

`decode_packs.py`, retained output:

```
A v0.1.7  /tmp/base187/sample.sqlite
   Ordinary          6786 obj     10405092 canonical      1022476 stored   10.18x
   Native            1098 obj     22055499 canonical      5695678 stored    3.87x
   WholeFile        44148 obj    348460709 canonical    110941054 stored    3.14x
   PooledMetadata       0 obj            0 canonical      2019419 stored    0.00x
   TOTAL            52032 obj    380921300 canonical    119678627 stored    3.18x
   pack framing 215664   =>  119678627 + 215664 = 119894291 = SUM(length(object_packs.data))

B v0.1.6  .../stride-10/deepseek-stride10/host-runtime/store.sqlite
   Ordinary          4847 obj      2013482 canonical       678339 stored    2.97x
   Native            1098 obj     22055499 canonical      3958057 stored    5.57x
   WholeFile        44141 obj    348460295 canonical     38983278 stored    8.94x
   PooledMetadata    1636 obj      8033879 canonical      2240958 stored    3.59x
   TOTAL            51722 obj    380563155 canonical     45860632 stored    8.30x
   pack framing 196100   =>   45860632 + 196100 = 46056732 = SUM(length(object_packs.data))
```

`PooledMetadata` shows 0 objects in v0.1.7 because v0.1.7 places pooled **leaf
objects** (role 6, 1,738 objects / 8,361,152 canonical) in the v1 Ordinary lane and
keeps only the **value groups** in v6; v0.1.6 puts both in v6. That is a lane
placement difference, not a byte-format difference, and it is why the Ordinary and
PooledMetadata rows are **not** comparable object-for-object between the columns.

Neither Store contains a v3 or a v5 pack.

### 4.3 The whole-file lane, classified per object

The v4 grammar is one record per group and its record begins with a tag byte:
v0.1.6 `0 = FULL, 1|2 = DELTA` (`delta.rs:96`), v0.1.7 `0 = FULL, 1 = PREFIX`
(`delta/record.rs:15-17`). So the class of every whole-file object is readable
from the Store, in **both** generations — including v0.1.6, whose `objects` table
has no `base_object_id`.

| | v0.1.7 | v0.1.6 |
| --- | --: | --: |
| whole-file objects | 44,148 | 44,141 |
| DELTA objects / canonical / stored | 18,344 / 82,033,173 / 17,196,162 | 35,904 / 285,486,222 / 16,310,397 |
| DELTA ratio | 4.77x | 17.50x |
| **DELTA share of canonical bytes** | **23.54 %** | **81.93 %** |
| FULL objects / canonical / stored | 25,804 / 266,427,536 / 93,744,892 | 8,237 / 62,974,073 / 22,672,881 |
| FULL ratio | 2.84x | 2.78x |
| lane total | 348,460,709 / 110,941,054 (3.14x) | 348,460,295 / 38,983,278 (8.94x) |

The v0.1.6 tag histogram is `{2: 31,108, 0: 8,237, 1: 4,796}`: **31,108 of the
35,904 deltas are kind 2** — the chain-format delta whose base came from the
explicitly declared predecessor (`small_predecessor` at `admission.rs:441-450`).
Only **4,796** are kind 1, the legacy delta served by the session-scoped candidate
cache. That split is what §5.4 uses to bound the cache-lifetime change.

### 4.4 The chunk (Native) lane, decoded per record

The native grammar is `[tag u8][raw_len u32][base 32 if tag=1][frame]` on both
sides, so the native records are decodable too:

| | v0.1.7 | v0.1.6 |
| --- | --: | --: |
| chunk records | 1,098 | 1,098 |
| FULL records / raw / stored | 1,098 / 22,032,441 / 5,690,762 (3.87x) | 448 / 8,604,206 / 2,378,527 (3.62x) |
| PREFIX records / raw / stored | **0** | **650 / 13,428,235 / 1,574,526 (8.53x)** |
| record bytes | 5,690,762 | 3,953,053 |
| group framing (4 + 4n per group) | 4,916 | 5,004 |
| lane group bodies | 5,695,678 | 3,958,057 |
| Δ | | **+1,737,621** |

v0.1.7 selects **zero** chunk deltas where v0.1.6 selected 650. The lane holds
**identical canonical bytes and an identical object count** (22,055,499 over 1,098),
so this is a pure selection difference, not a workload difference.

---

## 5. The arithmetic

### 5.1 Reconciliation of the apparent-byte difference — it closes to the byte

```
apparent   A 128,864,256   B 49,315,840   delta 79,548,416   (2.6130x)

  WholeFile              +71,957,776
  Native                  +1,737,621
  Ordinary                  +344,137
  PooledMetadata            -221,539
  pack framing (hdr+dir)     +19,564
  non-pack (SQLite)       +5,710,857
  ------------------------------------
  SUM                    +79,548,416   == 79,548,416   residual 0
```

**Whole-file share = 71,957,776 / 79,548,416 = 90.46 %.**

The `non-pack` term is `st_size − SUM(length(object_packs.data))`: 8,969,965 in
v0.1.7 against 3,259,108 in v0.1.6. It is SQLite b-tree, page and schema bytes —
including the two columns v0.1.7 added to `objects` and the `objects_bases` index.
It is **5,710,857 B, 7.18 %** of the excess, and it is not a codec or policy term.

### 5.2 Per-class rates over the objects both Stores hold

Both Stores hold the same 44,141 whole-file objects (v0.1.7 holds 7 more, 414
canonical bytes). Pricing each Store's own classes over **that common set**:

| | canonical | stored | rate (B stored / B canonical) | ratio |
| --- | --: | --: | --: | --: |
| v0.1.6 DELTA | 285,486,222 | 16,310,397 | 0.057131994 | 17.50x |
| v0.1.6 FULL | 62,974,073 | 22,672,881 | 0.360035169 | 2.78x |
| v0.1.7 DELTA | 82,033,173 | 17,196,162 | 0.209624489 | 4.77x |
| v0.1.7 FULL | 266,427,122 | 93,744,546 | 0.351858119 | 2.84x |

**FULL rate ratio v0.1.7 / v0.1.6 = 0.977288.** On the bytes that get no delta base,
v0.1.7 compresses **2.27 % smaller** than v0.1.6 with the identical codec. That is
the single most important number in this report: **the codec did not regress; it
very slightly improved.**

The DELTA rate is 3.67x worse (0.209624489 / 0.057131994). That is **not** a codec
change — the prefix-frame parameters are the same list (§3.3). It is a change in
*which* objects are selected against *which* base, which §5.3 attributes.

### 5.3 The class confusion and the exact decomposition

Class of each of the 44,141 common objects in each Store (`D` = delta, `F` =
full; v0.1.6's kind is its record tag):

| v0.1.6 | v0.1.7 | objects | canonical |
| --- | --- | --: | --: |
| kind 0 (FULL) | D | 1,260 | 9,079,691 |
| kind 0 (FULL) | F | 6,977 | 53,894,382 |
| kind 1 (legacy delta, session cache) | D | 4,247 | 13,237,572 |
| kind 1 (legacy delta, session cache) | **F** | **549** | **4,517,348** |
| kind 2 (chain delta, declared predecessor) | D | 12,837 | 59,715,910 |
| kind 2 (chain delta, declared predecessor) | **F** | **18,271** | **208,015,392** |

**Counterfactual.** Price v0.1.7's own objects at v0.1.6's own per-class rates
(each object at the rate of the class **v0.1.6** put it in):

```
predicted stored             38,983,278
actual v0.1.7 stored        110,940,708
residual                     71,957,430
```

**Exact decomposition of that residual** (exact rational rates, so it closes with
zero rounding):

| term | canonical | rate delta | bytes |
| --- | --: | --- | --: |
| **DF** — v0.1.6 delta → v0.1.7 FULL | 212,532,740 | 0.351858119 − 0.057131994 | **+62,638,951** |
| DD — delta in both, rate degraded | 72,953,482 | 0.209624489 − 0.057131994 | +11,124,859 |
| FD — v0.1.6 full → v0.1.7 delta | 9,079,691 | 0.209624489 − 0.360035169 | −1,365,682 |
| FF — full in both | 53,894,382 | 0.351858119 − 0.360035169 | −440,697 |
| | | **sum** | **+71,957,430** |

`62,638,951 / 71,957,430 = 87.05 %` of the whole-file residual, and
`62,638,951 / 79,548,416 = 78.74 %` of the **entire** apparent-byte excess, is the
single class **DF**: objects v0.1.6 stored as a delta and v0.1.7 stores as a FULL
object.

### 5.4 Which policy differences can move bytes, and by how much

Every difference found in §3, with its **byte ceiling** computed, not asserted:

| # | difference | can it move bytes? | ceiling / measured effect |
| --: | --- | --- | --- |
| D1 | `metadata_delta_max_depth` 16 → 8 | **yes, but bounded by the whole pooled lane** | The entire v0.1.6 pooled-metadata lane is **2,240,958 B** = **2.82 %** of the 79,548,416 B excess. Measured, v0.1.7's is **2,019,419 B — 221,539 B *smaller*.** The regression is real and has the **wrong sign**. |
| D2 | `METADATA_CLOSURE` 128 KiB → 65,536 | **same ceiling as D1** | Same lane, same 2,240,958 B ceiling, same −221,539 B measured. |
| D3 | chunk chain closure 1 MiB → 512 KiB canonical / 256 KiB encoded | **no — provably non-binding** | Depth cap is 4 on both sides and the maximum chunk payload is 32,768 B, so the largest chain a policy can accept is `4 × 33,024 + 32,789 = 164,885 B` encoded and `4 × 32,768 + 32,789 = 163,861 B` canonical — both **below** 256 KiB / 512 KiB. The v0.1.7 limit cannot refuse a chunk delta the v0.1.6 limit allowed. Ceiling **0 B**. |
| D4 | candidate cache lifetime session → one save | **yes, bounded by the kind-1 population** | v0.1.6's kind-1 deltas are 4,796 objects / 17,754,920 canonical; v0.1.7 keeps 4,247 of them. The lost part is **549 objects / 4,517,348 canonical**, and pricing it at `(0.351858119 − 0.057131994)` gives **1,331,384 B = 1.67 %** of the excess. |
| D5 | `GROUP_TARGET = 48 KiB` added | **yes, marginally** | It changes when an Ordinary/Native group is sealed, i.e. group count and therefore the 16-byte directory entries. Total measured pack-framing difference across both Stores: **+19,564 B = 0.025 %**. Ceiling well under 1 % of the excess. |
| D6 | v3 (`Small`) and v5 (`Metadata`) framings dropped; v7 (`Singleton`) added | **no, for this workload** | Neither Store contains a v3 or v5 pack (§4.2); the largest corpus blob is 1,241,221 B, which is chunked, and chunk records cap at 32,768 B, so no singleton pack is written either. Ceiling **0 B**. |
| D7 | the codec parameter set | **no** | Identical parameter list and values (§3.3); measured FULL rate ratio **0.977288** — v0.1.7 is 2.27 % **better** on the bytes that carry no delta. Ceiling **≤ 0 B** in the direction of the excess. |
| D8 | CDC profile | **no** | Same constants, same masks, same 256-entry GEAR table (`identical=True`). Ceiling **0 B**. |
| D9 | `APPLICATION_ID` +1, `SCHEMA_VERSION` 10→4, `store_policy` table, two new `objects` columns and one index | **yes, ~5.7 MB** | This is the `non-pack` term: **+5,710,857 B = 7.18 %** of the excess. It is schema overhead, not encoding policy. |
| D10 | `READ_OBJECT_LIMIT = 4,096`, `BATCH_*`, per-role encode workspaces | **no** | Resource bounds; they change how many bytes are resident, not how many are written. |

**Sum of the byte-moving ceilings that could push *toward* the excess:**
`0 (D3) + 1,331,384 (D4) + ≤19,564 (D5) + 5,710,857 (D9) ≈ 7,061,805 B = 8.88 %`.
Add the pooled lane's 2,240,958 B ceiling for D1/D2 and the absolute worst case is
**9,302,763 B = 11.69 %** of the 79,548,416 B — and both of those are measured to
have the **opposite sign**.

**88.3 % of the excess is not reachable by any policy or codec default difference
found in §3.** It is the DF class, i.e. a difference in *whether the whole-file
admission path acquires a base at all*.

### 5.5 Both Stores' whole-file chains reach the configured cap

Chain depths walked over `base_object_id` in v0.1.7:

```
depth 0: 32,771   depth 1: 18,705   depth 2: 190   depth 3: 137   depth 4: 81
depth 5:     55   depth 6:     44   depth 7:  27   depth 8:  22
```

22 objects sit exactly at the configured depth 8. **The depth cap is not what
stopped the DF class**: v0.1.6's cap is also 8 (§3.2), and v0.1.7 demonstrably
builds chains that reach it. `store_policy` in the v0.1.7 Store reads back exactly
`small_file_threshold_bytes = 131072, whole_file_delta_max_depth = 8,
chunk_delta_max_depth = 4, metadata_delta_max_depth = 8, format_profile = 1` —
the frozen default, as the harness requests it (`ops/history.rs:632` passes
`StoragePolicy::frozen_default()`).

### 5.6 The bases were available

For every one of the 18,820 DF objects, **the base identity v0.1.6 recorded in its
own record is present in the v0.1.7 Store**:

```
DF: v0.1.6's base id present in the v0.1.7 store: 18,820 / 18,820 (100.0%)
DD: base present:                                17,084 / 17,084 (100.0%)
v0.1.6 native PREFIX records: 650 ; distinct base ids 576 ; present in v0.1.7: 576 (100.0%)
```

So the loss is **not** "the base was never stored". The bytes were there and the
policy would have admitted them.

---

## 6. Negative results — what I ruled out, how, and with what number

**N1 — the codec did not regress.** Rule-out: read both parameter sequences
(`crates/.../pack.rs:1213-1218` vs `core/.../encoding/codec.rs:213-219` and
`codec.rs:347-362`); they are the same zstd `c_` parameters with the same values
(level 3 payload, level 1 group, window 18/20 payload, window ≤ 16 group,
contentSize 1, checksum 1, no dictID, 0 workers). Corroborated by measurement:
FULL-object rate **0.351858119 vs 0.360035169 B/B = 0.977288**, i.e. v0.1.7 is
2.27 % **better** on 266,427,122 canonical bytes. A codec regression would show the
opposite sign on exactly this population.

**N2 — v0.1.7's smaller chunk chain budget cannot refuse anything v0.1.6 allowed.**
Rule-out by worst case: depth cap 4 on both sides, maximum chunk payload 32,768 B
(`MAXIMUM_CHUNK_BYTES`, identical), maximum frame 33,024 B. Largest admissible
chain: `4 × 33,024 + 32,789 = 164,885 B` encoded < 262,144 B; `4 × 32,768 +
32,789 = 163,861 B` canonical < 524,288 B. The budget never binds. **The chunk lane's
1,737,621 B loss is therefore not a chain-budget effect.**

**N3 — the whole-file chain budgets are unchanged and could not have refused the DF
class.** `CHAIN_CANONICAL_LIMIT` is 512 KiB and `CHAIN_ENCODED_LIMIT` is 256 KiB on
both sides. v0.1.7 charges `chain.encoded_bytes + canonical.len()` where v0.1.6
charged `encoded_closure + delta.len() + 41`; that is a stricter *accounting* of the
same constant, but it is checked against a whole-file payload capped at 131,071 B,
so the worst-case difference between the two accountings is bounded by 131,071 B per
object — it cannot explain a class of 18,820 objects.

**N4 — v0.1.6's 2 MiB whole-file *owner* form is dead on the write path.** Rule-out:
`crates/layerfs-layerstack-store/src/objects/whole.rs:1` states *"Read compatibility
for previously compacted Stores; writers exist only in tests"*, and the only
non-test callers are in `read.rs` (`544, 900, 1096, 1648, 1742`). The v0.1.6
write path for a small file is `content::build_bytes` → `encode_small` →
`put_file_payload`, bounded by `SMALL_LIMIT`. **So v0.1.6's small-file payload
ceiling is also 131,071 B, not 2 MiB.** No byte difference here.

**N5 — v0.1.7's `MAXIMUM_DELTA_MAX_DEPTH = 50` is not a v0.1.6 default.**
Rule-out: the only 50 in the v0.1.6 tree is `whole.rs:19  const EDGES: usize = 50`,
in the read-only compatibility module N4 rules out. The operative v0.1.6 write bound
is `delta::CHAIN_EDGES = 8`, and v0.1.7's *default* is also 8. The 50 is a
validation ceiling.

**N6 — the pooled-metadata depth/closure regression has the wrong sign.** v0.1.6
depth 16 / closure 128 KiB; v0.1.7 depth 8 / closure 65,536 B — a genuine halving,
and the only place a default really did regress. Measured effect: v0.1.7's pooled
lane is **2,019,419 B against 2,240,958 B, i.e. 221,539 B smaller**, on 8,361,152
canonical bytes against v0.1.6's 8,033,879. The regression is real; it is not the
cause.

**N7 — the dropped v3/v5 framings cost nothing here.** Neither Store contains a v3
or v5 pack (§4.2); the v0.1.6 lane histogram over objects is `{1: 4,847, 2: 1,098,
4: 44,141, 6: 1,636}` and the v0.1.7 one is `{1: 6,786, 2: 1,098, 4: 44,148}` plus
the v6 value-group packs.

**N8 — `GROUP_TARGET` is not a byte lever.** Measured: total pack framing
(header + directory across all packs) is 215,664 B in v0.1.7 and 196,100 B in
v0.1.6 — a **+19,564 B = 0.025 %** difference.

**N9 — the "stride-3 union is 1.010x canonical" argument is confirmed, not
re-derived.** The brief's own numbers already exclude a workload difference;
independently, v0.1.7's whole-file lane holds 44,148 objects / 348,460,709 canonical
against v0.1.6's 44,141 / 348,460,295 — **+7 objects, +414 B, 0.0001 %**. The two
Stores hold the *same* whole-file work.

---

## 7. Hypotheses (labelled as hypotheses — no arithmetic behind them yet)

**H1 (hypothesis, not measured).** The DF class is the explicit-predecessor route.
v0.1.6's kind-2 population (31,108 objects, the base resolved through
`small_predecessor` on `prior_ids[0]`) is the exact analogue of v0.1.7's
`AdvisoryPredecessors::explicit(view.root())` route (`content/file/edit/apply.rs:121`
→ `storage/cas/save.rs:100` → `delta/select.rs:342 acquisition`). v0.1.7 retains
12,837 of 31,108 by count and 59,715,910 of 267,731,302 by bytes. **I have not
measured whether the advisory list is empty on the missing objects, or non-empty and
refused by `probe`.** Both are consistent with everything in §5.

**H2 (hypothesis, supported by a shape but not by arithmetic).** The v0.1.7 delta
chain does not extend across states the way v0.1.6's does. Slicing the 44,141 common
whole-file objects by v0.1.7 pack order (a proxy for state order, 8 slices of ~5,517):

```
slice    v0.1.6 delta share   v0.1.7 delta share
  0            64.6%                37.8%
  1            77.5%                38.4%
  2            83.3%                40.7%
  3            85.9%                40.3%
  4            86.7%                43.1%
  5            80.0%                41.6%
  6            83.1%                45.9%
  7            89.6%                44.6%
```

v0.1.6's coverage **rises** with history depth; v0.1.7's is **flat**. That is the
shape a chain that keeps extending would produce against one that stops early. It is
a shape, not a proof, and I did not test it: the pack-order slice is a proxy, not the
state index, and the state index is not recorded in the Store.

**H3 (hypothesis).** The chunk lane's total loss of PREFIX selection (§4.4) has the
same root cause as H1/H2. v0.1.6's chunk route reads up to four hints from
`cursor.hints(...)` (`objects.rs:2855`) and takes the first present one
(`admission.rs:649`); v0.1.7's takes `advisory.first()` only
(`delta/select.rs:247`). Whether the v0.1.7 advisory is empty for those chunks is
**not measured**.

**What would settle H1–H3** (a suggestion for whoever owns the mechanism, not a
result): decode `trace.jsonl`'s per-state `history.state.N.inserted` and
`objects` counters against a per-state pack range, or instrument a diagnostic
build — but **not** by changing any `core/crates/` default, and not at
`history-stride1`.

---

## 8. Bottom line

1. **The codec/policy set is a superset with the same defaults, with three
   exceptions — and none of the three explains the bytes.**
   - identical: cutoff (131,072, exclusive), `Representation` selection, whole-file
     depth 8, chunk depth 4, both payload codec profiles, the group codec, both chain
     budgets, every group/pack/directory limit, both record grammars, the CDC profile
     and GEAR table, and every candidate-cache capacity and rule;
   - **regressed**: `metadata_delta_max_depth` 16→8,
     `METADATA_CLOSURE` 128 KiB→65,536, and the candidate cache's lifetime
     (session→one save);
   - **added**: `GROUP_TARGET` 48 KiB, the singleton lane, configurable policy,
     `MAXIMUM_DELTA_MAX_DEPTH` 50 as a validation ceiling only.
2. **The measured byte effect of the regressions is bounded by 11.69 % of the
   excess in the worst case, and the two largest are measured with the opposite
   sign.** Pooled lane −221,539 B; cache lifetime ≤ 1,331,384 B; `GROUP_TARGET`
   ≤ 19,564 B; schema overhead +5,710,857 B.
3. **90.46 % of the 79,548,416 B excess is the whole-file lane**, and **78.74 % of
   the whole excess is one class**: 18,820 objects / 212,532,740 canonical bytes that
   v0.1.6 stored as a delta against a declared predecessor and v0.1.7 stores as a
   FULL object — even though **100 % of those bases are present in the v0.1.7
   Store**.
4. **The search should move off codec and policy defaults.** They are the same. It
   should move to the whole-file admission path's base acquisition — and to the
   chunk lane, which lost 100 % of its PREFIX selection (1,737,621 B).

---

## 9. Reproduction

```sh
cd /Users/yifanxu/Ephemeral-AI-Lab/layerfs
S=docs/roadmap/0.1/0.1.7/evidence/stage-6-history-187-20260920T000000Z/squad-a

# 1. the v0.1.6 source pin really is the reference tree
git diff --stat 7fab1027a HEAD -- crates/

# 2. both Stores, per lane (retained as decode_packs.txt)
python3 $S/decode_packs.py \
  /tmp/base187/sample.sqlite \
  benchmark-results/repository-history/stride-10/deepseek-stride10/host-runtime/store.sqlite

# 3. reconciliation, class rates, confusion matrix, counterfactual (retained as accounting.txt)
python3 $S/accounting.py
```

No lane was run, no product file was touched, nothing was committed, and no timing
was taken.
