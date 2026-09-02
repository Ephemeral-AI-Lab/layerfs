# LayerFS 0.1.1 terminal benchmark evidence

> **Status:** Final terminal evidence for the `v0.1.1` Developer Preview.

`fs-bench-pro` is LayerFS-only for 0.1.1. Historical comparison
artifacts remain immutable, but no external product is part of the active
runner or this release decision.

## Namespace-v2 terminal campaign

The terminal campaign is retained at:

`benchmark-results/fs-bench-pro/namespace/v011-rc-terminal-all4-r001-20260903`

Evidence identities:

| Item | SHA-256 |
| --- | --- |
| Source seal | `c17e554ac21d53fd168a70bc492bf5342eb5a90a6f7a9c067f81c4148976cd7c` |
| `report.md` | `c0b79f9ee626a7d64eea5893cdf348f092e7579fd8300da8a8f51e1660425c38` |
| Run status | `4653d003e0d4d2098f77f93493f8b3baabb2e7cbfd2c4eec930b8d97841fcd9c` |
| Composite proof | `3acd34a1c870b118ac1590bc0bd30226e1631f9f274d5ddf8aa86c850ff1bb70` |

Selected subsequent-cache medians:

| Scenario | Logical bytes | Initialization | Throughput | Files/s | Create | Commit |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `namespace-100` | 125 MB | 220.555 ms | 566.8 MB/s | 453 | 12.886 ms | 2.740 ms |
| `namespace-1000` | 200 MB | 286.319 ms | 698.5 MB/s | 3,492 | 16.233 ms | 3.077 ms |
| `namespace-10000` | 300 MB | 399.025 ms | 751.8 MB/s | 25,061 | 16.058 ms | 3.448 ms |
| `namespace-100000` | 500 MB | 2.805621 s | 178.2 MB/s | 35,642 | 14.648 ms | 3.891 ms |

Every setup, product, verification, performance, evidence, resource,
correctness, cleanup, quality, sample-shape, normal-overwrite, and composite
gate passes. This includes the strict 100-file Create ceiling and the
authorized 100,000-file gate of at most 3.235294118 seconds, at least 153 MB/s,
and at least 30,600 files/s. The preferred 200-MB/s / 2.5-second outcome remains
nonbinding.

## Registered payload terminal campaign

The latest retained LayerFS-only payload report is:

`benchmark-results/fs-bench-pro/runs/v011-rc-payload-final-r001-20260903`

Source seal:
`dd219ed9e7942a42891ff14646ee3c54a4580e6aaeeee7a25a01b30d1453a805`.
Report SHA-256:
`13f2d29362bdf409121af95b80ad1b2911a24fc1ed05760a75a3bf3160248bab`.
Every frozen hard gate passes:

| Metric | 0.1.1 median | Frozen gate |
| --- | ---: | ---: |
| Workspace Create | 14.550 ms | ≤20 ms |
| Small-edit Commit | 4.503 ms | ≤6 ms |
| Cold-create-32m complete | 131.774 ms | ≤150 ms |
| EDIT16 | 156.446 ms | ≤200 ms |
| Prepend | 223.763 ms | ≤250 ms |
| Read 32 MiB | 141.418 ms | ≤150 ms |
| Registered total | 653.401 ms | ≤700 ms |
| Inner write throughput | 505.6 MB/s | ≥314.6 MB/s |
| Host peak RSS | 97.1 MB | ≤128 MiB |

Against retained 0.1.0 medians of 636.378417 ms total and 113.571208 ms read,
the 0.1.1 result is approximately 2.7% slower in total and 24.5% slower
on read. The read difference is the accepted cost of bounding authenticated
read-ahead to 8 MiB instead of retaining a larger speculative payload window:
it limits owned memory and unused fetches at the cost of additional bounded
read service work. The 141.418-ms result remains below the frozen 150-ms hard
gate, and the 653.401-ms total remains below 700 ms.

## Publication identity

The benchmark source seals bind their captured development worktrees. The
annotated `v0.1.1` tag, required CI check, source archives, and release asset
checksums bind the published source identity; they do not rewrite the retained
benchmark evidence.
