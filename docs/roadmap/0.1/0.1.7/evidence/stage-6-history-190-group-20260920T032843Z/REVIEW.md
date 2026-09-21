# Independent review — group compression level

> Status: Research; informative and not a product contract.

Reviewer owns only this document. Source review is read-only; no build, test, measurement, Store read or large hash is performed concurrently with the root's measurements.

## Prospective protocol review

[PROTOCOL.md](PROTOCOL.md) passes review as an exploratory diagnostic, not admission evidence. It specifies one live sample per arm, immutable independently built binaries, the same corpus/harness/payload level/workspace/worker count, fresh growing Stores, declared uncontrolled OS and intra-chain residency, and serialized resource work. The one-second operation threshold and acceptance of a small allocation overage follow the owner's instructions; no new unlimited storage tolerance is inferred. The diagnostic caps do not convert ordinary complete-command budget misses into passes.

The retained historical codec CPU differences motivate this experiment only. They must not be described as the current operation saving. Likewise, whole-invocation CPU is not codec-exclusive CPU. Compression changes physical group bytes and may change pack layout, so canonical identities and successful read-back are the appropriate correctness comparisons; byte-identical Store files are not required.

## Source review before measurement

Baseline commit: `605f6efc6a095a1b6335cbc5549dc0fda78ed9ab`.

In `core/crates/layerfs-storage/src/encoding/codec.rs`, `PAYLOAD_LEVEL = 3` at line 94, `GROUP_LEVEL = 19` at line 102, `GROUP_WINDOW_LOG_MAX = 16` at line 104, and `ENCODE_WORKSPACE_BYTES = 16 * 1024 * 1024` at line 78. `CompressionWorkspace::compress_group` at lines 386–425 obtains compression parameters from `GROUP_LEVEL`, clamps the window log, resets the session and parameters, retains content size/checksum/no dictionary ID, enforces the frame bound, and compresses in the existing caller-owned static context. No worker is added. The public group decode path validates the same envelope independent of the chosen compression level.

Changing only `GROUP_LEVEL` from 19 to 1 changes group encoding parameters, including the level-selected algorithm. It does not justify changing the payload profile, workspace allocation, integrity validation, or construction parallelism. The code has pre-existing misleading comments that describe payload level 9 despite the actual constant being 3; these should be corrected if the candidate is retained, after source-pair identity is frozen and with comments-only changes distinguished from executable changes.

Candidate diff and measurement result review: **PENDING**, not a passing performance claim.

## Frozen candidate source check

The inspected working-tree diff against baseline changes exactly one source line: `const GROUP_LEVEL: i32 = 19;` to `const GROUP_LEVEL: i32 = 1;`. No payload/workspace/window/flag/worker or other algorithm change accompanies it. **Source treatment: PASS.** Baseline archive identity declares binary SHA-256 `82fb535ddc74eb84aebfaac98d90058600331eb52ca8a8feffc5341b222a9d11`; independent full-binary hashing remains deferred until the root permits a quiet verification window.

If comments are corrected after measurements, preserve the measured source identity and record the comments-only diff separately. A final build/check result can qualify final source, but does not retroactively make a rebuilt executable the measured executable. State executable-token equivalence and its exact scope rather than claiming identical full source or binary hashes.

## Stride10 raw arithmetic review

Recomputed directly from each arm's `runs/<arm>-history-stride10/raw/timing.json`, `raw/phases-perf.json`, `trace-perf.jsonl`, and `perf-receipt.json`, independently of the campaign summary/analyzer. The sum of exactly 17 named state children equals `operation_ns` in both arms. The following are integer nanoseconds, except rows explicitly labelled bytes.

