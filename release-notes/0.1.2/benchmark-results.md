# LayerFS 0.1.2 final-candidate benchmark results

> **Status:** Final v0.1.2 evidence, measured at code/harness candidate
> `c6c14d5a` and published with the documentation-only release commit.

**Headline:** Every 256 KiB count-changing temp-copy sample had a batch-average
mutation time below 10 ms/op; full LayerFS lifecycle medians were
approximately 25–343 ms across its 1/10/100-operation cases; 1/10/100 MiB
results demonstrate larger-file scaling behavior.

The headline is deliberately about mutation latency, not copied-payload MiB/s.
The 256 KiB delete/shrink cases pass their strict latency gate and are not
classified as slow because of a secondary throughput conversion.

## How to read the tables

- `N` is the number of raw samples; `N/arm` is the count for each A/B or
  baseline/candidate arm.
- A range is the minimum–maximum of the same raw samples used for the median.
  Every median is computed from the named raw field before display rounding,
  unless the column explicitly identifies a derived value.
- Nanoseconds are divided by `1,000,000` for ms and `1,000,000,000` for s.
  Bytes remain integer bytes; B/s is divided by `1,048,576` for MiB/s.
- A directional ratio is candidate-median / baseline-median. An A/A ratio is
  symmetric (`max(A/B, B/A)`), so it is always at least 1.0.
- Commit/visibility includes the public Commit return and explicit Branch-head
  visibility acknowledgement. Verification is always outside performance timing.

## Evidence identity

| Evidence | Immutable local path | Manifest SHA-256 | Disposition |
| --- | --- | --- | --- |
| Universal conformance | benchmark-results/fs-bench-pro/edit-engine-acceptance/final-v012-issue14-c6c14d5a | deca3578ce3aabbad6ff61c41c5d42297e6d8f02fbd699a4b523194193b2aa4b | pass |
| Owner-side timing supplement | benchmark-results/fs-bench-pro/edit-engine-acceptance/final-v012-issue14-performance-c6c14d5a-r3 | 0494d0d9c33ea79e488b3078e18714e86b17995df27e5123c11ecc285861f9e3 | pass; 9 measurements |
| Same-count | benchmark-results/fs-bench-pro/edit-same-count/final-v012-same-count-c6c14d5a | 07a17444ac938abbe27d3955fd6cb3eeca92f2a87ca10770a61777608e06cc05 | target-pass |
| Same-count anchor replay | benchmark-results/fs-bench-pro/edit-same-count/final-v012-same-count-c6c14d5a-anchor-custody | a401fd0092246d380fe626daa55d4e413543bbc2c299241410263416899bad63 | custody pass; no measurement rerun |
| Count-changing | benchmark-results/fs-bench-pro/edit-count-changing/final-v012-count-changing-c6c14d5a | 491da0d15babd56b38eef00e85f282f318e0f44a847ee5a0a7b289733d979e97 | tolerated-pass |
| Count-changing anchor replay | benchmark-results/fs-bench-pro/edit-count-changing/final-v012-count-changing-c6c14d5a-anchor-custody | 6c9145ae590d58dced850aa836c273036af07ae39842a214cad1b5eb110d284c | custody pass; no measurement rerun |
| Store footprint | benchmark-results/fs-bench-pro/store-footprint/final-v012-store-c6c14d5a | 7907b11fa3db15cca13fda6a99a949c3ee0b984cb743270ba182cc0ef586271b | baseline complete; footprint blocker |

Commit `c6c14d5a5a740665f5efbce439493f681bd7dd95`, tree `7c8b843c354fa49f4afa344d66c358a776bfd0d0`, source seal `6b3c039e4237a8ab27eebc5ea4752bc8ad9f58039725ac9b2e3230119b171ec9`, product seal
`438253c10b6b33ae33e6b81113390f0d06d5b98fb2c0fc6c0e0438e0d483431f`, harness seal `4c68f918828036082c7110e28bfb2a2e88983d46d404fc1de3899335ad15694c`, workload SHA-256 `c07029d3bf95c187ded2899f3e6840449301a1495c8a51fc694fbbca63fbf6d9`.
The count-changing frozen baseline has the same commit/tree/product and a
different workload/source seal; the candidate image is labeled clean and the
baseline image is explicitly labeled dirty only because it carries the frozen
workload source.

The generator checks each manifest's expected SHA-256, rehashes every file used
for these tables, and validates identities, row counts, unique case/arm/seed keys,
statuses, and the headline's strict per-sample condition. `--verify-all` also
rehashes every sealed raw artifact, including the multi-gigabyte Store files.

