# Optimization round — status

**Diagnostic.** Source pin @66bce8378@ + the working tree. **No timing figure appears here.**

## Done, verified

### 1. The lane's defaults are corrected (harness-only, production LOC delta 0)

The lane's historical default **declared no delta base at all** — the reason its headline number was not
a product measurement. Both corrections are now the default, with every historical arm reproducible:

| arm | how to reproduce | apparent |
| --- | --- | --: |
| **faithful model + v0.1.6's declaration rule** | *(nothing set — new default)* | **51,347,456** |
| same-path previous version only | @LAYERFS_HISTORY_SAME_PATH_ONLY=1@ | 57,749,504 |
| the historical registered lane | @LAYERFS_HISTORY_ADVISORY=0@ | 124,735,488 |

**The default arm beats v0.1.6 on the whole-file lane**: ours 37,347,557 B against its 38,983,278 B —
**1,635,721 B better** — on the lane that was 58.0 % of the gap.

@ADVISORY=0@ reads 124,735,488 rather than the historical 128,864,256 because the T1 squad's R2 removed
two indexes (**4,128,768 B**). That is expected, and it is the one figure in the table that is not
like-for-like with the campaign's earlier numbers.

### 2. The guards

| check | result |
| --- | --- |
| @shared/@ suite | **136 tests, OK** |
| @core/tools/check_product_boundary.py@ | **PASS**, 120 files |
| @core/tools@ self-tests | **6 tests, OK** |
| @--lane full@ / @--lane smoke@ | **220 / 20 — unchanged** |

## Done, verified

### 3. L4 — the C1 chunk cursor (production LOC +209, one crate)

Receipt: [`L4/README.md`](L4/README.md). `layerfs-content/src/file/mapping/predecessor.rs@
(**220 physical lines**), a new public entry point @construct_bytes_with_predecessor@, and 7 external
tests. **Zero changes in @layerfs-storage@**, as Squad V2 predicted — the consumer already existed in full.

| | before | after | v0.1.6 |
| --- | --: | --: | --: |
| apparent | 51,347,456 | **49,672,192** | 49,315,840 |
| native lane | 5,695,678 | **3,957,829** | 3,958,057 |
| native FULL / PREFIX records | 1,098 / 0 | **448 / 650** | 448 / 650 |
| @delta.trials@ | 37,886 | **38,538** | — |

**The saving is 1,737,849 B** — Squad V2's measured anchor (1,737,621 B) plus 228 B of group framing.
The selection is **byte-identical to v0.1.6's**: the same 1,098 records, the same raw bytes in each
class, and the same stored bytes in each class. The residual is now **356,352 B = 1.00723x**.

Squad V2's **Bound B (1,893,759 B, est) is falsified**: core's FULL frame ratio does not transplant from
the all-FULL lane to the post-selection FULL set — the observed ratio there is v0.1.6's own 3.62086x, to
the byte. The 3.87535x was a selection effect, not an encoder advantage.

The **cross-role half stays deferred** and still needs an amendment; a whole-file base is declined before
any chunk is emitted, so @small → large@ still stores every chunk FULL.

## Rulings still required — not taken

| item | bytes | why it is not proceeding |
| --- | --: | --- |
| **L5 — @objects@ row grammar** | 1,236,992 | **format change** — @SCHEMA_VERSION@ bump, old Stores rejected, and a read cost (one page read per chain edge) |
| **Codec level 3 -> 19** | ~4,270,443 | **CPU cost unmeasured**, and the lane already records @budget.complete-command@ **40.0 s against a 15 s ceiling** |
| **L7 — the depth regression** | 2,055,169 | **no identified mechanism**; the harness-side attempt made it 4.5 MB worse |

## Not claimed

- **No stride3 confirmation exists.** Nothing here is a gate claim.
- The verify phase is a **declared sample** (64 paths per state, 1,083 of 86,064 units) and reports
  @INCOMPLETE@, not PASS.
- The default arm's estimate was 8,625,119 B; the measurement is 6,402,048 B. The **2,223,071 B**
  difference is 489 objects that lose a base to an ineligible deep chain node — recorded, not smoothed.
