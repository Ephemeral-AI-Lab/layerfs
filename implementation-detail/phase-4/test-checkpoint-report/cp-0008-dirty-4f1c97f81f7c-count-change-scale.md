# CP-0008 — fixed-radix count-changing scale diagnostic

Status: `PASS / DIAGNOSTIC; RETAIN K64/F64 UNDER CURRENT POLICY`
Date: 2026-08-21
Parent: CP-0007
Observed accepted campaign wall: `89 seconds`
Configured campaign / command ceilings: `120 / 60 seconds`
Transient fixture/database bytes retained: `0`

## Identity

| Field | Value |
|---|---|
| Starting HEAD | `febc20f046bba84ccdce1256363d77799eabf2db` |
| Complete benchmark-source diff SHA-256 | `4f1c97f81f7cb855896c938bbead39f99ccfce354f36491c305ffbf072f2af73` |
| Benchmark source SHA-256 | `f1a0de44c335357eb43ef6cac3dd83c1cd148b0afffec34a126f13c2cb61f35a` |
| Release executable SHA-256 | `b5ec2b2cd8cae02e6e0c895a6f130d80477cd797c62e2328844328a9b16d8e68` |
| Runner SHA-256 | `96a95e132e04077f5a885530df615e0a04a966aa47a581b241617ff974090458` |
| Analyzer SHA-256 | `9c801d05e803e8d070114f0d4e769a56066d95d8354dc6ac511866318d3441a3` |
| Raw JSONL SHA-256 | `599a2dc8e62ace12876c14342435d4794ae349556fd87eeb3d6fa21e5fdd1804` |
| Analysis SHA-256 | `d477fe0a8e75bbf3fa6b63dcdf557ce288ec3e8ce63c468966a7a5c479d60a2c` |
| Profile | K64/F64 + DIR256K; `b0ebb845...f4ba1` |

Diagnostic source changes only admit exact 1/10/100/500-MiB fixtures and the
two count-changing operations. The CP-0007 mutation, construction proof,
canonical format, profile, SQLite schema, and durability path are unchanged.

## Schedule and custody

```text
sizes:      1, 10, 100, 500 MiB
operations: +1 early, +1 middle
per arm:    1 warmup + 3 measured
capture:    32 rows
roundtrip:   2 nonmedian 500-MiB rows
total:      34 rows
```

Every arm used one immutable prepared database/authority/expectation master;
every row used a fresh copy. The final master rehash passed. All temporary
sources and SQLite images were removed.

The first orchestration attempt completed all 32 capture samples and one
roundtrip but could not finish the second roundtrip inside the cap. It retained
no output. Its exact 2.4-GiB temporary root was deleted. The accepted attempt
changed only orchestration: paired master preparations ran concurrently and a
duplicate runner-side copy hash was removed because each child already hashes
and rejects its complete copied inputs. Sample count, row work, child custody,
verification, and limits did not change.

## Publication measurements

These medians exclude the separately reported first-after-reopen authority
scrub, matching CP-0007's durable publication boundary.

| Size | `+1` early median | Range | `+1` middle median | Range |
|---:|---:|---:|---:|---:|
| 1 MiB | 0.957833 ms | 0.864875–0.984834 | 1.080959 ms | 0.909125–5.898333 |
| 10 MiB | 1.738709 ms | 1.528084–1.764209 | 1.393625 ms | 1.174041–1.452333 |
| 100 MiB | 7.403083 ms | 7.081500–8.500459 | 5.715209 ms | 4.723333–6.418708 |
| 500 MiB | **27.140916 ms** | 24.967584–27.470708 | **15.102042 ms** | 14.102500–15.356917 |

The single 5.898-ms 1-MiB middle sample is an isolated COMMIT-side outlier;
the median remains 1.081 ms. No performance claim uses its minimum.

### Phase medians

| Size | Operation | Mapping/proof fold | Proof consume | COMMIT |
|---:|---|---:|---:|---:|
| 1 MiB | early / middle | 0.386 / 0.393 ms | 0.0065 / 0.0073 ms | 0.533 / 0.728 ms |
| 10 MiB | early / middle | 0.746 / 0.633 ms | 0.0078 / 0.0090 ms | 0.872 / 0.733 ms |
| 100 MiB | early / middle | 3.437 / 2.051 ms | 0.0222 / 0.0104 ms | 3.949 / 3.646 ms |
| 500 MiB | early / middle | 15.159 / 7.590 ms | 0.0516 / 0.0197 ms | 11.931 / 7.336 ms |

