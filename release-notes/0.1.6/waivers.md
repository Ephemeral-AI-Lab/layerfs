# v0.1.6 waivers, declared exceptions and accepted costs

> **Status:** LayerFS 0.1.6 release record. Referenced from
> [acceptance](acceptance.md). Every entry disposes of an existing measured result;
> none is a re-measurement, and none converts a miss into a pass.

## Owner-waived targets

| target | measured | waiver |
| --- | --- | --- |
| cold `init_namespace/namespace-100000` Init | 4.986 s (v0.1.5 control 4.398 s, 1.13×) | owner-waived for v0.1.6; recorded as `TARGET_MISS`, not a pass ([#152 report](../../docs/roadmap/0.1/0.1.6/evidence/issue152-final-report.md) §2) |

## Declared exceptions (reported with their measured walls)

| case | mode | declared ceiling | measured complete command |
| --- | --- | ---: | ---: |
| `v016-branch-mixed-500mb-30000-k100-v1` | verification | **30 s** (owner ruling, ledger L12) | 23.10 s (24.17 s standalone) |
| `v016-mixed-development-500mb-30000-k100-v1` | performance / verification | 25 s | 18.31 s / 22.12 s |
| `v016-workspace-mixed-500mb-30000-k100-v1` | performance / verification | 25 s | 15.76 s / 20.36 s |
| `v016-branch-mixed-500mb-30000-k100-v1` | performance | 25 s | 16.49 s |

The 15 s family target is unchanged and reported separately for every row; a row
above it is a `TARGET_MISS`, and the declared exception band never redefines the
target.

## Accepted costs and declared measurement limits

1. **One construction worker.** Six material regressions (dedup/CDC construction,
   1.50–1.65×) accepted as recorded; the direct diagnostic is 2 290.09 ms
   one-worker vs 1 458.89 ms with the variable unset, against a 1 426.04 ms
   comparator.
2. **Sandbox process memory is not measurable** on the frozen harness: only
   container-scoped numbers (quota, current, lifetime peak) are comparable, and a
   lifetime cgroup peak is not a phase number.
3. **The host-side FUSE write-spool metric is dead on the sandbox route** and is no
   longer a gate; re-wiring it needs a new cross-boundary counter.
4. **Timing is cache-stance and host-load sensitive** beyond the declared
   allowance; every row records its host load and no contended sample is pooled
   with a quiet one.
5. **The inherited `historical_access` artifact** (11 performance cases + 11
   proofs) stays `NOT_RUN` for its missing sealed v2 Store.
6. **Kernel-dirty shared `mmap` is not captured by a Commit** — the
   specification's own open obligation, isolated to exact byte level, with no
   registered selection affected.
