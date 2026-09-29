# #286 diagnostic correction r023: Zstandard workspace input

> **Correction, no new candidate sample.** The r022 report/JSON's **wide-policy workspace assertion is wrong**. Its 6,948/6,948 exact level9 frame calibration,322,340-byte default-history FULL-frame width diagnostic and1.856136s offline CPU difference remain unchanged. R021's strict at-run storage FAIL stays FAIL.

R022's context probe called `ZSTD_getCParams(level, raw_length, **0**)`, while the shipped product's `encoding/codec.rs` calls `ZSTD_getCParams(level, raw_length, **raw_length**)`. The [reproduction C probe](20260930-history-codec-workspace-correction-r023.c) passes the product's actual third argument to host Zstandard1.5.7. Its [result](20260930-history-codec-workspace-correction-r023.json) is:

| Raw width / context | Level9 | Level12 | Existing static encode bound |
| --- | ---: | ---: | ---: |
| 131,071 B (default history cutoff) | 3,662,872 B | **4,711,448 B** | 16,777,216 B; both fit |
| 1,000,000 B (wide accepted cutoff) | **13,100,056 B** | **25,682,968 B** | 16,777,216 B; level12 does not fit |

The first changed-source codec test refused the widest accepted policy with `Integrity("bounded Zstandard workspace unavailable")`, exactly as the corrected bound predicts. Source was then changed **before any candidate invocation** to select level12 only when the declared whole-file raw limit is at most131,071 B; wider accepted policies select the shipped level9 before the codec call, without catching an error or changing the16-MiB workspace. The focused codec, stored-payload, delta and cross-save-index tests passed after this repair, including a wide-policy frame roundtrip and exact selected-FULL frame check. The r022 frame-width/CPU diagnostic applies to the default history profile, which is within the corrected level12 bound. It is still a diagnostic: actual original-owner allocation and complete driver wall require a new source-sealed sample.

The original r022 report, JSON, script and hashes remain unmodified. This append-only correction identifies the affected fields and the source-input mistake instead of silently replacing evidence. The v3 selected PASS rows r020 and stride1 storage FAIL r021 remain unchanged; no family completion or release claim follows. This diagnostic/docs-only commit records production LOC reference65,417→65,417 (+0), Core70,109→70,109 (+0), combined135,526→135,526 (+0), counted with `tools/production_loc.py --json --root <snapshot>` on exact first-parent/staged/committed Git archives (counter SHA256 `c0fe7f36a0d4144bbd2b61c272c7579cc0d56ffe23f9588287ea30e793624adb`).