| Measurement | Level 19 baseline | Level 1 candidate | Baseline minus candidate |
| --- | ---: | ---: | ---: |
| Measured operation | 22,506,420,003 | 20,914,489,418 | 1,591,930,585 |
| `storage.begin` | 12,781,292 | 12,981,830 | -200,538 |
| `storage.accept_loop` | 9,828,309,917 | 8,958,882,915 | 869,427,002 |
| `storage.finish` | 1,209,011,123 | 392,981,959 | 816,029,164 |
| Sum of those three storage phases | 11,050,102,332 | 9,364,846,704 | 1,685,255,628 |
| `filesystem` | 9,517,619,792 | 9,569,275,834 | -51,656,042 |
| Whole-invocation user + system CPU | 32,955,231,000 | 30,396,308,000 | 2,558,923,000 |
| Complete command | 41,663,305,750 | 32,762,497,334 | 8,900,808,416 |
| Corpus-read trace interval | 17,192,211,998 | 9,855,647,587 | 7,336,564,411 |
| Lifetime peak RSS, bytes | 252,936,192 | 255,803,392 | -2,867,200 |
| Allocated Store bytes | 49,651,712 | 50,249,728 | -598,016 |
| Apparent Store bytes | 49,053,696 | 49,324,032 | -270,336 |

The operation saving is `1,591,930,585 / 22,506,420,003 = 7.073228815545979%`. Storage-phase reduction exceeds the one-second threshold and closely accounts for the operation reduction; other operation work regresses by exactly 93,325,043 ns. Whole-invocation CPU reduction corroborates less work but does not measure codec CPU exclusively. **Exclusive codec CPU: NOT_MEASURED.** The 8,900,808,416 ns command-wall reduction is not attributable to compression: the corpus-read trace interval alone falls by 7,336,564,411 ns. Do not market that whole wall difference as the codec saving.

The complete direct-child decomposition closes exactly by explicitly retaining `state.residual`: baseline 48,285,295 ns, candidate 76,828,630 ns. The residual is time outside the immediate named children, not a further attributed product mechanism.

All 17 root IDs match between perf traces. All final `delta.*` counters match. Provider read waves remain 66,616, requested/returned objects 74,279, returned canonical bytes 148,826,516, failed waves zero, physical group decodes 2,723, and pooled pack fetches 79,784. Only provider elapsed time and pooled pack bytes differ among the aggregated provider counters: elapsed increases 52,050,079 ns; pooled bytes increase from 7,378,994,999 to 7,985,771,449, exactly 606,776,450 more. This is application-level pack-byte copying, not a physical-disk byte claim.

The allocation increment is 598,016 bytes, or 1.2044217125886818% of this matched baseline allocation. The candidate is 905,216 bytes above the historical 49,344,512-byte target; the baseline already exceeds that target by 307,200 bytes. The numerical miss remains a miss, separately subject to the owner's time/space acceptance. The reviewer does not invent an absolute storage tolerance.

Both preflights satisfied the prospective idle check (73.77% baseline, 85.39% candidate). Cache residency remains uncontrolled and the records correctly declare cold admission **INELIGIBLE**. Missing O3 pinned counters remain **INCOMPLETE**. Current raw arithmetic supports proceeding to stride3 confirmation and semantic verification; it is not yet final acceptance. Separate read-back and canonical Store comparison are **PENDING** at this review checkpoint.

## Stride10 separate verification

Recomputed from `raw/phases-verify.json`, `raw/trace.jsonl`, and `verify-receipt.json` after both completed: baseline verification work is 5,617,963,792 ns and candidate 5,405,493,708 ns, both under the 10,000,000,000 ns target. Complete verifier commands are 5,633,108,834 and 5,449,594,167 ns, under the 60-second hard cap. Both execute successfully with 17 states, 101,477 path-states, 1,083 sampled comparisons, 893 files / 5,619,947 bytes read, six symlinks, and zero mismatches/missing/unexpected paths. This is sampled read-back, not exhaustive payload read-back. O3 remains INCOMPLETE and diagnostic cache admission remains INELIGIBLE despite semantic verification passing. Full canonical catalogue comparison is still pending the root's exclusive post-measurement resource window.

## Stride3 raw confirmation

Independently summed the 53 direct state children from each `raw/timing.json` and matched them to `raw/phases-perf.json`. Read matching perf receipts and immutable perf traces; no SQLite or binary reads occurred during another owner's resource window.