## Universal owner-side range edits

| Case | Description | N | Edit ms, median (range) | Commit ms, median (range) | Edit + Commit ms, median (range) | Lifecycle ms, median (range) | Ops/s | Disposition |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| `workspace-range-prepend-head-10b-on-32m` | Owner prepend, 10 B on 32 MiB | 3 | 9.167 (8.267–11.966) | 2.869 (2.856–2.977) | 12.144 (11.123–14.835) | 26.432 (24.239–27.861) | 109 | pass |
| `workspace-range-overwrite-middle-4k-on-256k-100` | 100 owner overwrites, 4 KiB on 256 KiB | 3 | 11.763 (8.314–12.339) | 3.359 (3.263–3.497) | 15.026 (11.673–15.836) | 25.368 (23.616–27.526) | 8,501 | pass |
| `workspace-range-insert-middle-4k-on-256k-100` | 100 owner inserts, 4 KiB on 256 KiB | 3 | 10.700 (10.256–12.305) | 4.096 (3.915–5.048) | 15.304 (14.796–16.220) | 26.986 (26.838–28.831) | 9,345 | pass |

Interpretation: Edit measures the public owner-side range-edit call; Commit is
the public Commit call; Edit + Commit is measured directly. Lifecycle begins
before Workspace creation and ends after clean Workspace end. LayerStack
initialization and Branch fork are excluded. These rows prove the structural
owner path: unchanged payload transfer is zero and conformance is separate.

## Same-count family (14 IDs)

