# The plan, and what "45 s allowed" actually buys

## 1. Where wave 1 landed

@```
  the registered lane      128,864,256    2.61304x
  W1 + W2 + W3             45,432,832    0.92126x   <- gate cleared by 3,883,008 B
                                                       T1 target cleared by 1,615,603 B
  v0.1.6 (the gate)         49,315,840    1.00000x
  the T1 target             47,048,435    0.95402x
@```

**Attribution, stated honestly: that is the TREE's number, not any one squad's.** W1 measured its own
contribution from the parent's baseline as **-3,876,934 B -> 52,172,730 = 1.05795x, still +2,856,890
above the gate. The codec alone does NOT close it.** W2's derived split puts 5,390,336 B on the index
(84.5 % of 6,377,472). W3's isolated change is 1,335,296 B. **The three together clear it; no one of them
does.**

## 2. "45 s allowed" — the honest reading

@```
  the contract ceiling                        15 s   (exception list up to 25 s)
  the lane's complete command NOW         44.53 s   <- includes W1's level 9/19 already
  headroom under a 45 s ceiling             0.47 s
  the registered lane (untouched)         50.42 s   <- ALREADY over 45 s
  the codec's own CPU cost                +7.589 s
  T2 at payload level 19                 +66.7 s    <- an OWNER RULING, not a parameter
@```

**Two things follow, and they point the same way:**

1. **The budget was never achievable for this lane.** The registered lane — no index, no codec change,
   declaring nothing — measured **50.42 s**. So a 45 s ceiling fails the *untouched* lane. The budget is
   a contract that this workload never met, and the current lane at 44.53 s is **faster than the
   registered one** while storing 64.7 % less.
2. **45 s buys almost nothing.** At 44.53 s there is **0.47 s** of headroom, and the codec level is
   already spent. **T2 at payload level 19 would need +66.7 s — that is a ruling, not a knob.**

**And the codec level is already chosen at the knee, measured:** marginal B per CPU-second is
**L3->L5 1,408,000 · L5->L9 227,500 · L9->L12 29,400 · L15->L19 3,900 · L19->L22 444.** Level 9 sits just
past the knee; everything above it is nearly free of bytes and expensive in time.

## 3. What wave 1 still owes — do these before calling it done

| # | item | why |
| --: | --- | --- |
| 1 | **Re-run the 217-row lane** | **the codec level is a PRODUCT change: it alters the stored bytes of every registered case that writes whole-file or chunk payloads.** The golden table and @--lane full@/@--lane smoke@ must be re-run. **Not run — flagged as a GAP by W1.** |
| 2 | **@core/docs/architecture/13-physical-writing.md@** lines 28, 73, 77, 87 (level 3 / level 1 / 2 MiB) and @c2-families.md@ line 28 | @core/AGENTS.md@ requires the architecture doc in the same commit. **Owed, not edited.** |
| 3 | **clippy @-D warnings@ and @fmt --check@** | W1 skipped both to avoid a third build at load 5-8. A GAP. |
| 4 | **@CompressionWorkspace::new(capacities)@ at @cas/lifecycle.rs:56@** | W1's request, in a file it does not own. @capacities@ is already in scope; sizing from it charges **8 MiB at the default cutoff and 16 MiB only at the widest — removing 12 MiB of provision for a policy nobody configures.** Byte result identical. |
| 5 | **stride3** | **the one unmet gate requirement.** Everything above is diagnostic until it is run. |

## 4. Then wave 2, with one claim to re-measure

**T2's "closes the gate only at level 19" came from a MODELLED level curve, and W1 has now replaced that
curve with measurement. T2 must re-measure at level 9 before anyone carries the claim.**

And the two remaining wave-2 items are unchanged: **pack grouping** and **cross-role transitions (#185,
<= 476,317 B, 0 B of the gap)**.

## 5. Then wave 3 — the Pareto frontier

The mandate is *bounded memory and CPU*, and wave 1 produced the first real trade point:

@```
  PRODUCT index   45,432,832 B    37.86 s operation    247,857,152 B peak RSS
  HARNESS index   44,175,360 B    40.35 s operation    255,049,728 B peak RSS
                  -1,257,472 B    +2.49 s              +7,192,576 B
@```

**1.26 MB of storage bought back as 2.5 s of CPU and 7.2 MB of memory** — that is a point on the curve,
and wave 3's job is to draw the curve rather than pick the point.

## 6. One more thing wave 1 found, which improves BOTH axes

**The codec level lowers bytes per read as well as bytes stored:** whole-file FULL 2,760.7 -> 2,629.8
(-4.74 %), whole-file PREFIX 494.4 -> 428.6 (-13.31 %), native FULL 5,304.2 -> 4,992.5, native PREFIX
2,384.7 -> 2,167.1 — with decoded bytes/read, records/read, chain depth and group count **all unchanged**.
**Not every lever trades one axis for another**, and the frontier is not monotone.

## 7. What is still NOT claimed

**No stride3 confirmation exists.** The verify phase is a declared sample reporting @INCOMPLETE@. And the
CPU numbers for the *lane* were **not taken** — W1 ran the mandated quiet check and it was **NOT empty**
(a sibling lane ran concurrently; the same arm measured 64.99 s and 46.46 s on two runs, a **40 % spread**),
so W1 reported **no wall time and no lane process CPU** and said so rather than quoting a confounded number.
The +7.589 s is the codec's own instrumented CPU, which is load-robust; the **lane** CPU is still unknown.
