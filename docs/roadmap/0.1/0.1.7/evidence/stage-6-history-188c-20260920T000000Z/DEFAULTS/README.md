# The history lane's defaults, corrected

**Harness-only. Production LOC delta 0.** Source pin @66bce8378@ + the working tree. No timing.

## What changed, and why

The lane's historical default **declared no delta base at all**, and that default is precisely what made
its headline number — 128,864,256 B apparent, the "2.65x" — **not a product measurement**. The Store was
supplied no base for 26,847 objects. Squad V1 and V4 then showed the remaining gap was not a missing
layer but v0.1.6's **declaration rule**: *use the declared predecessor when there is one; consult the
similarity cache only when @anchor.is_none()@*.

Both are now the lane's **default**. Every historical arm stays reproducible by environment.

| arm | how to reproduce | apparent |
| --- | --- | --: |
| **faithful model + v0.1.6's rule** | *(nothing set — the new default)* | **51,347,456** |
| same-path previous version only | @LAYERFS_HISTORY_SAME_PATH_ONLY=1@ | 57,749,504 |
| the historical registered lane | @LAYERFS_HISTORY_ADVISORY=0@ | 124,735,488 |
| the four-slot ordered list (negative) | @LAYERFS_HISTORY_ORDERED_PREDECESSORS=1@ | 68,157,440 |
| the every-commit producer (negative) | @LAYERFS_HISTORY_FULL_PRODUCER=1@ | 59,039,744 |

**The @ADVISORY=0@ arm is 124,735,488, not the historical 128,864,256**, and the difference is
**4,128,768 B** — exactly the two indexes the T1 squad's R2 removed (@objects_bases@ 1,576,960 +
@objects_locations@ 2,547,712 = 4,124,672, plus page rounding). That is expected, and it is the one
number in this table that is not a like-for-like comparison with the campaign's earlier figures.

## The default arm, measured

@```
  apparent                 51,347,456     = 1.0412x v0.1.6's 49,315,840
  whole-file lane          37,347,557     <- v0.1.6's own is 38,983,278; WE WIN by 1,635,721
  native                    5,695,678
  pooled-metadata           2,019,419
  ordinary                  1,022,795
```

**We now beat v0.1.6 on the whole-file lane** — the lane that was 58.0 % of the gap. The residual
inverts: **88.2 % of what remains is the non-pack row grammar and 85.5 % is the chunk lane.**

## What is NOT claimed

- **No stride3 confirmation exists.** Nothing here is a gate claim.
- The verify phase is a **declared sample** (64 paths per state, 1,083 of 86,064 units) and reports
  @INCOMPLETE@, not PASS.
- The estimate for this arm was 8,625,119 B and the measurement is 6,402,048 B. The 2,223,071 B
  difference is **489 objects that lose a base** because it became an ineligible deep chain node. It is
  recorded, not smoothed over.
- The product changes under @core/crates/@ in this tree are the **T1 squad's** (R1a, R2, R3), not this
  round's. This round changed only @ops/history.rs@.
