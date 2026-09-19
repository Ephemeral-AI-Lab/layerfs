# The gate AND the T1 target are cleared — from the product side

**Measured.** One arm, quiet machine (W3 held the lock and confirmed `ps` empty before and after),
`LAYERFS_CONSTRUCTION_WORKERS=1`. **Verified independently by the parent** with @shared/space.py@.

@```
  artifact   …/188d/LANE/w2f-product/sample.sqlite
  apparent   45,432,832 B      = 0.92126x v0.1.6
  sha256     729f9ecd7e5dcbbcfedcbcf144a8af73…
  quick_check ok
  pack blob  41,428,320 B      non-pack  4,004,512 B
```

## The result

| | apparent | vs v0.1.6 | vs the gate | vs the T1 target |
| --- | --: | --: | --: | --: |
| the registered lane | 128,864,256 | 2.61304x | -79,548,416 | -81,815,821 |
| today's default | 56,049,664 | 1.13654x | -6,733,824 | -9,001,229 |
| W2 alone (v1) | 49,324,032 | 1.00017x | -8,192 | -2,275,597 |
| **W2 + W3 — the product** | **45,432,832** | **0.92126x** | **+3,883,008** | **+1,615,603** |
| the harness arm | 44,175,360 | 0.89576x | +5,140,480 | +2,873,075 |
| **v0.1.6 (the gate)** | 49,315,840 | 1.00000x | — | -2,267,405 |
| **the T1 target** | 47,048,435 | 0.95402x | +2,267,405 | — |

@```
  the gate is cleared by   3,883,008 B   (7.87 %)
  the target is cleared by 1,615,603 B   (3.43 %)
  from the registered lane: 128,864,256 -> 45,432,832
                            -83,431,424 B = a 64.7 % reduction, 2.6130x -> 0.92126x
@```

**Both exit criteria are met, and the result is the product's** — no harness switch supplies the index.

## We now win three of the four lanes

| lane | ours | v0.1.6 | delta |
| --- | --: | --: | --: |
| **whole-file** | 34,761,166 | 38,983,278 | **-4,222,112** |
| **native (chunk)** | 3,676,262 | 3,958,057 | **-281,795** |
| **pooled-metadata** | 1,904,445 | 2,240,958 | **-336,513** |
| ordinary | 875,547 | 678,339 | +197,208 |

## The counters

@```
  delta.prefix_selected   38,175      (v0.1.6: 35,904)
  delta.full_records      12,940      (v0.1.6:  8,237)
  delta.no_candidate       6,988      (the registered lane: 26,847)
  delta.trials            38,230
  delta.ineligible_candidates 683
  delta.work_exceeded         21
  delta.reused             1,121
```

## What is still on the table

**The harness arm is a further 1,257,472 B better** (44,175,360 against 45,432,832) — that is the
**modelling-versus-product** gap, and it is the clearest remaining target. The harness's similarity index
and the product's persisted one differ in size and in admission, and the product's is currently the
smaller.

**And the codec level is not in these numbers.** W1 is still measuring it; a prior estimate puts it at
~4,270,443 B. If it lands, the lane goes well below 0.90x.

## Not claimed

- **No stride3 confirmation exists.** The guardrail requires one and none has been run, so this is
  **still not a gate claim** in the contract's sense.
- The verify phase is a **declared sample** (1,083 of 101,477 path-states) and reports @INCOMPLETE@.
- **CPU attribution between W2 and W3 is NOT separated.** W3 measured +5.32 s real / +6.13 s user on a
  pair of binaries that differ by both squads' work, and said so rather than inventing a number.
  **Peak RSS fell** 243,646,464 -> 239,714,304 B (-3.75 MiB).
- The whole-file regression W2 flagged (+739,246 against the harness arm at the v1 stage) is **not yet
  attributed**.
