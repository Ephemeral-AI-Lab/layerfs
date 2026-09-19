# All three tiers pass, and all three beat v0.1.6

**Diagnostic.** `PAYLOAD_LEVEL = 3`, `GROUP_LEVEL = 19`, `LAYERFS_CONSTRUCTION_WORKERS=1`, one sample per
tier, sequential under `/tmp/lane.lock`. Every Store re-verified with `space.py`, `quick_check = ok`.

## The three tiers

| tier | states | ours | v0.1.6 | vs v0.1.6 | delta |
| --- | --: | --: | --: | --: | --: |
| **stride10** | 17 | **49,053,696** | 49,315,840 | **0.99468x** | **+262,144** |
| **stride3** | 53 | **61,767,680** | 64,000,000 | **0.96512x** | **+2,232,320** |
| **stride1** | 157 | **80,273,408** | 82,685,952 | **0.97082x** | **+2,412,544** |

**All three PASS, and all three are BELOW v0.1.6.**

## The shape is identical on every tier — which is what makes it a confirmation

| lane | stride10 | stride3 | stride1 |
| --- | --: | --: | --: |
| whole-file | **-2,852,253** at stride3 · see below | | **-2,347,806** |
| native | **-122,144** | **-122,178** |
| pooled-metadata | **-629,789** | **-1,548,237** |
| ordinary | +414,110 | +542,610 |
| pack blob | **-3,147,356** | **-3,375,819** |
| non-pack | +915,036 | +963,275 |

**The same three lanes win and the same two lose, at every scale.** Stride1's reconciliation closes exactly:
pack **-3,375,819** + non-pack **+963,275** = **-2,412,544** = the apparent delta. **Residual 0.**

Stride1 whole-file is **56,391,902 against 58,739,708** — a **-2,347,806** win on the largest lane.

## Memory is bounded, and it is the strongest evidence for the mandate

| tier | declared content | states | **peak RSS** | wall | CPU |
| --- | --: | --: | --: | --: | --: |
| stride10 | 561,010,345 B | 17 | **262.2 MB** | 47.1 s | 39.6 s |
| stride3 | 1,676,767,835 B | 53 | **270.7 MB** | 97.9 s | 90.0 s |
| stride1 | 4,936,693,030 B | 157 | **304.5 MB** | 289.5 s | 269.3 s |

@```
  content grew   8.80x   (561 MB -> 4,937 MB)
  memory grew    1.16x   (262.2 MB -> 304.5 MB)
  -> SUB-LINEAR. The footprint is bounded by the lane's working set,
     not by the corpus, which is what "bounded memory" has to mean.
@```

## The guardrail was honoured

**`history-stride1` was run as a DIAGNOSTIC, not as an optimisation target.** The guardrail — *"never
optimise `history-stride1`; no constant, threshold, buffer, batch or policy value may be changed because
of a stride1 number"* — was respected: **no value was changed because of this run.** The configuration
under test (`PAYLOAD_LEVEL = 3`, `GROUP_LEVEL = 19`) was chosen by the **stride10 sweep**, before
stride1 ran, and is unchanged by its result.

**And the guardrail's anticipated failure did not materialise.** It says *"a stride1 failure while both
lower tiers pass is a scaling finding"* — **stride1 passed**, and the harness backing fix made earlier in
this round is very likely why: that bug's trigger (the pending row map outgrowing the in-memory ceiling
during an update walk) scales with the **base tree**, so it would have hit the largest tier hardest.

## Not claimed

- **The verify phase is still a declared sample reporting `INCOMPLETE`.** `PASS gates=2` is
  `g1.o1-chain-complete` plus `g4.swaps` — **not a read-back of the stored trees.** This is the one gap
  between these results and a contract-grade gate claim.
- **The machine was not fully quiet** (load 4.9-5.5, one `cargo` process throughout). Wall and CPU are
  indicative; **bytes and RSS are load-independent and stand.**
- One sample per tier, no best-of.
- The 217-row lane has **not** been re-run since the codec change, which alters every registered case's
  stored bytes.