Proof consumption remains effectively constant and authenticates zero objects
and zero canonical payload bytes in all 24 measured rows. Mapping construction
and COMMIT grow as more canonical mapping objects are created and dirtied.

## Direct scaling counters

| Size | Old refs | Suffix early / middle | Leaves early / middle | Branches early / middle | Mapping bytes early / middle |
|---:|---:|---:|---:|---:|---:|
| 1 MiB | 53 | 53 / 27 | 1 / 1 | 0 / 0 | 4,073 / 4,073 |
| 10 MiB | 531 | 531 / 266 | 9 / 5 | 0 / 0 | 37,121 / 19,601 |
| 100 MiB | 5,284 | 5,284 / 2,642 | 83 / 42 | 2 / 2 | 365,495 / 185,915 |
| 500 MiB | 26,533 | 26,533 / 13,267 | 415 / 208 | 7 / 4 | 1,833,348 / 918,921 |

From 100 to 500 MiB:

```text
early:
  suffix references  5.021x
  mapping bytes       5.016x
  mapping wall        4.410x
  publication wall    3.666x
  reopen authority    5.144x

middle:
  suffix references  5.022x
  mapping bytes       4.943x
  mapping wall        3.700x
  publication wall    2.642x
  reopen authority    5.040x
```

This is direct empirical evidence that the operation is not scale-independent.
COMMIT and other fixed costs make total publication grow less than the exact
metadata work over this interval, but suffix references and canonical mapping
bytes track file size almost exactly.

## First edit after reopen

| Size | Authority early / middle | Authority + publication early / middle |
|---:|---:|---:|
| 1 MiB | 2.368 / 2.365 ms | 3.331 / 3.438 ms |
| 10 MiB | 23.446 / 23.063 ms | 25.184 / 24.515 ms |
| 100 MiB | 240.164 / 240.711 ms | 248.665 / 247.129 ms |
| 500 MiB | 1,235.301 / 1,213.208 ms | **1,262.772 / 1,228.564 ms** |

The fresh-authority scrub is linear in the complete authenticated closure and
dominates single-edit-after-reopen experience. A prolly tree would localize
mapping mutation but would not, by itself, eliminate this independent reopen
authentication requirement.

## Memory and fresh verification

Logical Q remains bounded: 25,041 bytes at 1 MiB, 37,047 at 10 MiB, 55,375 at
100 MiB, and 58,335 at 500 MiB; every row returns to zero. Median RSS plateaus
near 12.7 MiB at 100/500 MiB. The maximum median external peak-footprint
observation is 11,403,648 bytes.

Both 500-MiB roundtrips pass exact root/transition/closure, fresh reopen, full
scrub, reconstruction, and ranges:

| Operation | Publication | Fresh scrub | Reconstruction | Complete lifecycle |
|---|---:|---:|---:|---:|
| early | 22.975 ms | 1,364.906 ms | 2,151.418 ms | 3,540.673 ms |
| middle | 16.857 ms | 1,381.097 ms | 2,442.545 ms | 3,842.080 ms |

These are nonmedian correctness checks, not publication statistics.

## Decision

The measurements confirm both sides of the prior conclusion:

1. Fixed-radix K64/F64 remains practically fast under the current stated
   suffix-linear policy: even 500-MiB publication is 27.141 ms early and
   15.102 ms middle, both below 50 ms.
2. It is not scale-independent: exact suffix and mapping work increase about
   fivefold from 100 to 500 MiB, and early publication already exceeds 25 ms.

Therefore **retain K64/F64 and keep WP4-P closed under the current product
contract**. If the actual product requirement is near-8–10-ms count-changing
publication at 500 MiB, multi-GiB, or 100 GiB, the tested implementation
already fails that requirement and work must stop before WP5 for a new
canonical prolly-tree specification.

No 100-GiB runtime or latency extrapolation was performed.

## Evidence

- [raw JSONL](cp-0008-dirty-4f1c97f81f7c-count-change-scale.jsonl)
- [independent analysis](cp-0008-dirty-4f1c97f81f7c-count-change-scale-analysis.json)
- [preregistration](../wp4p/count-change-scale-diagnostic.md)
- [runner](../test/run-phase4-count-change-scale.sh)
- [analyzer](../test/analyze-phase4-count-change-scale.py)
