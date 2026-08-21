# CP-0009 — current Phase-4 product-workflow baseline

Status: `PASS / BASELINE`
Date: 2026-08-21
Parent: CP-0008
Accepted campaign wall: `51 seconds`
Rows: `42/42 PASS`
Transient fixture/database bytes retained: `0`

## Purpose

CP-0009 is the one current-release-binary control for the next research-selected
candidate A/B. It consolidates durable submit, same-count edit, warm/fresh
logical materialization, tiny authenticated routing, a real returned 1-MiB
authenticated range, reopen/head readiness, and the two count-changing guards.

This is a baseline, not a candidate speedup or format/profile promotion.

## Identity

| Field | Value |
|---|---|
| Starting HEAD | `febc20f046bba84ccdce1256363d77799eabf2db` |
| Benchmark-source diff SHA-256 | `b073a7e04c7a7a2b17671f80c42aee598cc5d8039e4ba83d63b7cac89d150f84` |
| Benchmark source SHA-256 | `3284c3bdfe20426df78b4cb8ef310248e1e4f644b8422d79c4689653d870652a` |
| Release executable SHA-256 | `9cda87ee7fd92784281a6ec7ee3045eb661681d8b7b930dd36546119ae4749d7` |
| Runner SHA-256 | `82931bfe6e5c49399341d92ac2f777038837c38e93c16f6883d1f72633970c32` |
| Analyzer SHA-256 | `810ffe046940bc7a0d15aed050e239e3d834643c7d760d542a9aff8b43aabfd6` |
| Raw JSONL SHA-256 | `988f6960d2fa12a0d0fff1e0db5de655f05fb3b08d6682a451846f0bfa6d5224` |
| Analysis SHA-256 | `616bbb186a9cb9ce4121b91bc96f8cff14407907b506e762995a21f63cbb323c` |
| Profile | K64/F64 + DIR256K; `b0ebb845...f4ba1` |

The controlling manifest is
[current-baseline-v1-manifest.tsv](../baseline/current-baseline-v1-manifest.tsv).

## Correct benchmark vocabulary

Every retained row states:

```text
schema = phase4-current-baseline-v1
purpose = product_workflow_baseline
milestone = CURRENT-BASELINE-V1
acceptance_scope = baseline
candidate_comparison = false
promotion = false
sample_kind = smoke | warmup | measured | structural-guard
measurement_boundary = one exact user/verification boundary
```

CPU is explicitly whole-child-process scope; phase-local CPU is unavailable.
Fresh process/application state is distinguished from OS/filesystem cache,
which remains warm-or-unknown.

## Schedule and custody

At 1/10 MiB, one smoke each covers write, warm/fresh materialization, tiny
ranges, 1-MiB sequential range, and reopen. At 100 MiB, each controlling
operation receives one warmup and three measured samples. One `+1` early and
middle row are nonmedian structural guards; CP-0008 remains their scale
authority.

Nine immutable database/authority/expectation masters are prepared lazily.
Each child receives a fresh copy and hashes the complete triplet. Final master
rehash passes. All transient files are deleted.

An initial semantically passing attempt prepared every 100-MiB read/edit
master before full-write timing. Its full-write COMMIT median moved from the
historical ~87-ms region to ~156 ms while exact work stayed unchanged, showing
setup-induced writeback state. That root was not accepted and was deleted. The
accepted schedule prepares and measures full write immediately after its own
master, then prepares protected-operation masters lazily. No result-dependent
sample, threshold, or operation changed.

## 100-MiB controlling baselines

| Product boundary | Median | Min | Max | Interpretation |
|---|---:|---:|---:|---|
| Durable full-file submit | **640.109209 ms** | 626.806833 | 670.710083 | 156.223 MiB/s |
| Same-open same-count edit | **9.737250 ms** | 8.725250 | 9.824166 | latency, not whole-file throughput |
| Warm logical materialization | **425.800708 ms** | 423.087209 | 425.989583 | 234.852 MiB/s |
| Fresh-process logical materialization | **433.512791 ms** | 430.447875 | 437.672542 | 230.674 MiB/s; OS cache unknown |
| Tiny authenticated boundary suite | **0.770666 ms** | 0.762417 | 0.830333 | routing regression metric |
| Authenticated returned 1-MiB range | **3.285167 ms** | 3.178833 | 3.500833 | range boundary including routing |
| Fresh-process reopen/head ready | **3.007750 ms** | 2.819500 | 3.151500 | process launch excluded |

### Full-write phase baseline

| Phase | Median | Min | Max |
|---|---:|---:|---:|
| source/CDC/CAS/mapping + proof fold | 504.215417 ms | 499.284333 | 517.287875 |
| proof consumption | 0.038542 ms | 0.037542 | 0.053042 |
| SQLite publication/COMMIT | 135.855250 ms | 127.484958 | 153.369166 |