| Case | Description | Ops | N/arm | Execution A median (range) / B median (range) ms | Commit/visibility A median (range) / B median (range) ms | Lifecycle A median (range) / B median (range) ms | Ops/s A/B | Max RSS A/B | A/A lifecycle ratio | Class |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| `edit16` | legacy-overwrite-distributed-10b-ops-16-commit-each | 16 | 3 | 54.730 (47.221–56.147) / 44.544 (43.593–60.277) | 38.838 (37.918–39.844) / 36.565 (36.203–39.826) | 104.105 (97.092–108.510) / 91.554 (91.058–113.953) | 292 / 359 | 74.156 / 74.000 MiB | 1.137084 | diagnostic only |
| `overwrite-distributed-1b-to-4k-ops-1` | Distributed 1 B-4 KiB overwrite, 1 operation | 1 | 3 | 6.592 (6.446–6.799) / 6.180 (5.091–7.873) | 2.815 (2.523–2.870) / 2.220 (2.082–2.495) | 21.924 (20.594–23.181) / 21.709 (20.084–22.389) | 798 / 1,118 | 7.922 / 7.906 MiB | 1.009909 | diagnostic only |
| `overwrite-distributed-1b-to-4k-ops-10` | Distributed 1 B-4 KiB overwrite, 10 operations | 10 | 3 | 14.633 (12.872–16.139) / 16.217 (14.501–18.036) | 3.759 (3.473–3.791) / 3.470 (3.200–3.796) | 31.036 (29.450–32.651) / 34.173 (32.076–38.211) | 1,379 / 1,327 | 8.938 / 9.125 MiB | 1.101084 | diagnostic only |
| `overwrite-distributed-1b-to-4k-ops-100` | Distributed 1 B-4 KiB overwrite, 100 operations | 100 | 3 | 39.420 (36.426–49.410) / 46.577 (34.277–49.251) | 7.682 (7.591–10.527) / 7.506 (7.338–9.462) | 63.213 (55.521–70.380) / 68.130 (51.331–74.309) | 3,098 / 2,467 | 10.031 / 10.062 MiB | 1.077798 | diagnostic only |
| `overwrite-head-4k-ops-1` | Overwrite 4 KiB at head, 1 operation | 1 | 3 | 6.590 (5.371–8.062) / 7.939 (6.093–9.412) | 2.333 (2.041–2.732) / 2.279 (2.219–2.403) | 21.460 (18.934–24.241) / 23.177 (19.265–25.571) | 787 / 953 | 8.047 / 8.219 MiB | 1.079969 | diagnostic only |
| `overwrite-head-4k-ops-10` | Overwrite 4 KiB at head, 10 operations | 10 | 3 | 6.852 (6.734–7.465) / 7.065 (6.437–8.002) | 2.252 (2.163–2.623) / 2.449 (2.377–2.611) | 20.534 (19.869–22.278) / 20.231 (19.633–22.731) | 4,668 / 5,595 | 7.953 / 8.094 MiB | 1.014969 | diagnostic only |
| `overwrite-head-4k-ops-100` | Overwrite 4 KiB at head, 100 operations | 100 | 3 | 13.210 (13.034–13.996) / 17.750 (15.194–18.386) | 2.412 (2.201–2.481) / 2.377 (2.163–2.504) | 27.981 (26.971–28.594) / 31.599 (27.935–32.823) | 11,901 / 9,081 | 8.000 / 8.109 MiB | 1.129312 | diagnostic only |
| `overwrite-middle-4k-ops-1` | Overwrite 4 KiB in middle, 1 operation | 1 | 3 | 7.054 (5.823–7.921) / 5.914 (4.774–6.905) | 2.465 (2.126–2.472) / 2.515 (2.295–3.088) | 20.909 (20.709–21.277) / 18.876 (18.582–23.471) | 1,244 / 1,422 | 7.938 / 7.969 MiB | 1.107686 | diagnostic only |
| `overwrite-middle-4k-ops-10` | Overwrite 4 KiB in middle, 10 operations | 10 | 3 | 11.929 (10.919–13.212) / 12.334 (11.477–12.485) | 3.221 (3.181–3.226) / 3.297 (3.168–3.895) | 28.118 (27.922–28.675) / 28.013 (25.125–29.236) | 1,452 / 1,337 | 8.828 / 8.969 MiB | 1.003735 | diagnostic only |
| `overwrite-middle-4k-ops-100` | Overwrite 4 KiB in middle, 100 operations | 100 | 3 | 25.887 (25.611–35.561) / 26.697 (22.946–27.908) | 3.527 (3.427–3.868) / 3.308 (2.965–3.721) | 41.444 (40.423–51.974) / 40.744 (38.023–45.298) | 4,743 / 4,696 | 9.422 / 9.297 MiB | 1.017186 | diagnostic only |
| `overwrite-tail-4k-ops-1` | Overwrite 4 KiB at tail, 1 operation | 1 | 3 | 5.358 (5.253–5.957) / 6.421 (4.829–7.694) | 2.505 (2.348–2.613) / 2.610 (2.387–3.051) | 20.540 (19.689–23.560) / 21.587 (19.851–24.583) | 1,550 / 1,892 | 7.703 / 7.734 MiB | 1.050951 | diagnostic only |
| `overwrite-tail-4k-ops-10` | Overwrite 4 KiB at tail, 10 operations | 10 | 3 | 7.818 (6.035–9.651) / 7.248 (5.371–10.195) | 3.290 (2.576–3.901) / 2.359 (2.288–3.522) | 24.638 (22.704–28.541) / 22.767 (20.300–23.988) | 7,448 / 4,807 | 7.719 / 7.609 MiB | 1.082174 | diagnostic only |
| `overwrite-tail-4k-ops-100` | Overwrite 4 KiB at tail, 100 operations | 100 | 3 | 14.890 (14.085–15.102) / 13.689 (13.215–14.010) | 2.358 (2.233–2.375) / 2.988 (2.721–3.667) | 29.444 (26.093–29.526) / 30.128 (29.261–33.121) | 10,936 / 13,531 | 7.641 / 7.672 MiB | 1.023219 | diagnostic only |
| `small-edit` | legacy-overwrite-distributed-10b-ops-1 | 1 | 3 | 4.810 (4.304–4.982) / 4.707 (4.350–5.579) | 3.011 (2.918–3.155) / 2.856 (2.809–2.992) | 19.304 (19.262–23.126) / 22.970 (20.594–24.052) | 207 / 212 | 73.188 / 73.250 MiB | 1.189919 | diagnostic only |

Interpretation: both labels run identical source with one sealed daemon-container identity. The terminal
gate is the symmetric aggregate arm-wall ratio `1.004258171`
(repeat-a `1.436 s`, repeat-b `1.443 s`, target
`<=1.05`). Per-case A/A ratios are scheduling diagnostics—even values above
`1.10` do not become directional product regressions. Every row still uses a
fresh Store, Branch, Workspace, and workload process; six independent
fragmentation/root/reopen proofs plus one timing/status receipt pass outside
performance timing.

## Count-changing primary family (25 IDs)

