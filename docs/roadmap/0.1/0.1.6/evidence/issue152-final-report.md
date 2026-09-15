# #152 final report — v0.1.6 full registered benchmark suite, 8 groups

Status: **campaign complete.** All 8 groups were collected and their group
reports are posted on #152. Every registered selection is terminal. Two product
defects were found and fixed mid-campaign, which means **the campaign ran under
three candidate identities**; each group records the identity that was in force
when it was collected, and no number is re-labelled.

## 1. Identity chain

| # | groups | commit | source seal | product seal | compilation seal | image |
|---|---|---|---|---|---|---|
| C1 | G1, G2, G3 | `8b5e0955e` | `0debfccbfe56516f…` | `31a42c95197a21c5…` (the issue's frozen candidate) | `79dab102…` | `layerfs-bench-infra:0debfccbfe56516f` |
| C2 | G4, (G5 `mixed_load_bearing`) | `58e4f7f47` | `957eae85366e771a…` | `ce2336b71afe6b04…` | `1d17454b…` | `layerfs-bench-infra:957eae85366e771a` |
| C3 | G5 (`payload_create_read`), G6 | `29835f44d` | `bebc8c9805e2acef…` | `dc2b3a14f45a4eb7…` | `36a30d3c…` | `layerfs-bench-infra:bebc8c9805e2acef` |
| C3′ | G7, G8 | `b9bca593c` | `d1bbf882067d824b…` | `dc2b3a14f45a4eb7…` (**unchanged**) | `2c4ec5ee…` | `layerfs-bench-infra:d1bbf882067d824b` |

Harness identity `daa74be0c3a0b40a824617a6403c70ce4e7be115e7c47f4f1faf26029c55b044` and
workload source `821b240458fe968cec8ec61bf09f1db09e109bf1a3f64fc62e94c449e3c302b8`
were **unchanged for the whole campaign**.

Declared drift from the issue's frozen table: the source seal had already moved
three commits past `1558a121f` before the campaign started, because
`4787e7474`/`35980634e` changed `tools/preflight.sh` — a host-only shell file
that is inside the source seal but outside the Docker context and outside the
harness identity. Product, compilation, harness, workload and image identities
matched the frozen table at the start; C2/C3 are the two in-campaign product
fixes.

## 2. Terminal tally

| outcome | selections |
|---|--:|
| collected performance samples | 196 |
| PASS (comparative + absolute where registered) | 189 |
| FAIL — diagnosed material regression | 6 |
| FAIL — registered absolute target missed, owner-waived | 1 (`namespace-100000` 2.7 s cold Init) |
| REUSED-FROM (owner-banked B1/B2/B3, cited not re-run) | 3 |
| NOT_RUN_OPTIONAL — superseded capped-v1 duplicates not admitted to host execution | 5 |
| NOT_RUN — sealed v2 full157 Store unavailable | 11 performance + 11 proofs |
| NOT_RUN_OPTIONAL — `repository_history` optional profiles | 3 |
| NOT_RUN_OPTIONAL — `workspace-sustained-600s` (separately accounted optional long test) | 1 |
| FAIL — diagnosed fault-injection proofs not migrated to the sandbox route | 6 |

Registered performance selections: 215 (`fs-benchmark-pro infra-list`, authoritative).
Registered proof-only selections: `dedup-cdc-boundaries-proof` (PASS) and
`workspace_reliability` (28) = 29, plus `historical_access`'s 11 proofs.

**Nothing was dropped, nothing was silently promoted, and no failing cell was
omitted from a group report.**

## 3. Family → per-test

196 collected cells. `REUSED-FROM` rows (B1/B2/B3) and the non-collected
terminal rows follow the table.

| family | selection | candidate | v0.1.5 | ratio | Δ abs | disposition | proof |
|---|---|--:|--:|--:|--:|---|---|
| `init_namespace` | `namespace-100-compact-v3` | 31.67 ms | 28.70 ms | 1.10× | +2.96 ms | PASS | PASS |
| `init_namespace` | `namespace-1000-compact-v3` | 130.37 ms | 112.73 ms | 1.16× | +17.63 ms | PASS | PASS |
| `init_namespace` | `namespace-10000` | 1.101 s | 1.020 s | 1.08× | +80.29 ms | PASS | PASS |
| `init_namespace` | `namespace-100000` | 4.986 s | 4.398 s | 1.13× | +588.61 ms | TARGET_MISS | PASS |
| `edit_length_preserving` | `overwrite-head-4k-on-1mib-ops-1` | 6.31 ms | 10.95 ms | 0.58× | -4.63 ms | PASS | PASS |
| `edit_length_preserving` | `overwrite-middle-4k-on-1mib-ops-1` | 5.63 ms | 9.74 ms | 0.58× | -4.11 ms | PASS | PASS |
| `edit_length_preserving` | `overwrite-tail-4k-on-1mib-ops-1` | 4.72 ms | 8.39 ms | 0.56× | -3.66 ms | PASS | PASS |
| `edit_length_preserving` | `overwrite-head-4k-on-10mib-ops-1` | 5.90 ms | 11.79 ms | 0.50× | -5.90 ms | PASS | PASS |
| `edit_length_preserving` | `overwrite-middle-4k-on-10mib-ops-1` | 5.70 ms | 8.17 ms | 0.70× | -2.47 ms | PASS | PASS |
| `edit_length_preserving` | `overwrite-tail-4k-on-10mib-ops-1` | 6.06 ms | 8.57 ms | 0.71× | -2.51 ms | PASS | PASS |
| `edit_length_preserving` | `overwrite-head-4k-on-100mib-ops-1` | 5.81 ms | 7.88 ms | 0.74× | -2.07 ms | PASS | PASS |
| `edit_length_preserving` | `overwrite-middle-4k-on-100mib-ops-1` | 6.43 ms | 8.83 ms | 0.73× | -2.40 ms | PASS | PASS |
| `edit_length_preserving` | `overwrite-tail-4k-on-100mib-ops-1` | 5.82 ms | 9.80 ms | 0.59× | -3.98 ms | PASS | PASS |
| `edit_length_preserving` | `overwrite-head-4k-on-500mib-ops-1` | 7.34 ms | 8.78 ms | 0.84× | -1.45 ms | PASS | PASS |
| `edit_length_preserving` | `overwrite-middle-4k-on-500mib-ops-1` | 10.01 ms | 11.18 ms | 0.90× | -1.17 ms | PASS | PASS |
| `edit_length_preserving` | `overwrite-tail-4k-on-500mib-ops-1` | 7.37 ms | 11.80 ms | 0.62× | -4.43 ms | PASS | PASS |
| `edit_canonical_chunk_count` | `overwrite-fixed-64k-chunk-count-decrease-on-1mib-ops-1` | 6.02 ms | 7.59 ms | 0.79× | -1.57 ms | PASS | PASS |
| `edit_canonical_chunk_count` | `overwrite-fixed-64k-chunk-count-increase-on-1mib-ops-1` | 6.84 ms | 8.43 ms | 0.81× | -1.59 ms | PASS | PASS |
| `edit_canonical_chunk_count` | `overwrite-fixed-64k-chunk-count-preserve-on-1mib-ops-1` | 6.20 ms | 11.02 ms | 0.56× | -4.82 ms | PASS | PASS |
| `edit_canonical_chunk_count` | `overwrite-fixed-64k-chunk-count-decrease-on-10mib-ops-1` | 6.63 ms | 11.77 ms | 0.56× | -5.14 ms | PASS | PASS |
| `edit_canonical_chunk_count` | `overwrite-fixed-64k-chunk-count-increase-on-10mib-ops-1` | 6.90 ms | 10.31 ms | 0.67× | -3.41 ms | PASS | PASS |
| `edit_canonical_chunk_count` | `overwrite-fixed-64k-chunk-count-preserve-on-10mib-ops-1` | 7.24 ms | 13.05 ms | 0.55× | -5.81 ms | PASS | PASS |
| `edit_canonical_chunk_count` | `overwrite-fixed-64k-chunk-count-decrease-on-100mib-ops-1` | 13.54 ms | 10.91 ms | 1.24× | +2.63 ms | PASS | PASS |
| `edit_canonical_chunk_count` | `overwrite-fixed-64k-chunk-count-increase-on-100mib-ops-1` | 8.01 ms | 9.54 ms | 0.84× | -1.54 ms | PASS | PASS |
| `edit_canonical_chunk_count` | `overwrite-fixed-64k-chunk-count-preserve-on-100mib-ops-1` | 7.46 ms | 9.27 ms | 0.80× | -1.81 ms | PASS | PASS |
| `edit_canonical_chunk_count` | `overwrite-fixed-64k-chunk-count-decrease-on-500mib-ops-1` | 7.98 ms | 15.38 ms | 0.52× | -7.40 ms | PASS | PASS |
| `edit_canonical_chunk_count` | `overwrite-fixed-64k-chunk-count-increase-on-500mib-ops-1` | 9.27 ms | 12.26 ms | 0.76× | -2.99 ms | PASS | PASS |
| `edit_canonical_chunk_count` | `overwrite-fixed-64k-chunk-count-preserve-on-500mib-ops-1` | 8.13 ms | 10.04 ms | 0.81× | -1.91 ms | PASS | PASS |
| `edit_length_changing` | `append-tail-4k-on-1mib-ops-1` | 5.76 ms | 9.70 ms | 0.59× | -3.94 ms | PASS | PASS |
| `edit_length_changing` | `delete-middle-4k-on-1mib-ops-1` | 5.06 ms | 7.75 ms | 0.65× | -2.69 ms | PASS | PASS |
| `edit_length_changing` | `insert-middle-4k-on-1mib-ops-1` | 5.30 ms | 8.43 ms | 0.63× | -3.13 ms | PASS | PASS |
| `edit_length_changing` | `prepend-head-4k-on-1mib-ops-1` | 5.12 ms | 8.98 ms | 0.57× | -3.86 ms | PASS | PASS |
| `edit_length_changing` | `replace-grow-middle-2k-to-4k-on-1mib-ops-1` | 5.75 ms | 8.49 ms | 0.68× | -2.74 ms | PASS | PASS |
| `edit_length_changing` | `replace-shrink-middle-4k-to-2k-on-1mib-ops-1` | 5.30 ms | 7.82 ms | 0.68× | -2.52 ms | PASS | PASS |
| `edit_length_changing` | `truncate-tail-4k-on-1mib-ops-1` | 5.03 ms | 10.07 ms | 0.50× | -5.03 ms | PASS | PASS |
| `edit_length_changing` | `zero-extend-tail-4k-on-1mib-ops-1` | 4.94 ms | 7.73 ms | 0.64× | -2.79 ms | PASS | PASS |
| `edit_length_changing` | `append-tail-4k-on-10mib-ops-1` | 5.71 ms | 9.63 ms | 0.59× | -3.91 ms | PASS | PASS |
| `edit_length_changing` | `delete-middle-4k-on-10mib-ops-1` | 5.75 ms | 7.89 ms | 0.73× | -2.14 ms | PASS | PASS |
| `edit_length_changing` | `insert-middle-4k-on-10mib-ops-1` | 6.40 ms | 7.93 ms | 0.81× | -1.53 ms | PASS | PASS |
| `edit_length_changing` | `prepend-head-4k-on-10mib-ops-1` | 5.65 ms | 10.04 ms | 0.56× | -4.39 ms | PASS | PASS |
| `edit_length_changing` | `replace-grow-middle-2k-to-4k-on-10mib-ops-1` | 6.81 ms | 10.04 ms | 0.68× | -3.23 ms | PASS | PASS |
| `edit_length_changing` | `replace-shrink-middle-4k-to-2k-on-10mib-ops-1` | 5.89 ms | 8.33 ms | 0.71× | -2.44 ms | PASS | PASS |
| `edit_length_changing` | `truncate-tail-4k-on-10mib-ops-1` | 9.14 ms | 9.17 ms | 1.00× | -0.03 ms | PASS | PASS |
| `edit_length_changing` | `zero-extend-tail-4k-on-10mib-ops-1` | 5.52 ms | 9.80 ms | 0.56× | -4.29 ms | PASS | PASS |
| `edit_length_changing` | `append-tail-4k-on-100mib-ops-1` | 5.60 ms | 9.34 ms | 0.60× | -3.75 ms | PASS | PASS |
| `edit_length_changing` | `delete-middle-4k-on-100mib-ops-1` | 6.41 ms | 10.02 ms | 0.64× | -3.62 ms | PASS | PASS |
| `edit_length_changing` | `insert-middle-4k-on-100mib-ops-1` | 6.25 ms | 8.65 ms | 0.72× | -2.40 ms | PASS | PASS |
| `edit_length_changing` | `prepend-head-4k-on-100mib-ops-1` | 6.54 ms | 10.45 ms | 0.63× | -3.91 ms | PASS | PASS |
| `edit_length_changing` | `replace-grow-middle-2k-to-4k-on-100mib-ops-1` | 8.44 ms | 8.43 ms | 1.00× | +0.01 ms | PASS | PASS |
| `edit_length_changing` | `replace-shrink-middle-4k-to-2k-on-100mib-ops-1` | 7.13 ms | 8.48 ms | 0.84× | -1.35 ms | PASS | PASS |
| `edit_length_changing` | `truncate-tail-4k-on-100mib-ops-1` | 5.44 ms | 9.39 ms | 0.58× | -3.95 ms | PASS | PASS |
| `edit_length_changing` | `zero-extend-tail-4k-on-100mib-ops-1` | 5.76 ms | 9.45 ms | 0.61× | -3.69 ms | PASS | PASS |
| `edit_length_changing` | `append-tail-4k-on-500mib-result-capped-v2-ops-1` | 6.40 ms | 10.98 ms | 0.58× | -4.58 ms | PASS | PASS |
| `edit_length_changing` | `delete-middle-4k-on-500mib-ops-1` | 12.16 ms | 12.78 ms | 0.95× | -0.62 ms | PASS | PASS |
| `edit_length_changing` | `insert-middle-4k-on-500mib-result-capped-v2-ops-1` | 7.76 ms | 9.24 ms | 0.84× | -1.48 ms | PASS | PASS |
| `edit_length_changing` | `prepend-head-4k-on-500mib-result-capped-v2-ops-1` | 6.73 ms | 9.50 ms | 0.71× | -2.77 ms | PASS | PASS |
| `edit_length_changing` | `replace-grow-middle-2k-to-4k-on-500mib-result-capped-v2-ops-1` | 8.63 ms | 9.33 ms | 0.92× | -0.70 ms | PASS | PASS |
| `edit_length_changing` | `replace-shrink-middle-4k-to-2k-on-500mib-ops-1` | 7.35 ms | 13.87 ms | 0.53× | -6.52 ms | PASS | PASS |
| `edit_length_changing` | `truncate-tail-4k-on-500mib-ops-1` | 7.19 ms | 10.89 ms | 0.66× | -3.70 ms | PASS | PASS |
| `edit_length_changing` | `zero-extend-tail-4k-on-500mib-result-capped-v2-ops-1` | 6.50 ms | 10.33 ms | 0.63× | -3.83 ms | PASS | PASS |
| `workspace_change_locality` | `workspace-clean-commit-1-compact-v2` | 17.47 ms | 10.50 ms | 1.66× | +6.98 ms | PASS | PASS |
| `workspace_change_locality` | `workspace-dense-rewrite-1-compact-v2` | 89.23 ms | 109.38 ms | 0.82× | -20.15 ms | PASS | PASS |
| `workspace_change_locality` | `workspace-distributed-sdk-edit-1-compact-v2` | 14.85 ms | 25.27 ms | 0.59× | -10.42 ms | PASS | PASS |
| `workspace_change_locality` | `workspace-fixed-move-1-compact-v2` | 19.92 ms | 26.56 ms | 0.75× | -6.64 ms | PASS | PASS |
| `workspace_change_locality` | `workspace-clean-commit-10-compact-v2` | 19.19 ms | 11.87 ms | 1.62× | +7.32 ms | PASS | PASS |
| `workspace_change_locality` | `workspace-dense-rewrite-10-compact-v2` | 517.87 ms | 583.17 ms | 0.89× | -65.30 ms | PASS | PASS |
| `workspace_change_locality` | `workspace-distributed-sdk-edit-10-compact-v2` | 31.99 ms | 42.31 ms | 0.76× | -10.33 ms | PASS | PASS |
| `workspace_change_locality` | `workspace-fixed-move-10-compact-v2` | 19.13 ms | 28.83 ms | 0.66× | -9.70 ms | PASS | PASS |
| `workspace_change_locality` | `workspace-clean-commit-100-mixed-v4` | 11.94 ms | 15.47 ms | 0.77× | -3.53 ms | PASS | PASS |
| `workspace_change_locality` | `workspace-dense-rewrite-100-mixed-v4` | 2.191 s | 2.894 s | 0.76× | -703.25 ms | PASS | PASS |
| `workspace_change_locality` | `workspace-distributed-sdk-edit-100-mixed-v4` | 185.98 ms | 213.09 ms | 0.87× | -27.11 ms | PASS | PASS |
| `workspace_change_locality` | `workspace-fixed-move-100-mixed-v4` | 29.66 ms | 35.95 ms | 0.83× | -6.29 ms | PASS | PASS |
| `workspace_change_locality` | `workspace-clean-commit-500-mixed-v4` | 9.64 ms | 17.80 ms | 0.54× | -8.15 ms | PASS | PASS |
| `workspace_change_locality` | `workspace-dense-rewrite-500-mixed-v4` | 8.267 s | 10.499 s | 0.79× | -2232.30 ms | PASS | PASS |
| `workspace_change_locality` | `workspace-distributed-sdk-edit-500-mixed-v4` | 638.12 ms | 666.10 ms | 0.96× | -27.98 ms | PASS | PASS |
| `workspace_change_locality` | `workspace-fixed-move-500-mixed-v4` | 32.13 ms | 47.75 ms | 0.67× | -15.62 ms | PASS | PASS |
| `tiny_file_churn` | `tiny-bulk-create-1-compact-v2` | 93.74 ms | 97.12 ms | 0.97× | -3.38 ms | PASS | PASS |
| `tiny_file_churn` | `tiny-bulk-delete-1-compact-v2` | 94.50 ms | 110.07 ms | 0.86× | -15.56 ms | PASS | PASS |
| `tiny_file_churn` | `tiny-create-1-compact-v2` | 18.36 ms | 23.26 ms | 0.79× | -4.90 ms | PASS | PASS |
| `tiny_file_churn` | `tiny-stat-1-compact-v2` | 26.50 ms | 21.81 ms | 1.22× | +4.69 ms | PASS | PASS |
| `tiny_file_churn` | `tiny-unlink-1-compact-v2` | 17.38 ms | 19.56 ms | 0.89× | -2.18 ms | PASS | PASS |
| `tiny_file_churn` | `tiny-bulk-create-10-compact-v2` | 330.64 ms | 316.83 ms | 1.04× | +13.81 ms | PASS | PASS |
| `tiny_file_churn` | `tiny-bulk-delete-10-compact-v2` | 184.29 ms | 209.75 ms | 0.88× | -25.46 ms | PASS | PASS |
| `tiny_file_churn` | `tiny-create-10-compact-v2` | 26.92 ms | 30.16 ms | 0.89× | -3.24 ms | PASS | PASS |
| `tiny_file_churn` | `tiny-stat-10-compact-v2` | 19.95 ms | 28.63 ms | 0.70× | -8.68 ms | PASS | PASS |
| `tiny_file_churn` | `tiny-unlink-10-compact-v2` | 22.71 ms | 28.36 ms | 0.80× | -5.65 ms | PASS | PASS |
| `tiny_file_churn` | `tiny-bulk-create-100-mixed-v3` | 861.46 ms | 1.049 s | 0.82× | -187.34 ms | PASS | PASS |
| `tiny_file_churn` | `tiny-bulk-delete-100-mixed-v3` | 230.11 ms | 306.85 ms | 0.75× | -76.74 ms | PASS | PASS |
| `tiny_file_churn` | `tiny-create-100-mixed-v4` | 61.41 ms | 78.98 ms | 0.78× | -17.57 ms | PASS | PASS |
| `tiny_file_churn` | `tiny-stat-100-mixed-v4` | 34.42 ms | 45.70 ms | 0.75× | -11.29 ms | PASS | PASS |
| `tiny_file_churn` | `tiny-unlink-100-mixed-v4` | 55.80 ms | 63.78 ms | 0.87× | -7.99 ms | PASS | PASS |
| `tiny_file_churn` | `tiny-bulk-delete-500-mixed-v3` | 920.06 ms | 1.185 s | 0.78× | -265.27 ms | PASS | PASS |
| `tiny_file_churn` | `tiny-stat-500-mixed-v4` | 56.71 ms | 69.84 ms | 0.81× | -13.13 ms | PASS | PASS |
| `tiny_file_churn` | `tiny-unlink-500-mixed-v4` | 119.97 ms | 119.58 ms | 1.00× | +0.39 ms | PASS | PASS |
| `git_tool_workflow` | `git-tool-1-compact-v2` | 309.19 ms | 337.08 ms | 0.92× | -27.89 ms | PASS | PASS |
| `git_tool_workflow` | `git-tool-10-compact-v2` | 629.59 ms | 705.41 ms | 0.89× | -75.82 ms | PASS | PASS |
| `git_tool_workflow` | `git-tool-100-mixed-v4` | 2.255 s | 2.838 s | 0.79× | -583.61 ms | PASS | PASS |
| `git_tool_workflow` | `git-tool-500-mixed-v4` | 8.246 s | 8.803 s | 0.94× | -556.71 ms | PASS | PASS |
| `namespace_mutation` | `namespace-subtree-relocate-delete-1-compact-v2` | 24.58 ms | 29.55 ms | 0.83× | -4.97 ms | PASS | PASS |
| `namespace_mutation` | `namespace-subtree-relocate-delete-10-compact-v2` | 61.99 ms | 73.48 ms | 0.84× | -11.49 ms | PASS | PASS |
| `namespace_mutation` | `namespace-subtree-relocate-delete-100-mixed-v4` | 68.34 ms | 74.19 ms | 0.92× | -5.85 ms | PASS | PASS |
| `namespace_mutation` | `namespace-subtree-relocate-delete-500-mixed-v4` | 240.79 ms | 274.96 ms | 0.88× | -34.17 ms | PASS | PASS |
| `directory_construction_traversal` | `directory-construct-1-compact-v2` | 19.25 ms | 22.38 ms | 0.86× | -3.13 ms | PASS | PASS |
| `directory_construction_traversal` | `directory-content-scan-1-compact-v2` | 102.68 ms | 105.53 ms | 0.97× | -2.84 ms | PASS | PASS |
| `directory_construction_traversal` | `directory-metadata-scan-1-compact-v2` | 74.72 ms | 87.97 ms | 0.85× | -13.25 ms | PASS | PASS |
| `directory_construction_traversal` | `directory-construct-10-compact-v2` | 43.90 ms | 38.60 ms | 1.14× | +5.30 ms | PASS | PASS |
| `directory_construction_traversal` | `directory-content-scan-10-compact-v2` | 326.77 ms | 348.62 ms | 0.94× | -21.85 ms | PASS | PASS |
| `directory_construction_traversal` | `directory-metadata-scan-10-compact-v2` | 114.06 ms | 137.61 ms | 0.83× | -23.55 ms | PASS | PASS |
| `directory_construction_traversal` | `directory-construct-100-mixed-v4` | 213.05 ms | 253.42 ms | 0.84× | -40.37 ms | PASS | PASS |
| `directory_construction_traversal` | `directory-content-scan-100-mixed-v4` | 1.157 s | 1.231 s | 0.94× | -74.70 ms | PASS | PASS |
| `directory_construction_traversal` | `directory-metadata-scan-100-mixed-v4` | 279.88 ms | 333.03 ms | 0.84× | -53.15 ms | PASS | PASS |
| `directory_construction_traversal` | `directory-construct-500-mixed-v4` | 983.13 ms | 1.140 s | 0.86× | -157.19 ms | PASS | PASS |
| `directory_construction_traversal` | `directory-content-scan-500-mixed-v4` | 4.541 s | 4.434 s | 1.02× | +107.28 ms | PASS | PASS |
| `directory_construction_traversal` | `directory-metadata-scan-500-mixed-v4` | 588.25 ms | 694.66 ms | 0.85× | -106.41 ms | PASS | PASS |
| `mixed_load_bearing` | `agent-episodes-1-compact-v2` | 29.97 ms | 34.38 ms | 0.87× | -4.41 ms | PASS | PASS |
| `mixed_load_bearing` | `agent-episodes-10-compact-v2` | 77.14 ms | 62.13 ms | 1.24× | +15.02 ms | PASS | PASS |
| `mixed_load_bearing` | `agent-episodes-100` | 729.66 ms | 1.030 s | 0.71× | -300.42 ms | PASS | PASS |
| `mixed_load_bearing` | `agent-episodes-500` | 3.653 s | 8.219 s | 0.44× | -4565.86 ms | PASS | PASS |
| `payload_create_read` | `payload-create-1m-compact-v2` | 40.62 ms | 29.33 ms | 1.38× | +11.29 ms | PASS | PASS |
| `payload_create_read` | `payload-random-read-1-compact-v2` | 15.43 ms | 20.46 ms | 0.75× | -5.03 ms | PASS | PASS |
| `payload_create_read` | `payload-create-10m-compact-v2` | 83.28 ms | 82.57 ms | 1.01× | +0.71 ms | PASS | PASS |
| `payload_create_read` | `payload-random-read-10-compact-v2` | 18.69 ms | 25.17 ms | 0.74× | -6.48 ms | PASS | PASS |
| `payload_create_read` | `payload-create-100m` | 568.08 ms | 575.91 ms | 0.99× | -7.83 ms | PASS | PASS |
| `payload_create_read` | `payload-random-read-100` | 58.28 ms | 69.68 ms | 0.84× | -11.40 ms | PASS | PASS |
| `payload_create_read` | `payload-create-500m` | 2.835 s | 2.419 s | 1.17× | +416.27 ms | PASS | PASS |
| `payload_create_read` | `payload-random-read-500` | 230.75 ms | 232.30 ms | 0.99× | -1.55 ms | PASS | PASS |
| `dedup_branch_history` | `dedup-history-distributed-1` | 22.82 ms | 17.14 ms | 1.33× | +5.68 ms | PASS | PASS |
| `dedup_branch_history` | `dedup-history-hotset-1` | 17.77 ms | 23.41 ms | 0.76× | -5.64 ms | PASS | PASS |
| `dedup_branch_history` | `dedup-history-metadata-1` | 22.84 ms | 29.65 ms | 0.77× | -6.81 ms | PASS | PASS |
| `dedup_branch_history` | `dedup-history-recurring-1` | 16.02 ms | 24.06 ms | 0.67× | -8.04 ms | PASS | PASS |
| `dedup_branch_history` | `dedup-history-unrelated-1` | 232.99 ms | 453.82 ms | 0.51× | -220.83 ms | PASS | PASS |
| `dedup_branch_history` | `dedup-history-distributed-10` | 65.16 ms | 69.57 ms | 0.94× | -4.41 ms | PASS | PASS |
| `dedup_branch_history` | `dedup-history-hotset-10` | 101.03 ms | 125.41 ms | 0.81× | -24.38 ms | PASS | PASS |
| `dedup_branch_history` | `dedup-history-metadata-10` | 76.99 ms | 101.61 ms | 0.76× | -24.62 ms | PASS | PASS |
| `dedup_branch_history` | `dedup-history-recurring-10` | 55.44 ms | 101.62 ms | 0.55× | -46.18 ms | PASS | PASS |
| `dedup_branch_history` | `dedup-history-unrelated-10` | 2.276 s | 5.666 s | 0.40× | -3390.05 ms | PASS | PASS |
| `dedup_branch_history` | `dedup-history-distributed-100` | 588.06 ms | 735.40 ms | 0.80× | -147.35 ms | PASS | PASS |
| `dedup_branch_history` | `dedup-history-hotset-100` | 873.32 ms | 985.36 ms | 0.89× | -112.04 ms | PASS | PASS |
| `dedup_branch_history` | `dedup-history-metadata-100` | 594.81 ms | 918.53 ms | 0.65× | -323.73 ms | PASS | PASS |
| `dedup_branch_history` | `dedup-history-recurring-100` | 405.59 ms | 618.64 ms | 0.66× | -213.05 ms | PASS | PASS |
| `dedup_branch_history` | `dedup-history-unrelated-100-mixed-v2` | 2.537 s | 3.190 s | 0.80× | -652.88 ms | PASS | PASS |
| `dedup_branch_history` | `dedup-history-distributed-500` | 3.314 s | 4.304 s | 0.77× | -989.38 ms | PASS | PASS |
| `dedup_branch_history` | `dedup-history-hotset-500` | 4.297 s | 4.914 s | 0.87× | -617.12 ms | PASS | PASS |
| `dedup_branch_history` | `dedup-history-metadata-500` | 3.087 s | 4.442 s | 0.70× | -1354.82 ms | PASS | PASS |
| `dedup_branch_history` | `dedup-history-recurring-500` | 2.060 s | 2.787 s | 0.74× | -727.52 ms | PASS | PASS |
| `dedup_branch_history` | `dedup-history-unrelated-500-mixed-v2` | 12.254 s | 16.107 s | 0.76× | -3852.36 ms | PASS | PASS |
| `dedup_workspace_reuse` | `dedup-workspace-exact-1-compact-v2` | 24.06 ms | 34.79 ms | 0.69× | -10.73 ms | PASS | PASS |
| `dedup_workspace_reuse` | `dedup-workspace-local-1-compact-v2` | 27.95 ms | 30.20 ms | 0.93× | -2.25 ms | PASS | PASS |
| `dedup_workspace_reuse` | `dedup-workspace-unique-1-base128-v3` | 35.14 ms | 39.06 ms | 0.90× | -3.92 ms | PASS | PASS |
| `dedup_workspace_reuse` | `dedup-workspace-unique-1-compact-v2` | 35.67 ms | 37.94 ms | 0.94× | -2.27 ms | PASS | PASS |
| `dedup_workspace_reuse` | `dedup-workspace-exact-10-compact-v2` | 65.94 ms | 81.92 ms | 0.80× | -15.99 ms | PASS | PASS |
| `dedup_workspace_reuse` | `dedup-workspace-local-10-compact-v2` | 75.59 ms | 78.99 ms | 0.96× | -3.40 ms | PASS | PASS |
| `dedup_workspace_reuse` | `dedup-workspace-unique-10-base128-v3` | 92.79 ms | 93.55 ms | 0.99× | -0.76 ms | PASS | PASS |
| `dedup_workspace_reuse` | `dedup-workspace-unique-10-compact-v2` | 122.44 ms | 96.02 ms | 1.28× | +26.42 ms | PASS | PASS |
| `dedup_workspace_reuse` | `dedup-workspace-exact-100` | 459.38 ms | 524.62 ms | 0.88× | -65.24 ms | PASS | PASS |
| `dedup_workspace_reuse` | `dedup-workspace-local-100` | 514.31 ms | 532.42 ms | 0.97× | -18.11 ms | PASS | PASS |
| `dedup_workspace_reuse` | `dedup-workspace-unique-100` | 583.03 ms | 634.99 ms | 0.92× | -51.96 ms | PASS | PASS |
| `dedup_workspace_reuse` | `dedup-workspace-exact-500` | 2.539 s | 3.900 s | 0.65× | -1361.05 ms | PASS | PASS |
| `dedup_workspace_reuse` | `dedup-workspace-local-500` | 2.764 s | 4.509 s | 0.61× | -1744.42 ms | PASS | PASS |
| `dedup_workspace_reuse` | `dedup-workspace-unique-500` | 3.157 s | 4.708 s | 0.67× | -1551.16 ms | PASS | PASS |
| `dedup_cross_file` | `dedup-cross-file-anchor-1` | 11.15 ms | 7.61 ms | 1.46× | +3.54 ms | PASS | PASS |
| `dedup_cross_file` | `dedup-cross-file-identical-10` | 26.02 ms | 19.17 ms | 1.36× | +6.85 ms | PASS | PASS |
| `dedup_cross_file` | `dedup-cross-file-mixed-10` | 35.59 ms | 35.11 ms | 1.01× | +0.48 ms | PASS | PASS |
| `dedup_cross_file` | `dedup-cross-file-unique-10` | 46.21 ms | 34.39 ms | 1.34× | +11.83 ms | PASS | PASS |
| `dedup_cross_file` | `dedup-cross-file-identical-100` | 77.32 ms | 54.37 ms | 1.42× | +22.95 ms | PASS | PASS |
| `dedup_cross_file` | `dedup-cross-file-mixed-100` | 297.61 ms | 276.18 ms | 1.08× | +21.43 ms | PASS | PASS |
| `dedup_cross_file` | `dedup-cross-file-unique-100` | 365.16 ms | 312.51 ms | 1.17× | +52.65 ms | PASS | PASS |
| `dedup_cross_file` | `dedup-cross-file-identical-500` | 346.53 ms | 209.91 ms | 1.65× | +136.61 ms | **FAIL** | PASS |
| `dedup_cross_file` | `dedup-cross-file-mixed-500` | 1.294 s | 1.233 s | 1.05× | +60.94 ms | PASS | PASS |
| `dedup_cross_file` | `dedup-cross-file-unique-500` | 1.509 s | 1.291 s | 1.17× | +218.24 ms | PASS | PASS |
| `dedup_cdc_locality` | `dedup-cdc-common-body-1` | 9.49 ms | 8.86 ms | 1.07× | +0.63 ms | PASS | PASS |
| `dedup_cdc_locality` | `dedup-cdc-delete-1` | 9.26 ms | 8.25 ms | 1.12× | +1.01 ms | PASS | PASS |
| `dedup_cdc_locality` | `dedup-cdc-insert-1` | 9.41 ms | 7.59 ms | 1.24× | +1.83 ms | PASS | PASS |
| `dedup_cdc_locality` | `dedup-cdc-overwrite-1` | 9.04 ms | 8.15 ms | 1.11× | +0.89 ms | PASS | PASS |
| `dedup_cdc_locality` | `dedup-cdc-scattered-1` | 12.20 ms | 11.48 ms | 1.06× | +0.72 ms | PASS | PASS |
| `dedup_cdc_locality` | `dedup-cdc-common-body-10` | 32.72 ms | 24.89 ms | 1.31× | +7.83 ms | PASS | PASS |
| `dedup_cdc_locality` | `dedup-cdc-delete-10` | 28.31 ms | 18.97 ms | 1.49× | +9.34 ms | PASS | PASS |
| `dedup_cdc_locality` | `dedup-cdc-insert-10` | 23.13 ms | 18.76 ms | 1.23× | +4.37 ms | PASS | PASS |
| `dedup_cdc_locality` | `dedup-cdc-overwrite-10` | 26.56 ms | 18.28 ms | 1.45× | +8.28 ms | PASS | PASS |
| `dedup_cdc_locality` | `dedup-cdc-scattered-10` | 47.92 ms | 40.93 ms | 1.17× | +6.99 ms | PASS | PASS |
| `dedup_cdc_locality` | `dedup-cdc-common-body-100` | 138.94 ms | 131.23 ms | 1.06× | +7.71 ms | PASS | PASS |
| `dedup_cdc_locality` | `dedup-cdc-delete-100` | 92.07 ms | 57.37 ms | 1.60× | +34.70 ms | **FAIL** | PASS |
| `dedup_cdc_locality` | `dedup-cdc-insert-100` | 94.86 ms | 64.35 ms | 1.47× | +30.51 ms | PASS | PASS |
| `dedup_cdc_locality` | `dedup-cdc-overwrite-100` | 91.07 ms | 61.05 ms | 1.49× | +30.02 ms | PASS | PASS |
| `dedup_cdc_locality` | `dedup-cdc-scattered-100` | 367.15 ms | 308.33 ms | 1.19× | +58.82 ms | PASS | PASS |
| `dedup_cdc_locality` | `dedup-cdc-common-body-500` | 651.52 ms | 552.10 ms | 1.18× | +99.42 ms | PASS | PASS |
| `dedup_cdc_locality` | `dedup-cdc-delete-500` | 387.23 ms | 243.00 ms | 1.59× | +144.23 ms | **FAIL** | PASS |
| `dedup_cdc_locality` | `dedup-cdc-insert-500` | 386.43 ms | 257.57 ms | 1.50× | +128.86 ms | **FAIL** | PASS |
| `dedup_cdc_locality` | `dedup-cdc-overwrite-500` | 387.62 ms | 243.22 ms | 1.59× | +144.41 ms | **FAIL** | PASS |
| `dedup_cdc_locality` | `dedup-cdc-scattered-500` | 2.290 s | 1.426 s | 1.61× | +864.05 ms | **FAIL** | PASS |
| `store_footprint` | `store-footprint-large-object-10m-low-v1` | 62.93 ms | 63.20 ms | 1.00× | -0.26 ms | PASS | PASS |
| `store_footprint` | `store-footprint-metadata-cardinality-100-low-v1` | 62.38 ms | 60.36 ms | 1.03× | +2.03 ms | PASS | PASS |
| `store_footprint` | `store-footprint-unique-100-low-v1` | 52.89 ms | 49.65 ms | 1.07× | +3.24 ms | PASS | PASS |
| `store_footprint` | `store-footprint-large-object-500m` | 1.327 s | 1.254 s | 1.06× | +73.10 ms | PASS | PASS |
| `store_footprint` | `store-footprint-metadata-cardinality-100000` | 4.915 s | 6.640 s | 0.74× | -1724.76 ms | PASS | PASS |
| `store_footprint` | `store-footprint-unique-100000` | 4.486 s | 5.397 s | 0.83× | -910.77 ms | PASS | PASS |

**Cited, not re-collected — owner-banked one-sample pairs (C1-era receipts, harness `daa74be0…`).**

| family | selection | candidate | v0.1.5 | ratio | Δ abs | disposition | proof |
|---|---|--:|--:|--:|--:|---|---|
| `tiny_file_churn` | `tiny-create-500-mixed-v4` (B1) | 244.35 ms | 242.66 ms | 1.01× | +1.69 ms | REUSED-FROM | PASS |
| `tiny_file_churn` | `tiny-bulk-create-500-mixed-v3` (B2) | 4.755 s | 5.733 s | 0.83× | −978.30 ms | REUSED-FROM | PASS |
| `local_snapshot` | `local-snapshot-create-25000-onebyte-v1` (B3) | 10.381 s | 8.874 s | 1.17× | +1,507.47 ms | REUSED-FROM | PASS |

**Non-collected terminal rows.**

| family | selection | disposition | reason |
|---|---|---|---|
| `edit_length_changing_capped` | all 5 `…-capped-v1` | NOT_RUN_OPTIONAL | not admitted to host execution (`runner.py:31,309`); version-retained duplicates, each measured by its `…-result-capped-v2-ops-1` successor in `edit_length_changing` (same registered `fixture_bytes`) |
| `historical_access` | 11 performance + 11 proofs | NOT_RUN | the sealed v2 Store (`store_sha256 f323de0e…`, branch `1101a087…`) is not present anywhere reachable |
| `repository_history` | 3 profiles | NOT_RUN_OPTIONAL | optional profiles; #152 does not select them |
| `workspace_reliability` | `workspace-sustained-600s-compact-v2-proof` | NOT_RUN_OPTIONAL | `verification_supported: false`; separately accounted optional long test |
| `workspace_reliability` | 6 fault-injection proofs | FAIL (diagnosed) | injections still target the pre-v0.1.6 host-owned routes |

## 4. Bug ledger

| # | commit | class | one-line RCA | impact set re-run |
|---|---|---|---|---|
| 1 | *(infrastructure, no commit)* | H — host prepared-input cache | four prepared native fixtures had their 100 MiB anchor file(s) deleted at 2026-09-15 10:20 by an out-of-band cleanup, and `runner.py` deliberately trusts an owned native fixture's recipe instead of re-validating content | `namespace-10000`, `namespace-100000` re-prepared and re-collected; `store-footprint-{unique,metadata-cardinality}-100000` re-prepared for G7. Regenerated payloads proven byte-identical file-by-file |
| 2 | `58e4f7f47` | **S0 — product correctness** | `truncate_async` was the one edit path never migrated to the sandbox-owned spool: it still sent `wire::CHECK` to the host immutable-base service, which rejects the removed payload opcodes, so every size-changing truncate failed with `EINVAL` — and the kernel turns `open(O_TRUNC)` into `setattr(size)`, which is why `git commit` could not open `.git/COMMIT_EDITMSG` | `git_tool_workflow` 4/4 (all of G4 re-collected under C2 for one identity); regression test `truncate_applies_locally_without_a_removed_host_payload_check` verified to fail without the fix |
| 3 | `29835f44d` | **S0 — product correctness** | the host-continuation Commit route re-bases the host shell (advancing `expected_head`) *before* assembling the result, so `WorkspaceCommitResult::Created { previous_head }` returned the newly published head — a Commit reported itself as its own predecessor | `payload_create_read` 8/8 re-collected (perf + verify) under C3; caught by the registered proof `host_continuation_proof`, which fails 8/8 without the fix and passed 8/8 on the v0.1.5 control |
| 4 | `b9bca593c` | H — benchmark consistency check | the store-footprint cross-check compared the Commit's edit-spool allocation against the host shell's FUSE write-spool metric, which is dead on the sandbox route; the Commit's own accounting is internally consistent | `store_footprint` 6/6 re-collected under C3′; product seal unchanged by the fix |

Both product fixes were preceded by `tools/preflight.sh`: **all steps passed**
(rustfmt 1.96 `--all --check`, tools unit tests, workspace fast suite under
1.85.1, clippy `--workspace --locked -- -D warnings`, benchmark harness tests).
Both regression tests were verified to fail without their fix.

**Material regressions (6, all one cause).** `dedup-cross-file-identical-500`
1.65×, `dedup-cdc-scattered-500` 1.61×, `dedup-cdc-delete-100` 1.60×,
`dedup-cdc-overwrite-500` 1.59×, `dedup-cdc-delete-500` 1.59×,
`dedup-cdc-insert-500` 1.50×. All six are single `initialize` calls, and
`objects.rs:3545`/`:4488` admit `worker_limit.min(SMALL_CONTENT_WORKERS)`
constructors, so the mandated `LAYERFS_CONSTRUCTION_WORKERS=1` removes the
released 4-way small-content parallelism. Direct proof (labelled diagnostic, not
a gate row): `dedup-cdc-scattered-500` measures 2,290.09 ms at one worker and
**1,458.89 ms** with the variable unset, against a 1,426.04 ms comparator.

## 5. Architecture guardrails — evidence, not claims

1. **Non-pausing, continuous workspace.** `FREEZE=32`/`RESUME=33` survive as wire
   constants with a string mapping in `live_transport.rs:247`, but **no handler for
   either exists in `live_owner`'s dispatch**, and `live_owner.rs:3605` records that
   `CAPTURE` replaced the old FREEZE/RESUME facts export; every production
   `control()` call is `shutdown`. `commit_pause_fence_ns = 0` in **56/56** SDK-edit
   cells. Exercised directly by the env-gated probes: two live `exec` processes with
   writable `MAP_SHARED` mappings survive two Commits while an SDK edit races a
   Commit behind a `Barrier`, and `active_execution_count() == 2` is asserted after
   each Commit. **What fails is recorded, not hidden:** those same probes fail on the
   candidate at the first Commit's mapped-byte check, and the boundary is exact —
   adding `msync(MS_SYNC)` before the client reports "ready" lets the first Commit
   pass. Root cause: `live_owner.rs:2070` — *"Commit capture never calls this;
   snapshots are stable by frontier ownership, not by pausing the workspace"* — so a
   mapped write the kernel has not written back is not in the Commit's frontier.
   v0.1.5 passed both probes 2/2. This is the limitation the frozen spec already
   declares (`sandbox-local-snapshot-spec-and-plan.md` §10 and §9 boundary, roadmap
   README "dirty shared-mmap visibility stays unsolved"); both available repairs are
   out of bounds for this campaign (a pause in the Commit path is a
   declared-architecture change; the §9.1 direct-I/O variant is "PROPOSED, not
   adopted"; `AGENTS.md` §4 forbids dependency/kernel patches). **No registered
   selection is on this path** — the registered families write through ordinary
   FUSE/SDK routes, which are in the frontier.
2. **Simplified host↔docker connection, transaction at commit.** Opcode inventory
   found in C1 (37 constants, `live_wire.rs`): immutable-base service `SEED=1
   LOOKUP=2 LOOKUP_METADATA=16 DIRECTORY_PAGE=13 READ_BASE=6` (+
   `RESERVE/APPEND/READ_BACKING/CHECK/RELEASE/CANCEL_RESERVATION/FACTS_*/BATCH`);
   snapshot lane `CAPTURE=45 SNAP_RECORDS=50 SNAP_READ=51 SNAP_CANCEL=49
   COMPLETE_BEGIN/NODE/END=46/47/48`; edit lane `EDIT_BEGIN/PART/END=42/43/44`.
   The host immutable-base service **rejects** the removed payload opcodes, and its
   own test asserts that — the G4 defect was the one call site that had not been
   converted. Publication is atomic at Commit: `host_continuation_proof` commits
   twice in a live workspace and compares the returned `commit_id` against the
   published branch head, and after the G5 fix the lineage field agrees too.
3. **Snapshot is built incrementally.** Per-Commit work scales with the change:
   `final_live_non_base_bytes = 4,096` at every tier from 1 MiB to 500 MiB and
   `commit_cdc_bytes_scanned = 4,096` on the SDK-edit route. B3's banked
   three-Commit 25k lifecycle is the strongest generation evidence
   (`created_commit_count = 3`); `dedup_workspace_reuse` reuses one workspace across
   tiers 1→500 within 0.61–0.99× of v0.1.5.
4. **Storage and RAM bounded and safe.** cgroup `file_peak` is **4,096 B** on every
   SDK-edit cell including the 500 MiB tiers, `file_dirty_peak` 4,096 B, `shmem` 0,
   `swap` 0, `anon_peak` 0.66–1.26 MB — the L18 amplification signature is absent
   there, and I reproduced the L18 bound itself (`tiny-bulk-create-500-mixed-v3`:
   2.3 MiB of file cache for a 500 MiB payload, versus L18's recorded ≤ 2.6 MiB).
   **One route fails and is not excused:** `workspace-dense-rewrite-*` grows the
   container cgroup `file` cache at ~1.03× the payload (1 MiB→8,192 B;
   10 MiB→11.7 MiB; 100 MiB→103.1 MiB; 500 MiB→**504.7 MiB**), because create
   handles get `FOPEN_DIRECT_IO` (`filesystem.rs:1430`) while a rewrite of a
   pre-existing file uses an ordinary writable open. **It is not a v0.1.6
   regression**: the v0.1.5 control measures 501.4 MiB on the identical cell. Not
   repaired — the only bounding mechanism is the plan's §9.1 proposal, which removes
   supported writable shared mappings. Reported as a named guardrail FAIL.
5. **Metadata does not scale with file count.** `store_footprint` measures the
   metadata-cardinality overhead directly against its unique-content control at two
   tiers, with v0.1.5's own spread beside it: 100 entries 5,197,824 vs 5,165,056 B
   (+0.6 %) and 100,000 entries 558,481,408 vs 527,396,864 B (+5.9 %) on the
   candidate, versus +0.6 % and +6.6 % on v0.1.5 — the overlay snapshot adds **no**
   per-entry metadata beyond what v0.1.5 already did. Namespace Init's Store growth
   per ingested byte is sub-linear (54.6 → 20.6 → 3.05 → 0.52 B across tiers
   100/1,000/10,000/100,000).

## 6. Resource tables

**Transient backing and canonical Store (per family, candidate).**

| family / tier | transient backing peak | canonical Store after the command |
|---|--:|--:|
| `init_namespace` 100 / 1,000 / 10,000 / 100,000 | not applicable (native Init) | 5,152,768 / 20,570,112 / 304,939,008 / 520,560,640 B |
| `tiny_file_churn` `tiny-bulk-create-1` | 0 B spool high-water | 1,163,264 B |
| `tiny_file_churn` `tiny-bulk-create-100-mixed-v3` | 104,857,600 B physical spool | — |
| `workspace_change_locality` `dense-rewrite-500` | 524,288,000 B physical spool | 1,067,315,200 B |
| `local_snapshot` 25,000 (B3, banked) | **25,000 B against the 32 MiB ceiling** | banked receipt |
| `store_footprint` unique-100,000 | n/a | 527,396,864 B allocated / 515,588,096 apparent |
| `store_footprint` metadata-cardinality-100,000 | n/a | 558,481,408 / 552,194,048 B |

The 25k transient ceiling (32 MiB), the 64 MiB aggregate accounted allocations and
the 8 MiB staging/transfer bound all hold where they are registered; the one
declared exception is B3's banked receipt, which reports 25,000 B.

**Memory domains, SDK-edit route (56/56 cells, phase-domain values).**

| domain | value |
|---|--:|
| cgroup `file` peak | 4,096 B (every cell, including 500 MiB tiers) |
| cgroup `file_dirty` peak | 4,096 B |
| cgroup `file_writeback` peak | 0 B |
| cgroup `shmem` / `swap` peak | 0 B / 0 B |
| cgroup `anon` peak | 0.66–1.26 MB |
| cgroup `kernel`/`slab` peak | ~0.6–0.65 MB |

**Memory domains, rewrite route (diagnostic, `workspace-dense-rewrite-*`).**

| tier | `file` peak | `anon` peak | `memory.current` peak | control `file` peak |
|---|--:|--:|--:|--:|
| 1 MiB | 8,192 B | 0.66 MB | 5.15 MB | — |
| 10 MiB | 12,304,384 B | 3.98 MB | 21.70 MB | — |
| 100 MiB | 108,109,824 B | 18.78 MB | 132.77 MB | — |
| 500 MiB | **529,182,720 B** | 33.82 MB | 573.82 MB | **525,750,272 B** |

Container lifetime peaks are never quoted as phase peaks anywhere in this report
(L18 measured them varying ~8× on identical inputs).

## 7. Limitations that remain

1. **Kernel-dirty shared mmap is not captured by a Commit.** Proven on this
   candidate (5/5 and 4/4 failing probes, v0.1.5 passes 2/2), boundary isolated to
   exact byte level. This is the frozen spec's own declared open obligation;
   repairs are out of bounds for this campaign. No registered selection is affected.
2. **Six `workspace_reliability` fault-injection proofs do not exercise the sandbox
   route — five are instrumentation, one is a product defect.** See
   [`issue152-reliability-fix-handoff.md`](../issue152-reliability-fix-handoff.md)
   for the reproduction recipe, the per-case fix and the pitfalls.
   `workspace-final-publication-failure-retry` is the exception: its fault *does*
   fire and the Commit *does* surface the exact injected error, but the sandbox
   route then refuses the retry (`remote_commit.rs:50-58`, `workspace stage
   retained`) that the materialized route performs and the proof encodes — so
   recovery from a failed final publication on this route is Discard-only. Their injections still target `Workspace::build_candidate`, the host
   shell's append path and the host materialized projection. **The recovery evidence
   they exist to produce is therefore missing for the sandbox route.** Follow-up:
   re-point `VerificationFault::Candidate`/`VerificationStoreFault::*`, the short-write
   and ENOSPC injections and the presentation-failure injection into
   `build_remote_candidate`, the sandbox admission path, `LocalSpool` and the live
   projection, and re-pin the retry semantics.
3. **File cache grows with the payload on the rewrite route** (~1.03×), with
   v0.1.5 parity; the bound would require the un-adopted §9.1 writable-open policy.
4. **The host-side FUSE write-spool metric is dead on the sandbox route**
   (`spool_write_bytes` reads 0). It is no longer used as a gate here; re-wiring it
   would need a new cross-boundary counter.
5. **`historical_access` could not be run**: the sealed v2 Store
   (`store_sha256 f323de0e…`) is absent — not in the prepared cache, not in the
   issue118 evidence (which used a different store), not in the control worktree,
   and the documented `layerfs-issue100-45mb-evidence/retained-full157-1/…` path was
   removed by the L17 cleanup. 11 performance cases + 11 proofs are `NOT_RUN`.
6. **Six material time regressions from the single-worker directive**, diagnosed and
   proven by a labelled diagnostic; they exceed the bounded acceptance and are not
   waived.
7. **Host-state spread dominates the small cells**: 12 cells sit between 1.25× and
   1.49× and are accepted by the ratio test only; `payload-create-1m-compact-v2`
   (1.38×, +11.29 ms) is the clearest example, with 1.01×/0.99×/1.17× at the other
   tiers of the same shape.
8. **Sandbox process memory is still not emitted by the frozen harness** (carried
   from L18/L20); only container cgroup domains and host RSS are available.
9. `workspace_reliability`'s 600-second sustained proof is an optional long test and
   was not selected; the bounded parallel-read/write and repeated-publication proofs
   are not relabelled as 600 seconds.

## 8. Decisions I made, and why

1. **Repaired the host prepared-input cache instead of the harness.** The four
   truncated fixtures were an environment fault; `runner.py`'s trust of owned native
   recipes is a documented, deliberate choice, and changing it would have moved the
   frozen harness identity and invalidated every banked receipt. I quarantined,
   regenerated and proved byte identity instead.
2. **Reverted a cosmetic harness improvement** (a better scan-mismatch message)
   because it moved the compilation seal off the frozen `79dab102…` for a diagnostic
   that had already served its purpose. Recorded the underlying gap instead.
3. **Fixed both product defects rather than parking them**, each with a focused
   regression test that fails without the fix, a control A/B where the route allowed
   it, and `tools/preflight.sh` before the commit.
4. **Re-collected whole groups rather than half of one.** G4 was re-collected
   entirely under C2 so the group is not split across two products; G5 re-collected
   only `payload_create_read` under C3, exactly as #152 directs ("re-run only the
   affected cases"), because `mixed_load_bearing` cannot be affected by a change to
   one returned field. G1–G3 keep their C1 identity and are not re-labelled.
5. **Classified the store-footprint check as H, not a product bug**, after
   instrumenting it: the Commit's own accounting is internally consistent
   (allocated 10 = live 10 + superseded 0, peak 10 ≥ allocated 10) and only the
   cross-check against the dead host metric failed.
6. **Recorded the six reliability proofs as FAIL rather than WARN**, because their
   purpose is recovery evidence and on this route that evidence does not exist;
   absorbing them into a tally would overstate what was verified.
7. **Declared the budget reading explicitly.** A selection's complete command is
   `command_wall_ns` (product timer + container lifecycle + cleanup); preparation,
   one-time fixture generation and cold acquisition are excluded by the rule's own
   "excluding one-time prepared-input validation" clause, and the raw totals are
   printed beside every such row so the exclusion is auditable. No timer was moved,
   no timeout enlarged, no workload shrunk, and no run raised the worker count.
8. **Did not stop at the failures.** Every group was collected to completion, both
   product bugs were driven to a landed fix and a re-run, and the campaign ran to
   the final report.

Passing this campaign does not merge, close or release anything.