Exact work remains 105,291,554 new canonical bytes and 365,262 mapping bytes.
Full submit remains 140.109 ms above the 500-ms/200-MiB/s target.

Absolute full-write wall is sensitive to filesystem/writeback state: CP-0007
measured 578.403 ms on an earlier exact binary/environment interval, whereas
the current control is 640.109 ms with a 43.903-ms spread. Therefore the next
candidate must run adjacent balanced A/B against this exact control binary.
Historical median subtraction is not admissible evidence of a candidate win.

## Authenticated 1-MiB returned range

The operation reads the middle 1 MiB of the 100-MiB file and compares exact
bytes prepared outside the timed range interval. Each measured row returns
1,048,576 bytes after authenticating 60 objects and 1,090,255 canonical bytes.

| Range-only wall | Returned-byte rate |
|---:|---:|
| 3.076833 ms | 325.010 MiB/s |
| 3.171209 ms | 315.337 MiB/s |
| 3.250042 ms | 307.688 MiB/s |

The controlling median is **315.337 MiB/s**. Q is 2,128,074 bytes because the
fixed 1-MiB expected and actual bounded buffers overlap; terminal Q is zero.

## Same-open authority and count-changing guards

Same-count authority establishment is separately visible at a 245.330-ms
median; authority plus durable edit is therefore approximately 255.068 ms on
first use after reopen. The 9.737-ms edit boundary applies after authority.

| Guard | Same-open publication | Authority | First-after-reopen | Suffix refs | Mapping bytes |
|---|---:|---:|---:|---:|---:|
| `+1` early | 7.374750 ms | 241.116791 ms | 248.491541 ms | 5,284 | 365,495 |
| `+1` middle | 5.321541 ms | 238.984125 ms | 244.305666 ms | 2,642 | 185,915 |

These single rows protect mechanism/identity; CP-0008's 1/10/100/500-MiB
three-sample arms control the scaling conclusion.

## Small-size smoke

| Operation | 1 MiB | 10 MiB |
|---|---:|---:|
| Durable write | 7.323 ms | 66.202 ms |
| Warm materialization | 4.250 ms | 42.137 ms |
| Fresh materialization | 7.109 ms | 46.227 ms |
| Tiny range suite | 0.404 ms | 0.522 ms |
| 1-MiB returned range | 2.825 ms | 3.082 ms |
| Reopen/head ready | 3.225 ms | 3.214 ms |

These are correctness/scaling smokes, not distributions.

## Resources

| Operation | Median Q | Median RSS | Median peak footprint |
|---|---:|---:|---:|
| Full write | 88,093 B | 93,405,184 B | 92,160,432 B |
| Same-count edit | 2,222,803 B | 15,302,656 B | 6,881,664 B |
| Warm materialization | 34,243 B | 17,055,744 B | 12,550,552 B |
| Fresh materialization | 34,243 B | 17,645,568 B | 12,616,064 B |
| Tiny range suite | 31,484 B | 7,880,704 B | 1,966,416 B |
| 1-MiB range | 2,128,074 B | 7,847,936 B | 4,260,176 B |
| Reopen | 17,128 B | 7,847,936 B | 1,524,024 B |

Every row returns Q to zero. Q remains distinct from process RSS and external
peak footprint.

## What this baseline does not claim

- no candidate speedup or balanced A/B result;
- no true cold OS/filesystem or physical-media measurement;
- no native-file checkout/materialization wall;
- no phase-local CPU attribution;
- no universal host/fixture portability;
- no 500-MiB work except the separately linked CP-0008 affected-operation
  scale diagnostic;
- no qualification or promotion authority beyond the already completed WP4-P.

## Research handoff

The exact next-candidate control is now:

```text
durable submit:           640.109 ms
mapping/construction:     504.215 ms
proof consumption:         0.039 ms
COMMIT:                   135.855 ms
same-count edit:            9.737 ms
warm/fresh materialize:   425.801 / 433.513 ms
1-MiB range:                3.171 ms / 315.337 MiB/s
reopen/head:                3.008 ms
```

The research task should reconcile three separate questions, not bundle them:

1. full-create canonical-v2/ordered-authority opportunity against the
   504.215-ms construction lane;
2. prolly mapping only if the product requires near-constant count-changing
   latency beyond CP-0008's 500-MiB result;
3. reopen authority/open-session design for the separately linear ~240-ms
   first-edit authentication cost.

Whichever hypothesis advances must preregister one variable and run adjacent
balanced control/candidate pairs. CP-0009's standalone median is context, not
the comparison method.

## Evidence

- [raw JSONL](cp-0009-dirty-b073a7e04c7a-current-product-baseline.jsonl)
- [analysis](cp-0009-dirty-b073a7e04c7a-current-product-baseline-analysis.json)
- [manifest](../baseline/current-baseline-v1-manifest.tsv)
- [preregistration](../baseline/current-baseline-v1.md)
- [CP-0008 scale report](cp-0008-dirty-4f1c97f81f7c-count-change-scale.md)