| Case | Description | Implementation | Ops | N/arm | Mutation/op ms | Workload ms | Commit/visibility ms | Lifecycle ms | Throughput | Absolute gate | Max RSS | Candidate/baseline | Class |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| `append-tail-4k-ops-1` | Append 4 KiB at tail, 1 operation | direct-posix | 1 | 3 | 0.990 (0.972–0.990) | 5.474 (4.535–5.589) | 2.594 (2.393–2.803) | 19.616 (19.349–21.743) | 1,010 ops/s | ops/s ≥250 | 7.562 MiB | 0.853094 | target; absolute target |
| `append-tail-4k-ops-10` | Append 4 KiB at tail, 10 operations | direct-posix | 10 | 3 | 0.733 (0.679–0.851) | 12.097 (11.859–13.464) | 2.595 (2.231–2.603) | 27.733 (25.225–28.361) | 1,363 ops/s | ops/s ≥250 | 8.172 MiB | 1.033730 | target; absolute target |
| `append-tail-4k-ops-100` | Append 4 KiB at tail, 100 operations | direct-posix | 100 | 3 | 0.703 (0.578–0.748) | 74.493 (61.530–78.652) | 4.138 (4.113–4.159) | 92.337 (78.140–94.230) | 1,421 ops/s | ops/s ≥250 | 10.156 MiB | 1.076766 | tolerated; absolute target |
| `delete-middle-2k-ops-1` | Delete 2 KiB at middle, 1 operation | temp-copy-fsync-rename | 1 | 3 | 4.670 (4.352–5.684) | 9.423 (8.402–9.750) | 3.111 (2.843–3.498) | 25.441 (22.746–27.127) | 214 ops/s; 53.117 MiB/s copied | mutation ≤10 ms/op | 8.750 MiB | 0.903414 | target; absolute target |
| `delete-middle-2k-ops-10` | Delete 2 KiB at middle, 10 operations | temp-copy-fsync-rename | 10 | 3 | 3.140 (2.842–3.180) | 34.754 (32.094–35.532) | 2.954 (2.930–3.664) | 49.782 (45.061–51.187) | 318 ops/s; 76.199 MiB/s copied | mutation ≤10 ms/op | 8.953 MiB | 0.972634 | target; absolute target |
| `delete-middle-2k-ops-100` | Delete 2 KiB at middle, 100 operations | temp-copy-fsync-rename | 100 | 3 | 2.700 (2.427–2.762) | 274.183 (246.093–280.025) | 2.668 (2.482–2.755) | 287.735 (259.894–296.103) | 370 ops/s; 56.056 MiB/s copied | mutation ≤10 ms/op | 9.188 MiB | 1.096621 | tolerated; absolute target |
| `insert-middle-4k-ops-1` | Insert 4 KiB at middle, 1 operation | temp-copy-fsync-rename | 1 | 3 | 6.784 (5.279–8.115) | 11.418 (9.032–15.347) | 2.981 (2.547–4.866) | 27.483 (22.375–35.939) | 147 ops/s; 36.851 MiB/s copied | mutation ≤10 ms/op | 9.031 MiB | 1.079309 | tolerated; absolute target |
| `insert-middle-4k-ops-10` | Insert 4 KiB at middle, 10 operations | temp-copy-fsync-rename | 10 | 3 | 2.935 (2.889–3.662) | 33.655 (32.943–40.955) | 3.086 (2.964–3.186) | 51.300 (49.839–55.768) | 340 ops/s; 91.171 MiB/s copied | mutation ≤10 ms/op | 9.062 MiB | 1.044274 | target; absolute target |
| `insert-middle-4k-ops-100` | Insert 4 KiB at middle, 100 operations | temp-copy-fsync-rename | 100 | 3 | 3.232 (3.199–3.303) | 326.601 (323.215–333.199) | 4.058 (3.788–4.287) | 342.126 (338.368–348.870) | 309 ops/s; 137.191 MiB/s copied | mutation ≤10 ms/op | 11.547 MiB | 0.987945 | target; absolute target |
| `prepend-head-4k-ops-1` | Prepend 4 KiB at head, 1 operation | temp-copy-fsync-rename | 1 | 3 | 6.788 (4.757–6.976) | 11.670 (8.978–12.243) | 2.863 (2.693–3.128) | 25.667 (24.170–27.470) | 147 ops/s; 36.827 MiB/s copied | mutation ≤10 ms/op | 8.719 MiB | 1.031561 | target; absolute target |
| `prepend-head-4k-ops-10` | Prepend 4 KiB at head, 10 operations | temp-copy-fsync-rename | 10 | 3 | 3.201 (3.040–3.637) | 36.719 (36.674–42.176) | 3.089 (2.983–3.372) | 53.484 (50.235–57.217) | 312 ops/s; 83.594 MiB/s copied | mutation ≤10 ms/op | 9.031 MiB | 1.034174 | target; absolute target |
| `prepend-head-4k-ops-100` | Prepend 4 KiB at head, 100 operations | temp-copy-fsync-rename | 100 | 3 | 3.259 (3.210–3.326) | 329.248 (324.624–337.784) | 4.534 (4.188–4.923) | 342.895 (342.501–353.446) | 306 ops/s; 136.025 MiB/s copied | mutation ≤10 ms/op | 11.516 MiB | 1.009216 | target; absolute target |
| `prepend-temp-copy-rename` | legacy-prepend-head-10b-on-32m-temp-copy-rename | temp-copy-fsync-rename | 1 | 3 | 97.378 (90.804–110.626) | 97.378 (90.804–110.626) | 51.994 (51.977–52.085) | 161.682 (155.242–173.734) | 10 ops/s; 328.616 MiB/s copied | lifecycle ≤223.763 ms | 22.766 MiB | 1.022681 | target; absolute target |
| `replace-middle-grow-2k-to-4k-ops-1` | Replace middle 2 KiB with 4 KiB, 1 operation | temp-copy-fsync-rename | 1 | 3 | 4.804 (4.330–5.489) | 10.086 (7.627–10.897) | 2.793 (2.660–3.032) | 25.647 (23.944–27.103) | 208 ops/s; 51.631 MiB/s copied | mutation ≤10 ms/op | 8.703 MiB | 0.993537 | target; absolute target |
| `replace-middle-grow-2k-to-4k-ops-10` | Replace middle 2 KiB with 4 KiB, 10 operations | temp-copy-fsync-rename | 10 | 3 | 2.955 (2.724–3.046) | 35.380 (32.324–36.627) | 3.005 (2.979–5.423) | 51.081 (50.813–54.058) | 338 ops/s; 86.910 MiB/s copied | mutation ≤10 ms/op | 9.203 MiB | 1.026582 | target; absolute target |
| `replace-middle-grow-2k-to-4k-ops-100` | Replace middle 2 KiB with 4 KiB, 100 operations | temp-copy-fsync-rename | 100 | 3 | 3.048 (2.938–3.138) | 308.458 (297.207–317.752) | 3.322 (3.190–3.365) | 323.677 (313.556–332.008) | 328 ops/s; 113.107 MiB/s copied | mutation ≤10 ms/op | 10.188 MiB | 1.029851 | target; absolute target |
| `replace-middle-shrink-4k-to-2k-ops-1` | Replace middle 4 KiB with 2 KiB, 1 operation | temp-copy-fsync-rename | 1 | 3 | 5.491 (4.726–7.171) | 10.010 (8.403–10.799) | 2.822 (2.619–3.418) | 25.551 (21.874–25.833) | 182 ops/s; 44.818 MiB/s copied | mutation ≤10 ms/op | 8.953 MiB | 1.040565 | target; absolute target |
| `replace-middle-shrink-4k-to-2k-ops-10` | Replace middle 4 KiB with 2 KiB, 10 operations | temp-copy-fsync-rename | 10 | 3 | 3.259 (3.122–3.264) | 36.652 (35.017–37.859) | 3.060 (2.938–3.094) | 51.273 (51.131–51.961) | 306 ops/s; 72.807 MiB/s copied | mutation ≤10 ms/op | 9.000 MiB | 1.056754 | tolerated; absolute target |
| `replace-middle-shrink-4k-to-2k-ops-100` | Replace middle 4 KiB with 2 KiB, 100 operations | temp-copy-fsync-rename | 100 | 3 | 2.380 (2.376–2.587) | 242.043 (240.942–262.183) | 2.684 (2.658–2.717) | 256.053 (254.641–277.557) | 420 ops/s; 62.791 MiB/s copied | mutation ≤10 ms/op | 8.938 MiB | 0.948616 | target; absolute target |
| `sparse-write-past-eof-gap-60k-payload-4k-ops-1` | Sparse write 4 KiB after 60 KiB EOF gap, 1 operation | direct-posix | 1 | 3 | 0.880 (0.854–1.719) | 5.078 (3.911–6.223) | 2.929 (2.446–3.039) | 20.962 (17.574–20.977) | 1,135 ops/s | ops/s ≥250 | 8.234 MiB | 1.053121 | tolerated; absolute target |
| `sparse-write-past-eof-gap-60k-payload-4k-ops-10` | Sparse write 4 KiB after 60 KiB EOF gap, 10 operations | direct-posix | 10 | 3 | 0.732 (0.688–0.785) | 11.800 (11.253–12.339) | 4.460 (4.371–4.616) | 30.069 (27.934–32.253) | 1,365 ops/s | ops/s ≥250 | 9.812 MiB | 0.982220 | target; absolute target |
| `sparse-write-past-eof-gap-60k-payload-4k-ops-100` | Sparse write 4 KiB after 60 KiB EOF gap, 100 operations | direct-posix | 100 | 3 | 0.631 (0.589–0.676) | 68.697 (62.668–71.440) | 21.509 (20.087–22.728) | 100.583 (98.683–103.041) | 1,584 ops/s | ops/s ≥250 | 37.844 MiB | 0.924797 | target; absolute target |
| `truncate-tail-2k-ops-1` | Truncate 2 KiB at tail, 1 operation | direct-posix | 1 | 3 | 1.063 (0.999–1.248) | 5.774 (5.077–5.896) | 2.084 (1.953–2.236) | 20.023 (19.899–20.035) | 940 ops/s | ops/s ≥250 | 7.656 MiB | 0.990421 | target; absolute target |
| `truncate-tail-2k-ops-10` | Truncate 2 KiB at tail, 10 operations | direct-posix | 10 | 3 | 0.870 (0.731–1.049) | 13.375 (10.860–16.810) | 2.093 (1.844–2.438) | 26.729 (23.647–30.519) | 1,149 ops/s | ops/s ≥250 | 7.672 MiB | 0.881238 | target; absolute target |
| `truncate-tail-2k-ops-100` | Truncate 2 KiB at tail, 100 operations | direct-posix | 100 | 3 | 0.852 (0.790–0.873) | 89.345 (83.857–90.916) | 2.557 (2.324–2.965) | 103.913 (99.420–105.977) | 1,174 ops/s | ops/s ≥250 | 7.703 MiB | 0.955848 | target; absolute target |