| Measurement | Level 19 baseline | Level 1 candidate | Baseline minus candidate |
| --- | ---: | ---: | ---: |
| Measured operation, ns | 58,410,516,121 | 56,577,520,957 | 1,832,995,164 |
| `storage.begin`, ns | 28,925,585 | 30,912,208 | -1,986,623 |
| `storage.accept_loop`, ns | 19,645,751,793 | 18,509,215,583 | 1,136,536,210 |
| `storage.finish`, ns | 2,949,689,208 | 1,621,120,380 | 1,328,568,828 |
| Sum of those three storage phases, ns | 22,624,366,586 | 20,161,248,171 | 2,463,118,415 |
| `filesystem`, ns | 32,082,393,749 | 32,639,517,460 | -557,123,711 |
| Whole-invocation user + system CPU, ns | 71,778,254,000 | 69,653,420,000 | 2,124,834,000 |
| Complete command, ns | 82,295,809,041 | 79,910,471,208 | 2,385,337,833 |
| Corpus-read trace interval, ns | 23,237,955,165 | 22,659,132,708 | 578,822,457 |
| Lifetime peak RSS, bytes | 254,623,744 | 254,066,688 | 557,056 |
| Allocated Store bytes | 62,783,488 | 62,152,704 | 630,784 |
| Apparent Store bytes | 61,767,680 | 62,152,704 | -385,024 |

Operation reduction is `1,832,995,164 / 58,410,516,121 = 3.138125265325286%`. Storage saves 2,463,118,415 ns while all other operation work collectively costs 630,123,251 ns more. Direct-child residual is retained explicitly: 153,904,613 ns baseline, 203,648,505 ns candidate. The smaller allocated byte count does not establish a codec storage saving: apparent Store bytes grow, and filesystem allocation overhead differs across the matched files.

All 53 roots and all final `delta.*` counters match. Aggregated provider counters also match except elapsed time and pooled pack bytes. Baseline/candidate provider elapsed is 30,642,989,093 / 31,142,980,428 ns; pooled pack bytes are 23,399,127,004 / 25,105,269,671. Read waves remain 111,853, requested/returned objects 135,296, physical group decodes 20,959, and pack fetches 427,384. Preflight idle is 83.55% / 81.29%, both meeting the prospective rule. This confirms the operation saving on stride3 without claiming lower read work or codec-exclusive CPU. Final sampled read-back and catalogue identity/custody checks remain pending at this checkpoint.

## Post-measurement comment and architecture review

Manually inspected [final-comment-only.patch](final-comment-only.patch) and [final-executable-equivalence.json](final-executable-equivalence.json). The final source has only comment changes relative to the measured candidate: payload-level and group-level descriptions are corrected to 3 and 1, and the historical workspace narrative becomes an accurate explanation of the unchanged 16 MiB allocation. `PAYLOAD_LEVEL = 3` appears as moved context in the unified diff but is unchanged in the non-comment code sequence. All effective code lines remain equal. The full source hashes intentionally differ; the measured identity is not rewritten. **Comments-only equivalence review: PASS.**

The architecture addition accurately describes group level 1, fixed payload level 3, unchanged workspace/frame/validation/canonical policy, and potentially different physical sizes and byte-charged transaction cadence. It preserves prior pooled read and statement reuse. Final workspace build/check evidence remains a distinct proof from the archived executable's timing evidence.

## Independent post-measurement custody and catalogue checks

Held both global flocks and the normal private measurement lock for this read-only audit, after all four measured runs and verifiers completed. Rehashed both archived measured binaries, all four Stores, and canonical catalogue inventories independently; all hashes reproduce the declared identities and root inventory report. Compared the complete sorted `(object_id, object_role, canonical_length)` tuples directly between each matched pair: exact equality. Both stride10 Stores contain 52,032 objects / 380,921,328 canonical bytes; both stride3 Stores contain 72,560 objects / 589,916,570 canonical bytes. `PRAGMA quick_check` returns `ok` for all four.

