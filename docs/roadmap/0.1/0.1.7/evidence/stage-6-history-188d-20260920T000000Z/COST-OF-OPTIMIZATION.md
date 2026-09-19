# Will the optimisation cost more CPU and more read cost?

**It splits cleanly in two.** The *representation* changes are free or negative. The *coverage* and
*compression* changes cost. And the read cost is **intrinsic to delta encoding**, not incidental.

## D1 · The two classes

@```text
  CLASS 1 -- representation.  FREE or NEGATIVE.
    R2 / L5 row grammar      removes two indexes      -> FEWER write ops per object
    the smaller index        T1's 4 MiB -> W2's 672 KiB -> -3.3 MiB resident, and a smaller
                                                         structure scans faster
    the L4 chunk cursor      MEASURED -0.27 s (free)  -> 686.1 B per cursor, 2 reads,
                                                         and it NEVER reads a payload byte
    VACUUM at close          costs close-time I/O, then the file is smaller -> cheaper
                             subsequent reads (fewer pages)

  CLASS 2 -- coverage and compression.  THEY COST.
    the similarity index     MEASURED +10.0 % CPU (user+sys), +15.6 MiB peak RSS
    codec level 3 -> 19      UNMEASURED, but a higher zstd level is strictly more work per byte
    T2 grouping              MEASURED 78.7x single-record read amplification at 256 KiB
    T3 per-path chains       up to 11,263,931 B decoded per read
@```

## D2 · What is measured, exactly

| arm | wall | user+sys | peak RSS | apparent |
| --- | --: | --: | --: | --: |
| declared only | 46.63 s | 44.06 s | 231.2 MiB | 56,049,664 |
| + the similarity capability | 49.19 s | 48.46 s | 246.8 MiB | 49,672,192 |
| **delta** | **+2.56 s** | **+4.40 s (+10.0 %)** | **+15.6 MiB (+6.7 %)** | **-6,377,472** |

**Caveat: the machine was NOT quiet for these** (load 8.42 before, 6.39 after, one cargo process). Wall and
CPU are **contaminated**; **peak RSS is load-independent and valid.**

## D3 · The read cost is intrinsic, not incidental

**Delta encoding always costs read amplification**: to reconstruct one delta object you must walk its base
chain, so an object at depth @d@ costs @d + 1@ record reads. That is the mechanism, and it means:

- **more delta coverage -> more objects need chain reconstruction on read.** Trials went from **18,783**
  (registered lane) to **38,538** — roughly **2x the delta objects**.
- **deeper chains -> more reads per object.** The proxy: @delta.ineligible_candidates@ rose **572 -> 2,585**
  as the similarity source added cross-path bases, and every ineligible probe walks the candidate's depth
  chain before refusing it.

**So yes — the storage win is paid for in read cost.** That is what makes the storage-vs-access frontier
real rather than rhetorical.

## D4 · The one lever whose CPU is the whole question

**The codec level 3 -> 19.** It is worth ~4,270,443 B and it changes **two constants**, but a higher zstd
level is strictly more work per byte compressed. **That cost is unmeasured, and W1 is measuring it now.**
It is the only lever where the CPU number decides whether the lever is worth having — and the lane already
records @budget.complete-command@ **40.0 s against a 15 s ceiling**, so it is spending something already
overdrawn.

## D5 · One line

@```
  the cheap half:   row grammar, a smaller index, the chunk cursor   -> free or negative
  the costly half:  the similarity index (+10 % CPU), the codec level (?), grouping (78.7x reads)
  the invariant:    every byte of delta coverage is paid for in chain reads

  so "cheaper storage" and "cheaper reads" are not the same optimisation,
  and this round's mandate asks for both to be BOUNDED, not merely measured.
@```