Interpretation: the maximum directional ratio is `1.096620770`
for `delete-middle-2k-ops-100`: tolerated-pass, below the `1.10` no-go
boundary. Directional target is `<=1.05`; `>1.05` through `1.10` is tolerated
only with phase/counter disposition; `>1.10` is no-go. The under-2 ms
local-step exception can explain a noisy create/end phase but never exempts the
complete lifecycle ratio. The 256 KiB temp-copy mutation gate is strict
`median(inner_edit_ns) <= operation_count * 10,000,000 ns`, with no tolerance band.
Copied MiB/s is secondary. Direct-POSIX append/truncate/sparse rows retain their
operations/s gates. All absolute classifications are target-pass.

## Count-changing 1/10/100 MiB scaling supplement

| Operation | Fixture | N | Mutation ms | Workload ms | Commit/visibility ms | Lifecycle ms | Copied / read / written bytes | Copied MiB/s | User + system CPU median | RSS / cgroup peak / spool medians | Swap / OOM | Scaling gate |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| delete 2 KiB | 1 MiB | 3 | 8.342 (8.030–8.414) | 12.207 (11.886–13.605) | 3.438 (3.229–3.499) | 29.653 (27.035–30.230) | 1,046,528 / 1,048,576 / 1,046,528 | 119.635 | 0.012 s | 11.359 / 6.996 / 0.998 MiB | 0 / false | 100/10=1.257939; target |
| delete 2 KiB | 10 MiB | 3 | 34.856 (34.611–58.978) | 40.241 (38.208–65.528) | 18.912 (18.514–21.436) | 73.692 (68.028–101.071) | 10,483,712 / 10,485,760 / 10,483,712 | 286.836 | 0.064 s | 19.797 / 21.020 / 9.998 MiB | 0 / false | 100/10=1.257939; target |
| delete 2 KiB | 100 MiB | 3 | 277.140 (275.336–298.493) | 285.190 (283.880–305.523) | 164.120 (156.956–164.557) | 463.318 (456.855–483.589) | 104,855,552 / 104,857,600 / 104,855,552 | 360.822 | 0.565 s | 22.844 / 111.523 / 99.998 MiB | 0 / false | 100/10=1.257939; target |
| shrink 4 KiB→2 KiB | 1 MiB | 3 | 9.994 (8.709–10.323) | 15.525 (12.535–17.351) | 3.269 (2.917–3.522) | 30.223 (27.670–33.553) | 1,044,480 / 1,048,576 / 1,046,528 | 99.667 | 0.013 s | 11.203 / 7.273 / 0.998 MiB | 0 / false | 100/10=1.205768; target |
| shrink 4 KiB→2 KiB | 10 MiB | 3 | 36.013 (32.306–37.432) | 39.450 (37.985–42.429) | 19.028 (18.181–19.262) | 70.500 (70.476–76.719) | 10,481,664 / 10,485,760 / 10,483,712 | 277.573 | 0.064 s | 17.797 / 21.008 / 9.998 MiB | 0 / false | 100/10=1.205768; target |
| shrink 4 KiB→2 KiB | 100 MiB | 3 | 298.774 (280.519–325.951) | 306.473 (288.146–335.013) | 163.557 (157.433–166.808) | 488.006 (466.777–504.487) | 104,853,504 / 104,857,600 / 104,855,552 | 334.688 | 0.565 s | 21.781 / 111.715 / 99.998 MiB | 0 / false | 100/10=1.205768; target |

