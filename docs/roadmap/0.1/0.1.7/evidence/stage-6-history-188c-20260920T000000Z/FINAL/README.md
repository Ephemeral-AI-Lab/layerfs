# #188 — closing state

**Owner closed the issue on 2026-09-20 with the work at this state.** Every number is **diagnostic**.
Source pin @66bce8378@ + the working tree. **No timing figure is quoted** — the machine was shared for
most of the campaign.

## The result, stride10 (17 states, apparent bytes)

| arm | apparent | vs v0.1.6 | reduction from the registered lane |
| --- | --: | --: | --: |
| the registered lane (historical) | 128,864,256 | 2.6130x | — |
| **the lane's default now** | **56,049,664** | **1.13654x** | **-72,814,592 = 56.5 %** |
| **+ the similarity source** | **49,672,192** | **1.00723x** | **-79,192,064 = 61.5 %** |
| v0.1.6 | 49,315,840 | 1.0000x | |

**The gate is NOT met.** The default is **6,733,824 B above** v0.1.6; with the similarity source it is
**356,352 B above**. The issue is closed by owner decision at this state, not on a met gate.

## What was achieved

| step | bytes | how |
| --- | --: | --- |
| the declaration rule (harness) | -65,126,400 | the driver declares the previous version's content root — the Store had been supplied **no base for 26,847 objects** |
| the similarity source (harness, off by default) | -6,377,472 | declared predecessor first; the content-similarity index only when none was declared |
| **L4 — the C1 chunk cursor** | **-1,675,264** | C1 offers each emitted chunk the base extent covering its byte range |
| T1's product work (R1a, R2, R3) | -5,414,912 | lean row grammar, index capacity, depth-measurement fix |

**The lane now BEATS v0.1.6 on both large lanes:** whole-file **37,347,557** against its 38,983,278
(**-1,635,721**), and native **3,957,829** against its 3,958,057 (**-228**), with the chunk selection
**byte-identical** to v0.1.6's (448 FULL / 650 PREFIX).

## Code

| | |
| --- | --- |
| new product file | @core/crates/layerfs-content/src/file/mapping/predecessor.rs@ (220 lines) |
| new product test | @core/crates/layerfs-content/tests/chunk_predecessor.rs@ (7 tests) |
| product files touched | 9, all in @layerfs-content@ and @layerfs-storage@ |
| **zero** lines in @layerfs-storage@ for L4 | the consumer already existed |
| tests | **481 pass** (474 before), @edit_pipeline.rs:641@ re-run and survives, clippy @-D warnings@ clean, fmt clean |
| guards | boundary guard PASS (121 files), @core/tools@ 6 OK, @shared/@ 136 OK, @--lane full@ 220 / @--lane smoke@ 20 unchanged |
| production LOC | core subtotal **19,537 -> 19,766 (+229)**; combined 84,936 -> 85,165 |

## What the campaign got wrong, and corrected by measurement

1. **"The residual traces to the absence of Stage 7."** Falsified twice: v0.1.6's stride-10 Store holds
   **exactly 17 commits** and the same 44,141 whole-file objects. There was no richer pool.
2. **"The advisory list is always empty."** False — @filesystem/sorted/page.rs:380-390@ is a live
   producer and the sole source of the 917 cross-save InodeLeaf bases.
3. **"The delta encoder is defective."** False — B4 and B2 rebuilt it byte-exactly (44,146/44,148 record
   lengths, 0 per-object mismatches).
4. **"Four-slot cross-path ordering is the lever."** Falsified — it *gained* 8,625,119 B and *lost*
   18,214,699 B by replacing 17,141 good same-path bases.
5. **The "fallback" name.** Renamed: **declared** versus **similarity**. It was never an error path.
6. **A decoder bug of my own**, caught by the fail-closed module: whole-file directory offsets are
   absolute, not base-relative — a silent 183,584 B undercount.

## What remains, and needs a ruling

| lever | bytes | blocker |
| --- | --: | --- |
| a persisted cross-save similarity index (the product form of the similarity source) | ~6,377,472 | new table + @SCHEMA_VERSION@ bump |
| L5 — the @objects@ row grammar | 1,236,992 | format change; the remaining gap is dominated by this table (+1,634,304 B) |
| VACUUM at close | 495,616 net | close-time I/O **unmeasured** |
| codec level 3 -> 19 | ~4,270,443 | CPU **unmeasured**; the lane already records 40.0 s against a 15 s budget |
| #185 (cross-role transitions) | <= 476,317 | **0 B of the gap** — both trees refuse it |

## Not claimed

- **No stride3 confirmation exists.** Nothing here is a gate claim.
- The verify phase is a **declared sample** (1,083 of 101,477 path-states) and reports @INCOMPLETE@.
- The allocated axis is unusable: @st_blocks x 512@ gave four values for the same closed file.
- **No timing claim** except the one declared quiet-machine measurement in @TIMING/@.
