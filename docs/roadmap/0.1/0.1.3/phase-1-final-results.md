# Phase 1 final performance report

**Status: closed by explicit user withdrawal of verification. The original verification terminal gate is not claimed to have passed.**

[Verification withdrawal and design accountability](phase-1-verification-withdrawal.md). All 370 active performance observations below were independently qualified before withdrawal. Results are unoptimized; suppressed cases and original failures remain preserved. Phase 2 optimization and verification redesign are separate work.

| Case | Samples | Product median ms | Preparation median s | Whole sample median s | Actual sources |
|---|---:|---:|---:|---:|---|
| `agent-episodes-1` | 3 | 57.948 | 2.610 | 3.047 | e0922904 |
| `agent-episodes-10` | 3 | 292.278 | 2.575 | 3.287 | e0922904 |
| `agent-episodes-100` | 3 | 2120.955 | 2.599 | 5.259 | e0922904 |
| `agent-episodes-500` | 3 | 10045.371 | 2.589 | 13.081 | e0922904 |
| `append-tail-4k-input-524283904b-result-500mib-ops-1-capped-v1` | 5 | 81.731 | 0.912 | 1.369 | 7948df2d |
| `dedup-cdc-common-body-1` | 3 | 4.953 | 3.860 | 4.211 | 7948df2d |
| `dedup-cdc-common-body-10` | 3 | 21.993 | 3.958 | 4.321 | 7948df2d |
| `dedup-cdc-common-body-100` | 3 | 268.495 | 4.941 | 5.642 | 7948df2d |
| `dedup-cdc-common-body-500` | 3 | 1300.897 | 11.517 | 13.817 | 7948df2d |
| `dedup-cdc-delete-1` | 3 | 5.027 | 3.836 | 4.193 | 7948df2d |
| `dedup-cdc-delete-10` | 3 | 19.997 | 3.941 | 4.310 | 7948df2d |
| `dedup-cdc-delete-100` | 3 | 169.896 | 4.095 | 4.643 | 7948df2d |
| `dedup-cdc-delete-500` | 3 | 910.435 | 8.867 | 10.264 | 7948df2d, e24a3b34 |
| `dedup-cdc-insert-1` | 3 | 5.150 | 3.781 | 4.109 | 7948df2d |
| `dedup-cdc-insert-10` | 3 | 20.371 | 2.361 | 2.710 | 7948df2d |
| `dedup-cdc-insert-100` | 3 | 171.250 | 5.871 | 6.429 | 7948df2d |
| `dedup-cdc-insert-500` | 3 | 915.639 | 11.594 | 12.969 | 7948df2d |
| `dedup-cdc-overwrite-1` | 3 | 4.834 | 5.029 | 5.390 | 7948df2d |
| `dedup-cdc-overwrite-10` | 3 | 19.598 | 3.765 | 4.117 | 7948df2d |
| `dedup-cdc-overwrite-100` | 3 | 170.187 | 4.770 | 5.334 | 7948df2d |
| `dedup-cdc-overwrite-500` | 3 | 895.133 | 8.234 | 9.591 | 7948df2d |
| `dedup-cdc-scattered-1` | 3 | 5.955 | 5.741 | 6.108 | 7948df2d |
| `dedup-cdc-scattered-10` | 3 | 56.221 | 4.002 | 4.399 | 7948df2d |
| `dedup-cdc-scattered-100` | 3 | 289.743 | 5.025 | 5.967 | 7948df2d |
| `dedup-cdc-scattered-500` | 3 | 1681.869 | 8.105 | 10.705 | 7948df2d |
| `dedup-cross-file-anchor-1` | 3 | 4.171 | 3.497 | 3.823 | 7948df2d |
| `dedup-cross-file-identical-10` | 3 | 17.702 | 3.751 | 4.199 | 7948df2d |
| `dedup-cross-file-identical-100` | 3 | 156.959 | 5.688 | 6.187 | 7948df2d |
| `dedup-cross-file-identical-500` | 3 | 808.296 | 12.257 | 13.554 | 7948df2d |
| `dedup-cross-file-mixed-10` | 3 | 23.254 | 4.085 | 4.493 | 7948df2d |
| `dedup-cross-file-mixed-100` | 3 | 203.634 | 7.765 | 8.385 | 7948df2d |
| `dedup-cross-file-mixed-500` | 3 | 1056.044 | 11.729 | 13.636 | 7948df2d |
| `dedup-cross-file-unique-10` | 3 | 21.058 | 3.749 | 4.099 | 7948df2d |
| `dedup-cross-file-unique-100` | 3 | 176.675 | 4.609 | 5.183 | 7948df2d |
| `dedup-cross-file-unique-500` | 3 | 941.368 | 8.322 | 10.672 | 7948df2d |
| `dedup-history-distributed-1` | 3 | 18.987 | 3.811 | 4.218 | 7948df2d |
| `dedup-history-distributed-10` | 3 | 57.229 | 0.512 | 1.102 | 7948df2d |
| `dedup-history-distributed-100` | 3 | 1421.546 | 0.515 | 4.048 | 7948df2d |
| `dedup-history-hotset-1` | 3 | 18.270 | 0.483 | 0.891 | 7948df2d |
| `dedup-history-hotset-10` | 3 | 198.303 | 0.519 | 1.247 | 7948df2d |
| `dedup-history-hotset-100` | 3 | 2331.693 | 0.502 | 4.854 | 7948df2d |
| `dedup-history-hotset-500` | 3 | 12519.133 | 0.494 | 21.460 | 7948df2d |
| `dedup-history-metadata-1` | 3 | 23.566 | 2.353 | 2.769 | 30d13dee |
| `dedup-history-metadata-10` | 3 | 90.263 | 2.389 | 2.969 | 30d13dee |
| `dedup-history-metadata-100` | 3 | 755.511 | 2.376 | 5.129 | 30d13dee |
| `dedup-history-metadata-500` | 3 | 3768.286 | 2.368 | 14.415 | 30d13dee |
| `dedup-history-recurring-1` | 3 | 18.588 | 0.499 | 0.888 | 7948df2d |
| `dedup-history-recurring-10` | 3 | 52.336 | 0.511 | 1.094 | 7948df2d |
| `dedup-history-recurring-100` | 3 | 412.714 | 0.478 | 2.831 | 7948df2d |
| `dedup-history-recurring-500` | 3 | 2518.989 | 0.486 | 11.316 | 7948df2d |
| `dedup-history-unrelated-1` | 3 | 977.117 | 0.480 | 1.877 | 7948df2d |
| `dedup-history-unrelated-10` | 3 | 7527.608 | 0.489 | 8.637 | 7948df2d |
| `dedup-workspace-exact-1` | 3 | 55.700 | 6.026 | 6.994 | 7948df2d |
| `dedup-workspace-exact-10` | 3 | 108.581 | 0.628 | 1.107 | 7948df2d |
| `dedup-workspace-exact-100` | 3 | 765.595 | 0.573 | 1.758 | 7948df2d |
| `dedup-workspace-exact-500` | 3 | 3167.685 | 0.570 | 4.221 | 7948df2d |
| `dedup-workspace-local-1` | 3 | 30.605 | 0.596 | 0.999 | 7948df2d |
| `dedup-workspace-local-10` | 3 | 94.893 | 0.578 | 1.056 | 7948df2d |
| `dedup-workspace-local-100` | 3 | 710.889 | 0.623 | 1.774 | 7948df2d |
| `dedup-workspace-local-500` | 3 | 3070.117 | 0.583 | 4.094 | 7948df2d |
| `dedup-workspace-unique-1` | 3 | 35.280 | 0.592 | 0.999 | 7948df2d |
| `dedup-workspace-unique-10` | 3 | 101.143 | 0.582 | 1.085 | 7948df2d |
| `dedup-workspace-unique-100` | 3 | 726.023 | 0.587 | 1.714 | 7948df2d |
| `dedup-workspace-unique-500` | 3 | 3847.812 | 0.594 | 5.249 | 7948df2d |
| `directory-construct-1` | 3 | 145.402 | 4.445 | 4.954 | 7948df2d |
| `directory-construct-10` | 3 | 155.081 | 4.474 | 5.036 | 7948df2d |
| `directory-construct-100` | 3 | 1071.433 | 4.406 | 6.022 | 7948df2d |
| `directory-construct-500` | 3 | 5253.349 | 4.462 | 10.234 | 7948df2d |
| `directory-content-scan-1` | 3 | 275.933 | 2.345 | 3.011 | 30d13dee |
| `directory-content-scan-10` | 3 | 1803.143 | 2.415 | 4.683 | 30d13dee |
| `directory-metadata-scan-1` | 3 | 103.801 | 2.366 | 2.849 | 7948df2d |
| `directory-metadata-scan-10` | 3 | 215.148 | 2.382 | 2.963 | 7948df2d |
| `directory-metadata-scan-100` | 3 | 1650.183 | 3.533 | 5.599 | 7948df2d |
| `directory-metadata-scan-500` | 3 | 9039.163 | 4.724 | 14.236 | 7948df2d |
| `insert-middle-4k-input-524283904b-result-500mib-ops-1-capped-v1` | 5 | 82.696 | 0.926 | 1.395 | 7948df2d |
| `namespace-subtree-relocate-delete-1` | 3 | 176.534 | 3.461 | 4.037 | e0922904 |
| `namespace-subtree-relocate-delete-10` | 3 | 932.065 | 3.393 | 4.808 | e0922904 |
| `namespace-subtree-relocate-delete-100` | 3 | 8247.964 | 3.674 | 12.387 | e0922904 |
| `payload-create-100m` | 3 | 475.692 | 0.473 | 1.356 | 7948df2d |
| `payload-create-10m` | 3 | 83.486 | 0.483 | 0.935 | 7948df2d |
| `payload-create-1m` | 3 | 27.125 | 0.494 | 0.899 | 7948df2d |
| `payload-create-500m` | 3 | 3744.340 | 0.534 | 4.866 | 7948df2d |
| `payload-random-read-1` | 3 | 25.880 | 3.159 | 3.560 | 30d13dee |
| `payload-random-read-10` | 3 | 82.473 | 3.165 | 3.632 | 30d13dee |
| `payload-random-read-100` | 3 | 612.392 | 3.217 | 4.291 | 30d13dee |
| `payload-random-read-500` | 3 | 2772.245 | 3.198 | 6.449 | 30d13dee |
| `prepend-head-4k-input-524283904b-result-500mib-ops-1-capped-v1` | 5 | 74.250 | 0.882 | 1.335 | 7948df2d |
| `replace-grow-middle-2k-to-4k-input-524285952b-result-500mib-ops-1-capped-v1` | 5 | 71.015 | 0.894 | 1.338 | 7948df2d |
| `tiny-bulk-create-1` | 3 | 489.119 | 2.882 | 3.819 | 7948df2d |
| `tiny-bulk-create-10` | 3 | 2995.265 | 0.480 | 3.939 | 7948df2d |
| `tiny-bulk-delete-1` | 3 | 201.528 | 2.945 | 3.512 | 7948df2d |
| `tiny-bulk-delete-10` | 3 | 749.703 | 2.938 | 4.174 | 7948df2d |
| `tiny-bulk-delete-100` | 3 | 7627.604 | 3.284 | 11.400 | 7948df2d |
| `tiny-create-1` | 3 | 78.514 | 4.753 | 5.233 | 7948df2d |
| `tiny-create-10` | 3 | 70.990 | 1.528 | 1.986 | 7948df2d |
| `tiny-create-100` | 3 | 272.562 | 1.505 | 2.192 | 7948df2d |
| `tiny-create-500` | 3 | 1093.988 | 1.521 | 3.101 | 7948df2d |
| `tiny-stat-1` | 3 | 25.590 | 4.507 | 4.897 | 7948df2d |
| `tiny-stat-10` | 3 | 32.041 | 1.547 | 1.954 | 7948df2d |
| `tiny-stat-100` | 3 | 82.107 | 1.498 | 1.958 | 7948df2d |
| `tiny-stat-500` | 3 | 283.434 | 1.521 | 2.175 | 7948df2d |
| `tiny-unlink-1` | 3 | 120.322 | 1.497 | 1.999 | 7948df2d |
| `tiny-unlink-10` | 3 | 81.584 | 1.508 | 1.983 | 7948df2d |
| `tiny-unlink-100` | 3 | 209.665 | 1.501 | 2.102 | 7948df2d |
| `tiny-unlink-500` | 3 | 687.066 | 1.539 | 2.687 | 7948df2d |
| `workspace-clean-commit-1` | 3 | 15.077 | 2.986 | 3.366 | 7948df2d |
| `workspace-clean-commit-10` | 3 | 15.095 | 2.963 | 3.346 | 7948df2d |
| `workspace-clean-commit-100` | 3 | 14.234 | 3.345 | 3.756 | 7948df2d |
| `workspace-clean-commit-500` | 3 | 14.697 | 4.410 | 4.798 | 7948df2d |
| `workspace-dense-rewrite-1` | 3 | 670.320 | 1.529 | 2.715 | 7948df2d |
| `workspace-dense-rewrite-10` | 3 | 6103.153 | 1.557 | 8.319 | 7948df2d |
| `workspace-distributed-sdk-edit-1` | 3 | 64.033 | 1.508 | 1.955 | 7948df2d |
| `workspace-distributed-sdk-edit-10` | 3 | 88.606 | 1.508 | 1.981 | 7948df2d |
| `workspace-distributed-sdk-edit-100` | 3 | 279.633 | 1.515 | 2.208 | 7948df2d |
| `workspace-distributed-sdk-edit-500` | 3 | 1095.142 | 1.503 | 3.071 | 7948df2d |
| `workspace-fixed-move-1` | 3 | 26.345 | 2.379 | 2.800 | e0922904 |
| `workspace-fixed-move-10` | 3 | 32.639 | 2.372 | 2.799 | e0922904 |
| `workspace-fixed-move-100` | 3 | 52.789 | 2.651 | 3.096 | e0922904 |
| `workspace-fixed-move-500` | 3 | 188.365 | 3.956 | 4.526 | e0922904 |
| `zero-extend-tail-4k-input-524283904b-result-500mib-ops-1-capped-v1` | 5 | 80.156 | 0.900 | 1.342 | 7948df2d |

Product medians use the recorded public-call sum; import initialization and missing metrics remain represented by their original row details. Different source identities are shown explicitly; exact selection/compatibility and full metrics are retained in the closeout JSON and prior qualified checkpoints.

## Suppressed cases

- `dedup-history-distributed-500`
- `dedup-history-unrelated-100`
- `dedup-history-unrelated-500`
- `directory-content-scan-100`
- `directory-content-scan-500`
- `git-tool-1`
- `git-tool-10`
- `git-tool-100`
- `git-tool-500`
- `namespace-subtree-relocate-delete-500`
- `tiny-bulk-create-100`
- `tiny-bulk-create-500`
- `tiny-bulk-delete-500`
- `workspace-dense-rewrite-100`
- `workspace-dense-rewrite-500`

Suppression is not PASS. The directory-content-scan-100 trigger remains an actual timeout failure; its remaining two seeds were not run. The original CDC deletion seed-3 timing remains recipe-invalid historical evidence, replaced by its corrected source-bound observation.

## Phase 2

Reassess the withdrawn verification design before reuse. Keep performance/storage optimization separate, using [the optimization backlog](phase-2-backlog.md). Central #21 remains open.
