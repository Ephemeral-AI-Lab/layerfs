# Speed of operation — one declared measurement

**Diagnostic.** Taken on a **quiet machine**: load average 4.39 and falling, **0 competing
@cargo@/@rustc@/@fs-bench@ processes**, all analysis agents finished. **One sample per arm, no best-of.**

**Declared cache state — this is NOT a cold claim:**

- @store_state = CreatedInSample@ for every arm (each arm creates its own Store).
- @cache_state@: the **corpus is partially resident** from earlier runs in this session. Every arm reads
  the same corpus, so the *comparison* is fair, but **the absolute numbers are warm-corpus and must not
  be quoted as cold**.
- @construction_workers = 1@ (@LAYERFS_CONSTRUCTION_WORKERS=1@) on every arm.
- The corpus reading is **harness work**, and it sits inside the product's root and outside every child.

## The measurement

| arm | apparent | root (s) | **children (s)** | corpus read (s) | complete cmd (s) | @trials@ |
| --- | --: | --: | --: | --: | --: | --: |
| registered (declares nothing) | 124,735,488 | 50.22 | **33.47** | 16.75 | 50.42 | 18,783 |
| same-path + chunk cursor | 56,049,664 | 50.21 | **34.82** | 15.39 | 50.42 | 35,140 |
| fallback, no chunk cursor | 51,347,456 | 53.28 | **37.98** | 15.29 | 53.49 | 37,886 |
| **DEFAULT (fallback + cursor)** | **49,672,192** | 53.17 | **37.71** | 15.45 | **53.38** | 38,538 |

@root@ is the product's own root and **includes** the harness's untimed corpus reading; @children@ is the
sum of the 17 named per-state children — **the product's measured operation**, which is what this lane
publishes as @operation_ns@.

## The trade

@```
  storage    124,735,488 -> 49,672,192 B   -60.2 %
  product        33.47 s ->     37.71 s    +12.7 %   (+4.24 s)
  root           50.22 s ->     53.17 s     +5.9 %
  complete cmd   50.42 s ->     53.38 s     +5.9 %
@```

**Roughly 60 % of the storage for about 13 % more product time.** The corpus reading — 15.3-16.8 s,
harness work — is unchanged and dilutes the ratio at the root.

## Where the time goes, arm by arm

| step | storage bought | time cost |
| --- | --: | --: |
| the same-path declaration | -68,685,824 | **+1.35 s** |
| the **fallback** rule (cross-path when there is no predecessor) | -4,702,208 | **+3.16 s** |
| **the L4 chunk cursor** | **-1,675,264** | **-0.27 s — no measurable cost** |

**The chunk cursor is time-free**, which is exactly what its design predicts: it reads **686.1 B per
cursor** (106.0 B for a FileState base root + 580.1 B for an extent-leaf mapping page, 2 reads, pinned by
an external test) and **never reads a payload byte**. It adds 652 trials and removes 1.68 MB.

**The fallback is where the time goes.** It adds 2,746 trials but 3.16 s — far more per trial than the
same-path arm's ~83 µs — because a cross-path base is *further away and less local*, and because
@ineligible_candidates@ rises 572 -> 2,585, and every ineligible probe walks the candidate's depth chain
(a Store read per level) before refusing it. **That is the measured cost of the deeper chains.**

## The budget — this is a FAIL, and it was already one

The lane's @budget.complete-command@ is **<= 15 s**. Measured here: **53.38 s**. The T1 squad recorded
**40.0 s** and reported it @NOT_RUN@ rather than shrinking the workload to fit. **The lane does not fit
its budget in any arm, including the registered one at 50.42 s** — the corpus reading alone is 15-17 s.

**So the storage work did not create the budget failure, and it does not fix it.**

## What is not claimed

- **Warm corpus, declared.** Not a cold measurement; @shared/cold.py@'s invalidation-plus-residency
  contract was **not** applied — it covers the families it names, and this lane is not one of them.
- **One sample per arm**, no repeat, no best-of.
- **No stride3 confirmation exists**, so nothing here is a gate claim.
- The **codec level 3 -> 19** lever (~4.27 MB) is the one with a *known* CPU cost that remains
  **unmeasured** — it would move exactly this axis, and it is the reason that lever is held.
