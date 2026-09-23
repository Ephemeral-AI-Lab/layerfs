# #237 pack-space C2: exact length at every placement flush

> **Status: Research; informative and not a product contract.** One retained
> Core SDK 100k release candidate from the [prospective treatment plan](pack-space-treatment-plan-20260924.md).
> C2 solved most of the dense Store reservation, but added many pooled-metadata
> pack directories and has no #229 sparse-history proof.

## One-shot identity and verification

The committed treatment source was `e5dc8682c2ad9ed76b1a8c93b84439b4ab3078e3`
(product seal `bb8f5f39433e67ddc6cc8cc4c717e426da5473e89a50a14b43c624d610fad13e`).
It used the same sole Core SDK runner harness seal
`96a04c70338d34116c1c66ac04617bb77df368fbcae4e0c1ca29ac04f48e90a3`
as the control and C1, locked Cargo **release** examples, and the exact
seed-1 SHAKE manifest
`23246a276522418812df2392619c13391bc864835d09a6695b6bd6fc0307d7a5`.
The [run identity](evidence/packspace-c2-20260924/run.json),
[build receipt](evidence/packspace-c2-20260924/build.json), and
[independent copy](evidence/packspace-c2-20260924/source-copy.json) retain
the source and artifact details. The final [cold check](evidence/packspace-c2-20260924/cold-launch.json)
found **0/126,206 resident source payload pages**. Namespace metadata
residency remains unqualified, so the raw time is performance
`INELIGIBLE`, not a cold latency PASS.

The [one public receipt](evidence/packspace-c2-20260924/receipt.json) has
one `Client::init_project` call and cleanup PASS. The
[independent reopened oracle](evidence/packspace-c2-20260924/verification.json)
passed all 101,001 paths, portable metadata, 100,000 file hashes and
500,000,000 bytes in **9.758088334 s**, below the general under-10-s
expectation. The raw run's hash-manifest check returned PASS. The source
tree had no dirty paths at sampling.

## Dense 100k measurements

| One raw row | Core control | C1 | C2 | C2 minus control |
| --- | ---: | ---: | ---: | ---: |
| Public SDK call | 5.077702667 s | 5.512661042 s | **5.017461833 s** | −0.060240834 s, one-run observation |
| Complete driver command | 6.408342167 s | 6.065425084 s | **6.387194125 s** | −0.021148042 s |
| Driver lifecycle peak RSS | 155,631,616 B | 157,237,248 B | **154,173,440 B** | −1,458,176 B raw |
| Pack rows | 2,082 | 2,082 | **4,282** | +2,200 |
| Pack BLOB lengths | 545,783,808 B | 545,160,862 B | **519,366,346 B** | −26,417,462 B |
| Pack declared used | 510,615,731 B | 510,615,715 B | **519,366,346 B** | +8,750,615 B |
| Unused post-used pack tail | 35,168,077 B | 34,545,147 B | **0 B** | −35,168,077 B |
| Store + History apparent | 554,098,688 B | 553,472,000 B | **528,216,064 B** | −25,882,624 B |
| Store + History allocated | 558,145,536 B | 554,713,088 B | **534,118,400 B** | −24,027,136 B |

The [read-only C2 pack geometry](evidence/packspace-c2-20260924/candidate-pack-geometry.json)
pins Store SHA-256
`97e8d913231f8166cde32b1555b7f06ff3ba0b37246df9ade8c7801e9769966e`.
Its Store is 128,938 × 4,096-B pages, zero freelist, **528,130,048 B
apparent / 534,032,384 B allocated**; History adds **86,016 B** in both
columns. The old [exact-source v0.1.6 Store](v016-exact-100k-storage-result-20260924.md)
is 514,879,488 B apparent / 520,110,080 B allocated, leaving C2
**13,336,576 B apparent / 14,008,320 B allocated above it**.

## Why C2 stopped short

C2 inserted each of 2,001 pooled-metadata groups into its own v12 pack.
The Core control placed those same 2,001 groups into 16 v12 packs. Across
all lanes, the extra 2,200 packs split into 1,985 pooled, 102 native,
102 whole-file and 11 ordinary. Their declared-used growth is
8,178,200 + 420,252 + 106,892 + 45,271 = **8,750,615 B**. That is
almost exactly the sum of each new pack's 24-B header and lane-reserved
directory; source payload did not grow by 8.75 MB. The pooled portion alone
is 1,985 × (24 + 4,096) = **8,178,200 B**. Closing every flush eliminates
future-append capacity but pays for a fixed directory in every new pack.

Against v0.1.6 on the same source, C2's combined apparent gap partitions
into **12,340,917 B** more pack BLOB bytes, **909,643 B** other Store bytes,
and **86,016 B** of separate History. Those are disjoint file-length
terms. The v0.1.6 and Core formats, metadata grouping and object tables
also differ; the residual is not one single unallocated tail.

**Decision: keep the measured C2 row as a partial treatment, not the final
space fix.** It met the planned ≥80% tail reduction and no-worse combined
allocation criteria, with full readback, but remained 14.0 MB above the
matched old Store and has no matched #229 sparse-history proof. The
[prospective C3 plan](pack-space-c3-plan-20260924.md) freezes selective
pooled reuse before its own code/sample; no C2 result is resampled or
relabelled. The complete raw C2 output, including closed databases and its
hash manifest, is worktree-local at
`benchmark-results/fs-bench-pro/sdk-100k-packspace-c2-20260924-01/`.