Commit-side counters (median and range where a range is shown):

| Operation | Fixture | CDC bytes | Old payload read | Candidate objects | Candidate bytes | Inserted objects / bytes | Reused objects / bytes |
| --- | --- | --- | --- | --- | --- | --- | --- |
| delete 2 KiB | 1 MiB | 1,046,528 (1,046,528–1,046,528) | 0 | 19 (19–19) | 66,499 (66,499–66,499) | 17 / 31,997 | 2 / 34,502 |
| delete 2 KiB | 10 MiB | 10,483,712 (10,483,712–10,483,712) | 0 | 23 (23–23) | 85,369 (85,369–85,369) | 19 / 40,539 | 4 / 44,830 |
| delete 2 KiB | 100 MiB | 104,855,552 (104,855,552–104,855,552) | 0 | 23 (23–23) | 90,555 (90,555–90,555) | 19 / 45,725 | 4 / 44,830 |
| shrink 4 KiB→2 KiB | 1 MiB | 1,046,528 (1,046,528–1,046,528) | 0 | 19 (19–19) | 66,499 (66,499–66,499) | 17 / 31,997 | 2 / 34,502 |
| shrink 4 KiB→2 KiB | 10 MiB | 10,483,712 (10,483,712–10,483,712) | 0 | 23 (23–23) | 85,369 (85,369–85,369) | 19 / 40,539 | 4 / 44,830 |
| shrink 4 KiB→2 KiB | 100 MiB | 104,855,552 (104,855,552–104,855,552) | 0 | 23 (23–23) | 90,555 (90,555–90,555) | 19 / 45,725 | 4 / 44,830 |

