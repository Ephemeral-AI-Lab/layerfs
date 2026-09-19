# Can storage be lowered further? Yes — by roughly another 13-17 MB

**No, the gate is not the floor.** 49,324,032 B is where the *first wave* landed, not a limit.

## D1 · Where we are, and what is still on the table

@```text
  128,864,256   the registered lane                       2.6130x
   49,324,032   TODAY                                    1.00017x   <- the gate is 8,192 B away
   49,315,840   v0.1.6                                   1.00000x

   ---- levers still on the table, all measured unless marked ----
      495,616   VACUUM at close                           measured net
    4,270,443   codec level 3 -> 19                       est; W1 is measuring it now
    8,927,432   T2 grouping at 256 KiB                    measured at L19, COMBINED with the level change
    2,547,712   objects_locations index                   measured; needs the cleanup rework
      476,317   #185 cross-role                           ceiling, mostly modelled
      739,246   the whole-file regression                 UNEXPLAINED

   ---- the floors, derived independently from the corpus (Squad B1) ----
   45,880,389   P2, per-object delta                      product-expressible today
   31,867,138   P1, 254 group frames                      <= 11.26 MB decoded per read
   22,149,444   L, one stream                             a read decodes 372 MB -- NOT reachable
@```

## D2 · The two sums

@```
  OPTIMISTIC (every lever additive)      49,324,032 - 17,456,766 = 31,867,266 = 0.6462x
  CONSERVATIVE (level subsumed by the
                grouping figure, which
                was measured at L19)     49,324,032 - 13,186,323 = 36,137,709 = 0.7328x
@```

**So the realistic range is 0.65x - 0.73x of v0.1.6** — a further **26-36 % reduction** from today.

## D3 · The cross-check, and it is a good one

@```
  the optimistic lever sum, built lever by lever on THIS Store   31,867,266 B
  Squad B1's P1, built from the CORPUS compression floor         31,867,138 B
  --------------------------------------------------------------------------
  difference                                                            128 B
@```

**Two entirely independent derivations — one summing measured levers on this Store, one measuring what
the corpus can compress to — land 128 B apart.** That is strong evidence both are right, and it is the
first time in this campaign that a top-down floor and a bottom-up lever sum have met.

## D4 · What the further lowering costs

**It is bought with access, not given.** The mandate for this round is *bounded memory and CPU*, and:

- **P1's arrangement is 254 group frames** with a mean of 1,464,553 B and a maximum of **11,263,931 B
  decoded per read**. That is the price of the 0.6462x.
- **T2 grouping costs 78.7x single-record read amplification** at 256 KiB, and is 2.95 % *worse* on the
  records that already carry a dictionary — so it must be applied selectively.
- **The floor (0.4491x) is not a design.** One stream; a read decodes the whole 372 MB.

## D5 · One line

@```
  storage can be roughly halved again from here:  1.00017x -> 0.65x-0.73x
  the gate was never the floor;  it was just the first waypoint
  and the remaining gains are paid for in BYTES PER READ, which is the axis
  this round's mandate says to bound
@```
