# Count-changing scale diagnostic — preregistration

- Status: **PASS / COMPLETE — CP-0008 diagnostic retained**
- Date: 2026-08-21
- Starting HEAD: `febc20f046bba84ccdce1256363d77799eabf2db`
- Starting count-change implementation diff:
  `88ffb0bd6a30ee9a6926ccec4916ed917278ee0e80f6ababeaff18001395a3e9`
- Retained parent: CP-0007
- Profile: unchanged K64/F64 + DIR256K

## Question

Measure—not infer—the current fixed-radix `+1` publication curve at exact
1, 10, 100, and 500 MiB. This is a post-promotion diagnostic. It cannot
change canonical bytes, reopen WP4-P, or promote a different structure.

## One changed variable

Add diagnostic-only CLI/runner admission for count-changing rows at the four
declared sizes. The mutation, proof, publication, verification, profile,
goldens, schema, and durability code remain unchanged.

## Schedule

```text
sizes:       1, 10, 100, 500 MiB exactly
operations:  +1 early, +1 middle
per arm:     1 warmup + 3 measured
capture:     32 rows
roundtrip:   2 nonmedian checks at 500 MiB, one per operation
total:       34 rows
build:       one release executable before timing
hard wall:   120 seconds after build
command cap: 60 seconds
```

Every arm uses one immutable prepared database/authority/expectation master.
Each row receives a fresh byte copy. Masters are rehashed after the final row;
all temporary fixtures and SQLite images are deleted.

## Required counters

For every measured row:

```text
status PASS; exact selected profile
one transaction / one COMMIT / one successful return
construction proof consumptions = 1
qualification authenticated objects/bytes = 0/0
source bytes read = 1
terminal Q = 0
suffix references = old references - insertion ordinal
prior covered references = old references
exact leaves / branches / mapping bytes stable within arm
exact root / transition / closure stable within arm
```

Report separately:

```text
durable publication
mapping/proof fold
proof consumption
COMMIT
first-after-reopen authority
authority + publication
Q, RSS, peak footprint, user/system CPU
```

## Interpretation

Wall time is not declared scale-independent merely because COMMIT dominates
these four points. Direct suffix references, rewritten pages, and canonical
mapping bytes control the algorithmic classification. The diagnostic may
quantify constant factors but cannot change the established `O(suffix)`,
worst-case `Theta(N)` bound.

No 100-GiB latency extrapolation is permitted. A prolly-tree decision remains
conditional on an explicit product requirement for near-constant
count-changing latency at multi-GiB/100-GiB scale.

## Terminal result

The accepted 34-row campaign completed in 89 seconds. Publication medians:

| Size | `+1` early | `+1` middle |
|---:|---:|---:|
| 1 MiB | 0.957833 ms | 1.080959 ms |
| 10 MiB | 1.738709 ms | 1.393625 ms |
| 100 MiB | 7.403083 ms | 5.715209 ms |
| 500 MiB | 27.140916 ms | 15.102042 ms |

From 100→500 MiB, suffix references grow 5.021/5.022x and mapping bytes grow
5.016/4.943x early/middle. Mapping wall grows 4.410/3.700x. Proof consumption
remains <=0.052 ms with zero object/payload authentication. Q grows only from
55,375 to 58,335 bytes and returns to zero.

First-after-reopen authority plus publication at 500 MiB is
1,262.772/1,228.564 ms. Both nonmedian 500-MiB fresh roundtrips pass.

Decision: retain K64/F64 under the current honest suffix-linear contract. If
the required product SLA is near 8–10 ms at 500 MiB or larger, stop before WP5
and specify a canonical prolly tree. Controlling report:
[CP-0008](../test-checkpoint-report/cp-0008-dirty-4f1c97f81f7c-count-change-scale.md).