Interpretation: delete sustains `369.581` MiB/s
in the diagnostic linear fit and its 100/10 rate ratio is `1.257939`.
Shrink sustains `342.698` MiB/s
and its ratio is `1.205768`.
Both exceed the `0.90` floor. The 1 MiB rows have no absolute throughput
target. This supplement measures periodic destructive middle-edit suffix
relocation through FUSE/temp-copy; it does not claim near-size-independent
owner-side structural editing, CDC uniqueness, or ObjectId generalization.
`container_memory_peak_bytes` is the daemon cgroup's lifetime high-water while
that container exists, not an independent per-row process peak.

## Store footprint controls

| Control | Description | N | Init s | Commit ms | Reopen ms | Lifecycle s | Canonical bytes | Durable median (range) | Amplification | Verifier phase | Max perf RSS | Disposition |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| `store-footprint-large-object-500m` | 500 MB large-object Store footprint | 3 | 0.930 (0.914–0.977) | 3.687 (3.483–3.736) | 131.422 (129.583–132.065) | 1.417 (1.244–1.694) | 501,649,815 | 596,377,600 (596,377,600–596,377,600) | 1.188832× | 4.097 s | 59.047 MiB | explanatory baseline complete |
| `store-footprint-metadata-cardinality-100000` | 100,000-file metadata-cardinality Store footprint | 3 | 5.022 (4.998–5.053) | 4.746 (3.967–5.195) | 409.774 (402.194–413.475) | 5.841 (5.810–5.868) | 579,605,932 | 733,544,448 (732,430,336–735,051,776) | 1.265592× | 63.356 s | 121.875 MiB | explanatory baseline complete |
| `store-footprint-unique-100000` | 100,000-file unique-content Store footprint | 3 | 4.456 (3.600–4.481) | 3.707 (3.548–4.080) | 155.215 (137.612–159.329) | 4.842 (4.108–5.212) | 542,909,962 | 662,831,104 (662,634,496–663,158,784) | 1.220886× | 41.986 s | 104.094 MiB | no-go: >600,000,000 B |

Interpretation: the primary unique-content Store uses `662,831,104` bytes,
`62,831,104` above the `600,000,000`-byte goal, so the exact
patch-compatible result remains a recorded blocker rather than a fabricated
pass. Metadata-cardinality and large-object rows are explanatory controls.
Performance lifecycle includes initialization, one edit, Commit, end, reconnect,
and reopen; full tree digest is verifier-only. Store verifier cgroup peaks are
also shared-daemon lifetime high-waters and are not per-sample process RSS.

