# The CPU and memory baseline the squads are measured against

**Diagnostic.** Taken with `/usr/bin/time -l`, one sample per arm, `LAYERFS_CONSTRUCTION_WORKERS=1`.

## The numbers

| arm | wall | user CPU | sys CPU | **user+sys** | **peak RSS** | apparent |
| --- | --: | --: | --: | --: | --: | --: |
| **declared only** (today's default) | 46.63 s | 32.26 s | 11.80 s | **44.06 s** | **231.2 MiB** | 56,049,664 |
| **+ the similarity capability** | 49.19 s | 35.94 s | 12.52 s | **48.46 s** | **246.8 MiB** | 49,672,192 |
| delta | +2.56 s | +3.68 s | +0.72 s | **+4.40 s (+10.0 %)** | **+15.6 MiB (+6.7 %)** | **-6,377,472** |

## Cache state — declared, and a caveat

**The machine was NOT fully quiet.** `uptime` reported load average **8.42** immediately before and
**6.39** after, and one `cargo`/`rustc` process was running — wave 1's squads were building.

- **Wall time and CPU are therefore CONTAMINATED** and must not be quoted as clean. They are useful
  directionally.
- **Peak RSS is load-independent and is valid.** 231.2 MiB -> 246.8 MiB.
- Cache state: the Store is `CreatedInSample`; the **corpus is partially resident** from earlier runs in
  this session. Every arm reads the same corpus, so the comparison is fair; the absolute numbers are
  warm-corpus.

## Why these are the bounds that matter

The owner's mandate for this round is *"arithmetically and algorithmically optimised, at speed, with
**bounded memory and bounded CPU**"*. So every wave-1 change is measured against:

@```
  CPU baseline       44.06 s user+sys   (declared only)
  memory baseline   231.2 MiB peak RSS
  wall baseline      46.63 s           (contaminated)
  access baseline   stated in BYTES PER READ, never seconds
```

**A change that saves bytes by spending CPU or memory without bound is not an optimisation under this
mandate.** Two known levers already look expensive on this axis and must justify themselves:

- **pack grouping (T2)** — 78.7x single-record read amplification at 256 KiB.
- **per-path chains (T3)** — up to 11,263,931 B decoded per read, and a path concept the Store lacks.

## The lane's own budget, for context

@budget.complete-command@ is **<= 15 s**; the registered lane alone measured **50.42 s**. **The lane was
already 3.4x over its budget before any of this work.** No storage lever fixes that, and any CPU-costing
lever makes it worse — so a CPU cost must be argued explicitly, not absorbed.

## Not claimed

One sample per arm, no repeat, no best-of. Contaminated wall time. Warm corpus. **No stride3 confirmation
exists**, so none of this is a gate claim.