```json
[
  {
    "binary": "baseline",
    "sha256": "82fb535ddc74eb84aebfaac98d90058600331eb52ca8a8feffc5341b222a9d11"
  },
  {
    "binary": "candidate",
    "sha256": "13bcfe86598d3c13f902d6ab7d0a15d24f9ed26bb2c67dadbfe1a34ea6108a2d"
  },
  {
    "case": "history-stride10",
    "arm": "baseline",
    "objects": 52032,
    "canonical_bytes": 380921328,
    "inventory_sha256": "18a925c78bae5076e07200d324556b10dbd4d0a46641170ebde29a4be3eb3fab",
    "store_sha256": "ab64dab7aaed512ab93b7ccc46fd14f22e57791f642b39fdf2b94650a41a965f",
    "packs": [
      255,
      45035732
    ],
    "quick_check": "ok"
  },
  {
    "case": "history-stride10",
    "arm": "candidate",
    "objects": 52032,
    "canonical_bytes": 380921328,
    "inventory_sha256": "18a925c78bae5076e07200d324556b10dbd4d0a46641170ebde29a4be3eb3fab",
    "store_sha256": "4af37932aa3391b12269de8130b9c66dc504f64ca78fc3e585f7afddabed8487",
    "packs": [
      255,
      45297954
    ],
    "quick_check": "ok"
  },
  {
    "case": "history-stride3",
    "arm": "baseline",
    "objects": 72560,
    "canonical_bytes": 589916570,
    "inventory_sha256": "e93b492b77c6eb6d3dda180833454f3be17b9223360390cf7f8e510500b6c43d",
    "store_sha256": "cfb74bc9b1db6c9470129613283a8f4afa3b139c2819ae7bd7a4f14e548ecdb5",
    "packs": [
      437,
      56192535
    ],
    "quick_check": "ok"
  },
  {
    "case": "history-stride3",
    "arm": "candidate",
    "objects": 72560,
    "canonical_bytes": 589916570,
    "inventory_sha256": "e93b492b77c6eb6d3dda180833454f3be17b9223360390cf7f8e510500b6c43d",
    "store_sha256": "f5c7ff5a6b4f0821aa9a21ac5250335c4c3fb889637a0c5345c5278caadc2a9e",
    "packs": [
      437,
      56590252
    ],
    "quick_check": "ok"
  }
]
```

This catalogue equality proves identity/role/length inventory equality, not exhaustive re-decoding of every object. The separately retained sampled verifier supplies actual decode/read-back coverage. Binary and Store custody checks: **PASS**. All locks were released after the audit.

## Stride3 verification and reviewer disposition

Independently read both final verifier traces/phase receipts and confirmed each verifier binary hash matches its measured arm. Baseline verification work is 18,824,407,917 ns; candidate is 18,904,055,292 ns. Both are under the 20,000,000,000 ns stride3 target. Complete commands are 18,847,427,041 / 18,948,330,625 ns, below the 60-second hard cap. Each checks 53 states, 306,861 path-states, 3,377 sampled paths, 2,756 files / 18,936,332 bytes, and five symlinks. Mismatches, missing and unexpected paths are zero in both arms. Verification becomes 79,647,375 ns slower in the candidate; it is not a verification speedup.

**Disposition: retain the one-line group-level change, subject to completing the required final-workspace checks.** Stride10 saves 1,591,930,585 ns and stride3 saves 1,832,995,164 ns of measured operation, with larger corresponding storage-phase reductions and lower whole-invocation CPU in both pairs. The physical representation is larger: pack bodies increase by 262,222 bytes on stride10 and 397,717 bytes on stride3. Stride10's allocated Store increases by 598,016 bytes and exceeds the historical target by 905,216 bytes; this is explicitly reported and assessed under the owner's small-space-for-time ruling, not relabelled a numerical pass. The algorithm, canonical inventory, roots and sampled correctness checks remain intact.

The remaining evidence limits are material: one sample per arm, uncontrolled cache residency and desktop activity, no exclusive codec CPU measurement, no pinned O3 history counters, and sampled rather than exhaustive payload verification. No release-admission claim follows. The source change does not remove repeated pooled pack fetching; provider byte copying grows. Selective pack reads remain a separate unmeasured optimization direction and receive no speedup credit from this experiment.
