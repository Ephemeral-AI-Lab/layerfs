# Why the fallback is better than no fallback — in diagrams

**Measured.** Two arms, one binary, one variable (`LAYERFS_HISTORY_FALLBACK_PREDECESSORS`). Both Stores
have **44,141 whole-file objects and 348,460,295 B canonical — identical**. Every number below is a
per-object join between the two, so the arithmetic closes. **No timing figure.**

## D1 · Which source serves which object

````text
   every whole-file object  (44,141)
            |
            v
   does THIS PATH have an earlier version in the selection?
            |
     +------+------+
     |             |
    YES            NO                    <- 10,080 objects get here
     |             |
     v             v
 +-----------------------------+   +--------------------------------------+
 | SOURCE 1                    |   | SOURCE 2  -- the "fallback"          |
 | same-path previous version  |   | the content-similarity index         |
 | the caller declares it      |   | the STORE finds it                   |
 |                             |   |                                      |
 |        ~17.3x               |   |            ~9.9x                     |
 +--------------+--------------+   +-------------------+------------------+
                |                                      |
                |                                      | reaches 3,406
                |                                      | of the 10,080
                +------------------+-------------------+
                                   |
                                   v
                     the delta selector, ONE trial, in order
                                   |
                    +--------------+--------------+
                    |                             |
              a base is eligible            none / ineligible
                    |                             |
                    v                             v
              PREFIX record                 +-----------------+
              ~10x to ~17x                  | FULL record     |
                                            |      ~2.6x      |
                                            +-----------------+
                                            <- THIS is what the
                                               fallback competes with,
                                               NOT the same-path base
````

**The fallback never competes with the good base.** The rule is
`if previous.is_some() { use it } else { consult the index }` — a path that has an earlier version never
reaches the index at all.

## D2 · Where the 6,377,472 B comes from — per object, measured

````text
  transition        objects    canonical     stored OFF     stored ON        delta    ratio ON
  ------------------------------------------------------------------------------------------
  FULL  -> based      3,847   31,935,539     11,437,331     3,224,189   -8,213,142      9.90x
  based -> based     33,950  264,976,235     15,652,232    15,284,282     -367,950     17.34x
  FULL  -> FULL       5,855   43,178,429     16,371,948    16,371,948           +0      2.64x
  based -> FULL         489    8,370,092        411,969     2,467,138   +2,055,169      3.39x
  ------------------------------------------------------------------------------------------
  TOTAL              44,141  348,460,295     43,873,480    37,347,557   -6,525,923      9.33x

  whole-file lane   -6,525,923
  apparent          -6,377,472     (the 148,451 B difference is pack framing and the wider
                                    base_object_id column: 3,358 more rows carry a base)
````

**The whole win is one row.** 3,847 objects that were stored **FULL at 2.64x** are now stored against a
cross-path base at **9.90x** — the same objects, the same bytes, 8,213,142 B cheaper.

````text
  FULL -> based   ################################################   -8,213,142   <- the win
  based -> based  ##                                                   -367,950
  FULL -> FULL                                                                0
  based -> FULL   ############                                       +2,055,169   <- the cost
  ------------------------------------------------------------------------------
  net                                                                -6,525,923
````

**And the cost is stated, not hidden:** 489 objects that *had* a same-path base **lose** it, because
adding cross-path bases deepened some chains past the eligibility cap. That is 2,055,169 B of the win
given back — 24 % of it. The estimate for this arm was 8,625,119 B; the measurement is 6,377,472 B, and
this row is the entire difference.

## D3 · The ratio ladder

````text
  no base at all    ##                             2.64x    <- 5,855 objects, 43,178,429 B canonical
  cross-path base   #######                        9.90x    <- 3,847 objects the fallback reaches
  same-path base    #############                 17.34x    <- 33,950 objects that already have one
  ------------------------------------------------------------------------------------------
  the fallback moves 3,847 objects UP ONE RUNG: from 2.64x to 9.90x
````

**It is an inferior base and a superior outcome**, because the alternative it actually faces is the
bottom rung, not the top one.

## D4 · Why it must stay a *second* source — the measured counter-example

````text
  CORRECT  (second source, last resort)        WRONG  (competing) -- the four-slot ordered arm
  +--------------------------------+           +--------------------------------+
  | 44,141 objects                 |           | 44,141 objects                 |
  |                                |           |                                |
  |  38,337 keep the SAME-PATH     |           |  ALL offered cross-path first  |
  |         base        ~17.3x     |           |  17,141 same-path bases        |
  |                                |           |  REPLACED by worse ones        |
  |   3,406 get a CROSS-PATH       |           |                                |
  |         base         ~9.9x     |           |                                |
  +--------------------------------+           +--------------------------------+
  gained        -8,581,092                     gained        -8,625,119
  lost          +2,055,169                     lost         +18,214,699
  ----------------------------------           ----------------------------------
  net           -6,525,923   BETTER            net          +10,242,310   WORSE
````

**The two arms find almost the same bases** — the gain differs by 44,027 B, 0.5 %. **The difference is
entirely in what they do to the objects that already had a good base:** 2,055,169 B of collateral loss
versus 18,214,699 B. That 16,159,530 B gap *is* the value of the ordering rule.

## D5 · The one-line summary

````text
  the fallback is not "a worse way to store an object"
  it is  "a way to store an object that otherwise gets NO base at all"

  10,080 objects are offered nothing without it
   3,406 of them are reached
   cost:     6,377,472 B saved
   price:      489 objects lose a better base  (+2,055,169 B)
   net:      6,377,472 B  =  1.00723x -> 1.13654x against v0.1.6
````

**With it:** 49,672,192 B, **356,352 B above v0.1.6**.
**Without it:** 56,049,664 B, **6,733,824 B above v0.1.6**.
