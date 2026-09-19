# Milestone: the lane is at 1.00017x v0.1.6 — **8,192 B above the gate**

**Measured.** W2's product-side lane run, verified independently by the parent with @shared/space.py@.
One sample, `LAYERFS_CONSTRUCTION_WORKERS=1`. Artifact:
@…/188d/LANE/w2-product/sample.sqlite@, sha256 @4af37932aa3391b12269de81…@, @quick_check = ok@.

## The number

| arm | apparent | pack blob | non-pack | whole-file lane | vs v0.1.6 |
| --- | --: | --: | --: | --: | --: |
| declared only (baseline) | 56,049,664 | 51,084,839 | 4,964,825 | 43,873,480 | 1.13654x |
| similarity (harness arm) | 49,672,192 | 44,558,708 | 5,113,484 | 37,347,557 | 1.00723x |
| **W2+W3, product-side** | **49,324,032** | 45,297,954 | 4,026,078 | 38,086,787 | **1.00017x** |
| **v0.1.6 (the gate)** | **49,315,840** | | | | 1.00000x |

@```
  vs gate     -8,192 B      (1.00017x)
  vs target   -2,275,597 B
@```

**From the registered lane: 128,864,256 -> 49,324,032 = -79,540,224 B, a 61.7 % reduction, and
2.6130x -> 1.00017x.**

## Where the gain came from — and it is NOT where the harness arm's was

@```
  W2+W3 product  vs  the harness arm
    pack blob    45,297,954 - 44,558,708 =   +739,246   WORSE
    non-pack      4,026,078 -  5,113,484 = -1,087,406   BETTER
    apparent     49,324,032 - 49,672,192 =   -348,160
    check: (+739,246) + (-1,087,406) = -348,160   residual 0
@```

**The product's index produces a WORSE whole-file lane than the harness arm** (38,086,787 against
37,347,557, +739,246) **and the row grammar more than pays for it** (-1,087,406). Two mechanisms, opposite
signs, and the net is what clears.

**The whole-file regression is not yet explained and is the thing to chase next** — the harness's index and
the product's differ in size (8,192 slots against the harness's own structure) and in the admission rule.
W2 is asked to attribute it.

## What this means

- **The gate is 8,192 B away — 0.017 %.** Every remaining lever is far larger than that:
  VACUUM (495,616 B net), the codec level (~4,270,443 B est), grouping (~8,927,432 B at L19), L5's
  remaining index work.
- **The product now does this itself.** No harness switch supplies the index; the result is a product
  result.
- **The rule difference paid off as predicted**: the product consults the index when nothing was
  *acquired*, the harness arm only when nothing was *declared*, and the product lands lower overall.

## Not claimed

**No stride3 confirmation exists.** The verify phase is a declared sample and reports @INCOMPLETE@.
**No timing or memory figure for this arm yet** — W2's CPU/RSS numbers are still pending, and the machine
was not fully quiet during this window. **The attribution between W2's index and W3's row grammar is not
separated** — the tree carries both, so 49,324,032 is their combined result, not either alone.