## Family walls and resources

| Family | Performance N | Measured lifecycle total s | Performance external wall s | Control wall s | Verification wall s | Recorded command wall s | Peak RSS / cgroup / edit spool or Store temporary disk | Status |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| Owner timing supplement | 9 | 0.238 | not recorded as one wrapper | — | separate conformance below | — | 10.984 / 6.242 / 0 MiB | pass |
| Same-count | 84 | 2.879 | 86.658 | 0 | 2.716 | 95.046 | 74.156 / 6.352 / 0.391 MiB | target-pass |
| Count-changing | 168 | 18.741 | 147.406 | 33.278 | 38.779 | 230.224 | 37.844 / 111.824 / 99.998 MiB | tolerated-pass |
| Store | 9 | 36.035 | 39.212 | — | 123.163 | not recorded (component sum 162.375) | 217.422 / 957.504 / 6.165 MiB | baseline complete; footprint no-go |

Interpretation: measured lifecycle totals sum product intervals only. External
walls include daemon startup/shutdown, supervisors, and evidence handling; they
must not be presented as product latency. Count-changing performance wall includes
the 150 primary and 18 scaling rows. Resource columns report maxima, while cgroup
values retain the lifetime-high-water semantics described above.

## Verification walls

| Verification group | Setup/init | Verification phase | Complete lifecycle | External wall | Timeout/classification boundary | Status |
| --- | --- | --- | --- | --- | --- | --- |
| Universal conformance (9 groups) | — | — | — | 38.307 | group commands; separate from product timing | pass |
| Same-count: 6 proofs + 1 timing/status | 6.882 ms | 1.530 | — | 2.716 | 20 s | target-pass |
| Count-changing primary (7 receipts) | 0.623 | 1.667 | — | 9.624 | 40 s per verifier | target-pass |
| Count-changing scaling (18 receipts) | 2.391 | 10.355 | — | 29.155 | 40 s per verifier | target-pass |
| store-footprint-unique-100000 | 3.778 | 41.986 | 46.572 | 46.952 | 60 s target / 66 s tolerated phase; 90 s process | target-pass |
| store-footprint-metadata-cardinality-100000 | 5.345 | 63.356 | 69.403 | 69.756 | 60 s target / 66 s tolerated phase; 90 s process | tolerated-pass |
| store-footprint-large-object-500m | 0.966 | 4.097 | 6.105 | 6.455 | 60 s target / 66 s tolerated phase; 90 s process | target-pass |

Interpretation: setup/init, verification work, complete lifecycle, and external
wall are distinct. In particular, Store metadata verification is
`63.356 s`
and therefore tolerated under the 60/66-second phase policy; its longer external
wall is not compared with that phase gate. Count-changing exactness uses fresh
Store/Client reconnect, FUSE reopen, independent byte oracle, observed/expected
digest, and committed/reopened/canonical root equality.

## Historical and diagnostic evidence (not release-authorizing)

| Evidence | Recorded status | Why nonterminal | Disposition |
| --- | --- | --- | --- |
| `final-v012-count-changing-f6a4d987` | sealed no-go | maximum directional ratio 1.106301 exceeded 1.10; no verifier ran | superseded by exact `c6c14d5a` pass |
| `final-v012-count-changing-a5322303` | sealed resource failure | 100 MiB cgroup peak exceeded 128 MiB before direct-I/O policy | retained failure evidence |
| `final-v012-same-count-f6a4d987` | pass | older source/harness identity | superseded by exact `c6c14d5a` run |
| `final-v012-issue14-performance-c6c14d5a-r2` | measurements pass | command/nested-custody packaging incomplete | superseded without rerunning measurements by sealed r3 package |
| all `dev-*` focused runs | diagnostic only | dirty-source hypothesis tests | never admission-eligible |

No failed or diagnostic run is pooled with the final distributions, and no valid
outlier is discarded. Selected performance/verify commands are non-admission
diagnostics; same-count A/A is repeatability evidence, not a product-improvement
claim. Issue #18 remains the deferred physical-pack path for the Store blocker.

## Reproduce this report

```bash
python3 release-notes/0.1.2/generate_benchmark_tables.py --check
# Expensive: rehash every sealed file, including all Store databases.
python3 release-notes/0.1.2/generate_benchmark_tables.py --check --verify-all
```

All displayed milliseconds and seconds are rounded to three decimals; raw JSONL
retains integer nanoseconds. Byte counts and ratios are computed before display
rounding.
