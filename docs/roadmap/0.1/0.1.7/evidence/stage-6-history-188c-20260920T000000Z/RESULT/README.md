# stride10 — the result

**Diagnostic.** Measured on `/tmp/f1/sample.sqlite`, re-run for this receipt. Source pin `66bce8378@ +
the working tree. **No timing figure appears here.** The 217-row lane and @history-stride1@ were not run.

## The number

| stage | apparent | vs v0.1.6 |
| --- | --: | --: |
| the registered lane (historical) | 128,864,256 | 2.6130x |
| + the faithful declaration (T1's arm) | 57,749,504 | 1.1710x |
| + v0.1.6's declaration rule | 51,347,456 | 1.0412x |
| **+ L4, the chunk cursor — AS THE LANE STANDS NOW** | **49,672,192** | **1.00723x** |
| + a close-time VACUUM | **48,717,824** | **0.98787x** |
| **v0.1.6** | **49,315,840** | 1.0000x |

@```
  as-run                     49,672,192   vs v0.1.6 49,315,840   = 1.00723x   +356,352   (0.72 % above)
  vacuumed                   48,717,824   vs v0.1.6 49,315,840   = 0.98787x   -598,016   [recorded basis]
  vacuumed, like-for-like    48,717,824   vs v0.1.6 48,857,088   = 0.99715x   -139,264   [both vacuumed]
@```

**The lane as it now stands is 356,352 B (0.72 %) above v0.1.6. It is not yet at the gate.**
A close-time VACUUM takes it below — by 598,016 B on the basis the exit criterion names, or by
**139,264 B like-for-like**, which is the honest figure.

**From the registered lane to now: 128,864,256 -> 49,672,192 = -79,192,064 B, a 61.5 % reduction**, and
**2.6130x -> 1.00723x** against v0.1.6.

## What each step was

| step | bytes | how | ruling |
| --- | --: | --- | --- |
| the faithful declaration | -65,126,400 | the driver declares the previous version's content root | harness, 0 product lines |
| v0.1.6's declaration rule | -6,402,048 | use the declared predecessor when one exists; the similarity index only when it does not | harness, 0 product lines |
| **L4, the chunk cursor** | **-1,675,264** | C1 offers each emitted chunk the base extent covering its byte range | **none needed** — the cursor was proposed, never deferred |
| T1's product work (R1a, R2, R3) | -5,414,912 | lean row grammar; index capacity; the depth-measurement fix | rulings A and C, applied by the T1 squad |
| *VACUUM (not applied)* | *-954,368* | *rewrite the file densely* | ***a ruling — close-time I/O unmeasured*** |

## Why it is credible — the cross-checks

- **The whole-file lane is now 37,347,557 B against v0.1.6's 38,983,278 B — we beat it by 1,635,721 B.**
- **The native lane is 3,957,829 B against its 3,958,057 B — 228 B below**, and the selection is
  **byte-identical** to v0.1.6's (448 FULL / 650 PREFIX, same raw and stored bytes per class). The 228 B
  edge is group framing.
- Every residual in the decomposition sums to zero (V1, V4, V6).
- @shared/space.py@ (fail-closed) decodes every figure; the pack directory closes
  (@blob = bodies + framing@) on every Store.

## What is NOT claimed

- **No stride3 confirmation exists.** The guardrail requires one and none has been run, so **none of this
  is a gate claim.**
- The verify phase is a **declared sample** — 1,083 of 101,477 path-states, 893 files / 5,619,947 B read
  back, 0 mismatches — and the row reports @INCOMPLETE@, never a full PASS.
- **VACUUM's close-time I/O cost is unmeasured**, and this lane already records
  @budget.complete-command@ **40.0 s against a 15 s ceiling**.
- The **allocated** axis (49,344,512) is not reproducible on this volume: @st_blocks x 512@ read
  135,118,848 / 135,192,576 / 130,799,104 / 134,537,216 for the same closed file. **The gate should be
  decided on apparent bytes**, which equals @page_count x page_size@ exactly.
- **Production LOC: core subtotal 19,537 -> 19,766 (+229)**; combined 84,936 -> 85,165. Harness lines are
  separate.
